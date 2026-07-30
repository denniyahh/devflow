---
phase: 26-release-cut-automation
reviewed: 2026-07-29T23:59:00Z
depth: deep
review_mode: five-angle-parallel
angles:
  - doc-accuracy
  - security-leaked-data
  - ci-build-correctness
  - external-state-claims
  - generalist-deep
files_reviewed: 16
files_reviewed_list:
  - CONTRIBUTING.md
  - OPERATIONS.md
  - README.md
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/tests/release_check.rs
  - crates/devflow-cli/tests/release_execute.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/release.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/sync.rs
  - crates/devflow-core/src/version.rs
raw_findings: 49
merged_duplicates: 13
findings:
  critical: 7
  warning: 24
  info: 5
  total: 36
status: issues_found
ship_gate: BLOCKED
---

# Phase 26: Code Review Report

**Reviewed:** 2026-07-29
**Depth:** deep (five parallel finder passes, merged and deduplicated)
**Files Reviewed:** 16
**Status:** issues_found — **Ship gate: BLOCKED (7 Critical)**

## Scope

Scope was the 13 source files declared in the phase `SUMMARY` artifacts, plus
the 3 documentation files this phase changed (`README.md`, `CONTRIBUTING.md`,
`OPERATIONS.md`) so the doc-accuracy angle had its actual subject matter. This
supersedes the earlier standard-depth pass recorded at this path (13 files, 0
Critical / 7 Warning / 4 Info).

Five independent finders ran in parallel — doc-accuracy, security/leaked-data,
CI-build/false-green, external-state claims, and a generalist deep pass. Each
was required to attempt to refute its own findings before reporting. They
produced 49 raw findings; 13 were merged as duplicates across angles, leaving
36.

## What this phase adds

`devflow release --execute --yes-release`, a release-cut executor
(`devflow_core::release`, 1151 lines, new), a standalone `devflow sync`
(`devflow_core::sync`, 575 lines, new), and the `cargo_publish` /
`release_tag_state` / `publish_order` primitives in `devflow_core::git`
(+880), with version-bump machinery in `version.rs` (+403) and CLI wiring in
`main.rs` / `commands.rs`.

## Authorization gate: PASSES

The question this phase was scoped to answer — can any path reach `git push`,
`git tag`, or `cargo publish` without the typed `--yes-release` flag — is
answered **no**, and the finders confirmed it independently:

- `main.rs:602-635` is the only dispatch arm for `Command::Release`;
  `release_execute` is called on exactly one branch (`main.rs:629`), reachable
  only when `execute && yes_release && !check`.
- `yes_release` is a plain clap `#[arg(long)] bool` (`main.rs:255`) with no
  `env`, no `default_value`, no config or `State` read. It appears nowhere in
  `config.rs`, `state.rs`, or `config_parse.rs`.
- `release_execute` (`commands.rs:2274`) takes no authorization parameter, so
  it structurally cannot be invoked with a forged authorization.
- `execute_release`, `cargo_publish`, and `create_signed_release_tag` have no
  non-test callers outside this chain.
- The proving tests are real end-to-end binary drives, not source greps:
  `execute_without_yes_release_is_rejected` and
  `yes_release_is_not_settable_via_config_or_env` (which sets
  `devflow.toml: yes_release = true`, `DEVFLOW_YES_RELEASE=true`, and
  `YES_RELEASE=true`, and still requires refusal).

Two nuances are **not** gate bypasses but should be stated plainly:

1. `devflow sync` (new subcommand, `main.rs:266`, `commands.rs:2243`) performs a
   real remote mutation — a `-X ours` merge commit direct-pushed to
   `origin/develop` — with **no authorization flag of any kind**. That is
   D-07/D-08's stated intent (it ports a script operators ran by hand), but it
   means "mutating operation reachable without a typed flag" is true for the
   sync half of the sequence. Recorded as W-23.
2. `execute_release` is `pub` in `devflow-core`, a crate published to crates.io.
   Any downstream library consumer can call it directly; the gate is a
   CLI-surface gate only. Not a defect for this threat model. Recorded as W-24.

**And** — the gate authorizes the *sequence*, not the *target repository*
(C-06) and not the *correctness of the version being released* (C-01). The
gate holding is necessary but not sufficient, and the Criticals below all live
downstream of it.

**Tooling:** `cargo clippy --all-targets -- -D warnings` exits 0 and
`cargo fmt --check` is clean — this phase introduces no lint or format
violations.

---

## Critical Findings (7) — all block Ship

### C-01 — A stray unreachable semver tag is adopted as the release version and pushed to `origin/develop`

- **File:** `crates/devflow-core/src/release.rs:262-278` (mutation at `:286-321`)
- **Angle:** generalist · **Confidence:** high · **Reproduced by execution**

`compute_version` correctly refuses with `VersionError::UnreachableBaseline`
when the highest semver tag is not reachable from `HEAD` — that refusal is the
entire point of D-10. The executor catches that refusal and *adopts* the
unreachable tag's version as an assumed in-flight resume:

```rust
Err(crate::version::VersionError::UnreachableBaseline { tag }) => {
    let stripped = tag.strip_prefix('v').unwrap_or(tag.as_str());
    let resumed = parse_bare_version(stripped)?;
    steps.push(StepReport { step: ReleaseStep::VersionBump, status: StepStatus::Completed,
        detail: sanitize_changelog_subject(&format!(
            "resuming the in-flight release identified by unreachable tag `{tag}` \
             — the version bump already landed in a prior invocation")) });
    resumed
}
```

It then unconditionally writes, commits, and **pushes** that version.

**Failure scenario (executed, not hypothesized):** repo on `develop` at
`2.1.0`; a semver tag `v9.9.9` exists on an abandoned or squash-merged branch.
The executor writes `9.9.9` into `[workspace.package] version` *and* the
`[workspace.dependencies]` self-pin, commits `chore: bump version to 9.9.9`,
and pushes it to `origin/develop`, then halts at the human gate:

```
compute_version correctly refuses: unreachable v9.9.9
version=9.9.9 tag=v9.9.9 outcome=HaltedAtHumanGate
  VersionBump Completed resuming the in-flight release identified by unreachable tag `v9.9.9` …
  VersionBump Completed wrote and committed version 9.9.9, pushed develop to origin
remote develop log: c2f9d6f chore: bump version to 9.9.9
```

The reported detail is **false** — no prior invocation existed.

**Why this input is likely, not exotic:** `hooks_after_ship`'s `version_bump`
(`hooks.rs:327-328`) creates a `v{version}` tag at the end of *every* phase's
Ship stage, and this project ships from worktrees on feature branches that are
then squash-merged. A squash merge leaves that tag permanently unreachable from
`develop` and strictly higher than the reachable baseline — exactly this input.
`release_tag_state`'s own doc comment records that this repository *already
carries one such orphan tag* (`v1.3.69`).

**Refutation attempted and failed:** `26-06-PLAN.md:192` designs this branch and
argues "step 3 independently validates that the tag belongs to `origin/main`'s
tip, so a stale or foreign tag is caught there." That argument is wrong about
ordering — step 1's write, commit, and push all execute before step 2/3 run.
The human gate is step 2 at `:372`, strictly *after* `push_ref` at `:317`.

---

### C-02 — A failed publish step is unresumable: the re-run starts a *new* release, pushes another bump, and exits 0

- **File:** `crates/devflow-core/src/release.rs:229-278` vs `:484-545`; `version.rs:644-651`
- **Angles:** generalist + external-state (independently found, merged) · **Confidence:** high · **Reproduced by execution**

The module doc claims (`release.rs:226-228`):

```rust
//! Every step's own live-state predicate makes it independently resumable
//! (D-06); nothing is ever rolled back on any failure (D-05)
```

This is false for the one irreversible step. Publish is step 5, *after*
`sync_main_to_develop` at `:484`. The sync merges `origin/main` — which now
carries the release tag — into `develop`, making the tag reachable. The next
`compute_version` therefore uses it as the baseline and returns `version + 1`.
`apply_bump` (`version.rs:644-651`) compounds this by flooring `Bump::None` to
patch+1, so a re-run *always* computes a new version:

```rust
Bump::Patch | Bump::None => {
    semver::Version::new(baseline.major, baseline.minor, baseline.patch + 1)
}
```

**Failure scenario (executed):** release `0.1.0`; bump, signed tag, and sync all
land; publish then fails — `cargo publish` non-zero, missing
`CARGO_REGISTRY_TOKEN`, or a `PublishCheck::Ambiguous` network blip at
`git.rs:995`, all of which `?` straight out at `release.rs:533/542`. The
operator re-runs the identical command:

```
RUN 1: version=0.1.0 tag=v0.1.0 outcome=Completed   (bump, tag, sync all landed)
RUN 2: version=0.1.1 tag=v0.1.1 outcome=HaltedAtHumanGate
remote log: 16daa2e chore: bump version to 0.1.1
            dd56fa8 merge: sync main back into develop after release
            3c42d72 chore: bump version to 0.1.0
```

`v0.1.0` is tagged, pushed, and synced but **never published and never
publishable by the tool**. `0.1.1`, which nobody asked for, is pushed to
`origin/develop`. If `devflow-core@0.1.0` published but `devflow@0.1.0` failed
— the exact partial failure the two-package ordering makes likely — crates.io
is left permanently split across two versions with no automated path back, and
the command returns **exit 0**.

**Refutation attempted and failed:** the publish step *does* have a live-state
resume predicate (`crate_already_published`), but it is never reached with the
right version, because the version itself has moved on before step 5 is
re-entered. The entry guards do not refuse the second run (tree clean, on
develop, remote present).

---

### C-03 — Every accumulated `StepReport` is discarded on failure: the operator is told nothing about the irreversible steps that already succeeded

- **File:** `crates/devflow-core/src/release.rs:252` (accumulator) vs `:533`, `:542`, and every `?`/`return Err`; surfaced at `commands.rs:2343`
- **Angle:** external-state · **Confidence:** high · **Independently confirmed during merge**

`execute_release` accumulates `let mut steps: Vec<StepReport>` and moves it into
`ReleaseReport` only on the success path (`:550`). `ReleaseError` carries no
`steps` field — the source acknowledges this itself at `release.rs:1146-1147`:

```rust
// No Ok(ReleaseReport) was ever produced on this path, so — by
// construction, since `ReleaseError` carries no `steps` field — no
// Publish step report could possibly have been produced either
```

Every `?` and `return Err` in the sequence therefore drops the entire record.
`commands.rs:2343` prints only `err.to_string()`.

**Failure scenario:** the per-package publish loop (`:513-534`) publishes
`devflow-core` successfully, then `cargo_publish(project_root, package)?` at
`:533` fails for `devflow`. The operator sees only the second package's error.
Nothing tells them `devflow-core` is already live on crates.io — an
irreversible action — so the natural next move is a re-run, which triggers C-02.

---

### C-04 — `release --execute` prints "release cut complete" and exits 0 when the publish step published nothing

- **File:** `crates/devflow-core/src/release.rs:511-519`; gate at `commands.rs:2295`
- **Angles:** ci-build (reproduced live against the built binary) + generalist · **Confidence:** high

`check_publish_order` returns `"warn"` for an empty order; `release_execute`
blocks only on `"fail"`; `release.rs:511-519` records Publish as `Skipped` while
the overall outcome is still `Completed`. The "blocking" publish-order pre-gate
structurally cannot block.

**Failure scenario (reproduced):** a tag was cut and pushed, crates.io received
nothing, and the command printed `release cut complete` with `EXIT=0`. The
operator has no signal that the release is unpublished — this is a textbook
false green on the single irreversible step.

---

### C-05 — A `default-members` key silently truncates the publish set; the pre-gate reports ✓ and a partial publish is reported as a complete release

- **File:** `crates/devflow-core/src/git.rs:773` (`workspace_member_paths`)
- **Angle:** ci-build · **Confidence:** high · **Reproduced**

`workspace_member_paths` substring-matches `members`, which also matches
`default-members`. The parse latches onto the wrong key and silently truncates
the member list.

**Failure scenario (reproduced):** a workspace whose root manifest declares
`default-members` yields `publish in order: mycli` where the correct order is
`mycore -> mycli`. The pre-gate reports ✓, the release reports complete, and
`mycore` is never published — leaving a published `mycli` depending on an
unpublished `mycore`.

This does not fire on today's root `Cargo.toml` (no `default-members` key), so
it is not currently active. It is Critical rather than Warning because the
affected step is irreversible, the trigger is a single ordinary manifest key
that any maintainer may add, and the failure is silent in both the pre-gate and
the final report.

---

### C-06 — `release --execute --yes-release` silently retargets to an ancestor repository: the irreversible `cargo publish` / `git push` / `git tag` can run against a checkout the operator never named

- **File:** `crates/devflow-cli/src/main.rs:629` (dispatch) → `:662-683` (`project_root`)
- **Angle:** security · **Confidence:** high · **Independently confirmed during merge**

`project_root` walks *up* to the nearest `.devflow` ancestor:

```rust
let mut probe = start.as_path();
loop {
    if probe.join(".devflow").is_dir() { return Ok(probe.to_path_buf()); }
    match probe.parent() { Some(parent) => probe = parent, None => return Ok(start) }
}
```

Verified on disk: this worktree (`.worktrees/phase-26/`) has **no** `.devflow`;
the parent checkout `/var/home/denniyahh/Github/devflow/` **does**. Phase 26
newly routes `release_execute` through this resolver (`main.rs:629`, added in
this diff — the pre-phase code routed only `release_check`), and `Sync` too
(`:637`).

**Failure scenario:** a maintainer runs `devflow release --execute
--yes-release` from a phase worktree — the ordinary working posture on this
project, which dogfoods from worktrees. The command resolves `project_root` to
the main checkout and cuts a release from *that* tree's state: its branch, its
commits, its manifest. All four entry guards (clean tree, on-develop,
has-remote, pre-gate) test the **redirected** root, so a dirty worktree with a
clean parent makes the executor *more* likely to proceed, not less. The
operator is never shown the resolved path before the mutation.

---

### C-07 — README's manual history-link repair claims parity with `devflow sync` but omits the one check `sync` calls non-negotiable, and its "verify this changed nothing" command cannot detect the failure

- **File:** `README.md:44-53` (added in this phase) vs `crates/devflow-core/src/sync.rs:187-192`
- **Angle:** doc-accuracy · **Confidence:** high

The new README snippet asserts it repairs the history link "the same way
`devflow sync` does". It omits the fetch, and — critically — `sync.rs:187-192`
gates its push on a `HEAD^{tree}` before/after comparison that the source calls
"the one check that must never be relaxed". The README has no equivalent.

Its substitute verification step, `git diff --stat origin/main..HEAD`, is an
**inverted signal**: in the failing case the diff *shrinks*, so an operator
reading "this changed nothing" as success will read success precisely when the
merge has clobbered tree state. Following the documented procedure ends in a bad
push to `develop`.

This is Critical rather than Warning because the documented procedure is
addressed to an operator performing a manual repair on a shared branch, the
verification step actively misreports the failure it exists to catch, and the
outcome is an irreversible bad push.

---

## Warning Findings (24)

**W-01 — Unsanitized subprocess stderr reaches the operator's terminal, contrary to `ReleaseError`'s own doc claim.**
`release.rs:317`, `:413-414`, `:435`; doc at `release.rs:109-112`. The phase's own
T-26-37 sanitization control is bypassed on the `GitError` and `VersionError`
`#[from]` paths — the sanitized sibling at `sync.rs:196-200` shows the intended
shape. Confirmed empirically that git forwards remote pre-receive hook bytes
including `ESC`/`BEL` verbatim into `eprintln!("error: {err}")`, at exactly the
moment the operator is judging a release. Four of six `ReleaseError` variants
carry raw `stderr_or_status`. *(Merged: security ×2 + doc-accuracy ×1.)*

**W-02 — `sanitize_changelog_subject` strips only Unicode category Cc.**
`version.rs:504`. Bidi-override (U+202E) and zero-width characters survive into
`CHANGELOG.md` — a committed, published artifact — and into operator-facing
release output.

**W-03 — A manifest version ahead of the computed version is never corrected, publishes a version the report does not name, and deadlocks the human gate.**
`release.rs:286-288`. `write_needed` uses `<` rather than `==`, so when the
on-disk version is ahead, no write occurs but `cargo publish` still ships
whatever the manifest declares — a version the report never names. Step 1 then
reports a false statement about disk state. *(Merged: external-state + generalist.)*

**W-04 — A tag that was successfully created *and pushed* is reported as `TagCollision` when the confirming `ls-remote` transiently fails.**
`release.rs:417`, `:438`, `:454`, `:461`, `:470`. A network blip converts a
completed, irreversible remote mutation into an error that tells the operator the
opposite. *(Merged: external-state + generalist.)*

**W-05 — Steps 1 and 4 report "pushed to origin" from `git push`'s exit code alone.**
`release.rs:317`, `:484`. `SyncOutcome::Merged.merge_commit` documents itself as
`origin/develop`'s new tip while reading only local `HEAD`.

**W-06 — `release_tag_state` check 4 asks "is the released commit an ancestor of the tag", not "does the tag point at the released commit".**
`git.rs:640`. A tag ahead of `origin/main` is reported `Released` and the tag step
is skipped entirely. The doc comment for `ReleaseTagState::Released` states the
ancestry relation *backwards*, contradicting step 4 of its own doc block and the
code. *(Merged: ci-build + security + doc-accuracy.)*

**W-07 — The release tag is bound to `origin/main`'s tip at run time, not to the commit that declares the release version.**
`release.rs:413`. If `origin/main` advances between the version resolution and the
tag step, the tag names a commit that does not declare that version.

**W-08 — `HaltedAtHumanGate` exits 0 after mutating `origin/develop`.**
`release.rs:388`; `commands.rs:2295`. Indistinguishable from a completed release by
exit code — only by stdout prose. Any script or CI wrapper treating exit 0 as
"released" is wrong. *(Merged: ci-build + generalist.)*

**W-09 — A git failure reading `origin/main`'s version file is swallowed by `.ok()` and reported as the human gate.**
`release.rs:361`. Produces a permanent exit-0 halt loop: the condition never
resolves, and every re-run reports the same benign-looking gate.

**W-10 — The publish step and the pre-gate's blocking branch have zero test coverage.**
`release_execute.rs`; `release.rs` tests. The only "full sequence completes" test is
fixtured so the publish step *cannot* run (no workspace members), and
`off_develop_fixture` actively pins the fail-open behaviour. No test spawns
`cargo info` or a successful `cargo publish` — the registry-check contract rests
entirely on hand verification. This is why C-04 and C-05 survived the suite.
*(Merged: ci-build ×2.)*

**W-11 — `release.rs`'s fixture helpers check that git *spawned*, not that it *succeeded*.**
`release.rs` `commit_file` / `merge --ff-only` use `.status().expect()`, unlike every
sibling helper. A fixture whose setup git command fails silently produces a test
that asserts against the wrong repo state.

**W-12 — A failure to compute the changelog body is swallowed into a `warn!`.**
The shipped, committed `CHANGELOG.md` then claims "No changes recorded since the
previous release" on an otherwise-green Ship.

**W-13 — `release_execute.rs` does not scrub redirecting git environment variables.**
It does not use `hermetic_command`, leaving `GIT_DIR` / `GIT_CONFIG_GLOBAL` able to
retarget the binary-under-test's git calls at the developer's real repo.

**W-14 — Step 1 pushes the entire local `develop` branch with no currency check against `origin/develop`.**
`release.rs:317`. Any unrelated local commits on `develop` ride along into the
release push.

**W-15 — A sync merge that git cannot auto-resolve leaves the repo mid-merge with a conflicted index, and nothing says so.**
`sync.rs`. The operator is left in a broken state with no diagnostic.

**W-16 — `cargo publish` leaves the tracked `Cargo.lock` modified but uncommitted.**
The tagged release commit's lockfile disagrees with its manifest.

**W-17 — The live `develop` ruleset blocks this phase's headline capability, and the docs say otherwise.**
`CONTRIBUTING.md:238`, `README.md:36-39`. The live GitHub ruleset
`develop-merge-or-squash` is `enforcement: active` with `bypass_actors: []` and
`current_user_can_bypass: "never"` (unchanged since 2026-07-23), so `develop` is
still PR-protected and the direct push in step 1 cannot land against this
repository. Consequently `26-VERIFICATION.md:272-276`'s recommended operator run
fails at step 1, and all three UAT items are gated on a precondition proven
absent. *(Merged: external-state ×2.)*

**W-18 — `ReleaseReport::steps`' doc promises one entry per step; the publish loop emits one per package and the resume arm emits a duplicate.**
`release.rs:104`, `:267`, `:513-534`. Consumers using `steps.iter().find(...)`
retrieve a stale verdict. *(Merged: doc-accuracy + ci-build + generalist.)*

**W-19 — CONTRIBUTING's "environment preconditions" list omits the two hard entry guards.**
It omits `DirtyWorkingTree` and `NotOnDevelop`, while step 1 tells you to run
`cargo build` — which trips the first one on the required re-run.

**W-20 — CONTRIBUTING step 1's "before pushing further work" window does not exist.**
The executor performs write, commit, and push in a single call.

**W-21 — Both operator docs say "re-run the same command" with no terminal-state caveat.**
`release.rs:21-25` documents that a re-run after completion pushes a *fresh* bump
commit. See C-02 for the worse case.

**W-22 — `release_execute`'s doc comment asserts an authorization guarantee nothing enforces.**
`commands.rs:2274`. It claims "it cannot be called any other way" and cites, as the
cause, the very fact that removes the guarantee. The gate does hold (see above) —
but it holds at `main.rs:629`, not here, and this comment would license a future
caller to bypass it.

**W-23 — `devflow sync` direct-pushes a merge commit to `origin/develop` with no authorization flag of any kind.**
`main.rs:266`, `commands.rs:2243`. A `-X ours` merge commit is pushed to a shared
branch on a bare `devflow sync`. This is D-07/D-08's stated intent — it ports a
script operators previously ran by hand — but it means the "no mutation without a
typed flag" property holds only for the `release` half of the sequence, not the
sync half. Worth an explicit decision rather than an inherited one.

**W-24 — `execute_release` is `pub` in `devflow-core`, a crate published to crates.io.**
The `--yes-release` gate is a CLI-surface gate only; any downstream library
consumer can call the release sequence directly. Not a defect under this phase's
threat model, but the API is now public and irreversible.

---

## Info Findings (5)

**I-01** — The signing key identifier is placed in argv (`-c user.signingkey=…`), disclosing the maintainer's key path to any local user via `/proc`. Public key path only; no secret material.

**I-02** — "five steps" appears throughout the release docs; `ReleaseStep` has four variants and a completed report contains four step kinds.

**I-03** — `26-VALIDATION.md:30`'s test ledger calls 406 the pre-phase baseline; the merge-base count is 396, silently absorbing 26-02's 10 tests.

**I-04** — `check_self_pin`'s "could not read Cargo.toml" and "not a workspace Cargo.toml" verdicts are `"warn"`, hence non-blocking on the execute path.

**I-05** — `publish_order` includes workspace members marked `publish = false`.

---

## Hypotheses raised and refuted (recorded so they are not re-litigated)

The finders were required to attempt refutation before reporting. These were
dropped after verification against the live toolchain and repo:

- **Credentials in the remote URL do not leak.** git 2.55 anonymizes userinfo in
  both `unable to access` and `Authentication failed for`, verified against a
  local 401 server. Downgraded to a documentation defect.
- **No shell interpolation anywhere** — all subprocess calls use argv vectors.
  No reachable argument injection after enumerating every non-literal argv
  element. No `git add -A` / `commit_all` in any release path. No new network
  dependencies, no telemetry, no env capture, no hardcoded secrets.
- **`cargo info --registry crates-io` does not resolve a same-named local
  workspace member** (exit 101, `could not find`), and `@X.Y.Z` is exact, not a
  caret range — so `classify_cargo_info_result` has no live false
  `AlreadyPublished`.
- **`commit_path`'s "nothing to commit" → `Ok` masking is unreachable** when
  `write_needed` is true, since `write_version` errors if the field is missing.
- **The `.unwrap_or(false)` degradations in `is_ancestor` / `local_tag_is_verifiable`
  all degrade toward "act"/"refuse", never "skip"** — not fail-open.
- **CI itself is sound for this angle.** No workflow or script changed.
  `scripts/check.sh test` runs `cargo test --workspace --no-fail-fast` with no
  name filter, so the repo's known `cargo test --exact <nonexistent>` false-green
  trap is absent here. All 13 new core tests and 16 CLI `release_*` tests pass
  with 0 ignored; none are `#[ignore]`d, cfg'd out, or env-gated.
- **The phase's own artifacts are honest.** All 17 claimed commit hashes exist,
  every `26-VERIFICATION.md` line-number citation is exact, `release.rs` really
  is 1151 lines, all guard greps reproduce, and the suites re-run at the exact
  claimed counts (434 lib / 0 failed; 16 CLI binaries green). **No SUMMARY
  claims a merge, tag, publish, PR, or deletion that did not happen.** The
  `--help` snapshot matches the binary byte-for-byte.

## Audit-Fix Pass — 2026-07-30 (`/gsd-audit-fix 26`)

Five of the seven Criticals were classified auto-fixable and are fixed, each
with a test that fails against the prior code, each committed atomically with
its finding ID. Full gate re-run after the last commit: **436 lib tests**
(434 + 2 new) / 0 failed, **302 CLI tests across 16 targets** / 0 failed,
`clippy --workspace --all-targets -D warnings` clean, `fmt --check` clean.

| ID | Status | Commit | Fix |
|----|--------|--------|-----|
| C-05 | fixed | `e4a3236` | `members_key_offset` matches the exact `members` key, so `default-members` can no longer truncate the publish set. New test asserts both key orderings and the only-`default-members` case. |
| C-03 | fixed | `43a7a96` | `execute_release` returns `ReleaseFailure { error, steps }`; the sequence moved into `run_release`, which borrows the ledger, so every failure path reports what already landed. The CLI prints the ledger on both paths and states the steps are not rolled back. |
| C-04 | fixed | `8f5f2d1` | New `ReleaseOutcome::CompletedWithoutPublish`; the CLI can no longer print an unqualified "release cut complete" when the registry received nothing. Deliberately not an error — `publish_order` reads a Cargo workspace's `members`, so single-crate and non-Rust projects legitimately resolve to none. |
| C-01 | fixed | `7bd9a37` | The `UnreachableBaseline` arm resolves the tag's target commit and resumes only when it names `origin/main`'s tip; anything else is `StrayBaselineTag`, refused before any mutation. New test asserts the remote `develop` ref and the version file are byte-identical across the refusal. **Also fixes WR-01/W-18** — the resume note folds into step 1's own report instead of a second `VersionBump` entry, and the one-entry-per-step contract is now asserted (the duplicate was reproduced live by the new assertion before the fix). |
| C-07 | fixed | `0f0e17a` | README's manual repair now fetches, captures `HEAD^{tree}` before and after, aborts on a tree change, and pushes only when the merge is content-neutral. The inverted `git diff --stat` check is called out explicitly so it is not reintroduced. |

**Still blocking — classified manual-only, not attempted:**

- **C-02** (unresumable publish). The recommendation below is a persisted step
  ledger, which directly contradicts recorded decision **D-06** ("every step
  consults a live-state predicate rather than a persisted progress file"). An
  operator has to re-open D-06 before this can be fixed; picking a design here
  unilaterally would overwrite a recorded decision.
- **C-06** (`project_root` walks up to an ancestor `.devflow`, so a worktree run
  retargets the release at the parent checkout). `project_root` is the resolver
  for *every* command; whether mutating commands should refuse, warn, or scope
  to the invoking worktree is a cross-cutting policy decision, not a local fix.

The 24 Warnings were classified in scope at `--severity medium` but not
attempted — `--max 5` was exhausted by the high-severity set. **W-17 deserves
attention independent of the code**: the live `develop` ruleset is
`enforcement: active` with an empty bypass list, so the direct push in step 1
cannot land against this repository and all three `26-UAT.md` items are gated
on a precondition proven absent. That is a repository-settings action, not a
code change. The 5 Info findings are below the severity threshold.

---

## Recommendation

Do not ship. C-01, C-02, C-03, and C-04 are all reachable on the ordinary
operator path and all concern the one irreversible step (`cargo publish`) or a
push to a shared branch; C-01 and C-02 were reproduced by execution. C-06 is
specific to this project's own worktree-based working posture, which is how the
release would in fact be run.

The cluster has a common root worth fixing as one design change rather than
seven patches: the executor treats *issuing* a step as *completing* it, and
carries no durable record of what has already mutated external state. A
persisted step ledger that survives failure would address C-02, C-03, and C-04
together, and would give C-01 a safe refusal point.

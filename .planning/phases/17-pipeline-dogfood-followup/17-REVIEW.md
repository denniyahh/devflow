---
phase: 17-pipeline-dogfood-followup
reviewed: 2026-07-19T00:00:00Z
round: 4
depth: deep
angles:
  - doc-accuracy cross-reference
  - security / leaked-data
  - CI / build correctness
  - external-state claims
  - generalist deep pass
files_reviewed: 21
files_reviewed_list:
  - .github/workflows/ci.yml
  - .github/workflows/devcontainer.yml
  - CHANGELOG.md
  - OPERATIONS.md
  - crates/devflow-cli/build.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/version.rs
  - crates/devflow-core/tests/monitor_e2e.rs
findings:
  critical: 3
  warning: 14
  info: 12
  total: 29
status: issues_found
ship_gate: BLOCKED
---

# Phase 17: Code Review Report — Round 4 (five-angle deep)

**Reviewed:** 2026-07-19 (round 4)
**Depth:** deep — five parallel angles, merged and deduplicated
**Files Reviewed:** 21
**Status:** issues_found — **3 Critical**
**Ship gate:** BLOCKED

Angles run: doc-accuracy cross-reference · security/leaked-data · CI/build correctness ·
external-state claims · generalist deep pass. Each Critical below was independently
re-verified by the orchestrator against source or live git state before being recorded.

---

## Critical

### CR-01 (R4) — `CHANGELOG.md` claims a release (`1.4.106`) that does not exist

**File:** `CHANGELOG.md:3-5` (uncommitted working-tree edit)
**Angles:** external-state, doc-accuracy, generalist (I-5)

The working tree adds:

```
## 1.4.106 — 2026-07-19

- Released phase via DevFlow.
```

Verified against live state:

| Check | Result |
|---|---|
| `git tag -l` | `v1.0.1 v1.2.0 v1.3.0 v1.3.69` — no `v1.4.106` |
| `git ls-remote --tags origin` | tops out at `v1.3.69` |
| `gh release list` | `v1.3.69` (latest, 2026-07-18) |
| `Cargo.toml` | `version = "1.3.69"` — no bump |

No tag, no release, no version bump backs this heading. This is the **third recurrence** of
the defect this phase's own REVIEW designates CR-01 (R1 caught `1.4.26`, R2 caught `1.4.88`).
The root cause is `changelog_append()` having run without a matching `version_bump()`.

**Failure scenario:** the edit is staged and merged → a false release claim is embedded in
`develop`'s permanent history, and every consumer reading CHANGELOG.md believes 1.4.106 shipped.

**Fix:** drop the uncommitted CHANGELOG.md hunk before merge (`git checkout -- CHANGELOG.md`).

---

### CR-02 (R4) — `write_version` corrupts `package.json` by dropping the trailing comma

**File:** `crates/devflow-core/src/version.rs:253-297` (`replace_version_in_contents`)
**Angle:** generalist deep pass

The rewritten line is reassembled as `left.trim_end() + separator + quoted_version`, then a
newline — **everything after the value token is discarded**, including JSON's mandatory `,`
(and, in TOML, any trailing comment).

Traced on a real input:

```
in : {"name": "x", "version": "0.1.0", "private": true}
       line  ->   `  "version": "0.1.0",`
       split_once([':']) -> left=`  "version"`, value=` "0.1.0",`
       emitted           -> `  "version": "2.3.4"`      <-- comma gone
out: invalid JSON
```

**Reachable path:** `Hook::VersionBump` → `version::write_version` on any Node project where
`version` is not the last key — i.e. essentially every real `package.json`. `version_bump`
then `commit_path`s the corrupted file and tags it, so the corruption lands in history.

**Test that hides it:** `read_version_round_trips_through_write_version_in_package_json`
(`version.rs:526`) uses the fixture `{\n  "version": "0.1.0"\n}` — a single-key object with no
trailing comma. The round-trip passes vacuously and would pass against this broken
implementation. The Cargo.toml variants have the same blind spot for trailing comments.

**Fix:** emit `package.json` through a real JSON writer, or preserve the line remainder after
the value token verbatim. Add a fixture with a key following `version`.

---

### CR-03 (R4) — `hooks_after_ship()` desyncs the CHANGELOG heading from the tag when no version file exists

**File:** `crates/devflow-core/src/hooks.rs:190-241`
**Angle:** generalist deep pass

`version_bump` takes the `else` branch on a project with no `Cargo.toml`/`pyproject.toml`/
`package.json` (`warn!("no supported version file; tagging only")`) and **still tags**
`v{compute_version()}`. `changelog_append`, which runs next, calls `version::read_version` —
which errors with "no version file found" — and falls back to the literal `"unreleased"`.

Result on such a project:

```
TAG       = v0.0.3
CHANGELOG = ## unreleased — 2026-07-19
```

This is precisely the three-way tag ↔ changelog ↔ version-file agreement that 17-12 / WR-04
claims to have established. The regression test
`after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean` (`hooks.rs:445`)
cannot catch it because its `init_repo` helper always writes a `Cargo.toml`.

**Fix:** have `version_bump` return the version it actually tagged and thread it into
`changelog_append`, rather than round-tripping through a file that may not exist. Add a
no-version-file case to the after-ship batch test.

---

## Warning

### WR-01 — `build.rs` dirty flag is scoped asymmetrically with the live check, defeating the D-18 block
`crates/devflow-cli/build.rs:52-54` vs `main.rs` `tree_has_modified_build_inputs` / `combined_staleness`

`build.rs` derives `DEVFLOW_BUILD_DIRTY` from **unfiltered** `git status --porcelain` (any
dirty file, including `.planning/` and `CHANGELOG.md`), while the runtime arm filters through
`affects_compiled_binary`. A build made while only `.planning/` was dirty embeds `dirty=true`,
which routes `combined_staleness` down `Some(true) if build_dirty => Indeterminate` and
downgrades a genuine Stale to a warning. During a DevFlow self-run `.planning/` is dirty
essentially always — so the D-18 hard block is systematically weakened on the one workspace it
exists to protect. Fix: apply `affects_compiled_binary` in `build.rs` too.

### WR-02 — Self-dogfood staleness gate can brick a `--no-worktree` run mid-flight
`crates/devflow-cli/src/main.rs:874` (`embedded_commit_is_stale`), `:1092` (`enforce_build_staleness`)

Exit 0 from `merge-base --is-ancestor` is `Fresh` only on an **exact** HEAD match; a strict
ancestor is `Stale`. In `--no-worktree` mode the agent commits into `project_root`'s checkout,
so HEAD advances during the run and the next `launch_stage` sees the embedded commit as a
strict ancestor → `Stale` → self-dogfood → `StalenessOutcome::Block`, a hard `Err` that is
deliberately *not* an approvable gate. The phase is then stuck with no in-run recovery.
Recorded as Warning rather than Critical: the binary genuinely *is* stale, so the gate is
behaving as D-18 specifies — the defect is the absence of a recovery path, not a wrong verdict.
Fix: evaluate the block once per `devflow start`, or compare against the phase base commit.

### WR-03 — `run_preflight` recurses unboundedly on a deterministically-failing check
`crates/devflow-cli/src/main.rs:161-190`

On failure it calls `run_gate`; on `Advance` or `LoopBack` it calls `launch_stage`, which calls
`run_preflight` again. No depth counter, no attempt bound. Unlike `handle_stage_failure` (which
relaunches an agent whose behavior may differ), preflight predicates are deterministic —
`preflight_interactivity_check` re-reads the same absent `CONTEXT.md`, `preflight_gh_auth_check`
re-runs the same `gh auth status`. An operator who approves "advance" without fixing the
external condition gets an infinite gate→relaunch→gate cycle, one stack frame deeper each time.
Only the 7-day gate timeout breaks it.

### WR-04 — `find_version_in_contents` has no nesting awareness for `package.json`
`crates/devflow-core/src/version.rs:226-251`

For `package.json`, `field_for` returns the bare key `version`, so `section` and `current` are
both `""` for every line (JSON has no `[...]` headers). The first line whose left side is
`version` wins **at any nesting depth**. A `package.json` with an `overrides`/`resolutions`/
`engines` block containing a nested `"version"` ahead of the top-level one yields the wrong
version — and `replace_version_in_contents` rewrites that nested entry instead.

### WR-05 — `docs_update` retains the sweeping `git add .`
`crates/devflow-core/src/hooks.rs:178` → `crates/devflow-core/src/git.rs:310`

Phase 17 migrated `changelog_append` and `version_bump` to scoped `commit_path`, but
`docs_update` still calls `commit_all`, which does `git add .`. `cargo doc` writes only to
gitignored `target/`, so no doc artifact leaks — but any non-gitignored file sitting in the
tree when Validate→Ship fires (a forgotten `.env`, a credential file, a scratch artifact) is
staged and committed. This is the last unscoped `git add .` in the hook pipeline and the exact
sweep-in class the phase set out to close.

### WR-06 — `next_agents` is not shell-quoted in the Hermes cron command
`crates/devflow-core/src/ship.rs:182`

```rust
command: format!(
    "cd {} && devflow sequentagent --phase {phase} --agents {next_agents}",
    shell_quote(&project)   // project quoted; next_agents is not
),
```

`{phase}` is a `u32` (safe) and `project` is quoted, but `next_agents` is interpolated raw into
a string Hermes runs via `sh -c`. The only current call site sources it from the operator's own
`--agents` flag, so exploitation today requires a self-targeting operator. The issue is
structural: the moment `next_agents` derives from agent output or state, this becomes an
agent-controlled injection channel. Fix: `shell_quote(next_agents)`.

### WR-07 — `resource_killed_on_code_bumps_infra_failures_not_consecutive_failures` asserts neither counter
`crates/devflow-cli/src/main.rs` (test, ~line 1116)

The name and doc comment claim it proves `infra_failures` is bumped and `consecutive_failures`
is untouched. The body asserts only that `load_state` returns `MissingState` and that no
Validate gate file exists. Neither counter is read. It would pass unchanged against an
implementation that routed `ResourceKilled` through `handle_validate_outcome` and bumped
`consecutive_failures`. The Validate-stage sibling test (~1155) asserts both correctly — port
that shape.

### WR-08 — `code_unknown_does_not_transition_to_validate` can wedge CI for 7 days
`crates/devflow-cli/src/main.rs` (test, ~line 1039)

`advance()` runs on a `thread::scope` thread; the gate response is written only *after* the
polling loop's `assert!(seen, …)`. If the gate never appears — the exact regression this test
guards — the assert panics, and unwinding out of `thread::scope` joins a thread still blocked
in `Gates::poll_response` at the default `gate_timeout_secs()` of 7 days. The test does not
fail; it hangs. Fix: write the abort response before the assert, or set the gate-timeout env.

### WR-09 — `merge_feature` reports `"merged": false` for the already-merged case
`crates/devflow-core/src/hooks.rs:146-155`

The "already merged, nothing to do" success path emits `merge_result {"merged": false}` —
indistinguishable in `events.jsonl` from a merge that did not happen. Nothing consumes it today
(`merge_result` appears only at `hooks.rs:151,162`), but 18d's reconciliation is designed to
read these events. Use `"already_merged"` or add an `already` flag.

### WR-10 — `monitor_e2e` does not test the state transition it exists to guard
`crates/devflow-core/tests/monitor_e2e.rs:8-12`

The test's own header documents it: the monitor's tail `devflow check` self-call is a no-op
here, because cargo runs the `devflow-core` test binary, not the `devflow` CLI — it re-invokes
the test binary with a non-matching filter, which exits 0. This is the known false-green shape.
The test asserts only that the monitor wrote its capture files (exit code, stdout, pid). A
completely broken `devflow check` state-advance path would leave all of those intact and this
test would still pass.

### WR-11 — `17-VERIFICATION.md` Required-Artifacts row is stale and non-reproducible
`.planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md` (Required Artifacts table)

Asserts `ROADMAP.md:194` reads `"Plans: 11/11 plans executed"` with `17-11-PLAN.md` at `:199`.
Actual `ROADMAP.md:194` reads `**Plans:** 12/12 plans executed`, and `17-12-PLAN.md` sits at
`:198`. VERIFICATION ran at 2026-07-19T21:34:26Z; plan 17-12 completed at T22:36:01Z. The
behavioral spot-check count ("367 passed across 9 suites") is stale for the same reason —
round 3 records 376 across 10 targets. An auditor re-running this checklist finds a mismatch.

### WR-12 — `17-12-SUMMARY.md` claims a proof its test did not provide, and omits the real fix
`.planning/phases/17-pipeline-dogfood-followup/17-12-SUMMARY.md:163`

Claims `git.rs` gained "a direct unit test proving it doesn't sweep in unrelated dirt". In
commit `31757ef` the original `commit_path` called `git commit` with **no pathspec** (only
`git add <path>` was scoped), and the test left `unrelated.txt` *untracked* — any commit
trivially excludes untracked files, so it proved nothing about staged-but-unrelated files.
The real production bug and the test gap were closed later in `39e2e65`, which appears nowhere
in this SUMMARY's task commits, files-modified, or deviations sections. Add it.

### WR-13 — Round-3 REVIEW header's "63 commits ahead of develop" was stale
`.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` @ `ed374a2` (round-3 header,
superseded by this document)

`git rev-list --count develop..HEAD` → **81**. The branch grew by 18 commits (17-12 plans,
final review/validation artifacts, `fe7ed22`, `ed374a2`) after that line was written. Recorded
so the same drift is not reintroduced in the next round's header.

### WR-14 — `devcontainer.yml` is not guaranteed to be a required branch-protection check
`.github/workflows/devcontainer.yml`

The devcontainer job lives in a second, independent workflow file. If branch protection on
`develop`/`main` lists only `ci.yml`'s three jobs (`Test`, `Clippy`, `Format`), this workflow
can be skipped entirely and container-environment failures (image drift, volume corruption,
container-specific paths) never surface before merge. The `devcontainer_ci_failfast.rs`
regression guard mitigates the `set -e` case specifically, but not the environment class. No
branch-protection config exists in-repo, so this cannot be closed from code alone.

---

## Info

- **IN-01** — `version_bump` (`hooks.rs:230`) passes `path.file_name()` to `commit_path`,
  silently discarding any directory component. Correct only because `detect_version_file`
  never returns a nested path.
- **IN-02** — `run_preflight`'s recursive `launch_stage(state, None, None)` (`main.rs:179,183`)
  passes `archived_stage: None`, so `capture_archived` labels the generation with the *current*
  stage; `handle_stage_failure` correctly passes `Some(stage)` in the same situation.
- **IN-03** — `affects_compiled_binary` (`main.rs` ~339) covers only `.rs` plus four named
  files. Assets pulled in via `include_str!`/`include_bytes!`, `.proto`, or compiled-in `.toml`
  config won't trip the live arm.
- **IN-04** — `commit_path` uses `--allow-empty`, so `VersionBump` and `ChangelogAppend` each
  create a commit on every ship even when nothing changed. With `patch = commits-since-last-tag`,
  each ship's tag sits one commit behind the height its own version names.
- **IN-05** — `workflow_started_payload` (`main.rs:839-846`) serializes `exe_path` and
  `worktree` into the event. These land in gitignored `.devflow/events.jsonl` (regression-tested
  by `gitignore_coverage.rs`), so no git leak — but operator filesystem layout is unscrubbed if
  events are ever forwarded to a remote backend.
- **IN-06** — Gate context (first 300 chars of an agent failure reason via `truncate_reason`,
  `gates.rs:300-306`) is passed as `DEVFLOW_GATE_CONTEXT` to `DEVFLOW_GATE_NOTIFY_CMD`. Not
  shell-interpolated (correct). A credential echoed in an agent's `reason` would reach external
  notification infra up to that cap.
- **IN-07** — `ci.yml:20` runs `cargo test` without `--workspace`, diverging textually from
  `devcontainer.yml`'s `cargo test --workspace`. Functionally equivalent for a virtual
  workspace; becomes a real divergence if the root `Cargo.toml` gains a `[package]` section.
- **IN-08** — `commit_path` passes `--allow-empty`, making its
  `Err(GitError::Command(ref msg)) if msg.contains("nothing to commit")` arm permanently dead.
  Misleading if `--allow-empty` is later removed.
- **IN-09** — `scripts/install.sh:74` discards all stderr on `cargo install devflow` failure.
- **IN-10** — `scripts/install.sh:108` — `devflow doctor 2>/dev/null || warn` always exits 0;
  a broken post-install binary reports success. Not CI-triggered.
- **IN-11** — `scripts/deploy.sh:28` uses `gh api ... || echo "warning"`, so a failed Pages
  configuration never fails the script. Not referenced from any workflow today.
- **IN-12** — Local `develop` (`a2c314f`) is 18 commits ahead of `origin/develop` (`c034ad7`).
  No artifact falsely claims otherwise, but a merge/release prepared without reading this hits
  a non-trivial divergence.

---

## Angles that found nothing

**Security — clean areas (verified, not assumed):** no hardcoded secrets, tokens, or
credentials in any committed file. No `.devflow/` runtime state tracked —
`git ls-files | rg -i 'env|secret|token|credential|session'` matches only `events.rs` and
`state.rs` source files. Planning-artifact scan for `ghp_`, `github_pat_`, `sk-`, `AKIA`,
`Bearer`, `xox*` → zero matches. CI workflows carry no `pull_request_target`, no `secrets.*`
references, no env echoing. `gh auth status` output is never captured or logged — only a
boolean propagates (T-17-13, `main.rs:753-754`). `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY`
are a SHA and a boolean.

**CI false-green — no Critical.** No `continue-on-error: true`, `|| true`, `|| exit 0`,
`set +e`, unprotected pipes, or bare `cargo test --exact` in any workflow. The `build_provenance`
rework is correct: the `NEVER-EXISTS` sentinel unconditionally forces rerun, and the regression
test reads the cached `output` file rather than the embedded `env!` value.

**External-state — all named SHAs are real and do what they claim:** `3e39cf6`, `fd065e3`,
`71c4ebd`, `cb9ddab`, `46058a7`, `3d6e6a6`, `41345fc`, `92581fa`, `a3a1067`, `b81ec7d`,
`5431f9e`, `f531d08`, `a2c314f`. No artifact claims Phase 17 is merged, PR'd, or pushed —
correct: PRs #1–#10 exist, none for Phase 17.

**Doc-accuracy — verified correct:** all test names cited across the 12 SUMMARY coverage tables
exist at their stated locations; `hooks_for_transition(Validate, Ship) = [DocsUpdate]` and
`hooks_after_ship() = [Merge, VersionBump, ChangelogAppend, BranchCleanup]` match source;
`changelog_append` uses `read_version` not `compute_version`; `devflow resume --phase N` exists
in both CLI and OPERATIONS.md; all four OPERATIONS.md env vars are wired to source;
`STATE.md total_plans: 46` is correct.

---

## Ship gate

**BLOCKED — 3 Critical.**

CR-01 is a one-command fix (drop the uncommitted CHANGELOG hunk) and must happen before any
merge. CR-02 and CR-03 are real code defects in the version/changelog hook path that this
phase specifically claims to have hardened; both are masked by tests whose fixtures avoid the
failing shape. Fix all three, then re-run `/gsd-code-review 17` before shipping.

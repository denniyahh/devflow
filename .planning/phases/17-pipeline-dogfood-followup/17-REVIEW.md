---
phase: 17-pipeline-dogfood-followup
reviewed: 2026-07-19T17:10:00Z
depth: deep
files_reviewed: 14
files_reviewed_list:
  - CHANGELOG.md
  - OPERATIONS.md
  - crates/devflow-cli/build.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
review_angles:
  - doc-accuracy-cross-reference
  - security-leaked-data
  - ci-build-correctness
  - external-state-claims
  - generalist-deep
findings:
  critical: 3
  warning: 9
  info: 6
  total: 18
status: issues_found
---

# Phase 17: Code Review Report

**Reviewed:** 2026-07-19T17:10:00Z
**Depth:** deep (5 parallel angles, merged and deduplicated)
**Files Reviewed:** 14
**Status:** issues_found — **3 Critical**

## Summary

This review supersedes the standard-depth pass captured at commit `3caf985`. It was run as
five independent angles — doc-accuracy cross-reference, security/leaked-data, CI/build
correctness, external-state claims, and a generalist deep pass — merged and deduplicated here.
Every Critical was independently re-verified by the orchestrator before being recorded.

**Baseline is genuinely green.** `cargo build --workspace`, `cargo test --workspace`
(364 passed / 0 failed across 10 targets), `cargo clippy --workspace --all-targets -- -D warnings`,
and `cargo fmt --check` all pass at `6995b6f`. Exit codes were captured directly, not through a
pipe. Nothing in this report is a currently-failing test.

**Prior findings, re-verified:**

| ID | Status |
|---|---|
| CR-01 (`run_preflight` double-launch) | **Genuinely fixed** (`c03498d`). `run_preflight` returns `Result<bool>`; the single production call site at `main.rs:1126` short-circuits on `Ok(false)`. Two RED-verified regression tests cover the Advance and LoopBack arms. |
| WR-01 (staleness warning noise) | **Still open** — see WR-01 below. |
| WR-02 (substring self-dogfood match) | **Still open** — `is_self_dogfood_workspace` is byte-identical to the reviewed version. |
| WR-03 (`gate_or_abort_infra` doc comment) | **Still open** — doc at `main.rs:1367` still misdescribes the call graph. |

**Security angle is clean at Critical.** No network egress anywhere in `crates/` (no HTTP
dependency in any `Cargo.toml`), no credentials or session data committed, no shell injection
(all 26 `Command::new` sites use argv arrays), and `gh auth status` output is discarded rather
than logged. All 15 runtime-written paths are correctly gitignored today.

The three Criticals below are: a false release claim that a merge would propagate into the
project changelog, a build-provenance gate that silently certifies stale binaries under CI
conditions, and a rate-limit path that violates the never-silent invariant this very phase exists
to enforce.

---

## Critical Issues

### CR-01: `CHANGELOG.md` asserts a release (1.4.26) that does not exist

**File:** `CHANGELOG.md:3` (introduced by `bde8f73`)

**Issue:** The diff adds a dated release heading for a version that exists nowhere else in the
project:

```
## 1.4.26 — 2026-07-19

- Released phase via DevFlow.
```

Verified against actual repository state:

```
$ git tag -l
v1.0.1  v1.2.0  v1.3.0  v1.3.69          # no v1.4.26, local or on origin

$ rg -n '^version' Cargo.toml
9:version = "1.3.69"                      # workspace still at 1.3.69

$ devflow --version
devflow 1.3.69
```

The string `1.4.26` appears in exactly two places in the tree: this changelog heading and a
tempdir test fixture at `main.rs:5316`. The branch's own `.planning/STATE.md` Completed table
records Phase 17 with Version `—`, directly contradicting the changelog.

**Root cause** is visible in `crates/devflow-core/src/hooks.rs`: `changelog_append()` (`:173`) and
`version_bump()` (`:183`) each independently call `version::compute_version()`, which derives
`minor = count_git_tags()` (now 4) and `patch = commits_since_last_minor_tag()` — yielding 1.4.26.
`changelog_append` ran and wrote the heading; `version_bump`, which writes `Cargo.toml` and
creates the `v{version}` tag, did not. This is not an unsupported-format fallback: `write_version`
handles `[workspace.package]` correctly, proven by the passing test
`write_version_replaces_in_workspace_cargo_toml` (`version.rs:415`).

**Impact:** Merging this puts a dated, false release record at the top of the project's changelog.
A user reading it sees 1.4.26 as current, while `devflow --version` prints 1.3.69 and no such tag
is fetchable. This is the only external-state assertion on the branch that is provably false, and
it is the one a merge propagates. It is also the single user-facing doc claim the phase adds.

Note the `bde8f73` "move stranded changelog entry onto the branch it describes" operation itself
is sound — no entry was dropped or duplicated (`git log --all -S'1.4.26' -- CHANGELOG.md` returns
exactly one commit; `develop` and `main` changelogs are untouched at 1.3.69). The defect is the
entry's *content*, not the move.

**Fix:** Either remove the heading until `version_bump` actually runs and tags, or make
`changelog_append` and `version_bump` share one computed version and run atomically so a changelog
heading can never outlive its tag. See WR-04 for the related ordering defect in the same hook batch.

---

### CR-02: `build.rs` never re-runs on working-tree changes — the staleness gate certifies stale binaries as Fresh

**File:** `crates/devflow-cli/build.rs:36-41` (declared triggers) vs `:48` and `:67` (actual inputs)

**Issue:** Input-versus-trigger audit of the build script:

| Input read | Covered by a `rerun-if-changed`? |
|---|---|
| `git rev-parse HEAD` (`:47`) | Yes — `.git/refs` / `HEAD` / `packed-refs` |
| `git status --porcelain` (`:48`) — reads the **entire working tree** | **No trigger at all** |
| `SystemTime::now()` (`:67`) | **No trigger possible; never refreshed** |

Reproduced in a fresh `git clone` — exactly what `actions/checkout@v4` produces, and which has
`packed-refs` present:

```
$ git clone --no-hardlinks <worktree> /tmp/dfprobe && cd /tmp/dfprobe
$ ls .git/packed-refs
.git/packed-refs                              # CI-like: packed refs exist

=== BUILD 1 ===
DEVFLOW_BUILD_COMMIT=6995b6f0922ccf5fd3f663c6effd33551f10f2e2
DEVFLOW_BUILD_DIRTY=false
DEVFLOW_BUILD_TIMESTAMP=1784493764

# append a line to crates/devflow-cli/src/main.rs, rebuild (crate DOES recompile)
$ git status --porcelain
 M crates/devflow-cli/src/main.rs              # git says dirty

=== BUILD 2 ===
DEVFLOW_BUILD_COMMIT=6995b6f0922ccf5fd3f663c6effd33551f10f2e2
DEVFLOW_BUILD_DIRTY=false                      # still false
DEVFLOW_BUILD_TIMESTAMP=1784493764             # byte-identical, unchanged
```

The binary is rebuilt from modified source but embeds `dirty=false` and the *previous* build's
timestamp. Both values feed `enforce_build_staleness(...)` (`main.rs:1133-1138`) and the
`workflow_started` payload (`main.rs:841-843`).

**Impact:** The build-provenance/staleness gate is the headline deliverable of 17d, and its stated
purpose is catching "you forgot to rebuild." Under CI conditions it reports a binary carrying
uncommitted changes as a clean build of the old commit — the exact failure it exists to prevent.
Commits *are* covered (a commit bumps `.git/refs/heads/...` and correctly re-triggers); uncommitted
edits are not.

**This is currently masked on the developer machine.** `/var/home/denniyahh/Github/devflow/.git/packed-refs`
does not exist, and cargo treats a missing `rerun-if-changed` path as *always rerun* — so locally
the provenance looks correct by accident. Any `git gc`, or any CI checkout, creates `packed-refs`
and the bug appears. That is why no test catches it, and why the phase's own dogfood runs did not
surface it.

**Fix:** `git status --porcelain` cannot be fingerprinted by path. Either drop the caching intent
(emit `cargo:rerun-if-changed=` against a non-existent sentinel, accepting one build-script run per
build) or stop embedding a dirty flag and timestamp that the trigger set cannot honor. Add a test
that builds twice across a working-tree edit and asserts the provenance actually changed.

---

### CR-03: rate-limit auto-resume stalls the phase silently when the retry hint isn't a timestamp

**File:** `crates/devflow-cli/src/main.rs:1417-1421` (and the `sequentagent` path at `:2344`)

**Issue:**

```rust
if instructions.hermes_cron.schedule.is_empty() {
    println!("no parseable retry time — auto-resume cron not scheduled; resume manually");
} else { ... }
// ... events::emit(...) ...
Ok(())
```

There is **no gate and no `fire_gate_notify`** on the empty-schedule branch — verified by reading
through to the function's `Ok(())` at `main.rs:1443`.

**Traced failure path:** `detect_claude_rate_limit` (`agent_result.rs:178`) falls back to the
literal `Some("usage limit".to_string())` when a 429 payload carries no `retry_after`/`message`/`error`
field. `rate_limited_result` (`agent_result.rs:518`) builds `reason: "rate limited until usage limit"`.
`retry_after_from_reason` (`main.rs:2327`) strips the prefix, yielding `"usage limit"`.
`cron_schedule_from_retry_after("usage limit")` returns `None` (correctly — WR-06 forbids turning
unparseable agent output into an every-minute cron), so the schedule is empty.

**Concrete scenario:** Claude returns `{"subtype":"error_rate_limit"}` with no retry field.
`advance()` dispatches `Action::AutoResume` → `handle_rate_limited_outcome` prints to the
**detached monitor process's stdout**, which nobody reads → emits an event → returns `Ok(())`.
The monitor exits. `state.gate_pending` is untouched, no gate file is written, no notification
fires. The phase is permanently stalled with zero operator signal.

**Impact:** This is precisely the "never-silent" invariant (WR-11 / D-15) that the rest of this
phase enforces everywhere else. A rate limit with no parseable retry time is a routine upstream
condition, not an exotic edge case.

**Fix:** Route the empty-schedule branch through the same gate/notify path the infra ceiling uses,
so an operator is told the phase needs a manual resume rather than left with a silently dead monitor.

---

## Warnings

### WR-01: `enforce_build_staleness` prints a near-universally-firing, misleading warning for every non-self-dogfood project

**File:** `crates/devflow-cli/src/main.rs:1033-1041`, `:1082-1086`, `embedded_commit_is_stale` at `:895`

Carried forward from the prior review and **confirmed still open** — no commit touches it.

`embedded_commit_is_stale` shells `git merge-base --is-ancestor <DevFlow's embedded build commit> HEAD`
inside `project_root`. For the common case (DevFlow driving some *other* project), that commit does
not exist in the target's object store, so git exits `128` and the `_ => Staleness::Indeterminate`
arm catches it. `staleness_outcome(false, Indeterminate)` maps to `Warn` for every project:

```rust
(_, Staleness::Indeterminate) => StalenessOutcome::Warn,
```

So on essentially every stage launch of every ordinary project, DevFlow prints "build provenance
staleness check did not confirm a fresh build." Accurate but meaningless in that context, and it
trains operators to ignore DevFlow warnings generally — including the rare real ones from this same
codepath.

**Fix:** Short-circuit `enforce_build_staleness` for non-self-dogfood projects; the design already
treats that combination as a no-op-equivalent outcome, so no information is lost.

### WR-02: `is_self_dogfood_workspace` uses substring matching, which can false-positive-block an unrelated project

**File:** `crates/devflow-cli/src/main.rs:1002`

Confirmed still open. The check is `members.contains("crates/devflow-core") && members.contains("crates/devflow-cli")`
— `str::contains` is a substring match, not an exact array-element match. A workspace with members
like `crates/devflow-core-extras` / `crates/devflow-cli-plugin` satisfies both without containing
either real member, and would be classified as self-dogfood. Combined with the hard `Block` outcome
for self-dogfood + `Stale`, that hard-blocks an unrelated project's entire pipeline — the one
outcome this feature set exists to never inflict.

**Fix:** Split `members` on `,`, trim quotes and whitespace from each entry, and compare for exact
equality.

### WR-03: `gate_or_abort_infra`'s doc comment misdescribes the call graph

**File:** `crates/devflow-cli/src/main.rs:1367`

Confirmed still open. The comment claims the AutoResume arm "bumps `infra_failures` itself before
calling this." It does not — `handle_rate_limited_outcome`'s ceiling branch (`main.rs:1408`) calls
`handle_infra_outcome`, which performs the increment. Behaviour is correct (bumped exactly once);
the comment describes a call graph that does not exist, and a maintainer trusting it could
reintroduce a double-increment.

### WR-04: `ChangelogAppend` is written after the only committing hook, so the entry is never committed

**File:** `crates/devflow-cli/src/main.rs:1627` (`hook_context_root`, added in `ae744ed`) + `crates/devflow-core/src/hooks.rs:173`

`changelog_append` writes the file and never commits:

```rust
fn changelog_append(ctx: &HookContext) -> Result<(), HookError> {
    ...
    std::fs::write(&path, updated)?;      // writes, never commits
```

The batch order (`hooks.rs:81`) is `vec![Hook::DocsUpdate, Hook::ChangelogAppend]`, and `docs_update`
(`hooks.rs:159`) is the only hook that runs `git.commit_all(...)` — it runs **first**.

**Scenario:** Validate→Ship fires with a worktree configured. `hook_context_root` returns the
worktree; `docs_update` commits what is dirty at that moment; then `changelog_append` writes
`CHANGELOG.md` into the worktree, uncommitted. `Merge` (terminal batch, `project_root`) merges the
feature branch into develop, so the changelog edit reaches neither branch. `BranchCleanup`'s
non-force delete succeeds (the branch *is* merged), leaving an orphaned worktree holding the only
copy. Commit `bde8f73` is the operator hand-fixing this exact outcome — `ae744ed` relocated the
write but left the root cause in place, and moved it off the primary checkout where `git status`
would have surfaced it.

**Secondary:** `changelog_append` now calls `version::compute_version(&ctx.project_root)` against
the *worktree* HEAD. `commits_since_last_minor_tag` (`version.rs:110`) counts `tag..HEAD`, so the
worktree (N commits ahead) yields a different patch number than `VersionBump` later computes on
post-merge develop — the changelog heading and the git tag disagree. This is the same machinery
that produced CR-01.

### WR-05: CI's clippy gate does not lint test code — a required check passes on code clippy rejects

**File:** `.github/workflows/ci.yml:30` — `cargo clippy -- -D warnings` (no `--all-targets`)

Proved by planting a `needless_range_loop` in `tests/build_provenance.rs`:

```
=== ci.yml's exact command: cargo clippy -- -D warnings ===
exit=0                                     <- CI is GREEN

=== with --all-targets ===
error: the loop variable `i` is only used to index `v`
error: could not compile `devflow` (test "build_provenance")
exit=101
```

This branch adds ~2200 lines to `main.rs` plus test files; no `tests/` target is linted by CI today.

**Fix:** `cargo clippy --workspace --all-targets -- -D warnings`.

### WR-06: the devcontainer job shares the identical clippy scope gap

**File:** `.github/workflows/devcontainer.yml:26` — `cargo clippy --workspace -- -D warnings`

Has `--workspace` but still no `--all-targets`, so the second "CI-parity" job does not compensate
for WR-05. Both required checks share the same blind spot.

### WR-07: the `.gitignore` regression guard covers 3 of 15 runtime paths, omitting raw agent stdout

**File:** `crates/devflow-cli/tests/gitignore_coverage.rs:27-33`

The test asserts only `.devflow/state.json`, `.devflow/events.jsonl`, and `.devflow/gates/probe.json`.
Its own docstring states its purpose is preventing a repeat of commit `d021e3a`, where "a `.gitignore`
rewrite silently dropped coverage and leaked runtime telemetry into git."

Nothing leaks today — all 15 written paths verified `git check-ignore` IGNORED. But a future
`.gitignore` rewrite could drop `.devflow/phase-*-stdout`, `.devflow/phase-*-stderr.log`,
`.devflow/history/`, or `.devflow/cron-instructions*.json` and the guard stays green — including
raw agent stdout, the highest-value leak surface since it captures whatever an agent printed.
Phase 17 added `cron-instructions-NN.json` to the written set without extending the guard.

**Fix:** Extend the `check-ignore` argument list to all 15 paths. One line.

### WR-08: the cb9359f "bounded poll" is test-only — the production wedge it documents is still unbounded

**File:** `crates/devflow-cli/src/main.rs:1791-1813` (`finish_workflow`), gate default at `:30-33`

`git show cb9359f` touches **only** `mod tests` — it sets `DEVFLOW_GATE_TIMEOUT_SECS=2` inside
`concurrent_ship_advances_finish_both_phases_independently`. No production code changed:

```rust
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
raw.and_then(|s| s.parse().ok()).unwrap_or(SEVEN_DAYS)
```

**Scenario** (from the commit's own RED evidence): two phases reach `version_bump` concurrently and
compute the same version; the loser's `git tag` fails with "reference already exists";
`finish_workflow`'s loop calls `run_gate` → `Gates::poll_response(..., gate_timeout_secs())` →
**7 days**, holding that phase's `lock::acquire` guard throughout.

The timeout path itself is safe — `poll_response` returning `None` produces
`CliError::Message("gate ... timed out")` (`main.rs:1899`), never a success, so there is no false
green. But nothing in production bounds it, and 17-09's summary lists this as a shipped feature.

### WR-09: `Ahead` and `Indeterminate` collapse to the same output, and `DEVFLOW_BUILD_DIRTY` is never read

**File:** `crates/devflow-cli/src/main.rs:1033-1041`

The (dirty-tree, ancestor, descendant, exact, unrelated) × (self-dogfood, not) matrix is coherent
after `f73a968`/`3c2774e`/`ae744ed` — every cell traced, no wrong classification. Two gaps remain:

- `Ahead` and `Indeterminate` both map to `Warn` and print the *same* message, so "your binary is
  newer than your source" is unreportable from the logs.
- `build.rs:57` embeds `DEVFLOW_BUILD_DIRTY` and `build_dirty_is_exactly_true_or_false` asserts on
  it, but **no staleness code path reads it**. Scenario: build from a dirty tree, then `git checkout .`.
  `embedded_commit == HEAD` → `Fresh` → `Ok`; the tree is clean so the mtime arm returns `Some(false)`.
  A binary containing code matching no commit is certified Fresh, and the one signal that would catch
  it is embedded and discarded. (Compounded by CR-02, which makes the flag unreliable anyway.)

---

## Info

### IN-01: `build_commit_is_accessible_and_does_not_panic` asserts nothing

**File:** `crates/devflow-cli/tests/build_provenance.rs:31-37`

```rust
let commit = env!("DEVFLOW_BUILD_COMMIT");
let _ = commit.len();
```

`env!` resolves at compile time to a `&'static str`; `.len()` cannot panic and the result is
discarded. The test passes unconditionally regardless of what `build.rs` emits, inflating the pass
count by one. The sibling tests are fine. Note that none of the three would catch CR-02, since a
*stale* value is still parseable and still `"false"`.

### IN-02: an orphaned doc comment makes `cargo doc` actively misleading

**File:** `crates/devflow-cli/src/main.rs:1498-1512`

The doc comment describing `handle_stage_failure` ("Handle a non-Validate stage failure… WR-11:
this path must never be silent") is attached to `truncate_reason`, whose own doc comment follows at
`:1505`. `handle_stage_failure` at `:1538` is now undocumented.

### IN-03: `events.jsonl` records the binary's absolute path, disclosing the OS username

**File:** `crates/devflow-cli/src/main.rs:835-848` (new in this diff)

`workflow_started_payload` emits `"exe_path": std::env::current_exe()` and `"worktree"`, e.g.
`/var/home/denniyahh/...`. Gitignored and local-only, so not a leak today — but `OPERATIONS.md:105`
documents `events.jsonl` as "tail it from any tool," making it an explicit integration surface.
Worth knowing before anyone pipes it somewhere shared.

### IN-04: `truncate_reason` bounds but does not redact agent output

**File:** `crates/devflow-cli/src/main.rs:1510-1536` (pre-existing, not introduced here)

Sanitizes control characters and caps at 300 chars, then routes into gate JSON and the desktop
notification. If an agent echoed a token, up to 300 characters of it reach the gate context.
Bounded, and `.devflow/gates/` is gitignored.

### IN-05: `run_preflight`'s gate arms recurse into `launch_stage` with no depth bound and drop `archived_stage`

**File:** `crates/devflow-cli/src/main.rs:810-819`

Both arms call `launch_stage(state, None, None)`, whereas `handle_stage_failure` (`:1553`, `:1560`)
passes `Some(stage)`. Consequence: the `capture_archived` event records `to_stage` as the current
stage rather than the archived one. Separately, if the preflight predicate is *permanent* (Codex +
Auto + Define + no `CONTEXT.md` on develop — `preflight_interactivity_check`, `:722`), each
`Advance` response re-enters `launch_stage` → `run_preflight` → re-gates, growing the stack one
frame per approval. Human-gated, so not reachable as a spin, but there is no retry ceiling.

### IN-06: planning artifacts carry stale counts, stale line citations, and miss plan 17-10

The behavioural claims in the artifacts are accurate; the numbers attached to them are not.
Actual suite at HEAD: 66 (bin) + 276 (core lib) + 22 (integration) = **364 passed**.

- `17-VERIFICATION.md:41,52,93` claim "61 devflow-cli unit tests"; `:175` claims "64/64 bin" — the
  document contradicts itself, and both differ from the actual 66.
- `17-VALIDATION.md:105` claims "64/64"; `:335` claims a 361-test total.
- `.planning/ROADMAP.md:194` says `9/9 plans executed` and lists only 17-01…17-09, while 17-10 was
  planned (`504fa38`), executed (`ae744ed`, +206 lines to `main.rs`), and summarized (`6995b6f`).
  `.planning/STATE.md` `stopped_at` likewise reads `Completed 17-09-PLAN.md`, and its status line
  says `14/14` while its own `progress` block says `43/43` — three denominators for one phase.
- Line citations drifted: `main.rs:1640-1641` for the `infra_failures` reset is actually `:1732`;
  `main.rs:870-874` for the `rev-parse HEAD` equality gate is actually `:884`.

A reviewer reading ROADMAP would not know `ae744ed` is part of this phase.

---

## Verified Clean (no finding)

Recorded so a later pass need not redo the work:

- **No network egress or telemetry.** No `reqwest`/`hyper`/`ureq`/`std::net`/`TcpStream` anywhere
  in `crates/`; no HTTP dependency in any `Cargo.toml`.
- **No credentials or session data committed.** Scanned all added lines for `ghp_`/`gho_`/`github_pat_`/`sk-`/`Bearer`/`AKIA`/`xox`/PEM headers — zero matches. All 16 added files are `.planning/*.md`.
- **`.gitignore` coverage is complete today** — all 15 runtime paths verified IGNORED via
  `git check-ignore`. `archive_phase_files` only moves data *into* ignored `.devflow/history/`.
- **`build.rs` leaks no builder identity** — embeds only a commit SHA, a dirty bool, and a Unix
  timestamp. No hostname, username, or env passthrough.
- **`gh auth status` output is never logged** (`main.rs:759-773`) — on failure the output is
  discarded and a fixed literal is returned.
- **Subprocess safety** — all 26 `Command::new` sites use argv arrays with `.current_dir()`. The
  single `sh -c` (`main.rs:3024`) interpolates only hardcoded literals and predates this diff.
- **No CI exit-code masking** — no `|| true`, `continue-on-error`, `set +e`, backgrounded steps, or
  vacuous `if:` guards in either workflow; no pipes at all.
- **The known `--exact` false-green trap is not triggered** — no `--exact` and no bare test-name
  filter anywhere in CI, scripts, or tests. The only `-p` reference names the real `devflow` package.
- **`devcontainer.yml`'s fail-fast guard is strong** — `devcontainer_ci_failfast.rs` asserts `set -e`
  is the first non-blank line and precedes every `cargo` invocation, rather than grepping loosely.
- **Help snapshot is accurate** — `cargo run -- --help` is byte-identical to the snapshot; the one
  absent variant (`Advance`) carries `#[command(hide = true)]`.
- **All 17 tests named in the artifacts exist**, individually verified. The one apparent miss is
  documented as a rename in `17-03-SUMMARY.md:145`, and its replacement exists at `agent_result.rs:1554`.
- **`ae744ed`'s "content hooks at the worktree" claim is real** — these are DevFlow's internal `Hook`
  enum variants, not git hooks. Landed as `hook_context_root()` (`main.rs:1612-1685`) with a covering
  test at `:5366`.
- **Every commit SHA cited in the artifacts exists and is an ancestor of HEAD.**
- **`ship.rs` makes no unverified external-success claim** — its diff is purely additive
  (`build_single_agent_cron_instructions` + one test); it builds a manifest and returns it.
- **No artifact claims Phase 17 is merged, PR'd, or pushed.** ROADMAP's `## Shipped` section was
  correctly left untouched.
- **`log_format_env.rs` is high quality** — `env_remove` sanitizes ambient `RUST_LOG`/`DEVFLOW_LOG_FORMAT`
  so the runner's shell cannot mask behaviour; all three tests assert both directions.

**Context, not a finding:** local `develop` (`a2c314f`, the merge-base) is 18 commits ahead of
`origin/develop` (`c034ad7`). Phase 17-01/17-02 work is committed locally but unpushed. No artifact
claims otherwise, so nothing asserts falsely — worth knowing before a ship decision.

---

_Reviewed: 2026-07-19T17:10:00Z_
_Reviewer: Claude (5-angle merged deep review)_
_Depth: deep_

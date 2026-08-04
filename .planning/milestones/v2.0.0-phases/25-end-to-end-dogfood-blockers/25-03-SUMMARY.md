---
phase: 25-end-to-end-dogfood-blockers
plan: 03
subsystem: infra
tags: [rust, cargo-test, git, self-dogfood, staleness]

# Dependency graph
requires:
  - phase: 18
    provides: worktree-aware build staleness enforcement (18c, execution_root idiom)
  - phase: 21
    provides: dogfood staleness guard content-awareness (21d/999.29)
provides:
  - enforce_build_staleness adjudicated exactly once per run, in commands::start, instead of on every stage launch
  - a behavioural regression test proving a mid-run stage transition does not re-invoke the staleness check
  - a de-raced staleness.rs::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking (999.38)
affects: [25-01, 25-02, 25-04, 25-05, 25-06, 25-07]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Adjudicate a one-shot gate at the entry point (commands::start) rather than re-checking it at every internal re-entry (pipeline_launch::launch_stage_inner)"
    - "Behavioural (not structural/grep) regression test: drive the SAME fixture through two call shapes and assert one refuses while the other completes"
    - "Hermetic fixture reads via devflow_core::test_support::git_command, never the production run_git_stdout helper, in test code"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/staleness.rs

key-decisions:
  - "D-03: enforce_build_staleness's call site moved out of launch_stage_inner into commands::start, placed after state.worktree_path is set and before workflow::save_state/launch_stage."
  - "D-04 (accepted trade, recorded in source): pipeline_launch::resume no longer re-adjudicates staleness mid-run — a different binary resuming a phase is never re-checked. Reversal path: a persisted staleness_pin field on State."
  - "D-05 (non-weakening, recorded in source): the check is relocated, not removed. is_self_dogfood_workspace is unmodified; no bypass flag introduced."
  - "The plan's literal verify command (cargo test --package devflow --lib staleness::) fails — devflow is a binary-only crate (main.rs, no lib.rs). Used cargo test --package devflow --bin devflow staleness:: instead, matching the project's known devflow/devflow-cli package-name test trap."

patterns-established:
  - "One-shot gate adjudication at the pipeline entry point, not re-checked at every internal transition"
  - "RED verification for a retroactively-added regression test via a temporary git-revert-and-rebuild commit pair, squashed away once RED is observed"

requirements-completed: ["25b", "999.38"]

coverage:
  - id: D1
    description: "enforce_build_staleness is adjudicated exactly once per run, from commands::start, after the worktree fork and before launch_stage — no longer re-invoked from pipeline_launch::launch_stage_inner on every stage transition"
    requirement: "25b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::enforce_build_staleness_blocks_self_dogfood_behind_worktree_head"
        status: pass
    human_judgment: false
  - id: D2
    description: "staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking (999.38) is de-raced: guarded under ENV_MUTEX and its fixture reads route through the hermetic test_support::git_command builder"
    requirement: "999.38"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking"
        status: pass
      - kind: integration
        ref: "cargo test --workspace (4 consecutive runs, all green)"
        status: pass
    human_judgment: false

# Metrics
duration: ~40min
completed: 2026-07-27
status: complete
---

# Phase 25 Plan 03: Hoist Self-Dogfood Staleness Check + De-race 999.38 Summary

**Moved `enforce_build_staleness` out of the per-stage launch path into `devflow start`'s one-shot adjudication, proved it with a behavioural (not structural) regression test, and de-raced the flaky `ahead_build_from_descendant_commit` test under the module's existing `ENV_MUTEX` precedent.**

## Performance

- **Duration:** ~40 min (first commit 20:20, last commit 20:38 local time, plus read/discovery time)
- **Started:** 2026-07-27 (session start)
- **Completed:** 2026-07-27T20:38:19-04:00
- **Tasks:** 3 of 3 completed
- **Files modified:** 3 (`commands.rs`, `pipeline_launch.rs`, `staleness.rs`)

## Accomplishments

- `enforce_build_staleness` now runs exactly once per `devflow start`, after `state.worktree_path` is set and before `workflow::save_state`/`launch_stage` — a phase that modifies DevFlow's own source can now progress past every stage boundary instead of being re-blocked at each one (999.48/DEN-73).
- Deleted the corresponding call from `pipeline_launch::launch_stage_inner`, updated its surrounding doc comments (module-level function doc, WR-04 comment) so they no longer claim staleness enforcement happens in that function.
- Documented D-04's accepted trade (resume no longer re-adjudicates staleness mid-run) and D-05's non-weakening (no bypass flag, `is_self_dogfood_workspace` unmodified) directly at the new call site in `commands.rs`.
- Added `mid_run_stage_transition_does_not_readjudicate_staleness`: a single behavioural test that drives the same `worktree_staleness_fixture` through two call shapes — a direct `enforce_build_staleness` call (start-shaped, still refuses) and a real `launch_stage_inner` call with a stubbed `claude` binary (stage-transition-shaped, completes without a second refusal) — and asserts exactly one `self_dogfood_stale_blocked` event exists afterward.
- De-raced `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking` (999.38): added the `ENV_MUTEX` guard its three siblings already carry, and rerouted its fixture stdout reads (`rev-parse HEAD`, `status --porcelain`) from the production `run_git_stdout` helper to `devflow_core::test_support::git_command`.
- Verified the de-raced test across 4 consecutive `cargo test --workspace` runs — all green (188/3/7/4/1/1/1/3/17/5/10/2/9/1/368/2/2/0 passed per target, 0 failed, every run).

## Task Commits

Each task was committed atomically:

1. **Task 1: Hoist the staleness adjudication into the start path (D-03)** - `7d3b5f3` (fix)
2. **Task 2: Regression test — a mid-run stage transition does not re-adjudicate staleness** - `59124ed` (test)
3. **Task 3: De-race the descendant-commit test (999.38 / D-14)** - `e2f9fed` (fix)

**Plan metadata:** (this commit)

_Note: Task 2 is `tdd="true"` but the implementation (Task 1) already existed by construction of this plan's sequencing — there is no separate RED-before-GREEN cycle. Instead, its RED discrimination was proven via a scripted revert-and-rebuild check (see "RED Verification" below) rather than a `test(...)`-then-`feat(...)` commit pair, per the plan's own acceptance criteria wording ("run it once against a locally reverted Task 1... then restore")._

## Files Created/Modified

- `crates/devflow-cli/src/commands.rs` - Inserted the hoisted `enforce_build_staleness` call in `start()`, after the worktree fork and before `workflow::save_state`, with D-03/D-04/D-05 doc comments recording placement rationale and the accepted trade
- `crates/devflow-cli/src/pipeline_launch.rs` - Deleted the per-stage `enforce_build_staleness` call and its now-unused import from `launch_stage_inner`; updated the function's module doc and the WR-04 comment to stop claiming staleness enforcement happens there
- `crates/devflow-cli/src/staleness.rs` - Added `mid_run_stage_transition_does_not_readjudicate_staleness` (Task 2); guarded `ahead_build_from_descendant_commit_warns_instead_of_blocking` under `ENV_MUTEX` and rerouted its fixture reads through `test_support::git_command` (Task 3)

## Decisions Made

- **D-03** (from plan): the call site moves, not the check itself. Placement is pinned after `state.worktree_path` is set (so it evaluates against the phase's worktree HEAD, not the main checkout) and before `workflow::save_state` (so a refusal never persists state for a run that will not start).
- **D-04** (accepted trade, recorded in source at the new call site): `pipeline_launch::resume` no longer re-adjudicates staleness after a rate-limit/infra pause — accepted per the operator's standing position that only validated, pushed code should drive a run. Reversal path named: a persisted `staleness_pin` field on `State`.
- **D-05** (non-weakening, recorded in source): the check is relocated, never removed or softened. Neither of 999.48's rejected alternatives (mid-run rebuild, dogfood bypass flag) appears anywhere in this plan's diff — confirmed via `rg -ci bypass` returning only prose describing the *absence* of a bypass, and `git diff -- staleness.rs::is_self_dogfood_workspace` showing no change to that function until Task 3 touched an unrelated test in the same file.
- **Test filter deviation** (Rule 3 — blocking issue, auto-fixed): the plan's literal verify command `cargo test --package devflow --lib staleness::` fails with `error: no library targets found in package devflow` — the CLI crate is binary-only (`main.rs`, no `lib.rs`). Used `cargo test --package devflow --bin devflow staleness::` instead throughout, which is exactly the `devflow`/`devflow-cli` package-name trap already flagged in this project's known test traps, extended to also cover the missing `--lib` target.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Verify command's `--lib` flag targets a nonexistent library**
- **Found during:** Task 1 (running the plan's literal `<verify>` command)
- **Issue:** `cargo test --package devflow --lib staleness::` exits with `error: no library targets found in package devflow` — `devflow` (the CLI crate) has only a `[[bin]]`/binary target (`src/main.rs`), no `lib.rs`.
- **Fix:** Ran `cargo test --package devflow --bin devflow staleness::` instead for every staleness-scoped test invocation in this plan.
- **Files modified:** none (test invocation only, no source change)
- **Verification:** Confirmed the same 39/40 test count and `0 failed` result either way once the correct target flag was used.
- **Committed in:** n/a (verification methodology only, not a source change)

**2. [Rule 1 - Bug] Unused `project_root` variable left behind after deleting the staleness call**
- **Found during:** Task 1, immediately after deleting the `enforce_build_staleness` call from `launch_stage_inner`
- **Issue:** `let project_root = state.project_root.clone();` was only ever consumed by the deleted call, leaving an unused-variable warning (`cargo build --workspace` would still pass, but `cargo clippy -- -D warnings` — mandated by this project's CLAUDE.md — would fail).
- **Fix:** Removed the now-dead `let project_root = ...` binding.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Verification:** `cargo build --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` both exit 0 with zero warnings.
- **Committed in:** `7d3b5f3` (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking test-invocation correction, 1 bug — unused variable). No scope creep; both fixes are directly required by the task's own goal.

## RED Verification (Task 2)

Per the plan's acceptance criteria, `mid_run_stage_transition_does_not_readjudicate_staleness`'s discriminating power was proven by reverting Task 1's fix and observing the test fail, then restoring the fix:

1. Committed Task 2's test on top of Task 1's fix (clean tree at commit `59124ed`).
2. Restored `pipeline_launch.rs`/`commands.rs` to their pre-Task-1 content (`git show a5a068f:<path> > <path>`) and committed the revert as a temporary commit, so the tree was clean again at build time (this matters: `DEVFLOW_BUILD_DIRTY` is baked in at `cargo build` time from *this actual repo's* working-tree cleanliness, decoupled from the test fixture's own synthetic git history).
3. Rebuilt and ran the test. **Observed RED failure:**
   ```
   thread 'staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness' panicked at crates/devflow-cli/src/staleness.rs:777:16:
   a mid-run stage transition must not re-invoke the staleness adjudication — this same fixture just refused via the direct start-shaped call above: Message("self-dogfood stale build blocked for stage code: a build-relevant file (.rs/Cargo.toml/Cargo.lock/build.rs/rust-toolchain.toml) changed in /tmp/.tmpOvmEgG/worktree's tracked source since this devflow binary was built, or its embedded commit is not an ancestor of current HEAD at all — rebuild devflow before driving its own workspace (D-18; the Phase 16 false-evidence incident) — evaluated against this phase's WORKTREE HEAD, not the main checkout; rebuild and reinstall the binary before resuming")
   test result: FAILED. 0 passed; 1 failed
   ```
4. Restored Task 1's fix via `git revert --no-edit <temp-revert-commit>` (a clean forward-only operation, not a history rewrite of already-shared commits), confirmed the working tree was byte-identical to before the temp commits (`git diff 59124ed HEAD` — 0 lines), then squashed the two scratch commits away with `git reset --soft 59124ed` (a metadata-only op; the working tree did not change) to keep the local history free of a non-conventional `TEMP:` commit message.
5. Rebuilt and reran the full `staleness::` suite — GREEN again, 40 passed / 0 failed.

**Design note on why the fixture needed an uncommitted dirty edit:** the production call sites always pass `env!("DEVFLOW_BUILD_COMMIT")`/`env!("DEVFLOW_BUILD_DIRTY")` (this binary's own build provenance), not a test-supplied `embedded_commit`. Against a synthetic, unrelated fixture repo, an ancestry check against that real (foreign) SHA always resolves `Indeterminate` (`git merge-base` reports "unknown revision"), so the test's `worktree_staleness_fixture`-based "ahead by two committed commits" alone could never discriminate through the *production* call shape. Adding an **uncommitted** build-affecting edit to the fixture's worktree gives `combined_staleness`'s dirty-flag arm (`tree_has_modified_build_inputs`) an independent, ancestry-decoupled path to `Stale` — which fires correctly as long as this session's own tree is clean at build time (`DEVFLOW_BUILD_DIRTY == "false"`), which the RED-check procedure above guarantees.

## Issues Encountered

None beyond the two auto-fixed deviations above and the RED-verification design challenge (resolved as described).

## User Setup Required

None - no external service configuration required.

## Known Stubs

None.

## Next Phase Readiness

- `enforce_build_staleness` is now a true one-shot gate at `devflow start`; any future work touching the launch/resume/preflight paths should be aware that `pipeline_launch::resume` and `preflight`'s `LoopBack` retry no longer re-check staleness (D-04's accepted trade) — if that trade is ever revisited, the reversal path is a persisted `staleness_pin` field on `State`.
- **999.38's broader remainder is explicitly NOT closed by this plan** and should be re-filed: the five PATH-mutating call sites in `pipeline_launch.rs`/`pipeline_outcomes.rs`/`preflight.rs` that use process-global `std::env::set_var` (rather than per-`Command` `env`) are untouched, and `ensure_agent_binary`/`agent_binary_available`'s direct `std::env::var_os("PATH")` read does not transfer to the per-`Command` idiom without a signature change to accept an injected search path (25-RESEARCH.md Pitfall 3). `ENV_MUTEX` therefore still exists and still serializes this file's PATH-mutating tests against each other.
- No blockers for 25-04 through 25-07 (parallel wave siblings) — this plan's file set (`commands.rs`, `pipeline_launch.rs`, `staleness.rs`) does not overlap with 25c/25d/25e's declared files per the phase's pattern map.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-27*

## Self-Check: PASSED

- FOUND: `.planning/phases/25-end-to-end-dogfood-blockers/25-03-SUMMARY.md`
- FOUND commit: `7d3b5f3` (Task 1)
- FOUND commit: `59124ed` (Task 2)
- FOUND commit: `e2f9fed` (Task 3)
- FOUND commit: `1692a44` (docs: SUMMARY)

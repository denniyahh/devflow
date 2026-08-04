---
phase: 23-end-to-end-dogfood
plan: 08
subsystem: core
tags: [rust, dead-code-removal, test-coverage-repointing, sequentagent-deletion]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood
    provides: "23-07 — the CLI-side sequentagent verb deletion, confirming zero remaining non-test callers of the core-side surface this plan deletes"
provides:
  - "devflow-core with the two-agent (sequentagent) construct's core-side surface fully deleted: SequentagentSlotKind, SequentagentSlot, and their path/write/read/clear functions; monitor::spawn_monitor_no_advance and monitor::wait_for_agent_exit"
  - "The .devflow directory-creation constructor guarantee (devflow_dir_gitignore.rs) re-pointed at the surviving monitor::spawn_monitor, with its 7-constructor count unchanged"
  - "git.rs's fast_forward_branch/rebase_in (and the now-orphaned git_in helper) removed as truly dead code — zero production callers, exercised only by the deleted verb's own fixture-style tests"
affects: [23-09-cli-side-remnants, 23-10-baseline-capture, 23-11-post-run-delta]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Cross-task compile coupling in a single-package subtractive deletion: Task 1 deletes a function a Task-2-owned integration test still calls, so Task 1's own `cargo test -p <pkg>` cannot compile standalone until Task 2 lands — verified narrowly with `--lib` for Task 1, full workspace confirmed once Task 2 completed"
    - "A test's advance-tail Command invokes `std::env::current_exe()`, which inside `cargo test` resolves to the test binary itself, not the real CLI — confirmed empirically (direct invocation) rather than assumed, before relying on it as a fail-fast, non-blocking synchronisation boundary"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/agent.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/tests/devflow_dir_gitignore.rs
    - ARCHITECTURE.md

key-decisions:
  - "git.rs disposition (the one reference whose fate plan-time couldn't settle): fast_forward_branch and rebase_in have zero production callers anywhere in the workspace — grep-confirmed via `rg -n 'fast_forward_branch|rebase_in' crates/ --glob '!crates/devflow-core/src/git.rs'` returning empty, and their only two callers were sequentagent_helpers_integrate_and_rebase_cleanly and rebase_in_aborts_and_errors_on_conflict, both exercising exactly the two-agent worktree-integration mechanism (fast-forward the base, rebase the second worktree onto it) the deleted verb used. Deleted both helpers and both tests together, per the plan's explicit fallback instruction."
  - "ensure_branch (also exercised only by those same two now-deleted tests) was left in place despite losing its only callers/coverage, rather than deleted — matching this file's own documented precedent for release_start/release_finish (\"exist on GitFlow but not called from any production CLI path today\"), and because the plan's read_first scoped the disposition decision to 'the git helpers that test exercises' toward fast_forward_branch/rebase_in specifically, not the general-purpose ensure_branch/branch_exists/branch_tip trio it also happened to call."
  - "Constructor #3's re-pointing relies on spawn_monitor's advance tail resolving std::env::current_exe() to the cargo-test binary itself rather than the real devflow CLI, so `advance <root> --phase N` hits the Rust test harness's own arg parser (\"Unrecognized option: 'phase'\") and fails in milliseconds — verified by direct invocation of the built test binary before committing to it as the synchronisation design, and by 5 repeated runs of the re-pointed test (~0.06s each, no flake). This is the same fail-fast behavior every other spawn_monitor-based test in monitor.rs already relies on implicitly; no new timeout mechanism was added."

requirements-completed: [23d]

coverage:
  - id: D1
    description: "The two-agent construct is entirely absent from the workspace's production/test source outside the two pre-existing intentional survivors from 23-07"
    requirement: "23d"
    verification:
      - kind: unit
        ref: "source assertion: rg -c 'sequentagent|Sequentagent|SequentAgent' crates/ returns matches only in ship.rs (2, a negative assertion + doc comment predating this plan) and pipeline_outcomes.rs (1, a historical doc-comment comparison predating this plan) — both outside this plan's declared files_modified"
        status: pass
    human_judgment: false
  - id: D2
    description: "The surviving monitor spawn API (spawn_monitor, wait_for_agent_pid, spawn_monitor_inner) and its SIGTERM regression coverage are intact"
    requirement: "23d"
    verification:
      - kind: unit
        ref: "source assertion: rg -n 'pub fn spawn_monitor\\b' and 'pub fn wait_for_agent_pid' each match exactly one line; sigterm_to_monitor_also_kills_the_agent present by name"
        status: pass
    human_judgment: false
  - id: D3
    description: "The .devflow constructor guarantee is re-pointed, never narrowed — 7 constructors before and after, each named in the failures accumulator"
    requirement: "23d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/tests/devflow_dir_gitignore.rs#all_seven_devflow_constructors_produce_the_gitignore"
        status: pass
      - kind: other
        ref: "source assertion: numbered constructor comments (// 1. through // 7.) unchanged at 7"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every test-count change is explained: workspace test count 595 before this plan, 586 after, delta of exactly 9 (the sequentagent-specific tests deleted alongside the code they covered)"
    requirement: "23d"
    verification:
      - kind: integration
        ref: "cargo test --workspace, run against a scratch git-worktree baseline at the pre-plan commit (595 passed) and again after this plan's two commits (586 passed)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Full gate chain — cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check — exits 0 as a single chain, gated on exit status"
    requirement: "23d"
    verification:
      - kind: integration
        ref: "cargo test -p devflow-core --features test-support --test devflow_dir_gitignore && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check, run as one chained command"
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 08: Delete the core-side sequentagent surface Summary

**Deleted `SequentagentSlotKind`/`SequentagentSlot` and their path/write/read/clear functions from `agent_result.rs`, `monitor::spawn_monitor_no_advance`/`wait_for_agent_exit` from `monitor.rs`, and `git.rs`'s `fast_forward_branch`/`rebase_in` (zero production callers, only exercised by the deleted verb's own fixture tests) — then re-pointed the `.devflow` directory-creation constructor-coverage test's point #3 from the deleted no-advance spawn function to the surviving `spawn_monitor`, keeping all 7 constructors named and covered.**

## Performance

- **Duration:** ~35 min (base commit `5bf1034` at 00:13:36 through Task 2 commit `3bcd24d` at 00:32:54, plus SUMMARY authoring)
- **Started:** 2026-07-26T00:13 (base commit `5bf1034`)
- **Completed:** 2026-07-26T00:32 (Task 2 commit `3bcd24d`)
- **Tasks:** 2/2 executed
- **Files modified:** 6 (across both task commits)

## Accomplishments

- Deleted `SequentagentSlotKind` (and its `as_str`), `SequentagentSlot`, `sequentagent_slot_path`, `write_sequentagent_slot`, `read_sequentagent_slot`, `clear_sequentagent_slot`, and their 5 colocated tests from `agent_result.rs` — 48 references down to 0
- Reworded 3 surviving `agent_result.rs` doc comments (2 production, 1 test) that named the deleted verb, without changing behavior
- Deleted `monitor::spawn_monitor_no_advance` and `monitor::wait_for_agent_exit` and their 2 dedicated tests from `monitor.rs`; `spawn_monitor`, `wait_for_agent_pid`, `spawn_monitor_inner`, and the `sigterm_to_monitor_also_kills_the_agent` SIGTERM regression test are untouched
- Reworded `agent.rs`'s module doc comment to drop the sequentagent reference; `terminate`/`agent_running`/`looks_like_devflow_process` untouched
- Deleted `git.rs`'s `fast_forward_branch` and `rebase_in` (zero production callers anywhere in the workspace) together with the two tests that exclusively exercised them (`sequentagent_helpers_integrate_and_rebase_cleanly`, `rebase_in_aborts_and_errors_on_conflict`); also removed `git_in`, which became dead code once `rebase_in` (its only caller) was gone
- Fixed 2 `ARCHITECTURE.md` doc-comment references to now-deleted identifiers (`spawn_monitor_no_advance`, `fast_forward_branch`) that broke `doc_check::doc_referenced_identifiers_exist_in_source`
- Re-pointed `devflow_dir_gitignore.rs` constructor #3 from `spawn_monitor_no_advance` to `spawn_monitor`, keeping the numbered label, failures-accumulator entry text (renamed to the surviving function), and assertion-message shape intact; reworded the `wait_for_pid_to_die` doc comment and constructor #4's comment to drop their sequentagent references
- Verified the re-pointed constructor's `devflow advance` tail fails fast (milliseconds, via the Rust test harness's own arg parser rejecting `--phase` when `current_exe()` resolves to the test binary) rather than risking a multi-day gate-wait hang — confirmed by direct invocation of the built test binary and 5 repeated test runs (all ~0.06s, no flake, no zombie)
- Full gate chain green: `cargo test -p devflow-core --features test-support --test devflow_dir_gitignore && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`, run as a single chained command, exit 0

## Task Commits

1. **Task 1: Delete the core-side surface end-to-end and take the workspace count to zero** — `0833a6c` (feat)
2. **Task 2: Re-point the .devflow constructor coverage instead of narrowing it** — `3bcd24d` (test)

## Files Created/Modified

- `crates/devflow-core/src/agent_result.rs` — deleted `SequentagentSlotKind`/`SequentagentSlot`/4 functions/5 tests; reworded 3 doc comments
- `crates/devflow-core/src/monitor.rs` — deleted `spawn_monitor_no_advance`/`wait_for_agent_exit`/2 tests; `spawn_monitor` and the SIGTERM regression test untouched
- `crates/devflow-core/src/agent.rs` — reworded module doc comment only
- `crates/devflow-core/src/git.rs` — deleted `fast_forward_branch`/`rebase_in`/`git_in`/2 tests (all zero-caller dead code once the deletion completed)
- `crates/devflow-core/tests/devflow_dir_gitignore.rs` — re-pointed constructor #3 to `spawn_monitor`; reworded 2 comments
- `ARCHITECTURE.md` — reworded 2 mentions of now-deleted identifiers

## Decisions Made

- **git.rs's `fast_forward_branch`/`rebase_in` disposition:** deleted, not renamed-and-kept. Grep-confirmed zero production callers anywhere in the workspace (`rg -n 'fast_forward_branch|rebase_in' crates/ --glob '!crates/devflow-core/src/git.rs'` returns empty), and their only two callers were the two tests that exercised exactly the two-agent worktree-integration mechanism (fast-forward the base branch, rebase the second agent's worktree onto it) the deleted verb used. Both helpers and both tests were deleted together, per the plan's explicit "if they have none, delete helpers and test together" fallback.
- **`ensure_branch` left in place** despite this deletion also removing its only two callers (the same two deleted tests), leaving it with zero current test coverage. This diverges from the strict "delete now-dead code" instinct applied to `git_in`, for two reasons: (1) the plan's `read_first` scoped the git.rs disposition decision specifically to "the git helpers that test exercises" in the context of `fast_forward_branch`/`rebase_in` (the two-agent handoff mechanism), not the general-purpose `ensure_branch`/`branch_exists`/`branch_tip` trio it also happened to call; (2) `git.rs`'s own `## Git and ship model` documentation in `ARCHITECTURE.md` already establishes precedent for keeping `GitFlow` methods with no current production caller — `release_start`/`release_finish` are explicitly kept "not called from any production CLI path today (only exercised in git.rs's own tests)." `ensure_branch` is now in that same category, just without its own dedicated test. Flagged here rather than silently deleted or silently left uncovered.
- **Constructor #3's re-pointing verified empirically, not assumed.** Before committing to `spawn_monitor`'s advance tail as safe inside this fixture, ran the actual compiled test binary directly with the exact args the tail would pass (`<test-binary> advance <tmpdir> --phase 1`) and confirmed it fails in milliseconds via the Rust test harness's own `Unrecognized option: 'phase'` rejection — never reaching `devflow advance`'s real state-loading or gate logic. No env-based timeout override was added since the empirical evidence showed none was needed; the existing `wait_for_file`/`wait_for_pid_to_die` synchronisation points already bound the test.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `git.rs`'s `git_in` became dead code once `rebase_in` (its only caller) was deleted**
- **Found during:** Task 1, first `cargo build --workspace` after the agent_result.rs/monitor.rs/agent.rs/git.rs edits
- **Issue:** `warning: function 'git_in' is never used` — `git_in` had exactly one caller, `rebase_in`, which this task deleted as dead code (zero production callers, only exercised by the two now-deleted sequentagent-adjacent tests).
- **Fix:** Deleted `git_in`. `stderr_or_status` (defined immediately below it) is still called from 4 other sites and was left untouched.
- **Files modified:** `crates/devflow-core/src/git.rs`
- **Verification:** `cargo build --workspace` clean, zero warnings.
- **Committed in:** `0833a6c` (Task 1 commit)

**2. [Rule 3 - Blocking] `ARCHITECTURE.md` referenced 2 now-deleted identifiers, breaking `doc_check::doc_referenced_identifiers_exist_in_source`**
- **Found during:** Task 1, `cargo test -p devflow-core --lib` (the scoped substitute verify, see deviation 4)
- **Issue:** `ARCHITECTURE.md` named `spawn_monitor_no_advance()` (as "a second entry point" for synchronous agent execution) and `fast_forward_branch()` (as part of "branch integration") — both deleted by this task. `doc_check`'s identifier-existence check panicked on the first: `documented Rust identifier 'fast_forward_branch' does not exist in Rust source`.
- **Fix:** Reworded both paragraphs. The `spawn_monitor_no_advance` paragraph now states `spawn_monitor()` is the single way an agent process is spawned (23d), matching `agent.rs`'s own reworded module doc comment. The `fast_forward_branch()` sentence was removed from the branch-integration paragraph, leaving only the still-accurate `ensure_branch()` description.
- **Files modified:** `ARCHITECTURE.md`
- **Verification:** `cargo test -p devflow-core --lib doc_check::` passes; full `cargo test -p devflow-core --lib` 360/360.
- **Committed in:** `0833a6c` (Task 1 commit)

**3. [Rule 1 - Bug, verify-command-only] Task 1's literal `<verify>` command cannot compile standalone**
- **Found during:** Task 1, running the plan's literal verify chain
- **Issue:** `cargo test -p devflow-core` (the middle segment of Task 1's literal verify) compiles ALL of devflow-core's test targets, including `tests/devflow_dir_gitignore.rs` — which is outside Task 1's declared `<files>` (it belongs to Task 2) and still calls the just-deleted `monitor::spawn_monitor_no_advance` until Task 2's re-pointing lands. This is a genuine cross-task compile coupling inherent to a single-package subtractive deletion split across two tasks, not a false-green — the command correctly fails to compile (4 `E0425`/`E0433` errors), it just cannot be made to pass by anything within Task 1's own scope.
- **Fix:** Ran the scoped substitute `cargo test -p devflow-core --lib` (excludes the Task-2-owned integration test binaries) for Task 1's own verification — 360 passed (down from a 369-test pre-plan baseline, confirmed via a scratch `git worktree` at the pre-plan commit; delta of 9 is exactly the tests this task deleted). The full, unscoped `cargo test -p devflow-core` / `cargo test --workspace` was then run and confirmed green as part of Task 2's own verify chain, once the re-pointing landed — closing the gap the same execution session, not left open.
- **Files modified:** none (verify-command-only)
- **Verification:** `cargo test -p devflow-core --lib` → 360 passed, 0 failed; `cargo test -p devflow-core --lib agent_result::` → 71 passed, 0 failed. Both re-confirmed against `cargo test --workspace` after Task 2 (586 passed workspace-wide, 0 failed).
- **Committed in:** n/a (verify-command-only; no PLAN.md edit)

**4. [Rule 1 - Bug, verify-command-only] Task 2's literal `<verify>` command's first segment reproduces 23-07's already-documented `test-support` feature-gating issue**
- **Found during:** Task 2, running the plan's literal verify chain
- **Issue:** `cargo test -p devflow-core --test devflow_dir_gitignore` (Task 2's literal verify, without `--features test-support`) fails to compile — 3 `E0433` errors, `could not find test_support in devflow_core` — because `devflow-core`'s `test_support` module is gated behind `#[cfg(any(test, feature = "test-support"))]` and is not enabled by default for a standalone `-p devflow-core` invocation. This is the identical issue 23-07-SUMMARY.md documented as its own deviation #3 (`cargo test -p devflow-core ship::` had the same problem).
- **Fix:** Ran `cargo test -p devflow-core --features test-support --test devflow_dir_gitignore` for the standalone form (2 passed, 0 failed), matching 23-07's established precedent. Separately confirmed `cargo test --workspace` (which picks up the feature via `devflow-cli`'s dev-dependency feature unification) also passes the same 2 tests.
- **Files modified:** none (verify-command-only)
- **Verification:** `cargo test -p devflow-core --features test-support --test devflow_dir_gitignore` → `test result: ok. 2 passed; 0 failed`; `cargo test --workspace` → same 2 tests pass among 586 workspace-wide passing tests, 0 failed.
- **Committed in:** n/a (verify-command-only; no PLAN.md edit)

**5. [Rule 1 - Bug, documentation only] The phase-wide `rg -c 'sequentagent...' crates/` acceptance criterion's stated exception list is incomplete**
- **Found during:** Task 1 (initial scoping grep) and reconfirmed after Task 2
- **Issue:** Both tasks' acceptance criteria state `rg -c 'sequentagent|Sequentagent|SequentAgent' crates/` should return 0 for every file except `crates/devflow-core/tests/devflow_dir_gitignore.rs` (Task 1) / return 0 workspace-wide (Task 2). In reality, two more files retain references and always have since 23-07 landed: `crates/devflow-core/src/ship.rs` (2 — a doc comment and a negative assertion `assert!(!record.hermes_cron.command.contains("sequentagent"))` proving the surviving builder never regresses) and `crates/devflow-cli/src/pipeline_outcomes.rs` (1 — a doc-comment historical comparison). Both were explicitly identified and left in place by 23-07-SUMMARY.md's own deviation #6 as intentional survivors (a negative-assertion test needs the literal string to prove its absence; deleting or rewording it would weaken a genuine regression check). Neither file is in this plan's declared `<files_modified>`.
- **Resolution:** Left both files untouched, consistent with 23-07's precedent and this plan's own scope boundary. The intent behind the criterion — no *implementation* of the deleted verb survives — is fully met; only a doc-comment historical mention and a negative-assertion proof-of-absence remain, in files this plan was never scoped to touch.
- **Files modified:** none
- **Committed in:** n/a (documentation-only finding, no code change)

---

**Total deviations:** 5 (1 Rule 1 dead-code cleanup, 1 Rule 3 blocking doc fix, 2 Rule 1 verify-command-only false-negative fixes, 1 Rule 1 documentation-only finding with no code change)
**Impact on plan:** All fixes necessary for correctness (dead code, broken doc_check) or accurate verification (feature-gating, cross-task compile coupling). No scope creep — `ARCHITECTURE.md` was touched only because this task's own deletion broke a test asserting its accuracy; `ensure_branch` was deliberately left alone rather than following `git_in`'s deletion, and that judgment call is recorded above rather than silently applied.

## Issues Encountered

None beyond the deviations documented above.

## User Setup Required

None — no external service configuration required.

## Known Stubs

None.

## Threat Flags

None — this plan is purely subtractive (core-side deletion) plus one re-pointed test and two doc-comment reworks; no new network endpoints, auth paths, file access patterns, or schema changes were introduced. The plan's own `<threat_model>` register (T-23-81 through T-23-SC) covers this plan's actual risk surface:

- **T-23-81** (constructor coverage narrowing) — mitigated: 7 constructors before and after, verified by direct grep of the numbered comments, not just a test-pass count.
- **T-23-82** (SIGTERM regression test collateral deletion) — mitigated: `sigterm_to_monitor_also_kills_the_agent` confirmed present by name via `rg -n`.
- **T-23-83** (zombie processes from the re-pointed fixture) — mitigated: `wait_for_pid_to_die` still called; empirically confirmed no zombie processes after 5 repeated test runs (`ps aux | grep defunct` showed only pre-existing, unrelated sandbox zombies from before this session).
- **T-23-84** (unexplained test-count delta) — mitigated: 595 before / 586 after / 9 removed, all three numbers recorded and the delta traced to the exact 9 deleted tests.
- **T-23-SC** (package installs) — not applicable; no installs in this plan.

## Next Phase Readiness

- **The phase-wide "two-agent construct absent from the workspace" goal is met**, modulo the two pre-existing, intentionally-kept survivors in `ship.rs`/`pipeline_outcomes.rs` from 23-07 (documented above, deviation 5) — neither is functional code, both are doc-comment/negative-assertion artifacts outside this plan's scope.
- **Workspace test count: 595 before this plan's base commit, 586 after** — the 9-test delta is exactly the sequentagent-specific tests this plan deleted alongside the code they covered (5 in `agent_result.rs`, 2 in `monitor.rs`, 2 in `git.rs`). Plan 23-10's baseline pair and plan 23-11's post-run delta can consume this 9 as an exact, explained figure.
- **No blockers.** Full workspace gate green: `cargo build --workspace`, `cargo test --workspace` (586 passed, 0 failed), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.
- Ready for plan 23-09 (the CLI-side remnants, per the wave's `depends_on`) — this plan's declared `<files_modified>` (`crates/devflow-core/src/agent_result.rs`, `monitor.rs`, `agent.rs`, `git.rs`, `crates/devflow-core/tests/devflow_dir_gitignore.rs`) do not overlap 23-09's stated ownership of `crates/devflow-core/src/state.rs` and `crates/devflow-cli/src/*`.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*

## Self-Check: PASSED

All 7 referenced files confirmed present on disk (`agent_result.rs`, `monitor.rs`,
`agent.rs`, `git.rs`, `devflow_dir_gitignore.rs`, `ARCHITECTURE.md`,
`23-08-SUMMARY.md`). Both task commit hashes (`0833a6c`, `3bcd24d`) confirmed
present in `git log --oneline --all`. No missing items.

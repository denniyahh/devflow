---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 12
subsystem: state-write locking and gate recovery
tags: [rust, cli, state-lock, gates, e2e]
requires:
  - phase: 48-10
    provides: collision-safe state and gate persistence with gate cleanup
  - phase: 48-11
    provides: bounded phase-lock acquisition
provides:
  - start holds the per-phase lock across all post-dry-run effects
  - fresh start clears stale gate artifacts only while holding that lock
  - stop acquires before loading or writing state and retries after a successful signal
affects: [start, stop, gate-waiters, phase-lock, 48-13]
tech-stack:
  added: []
  patterns: [lock-before-effects, lock-before-load, child-command fixture environment]
key-files:
  created:
    - crates/devflow-cli/tests/start_lock_e2e.rs
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/stop_e2e.rs
key-decisions:
  - "start acquires the phase lock immediately after dry-run and retains its guard until return."
  - "stop uses the 3-second TERMINATE_VERIFY_WAIT only after it successfully sends SIGTERM."
  - "A gate-path contention is success without a state write; all other live-holder contention is an error."
actuals:
  tokens: 5049
  tasks: 2
  commits: 1
commits: 1
plan_head_before: 167363fec2f91ff84473054e0e9d452e86f6b757
metrics:
  duration: 10 minutes
  completed_date: 2026-09-17
status: complete
coverage:
  - id: D1
    description: "start refuses a live phase-lock holder without writing state or creating a phase branch/worktree."
    requirement: SURV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_refuses_while_another_process_holds_the_phase_lock"
        status: pass
    human_judgment: false
  - id: D2
    description: "fresh start removes a planted stale Define response before that response can abort the new gate."
    requirement: SURV-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_clears_leftover_gate_files_after_taking_the_lock"
        status: pass
    human_judgment: false
  - id: D3
    description: "stop preserves state when a signalled holder survives and marks state only after a reclaimable holder exits."
    requirement: SURV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_writes_no_state_while_the_lock_holder_survives_the_signal"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_marks_stopped_after_the_signalled_lock_holder_exits"
        status: pass
    human_judgment: false
---

# Phase 48 Plan 12: Start and stop phase-lock ownership Summary

`start` now owns its phase lock before every real effect, and `stop` cannot recreate or mark state while a verified live lock holder survives its signal.

## Performance

- **Duration:** 10 minutes
- **Started:** 2026-09-17T09:14:48Z
- **Completed:** 2026-09-17T09:24:50Z
- **Tasks:** 2/2
- **Files modified:** 4

## Delivered

- `start` acquires its per-phase guard immediately after dry-run, refuses contention with phase and PID, and retains the guard through all remaining work.
- A locked fresh start cleans request, response, acknowledgement, and temporary gate artifacts for every stage; a planted abort response can no longer decide its new Define gate.
- `stop` now follows acquire → load/re-check → mutate → save. A successful signal receives the bounded `TERMINATE_VERIFY_WAIT` retry; a surviving holder causes a nonzero no-write outcome.
- Added real-binary E2E coverage for start lock refusal/control/stale cleanup plus stop's post-signal, monitor-PID, and stale-identity contracts.

## Verification

- Task 1 gate: `task1_gate_rc=0`; each of the three exact `start_lock_e2e` tests printed exactly `1 passed` with nonzero filtered tests; `start_reachability_e2e` passed; `wr11_block_lines=19 wr11_block_unchanged=1` against `c0fb193`.
- Task 1 negative control: a byte mutation of the extracted WR-11 block made the comparator differ (`mutation_detected=1`). This proves the comparator detects a block change; it does not independently exercise the historical save/launch reordering.
- Task 2 gate: `task2_gate_rc=0`; seven exact stop cases each printed exactly `1 passed` with nonzero filtered tests; the complete `stop_e2e` module passed; the first static call in `persist_stopped_state` is `lock::acquire`; package Clippy passed.
- Final local checks: complete `start_lock_e2e`, complete `start_reachability_e2e`, complete `stop_e2e`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `git diff --check` all passed.
- Independent orchestrator recheck: all ten exact start/stop controls each reported exactly one passing test with a nonzero filtered count; the start-lock, start-reachability, and stop modules passed. The full host `scripts/check.sh test` exited 0 with 32 of 32 `test result:` lines successful. An earlier combined recheck exited 101 before it reported the stop-module result, but its temporary inner log was cleaned; an immediate module rerun plus five preserved repeats and the final full host check all passed. This makes that one failure unreproducible, not explained or fixed.
- Container parity: attempted `scripts/check-in-container.sh test`; it exited 1 because Docker was not reachable. No container result exists, so the host checks do not establish pinned-image/CI parity.

These deterministic fixture tests prove the tested lock, stale-response, post-signal, and identity paths. They do not establish a field state-write interleaving, reliability over repeated production runs, or pinned-container parity; SURV-01's field arm remains intentionally unresolved for Phase 49.

## TDD Gate Compliance

### RED

- `start_refuses_while_another_process_holds_the_phase_lock` exited 101 before implementation, with its intended assertion: `start must refuse a held lock`.
- `stop_writes_no_state_while_the_lock_holder_survives_the_signal` exited 101 before implementation, with its intended assertion: `stop must refuse to mark state while the holder survives`; current output showed the holder was signalled.

The Phase 48 truthful-Cargo exception was used: both runs discovered exactly one named test and failed on the planned behavior assertion. No synthetic TAP was produced for the GSD parser.

### GREEN

All plan exact-test gates and relevant full E2E modules passed after the implementation. No refactor-only pass was needed.

## Deviations from Plan

### Auto-fixed Issues

1. [Rule 1 - Test isolation] Added `--legacy-claude-launch` to the start fixture.
   - **Found during:** Task 1 RED
   - **Issue:** The fake `claude` exits the delivery canary before the lock behavior is reached, producing an unrelated failure.
   - **Fix:** Use the established legacy fixture path so the RED assertion reaches start locking.
   - **Files modified:** `crates/devflow-cli/tests/start_lock_e2e.rs`

2. [Rule 1 - Test assertion] Compared worktree porcelain output to its pre-start baseline.
   - **Found during:** Task 1 GREEN
   - **Issue:** Counting porcelain lines assumed one line per worktree; Git emits multiple records for the existing checkout.
   - **Fix:** Assert byte-identical pre/post output, which directly tests that refusal created no worktree.
   - **Files modified:** `crates/devflow-cli/tests/start_lock_e2e.rs`

3. [Rule 1 - Lint] Replaced a boolean equality with negation.
   - **Found during:** Task 2 verification
   - **Issue:** Clippy rejected `success() == false` under `-D warnings`.
   - **Fix:** Used `!success()`; behavior is unchanged.
   - **Files modified:** `crates/devflow-cli/tests/stop_e2e.rs`

## Known Stubs

None.

## Execution Note

Per the operator-approved Phase 48 GSD #4799 workaround, the executor made no task or metadata commit. After independent verification, the orchestrator made the one scoped normal-hook commit for the four owned changes.

## Self-Check: PASSED

All four owned files exist, the normal-hook commit contains only the three code/test files plus this summary, and the pre-existing `.planning/research/.cache/` and `.planning/user/errors/` directories remain untracked and untouched.

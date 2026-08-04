---
phase: 25-end-to-end-dogfood-blockers
plan: 17
subsystem: testing
tags: [rust, rust-drop-guard, unwind-safety, process-reaping, test-infrastructure]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: "25-16's reap_spawned_monitor(&state) trailing call, wired into launch_stage_persists_monitor_pid_for_reload and mid_run_stage_transition_does_not_readjudicate_staleness (WR-03, 999.46)"
provides:
  - "ReapMonitorOnDrop: RAII Drop guard in test_support.rs that reaps a spawned monitor wrapper on every exit path, including a panicking unwind"
  - "reap_monitor_pid: single shared private reap primitive, returns verified liveness instead of asserting"
  - "Two discriminating tests proving the guard reaps during a real unwind and a control proving the old trailing-call form does not"
  - "Both prior call sites (pipeline_launch.rs, staleness.rs) converted from trailing call to guard binding"
affects: [25-end-to-end-dogfood-blockers, any future test that drives launch_stage/launch_stage_inner and must reap the monitor it spawns]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "RAII teardown guard with a std::thread::panicking() interlock: a Drop that may run during an unwind must never itself panic while unwinding is already in flight (would abort() the process), so it downgrades its complaint to eprintln! on that path only"
    - "Discriminating test + control pair: prove a fix works AND prove the pre-fix form would not, using the same deliberate-failure shape in both, so the fix test cannot be vacuous"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/staleness.rs

key-decisions:
  - "ReapMonitorOnDrop::after_launch captures state.monitor_pid BY VALUE (Option<u32>), not a borrowed &'a State, keeping the guard independent of the State binding's own scope"
  - "reap_spawned_monitor is retained (not deleted) as the narrow trailing-call form for the one caller that must demonstrate the failure mode it fixes: the control test"
  - "reap_monitor_pid centralizes the single reap mechanism (terminate_and_verify then verified agent_running check) so both the guard and the plain helper delegate to one implementation — no second escalation path exists"

requirements-completed: [G-25-2]

coverage:
  - id: D1
    description: "ReapMonitorOnDrop RAII guard reaps a spawned monitor on every exit path, including during a panicking unwind, verified by a real sleep(300) child rather than the stubbed agent wrapper (whose exit timing is nondeterministic per WR-05)"
    requirement: "G-25-2"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/test_support.rs#test_support::tests::reap_guard_reaps_the_monitor_when_a_later_assertion_panics"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/test_support.rs#test_support::tests::trailing_reap_call_is_skipped_when_a_later_assertion_panics"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both prior call sites (pipeline_launch.rs launch_stage test, staleness.rs mid-run transition test) converted from the trailing reap_spawned_monitor(&state) call to a guard bound immediately after the launch call and before every panicking checkpoint in the test body, with no assertion deleted, weakened, or reordered"
    requirement: "G-25-2"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness"
        status: pass
      - kind: unit
        ref: "cargo test --workspace"
        status: pass
    human_judgment: false

# Metrics
duration: 12min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 17: Unwind-Safe Monitor Reap Guard Summary

**Replaced the plain trailing `reap_spawned_monitor(&state)` call at both 25-16 sites with an RAII `ReapMonitorOnDrop` guard, proven by a real-unwind test and a control test that demonstrates the trailing form silently skips the reap.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-28T19:04:00Z (approx, first task commit at 15:04:35 -04:00)
- **Completed:** 2026-07-28T19:09:46Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- Added `test_support::reap_monitor_pid(pid) -> bool`, the single shared reap primitive (escalate via `terminate_and_verify`, then return a verified `agent_running` liveness answer), which both the guard and the plain helper now delegate to
- Added `test_support::ReapMonitorOnDrop` — a `pub(crate)` RAII guard whose `Drop` reaps the captured pid unconditionally, with a `std::thread::panicking()` interlock so a still-alive pid during an in-flight unwind is reported via `eprintln!` instead of a second panic (which would `abort()` the whole test binary)
- Added two discriminating tests in `test_support.rs::tests`: `reap_guard_reaps_the_monitor_when_a_later_assertion_panics` (proves the guard reaps during a real `catch_unwind`-driven unwind) and its control `trailing_reap_call_is_skipped_when_a_later_assertion_panics` (proves the old trailing-call form does NOT — the pid is still alive after the unwind), both against a real `sleep 300` child via a `ChildGuard` fixture, per WR-05's finding that the stubbed agent wrapper's exit timing is nondeterministic
- Converted both 25-16 call sites — `pipeline_launch.rs::launch_stage_persists_monitor_pid_for_reload` (guard bound at line 426, ahead of `result.unwrap()` at line 435 and three further panicking checkpoints) and `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` (guard bound at line 776, ahead of `result.expect(...)` at line 786 and the `assert_eq!` on `blocked_count`) — with no assertion deleted, weakened, or reordered
- Whole workspace test suite: 696 passed, 0 failed (694 baseline + 2 new). `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` both clean.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add the unwind-safe reap guard and prove it works, with a control that proves the proof** - `79e0678` (test)
2. **Task 2: Convert pipeline_launch.rs's reap to the guard, bound before all four panicking checkpoints** - `937a057` (test)
3. **Task 3: Convert staleness.rs's reap to the guard, bound before both panicking checkpoints** - `c21ae1d` (test)

_Note: this is a `type="tracer" tdd="true"` Task 1 followed by two `type="auto"` conversion tasks, not a multi-commit RED/GREEN/REFACTOR cycle — Task 1 lands the guard and its two proving tests in one commit since the "RED" state (guard not yet existing) was never itself committed._

## Files Created/Modified
- `crates/devflow-cli/src/test_support.rs` - Added `reap_monitor_pid`, `ReapMonitorOnDrop` (with `after_launch` constructor and `Drop` impl), redocumented `reap_spawned_monitor` as the narrow trailing-call form, and a new `#[cfg(test)] mod tests` with `ChildGuard`, `state_holding`, and the two discriminating tests
- `crates/devflow-cli/src/pipeline_launch.rs` - `launch_stage_persists_monitor_pid_for_reload` now binds `ReapMonitorOnDrop::after_launch(&state)` at line 426 instead of calling `reap_spawned_monitor(&state)` at the former trailing position
- `crates/devflow-cli/src/staleness.rs` - `mid_run_stage_transition_does_not_readjudicate_staleness` now binds `ReapMonitorOnDrop::after_launch(&state)` at line 776 instead of calling `reap_spawned_monitor(&state)` at the former trailing position

## Decisions Made
- `ReapMonitorOnDrop::after_launch` captures `state.monitor_pid` BY VALUE (`Option<u32>`), not a borrowed `&'a State` — avoids tying the guard's lifetime to the `State` binding's own scope, keeping the guard usable regardless of what else happens to `state` afterward
- `reap_spawned_monitor` is retained rather than deleted: it is the plain trailing-call form the control test needs in order to demonstrate the exact failure mode the guard fixes. Its doc comment now opens with that framing and states it should not be used in any new test with assertions between launch and teardown
- Removed the doc-comment-only mention of the literal string `std::thread::panicking()` inside `ReapMonitorOnDrop::drop` (rephrased to refer to "the check below") so the file contains exactly one occurrence of that call, matching the plan's verify gate (`rg -c 'std::thread::panicking' ... | rg -q '^1$'`) while still documenting the interlock in full

## Deviations from Plan

None - plan executed exactly as written. One micro-adjustment during Task 1: the first draft's `Drop` doc comment repeated the literal text `std::thread::panicking()` in prose before the actual call, which made the plan's own verify command (exactly one occurrence of that string in the file) fail; reworded the comment to describe the check without repeating the literal call text. This is not a behavior change, only a self-correction to satisfy the plan's own automated verify gate — logged here rather than as a numbered auto-fix since it did not touch behavior, only a doc comment.

## Issues Encountered
None. All three tasks' `<verify>` blocks passed on first execution (after the doc-comment wording correction above), including the full-workspace 696-test run.

## Control Test Determinism Note

The control test (`trailing_reap_call_is_skipped_when_a_later_assertion_panics`) needed no adjustment to stay deterministic on this machine — it passed on the first run using the `sleep 300` child exactly as specified, confirming `agent_running(pid)` reads `true` after the `catch_unwind`-driven unwind when only the trailing call form is used.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
G-25-2 / `.planning/WINDOWS.md` item 3 is closed: both sites 25-16 wired now reap on every exit path, including a panicking one, and the guard's own correctness is proven by an automated discriminating test rather than asserted by inspection. No further monitor-reap gap is open in this phase's gap-closure tracking as of this plan. `ReapMonitorOnDrop` is now available in `test_support.rs` for any future test that drives `launch_stage`/`launch_stage_inner` and needs to reap the monitor it spawns.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*

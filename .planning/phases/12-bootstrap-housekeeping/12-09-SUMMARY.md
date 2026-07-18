---
phase: 12-bootstrap-housekeeping
plan: 09
subsystem: testing
tags: [rust, cargo-test, orchestration, gates, devflow-cli]

# Dependency graph
requires:
  - phase: 12-bootstrap-housekeeping
    provides: "12-02 (mode/gate primitives this plan's tests drive: Mode::should_gate, GateAction::from_response)"
provides:
  - "Inline unit tests covering advance()'s Ship-success terminal path (finish_workflow) and the Validate consecutive-failures → forced-gate → abort path in crates/devflow-cli/src/main.rs"
affects: [12-bootstrap-housekeeping, 14-reliability-observability-hardening]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Seed a temp git repo + pre-written gate response file to drive run_gate's poll_response to return immediately (no blocking, no spawn) — the standard pattern for testing gate-mediated terminal paths without a real monitor process."

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "Tested the terminal, non-spawning branches of advance()/handle_validate_outcome/handle_ship_outcome only. The spawning transitions (launch_stage → monitor::spawn_monitor) remain out of scope for unit tests per the plan's testability constraint — wiring a launcher seam for that is a refactor, explicitly deferred."
  - "Used raw JSON string literals for seeded GateResponse/DEVFLOW_RESULT files instead of adding a serde_json dev-dependency to devflow-cli — avoids an unnecessary Cargo.toml change since the JSON shapes are simple and stable."

requirements-completed: [12f-advance, 12f-consecutive-failures, 12f-abort]

coverage:
  - id: D1
    description: "advance() over a Ship-stage success with an approved Ship gate runs the terminal finish_workflow flow (after-ship hooks + gate cleanup + state cleared)"
    requirement: "12f-advance"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::advance_ship_success_runs_finish_workflow"
        status: pass
    human_judgment: false
  - id: D2
    description: "Reaching MAX_CONSECUTIVE_FAILURES on a failed Validate forces a gate, and an abort gate response clears state without spawning a new stage"
    requirement: "12f-consecutive-failures"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::validate_failure_threshold_forces_gate_then_aborts"
        status: pass
    human_judgment: false
  - id: D3
    description: "Abort gate-response path (GateAction::Abort) ends the workflow by clearing state"
    requirement: "12f-abort"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::validate_failure_threshold_forces_gate_then_aborts"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-07-08
status: complete
---

# Phase 12 Plan 09: Advance/Ship/Abort Terminal-Path Tests Summary

**Two inline tests in `crates/devflow-cli/src/main.rs` cover `advance()`'s Ship-success finish path and the Validate consecutive-failures→forced-gate→abort path, both previously exercised only via manual/e2e flows per `11-VALIDATION.md`.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-07-08T23:35:00Z (approx.)
- **Completed:** 2026-07-08T23:48:33Z
- **Tasks:** 2/2 completed
- **Files modified:** 1

## Accomplishments
- Added `advance_ship_success_runs_finish_workflow`: seeds a real temp git repo, a `State` at `Stage::Ship`, a `DEVFLOW_RESULT` success marker, and a pre-written approved Ship gate response, then calls `advance(project_root)` and asserts `finish_workflow`'s terminal effects — state cleared (`load_state` errors `MissingState`) and the Ship/Validate gate, response, and ack files all cleaned up.
- Added `validate_failure_threshold_forces_gate_then_aborts`: seeds a `State` at `Stage::Validate` with `consecutive_failures == MAX_CONSECUTIVE_FAILURES - 1` and a pre-written rejected gate response whose note contains "abort", then calls `handle_validate_outcome(project_root, &mut state, false)` directly and asserts the forced gate request was written, `consecutive_failures` reached `MAX_CONSECUTIVE_FAILURES`, and the abort path cleared state.
- Both tests exercise real gate-file I/O and `Gates::poll_response`'s immediate-return-when-present path — no `std::thread::sleep` delay, no monitor/agent process spawned.

## Task Commits

Each task was committed atomically:

1. **Task 1: advance() terminal orchestration — Ship success → finish** - `a861f17` (test)
2. **Task 2: consecutive_failures → forced Validate gate → abort** - `b6b2a7d` (test)

**Plan metadata:** (this commit, docs)

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - Added an `init_repo` test helper (mirrors the pattern in `hooks.rs`'s test module) and the two new inline tests described above, appended to the existing `#[cfg(test)] mod tests` block.

## Decisions Made
- Fully qualified/raw-string JSON literals used for seeded gate response and DEVFLOW_RESULT marker files rather than pulling in `serde_json` as a direct `devflow-cli` dev-dependency — the JSON shapes (`GateResponse`, the marker line) are small and stable, so a hand-written literal keeps the diff surgical.
- Per the plan's explicit testability constraint, the spawning transitions (`transition`/`loop_back_to_code` → `launch_stage` → `monitor::spawn_monitor`) are **not** unit-tested here; they remain covered at the decision level by existing `mode::should_gate`/`hooks::hooks_for_transition` tests. Wiring a launcher seam to make the spawn paths unit-testable is out of scope for this housekeeping phase (noted in the plan itself).

## Deviations from Plan

None - plan executed exactly as written. Both tasks followed the plan's read_first guidance directly (the terminal Ship-success path via `advance()`, and calling `handle_validate_outcome` directly for the threshold→abort path); no fallback to calling `handle_ship_outcome` directly was needed since seeding a DEVFLOW_RESULT success marker for `evaluate_agent_result` worked cleanly through the full `advance()` entry point.

## Issues Encountered
None. All hook side effects fired by `finish_workflow` (`VersionBump`, `BranchCleanup`) are `Result`-based internally and any failure is caught and printed as a warning rather than propagated, so the temp git repo (built via a `hooks.rs`-style `init_repo` helper) didn't need special-casing beyond a standard `main`/`develop`-branch init with a committed `Cargo.toml`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Task coverage gap flagged in `11-VALIDATION.md` under 12f (advance/consecutive-failures/abort) is closed. The remaining 12f items (if any) and the explicitly out-of-scope spawning-path coverage should be tracked against Phase 14 (Reliability & Observability Hardening) if a launcher seam refactor is ever prioritized — no blocker for the rest of Phase 12's plans (12-10, 12-11, 12-12).

---
*Phase: 12-bootstrap-housekeeping*
*Completed: 2026-07-08*

## Self-Check: PASSED

- FOUND commit a861f17 (test(12-09): cover advance() Ship-success finish path)
- FOUND commit b6b2a7d (test(12-09): cover Validate failure-threshold abort path)
- FOUND crates/devflow-cli/src/main.rs
- FOUND tests::advance_ship_success_runs_finish_workflow
- FOUND tests::validate_failure_threshold_forces_gate_then_aborts

---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 11
subsystem: state-write locking and monitor-to-advance binding
tags: [rust, locking, monitor, cli, survivability]
requires: [48-09]
provides: [bounded-advance-lock-wait, launched-stage-binding]
affects: [advance, hidden-monitor, phase-lock]
tech_stack:
  added: []
  patterns: [bounded exponential lock retry, monitor-carried stage identity]
key_files:
  created: []
  modified:
    - crates/devflow-core/src/lock.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/main.rs
decisions:
  - Advance waits a chosen 600 seconds for a phase lock, then writes advance_failed with holder identity.
  - Both monitor shapes carry the launched stage; absent --stage remains a logged legacy-compatible path.
metrics:
  duration: approximately 65 minutes
  completed_date: 2026-09-17
status: complete
plan_head_before: 9e954e1e2f18bf106044879594d7f69f8e8cf122
commits: 1
actuals:
  tokens: 6609
  tasks: 2
  commits: 1
---

# Phase 48 Plan 11: Bounded advance locking and stage binding Summary

`advance` now waits through a bounded per-phase lock contention, logs an observable expiry, and refuses an advance whose monitor-launched stage no longer matches saved state.

## Delivered

- Added `lock::acquire_blocking`, using the existing acquisition path so every retry retains stale-lock reclamation.
- Added the chosen, documented ten-minute `ADVANCE_LOCK_WAIT`; expired waits emit `advance_failed` with reason, holder PID, and elapsed bound before returning an error.
- Added `--stage` to the hidden `advance` and `__monitor` commands; both Legacy and PipeOwning monitor launches send their saved stage.
- Added stage binding after lock acquisition: mismatches emit `advance_failed` with expected/actual stages without changing state; absent `--stage` emits `advance_stage_unbound` and proceeds for old monitor scripts.
- Migrated 14 current `pipeline_launch` fixture callers to explicit `Some(Stage::Code)`: `code_unknown_does_not_transition_to_validate`, `advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed`, `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`, `checkpoint_added_after_code_preflight_parks_at_the_rescan_gate`, `run_rescan_advance`, `approving_the_rescan_gate_records_the_set_and_relaunches`, `rejecting_the_rescan_gate_records_nothing_and_falls_through`, `rejecting_the_rescan_gate_with_abort_aborts`, `advance_with_worktree_declared_checkpoint_reads_the_execution_root`, `advance_without_declared_checkpoint_falls_through_to_generic_gate`, `advance_with_declared_checkpoint_but_unreported_gate_falls_through`, `advance_with_confirmed_checkpoint_and_no_session_id_falls_through`, `advance_at_checkpoint_resume_ceiling_falls_through_to_generic_gate`, and `advance_with_non_claude_agent_never_resumes`.

## Verification

- Task 1 gate: `gate_rc=0`; four exact lock/advance tests each reported exactly one pass with nonzero filtered tests, and the source has one blocking acquire use.
- Task 2 gate: `gate_rc=0`; exact monitor/stage tests passed, `help_snapshot` passed, the full `pipeline_launch::tests::` module passed, and `cargo clippy --workspace --all-targets -- -D warnings` passed.
- Final post-edit checks: `cargo fmt --check`, full `pipeline_launch::tests::`, Clippy, and `git diff --check` all passed.

These checks establish the lock wait, expiry event, stage mismatch, legacy-unbound, monitor argument, hidden-help, and module cases exercised above. They do not establish behavior under a real detached monitor held for the full ten-minute production bound or prove long-run lock contention reliability beyond the deterministic 300 ms boundary controls.

## TDD Gate Compliance

### RED

`pipeline_launch::tests::advance_waits_for_a_held_lock_then_proceeds` failed before implementation with the intended assertion and actual `Err(Message("another devflow process (pid ...) is already running"))`, exit 101, after one discovered Cargo test.

The upstream `tdd-red-evidence` checker returned `INVALID_RED / zero_tests_discovered` because it parses TAP rather than Cargo's test summary. The operator pre-approved the Phase 48 truthful-Cargo exception; the recorded Cargo output names the target test and its planned-behavior assertion, so no TAP evidence was fabricated.

### GREEN

The exact behavior tests and both plan gates passed after implementation. No refactor-only pass was needed.

## Deviations from Plan

### Auto-fixed Issues

1. [Rule 1 - Test assertion] Corrected the expiry-event phase assertion from a string to the event schema's numeric phase value.
2. [Rule 1 - Lint] Added a documented `clippy::too_many_arguments` allowance to the internal hidden-monitor dispatch function after the required stage parameter made its parsed-argument signature eight fields.
3. [Rule 3 - Build compatibility] Preserved a `cfg(test)` two-argument `advance` wrapper for seven callers outside this plan's declared files; production dispatch uses the stage-aware entry point, avoiding unrelated edits.

## Known Stubs

None.

## Self-Check: PASSED

All four declared source files exist and the summary is present. The orchestrator independently verified the change set and committed it with normal hooks under the user-approved Phase 48 workaround for GSD #4799. `.planning/research/.cache/` and `.planning/user/errors/` remain untracked and untouched.

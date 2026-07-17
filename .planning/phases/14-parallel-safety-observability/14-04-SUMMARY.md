---
phase: 14-parallel-safety-observability
plan: "04"
subsystem: observability
tags: [rust, events-jsonl, logs, status, cli]
requires:
  - phase: 14-parallel-safety-observability
    provides: Per-phase workflow state and phase identity
provides:
  - Append-only phase-aware runtime events
  - CLI log following and richer multi-phase status
affects: [hermes-support, status, logs, gate-watchers]
tech-stack:
  added: []
  patterns: [append-only events, fail-soft observability emission, phase-aware CLI diagnostics]
key-files:
  created: [crates/devflow-core/src/events.rs]
  modified: [crates/devflow-cli/src/main.rs, crates/devflow-core/src/workflow.rs]
key-decisions:
  - "Events are append-only, phase-tagged, and fail soft so telemetry never aborts a workflow."
requirements-completed: []
completed: 2026-07-16
status: complete
---

# Phase 14 Plan 04: Events, logs, and richer status

**Added phase-aware operational visibility through append-only events, `devflow logs`, and multi-phase status diagnostics.**

## Accomplishments

- Added schema-v1 `.devflow/events.jsonl` emissions for workflow, stage, gate, hook, and terminal lifecycle events.
- Added `devflow logs [--follow] [--phase N] [--stderr]` and richer `devflow status` output with last-action context.
- Made log following resilient to stage capture-file rollover and made status event lookup a single parse pass.

## Evidence

- **Implementation:** `0ef85ee`.
- **Review fixes:** `07272ad` (capture rollover), `e3c90b2` (one-pass event lookup), and `840064c` (operator watch-live guidance).
- **Validation:** Phase 14's final live rerun recorded 46 events, observed active workflows through `logs` and `status`, and completed both concurrent Ship gates; 260 tests passed with clean clippy and formatting.

## Task Commits

1. **Events, logs, and richer status** - `0ef85ee`
2. **Observability correctness hardening** - `07272ad`, `e3c90b2`, `840064c`

## Decisions Made

- Event emission is intentionally fail-soft; losing telemetry must not abort an active development workflow.
- Logs default only when a single active phase is unambiguous, otherwise the operator selects `--phase`.

## Deviations from Plan

Post-ship review corrected log-follow rollover and status-scan efficiency issues. These fixes were within the observability scope and are recorded in `14-REVIEW-FIX.md`.

## Next Phase Readiness

Phase 17 can consume the stable `events.jsonl` protocol for Hermes gate-watcher support.

---
*Phase: 14-parallel-safety-observability*
*Completed: 2026-07-16*

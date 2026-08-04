---
phase: 14-parallel-safety-observability
plan: "03"
subsystem: workflow-operations
tags: [rust, status, recover, integration-test, parallel]
requires:
  - phase: 14-parallel-safety-observability
    provides: Per-phase state files and checkout locking
provides:
  - Multi-phase status and safe recovery behavior
  - Parallel integration coverage for independent phase progress
affects: [status, recover, logs, operations]
tech-stack:
  added: []
  patterns: [stale-only recovery cleanup, multi-phase operational views]
key-files:
  created: []
  modified: [crates/devflow-core/src/recover.rs, crates/devflow-cli/src/main.rs, crates/devflow-cli/tests/phase7_cli.rs]
key-decisions:
  - "Recover cleanup retains live and fresh sibling phases unless an explicit phase is requested."
requirements-completed: []
completed: 2026-07-16
status: complete
---

# Phase 14 Plan 03: Multi-phase status, recovery, and parallel acceptance

**Made operational commands enumerate every active phase and proved independent parallel progress with a real two-phase workflow run.**

## Accomplishments

- Updated `devflow status` and `devflow recover` to operate over all active per-phase states.
- Added parallel acceptance coverage for independent state machines and no shared-file clobbering.
- Corrected `recover --clean` to remove stale phases only, with `--phase N` as the explicit force-clean escape hatch.

## Evidence

- **Implementation:** `0ef85ee`.
- **Review fixes and regression coverage:** `49859fd` (safe stale-only cleanup), `2116e35` (parallel pid-file test stability), and `e3c90b2` (partial-integration guidance).
- **Validation:** the live two-phase end-to-end run showed both workflows reaching distinct Ship gates and completing without state clobbering.

## Task Commits

1. **Multi-phase status/recovery and acceptance test** - `0ef85ee`
2. **Recovery and parallel-test hardening** - `49859fd`, `2116e35`, `e3c90b2`

## Decisions Made

- `recover --clean` favors liveness: live or fresh sibling phases are retained, while an explicit phase argument is required to clear one unconditionally.

## Deviations from Plan

Post-ship review found that indiscriminate recovery cleanup could destroy a live sibling workflow; the correction and fail-to-pass evidence are preserved in `14-REVIEW-FIX.md`.

## Next Phase Readiness

Plan 04 can expose phase-aware state and captures through logs, events, and richer status output.

---
*Phase: 14-parallel-safety-observability*
*Completed: 2026-07-16*

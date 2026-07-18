---
phase: 14-parallel-safety-observability
plan: "02"
subsystem: workflow-concurrency
tags: [rust, locking, sequentagent, monitor, git]
requires:
  - phase: 14-parallel-safety-observability
    provides: Per-phase state and explicit monitor advance identity
provides:
  - Serialized primary-checkout mutations
  - Monitor-owned synchronous agent capture for sequentagent
affects: [ship, sequentagent, recover, events]
tech-stack:
  added: []
  patterns: [short checkout lock, monitor as the single agent-launch path]
key-files:
  created: []
  modified: [crates/devflow-core/src/lock.rs, crates/devflow-core/src/agent.rs, crates/devflow-core/src/monitor.rs, crates/devflow-cli/src/main.rs]
key-decisions:
  - "Checkout mutations serialize briefly and are never held across a gate wait."
  - "Sequentagent uses the monitor rather than a second synchronous capture path."
requirements-completed: []
completed: 2026-07-16
status: complete
---

# Phase 14 Plan 02: Checkout lock and monitor-owned sequentagent

**Serialized shared-checkout mutations while consolidating every agent launch and capture path behind the monitor.**

## Accomplishments

- Added the bounded `.devflow/lock-project` for primary-checkout git mutations and phase-specific cron-instruction handling.
- Moved `sequentagent` to a no-advance monitor path and removed the obsolete synchronous `launch_agent` / `capture_agent_output` path.
- Hardened timeout behavior so checkout hooks are skipped with an explicit warning instead of ever running unserialized.

## Evidence

- **Implementation:** `0ef85ee`.
- **Review fixes:** `a4a9f54` (checkout-lock timeout), `840064c` and `d4e527e` (agent-binary preflight), plus `e3c90b2` (integration-timeout guidance).
- **Validation:** the final Phase 14 rerun passed 260 workspace tests, clippy, formatting, and the two-phase end-to-end scenario.

## Task Commits

1. **Checkout lock and monitor-owned sequentagent** - `0ef85ee`
2. **Post-ship correctness hardening** - `a4a9f54`, `840064c`, `d4e527e`

## Decisions Made

- A checkout-lock timeout preserves safety by skipping the hook batch; it must not execute git mutations concurrently.
- The monitor is the only agent-process execution path, including blocking `sequentagent` runs.

## Deviations from Plan

Post-ship review identified operational correctness gaps and fixed them in the listed commits; the complete disposition is recorded in `14-REVIEW-FIX.md`.

## Next Phase Readiness

Plan 03 can enumerate and recover multiple phase states without reintroducing shared mutable state.

---
*Phase: 14-parallel-safety-observability*
*Completed: 2026-07-16*

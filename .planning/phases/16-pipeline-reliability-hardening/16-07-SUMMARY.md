---
phase: 16-pipeline-reliability-hardening
plan: 07
subsystem: operator-observability
tags: [gates, status, history, retained-captures, review-artifacts]

requires:
  - phase: 16-03
    provides: timestamped retained capture generations
  - phase: 16-06
    provides: walk-up project-root resolution and safe CLI argument shapes
provides:
  - persistent status-side pending-gate signal with age escalation
  - read-only per-phase timeline joining schema-v1 events and retained evidence
  - devflow history CLI surface
affects: [operator-response, incident-forensics, status, cli]

tech-stack:
  added: []
  patterns:
    - additive status-side reliability signal independent of external notification transport
    - single-pass event-log fold with read-only artifact correlation

key-files:
  created:
    - crates/devflow-core/src/history.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md
    - README.md

key-decisions:
  - "Pending gates escalate after the named 30-minute GATE_ESCALATION_THRESHOLD_SECS threshold."
  - "The banner renders phase, stage, age, truncation-safe context, and literal approve/reject commands between prominent delimiters."
  - "AttemptTimeline contains chronological AttemptEntry values; retained capture generations and REVIEW.md files attach to the nearest preceding event."
  - "devflow history [phase] is positional and defaults to the single active phase."
  - "fire_gate_notify and run_notify_command remain unchanged; the persistent status banner is additive."

patterns-established:
  - "Operator-controlled history views derive from events.jsonl and retained local artifacts without adding a store."
  - "Agent-controlled text reused in a status surface passes through truncate_reason."

requirements-completed: [16j, 16h]

coverage:
  - id: D1
    description: "Pending gates remain prominently visible and escalate with age while bounding untrusted context."
    requirement: 16j
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#status_shows_pending_gate_prominently"
        status: pass
      - kind: manual
        ref: "devflow status against an old temporary Ship gate"
        status: pass
    human_judgment: false
  - id: D2
    description: "Phase attempts render chronologically with retained capture evidence and an empty-phase result."
    requirement: 16h
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#timeline_orders_events_and_correlates_retained_captures"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#empty_phase_has_clean_no_attempts_result"
        status: pass
      - kind: other
        ref: "cargo build -p devflow"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-07-18
status: complete
---

# Phase 16 Plan 07: Operator Visibility Summary

**Open gates now remain impossible to miss in status, and phase attempts are reconstructable from one read-only CLI view.**

## Performance

- **Duration:** 4 min
- **Completed:** 2026-07-18T01:21:43Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Added a persistent pending-gate banner that escalates after 30 minutes and includes exact response commands.
- Reused `truncate_reason` before rendering gate context; external notify functions were not changed.
- Added a schema-v1 event timeline correlated with timestamped capture generations and retained review artifacts.
- Exposed the timeline as `devflow history [phase]` through the walk-up project resolver.

## Task Commits

1. **Task 1 RED: Pending-gate banner contract** - `b9e90af`
2. **Task 1 GREEN: Persistent escalating status banner** - `76d2bb4`
3. **Task 2 RED: Attempt-history contract** - `8df6566`
4. **Task 2 GREEN: Correlator and CLI view** - `ec3d919`

## Deviations from Plan

None.

## Issues Encountered

None.

## User Setup Required

None.

## Self-Check: PASSED

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-18*

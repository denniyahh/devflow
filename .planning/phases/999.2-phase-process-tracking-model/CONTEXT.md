---
status: backlog
source: Phase 17 dogfood run (2026-07-18/19), moved from ROADMAP Phase 19 (19b) on 2026-07-20
---

# Backlog: A Phase Tracks Exactly One Process

## Goal

One `phase-N-agent-pid` file per phase leaves the monitor unrecorded and
`sequentagent`'s second agent homeless. Frame as two tracked processes per
phase, not a general multi-agent table. Orphaned strays (a live
test-fixture agent under `/tmp`) are invisible to every devflow command.

## Notes

Related to `18b` (monitor liveness observability, promoted to active Phase
18) — that item persists and probes a single `monitor_pid`; this item is
the broader "a phase has two tracked processes, not one" data-model change
that `18b`'s persistence field should eventually live inside of. Revisit
after 18a/18b ship so this doesn't duplicate their liveness-probing logic.

Promote with `/gsd-review-backlog` when ready.

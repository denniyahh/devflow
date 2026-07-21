---
status: backlog
source: operator request 2026-07-20, during the Phase 18 planning dogfood attempt
---

# Backlog: Manual Ship Override

## Goal

A command that lets an operator drive a phase through Ship by hand, without
depending on a live monitor to consume a gate response.

## Why `devflow gate approve` does not already cover this

Verified at source 2026-07-20:

1. **`respond()` refuses when no gate is open** — `gates.rs:186` returns
   `GateError::NoOpenGate` (test: `respond_refuses_when_no_gate_is_open`,
   `gates.rs:506`). If the monitor died before it ever wrote the Ship gate
   request, there is nothing to approve and no way in.
2. **Approving only writes a response file.** `respond()` writes
   `NN-ship.response.json`; a *live monitor polling that path* is what
   actually advances the workflow. If the monitor is dead — and per `18b`
   a dead monitor is currently indistinguishable from a healthy
   between-stages moment — the approval sits unconsumed forever and
   nothing happens.

So the existing gate commands assume a healthy pipeline. The gap is
recovery when that assumption fails, which on this project's dogfood
history is not rare.

## Proposed shape

`devflow ship --phase N [--force]` (name TBD) that executes the terminal
transition directly: run the after-ship hook batch (Merge, VersionBump,
ChangelogAppend, BranchCleanup) in-process, honoring the existing
fail-closed contract — a failed Merge must still stop the batch, preserve
state, and refuse to emit `workflow_finished` (the Phase 16 invariant,
regression-tested).

Open questions for discuss-phase:

- Should it require the phase to actually be *at* Ship, or allow forcing
  from an earlier stage? (Leaning: require Ship, with `--force` as the
  documented escape hatch, since skipping Validate silently is how false
  greens happen.)
- Interaction with `18f`'s preflight-override decision — both are
  "human has adjudicated this, stop re-asking" semantics and should
  probably share one mechanism rather than inventing two.
- Must not become a way to bypass the terminal Ship invariant that Phase
  16 established and Phase 17 verified.

## Notes

Sequence after `18a`/`18b` — a manual override is most valuable exactly
when reconciliation can tell you *why* the pipeline is stuck, and those
two items provide that.

Promote with `/gsd-review-backlog` when ready.

# Phase 40: Pi Dogfood - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-18
**Phase:** 40-pi-dogfood
**Areas discussed:** Dogfood subject, Run depth, Isolated-context dispatch, Hardening bar

---

## Dogfood Subject

| Option | Description | Selected |
|--------|-------------|----------|
| 1 | A small real DevFlow change (a backlog item or scoped fix) | ✓ |
| 2 | The isolated-context dispatch patch (build it as the subject) | |
| 3 | A deliberately trivial change (doc/test-only) | |

**User's choice:** Option 1, specifically **999.85** (the two stale comments).
**Notes:** User asked for an explanation of the isolated-context dispatch patch and a viability
assessment. Assessment: poor subject — it builds new capability rather than verifying the shipped
driver, the drain-predicate half is a real project (the Claude drain-gate problem), it has no
driving need (`@bacnh85`'s in-process model covers it), and it muddies the verification signal.
User agreed; 999.85 chosen. This moves MAINT-01 (999.85) from Phase 45 → Phase 40.

## Run Depth

| Option | Description | Selected |
|--------|-------------|----------|
| 1 | Through Validate only | ✓ |
| 2 | Full through Ship (real version bump + tag) | |

**User's choice:** 1.
**Notes:** Ship is agent-independent (version bump/tag/changelog), already proven by the
v2.5.0/v2.7.0 dogfoods, and a real release for a comment-only change is unwanted.

## Isolated-Context Dispatch

**User's choice:** Re-filed (not built this phase).
**Notes:** Resolved implicitly by the subject decision; consistent with Phase 39's own deferred note.

## Hardening Bar

| Option | Description | Selected |
|--------|-------------|----------|
| 1 | Happy path only | |
| 2 | Happy path + one live gate | ✓ |
| 3 | Happy path + Pi-specific failure drills | ✓ |

**User's choice:** 2 & 3, with a question about whether simulation is required.
**Notes:** User correctly observed the failure drills (marker-less run, non-zero exit, hung Pi)
cannot be guaranteed in a real run. Resolution: the live gate (option 2) is real — supervise mode
fires a deterministic gate; the failure drills (option 3) are regression tests with a stubbed `pi`
binary, and only the Pi-transport delta is new (the generic marker/exit/liveness logic is already
regression-tested in Phases 13/17/18).

## the Agent's Discretion

- Exact live-gate point (natural stage transition vs. declared checkpoint) — planner's call.
- Precise regression-test gap set — audit existing generic coverage, add only Pi-transport-specific tests.

## Deferred Ideas

- Isolated-context (process-spawning) dispatch — re-filed.
- DEN-95 (999.74) / DEN-98 (999.76) status sweep — out of scope (flagged in the 999.85 entry).

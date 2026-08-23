---
status: backlog
source: Phase 20 decision D-08 / 20-RESEARCH.md assumption A1 (2026-07-22)
---

# Backlog: `devflow parallel` Git Object-Store Race

## Goal

Confirm-or-refute whether `devflow parallel`'s concurrent per-worktree commits
can hit the same git object-store corruption observed in Phase 20's 20b
instance 2, and fix it at the product level if the race is real.

## The finding

Phase 20's 20b instance 2 was a CI flake: `start_worktree_mode_ignores_main_checkout_divergence`
(`phase7_cli.rs:236`) hit `error: invalid object ... for 'f46.txt'` mid-way
through a 60-commit loop — the index referenced a loose object absent from the
object store, consistent with weak fsync ordering on shared CI runners under
concurrent test-binary load. Phase 20 fixed it **fixture-side only** (per D-05):
durability settings on fixture repos and/or shrinking the loop window.

While resolving it, 20-RESEARCH.md (assumption A1) flagged a *plausible but
unconfirmed* product analog: `devflow parallel` runs concurrent per-worktree
commits with **no DevFlow-level lock serializing them**. If the same
fsync-ordering weakness that corrupted the test fixture can occur under real
concurrent `devflow parallel` load, a real user's repository could intermittently
produce genuine object-store corruption — low likelihood, high severity.

The research could not confirm this analog from source alone; it needs a
deliberate reproduction attempt, not a code read.

## Why this matters

20b instance 2 is fixed where it was observed (the fixture), but a fixture-only
fix says nothing about the product. If `devflow parallel` shares the hole, the
next occurrence is not a re-runnable red CI job — it is a corrupted user repo
with an opaque `invalid object` error and no obvious cause. Confirming or ruling
this out is the difference between "we fixed a flaky test" and "we know the
product is safe under concurrent load."

## Possible shapes (not yet decided)

- A stress reproduction: drive `devflow parallel` across N worktrees committing
  concurrently under CI-like fsync conditions, looking for the `invalid object`
  signature. If it never reproduces after a strong effort, close as
  fixture-only-confirmed and record the negative result.
- If it reproduces: a DevFlow-level serialization or per-worktree fsync
  discipline around the concurrent commit path — mirroring the durability fix
  20b applied fixture-side, but in the product commit path.
- Cross-reference 999.4 (version-tag contention on concurrent ship) — same
  concurrency family, different resource (tag ref vs. object store).

## Notes

Priority: Medium — low likelihood, but high severity if real, and it is the only
open thread from Phase 20's 20b that touches a real user's repository rather than
a test fixture.
Size: M — dominated by the reproduction effort; the fix (if needed) is bounded.

Relates to 999.4 (concurrent-ship contention) and Phase 20's 20b.

Promote with `/gsd-review-backlog` when ready.

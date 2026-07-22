# Phase 20: Release Correctness + Operator Control - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-22
**Phase:** 20-release-correctness-operator-control
**Areas discussed:** 20e mechanism sharing, 20e scope of force, 20d's ceiling, 20b product-vs-fixture (both instances)

---

## 20e mechanism sharing

| Option | Description | Selected |
|--------|-------------|----------|
| Shared adjudication record, two consumers | One on-disk gate-response record; 18f's live poll and 20e's out-of-process command both consume it, both driving the same `finish_workflow()` effect | ✓ |
| 20e re-derives the effect independently | Standalone path replicating the hook batch, no shared code with 18f | |

**User's choice:** Shared adjudication record, two consumers.
**Notes:** Grounded against source: `run_gate`'s blocking poll (`pipeline_gate.rs:168`) is what 18f's override actually modifies; 20e can't depend on that process being alive, so it reads the same response file/schema and calls `finish_workflow()` directly instead of duplicating what "approving Ship" means.

---

## 20e scope of force

| Option | Description | Selected |
|--------|-------------|----------|
| Require Stage::Ship only | `--force` only works when already at Ship with an unconsumed gate; earlier stages error out | ✓ |
| Allow forcing from any stage | `--force` can skip Validate entirely from an earlier stage | |

**User's choice:** Require Stage::Ship only.
**Notes:** Preserves the Phase 16 terminal-Ship invariant and Phase 17's verification of it; avoids becoming a Validate-skipping bypass.

---

## 20d's ceiling

| Option | Description | Selected |
|--------|-------------|----------|
| --check only | Read-only preflight, four checks, no execution | ✓ |
| --check plus executor | Also build the full release-cut executor (merge/tag/sync/publish) | |

**User's choice:** `--check` only — with an explicit requirement to file a backlog item for the executor so it isn't lost.
**Notes:** User: "let's do check only but make sure that we have a backlog item to do the executor later on." Captured in CONTEXT.md's Deferred section as an action item — not yet filed as a Linear/backlog issue; to be raised explicitly at `/gsd-review-backlog` or Phase 20 ship time.

---

## 20b instance 1 — worktree removal race

| Option | Description | Selected |
|--------|-------------|----------|
| Product fix: retry + prune fallback | Lock in bounded-backoff retry + `git worktree prune` fallback in `cleanup --force` now | |
| Verify reachability first, decide during planning | Have the phase-researcher confirm a real user can hit the same race before committing to the fix shape | ✓ |

**User's choice:** Verify reachability first.
**Notes:** CONTEXT.md's existing lean (product-fix-likely) is preserved as the working hypothesis, but not locked — the researcher must confirm before planning commits to it.

---

## 20b instance 2 — git object-store corruption

| Option | Description | Selected |
|--------|-------------|----------|
| Fixture-only fix | Lock in as fsync/durability settings on fixture repos, no product change | |
| Still worth a product-reachability check | Have the researcher check whether DevFlow's own git operations could hit the same race under real concurrent load before locking it as fixture-only | ✓ |

**User's choice:** Still worth a product-reachability check.
**Notes:** CONTEXT.md's existing lean (fixture-durability, no obvious product analog) stands as the default, but the researcher must still check before it's locked.

---

## Claude's Discretion

None — all five areas resulted in explicit user selections.

## Deferred Ideas

- **Release-cut executor** (a `devflow release` that runs the actual merge → tag → sync → publish sequence, not just `--check`) — out of scope for Phase 20 per the 20d decision above. Needs a new backlog item filed before/at ship.

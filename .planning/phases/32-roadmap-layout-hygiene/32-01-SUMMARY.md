---
phase: 32-roadmap-layout-hygiene
plan: 01
subsystem: planning-tooling
tags: [gsd-tools, roadmap-parsing, planning-docs]
provides:
  - "ROADMAP.md layout that gsd-tools' milestone-scoped parsers (roadmap.analyze, milestone.complete --dry-run) read correctly for the active milestone"
  - "## Progress table so the roadmap-derived completion check is used instead of the legacy STATE.md two-scale comparison"
  - "999.72/999.72a backlog entries marked resolved with an inline provenance note"
  - "CLAUDE.md convention documenting the two root causes for future milestone boundaries"
affects: [gsd-hygiene milestone close, future /gsd-new-milestone runs]
actuals:
  tokens: ~4k (docs edits only, no code)
  tasks: 3
  commits: 6 (9c663ad, ee4b3a4, 09db08e, d043a68, ceb3b01, 77799a9)
tech-stack:
  added: []
  patterns: []
key-files:
  created:
    - .planning/phases/32-roadmap-layout-hygiene/32-CONTEXT.md
    - .planning/phases/32-roadmap-layout-hygiene/32-DISCUSSION-LOG.md
    - .planning/phases/32-roadmap-layout-hygiene/32-VERIFICATION.md
  modified:
    - .planning/ROADMAP.md
    - CLAUDE.md
    - .planning/REQUIREMENTS.md
    - .planning/STATE.md
key-decisions:
  - "D-01: plan scope is verification-only, no ROADMAP.md structural edits — the fix had already landed via gsd-roadmapper's milestone-creation write, before this phase was ever discussed"
  - "D-02 (operator decision): 999.72/999.72a backlog entries flipped from (BACKLOG) to (RESOLVED — 2026-08-04, Phase 32) with an inline provenance note"
  - "D-03 (operator decision): durability for future milestones handled via a documented CLAUDE.md convention (option a), not a CI check (option b) or a gsd-core fix (option c, out of scope)"
duration: ~90min (interactive discuss-phase session + follow-up)
completed: 2026-08-04
status: complete
---

# Phase 32: ROADMAP Layout Hygiene Summary (Minimal)

**Confirmed the ROADMAP.md layout fix (HYGIENE-01/02/03) had already landed as a side effect of creating this milestone, then closed the loop: flipped the 999.72/999.72a backlog entries, documented a durability convention, and independently re-verified the phase goal from scratch.**

## Performance
- **Duration:** ~90 minutes, interactive
- **Tasks:** 3
- **Files modified:** 6 (2 core: ROADMAP.md, CLAUDE.md; 4 planning-bookkeeping: REQUIREMENTS.md, STATE.md, plus this phase's own CONTEXT/DISCUSSION-LOG/VERIFICATION)

## Accomplishments
- Re-verified live (not trusted from a prior claim) that `roadmap.analyze` reports `phase_count: 1` and `milestone.complete gsd-hygiene --dry-run` no longer triggers the pass-all degrade — both were the exact symptoms of backlog `999.72`.
- Flipped `999.72`/`999.72a` from `(BACKLOG)` to `(RESOLVED — 2026-08-04, Phase 32)`, with an inline note that the actual fix came from `gsd-roadmapper`'s write, not a plan/execute cycle — an explicit operator decision, not assumed.
- Added a CLAUDE.md convention ("Keep the active milestone's phase headings inside its own window") so the next `/gsd-new-milestone` / `/gsd-complete-milestone` has the two root causes and a spot-check command in front of it — an explicit, scoped-down operator choice ("good enough for now," not a CI-enforced guarantee).
- Independently re-verified via a standalone `gsd-verifier` run (not self-reported): 4/4 success criteria pass, `status: passed`.
- `phase.complete 32` ran clean: ROADMAP Progress table row → Complete, REQUIREMENTS.md's three HYGIENE checkboxes → checked, STATE.md → `status: completed`.

## Task Commits
1. **Task 1: Discuss phase context and capture decisions** - `9c663ad`, `ee4b3a4`
2. **Task 2: Apply D-02 and D-03** - `09db08e`, `d043a68`
3. **Task 3: Independently verify phase goal achievement** - `ceb3b01`
4. (phase closure, not its own task) `phase.complete 32` - `77799a9`

## Files Created/Modified
- `.planning/phases/32-roadmap-layout-hygiene/32-CONTEXT.md` — decisions D-01/D-02/D-03, canonical refs
- `.planning/phases/32-roadmap-layout-hygiene/32-DISCUSSION-LOG.md` — audit trail of the discuss-phase session and its follow-up
- `.planning/phases/32-roadmap-layout-hygiene/32-VERIFICATION.md` — independent goal-backward verification, 4/4 passed
- `.planning/ROADMAP.md` — 999.72/999.72a status flip (this plan's only structural edit; the Phase 32 section and `## Progress` table already existed from `0b1ad74`)
- `CLAUDE.md` — new durability-convention section

## Next Phase Readiness
This was the only phase in the `gsd-hygiene` milestone. Ready for `/gsd-complete-milestone` (unversioned — see PROJECT.md's "Current Milestone" framing for why no `vX.Y.Z` applies here).

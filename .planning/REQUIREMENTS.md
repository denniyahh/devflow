# Requirements: GSD Workflow Hygiene

**Defined:** 2026-08-04
**Core Value:** A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never silently corrupt its
own state or lose a human's gate decision, even under a mid-run crash or kill.

**Note:** this is the first `REQUIREMENTS.md` this project has ever had — prior milestones
tracked requirements per-phase in each phase's `CONTEXT.md`, not via formal REQ-IDs (see
`PROJECT.md` § Context). Adopted here because this milestone runs through the full
`/gsd-new-milestone` workflow rather than being hand-authored.

## v1 Requirements

Requirements for this milestone. Each maps to roadmap phases.

### ROADMAP Layout

- [ ] **HYGIENE-01**: Operator can run `roadmap.analyze` against the active milestone and get a
  correct non-zero phase count instead of today's `phase_count: 0` misfire
- [ ] **HYGIENE-02**: Operator can run `milestone.complete --dry-run` against the active milestone
  without it triggering the pass-all degrade that sweeps unrelated directories
- [ ] **HYGIENE-03**: `ROADMAP.md` has a `## Progress` table, closing 999.72a, so
  `state.validate`'s roadmap-derived path works instead of falling back to a legacy STATE.md
  comparison

## v2 Requirements

None identified — this milestone is intentionally narrow (see PROJECT.md's "Not fixable by
anything DevFlow-side" note: the remaining gsd-core defects, including this milestone's own
verification tools, live in a different codebase and aren't DevFlow requirements).

## Out of Scope

| Item | Reason |
|---|---|
| Fixing `gsd-core`'s source (entries 13, 14, 16, 17 in the upstream issue ledger) | Lives in a different repository; not DevFlow's code to change. Filed upstream, tracked separately per operator decision (2026-08-04: "not now"). |
| Retroactively archiving what's left un-milestoned (there is none — phases 1-31 are fully archived as of this milestone's start) | N/A — already complete |
| Restructuring the `## Backlog` section itself | Out of scope for this milestone; only the milestone-heading/phase-detail ordering is in scope |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| HYGIENE-01 | TBD | Pending |
| HYGIENE-02 | TBD | Pending |
| HYGIENE-03 | TBD | Pending |

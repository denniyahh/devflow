# Phase 32: ROADMAP Layout Hygiene - Context

**Gathered:** 2026-08-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Restructure `.planning/ROADMAP.md` so `gsd-tools query roadmap.analyze` and `gsd-tools query
milestone.complete <label> --dry-run`, run against this repo's active milestone, correctly find
this milestone's own phase (32) instead of misfiring — and so the `## Progress` table exists and
is used by `state.validate`'s roadmap-derived completion check instead of the legacy STATE.md
comparison fallback. Pure `.planning/` documentation structure — no crates code changes (per
PROJECT.md's Current Milestone framing).

**Load-bearing finding from this discussion session, not from the user:** the phase's own success
criteria are already met. `gsd-roadmapper`'s write when creating this milestone (commit
`0b1ad74`, "docs: create milestone GSD Workflow Hygiene roadmap (1 phase)") already produced the
layout Phase 32 was scoped to deliver. Re-verified live in this session (2026-08-04), not just
trusted from STATE.md's claim:
- `gsd-tools query roadmap.analyze` → `phase_count: 1`, lists Phase 32, `next_phase: "32"` (was
  `phase_count: 0` per the 999.72 bug report).
- `gsd-tools query milestone.complete gsd-hygiene --dry-run` → correctly errors "1 unstarted
  phase(s) (e.g. Phase 32)" instead of the pass-all degrade that used to sweep all 17 backlog
  directories.
- `.planning/ROADMAP.md` already contains a `## Progress` table (columns `Phase`, `Plans
  Complete`, `Status`, `Completed`) at line 42.

So this is a verify-and-close phase, not a build phase.

</domain>

<decisions>
## Implementation Decisions

### Discussion outcome
The user was offered three gray areas (plan scope given the fix already landed; whether to close
out the 999.72/999.72a backlog entries' status labels; durability for future milestones) and
explicitly declined all three, selecting "None of these — I'm ready for context: treat as a
straightforward verify-and-close with no open decisions." The items below are **Claude's
defaults resulting from that decline**, not operator-chosen answers to each sub-question — the
operator did not pick among the options, they opted out of picking. Flagging the provenance
explicitly per the "no invented constraints" convention: do not treat these as re-litigated or
re-affirmed operator decisions if a downstream agent needs to revisit them.

### Claude's Discretion
- **D-01 (plan scope):** The plan should be verification-only — re-run the three checks above
  live against HEAD, and write SUMMARY.md/VERIFICATION.md documenting that they pass. No edits to
  `.planning/ROADMAP.md` are planned; the layout is already correct and Success Criterion 4
  ("this phase only inserts a new section; it moves nothing") is satisfied by construction since
  nothing needs inserting. — **Reversibility:** reversible — if verification surfaces a real gap
  (e.g., a check that only passes today because of the specific `--dry-run` label used, or a
  parser edge case the three checks don't exercise), the plan can still add a targeted ROADMAP.md
  edit.
- **D-02 (999.72 / 999.72a backlog entries):** Left untouched — do not change their `(BACKLOG)`
  status suffix to `(DELIVERED — Phase 32)` or similar, even though other resolved backlog items
  in ROADMAP.md follow that convention (e.g. 999.29, 999.30). Rationale: REQUIREMENTS.md's Out of
  Scope table lists "Restructuring the `## Backlog` section itself" with the reason "only the
  milestone-heading/phase-detail ordering is in scope" — read narrowly as covering this too.
  — **Reversibility:** reversible — a one-line edit if a future session decides otherwise.
- **D-03 (durability for future milestones):** Not addressed by this phase. 999.72's own
  ROADMAP.md text warns the fix "is not guaranteed" to hold at the next milestone boundary
  (depends on `gsd-roadmapper` doing the right thing again, plus a documented `phase.add`
  insertion-point bug). REQUIREMENTS.md scopes this milestone as "intentionally narrow" and
  places gsd-core-side fixes out of scope. If this needs to become durable, that is a future
  backlog decision, not this phase's.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### This phase's own scope definition
- `.planning/ROADMAP.md` §"Phase 32: ROADMAP Layout Hygiene" (line 20) — goal, success criteria,
  the already-satisfied state
- `.planning/ROADMAP.md` §"## Progress" (line 42) — the table HYGIENE-03 requires; already present
- `.planning/REQUIREMENTS.md` — HYGIENE-01/02/03, and the "Out of Scope" table (backlog-section
  restructuring, gsd-core fixes)
- `.planning/PROJECT.md` §"Current Milestone: GSD Workflow Hygiene" — goal and target framing

### The bug this phase closes
- `.planning/ROADMAP.md` §"Phase 999.72: ROADMAP.md Layout Hides Every Phase From `gsd-tools`'
  Milestone-Scoped Parsers (BACKLOG)" (line 325) — full root-cause history (Cause 1: closed
  milestone heading interrupts the active window; Cause 2: `### Phase N:` reused by historical
  entries), including sub-item 999.72a (missing `## Progress` table) and the "not guaranteed to
  hold for future milestones" caveat
- `.planning/UPSTREAM-GSD-ISSUES.md` (symlink → `../../gsd-core/scratch/UPSTREAM-GSD-ISSUES.md`)
  — entry 16, the write-path sibling defect in `gsd-tools milestone.complete`'s own pass-all
  degrade, found the same day as this milestone; not this phase's code to fix, but useful context
  for why `--dry-run` is the check used instead of a real completion attempt

### Parser implementation (outside this repo — informational only)
- `~/.claude/gsd-core/bin/lib/roadmap-parser.cjs` and `~/.claude/gsd-core/bin/lib/roadmap.cjs` —
  where `extractCurrentMilestone` and `deriveProgressFromRoadmap` live. Global gsd-core install,
  not tracked in this repo — read only if the verification plan needs to understand exact parser
  behavior beyond the black-box checks already run in this session.

</canonical_refs>

<code_context>
## Existing Code Insights

No crates code is in scope. The only "codebase" here is `.planning/ROADMAP.md` itself, already
in the correct shape (see `<domain>` above).

### Reusable Assets
- None — no code changes.

### Established Patterns
- Resolved backlog entries elsewhere in ROADMAP.md carry a status suffix on their own heading,
  e.g. `### Phase 999.29: ... (DELIVERED — Phase 21 / 21d)`. Per D-02, this phase does not apply
  that pattern to 999.72/999.72a — noted here only so a downstream agent doesn't "helpfully"
  apply it.

### Integration Points
- `phase.complete` will update this phase's own `## Progress` table row (`| 32 | ... | Not
  started | — |`) automatically once the phase completes — confirmed by 999.72a's own text
  ("maintained incrementally by `phase.complete`"). Not something the plan needs to do by hand.

</code_context>

<specifics>
## Specific Ideas

No specific implementation ideas from discussion — the user deferred entirely to the "already
verified, just close it out" framing established in this session.

</specifics>

<deferred>
## Deferred Ideas

- **Durability for future milestones** (D-03 above) — making the ROADMAP.md layout fix
  self-sustaining across milestone boundaries, rather than dependent on `gsd-roadmapper`
  happening to write phases inside the right window each time. Would need its own backlog item
  and operator scoping decision; not raised as a new backlog entry here since that itself would
  be a scope decision the operator didn't make.
- **Closing 999.72 / 999.72a's status labels** (D-02 above) — deferred by narrow reading of
  REQUIREMENTS.md's Out of Scope line, not ruled out permanently. A future session could revisit
  whether "no longer live" (PROJECT.md's Goal) implies the backlog entry should say so.

### Reviewed Todos (not folded)
None — `todo.match-phase 32` returned zero matches.

</deferred>

---

*Phase: 32-ROADMAP Layout Hygiene*
*Context gathered: 2026-08-04*

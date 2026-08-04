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
initially declined all three, selecting "None of these — I'm ready for context: treat as a
straightforward verify-and-close with no open decisions." Immediately after CONTEXT.md was first
written, the user reopened D-02 directly ("flip the backlog items' status") in the same session —
recorded below as an operator decision, superseding the earlier Claude's-Discretion default. D-01
and D-03 remain Claude's defaults from the opt-out, not operator-chosen answers.

### Decisions
- **D-02 (999.72 / 999.72a backlog entries) — operator decision, reversing the original default:**
  Flipped. `.planning/ROADMAP.md`'s `### Phase 999.72:` heading now reads
  `(RESOLVED — 2026-08-04, Phase 32)`, with an inline "Resolution" paragraph documenting that
  `gsd-roadmapper`'s milestone-creation write (not a Phase-32 plan/execute cycle) is what actually
  closed it, and that the fix is confirmed for this milestone only — not structural. Sub-item
  999.72a is marked resolved the same way. No `gsd-tools` handler exists for backlog-heading
  status edits (checked: `roadmap`'s subcommands are `analyze, get-phase, update-plan-progress,
  annotate-dependencies, validate, upgrade`; `phase`'s are `uat-passed, next-decimal, add,
  add-batch, insert, remove, complete, list-plans` — none touch freeform `999.x` backlog prose),
  so this was a direct edit, consistent with how this file's other resolved backlog entries
  (999.29, 999.30, etc.) appear to have been closed. — **Reversibility:** reversible — a one-line
  heading edit to revert.

### Claude's Discretion
- **D-01 (plan scope):** The plan should be verification-only — re-run the three checks above
  live against HEAD, and write SUMMARY.md/VERIFICATION.md documenting that they pass. No further
  edits to `.planning/ROADMAP.md` beyond the D-02 backlog-status flip (already made) are planned;
  the layout itself is already correct and Success Criterion 4 ("this phase only inserts a new
  section; it moves nothing") is satisfied by construction since nothing needs inserting.
  — **Reversibility:** reversible — if verification surfaces a real gap, the plan can still add a
  targeted ROADMAP.md edit.
- **D-03 (durability for future milestones):** Not addressed by this phase as of the initial
  opt-out. Options were explained to the user in the same session (see chat transcript / a future
  `32-DISCUSSION-LOG.md` addendum if the user acts on one): (a) a documented convention that
  `/gsd-new-milestone` / `/gsd-complete-milestone` must land the active milestone's own
  `### Phase N:` headings inside its own window before archival, (b) a repo-local check (e.g.
  wired into `scripts/check.sh`) that asserts `roadmap.analyze`'s `phase_count > 0` and a sane
  `current_phase`/`next_phase`, catching a regression at CI time instead of at the next
  `/gsd-new-milestone` run, (c) a gsd-core parser fix — explicitly out of scope per
  REQUIREMENTS.md. Still open pending explicit operator direction; not implemented.

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
  happening to write phases inside the right window each time. Three options were laid out for
  the operator (documented convention / repo-local CI check / gsd-core parser fix — the last one
  out of scope per REQUIREMENTS.md); no option has been chosen yet. Would need its own backlog
  item if pursued.

### Reviewed Todos (not folded)
None — `todo.match-phase 32` returned zero matches.

</deferred>

---

*Phase: 32-ROADMAP Layout Hygiene*
*Context gathered: 2026-08-04*

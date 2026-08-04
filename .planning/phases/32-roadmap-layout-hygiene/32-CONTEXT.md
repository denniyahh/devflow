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
straightforward verify-and-close with no open decisions." In the same session, the user then
reopened both D-02 ("flip the backlog items' status") and D-03 ("option 1 would be good enough
for now") directly — both recorded below as operator decisions, superseding the earlier
Claude's-Discretion defaults. D-01 remains Claude's default from the opt-out, not an
operator-chosen answer.

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
- **D-03 (durability for future milestones) — operator decision:** Option (a), a documented
  convention, chosen over (b) a repo-local CI check and (c) a gsd-core parser fix (out of scope).
  Added as a new "Keep the active milestone's phase headings inside its own window" section in
  `CLAUDE.md` — the project-instructions file already loaded into every session for this repo, so
  it reaches whoever runs `/gsd-new-milestone` / `/gsd-complete-milestone` or hand-edits
  `ROADMAP.md` next, without needing a code change. Covers: the two root causes (interrupting
  closed-milestone heading; reused `### Phase N:` headings), the requirement to keep the
  `## Progress` table, and a spot-check command (`gsd-tools query roadmap.analyze`). Explicitly
  "good enough for now" per the operator — not a CI-enforced guarantee; a regression would still
  only be caught by whoever reads CLAUDE.md at the next milestone boundary, not automatically.
  — **Reversibility:** reversible — a documentation-only addition.

### Claude's Discretion
- **D-01 (plan scope):** The plan should be verification-only — re-run the three checks above
  live against HEAD, and write SUMMARY.md/VERIFICATION.md documenting that they pass. No further
  edits to `.planning/ROADMAP.md` beyond the D-02 backlog-status flip (already made) are planned;
  the layout itself is already correct and Success Criterion 4 ("this phase only inserts a new
  section; it moves nothing") is satisfied by construction since nothing needs inserting.
  — **Reversibility:** reversible — if verification surfaces a real gap, the plan can still add a
  targeted ROADMAP.md edit.

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
  e.g. `### Phase 999.29: ... (DELIVERED — Phase 21 / 21d)`. Per D-02, 999.72/999.72a now follow
  this pattern too (`RESOLVED — 2026-08-04, Phase 32`) — already applied, not something the plan
  needs to do.

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

- **Options (b) and (c) from D-03** — a repo-local CI check (`scripts/check.sh` assertion on
  `roadmap.analyze`'s `phase_count`) and a gsd-core parser fix were both raised and not chosen;
  option (a), a documented convention, was picked as "good enough for now." If the CLAUDE.md
  convention proves insufficient (a future milestone reproduces 999.72 despite it), option (b) is
  the natural escalation — would need its own backlog item.

### Reviewed Todos (not folded)
None — `todo.match-phase 32` returned zero matches.

</deferred>

---

*Phase: 32-ROADMAP Layout Hygiene*
*Context gathered: 2026-08-04*

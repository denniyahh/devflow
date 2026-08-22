# Phase 32: ROADMAP Layout Hygiene - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-04
**Phase:** 32-ROADMAP Layout Hygiene
**Areas discussed:** None selected — user opted out at the gray-area selection gate

---

## Gray Area Selection

Before presenting gray areas, this session re-verified live (not trusted from STATE.md) that
Phase 32's own success criteria were already satisfied by `gsd-roadmapper`'s write when creating
this milestone (commit `0b1ad74`):
- `gsd-tools query roadmap.analyze` → `phase_count: 1`, lists Phase 32 (was `0`)
- `gsd-tools query milestone.complete gsd-hygiene --dry-run` → correctly reports 1 unstarted
  phase instead of the old pass-all degrade
- `.planning/ROADMAP.md` already has a `## Progress` table

Given that, three gray areas were offered instead of the usual implementation-mechanics
questions, since there's no code and the layout is already correct:

| Option | Description | Selected |
|--------|-------------|----------|
| Plan scope given the fix already landed | Verification-and-close only, or is there still something to build? | |
| Closing the 999.72 / 999.72a backlog entries | Mark them DELIVERED like other resolved backlog items, or does REQUIREMENTS.md's backlog-restructuring exclusion rule that out? | |
| Durability for future milestones | Leave a durable note/convention, or explicitly defer as its own backlog item? | |
| None of these — I'm ready for context | Skip discussion; treat as straightforward verify-and-close with no open decisions | ✓ |

**User's choice:** "None of these — I'm ready for context"
**Notes:** The user did not pick among the three sub-questions — they declined to open any of
them. CONTEXT.md records the resulting defaults under "Claude's Discretion," explicitly flagged
as Claude's inference from the opt-out rather than as answers the user gave to each question.

---

## Post-write follow-up (same session)

Immediately after CONTEXT.md was first written, the user reopened two of the three declined gray
areas directly, in plain-text follow-up rather than via AskUserQuestion:

**"flip the backlog items' status"** → D-02 reversed. `.planning/ROADMAP.md`'s `999.72` and
`999.72a` headings changed from `(BACKLOG)` to `(RESOLVED — 2026-08-04, Phase 32)`, with an inline
paragraph noting the resolution came from `gsd-roadmapper`'s milestone-creation write, not a
Phase-32 execute cycle. Committed `09db08e`.

**"what would making the layout fix durable entail?"** → three options presented (documented
convention / repo-local CI check / gsd-core parser fix, the last out of scope). User replied "i
think option 1 would be good enough for now" → D-03 resolved. Added a new section to `CLAUDE.md`
("Keep the active milestone's phase headings inside its own window") documenting the two root
causes, the `## Progress` table requirement, and a spot-check command. Not yet committed at the
time this log entry was written — see the phase's git history for the actual commit.

## Claude's Discretion

- Plan scope: verification-only, no further ROADMAP.md edits beyond the D-02 flip (D-01 in
  CONTEXT.md) — not reopened by the user.

## Deferred Ideas

- Options (b) (repo-local CI check) and (c) (gsd-core parser fix) from the durability discussion
  — not chosen this round; (b) is the natural escalation if the CLAUDE.md convention alone proves
  insufficient at a future milestone boundary.

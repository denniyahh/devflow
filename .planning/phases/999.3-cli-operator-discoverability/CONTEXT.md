---
status: backlog
source: Phase 17 dogfood run (2026-07-18/19), moved from ROADMAP Phase 19 (19c) on 2026-07-20
---

# Backlog: The CLI Assumes a Reader Who Will Parse JSONL

## Goal

- Gate reasons truncate to `[truncated; full output in .devflow/]` with no
  `devflow gate show`.
- Rate-limit reset times exist only inside raw agent JSON.
- `status` reports the stage but nothing about progress inside it.
- Recovery verbs (`advance`, `resume`) are undiscoverable from a stuck
  state.

## Notes

UX/discoverability work, not a correctness bug — safe to sequence behind
the Phase 18 reliability items. `17-REVIEW.md` independently names part of
this gap too (see the `ROADMAP 19c` reference near line 404 of
`17-REVIEW.md`).

Promote with `/gsd-review-backlog` when ready.

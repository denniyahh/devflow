---
status: backlog
source: TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P1 recommendation
---

# Backlog: Separate Fast and Slow Validation Lanes

## Goal

Keep deterministic unit and ordinary integration tests in the fast pull-request
lane. Move nested-build provenance tests (`build_provenance.rs`, which
dominates suite runtime today), mutation testing (999.17), repeated
concurrency stress, and fuzz smoke runs (999.18) into explicit slow or
scheduled lanes. Both lanes stay visible and required at an appropriate
release boundary, rather than the slow lane being invisible/optional.

## Notes

Mostly mechanical CI-workflow restructuring once 999.17/999.18 exist to
actually route into a slow lane — there isn't much to put in a slow lane yet
beyond `build_provenance.rs`. Reasonable to sequence this after (or alongside
the tail end of) 999.17/999.18 rather than doing the workflow split first with
nothing to route into it.

Promote with `/gsd-review-backlog` when ready.

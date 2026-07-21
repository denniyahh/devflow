---
status: backlog
source: TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P2 recommendation
---

# Backlog: Differential Coverage Enforcement

## Goal

Do not optimize for a global coverage percentage alone (currently 92.81%).
Enforce high coverage on changed lines specifically, and require a written
justification when new branches are intentionally left uncovered. Coverage
should support review, not replace behavioral inspection or mutation-testing
results (999.17).

## Notes

Real risk if implemented naively: blocking merges on any uncovered line even
for legitimately-hard-to-test paths (e.g. OS-level failure handling) creates
friction without catching real defects. The "written justification" escape
hatch in the original recommendation is the right shape — keep it, don't drop
it under time pressure when this gets built.

Promote with `/gsd-review-backlog` when ready.

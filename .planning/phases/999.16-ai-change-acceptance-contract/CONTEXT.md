---
status: backlog
source: TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P0 recommendation #2
---

# Backlog: AI Change Acceptance Contract

## Problem

This session's own QA review found tests that only reproduce the production
algorithm, compare a function's output with itself, or exercise entirely
test-invented behavior with no production analog (the `ReviewerSetTestAdapter`
found and removed this session is the concrete example — a fake `AgentAdapter`
whose `preflight()` logic existed only in the test). Nothing currently requires
an AI-generated change to prove its own test actually exercises real behavior.

## Proposed shape

Require every AI-generated behavioral change to include:

1. A regression test that fails before the implementation change.
2. At least one assertion at a public or stable domain boundary.
3. Evidence that the test fails for the intended reason (not just that a test
   exists).
4. Full affected-package tests, clippy with warnings denied, and formatting.
5. Independent review of both implementation and test signal.

Reject tests that only assert constants, reproduce the production algorithm,
compare a function call with itself, or grep implementation text without a
runtime contract.

## Where this should live

Not just prose in `CONTRIBUTING.md` — this project already runs
`/gsd-code-review` before Ship and refuses to ship on Critical findings. The
contract has teeth only if woven into that review's actual criteria, not left
as a checklist nobody consults. Open question for discuss-phase: does this
belong in the `gsd-code-review` prompt, a separate lint pass, or both.

## Notes

Directly motivated by tonight's own QA review — the exact defect classes this
contract targets are the ones that were just found and removed by hand.

Promote with `/gsd-review-backlog` when ready.

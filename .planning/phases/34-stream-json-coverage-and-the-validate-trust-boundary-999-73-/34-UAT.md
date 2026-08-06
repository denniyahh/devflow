---
status: testing
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
source: [34-VERIFICATION.md]
started: 2026-08-06T10:30:00Z
updated: 2026-08-06T10:30:00Z
---

## Current Test

number: 1
name: Layer 0's second call site — `phase_has_blocking_human_checkpoint` under `Action::GateReview` in worktree mode
expected: |
  Drive `advance()` through `Action::GateReview` in worktree mode with a
  blocking-human-checkpoint PLAN placed ONLY inside the worktree (absent from the
  main checkout), a session id set, and the checkpoint reported in the capture.
  The run auto-decides the checkpoint — the plan-28-03 path fires — rather than
  falling through as it did before 34-04.
awaiting: user response

## Tests

### 1. Layer 0's second call site under `Action::GateReview` in worktree mode

expected: The checkpoint is auto-decided (the plan-28-03 path fires), matching the code's stated behaviour.

why_human: No automated test exercises this call site end-to-end. Two halves are
verified separately and independently — that `phase_has_blocking_human_checkpoint`
is root-sensitive (`verify.rs` tests), and that the `Action::GateReview` call site
passes the execution root (source read). The inference from those two halves to
"the path works" is sound but is not a demonstration.

The gap was measured, not assumed, and twice: plan 34-04 self-disclosed it by
reverting its own fix and running the full binary suite (279 passed, 0 failed),
and the verifier independently reproduced that result rather than accepting the
claim — reverting the single argument back to `project_root` and re-running, with
the suite staying green. A regression here would be caught by nothing.

Closing this needs either a live worktree-mode run through `GateReview`, or a new
integration test that drives it end-to-end. The integration test is the more
durable answer, since it converts a standing inference into a guard.

result: [pending]

## Summary

total: 1
passed: 0
issues: 0
pending: 1
skipped: 0
blocked: 0

## Gaps

---
phase: 16-pipeline-reliability-hardening
reviewed: 2026-07-18T11:56:44Z
depth: inline-standard
baseline: af32f1a^
head: 4cd0b0a
files_reviewed: 19
findings:
  critical: 0
  warning: 1
  info: 0
  total: 1
status: issues_found
recommendation: APPROVE_WITH_WARNING
---

# Phase 16: Inline Code Review Report

## Summary

The current Phase 16 implementation and the fixes applied after the previous
review were inspected inline. No open Critical-severity finding remains. The
terminal completion path now fails closed when the feature branch is absent,
stops the ordered finalization batch after any hook failure, preserves the
branch for retry, reopens an actionable Ship gate, and emits
`workflow_finished` only after all required terminal hooks succeed.

The two earlier warning findings are also resolved: gate context rendering
neutralizes all control characters through one bounded helper, and capture
archival rolls back stdout/exit publication when a later archive operation
fails.

## Severity-classified findings

### Warning — WR-03: Parallel CLI integration test has a check-then-use race

**File:** `crates/devflow-cli/tests/phase7_cli.rs:184-200`

`parallel_creates_two_worktrees_and_spawns_two_monitors` waits until each live
stdout capture exists, performs unrelated state assertions, and then asserts
that the same capture paths still exist. A fast monitor can archive a capture
between the wait and the final assertion. This produced one failure in the
first full-suite run; the isolated rerun and a second full-suite run passed.

This is a test-harness reliability issue, not evidence of a product false
success. A future cleanup should either read/assert the capture immediately,
accept the corresponding retained-history generation, or wait for a stable
workflow state rather than rechecking a transient live path.

## Prior finding verification

- CR-01 is closed by `83602c7`: absent feature branches are rejected as
  unproven merges; terminal hook batches stop on the first failure; regression
  tests cover the missing-branch and pre-cleanup failure paths.
- WR-01 is closed by `5fcaaa5`: `render_gate_context` bounds output and maps
  every control character to a safe space for both status and gate-list paths.
- WR-02 is closed by `8db68bb`: archival uses a pending generation and restores
  the complete live stdout/exit pair after second-publish or review-copy
  failures.

## Validation

- `cargo test --workspace --all-targets`: 313 passed on the final full rerun.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check af32f1a^..HEAD`: passed.

## Gate decision

Open Critical findings: **0**. Phase 16 is approved to proceed to the Ship
workflow with WR-03 recorded as non-blocking follow-up debt.

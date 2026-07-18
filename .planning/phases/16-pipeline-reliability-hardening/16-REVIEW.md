---
phase: 16-pipeline-reliability-hardening
reviewed: 2026-07-18T02:22:08Z
depth: standard
files_reviewed: 19
files_reviewed_list:
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/history.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/lock.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/verify.rs
  - crates/devflow-core/src/workflow.rs
  - docs/guides/adding-agent.md
  - docs/guides/configuration.md
  - docs/guides/quickstart.md
findings:
  critical: 1
  warning: 2
  info: 0
  total: 3
status: issues_found
---

# Phase 16: Code Review Report

**Reviewed:** 2026-07-18T02:22:08Z
**Depth:** standard
**Files Reviewed:** 19
**Status:** issues_found

## Summary

The Phase 16 reliability paths were reviewed file by file, including terminal
Ship finalization, external verification, capture/history retention, gate
rendering, configuration, tests, and operator documentation. The implementation
still has one Critical terminal-truth defect: deleting or otherwise losing the
feature branch is treated as proof that it was merged, so DevFlow can tag and
report a successful shipment without establishing that the phase work reached
`develop`.

Validation completed successfully with `cargo test --workspace --all-targets`
(309 tests), `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all -- --check`, and `git diff --check`. These gates do not cover
the missing-branch false-success case described below.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: A missing feature branch is accepted as already merged

**File:** `crates/devflow-core/src/git.rs:89-105`
**Issue:** `is_merged_into_develop` returns `true` whenever the phase branch is
absent. `merge_feature` then records a successful no-op, and
`finish_workflow` is free to run version/tag bookkeeping, clear state, and emit
`workflow_finished`. Branch absence does not prove ancestry: the branch may
have been deleted before its commits reached `develop`, or may never have been
created. The existing `merge_is_fail_soft_when_branch_absent` test explicitly
locks in this unsafe behavior. This violates the terminal-truth contract and
can report a shipped phase after its implementation commits have become
unreachable (data-loss/false-success risk).
**Fix:** Make branch absence fail closed unless DevFlow has durable evidence of
an earlier successful merge. Persist the feature tip/merge commit before any
cleanup, verify that commit is an ancestor of `develop` on retries, and only
then allow an absent branch to count as merged. Also stop the terminal hook
batch before branch cleanup after any preceding hook failure. For example:

```rust
let feature_tip = persisted_feature_tip(state)?;
if !git.is_ancestor(&feature_tip, DEVELOP)? {
    return Err(HookError::Git(GitError::Command(
        "phase feature tip is not present on develop".into(),
    )));
}
```

The relevant caller is `crates/devflow-core/src/hooks.rs:120-142`, and the
terminal completion path is `crates/devflow-cli/src/main.rs:1174-1207`.

## Warnings

### WR-01: `gate list` prints agent-controlled terminal control sequences

**File:** `crates/devflow-cli/src/main.rs:1985-1996`
**Issue:** `gate_list` replaces newlines and truncates length, but leaves other
control characters intact. Gate context originates from agent-controlled
failure text, so ANSI sequences can clear the terminal, reposition the cursor,
or spoof surrounding operator output. `render_pending_gate_banner` correctly
sanitizes controls at lines 1955-1964, making this an inconsistent second
rendering path.
**Fix:** Centralize bounded gate-context rendering and map every control
character to a safe visible separator before both `status` and `gate list`
print it.

### WR-02: Capture archival can leave a partially moved generation on error

**File:** `crates/devflow-core/src/agent_result.rs:812-824`
**Issue:** The live stdout is renamed before the exit capture and REVIEW copy
are attempted. If a later operation fails, the function returns an error after
already removing stdout from its live path, leaving evidence split across a
partial history generation and the live paths. A retry creates a new stamp, so
the files can remain permanently mis-correlated. The current failure test only
covers failure before the first rename.
**Fix:** Stage all files under a temporary generation, then atomically publish
the generation, or roll back every completed move when a later operation
fails. Add failure-injection tests for the second rename and REVIEW copy.

---

_Reviewed: 2026-07-18T02:22:08Z_
_Reviewer: the agent (gsd-code-reviewer; generic-agent workaround)_
_Depth: standard_

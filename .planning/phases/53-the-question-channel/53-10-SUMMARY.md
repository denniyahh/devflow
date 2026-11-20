---
phase: 53-the-question-channel
plan: 10
subsystem: gates, state, preflight, routers
tags: [cleanup, semver-major, checkpoint, gates]
status: complete
requires:
  - phase: 53-03
    provides: the checkpoint relaunch removed, leaving the re-scan machinery orphaned
  - phase: 53-09
    provides: the question gate's abort and backstop, the last change to the routers before this cut
provides:
  - routers, preflight and State without the checkpoint-declaration family, the recorded checkpoint set or the resume counter
affects: [53-11, 54]
tech-stack:
  added: []
  patterns: []
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs
    - OPERATIONS.md
key-decisions:
  - "verify::phase_checkpoint_declarations and verify::CheckpointDeclaration are kept: the action deletes them only if nothing else uses them, and verify::phase_has_human_only_checkpoint (the unattended-launch refusal at preflight.rs:1010) still reads them. verify.rs is therefore untouched."
  - "stage_launch keeps its pre-spawn state commit as commit(state, |_| {}): it used to carry the checkpoint_resumes reset, and it also re-reads the state under the lock before the launch is returned, so dropping it would change which state the launch carries."
requirements-completed: [ASK-05]
duration: about 12 min
completed: 2026-11-19
commits: 1
plan_head_before: d0270f0deaf77e1e2fd3c23518365753f62156c5
plan_head_after: 8308607b29a674e3dfb506157029c14eb170c649
actuals:
  tokens: 21553
  tasks: 1
  commits: 1
---

# Phase 53 Plan 10: Remove the checkpoint re-scan machinery Summary

The `checkpoint_declaration` gate kind, its router family and status-less step, the Code-preflight checkpoint-set recording, `State::checkpoint_approval`/`checkpoint_resumes` and `mode::MAX_CHECKPOINT_RESUMES` are deleted from both crates and OPERATIONS.md in one commit that builds (1376 lines removed).

## Task

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Remove the checkpoint re-scan machinery from both crates and the docs, in one compile unit | 8308607 | 9 files (verify.rs untouched, see decisions) |

Removed:
- core: `GateKind::CheckpointDeclaration`, `State::checkpoint_approval`, `CheckpointApproval` (with `unapproved`), `ApprovedCheckpoint`, `State::checkpoint_resumes`, `mode::MAX_CHECKPOINT_RESUMES` and its const assertion, their tests and the intra-doc links that named them.
- CLI: `AnswerFamily::CheckpointDeclaration`, `StatusLessStep::RecordCheckpointSet` and every arm naming them (status-less step table, approve/reject routers, response screen, way-on sentence, ceiling clause, `listed` match, parity and prop tests, commands.rs route tables); preflight's `approval_for_code_evaluation`/`fill_pending_approval` and both call sites with their nine recording tests; pipeline_launch's `recorded_approval`, the `checkpoint_resumes` reset and `launch_stage_inner_resets_checkpoint_resumes_counter`; the `checkpoint_approval` block of `resume_with_agent_preserves_every_state_field_except_agent` (the test keeps its whole-state comparison); the `checkpoint_resumes` lines of `phase_validate_failures_survive_a_forced_restart`.
- docs: the `checkpoint_declaration` row of the status-less gate table and its mention in the gate_kind list.

`rescan_gate_context` named in the plan no longer existed at this plan's start, so there was nothing to remove.
`GateKind::Question` arms and `AgentResult::declares_blocking_human_checkpoint` are untouched.

## Verification

- `cargo clippy --workspace --all-targets -- -D warnings`: clean (run before the commit and again as the commit's gate).
- The four named nextest gates in `<verify>`: each `1 passed`, chained with `&&`, exit 0.
- `scripts/check.sh all`: exit 0, `all OK`; core lib 930 passed, CLI bin 509 passed, no failures. The known flaky monitor test did not fail.
- `cargo doc -p devflow-core`: no unresolved link in a touched file (the one unresolved link is the existing `Display` in phase_id.rs).

### semver-checks (baseline = this plan's start, d0270f0)

`cargo semver-checks check-release -p devflow-core --baseline-rev d0270f0 --release-type minor`: 202 checks, 196 pass, 6 fail, exit 100.

| Lint | Item |
|------|------|
| enum_missing | `state::CheckpointApproval` |
| struct_missing | `state::ApprovedCheckpoint` |
| enum_variant_missing | `GateKind::CheckpointDeclaration` |
| pub_module_level_const_missing | `mode::MAX_CHECKPOINT_RESUMES` |
| struct_pub_field_missing | `State::checkpoint_resumes`, `State::checkpoint_approval` |
| enum_no_repr_variant_discriminant_changed | `GateKind::Question` 14 -> 13 |

The first five are exactly the planned removals. The sixth is not on the must_haves list: removing a variant ahead of `Question` in a non-`repr` enum shifts its implicit discriminant. It breaks only an `as isize` cast; gate files serialize `GateKind` by name (`rename_all = "snake_case"`), so nothing persisted changes. It is a direct consequence of the planned variant removal and not a defect in behaviour, but it is recorded here as the plan asks.

## Deviations from Plan

**1. [Scope] verify.rs items kept**
- **Found during:** Task 1
- **Issue:** must_haves lists `verify::phase_checkpoint_declarations` and `verify::CheckpointDeclaration` as deleted, but the action deletes them only "if nothing else uses them"; `verify::phase_has_human_only_checkpoint` (used by the unattended-launch check) is built on them.
- **Fix:** kept both, with their tests; verify.rs is not in the commit.
- **Commit:** 8308607

Otherwise the plan executed as written.

## Self-Check: PASSED

- 53-10-SUMMARY.md written; commit 8308607 is an ancestor of HEAD; the commit deletes no file.

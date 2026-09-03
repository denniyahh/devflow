---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
plan: 02
subsystem: staleness detection
tags: [rust, git, cargo, staleness, dogfood]
requires:
  - phase: 45-01
    provides: configurable phase-worktree base branch
provides:
  - Self-dogfood build-input classification scoped to Cargo workspace members
  - Direct predicate and porcelain-normalization tests with negative controls
affects: [phase-45-03, dogfood-staleness, unattended-auto-mode]
actuals:
  tokens: 10017
  tasks: 2
  commits: 3
tech-stack:
  added: []
  patterns:
    - Parameterize the shared staleness predicate by workspace scope rather than fork it.
    - Pair ignored-path coverage with an in-fixture build-source negative control.
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/staleness.rs
key-decisions:
  - "Accepted the plan's workspace_scoped design: narrow only DevFlow's own workspace and retain the broad rule for downstream projects."
  - "Keep porcelain path normalization unchanged; document why a leading ./ is deliberately not stripped."
patterns-established:
  - "Self-dogfood source fixtures use crates/devflow-core or crates/devflow-cli, never root src/."
requirements-completed: [AUTO-02]
coverage:
  - id: D1
    description: Spikes-only tracked changes no longer hard-block DevFlow's self-dogfood workspace while a member-source change still does.
    requirement: AUTO-02
    verification:
      - kind: integration
        ref: crates/devflow-cli/src/staleness.rs#spikes_only_dirty_tree_does_not_block_self_dogfood
        status: pass
    human_judgment: false
  - id: D2
    description: Scoped and unscoped path classification preserves root build files, member paths, prefix boundaries, and downstream-project behavior.
    requirement: AUTO-02
    verification:
      - kind: unit
        ref: crates/devflow-cli/src/staleness.rs#affects_compiled_binary_in_workspace_scope_accepts_only_members_and_root_build_files
        status: pass
      - kind: unit
        ref: crates/devflow-cli/src/staleness.rs#affects_compiled_binary_unscoped_preserves_the_pre_phase_45_rule
        status: pass
    human_judgment: false
duration: 32min
completed: 2026-09-02
status: complete
---

# Phase 45 Plan 02: Dogfood Staleness Hardening Summary

**DevFlow now ignores non-workspace spikes for its self-dogfood stale-build block while retaining true-positive member-source and downstream-project detection.**

## Performance

- **Duration:** 32 min including recovery and verification
- **Started:** 2026-09-02T08:55:56Z
- **Completed:** 2026-09-02T09:27:32Z
- **Tasks:** 2/2
- **Files modified:** 1

## Accomplishments

- Threaded `workspace_scoped` from `is_self_dogfood_workspace(project_root)` through both staleness inputs to one parameterized predicate.
- Scoped DevFlow's own workspace to `crates/` members plus exact root build files, rejecting parent-directory segments and substring lookalikes.
- Preserved the pre-Phase-45 broad rule for all non-dogfood projects and added direct tests for predicate and porcelain normalization behavior.

## TDD Evidence

Retained Claude's RED commit `d67e05f` and independently reran its tests from that commit in an isolated temporary source tree. Both failed before the implementation:

- `spikes_only_dirty_tree_does_not_block_self_dogfood` returned the expected pre-fix `Err(self-dogfood stale build blocked ...)` where the test required `Ok`.
- `red_probe_scoped_rule_rejects_root_src_main_rs` reported `left: Some(true)` and `right: Some(false)`.

The GREEN implementation and direct test commits are listed below. This confirms the two expected failure modes; it does not prove every possible Cargo workspace layout beyond the explicitly tested member/root-path cases.

## Task Commits

1. **Task 1: End-to-end — a spikes-only change no longer blocks the dogfood workspace**
   - `d67e05f` — RED tests retained from the interrupted Claude session
   - `172540a` — GREEN scoped predicate, threaded call chain, fixtures, and end-to-end tests
2. **Task 2: Direct unit tests for the predicate and porcelain normalizer**
   - `d509f4f` — direct workspace/unscoped predicate and porcelain tests

## Verification

- `cargo test -p devflow --bin devflow spikes_only_dirty_tree_does_not_block_self_dogfood`: 1 passed.
- `cargo test -p devflow --bin devflow non_dogfood_project_keeps_the_broad_build_input_rule`: 1 passed.
- `cargo test -p devflow --bin devflow mixed_range_docs_and_source_is_stale`: 1 passed, with its assertions unchanged.
- `cargo test -p devflow --bin devflow staleness::`: 48 passed.
- `cargo test -p devflow --bin devflow affects_compiled_binary`: 2 passed.
- `cargo test -p devflow --bin devflow porcelain_tracked_path_normalizes_status_bytes_renames_and_quotes`: 1 passed.
- `cargo test --workspace -q`: exit 0; all reported suites passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0; `^error` count 0.
- `cargo fmt --all -- --check`: exit 0.

The workspace suite establishes the tested repository state, and clippy/fmt establish lint/format status; neither establishes runtime behavior against arbitrary repositories or concurrent worktree mutation.

## Decisions Made

- Accepted the planned `workspace_scoped` boolean. It prevents DevFlow's `.planning/spikes/` false positive without weakening stale-build warnings for ordinary Rust projects.
- Kept `porcelain_tracked_path` behavior unchanged and documented that neither real producer emits a leading `./` path prefix.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Fixture correctness] Updated self-dogfood fixtures to use real workspace member paths**

- **Found during:** Task 1 staleness-suite verification
- **Issue:** Two existing self-dogfood fixtures wrote source under root `src/`; the new scoped rule correctly excludes that non-member path, causing assertions that expected a hard block to fail.
- **Fix:** Changed those fixture writes to `crates/devflow-core/src/lib.rs`, preserving their original staleness behavior while accurately modeling DevFlow's workspace.
- **Files modified:** `crates/devflow-cli/src/staleness.rs`
- **Verification:** `cargo test -p devflow --bin devflow staleness::` reported 48 passed.
- **Committed in:** `172540a`

**Total deviations:** 1 auto-fixed (Rule 1)

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

AUTO-02 is complete and isolated on this worktree branch. Graphify payloads were intentionally not refreshed here because the recovery assignment explicitly prohibited altering them; that leaves graph freshness unverified for this commit only.

## Self-Check: PASSED

- Confirmed the modified source file and this summary exist.
- Confirmed the retained RED commit `d67e05f` and both GREEN commits `172540a` and `d509f4f` exist in Git history.

*Phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94*
*Completed: 2026-09-02*

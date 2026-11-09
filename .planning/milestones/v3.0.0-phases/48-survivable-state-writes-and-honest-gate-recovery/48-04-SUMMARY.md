---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 04
subsystem: testing
tags: [rust, cargo-test, child-process, path-isolation, pipeline-outcomes]
requires:
  - phase: 48-03
    provides: Exact-name child test helper with command-local PATH and vacuity guard
provides:
  - Pipeline-outcome tests execute former PATH replacements in child processes
  - NeutralPath removal with no remaining non-comment code callers
affects: [48-08, 48-09, TEST-01]
tech-stack:
  added: []
  patterns: [directory-only child PATH, guarded exact-test child run]
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/test_support.rs
key-decisions:
  - "Use devflow_core::test_support::run_test_in_child with each test's module-qualified name."
  - "Use separate child invocations for the empty-PATH and git-only-PATH cycles in the unmeasurable-count regression."
plan_head_before: de6cd13c629c89a7ee28d05663b66c1529a53f3b
commits: 1
actuals:
  tokens: 12169
  tasks: 2
  commits: 1
requirements-completed: []
coverage:
  - id: D1
    description: Pipeline outcome tests use an agent-free child PATH and exact-test guard instead of process-global PATH mutation.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow --bin devflow pipeline_outcomes::tests::
        status: pass
      - kind: other
        ref: Plan 48-04 structural and clippy gate
        status: pass
    human_judgment: false
duration: 10m
completed: 2026-09-16
status: complete
---

# Phase 48 Plan 04: Pipeline Outcome PATH Isolation Summary

**All pipeline-outcome PATH fixtures now run under a directory-only child process with exact-test vacuity guards.**

## Performance

- **Duration:** 10m
- **Started:** 2026-09-16T12:06:55Z
- **Completed:** 2026-09-16T12:16:14Z
- **Tasks:** 2/2 verified; Git permissions prevented per-task commits, so the orchestrator records the verified plan scope in one commit.
- **Files modified:** 2

## TEST-01 Baseline

Before the first source edit, `cargo test -p devflow --bin devflow pipeline_outcomes::tests::` passed **68 tests**. The converted tracer test, `pipeline_outcomes::tests::external_verify_agreement_advances_to_ship`, passed under `--exact` with `1 passed` and `363 filtered out`.

## Accomplishments

- Converted every direct PATH replacement and all ten `NeutralPath::install()` callers in `pipeline_outcomes.rs` to guarded child runs using `devflow_core::test_support::run_test_in_child`.
- Preserved the two distinct PATH cases in `validate_failure_with_unmeasurable_count_accumulates_the_streak`: its empty-PATH probe remains a child, and its git-only continuation now uses a separate child with a persisted fixture root.
- Deleted `NeutralPath`'s struct, inherent implementation, and `Drop` implementation from `test_support.rs`.

## Verification

- Final plan gate: `path_mutation_lines=0`, `neutral_path_uses=0`, `neutral_path_struct_defs=0`, `neutral_path_code_calls=0`, `base_tests=72`, `now_tests=72`, `child_runs=27`, `child_guards=32`, `cr01_one_passed=1`, `cr01_filtered=363 filtered out`, `module_exit=0`, `clippy_exit=0`, `gate_rc=0`.
- `cargo test -p devflow --bin devflow pipeline_outcomes::tests::` passed 68/68 tests.
- `cargo clippy -p devflow --all-targets -- -D warnings` passed.

These checks establish the converted module's current child-PATH behavior and the specified structural properties. They do not establish that the broader test-suite PATH race cannot recur under repeated concurrent full-suite runs.

## Commit Handling

Git could not create `.git/worktrees/phase-48/index.lock`: `Read-only file system`. The executor could not create its two task commits. The orchestrator validated the completed scope and records both verified tasks plus this SUMMARY in one plan commit; the lost per-task atomicity is an infrastructure deviation, not evidence that either task was skipped.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added `sh` only to the child PATH needed by the ceiling-clause fixture.**
- **Found during:** Task 2 verification
- **Issue:** The isolated fixture invoked a shell path and failed with `NotFound` when its child PATH contained only `git`.
- **Fix:** Linked the parent-resolved `sh` binary into that test's temporary child PATH; no parent PATH entries are inherited.
- **Files modified:** `crates/devflow-cli/src/pipeline_outcomes.rs`
- **Verification:** Module test suite and final plan gate passed.

**Total deviations:** 1 auto-fixed bug. The added `sh` symlink is required for the existing test body while retaining an agent-free PATH.

## Known Stubs

None found in the plan-modified source files.

## TDD Gate Compliance

The plan is `type: execute` with `tracer` and `auto` tasks; `task.is-behavior-adding` returned `false`, so the runtime RED/GREEN gate did not apply.

## Self-Check: PASSED

- Source files exist and `git diff --check` passed.
- The orchestrator independently reran the 68-test module, the exact CR-01 tracer, warning-deny Clippy, and the zero-mutation/zero-NeutralPath structural controls.

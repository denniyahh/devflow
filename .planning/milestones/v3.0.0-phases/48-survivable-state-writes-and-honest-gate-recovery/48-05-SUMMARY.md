---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 05
subsystem: testing
tags: [rust, cargo-test, child-process, path-isolation, pipeline-launch]
requires:
  - phase: 48-03
    provides: Exact-name child-test helper with command-local PATH
provides:
  - PATH-isolated pipeline-launch test fixtures
  - Child guard accepting legitimate child test output
affects: [48-06, 48-08, TEST-01]
tech-stack:
  added: []
  patterns: [parent-child hermetic fixture, explicit abort-fixture child marker]
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-core/src/test_support.rs
key-decisions:
  - "Operator approved an orchestrator commit on feature/phase-48 with normal hooks because upstream GSD bug #4799 incorrectly applies the agent-worktree branch guard to sequential execution."
  - "Accept libtest output interleaved between a requested test prefix and its final ok only when the one-test success summary also exists."
plan_head_before: 55446e854bfe532ae2eb7a7c993cd4db84fd2736
commits: 1
actuals:
  tokens: 9211.5
  tasks: 2
  commits: 1
requirements-completed: []
coverage:
  - id: D1
    description: Pending commit: pipeline-launch tests use child-local PATH and structurally mark abort fixtures.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow --bin devflow pipeline_launch::tests::
        status: pass
      - kind: other
        ref: 48-05 automated zero/count and exact-contract gate
        status: pass
    human_judgment: false
  - id: D2
    description: Pending commit: child-test guard accepts normal child output without allowing a vacuous test filter.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow-core --lib test_support::tests::child_guard_accepts_output_emitted_by_the_passing_child -- --exact
        status: pass
      - kind: unit
        ref: cargo test -p devflow-core --lib test_support::tests::child_guard_rejects_a_name_that_matches_no_test -- --exact
        status: pass
    human_judgment: false
duration: "18m resumed execution only"
completed: 2026-09-16
status: complete
---

# Phase 48 Plan 05: Pipeline Launch PATH Isolation Summary

**Pipeline-launch PATH fixtures and the child-output guard repair are verified and recorded through an approved normal-hook commit on the intended phase branch.**

## Commit-Gate Deviation

The executor's mandatory pre-stage guard ran at `55446e8`. It confirmed the branch is not protected, then refused to commit because this linked worktree is on `feature/phase-48`, not an allowed `agent-*`, `worktree-agent-*`, or `worktree-wf_*` branch. The operator approved an orchestrator-managed commit of only the independently verified two-file scope with normal hooks. This is the temporary workaround for upstream GSD bug #4799; it does not change the guard or bypass hooks.

## Verified Work

- Converted every former process-global `PATH` mutation in `pipeline_launch.rs` to a child re-execution with an agent-free fixture directory; the static gate reports `path_mutation_lines=0`.
- Made the two direct abort-note fixtures explicitly assert child mode; the shared abort-response helper asserts it too. The fixture gate reports `fixture_fns_without_child=0`.
- Preserved all pipeline-launch tests: the c0fb193 baseline and current source both contain 54 `#[test]` declarations.
- Preserved the tracer's parent-built repository fixture and extra child environment; its exact test passed with one test and 363 filtered out.
- Repaired the shared child guard so legitimate child output between `test <name> ...` and libtest's final `ok` does not cause a false failure. The guard still requires a successful process, the requested test prefix, the one-test success summary, and a non-zero filtered count.

## Verification

- Formal 48-05 gate: `gate_rc=0`; it reported zero PATH mutations, zero unwrapped abort fixtures, `child_runs=12`, `child_guards=13`, and all four exact C-8 contract tests with one pass and 363 filtered out.
- Module test: `cargo test -p devflow --bin devflow pipeline_launch::tests::` passed `54 passed; 0 failed`.
- Child-output regression: the output-emitting child test passed, and the existing misspelled-name negative control passed by observing its required panic.
- Quality: `cargo clippy --workspace --all-targets -- -D warnings` exited 0; `cargo fmt` and `git diff --check` passed.

These checks prove the listed test paths and static controls at this working-tree state. They do not establish repeated concurrent-suite reliability or a full workspace test run.

## Task Boundaries

1. **Task 1: Tracer — checkpoint relaunch child fixture** — implementation and tracer verification complete.
2. **Task 2: Remaining PATH and abort-fixture migration** — implementation and final verification complete.

## Files Modified

- `crates/devflow-cli/src/pipeline_launch.rs` — child-local PATH wrappers, abort-fixture markers, and removal of process-global PATH replacement.
- `crates/devflow-core/src/test_support.rs` — robust child-result parsing plus an output-interleaving regression control.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Child-result parsing rejected passing tests that write output.**
- **Found during:** Task 2 module verification.
- **Issue:** Eight converted tests emitted legitimate launch diagnostics before libtest printed `ok`, so the guard's contiguous `test <name> ... ok` match failed even though each child passed exactly one test.
- **Fix:** Require the requested test prefix, a final one-test success summary, and either contiguous or output-interleaved completion; added a child-output regression test while retaining the misspelled-name negative control.
- **Files modified:** `crates/devflow-core/src/test_support.rs`.
- **Verification:** Output-emitting child control, misspelled-name control, full `pipeline_launch::tests::` module, and workspace clippy all passed.
- **Committed in:** One operator-approved orchestrator-managed normal-hook commit on `feature/phase-48`.

**Total deviations:** 1 Rule 1 bug fix. It is directly required for the migrated tests to use the existing non-vacuous child guard.

## Known Stubs

None found in the two changed source files.

## Next Phase Readiness

The verified commit scope is limited to the two files above. The branch-policy mismatch remains tracked as upstream GSD bug #4799; later sequential executor commits may require the same orchestrator workaround until it is fixed.

## Self-Check: PASSED

`48-05-SUMMARY.md`, `pipeline_launch.rs`, and `test_support.rs` exist; `git diff --check` passed; the focused module, both child-guard controls, and warning-deny Clippy passed; and the verified scope is committed with normal hooks.

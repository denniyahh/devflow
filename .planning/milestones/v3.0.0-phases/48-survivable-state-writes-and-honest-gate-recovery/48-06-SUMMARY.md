---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 06
subsystem: testing
tags: [rust, cargo-test, child-process, path-isolation, preflight, pipeline-gate]
requires:
  - phase: 48-03
    provides: Exact-name child-test helper with command-local PATH
  - phase: 48-05
    provides: Child-output guard and prior pipeline-launch helper migration
provides:
  - PATH-isolated preflight and pipeline-gate unit fixtures
  - Child-marked abort fixtures that cannot resolve a host agent CLI
  - Removal of obsolete PATH-prepend helpers
affects: [48-08, 48-09, TEST-01]
tech-stack:
  added: []
  patterns: [module-local parent-child PATH fixture, explicit abort-fixture child marker]
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/staleness.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/test_support.rs
key-decisions:
  - "Use directory-only child PATH fixtures; add a stub only to tests that must launch an agent."
  - "Remove both obsolete helpers after source-wide caller scans, rather than leaving warning-denied test-only dead code."
patterns-established:
  - "Every migrated abort fixture asserts CHILD_TEST_ENV in its own test body so the structural fixture gate is non-vacuous."
requirements-completed: []
plan_head_before: ff90e8587f18ff125bddd02a7b1806ee1a72433b
commits: 2
actuals:
  tokens: 8112
  tasks: 3
  commits: 2
coverage:
  - id: D1
    description: Preflight and pipeline-gate PATH fixtures execute under exact-name child processes without parent-process PATH mutation.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow --bin devflow preflight::tests:: and pipeline_gate::tests::
        status: pass
      - kind: other
        ref: 48-06 Task 2 and Task 3 static PATH/fixture gates
        status: pass
    human_judgment: false
  - id: D2
    description: The observed staleness flake victim remains isolated and passing.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow --bin devflow staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking -- --exact
        status: pass
      - kind: unit
        ref: cargo test -p devflow --bin devflow staleness::tests::
        status: pass
    human_judgment: false
duration: "resumed execution; start timestamp was not instrumented"
completed: 2026-09-16
status: complete
---

# Phase 48 Plan 06: Preflight and Pipeline-Gate PATH Isolation Summary

**All TEST-01 fixtures now use child-local PATHs, abort fixtures prove child mode, and obsolete PATH-prepend helpers are removed.**

## Commit Status

Task 1 is committed as `7a98c0a`. Per the explicit continuation handoff, the verified Task 2–3 source changes and this summary are intentionally uncommitted and unstaged for the orchestrator's independent normal-hook validation and commit. The measured commit count above is therefore one (from `ff90e85` to current `HEAD`), not a claim that the remaining diff is committed.

## Accomplishments

- Converted all seven preflight and four pipeline-gate parent-process PATH replacements to exact-name child-process fixtures.
- Marked all three preflight and both pipeline-gate abort fixtures with a child-mode assertion; PATH has no real host agent entries.
- Removed `prepend_path` and the now-unreferenced `stub_agent_binary` helper after a source-wide caller scan.

## Verification

- Task 2 gate: `path_mutation_lines=0`, `fixture_fns_without_child=0`, `base_tests=61`, `now_tests=61`, exact C-11 test passed once, and the preflight module passed 61 tests.
- Task 3 gate: `path_mutation_lines=0`, `fixture_fns_without_child=0`, `base_tests=17`, `now_tests=17`, `prepend_path_defs=0`, `prepend_path_code_refs=0`, pipeline-gate module passed 17 tests, and package Clippy passed with warnings denied.
- Current-head checks: the staleness flake-victim exact test passed once with 363 filtered tests; the staleness, preflight, and pipeline-gate modules passed 30, 61, and 17 tests respectively; workspace `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and `git diff --check` passed.

These checks establish the current targeted behavior and warning-free compilation. They do not establish repeated concurrent-suite reliability; one full-suite run is not a statistical reliability bound.

## Verification Proxy Correction

The original Task 1 proxy counted every successful libtest summary. A child-wrapped module correctly emits an exact child summary and an enclosing module summary, so the proxy was corrected in the PLAN to validate the terminal enclosing summary while retaining the exact child test and non-vacuity checks. The corrected gate prints `gate_rc=0`.

## Task Commits

1. **Task 1: Tracer — convert staleness.rs PATH block** — `7a98c0a` (`test`)
2. **Task 2: Convert preflight.rs PATH blocks and abort fixtures** — one operator-approved orchestrator normal-hook commit.
3. **Task 3: Convert pipeline_gate.rs PATH blocks and abort fixtures; delete unused helpers** — included in the same operator-approved normal-hook commit.

## Files Modified

- `crates/devflow-cli/src/staleness.rs` — committed tracer child fixture from Task 1.
- `crates/devflow-cli/src/preflight.rs` — module-local child-entry macro and PATH/abort fixture migrations.
- `crates/devflow-cli/src/pipeline_gate.rs` — module-local child-entry macro and PATH/abort fixture migrations.
- `crates/devflow-cli/src/test_support.rs` — deleted `prepend_path` and `stub_agent_binary` after their final callers disappeared.

## Decisions Made

- Kept all fixture setup inside the child unless a test needs a parent-built shared root; the child receives only a directory-local PATH.
- Used `agent_free_git_only_path_dir` for non-launch tests and `agent_free_dir_with_agent_stub` only where a tested relaunch requires the fake agent and `sh`.

## Deviations from Plan

The Task 1 module-summary verification proxy became over-strict after child-process fixtures produced a legitimate nested exact-test summary. The PLAN now selects the terminal enclosing summary, preserving the exact child assertion and making the gate discriminating rather than false-negative.

## Known Stubs

None in the plan-modified fixture code.

## Threat Flags

None. The change narrows test-process access to agent binaries and adds no production trust boundary.

## TDD Gate Compliance

This is an `execute` plan with test-fixture migrations, not a `type: tdd` plan; the runtime RED/GREEN commit gate does not apply. The known-positive static controls (21 preflight and 12 pipeline PATH-mutation lines; three and two unmarked abort fixtures respectively) were checked before migration, then re-run at zero after it.

## Next Phase Readiness

The remaining TEST-01 fixture conversions can reuse the module-local child-entry pattern. TEST-01 remains pending because later Plans 48-08 and 48-09 still own its remaining work.

## Self-Check: PASSED

Verified this summary and all four plan files exist, `7a98c0a` is reachable, and the remaining verified scope passes the corrected Task 1 proxy, affected module tests, formatting, and workspace Clippy.

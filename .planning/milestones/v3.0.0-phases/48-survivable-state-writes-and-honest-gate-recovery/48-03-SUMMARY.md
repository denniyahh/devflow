---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 03
subsystem: testing
tags: [rust, cargo-test, child-process, path-isolation, pi, opencode]
requires:
  - phase: 48-01
    provides: Phase 48 execution baseline
provides:
  - Exact-name child test re-execution with command-local PATH and extra environment
  - Pi and OpenCode driver tests free of process-global PATH mutation
affects: [48-04, 48-05, 48-06, 48-08, TEST-01]
tech-stack:
  added: []
  patterns: [parent-child hermetic test fixture, exact libtest child guard]
key-files:
  created: []
  modified:
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-core/src/agents/pi.rs
    - crates/devflow-core/src/agents/opencode.rs
key-decisions:
  - "Child marker value is the exact module-qualified test name, and the guard requires a non-vacuous exact-test result."
  - "Driver fixtures supply PATH and configuration only through the spawned child Command."
plan_head_before: 3aa5e5dba1871b09ece43784b3fd68603cf010a1
commits: 2
actuals:
  tokens: 5948
  tasks: 2
  commits: 2
requirements-completed: [TEST-01]
coverage:
  - id: D1
    description: Hermetic child test helper rejects vacuous exact-name child runs and isolates PATH.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow-core --lib test_support::tests::child_with_an_empty_path_dir_cannot_spawn_git_while_the_parent_can -- --exact
        status: pass
      - kind: unit
        ref: cargo test -p devflow-core --lib test_support::tests::child_guard_rejects_a_name_that_matches_no_test -- --exact
        status: pass
    human_judgment: false
  - id: D2
    description: Pi and OpenCode driver fixtures run through guarded child processes without process-global PATH mutation.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: cargo test -p devflow-core --lib agents::pi::tests::
        status: pass
      - kind: unit
        ref: cargo test -p devflow-core --lib agents::opencode::tests::
        status: pass
      - kind: other
        ref: plan 48-03 automated zero/count gates and cargo clippy -p devflow-core --all-targets -- -D warnings
        status: pass
    human_judgment: false
duration: 12m
completed: 2026-09-16
status: complete
---

# Phase 48 Plan 03: Hermetic Core Driver Tests Summary

**Child-process test support now gives Pi and OpenCode fixtures an exact-test, command-local PATH instead of racing the process-global environment.**

## Performance

- **Duration:** 12m (from baseline-log creation to summary preparation)
- **Started:** 2026-09-16T07:38:04-04:00
- **Completed:** 2026-09-16T07:49:27-04:00
- **Tasks:** 2/2
- **Files modified:** 3

## TEST-01 baseline

Before the first source edit, `/usr/bin/time -p scripts/check.sh test` completed with `real 92.35`, `user 146.62`, and `sys 34.28` seconds. The command emitted these per-binary result lines:

```text
1 passed; 0 failed; 0 ignored; 0 measured; 363 filtered out; finished in 0.01s
1 passed; 0 failed; 0 ignored; 0 measured; 363 filtered out; finished in 0.01s
364 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.46s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 29.05s
26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s
1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.61s
20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.17s
6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s
1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s
5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
786 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.10s
5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

This is one pre-conversion timing sample while other Wave 1 work may have run concurrently. It establishes neither a stable full-suite duration nor repeated-load freedom from the prior race; the targeted gates below are the evidence for this plan's isolation behavior.

## Accomplishments

- Added public std-only child-test support with exact filtering, a directory-only PATH precondition, and per-Command extra environment.
- Added a discriminating empty-PATH control and a misspelled-name control that must panic when a child runs zero tests.
- Migrated all eight Pi and seven OpenCode PATH fixture tests; removed both `PathGuard`s, Pi's `EnvGuard`, and module-local environment mutexes.

## Verification

- The final automated plan gate printed `gate_rc=0`: all four Task 1 tests had one passing test and a non-zero filtered count; `tempfile_outside_cfg_test=0`; and `cargo check -p devflow --tests` exited 0.
- The same gate printed `pi_pathguard=0 pi_path_mutations=0 pi_child_guards=8`, `opencode_pathguard=0 opencode_path_mutations=0 opencode_child_guards=7`, `pi_non_path_mutations=0`, both module runs exited 0 with one success summary, and `clippy_exit=0`.
- The Pi module reported 13 passing tests and OpenCode 24 passing tests. These checks do not establish full-suite behavior across repeated concurrent executions.

## Task Commits

1. **Task 1: Tracer — baseline, helper controls, and one Pi conversion** — `1e732b7` (`feat`)
2. **Task 2: Remaining Pi/OpenCode migrations and guard removal** — `8b534d9` (`test`)

## Files Modified

- `crates/devflow-core/src/test_support.rs` — reusable child test API and isolation/vacuity controls.
- `crates/devflow-core/src/agents/pi.rs` — child-process Pi fixtures with child-local config override.
- `crates/devflow-core/src/agents/opencode.rs` — child-process OpenCode fixtures and explicit isolated `sleep` utility.

## Decisions Made

- Use the exact module-qualified test name as the child marker to prevent a child from taking another test's inner branch.
- Keep child PATH as only the fixture directory; the hanging OpenCode fixture gets exactly one `sleep` symlink rather than inheriting the parent PATH.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Child Pi tracer rebuilt a new fixture instead of reading the parent's fixture.**
- **Found during:** Task 1
- **Fix:** The child reads the parent fixture directory from its command-local `PI_CODING_AGENT_DIR` override.
- **Verification:** Targeted Pi tracer and the final plan gate passed.
- **Committed in:** `1e732b7`

**2. [Rule 1 - Bug] The isolated hanging OpenCode fixture needed `sleep`, which is not available from an empty PATH.**
- **Found during:** Task 2
- **Fix:** Linked only `/usr/bin/sleep` into the temporary stub directory.
- **Verification:** OpenCode module suite and final plan gate passed.
- **Committed in:** `8b534d9`

**Total deviations:** 2 auto-fixed bugs. Both were fixture-isolation corrections required by the plan; no production behavior changed.

## TDD Gate Compliance

The plan is `type: execute` with `tracer`/`auto` tasks rather than a `type: tdd` plan, so the runtime's RED/GREEN commit gate is not applicable. Its behavioral tests and required negative controls were added and verified with the implementation; no separate RED evidence record was produced.

## Known Stubs

None found in the three plan-modified source files.

## Next Phase Readiness

The reusable `devflow_core::test_support` API is available for the remaining TEST-01 fixture migrations.

## Self-Check: PASSED

Verified the three source files and this summary exist, and both task commits are reachable in the local repository.

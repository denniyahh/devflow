---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 17
subsystem: testing
tags: [test-isolation, path, clippy, cargo-test]
requires:
  - phase: 48-03
    provides: child-process PATH helper and pre-conversion baseline
  - phase: 48-09
    provides: enforced disallowed-methods lint and reasoned exceptions
provides:
  - Documented command-local PATH and exact-child-result contract
  - Post-conversion full and two-CPU test-suite evidence with stated limits
affects: [TEST-01, future CLI test fixtures]
actuals:
  tokens: 5232
  tasks: 2
  commits: 1
plan_head_before: 44702dde1552cea5cfbec24a4401bcd00d125856
tech-stack:
  added: []
  patterns:
    - PATH only on child Commands, with exact filtered-test evidence
    - Reasoned clippy expectations for deferred process-global test mutations
key-files:
  created: [48-17-SUMMARY.md]
  modified:
    - .planning/codebase/TESTING.md
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
key-decisions:
  - "The four remaining process-global variables retain the one-mutex-per-variable rule; PATH is not in that deferred set."
  - "One unpinned and one two-CPU run are sanity evidence only, not a flake-rate claim."
requirements-completed: [TEST-01]
coverage:
  - id: D1
    description: Test-isolation guidance names the enforced child PATH helper and clippy exception rule.
    requirement: TEST-01
    verification:
      - kind: other
        ref: 48-17 Task 1 automated gate
        status: pass
    human_judgment: false
  - id: D2
    description: Post-conversion test-suite runs completed once unpinned and once pinned to CPUs 0 and 1.
    requirement: TEST-01
    verification:
      - kind: other
        ref: timeout 45m /usr/bin/time -p scripts/check.sh test; timeout 45m taskset -c 0,1 scripts/check.sh test
        status: pass
    human_judgment: false
duration: session measurement window
completed: 2026-09-17
status: complete
---

# Phase 48 Plan 17: Test-Isolation Contract and Post-Conversion Evidence Summary

**The testing guide and stale comments now describe command-local, directory-only PATH fixtures guarded by exact child-test evidence, alongside bounded post-conversion suite measurements.**

## Performance

- **Completed:** 2026-09-17T09:02:30Z
- **Tasks:** 2/2
- **Files modified:** 4 (including this summary)
- **Task commits:** 1 — after independent verification, the orchestrator committed these owned changes with normal hooks under the user-approved Phase 48 #4799 workaround.

## Accomplishments

- Documented `devflow_core::test_support::run_test_in_child` as the sole PATH configuration path, requiring a directory value and `assert_child_ran_exactly_one_passing_test` with a module-qualified name.
- Retained the one-mutex-per-variable rule only for the four deferred process-global variables; each must carry a reasoned `expect(clippy::disallowed_methods)` enforced by `clippy.toml`.
- Removed stale `NeutralPath` commentary without changing test behavior, and recorded the successful bounded suite runs and their limits.

## Task Results

1. **Task 1: Tracer — publish the enforced PATH child-process rule and update stale helper commentary** — complete; committed by the orchestrator after independent verification.
   - Automated gate: `testing_md_run_test_in_child=3`, `testing_md_clippy.toml=1`, `testing_md_expect(clippy::disallowed_methods=1`, `neutral_path_all_lines=0`, `c25_exit=0 one_passed=1 filtered=380 filtered out`, `gate_rc=0`.
2. **Task 2: Record full-suite and two-CPU post-conversion evidence with its limits** — complete; committed by the orchestrator after independent verification.
   - Unpinned: `timeout 45m /usr/bin/time -p scripts/check.sh test` exited `0`; `/usr/bin/time -p` recorded `real 88.22`, `user 115.16`, `sys 29.82`; `check.sh: test OK` was emitted.
   - Two-CPU: `timeout 45m taskset -c 0,1 scripts/check.sh test` exited `0`; its attached execution session remained active through three 30-second polls and completed on the next poll after 21.28 seconds, for an observed session wall interval of 111.28 seconds; `check.sh: test OK` was emitted.

## Post-Conversion Test Results

The complete two-CPU log emitted 31 `test result:` summaries, all successful and with zero failures:

```text
1 passed; 0 failed; 0 ignored; 0 measured; 380 filtered out; finished in 0.01s
1 passed; 0 failed; 0 ignored; 0 measured; 380 filtered out; finished in 0.01s
381 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 33.10s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 37.03s
26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.77s
20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.17s
6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s
1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s
5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
804 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.57s
5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The 48-03 pre-conversion full-suite baseline was `real 92.35`, `user 146.62`, and `sys 34.28`. The unpinned post-conversion `real 88.22` is one measurement under whatever concurrent load existed in that run; it is not a stable duration claim. The two-CPU observed interval is a session-wall measurement, not a `/usr/bin/time` `real` value, so it must not be compared as a precise performance delta.

## Verification

- Task 1 source-and-exact-test gate: passed (`gate_rc=0`).
- Task 2 bounded runs: both exit codes were `0`, neither reached timeout exit `124`, the pinned log contained 31 test-result lines, and the unpinned timed run emitted exactly one `real` line.
- `cargo fmt --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `git diff --check`: passed.
- Independent orchestrator recheck: unpinned `scripts/check.sh test` exited 0 with 31 of 31 `test result:` lines successful and `/usr/bin/time -p` `real 80.06`; the two-CPU control exited 0 with 31 of 31 successful result lines. This recheck is another bounded observation, not a reliability measurement.

## Evidence Limits

One full run and one two-CPU pinned run demonstrate only that these completed runs were green. They do not establish a flake rate, reliable repeated concurrent behavior, absence of future PATH races, or that the known unrelated `commands.rs` PATH race noted in 48-09 cannot recur. The 45-minute timeout is a ceiling: both runs exited normally before it, not a proof about behavior near that ceiling.

## Decisions Made

- `clippy.toml` is intentionally documented but does not match the repository post-commit checklist trigger; no `DEV-SETUP-CHECKLIST` edit was made.
- No test behavior changed: comments now match the already-converted child-process fixture premise.

## Deviations from Plan

None - plan scope was followed. Per the parent executor's explicit Phase 48 #4799 workaround, the normal per-task and metadata commits were made by the orchestrator after independent verification.

## Known Stubs

None.

## Next Phase Readiness

TEST-01 has current documentation and bounded evidence. The orchestrator independently reviewed and committed the four owned files with normal hooks.

## Self-Check: PASSED

The four owned files exist; the required guidance terms and zero `NeutralPath` Rust references were rechecked; the exact C-25 test, formatting, workspace clippy, and diff check passed. The two suite runs are evidenced above and were independently repeated with the recorded limits.

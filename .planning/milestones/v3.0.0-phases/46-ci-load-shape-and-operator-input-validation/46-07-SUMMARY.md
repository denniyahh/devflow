---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 07
subsystem: testing
tags: [rust, test-harness, process-isolation, path, state-persistence, clippy, gap-closure]

requires:
  - phase: 46-ci-load-shape-and-operator-input-validation
    provides: "46-06's `run_test_without_git`, child-run anti-vacuity assertions, and the first two of seven PATH-emptying migrations"
provides:
  - "Five remaining PATH-emptying test windows migrated to child processes"
  - "A persisted-state control between the unmeasurable first Validate cycle and the real-git second cycle"
  - "Deletion of `NoGitPath` and corrected prose across CLI and core test support"
affects: [46-08, phase-46 verification, any future test that needs git to be unresolvable]

actuals:
  tokens: 8602
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Parent builds the git-requiring fixture; a child owns the empty PATH and executes the assertion"
    - "A state crossing a process boundary is re-asserted after `save_state`/`load_state`, not merely trusted from the child"
    - "A dead-code clippy failure before deletion is evidence that the final caller has gone"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-core/src/agent_result.rs

key-decisions:
  - "All five remaining empty-PATH windows move into child processes; none is serialized in the parent with `env_lock`."
  - "The two-cycle test persists the child's cycle-1 state and re-asserts its baseline and streak after reload before cycle 2 runs in the parent."
  - "`NoGitPath` was deleted only after clippy's dead-code failure certified that it had no callers."

patterns-established:
  - "A child-process test must prove its exact test ran, passed once, and filtered other tests; a successful process status is insufficient."
  - "A process-global environment hazard is removed by relocating it to a child, not by expanding the mutex's victim inventory."

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "The four single-call evaluate tests retain their assertions while their unresolvable-git calls run in a child process."
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#evaluate_layer2_unrunnable_git_* — 3 passed, 360 filtered out"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#evaluate_agent_result_with_unrunnable_git_does_not_report_failed — 1 passed, 362 filtered out"
        status: pass
    human_judgment: false
  - id: D2
    description: "The two-cycle Validate failure test runs its unmeasurable first cycle in a child, round-trips State, then runs its real-git second cycle in the parent."
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#validate_failure_with_unmeasurable_count_accumulates_the_streak — 1 passed, 362 filtered out"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow --bin devflow — 363 passed, 0 failed"
        status: pass
    human_judgment: false
  - id: D3
    description: "No in-process PATH-emptying guard or textual `NoGitPath` reference remains under crates/."
    requirement: INFRA-01
    verification:
      - kind: other
        ref: "grep -rn NoGitPath crates/ — 0 matches"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace --all-targets -- -D warnings; cargo fmt --check; cargo test --workspace --no-fail-fast"
        status: pass
    human_judgment: false

duration: 13 min
completed: 2026-09-06
status: complete
---

# Phase 46 Plan 07: Child-process the final PATH-emptying tests Summary

**All five remaining `pipeline_outcomes.rs` empty-PATH tests now execute only in child processes, with the two-cycle Validate case persisting and rechecking its State between the child and the parent; `NoGitPath` is gone and clippy enforces that it has no callers.**

## Performance

- **Duration:** 13 min
- **Started:** 2026-09-06T11:58:18Z
- **Completed:** 2026-09-06T12:11:53Z
- **Tasks:** 3 of 3
- **Files modified:** 4

## Accomplishments

- Migrated all four single-call `evaluate_*` sites: the parent builds the fixture, and the child retains every assertion under an empty PATH.
- Split the two-cycle streak test at the safe boundary: the child runs and saves cycle 1, while the parent reloads and reasserts the baseline/streak before cycle 2 with a real `git`.
- Removed `NoGitPath`, its process-global PATH mutation, and every prose reference under `crates/`.
- Preserved all 47 `env_lock()` lines introduced by `d525f9a`; 46-08 can now safely own their revert.

## Task Commits

1. **Task 1: migrate the four single-call evaluate sites** — `a84b731` (test)
2. **Task 2: split the two-cycle test across the process boundary** — `a1a8748` (test)
3. **Task 3: delete `NoGitPath` and correct its prose** — `35e0259` (fix)

## Required Measurements

### Targeted test evidence

| Test set | Result | Filtered out |
| --- | --- | --- |
| `evaluate_layer2_unrunnable_git_*` | 3 passed, 0 failed | 360 |
| `evaluate_agent_result_with_unrunnable_git_does_not_report_failed` | 1 passed, 0 failed | 362 |
| `validate_failure_with_unmeasurable_count_accumulates_the_streak` | 1 passed, 0 failed | 362 |
| Full `devflow` bin target | 363 passed, 0 failed | 0 |

The non-zero filtered counts on every exact-child invocation prove the filter selected a test from a larger binary; they do not establish a general reliability rate for the old race.

### Deliberate failure probes

- **Task 1:** Inverting the child's `result.is_none()` assertion failed with exit 101. The parent reported `child test process must exit 0`, included `--- child stdout ---`, and surfaced the child's `got: None` assertion at `pipeline_outcomes.rs:3227`.
- **Task 2:** Changing the child's expected baseline to `Some(0)` failed with exit 101. The parent again exposed the child output, including `left: Some(1)` and `right: Some(0)`.

These probes establish that each parent wrapper surfaces a failing child assertion. They do not prove failure handling for every child crash mode.

### Compiler proof before deletion

Before deletion, `cargo clippy --workspace --all-targets -- -D warnings` failed with:

```
error: struct `NoGitPath` is never constructed
error: associated function `install` is never used
```

After deletion, clippy and `cargo fmt --check` both exited 0. `grep -rn "NoGitPath" crates/` returned 0 matches.

### C-01 closure accounting

46-06 migrated two sites; this plan migrated the five remaining sites. The current source has zero non-comment `NoGitPath::install` calls, compared with one immediately before Task 2. Removing the now-dead type makes any remaining caller a `-D dead-code` compiler failure rather than a grep-only claim.

`d525f9a` added 47 `env_lock()` lines; a current `git blame` comparison counted all 47 still attributed to that commit. This establishes that 46-07 did not begin 46-08's revert.

## Plan-Level Verification

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo fmt --check` | pass |
| `cargo test --workspace --no-fail-fast` | cargo exit 0; `failed_binaries=0`; includes `devflow` 363 passed and `devflow-core` 764 passed |
| Text references to `NoGitPath` under `crates/` | 0 |
| `d525f9a` `env_lock()` additions still present | 47 added / 47 currently attributed |

The workspace test exercises the repository's current suite, but it does not reproduce or quantify the prior scheduling race and it did not run in GitHub's CI container. C-01's closure here is mechanical: an empty PATH in a child cannot mutate the parent's environment.

## Decisions Made

1. Kept every original test assertion inside its child branch rather than reducing the tests to child process exit status.
2. Reasserted `last_validate_failure_commit_count == Some(1)` and `consecutive_failures == 2` after reload; this is the persistence control before cycle 2 depends on those fields.
3. Deleted the obsolete guard only after its two clippy diagnostics were observed, then updated prose to recommend the child-process pattern for future core-crate tests.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Corrected the Task 3 formatting exit-code measurement**

- **Found during:** Task 3 verification
- **Issue:** The planned `cargo fmt --check 2>&1 | tail -3; echo "fmt_exit=$?"` reports `tail`'s status, so a formatting failure could read as green.
- **Fix:** Recorded `fmt_exit=${PIPESTATUS[0]}` from the Bash pipeline.
- **Files modified:** none
- **Verification:** `fmt_exit=0` after the corrected measurement.
- **Committed in:** n/a — plan-verification correction only.

**2. [Rule 1 - Bug] Corrected the stale State activity description**

- **Found during:** Plan close-out self-check
- **Issue:** `state.advance-plan` moved the current plan to 8 but left `last_activity_desc` claiming that five callers still survived for 46-07.
- **Fix:** Replaced it with the completed seven-site migration, deletion, and 46-08-ready state.
- **Files modified:** `.planning/STATE.md`
- **Verification:** Current plan is 8, the source-tree text scan is zero, and the updated description matches the committed task results.
- **Committed in:** metadata correction commit.

**Total deviations:** 2 auto-fixed (1 Rule 3 verification-measurement correction, 1 Rule 1 state-record correction).
**Impact on plan:** No scope expansion or product-code change; the corrections prevent false-green verification and stale planning state.

## TDD Gate Compliance

**Warning:** This `type: tdd` plan has test/test/fix task commits rather than a classic committed RED-to-GREEN production pair. It migrates existing assertions onto the helper built in 46-06 rather than adding production behaviour. Both migrations did execute deliberate failing child assertions and then passed after restoration, but that is not a substitute for a separately committed RED gate.

## Known Stubs

None. The changed diff contains no placeholder, TODO, FIXME, empty rendered data, or unwired mock value.

## Issues Encountered

The foreground executor caps commands at roughly 30 seconds. The full workspace verification was therefore run in a detached process with a five-minute timeout and its actual `cargo_exit=0` / `failed_binaries=0` output was read from the log; the bound, not the test suite, explains the foreground truncation.

## User Setup Required

None — no external service configuration is required.

## Next Phase Readiness

46-08 is unblocked to remove `d525f9a`'s 47 no-longer-needed `env_lock()` additions. This plan does not itself prove a scheduling-race rate or a hosted-CI run; it proves the process-global empty-PATH windows are absent from the `devflow` bin target.

## Self-Check: PASSED

- Summary file exists and all three task commits (`a84b731`, `a1a8748`, `35e0259`) resolve in `git log --all`.
- Coverage metadata classifies all three deliverables as automated with non-empty passing verification.

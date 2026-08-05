---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
plan: 02
subsystem: infra
tags: [rust, serde, state-machine, devflow-core]

# Dependency graph
requires: []
provides:
  - "State::last_validate_failure_commit_count — persisted forward-progress baseline (Option<u32>, #[serde(default)])"
  - "mode::consecutive_failures_made_progress — pure reset-vs-accumulate predicate, (Option<u32>, u32) -> bool"
affects: [33-03]

# Actuals (#2632)
actuals:
  tokens: 2240
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Baseline-observation fields (replaced wholesale, not incremented) are excluded from transition()'s counter resets, matching preflight_retries and checkpoint_resumes"
    - "Option<u32> distinguishing 'never observed' from 'observed zero' for a persisted forward-progress signal"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs

key-decisions:
  - "Field type is Option<u32>, not bare u32 — None means no prior failure recorded (including pre-field state), Some(0) means a failure was recorded against a branch that genuinely carried zero commits. Collapsing these would make every phase's first failure behave like its second."
  - "last_validate_failure_commit_count is NOT touched by transition() — it's a baseline observation, not a counter, matching preflight_retries and checkpoint_resumes rather than consecutive_failures and infra_failures."
  - "consecutive_failures_made_progress takes no Path, git handle, or I/O-derived argument — the caller (33-03) does the I/O and passes in already-computed numbers, preserving the pure-predicate shape transition_resets_consecutive_failures established."
  - "Comparison is strictly-greater, not not-equal: a count that went down (branch rewound/rebuilt) is not evidence the reported problem was addressed, so it must not reset the streak."

requirements-completed: [DOGFOOD-02]

coverage:
  - id: D1
    description: "State persists a forward-progress baseline (last_validate_failure_commit_count) that survives across devflow advance invocations, defaulting safely for state written before the field existed"
    requirement: "DOGFOOD-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::last_validate_failure_commit_count_round_trips_through_serde"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::last_validate_failure_commit_count_absent_from_json_defaults_to_none"
        status: pass
    human_judgment: false
  - id: D2
    description: "A pure predicate (consecutive_failures_made_progress) decides reset-vs-accumulate from two already-computed numbers, with no I/O in its signature, and documents its known trivial-commit limitation in source"
    requirement: "DOGFOOD-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/mode.rs#mode::tests::made_progress_treats_no_prior_record_as_progress"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/mode.rs#mode::tests::made_progress_requires_a_strictly_higher_count"
        status: pass
    human_judgment: false

# Metrics
duration: 21min
completed: 2026-08-04
status: complete
---

# Phase 33 Plan 02: Forward-Progress Baseline and Reset Predicate Summary

**Added `State::last_validate_failure_commit_count` (`Option<u32>`) and the pure predicate `mode::consecutive_failures_made_progress` that reads it — both unwired, ready for 33-03 to consume.**

## Performance

- **Duration:** 21 min
- **Started:** 2026-08-04T19:51:21-04:00 (base commit)
- **Completed:** 2026-08-04T20:12:32-04:00
- **Tasks:** 2 completed
- **Files modified:** 2

## Accomplishments
- `State` now carries `last_validate_failure_commit_count: Option<u32>` (`#[serde(default)]`), initialized to `None` in `State::new`, doc-commented to the five-part standard `infra_failures` sets, and explicitly excluded from `transition()`'s counter resets.
- `mode::consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool` added immediately after `transition_resets_consecutive_failures`, body `previous.is_none_or(|p| current > p)`, with a doc comment that names its own known limitation (see below).
- Four new tests across the two files, all passing by exact name; `scripts/check.sh all` exits 0 (fmt, clippy `-D warnings`, full workspace test suite of 545 lib tests + integration suites — captured as a direct exit code, not through a pipeline, per this repo's CLAUDE.md).

## Task Commits

Each task was committed atomically:

1. **Task 1: Persist the forward-progress baseline on State** - `8ce5bf1` (feat)
2. **Task 2: The pure reset-vs-accumulate predicate** - `558bf18` (feat)

_TDD note: both tasks were marked `tdd="true"` in the plan, but the plan's `<action>` specified the field/function and its tests as a single unit rather than a separate RED-then-GREEN sequence (unlike the module's existing `transition_resets_consecutive_failures` precedent, which also has no standalone test-only commit in its own history). Implementation and tests were written and verified together per task, then committed once as `feat`, matching the plan's task-commit-protocol default and the sibling predicate's established commit shape in this file's git history._

## Files Created/Modified
- `crates/devflow-core/src/state.rs` - new `pub last_validate_failure_commit_count: Option<u32>` field (`#[serde(default)]`), `State::new` initializer, two tests
- `crates/devflow-core/src/mode.rs` - new `pub fn consecutive_failures_made_progress`, two tests

## The predicate's documented limitation (quoted verbatim, per plan's Output spec)

From `crates/devflow-core/src/mode.rs`'s doc comment on `consecutive_failures_made_progress`:

> **What this predicate does not establish.** A `true` result means new commits exist, not that those commits addressed anything. An agent that commits something trivial on every cycle resets the streak every cycle and never reaches `MAX_CONSECUTIVE_FAILURES`. This is the accepted, documented weakness of the commit-count signal recorded in `33-RESEARCH.md`'s D-03 Recommendation and Assumptions Log A1 — the same weakness `evaluate_layer2`'s own "no work done" gate already carries, which a single trivial commit also already defeats today. It is a real narrowing of the guarantee that `MAX_CONSECUTIVE_FAILURES` bounds a genuinely stuck loop, and it is deliberately NOT strengthened here with a lines-changed or files-touched threshold — that is a follow-up if the assumption proves wrong, not a speculative heuristic to add to the safety gate's path now.

## `devflow-cli` untouched

Confirmed by `git diff --stat` across both commits: only `crates/devflow-core/src/state.rs` and `crates/devflow-core/src/mode.rs` changed. No `crates/devflow-cli/` file was touched, as the plan's `<verification>` requires — the primitives ship with no consumers, which is what let this plan run in the same wave as 33-01.

## Decisions Made
None beyond what CONTEXT.md/the plan already locked (field shape, predicate signature, and the deliberate omission of a `checkpoint:decision` gate — all pre-decided in the plan body, not re-derived here).

## Deviations from Plan
None - plan executed exactly as written. The field and predicate signatures, doc-comment content, and test names all match the plan's `must_haves` and task `<action>` sections verbatim.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Both 999.66 primitives exist, compile, and are covered by direct unit tests with no I/O and no filesystem setup, per the plan's reversibility/prohibition constraints (`transition_resets_consecutive_failures`'s signature is byte-unchanged, confirmed via `rg` match).
- 33-03 can now wire `last_validate_failure_commit_count` and `consecutive_failures_made_progress` together with the commit-count I/O helper it is responsible for adding.
- No blockers.

## Self-Check: PASSED
- `crates/devflow-core/src/state.rs` — FOUND
- `crates/devflow-core/src/mode.rs` — FOUND
- `.planning/phases/33-loop-back-correctness-for-multi-wave-validate-code-cycles-99/33-02-SUMMARY.md` — FOUND
- Commit `8ce5bf1` (Task 1) — FOUND in git log
- Commit `558bf18` (Task 2) — FOUND in git log
- Commit `c178889` (SUMMARY) — FOUND in git log

---
*Phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99*
*Completed: 2026-08-04*

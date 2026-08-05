---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
plan: 03
subsystem: infra
tags: [rust, git, state-machine, safety-gate, devflow-core, devflow-cli]

# Dependency graph
requires:
  - phase: 33
    provides: "33-01's phase_verification_exists/select_loop_back_fix (unrelated D-01 fix, same phase) and 33-02's State::last_validate_failure_commit_count + mode::consecutive_failures_made_progress (the two primitives this plan wires together)"
provides:
  - "agent_result::phase_commit_count — the single git-derived commit count for a phase's feature branch, consumed by both evaluate_layer2 and handle_validate_outcome"
  - "handle_validate_outcome's reset-vs-accumulate branch, replacing the unconditional consecutive_failures increment"
  - "test_support::commit_on_feature_branch — lands one real commit on feature/phase-NN, repeatable within a test"
affects: [phase-34]

# Actuals (#2632)
actuals:
  tokens: 4930
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Single git-derived count helper (phase_commit_count) consumed by both the Layer-2 result evaluator and the safety-gate reset decision, so the two can never silently diverge"
    - "Baseline-vs-fresh-count comparison narrows a safety-gate ceiling's guarantee instead of disabling it — degrades toward gating (unrunnable git / missing branch / no baseline all read as accumulate-or-first-failure, never as permanent progress)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/test_support.rs

key-decisions:
  - "phase_commit_count is a pure extraction of evaluate_layer2's existing inline block — no behavior change, verified by the unmodified agent_result:: test slice staying green (151 passed)"
  - "On progress, consecutive_failures is set to 1, not 0 — the gate context message a few lines below interpolates the counter, and zeroing it would make a real failure's message read zero"
  - "The forward-progress baseline (state.last_validate_failure_commit_count) advances on EVERY recorded failure, not only the progress branch — a stale low baseline would otherwise report progress forever"
  - "Three pre-existing tests that seed state.consecutive_failures directly (bypassing the mechanism that would record the baseline) needed last_validate_failure_commit_count = Some(0) seeded alongside — otherwise the fresh None baseline reads as a first-ever failure and resets their pre-seeded streak. Documented inline as a Rule 1 fix, not a plan deviation in scope or design."

patterns-established:
  - "A safety-gate's forward-progress signal is git-derived (phase_commit_count), never agent-self-reported — closing 33-RESEARCH.md's Don't Hand-Roll concern about the count implementation diverging between two call sites"

requirements-completed: [DOGFOOD-02]

coverage:
  - id: D1
    description: "A 3+ wave phase making genuine forward progress (a real commit before every Validate failure) no longer false-gates — ROADMAP criterion 3"
    requirement: DOGFOOD-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::healthy_multi_wave_progress_does_not_reach_the_ceiling"
        status: pass
    human_judgment: false
  - id: D2
    description: "A phase making no forward progress (stable non-zero commit count, no new commits) still reaches MAX_CONSECUTIVE_FAILURES and still forces the gate — ROADMAP criterion 4, on a route the pre-existing no-repository guard doesn't reach"
    requirement: DOGFOOD-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::repeated_failure_without_new_commits_still_reaches_the_ceiling"
        status: pass
    human_judgment: false
  - id: D3
    description: "One commit-counting implementation (phase_commit_count) serves both evaluate_layer2 and the new reset decision — the two counts cannot silently diverge"
    requirement: DOGFOOD-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests (full slice, 151 passed, unmodified behavior)"
        status: pass
    human_judgment: false
  - id: D4
    description: "The pre-existing safety-gate regression guard (consecutive_failures_reaches_ceiling_across_cycles) passes with a byte-unchanged body, proving the fix narrowed the false-gate rather than disabling the gate"
    requirement: DOGFOOD-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::consecutive_failures_reaches_ceiling_across_cycles"
        status: pass
    human_judgment: false

# Metrics
duration: 27min
completed: 2026-08-04
status: complete
---

# Phase 33 Plan 03: Forward-Progress Wiring for the Consecutive-Failures Gate Summary

**Closed 999.66 (DOGFOOD-02): `handle_validate_outcome`'s consecutive-failures counter now resets to 1 on a real new commit and only accumulates when a Validate→Code loop produces nothing, so a healthy 3+ wave phase no longer false-gates at wave 3 while a genuinely stuck loop still reaches `MAX_CONSECUTIVE_FAILURES`.**

## Performance

- **Duration:** 27 min (base commit 20:33:11 → last task commit 21:00:26, 2026-08-04)
- **Tasks:** 3/3 completed
- **Files modified:** 3 (`agent_result.rs`, `pipeline_outcomes.rs`, `test_support.rs`)

## Accomplishments

- `agent_result::phase_commit_count(project_root, git_flow, phase) -> u32` — a pure extraction of
  `evaluate_layer2`'s existing inline `rev-parse --verify` / `rev-list --count` block into the
  single implementation of "commits on the phase's feature branch not on develop." `evaluate_layer2`
  now calls it instead of re-deriving the count inline. New test
  `phase_commit_count_reports_zero_without_a_branch` asserts the no-branch case the ceiling
  regression guard silently depended on.
- `handle_validate_outcome`'s failure block now decides between beginning a fresh streak and
  continuing the existing one: it reads `phase_commit_count` fresh, asks
  `mode::consecutive_failures_made_progress(state.last_validate_failure_commit_count, current)`,
  sets the counter to 1 on progress (not 0 — the gate context message interpolates it) or
  `saturating_add(1)` on no progress, and updates the baseline to `Some(current)` on every recorded
  failure regardless of branch.
- `test_support::commit_on_feature_branch(root, phase, label)` — lands one real commit on
  `feature/phase-{phase:02}`, creating the branch on first call and using plain `checkout` (never
  `checkout -B`) on later calls so it doesn't discard commits an earlier call already made.
- The matched pair: `healthy_multi_wave_progress_does_not_reach_the_ceiling` (criterion 3, a real
  commit before every one of `MAX_CONSECUTIVE_FAILURES + 1` failures leaves the counter at 1 and
  never forces the gate) and `repeated_failure_without_new_commits_still_reaches_the_ceiling`
  (criterion 4, the negative control — a branch with a stable non-zero commit count and no further
  commits still reaches the ceiling).
- `consecutive_failures_reaches_ceiling_across_cycles` (the pre-existing regression guard, run
  against a root with no git repository at all) passes with a byte-unchanged body — confirmed via
  `git diff` showing the two new tests as the only addition to `pipeline_outcomes.rs` relative to
  the Task 2 commit.

## RED-first evidence (ai-change-acceptance requirement 1 + 3)

Task 2 was already committed by the time Task 3's tests were written, so RED evidence was captured
by temporarily reverting `handle_validate_outcome`'s counter block to the pre-999.66 unconditional
`saturating_add(1)`, running both new tests, then restoring the fix byte-identical (confirmed via
`git diff HEAD` showing zero `-` lines against the Task 2 commit other than the diff header, i.e.
the restore was exact).

- `healthy_multi_wave_progress_does_not_reach_the_ceiling` against the pre-fix code — a genuine
  assertion failure, not a compile error or panic:
  ```
  thread '...' panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1323:9:
  assertion `left == right` failed: a new commit before every failure must restart the streak at 1, not accumulate it
    left: 4
   right: 1
  ```
- `repeated_failure_without_new_commits_still_reaches_the_ceiling` **also passed against the
  pre-fix code** (`1 passed; 0 failed`) — the correct negative-control shape: removing the one
  variable this test isolates (repeated commits) restores the pre-fix behavior exactly, so the
  control is not vacuous.

After restoring the fix, both tests pass (`1 passed; 0 failed` each, confirmed by `--exact` name).

## The predicate's documented limitation, quoted (per plan's Output spec)

From `handle_validate_outcome`'s doc comment:

> A `true` (progress) result means new commits exist on the feature branch since the last recorded
> failure — **not** that those commits addressed what Validate reported. This narrows the ceiling's
> guarantee to loops that produce no commits at all; it does not disable the ceiling. That is the
> accepted weakness of the commit-count signal recorded in `33-RESEARCH.md`'s D-03 Recommendation
> and Assumptions Log A1. The failure direction is toward gating: an unrunnable `git` or a missing
> branch counts zero every cycle, so once a baseline is recorded the counter accumulates and the
> gate stays reachable.

And from the criterion-3 test's own doc comment: "It proves the counter does not accumulate when
new commits land between failures — it cannot distinguish a commit that fixed what Validate
reported from a commit that did not. An agent that commits anything at all on every cycle resets
the streak every cycle."

## Task Commits

1. **Task 1: Extract one shared commit-count helper from evaluate_layer2** - `ed274ef` (refactor)
2. **Task 2: Replace the unconditional increment with the reset-vs-accumulate branch** - `7e356cf` (feat)
3. **Task 3: The matched multi-wave pair — progress passes, no progress still gates** - `4e135bf` (test)

## Files Created/Modified

- `crates/devflow-core/src/agent_result.rs` — `phase_commit_count` (new), `evaluate_layer2` now
  calls it, `phase_commit_count_reports_zero_without_a_branch` (new test)
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `handle_validate_outcome`'s reset-vs-accumulate
  branch, extended doc comment, two new multi-wave tests, and three pre-existing tests' setup
  extended with a seeded baseline (see Deviations)
- `crates/devflow-cli/src/test_support.rs` — `commit_on_feature_branch` (new test-only helper)

## Decisions Made

- **Counter set to 1, not 0, on progress** — locked in the plan's Task 2 action text: the gate
  context rendered a few lines below interpolates `state.consecutive_failures` into a message
  naming how many times validation has failed, and 0 would misreport a real failure as "0 times."
- **Baseline updates on both branches, always** — also locked in the plan: updating only on the
  progress branch would let a stale low baseline report progress forever once a "no progress" cycle
  had actually occurred.
- **`prepare_loop_back_to_code` and `transition_resets_consecutive_failures` left untouched** —
  the plan's explicit prohibition; the counter's only increment site is `handle_validate_outcome`,
  confirmed via `sed`+`grep` showing exactly 1 production assignment of `state.consecutive_failures
  =` in `pipeline_gate.rs` (unchanged, still `transition()`'s existing reset).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Three pre-existing tests broke because they seed `consecutive_failures` directly, bypassing the mechanism that would have recorded the new baseline**
- **Found during:** Task 2, running the whole `pipeline_outcomes::` slice for the first time after
  wiring the reset-vs-accumulate branch.
- **Issue:** `validate_failure_threshold_forces_gate_then_aborts`,
  `drive_validate_advance_and_read_gate_context` (the shared helper behind
  `validate_gaps_does_not_advance_to_ship` and `validate_missing_verdict_does_not_advance`), and
  `consecutive_failures_increment_saturates` all set `state.consecutive_failures` to a specific
  value directly (e.g. `MAX_CONSECUTIVE_FAILURES - 1`, `u32::MAX`) to skip past the setup needed to
  reach that value through repeated real failures. None of them had ever gone through
  `handle_validate_outcome`'s failure path before, so `state.last_validate_failure_commit_count`
  was still its fresh `None` default. `mode::consecutive_failures_made_progress(None, _)` is `true`
  by construction (33-02's design: "no prior record" reads as progress) — so the very first call to
  `handle_validate_outcome` in each of these tests reset the directly-seeded streak to 1 instead of
  continuing it, failing assertions that expected the pre-seeded value plus one.
- **Fix:** Seeded `state.last_validate_failure_commit_count = Some(0)` alongside the direct
  `consecutive_failures` assignment in all three call sites, matching the commit count
  `phase_commit_count` actually reports in these tests (none of them run against a git repository,
  so the count is 0 by the branch-missing fallback). This correctly simulates "a baseline was
  already recorded, at 0, on a prior cycle" — consistent with what the pre-seeded
  `consecutive_failures` value implies happened before the test starts.
- **Files modified:** `crates/devflow-cli/src/pipeline_outcomes.rs` (same file already in scope for
  Task 2 — no additional files touched)
- **Verification:** Re-ran the full `pipeline_outcomes::` slice: 35 passed, 0 failed (up from 4
  failures before the fix). Each of the three affected tests re-run individually by `--exact` name
  also confirmed `1 passed`.
- **Committed in:** `7e356cf` (Task 2 commit — the fix landed before the commit, so no separate fix
  commit was needed)

---

**Total deviations:** 1 auto-fixed (Rule 1 — a bug in three pre-existing tests' setup that the
reset-vs-accumulate change exposed, not a defect in the fix's own logic).
**Impact on plan:** No scope creep; the fix only touched the same three test functions' setup code,
bringing them into conformance with the new baseline field's contract. The plan's acceptance
criterion for Task 2 ("the whole `pipeline_outcomes::` and `pipeline_gate::` slices report `test
result: ok`") is what surfaced this — it did its job.

## Issues Encountered

The plan's own `<verify>` commands for Tasks 2 and 3 specify `cargo test -p devflow --lib
pipeline_outcomes::` / `pipeline_gate::`. As this repo's `ai-change-acceptance` skill and
`CLAUDE.md` both document, `-p devflow --lib` hard-errors ("no library targets found") because
`devflow` (the CLI package) is binary-only. Ran the documented working form instead — `cargo test
-p devflow pipeline_outcomes::` / `pipeline_gate::` (bare, no `--lib`) — which is the same
documented dead end 33-01/33-02 already recorded, not a new finding.

## User Setup Required

None — no external service configuration required.

## Verification Summary

All plan-mandated verification commands pass with a literal `1 passed` line (or `test result: ok`
for whole-slice commands):

```
cargo test -p devflow-core --lib agent_result::tests::phase_commit_count_reports_zero_without_a_branch -- --exact
  -> 1 passed
cargo test -p devflow-core --lib agent_result::
  -> test result: ok. 151 passed; 0 failed
cargo test -p devflow pipeline_outcomes::  (bare form, not --lib — see Issues Encountered)
  -> test result: ok. 35 passed; 0 failed
cargo test -p devflow pipeline_outcomes::tests::consecutive_failures_reaches_ceiling_across_cycles -- --exact
  -> 1 passed
cargo test -p devflow pipeline_gate::
  -> test result: ok. 17 passed; 0 failed
cargo test -p devflow pipeline_outcomes::tests::healthy_multi_wave_progress_does_not_reach_the_ceiling -- --exact
  -> 1 passed
cargo test -p devflow pipeline_outcomes::tests::repeated_failure_without_new_commits_still_reaches_the_ceiling -- --exact
  -> 1 passed
```

`scripts/check.sh all` (fmt + `clippy --workspace --all-targets -- -D warnings` + `cargo test
--workspace --no-fail-fast`) exits 0, captured directly (not through a pipeline): `==> check.sh:
all OK`. `devflow-core`'s full suite reports 547 passed / 0 failed; `devflow`'s (the CLI package)
own unittests report 269 passed / 0 failed, plus 3 more passed across its integration test
binaries.

Additional structural acceptance checks confirmed live:
- `rg -c "phase_commit_count" crates/devflow-core/src/agent_result.rs` = 5 (definition, doc-comment
  mentions, and `evaluate_layer2`'s call plus the new test — at least 3 required).
- Read confirmation: `evaluate_layer2` no longer builds its own count — `commits` is bound from
  `phase_commit_count(project_root, git_flow, phase)`, and the `rev-parse --verify` /
  `rev-list --count` pair now appears only inside `phase_commit_count` itself.
- `sed -n '1,/^#\[cfg(test)\]/p' crates/devflow-cli/src/pipeline_gate.rs | grep -c
  'state\.consecutive_failures ='` = 1 — the only production assignment in that file is still
  `transition()`'s existing reset.
- `rg -n "consecutive_failures_made_progress|phase_commit_count" crates/devflow-cli/src/pipeline_outcomes.rs`
  shows both called from inside `handle_validate_outcome`.
- `rg -n "saturating_add" crates/devflow-cli/src/pipeline_outcomes.rs` still matches on the
  `consecutive_failures` accumulate branch (`infra_failures`' two saturating_add sites plus this
  one).
- `rg -n "fn commit_on_feature_branch" crates/devflow-cli/src/test_support.rs` matches; `rg -c
  "commit_on_feature_branch" crates/devflow-cli/src/pipeline_outcomes.rs` = 2 (both new tests use
  it).
- `git diff HEAD~1 HEAD --diff-filter=D --name-only` empty after every task commit — no unexpected
  file deletions.

## Next Phase Readiness

- 999.66 (DOGFOOD-02) is closed: ROADMAP criteria 3 and 4 both have a direct automated assertion on
  structurally different paths (branch-present-with-progress, branch-present-without-progress), and
  the pre-existing branch-absent regression guard (criterion 4's other route) still passes
  unmodified.
- Both of Phase 33's defects (999.65/DOGFOOD-01 in 33-01, 999.66/DOGFOOD-02 across 33-02/33-03) are
  now closed. No blockers for Phase 34.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/agent_result.rs`
- FOUND: `crates/devflow-cli/src/pipeline_outcomes.rs`
- FOUND: `crates/devflow-cli/src/test_support.rs`
- FOUND commit `ed274ef` (Task 1)
- FOUND commit `7e356cf` (Task 2)
- FOUND commit `4e135bf` (Task 3)

---
*Phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99*
*Completed: 2026-08-04*

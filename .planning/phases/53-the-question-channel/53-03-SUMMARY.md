---
phase: 53-the-question-channel
plan: 03
subsystem: pipeline, agent_result, verify, agents
tags: [checkpoint, failure-gate, relaunch-removal, breaking-change]
status: complete

requires: []
provides:
  - "advance_with: a failed stage whose final result declares `checkpoint: blocking-human` (any agent, any stage) opens the stage failure gate with the reason sentence `the agent reported a blocking-human checkpoint it could not ask through devflow_ask` (D-13)"
  - "A declared checkpoint at Validate goes to handle_stage_failure, not handle_validate_outcome (review concern 12)"
  - "The checkpoint auto-decide relaunch is gone: relaunch_checkpoint_session, resume_launch_shape, spawn_agent_and_record, rescan_gate_context (CLI) and exec_resume_command, phase_has_blocking_human_checkpoint, blocking_human_checkpoint_reported, checkpoint_reported_in_capture (core) are deleted (D-12)"
affects: [53-06, 53-10]

actuals:
  tokens: 57757
  tasks: 2
  commits: 3
plan_head_before: b7d259ef8c2a88778bacd8d3438432b5a1f376bc
plan_head_after: 61f1e9df1a781351134c34cdc30d140554366050

tech-stack:
  added: []
  patterns:
    - "Gate-open tests read the run's state while the gate is open (thread + poll on Gates::gate_path), then answer abort: an abort clears the state file, so state read after the advance is gone"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-core/src/agents/claude.rs
    - crates/devflow-core/src/verify.rs
    - crates/devflow-core/src/agent_result.rs
  deleted:
    - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap
    - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap

key-decisions:
  - "augment_unresolved_checkpoint_reason now appends `— {why}` (or uses `why` alone) instead of the old `confirmed checkpoint could not auto-resolve: {why}` prefix, which named a route that no longer exists"
  - "rescan_gate_context deleted with its one test-only use in pipeline_outcomes' way-on sentence test: after this plan nothing opens the CheckpointDeclaration gate, so the function was dead code that failed clippy; the gate kind itself still goes in 53-10"
  - "ASK-05 not marked complete: five later plans (53-06, 53-09, 53-10, 53-11, 53-13) also carry it"

patterns-established: []

requirements-completed: []

coverage:
  - id: D1
    description: "A Code failure declaring blocking-human opens the stage failure gate naming devflow_ask, for Claude and Codex, with no relaunch and one stage_launched"
    requirement: ASK-05
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_declared_checkpoint_opens_the_failure_gate_without_a_relaunch"
        status: pass
    human_judgment: false
  - id: D2
    description: "Control: the same failure with no declaration opens the same gate kind without the sentence"
    requirement: ASK-05
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_failure_without_a_declaration_has_no_checkpoint_sentence"
        status: pass
    human_judgment: false
  - id: D3
    description: "A Validate failure declaring the checkpoint opens the stage failure gate and leaves consecutive_failures and the stage unchanged; without the declaration the fix loop still runs"
    requirement: ASK-05
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_declared_checkpoint_at_validate_opens_the_failure_gate_not_the_fix_loop"
        status: pass
    human_judgment: false

duration: 78min
completed: 2026-11-19
---

# Phase 53 Plan 03: Remove the checkpoint relaunch Summary

A failed stage that declares `checkpoint: blocking-human` now opens the never-silent stage failure gate with a reason saying the agent could not ask through `devflow_ask`. This holds for every agent and stage, including Validate, which no longer enters the fix loop. The checkpoint auto-decide relaunch and its core readers are deleted.

## What changed

- `advance_with`'s `Action::GateReview` arm no longer checks agent, stage, plan scan, capture scan, session id or resume ceiling. It reads `result.declares_blocking_human_checkpoint()`, appends the D-13 sentence to the reason when the declaration is there, and routes Validate to `handle_stage_failure` in that case.
- CLI deletions: `relaunch_checkpoint_session`, `resume_launch_shape`, `spawn_agent_and_record` (only the relaunch called it) and `rescan_gate_context`. The tests for the relaunch, the re-scan gate and the confirm predicate are deleted, with their fixtures and the two turn snapshots. `recorded_approval` stays because preflight uses it. `state.checkpoint_resumes = 0` stays in `launch_stage_inner` and goes in 53-10.
- `each_status_less_gate_records_its_kind` keeps its other kinds and drops its `checkpoint_declaration` segment, because the advance can no longer open that gate.
- Core deletions: `ClaudeDriver::exec_resume_command`, `verify::phase_has_blocking_human_checkpoint`, `agent_result::{blocking_human_checkpoint_reported, checkpoint_reported_in_capture}`, the raw-capture reader only they used (`capture_reports_human_gate`, `claude_stream_reports_human_gate`, `json_strings_stated_outcome`, `latest_stated_outcome`, `line_reports_human_gate`, `StatedOutcome`, `strip_leading_markup`), and their tests and fixtures.
- verify.rs tests that used `phase_has_blocking_human_checkpoint` as the second of two predicates now use a test-local `blocking_human_declared` built on `phase_checkpoint_declarations`, so the parser keeps its `blocking_human` coverage. `decision_reasoning_above_the_result_line_parses_to_that_result` drops its resume-prompt half, so no caller of `checkpoint_auto_decide_prompt` is left outside prompt.rs.
- `AgentResult::declares_blocking_human_checkpoint` and its `declared_checkpoint_*` tests stay (D-13 reader). `verify::phase_checkpoint_declarations` and `CheckpointDeclaration` stay (preflight, until 53-10).

## Verification

- Task 1 gate: all three named tests pass and both `test ! -e` snapshot checks exit 0. RED commit 8fef947: the Code test failed on `checkpoint_declaration` != `stage_failure`, because the old route opened the re-scan gate, and the Validate test failed on `reader_failure` != `stage_failure`. Negative control after GREEN: removing the `if !declared_checkpoint` Validate guard makes the Validate test fail on `reader_failure` != `stage_failure`. The guard was then restored.
- Task 2 gate: `declared_checkpoint_is_read_from_the_marker_field` and `declared_checkpoint_in_a_stream_counts_only_on_the_last_result` pass. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo nextest run -p devflow-core --lib`: 921 passed. `cargo nextest run -p devflow --bin devflow`: 497 passed, 3 skipped.
- `scripts/check.sh all` exit 0 on 61f1e9d. This includes doc_check, and the plan added no env var or Command variant.

### semver-checks

`cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor` exits 100 with `202 checks: 199 pass, 3 fail`:

- `function_missing`: `verify::phase_has_blocking_human_checkpoint`, `agent_result::blocking_human_checkpoint_reported`, `agent_result::checkpoint_reported_in_capture` (this plan, expected)
- `inherent_method_missing`: `ClaudeDriver::exec_resume_command` (this plan, expected)
- `enum_variant_added`: `GateKind::Question` (53-01's planned major change, expected)

No other item fails, so there is no defect to report.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Deleted `spawn_agent_and_record` and `rescan_gate_context`, and edited pipeline_outcomes.rs and preflight.rs**
- **Found during:** Task 1
- **Issue:** With the relaunch gone, both functions were dead in the non-test build, and clippy `-D warnings` failed. The plan expected `rescan_gate_context` might still have a production caller in pipeline_outcomes. Its only caller there was a test.
- **Fix:** Deleted both functions. Removed the `checkpoint_declaration` way-on check in `pipeline_outcomes`' gate-sentence test, which was the deleted function's only caller. Reworded three preflight.rs comments and one assertion message that named `spawn_agent_and_record`.
- **Files modified:** crates/devflow-cli/src/pipeline_outcomes.rs, crates/devflow-cli/src/preflight.rs (outside the plan's files_modified)
- **Commit:** 8abfc0e

**2. [Rule 1 - Bug] Validate test read the state after the abort had cleared it**
- **Found during:** Task 1 GREEN
- **Issue:** The pre-written abort answer clears `.devflow/state-N.json`, so the planned `consecutive_failures` assertion had no state to read.
- **Fix:** The test helper runs the advance on a scoped thread, reads the state while the gate is open, and then answers abort.
- **Commit:** 8abfc0e

**3. TDD note:** the control test `a_failure_without_a_declaration_has_no_checkpoint_sentence` passed at RED by design, because it pins the undeclared path that the change must leave alone. The other two tests failed at RED.

## Known Stubs

None.

## Deferred

- prompt.rs doc text still names `verify::phase_has_blocking_human_checkpoint` and `agent_result::latest_stated_outcome` in plain backticks, not intra-doc links, on `checkpoint_auto_decide_prompt`. 53-06 deletes that function. This is the backlog item pi F-12.

## Self-Check: PASSED

- FOUND: 8fef947, 8abfc0e, 61f1e9d (ancestors of HEAD)
- FOUND: crates/devflow-cli/src/pipeline_launch.rs; both snapshot files absent

---
phase: 53-the-question-channel
plan: 09
subsystem: question, ask-server, runner
tags: [mcp, question, gates, abort, backstop]
status: complete
requires:
  - phase: 53-07
    provides: the ask server's abort ending, Gates::close_if_current, the question_closed event
  - phase: 53-08
    provides: the runner-driven codex stub that asks through its registration
provides:
  - State::abort_requested (serde default, additive under #[non_exhaustive])
  - the ask server's abort branch, which commits abort_requested before it claims, closes or replies
  - advance_with's post-agent abort, run before the stage outcome is routed
  - close_question_gate (agent-exit backstop, applies a bound abort) and record_leftover_question (resume)
  - OPERATIONS.md "Question events"
affects: [53-10, 53-13, 53-14, 54]
tech-stack:
  added: []
  patterns:
    - "A second process asks the runner to act through a state field the runner reads after the agent exits"
key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/ask_server.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/ask_launch_e2e.rs
    - OPERATIONS.md
key-decisions:
  - "abort_requested is consumed by the abort removing the state file. pipeline_gate::abort's state commit is clear_state, so there is no relaunch_note-style field clear to add, and a commit after abort would fail with MissingState."
  - "The server commits abort_requested before it claims the answer, not only before it closes, so a server killed between claim and commit cannot lose the abort."
  - "On resume a leftover question gate gets question_closed{closed_on_resume}, but a bound abort is not applied: the operator's resume is the later word, and resume's existing close reports the answer it sets aside."
  - "A state that still carries abort_requested because the runner died before its advance is left as it is: the next advance of the same run applies the operator's abort. A new start builds a fresh state with None."
requirements-completed: [ASK-01, ASK-04, ASK-05]
duration: about 14 min
completed: 2026-11-19
commits: 5
plan_head_before: 0c6822470920dacf1b6f3fcf87d548e7cd4b023a
plan_head_after: b360cff6e6eed4ce8d00c2d8790f1941ec81ba63
actuals:
  tokens: 7729
  tasks: 3
  commits: 5
---

# Phase 53 Plan 09: The question abort ends the run, and the question gate backstop Summary

**`gate abort` on a question now ends the run. The `__ask` server records `State::abort_requested` before it claims the answer or closes the gate. Once the agent exits, the runner applies it with the usual `abort()`. After every agent exit the runner also closes any question gate the agent left open, and it applies an abort that was published before the server died.**

## Performance

- **Duration:** about 14 min, from 16:53Z to the last task commit at 17:07Z.
- **Tasks:** 3 of 3.
- **Files:** 5 changed, 608 insertions.

## Task Commits

1. **Task 1: `gate abort` on a question ends the run.** RED `2c4f226`, GREEN `7942001`.
2. **Task 2: The runner closes a question gate its agent left behind.** RED `e7d656e`, GREEN `9a47b46`.
3. **Task 3: Document the events, the abort path and the backstop.** `b360cff`.

## TDD Gate Compliance

- **Task 1 RED (`2c4f226`).** `abort_requested` was stubbed as `#[serde(skip)]` so that the tests compiled. Each target test then failed on its own assertion:
  - `state::tests::abort_requested_round_trips_and_defaults_to_none` failed with "abort_requested must be persisted under its own key".
  - `pipeline_launch::tests::the_abort_request_is_consumed_by_the_abort` failed on its precondition assert, "the saved state carries the request". That is the same stub at work: the field never reached disk.
  - `ask_launch_e2e::gate_abort_on_a_question_ends_the_run` failed with "the run did not abort: a gate opened after the question". The runner had routed the stub's failed result to a `stage_failure` gate.
- **Task 2 RED (`e7d656e`).** `close_question_gate` and `record_leftover_question` were no-ops, already wired into `advance_with` and `close_gates_for_resume`. Each test failed on its own assertion:
  - The agent-exit test and the kill-window test failed with "the question gate is still present".
  - The resume test failed with "one question_closed: []".
- RED evidence was checked by hand against the nextest output. `gsd_run check tdd-red-evidence` has no nextest adapter and was not run.

## Verification

- **Task 1 gates.** All four named tests pass, including 53-08's control `a_stage_agent_asks_through_its_registration_and_finishes`.
- **Task 2 gates.** All three named tests pass.
- **Task 3 gates.** `doc_check::doc_referenced_identifiers_exist_in_source` and `doc_check::source_devflow_env_vars_and_subcommands_are_documented` both pass.
- **Plan verification.**
  - `cargo nextest run -p devflow --test ask_launch_e2e`: 4 of 4 passed.
  - `cargo nextest run -p devflow --bin devflow`: 517 passed, 3 skipped.
  - `scripts/check.sh all`: exit 0 ("all OK"). The known flaky monitor test did not fail.
- **Clippy.** `cargo clippy -p devflow-core -p devflow --all-targets -- -D warnings` exited 0 before each GREEN commit.
- **semver-checks.** `--baseline-rev 0c68224` (this plan's start) with `--release-type minor` exited 0: 202 checks, 202 pass, 58 skip, "no semver update required". `State::abort_requested` reads as additive. The run against origin/develop was not repeated. 53-08 recorded 4 failures there, all from earlier plans.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The plan's "commit right after `abort` returns Ok" cannot run**
- **Found during:** Task 1.
- **Issue:** `pipeline_gate::abort` ends with `clear_state`, which removes the state file. There is no state commit to put the clear beside, and a later `update_state` would fail with `MissingState`.
- **Fix:** The removal itself consumes the field, and `pipeline_gate.rs` is unchanged. The consumption test asserts that the reload returns `MissingState` and that exactly one `workflow_aborted` is recorded. Its control (no request) routes to its `--until` stop.
- **Files modified:** none beyond the plan's.
- **Commit:** `7942001`.

**2. [Rule 1 - Bug] The resume half closes through resume's own close, not through `close_question_gate`**
- **Found during:** Task 2.
- **Issue:** Closing the question gate before `Gates::close_phase` would hide that gate's set-aside answer from resume's report lines.
- **Fix:** `close_gates_for_resume` calls `record_leftover_question` for each stage before its existing close. That call records `closed_on_resume` and does not apply a bound abort. The close and its report stay as before.
- **Commit:** `9a47b46`.

**3. [Rule 2] The server commits the abort before the claim**
- **Found during:** Task 1.
- **Issue:** The plan asks for the state commit before the close. A commit placed after `Gates::claim` would leave a kill window: once the answer is claimed, the backstop's `read_answer` no longer sees it.
- **Fix:** The server commits before the claim. If the commit fails, it logs to stderr, and the agent is still told to stop.
- **Commit:** `7942001`.

## Threat Flags

None. No new endpoint or trust boundary beyond the plan's threat model. The server's second state writer (T-53-24) goes through `workflow::update_state` with a closure that only sets the field.

## Self-Check: PASSED

- Files: state.rs, ask_server.rs, pipeline_launch.rs, ask_launch_e2e.rs and OPERATIONS.md are all modified in `0c68224..b360cff`.
- Commits: `2c4f226`, `7942001`, `e7d656e`, `9a47b46` and `b360cff` are all ancestors of HEAD.

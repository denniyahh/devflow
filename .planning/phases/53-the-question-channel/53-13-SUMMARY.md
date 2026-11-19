---
phase: 53-the-question-channel
plan: 13
subsystem: ask server, agents, monitor
tags: [ask, mcp, hold-cap, codex, idle-timeout, od-1]
status: complete
requires:
  - phase: 53-08
    provides: the claude and codex registrations and ASK_CLIENT_TIMEOUT_MARGIN_SECS
  - phase: 53-10
    provides: the last earlier edits to pipeline_launch.rs, ask_server.rs and OPERATIONS.md
provides:
  - "`__ask --hold-cap-secs`: a capped call replies `pending` with its question_id at the cap; the question stays open; a re-call by question_id continues it"
  - "T counted from a question's first call: an unheld open question is closed `timed_out` at T; EOF, signal, stop and runner-gone close unheld questions"
  - "ASK_HOLD_CAP_SECS = 900; codex registered with --hold-cap-secs 900 and tool_timeout_sec 960"
  - "StageSpec::question_wait: an open question gate holds the stream idle window for at most 960 s of continuous silence"
affects: [53-14, 53-VALIDATION]
tech-stack:
  added: []
  patterns:
    - "ask_server Worker trait: call / idle / shut_down, so the single worker ends unheld questions between calls"
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/ask_server.rs
    - crates/devflow-core/src/question.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/ask_e2e.rs
    - crates/devflow-cli/tests/common/ask_client.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-core/src/agents/codex.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - OPERATIONS.md
key-decisions:
  - "The re-call id is parsed by a new `QuestionSpec::from_call_arguments` returning `(spec, Option<question_id>)`; `from_arguments` and `QuestionError` are unchanged, and a malformed id is refused as `WrongType` on `question_id`. This keeps devflow-core additive against the plan's start commit (no new enum variant, no changed signature)."
  - "A call that reaches its cap before its own gate opens (it waits on the stage's ask lock) also replies `pending` with its question_id. The question is kept with no gate, the reply text says it is not asked yet, and T still counts from its first call."
  - "A question this server ended keeps its final reply. A re-call of that id gets the same reply again: `devflow_ask closed: <reason>` for a closed question, the answer for an answered one."
  - "Idle guard source: a planner/orchestrator decision tied to operator answer OD-1 (b) by review concern 13. It is not an operator decision. It bounds continuous silence measured from the last stream line, not the gate's age."
requirements-completed: [ASK-01, ASK-05]
duration: about 19 min
completed: 2026-11-19
commits: 3
plan_head_before: 1c6ac248dbef5bd6e96583936b19daae4ab60de0
plan_head_after: 0dcf604fafd206f5de532dc1009a1717d93234c3
actuals:
  tokens: 17675
  tasks: 3
  commits: 3
---

# Phase 53 Plan 13: devflow_ask hold cap, pending re-call and bounded idle hold Summary

When `--hold-cap-secs` is set, a held `devflow_ask` call returns `pending` with its question_id at the cap, and a re-call with that id continues the same open question. Codex is launched with a 900 s cap and a 960 s client timeout. Claude has no cap. The stream idle monitor holds its window for an open question gate only up to 960 s of continuous silence.

## Tasks

| Task | Commit | What |
|---|---|---|
| 1 | 7e1ee88 | Hold cap, `pending` result, re-call by question_id, unknown-id refusal, an unheld question closed at T from its first call, and unheld questions closed on EOF, signal, stop and runner-gone |
| 2 | 240a0e2 | `ASK_HOLD_CAP_SECS = 900`, codex args `--hold-cap-secs 900` and `tool_timeout_sec=960`, and OPERATIONS.md text on the cap, both client timeouts and the measured cost |
| 3 | 0dcf604 | `StageSpec::question_wait`, the question-gate probe in the idle arm, the runner's 960 s bound, and the idle-timeout rows in OPERATIONS.md |

## Verification

- Task 1: each of the four named `ask_e2e` tests passed on its own. The whole `ask_e2e` suite passed (23/23), as did the 4 `ask_server` unit tests and the new `question::tests::a_recall_question_id_is_returned_beside_the_spec_and_its_form_checked`. RED before the change: the capped and unknown-id tests failed, because the old binary refused `--hold-cap-secs`. The uncapped control passed both before and after the change.
- Task 2: `codex_holds_up_to_the_cap_and_its_client_waits_longer`, `only_codex_carries_a_hold_cap`, the updated `launch_args_set_tool_timeout_and_approval_before_the_prompt` (now expects 960) and `doc_check::doc_referenced_identifiers_exist_in_source` all passed. `cargo nextest run -p devflow-core --lib` passed 933/933.
- Task 3: the four new monitor tests, `idle_timeout_still_fires_when_no_background_task_is_open` and `pipeline_launch::tests::the_runner_bounds_a_question_hold_by_the_hold_cap` all passed. RED before the change: with only the field added, `idle_timeout_does_not_fire_while_the_stages_question_gate_is_open` failed.
- `scripts/check.sh all`: exit 0, "all OK". The known flaky monitor test did not fail in this run.
- Clippy `-D warnings` on both crates, all targets: clean before each commit.
- Semver: `cargo semver-checks check-release -p devflow-core --baseline-rev 1c6ac24 --release-type minor` exited 0 with "Checked 202 checks: 202 pass, 58 skip" and "no semver update required". The baseline is this plan's start commit, as the orchestrator directed; origin/develop already carries earlier plans' planned major changes. This plan's changes to devflow-core are additive: `ASK_HOLD_CAP_SECS`, `StageSpec::question_wait` (the struct is `#[non_exhaustive]`) and `QuestionSpec::from_call_arguments`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Additive question_id parse instead of changing `from_arguments`**
- **Found during:** Task 1
- **Issue:** The plan says to extend `QuestionSpec::from_arguments` so it returns the id beside the spec. That changes a pub signature in devflow-core, a semver break against this plan's start commit.
- **Fix:** Added `QuestionSpec::from_call_arguments` and left `from_arguments` unchanged. No new error variant.
- **Files modified:** crates/devflow-core/src/question.rs
- **Commit:** 7e1ee88

**2. [Rule 3 - Blocking] Test client override for the new flag**
- **Found during:** Task 1
- **Issue:** `AskOverrides` had no way to pass `--hold-cap-secs`.
- **Fix:** Added `hold_cap_secs: Option<u64>` to `AskOverrides` in `tests/common/ask_client.rs`. The file is not in the plan's `files_modified`.
- **Commit:** 7e1ee88

**3. [Rule 2 - Correctness] A call capped before its gate opens**
- **Found during:** Task 1
- **Issue:** The plan says the cap covers the ask-lock wait, but it gives no reply shape for a call that reaches its cap before its own gate opens.
- **Fix:** That call also replies `pending` with its question_id. The question is kept without a gate until a re-call, or until T ends it.
- **Commit:** 7e1ee88

Task 3's commit subject was amended to fit 72 characters, before anything else was committed.

## Known Stubs

None.

## Notes

- If the operator answers abort while no call holds a pending question, the abort is applied (`abort_requested` recorded) only when the agent re-calls. The idle worker ends unheld questions on T, run stop and runner-gone, but does not read answers. Codex re-calls at once after `pending`, so the window is one re-call. No test covers this case.
- The idle arm is checked once per idle window, so a silence past the question bound trips the timeout up to one idle window after the bound: 960 s to 1080 s in production with the 120 s idle floor.
- Not measured: whether claude keeps sending its `tool_progress` heartbeat past 900 s. If it stops, a claude hold with more than 960 s of silence is ended as a hang (plan Task 3).

## Self-Check: PASSED

- All ten modified files are present in the worktree.
- Commits 7e1ee88, 240a0e2 and 0dcf604 are ancestors of HEAD (`git merge-base --is-ancestor`).

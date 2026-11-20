---
phase: 53-the-question-channel
reviewed: 2026-11-19T00:00:00Z
depth: deep
files_reviewed: 53
files_reviewed_list:
  - .gitignore
  - .workspace-divergence
  - AGENTS.md
  - ARCHITECTURE.md
  - CLAUDE.md
  - OPERATIONS.md
  - crates/devflow-cli/src/ask_server.rs
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/ask_checkpoint_e2e.rs
  - crates/devflow-cli/tests/ask_e2e.rs
  - crates/devflow-cli/tests/ask_launch_e2e.rs
  - crates/devflow-cli/tests/common/ask_client.rs
  - crates/devflow-cli/tests/common/mod.rs
  - crates/devflow-cli/tests/failover_e2e.rs
  - crates/devflow-cli/tests/fallback_agent_config.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/start_refusals_e2e.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/antigravity.rs
  - crates/devflow-core/src/agents/claude.rs
  - crates/devflow-core/src/agents/codex.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/agents/pi.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-core/src/events.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/history.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/liveness.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/question.rs
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/verify.rs
  - devflow.toml
  - scripts/hooks/post-commit
  - skills-lock.json
findings:
  critical: 0
  warning: 6
  info: 5
  total: 11
  necessity: 2
status: issues_found
---

# Phase 53: Code Review Report

**Reviewed:** 2026-11-19
**Depth:** deep
**Files Reviewed:** 53
**Status:** issues_found

## Summary

I reviewed the diff from `be5e4b82` to `ed4712bc`, focusing on the question channel. That covers the `devflow __ask` stdio MCP server (`ask_server.rs`), `question.rs`, the question-gate additions in `gates.rs`, `gate answer` and the abort handling in `commands.rs` and `main.rs`, the runner backstop and abort application in `pipeline_launch.rs`, the hold-cap and idle-window coupling in `monitor.rs`, the claude, codex and pi registrations, and the removal of the checkpoint-declaration and auto-decide route. I traced calls across these modules: ask server to `Gates`, `workflow::update_state` and `liveness`; runner `advance` to `close_question_gate` and `abort_requested`; prompt contract to server reply prefixes; notify hook to `run_notify_command_to`.

Nothing was found that blocks shipping. Payload validation, option-id checks, gate-id binding, terminal sanitising of agent-authored text (`gate show` and `history` both route through `render_gate_context`), and keeping stdout clean in `__ask` (tracing goes to stderr, and the hook's stdout is `/dev/null`) all hold up under tracing.

The defects are in the edge paths:
- A committed `abort_requested` survives `resume`.
- Run-binding refusals use the prefix that the prompt tells the agent to retry on.
- Failed endings record no `question_closed`.
- Two closers can each record a `question_closed` for the same question.
- The notify-hook group kill's safety claim does not hold once `try_wait` has reaped the shell.

`scripts/hooks/post-commit`, `devflow.toml` and `.workspace-divergence` are workspace-branch files listed as `only`/built-in in `.workspace-divergence`, not phase-53 code. I only skimmed them.

## Warnings

### WR-01: A committed `abort_requested` survives `resume`, so a resumed run is aborted at its first advance

**File:** `crates/devflow-cli/src/pipeline_launch.rs:2461-2474` (`record_leftover_question`), `:2490+` (`resume_loaded`), `:2764-2767`
**Issue:** The `__ask` server commits `state.abort_requested` before it claims the answer (`ask_server.rs:1237-1249`). The runner applies the abort only inside `advance` (`pipeline_launch.rs:2764`). Suppose the runner dies after the server commits and before `advance` runs (killed, OOM, host restart), and the operator then runs `devflow resume`. `resume_loaded` neither reads nor clears `abort_requested`; the only references are `pipeline_launch.rs:2429, 2764` and `state.rs`. The resumed stage runs to completion, and then its first `advance` aborts the run and removes the state file. The doc comment on `record_leftover_question` says "the operator's `resume` is the later word". The code honours that only for an uncommitted abort file. A committed one wins later, after a whole wasted stage.
**Fix:** In `resume_loaded`, inside the same `update_state` commit that clears `stopped`, either refuse the resume or clear the request with a never-silent line:
```rust
if let Some(note) = s.abort_requested.take() {
    println!("phase {phase}: cleared the abort requested at a question ({note}); resume is the later word");
    events::emit(project_root, phase, "abort_request_cleared_on_resume", json!({ "note": note }));
}
```
Add a test: commit `abort_requested`, resume, advance, and assert that the run is not aborted. As a control, run with no resume and assert that it is aborted.

### WR-02: Run-binding refusals use the `devflow_ask refused:` prefix, which the prompt tells the agent to "fix the payload and call again"

**File:** `crates/devflow-cli/src/ask_server.rs:816-842`, `crates/devflow-core/src/prompt.rs:78-79`
**Issue:** `check_run_binding` refuses a call with `devflow_ask refused: …` when the runner lock is not live, the state is missing, `started_at` differs or the stage differs. The prompt says: "On a result starting `devflow_ask refused:`, fix the payload and call again." None of these causes can be fixed by changing the payload. An orphaned agent whose runner died is therefore told to retry, gets refused again at once, and loops, spending model turns until it gives up on its own. The same applies to a re-call after the runner went away: `check_run_binding` runs before the open question is looked up at `:631`. That call gets `refused` rather than the `closed: runner_gone` result that `idle()` later records for that question.
**Fix:** Return binding failures as a closed result, which the prompt already maps to "end the stage". For example, map them to `closed_error(Ending::RunnerGone, …)` or `Ending::RunStopped`. Keep `refused:` for `QuestionSpec` validation and the unknown-`question_id` case only. Add a test that a call with the runner lock unheld returns text starting `devflow_ask closed:`.

### WR-03: A question that ends `Failed` after its gate opened records no `question_closed`, so history shows it as open forever

**File:** `crates/devflow-cli/src/ask_server.rs:922-928` (plus `:891-897` and `:1180-1220`)
**Issue:** The module doc says (`:14-15`): "Every ending other than an answer closes the call's own gate … and records `question_closed`." `close_gate` does close the gate for every result, but it emits nothing for `Unanswered::Failed(_)`. `Failed` can happen after `question_asked` has been emitted:
- `await_bound_answer` fails to set aside an unbound answer (`:1166`).
- `read_question_choice` errors (`:1187`).
- `Claim::AlreadyConsumed` or a claim error (`:1197-1206`).
- An approved answer carries no choice (`:1216`).

In each case the gate is gone, but `history` (`history.rs`, `resolved_question_ids`) never sees a resolution and prints `answer: not yet answered` for that question permanently.
**Fix:** Emit `question_closed` with a distinct reason when a gate existed:
```rust
Err(Unanswered::Failed(why)) => emit_question_closed(args, spec, &gate_id, Ending::Failed /* reason "failed" */, None),
```
This needs a new `Ending::Failed` variant, or a reason string passed through. Add a test that drives the no-choice answer and asserts that a `question_closed` event is recorded.

### WR-04: Two closers can each record `question_closed` for one question (server shutdown vs. runner backstop)

**File:** `crates/devflow-cli/src/ask_server.rs:917-926`, `crates/devflow-cli/src/pipeline_launch.rs:2420-2453`
**Issue:** When the agent exits, its `__ask` child sees end of stdin. `shut_down` then calls `close_gate`, which emits `question_closed{client_disconnected}` whether `close_if_current` returned `Some` (it closed the gate) or `None` (someone else already had). At the same moment, the runner's `advance` calls `close_question_gate`. That function reads the gate file without the gate lock, emits `question_closed{agent_exited}`, and only then calls `close_if_current`. If the runner reads the gate before the server's close, both events are written. `history.rs` shows the later one as "ignored … already answered or closed". That masks the record rather than preventing it, and the event stream still holds two contradictory endings, or an answer and a close.
**Fix:** In both places, emit only when this closer actually closed the gate:
```rust
match Gates::close_if_current(root, phase, stage, &gate_id) {
    Ok(Some(_)) => emit_question_closed(...),
    Ok(None) => {} // another closer ended it
    Err(e) => eprintln!(...),
}
```
In `close_question_gate`, do the `close_if_current` before the emit. For the abort branch, keep the `abort_requested` commit before the close.

### WR-05: `kill_hook_group` sends `killpg` after `try_wait` may already have reaped the shell, so its SAFETY claim is false

**File:** `crates/devflow-core/src/gates.rs:2214-2259`
**Issue:** `await_hook` calls `child.try_wait()`. When that returns `Some(status)`, the shell has been reaped, but the loop keeps waiting for stderr's end of file. If the bound then runs out (`HookEnd::TimedOut`), `kill_hook_group` calls `killpg(pgid, SIGKILL)`, and its SAFETY comment says "the shell is not reaped yet, so the id cannot name another group". On this path that claim is false. If every member of the group has left it, for example a backgrounded child that called `setsid` and still holds stderr, the pid/pgid number is free. The kernel may then reuse it for an unrelated process that becomes a session or group leader, and `killpg` sends it SIGKILL. The window is narrow, but this is a hard kill of an arbitrary process.
**Fix:** Track whether the shell has been reaped. Only `killpg` when it has not, or when the group is known to still exist. Alternatively, use `waitid(P_PID, …, WNOWAIT|WEXITED)` instead of `try_wait`, so the leader stays a zombie that pins the pgid until after the kill. Then reap.

### WR-06: Impossible-state guard: "the question has no open gate" in `hold_question`

**File:** `crates/devflow-cli/src/ask_server.rs:886-890`
**Category:** necessity
**Issue:** The `let Some(gate_id) = question.gate.as_ref()… else { Failed("the question has no open gate") }` branch cannot run. The block just above (`:854-885`) either returns early on every path where `question.gate` stays `None`, or assigns `question.gate = Some(OpenGate { … })`. No code between them clears it. The branch defends a state this function rules out.
**Fix:** Restructure so the gate id comes from the open step. For example, have the `if question.gate.is_none()` block yield `gate_id`, or use `question.gate.get_or_insert_with` through a fallible helper. Then drop the dead `Failed` arm.

## Info

### IN-01: Run-on whitespace inside an error message

**File:** `crates/devflow-cli/src/ask_server.rs:1245`
**Issue:** The literal reads `"…in the run state,              so the runner…"`. A line break was joined without a `\` continuation, so the operator's stderr line contains 14 spaces.
**Fix:** End the first half with `state, \` and continue the string on the next line.

### IN-02: Stale docs describe the removed checkpoint routing; `declares_blocking_human_checkpoint` has no production caller

**File:** `crates/devflow-core/src/prompt.rs:89-99`, `crates/devflow-core/src/verify.rs:341-342`, `crates/devflow-core/src/agent_result.rs:63-75`
**Issue:**
- `prompt.rs` says the `checkpoint` field is one "which the pipeline reads through `declares_blocking_human_checkpoint`" and that "the pipeline ignores the field elsewhere". After this phase, nothing outside tests calls that method; the base callers at `pipeline_launch.rs:2854` and `agent_result.rs:855` were removed. A declared checkpoint is now an ordinary `Failed` result, routed to `GateReview` by `outcome_policy.rs:59`.
- `verify.rs:342` still says "a blocking-human match arms the agent's auto-decide route", and that route is gone.
**Fix:** Reword both doc comments to say that the field is informational and that a declared checkpoint routes as a failure to the review gate. Decide whether the `pub` method stays; removing it is a semver change in `devflow-core`.

### IN-03: `take_ask_lock` checks remaining time before its first claim, so a call whose cap has already passed never tries the lock

**File:** `crates/devflow-cli/src/ask_server.rs:948-955`
**Issue:** When `watch.deadline` has already passed on entry, the call returns `NotAsked(TimedOut)` without one `claim_ask_lock` attempt. That happens with `--hold-cap-secs 0`, or when a re-call sat in the queue past its cap, because the cap counts from arrival. With a capped call this becomes `Pending` every time, a livelock for cap 0. The drivers pass 900 today, so this is latent.
**Fix:** Try the claim once before the deadline check. You can also refuse `--hold-cap-secs 0` at parse time.

### IN-04: Impossible-state guard: the third arm of `answer_choice`

**File:** `crates/devflow-cli/src/main.rs:647-661`
**Category:** necessity
**Issue:** The `ArgGroup` `choice` at `main.rs:328-333` is `.required(true).multiple(false)` over `option` and `text`, so clap never hands over both or neither. The test at `main.rs:681-700` proves both refusals. The fallback arm only guards a future parser change, and its own comment says so.
**Fix:** Optional: keep the arm as a cheap defence, or replace it with `unreachable!` and a pointer to the group. This rests on judgment.

### IN-05: The sweep's "question gate" message claims a timeout that does not exist when the server is dead

**File:** `crates/devflow-cli/src/commands.rs:4068-4078`
**Issue:** The sweep leaves every question gate alone with the message "it ends at its own timeout, by gate answer or by gate abort". That check comes before the waiter check. When the `__ask` server was SIGKILLed and no runner `advance` followed, nothing will time the gate out, and the message is false. Only `resume` or `gate abort` clears it.
**Fix:** Read `waiter_is_live` first and choose the wording from it. With no live waiter, point at `devflow resume <phase>` or `devflow gate abort`.

---

_Reviewed: 2026-11-19_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_

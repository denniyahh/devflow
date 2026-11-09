---
phase: 50-addressable-monitor-liveness
reviewed: 2026-10-21T22:24:02Z
depth: standard
review_kind: incremental
diff_base: 6b120b2
files_reviewed: 7
files_reviewed_list:
  - ARCHITECTURE.md
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/tests/supervisor_e2e.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/supervisor/runtime.rs
  - doc-check-allowlist.toml
findings:
  critical: 0
  warning: 1
  info: 3
  total: 4
status: issues_found
---

# Phase 50: Code Review Report (incremental re-review)

**Reviewed:** 2026-10-21T22:24:02Z
**Depth:** standard
**Files Reviewed:** 7
**Status:** issues_found

## Summary

This review covers only `git diff 6b120b2..HEAD` for the seven files above. That diff holds the CR-01 fix (`8946d7d`,
`373f1e6`) and the WR-03 gap closure (50-26: `eabb5f4`, `07e237b`, `493156b`, `5b1c417`, `60b1538`). The surrounding
code it depends on was read too: `wait_for_gate_response`, `poll_response_cancellable`, `handle_ship_outcome`,
`run_supervised_stage` and `wait_for_exit_without_reaping`. Findings already in 50-REVIEW.md and its deferrals
(WR-01, WR-02, IN-01..IN-03), and the stated limits in 50-26-PLAN.md and 50-26-SUMMARY.md, are not reported again.

**CR-01: the reported window is closed.** Safe point 5 runs before the `--yes-ship` auto-response is written. Safe
point 6 runs before the first pass of the after-ship hooks. Both clear the gate, record the stop and unwind with an
error. The stop token is marked as stopped, so `finish()` answers the stop's caller `stopped`. Each safe point has a
test that holds the advance at a named point and a no-stop control that does ship. The stop tests check that the
feature branch is still there, so they would catch a real merge. One narrower instance of the same class is left, in
the finalization-retry loop (WR-01 below).

**WR-03: closed.** The stream loop now also ends on an `AgentExited` event, sent by a `waitid(WNOWAIT)` watch thread.
The thread join is bounded by `READER_DRAIN_WAIT`, the capture is taken from the reader at the bound, and unfinished
threads are detached. The reader keeps draining the pipe, so a leftover process is not hit with SIGPIPE while the
supervisor lives. No unbounded wait is left between the agent's exit and the advance, and nothing else joins the
detached handles. The watch thread treats ECHILD after the caller's reap as "exited", which is correct. A reused pid
is not a realistic risk in the few microseconds involved.

## Warnings

### WR-01: The finalization-retry loop re-runs the merge, tag and push after an accepted stop

**File:** `crates/devflow-cli/src/pipeline_gate.rs:243-286` (safe point 6 sits before the `loop` at 251; the
`GateAction::Advance` arm at 277-279 loops straight back into `run_checkout_hooks` at 252)

**Issue:** Safe point 6 is checked once, before the first `run_checkout_hooks`. If those hooks fail, the loop reopens
a retry gate. When the operator approves it, the loop runs the after-ship hooks again (merge, version bump, tag,
push) without checking the stop token.

`poll_response_cancellable` reads the response file before it calls `cancel()`, and it polls in 1 s slices
(`gates.rs:339-347`). Suppose a `devflow stop` is accepted (owner `gate_wait`/`advancing`, so the stop is only
flagged) and the operator's retry approval lands in the same ≤1 s slice. The next poll returns `Answered`, the arm
returns `Advance`, and the irreversible hooks run under a stop the supervisor has already accepted. The run never
ends as stopped. `finish()` sees `ended_stopped == false`, and the stop's caller gets the misleading result
50-REVIEW CR-01 described. This breaks `runtime.rs:20-24` ("a stop that wins the race always ends the run as
stopped"). It is the "a response on disk beats a flagged stop" shape CR-01 named, at the one gate the fix did not
cover.

**Window, stated plainly:** the stop and the retry approval must both arrive within the same poll slice, or the stop
must arrive in the microseconds between `Answered` and the loop's next `run_checkout_hooks`. That is narrow. It
needs a failed first finalization plus two operator actions that contradict each other, and the effect cannot be
undone. A stop that lands while the hooks are running is WR-01's deferred "all of Ship finalization" stretch, not
this finding.

**Fix:** check the safe point at the top of every pass, not once before the loop:

```rust
loop {
    if crate::pipeline_launch::stop_requested_at_safe_point() {
        let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
        state.gate_pending = false;
        crate::pipeline_launch::record_advance_stop(state, 6)?;
        return Err(CliError::Message(format!(
            "finalization did not run: {ADVANCE_STOP_REASON}"
        )));
    }
    if run_checkout_hooks(project_root, state, &hooks::hooks_after_ship(), Stage::Ship) {
        break;
    }
    // … retry gate unchanged …
}
```

Then add a hold-point test: make the after-ship hooks fail once, answer the retry gate, hold before the second
pass, send a socket stop, release, and assert `advance_stopped` with `safe_point: 6` and that the feature branch
still exists. Add a no-stop control that ships.

## Info

### IN-01: Safe point 5 runs after the gate is written and the operator has been notified

**File:** `crates/devflow-cli/src/pipeline_gate.rs:387-451`

**Issue:** Safe point 5 is checked after `save_state(gate_pending = true)`, `Gates::write_gate`, the `gate_fired`
event, the gate-notify command and the `notify_fired` event. A `--yes-ship` run stopped there has already sent the
operator's notifier (for example Hermes) a "Ship gate awaiting approval" message. The gate is withdrawn a moment
later. Any answer the notifier sends is refused with `NoOpenGate`, and the event log shows a fired gate with no
`gate_resolved`. Nothing irreversible happens, but the notification is misleading.

**Fix:** move the check to the top of `run_gate_with_timeout`, guarded by `auto_response.is_some()`, before the
first `save_state`. Nothing has been written at that point, so only `record_advance_stop(state, 5)` is needed.

### IN-02: ARCHITECTURE.md's description of the stream wait does not match the code

**File:** `ARCHITECTURE.md:280-283`; `crates/devflow-core/src/monitor.rs:983-1019`, `1028-1048`

**Issue:** The new text says the supervisor "stops reading when the agent exits" and "waits at most 3 s for its pipe
threads, closes the capture file". The code differs in two ways:

- The *loop* stops at the agent's exit, but the reader thread keeps reading, and keeps teeing until the capture is
  taken. It then discards lines until EOF, on purpose, so the pipe stays drained.
- `close_shared_capture` retries `try_lock` for up to one more `READER_DRAIN_WAIT`, so the bound is up to 6 s, not
  3 s.

**Fix:** reword it to say the stream loop ends at the agent's exit, the threads are waited for up to 3 s, the capture
is closed within up to 3 s more, and the reader goes on draining the pipe (discarding what it reads) until the
process exits.

### IN-03: Safe point 6's comment says "a resumed run asks again", which is false for `--yes-ship` runs

**File:** `crates/devflow-cli/src/pipeline_gate.rs:238-242`; `crates/devflow-cli/src/pipeline_outcomes.rs:839-843`

**Issue:** `state.yes_ship` is persisted, and neither `record_advance_stop` nor `resume` clears it (`commands.rs:341`
is the only assignment). When a `--yes-ship` run stopped at safe point 5 or 6 is resumed and reaches the Ship gate
again, `handle_ship_outcome` auto-approves again. It does not ask. An operator who stopped the run to prevent the
release could read this comment and resume expecting a prompt. The fix's behaviour is correct; the comment claims too
much.

**Fix:** reword it to "the consumed approval is cleared, so a resumed run reaches the Ship gate again (auto-answered
again while `yes_ship` is set)". Alternatively, decide whether a stop should clear `yes_ship`. That is a behaviour
choice for the operator, not part of this review.

## What this review does not establish

- No tests were run. The findings come from reading the code. The CR-01 and 50-26 tests' pass and fail behaviour was
  not reproduced here.
- WR-01's window was found by reading `poll_response_cancellable` and the loop. No test or probe exercised it.
- The 50-26 test fixtures (E1-E3, A-G) were read for reliability, not line-audited. Leftover `sleep` processes when a
  fixture panics before its holder record exists were not checked.
- Lines outside the diff were read only where the diff depends on them. That includes `resume` beyond its `yes_ship`
  handling.

---

_Reviewed: 2026-10-21T22:24:02Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

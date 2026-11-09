---
phase: 50-addressable-monitor-liveness
reviewed: 2026-10-20T00:00:00Z
depth: standard
files_reviewed: 53
files_reviewed_list:
  - ARCHITECTURE.md
  - Cargo.lock
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/auto_chain_flag_e2e.rs
  - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
  - crates/devflow-cli/tests/common/mod.rs
  - crates/devflow-cli/tests/doctor_antigravity.rs
  - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
  - crates/devflow-cli/tests/gate_sweep_e2e.rs
  - crates/devflow-cli/tests/gate_wedge_e2e.rs
  - crates/devflow-cli/tests/help_snapshot.rs
  - crates/devflow-cli/tests/hermetic_spawn_sites.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/reap_strays_e2e.rs
  - crates/devflow-cli/tests/recover_clean_e2e.rs
  - crates/devflow-cli/tests/release_check.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-cli/tests/start_lock_e2e.rs
  - crates/devflow-cli/tests/start_reachability_e2e.rs
  - crates/devflow-cli/tests/stop_e2e.rs
  - crates/devflow-cli/tests/supervisor_e2e.rs
  - crates/devflow-cli/tests/test_home_isolation.rs
  - crates/devflow-cli/tests/yes_ship_config.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/lock.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/registry.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/supervisor/backstop.rs
  - crates/devflow-core/src/supervisor/protocol.rs
  - crates/devflow-core/src/supervisor.rs
  - crates/devflow-core/src/supervisor/runtime.rs
  - crates/devflow-core/src/test_support.rs
  - crates/devflow-core/tests/devflow_dir_gitignore.rs
  - crates/devflow-core/tests/monitor_e2e.rs
  - doc-check-allowlist.toml
  - docs/diagrams/agent-lifecycle.md
  - docs/guides/unattended-mode.md
  - OPERATIONS.md
findings:
  critical: 1
  warning: 3
  info: 3
  total: 7
status: issues_found
---

# Phase 50: Code Review Report

**Reviewed:** 2026-10-20
**Depth:** standard
**Files Reviewed:** 53 (see the coverage note below)
**Status:** issues_found

## Summary

The review covered the phase diff `4b33012..HEAD`. These parts were read in full: the production halves of
`supervisor.rs`, `supervisor/{protocol,runtime,backstop}.rs` and `monitor.rs`, the supervisor body and advance path in
`pipeline_launch.rs` (`run_monitor`, safe points, publish, `advance_with`), the gate wait in `pipeline_gate.rs`, and,
in `commands.rs`, the `stop` family, the start/resume launch guard, `recover --clean` and the doctor socket sweep.
`lock.rs`, `state.rs`, `gates.rs`, `agent.rs` and `recover.rs` were reviewed through their diffs plus the surrounding
callers. Test files, `doc_check.rs` and the docs were skimmed for test reliability and for claims that contradict the
code. They were not line-audited, so finding nothing there does not mean they are correct.

Excluded as already recorded: the 2026-10-19 triage (fixed and rejected items), `deferred-items.md` (including the
unlistable-`sock/` stop sweep and the flaky `stop_e2e` test), backlog 999.164 (doctor probe-error skip, ping timeouts
during the kill grace, `.part` leaks, connection flooding, members left after a normal completion), backlog 999.165,
the T-50-90/T-50-91 launcher-window gap, and every accepted risk in `50-SECURITY.md`. The `State::monitor_pid` removal
is a semver break, but 50-CONTEXT R-9 plans it for 3.0.0, so it is not reported.

The main problem: the supervisor's own stated invariant is "a stop that wins the race always ends the run as stopped"
(`runtime.rs:20-24`). The advance path breaks it. A stop accepted while the supervisor is `Advancing` is checked only
at four safe points, and none of them sits before Ship's merge when `--yes-ship` pre-writes the approval. The CLI's
stop messaging also turns an accepted, pending stop into "nothing was signalled or recorded".

## Critical Issues

### CR-01: A stop accepted during a Ship advance does not prevent the `--yes-ship` merge and release

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:838-852`, `crates/devflow-cli/src/pipeline_gate.rs:423-438`,
`crates/devflow-core/src/gates.rs:338-345`, `crates/devflow-cli/src/pipeline_launch.rs:1113-1133`

**Issue:** When the Ship agent completes, the supervisor's advance passes safe point 1 (`pipeline_launch.rs:2472`),
evaluates the result, then calls `handle_ship_outcome`. With `state.yes_ship`, `run_gate_with_timeout` writes the
auto-response *before* it calls `wait_for_gate_response` (`pipeline_gate.rs:423-431`). `poll_response_cancellable`
reads the response file before it consults `cancel` ("a response already on disk wins over a cancel",
`gates.rs:338-345`), so the wait returns `Answered` at once and `finish_workflow` runs the after-ship hooks: the merge,
version bump, tag and push.

A `devflow stop` or SIGTERM that lands between safe point 1 and that point is answered `accepted` (owner
`advancing`) and sets `stop_requested`. Nothing reads the flag again before the irreversible step. No safe point
exists in `handle_ship_outcome`, the `Answered` arm (`pipeline_gate.rs:438-464`) or `finish_workflow`
(`pipeline_gate.rs:231-237`).

The operator then sees one of two results, both wrong:
- If finalization takes longer than 22 s (`shutdown_final_timeout`), the stop fails with "did not answer the
  shutdown … nothing was signalled or recorded", while the merge proceeds.
- If finalization is quicker, `finish()` answers `handed_off` (`ended_stopped` is false), `stop` re-locates, finds no
  state (`finish_workflow` clears it) and prints "no persisted state … already stopped", exit 0, for a phase that was
  just merged and released.

This contradicts `runtime.rs:20-24` and OPERATIONS.md:149-151 ("does what `devflow stop`'s ALIVE arm does … no next
stage is launched, and `stopped` is recorded").

**Window, stated plainly:** the stop must land after the advance takes the lock and before the auto-response is read.
That covers `evaluate_agent_result` plus the gate notify command, usually seconds. Nothing in the diff tests it. The
same "pre-written response beats a flagged stop" shape applies to any gate whose response is already on disk when the
wait starts.

**Fix:** take a safe point before acting on a pre-authorized response, and once more before finalization:

```rust
// pipeline_gate.rs, run_gate_with_timeout, before writing `auto_response`:
if crate::pipeline_launch::stop_requested_at_safe_point() {   // wraps with_stop_token(StopToken::safe_point)
    let _ = Gates::cleanup(project_root, state.phase, stage);
    state.gate_pending = false;
    crate::pipeline_launch::record_advance_stop(state, 5)?;
    return Err(CliError::Message(format!(
        "the gate for stage {stage} was not auto-answered: {ADVANCE_STOP_REASON}"
    )));
}

// pipeline_gate.rs, finish_workflow_with_gate_timeout, first statement:
if crate::pipeline_launch::stop_requested_at_safe_point() {
    crate::pipeline_launch::record_advance_stop(state, 6)?;
    return Err(CliError::Message(ADVANCE_STOP_REASON.to_string()));
}
```

Add an e2e test that holds the supervisor after safe point 1 (a `test_hold` point), sends a socket stop, releases it
with `yes_ship = true`, and asserts that no ship hook ran and that `stop` reports `stopped`. Include a no-stop control
that does ship.

## Warnings

### WR-01: `stop` reports "nothing was signalled or recorded" for a stop the supervisor accepted and will still carry out

**File:** `crates/devflow-cli/src/commands.rs:2477-2527` (and `2186-2231` for `--launch`), `commands.rs:2256-2260`;
`crates/devflow-cli/src/pipeline_launch.rs:2444`, `pipeline_gate.rs:92`, `pipeline_launch.rs:163`; `OPERATIONS.md:43`

**Issue:** A shutdown that reaches an `Advancing` supervisor is flagged (`FlagForSafePoint`) and honoured only at
safe points 1 to 4. Several stretches with no safe point can outlast the client's 22 s final-line wait:
- `advance_with`'s lock wait, up to `ADVANCE_LOCK_WAIT` (600 s), which runs before safe point 1;
- `transition`'s operator checkout hooks (`pipeline_gate.rs:92`);
- preflight;
- the first-launch delivery canary (`pipeline_launch.rs:163`);
- all of Ship finalization.

When the final line times out, `shutdown_supervisor` probes again, sees ALIVE, and returns "did not answer the
shutdown (…); nothing was signalled or recorded". The stop is in fact pending and will be recorded at the next safe
point. An operator told that nothing happened is likely to escalate with `kill -9` (which leaves a STALE socket and
possibly a refused leader-gone group) or to run `stop` again.

OPERATIONS.md:43 has the same gap: "ALIVE → a `shutdown` request: the supervisor ends the agent's process group …
and runs no advance". That holds only for `AgentRunning`.

**Fix:** have `request_shutdown` return the `accepted` line's `owner`. When the final line times out after
`accepted` with owner `advancing` or `gate_wait`, report "the supervisor accepted the stop and will stop at its next
safe point; check `devflow status`" and exit with a distinct non-zero code, instead of "nothing was signalled". Make
the advance lock wait cancellable: poll `lock::acquire` in short slices and check `token.stop_requested()` between
them. Correct the OPERATIONS.md ALIVE-arm sentence to describe the flagged case.

### WR-02: Implicit `recover --clean` ignores the supervisor socket, though the docs say it probes it

**File:** `crates/devflow-cli/src/commands.rs:3797-3798`, `crates/devflow-core/src/recover.rs:186-205`,
`recover.rs:420-437`, `OPERATIONS.md:74-78`

**Issue:** The triage fix (`3c0aeef`/`b431fba`) put `refuse_a_live_supervisor` in front of `recover --clean --phase N`
only (`commands.rs:3760`). The no-phase sweep still calls `recover::clean_report` directly. It keeps a phase only for
a live agent pid, a state younger than 24 h by `started_at` (the phase's start, not its last activity), or a held
per-phase lock. A supervised run that started more than 24 h ago is deleted whenever its supervisor is alive and
holds neither the agent nor the lock. That happens in the stretch after the agent exits and before `advance_with`
takes the lock, which WR-03 can make unbounded. It also happens for a STALE supervisor whose agent leader has exited
while group members live (a group the backstop itself refuses to call gone).

OPERATIONS.md:76-77 states that `recover --clean` probes the recorded socket, and the sweep does not.

**Fix:** in the `None` arm, run the same `possibly_live_sockets` check per listed phase before `clean_report` deletes
it, and skip phases that may be live with a warning naming `devflow stop --phase N`. Alternatively, pass a "keep these
phases" set into `clean_report`. Add a test: a state more than 24 h old with an ALIVE recorded socket, no lock and no
agent pid file must be kept.

### WR-03: A stream supervisor whose agent completed can hang forever in `threads.join()`, and no DevFlow verb can end it

**File:** `crates/devflow-core/src/monitor.rs:1093-1099`, `monitor.rs:1340-1354`,
`crates/devflow-core/src/supervisor/runtime.rs:256-262`

**Issue:** After the close rule releases stdin, a quiet stream ends `drive_stream_io` via the `close_signalled` timeout
branch while the reader thread is still blocked on the agent's stdout. On `Completed`, `observe_exit` reaps the leader
without signalling the group, unlike the stop path. `run_supervised_stage` then joins the reader. If any process still
holds the write end of the agent's stdout (a group member that inherited fd 1), the join never returns.

The supervisor then stays ALIVE with owner `advancing` forever and never advances:
- `devflow stop` is flagged at a safe point that is never reached, so it times out (WR-01's message).
- Killing the supervisor by hand gives STALE, and the backstop answers `RefuseLeaderGone`, because the leader was
  already reaped.

The join itself is not new (the 2.x pipe-owning monitor joined the reader too), and its likelihood depends on an
agent child inheriting fd 1. Claude's tool shells normally use their own pipes. What is new is that this body is now
the only supervisor, and the stop and backstop paths built in this phase cannot recover from the hang. Backlog
999.164 item 5 records the members left behind, not this consequence.

**Fix:** do not block the run's outcome on the reader for a `Completed` run. Give the join a bound (for example, the
reader sends on a channel at EOF, and the supervisor waits up to `TERMINATE_VERIFY_WAIT` then detaches the thread),
or end the group's remaining members inside `observe_exit` while the zombie leader still pins the pgid, as the stop
path already does. Test it with an agent that starts `sleep 300 &` holding stdout and emits the result marker. The
supervisor must reach the advance.

## Info

### IN-01: `poll_response_cancellable` keeps a backoff that can never take effect

**File:** `crates/devflow-core/src/gates.rs:331-350`
**Issue:** `backoff` starts at 1 s and every sleep is `min(slice = 1 s)`, so `backoff`/`cap` and the doubling are dead.
A supervised gate now polls the response file every second for its whole multi-day timeout, which `poll_response`'s
60 s cap was meant to avoid. The doc ("the same loop") hides this.
**Fix:** drop `backoff`/`cap` and say it polls every second, or sleep `min(backoff, deadline - waited)` in 1 s slices
and check `cancel` between slices.

### IN-02: The control thread spins on a persistent `accept` error

**File:** `crates/devflow-core/src/supervisor/runtime.rs:711-723`, `runtime.rs:694-705`
**Issue:** On an `accept` error other than `WouldBlock`/`Interrupted` (for example `EMFILE`), `accept_all` returns,
the pending connection stays queued, and the level-triggered `poll` returns `POLLIN` at once. The thread busy-loops,
logging a warning on each pass, until the condition clears.
**Fix:** on a non-transient accept error, sleep one `tick` before the next `poll`, or rate-limit the warning.

### IN-03: The stop success line claims "agent group ended" for stops honoured at a safe point

**File:** `crates/devflow-cli/src/commands.rs:2199`, `commands.rs:2492`
**Issue:** `ShutdownStopped` carries no indication of where the stop happened, so a stop flagged during a gate wait
or an advance, where no group was signalled because the agent had already exited, prints "agent group ended, exit
code 143".
**Fix:** add the stopping owner phase (or a `group_signalled: bool`) to the final `stopped` line, and word the message
from it.

---

_Reviewed: 2026-10-20_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

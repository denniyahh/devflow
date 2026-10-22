# Phase 50 adversarial review — Addressable Monitor Liveness

I read the snapshot's real files (not just the diff). Findings are ordered by severity; every line number is from the snapshot at `…/scratchpad/review/snapshot`.

---

## 1. HIGH — `stop` during a `start`/`resume` launch kills the launcher and leaves the already-spawned supervisor unreachable and unrecorded

**Where:** `crates/devflow-cli/src/pipeline_launch.rs:1599` and `:1907`, `:1737`; `crates/devflow-cli/src/commands.rs:2293-2298`, `:2791-2797`

```rust
// pipeline_launch.rs:1599 (spawn_agent_and_record)
    workflow::save_state(state)?;          // state.supervisor is still None
// ... then, up to PUBLISH_WAIT (10 s) later:
    let published = monitor::spawn_monitor(state, program, args, extra_env, launch, &ctx)  // :1907
// and only then:
    state.supervisor = Some(record.clone());                                              // :1737
```
```rust
// commands.rs:2293 (locate_phase_socket)
        Ok(state) => {
            return Ok(state.supervisor.map(|record| LocatedSocket {
// commands.rs:2793 (stop_via_lock)
            "stop: no live supervisor socket and no lock held for phase {phase} — nothing is \
             running"
```

**Scenario (CONFIRMED).** `devflow start` saves the state (supervisor `None`), holds the per-phase lock (`commands.rs:369`) and spends up to `PUBLISH_WAIT` in `spawn_monitor`. If the operator runs `devflow stop --phase N` in that window:
1. `stop_via_socket` → `locate_phase_socket`: the state *loads*, `supervisor` is `None`, so it returns `Ok(None)` and never consults `sweep_for_in`. The socket sweep is only used when the state **fails to load** (`Err(_) =>` branch).
2. Fall-through `stop_via_lock` finds the live lock holder — `devflow start` itself — confirms its identity, and sends SIGTERM (`agent::terminate`, `agent.rs:86-91` has no escalation).
3. `start` dies; the `__monitor` it already spawned (own process group, detached) keeps starting, publishes its socket, spawns the agent and runs it. `publish_record` never runs, so **no state file ever names the supervisor**. `persist_stopped_state` then writes `stopped = true` and `stop` exits 0.

The agent keeps writing/committing for however long the stage takes; its later `advance` refuses because `state.stopped` is now true (`pipeline_launch.rs:2468-2480`), so the whole stage is burned. `devflow stop` again reports “nothing is running” a second time (`locate_phase_socket` still sees `supervisor: None`). Only `devflow doctor`'s orphan sweep + `stop --launch <id>` can recover it.

This is the same shape as the launch-guard gap the code documents as “accepted until Phase 50.1” (`commands.rs:3250-3256`), but `stop` — the verb D-04 says always ends the run — has no such admission and no sweep fallback. A `supervisor: None` (or `Gone`) state that still has a `.sock` for this root+phase should make `stop` sweep, not fall through to signalling the launcher.

---

## 2. MEDIUM — `recover --clean --phase N` treats only ALIVE as live: a probe error (fail-closed everywhere else) lets it delete the phase state

**Where:** `crates/devflow-cli/src/commands.rs:3681-3693`, contrast `:3345`

```rust
    let is_alive = |liveness: &Result<supervisor::Liveness, supervisor::ProbeError>| {
        matches!(liveness, Ok(supervisor::Liveness::Alive(_)))
    };
    let alive = match workflow::load_state(project_root, phase) {
        Ok(state) => state.supervisor.is_some_and(|record| is_alive(&supervisor::probe(&record.socket_path))),
```
versus the launch guard, which treats the same result as live:
```rust
// commands.rs:3345 (recorded_live_socket)
        Err(error) => Some(live(&probe_failure(&error), None)),
```

**Scenario (CONFIRMED).** `probe` returns `Err` for EACCES on a mode-000 socket, a non-socket at the path, a foreign owner, or a `NoReply` from a busy/dead-ish supervisor (`supervisor.rs:322-350`). `start`, `resume`, `cleanup` and `stop` all fail closed on that (`commands.rs:3345`, `:3357`, `:954`, `:2246`); `recover --clean --phase N` fails **open**: `refuse_a_live_supervisor` passes, and `recover::clean_phase_report` (`recover.rs:360-372`) deletes `state-NN.json`, every gate file and the cron record while the run may be live. The same call also passes for a STALE socket whose `backstop::assess` says `LeaderPresentSignal` — it never runs the backstop, so the agent group survives and the state that named it is gone; the operator sees “cleaned up workflow state for phase N” (plus, at best, the “agent appears to still be running — cleared anyway” warning).

---

## 3. MEDIUM — the final shutdown reply conflates “handed off” with “finished/failed”: a stop after the last safe point is silently lost (SIGTERM) or reported as a handoff that never happened

**Where:** `crates/devflow-core/src/supervisor/runtime.rs:585-594`

```rust
        let reply = if self.token.ended_stopped() {
            Reply::Stopped(ShutdownStopped { exit_code: STOPPED_EXIT_CODE, advance_suppressed: true })
        } else {
            Reply::Error("handed_off".to_string())
        };
```

`finish` is called for **every** outcome (`pipeline_launch.rs:1278`). `ended_stopped` is set only by `observe_exit` on `AgentOutcome::Stopped` or by a safe point that returned `StopHere` (`runtime.rs:293-310`, `:219-230`). A stop request that arrives while `Advancing` **after** the last safe point (i.e. the advance's tail: `evaluate_agent_result`, a git call, `finish_workflow`, or the `?` on a failing transition) is flagged but never consumed.

**Scenario (CONFIRMED).**
- Socket: `stop` gets `handed_off`, reloads state (still naming this launch), probes → Gone → `SocketStop::NoLiveSocket` → `stop_via_lock` → no holder → `persist_stopped_state` sets `stopped = true` with reason “stopped via devflow stop” on a run that *completed*. The operator also sees “nothing is running”.
- SIGTERM/SIGINT: `on_stop_signal` writes the pipe byte, `Control::stop` takes `StopAction::FlagForSafePoint`, and nothing ever reads the flag. The signal is dropped with no record and no event — contradicting OPERATIONS.md “SIGTERM or SIGINT … does what `devflow stop`'s ALIVE arm does … and `stopped` is recorded”.

The protocol doc (`protocol.rs:1-14`) says `handed_off` means “The supervisor had already handed the run off”; here it also means “the run ended some other way and your stop was ignored”. `stop`'s four-request retry loop (`commands.rs:2444`) is built on that meaning.

---

## 4. MEDIUM — unknown identity is stored as `0` / `""`, and both sentinels select the *permissive* backstop branch that removes the record and signals nothing

**Where:** `crates/devflow-core/src/monitor.rs:1542-1543`, `crates/devflow-core/src/supervisor/backstop.rs:328-337`, `:496-501`

```rust
// monitor.rs:1542 (stage_sidecar)
        agent_start_time: crate::agent::process_start_time(agent_pid).unwrap_or(0),
        boot_id: crate::supervisor::boot_id().unwrap_or_default(),
```
```rust
// backstop.rs:328-337 (classify)
    if recorded.boot_id != current_boot {
        return BackstopDecision::PreviousBoot;
    }
    ...
        Some(leader) if leader.start_time != recorded.start_time => {
            BackstopDecision::LeaderRecycled
```
```rust
// backstop.rs:496-500 (settle)
        BackstopDecision::PreviousBoot
        | BackstopDecision::EmptyGroup
        | BackstopDecision::LeaderRecycled => {
            super::remove_launch_files(socket)?;
```

**Scenario.** A sidecar written when the identity could not be read records `agent_start_time = 0` / `boot_id = ""`. At teardown, `0` can never equal any real start time and `""` can never equal the current boot id, so a **live, non-empty** group classifies as `LeaderRecycled`/`PreviousBoot` — the two decisions whose `settle` arm deletes the socket **and the sidecar** and signals nothing. `devflow stop` then prints “nothing was running … removed the stale socket” and `end_recorded_agent_group` (`monitor.rs:1835-1875`) does the same on the launch-failure path, after which nothing on disk records the pgid. The `is_same_process(pgid, sidecar.agent_start_time)` re-check can never rescue it because `process_start_time` never returns `Some(0)`.

Mechanism CONFIRMED; how a launch reaches `0`/`""` on Linux is SPECULATION (a `/proc` read failure at the two read sites). Note that `/proc`-unreadable-both-times is handled (`identity_provable` / `RefuseIdentityUnavailable`, `backstop.rs:317-320`) — only the *asymmetric* case fails open. No test covers it: `classify_covers_every_rule_in_order` (`backstop.rs:1475+`) has no `start_time == 0` or empty-`boot_id` row, and `sidecar_for` is only ever given real values.

---

## 5. LOW/MEDIUM — a naturally-completed stage's leftover process-group members are never killed, and `finish` deletes the only record of the group

**Where:** `crates/devflow-core/src/supervisor/runtime.rs:557-564`, `crates/devflow-core/src/supervisor/runtime.rs:616` → `crates/devflow-core/src/supervisor.rs:645-656`

```rust
        self.token.observe_exit(|stop_requested| {
            if stop_requested {
                // The zombie leader still holds the pgid ...
                signal_group(pgid, libc::SIGKILL);
            }
            child.wait().map(exit_code)
```
```rust
        if let Err(error) = super::remove_launch_files(&self.socket) {   // removes .sock AND .json
```

**Scenario (CONFIRMED).** The group is killed only when `stop_requested`. On a normal completion only the leader is reaped; background children the agent left in its group (the capture shape has no pipe EOF to notice them) keep running. `close()` then removes the socket **and the sidecar**, and the state's record is superseded by the next stage (or the run ends), so the pgid exists nowhere. `stop` afterwards probes Gone → `stop_via_lock` → “nothing is running”; `doctor` skips Gone entries (`commands.rs:5629`: `_ => continue`). The leaked group is unreachable by every verb. (The stale `phase-NN-agent-pid` file survives until the next `archive_phase_files`, but nothing consumes it as a group record.)

---

## 6. LOW — a probe during a supervisor's stop-kill grace returns `NoReply` (an error), not ALIVE; a second concurrent `stop` therefore reports failure for a stop that succeeded

**Where:** `crates/devflow-core/src/supervisor/runtime.rs:796-807`, `crates/devflow-core/src/supervisor/protocol.rs:30`

```rust
                .wait_timeout_while(state, crate::agent::TERMINATE_VERIFY_WAIT, |state| {
                    !state.agent_exited
                })
```
```rust
pub const IO_TIMEOUT: Duration = Duration::from_secs(2);
```

**Scenario (CONFIRMED).** `Control::stop`'s `KillAgentGroup` arm blocks the single control thread on the condvar for up to `TERMINATE_VERIFY_WAIT` (3 s) *after* writing `accepted`. Any other connection's ping read times out at 2 s, so `probe` returns `ProbeError::NoReply`. Consequences: `devflow status`/`doctor` racing a stop print “UNKNOWN — the probe failed” for a perfectly healthy run; a second `devflow stop` whose *accepted* line times out re-probes, sees `Alive`, and takes `_ =>` in `shutdown_supervisor` → “did not answer the shutdown … nothing was signalled or recorded” even though the first stop is ending the run. `cleanup` keeps the worktree on `ProbeFailed` (`commands.rs:954`). The control thread should not be the thing that waits out the kill grace (or `probe` should treat “connect accepted, no reply” during a stop as a distinct, non-fatal state).

---

## 7. LOW — `.part` publish files leak permanently and nothing reports or removes them

**Where:** `crates/devflow-core/src/supervisor.rs:186-191`, `:526-529`, `:645-656`

```rust
    let linked =
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o600)).and_then(|()| {
            before_link();
            std::fs::hard_link(&part, path.as_path())
        });
    let cleanup = std::fs::remove_file(&part);
```
```rust
        if let Some(stem) = name.strip_suffix(".sock") {
            stems.entry(stem.to_string()).or_default().0 = true;
        } else if let Some(stem) = name.strip_suffix(".json") {
```

**Scenario (CONFIRMED).** A SIGKILL (or `abort`) between `UnixListener::bind(&part)` and `hard_link` leaves `<stem>.part` in `$XDG_CACHE_HOME/devflow/sock`. `publish`'s cleanup, `remove_launch_files`, `sweep_in` (stems are built only from `.sock`/`.json`) and therefore `doctor` all ignore it. The file is never reclaimed, so `sock/` accumulates one dead inode per crash-in-window; the "sweep that filters on `.sock` never sees a half-published socket" design decision (`supervisor.rs:113-118`) makes it invisible rather than harmless. Because `.part` has the launch id in it, a *reused* launch id would also fail `bind` with `AddrInUse` (`PublishError::Io`), not the clean `AlreadyExists` path.

---

## 8. LOW — OPERATIONS.md documents a `/proc/<pid>/cmdline` check that the code deliberately refuses to do

**Where:** `OPERATIONS.md:43` vs `crates/devflow-cli/src/commands.rs:2813-2817`

```
… is signalled, after confirming it is alive and its `/proc/<pid>/cmdline` identifies it as belonging to DevFlow.
```
```rust
    // Identity must be MATCHED against what the lock recorded, never inferred
    // from /proc (999.47). A bare cmdline-basename check alone returns true
    // for any freshly forked child of a devflow process that has not finished
    // execve …
```

**Scenario (CONFIRMED).** `stop_via_lock` signals only on recorded `(pid, start_time)` identity (`lock::holder_identity` + `is_same_process`). The operator procedure describes a weaker, previously-rejected mechanism, so an operator reasoning about step 3's safety from the doc gets the wrong model (and a version that *did* use cmdline would have the 999.47 execve-window bug the code comment names). The neighbouring claim in the same cell — that `stop` “records `stopped`/`stop_reason`” — is also false for the gate arm, which aborts and deletes the state (`stop_via_gate` → `pipeline_gate::abort`).

---

## Test-quality notes (no separate finding)

- The required R-2 determinism test `supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances` (`runtime.rs:1254`) exercises `run_stage` (`:1208`), a hand-written mirror of `run_monitor`'s ordering, not the production safe points in `pipeline_launch.rs`. It cannot fail if a production launch path skips or reorders safe point 2/3 or drops the `token.stop_requested()` forward (`pipeline_launch.rs:1253-1256`). The held-window tests in `pipeline_launch.rs` (`:8394`, `:8480`, `:8726`) do cover the real paths, so the gap is drift-detection, not current behaviour.
- No test pins the `agent_start_time == 0` / empty-`boot_id` sentinel behaviour described in finding 4, so the fail-open branch is invisible to the suite.

---

## NOT REVIEWED

Files read only partially or not at all (I read the parts needed to confirm/refute claims, not every line):

- `crates/devflow-cli/src/commands.rs` — ~3.5k of 11,108 lines (skipped `gate_*`, `status`, `parallel`, `ship`, most of the test module).
- `crates/devflow-cli/src/pipeline_launch.rs` — ~1.8k of 9,865 lines (skipped `resume` body, preflight/Advance arms, checkpoint relaunch internals, and most `#[cfg(test)]` tests).
- `crates/devflow-core/src/monitor.rs` — ~1.9k of 6,312 lines (skipped most idle-timeout/commit-enumeration helpers and most tests).
- `crates/devflow-core/src/state.rs` (except `supervisor`/`SupervisorRecord`), `gates.rs` (only `poll_response_cancellable`), `agent.rs` (only the identity/terminate primitives), `lock.rs` (only `classify_holder_record`/`acquire_after_existing`), `recover.rs` (only `sweep_phase`/`clean_phase_report`), `doc_check.rs` (only the monitor-pid scan).
- `crates/devflow-cli/src/preflight.rs` (only the launch-shape condition), `main.rs` (only the CLI surface and `__monitor` dispatch), `pipeline_gate.rs`/`pipeline_outcomes.rs` (only the gate wait and ship outcome).
- `crates/devflow-core/src/supervisor.rs` tests after line ~1350 and `supervisor/runtime.rs` tests after ~1690.
- Integration suites: `stop_e2e.rs` (partly), `start_lock_e2e.rs` (partly), `supervisor_e2e.rs` (partly), `test_home_isolation.rs` (partly), `hermetic_spawn_sites.rs`, `gate_*_e2e.rs`, `auto_chain_*`, `phase7_cli.rs`, `plan_bashism_scanner.rs`, `monitor_e2e.rs`, `devflow_dir_gitignore.rs` — not read.
- `ARCHITECTURE.md` was scanned (grep) but not read end-to-end; the only contradiction I found is documented in finding 8 via OPERATIONS.md.

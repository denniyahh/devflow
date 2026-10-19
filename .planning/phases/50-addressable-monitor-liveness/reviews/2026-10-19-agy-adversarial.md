### Needs my awareness

1. **Critical (CONFIRMED): `devflow stop` destroys phase state and permanently prevents resumption when called while parked at an approval gate.**
   - **Locations:** [`crates/devflow-cli/src/commands.rs:2009-2012`](crates/devflow-cli/src/commands.rs#L2009-L2012), [`crates/devflow-cli/src/commands.rs:2695-2701`](crates/devflow-cli/src/commands.rs#L2695-L2701), [`crates/devflow-cli/src/pipeline_gate.rs:273`](crates/devflow-cli/src/pipeline_gate.rs#L273), and [`crates/devflow-cli/src/pipeline_gate.rs:504`](crates/devflow-cli/src/pipeline_gate.rs#L504)
   - **Quoted code:**
     ```rust
     // commands.rs:2695-2701
     match Gates::reap(
         project_root,
         phase,
         gate.stage,
         "abort: stopped by `devflow stop`",
         "devflow-stop",
     )
     ```
     and in `pipeline_gate.rs:504`:
     ```rust
     let _ = workflow::clear_state(project_root, state.phase);
     ```
   - **Failure scenario:** Contradicts `OPERATIONS.md:146-148` (*"A supervisor parked at a gate clears the gate and records stopped rather than aborting the phase, so the phase stays resumable"*) and `ARCHITECTURE.md:235-238` (*"`stopped` is recorded in the state, which is kept for `devflow resume` (not an abort)"*). When an operator runs `devflow stop --phase N` while the supervisor is parked waiting at an approval gate, `stop_via_gate` runs before socket stop and writes an abort response (`"abort: stopped by devflow stop"`). The parked supervisor's `Gates::poll_response_cancellable` prioritizes on-disk responses over cancellation tokens (*"a response already on disk wins over a cancel"*). The advance loop maps this rejection to [`GateAction::Abort`](crates/devflow-cli/src/pipeline_gate.rs#L273), which invokes [`abort()`](crates/devflow-cli/src/pipeline_gate.rs#L499-L515). `abort()` calls `workflow::clear_state`, deleting `.devflow/state-NN.json`. The phase state is wiped, and `devflow resume --phase N` fails because the state file no longer exists.

2. **Critical (CONFIRMED): Unconditional atomic file `rename` in `write_sidecar` corrupts existing launch sidecars upon socket path collision, misdirecting or blocking backstop teardown.**
   - **Locations:** [`crates/devflow-core/src/supervisor.rs:464`](crates/devflow-core/src/supervisor.rs#L464) and [`crates/devflow-core/src/supervisor/runtime.rs:504-511`](crates/devflow-core/src/supervisor/runtime.rs#L504-L511)
   - **Quoted code:**
     ```rust
     // supervisor.rs:464
     .and_then(|()| std::fs::rename(&temp, &target));
     ```
     and in `runtime.rs:504-511`:
     ```rust
     super::write_sidecar(&socket, &sidecar)?;
     let listener = match super::publish(&socket) {
         Ok(listener) => listener,
         // Another launch's live socket holds the name: its sidecar is
         // not ours to remove.
         Err(PublishError::AlreadyExists(path)) => {
             return Err(PublishError::AlreadyExists(path));
         }
     ```
   - **Failure scenario:** [`write_sidecar`](crates/devflow-core/src/supervisor.rs#L448-L471) creates a temporary file and executes an unconditional `std::fs::rename` over the target sidecar path (`<stem>.json`). If an existing supervisor holds that socket stem, `write_sidecar` overwrites its sidecar on disk with the newcomer's launch ID, `agent_pgid`, and start time. When line 505 calls `super::publish(&socket)`, `bind` fails with `EADDRINUSE` (`PublishError::AlreadyExists`). The handler exits without touching the sidecar under the assumption that *"its sidecar is not ours to remove"*. However, the sidecar has already been overwritten. If the original supervisor later crashes:
     - Under standard recovery, backstop detects a launch ID mismatch between state and the corrupted sidecar, permanently refusing teardown (`RefuseMismatch`).
     - Under unreadable state recovery (D-09), backstop trusts the corrupted sidecar and validates/signals the wrong process group.

3. **High (CONFIRMED): `shutdown_final_timeout` in `devflow stop` is too short to accommodate lock contention in `record_supervisor_stop`, producing spurious CLI stop failures.**
   - **Locations:** [`crates/devflow-cli/src/commands.rs:2251-2253`](crates/devflow-cli/src/commands.rs#L2251-L2253), [`crates/devflow-cli/src/commands.rs:2497-2511`](crates/devflow-cli/src/commands.rs#L2497-L2511), and [`crates/devflow-cli/src/pipeline_launch.rs:962-963, 1444`](crates/devflow-cli/src/pipeline_launch.rs#L962-L963)
   - **Quoted code:**
     ```rust
     // commands.rs:2251-2253
     fn shutdown_final_timeout() -> std::time::Duration {
         2 * agent::TERMINATE_VERIFY_WAIT + std::time::Duration::from_secs(5)
     }
     ```
     and in `pipeline_launch.rs:1444`:
     ```rust
     let _lock = match lock::acquire_blocking(project_root, phase, SUPERVISOR_STOP_WAIT) {
     ```
   - **Failure scenario:** [`shutdown_final_timeout()`](crates/devflow-cli/src/commands.rs#L2251-L2253) is 11 seconds (2 * 3s + 5s). When `devflow stop` issues a shutdown request, the agent can take up to 3 seconds to terminate. The supervisor then invokes `record_supervisor_stop` before `Supervisor::finish()` can send the final protocol reply. `record_supervisor_stop` acquires the phase lock using `SUPERVISOR_STOP_WAIT` (11 seconds). If another command (such as a concurrent `devflow status` or inspection command) holds the lock for ≥6 seconds, total elapsed time exceeds 11 seconds. `request_shutdown` hits the read timeout (`ProbeError::NoReply`). Line 2497 immediately falls back to `supervisor::probe()`. Because the supervisor is still blocked waiting for the phase lock and has not closed its socket listener, `probe` returns `Alive`. Line 2506 triggers, and `devflow stop` errors out with `"stop: phase N's supervisor ... did not answer the shutdown ... nothing was signalled or recorded"`, despite the supervisor successfully stopping the agent and recording the stop seconds later.

4. **High (CONFIRMED): `devflow doctor` orphan sweep silently ignores sockets with probe errors, concealing corrupted sockets from the operator.**
   - **Locations:** [`crates/devflow-cli/src/commands.rs:5621-5626`](crates/devflow-cli/src/commands.rs#L5621-L5626)
   - **Quoted code:**
     ```rust
     for entry in supervisor::sweep_in(cache_dir) {
         let liveness = match &entry.liveness {
             Ok(supervisor::Liveness::Alive(_)) => "alive",
             Ok(supervisor::Liveness::Stale) => "stale",
             _ => continue,
         };
     ```
   - **Failure scenario:** Contradicts `OPERATIONS.md:100-105` (*"A probe that cannot decide ... reads `UNKNOWN — the probe failed: …`"*) and `OPERATIONS.md:119-126` (*"doctor: ... If a probe fails, report the socket and error so the operator can inspect it"*). If a socket in `<cache_dir>/sock` experiences a probe failure (e.g., bad permissions, `EACCES`, `NotASocket`, or connection timeout), `entry.liveness` is `Err(ProbeError)`. Line 5625 silently skips the entry via `_ => continue;`. `devflow doctor` reports no issues, hiding the broken or unexaminable orphan from the operator.

5. **Medium (CONFIRMED): Stale backstop permanently wedges teardown if agent leader terminates during SIGTERM grace period while child processes survive.**
   - **Locations:** [`crates/devflow-core/src/supervisor/backstop.rs:550-562`](crates/devflow-core/src/supervisor/backstop.rs#L550-L562)
   - **Quoted code:**
     ```rust
     if signal_validated_group(recorded.pgid, recorded.start_time, libc::SIGKILL)?
         == GroupSignal::LeaderGone
     {
         // The leader died during the grace period and members outlived the
         // SIGTERM. Without a leader nothing proves they are the agent's, so
         // decide again: an empty group is done, anything else is kept.
         return match classify(&gather(sidecar, state_record)) {
             BackstopDecision::EmptyGroup => {
                 super::remove_launch_files(socket)?;
                 Ok(BackstopOutcome::Killed { via, members })
             }
             decision => Ok(BackstopOutcome::Refused(decision)),
         };
     }
     ```
   - **Failure scenario:** A monitor crashes, leaving its socket STALE. The agent process group contains the leader (agent, PID=PGID) and a spawned child process. The operator runs `devflow stop`. Backstop validates the leader and delivers `SIGTERM` to the process group. The agent leader exits on `SIGTERM`, but the child process ignores `SIGTERM` and remains alive past the 3-second grace period (`TERMINATE_VERIFY_WAIT`). Line 550 attempts escalation to `SIGKILL`. Because the leader process is already dead, `signal_validated_group` returns `GroupSignal::LeaderGone` without sending `SIGKILL`. Line 556 re-gathers census data; [`classify`](crates/devflow-core/src/supervisor/backstop.rs#L220-L245) observes that child processes remain without a leader, returning `RefuseLeaderGone(members)`. Line 561 returns `BackstopOutcome::Refused`. Sockets and sidecars are retained, no `SIGKILL` is sent, and child processes leak. Every subsequent invocation of `devflow stop` immediately refuses at entry for the same reason, permanently wedging the run.

6. **Medium (CONFIRMED): Redundant atomic sidecar serialization and `fsync` on every stage launch.**
   - **Locations:** [`crates/devflow-core/src/monitor.rs:1310-1314`](crates/devflow-core/src/monitor.rs#L1310-L1314) and [`crates/devflow-core/src/supervisor/runtime.rs:504`](crates/devflow-core/src/supervisor/runtime.rs#L504)
   - **Quoted code:**
     In `monitor.rs`:
     ```rust
     let sidecar = stage_sidecar(&spec, pid);
     crate::supervisor::write_sidecar(&spec.socket, &sidecar)?;
     let supervisor = Supervisor::serve(spec.socket.clone(), sidecar)?;
     ```
     In `runtime.rs`:
     ```rust
     pub fn serve(socket: SocketPath, sidecar: Sidecar) -> Result<Self, PublishError> {
         super::write_sidecar(&socket, &sidecar)?;
     ```
   - **Failure scenario:** [`write_sidecar`](crates/devflow-core/src/supervisor.rs#L448-L471) performs JSON serialization, temporary file creation, `write_all`, `sync_all` (`fsync`), permission modification (0600), and atomic `rename`. Both `run_supervised_stage` and `Supervisor::serve` call `write_sidecar` consecutively for the same socket path and payload on every stage launch, doubling synchronous disk sync operations without functional benefit.

---

### Handled
- Completed read-only adversarial review of Phase 50 source snapshot across all requested priority modules without mutating repository files or spawning background builds.

---

### Detail
- **Limits of verification:** This analysis is based on static code tracing and cross-referencing against specifications in `.planning/phases/50-addressable-monitor-liveness/50-CONTEXT.md`, `OPERATIONS.md`, and `ARCHITECTURE.md`. It does not evaluate dynamic kernel-level pidfd behavior under rapid PID wrap-around or filesystem-specific atomic rename semantics outside standard POSIX tmpfs/ext4.
- **Negative control:** Verified that normal gate resolution paths (`GateAction::Advance` and `GateAction::LoopBack`) correctly persist state via `workflow::save_state`, confirming that state destruction is unique to the `GateAction::Abort` branch triggered by `stop_via_gate`.

---

### NOT REVIEWED
- [`crates/devflow-cli/src/preflight.rs`](crates/devflow-cli/src/preflight.rs): Read only targeted sections matching gate and stop symbols; lines 1–4698 were not read in full.
- Test suites: Integration tests under `crates/devflow-cli/tests/` and test modules inside `commands.rs`, `pipeline_launch.rs`, and `monitor.rs` were sampled for specific assertion verification rather than exhaustively audited.
- [`crates/devflow-cli/src/main.rs`](crates/devflow-cli/src/main.rs): Reviewed `__monitor` invocation routing and arguments (lines 115–165, 610–650); unrelated CLI subcommand handlers were not read.

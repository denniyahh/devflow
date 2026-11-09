## FINDINGS

### 1. `drive_stream_io`'s 120s Idle Wait Precedes `join_within`, Defeating the 3s Bound on Stopped Runs with External Holders

* **Files & Citations:**
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L26-L27): *"The bound applies to both outcomes: a stopped run whose stdout is held by a process outside the agent's process group (which the stop's group SIGKILL cannot reach) still returns, its parked shutdown caller gets `stopped` (143, advance suppressed) and its socket is removed"*
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L197-L199): *"In `run_supervised_stage`, call `threads.join_within(READER_DRAIN_WAIT)` where the unbounded join stands: after the exit file is written, for both `AgentOutcome` arms."*
  * [monitor.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-core/src/monitor.rs#L1066-L1146): `line_rx.recv_timeout(idle_timeout)` inside `drive_stream_io`
  * [monitor.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-core/src/monitor.rs#L1326-L1354): `drive_stream_io` runs *before* `Supervisor::wait_agent` and *before* `threads.join_within`
  * [commands.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-cli/src/commands.rs#L2477-L2527): `devflow stop` sets `final_timeout = SHUTDOWN_FINAL_TIMEOUT` (22 s)

* **Contradiction & Concurrency Defect:**
  The plan assumes bounded termination is achieved solely by swapping `threads.join()` for `threads.join_within(3s)`. However, `threads.join_within` is called at line 1353, *after* `drive_stream_io` has already returned (line 1326) and after `wait_agent` has reaped the agent (line 1340).
  
  `drive_stream_io` possesses no reference to `Supervisor` or `StopToken`. When an operator runs `devflow stop` against an active stage where a child process outside the agent group holds stdout:
  1. The control thread signals the agent group and parks the shutdown connection waiting for final status (22s timeout in client).
  2. The agent dies, but `child_stdout` remains open via the external holder.
  3. `drive_stream_io` remains blocked in `line_rx.recv_timeout(idle_timeout)`. In production, `idle_timeout` is 120 seconds (`STAGE_IDLE_TIMEOUT` / `--idle-timeout-secs`).
  4. If the agent had not yet emitted `RESULT_WITH_MARKER`, `close_signalled` is `false`. When the 120s timeout fires, line 1144 invokes `on_idle_timeout()`, writing an authoritative `IdleTimeout` failure to disk on a run that was actually stopped.
  5. The client's 22s shutdown timeout expires at t=22s, returning `"did not answer the shutdown ... nothing was signalled or recorded"` (WR-01).

  Task 2's Test D only passes because it artificially injects `idle 3 s` via `STAGE_IDLE_SECS_ENV` and forces the agent to emit `RESULT_WITH_MARKER` prior to stopping, avoiding `on_idle_timeout()`. In production, a stopped stream run with an external holder does **not** return in 3 seconds; it blocks for 120 seconds inside `drive_stream_io` before `join_within` is ever reached.

---

### 2. Revoking the Capture Handle Does Not Unblock the Reader Thread for Silent Holders, and Causes Kernel `SIGPIPE` Death for Active Leftover Processes

* **Files & Citations:**
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L27): *"The fix sends no signal to free the agent's stdout: the process holding it is still alive when the supervisor returns ... members left in the agent's group after a normal completion keep running exactly as before (backlog 999.164 item 5, unchanged)"*
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L183-L194): *"In `drive_stream_io`, move the capture `File` into a shared handle, `Arc<Mutex<Option<std::fs::File>>>` ... On `None`, stop reading and leave the loop without writing ... If either is still running, it takes the capture handle out under the lock (setting it to `None` drops the `File`), drops each unfinished `JoinHandle` (detaching the thread)"*
  * [monitor.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-core/src/monitor.rs#L1029-L1040): reader thread loop:
    ```rust
    let mut reader_buf = BufReader::new(child_stdout);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match reader_buf.read_until(b'\n', &mut raw) {
            Ok(0) => break, // real EOF
            Ok(_) => {}
            Err(err) => { ... break; }
        }
    ```

* **Unstated Assumptions & Broken Contract:**
  * **Silent holders freeze the thread:** When `join_within` takes the capture handle and detaches the reader thread after 3s, the reader thread is blocked at `reader_buf.read_until(b'\n', &mut raw)` in a blocking `read(2)` syscall. Taking the `Mutex<Option<File>>` does not interrupt or wake up the thread. For any silent process (such as `sleep 300 &`), `read_until` never returns. The detached thread remains blocked in kernel space holding `child_stdout`, TLS, and its thread stack indefinitely.
  * **Active holders receive `SIGPIPE`:** If the leftover process *does* write output after the 3s bound expires (e.g. Test C's late line), `read_until` unblocks, locks the mutex, sees `None`, and breaks out of the loop. This causes `reader_buf` to drop, closing the read end of `child_stdout`. On the leftover process's very next write to stdout, the kernel delivers `SIGPIPE`, killing the process. This directly contradicts Must-Have Truth 3 and Threat T-50-98, which assert that leftover processes are left completely unharmed and continue running.

---

### 3. Ordering Inversion: Writing `exit_file` Before `join_within` and Safe Point 1 Corrupts On-Disk Exit Records When Stopped in Advance

* **Files & Citations:**
  * [monitor.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-core/src/monitor.rs#L1351-L1354):
    ```rust
    std::fs::write(&exit_file, format!("{code}\n"))?;
    if let Some(threads) = threads {
        threads.join();
    }
    ```
  * [pipeline_launch.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-cli/src/pipeline_launch.rs#L1063-L1080): `record_advance_stop` writes `state.json` and emits `advance_stopped`, but never modifies `exit_file`
  * [pipeline_launch.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-cli/src/pipeline_launch.rs#L2480): `if stopped_at_safe_point(&mut state, 1)? { return Ok(()); }`
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L57): *"called with READER_DRAIN_WAIT right after the exit file is written, for Completed and Stopped alike, where the unbounded join stood"*
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L240): Test E3 asserts `ShutdownOutcome::Stopped { exit_code: 143, advance_suppressed: true }`

* **Ordering Defect & State Inconsistency:**
  In a normal completion where stdout is held, line 1351 writes `code = 0` (`"0\n"`) to `.devflow/phase-NN-exit`. Then line 1353 enters `threads.join_within(3s)`.
  
  During this 3-second window, the supervisor owner is `advancing`. If an operator sends `devflow stop`, the supervisor accepts the stop and flags it for safe points.
  
  When `join_within` expires, `run_monitor` invokes `advance_for_stage`, which immediately hits safe point 1 (`stopped_at_safe_point`). The run terminates as stopped, and the shutdown client receives `exit_code: 143`.
  
  However, `exit_file` was already committed to disk as `0` at line 1351 and is never updated by `record_advance_stop`. Any tool or doctor command inspecting disk state sees `state.stopped = true`, `events.jsonl` recording an aborted run at safe point 1, but `.devflow/phase-NN-exit` reporting a successful clean exit code of `0`.

---

### 4. Test E3 Contains an Unsynchronized 3-Second TOCTOU Race Condition Against Safe Point 1

* **Files & Citations:**
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L240): *"E3 ... The test polls the socket's ping for up to 30 s until `owner` is `advancing`, then calls `supervisor::request_shutdown(&socket, 20 s)`. It must return `Ok(ShutdownOutcome::Stopped { exit_code: 143, advance_suppressed: true })` ... exactly one `advance_stopped` event for phase 74 has `safe_point` 1 ... no `workflow_finished` event with reason `stopped_at` exists"*
  * [pipeline_launch.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/crates/devflow-cli/src/pipeline_launch.rs#L2480-L2485):
    ```rust
    if stopped_at_safe_point(&mut state, 1)? {
        return Ok(());
    }
    #[cfg(test)]
    test_hold("after_safe_point_1");
    ```

* **Timing Hazard & Flaky Assertion:**
  The window where the supervisor is in `owner: advancing` *prior* to reaching safe point 1 is strictly bounded by `READER_DRAIN_WAIT` (3.0 seconds).
  
  In Test E3, the test harness polls the socket until `owner == "advancing"`, and *then* initiates a connection to call `request_shutdown`.
  
  If the host experiences scheduling latency or CPU contention (e.g., during `scripts/check.sh all` where the entire test suite runs concurrently), the 3.0-second countdown in `join_within` can elapse before `request_shutdown` connects and writes `Request::Shutdown`.
  
  Once `join_within` returns, `advance_with` immediately evaluates safe point 1 without stopping, evaluates the stage result, reaches the `--until code` gate, emits `workflow_finished` (`stopped_at`), and sets `stop_reason = "stopped after code completed (--until code)"`.
  
  Test E3 will fail on three separate assertions: `advance_stopped` will be missing, `workflow_finished` will be present, and `stop_reason` will mismatch. There is no `test_hold` before safe point 1 in production or test code to make this race window deterministic.

---

### 5. Speculative Scope Leak in Backlog 999.166 Item 7 and Incomplete Revert Check Validation

* **Files & Citations:**
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L336-L340): *"Append item 7: '(found 2026-10-21 by reading `drive_stream_io` while planning 50-26; not reproduced) After the close rule releases stdin, every line still arriving on the agent's stdout restarts the idle wait...'"*
  * [ROADMAP.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/ROADMAP.md#L1233-L1265): Phase 999.166 defect ledger
  * [50-26-PLAN.md](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ddf73672-4517-4429-bb90-b2320ee6b489/scratchpad/xr-5026/snap/.planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md#L284): Task 2 revert check script

* **Scope Leak & Revert Fragility:**
  * **Speculative Backlog Injection:** Phase 50-26 is strictly scoped as a gap closure for WR-03. Injecting an unverified, unreproduced defect ("item 7") into `ROADMAP.md` backlog 999.166 violates the project's Rule Zero. If an adjacent issue is observed during analysis, it must either be reproduced with a failing test or omitted from formal roadmaps until triaged.
  * **Revert Check Fragility:** The revert script in line 284 executes:
    `F=$(git log --format=%H -F --grep="fix(50-26): bound the stdout wait after the agent exits (WR-03)" HEAD)`
    followed by `git -C "$S" revert --no-commit "$F"`.
    The Task 1 commit message is constrained to this exact subject line. However, because Task 2's commit modifies `crates/devflow-core/src/monitor.rs` by adding test D, if test D is placed adjacent to the lines reverted in `monitor.rs` (such as `supervise_stream_child` or `StreamThreads`), `git revert --no-commit` will produce a merge conflict and abort the entire gate with `REVERT revert_conflict`. The plan warns about this at line 275, but provides no syntactic guardrails in the harness to guarantee disjoint patch hunks across commits.

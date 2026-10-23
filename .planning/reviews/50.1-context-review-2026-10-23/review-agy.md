## CONFIRMED

### 1. Fatal Contradiction: D-06 Re-opens Stale-Answer Vulnerability (AR-48-04 Path `a` / D2) That D-05 and Criterion 4 Claim to Close
* **Citations:**
  * `50.1-CONTEXT.md:108-110` (D-06): *"`resume` re-opens a gate that is still pending under the **same** id, so an answer written after the wait timed out still decides it, as `README.md:171` promises. An answer to an earlier, closed gate never decides a new one."*
  * `50.1-CONTEXT.md:101-102` (D-05): *"Each stale-answer path in 999.130 and AR-48-04 gets a failing-then-passing test: a waiter killed after an answer is written, then `resume`"*
  * `ROADMAP.md:509-512` (Phase 50.1 Criterion 4): *"`resume` re-opens a still-pending gate under the same id, so a late answer still works. Each stale-answer path (AR-48-04, the T-48-12-05 caveat) has a failing-then-passing test."*
  * Code: [`crates/devflow-core/src/gates.rs:276-279`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/gates.rs#L276-L279) (`poll_response`), [`crates/devflow-cli/src/pipeline_launch.rs:1383`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_launch.rs#L1383) (`resume` relaunches stage), [`crates/devflow-cli/src/pipeline_launch.rs:6169`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_launch.rs#L6169) (`resume_relaunches_without_consuming_a_pending_gate_answer`).
  * Threat: `.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-SECURITY.md:131` (AR-48-04 item `a`).
* **Defect:** 
  D-06 directly contradicts D-05, AR-48-04, and ROADMAP Criterion 4. In AR-48-04 item (a), a gate is opened, a response is written to disk, and the waiter is killed before consuming it. When `devflow resume` runs, it relaunches the stage. Under D-06, `resume` re-opens the gate under the **same** gate ID to keep `README.md:171`'s late-answer promise. Because the gate ID is identical, the new gate poll immediately matches and consumes the stale response written for the prior, dead run.
  You cannot simultaneously reuse the same gate ID across `resume` to accept late answers while asserting that pre-resume stale answers are refused. D-06 makes closing AR-48-04 item (a) and crash row D2 impossible.

---

### 2. Lock Split (D-08) Destroys Live-Waiter Verification in CLI Answer Writers and Breaks Phase 48 D-02 / D-05
* **Citations:**
  * `50.1-CONTEXT.md:135-136` (D-08): *"No gate wait, agent run or lock wait holds that lock."*
  * `50.1-CONTEXT.md:88-91` (D-04): *"This replaces Phase 48 D-02's side effect, where every waiting process holds the per-phase lock. The CLI answer writers (`gate respond`, `gate sweep`, `stop`) relied on that through `lock::holder_status`. With D-08 removing the long lock, they must identify the live waiter some other way: from its registration, or from the owning supervisor's socket. Claude picks which."*
  * Code: [`crates/devflow-cli/src/commands.rs:1535-1540`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L1535-L1540) (`gate_respond`), [`crates/devflow-cli/src/commands.rs:2686-2710`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L2686-L2710) (`stop_via_gate`), [`crates/devflow-core/src/lock.rs:39-45`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/lock.rs#L39-L45) (`holder_status`), [`crates/devflow-core/src/supervisor/protocol.rs:45-68`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/supervisor/protocol.rs#L45-L68) (`PingReply`).
  * Phase 48: `.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-CONTEXT.md:110-120` (D-05).
* **Defect:**
  Phase 48 D-05 established that `gate approve`, `gate reject`, and `stop` write *no* response file at a non-Ship gate unless a live waiter holds the per-phase lock (`lock::holder_status`). If D-08 removes the lock held across gate waits:
  1. **File registration is prone to crash leakage:** An on-disk waiter registration in `.devflow/gates/` does not disappear when a process is killed (`SIGKILL`, segfault, power cut). A dead waiter leaves an on-disk registration behind, causing CLI writers to falsely conclude a live waiter is present and write orphan answers.
  2. **Socket protocol lacks gate identity:** [`supervisor::protocol::PingReply`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/supervisor/protocol.rs#L45-L68) only exposes coarse `owner: String` (`"gate_wait"`). It contains no gate ID, stage, or registration token to confirm which gate is waiting.
  3. **Foreground gates have no supervisor:** Foreground operations (e.g. `devflow start` running preflight gates at [`crates/devflow-cli/src/preflight.rs:1367`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/preflight.rs#L1367) or `devflow ship` running finalization at [`crates/devflow-cli/src/pipeline_gate.rs:518`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_gate.rs#L518)) have NO supervisor socket. If they hold no lock across the gate wait, answer writers cannot verify their liveness at all.

---

### 3. Record-Before-Spawn (D-01 / D-02) Leaves Permanent Wedge on Failed Spawn
* **Citations:**
  * `50.1-CONTEXT.md:60-64` (D-01): *"Before spawning a supervisor, the launcher saves a pending supervisor record carrying the new launch id, under the write lock... A launcher that dies anywhere between the save and the publish leaves a record that the start/resume guard probes."*
  * `50.1-CONTEXT.md:74-77` (D-02): *"The repair is `stop`. When the guard finds a live supervisor, recorded or pending, it refuses and names `devflow stop`. `stop` reaches a pending-record supervisor through its socket, exactly as it reaches a recorded one; once that supervisor is GONE, `resume` is safe."*
  * Code: [`crates/devflow-cli/src/commands.rs:3283-3301`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L3283-L3301) (`refuse_launch_over_a_live_run`), [`crates/devflow-cli/src/commands.rs:2014-2017`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L2014-L2017) (`SocketStop::NoLiveSocket`), [`crates/devflow-cli/src/commands.rs:2936-2942`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L2936-L2942) (`persist_stopped_state`).
* **Defect:**
  If the launcher saves a pending record and then fails before publishing:
  * Failure A: `spawn()` fails (`ENOMEM`, `EAGAIN`, binary missing, or `SIGKILL` prior to `spawn`).
  * Failure B: Child spawns but crashes before binding its socket.
  In either case, `state.json` contains a pending supervisor record, but **no socket exists**.
  When the operator runs `start` or `resume`, the guard refuses due to the pending record and directs the operator to `devflow stop` (D-02).
  When the operator runs `devflow stop`:
  1. `stop` probes the socket path, finds nothing, and returns `SocketStop::NoLiveSocket` ([`commands.rs:2014`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L2014)).
  2. `stop` invokes `persist_stopped_state` ([`commands.rs:2936`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L2936)), which updates `state.stopped = true`, but **never clears `state.supervisor`**.
  3. The pending record remains in `state.json`. Subsequent `resume` calls see the pending record and continue to refuse. The run is permanently wedged.
  Conversely, if the guard probes the socket and treats `Liveness::Gone` as "safe to launch", then crash-table row A7 is **not** closed: a launcher killed during the publish wait while the supervisor is initializing will read as `Gone`, allowing a second launcher to start a duplicate supervisor.

---

### 4. D-03 Ownership Token Does Not Stop a Superseded Supervisor from Consuming Gates or Mutating Git
* **Citations:**
  * `50.1-CONTEXT.md:78-80` (D-03): *"A supervisor whose launch id is not the one the state records ends without writing state, and logs an event saying why. This covers its advance and every other state write. This is the ownership-token half of the handoff."*
  * Code: [`crates/devflow-cli/src/pipeline_gate.rs:460-476`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_gate.rs#L460-L476) (`run_gate_with_timeout`), [`crates/devflow-core/src/gates.rs:210-234`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/gates.rs#L210-L234) (`Gates::respond`), [`crates/devflow-core/src/gates.rs:294`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/gates.rs#L294) (`Gates::cleanup`).
* **Defect:**
  D-03 asserts that a superseded supervisor ends without writing state. However, state writes are only the tail end of supervisor actions:
  1. A superseded supervisor waiting at a gate in [`pipeline_gate.rs:460`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_gate.rs#L460) continues polling `.devflow/gates/`.
  2. When an answer is written, the superseded supervisor reads it, writes an ack file ([`pipeline_gate.rs:475`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_gate.rs#L475)), and cleans up the gate files ([`pipeline_launch.rs:2699`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_launch.rs#L2699)).
  3. Only when attempting to save state or advance does it check its launch ID, detect that it is superseded, and exit.
  The human's gate response has been consumed, acknowledged, and purged from disk by a zombie supervisor. The active supervisor never receives it. Furthermore, a superseded supervisor's child agent can continue executing Git commands, writing worktree files, and pushing branches. Ownership tokens checked only at `save_state` do not sandbox external side effects.

---

### 5. Scout Claim Conflates Lock Acquisition Timeout with Hold Duration
* **Citations:**
  * `50.1-CONTEXT.md:137-138`: *"`advance_with` holds the per-phase lock through every gate wait (`acquire_blocking`, 600 s wait; `pipeline_launch.rs:2452`, constant at `:57`)."*
  * Code: [`crates/devflow-cli/src/pipeline_launch.rs:57`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_launch.rs#L57) (`ADVANCE_LOCK_WAIT`), [`crates/devflow-cli/src/pipeline_launch.rs:2452`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_launch.rs#L2452) (`acquire_blocking`), [`crates/devflow-cli/src/pipeline_gate.rs:368-375`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_gate.rs#L368-L375) (`gate_timeout_secs`).
* **Defect:**
  The Scout report claims `advance_with` holds the lock for "600 s".
  [`ADVANCE_LOCK_WAIT`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_launch.rs#L57) (600 s) is strictly the timeout for *acquiring* the lock when contended. Once acquired at line 2452, `_lock` is held for the entire lifecycle of `advance_with`, which includes gate waits. In [`pipeline_gate.rs:370`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/pipeline_gate.rs#L370), `run_gate_with_timeout` blocks on human input for up to `gate_timeout_secs()`, whose default is **3 days** (259,200 s). The lock hold duration is multi-day, not 600 seconds.

---

### 6. Acceptance Criteria 1, 2, 3, 5 Lack Essential Negative Controls or Are Unfalsifiable
* **Citations:**
  * `ROADMAP.md:499-516` (Phase 50.1 Success Criteria 1–6).
* **Defect:**
  * **Criterion 1 (Missing negative control):** Asserts that when a supervisor is live and unrecorded, launch is refused; its negative control only asserts that launch proceeds "with no such supervisor". It has no control for when a *pending record exists without a live supervisor* (e.g. spawn aborted/failed). Without this control, an implementation that unconditionally blocks whenever a pending record is present will falsely pass.
  * **Criterion 2 (Unfalsifiable / Missing negative control):** Asserts that *"`status` nor `stop` suggests `devflow resume` while that supervisor is live"*. It specifies no negative control asserting that `status` *does* suggest `resume` once the supervisor is confirmed `GONE`. A broken status command that outputs an empty string or error passes Criterion 2.
  * **Criterion 3 (Missing negative control & untestable claim):** Requires a source-scan test for `update_state`, but omits a negative control (a planted un-fsynced write that fails the scanner). Furthermore, the clause *"no gate wait holds a lock"* provides no verifiable check or test harness.
  * **Criterion 5 (Missing negative control):** Asserts that an unrecorded supervisor writes no state and logs why. It lacks the positive/negative control that a supervisor with a *matching* launch ID successfully writes state and does *not* log the refusal.
  * **Criterion 6 (Unfalsifiable escape hatch):** States that eight 999.166 items are *"each fixed with a failing-first test, or accepted with a reason"*. The clause "or accepted with a reason" is an escape hatch allowing all items to be deferred without verification.

---

### 7. Scope Leaks in 50.1-CONTEXT.md
* **Citations:**
  * `50.1-CONTEXT.md:21-22, 176-191` (D-13: folding 999.166).
  * `50.1-CONTEXT.md:162-166` (D-11: retiring `migrate_legacy_state`).
  * `50.1-CONTEXT.md:156-161` (D-10: breaking public APIs in `devflow-core`).
  * `50.1-CONTEXT.md:167-175` (D-12: live-run rig dogfood).
* **Defect:**
  Phase 50.1 was promoted from 999.130 as an inserted dot-phase strictly scoped to: (1) short OS lock, (2) `update_state`, (3) launcher-to-monitor ownership token, (4) gate-waiter registration, and (5) fsync durability.
  The CONTEXT document admits four significant scope leaks:
  1. **D-13 folds eight unrelated items from 999.166:** Includes CLI string fixes (item 1, item 6), socket accept loop EMFILE handling (item 5), and test-harness PID-reaping fixes (item 9).
  2. **D-11 deletes legacy migration logic:** Retiring `migrate_legacy_state` ([`workflow.rs:156-185`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/workflow.rs#L156-L185)) is general technical debt cleanup unrelated to atomic state writes.
  3. **D-10 breaks `devflow-core` public API without constraint:** Permitting breaking changes to `GateFile` and `GateResponse` without backwards compatibility attributes or deprecation cycles.
  4. **D-12 mandates a full Phase 49 live dogfood run to Ship:** Running `scripts/scratch-dogfood-repo.sh` to retroactively validate Phase 50 supervisor stability at HEAD shifts Phase 50 closeout obligations into Phase 50.1.

---

## ## SUSPECTED

### 8. Concurrent Launchers Race to Spawn Redundant Supervisors Under Split Lock (D-08)
* **Citations:**
  * `50.1-CONTEXT.md:144-145` (D-08): *"Whether a coarse per-phase launch exclusion survives, around start, resume and advance entry for example, is Claude's call."*
  * Code: [`crates/devflow-cli/src/commands.rs:367-376`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L367-L376) (`lock::acquire`), [`crates/devflow-cli/src/commands.rs:3309-3321`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-cli/src/commands.rs#L3309-L3321) (`possibly_live_sockets`).
* **Analysis (Speculation / Design Risk):**
  If the long-held per-phase lock is removed and coarse launch exclusion is not implemented:
  1. Launcher A checks `refuse_launch_over_a_live_run`: no supervisor is running.
  2. Launcher B starts concurrently and checks `refuse_launch_over_a_live_run`: no supervisor is running.
  3. Launcher A calls `update_state` to write pending record `L_A`, then invokes `child_cmd.spawn()`.
  4. Launcher B calls `update_state` to write pending record `L_B`, then invokes `child_cmd.spawn()`.
  5. Both child supervisors spawn. Both agents execute in the same worktree simultaneously.
  Leaving coarse launch exclusion as discretionary ("Claude's call") rather than a mandatory requirement risks re-opening multi-supervisor concurrency in the launch window.

---

### 9. Durability Specification (D-09) Leaves Sidecar Directory and `clear_state` Un-fsynced
* **Citations:**
  * `50.1-CONTEXT.md:148-154` (D-09): *"State, gate, response and ack writes all go through one durable helper: fsync of the file before the rename, and of the directory after it."*
  * Code: [`crates/devflow-core/src/supervisor.rs:486-505`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/supervisor.rs#L486-L505) (`write_sidecar`), [`crates/devflow-core/src/workflow.rs:359-385`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/workflow.rs#L359-L385) (`clear_state_with_partial_result_impl`).
  * Crash Table: `50-CRASH-TABLE.md:74` (Row D1).
* **Analysis (Speculation / Incomplete Mitigation):**
  D-09 defines the durable helper exclusively for state, gate, response, and ack files.
  1. In [`crates/devflow-core/src/supervisor.rs:493-495`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/supervisor.rs#L493-L495), `write_sidecar` runs `file.sync_all()` followed by `std::fs::hard_link(&temp, &target)`, but **does not fsync the directory**. Power loss immediately following sidecar creation can leave a socket with a lost sidecar entry.
  2. In [`crates/devflow-core/src/workflow.rs:368`](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/41a384df-3e3f-4db2-a690-c92d269a4d5b/scratchpad/rv/snap/crates/devflow-core/src/workflow.rs#L368), `clear_state` calls `std::fs::remove_file(&path)` with no directory fsync. On abrupt power cut, filesystem journal replay can resurrect the deleted state file. Row D1 is not fully closed without directory synchronization on sidecars and unlinks.

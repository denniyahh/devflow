### Needs my awareness

The Phase 48 recovery patch introduces severe state-machine inconsistencies, invalidates its own test bounds via proxy testing, and leaves unhandled deadlock and stale response paths.

---

#### 1. Holder Identity & PID-Reuse Contention Race
* **Status**: **CONFIRMED**
* **Diff hunk**:
  ```rust
  @@ -2095,15 +2105,19 @@ fn persist_stopped_state(
       let _phase_lock = match (_phase_lock, answered_gate) {
           (Ok(guard), _) => guard,
  -        (Err(lock::LockError::Contended { pid, .. }), Some((stage, holder))) => {
  +        (Err(lock::LockError::Contended { pid, .. }), Some((stage, _holder))) => {
               println!(
                   "{}",
  -                answered_gate_contention_message(phase, stage, holder, &pid)?
  +                answered_gate_contention_message(
  +                    phase,
  +                    stage,
  +                    lock::holder_status(project_root, phase),
  +                    &pid,
  +                )?
               );
               return Ok(());
           }
  ```
* **Mechanism**: 
  `stop_via_gate` observes the pre-reap lock holder (`holder_before_reap`) and passes it into `persist_stopped_state` via `answered_gate`. Line 2108 prefixes `holder` as `_holder` and discards it, dynamically re-sampling `lock::holder_status(project_root, phase)` at the moment of contention.
  If the observed live holder aborts and releases the lock as intended, and a queued process (e.g. `devflow advance`) takes the lock with `pid2`, `lock::holder_status` evaluates `pid2` as `Live { pid: pid2 }`. `answered_gate_contention_message` compares `pid2 == contending_pid` (which evaluates to `true`) and outputs:
  ```text
  stop: phase {phase}'s lock holder (pid {pid2}) is waiting on the gate and will clear phase state as it aborts
  ```
  `stop` returns `Ok(())` without persisting stopped state. A completely unrelated process that took the lock *after* the target exited is claimed as the aborting waiter.
* **Why the test missed it**:
  Test `answered_gate_contention_claims_a_waiter_only_for_the_observed_live_holder` directly invoked `answered_gate_contention_message(phase, Stage::Code, holder, contending_pid)` using synthetic mismatched pairs (`Live { pid: 7 }` vs `"8"`). It did not exercise `persist_stopped_state`, which discards `_holder` and passes the live contended PID to itself.

---

#### 2. Auto-Mode LoopBack Hang / Deadlock on Re-Scan Rejection
* **Status**: **CONFIRMED**
* **Diff hunk**:
  ```rust
  @@ -1775,15 +1785,19 @@ pub(crate) fn advance_with(
                                   // The response is deliberately NOT cleaned up:
                                   // the fall-through gate asks the same question
                                   // about the same stage, so it consumes the same
                                   // answer rather than re-asking a human who has
                                   // already replied.
                                   GateAction::LoopBack(_) => {
                                       reason = Some(augment_unresolved_checkpoint_reason(
                                           reason,
                                           &format!(
                                               "checkpoint added or changed after Code preflight in {}",
                                               unapproved_files.join(", ")
                                           ),
                                       ));
                                   }
  ```
* **Mechanism**:
  When a human rejects the re-scan gate with `LoopBack` in `Mode::Auto`, the response file is intentionally preserved on disk and execution falls through to `handle_stage_failure`. `handle_stage_failure` consumes the `LoopBack` response and invokes `launch_stage` -> `run_preflight`.
  In `Mode::Auto`, `preflight_unattended_launch_check` refuses to launch because the plan contains human-only checkpoints that were never approved. This triggers a third (preflight) gate. Because the response file was already consumed by `handle_stage_failure`, the preflight gate has no response and hangs unattended for 3 days (`gate_timeout_secs()`).
* **Evidence**:
  Admitted verbatim by the author in test comment `rejecting_the_rescan_gate_records_nothing_and_falls_through` (`pipeline_launch.rs:4370-4384`):
  > *"In AUTO, `preflight_unattended_launch_check` refuses that launch precisely BECAUSE the plan declares a human-only checkpoint, so a third gate opens with no response left and the test blocks on the three-day poll instead of failing. Measured, not assumed: the Auto run printed `[DOES NOT HOLD] no plan declares a human-only checkpoint` and spawned zero monitors."*
  
  Rather than fixing the state machine to prevent the unattended launch loop in Auto mode, the author altered the test fixture to `Mode::Supervise`, leaving the production Auto-mode hang unaddressed.

---

#### 3. Misleading Re-Scan Gate Contract & Double-Gating in Supervise Mode
* **Status**: **CONFIRMED**
* **Diff hunks**:
  ```rust
  @@ -1124,7 +1124,7 @@ fn rescan_gate_context(unapproved: &[&CheckpointDeclaration]) -> String {
       format!(
           "[checkpoint re-scan] {} human-only checkpoint(s) in {} were added or changed after this \
            phase's Code preflight — a human must review them before the agent is resumed to decide \
            them itself (approve, loop-to-code, or abort)",
           unapproved.len(),
           plan_files.join(", ")
       )
  ```
  ```rust
  @@ -1756,12 +1766,20 @@ pub(crate) fn advance_with(
                                       Gates::cleanup(project_root, phase, stage);
                                       state.gate_pending = false;
                                       state.checkpoint_approval = recorded_approval(&current);
                                       workflow::save_state(&state)?;
  -                                    return relaunch_checkpoint_session(&mut state, &session_id);
  +                                    if state.mode == Mode::Auto {
  +                                        return relaunch_checkpoint_session(
  +                                            &mut state,
  +                                            &session_id,
  +                                        );
  +                                    }
  +                                    reason = Some(augment_unresolved_checkpoint_reason(
  +                                        reason,
  +                                        "Supervise mode requires a human decision for the declared blocking-human checkpoint",
  +                                    ));
                                   }
  ```
* **Mechanism**:
  1. `rescan_gate_context` explicitly tells the operator: *"a human must review them before the agent is resumed to decide them itself"*. Under `Mode::Supervise`, this statement is false: the agent is never resumed to auto-decide checkpoints (`events_of_kind("checkpoint_auto_decided")` is verified empty).
  2. In `Mode::Supervise`, approving the re-scan gate records the approval, removes the gate file, sets `reason`, and falls through into `handle_stage_failure(project_root, &mut state, stage, reason)`. This immediately opens a second gate on the exact same stage (`Stage::Code`). An operator who just clicked "Approve" expecting execution to proceed is greeted with an immediate second gate prompt demanding a manual decision.

---

#### 4. Stale Abort Writes on Ship Gates with Non-Live Holders
* **Status**: **CONFIRMED**
* **Diff hunk**:
  ```rust
  @@ -1896,15 +1896,23 @@ fn stop_via_gate(
       if !gate_is_current {
           ...
           return Ok((false, None));
       }
  -    if gate.stage != Stage::Ship && !holder_before_reap.may_be_waiting() {
  +    if gate.stage != Stage::Ship && !matches!(holder_before_reap, lock::HolderStatus::Live { .. }) {
           println!(
  -            "stop: phase {phase} {} has no confirmed waiter; no response was written. {}",
  +            "stop: phase {phase} {} has no confirmed live waiter; no response was written. {}",
               gate.stage,
               no_waiter_repair(phase, gate.stage)
           );
           return Ok((false, Some((gate.stage, holder_before_reap))));
       }
       match Gates::reap(
  ```
* **Mechanism**:
  For all stages except `Stage::Ship`, `stop_via_gate` halts without writing if `holder_before_reap` is not `Live`. For `Stage::Ship`, this check is bypassed.
  - If `holder_before_reap` is `Recycled`, `Gates::reap` writes an abort response to `response-XX-ship.json`. Execution then proceeds to `persist_stopped_state`, which fails on lock contention with the recycled PID and returns `Err` without marking phase state as stopped. The aborted response file is permanently abandoned on disk. When the operator runs the recommended repair (`devflow ship --phase N`), `devflow ship` encounters and executes the stale abort response.
  - If `holder_before_reap` is `NoHolder`, `Gates::reap` writes an abort response file to disk, even though there is no poller to ever read or clean it up.

---

#### 5. Phantom "Treating as No Waiter" Followed by Immediate Error on Recycled PID
* **Status**: **CONFIRMED**
* **Diff hunk**:
  ```rust
  @@ -1867,14 +1867,17 @@ pub(crate) fn stop(project_root: &Path, phase: PhaseId) -> Result<(), CliError>
       let (gate_answered, gate_holder) = stop_via_gate(project_root, phase)?;
       let signal_sent = if gate_answered {
           false
       } else {
           stop_via_lock(project_root, phase)?
       };
       if matches!(gate_holder, Some((_, lock::HolderStatus::Recycled { .. }))) {
           println!("stop: phase {phase}'s lock holder was recycled; treating it as no waiter");
       }
  ```
* **Mechanism**:
  When `stop_via_gate` encounters a non-Ship recycled lock holder, it returns `(false, Some((stage, Recycled)))`.
  Line 1874 logs: `"stop: phase {phase}'s lock holder was recycled; treating it as no waiter"`.
  However, because `gate_answered` was `false`, line 1871 had *already* executed `stop_via_lock(project_root, phase)?`. Inside `stop_via_lock`, start-time verification detects the recycled PID and returns:
  ```text
  refusing to signal pid {pid} for phase {phase} — it is not the process that took the lock. ... the pid has been recycled
  ```
  `stop` aborts with an `Err`. It does not "treat it as no waiter"; it fails to stop or mark the phase stopped.

---

#### 6. Abandoned Stale Gate Files on Disk
* **Status**: **CONFIRMED**
* **Diff hunk**:
  ```rust
  @@ -1891,8 +1891,14 @@ fn stop_via_gate(
       let gate_is_current = workflow::load_state(project_root, phase)
           .is_ok_and(|state| state.gate_pending && state.stage == gate.stage);
       if !gate_is_current {
           println!(
               "stop: phase {phase} {} has a stale gate request; no response was written. {}",
               gate.stage,
               no_waiter_repair(phase, gate.stage)
           );
           return Ok((false, None));
       }
  ```
* **Mechanism**:
  When `gate_is_current` is false (e.g. gate file present, but `state.gate_pending` is false), `stop_via_gate` refuses to write a response and falls through to `stop_via_lock`. However, the physical file `.devflow/gate-XX-<stage>.json` is never deleted or unlinked. It remains on disk, continually surfacing in `Gates::list_open` and `gate_show`.

---

#### 7. TOCTOU Window Between Start-Time Confirmation and Signal
* **Status**: **SUSPECTED**
* **Diff hunk**:
  ```rust
  @@ -2038,7 +2048,7 @@ fn stop_via_lock(project_root: &Path, phase: PhaseId) -> Result<bool, CliError>
       if !agent::is_same_process(pid, recorded_start) {
           return Err(...);
       }
       if agent::terminate(pid) {
           ...
       }
  ```
* **Mechanism**:
  Standard POSIX PID race: `agent::is_same_process` checks `/proc/{pid}/stat`. If the process terminates immediately after this check and the OS reallocates the PID within the interleaving window, `agent::terminate(pid)` (`kill(pid, SIGTERM)`) signals the new occupant. On modern Linux, closing this requires `pidfd_open(2)`.

---

### Handled

- Checked diff `9a7cc61..b69a9ab` across all files (`commands.rs`, `pipeline_launch.rs`, `stop_e2e.rs`).
- Negative control audit: verified that test `answered_gate_contention_claims_a_waiter_only_for_the_observed_live_holder` isolates function `answered_gate_contention_message` and does not cover the callsite mutation at line 2108.
- Validated state transition invariants: verified `preflight_unattended_launch_check` failure mode documented in test comments for Auto mode re-scan loop-backs.

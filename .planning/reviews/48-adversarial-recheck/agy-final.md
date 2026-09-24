## Needs my decision

- **Block or accept Phase 48 recovery patch**: The patch introduces a **CONFIRMED race condition** in the test fixture and a **CONFIRMED correctness regression** in `stop_with_existing_response`.
  - **Option 1 (Recommended)**: Reject the patch until the two confirmed regressions (the test-fixture gate race and the Ship `stop_with_existing_response` omission) and the dead-code path in `stop()` are fixed.
  - **Option 2**: Accept conditionally and require immediate follow-up patch fixing the missing `ship_without_holder` guard in `stop_with_existing_response` and restoring protocol-file synchronization in `supervise_rescan_approval_records_but_requires_a_human_gate`.

---

## Needs my awareness

### 1. [CONFIRMED RACE CONDITION] Gate protocol cleanup race in `supervise_rescan_approval_records_but_requires_a_human_gate`

* **Diff hunk** (`crates/devflow-cli/src/pipeline_launch.rs:4385-4405`):
```rust
-            while response_path.exists() {
+            loop {
+                if workflow::load_state(&writer_root, phase)
+                    .is_ok_and(|state| state.checkpoint_approval == expected_approval)
+                {
+                    break;
+                }
                 assert!(
                     std::time::Instant::now() < deadline,
-                    "the re-scan response was not consumed"
+                    "approval was not persisted before the human Code gate"
                 );
                 std::thread::sleep(std::time::Duration::from_millis(5));
             }
-            assert_eq!(
-                workflow::load_state(&writer_root, phase)
-                    .expect("approval must be persisted before the human Code gate")
-                    .checkpoint_approval,
-                expected_approval,
-                "Supervise approval must durably record the declaration before the next gate"
-            );
             write_gate_response(&writer_root, phase, Stage::Code, false, Some("abort"));
```
* **Mechanism**:
  The pre-patch code waited `while response_path.exists()` until `advance_for_stage` consumed and deleted `response-111-code.json`. The patch replaced this with polling `workflow::load_state(...) == expected_approval`.
  `advance_for_stage` saves state (`workflow::save_state`) *before* executing `Gates::cleanup(root, phase, Stage::Code)` to unlink the re-scan response.
  When `load_state` sees `checkpoint_approval == expected_approval`, the background writer immediately writes the second (`abort`) response to `response-111-code.json`.
  The main thread then runs `Gates::cleanup`, unlinking the newly written abort response file. When `advance_for_stage` proceeds to open the human Code gate, its response is already destroyed. The main thread blocks indefinitely on gate polling.
* **Reproduction**:
  Run `supervise_rescan_approval_records_but_requires_a_human_gate` with a slight scheduling delay between `workflow::save_state` and `Gates::cleanup` in `advance_for_stage`. The writer writes `response-111-code.json`, `Gates::cleanup` deletes it, and `advance_for_stage` hangs waiting for human Code gate input until deadline timeout.
* **Limits**: This is a test-fixture race; it does not affect CLI production runtime, but causes intermittent CI hangs in child test runs.

---

### 2. [CONFIRMED CORRECTNESS REGRESSION] Ship no-holder path broken on concurrent / pre-existing response in `stop_with_existing_response`

* **Diff hunk** (`crates/devflow-cli/src/commands.rs:1925-1928` vs lines `1989-1994`):
```rust
+    let ship_without_holder =
+        gate.stage == Stage::Ship && matches!(holder_before_reap, lock::HolderStatus::NoHolder);
+    if !matches!(holder_before_reap, lock::HolderStatus::Live { .. }) && !ship_without_holder {
```
  versus unchanged `stop_with_existing_response`:
```rust
     match GateAction::from_response(&response) {
         GateAction::Abort(_) if matches!(holder_before_reap, lock::HolderStatus::Live { .. }) => {
             println!(
                 "stop: phase {phase} {stage} already has an abort response awaiting a live lock holder; stop did not write it"
             );
             Ok((
                 true,
                 Some((stage, holder_before_reap, holder_identity_before_reap)),
             ))
         }
```
* **Mechanism**:
  `stop_via_gate` introduced `ship_without_holder` so that a Ship gate with `NoHolder` can be reaped and answered with `Abort`.
  However, if `Gates::reap` returns `Err(GateError::AlreadyResponded { .. })` (e.g., duplicate `devflow stop` invocation, or concurrent sweep/human response), it calls `stop_with_existing_response`.
  `stop_with_existing_response` checks `GateAction::Abort(_) if matches!(holder_before_reap, lock::HolderStatus::Live { .. })`. It lacks the `ship_without_holder` branch.
  Since `holder_before_reap` is `NoHolder`, the guard fails. `stop_with_existing_response` falls through, returns `gate_answered = false`, and attempts `stop_via_lock`.
  This breaks idempotency for `devflow stop` on a Ship gate without a holder: the first call succeeds, but a subsequent call or race treats the abort response as unhandled.
* **Reproduction**:
  1. Create a phase at `Stage::Ship` with `gate_pending = true` and no lock file (`NoHolder`).
  2. Write an abort response to `response-{phase}-ship.json`.
  3. Call `stop(root, phase)`.
  4. `Gates::reap` returns `AlreadyResponded`.
  5. `stop_with_existing_response` rejects the abort response because `holder_before_reap` is not `Live`, returning `gate_answered = false`.
* **Limits**: Only triggers at `Stage::Ship` when a response file already exists prior to `Gates::reap`.

---

### 3. [CONFIRMED DEAD CODE / FALSE-CLAIM REGRESSION] Dead code and misleading control flow in `stop` for recycled lock holders

* **Diff hunk** (`crates/devflow-cli/src/commands.rs:1869-1888` and `1925-1928`):
```rust
pub(crate) fn stop(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    let (gate_answered, gate_holder) = stop_via_gate(project_root, phase)?;
    let signal_sent = if gate_answered {
        false
    } else {
        stop_via_lock(project_root, phase)?
    };
    if matches!(
        gate_holder,
        Some((_, lock::HolderStatus::Recycled { .. }, _))
    ) {
        println!("stop: phase {phase}'s lock holder was recycled; treating it as no waiter");
    }
    persist_stopped_state(
        project_root,
        phase,
        gate_answered.then_some(gate_holder).flatten(),
        signal_sent,
    )
}
```
* **Mechanism**:
  Before this patch, `Stage::Ship` bypassed the `Live` check in `stop_via_gate` and returned `gate_answered = true`, reaching line 1877 to print `"treating it as no waiter"`.
  Now, `ship_without_holder` explicitly excludes `Recycled` holders. Across all stages, if `holder_before_reap` is `Recycled`, `stop_via_gate` returns `gate_answered = false`.
  `stop` evaluates `else { stop_via_lock(project_root, phase)? }`.
  `stop_via_lock` encounters the recycled PID and returns `Err(CliError::Message("refusing to signal ..."))`.
  Because of `?`, `stop` exits immediately with an error.
  Lines 1877–1882 (`println!("stop: phase {phase}'s lock holder was recycled; treating it as no waiter")`) and line 1883 (`persist_stopped_state`) are unreachable dead code on all paths.
* **Reproduction**:
  Call `stop(root, phase)` with a recycled lock PID at any stage (including `Stage::Ship` as in unit test 4806). `stop` returns `Err("refusing to signal")`; the `"treating it as no waiter"` message is never emitted, and `persist_stopped_state` is never reached.

---

### 4. [SUSPECTED TOCTOU] Non-atomic sampling of holder status and identity in `stop_via_gate`

* **Diff hunk** (`crates/devflow-cli/src/commands.rs:1908-1910`):
```rust
    let holder_before_reap = lock::holder_status(project_root, phase);
    let holder_identity_before_reap = lock::holder_identity(project_root, phase);
```
* **Mechanism**:
  `lock::holder_status` and `lock::holder_identity` perform separate, un-synchronized filesystem reads of the lock file. If the process terminates or a new process acquires the lock between these two statements, `holder_before_reap` will be `Live { pid: A }` while `holder_identity_before_reap` contains PID `B` or `None`.
  When `persist_stopped_state` subsequently invokes `answered_gate_contention_message`, the identity check will mismatch against the actual waiter, falsely concluding the waiter was not observed.
* **Limits**: Relies on a narrow thread-scheduling window between two consecutive file reads.

---

### 5. [SUSPECTED RESOURCE LEAK / TIMEOUT PROXY] Orphan process leak in `doctor` stray test fixture

* **Diff hunk** (`crates/devflow-cli/src/commands.rs:7378-7388`):
```rust
         fn doctor_finds_a_real_stray_and_never_signals_it_across_two_runs() {
             let mut child = std::process::Command::new("sh")
                 .arg("-c")
-                .arg("trap cleanup TERM INT; sleep 30")
+                .arg("trap cleanup TERM INT; sleep 120")
```
* **Mechanism**:
  1. `child.kill()` at test cleanup sends SIGKILL only to the `sh` process (`pid`). In POSIX shells, child process `sleep 120` is reparented to PID 1 / systemd and is not terminated by killing `sh`. Increasing the sleep from 30s to 120s leaves an orphan `sleep` process running on the host for 2 full minutes per test run.
  2. The test asserts `agent::agent_running(pid)` to verify `doctor` did not signal the process. Bumping to 120s confirms the assertion is pinned by a fixed wall-clock timeout rather than an intercepting signal monitor; on an overloaded test runner exceeding 120s, a natural process exit is misreported as `"doctor() itself must not signal"`.

---

## Handled

- Verified Auto rejection-to-Supervise handoff diff hunks in `pipeline_launch.rs`: mode switch logic remains identical to pre-patch; changes are restricted to user-facing notification strings and test assertions.
- Verified `answered_gate_contention_claims_a_waiter_only_for_the_observed_live_holder` unit test logic: same-identity holder comparison correctly fails when PID or start time shifts.

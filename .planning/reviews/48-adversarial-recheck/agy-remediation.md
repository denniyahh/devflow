### Needs your decision

1. **Remediation strategy for irreversible mode demotion in `park_auto_rescan_repair`:**
   - **Problem:** When an operator rejects a checkpoint re-scan in Auto mode, `park_auto_rescan_repair` mutates `state.mode = Mode::Supervise` in place. Because `devflow resume` lacks any `--mode` flag, this permanently strips `Auto` capabilities from all subsequent stages (Code, Validate, Ship) with no route to restore unattended operation.
   - **Option A (Recommended):** Preserve `state.mode == Mode::Auto` across the park, track the parked state via a dedicated state flag/reason (e.g. `parked_for_repair: bool`), and let `resume` validate that the plan was repaired (or require an explicit `--mode supervise` / `--supervised` flag to convert the run).
   - **Option B:** Add a `--mode <mode>` flag to `devflow resume` so operators can explicitly select their target mode upon unparking.

---

### Needs your awareness

#### 1. [CONFIRMED] Stale Gate Response Artifact & Command Lockout at Ship Gates on Stop
- **Diff Hunk:** [`crates/devflow-cli/src/commands.rs:1888-1915, 1968-1995, 2038-2060`](file:///crates/devflow-cli/src/commands.rs#L1888-L1915)
- **Mechanism:**
  In `stop_via_gate`, non-Ship gates enforce a live waiter check before writing an abort response:
  ```rust
  if gate.stage != Stage::Ship && !matches!(holder_before_reap, lock::HolderStatus::Live { .. }) {
      println!("stop: phase {phase} {} has no confirmed live waiter; no response was written...", ...);
      return Ok((false, Some((gate.stage, holder_before_reap, holder_identity_before_reap))));
  }
  ```
  `Stage::Ship` is explicitly exempt from this check. When `gate.stage == Stage::Ship` and the lock is held by a recycled process, `stop_via_gate` calls `Gates::reap(...)`, immediately writing an abort response to `.devflow/response-{phase}-ship`.
  Next, `stop` calls `persist_stopped_state`. `lock::acquire` fails with `Contended` (the recycled PID is still alive). `answered_gate_contention_message` matches arm 2 (`HolderStatus::Recycled`) and returns an `Err(CliError)`.
- **Impact & Reproduction:**
  1. `stop` returns an error (`Err: "stop: nothing is waiting on phase ... ship gate — pid ... is not its lock holder; phase state was not marked stopped. devflow ship --phase ..."`).
  2. Phase state is **not** marked stopped (`state.stopped` remains `false`, `state.gate_pending` remains `true`).
  3. The abort response file `.devflow/response-{phase}-ship` is left on disk without rollback.
  4. The operator runs the suggested repair: `devflow ship --phase <phase>`. It immediately aborts because `.devflow/response-{phase}-ship` exists. Furthermore, `devflow gate respond` fails with `AlreadyResponded`. The gate is rendered permanently unanswerable.
- **Evidence from the diff:**
  The unit test `stop_at_a_ship_gate_with_a_recycled_holder_claims_no_waiter` (lines 4410–4430) explicitly documents and asserts this exact artifact leakage:
  ```rust
  let err = stop(root, phase).unwrap_err().to_string();
  assert!(err.contains("not marked stopped"), "{err}");
  assert!(err.contains("devflow ship --phase 4806"), "{err}");
  assert!(Gates::response_path(root, phase, Stage::Ship).exists()); // <-- STALE ARTIFACT ASSERTED
  ```

---

#### 2. [CONFIRMED] Repudiation Bypass & Silent Approval on Supervised Resume
- **Diff Hunk:** [`crates/devflow-cli/src/pipeline_launch.rs:1115-1150, 1165-1180, 1805-1825`](file:///crates/devflow-cli/src/pipeline_launch.rs#L1115-L1150)
- **Mechanism:**
  1. In Auto mode, Claude adds a human checkpoint. DevFlow detects it and opens the re-scan gate.
  2. The operator rejects the re-scan gate (`GateAction::LoopBack`).
  3. `park_auto_rescan_repair` runs: it sets `state.mode = Mode::Supervise`, `state.stopped = true`, leaves `state.checkpoint_approval` unchanged, and prints:
     `"Review the changed plan, then run devflow resume --phase {}."`
  4. If the operator runs `devflow resume --phase <P>` (e.g. intending to supervise the phase without removing the plan task), `resume` calls `launch_stage(Stage::Code)`.
  5. `launch_stage` executes `run_preflight`. In `Mode::Supervise`, preflight passes and executes its recording site:
     ```rust
     state.checkpoint_approval = recorded_approval(&current);
     ```
- **Impact:**
  The unapproved checkpoint that the operator explicitly rejected at the re-scan gate is recorded into `state.checkpoint_approval` as preflight-approved. The human rejection is erased without any affirmative approval having ever been given.

---

#### 3. [CONFIRMED] Unsafe Stage Restart Violating D-04 Guarantees
- **Diff Hunk:** [`crates/devflow-cli/src/pipeline_launch.rs:1115-1150, 1180-1200`](file:///crates/devflow-cli/src/pipeline_launch.rs#L1115-L1150)
- **Mechanism:**
  D-04 establishes that handling a checkpoint must resume the existing session (`relaunch_checkpoint_session` via `resume_launch_shape(state.phase, session_id)`) to preserve conversational context, completed tasks, and avoid dirty-worktree re-execution.
  `park_auto_rescan_repair` instructs the operator to run `devflow resume --phase <P>`. `resume` does not invoke `relaunch_checkpoint_session` or pass `--resume`; it spawns a brand new `Stage::Code` agent run from task 1 against the dirty worktree left by the exited session.

---

#### 4. [CONFIRMED] Dead Negative Control and Proxy Test in Auto-Rescan Suite
- **Diff Hunk:** [`crates/devflow-cli/src/pipeline_launch.rs:4400-4485`](file:///crates/devflow-cli/src/pipeline_launch.rs#L4400-L4485)
- **Mechanism:**
  In `auto_rescan_rejection_parks_for_supervised_repair`:
  ```rust
  let writer = std::thread::spawn(move || {
      let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
      loop {
          let state = workflow::load_state(&writer_root, phase);
          if state.as_ref().is_ok_and(|state| state.stopped) {
              return;
          }
          if !writer_response_path.exists() {
              write_gate_response(&writer_root, phase, Stage::Code, false, Some("abort"));
              return;
          }
          ...
  ```
  In `park_auto_rescan_repair`:
  ```rust
  workflow::save_state(state)?;             // 1. Sets state.stopped = true
  Gates::cleanup(project_root, state.phase, stage)?; // 2. Deletes writer_response_path
  ```
- **Why the negative control fails:**
  Because `save_state` precedes `cleanup`, `state.stopped == true` is visible on disk before `writer_response_path` is deleted. The writer thread always hits `if state.stopped { return; }` and terminates immediately. The arm writing `"abort"` is dead code and never executes under any circumstance. If `park_auto_rescan_repair` regressed and opened a third gate after setting `stopped`, the writer would not supply an abort response; it would have already exited.
- **Why it proxies:**
  The test verifies:
  1. `resumed.mode == Mode::Supervise`
  2. `!resumed.stopped`
  3. `events_of_kind("stage_launched").any(...)`
  It never runs the resumed stage to completion, never tests whether `preflight` re-approves the rejected declarations, and never measures whether supervised execution can actually complete.

---

#### 5. [CONFIRMED] Misleading Context Claim in Re-scan Prompt
- **Diff Hunk:** [`crates/devflow-cli/src/pipeline_launch.rs:1090-1110, 1140-1150`](file:///crates/devflow-cli/src/pipeline_launch.rs#L1090-L1110)
- **Detail:**
  `rescan_gate_context` tells the operator:
  `"[checkpoint re-scan] ... review them before continuing (approve, park for supervised repair, or abort)"`
  and prints:
  `"... phase {} is parked for supervised repair. Review the changed plan, then run devflow resume --phase {}."`
  This claims to the operator that only the *repair* is supervised, masking the fact that the phase lifecycle is being permanently stripped of `Auto` mode.

---

#### 6. [SUSPECTED] Non-Atomic State Persistence vs Gate Cleanup
- **Diff Hunk:** [`crates/devflow-cli/src/pipeline_launch.rs:1130-1135`](file:///crates/devflow-cli/src/pipeline_launch.rs#L1130-L1135)
- **Detail:**
  `park_auto_rescan_repair` executes `workflow::save_state(state)?` (clearing `gate_pending`) before `Gates::cleanup(project_root, state.phase, stage)?`. If file deletion fails or process termination occurs between these lines, `state.gate_pending` is `false` while orphaned gate/response protocol files remain on disk.

---

### Detail

- **Exact Reproduction for Ship Gate Lockout (Finding 1):**
  1. Create open gate at `Stage::Ship`.
  2. Write a lock file containing `"<current_pid>\n<start_time + 1>"` (simulating a recycled PID).
  3. Invoke `devflow stop --phase <phase>`.
  4. Observe `stop` fails with `not marked stopped`.
  5. Inspect `.devflow/response-<phase>-ship`: file exists containing an unconsumed abort response.
  6. Attempt `devflow ship --phase <phase>`: command fails due to existing abort response. Attempt `devflow gate respond`: fails with `AlreadyResponded`.

## CONFIRMED

### C-1. Supervise re-scan rejection silently consumes stale response at stage failure gate

**File:** `crates/devflow-cli/src/pipeline_launch.rs:1809-1873`, `crates/devflow-cli/src/pipeline_outcomes.rs:903-931`, `crates/devflow-cli/src/pipeline_gate.rs:371-428`, `crates/devflow-core/src/gates.rs:263-289`

**Mechanism:**
1. During `advance_with` at `Stage::Code`, an unapproved human checkpoint (new or changed relative to the preflight baseline) opens a re-scan gate via `run_gate_with_timeout` (`pipeline_launch.rs:1809-1816`).
2. This creates `.devflow/gates/{phase}-code.json`, waits for response, and receives an operator rejection via `devflow gate reject` (writing `GateResponse { approved: false, ... }` to `.devflow/gates/{phase}-code.response.json`).
3. `run_gate_with_timeout` reads the response, writes `.ack.json`, and returns `GateAction::LoopBack(Stage::Code)` (`pipeline_gate.rs:428`).
4. In `pipeline_launch.rs:1853-1873`:
   - Under `Mode::Auto`, it invokes `park_auto_rescan_repair` (which performs cleanup and parks the phase).
   - Under `Mode::Supervise`, it sets `reason = Some(...)` and does **not** call `Gates::cleanup(project_root, phase, stage)`. (In contrast, `GateAction::Advance` at line 1830 explicitly calls `Gates::cleanup`, citing: `"CR-01: drop the gate/response/ack now, so a later Code gate in this run cannot silently consume this approval as its own answer"`).
5. Execution completes the checkpoint match and falls through directly to line 1922: `handle_stage_failure(project_root, &mut state, stage, reason)`.
6. `handle_stage_failure` (`pipeline_outcomes.rs:903-931`) formats a `[never-silent] stage Code failed: ...` context and calls `run_gate` -> `run_gate_with_timeout` (`pipeline_gate.rs:371-428`).
7. `run_gate_with_timeout` writes the new gate request file, then immediately calls `Gates::poll_response` (`pipeline_gate.rs:423`).
8. In `Gates::poll_response` (`gates.rs:276-280`), `Self::response_path` still exists from the re-scan gate rejection. It immediately reads and parses `.devflow/gates/{phase}-code.response.json` on the first iteration without blocking or waiting for operator input.
9. `handle_stage_failure` receives `GateAction::LoopBack(_)`, cleans up the gate, and immediately invokes `launch_stage(state, None, Some(stage))` (`pipeline_outcomes.rs:927`).
10. The human operator is never given an opportunity to decide on the failure gate; DevFlow instantly relaunches the Code agent in an unattended loop, violating Supervise mode guarantees and the CR-01 requirement.

**Reproduction:**
Configure a phase in `Mode::Supervise` at `Stage::Code` with an unapproved checkpoint. Answer the re-scan gate with `devflow gate reject --phase <P> --stage code --reason "fix checkpoint"`.
Observed outcome: `advance_with` reads the rejection, emits `gate written: .devflow/gates/<P>-code.json — awaiting response` for the subsequent failure gate, but immediately consumes the uncleaned `.response.json` in `poll_response` on the first poll iteration, relaunching the Code stage without waiting for human intervention.

**Severity:** high — bypasses human review on a failed stage in Supervise mode and violates CR-01 by immediately re-executing Code unattended upon a checkpoint rejection.

---

### C-2. Implicit `recover --clean` deletes state of an active gate waiter

**File:** `crates/devflow-core/src/recover.rs:93-110`, `:173-196`; `crates/devflow-cli/src/pipeline_gate.rs:369`; `crates/devflow-core/src/workflow.rs:309`

**Mechanism:**
1. When a workflow sits waiting at a human gate (e.g. `run_gate_with_timeout`), the monitor process holds the per-phase lock. The stage's agent child process has already exited.
2. If the gate wait remains open past the staleness threshold (e.g. >24 hours awaiting manual review), `is_stale_state(&state)` evaluates to `true`.
3. An operator or cron maintenance job invokes `devflow recover --clean` (without `--phase`).
4. `recover::clean` (`recover.rs:93-110`) iterates over persisted states and checks `agent_pid_for(&state).is_some_and(crate::agent::agent_running)`. Because the agent process exited, this check returns `false`.
5. Unlike `clean_phase` (which checks `lock::acquire`) and `inspect_all` (which records `lock::holder`), `recover::clean` never inspects or acquires the phase lock.
6. `recover::clean` executes `workflow::clear_state(project_root, phase)?;` (line 109), deleting `.devflow/state-<phase>.json` while the monitor process remains alive and polling for gate responses.
7. Subsequent gate resolution finds state missing; if the monitor process crashes or exits, persisted workflow and cron recovery state has been permanently lost.

**Reproduction:**
Create a state file with `started_at` older than 24 hours and a non-running agent PID. Hold the corresponding `.devflow/phase-<phase>.lock` via an active process. Run `devflow recover --clean`.
Observed outcome: `workflow::clear_state` unlinks the state file despite the active phase lock.

**Severity:** high — an automated or operator sweep deletes persisted state for a live gate-waiting process without verifying lock ownership, preventing honest recovery if the waiting monitor terminates.

---

### C-3. Explicit `recover --clean --phase <phase>` claims success and exits 0 on lock contention

**File:** `crates/devflow-cli/src/commands.rs:2552-2567`, `crates/devflow-core/src/recover.rs:136-145`

**Mechanism:**
1. An operator or automated script runs `devflow recover --clean --phase <phase>`.
2. `commands.rs:2556` calls `recover::clean_phase(project_root, phase)?`.
3. In `recover.rs:137-144`, `clean_phase` attempts non-blocking lock acquisition:
   ```rust
   let guard = match crate::lock::acquire(project_root, phase) {
       Ok(guard) => guard,
       Err(crate::lock::LockError::Contended { pid, .. }) => {
           return Ok(vec![format!(
               "phase {phase} is live or contended by pid {pid}; recover --clean deleted neither state nor gate files"
           )]);
       }
       Err(err) => return Err(err.into()),
   };
   ```
4. If contended (by an active waiter or a recycled live PID), `clean_phase` aborts deletion and returns `Ok(vec![warning])`.
5. In `commands.rs:2560-2566`, `recover_cmd` iterates over `warnings` and prints `warning: ...`.
6. Immediately following the warnings, `recover_cmd` executes line 2564:
   ```rust
   match phase {
       Some(phase) => println!("cleaned up workflow state for phase {phase}"),
       None => println!("cleaned up stale workflow state"),
   }
   return Ok(());
   ```
7. The process exits with status 0, printing a message asserting that workflow state was cleaned up when zero files were deleted.

**Reproduction:**
Acquire `.devflow/phase-48.lock` from an external process, then run `devflow recover --clean --phase 48`.
Observed output:
```
warning: phase 48 is live or contended by pid <pid>; recover --clean deleted neither state nor gate files
cleaned up workflow state for phase 48
```
Exit status: 0. State and gate files remain untouched on disk.

**Severity:** medium — contradicts operator reporting invariants; automated wrappers checking exit status 0 assume cleanup succeeded and proceed to launch conflicting runs against existing state.

---

### C-4. `gate approve` and `gate reject` bypass live waiter check for recycled lock holders at Ship gate

**File:** `crates/devflow-cli/src/commands.rs:1414-1420`, `:1482-1488`, `:1920-1928`; `crates/devflow-cli/src/pipeline_gate.rs:518-527`

**Mechanism:**
1. Requirement SURV-02 and Phase 48 design specify: "Ship's stored-response exception only for NoHolder". Test `commands.rs:4436` asserts: "The Ship exception is intentionally narrow: no holder permits a stored response for manual recovery, but a recycled live PID does not."
2. In `stop_via_gate` (`commands.rs:1920-1928`), this invariant is enforced:
   ```rust
   let ship_without_holder =
       gate.stage == Stage::Ship && matches!(holder_before_reap, lock::HolderStatus::NoHolder);
   if !matches!(holder_before_reap, lock::HolderStatus::Live { .. }) && !ship_without_holder {
       // Refuses to write response
   }
   ```
3. However, in `gate_respond` (`commands.rs:1414-1420`), the check is:
   ```rust
   let holder_before_response = lock::holder_status(project_root, phase);
   if stage != Stage::Ship && !holder_before_response.may_be_waiting() {
       return Err(CliError::Message(...));
   }
   ```
4. If `stage == Stage::Ship` and `holder_before_response` is `HolderStatus::Recycled` (a dead monitor's PID has been recycled by an unrelated live process), `stage != Stage::Ship` evaluates to `false`. The check is bypassed and `Gates::respond` writes the response.
5. `gate_respond` advises: `"no confirmed waiter remains; use devflow ship --phase for Ship recovery"`.
6. When the operator runs `devflow ship --phase <phase>`, `ship_override` (`pipeline_gate.rs:518-527`) invokes `lock::acquire(project_root, phase)`. Because the recycled PID is alive, `lock::acquire` returns `LockError::Contended` and errors out: `"another devflow process (pid ...) holds the per-phase lock — refusing to race its poll of the Ship gate response"`.
7. The operator is unable to complete Ship recovery, leaving an unconsumable gate response on disk.

**Reproduction:**
Create an open Ship gate for a phase whose lock file records the PID of an unrelated live process (e.g. PID 1). Run `devflow gate approve --phase <P> --stage ship`.
Observed outcome: `gate approve` writes `.devflow/gates/<P>-ship.response.json` and exits 0, instructing the user to run `devflow ship --phase <P>`. Running `devflow ship --phase <P>` fails with `LockError::Contended` and refuses execution.

**Severity:** medium — violates the SURV-02 invariant restricting stored Ship responses strictly to `NoHolder`, creating dangling gate responses that cannot be consumed by the recovery command.

---

## SUSPECTED

None.

---

## CHECKED AND CLEAN

- Unique PID-and-monotonic-timestamp temporary paths in `workflow::write_state_atomic` prevent collisions and torn writes: `crates/devflow-core/src/workflow.rs:186-245`.
- Gate response publishing via atomic hard link (`publish_response_exclusive`) reliably enforces first-writer-wins and rejects competing writers: `crates/devflow-core/src/gates.rs:387-417`.
- Coordination flock file (`.coord`) serializes phase lock publication, stale lock detection, and reclamation against races: `crates/devflow-core/src/lock.rs:194-260`, `:409-438`.
- Checkpoint parsing, fence tracking, and fallback re-scan for unclosed fences prevent stealth checkpoint omissions: `crates/devflow-core/src/verify.rs:179-220`, `:274-321`.
- Preflight checkpoint recording runs strictly from `CheckpointApproval::Pending`, preventing loop-backs from adopting unapproved checkpoints: `crates/devflow-cli/src/preflight.rs:1315-1326`, `:1460-1464`.
- Child-process PATH test harness enforces non-zero filter counts and exact summary assertions, preventing vacuous passes: `crates/devflow-core/src/test_support.rs:154-220`.
- Stage-binding verification in `advance_with` rejects advancing a stage mismatched with the launched monitor: `crates/devflow-cli/src/pipeline_launch.rs:1648-1673`.

## CONFIRMED

### C-1. `recover::clean_report` deletes cron records without acquiring the per-phase lock
**File:** [snapshot/crates/devflow-core/src/recover.rs:149-159](snapshot/crates/devflow-core/src/recover.rs#L149-L159); [snapshot/crates/devflow-core/src/ship.rs:69](snapshot/crates/devflow-core/src/ship.rs#L69)
**Mechanism:** 
1. Fix A added per-phase lock acquisition to the state-clearing loop in `recover::clean_report` ([recover.rs:128-138](snapshot/crates/devflow-core/src/recover.rs#L128-L138)). The lock guard `_guard` drops at the end of each iteration of `workflow::list_states`.
2. After the loop and after `remove_stale_locks`, `clean_report` runs an orphan-cron sweep at lines 149–159:
   ```rust
   for instructions in crate::ship::list_cron_instructions(project_root) {
       if workflow::state_path(project_root, instructions.phase).exists() {
           continue;
       }
       if let Err(err) = crate::ship::delete_cron_instructions(project_root, instructions.phase) {
           warnings.push(...);
       }
   }
   ```
3. Unlike `clean_phase` ([recover.rs:170-192](snapshot/crates/devflow-core/src/recover.rs#L170-L192)), which holds the phase lock across state, gate, and cron deletion, this loop acquires no lock.
4. If a live process holds `lock-NN` during an early initialization window, a restart, or a ship-retry window where `.devflow/state-NN.json` is temporarily absent or being rewritten, `clean_report` sees `!state_path.exists()` and unconditionally calls `delete_cron_instructions`, destroying the live lock holder's pending cron instructions without lock synchronization.
**Reproduction:** In an initialized repository, hold a phase lock (`.devflow/lock-42`) with a live process, create `.devflow/cron-instructions-42.json`, and ensure `.devflow/state-42.json` is absent. Run `devflow recover --clean`. The sweep outputs warnings about keeping locked phases if present, but silently deletes `.devflow/cron-instructions-42.json`. The live process loses its cron record.
**Severity:** high — breaks the invariant that every state deleter holds the per-phase lock; destructive deletion occurs out from under a live lock holder.

---

### C-2. `recover::clean_report` deletes phase state without cleaning gate files, permanently orphaning open gates
**File:** [snapshot/crates/devflow-core/src/recover.rs:128-140](snapshot/crates/devflow-core/src/recover.rs#L128-L140); [snapshot/crates/devflow-cli/src/commands.rs:1316](snapshot/crates/devflow-cli/src/commands.rs#L1316); [snapshot/crates/devflow-cli/src/commands.rs:1556-1558](snapshot/crates/devflow-cli/src/commands.rs#L1556-L1558); [snapshot/crates/devflow-cli/src/commands.rs:1737](snapshot/crates/devflow-cli/src/commands.rs#L1737)
**Mechanism:**
1. `recover::clean_phase` explicitly cleans gate files for all five stages under the lock before clearing state ([recover.rs:180-189](snapshot/crates/devflow-core/src/recover.rs#L180-L189)):
   ```rust
   for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate, Stage::Ship] {
       crate::gates::Gates::cleanup(project_root, phase, stage)?;
   }
   workflow::clear_state(project_root, phase)?;
   ```
2. In contrast, `recover::clean_report` ([recover.rs:137-139](snapshot/crates/devflow-core/src/recover.rs#L137-L139)) only calls `workflow::clear_state(project_root, phase)?;` and omits `Gates::cleanup`.
3. When `recover --clean` sweeps a stale phase that had an open gate (e.g. `.devflow/gates/12-Code.json`), the state file is removed, but the gate file remains on disk.
4. Because `state-12.json` is gone, subsequent runs of `devflow recover --clean` do not visit phase 12.
5. `devflow gate list` ([commands.rs:1316](snapshot/crates/devflow-cli/src/commands.rs#L1316)) enumerates `.devflow/gates/` and continues to display the gate.
6. Attempting to approve the gate via `devflow gate approve 12` fails with `no confirmed waiter holds phase 12's lock`. Running `devflow gate sweep` refuses to reap it because `gate_sweep_may_reap` requires `HolderStatus::Live`. The gate remains orphaned indefinitely unless manually purged with `recover --clean --phase 12`.
**Reproduction:** Create a stale state (>24h old, dead PID) for phase 12 with an open gate `.devflow/gates/12-Code.json`. Run `devflow recover --clean`. Output: `cleaned up stale workflow state for phase 12`. Check `.devflow/gates/12-Code.json`: it still exists. Run `devflow gate list`: phase 12 is still listed. Run `devflow recover --clean` again: `no stale workflow state was cleaned`.
**Severity:** medium — incomplete cleanup in implicit recovery leaves ghost gates in operator listings that cannot be resolved by standard commands.

---

### C-3. `status` and `list_states` move and delete legacy state without holding the per-phase lock
**File:** [snapshot/crates/devflow-cli/src/commands.rs:1055](snapshot/crates/devflow-cli/src/commands.rs#L1055); [snapshot/crates/devflow-core/src/workflow.rs:146-175](snapshot/crates/devflow-core/src/workflow.rs#L146-L175); [snapshot/crates/devflow-core/src/workflow.rs:264-265](snapshot/crates/devflow-core/src/workflow.rs#L264-L265)
**Mechanism:**
1. `devflow status` calls `workflow::list_states(project_root)`.
2. `list_states` unconditionally calls `migrate_legacy_state(project_root)` at line 265 before reading the directory.
3. `migrate_legacy_state` reads `.devflow/state.json`. If `target` (`state-{NN}.json`) exists, it removes `.devflow/state.json` (`std::fs::remove_file(&legacy)`). If `target` does not exist, it renames `.devflow/state.json` to `.devflow/state-{NN}.json` (`std::fs::rename(&legacy, &target)`).
4. Neither branch acquires `crate::lock::acquire(project_root, state.phase)`.
5. A running legacy binary holding the phase lock and reading/writing `.devflow/state.json` loses its state file mid-execution when an operator runs `devflow status`.
**Reproduction:** Place a valid `.devflow/state.json` for phase 7. Acquire `lock-07` with a running process. Call `workflow::list_states(root)`. The state file is renamed to `.devflow/state-07.json` while `lock-07` remains held by the live process.
**Severity:** medium — violates the "every state writer takes the lock" rule during state migration, stranding live legacy runs.

---

### C-4. `gate_respond` permits responses for `Unconfirmable` holders, while `stop_via_gate` refuses them
**File:** [snapshot/crates/devflow-cli/src/commands.rs:1414-1424](snapshot/crates/devflow-cli/src/commands.rs#L1414-L1424); [snapshot/crates/devflow-cli/src/commands.rs:1924-1936](snapshot/crates/devflow-cli/src/commands.rs#L1924-L1936); [snapshot/crates/devflow-core/src/lock.rs:57-61](snapshot/crates/devflow-core/src/lock.rs#L57-L61)
**Mechanism:**
1. Fix E intended to align `gate_respond` with `stop`'s holder verification rules.
2. In `gate_respond` ([commands.rs:1419](snapshot/crates/devflow-cli/src/commands.rs#L1419)):
   ```rust
   let ship_without_holder =
       stage == Stage::Ship && matches!(holder_before_response, lock::HolderStatus::NoHolder);
   if !holder_before_response.may_be_waiting() && !ship_without_holder {
       return Err(...);
   }
   ```
   By definition ([lock.rs:60](snapshot/crates/devflow-core/src/lock.rs#L60)), `may_be_waiting()` returns `true` for `HolderStatus::Unconfirmable { .. }` (e.g. a live PID without a recorded start time in a legacy lock file). Thus, `gate_respond` approves or rejects the gate.
3. In `stop_via_gate` ([commands.rs:1926](snapshot/crates/devflow-cli/src/commands.rs#L1926)):
   ```rust
   let ship_without_holder =
       gate.stage == Stage::Ship && matches!(holder_before_reap, lock::HolderStatus::NoHolder);
   if !matches!(holder_before_reap, lock::HolderStatus::Live { .. }) && !ship_without_holder {
       println!("stop: phase {phase} {} has no confirmed live waiter; no response was written...", gate.stage);
       return Ok((false, ...));
   }
   ```
4. For an `Unconfirmable` holder:
   - `devflow gate approve <phase>` allows the response and writes `.devflow/gates/<phase>-<stage>-response.json`.
   - `devflow stop --phase <phase>` refuses to write a gate response, claims there is no confirmed live waiter, and falls back to `stop_via_lock` where it fails closed.
**Reproduction:** Create a single-line legacy lock file `.devflow/lock-05` containing the PID of a live process. Write an open gate for stage `Code`. Run `devflow gate approve 5`: exits 0 and writes the response file. In contrast, on the same state, run `devflow stop --phase 5`: prints `has no confirmed live waiter; no response was written` and refuses to write the response.
**Severity:** medium — `gate_respond` applies a weaker holder rule than `stop` when encountering `Unconfirmable` holders, allowing an unverified process to be fed gate input while `stop` refuses it.

---

### C-5. `devflow recover --clean --phase <PHASE>` claims cleanup and exits 0 for an absent phase
**File:** [snapshot/crates/devflow-cli/src/commands.rs:2561-2576](snapshot/crates/devflow-cli/src/commands.rs#L2561-L2576); [snapshot/crates/devflow-core/src/recover.rs:169-195](snapshot/crates/devflow-core/src/recover.rs#L169-L195)
**Mechanism:**
1. Fix D patched `recover --clean` sweep mode to avoid claiming cleanup when 0 phases were cleared ([commands.rs:2578-2598](snapshot/crates/devflow-cli/src/commands.rs#L2578-L2598)).
2. However, for explicit phase recovery ([commands.rs:2561-2576](snapshot/crates/devflow-cli/src/commands.rs#L2561-L2576)), it calls `recover::clean_phase(project_root, phase)`.
3. If `<phase>` does not exist (no state, no gates, no cron instructions), `clean_phase` succeeds idempotently and returns `Ok(warnings)` with an empty `Vec`.
4. `recover_cmd` unconditionally executes line 2576:
   ```rust
   println!("cleaned up workflow state for phase {phase}");
   ```
   and exits 0.
5. The command claims to the operator that it cleaned up workflow state when no state existed and nothing was deleted.
**Reproduction:** In a repository with no active or past state for phase 99:
```text
$ devflow recover --clean --phase 99
cleaned up workflow state for phase 99
$ echo $?
0
```
**Severity:** low — direct recurrence of defect class D (operator-facing false success on a no-op).

---

### C-6. `devflow cleanup` reports "no worktrees to clean up" and exits 0 after a worktree removal failure
**File:** [snapshot/crates/devflow-cli/src/commands.rs:953-965](snapshot/crates/devflow-cli/src/commands.rs#L953-L965)
**Mechanism:**
1. In `commands::cleanup`, the loop attempts to remove worktrees via `git.remove_worktree(&wt.path, force)`.
2. On success, `removed += 1` (line 951).
3. If removal fails (e.g. dirty working tree or untracked files without `--force`), it executes lines 953–958:
   ```rust
   Err(err) => {
       println!(
           "warning: could not remove worktree {} after retrying — manually delete this directory: {err}",
           wt.path.display()
       );
   }
   ```
4. Because removal failed, `removed` remains `0`.
5. At line 963:
   ```rust
   if removed == 0 {
       println!("no worktrees to clean up");
   }
   ```
6. The function returns `Ok(())` (line 974). The operator sees a warning stating the worktree could not be removed, immediately followed by the false claim "no worktrees to clean up", and the process exits 0.
**Reproduction:** Create a worktree under `.worktrees/phase-01` containing an untracked file. Run `devflow cleanup`.
Output:
```text
warning: could not remove worktree ... after retrying — manually delete this directory: ...
no worktrees to clean up
```
Exit code is 0.
**Severity:** low — contradictory operator output and exit 0 following an incomplete cleanup operation.

---

### C-7. `pipeline_gate::abort` reports success and returns `Ok(())` when state deletion fails
**File:** [snapshot/crates/devflow-cli/src/pipeline_gate.rs:461-477](snapshot/crates/devflow-cli/src/pipeline_gate.rs#L461-L477); [snapshot/crates/devflow-core/src/workflow.rs:309](snapshot/crates/devflow-core/src/workflow.rs#L309)
**Mechanism:**
1. `pipeline_gate::abort` is called when a gate receives an abort response or times out.
2. It prints `println!("workflow aborted for phase {}: {reason}", state.phase);` (line 462).
3. Lines 465–466 attempt cleanup:
   ```rust
   let _ = Gates::cleanup(project_root, state.phase, state.stage);
   let _ = workflow::clear_state(project_root, state.phase);
   ```
4. Errors from `clear_state` are silently discarded with `let _ =`.
5. It then emits a `workflow_aborted` event and returns `Ok(())` (line 476).
6. If `clear_state` fails (e.g. read-only filesystem or path conflict), the active state file remains on disk while the monitor and CLI report a successful abort.
**Reproduction:** Make `.devflow/state-NN.json` a non-removable file/directory before triggering an abort. Run the monitor or CLI through an abort path. It prints `workflow aborted for phase NN`, emits the aborted event, returns `Ok(())`, and leaves the active state file on disk.
**Severity:** medium — silent failure to clear active state upon terminal workflow abort.

---

## SUSPECTED

### S-1. `devflow recover --phase <PHASE>` inspection exits 0 silently when the named phase does not exist
**File:** [snapshot/crates/devflow-cli/src/commands.rs:2612-2648](snapshot/crates/devflow-cli/src/commands.rs#L2612-L2648)
**Mechanism:**
1. When `devflow recover --phase <PHASE>` is run without `--clean`, `recover_cmd` inspects state via `recover::inspect(project_root)`.
2. If state exists for other phases (e.g. phase 1), `inspect` returns `Ok(statuses)`.
3. Lines 2613–2618 filter by phase:
   ```rust
   for status in &statuses {
       if let Some(only) = phase && status.state.phase != only {
           continue;
       }
       ...
   }
   ```
4. If `<PHASE>` does not match any existing state, every entry is skipped. Nothing is printed.
5. Line 2647 returns `Ok(())` (exit 0).
**Confirmation/Refutation Evidence:** Running `devflow recover --phase 99` in a repository that has state for phase 1 produces zero output and exits 0. If no states exist at all, line 2602 prints `no state to recover — project is idle`. A silent exit 0 specifically occurs when state exists for *other* phases.
**Severity:** low — operator confusion; misleading exit 0 on a non-existent phase query.

---

## CHECKED AND CLEAN

- `commands::start` acquires `lock::acquire` before gate cleanup, configuration validation, `workflow::save_state`, and stage launching: [snapshot/crates/devflow-cli/src/commands.rs:360](snapshot/crates/devflow-cli/src/commands.rs#L360).
- `commands::stop` fallback (`persist_stopped_state`) acquires `lock::acquire_blocking` before writing `workflow::save_state`: [snapshot/crates/devflow-cli/src/commands.rs:2121](snapshot/crates/devflow-cli/src/commands.rs#L2121).
- `pipeline_launch::resume` acquires `lock::acquire` before reading, mutating, or persisting workflow state: [snapshot/crates/devflow-cli/src/pipeline_launch.rs:1414](snapshot/crates/devflow-cli/src/pipeline_launch.rs#L1414).
- `pipeline_launch::advance_with` acquires `lock::acquire_blocking` before evaluating outcomes or saving state transitions: [snapshot/crates/devflow-cli/src/pipeline_launch.rs:1621](snapshot/crates/devflow-cli/src/pipeline_launch.rs#L1621).
- `pipeline_gate::ship_override` acquires `lock::acquire` before reading gate responses or advancing to Ship completion: [snapshot/crates/devflow-cli/src/pipeline_gate.rs:518](snapshot/crates/devflow-cli/src/pipeline_gate.rs#L518).
- `pipeline_gate::maybe_auto_respond_gate` emits gate responses via `Gates::respond` strictly within the active monitor loop that holds the per-phase lock: [snapshot/crates/devflow-cli/src/pipeline_gate.rs:415](snapshot/crates/devflow-cli/src/pipeline_gate.rs#L415).
- `pipeline_outcomes::handle_ship_outcome` (`--yes-ship` auto-response) executes inside the monitor loop holding the exclusive phase lock: [snapshot/crates/devflow-cli/src/pipeline_outcomes.rs:838](snapshot/crates/devflow-cli/src/pipeline_outcomes.rs#L838).
- `commands::gate_sweep` checks holder status via `gate_sweep_may_reap` and strictly requires `HolderStatus::Live`, refusing to reap gates for `Recycled`, `Unconfirmable`, or `NoHolder`: [snapshot/crates/devflow-cli/src/commands.rs:1556-1558](snapshot/crates/devflow-cli/src/commands.rs#L1556-L1558); [snapshot/crates/devflow-cli/src/commands.rs:1736-1738](snapshot/crates/devflow-cli/src/commands.rs#L1736-L1738).
- Core gate methods `Gates::respond` and `Gates::reap` do not take locks by design; they are pure filesystem operations whose synchronization and holder policies are governed by calling workflows: [snapshot/crates/devflow-core/src/gates.rs:189-225](snapshot/crates/devflow-core/src/gates.rs#L189-L225).
- `commands::stop_via_lock` reports specific diagnostic reasons and exits cleanly when a lock is absent or contains an unconfirmable/recycled PID: [snapshot/crates/devflow-cli/src/commands.rs:2033-2094](snapshot/crates/devflow-cli/src/commands.rs#L2033-L2094).
- `commands::gate_sweep` tallies reaped, skipped, and left alone gates and reports exact counts rather than claiming unexecuted work: [snapshot/crates/devflow-cli/src/commands.rs:1604](snapshot/crates/devflow-cli/src/commands.rs#L1604); [snapshot/crates/devflow-cli/src/commands.rs:1731](snapshot/crates/devflow-cli/src/commands.rs#L1731).
- `commands::doctor` is purely diagnostic and read-only; no mutating `--fix` path exists: [snapshot/crates/devflow-cli/src/commands.rs:3767](snapshot/crates/devflow-cli/src/commands.rs#L3767).
- `recover::clean_phase` lock scope spans all five gate cleanups, state deletion, and per-phase cron deletion before dropping the guard: [snapshot/crates/devflow-core/src/recover.rs:170-193](snapshot/crates/devflow-core/src/recover.rs#L170-L193).
- `recover_cmd` maps `clean_phase`'s `RecoverError::Lock(Contended)` to an explicit error message and non-zero exit code: [snapshot/crates/devflow-cli/src/commands.rs:2562-2570](snapshot/crates/devflow-cli/src/commands.rs#L2562-L2570).

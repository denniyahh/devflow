## CONFIRMED

### C-1. `recover --clean` sweep silently removes orphan cron instructions and falsely reports "no stale workflow state was cleaned"
**File:** [crates/devflow-core/src/recover.rs:151-161], [crates/devflow-core/src/recover.rs:506-516], [crates/devflow-cli/src/commands.rs:2609-2623]
**Mechanism:**
1. A phase has no state or gate files, but has a leftover cron-instruction record on disk (e.g. `.devflow/cron-instructions-05.json`).
2. An operator executes `devflow recover --clean`.
3. In [`clean_report`], the sweep iterates over `list_cron_instructions(project_root)`. Finding no state file for phase 5, it calls `crate::ship::delete_cron_instructions(project_root, 5)`, which unlinks the file and returns `Ok(true)`.
4. `clean_report` completely ignores the `bool` return value. It does not push `5` to `report.cleared`, does not populate any `orphan_cron_cleared` field (none exists on [`CleanReport`]), and writes nothing to `report.warnings`.
5. [`recover_cmd`] checks `if report.cleared.is_empty() && report.orphan_gates_cleared.is_empty()`. Because both are empty, it prints `"no stale workflow state was cleaned"`.
6. The CLI exits 0 having silently deleted the cron-instructions file while asserting that nothing was cleaned, directly violating the class rule (WR-03).
**Reproduction:**
Repository with only `.devflow/cron-instructions-05.json`. Executed `devflow recover --clean`:
- Output: `no stale workflow state was cleaned`
- Exit code: `0`
- Disk state: `.devflow/cron-instructions-05.json` was deleted.
**Severity:** Medium — direct recurrence of WR-03: `recover --clean` claims no action occurred while deleting artifacts behind the operator's back.

---

### C-2. `recover --clean` sweep skips unparsable cron-instruction records completely
**File:** [crates/devflow-core/src/ship.rs:129-148], [crates/devflow-core/src/recover.rs:151-161]
**Mechanism:**
1. A corrupted or unparsable cron instruction file exists on disk (e.g. `.devflow/cron-instructions-05.json` or `.devflow/cron-instructions.json`).
2. `clean_report` attempts to sweep cron files by looping exclusively over `crate::ship::list_cron_instructions(project_root)`.
3. In [`list_cron_instructions`], any entry failing `serde_json::from_str::<CronInstructions>(&contents)` is silently dropped.
4. As a result, the unparsable cron file is never passed to `delete_cron_instructions`.
5. Unlike state files (where [`workflow::state_file_phases`] extracts phases from filenames and emits a warning for unparsable state) or gates (where [`Gates::phases_on_disk`] cleans without parsing), the sweep provides no complement scanner for cron instructions. The corrupt file remains on disk with neither cleanup nor warning.
**Reproduction:**
Repository with corrupt `.devflow/cron-instructions-05.json` containing `{invalid json`. Executed `devflow recover --clean`:
- Output: `no stale workflow state was cleaned`
- Exit code: `0`
- Disk state: `.devflow/cron-instructions-05.json` remains on disk.
**Severity:** Medium — direct sibling of WR-02: the sweep fails to reach artifacts that cannot be parsed by an entity listing helper.

---

### C-3. Failed stale lock removals in `remove_stale_locks` do not set `report.removal_failed`: `recover` exits 0 (success)
**File:** [crates/devflow-core/src/lock.rs:492-494], [crates/devflow-core/src/recover.rs:146-148], [crates/devflow-core/src/recover.rs:314-316], [crates/devflow-cli/src/commands.rs:2576-2586], [crates/devflow-cli/src/commands.rs:2624-2629]
**Mechanism:**
1. A stale lock exists on disk (holder PID dead), but filesystem removal fails (e.g. `EACCES`, permissions, or coordination failure).
2. [`remove_stale_locks`] catches `fs::remove_file` errors and pushes a formatted string to its `warnings: Vec<String>`. It has no return channel to signal a failed deletion vs. a harmless live-lock notice.
3. In both [`clean_report`] and [`clean_phase_report`], the returned `Vec<String>` is appended directly to `report.warnings`. Neither caller invokes `report.fail()`.
4. `report.removal_failed` remains `false`.
5. [`recover_cmd`] prints the warning, evaluates `if report.removal_failed`, finds it false, and exits with code 0 (success).
**Reproduction:**
Repository with stale lock `.devflow/lock-01` (dead PID) and read-only `.devflow` directory. Executed `devflow recover --clean`:
- Output: `warning: could not coordinate ... Permission denied`, `no stale workflow state was cleaned`
- Exit code: `0` (expected non-zero per `OPERATIONS.md:155` and WR-04).
**Severity:** Medium — violates the core contract introduced in `4d119fb` that `recover --clean` must exit non-zero when any removal fails.

---

### C-4. `workflow::clear_state` swallows orphaned state temp deletion failures into `tracing::warn!`: CLI reports success and exits 0
**File:** [crates/devflow-core/src/workflow.rs:364-371], [crates/devflow-core/src/recover.rs:638-644], [crates/devflow-core/src/recover.rs:329-336]
**Mechanism:**
1. A phase has an orphaned atomic write temp file (`.devflow/.state-01.json.*.tmp`) that cannot be deleted (e.g. read-only file/permissions).
2. During `clear_state`, `std::fs::remove_file(entry.path())` returns `Err`.
3. In [`workflow::clear_state`], the error is caught by `Err(error) => warn!("could not remove orphaned state temp...")` and ignored. `clear_state` returns `Ok(removed)`.
4. Contrast with [`Gates::cleanup`], which propagates temp file errors via `std::fs::remove_file(entry.path())?`, causing `report.fail()` to be invoked and `recover_cmd` to fail non-zero.
5. For state temps, the deletion error is completely hidden from `CleanReport` and `PhaseCleanReport`. No warning reaches `report.warnings`, `removal_failed` remains `false`, and `recover_cmd` exits 0.
**Reproduction:**
Confirmed by tracing and comparing `workflow.rs:364-371` against `gates.rs:355-358`.
**Severity:** Low — inconsistency within the temp-file cleanup defect class; temp files on disk survive without surfacing a failure.

---

### C-5. `recover --clean` sweep prints "no stale workflow state was cleaned" after removing an unparsable legacy `state.json`
**File:** [crates/devflow-core/src/recover.rs:139-145], [crates/devflow-cli/src/commands.rs:2609-2623]
**Mechanism:**
1. A corrupted legacy `.devflow/state.json` exists without any per-phase `state-NN.json` or gate files.
2. The user runs `devflow recover --clean`.
3. In [`clean_report`], `workflow::remove_corrupt_legacy_state(project_root)` unlinks `.devflow/state.json` and pushes `"removed unparsable legacy state.json"` to `report.warnings`.
4. Neither `report.cleared` nor `report.orphan_gates_cleared` is populated.
5. In [`recover_cmd`]:
   - It prints `warning: removed unparsable legacy state.json`.
   - Then evaluates `if report.cleared.is_empty() && report.orphan_gates_cleared.is_empty()`, which is true.
   - It prints `no stale workflow state was cleaned`.
6. Commit `4d119fb` fixed this for `--phase N` (`explicit_clean_of_legacy_state_says_it_cleaned`), but missed the sweep path.
**Reproduction:**
Repository with corrupt `.devflow/state.json`. Executed `devflow recover --clean`:
- Output:
  ```
  warning: removed unparsable legacy state.json
  no stale workflow state was cleaned
  ```
**Severity:** Low — contradictory output asserting no stale workflow state was cleaned directly below a warning stating legacy state was removed.

---

## SUSPECTED

None.

---

## CHECKED AND CLEAN

- **Monitor script Window F (pre-fork / fork to `$!`):** Clean. `${apid:-$!}` correctly resolves `$!` immediately upon fork return on both bash and dash ([crates/devflow-core/src/monitor.rs:525-530]).
- **Monitor script Window A (agent pre-exec):** Clean. Agent child checks `[ -e {stop_file} ] && exit 143` after trap reset on both shells ([crates/devflow-core/src/monitor.rs:529]).
- **Monitor script post-reap tail execution:** Clean. `reaped=1` is set before `advance_tail`; a deferred TERM arriving during `devflow advance` skips stop-marker write and `kill` under both bash and dash ([crates/devflow-core/src/monitor.rs:525, 531]).
- **Interrupted `wait $apid`:** Clean. A trapped signal arriving while `wait $apid` blocks executes `cleanup()`, which sets the marker, kills the running agent, and exits 0 ([crates/devflow-core/src/monitor.rs:525-531]).
- **`MonitorTail` production invariants:** Clean. `spawn_monitor` hardcodes `MonitorTail::Advance`; `MonitorTail::Script` is `#[cfg(test)]` and does not affect production launches ([crates/devflow-core/src/monitor.rs:260, 264-272]).
- **Test `a_term_after_the_agent_exits_leaves_no_stop_marker` non-vacuity:** Clean. Verified passing on host (bash) and in Docker container (dash); verified failing when `reaped=1` fix is reverted on both shells ([crates/devflow-core/src/monitor.rs:3391-3474]).
- **`Gates::phases_on_disk` non-gate file parsing:** Clean. Non-gate filenames matching `<phase>-...` parse to a `PhaseId`, but `Gates::cleanup` removes only `{padded}-{stage}` request/response/ack files and exact prefix `.tmp` files, leaving foreign files untouched ([crates/devflow-core/src/gates.rs:187-203, 323-359]).
- **`Gates::phases_on_disk` coverage of real gate files:** Clean. All request, response, ack, and atomic temp files follow `[.]<padded>-...`, correctly matched by `name.strip_prefix('.').split_once('-')` for both integer and decimal phases ([crates/devflow-core/src/gates.rs:195-201]).
- **Live-run gate file destruction race:** Clean. In `start`, `resume`, `advance`, `run_gate_with_timeout`, and `parallel`, state files are persisted before gates are created, and the per-phase lock is continuously held while waiting; `sweep_orphan_gates` checks lock and `state_path.exists()` before and after acquiring lock ([crates/devflow-core/src/recover.rs:651-662]).
- **Caller `Gates::cleanup` in `commands.rs:377`:** Clean. Return type `bool` safely ignored in `if let Err(...)` error handler ([crates/devflow-cli/src/commands.rs:377]).
- **Caller `Gates::cleanup` in `preflight.rs:1408, 1441`:** Clean. Explicitly discarded via `let _ = ...` ([crates/devflow-cli/src/preflight.rs:1408, 1441]).
- **Caller `Gates::cleanup` in `pipeline_launch.rs:1167, 1830`:** Clean. Discarded via `?;` or `let _ = ...` ([crates/devflow-cli/src/pipeline_launch.rs:1167, 1830]).
- **Caller `Gates::cleanup` in `pipeline_outcomes.rs:917, 926`:** Clean. Explicitly discarded via `let _ = ...` ([crates/devflow-cli/src/pipeline_outcomes.rs:917, 926]).
- **Caller `Gates::cleanup` in `pipeline_gate.rs:178, 240, 260, 273, 274, 465`:** Clean. Explicitly discarded via `let _ = ...` ([crates/devflow-cli/src/pipeline_gate.rs:178, 273, 465]).
- **Caller `workflow::clear_state` in `pipeline_gate.rs:275, 466`:** Clean. Discarded via `?;` or `let _ = ...` ([crates/devflow-cli/src/pipeline_gate.rs:275, 466]).
- **Caller `workflow::clear_state` in `commands.rs:700`:** Clean. Return type `bool` safely ignored in `if let Err(...)` handler ([crates/devflow-cli/src/commands.rs:700]).
- **Caller `delete_cron_instructions` in `recover.rs:337`:** Clean. Used via `report.removed_anything |= removed` in `clean_phase_files` ([crates/devflow-core/src/recover.rs:338]).
- **Artifact reachability: State (`.devflow/state-{NN}.json`):** Clean. Reached by `sweep_phase` and `clean_phase_files` ([crates/devflow-core/src/recover.rs:638, 780]).
- **Artifact reachability: Legacy state (`.devflow/state.json`):** Clean. Reached by `remove_corrupt_legacy_state`, `list_states` migration, and `clear_state` ([crates/devflow-core/src/recover.rs:139, 780]).
- **Artifact reachability: Gate request/response/ack files (`.devflow/gates/{NN}-{stage}*.json`):** Clean. Reached by `sweep_phase`, `sweep_orphan_gates`, and `clean_phase_files` ([crates/devflow-core/src/recover.rs:632, 663, 769]).
- **Artifact reachability: Gate write atomic temps (`.devflow/gates/.{padded}-{stage}*.tmp`):** Clean. Reached by `Gates::cleanup` via prefix match ([crates/devflow-core/src/gates.rs:355]).
- **Artifact reachability: Agent execution captures (stdout, stderr, exit code, agent pid, stop marker, prompt, monitor log, idle timeout):** Clean. Not touched by `recover --clean`; correctly retained as diagnostic records (e.g. `default_logs_phase`) and archived or unlinked on the next stage launch by `archive_phase_files` and `spawn_monitor` ([crates/devflow-core/src/agent_result.rs:3184-3215], [crates/devflow-cli/src/commands.rs:2375-2405]).
- **Artifact reachability: Historical stage archives (`.devflow/history/phase-{NN}/*`):** Clean. Not touched by `recover --clean`; immutable phase history ([crates/devflow-core/src/agent_result.rs:3217]).

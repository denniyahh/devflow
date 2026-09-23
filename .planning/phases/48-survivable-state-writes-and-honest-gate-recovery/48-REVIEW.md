---
phase: 48-survivable-state-writes-and-honest-gate-recovery
reviewed: 2026-09-23T12:09:09Z
depth: deep
files_reviewed: 3
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/tests/gate_wedge_e2e.rs
  - crates/devflow-cli/tests/start_lock_e2e.rs
findings:
  critical: 1
  warning: 6
  info: 5
  total: 12
status: issues_found
---

# Phase 48: Code Review Report

**Reviewed:** 2026-09-23T12:09:09Z
**Depth:** deep
**Files Reviewed:** 3
**Status:** issues_found

## Summary

Scope: `git diff ea8ac12..HEAD` of the three files. That covers plan 48-18 (the start-driven #200 arms in `gate_wedge_e2e.rs`) and plan 48-19 (`refuse_start_over_a_live_run` and `live_run_refusal` in `commands.rs`, called from `start` under the phase lock, plus four tests in `start_lock_e2e.rs`). I traced these call chains in source: `lock::acquire`/`lock_path`/`LockGuard::drop`, `workflow::load_state` with `migrate_legacy_state`, `agent_pid_from_file`, `agent::agent_running`/`is_zombie`, `Gates::cleanup`/`Gates::dir`, `stop`/`stop_via_gate`/`stop_via_lock`/`persist_stopped_state`, `recover::clean_phase_report`/`is_stale_state`, the Legacy monitor script (`monitor.rs:510-531`), the pipe-owning `__monitor` (`run_monitor` → `run_pipe_owning_monitor` → `advance_for_stage`), `spawn_agent_and_record` and `resume`.

The guard itself sits where it should: after `lock::acquire`, before `Gates::cleanup`. It does refuse when a recorded monitor or agent pid is live. I checked that against production traces. The operator's machine-global registry holds tempdir entries for phases 95 and 96 (the refusal arms) only up to 07:48, which is before the GREEN commit. Phases 97 and 98 (the proceed arms) keep registering through 07:54 (see WR-05).

The defects are in the refusal text, in one unguarded sibling launch verb, and in the tests:

- **Refusal text (the (b) question).** Three of its claims are false against source. `ps -p` cannot confirm what the text says it confirms (CR-01). A recycled pid does not let `start` proceed (WR-01). "SIGTERM the monitor to end the run now" fails during the Legacy monitor's advance tail (WR-02).
- **Paths past the guard (the (a) question).** An unloadable state file hides a live monitor, and that path is not on the approved list (WR-03). `resume` is an unguarded sibling of `start` (WR-04).
- **Tests.** The proceed arms leak detached monitors and pollute the operator's real registry (WR-05). The 48-18 wedge arm asserts that `recover --clean` removes temps, but no temp ever exists in that test (WR-06).

What I verified by execution: the `ps -p` output shape (CR-01), and the registry pollution with a before/after-GREEN negative control (WR-05). Everything else is traced from source, not reproduced.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: The refusal tells the operator that `ps -p <pid>` confirms process identity before a SIGTERM, but `ps -p` shows only the executable name

**Class:** an identity check that cannot discriminate, offered as the safeguard before a destructive signal.
**Sites:**
- `crates/devflow-cli/src/commands.rs:2457`: the per-pid `ps -p {pid}` lines.
- `crates/devflow-cli/src/commands.rs:2464-2465`: "confirm each pid is this phase's DevFlow monitor or agent with `ps -p <pid>`, then send it SIGTERM".
- `crates/devflow-cli/src/commands.rs:2468`: "after `ps -p` shows they are no longer this phase's processes".
- `crates/devflow-cli/tests/start_lock_e2e.rs:433`: the test pins the non-discriminating command as a required fragment.
- Related pre-existing sites say only "inspect it manually (e.g. `ps -p`)" and do not claim confirmation: `commands.rs:1684`, `:1693`, `:2070`, `:2083`, `:2092`.

**Issue:** The plan's Known limits and the SUMMARY accept pid-liveness-not-identity (Option A) on one condition: "The refusal tells the operator to check each pid with `ps -p <pid>` before signalling it, so an unrelated process is not killed." Default `ps -p` output is `PID TTY TIME CMD`, and CMD is the executable name only. I checked it here. For an `sh -c "sleep 5; echo x"` process, `ps -p` prints `sh`, while `ps -o args= -p` prints `sh -c sleep 5; echo x`.

So the Legacy monitor shows as `sh`, the pipe-owning monitor as `devflow`, and the agent as `claude` or `node`. Nothing in that output names the phase or the project. Under `devflow parallel`, a recycled pid that now belongs to another phase's live monitor looks identical. The operator is then told to SIGTERM it, "monitor first", which ends a different phase's run. The compensating control for the accepted recycled-pid limit does not establish what it claims.

**Fix:** Name a command whose output carries the identity, and say what to look for.
```rust
.map(|(role, pid)| format!("\n  {role} pid {pid}: ps -o pid=,lstart=,args= -p {pid}"))
// and in the body:
"confirm each pid is this phase's DevFlow process: the monitor's args contain \
 `--phase {phase}` (pipe-owning `__monitor`) or `phase-{padded}-` paths (Legacy `sh`), \
 and the agent's parent (`ps -o ppid= -p <pid>`) is the named monitor"
```
Update the `start_lock_e2e.rs:433` fragment to match.

## Warnings

### WR-01: The refusal says `devflow start` may be run again once `ps -p` shows the pids are not this phase's processes, but `start` refuses again for any live pid

**Class:** refusal text that promises a path the guard blocks.
**Site:** `crates/devflow-cli/src/commands.rs:2467-2470`

**Issue:** The text reads: "Run `devflow recover --clean --phase N` or `devflow start` again only after the named processes have exited, **or after `ps -p` shows they are no longer this phase's processes**". The second condition is the recycled-pid case. `refuse_start_over_a_live_run` tests only `agent::agent_running(pid)` (`commands.rs:2431-2435`). A recycled pid is live, so `start` refuses again with the same text. Only `recover --clean` followed by `start` works: it removes the state, and the no-state carve-out then skips the agent pid file. The plan's own Known limits states the correct sequence ("`recover --clean` removes the state and the next `start` proceeds"). The shipped text does not.

**Fix:** Split the sentence. If the processes have exited, run `start` again. If they are live but not this phase's processes, run `devflow recover --clean --phase N` first, then `start`.

### WR-02: "To end this run now … send it SIGTERM, monitor first" does not end the run while the Legacy monitor is in its advance tail

**Class:** a claim that holds in one window of the run and is presented as holding in all of them.
**Sites:**
- `crates/devflow-cli/src/commands.rs:2461-2466`: the refusal text.
- `crates/devflow-core/src/monitor.rs:500-509`: the script's own comment says the trap is deferred.
- `crates/devflow-core/src/monitor.rs:525-531`: `reaped=1` gates `cleanup`.
- SUMMARY class-enumeration row 10: claims the guard covers this window.

**Issue:** After the agent exits, the Legacy `sh` monitor sets `reaped=1` and runs `devflow advance` as a foreground child. That child blocks in `acquire_blocking` while `start` holds the lock. The monitor's pid is live, so the guard fires and names only "monitor pid X", because the agent pid is dead. Following the advice then fails:

- SIGTERM to the `sh` is deferred until the foreground `advance` returns. monitor.rs says so: "the shell defers it until that foreground command returns". When the trap does run, `reaped` is set, so `cleanup` kills nothing and just exits.
- Meanwhile the unrecorded `advance` child takes the lock as soon as the refused `start` exits, and launches the next stage.

The run continues. Also, once that `advance` holds the lock, `devflow stop` would signal it, so "stop … does not end this run" becomes false moments after the refusal. Plan 48-19 reasoned that the guard *refuses* in this window, and that part is true. It never checked that the *remedy text* works there. 999.136 defers the lock gap, not the correctness of the new text.

**Fix:** When only the monitor is live, read its children (`/proc/<pid>/task/<pid>/children`; `monitor.rs:3211` already uses this in tests). If a child is a `devflow advance`, name it in the refusal as the process to signal, or tell the operator to wait and re-run `start`. At minimum, qualify the claim: "if the agent is already dead, the monitor may be running `devflow advance`; signal that child (`ps -o pid=,args= --ppid <monitor pid>`), not the monitor".

### WR-03: A state file that exists but does not load hides a live monitor from the guard, and that path is not operator-approved

**Class:** fail-open on an unreadable liveness record.
**Site:** `crates/devflow-cli/src/commands.rs:2427-2434`

**Issue:** When `load_state` returns an error and the file exists, the guard sets `loaded.ok()` to `None`, so `monitor` is `None` and only the agent pid file is checked. Two ways a live monitor goes unseen:

- A serde failure, for example an older binary reading a newer `Stage` variant.
- A read error (EACCES).

If the agent is dead at that moment (the Legacy advance tail, or the pipe-owning monitor between the child exiting and its in-process advance taking the lock), `start` proceeds. It clears the gates, overwrites the state and launches beside the live monitor. The approved paths are the no-state carve-out, the recycled-pid false refusal and 999.136. This path appears only as an executor-recorded limit ("A state file that exists but cannot be loaded … is checked on the agent pid file only"), and no test covers it.

**Fix:** Fail closed, or read the one field leniently.
```rust
let monitor_pid = match &loaded {
    Ok(state) => state.monitor_pid,
    Err(_) => std::fs::read_to_string(workflow::state_path(project_root, phase))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("monitor_pid")?.as_u64())
        .and_then(|pid| u32::try_from(pid).ok()),
};
```
Add an e2e arm with a corrupt state file that still records a live `monitor_pid`.

### WR-04: `resume` launches under the lock with no live-run check, and `status` recommends `resume` for a phase whose agent is alive

**Class:** an operator verb that launches an agent without refusing a recorded live run (one writer per phase, D-01). `start` is fixed; this is the sibling.
**Sites:**
- `crates/devflow-cli/src/pipeline_launch.rs:1408-1504` (`resume`): takes the lock, loads state, and calls `launch_stage` with no `agent_running` check.
- `crates/devflow-cli/src/commands.rs:1012-1019` (`liveness`): `(false, _)` becomes `Stuck`, including when the agent is alive.
- `crates/devflow-cli/src/commands.rs:1031-1038` (`recovery_hints`): `Stuck` becomes "devflow resume --phase N".

**Issue:** The new refusal tells the operator to SIGTERM the monitor first. On the pipe-owning launch (the Claude default), the `__monitor` process installs no SIGTERM handler; I found none with `rg` across `crates/*/src`. It dies, and the agent keeps running in its own process group (`process_group(0)`, `monitor.rs:984`). Between the two signals, `status` reports "stuck — needs devflow resume". If the operator runs it, `resume` spawns a second agent beside the live one. `spawn_agent_and_record` also archives the live run's capture and overwrites the agent pid file, which hides the first agent from every later check.

**Fix:** Extract the guard into a shared `refuse_launch_over_a_live_run(project_root, phase)` and call it in `resume` right after its lock. Make `liveness(Some(_), false, true)` return a distinct state ("orphaned agent — signal it first") instead of `Stuck`/`resume`.

### WR-05: The proceed-arm tests leak detached monitors past their TempDir and write into the operator's real `~/.cache/devflow` registry

**Class:** tests that launch a real run without isolating machine-global state or reaping what the run spawns.
**Sites:**
- `crates/devflow-cli/tests/start_lock_e2e.rs:74-94` (`child`) and `:377-383` (`run_start`): only `PATH` and `DEVFLOW_GATE_TIMEOUT_SECS` are set. `HOME`, `XDG_CACHE_HOME` and `DEVFLOW_CACHE_DIR` are inherited.
- New tests that launch: `start_replaces_a_leftover_state_whose_recorded_processes_are_dead` (`:514`) and `start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process` (`:547`).
- Same file, pre-existing: `start_proceeds_when_no_process_holds_the_phase_lock` (`:174`, which has no gate timeout at all) and `start_clears_leftover_gate_files_after_taking_the_lock` (`:202`, where `kill_and_reap` reaps only `start`, not the `sleep 60` monitor it launched).

**Issue:**
- **(d) Reaping.** `run_start` waits on `devflow start`, but `start` detaches a Legacy `sh` monitor, which runs the fake agent and then a real `devflow advance`. Nothing in the test kills or waits on those processes, on success or on panic. They outlive the test and run against a deleted TempDir until the 15 s gate timeout expires.
- **Hermeticity.** `spawn_agent_and_record` calls `registry::register`. `registry::cache_dir` resolves `DEVFLOW_CACHE_DIR`, then `XDG_CACHE_HOME`, then `HOME/.cache/devflow`, so each run writes into the operator's real registry. Observed: `~/.cache/devflow/roots` holds 14,796 entries, 14,787 of them `/tmp/.tmp*` roots.
- **Negative control.** Phase 97 and 98 entries with tempdir roots were registered at 07:49-07:54 on 2026-09-23 (after GREEN `fd9be36` at 07:54 and the RED runs). Phase 95 and 96 entries stop at 07:48, the RED runs, because the fixed guard refuses before launch. That also confirms the refusal arms no longer launch.

This breaks the project's own test-hermeticity rule. `devflow gate list --all-roots` walks this registry.

**Fix:** In `child`, set `.env("DEVFLOW_CACHE_DIR", root.join(".test-cache"))`. For the proceed arms, add a drop guard. It reads `state.monitor_pid` and the agent pid file after `start` returns, SIGTERMs the monitor (whose trap kills the agent), and waits for both to be gone before the TempDir drops. Apply the same guard to the pre-existing arms at `:174` and `:202`.

### WR-06: The 48-18 wedge arm asserts that `recover --clean` removes orphaned temps, but no temp exists in the fixture

**Class:** a removal assertion with no observed precondition, which cannot fail.
**Site:** `crates/devflow-cli/tests/gate_wedge_e2e.rs:486-508`. It covers state `:486-490`, temps `:496-501` and lock `:502-507`, inside `start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers`.

**Issue:**
- **Temps.** The `start` is SIGKILLed while parked in a gate wait, with no write in flight, and the test plants no temp. `temps_left.is_empty()` holds whether or not `recover --clean` removes temps. It is also scoped wrong. The scan covers only `.devflow/`, but gate temps are written beside their targets in `.devflow/gates/` (`Gates::dir`, `gates.rs:121-123`). Temp names start with `.`, so `phase_gate_entries`' `{padded}-` prefix filter misses them too.
- **State and lock.** Only the gate scan has a before-recovery control (`:471-475`). The state and lock-NN assertions never check that those files existed before `recover`.

48-18-SUMMARY coverage row D claims "recover --clean removes state, gate files, temps and lock-NN" for this test. The temps part is unproven here. It is covered only by the unit test at `recover.rs:531-540`.

**Fix:** Before calling `recover`, plant `.devflow/.state-93.json.1.0.tmp` and `.devflow/gates/.93-define.json.1.0.tmp`. Assert that `state_path` and `.devflow/lock-93` exist. After recovery, scan both directories for `.tmp`.

## Info

### IN-01: The no-state carve-out is reachable from more producers than the one its approval was reasoned on

**Sites:** `crates/devflow-cli/src/commands.rs:699-701`; `crates/devflow-cli/src/pipeline_launch.rs:1067-1068`; `crates/devflow-core/src/recover.rs:419-438`

**Issue:** The operator approved the carve-out on the reasoning that `recover --clean` leaves the agent pid file behind. Two other writers also remove state while a recorded process is live:

- `start`'s own `clear_state` after a failed `launch_stage`. This is reached when the post-spawn `save_state(state)?` fails after `spawn_monitor` has already succeeded.
- The `recover` sweep's `is_stale_state`. It checks only the agent, never the monitor. This one is within 999.136's scope.

Both leave a live monitor or agent with no state, so the next `start` proceeds.

**Fix:** Bring these producers to the operator's attention when 999.136 is triaged. In `start`, do not `clear_state` once a monitor pid has been returned.

### IN-02: The refusal tests never assert the role, and two fragments are redundant

**Site:** `crates/devflow-cli/tests/start_lock_e2e.rs:427-436`

**Issue:** `phase.to_string()` is contained in `devflow stop --phase {phase}`, and `live_pid.to_string()` in `ps -p {live_pid}`. Neither fragment can fail on its own. Nothing asserts `"monitor pid {pid}"` versus `"agent pid {pid}"`, so swapping the roles in `refuse_start_over_a_live_run` would pass both arms.

**Fix:** Assert `format!("monitor pid {pid}")` in the monitor arm and `format!("agent pid {pid}")` in the agent arm. Drop the two redundant fragments.

### IN-03: "nothing was written" is not strictly true

**Sites:** `crates/devflow-cli/src/commands.rs:321` (via `fresh_state_carrying_phase_failures`, before the lock), `:2427` (the guard's own `load_state`), `:2459`

**Issue:** `load_state` runs `migrate_legacy_state`, which can rename a legacy `.devflow/state.json` or delete it. `lock::acquire` creates and removes `lock-NN`, and `ensure_devflow_dir` may create `.devflow/.gitignore`. The contention refusal at `:363-364` makes the same claim, so this is pre-existing. The SUMMARY acknowledges the migration write.

**Fix:** Word it as "no phase state, gate or branch was changed".

### IN-04: The `sleep 60` live process can go zombie on a slow run and fail the refusal arms spuriously

**Site:** `crates/devflow-cli/tests/start_lock_e2e.rs:281-286`

**Issue:** If more than 60 s pass between `LiveProcess::spawn` and the guard (a cold container), `sleep` exits. It stays unreaped until Drop, and `agent_running` treats zombies as dead. `start` then proceeds and the arm fails. This fails loudly rather than passing vacuously.

**Fix:** Use `sleep 3600`; Drop kills it anyway.

### IN-05: The "monitor first" advice ignores the pipe-owning launch's process group and its SIGTERM disposition

**Sites:** `crates/devflow-cli/src/commands.rs:2464-2466`; `crates/devflow-core/src/monitor.rs:984`, `:1469-1485`; `crates/devflow-cli/src/pipeline_launch.rs:962-977`

**Issue:** On the pipe-owning arm the agent leads its own process group. The monitor itself terminates the whole group (`terminate_child_group`), but the refusal tells the operator to signal only the leader pid. The `__monitor` has no SIGTERM handler, so `AutoChainGuard`'s Drop does not run and `_auto_chain_active` leaks into the worktree config. The next `start` or `resume` repairs it.

"Signalling the agent first lets the monitor launch the next stage" is also imprecise. `advance` evaluates exit 143 and may relaunch the same stage or open a gate. The direction (another agent may launch) is right.

**Fix:** For the pipe-owning launch, suggest `kill -TERM -- -<agent pid>`. Say "lets the monitor run `advance`, which may launch another agent".

---

_Reviewed: 2026-09-23T12:09:09Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_

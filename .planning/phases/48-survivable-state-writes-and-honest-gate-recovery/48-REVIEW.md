---
phase: 48-survivable-state-writes-and-honest-gate-recovery
reviewed: 2026-09-22T19:27:40Z
depth: deep
files_reviewed: 5
files_reviewed_list:
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-core/src/agents/opencode.rs
findings:
  critical: 0
  warning: 5
  info: 3
  total: 8
status: issues_found
---

# Phase 48: Code Review Report

**Reviewed:** 2026-09-22T19:27:40Z
**Depth:** deep
**Files Reviewed:** 5
**Status:** issues_found

## Summary

Scope: the commits after f745bd5 that no earlier review covered (d36936e, 854bbce, f73990f, 4fbe23c/745ad9b, 3ca90b6, e683f89). Production code was traced into its callers: `commands.rs::recover_cmd`, `pipeline_launch.rs` (lock scope, `archive_phase_files`, `spawn_monitor`), `gates.rs::cleanup`, `workflow.rs` (`clear_state`, `load_state`, legacy migration) and `agent.rs` (stray classifier, `terminate_and_verify`).

**What holds up (checked by experiment, not only by reading):**
- The stop marker plus `${apid:-$!}` closes both spawn windows it targets. The fix was reverted in scratch copies (never in the worktree), and each test ran 3 times per variant:
  - **Window F test:** fails 3/3 on bash (host) and 3/3 on dash (the pinned `rust:2.0.13-1-bookworm` image), each time with the intended "survived a TERM" message.
  - **Window A test:** fails 3/3 on dash but passes 3/3 on bash. So it is only a real check on dash (see IN-01).
  - **Unmodified HEAD:** passes 3/3 on both shells.
  - **Stale-marker test:** removing only the Rust-side marker cleanup makes it fail 3/3, with exit code 143 instead of 7.
- The recover tests are real checks. Reverting the sweep's gate cleanup fails `clean_removes_the_gate_files_of_a_stale_phase_it_clears`, and forcing `found_anything = true` fails `clean_phase_report_finds_nothing_for_an_absent_phase`. The two control tests still pass under both reverts.
- Lock scope in recover.rs: every state and gate-file deletion in both `clean_report` and `clean_phase_report` happens while the phase lock is held. A locked phase's state and gate files are left alone, and tests pin this for both paths.
- The new file is covered in `doc_check`, the gitignore e2e test and OPERATIONS.md.

**What does not hold up:** the same pattern as before, fixing one instance but not the whole class, shows up again on both fronts:
- The monitor fix handles a TERM that arrives before the agent's exec, but not one that arrives after the agent has exited (WR-01).
- The recover fix clears gates only for phases that still have state (WR-02).
- The "report only what was cleaned" fix reports what was present at the start rather than what was actually removed (WR-03).

No finding is rated Critical. WR-01 and WR-05 need specific timing to trigger. WR-01's mechanism was reproduced in a shell simulation, but not end-to-end through `devflow advance`.

## Warnings

### WR-01: A TERM that arrives after the agent exits runs `cleanup` late: it writes a stop marker that can stop the NEXT agent, and it kills a reaped pid

**File:** `crates/devflow-core/src/monitor.rs:502-508`
**Issue:** The trap stays armed after `wait $apid` returns, through `echo $? > exit` and the foreground `devflow advance` tail. POSIX shells hold back a trapped signal until the foreground command finishes. A TERM sent to the wrapper while `advance` is running therefore runs `cleanup` only after `advance` exits. By then two things have happened:

1. **The marker comes back after it was cleared.** On an agent stage, `advance` has already called `spawn_monitor` for the next stage. That call deleted the phase's stop marker and started a new wrapper for the same phase. The old wrapper's late `cleanup` now runs `echo > phase-NN-monitor-stop`, which recreates the marker. If the new agent child has not yet reached its `[ -e marker ]` check, it runs `exit 143` without ever starting. The new monitor, which nobody signalled, records exit 143 and advances on a kill nobody sent. This is exactly the failure `a_stale_stop_marker_does_not_stop_the_next_agent` describes, reached through a marker written late rather than a stale one.
2. **It kills the wrong process.** `kill "${apid:-$!}"` targets the agent pid that `wait` reaped long ago. `advance` can block on a gate for days, so that pid may have been reused by an unrelated process owned by the same user by the time the trap finally runs.

The `${apid:-$!}` fallback also blocks the obvious fix: clearing `apid` after `wait` does nothing, because `$!` still holds the same reaped pid.

Reproduced in a shell simulation of this script's shape, with the `advance` tail replaced by `sh -c 'rm -f M; sleep 2'`:
- TERM was sent at t+0.5s.
- `cleanup` ran at t+2.0s, after the stand-in `advance` had removed the marker.
- It recreated the marker and targeted the agent's reaped pid.

This was not reproduced through a real `devflow advance`. Among existing callers, a plausible trigger is an operator `kill` of the wrapper, or `gate sweep`'s reaper when the `advance` finishes within `TERMINATE_VERIFY_WAIT`. `devflow stop` does not trigger it, because it signals the lock holder, not the wrapper.

**Fix:** turn `cleanup` into a no-op for the agent once the agent has been reaped, using a flag that `$!` cannot bring back:
```sh
apid=''; reaped=''; cleanup() { [ -z "$reaped" ] && { echo > M; [ -n "${apid:-$!}" ] && kill "${apid:-$!}" 2>/dev/null; }; exit 0; }; \
trap cleanup TERM INT; ...; \
wait $apid; rc=$?; reaped=1; echo $rc > EXIT; <advance tail>
```
Add a held-window test: TERM the wrapper after the agent exits, then assert that the marker is absent and that no signal went to the old pid.

### WR-02: The sweep never removes gate files for a phase that has no state. This includes every orphan gate the old sweep left behind

**File:** `crates/devflow-core/src/recover.rs:112-163`
**Issue:** Commit 854bbce made the sweep clear gate files, but only inside `for state in workflow::list_states(..)`. A phase without a parseable state file never enters that loop, so its gate request, response and ack files are never touched. The cron loop right below it (lines 154-162) does handle the stateless case for cron records, and gates have no equivalent.

Three groups of orphan gates stay behind:
- Every gate left by the pre-854bbce sweep, which is the population agy C-2 reported.
- Gates from other stages after `abort` (`pipeline_gate.rs:465` cleans only `state.stage`).
- The gates of a phase whose `state-NN.json` cannot be parsed. `list_states` skips that phase with only a `warn!` log.

Verified with a scratch probe: a lone `09-code.json` gate plus `clean_report()` returned `cleared=[]`, left the gate on disk, and `Gates::list_open` still showed 1 open gate. The CLI then prints "no stale workflow state was cleaned", and `gate list` keeps showing a gate that nothing will ever answer. `--phase 9` does clear it, but the sweep, which is the documented reset, does not.

**Fix:** after the state loop, list the phases that have files in `.devflow/gates/` but no `state_path`. For each one:
- take the phase lock (skip the phase with a warning if the lock is contended);
- run `Gates::cleanup` for all `STAGES`;
- report it on a separate line, e.g. "removed orphan gate files for phase N".

Add a test that uses a lone gate as the positive case and a gate under a held lock as the control.

### WR-03: `found_anything` records what existed before the clean, not what it removed, so both messages can be wrong

**File:** `crates/devflow-core/src/recover.rs:206`, `recover.rs:233-242`; caller `crates/devflow-cli/src/commands.rs:2576-2583`
**Issue:** The class rule is: "the message states what was actually removed". The implementation instead checks which files were present at the start. It gets this wrong in both directions:
1. **It says "nothing to clean" after deleting state.** `phase_has_artifacts` checks `state_path(N).exists()` before `load_state` moves a legacy `.devflow/state.json` into `state-NN.json`, and `clear_state` then deletes it. Verified with a scratch probe: legacy state for phase 7, then `clean_phase_report(7)`, gave `found_anything=false` with the state file gone. The CLI then prints "nothing to clean for phase 7: no workflow state, gate files or cron record" after deleting phase 7's workflow state. The doc comment says legacy files are not counted, but the CLI message states flatly that there was no workflow state.
2. **It says "cleaned up" when the removal failed.** If the phase's only artifact is a cron record and `delete_cron_instructions` fails, the result is a warning and `found_anything=true`. The CLI prints "cleaned up workflow state for phase N" even though nothing was removed.
3. **Some deletions are never counted.** Orphan `.tmp` files for state and gates, and an unparsable legacy cron record (deleted for any phase because of `unwrap_or(true)` at `ship.rs:162`), are deleted without being counted.

**Fix:** base the flag on actual removals. Have `Gates::cleanup`, `clear_state` and `delete_cron_instructions` each return whether they removed anything, and OR those results together. Alternatively, run the legacy migration before `phase_has_artifacts` and count the legacy path when it parses to `phase`. Also do not print "cleaned up" when the only artifact that was found failed to be removed.

### WR-04: A gate-cleanup error now aborts the whole sweep partway and hides phases it already cleared

**File:** `crates/devflow-core/src/recover.rs:141`
**Issue:** 854bbce added `Gates::cleanup(..)?` inside the per-phase loop. Any error from it aborts `clean_report`: an EACCES or EISDIR on one gate file, or a `read_dir` entry error while scanning for `.tmp` files. The cost of that:
- Phases already cleared in earlier iterations are lost from the report. `recover_cmd` hits `?` at `commands.rs:2587` and prints only the error, so the operator is not told that those phases' state was deleted.
- The remaining phases are skipped.
- `remove_corrupt_legacy_state`, `remove_stale_locks` and the cron sweep never run.
- The failing phase can end up with some of its gates deleted and its state kept.

Elsewhere in the same function, failures are downgraded to warnings (the cron and legacy-state removals), so this one hard `?` does not match the function's own error policy.
**Fix:** inside the loop, collect gate-cleanup errors as warnings, e.g. `kept phase N — could not remove gate files: {err}`, and `continue` without clearing that phase's state. Keep `?` only for errors that make the whole sweep meaningless.

### WR-05: The sweep decides a phase is stale without checking its monitor, and does not re-check after taking the lock

**File:** `crates/devflow-core/src/recover.rs:112-139` (with `is_stale_state` at 246-262)
**Issue:** This is pre-existing, but 854bbce now also deletes gate files because of it. A phase counts as stale when `started_at` is older than 24h and the agent-pid file names no live process. `started_at` is only ever set at `State::new`; a repo-wide search finds no other non-test write. So any phase that has lived through a long gate stays permanently "old", and only the agent pid protects it.

In the Legacy flow, no lock is held between the agent exiting (`wait $apid` returns) and the tail's `devflow advance` acquiring the phase lock (`pipeline_launch.rs:1621`). A sweep that runs in that window:
- sees a dead agent and an old `started_at`;
- acquires the free lock;
- deletes the live run's state and gates.

`advance` then fails on missing state. `state.monitor_pid`, written at `pipeline_launch.rs:1067`, is never consulted. Separately, the staleness decision uses the `list_states` snapshot taken before the lock and is not re-checked once the lock is held.

This was not reproduced, and the window is about as long as `devflow advance` takes to start.
**Fix:** once the lock is held, reload the state and re-run the checks. Also treat a live `state.monitor_pid` (identity-checked) as not stale. `inspect_state` has the same blind spot and should get the same monitor check.

## Info

### IN-01: The Window A regression test cannot fail on the Fedora dev host

**File:** `crates/devflow-core/src/monitor.rs` (`sigterm_before_the_agent_child_resets_its_traps_still_kills_the_agent`)
**Issue:** The monitor script always runs as `sh`, which is bash on the host. With the fix reverted, this test passed 3/3 on the host and failed 3/3 under dash in the container. The doc comment says so, but a local `cargo test` or `scripts/check.sh` run gives no protection for the dash-specific defect. Only the container or CI run does.
**Fix:** run the Window A test inside `scripts/check-in-container.sh` before pushing, or have the test print a visible "vacuous on bash /bin/sh" note when `readlink /bin/sh` is not dash, so a green result on the host is not mistaken for coverage.

### IN-02: The probe kill-path breadcrumb is skipped when `kill_probe_group` returns an error

**File:** `crates/devflow-core/src/agents/opencode.rs:412`, `opencode.rs:474`
**Issue:** `kill_probe_group(child.id())?` returns before `record_probe_kill_path` runs. When the kill fails with a non-ESRCH error such as EPERM, the diagnostic then reports "<never recorded: spawn_with_timeout returned without killing anything>". That is false, because a kill was attempted. This only affects test diagnostics.
**Fix:** record the path before propagating, e.g. `let group_killed = kill_probe_group(id); record_probe_kill_path(id, || format!("... {group_killed:?}")); let group_killed = group_killed?;`.

### IN-03: Under dash, the held-monitor tests leave a `sleep 30` running for up to 30s

**File:** `crates/devflow-core/src/monitor.rs` (`HeldMonitor::spawn` stub args `-c "sleep 30"`, and `Drop`)
**Issue:** dash does not `exec` the last command of `-c`. In the container runs the agent shows up as `Name=sh cmdline=[sh -c sleep 30]`. The trap's TERM, or the `Drop`'s SIGKILL, kills that `sh` and leaves its `sleep 30` child orphaned under init for up to 30s on every run. On bash, `sleep` replaces the shell. This does not affect results, but it adds stray processes that concurrent `/proc`-census tests have to tolerate.
**Fix:** make the stub `exec sleep 30` so the agent pid is the sleeper on both shells.

---

_Reviewed: 2026-09-22T19:27:40Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_

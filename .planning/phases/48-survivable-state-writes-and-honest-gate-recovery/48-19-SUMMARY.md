---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 19
subsystem: cli
tags: [rust, e2e, lock, start, liveness, gap-closure]

requires:
  - phase: 48-18
    provides: "start-driven #200 arms (Gap A); start_lock_e2e fixtures reused here"
provides:
  - "`start` refuses, under lock-NN and before any gate cleanup, git mutation or save_state, when the phase's recorded monitor pid or agent pid is alive; `--force` does not bypass it"
  - "Refusal text that is honest about the lock-free window: names each live pid and role, says `devflow stop --phase N` would only mark the state stopped, gives `ps -p <pid>` per pid with SIGTERM monitor first, and allows `recover --clean`/`start` only after the named processes have exited"
  - "Four e2e tests: monitor-live refusal, agent-live refusal with --force, dead-leftover control, no-state carve-out"
affects: [phase-48 verification criterion 2, human_verification item 1, backlog 999.136, backlog 999.138]

actuals:
  tokens: 3700      # chars/4 over the realized diff f7cc0d6..HEAD in crates/ (14863 changed-line chars)
  tasks: 2
  commits: 2        # measured: git rev-list --count f7cc0d6..HEAD = 2 (047c88e RED, fd9be36 GREEN; both by the orchestrator, #4799); this SUMMARY is committed separately
plan_head_before: f7cc0d6

tech-stack:
  added: []
  patterns:
    - "LiveProcess(Child) test-owned `sleep 60`, killed and waited in Drop with no expect, so a panicking assertion leaks nothing"
    - "Byte-identity by (path, bytes) snapshot equality with a coverage precheck on the snapshot itself"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/start_lock_e2e.rs

key-decisions:
  - "Liveness is pid liveness through agent_pid_from_file + agent::agent_running (Option A, operator 2026-09-22); no identity mechanism added"
  - "No-state carve-out kept (operator 2026-09-22): no state file means no refusal, even if the agent pid file names a live process"
  - "Bin-suite gate line count corrected to the harness's top-level summary line (orchestrator's verdict, option 1); the plan gate text itself is unedited"

requirements-completed: [SURV-01, SURV-02]

coverage:
  - id: D1
    description: "start refuses a phase whose recorded monitor pid is alive: non-zero exit, every refusal fragment in stderr, state/agent-pid/gate files byte-identical, no lock-NN, no phase branch, no workflow_started"
    requirement: SURV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_refuses_a_phase_whose_recorded_monitor_is_alive"
        status: pass
    human_judgment: false
  - id: D2
    description: "start --force refuses a phase whose recorded agent pid is alive, with the same no-effects assertions"
    requirement: SURV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_refuses_a_phase_whose_recorded_agent_is_alive"
        status: pass
    human_judgment: false
  - id: D3
    description: "A leftover state whose recorded pids are dead is replaced as before: exit 0, planted Code gate files cleared, workflow_started emitted"
    requirement: SURV-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_replaces_a_leftover_state_whose_recorded_processes_are_dead"
        status: pass
    human_judgment: false
  - id: D4
    description: "No-state carve-out: with no state file, start proceeds even when the agent pid file names a live process"
    requirement: SURV-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process"
        status: pass
    human_judgment: false
  - id: D5
    description: "Refusal guidance (`stop` would not end the run; SIGTERM monitor first; recover --clean only after exit) is correct end to end"
    requirement: SURV-02
    verification: []
    human_judgment: true
    rationale: "Worded from source reading; no test drives stop or a signal against a live monitor-owned run"

duration: "not measured (split RED/GREEN execution across orchestrator hand-offs; RED committed 07:48, GREEN 07:54 -0400)"
completed: 2026-09-23
status: complete
---

# Phase 48 Plan 19: Refuse `start` over a live run Summary

**`start` now refuses, under the per-phase lock and before it touches any gate, branch or state, when the phase's recorded monitor or agent pid is alive (`--force` included), with a refusal that says `stop` would not end this lock-free run and to SIGTERM the monitor first after `ps -p`; a dead leftover still restarts exactly as before.**

## Performance

- **Duration:** not measured (see frontmatter)
- **Completed:** 2026-09-23
- **Tasks:** 2 (RED, GREEN)
- **Files modified:** 2

## Accomplishments

- Closed verification Gap B for `start`: a second `start` against a live run no longer wipes its gates, overwrites its state and launches a second agent.
- `refuse_start_over_a_live_run` runs at `start` line 65, strictly between the `_phase_lock` binding (line 55) and the first `Gates::cleanup` (line 73), as printed by the gate.
- Four e2e tests pin the refusal (both pid roles), the dead-leftover control and the no-state carve-out.

## Task Commits

1. **Task 1: RED tests** - `047c88e` (test, committed by the orchestrator after its independent re-run of the RED gate)
2. **Task 2: GREEN guard** - `fd9be36` (feat, committed by the orchestrator as `feat(48-19): …` after its independent re-run of the GREEN gate)

**Plan metadata:** this SUMMARY is committed separately by the orchestrator.

## Files Created/Modified

- `crates/devflow-cli/src/commands.rs` - `refuse_start_over_a_live_run`, `live_run_refusal`, and the guard call in `start`
- `crates/devflow-cli/tests/start_lock_e2e.rs` - four tests plus `LiveProcess`, `plant_leftover_run`, `write_agent_pid`, `phase_file_snapshot`, `run_start`, `assert_start_refuses_live_run`

## RED evidence

Task 1 gate, run byte-for-byte (outer exit 0):

```
start_refuses_a_phase_whose_recorded_monitor_is_alive exit=101 failed_line=1 reason=1
start_refuses_a_phase_whose_recorded_agent_is_alive exit=101 failed_line=1 reason=1
start_replaces_a_leftover_state_whose_recorded_processes_are_dead exit=0 one_passed=1 filtered=6 filtered out
start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process exit=0 one_passed=1 filtered=6 filtered out
production_untouched_diff_exit=0
gate_rc=0
```

Failure reasons, from separate re-runs (the gate deletes its logs):

- Monitor arm: panicked at `start_lock_e2e.rs:422:5` with `start must refuse a phase whose recorded monitor is alive`. Line 422 is the `!output.status.success()` assertion; captured stdout shows `started phase 95 in supervise mode … monitor will auto-advance`, so pre-fix `start` ran to completion over the live run.
- Agent arm (`--force`): same line, `start must refuse a phase whose recorded agent is alive`; stdout shows `started phase 96 in supervise mode …`.
- Both logs show `0 passed; 1 failed; … 6 filtered out`: no compile error, no panic elsewhere, no zero-match run. `commands.rs` was untouched at RED.
- The orchestrator re-ran the RED gate independently with an identical result before committing 047c88e. The `gsd check tdd-red-evidence` record was not written; the orchestrator accepted the plan gate as the contract.

## GREEN evidence

Task 2 gate, run byte-for-byte after a warm build (the orchestrator's independent re-run was identical):

```
start_refuses_a_phase_whose_recorded_monitor_is_alive exit=0 one_passed=1 filtered=6 filtered out
start_refuses_a_phase_whose_recorded_agent_is_alive exit=0 one_passed=1 filtered=6 filtered out
start_replaces_a_leftover_state_whose_recorded_processes_are_dead exit=0 one_passed=1 filtered=6 filtered out
start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process exit=0 one_passed=1 filtered=6 filtered out
file start_lock_e2e exit=0 ok_lines=1
file gate_wedge_e2e exit=0 ok_lines=1
file start_reachability_e2e exit=0 ok_lines=1
file yes_ship_config exit=0 ok_lines=1
file phase7_cli exit=0 ok_lines=1
start_lock_seven_passed=1
bin_exit=0 ok_lines=3
start_fn_lines=407 lock_line=55 guard_line=65 cleanup_line=73 guard_calls_in_start=1
doc_names_999_136=1
clippy_exit=0
fmt_exit=0
gate_rc=1
outer_exit=1
```

`gate_rc=1` comes solely from `bin_exit=0 ok_lines=3`; see Deviations. With the corrected bin-suite check the count is 1 and every other line already passes. Recorded line numbers: lock_line=55 < guard_line=65 < cleanup_line=73, guard_calls_in_start=1.

## Class enumeration: every path by which `start` gets past the guard while a recorded pid is live

| # | Path | Outcome |
|---|------|---------|
| 1 | `--force` | Refused. The guard ignores `force`; the agent arm runs with `--force` |
| 2 | No state file, agent pid file names a live process | Allowed on purpose: the no-state carve-out, pinned by the phase-98 test |
| 3 | No state file, live monitor | Allowed (the monitor pid lives only in state, so it is unreadable). Covered by the carve-out. Reachable only after `recover --clean` over a live run or a hand deletion; the refusal text allows `recover --clean` only after the named processes have exited |
| 4 | State file exists but does not parse (corrupt or legacy) | Partial: the agent pid file is still checked and refuses if live; a live monitor is not seen. Recorded limit, not tested |
| 5 | State loads with `monitor_pid: None` and a missing or unparseable agent pid file while an agent runs | Allowed; nothing to read. This arises between `launch_stage_inner` clearing `monitor_pid` (pipeline_launch.rs:1023) and saving the new pid (:1067), which runs inside `start` or `advance` while they hold the lock, so the existing contention refusal covers it. After spawn and before the monitor writes the agent pid file, the saved monitor pid refuses. Reasoned from source, not tested |
| 6 | Recorded pid is a zombie | Treated as dead (`agent_running` excludes zombies); start proceeds. Correct: the process is dead |
| 7 | Recorded pid now belongs to another user's process | Treated as dead (`kill(0)` returns EPERM, so `agent_running` is false). Fails open, but such a process cannot be this user's DevFlow run. Existing helper behaviour, unchanged |
| 8 | Recorded pid recycled by an unrelated process of the same user | Refused (false refusal), accepted with Option A; the message sends the operator to `ps -p` |
| 9 | `--dry-run` | Returns before the lock and the guard and writes nothing; not a way past the guard |
| 10 | Agent-exit → `advance` lock-free window (999.136) | Refused while the Legacy monitor's recorded pid is alive. Reasoned, not tested; other writers in that window remain 999.136's scope |

The guard calls `workflow::load_state`, which runs `migrate_legacy_state`, a possible write. `start` already calls it before the lock through `fresh_state_carrying_phase_failures`, so the guard adds no new first write.

## Known limits (recorded, not fixed here)

- **999.136, operator-deferred.** The window between agent exit and the monitor's `advance` taking lock-NN stays lock-free. Reasoned, not tested: the Legacy monitor stays alive across that window, so its recorded pid should make `start` refuse there too. Other writers in that window, such as the recover sweep, remain 999.136's scope. Not exercised by any test.
- **Pid liveness, not identity (accepted by the operator with Option A, 2026-09-22).** A recycled pid makes a dead run look live: a fail-closed false refusal. The refusal tells the operator to check each pid with `ps -p <pid>` before signalling it; once `ps -p` shows a named pid is not this phase's DevFlow process, `devflow recover --clean --phase N` removes the state and the next `start` proceeds. Same class as D-03's pid-only lock reclaim and the deferred "lock reclaim by pid plus start time" (Phase 50). Not exercised by a test.
- **No-state carve-out (operator-approved narrowing, 2026-09-22).** The guard applies only when the phase has a state file. Reason: `recover --clean` removes the state but not the agent pid file, and no DevFlow verb removes that file, so a recycled pid in a leftover pid file would otherwise block `start` with no DevFlow repair. Cost: if `recover --clean` has already cleared state while the agent was alive, `start` launches a second agent beside it; recover's own "cleared anyway" warning and the refusal text's "only after the named processes have exited" guard against that. **Exercised** by `start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process`.
- **Pre-existing: `stop` does not end a lock-free run (backlog 999.138).** In the window the guard targets, `devflow stop --phase N` marks the state stopped and signals nothing, and `advance_for_stage` never reads that mark. The refusal says so instead of offering `stop`. Not exercised by a test.
- **"Monitor first" SIGTERM ordering is reasoned from source, not tested.** It rests on monitor.rs's `sigterm_to_monitor_also_kills_the_agent` and on the reading that signalling the agent first lets its monitor run `advance` into the next stage. No test signals a live monitor-owned run in either order.
- **Unloadable state file.** Checked on the agent pid file only; no monitor pid can be read. Not exercised by a test.

## What this does NOT establish

- The fixtures fabricate a live run: a test-owned `sleep` is recorded as the monitor or agent. They prove the guard reads the recorded pids, not that a real launch always records a pid before a second `start` can run (during the pre-save window the first `start` holds the lock, which the existing contention refusal covers).
- Identity: a live recorded pid is assumed to be this phase's process (see the pid-liveness limit).
- That the refusal's `stop`, `ps -p`, SIGTERM-ordering and `recover --clean` guidance works end to end. The tests assert the text is present, not that following it ends a run safely.
- The corrupt-state branch, 999.136's window and the `stop` finding are untested.
- Container parity: that is the orchestrator's `scripts/check-in-container.sh` run, not done here.
- The four tests are single runs each (plus the orchestrator's re-runs); they bound nothing about flakiness beyond "passed every time it was run here".

## Operator rulings (2026-09-22)

Recorded as the plan records them, for re-verification to close 48-VERIFICATION.md's human_verification item on criterion 2's scope:

- **Gap A:** add the two start-driven #200 arms. Done in 48-18.
- **Gap B:** the second-`start`-over-a-live-run hole is ruled a gap, and this plan closes it for `start`.
- **Liveness, Option A,** approved in the operator's words: "refuse when the recorded agent pid or monitor pid is alive". The recycled-pid false refusal is an accepted limit.
- **No-state carve-out, an operator-approved narrowing:** `start` does not refuse when the phase has no state file, even if the agent pid file names a live process. Reason: `recover --clean` leaves that pid file, so a recycled pid would otherwise block `start` permanently.
- **The agent-exit → `advance` lock-free window** remains backlog 999.136, operator-deferred.

## Decisions Made

- Liveness uses only the existing helpers (`agent_pid_from_file`, `agent::agent_running`); no State fields, start times, or lock/status/doctor/recover changes.
- The refusal text is built by a separate `live_run_refusal` so the guard function stays short.

## Deviations from Plan

**1. [Gate defect - orchestrator verdict] Task 2's bin-suite line count cannot pass on correct work**
- **Found during:** Task 2 gate run.
- **Issue:** The verbatim gate returns `gate_rc=1` solely from `bin_exit=0 ok_lines=3`; the bin suite itself exits 0 with 408 passed, 0 failed. The gate requires exactly one `^test result: ok\. [1-9][0-9]* passed; 0 failed` line. Two phase-27 hostile-GIT_DIR tests, added in 1542de6 and 48fa2de (`commands::tests::planning_doc_staleness::tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir`, `staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`), re-invoke the test binary as a filtered child, and each child prints its own `test result: ok. 1 passed; … 407 filtered out` line to the inherited stdout.
- **Corrected check:** `bok=$(grep -cE "^test result: ok\. [1-9][0-9]* passed; 0 failed; [0-9]+ ignored; [0-9]+ measured; 0 filtered out" "$L/bin.log" || true)`, which matches only the harness's top-level line. `bin_exit=0` remains a separate requirement.
- **Three-way test (orchestrator, on a real bin log):** full run old=3, new=1 (lines 97 and 482 are the children's `407 filtered out` lines; line 511 is the real `408 passed … 0 filtered out` line); a real filtered run (`planning_doc_staleness`) new=0; a sed copy turning the top-level line into `FAILED. … 1 failed` new=0.
- **Verdict:** the orchestrator's call (option 1). The 48-19 plan gate itself is unedited (`git diff --quiet f7cc0d6..HEAD -- 48-19-PLAN.md` exits 0).

**2. [Test hygiene] Test-file choices in Task 1 (accepted by the orchestrator)**
- Every `start` run in the four tests goes through `run_start`, which sets `DEVFLOW_GATE_TIMEOUT_SECS=15` on the child only (`Command::env`), so a monitor launched by a proceeding `start` cannot park forever. It is applied uniformly, so the fixtures still differ only in pid liveness plus `--force`.
- Each refusal arm asserts its pre-run snapshot covers the state file and both planted Code gate files, so byte-identity cannot pass vacuously.
- The snapshot lists only existing files, so a deleted file shows as a snapshot difference, not a read panic; equality is on (path, bytes) pairs and the failure message prints paths only.

**Total deviations:** 1 gate defect (orchestrator verdict), 1 set of test-hygiene choices. **Impact:** none on production behaviour; the gate deviation is a counting defect in the plan's verify text.

## TDD Gate Compliance

RED `test(48-19)` 047c88e precedes GREEN `feat(48-19)` fd9be36. No REFACTOR commit.

## Issues Encountered

Only the gate line-count defect above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Criterion 2's residual hole is closed for `start`. Re-verification can close human_verification item 1 using the Operator rulings above. 999.136 and 999.138 remain open backlog items.

## Self-Check: PASSED

- `crates/devflow-cli/src/commands.rs` and `crates/devflow-cli/tests/start_lock_e2e.rs` exist and are modified in fd9be36 / 047c88e.
- `git log --oneline -3` shows fd9be36 (feat) and 047c88e (test) above f7cc0d6; `git rev-list --count f7cc0d6..HEAD` = 2.

---
*Phase: 48-survivable-state-writes-and-honest-gate-recovery*
*Completed: 2026-09-23*

## Post-review fix (2026-09-23)

The gap-closure code review (48-REVIEW.md) found that three pieces of the refusal guidance recorded above were false. They were fixed test-first: RED `6ee9459`, GREEN `20cc64c`. The fix changes text only, not behaviour.
- **CR-01.** `ps -p <pid>` shows only the executable name, so it cannot confirm that a pid belongs to this phase. The text now names `ps -ww -o pid=,lstart=,args= -p <pid>` and says what identifies this phase's processes:
  - the monitor is a `__monitor` process with `--phase N` in its args, or an `sh -c` script that names `.devflow/phase-NN-` files and ends in `advance --phase N`;
  - the agent's parent is that monitor.
  This matters because the operator's acceptance of Option A relied on this identity check.
- **WR-01.** A recycled live pid needs `recover --clean` and then `start`. Running `start` again only refuses again.
- **WR-02.** The advice to signal the monitor first, and the claim that `stop` "does not end this run", are now qualified. While the Legacy monitor runs a foreground `devflow advance` child, that child has to be signalled too.
Deferred to backlog: WR-03 → 999.139, WR-04 → 999.140, WR-05 → 999.141. The guidance is still not exercised end to end.

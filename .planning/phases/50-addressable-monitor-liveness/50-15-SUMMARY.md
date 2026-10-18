---
phase: 50-addressable-monitor-liveness
plan: 15
subsystem: cli
tags: [launch-guard, supervisor, socket, sidecar, corrupt-state, concurrency, rust]

requires:
  - phase: 50-06
    provides: "sweep_for_in with name attribution for sidecar-less sockets (RV-05)"
  - phase: 50-09
    provides: "backstop::assess, BackstopDecision"
  - phase: 50-12
    provides: "socket_dir_unlistable (doctor's unlistable sock dir distinction)"
  - phase: 50-14
    provides: "stop --phase N --launch <id>; bare stop on an unloadable state"
provides:
  - "refuse_launch_over_a_live_run: probes the state's supervisor record, or sweeps the sidecars when the state will not load; reads no monitor_pid"
  - "socket-naming live-run refusal for start and resume, with a stop line per socket"
  - "fail-closed refusal over an unlistable socket directory"
  - "V-23 (start, resume), V-37 (RV-05) and V-12 tests with controls"
affects: [50-16, 50-17, 50-22, 50.1]

actuals:
  tokens: 22394      # chars/4 over `git diff d553852..58579de` (89,576 chars); chars/4 over the three changed files whole is 140,915
  tasks: 2
  commits: 3
plan_head_before: d5538529230abfa47479372625cc55b4924d6b11
plan_head_after: 58579de1ffd5431a98b1ddcc4a1f25278acc4ed6

tech-stack:
  added: []
  patterns:
    - "LiveRun e2e fixture: a real `devflow start` with a sleeping fake agent, reaped on drop through its socket, then by identity-checked SIGKILL of the agent group and supervisor, so a test may corrupt the state or sidecar"
    - "Supervisor-arm fixtures remove the agent pid file so only the socket can refuse"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/start_lock_e2e.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs

key-decisions:
  - "An unlistable socket directory over an unloadable state refuses the launch and names the directory; a missing directory means nothing is live (50-06 threat flag, reusing 50-12's socket_dir_unlistable)"
  - "With no cache dir (no HOME/XDG/DEVFLOW_CACHE_DIR), the corrupt-state branch sweeps nothing, as stop and recover already do"
  - "A STALE socket counts as live unless backstop::assess returns PreviousBoot, EmptyGroup or LeaderRecycled; every other decision, including ones added later, fails closed"
  - "A loaded-state STALE socket whose sidecar is missing or broken refuses and names bare `devflow stop --phase N`"

patterns-established:
  - "Launch refusal lists one repair line per live socket: `--launch <id>` when its sidecar or record identifies it, bare `stop --phase N` when it has no usable sidecar"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "start --force over a live supervisor whose state will not load refuses, names `devflow stop --phase N --launch <id>`, publishes no second socket, leaves the state bytes and events unchanged (V-23)"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_refuses_over_a_live_supervisor_when_the_state_will_not_load"
        status: pass
    human_judgment: false
  - id: D2
    description: "The refusal's printed `--launch` stop line, run as printed, exits 0, the socket probes Gone, the agent group is gone, the state bytes are unchanged, and a following start no longer refuses"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#the_corrupt_state_refusal_names_a_stop_that_ends_the_run"
        status: pass
    human_judgment: false
  - id: D3
    description: "resume over the same fixture is refused the same way (R-12)"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#resume_refuses_over_a_live_supervisor_when_the_state_will_not_load"
        status: pass
    human_judgment: false
  - id: D4
    description: "RV-05: state and live sidecar both corrupt refuses, naming the socket, the sidecar parse error and bare `devflow stop --phase N`, which ends the run (V-37); another root's corrupt-sidecar socket does not refuse (control)"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_refuses_when_the_state_and_the_live_sidecar_both_will_not_load"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#a_live_socket_named_for_another_root_does_not_block_a_corrupt_state_start"
        status: pass
    human_judgment: false
  - id: D5
    description: "Controls: a corrupt state with no live supervisor proceeds as before; the no-state carve-out is unchanged"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_proceeds_over_a_corrupt_state_with_no_live_supervisor"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process"
        status: pass
    human_judgment: false
  - id: D6
    description: "An unlistable socket directory over an unloadable state refuses and names the directory; a missing directory proceeds"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::launch_guard::the_launch_guard_refuses_an_unlistable_socket_dir_over_an_unloadable_state"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::launch_guard::the_launch_guard_proceeds_over_an_unloadable_state_when_the_socket_dir_is_missing"
        status: pass
    human_judgment: false
  - id: D7
    description: "STALE classification: only PreviousBoot, EmptyGroup and LeaderRecycled let a STALE socket through; a probe error refuses"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::launch_guard::only_a_provably_gone_group_lets_a_stale_socket_through_the_launch_guard"
        status: pass
    human_judgment: true
    rationale: "Only the decision-to-liveness mapping is tested. No test drives a real STALE socket (supervisor killed, group alive or gone) through the guard, and no test drives a probe error through it; those arms are covered by reading the code only."
  - id: D8
    description: "Live-run refusal text rewritten for start and resume: socket-naming lines, deleted shell-monitor sentences and the false 'stop signals nothing' claim asserted absent, golden updated"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs#start_refusal_text_stays_byte_identical_for_a_live_monitor"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_lock_e2e.rs (start/resume monitor and agent arms, 4 tests)"
        status: pass
    human_judgment: false
  - id: D9
    description: "Two racing starts for one phase yield exactly one ALIVE supervisor and one agent pgid; the loser is refused at the lock or the guard (V-12); two phases yield two (control)"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#two_concurrent_starts_yield_exactly_one_live_supervisor"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#two_starts_for_different_phases_yield_two_live_supervisors"
        status: pass
    human_judgment: false

duration: 23min
completed: 2026-10-18
status: complete
---

# Phase 50 Plan 15: Launch Guard Over a Corrupt State Summary

**`start` and `resume` now find a live run through its supervisor socket: the state's record when the state loads, the sidecars (or the socket's file name) when it does not, and they refuse over an unlistable socket directory. No `monitor_pid` is read.**

## Needs awareness

- **No test drives a STALE socket through the guard.** The STALE arms (record and sweep) and the probe-error arm are read-verified only. The unit test covers the mapping from each backstop decision to live/not live, nothing more. An e2e would need a SIGKILLed supervisor whose agent group outlives it, with the agent pid file removed (otherwise the pid half refuses first).
- **V-12's loser always lost at the per-phase lock in normal runs** (8 of 8, counting the gate runs). The guard path for a racing loser ran only under mutant MA, which made the lock blocking (2 of 2, guard refusal printed). With the guard also disabled (MB), the test failed at its stderr check: the loser hit "branch exists". The one-socket count was therefore not the assertion that caught MB.
- **`cleanup`'s live-run refusal still names `devflow resume` where `stop` is right.** This plan does not touch that path (50-12 and 50-14 noted it too).
- **T-50-90/T-50-91 stay open as accepted.** A loaded state with no record (or a GONE record) over a live, unrecorded supervisor is not refused. Phase 50.1 owns it.
- **Commit `b464aec`'s subject is 76 characters.** The commit-msg hook warned and the commit landed; per instructions it was not amended.

## Performance

- **Duration:** 23 min
- **Started:** 2026-10-18T14:19:03Z
- **Completed:** 2026-10-18T14:41:19Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- **The guard asks the socket.** When the state loads, it probes `state.supervisor`:
  - ALIVE refuses.
  - STALE refuses unless `backstop::assess` proves the group gone.
  - A probe error refuses.
  - GONE proceeds.
- **A corrupt state no longer hides a run (999.139).** When the state exists but will not load, the guard runs `sweep_for_in` for this root and phase.
  - A socket whose sidecar parsed is classified the same way, with no record.
  - A socket with no usable sidecar refuses unless it probes GONE (RV-05).
  - A socket directory that exists but cannot be listed refuses and names the directory. A missing directory means nothing is live.
- **The refusal names one repair line per socket.**
  - When a record or sidecar identifies the socket: `devflow stop --phase N --launch <id>`.
  - When the socket has no usable sidecar: its path, its launch id, the sidecar problem and bare `devflow stop --phase N`.
- **Stale text is gone.** The shell-monitor sentences and the claim that `stop` "signals nothing" are removed. The agent-pid paragraph is kept, with its identity checks.
- **V-12.** Two racing starts produce one live supervisor and one agent group.

## Task Commits

1. **Task 1 (tracer): a corrupt state over a live supervisor refuses start and resume**
   - RED `4f29135` (test)
   - GREEN `b464aec` (feat)
2. **Task 2: two racing starts yield exactly one live supervisor** — `58579de` (test)

**Plan metadata:** see the docs commit that adds this file.

## Files Created/Modified

- `crates/devflow-cli/src/commands.rs`:
  - `refuse_launch_over_a_live_run` now takes the cache dir;
  - new `LiveSocket`, `recorded_live_socket`, `swept_live_socket`, `stale_group_may_live`, `unlistable_socket_dir_refusal` and `agent_refusal_guidance`;
  - `live_run_refusal` is rewritten; three `launch_guard` unit tests are added.
- `crates/devflow-cli/tests/start_lock_e2e.rs`:
  - the `LiveRun` fixture;
  - six new corrupt-state tests;
  - the monitor arms now launch a real supervisor;
  - fragments are split per arm, removed sentences are asserted absent, and the golden is rewritten;
  - the plant helpers no longer write `monitor_pid`.
- `crates/devflow-cli/tests/supervisor_e2e.rs`: V-12 and its control, plus a shared `live_supervisors` counter.

## TDD Gate Compliance

RED evidence came from nextest JUnit (scratch profile `red`), checked with `gsd-tools check tdd-red-evidence`. The records are `target/red-evidence/50-15-t1-*.json` (gitignored).

| Test | RED commit | Classifier | Failed on | GREEN |
|------|-----|------|------|------|
| `start_refuses_over_a_live_supervisor_when_the_state_will_not_load` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | `start --force` succeeded over the live run (it launched a second run; both were reaped) | `b464aec` |
| `the_corrupt_state_refusal_names_a_stop_that_ends_the_run` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | no refusal: start proceeded | `b464aec` |
| `resume_refuses_over_a_live_supervisor_when_the_state_will_not_load` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | resume passed the guard and failed later, at `state JSON failed`; no refusal | `b464aec` |
| `start_refuses_when_the_state_and_the_live_sidecar_both_will_not_load` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | `start --force` succeeded over the live socket | `b464aec` |
| `start_refuses_a_phase_whose_recorded_monitor_is_alive` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | old text (`monitor pid N alive`, no `--launch` line) | `b464aec` |
| `start_refuses_a_phase_whose_recorded_agent_is_alive` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | missing the new closing sentence; old sentences present | `b464aec` |
| `resume_refuses_a_phase_whose_recorded_monitor_is_alive` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | old monitor-pid text | `b464aec` |
| `resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | old text | `b464aec` |
| `start_refusal_text_stays_byte_identical_for_a_live_monitor` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | golden mismatch against the 48-19 text | `b464aec` |
| `commands::tests::launch_guard::the_launch_guard_refuses_an_unlistable_socket_dir_over_an_unloadable_state` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | `Ok(())`: the guard failed open | `b464aec` |
| `commands::tests::launch_guard::only_a_provably_gone_group_lets_a_stale_socket_through_the_launch_guard` | `4f29135` | exit 100, `RED_EVIDENCE_OK` | `PreviousBoot proves the group gone` (the scaffold counted every decision live) | `b464aec` |
| Controls: `start_proceeds_over_a_corrupt_state_with_no_live_supervisor`, `a_live_socket_named_for_another_root_does_not_block_a_corrupt_state_start`, `…_socket_dir_is_missing` | in `4f29135` | green at RED, as controls should be | — | — |

**Semantic assessment.**
- Each RED failed on its planned assertion with the fixture's preconditions met. The live run's start succeeded, the supervisor answered ALIVE, and the state was confirmed unloadable.
- The RED commit carried a behaviour-preserving scaffold: the guard took `cache_dir` and ignored it, and `stale_group_may_live` returned `true`. GREEN replaced both.
- **Task 2 has no RED.** V-12 passed on its first run, because the per-phase lock already serialised racing starts before this plan. It is a proof test of R-1, so there is no `feat` commit for it. Its discriminating evidence is mutants MA/MB below, not a RED run.

## Mutation and Discrimination Evidence

Each mutant was applied to `commands.rs` and then restored from a saved copy. `cmp` confirmed the restored file was byte-identical.

| Mutant | Result |
|---|---|
| M1: the corrupt-state branch sweeps nothing | 4 fail: V-23, the repair test, resume V-23, V-37 |
| M2: a sweep entry with no usable sidecar is not live | 1 fails: V-37 |
| M3: unlistable socket-dir check disabled | 1 fails: the unlistable unit test |
| M4: sweep every socket (`sweep_in`), not this root's | 1 fails: the other-root control. This is the evidence that the control discriminates |
| MA: start's lock made blocking (the loser always reaches the guard) | V-12 passes 2/2 and prints "live-supervisor guard" |
| MB: MA plus the start guard disabled | V-12 fails 2/2 at the stderr check |
| Gate-2 scan control | The shell scan run against the RED commit's guard finds no `sweep_for_in` and does find `monitor_pid`, so it would fail there |

## Verification (commands run, actual exit codes)

- **Task 1 gate 1** (seven named tests, verbatim): rc 0, seven `1 test run: 1 passed`.
- **Task 1 gate 2** (comment-stripped guard scan plus all of `start_lock_e2e`): rc 0, `18 tests run: 18 passed`.
- **Tracer gate** (interactive, `end-of-phase`, automated-only verify): Task 1's verify was re-run and passed before Task 2 began.
- **Task 2 gate 1** (two named tests, verbatim): rc 0, two `1 passed`. Earlier I also ran the pair 6 more times, 6 of 6 passing.
- **Task 2 gate 2 (sentinel-HOME `scripts/check.sh all`), verbatim, at `58579de`:** `==> check.sh: all OK`, `check_all_exit=0 leaked_entries=0`, gate rc 0.
- **All `devflow` package tests at GREEN** (`b464aec` plus uncommitted Task 2 tests absent): `685 tests run: 685 passed (3 leaky), 2 skipped`.
- **`cargo clippy -p devflow --all-targets -- -D warnings`:** rc 0 at RED, at GREEN and before the Task 2 commit.
- **Semver:** no `devflow-core` file changed, so the 50-01 semver gate was not run. This plan adds or changes no `pub` item.
- **What these checks do not establish:**
  - the STALE and probe-error arms under a real socket (see Needs awareness);
  - V-12's guard path without a mutant;
  - behaviour as root, where a 0o000 directory stays listable and the unlistable test's precondition assert fails loudly.

## Decisions Made

- An unlistable socket directory refuses and names the directory with its error kind. A missing directory proceeds. This reuses 50-12's `socket_dir_unlistable`.
- With no cache dir, nothing is swept, the same as `stop` and `recover --clean`.
- A loaded-state STALE socket whose sidecar is missing or broken refuses and names bare `devflow stop --phase N`. That stop then refuses with its own manual repair, because nothing can validate the group.
- The resume paragraph now says "`devflow status` names `devflow resume` for a phase whose supervisor is GONE". The old text also named `doctor`, but I checked only `status`'s `recovery_hints` (GONE while active names resume), so `doctor` was dropped from the claim.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Fail closed on an unlistable socket directory**
- **Found during:** Task 1. This was the carried-forward 50-06 threat flag, which the plan text did not cover.
- **Issue:** `sweep_in` returns `[]` both for a missing `sock/` and for one it cannot list. Read as "nothing live", the second case would let a launch run over a live run.
- **Fix:** The corrupt-state branch now checks `socket_dir_unlistable` first. If the directory cannot be listed, it refuses, naming the state file, the directory and the error kind. A missing directory still proceeds.
- **Files modified:** `crates/devflow-cli/src/commands.rs`.
- **Verification:**
  - the unlistable unit test, RED then GREEN;
  - the missing-directory control;
  - mutant M3.
- **Committed in:** `4f29135` (tests), `b464aec` (fix).

**2. [Rule 1 - Test design] Supervisor-arm fixtures remove the agent pid file**
- **Found during:** Task 1, while writing the fixtures.
- **Issue:** A real launched run writes the agent pid file, so the unchanged pid half refuses every live-run fixture by itself. The supervisor-arm and corrupt-state tests would then pass whether or not the socket check works. The plan said to keep the agent-pid plant.
- **Fix:** `LiveRun::hide_agent_pid_file` removes the file in the supervisor-arm and corrupt-state fixtures. Those tests also assert that the refusal names no `agent pid`. The agent-arm tests and the golden (which covers both paragraphs) keep the pid file. This models 999.139's shape, where the pid half saw nothing.
- **Files modified:** `crates/devflow-cli/tests/start_lock_e2e.rs`.
- **Verification:** RED made V-23 and V-37 proceed. The pid half alone would have refused them.
- **Committed in:** `4f29135`.

**3. [Rule 3 - Fixture] The V-12 control uses two project roots**
- **Found during:** Task 2.
- **Issue:** Two concurrent `--no-worktree` starts for different phases in one checkout would each run `git` branch creation and checkout in the same repository. That is a race unrelated to the guard, and it could make the control flaky. This is inferred from reading `start`, which takes no project-level lock. I did not try it.
- **Fix:** The control runs phases 64 and 65 in two roots that share one hermetic cache. Its purpose, showing that the counter can see more than one supervisor, is unchanged.
- **Files modified:** `crates/devflow-cli/tests/supervisor_e2e.rs`.
- **Committed in:** `58579de`.

**4. Additions beyond the plan's test list**
- The STALE decision-mapping unit test.
- The missing-socket-dir control.
- `resume_relaunches_a_phase_whose_recorded_processes_are_dead` now asserts that `state.supervisor` is recorded. Before, it asserted `monitor_pid != NEVER_LIVE_PID`, which stopped meaning anything once the plant no longer writes `monitor_pid`.

---

**Total deviations:** 3 auto-fixed (1 missing critical, 1 test design, 1 fixture), plus extra tests.
**Impact on plan:** The Rule 2 fix closes a fail-open path. The other changes make the tests discriminate. No scope creep into other verbs.

## Issues Encountered

None blocking. The known flaky `auto_rescan_rejection_parks_for_supervised_repair` did not fail in either full run.

## Known Stubs

None. The `placeholder` matches in `start_lock_e2e.rs` are the golden's `{launch}`, `{socket}` and `{pid}` substitution markers, not stubs. No broken-windows entry was recorded: there was no stub, skipped test or unrun verify, and the deviations above are documentation, not open defects.

## Cleanup

- **Processes:** checked by `ps` after every e2e, mutant and gate run.
  - No `__monitor`, `sleep 120` or `sleep 60` process survived.
  - Five `sleep 30` children came from the known leaky tests: four from `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` and one from a `devflow-core` test during `check.sh`.
  - I confirmed each one's `CARGO_MANIFEST_DIR` was this worktree before acting. All had exited on their own by the time I tried to kill them by PID.
- **Files:**
  - no `/tmp/dfs*`, `/tmp/dfh*`, `/tmp/dfu*` or `/tmp/p50home*` remain;
  - the operator's `~/.cache/devflow` still holds 1041 files and 0 sockets, the same count as at start.
- **Kept on purpose:** the RED evidence records under `target/red-evidence/50-15-*` (gitignored), and the scratch directory `exec-50-15/`, which is removed at hand-off.

## User Setup Required

None.

## Next Phase Readiness

- **50-16 / 50-22:** the guard no longer reads `monitor_pid`. The test plant helpers no longer write it.
- **50-17 (OPERATIONS):** document:
  - the new refusal (one `stop` line per socket, `--launch` or bare);
  - the unlistable-socket-directory refusal;
  - that a corrupt state no longer hides a live run.
- **Phase 50.1:** T-50-90 and T-50-91 (a live supervisor with no record) are still not refused when the state loads.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-18*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/commands.rs, crates/devflow-cli/tests/start_lock_e2e.rs, crates/devflow-cli/tests/supervisor_e2e.rs
- FOUND commits (ancestors of HEAD): 4f29135, b464aec, 58579de
- RED gate commit `test(50-15)` precedes GREEN `feat(50-15)` for Task 1; Task 2 has no feat commit (proof test, see TDD Gate Compliance)
- Artifact `contains` checks: `sweep_for_in` and `backstop::assess` in commands.rs; `fn start_refuses_over_a_live_supervisor_when_the_state_will_not_load` in start_lock_e2e.rs; `fn two_concurrent_starts_yield_exactly_one_live_supervisor` in supervisor_e2e.rs

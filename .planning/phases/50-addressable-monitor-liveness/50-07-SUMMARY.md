---
phase: 50-addressable-monitor-liveness
plan: 07
subsystem: supervisor
tags: [rust, unix-socket, signals, self-pipe, waitid, process-group, cargo-mutants]

requires:
  - phase: 50-05
    provides: "SocketPath, publish, probe, Liveness, protocol::{read_request, write_reply, peer_is_same_user, Reply, PingReply, ShutdownAccepted, ShutdownStopped}, test_support::spawn_test_in_child"
  - phase: 50-06
    provides: "Sidecar, write_sidecar, remove_launch_files, request_shutdown, ShutdownOutcome"
provides:
  - "devflow_core::supervisor::runtime: OwnerPhase (as_str), StopSource, AgentOutcome {Completed, Stopped}, StopAction, SafePoint, StopToken (stop_requested, phase, enter, request_stop, safe_point), on_stop_request, on_agent_exit, at_safe_point, install_signal_pipe, Supervisor (serve, wait_agent, token, hand_off, finish, Drop)"
  - "The server side of the 50-05 protocol now has a non-test caller: its expect(dead_code) attributes are gone, and so is write_sidecar's"
affects: [50-08, 50-11, 50-13, 50-23]

actuals:
  tokens: 17862
  tasks: 3
  commits: 7
plan_head_before: 1ceb948ceaa764cb85a184c1375764c1d635ce29
plan_head_after: b02ab80b38543501ae70536a0b44a8c2d502769b

tech-stack:
  added: []
  patterns:
    - "Terminal owner: one Mutex<OwnerState> + Condvar behind StopToken; request_stop, the exit decision (observe_exit) and safe_point all decide under it, and the control thread sends group signals under it only while agent_exited is false"
    - "Exit observation: waitid(P_PID, pid, WEXITED|WNOWAIT), then under the lock SIGKILL leftover members while the zombie leader holds the pgid, then reap"
    - "Self-pipe: getpid guard + one-byte write in the handler; the control thread polls {listener, pipe} with a TERMINATE_VERIFY_POLL tick"
    - "Runtime process tests: spawn_test_in_child supervisor, ChildGuard (SIGKILL + reap on drop) and GroupGuard (SIGKILL leftover group on drop) bound before any assertion"

key-files:
  created:
    - crates/devflow-core/src/supervisor/runtime.rs
  modified:
    - crates/devflow-core/src/supervisor.rs
    - crates/devflow-core/src/supervisor/protocol.rs

key-decisions:
  - "finish joins the control thread and removes the socket and sidecar before it writes the final line to parked callers (the plan lists the line first), so a client that reads stopped always finds the socket already gone"
  - "A shutdown in Finished replies accepted then stopped when the run ended as a stop, and {ok:false,error:already_finished} otherwise; the plan only said 'reply that the run already finished'"
  - "A stop always reports exit_code 143 (STOPPED_EXIT_CODE = 128 + SIGTERM), whatever the agent's own exit status, per RESEARCH Finding 3"
  - "The control thread waits for the exit on the owner Condvar (up to TERMINATE_VERIFY_WAIT) and then SIGKILLs; the KillAgentGroup SIGTERM and the SIGKILL are both sent under the owner lock"
  - "rfc3339_now keeps its expect(dead_code): the runtime does not build sidecars, the launcher (50-08) will; only its reason text changed from 50-07 to 50-08"

patterns-established:
  - "A barrier-driven interleaving test must assert its window was reached before joining the other thread, or a wrong early exit hangs instead of failing"
  - "A child-side cleanup guard must be disarmed once the code under test owns the cleanup, or it hides the defect the test is for"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "A child supervisor answers ping with owner agent_running, takes a socket shutdown, ends its agent group, reports Stopped {143, advance_suppressed}, and its socket is Gone; control: an agent that exits 7 on its own completes with Completed(7)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::shutdown_over_the_socket_kills_the_agent_group_and_reports_stopped"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::an_agent_that_exits_on_its_own_completes_with_its_code"
        status: pass
    human_judgment: false
  - id: D2
    description: "V-14: five stop/exit interleavings against the real StopToken and a real agent process all end stopped; the same machine with no stop advances; every (OwnerPhase, stop) pair has exactly one decision"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::natural_completion_without_a_stop_advances"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::every_owner_phase_has_exactly_one_stop_decision"
        status: pass
      - kind: other
        ref: "Task 2 cargo-mutants gate verbatim (TMPDIR on disk): mutants_exit=0 caught=1 missed=0 timeouts=0, gate rc 0; plus 10 hand mutants, all caught"
        status: pass
    human_judgment: false
  - id: D3
    description: "SIGTERM and SIGINT to the supervisor become a stop through the self-pipe (agent group and socket gone, outcome Stopped); the agent's SigBlk has no TERM/INT bit and its SigCgt no TERM bit; control: without the pipe the supervisor dies on TERM and leaves its group running"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::sigterm_is_delivered_as_a_stop_event_not_handled_in_the_handler"
        status: pass
    human_judgment: false
  - id: D4
    description: "Pitfall 9: a group member that ignores TERM is SIGKILLed after the leader exits, while the unreaped leader still holds the pgid"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::a_term_ignoring_group_member_is_killed_after_the_leader_exits"
        status: pass
    human_judgment: false
  - id: D5
    description: "No process exit, signal-mask or signal-wait call on a non-comment line of runtime.rs, with a planted-positive control; a parked shutdown caller hears stopped only when the run stopped"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::runtime_source_has_none_of_the_forbidden_signal_or_exit_calls"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::a_parked_shutdown_hears_stopped_only_when_the_run_stopped"
        status: pass
    human_judgment: false
  - id: D6
    description: "macOS: devflow-core and devflow compile for aarch64-apple-darwin (--all-targets), including the pipe+fcntl fallback and waitid"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "cargo check --target aarch64-apple-darwin -p devflow-core --all-targets && ... -p devflow --all-targets -> rc 0, no warnings at b02ab80"
        status: pass
    human_judgment: true
    rationale: "Compile only. No macOS host has run the signal pipe, waitid WNOWAIT or the group kill (backlog 999.160)"

duration: 27min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 07: Supervisor Runtime Summary

**`devflow_core::supervisor::runtime`: a supervisor that serves `ping` and `shutdown` on its socket, turns SIGTERM and SIGINT into stop events through a self-pipe, ends its agent's process group (SIGTERM, then SIGKILL, with leftover members killed while the zombie leader still holds the pgid), and takes every stop/exit decision under one owner mutex, so a stop that races the agent's natural exit never advances the pipeline.**

## Performance

- **Duration:** 27 min
- **Started:** 2026-10-17T17:30:19Z
- **Completed:** 2026-10-17T17:57Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- `StopToken` is the single terminal-state owner (R-2). `request_stop` sets the stop flag in `AgentRunning`, `Advancing` and `GateWait`, and changes nothing in `HandedOff` or `Finished`. `safe_point` returns `StopHere` once a stop is flagged. The exit decision runs under the same lock. Its outcome is decided only by the pure `on_stop_request`, `on_agent_exit` and `at_safe_point`.
- `Supervisor::serve` writes the sidecar, publishes the socket, then starts one control thread. That thread polls the listener and the signal pipe, answers `ping` with the sidecar's identity and the current owner, and turns a `shutdown` or a signal byte into a stop. For `KillAgentGroup` it replies `accepted`, parks the caller, sends SIGTERM to the group, and sends SIGKILL if the main thread has not seen the exit within `TERMINATE_VERIFY_WAIT`.
- `wait_agent` waits with `waitid(WNOWAIT)`. Under the lock it decides the outcome, SIGKILLs any leftover members if the run was stopped, then reaps. A completed run is left in `Advancing` and a stopped one in `Finished`.
- `install_signal_pipe` creates a close-on-exec, non-blocking pipe once per process (`pipe2` on Linux, `pipe` plus `fcntl` elsewhere) and installs an `SA_RESTART` handler that only calls `getpid` and `write`. Nothing blocks a signal.
- Every group signal goes through `signal_group`, which refuses a pgid that does not fit `pid_t`, is ≤ 1, or equals `getpgrp()`.

## Task Commits

1. **Task 1 (tracer): socket shutdown kills the agent group and reports stopped.** RED `8f10989` test(50-07), GREEN `1bab753` feat(50-07)
2. **Task 2: V-14 interleaving proof and decision tables.** `36cd36e` test(50-07), `41d7e7f` test(50-07) (the hang fix, deviation 1)
3. **Task 3: SIGTERM/SIGINT through the self-pipe, and the TERM-ignoring member.** RED `4193d3f` test(50-07), GREEN `34faeff` feat(50-07)
4. **Extra test: the final line a parked caller hears.** `b02ab80` test(50-07) (deviation 3)

**Tracer feedback gate:** this run was interactive (`auto_advance=false`, `_auto_chain_active=false`) with `human_verify_mode` at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. Task 1's gate was re-run at `1bab753`: rc 0, with each test reporting `1 test run: 1 passed`. Expansion followed.

## TDD Gate Compliance

| Task | RED | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------------------------------------------------------------------|-------|
| 1 | `8f10989` | exit 100, 3 run / 2 failed (the control passed, as intended); `RED_EVIDENCE_OK target_test_failed` for the shutdown test and for the source scan | `1bab753` |
| 2 | none (see below) | substitute: 10 hand mutants plus cargo-mutants | n/a (tests over Task 1 code) |
| 3 | `4193d3f` | exit 100, 2 run / 1 failed; `RED_EVIDENCE_OK target_test_failed` for the SIGTERM test | `34faeff` |

**Semantic assessment.**
- Task 1, shutdown test: it failed at its target assertion, `a socket shutdown must report the stop, got Ok(Refused("not_implemented"))`, because the RED control thread answered `shutdown` with a stub error. The fixture worked: the child supervisor started, served ping, and was killed and reaped by the guards afterwards.
- Task 1, source scan: it failed at its planted-positive control, `every planted call is flagged: []`, because `forbidden_calls` was a stub returning nothing.
- Task 3, SIGTERM test: it failed at `control: the mask reader sees the supervisor's own TERM handler` (SigCgt TERM bit 0), because the stub `install_signal_pipe` installed nothing. This is the first assertion that depends on the handler. The test's later "a TERM is a stop" assertions were not reached at RED.
- Task 3, TERM-ignoring member test: it **passed at RED**. Task 1's action already required `wait_agent` to SIGKILL leftover members, so the behaviour existed. Its discrimination is shown by a hand mutant instead (below), and that mutant first survived. See deviation 2.

**Task 2 has no RED commit, by the plan's structure.** Task 1 implements the decision functions, so V-14 and the tables passed on their first run. The discrimination evidence is the hand mutants below.

## Mutation and Discrimination Evidence

**cargo-mutants gate (Task 2, verbatim, `TMPDIR` on disk):** rc 0. The run at `36cd36e` and the run at `34faeff` gave the same result: `mutants_exit=0 caught=1 missed=0 timeouts=0`, plus 6 unviable. No mutant timed out.
- Caught: `runtime.rs:233:21: replace == with != in StopToken::safe_point`.
- Unviable: the six `replace <fn> -> <enum> with Default::default()` mutants, because none of these enums implements `Default`.
- **This gate is thin.** cargo-mutants built only one viable mutant over these functions. "Zero survivors" here does not show the tests pin the decisions. The hand mutants below are the real evidence.

**Hand mutants on the decision functions** (V-14, control and both tables run together; the source was restored byte-identical and checked):

| Mutant | Result |
|---|---|
| `on_stop_request`: AgentRunning → FlagForSafePoint | caught (2 tests fail) |
| `on_stop_request`: GateWait → ReplyHandedOff | caught (2) |
| `on_stop_request`: HandedOff → FlagForSafePoint | caught (1, the table) |
| `on_agent_exit`: always Completed | caught (2) |
| `on_agent_exit`: always Stopped | caught (3). This mutant first **hung** V-14 (deviation 1) |
| `at_safe_point`: always Proceed | caught (3) |
| `request_stop_locked`: never sets the flag | caught (3) |
| `request_stop_locked`: always sets the flag | caught (1, the table) |
| `safe_point`: does not record `ended_stopped` | caught (2) |
| `observe_exit`: decides with `false` instead of the stop flag | caught (2) |

**Hand mutants on Task 3 and the extra test:**

| Mutant | Result |
|---|---|
| `wait_agent` does not SIGKILL leftover members | **survived** at first. Caught after deviation 2: the member outlives the 3 s window |
| `install_once` blocks TERM and INT | caught: `the agent must not inherit a blocked TERM or INT` |
| signal bytes drained but not turned into a stop | caught: the child supervisor did not exit within 30 s |
| handler `getpid` check inverted | caught: the same 30 s timeout |
| `finish` always sends `stopped` | caught: the control case got `Stopped` instead of `HandedOff` |
| `finish` never sends `stopped` | caught: the stopped case got `HandedOff` |

## Verification (commands run at HEAD `b02ab80`, actual exit codes)

| Gate | Result |
|---|---|
| Task 1 `<automated>` (3 named tests), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 2 `<automated>` (V-14, control), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 2 mutants gate, verbatim + disk `TMPDIR` | rc 0 at `34faeff` (see above). Not re-run after `b02ab80`, which only adds a test |
| Task 3 first `<automated>` (2 named tests), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 3 second `<automated>`: core lib + source scan + fmt + clippy workspace | rc 0; `910 tests run: 910 passed (1 leaky)`; the source scan `1 passed` |
| Extra: `cargo nextest run -p devflow --bin devflow --no-tests=fail` | rc 0, `415 tests run: 415 passed (2 leaky)` |
| Extra: `cargo check --target aarch64-apple-darwin` for `devflow-core` and `devflow`, `--all-targets` | rc 0, no warnings (the first Task 3 run warned that `signal_child` was unused there; it is now gated to Linux) |
| Extra: `semver-gate.sh check --through baseline` | rc 0, `SEMVER verdict=match`: the new module is additive |

The leaky tests are `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`, `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` and `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates`. They predate this plan, and none of them is in a file it touches. The CLI run left four orphaned `sleep 30` processes (reparented to `systemd --user`, started during that run), and I killed them by their recorded PIDs. The runtime tests use `sleep 60`, and none of those was left after any run.

**What these checks do not establish:**
- The signal tests and the TERM-ignoring member test are Linux-only (`/proc`). macOS only compiles.
- No test covers a signal that arrives while the CLI owns the run (`Advancing` or `GateWait`) through the pipe. Only the decision tables cover that path.
- No test covers a shutdown answered in `Finished`, or a `Supervisor` dropped without `finish`.
- While the control thread waits for a TERM-ignoring **leader** to exit (up to `TERMINATE_VERIFY_WAIT`, 3 s), it answers no ping. A probe in that window gets `NoReply` after its 2 s timeout. No test covers this.
- The signal handler does not save and restore `errno`, because the plan limits it to `getpid` and `write`. A failed `write` (a full pipe) could change `errno` for the interrupted thread.
- Each test passed the runs listed here, a handful of times in total. That is a weak bound on flakiness, not a reliability figure.

## Files Created/Modified

- `crates/devflow-core/src/supervisor/runtime.rs` (new): the runtime and its 10 tests
- `crates/devflow-core/src/supervisor.rs`: `pub mod runtime;`. `write_sidecar` loses its `expect(dead_code)`. `rfc3339_now`'s reason text now names 50-08
- `crates/devflow-core/src/supervisor/protocol.rs`: the 7 `expect(dead_code)` attributes are removed from `Reply`, `to_json`, `read_request`, `write_reply` and the three `peer_is_same_user` bodies

## Decisions Made

See `key-decisions` in the frontmatter. All are inside the plan's remit (reply details and the order of steps within `finish`). None changes a planned signature.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Test bug] V-14 hung instead of failing when a stage ended before its window**
- **Found during:** Task 2 hand mutation (`on_agent_exit` always Stopped)
- **Issue:** when `run_stage` returned before reaching its window, the stopper thread stayed blocked on the barrier and the test hung on `join`. The mutation pass had to be stopped, by PID, after 10 minutes.
- **Fix:** the pause records that it ran, and the test asserts this before joining.
- **Verification:** the same mutant now fails in 0.02 s
- **Committed in:** `41d7e7f`

**2. [Rule 1 - Test strength] The child's group guard hid the Pitfall 9 defect**
- **Found during:** Task 3 hand mutation (`wait_agent` without the leftover SIGKILL)
- **Issue:** the child supervisor's `GroupGuard` SIGKILLed the agent group as it dropped after `finish`, so the TERM-ignoring member died even when the supervisor never killed it, and the mutant passed.
- **Fix:** the child disarms its guard once `wait_agent` returns. The parent's guard still reaps anything left over.
- **Verification:** the mutant now fails at "the TERM-ignoring member must be gone within the grace window" and leaves no process behind
- **Committed in:** `34faeff`

**3. [Rule 2 - Missing test of a stated contract] The final line a parked caller hears**
- **Issue:** the plan requires that `finish` send `stopped` only when the run stopped, and `handed_off` otherwise, but none of its named tests covered the `handed_off` side or a stop taken at a safe point.
- **Fix:** added `a_parked_shutdown_hears_stopped_only_when_the_run_stopped`, served in-process with a sidecar pgid of 0, which `signal_group` refuses. Both directions are proved by hand mutants.
- **Committed in:** `b02ab80`

**4. [Scope note] Additions beyond the named tests**
- `every_owner_phase_has_exactly_one_stop_decision` and `exit_and_safe_point_decisions_cover_every_input`: the table tests Task 2's action asks for.
- `finish`'s step order and the `Finished` reply. See key-decisions.

**5. [Orchestrator-directed] Tracking and commit guard**
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched. `SUPV-01` is declared by other plans too.
- The executor's `agent-*` branch allow-list does not apply to this sequential executor on `feature/phase-50`. `pin-guard.sh` passed before the first edit and before every commit.
- Commit `36cd36e` has a 73-character subject, and the commit-msg hook warned about it. It was not amended, per the dispatch's no-amend rule.

---

**Total deviations:** 3 auto-fixed (2 test bugs found by mutation, 1 missing contract test), plus 2 scope/process notes
**Impact on plan:** none on the planned public API

## Issues Encountered

- The first hand-mutation pass hung (deviation 1). I identified it by its process tree, stopped the Python driver, `cargo-nextest` and the hung test binary by their recorded PIDs, and restored the source with `git checkout -- crates/devflow-core/src/supervisor/runtime.rs`, the committed file.
- cargo-mutants was given a `TMPDIR` on disk (`~/.cache/exec-50-07-mutants`), because `/tmp` is RAM-backed. The directory was removed after each run. No `/tmp/dfs*` fixture directory was left after any run.

## Known Stubs

None. Every RED stub was replaced in its GREEN commit: `rg 'RED stub'` finds 0 at HEAD and 3 at `8f10989`.

## User Setup Required

None.

## Next Phase Readiness

- 50-08 can run both monitor arms on `Supervisor`: spawn the agent with `process_group(0)`, call `install_signal_pipe` first, build the `Sidecar` (its first non-test use of `rfc3339_now`, whose `expect(dead_code)` it must then drop), then `serve`, `wait_agent`, and advance only on `AgentOutcome::Completed`.
- 50-11 and 50-13 get `StopToken::safe_point` for the three advance safe points, and `enter(OwnerPhase::GateWait)` plus `safe_point` for the cancellable gate poll.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Files: `crates/devflow-core/src/supervisor/runtime.rs` and this SUMMARY are present.
- Commits 8f10989, 1bab753, 36cd36e, 41d7e7f, 4193d3f, 34faeff and b02ab80 are ancestors of HEAD. Each RED `test(50-07)` commit precedes its GREEN `feat(50-07)` commit (8f10989 before 1bab753, 4193d3f before 34faeff).

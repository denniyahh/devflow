---
phase: 50-addressable-monitor-liveness
plan: 08
subsystem: supervisor
tags: [rust, unix-socket, process-group, signals, stream-json, monitor, semver]

requires:
  - phase: 50-07
    provides: "supervisor::runtime: install_signal_pipe, Supervisor (serve, wait_agent, token, hand_off, finish), AgentOutcome, StopToken"
  - phase: 50-06
    provides: "Sidecar, write_sidecar (via serve), sweep_for_in, rfc3339_now"
  - phase: 50-05
    provides: "SocketPath (for_launch, sidecar_path), probe, Liveness, test_support::spawn_test_in_child"
  - phase: 50-01
    provides: "tools/semver-gate.sh check --through 50-08"
provides:
  - "devflow_core::monitor: StageIo {Capture, Stream {prompt, idle_timeout}} (+ label), StageSpec (+ new), StageRun {outcome, supervisor}, run_supervised_stage"
  - "MonitorError is #[non_exhaustive] with SocketPath, Publish, PgidMismatch {pid, pgid}, NotReady (named R-9 break, stage 50-08)"
  - "private drive_stream_io + StreamThreads shared by run_pipe_owning_monitor and the stream shape; spawn_stage_agent, confirm_own_group, abandon_agent, stage_sidecar"
  - "supervisor::runtime::STOPPED_EXIT_CODE is pub(crate); supervisor::rfc3339_now lost its expect(dead_code)"
affects: [50-10, 50-11, 50-13, 50-23]

actuals:
  tokens: 16375
  tasks: 3
  commits: 7
plan_head_before: 9a290d2f20c733cae9f1e11d130ed06847e57479
plan_head_after: 580ca1c5650ccea72abf0f122e0da77fa9748e2f

tech-stack:
  added: []
  patterns:
    - "One supervisor body, two I/O shapes: spawn (process_group(0)) -> confirm getpgid == pid -> pid file -> Supervisor::serve -> [stream: drive_stream_io] -> wait_agent -> exit file (143 on Stopped)"
    - "Supervised-stage process tests: the parent re-runs the test as a child supervisor (spawn_test_in_child), passes the spec by DEVFLOW_TEST_STAGE_* env, probes the socket, and signals only the Child it owns; StageChild and StageGroup guards are bound before any assertion"
    - "Responsibility table: one checker fn per spike row returning Result<(), String>, collected and asserted together, so a control can apply one checker to a known-bad run"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/supervisor/runtime.rs
    - crates/devflow-core/src/supervisor.rs

key-decisions:
  - "StageSpec::new takes 7 arguments (project_root, phase, agent, program, io, socket, launch_id) and defaults workdir to project_root, stage to None and args/envs to empty; callers set the pub fields. Taking all 11 would need a too_many_arguments allow on a new public fn"
  - "The stream shape's pipe threads, close rule and idle-timeout loop moved into one private drive_stream_io(child, capture, agent, prompt, idle_timeout, on_idle_timeout) rather than three helpers; run_pipe_owning_monitor passes fire_idle_timeout as the callback and keeps its exact order (pid file, drive, release stdin, child.wait, exit file, join)"
  - "A pre-supervisor failure (pgid mismatch, pid-file write, serve, a stream pipe missing, wait_agent error) SIGKILLs the agent's group and reaps it (abandon_agent), using the in-memory Child's pid, never a pid from disk"
  - "The V-08 SigBlk row checks the agent leader for no TERM/INT bit and a child the agent runs for an empty mask, instead of an empty leader mask (deviation 3: bash blocks SIGCHLD itself while it waits)"
  - "MonitorError::NotReady is no longer constructed after Task 2. It stays because the plan's R-9 artifact list names it"

patterns-established:
  - "A test that needs the mask an agent was handed reads a child of the agent, because a shell agent changes its own SigBlk"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "The capture shape (stdin null, stdout and stderr to separate files) runs a fake Codex agent under an addressed supervisor: Alive with launch=capture while it runs, sidecar readable at the first Alive, Completed(7) advancing, pid and exit files correct, socket and sidecar gone after finish"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::capture_arm_runs_the_agent_under_an_addressed_supervisor"
        status: pass
    human_judgment: false
  - id: D2
    description: "The stream shape runs a fake Antigravity agent on the same body: first turn delivered on stdin, result line captured verbatim, stderr separate, Completed(0) advancing, socket gone after finish"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::stream_arm_runs_the_agent_under_an_addressed_supervisor"
        status: pass
    human_judgment: false
  - id: D3
    description: "V-10 (R-3): in both shapes the agent's pgid equals its pid and differs from the supervisor's group; control: the same check fails for a process left in the launcher's group"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::both_arms_put_the_agent_in_its_own_group"
        status: pass
    human_judgment: false
  - id: D4
    description: "V-09: shape x {running, supervisor SIGKILLed, finished} probes Alive(with that shape's label), Stale, Gone; control: an Alive reply fails the other shape's label"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::both_arms_answer_liveness_identically_for_each_condition"
        status: pass
    human_judgment: false
  - id: D5
    description: "V-16 (D-05, 999.134): SIGTERM and SIGINT to the supervisor in either shape end the agent group, write 143, report Stopped (no advance) and leave the socket Gone; control: SIGTERM to the agent itself is Completed(143)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance"
        status: pass
    human_judgment: false
  - id: D6
    description: "V-08 (R-11): R-A, R-B, R-C, R-D (exit 7 and SIGUSR1 -> 138), R-E, R-F (GIT_CONFIG_COUNT reaches a grandchild), R-J, R-K, R-L, R-M and SigBlk under fake codex (capture) and fake antigravity (stream), 26 rows; control: the R-B checker rejects a capture whose agent merged stderr into stdout"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter"
        status: pass
    human_judgment: false
  - id: D7
    description: "run_pipe_owning_monitor's behaviour is unchanged by the shared helper: every pre-existing monitor test still passes"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail -> 916 tests run: 916 passed (1 leaky, pre-existing)"
        status: pass
    human_judgment: false
  - id: D8
    description: "R-G (the supervisor survives the spawning CLI) is not proved here; 50-11 proves it when spawn_monitor gives the supervisor its own group"
    requirement: SUPV-01
    verification: []
    human_judgment: true
    rationale: "Deferred by the plan to 50-11; no 50-08 test exercises a supervisor detached from a CLI"

duration: 21min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 08: Both Supervision Shapes on the Runtime Summary

**`monitor::run_supervised_stage` runs the capture shape (the Legacy `sh` script's Rust replacement) and the stream shape (PipeOwning with an address) on one supervisor body. Each puts the agent in its own process group and checks that before recording anything. Each serves ping and shutdown on a per-launch socket, writes 143 and reports `Stopped` when SIGTERM or SIGINT reaches the supervisor, and meets every R-A…R-M responsibility under a non-Claude adapter.**

## Performance

- **Duration:** 21 min (19 min to the last task commit)
- **Started:** 2026-10-17T18:06:00Z
- **Completed:** 2026-10-17T18:27Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- `run_supervised_stage(spec)` installs the signal pipe and spawns the agent through `hermetic_command`, with the gate notify command stripped, the adapter env applied after the scrub, and `process_group(0)`. It then requires `getpgid(agent) == agent`, re-reading `/proc/<pid>/stat` field 5 for up to 100 ms. A mismatch SIGKILLs and reaps the agent and returns `PgidMismatch`. After that it writes the pid file, serves (the sidecar is written before the socket name appears), drives the stream pipes (stream shape only), waits through `wait_agent`, and writes the exit file (`code`, or `143` on `Stopped`).
- Capture shape: stdin is `/dev/null`, and stdout and stderr go to truncated files that are kept separate. Stream shape: both pipes are owned through the same `drive_stream_io` that `run_pipe_owning_monitor` now calls. An idle-timeout kill stays a natural `Completed(128 + n)`.
- `MonitorError` is `#[non_exhaustive]` and adds `SocketPath`, `Publish`, `PgidMismatch { pid, pgid }` and `NotReady`. The semver gate matches `--through 50-08`.
- Six child-process tests cover V-08, V-09, V-10, V-16 and both tracers, each with a control. Hand mutants show each one discriminates (see below).

## Task Commits

1. **Task 1 (tracer): capture shape.** RED `b1e5642` test(50-08), GREEN `8501dc7` feat(50-08)
2. **Task 2: stream shape, V-10, V-09.** `ca9435d` refactor(50-08) (shared helper, behaviour-neutral, stream still `NotReady`), RED `dc5d495` test(50-08), GREEN `266371c` feat(50-08)
3. **Task 3: V-16 and V-08.** `e7a1e94` test(50-08), plus `580ca1c` test(50-08) (macOS cfg fix, deviation 4)

**Tracer feedback gate:** the run was interactive (`auto_advance=false`, `_auto_chain_active=false`) with `human_verify_mode` at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. Both Task 1 gates were re-run on the GREEN tree before `8501dc7`: the named test reported `1 test run: 1 passed` (rc 0), and the semver gate reported `verdict=match` (rc 0). Expansion followed.

## TDD Gate Compliance

| Task | RED | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------------------------------------------------------------------|-------|
| 1 | `b1e5642` | exit 100, 1 run / 1 failed; `RED_EVIDENCE_OK target_test_failed` | `8501dc7` |
| 2 | `dc5d495` | each of the three: exit 100, 1 run / 1 failed; `RED_EVIDENCE_OK target_test_failed` | `266371c` |
| 3 | none (see below) | substitute: six hand mutants | n/a (tests over Task 1-2 code) |

**Semantic assessment.**
- Task 1: the child test process ran its one test and panicked at `supervise the stage: NotReady("capture shape: RED stub")`. The parent then failed at `the child supervisor never answered a ping (exited: true)`. No compile, fixture or load fault: the child spawned, read its env and reached `run_supervised_stage`.
- Task 2: all three tests failed the same way on the stream shape (`NotReady("stream shape lands in task 2")`). In V-10 and V-09 the capture rows ran first and passed their assertions. The failure is the first stream row.
- Task 3 has no RED commit. Task 1's action already writes 143 on `Stopped` and installs the signal pipe, and Tasks 1-2 already pass the adapter env, cwd and pid file. So V-16 and V-08 passed on their first run, the "feature already exists" case of the fail-fast rule. The hand mutants below are the discrimination evidence.

## Mutation and Discrimination Evidence

Each mutant was applied to `monitor.rs`, the named test was run, and the file was restored byte for byte (checked with `cmp`):

| Mutant | Test | Result |
|---|---|---|
| capture stderr appended to the capture file | capture tracer | caught |
| no `process_group(0)` and `confirm_own_group` always Ok | V-10 | caught: `Capture: pgid == pid` |
| stream label reports `capture` | V-09 | caught: the label control fails |
| `Stopped` writes 0 instead of 143 | V-16; V-08 | caught: `Capture SIGTERM` exit file; V-08 rows `Capture R-K`, `Stream R-K` only |
| no `install_signal_pipe` | V-16; V-08 | caught: the child supervisor died of signal 15 (`child test process must exit 0`) |
| adapter env not applied | V-08 | caught: rows `R-F` (both shapes) only |
| agent cwd is the project root | V-08 | caught: rows `R-A` (both shapes) only |
| agent handed a blocked SIGTERM (`pre_exec` + `pthread_sigmask`) | V-08 | caught: rows `SigBlk` (both), `leader 0x14000, inherited 0x4000` |
| pid file names the supervisor | V-08 | caught: rows `R-C` (both shapes) only |

Negative control for the semver gate: the saved `--through 50-08` log, compared `--through baseline`, prints `SEMVER verdict=mismatch unexpected=enum_marked_non_exhaustive:enum MonitorError missing=-` and exits 1.

## Semver Gate (Task 1, before `b1e5642` and again before `8501dc7`)

```
SEMVER lint=constructible_struct_adds_field item=field AgentResult.checkpoint
SEMVER lint=enum_marked_non_exhaustive item=enum MonitorError
SEMVER lint=enum_marked_non_exhaustive item=enum RecoverError
SEMVER lint=enum_marked_non_exhaustive item=enum WorkflowError
SEMVER verdict=match
semver_gate_exit=0 verdict=match
```

semver-checks reported no `enum_variant_added` item: once an enum is marked `#[non_exhaustive]`, it reports only that lint. The gate table lists that row as `allowed`, not `required`, so the match holds.

## Verification (commands run at HEAD, actual exit codes)

| Gate | Result |
|---|---|
| Task 1 `<automated>` 1 (capture tracer), verbatim | rc 0, `1 test run: 1 passed` |
| Task 1 `<automated>` 2 (semver gate) | rc 0, `verdict=match`. Not verbatim: see deviation 1 |
| Task 2 `<automated>` 1 (3 named tests), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 2 `<automated>` 2 (core lib suite), verbatim | rc 0, `914 tests run: 914 passed (1 leaky)` at `266371c` |
| Task 3 `<automated>` 1 (V-16, V-08), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 3 `<automated>` 2 (core lib + CLI bin + fmt + clippy workspace), verbatim | rc 0; core `916 passed (1 leaky)`, CLI `415 passed (2 leaky)` at `e7a1e94` |
| Extra: `cargo check --target aarch64-apple-darwin --all-targets` for `devflow-core` and `devflow` | rc 0, no warnings at `580ca1c` (3 warnings at `e7a1e94`; see deviation 4) |
| Extra: the six new tests together, 10 consecutive runs | 10/10 runs, `6 tests run: 6 passed` each |
| Acceptance: `rg "stream shape lands in task 2" crates/` | no match (rc 1); positive control: the same search on `dc5d495` finds 1 |

The leaky tests are `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper` (core), and `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` and `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` (CLI). They were leaky before this plan, and none is in code it touches. No `sleep 30` from them was still running when I checked after the runs.

**What these checks do not establish:**
- V-08 is Linux-only (it reads `/proc`). On macOS the shapes are only compiled, and V-16, V-09 and V-10 are compiled but have never run there (backlog 999.160).
- No test sends the signal before `serve` publishes the socket (Window A/F of the old script). The signal pipe is installed before the spawn, and the byte waits in the pipe until the control thread drains it. That ordering is true by construction, but untested. See the mapping below.
- No test exercises the stream shape's idle timeout through `run_supervised_stage`. It shares `drive_stream_io` with `run_pipe_owning_monitor`, whose idle-timeout tests still pass, but the `Completed(128 + n)` mapping under a supervisor is shown only by reading the code.
- Supervisor-level stops are tested only while the agent runs (`AgentRunning`), not during `Advancing` or `GateWait`. 50-07's decision tables cover those states.
- Ten clean runs are a weak bound on flakiness, not a reliability figure.

## Legacy-Script Tests 50-11 Deletes, and What Re-proves Each

| Deleted (D-03, 50-PATTERNS) | Re-proved by | Note |
|---|---|---|
| `sigterm_to_monitor_also_kills_the_agent` (template) | V-16 `sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance` | TERM and INT, both shapes, whole group gone, 143, `Stopped` |
| `sigterm_after_a_held_agent_child_execs_kills_the_agent` | V-16 | TERM after the agent is running |
| `sigterm_before_the_agent_child_resets_its_traps_still_kills_the_agent` (Window A) | V-16 for the outcome only | The script's trap-reset window no longer exists, because the Rust supervisor has no shell trap. The signal pipe is installed before the spawn, so a TERM landing before `serve` waits as a byte. No 50-08 test times a signal into that window; 50-11 should add one if it wants the window itself proved |
| `sigterm_before_the_monitor_records_the_agent_pid_still_kills_the_agent` (Window F) | V-16 for the outcome only | Same as Window A: true by construction, not timing-tested |
| `a_stale_stop_marker_does_not_stop_the_next_agent` | none needed | The stop marker file is gone. A stop is scoped to one launch's `StopToken` and socket |
| `a_term_after_the_agent_exits_leaves_no_stop_marker` | 50-07 V-14 and decision tables (a stop after the exit decision is `FlagForSafePoint`, never a kill) | Not re-proved by a 50-08 test |
| `spawn_monitor_captures_agent_pid_and_output` | capture tracer; V-08 R-B, R-C | |
| `legacy_advance_tail_passes_the_launched_stage` | **not re-proved** | `run_supervised_stage` does not advance; the caller (50-11) owns that, and its launch-path tests must carry the stage |
| `shell_escape_wraps_basic_strings`, `shell_escape_handles_single_quotes`, `shell_escape_handles_empty_string` | none needed | No shell remains: argv and paths are passed to `Command` directly |

## Files Created/Modified

- `crates/devflow-core/src/monitor.rs`: `StageIo`, `StageSpec`, `StageRun`, `run_supervised_stage` and its private helpers, `drive_stream_io` (moved out of `run_pipe_owning_monitor`), the `MonitorError` variants, `signal_mask` (from `sigterm_in_mask`), and the six tests with their fixtures
- `crates/devflow-core/src/supervisor/runtime.rs`: `STOPPED_EXIT_CODE` is `pub(crate)`
- `crates/devflow-core/src/supervisor.rs`: `rfc3339_now` loses its `expect(dead_code)`. `stage_sidecar` is its first non-test caller, which is what 50-07 handed forward

## Decisions Made

See `key-decisions` in the frontmatter. Each is inside the plan's remit: a constructor's arity, how the shared helpers are split, cleanup on a failed start, and how the SigBlk row is measured.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The semver gate could not run verbatim**
- **Found during:** Task 1, before the RED commit
- **Issue:** the harness's safety check refused the plan's `bash -c '…; trap "rm -rf \"$L\"" EXIT; …'`, because the script runs `rm -rf` on a variable.
- **Fix:** a scratch script runs the same `semver-gate.sh check --through 50-08 --log <f>` with the same `^SEMVER` print and the same `exit 0 && verdict=match` condition. The only change is fixed scratch files instead of `mktemp` and the `rm` trap. The gate script was not edited.
- **Verification:** rc 0, `verdict=match`, both before `b1e5642` and before `8501dc7`; the negative control above fails as it should.

**2. [Rule 1 - Test structure] Task 2 was split into refactor, RED and GREEN commits**
- **Issue:** the shared-helper refactor and the stream shape were written together. Committing them together would have left no RED for Task 2.
- **Fix:** the refactor was committed with the stream shape still returning `NotReady` (`ca9435d`; all 49 monitor tests passed). Then came the failing tests (`dc5d495`), then the guard's removal (`266371c`).

**3. [Rule 1 - Unachievable assertion] "The agent's SigBlk is empty" is false for any shell agent**
- **Found during:** Task 3, measured before writing the row
- **Issue:** bash (`/bin/sh` here) blocks SIGCHLD (`0x10000`) in its own mask while it waits on a child: 200 of 200 samples were non-zero. So an empty-mask assertion on the leader fails for every correct supervisor.
- **Fix:** the row checks that the leader has no TERM or INT bit, and that a child the agent runs (which bash starts with the mask the agent inherited) has an empty mask. Measured control: when the launcher blocks TERM, the leader shows `0x14000` and its child `0x4000`. The `block_term` mutant reproduces that and is caught.

**4. [Rule 1 - Warning] V-08 constants unused on macOS**
- **Found during:** the extra macOS compile check after `e7a1e94`
- **Issue:** `RESPONSIBILITIES_TEST`, `R_F_VARIABLE` and `R_F_VALUE` were not cfg-gated with V-08, so `aarch64-apple-darwin` warned three times.
- **Fix:** gated them to Linux in `580ca1c`. The macOS check is now clean for both crates.

**5. [Scope note] `supervisor.rs` is outside the plan's `files_modified`**
- The `rfc3339_now` attribute removal was required by 50-07's hand-forward. Leaving it would have failed clippy, because the `expect` would go unfulfilled.

**6. [Orchestrator-directed] Tracking and commit guard**
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched. `pin-guard.sh` passed before the first edit and before every commit. The `agent-*` branch allow-list does not apply to this sequential executor on `feature/phase-50`.
- The first attempt at the RED commit was refused by the pre-commit `cargo fmt --check`. I ran `cargo fmt` and made the commit anew; nothing was amended.

---

**Total deviations:** 4 auto-fixed (1 blocking, 3 test-structure or correctness), plus 2 scope/process notes
**Impact on plan:** none on the planned public API

## Issues Encountered

- **Two agents leaked under the `no_group` mutant**, and I killed them by their recorded PIDs, after matching `CARGO_MANIFEST_DIR` and the fake agent's cmdline. Under that mutant the agent stays in the child test process's group. The parent's `StageGroup` guard kills the group named in the ping reply (the agent's own pid), which does not exist then, so the agent outlived the test. This needs two defects at once: the production `confirm_own_group` check must be disabled too, or it kills the agent itself. It is a limit of the test guard, not of production code. Nothing leaked from any unmutated run.
- The nextest JUnit target name must be the qualified `monitor::tests::<name>`. With the bare name, `tdd-red-evidence` returned `INVALID_RED no_target_test_failure` on a genuine failure. The re-run with the qualified name returned `RED_EVIDENCE_OK`.

## Known Stubs

None. `MonitorError::NotReady` is no longer constructed after Task 2, but it is a named variant of the R-9 break, not a stub.

## User Setup Required

None.

## Next Phase Readiness

- 50-11 can switch the launch path: `spawn_monitor` re-execs `__monitor`, which builds a `StageSpec` and calls `run_supervised_stage`. It then advances only on `Completed`, and calls `hand_off` or `finish`. Its own tests must cover R-G (detached from the CLI) and the advance tail, which this plan does not re-prove (see the mapping).
- The stream shape now carries adapter env explicitly in `StageSpec.envs`, so the `pipe-owning monitor: … adapter env var(s) will not survive` warning in `spawn_monitor_inner` becomes obsolete once 50-11 routes through it.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Files: `crates/devflow-core/src/monitor.rs`, `crates/devflow-core/src/supervisor/runtime.rs`, `crates/devflow-core/src/supervisor.rs` and this SUMMARY exist.
- Commits `b1e5642`, `8501dc7`, `ca9435d`, `dc5d495`, `266371c`, `e7a1e94` and `580ca1c` are ancestors of HEAD. Each RED `test(50-08)` precedes its GREEN `feat(50-08)`: `b1e5642` before `8501dc7`, `dc5d495` before `266371c`.

---
phase: 50-addressable-monitor-liveness
plan: 11
subsystem: infra
tags: [rust, supervisor, unix-socket, process-group, monitor, e2e, semver]

requires:
  - phase: 50-24
    provides: wait_for_publish, PUBLISH_WAIT and the pre-publish teardown (with 18 dead-code expectations to remove)
  - phase: 50-10
    provides: LaunchContext, MonitorExec::HarnessEntry, monitor_entry_full / monitor_entry_supervise_only
  - phase: 50-08
    provides: run_supervised_stage, StageSpec, StageIo, and the Legacy-test deletion mapping
  - phase: 50-03
    provides: per-launch SocketPath under the cache dir, probe, request_shutdown, sidecar
  - phase: 50-21
    provides: hermetic test children (HermeticHome, only_common_names_the_devflow_binary)
provides:
  - "spawn_monitor re-execs __monitor for both launch shapes and returns Published after wait_for_publish"
  - "__monitor --launch capture|stream, --socket, --launch-id, --keep-env; run_monitor advances only on Completed and records state.stopped on a supervisor stop"
  - "state.supervisor saved in the same save as monitor_pid; stage_launched carries socket_path, launch_id, launch"
  - "the sh monitor script, stop-marker code, MonitorTail, advance_tail, shell_escape, pipe_owning_monitor_args and run_pipe_owning_monitor deleted"
  - "test reapers end runs through the socket, with an identity-confirmed agent-group fallback"
  - "crates/devflow-cli/tests/supervisor_e2e.rs: tracer, CLI SIGTERM, re-homed capture-result tests"
affects: [50-12, 50-13, 50-14, 50-15, 50-16, 50-22, 50-23]

actuals:
  tokens: 42600
  tasks: 3
  commits: 4
plan_head_before: d8eee12c31bf35602d80404b66fbecb00a3e61dc
plan_head_after: b921aced0192051ce4c5998c3a610c0d283f7871

tech-stack:
  added: [libc 0.2 as a devflow dev-dependency (already a devflow-core dependency; no new crate downloaded)]
  patterns:
    - "Launch = re-exec __monitor in its own process group + wait_for_publish; the persisted handle is the ping-confirmed SupervisorRecord"
    - "Test reapers: socket shutdown first, then SIGKILL of the recorded agent group only after agent::is_same_process confirms its leader"

key-files:
  created:
    - crates/devflow-cli/tests/supervisor_e2e.rs
  modified:
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/supervisor/runtime.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/tests/common/mod.rs
    - crates/devflow-cli/tests/auto_chain_flag_e2e.rs
    - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
    - crates/devflow-core/tests/devflow_dir_gitignore.rs
    - crates/devflow-core/tests/monitor_e2e.rs
    - crates/devflow-cli/Cargo.toml
    - Cargo.lock

key-decisions:
  - "The AutoChainGuard is dropped right after the agent's run, BEFORE the advance: the advance now waits for the next stage's supervisor to publish, and that supervisor has already set the flag for its own agent, so clearing after the advance would clear the next stage's value"
  - "The supervise-only harness path waits (bounded, 15 s) until its launcher has recorded this launch id in state before finishing, so a fast-exiting test agent cannot finish before the launcher's publish wait sees it; production gets the same ordering from advance taking the per-phase lock the launcher holds"
  - "The stream-shape pipe tests that called run_pipe_owning_monitor now call a test-module helper built from the production pieces (spawn_stage_agent, drive_stream_io, runtime::exit_code); runtime::exit_code became pub(crate) so the 128+signal test still exercises production code"
  - "libc added as a devflow dev-dependency so the test reapers can signal a process group; no devflow-core public API offers a group signal"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "devflow start publishes a live supervisor socket under the cache dir for both launch shapes; probe ALIVE with the shape's label; shutdown stops the run and leaves the socket Gone"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#start_publishes_a_live_supervisor_socket_for_each_launch_shape"
        status: pass
    human_judgment: false
  - id: D2
    description: "spawn_monitor starts both processes in their own groups and waits for the publish (permanent source scan, planted control)"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::spawn_monitor_groups_both_processes_and_waits_for_publish"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_supervisor_that_never_publishes_is_ended_with_its_agent_group"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_supervisor_that_fails_to_publish_ends_its_agent_group"
        status: pass
    human_judgment: false
  - id: D3
    description: "V-07: the persisted socket path follows the launch context's cache dir, never the project tree; monitor_entry_full is reached and its advance is passed the launched stage"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state"
        status: pass
    human_judgment: false
  - id: D4
    description: "D-05 at the CLI level: SIGTERM to either shape's supervisor ends the run without advancing; control: a finishing agent advances to Plan"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#sigterm_to_a_live_supervisor_ends_the_run_without_advancing"
        status: pass
    human_judgment: false
  - id: D5
    description: "Capture-result check re-homed onto the real binary; devflow_dir_gitignore case 3 proves the .gitignore through NotReady"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#capture_supervisor_owns_a_fake_agent_and_records_devflow_result"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/tests/devflow_dir_gitignore.rs#all_seven_devflow_constructors_produce_the_gitignore"
        status: pass
    human_judgment: false
  - id: D6
    description: "Reapers end runs through the socket; direct __monitor callers use the 3.x argv"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/auto_chain_flag_e2e.rs (3 tests) and auto_chain_leak_repair_e2e.rs (2 tests)"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/test_support.rs#test_support::tests::reap_guard_reaps_the_monitor_when_a_later_assertion_panics"
        status: pass
    human_judgment: false
  - id: D7
    description: "No feature flag back to the shell monitor (D-03), and a launcher never unlinks a socket that answers ping"
    verification: []
    human_judgment: true
    rationale: "The plan marks both prohibitions verification: judgment; the absence gate covers the deleted code, not the absence of a toggle elsewhere"

duration: 35min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 11: Both launch shapes under the addressed Rust supervisor

**`devflow start` now launches every stage, capture and stream alike, as a re-exec'd `__monitor` supervisor in its own process group that publishes a socket under the cache dir before `start` returns; the `sh` monitor script and `run_pipe_owning_monitor` are deleted.**

## Needs awareness

- **The `/proc` stray census is now blind to 3.x processes (leak detection superseded).** `agent::discover_stray_devflow_processes` matches only two process shapes: the `sh -c` wrapper carrying `MONITOR_WRAPPER_MARKER`, and its `devflow advance` child. This plan deleted both. Doctor's stray finding and `gate sweep --reap-strays` can no longer see any process a 3.x binary starts. Their tests still pass, because they plant the 2.x shapes themselves. The census code is unchanged on purpose: no `__monitor` matcher was added, because once 50-22 drops `monitor_pid` from `registry_reachable_pids`, a live supervisor would be in no safety set. 50-12 documents the census as 2.x-only, pins it with a test, and moves its deleted-root role to the socket sweep.
- **A supervisor in its advance tail does not act on SIGTERM.** I saw this while running a mutant (M1 below). An advance that blocked in a wait kept its `__monitor` alive after SIGTERM, because the signal pipe catches TERM and only 50-13's safe points act on it. That `__monitor` needed a SIGKILL by pid. This is expected until 50-13 lands, but it has a consequence now: an e2e test whose chain reaches a gate can leave a `__monitor` waiting after its reaper runs. The reaper's shutdown is accepted but gets no final line before its 15 s bound. The full `scripts/check.sh all` left no such process (checked by pid afterwards).
- **Commit `a03b572` has one red test that `b921ace` fixes.** At `a03b572`, `devflow-core/tests/monitor_e2e.rs::monitor_owns_fake_agent_and_records_devflow_result` fails with `NotReady`. The plan schedules its deletion and re-homing in Task 3, so the tree is green again from `b921ace`. A bisect that lands on `a03b572` will see that failure.
- **`start`'s live-run refusal text still describes the `sh` monitor.** The golden text in `start_lock_e2e.rs` says "an `sh` monitor may be running `devflow advance`". It is pinned byte-for-byte by a golden test and is outside this plan's files, so it is unchanged.

## Performance

- **Duration:** about 35 min
- **Started:** 2026-10-17T19:56:42Z
- **Completed:** 2026-10-17T20:31:52Z
- **Tasks:** 3
- **Files modified:** 13 (1 created)

## Accomplishments

- Both `MonitorLaunch` arms run `__monitor`, which runs `run_supervised_stage`. `spawn_monitor` returns `Published { record, supervisor_pid }` only after 50-24's `wait_for_publish` sees this launch id. All 18 `expect(dead_code)` attributes 50-24 added are gone; an unfulfilled one would fail clippy.
- `run_monitor` advances in-process only on `AgentOutcome::Completed`. On a supervisor stop it records `state.stopped` with "stopped through the supervisor (socket shutdown or SIGTERM/SIGINT)" under the per-phase lock, then calls `finish()`.
- Adapter env values travel only on the `__monitor` environment; the argv carries only `--keep-env NAME`. `monitor_args_carry_the_launch_and_only_adapter_env_names` pins this.
- R-G (the supervisor outlives the CLI) is shown end to end: after `devflow start` exits, its recorded socket answers ALIVE, for both shapes.
- `monitor_entry_full` is used for the first time, and the test shows it is reached. Negative control: the supervise-only entry records no advance event.

## Task Commits

1. **Task 1 (tracer, TDD):** RED `e69257f` `test(50-11): add failing tests for the published supervisor launch`; GREEN `a03b572` `feat(50-11): launch both shapes under the published Rust supervisor`
2. **Task 2:** `39c2af8` `test(50-11): end launched runs through the supervisor socket`
3. **Task 3 (TDD):** `b921ace` `test(50-11): prove the cache-dir socket, CLI SIGTERM stop and capture result`. The tests passed on their first run; see TDD Gate Compliance.

## TDD Gate Compliance

| Test | RED commit | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------|-------|
| `monitor::tests::spawn_monitor_groups_both_processes_and_waits_for_publish` | `e69257f` | exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `a03b572` |
| `start_publishes_a_live_supervisor_socket_for_each_launch_shape` | `e69257f` | exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `a03b572` |
| Task 3's three tests | none | **passed on first run**: the behaviour was built by Task 1's GREEN | `a03b572` (Task 1) |

**Semantic assessment.**
- **Scan test, RED.** The planted control passed. The test then failed on its planned assertion: `non-test code must define wait_for_publish( and call it from spawn_monitor; found 1`.
- **Tracer, RED.** `start` succeeded and the stream canary was `Confirmed`. The test then failed on `Stream: start must record state.supervisor` with `supervisor: None`, the pre-plan behaviour. There was no compile, fixture or load fault.
- **Task 3, "feature already exists".** Task 1, the tracer, built the stop recording, the advance-only-on-Completed rule and the socket placement. These tests verify that work but could not fail first. Mutants M1–M6 below are their discrimination evidence. No `feat` commit follows Task 3's `test` commit.

## Mutation and Discrimination Evidence

For each mutant: apply it, run the named test, then restore the file from a saved copy and confirm the restore with `cmp`. All restores matched.

| Mutant | Test | Result |
|---|---|---|
| M1: a supervisor stop advances instead of recording | sigterm e2e | caught: `Stream: the supervisor exits after SIGTERM` (its advance blocked) |
| M2: `Completed` does not advance | sigterm e2e (control half) | caught: `Stream control: a completed agent must advance and launch the next stage` |
| M3: `record_supervisor_stop` does not set `stopped` | sigterm e2e | caught: `Stream: state.stopped after the signal` |
| M4: the capture shape sends stdout to `/dev/null` | capture-result e2e | caught: `captured stdout missing marker: ""` |
| M5: the socket is placed under `<project>/.devflow` instead of the context cache dir | V-07 | caught: `…/.devflow/sock/… must be under /tmp/dfu…/sock` |
| M6: `monitor_entry_full` runs without the advance | V-07 (full child) | caught: `monitor_entry_full must run the advance: []` |
| Reaper: group SIGKILL fallback removed | `reap_guard_reaps_the_monitor_when_a_later_assertion_panics` | caught: `the guard did not reap the monitor during the unwind` |
| Absence gate on the pre-plan tree | (shell gate) | exit 1 at `d8eee12`, exit 0 at HEAD |
| `monitor_pid` absence gate on the pre-plan tree | (shell gate) | exit 1 at `d8eee12`, exit 0 at HEAD |

## Semver Gate (before `a03b572`, again at HEAD `b921ace`)

```
SEMVER lint=constructible_struct_adds_field item=field AgentResult.checkpoint
SEMVER lint=enum_marked_non_exhaustive item=enum MonitorError
SEMVER lint=enum_marked_non_exhaustive item=enum RecoverError
SEMVER lint=enum_marked_non_exhaustive item=enum WorkflowError
SEMVER lint=function_missing item=function devflow_core::monitor::run_pipe_owning_monitor
SEMVER lint=function_parameter_count_changed item=devflow_core::monitor::spawn_monitor now takes 6 parameters instead of 5,
SEMVER verdict=match
```

**`spawn_monitor`'s return-type change (`Result<u32, MonitorError>` → `Result<Published, MonitorError>`) is a named R-9 break that the semver gate did NOT check.** cargo-semver-checks 0.51.0 has no lint for a change between two non-unit return types. The change is covered by the compilation of every caller this plan moved: `pipeline_launch.rs`, the in-crate harness-entry test, `devflow_dir_gitignore.rs` and `monitor_e2e.rs`. It is listed for the 3.0.0 major bump (R-9, 50-16) (RV-11). `pub struct Published` and the removal of `run_pipe_owning_monitor` are the other public changes. The semver gate tool was not edited.

## Deleted and Moved Tests (T-50-29, against 50-08's mapping)

| Test | Now | Re-proved by |
|---|---|---|
| `spawn_monitor_captures_agent_pid_and_output` | deleted | capture tracer `capture_arm_runs_the_agent_under_an_addressed_supervisor`; V-08 R-B/R-C; new e2e `start_publishes_…` and `capture_supervisor_owns_…` |
| `sigterm_to_monitor_also_kills_the_agent` | deleted | V-16 `sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance`; new e2e `sigterm_to_a_live_supervisor_ends_the_run_without_advancing` |
| `sigterm_after_a_held_agent_child_execs_kills_the_agent` | deleted | V-16 |
| `sigterm_before_the_agent_child_resets_its_traps_still_kills_the_agent` (Window A) | deleted | V-16 for the outcome only; the shell-trap window no longer exists |
| `sigterm_before_the_monitor_records_the_agent_pid_still_kills_the_agent` (Window F) | deleted | V-16 for the outcome only |
| `a_stale_stop_marker_does_not_stop_the_next_agent` | deleted | none needed (no stop marker) |
| `a_term_after_the_agent_exits_leaves_no_stop_marker` | deleted | 50-07 V-14 |
| the `HoldSide` / `HOLDFORK_SHIM_C` trap-window fixture | deleted with the five tests above | — |
| `legacy_advance_tail_passes_the_launched_stage` | deleted | V-07's full child (`advance_failed` names `expected: define`), and the e2e control (the next `stage_launched` is `plan`, for both shapes) |
| `pipe_owning_monitor_args_pass_the_stage` | replaced | `monitor_args_carry_the_launch_and_only_adapter_env_names` |
| `shell_escape_wraps_basic_strings`, `…_handles_single_quotes`, `…_handles_empty_string` | deleted | none needed (no shell) |
| `spawn_monitor_runs_agent_in_worktree_but_captures_in_project_root` | rewritten, same name | `run_supervised_stage` (capture) in a child test process |
| `spawn_monitor_agent_git_calls_resolve_workdir_not_a_hostile_git_dir` | rewritten, same name | same; new control reads the child's `/proc/<pid>/environ` for the hostile `GIT_DIR` |
| `spawn_monitor_treats_agent_args_as_literal_argv` | rewritten, same name | same |
| 11 stream-pipe tests that called `run_pipe_owning_monitor` (prompt delivery, idle timeout ×6, quota denial, non-UTF-8, 128+signal, Antigravity turn) | kept, re-pointed | test-module `supervise_stream_child`, which uses `spawn_stage_agent`, `drive_stream_io` and `runtime::exit_code`, the production pieces `run_supervised_stage` uses |
| `monitor_e2e::monitor_owns_fake_agent_and_records_devflow_result` | moved | `supervisor_e2e::capture_supervisor_owns_a_fake_agent_and_records_devflow_result` |

**Early-signal windows before the socket is published are not timing-tested.** They are safe by construction, because the signal pipe is installed before the spawn. No test here lands a signal inside that window.

## Verification (commands run, actual exit codes)

All plan gates were run verbatim at HEAD `b921ace`:
- Task 1 gate 1 (tracer + scan): exit 0, 2 × `1 test run: 1 passed`.
- Task 1 gate 2 (absence of the deleted code): exit 0. The same command exits 1 on the pre-plan tree `d8eee12`.
- Task 1 gate 3 (two NotReady tests, two `devflow_dir_gitignore` tests, semver gate): exit 0, 4 × `1 passed`, `SEMVER verdict=match`.
- Task 2 gate 1 (both auto_chain e2e targets, `only_common_names_the_devflow_binary`, `recover_clean_e2e.rs` check): exit 0. 3 + 2 + 1 tests passed.
- Task 2 gate 2 (argv presence, `monitor_pid` absence): exit 0. The same absence check exits 1 on the pre-plan `test_support.rs` and `common/mod.rs`.
- Task 3 gate 1 (V-07, SIGTERM e2e, capture-result e2e, monitor_e2e): exit 0, 4 × `1 passed`.
- Task 3 gate 2 (`scripts/check.sh all` under a sentinel HOME, then the `find` for a devflow file or socket): exit 0. The gate's trap removed its own log, so I have no test counts from that run.
- `scripts/check.sh all` with the real HOME, 20:28:53Z–20:30:40Z: exit 0, `check.sh: all OK`. The libtest summaries across 39 test binaries total 1565 passed, 0 failed, 3 ignored.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0 before each task commit.
- **Operator cache (read-only count):** files and sockets under `~/.cache/devflow` were 1041 before the plan and 1041 after both full runs. `~/.cache/devflow/roots` stayed at 1041 entries and `~/.cache/devflow/sock` at 0.

**What these runs do not establish:**
- Each new e2e test passed in a few runs at most. That shows the paths work. It does not show they are free of timing flakes; the publish wait and the 25 ms polls are the timing-sensitive parts.
- The `check.sh` counts come from libtest summary lines, summed once. I did not count them a second, independent way.
- No test covers a launcher that does not hold the per-phase lock while its agent exits before the publish wait's first poll. In production, every launch caller I checked (`start`, `advance_with`, `resume`) holds the lock, so advance-side finishing cannot race the publish. I did not audit every other caller.

## Files Created/Modified

- `crates/devflow-core/src/monitor.rs`: new `spawn_monitor`, `Published`, `MonitorArgs` and `monitor_args`. The shell monitor and `run_pipe_owning_monitor` are deleted, and the docs are rewritten. Tests are deleted, rewritten or re-pointed as tabled above.
- `crates/devflow-core/src/supervisor/runtime.rs`: `exit_code` is now `pub(crate)`.
- `crates/devflow-cli/src/main.rs`: the new `__monitor` flags, and the `--legacy-claude-launch` doc.
- `crates/devflow-cli/src/pipeline_launch.rs`: `LaunchShape`, `MonitorInvocation`, the new `run_monitor` and `supervise_monitor`, the stop recording, the `spawn_agent_and_record` record and event, the V-07 test, and the resume-tripwire update.
- `crates/devflow-cli/src/test_support.rs`: the harness-entry parse, the socket-first reapers and the `state_holding` fixture.
- `crates/devflow-cli/tests/common/mod.rs`: `LaunchedRunReaper` goes socket-first; `signal_group`.
- `crates/devflow-cli/tests/{auto_chain_flag_e2e,auto_chain_leak_repair_e2e}.rs`: the 3.x argv.
- `crates/devflow-cli/tests/supervisor_e2e.rs`: new.
- `crates/devflow-core/tests/{devflow_dir_gitignore,monitor_e2e}.rs`: case 3 now goes through `NotReady`; the spawning test is gone.
- `crates/devflow-cli/Cargo.toml` and `Cargo.lock`: `libc` dev-dependency.

## Decisions Made

See `key-decisions` in the frontmatter. All were made under Claude's remit, inside the plan's mechanism.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The AutoChainGuard now drops before the advance, not after it**
- **Found during:** Task 1
- **Issue:** With the publish wait in place, the advance returns only after the next stage's supervisor has engaged its own guard. The old drop point, at the end of the function, would then reset the flag to `false` underneath the next stage's agent.
- **Fix:** `drop(auto_chain_guard)` right after `run_supervised_stage` returns. The guard is also now conditioned on `LaunchShape::Stream`, as the plan asks.
- **Verification:** the `auto_chain_flag_e2e` tests (child observes the flag set; flag clear after the run; clear on the error path) pass.
- **Commit:** `a03b572`

**2. [Rule 3 - Blocking] Supervise-only harness waits for its launcher's record**
- **Found during:** Task 1
- **Issue:** An in-crate launch whose stub agent exits at once could finish and remove its socket before the launcher's publish wait polled. The launch would then fail as `NotReady`.
- **Fix:** On the `advance == false` path, wait up to 15 s for `state.supervisor.launch_id` to match before `finish()`.
- **Commit:** `a03b572`

**3. [Rule 3 - Blocking] 11 stream-pipe tests re-pointed instead of deleted**
- **Found during:** Task 1
- **Issue:** The plan says to delete `run_pipe_owning_monitor` "and their tests". 50-08's mapping covers only the Legacy-script tests, and these 11 tests cover idle-timeout, close-rule and exit-code behaviour that still exists.
- **Fix:** A test-module helper built from the production pieces. `runtime::exit_code` became `pub(crate)`. This is a one-word visibility change in `supervisor/runtime.rs`, which the plan otherwise leaves alone; `finish`'s final-line rule is untouched.
- **Commit:** `a03b572`

**4. [Rule 3 - Blocking] The resume whole-state tripwire also excludes `supervisor`**
- **Found during:** Task 1, CLI bin suite
- **Issue:** `resume_with_agent_preserves_every_state_field_except_agent_and_monitor_pid` failed, because a relaunch now also writes `state.supervisor`.
- **Fix:** The test asserts the field was null before and holds a socket path and launch id after, then removes it. The test name is unchanged.
- **Commit:** `a03b572`

**5. [Rule 3 - Blocking] `libc` dev-dependency for the reapers' group signal**
- **Found during:** Task 2
- **Issue:** The plan's reaper must signal the recorded agent pgid. The devflow binary crate had no `libc`, and no devflow-core public function signals a group.
- **Fix:** `libc = "0.2"` under `[dev-dependencies]`. It resolves to the version already locked for devflow-core, and nothing new was downloaded.
- **Commit:** `39c2af8`

**6. Task 1 also edited `test_support.rs` (a Task 2 file).** The harness entry parses `Command::Monitor`, so the argv change had to land with it.

**7. Commit subject over the 72-character advisory.** `b921ace`'s subject is 76 characters. The commit-msg hook only warned, and the commit was not amended.

---

**Total deviations:** 5 auto-fixed (1 Rule 1, 4 Rule 3), plus 2 notes.
**Impact on plan:** None of these changes the mechanism. The semver gate still matches.

## Issues Encountered

- My first RED run of the tracer hung for 120 s. The fake `claude` waited on `--version` (the launch health check). I killed that run by recorded pids and fixed the fake to answer `--version` first; the evidence run was a clean RED.
- Mutant M1 left one `__monitor` in an advance wait after its test failed. I confirmed by its environment that it was this run's process, then ended it by pid (SIGTERM did nothing, as noted above, so SIGKILL).

## Cleanup

- Pre-existing leaky unit tests left `sleep 30` children. I ended them by recorded pid after `/proc/<pid>/environ` showed this worktree's `CARGO_MANIFEST_DIR`. One more, 2857695, had already exited when I went to check it.
- I removed one empty `/tmp/dfh*` dir left by the killed first RED run. At the end, no `/tmp/dfs*`, `dfh*`, `dfu*` or `p50home*` dirs remain, no `__monitor`, `monitor_entry*` or `sleep 30/120/300` process remains, and `~/.cache/devflow/sock` is empty.
- The RED evidence JSON stays under `target/red-evidence/50-11-*` (gitignored). I remove the scratch dir after this SUMMARY is committed.

## User Setup Required

None.

## Next Phase Readiness

- 50-12…50-15 can read `state.supervisor` and probe it. `monitor_pid` still names the supervisor until 50-22.
- 50-12 must document the `/proc` census as 2.x-only (see Needs awareness).
- 50-13's safe points are what make a supervisor in its advance tail honour SIGTERM and shutdown.
- T-50-70 (a SIGKILL between the agent fork and the sidecar rename leaves no record) stays open. This plan does not close it.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

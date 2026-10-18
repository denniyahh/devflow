---
phase: 50-addressable-monitor-liveness
plan: 23
subsystem: cli
tags: [rust, supervisor-socket, launch, handoff, stop-forward, test-hold]

requires:
  - phase: 50-11
    provides: launch records state.supervisor from spawn_monitor's Published
  - phase: 50-13
    provides: publish-before-unlink handoff, safe points 2/3, test_hold, record_successor/take_successor, the lock-skip rule
  - phase: 50-14
    provides: devflow stop --phase N --launch <id> (the RV-08 repair)
  - phase: 50-24
    provides: wait_for_publish / NotReady teardown inside spawn_monitor; the DEVFLOW_TEST_HOLD allowlist entry
provides:
  - "record_failed_launch: a refused, early-failed or spawn-failed launch leaves state.supervisor cleared (reads NotLaunched, never GoneWhileActive)"
  - "publish_record: one save; a failed save shuts the new supervisor down, and an unconfirmed shutdown keeps the record and names devflow stop --phase N --launch <id>"
  - "post-save re-probe: a successor that already ended is recorded stopped with its own reason"
  - "run_monitor forwards a stop flagged after safe point 3 to the recorded successor, checked right after the handed-off marking (R5-I1)"
  - "test-only stopped_outcome_test_hold in Supervisor::wait_agent (DEVFLOW_TEST_STOPPED_OUTCOME_HOLD), allowlisted"
  - "stop_forwarded event"
affects: [50-15, 50-16, 50-17, 50-22]

actuals:
  tokens: 20046
  tasks: 3
  commits: 6
plan_head_before: d846c6210488333b2b95674f1ca448e24fbe4bca
plan_head_after: 0c738d226c9c91236e1abb2e8c2f24228b4a1c4c

tech-stack:
  added: []
  patterns:
    - "A launch's fallible pre-publish steps live in one helper (spawn_next_supervisor) so a single match routes every Err through the failure-path clear"
    - "A stopped-outcome hold in the supervisor, gated by cfg(any(test, feature = \"test-support\")), orders a stop decision against another process's publish by sync points, not sleeps"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-core/src/supervisor/runtime.rs
    - doc-check-allowlist.toml

key-decisions:
  - "The pre-publish steps of spawn_agent_and_record moved into spawn_next_supervisor, so the record clear stays in a helper on the error path and the success path never clears the record"
  - "publish_record takes FnMut(&State) -> Result<(), E: Into<CliError>>, so the injected save is used for the post-save stop too and workflow::save_state passes as written"
  - "The forward's outcome is also recorded as a stop_forwarded event, because the supervisor's stderr is /dev/null in production"
  - "The forward does not branch on the stop's source (the token keeps none); T-50-75 stays accepted"

patterns-established:
  - "Launch failure: clear state.supervisor only on an error return, through record_failed_launch"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "A refused, early-failed or spawn-failed launch clears state.supervisor in memory and on disk (T-50-62)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_failed_monitor_spawn_clears_the_supervisor_record"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::launch_stage_inner_clears_monitor_pid_on_early_failure"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::absent_canary_refuses_to_launch"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_supervisor_record_is_cleared_only_on_launch_failure_paths"
        status: pass
    human_judgment: false
  - id: D2
    description: "A failed publish save shuts the new supervisor down; an unconfirmed shutdown keeps the record and names devflow stop --phase N --launch <id> (T-50-63, RV-08)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_failed_publish_save_whose_shutdown_is_not_confirmed_names_the_stop_launch_repair"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_failed_publish_save_shuts_the_new_supervisor_down"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_successful_publish_save_records_and_keeps_the_new_supervisor"
        status: pass
    human_judgment: false
  - id: D3
    description: "A stop that reached the successor before its record was saved is recorded: by N's re-probe when already carried out, by N+1 itself when still in its kill grace (T-50-64, R6-E)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stop_sent_straight_to_the_next_supervisor_before_its_record_is_saved_is_recorded"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::without_a_stop_to_the_next_supervisor_the_held_publish_hands_off"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_next_supervisor_still_in_its_kill_grace_at_publish_records_its_own_stop"
        status: pass
    human_judgment: false
  - id: D4
    description: "The stopped-outcome hold is test-only and read literally; its allowlist entry passes the doc check (T-50-85)"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/runtime.rs#supervisor::runtime::tests::the_stopped_outcome_hold_is_test_only_and_read_literally"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::allowlist_entries_require_reasons"
        status: pass
    human_judgment: false
  - id: D5
    description: "A stop reaching N after safe point 3 is forwarded to N+1, which stops and records its own stop; a socket caller is answered stopped (T-50-73, T-50-83)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stop_reaching_the_supervisor_after_safe_point_3_is_forwarded_to_the_next_supervisor"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::without_a_stop_after_safe_point_3_the_next_supervisor_runs_on"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::run_monitor_checks_for_a_stop_right_after_the_handed_off_marking"
        status: pass
    human_judgment: false

duration: 27min
completed: 2026-10-18
status: complete
---

# Phase 50 Plan 23: Defined End States for Every Launch Failure Path Summary

**A launch that fails or is refused now clears the supervisor record. A publish save that fails shuts the unrecorded successor down, or names the `stop --launch` repair when that shutdown can't be confirmed. A stop that beats the publish is recorded as a stop, and a stop that reaches N after safe point 3 is forwarded to N+1.**

## Needs awareness

- **One mutant was caught somewhere other than the test meant to catch it.** Mutant M2 removed `agent_running` from the owners the post-save probe treats as running. The kill-grace test failed, but at stage N's own launch rather than at run (a)'s intermediate check: N is `agent_running` when it is published, so it was recorded as ended and never advanced ("hold was never reached" after 45 s). This shows the owner check matters on every launch. It does not show that run (a)'s intermediate check is what catches a successor wrongly read as ended. Run (b) reaches the opposite end state (this plan's reason) through the release order, and M1 hits run (a)'s owner check directly.
- **The post-save re-probe can give a slightly wrong reason in one case.** Suppose a successor whose agent completed then fails its advance before taking the per-phase lock, and finishes before the launcher's probe. The run is then recorded `stopped` with "the next stage's supervisor was stopped before its record was saved". The plan specifies this behaviour ("Alive with any other owner → stopped"). In normal operation the successor's advance waits on the per-phase lock, which the launcher holds through the probe. I checked by reading that `start`, `resume` and `advance` each hold the lock across `launch_stage`. No test covers this.
- **The RV-04 residual (T-50-74, accepted) is still open, as the plan says.** A signal that lands after N's handed-off marking, which also means after its post-marking check, and before `finish` stops N's control thread is not forwarded. A socket stop in that span is answered `handed_off` and retried, which is correct. Closing the signal case needs 50-07's runtime to forward a signal it receives while handed off.
- **T-50-75 (accepted) is unchanged.** A targeted `devflow stop --phase N --launch <N's id>` that reaches N during its post-save remainder is forwarded too, and stops N+1. The token keeps no stop source.
- **T-50-70 is still open.** A SIGKILL between an agent's fork and the sidecar rename leaves no record. This plan does not claim to close it.

## Performance

- **Duration:** 27 min
- **Started:** 2026-10-18T13:43:39Z
- **Completed:** 2026-10-18T14:10:56Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- **Failed launches leave no record (WR-04's equivalent).** Every error before the publish save is routed through `record_failed_launch`: the binary check, the capture archive, the launch context, `spawn_monitor` (`SocketPath`, `NotReady`, spawn) and the safe-point saves. `refuse_launch` clears the record beside the monitor pid. The run reads not-launched instead of sending the operator to `devflow resume`.
- **`publish_record` does the publish in one save.**
  - Save fails, shutdown confirmed: the new supervisor is shut down through its socket and the error says so.
  - Save fails, shutdown not confirmed (RV-08): nothing is cleared or saved, and the error names the socket, the launch id and `devflow stop --phase N --launch <id>`.
  - Save succeeds: the new socket is re-probed. A successor that already ended is recorded `stopped` with its own reason. A STALE successor is re-probed for 200 ms and left recorded if it stays STALE.
- **A late stop is forwarded.** `run_monitor` holds at `before_handoff_mark` (test builds only), marks itself handed off, then checks for a flagged stop and forwards it through the successor's socket. A confirmed forward takes a safe point, so a parked socket caller is answered `stopped`. Both outcomes are recorded as a `stop_forwarded` event.
- **A test-only hold orders the in-grace case by sync points.** `stopped_outcome_test_hold` in `Supervisor::wait_agent` lets the kill-grace test order N+1's stop decision against N's publish by events, not sleeps.

## Task Commits

1. **Task 1: a refused or failed launch clears the supervisor record (tracer)**: `3e70e04` (test, RED), `9fc3d60` (feat, GREEN)
2. **Task 2: the publish save, its shutdown, and recording a stop that beat it**: `b76ee88` (test, RED), `2aeadd4` (feat, GREEN)
3. **Task 3: forward a stop that reaches N after safe point 3 (RV-04)**: `04902d8` (test, RED), `0c738d2` (feat, GREEN)

Tracer gate: an interactive run with end-of-phase verification and only `<automated>` checks. I re-ran both Task 1 gates after GREEN; both passed, so the plan expanded without a checkpoint.

## TDD Gate Compliance

RED evidence came from nextest JUnit through `gsd-tools check tdd-red-evidence`. The records are in `target/red-evidence/50-23-*.json` (gitignored).

| Test | RED commit | Classifier | Failed on | GREEN |
|------|-----|------|------|------|
| `a_failed_monitor_spawn_clears_the_supervisor_record` | `3e70e04` | exit 100, `RED_EVIDENCE_OK` | the error named `sun_path`, then `state.supervisor` was still `Some(prior)` in memory | `9fc3d60` |
| `launch_stage_inner_clears_monitor_pid_on_early_failure` | `3e70e04` | exit 100, `RED_EVIDENCE_OK` | the prior record was not cleared in memory | `9fc3d60` |
| `absent_canary_refuses_to_launch` | `3e70e04` | exit 100, `RED_EVIDENCE_OK` | the refusal left the prior record | `9fc3d60` |
| `the_supervisor_record_is_cleared_only_on_launch_failure_paths` | `3e70e04` | exit 100, `RED_EVIDENCE_OK` | real file: no `record_failed_launch` call; `refuse_launch` does not clear | `9fc3d60` |
| `a_failed_publish_save_whose_shutdown_is_not_confirmed_names_the_stop_launch_repair` | `b76ee88` | exit 100, `RED_EVIDENCE_OK` | the error was only the save error, with no `devflow stop --phase 2302 --launch …` | `2aeadd4` |
| `a_failed_publish_save_shuts_the_new_supervisor_down` | `b76ee88` | exit 100, `RED_EVIDENCE_OK` | the error did not name a shutdown (the new supervisor ran on) | `2aeadd4` |
| `a_stop_sent_straight_to_the_next_supervisor_before_its_record_is_saved_is_recorded` | `b76ee88` | exit 100, `RED_EVIDENCE_OK` | `stopped` false (`stop_reason None`) | `2aeadd4` |
| `a_next_supervisor_still_in_its_kill_grace_at_publish_records_its_own_stop` | `b76ee88` | exit 100, `RED_EVIDENCE_OK` | `stopped-outcome hold never reached` | `2aeadd4` |
| `supervisor::runtime::tests::the_stopped_outcome_hold_is_test_only_and_read_literally` | `b76ee88` | exit 100, `RED_EVIDENCE_OK` | real file: no literal read, no hold fn, no call in `wait_agent` | `2aeadd4` |
| `a_stop_reaching_the_supervisor_after_safe_point_3_is_forwarded_to_the_next_supervisor` | `04902d8` | exit 100, `RED_EVIDENCE_OK` | run (a): N finished and the state named N+1, but N+1 was never stopped (the review's finding) | `0c738d2` |
| `run_monitor_checks_for_a_stop_right_after_the_handed_off_marking` | `04902d8` | exit 100, `RED_EVIDENCE_OK` | real file: `no before_handoff_mark hold after take_successor` | `0c738d2` |
| controls: `a_successful_publish_save_…`, `without_a_stop_to_the_next_supervisor_…`, `without_a_stop_after_safe_point_3_…` | in the RED commits | `INVALID_RED unexpected_green` (expected: they are controls) | — | — |

**Semantic assessment.** Each RED failed on its planned assertion with its preconditions met. In the spawn test, the error already named `sun_path` before the record assertion failed. In the held-publish and forward tests, N had reached its hold and the state named the expected supervisor. The Task 2 RED commit carried a behaviour-preserving scaffold: `publish_record` doing today's single save, plus the reason constant under `cfg_attr(not(test), expect(dead_code))`. GREEN replaced both.

## Mutation and Discrimination Evidence

| Mutant | Test run | Result |
|---|---|---|
| (control) `ok` run of the spawn test made to expect the prior launch id | `a_failed_monitor_spawn_clears_the_supervisor_record` | fails with a real new launch id (`18dfa3f8b895b604`), so the `ok` child really launches; the 0.14 s run time is not a skipped child |
| M1: hold moved after the `observe_exit` decision | kill-grace test | fails, `expected owner agent_running, found Some("finished")` |
| M2: `agent_running` not treated as running | kill-grace test | fails, but at stage N's own launch (see Needs awareness) |
| M3: RV-08 branch calls `record_failed_launch` anyway | V-38 test | fails, `the in-memory record still names the new launch` |
| M4: stop check moved before the hold and the marking | forward test / order scan | forward test fails only at run (d) (`N+1 was stopped through the forward`); scan reports `no stop_requested() check after the handed-off marking` |
| Release-build exclusion (T-50-85) | `rg -a DEVFLOW_TEST_STOPPED_OUTCOME_HOLD` | absent from `target/debug/devflow` (normal build); present (3 hits) in the CLI test binary |

Every mutant was reverted from a saved copy. `git diff` confirmed that only the intended changes were left.

## Verification (commands run, actual exit codes)

- **Task 1 gate 1** (three named tests, one nextest command each, chained with `&&`): each `1 test run: 1 passed`. I first piped them to `tail`, which hides nextest's exit code, so I re-ran them under `pipefail`: rc 0.
- **Task 1 gate 2** (`the_supervisor_record_is_cleared_only_on_launch_failure_paths`): `1 passed`, rc 0.
- **Task 2 gate 1** (six named tests, verbatim): exit 0, six `1 test run: 1 passed`. The kill-grace test took 6.6 s for two child runs.
- **Task 2 gate 2** (runtime scan plus the two doc-check tests, verbatim): exit 0, three `1 passed`.
- **Task 3 gate 1** (forward test and control, verbatim): exit 0.
- **Task 3 gate 2** (order scan): exit 0.
- **The sentinel-HOME `scripts/check.sh all` gate (Tasks 2 and 3) was run once, verbatim, at HEAD `0c738d2`.** It covers all three tasks' code. Result: `==> check.sh: all OK`, `check_all_exit=0 leaked_entries=0`, gate exit 0, on the first run. It was not also run at Task 2's GREEN.
- **`cargo clippy --workspace --all-targets -- -D warnings`:** exit 0. `cargo clippy -p devflow-core --lib -- -D warnings` (without `test-support`): exit 0.
- **All `pipeline_launch::` tests:** 104/104 passed. All `supervisor::` core tests: 41/41 passed.
- **Semver gate** (`semver-gate.sh check --through 50-11`): `SEMVER verdict=match`, exit 0. No `pub` item was added, changed or removed. `runtime.rs`'s diff has insertions only, and its only added `pub` text is inside planted test strings.

## Files Created/Modified

- `crates/devflow-cli/src/pipeline_launch.rs`: `record_failed_launch`, `spawn_next_supervisor` / `SpawnedLaunch`, `publish_record`, `shut_down_unrecorded`, `successor_runs_on`, `PublishOutcome`, `NEXT_STOPPED_BEFORE_SAVE_REASON`, `forward_stop_to_successor`; `refuse_launch` clears the record; `run_monitor`'s `before_handoff_mark` hold and post-marking check; 12 new tests and 2 extended tests.
- `crates/devflow-core/src/supervisor/runtime.rs`: `stopped_outcome_test_hold` and `STOPPED_OUTCOME_HOLD_WAIT`, gated by `cfg(any(test, feature = "test-support"))`; one gated call in `Supervisor::wait_agent`; the source-scan test.
- `doc-check-allowlist.toml`: a `source_to_docs` exception for `DEVFLOW_TEST_STOPPED_OUTCOME_HOLD`, with the plan's reason.

## Decisions Made

See `key-decisions`. All four are within Claude's remit and inside the plan's mechanism.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The pre-publish steps moved into `spawn_next_supervisor`**
- **Found during:** Task 1
- **Issue:** Routing every `?` through one clear needs one place that sees every error. The plan names the binary check, the archive and `spawn_monitor`. `launch_context()` and the safe-point saves can also fail before the publish.
- **Fix:** Those steps are now a helper returning `Result<Option<SpawnedLaunch>, CliError>`, where `None` means a stop was honoured at safe point 2 or 3. One `match` in `spawn_agent_and_record` calls `record_failed_launch` on `Err`, and names both errors if that save fails too. The WR-04 pre-clear and its save stay in place with their plain `?`, as the plan says.
- **Verification:** Task 1's four tests, and 50-13's `handoff_order_violations` scan (`spawn_agent_and_record` is still at least 40 lines and still calls `record_successor(`).
- **Committed in:** `9fc3d60`

**2. [Rule 3 - Blocking] `publish_record`'s `save` is `FnMut(&State) -> Result<(), E: Into<CliError>>`**
- **Issue:** The plan's signature is `FnOnce(&State) -> Result<(), CliError>`. The post-save stop needs a second save, and `workflow::save_state` returns `WorkflowError`. The plan's literal call `publish_record(state, published, workflow::save_state)` would not compile against that signature.
- **Fix:** The generic `FnMut` signature. The call site is the plan's, byte for byte.
- **Committed in:** `b76ee88` (scaffold), `2aeadd4`

**3. [Rule 2 - Observability] `stop_forwarded` event**
- **Issue:** The forward's outcome lines go to the `__monitor` process's stderr, which is `/dev/null` in production. This is the carried-forward 50-13 note. The unconfirmed case's repair line would never be seen.
- **Fix:** `forward_stop_to_successor` also emits `stop_forwarded {launch_id, confirmed, outcome, repair}`. It writes no state, as the plan requires. The forward test asserts exactly one event with `confirmed: true` in each run, and the control asserts none.
- **Committed in:** `04902d8`, `0c738d2`

**4. [Process] The allowlist entry landed with the RED commit `b76ee88`**
- The RED runtime test's text contains the literal read form that `doc_check` detects. Adding the entry with that test kept the doc-check tests green at every commit.

**5. [Process] A failing kill-grace run cannot strand N+1**
- `ReleaseOnDrop` creates the stopped-outcome release file when the test unwinds. N+1 is then never parked at the hold for up to 60 s after a failure. It is declared after `HandoffRun`, so it drops before the reaper.

**6. [Hygiene] Commit `3e70e04`'s subject is 73 characters**
- The `commit-msg` hook only warned. The commit was not amended.

**7. [Orchestrator-directed] Tracking**
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched. `.planning/user/errors/state-stale.json` was left uncommitted. `pin-guard.sh` passed before the first edit and before every commit.

---

**Total deviations:** 3 auto-fixed (2 Rule 3, 1 Rule 2), plus 4 process and hygiene notes.
**Impact on plan:** No public `devflow-core` change, and the semver gate matches. The mechanisms are the plan's. The event is additive observability.

## Issues Encountered

- Two of my own check commands piped nextest into `tail`, so their exit codes were `tail`'s. I re-ran those gates under `pipefail` and recorded nextest's exit codes above.

## Known Stubs

None. The Task 2 RED scaffold (a behaviour-preserving `publish_record` and the `expect(dead_code)` attributes) was replaced in `2aeadd4`. `rg` finds no `RED scaffold`, `TODO` or `FIXME` text in either source file.

## Cleanup

- **Processes:** none left. After every child-test and mutant run I checked `ps` for `monitor_entry`, `__monitor`, `sleep 0.05`, `sleep 30` and `sleep 300`. Nothing was killed: every reaper did its job.
- **Directories:** no `/tmp/dfs*`, `dfh*`, `dfu*` or `p50home*` directory remains. The operator cache count is unchanged at 1041, and `~/.cache/devflow/sock` is empty.
- **Kept on purpose:**
  - The RED evidence records and logs under `target/red-evidence/50-23-*` (gitignored).
  - The scratch directory `exec-50-23/` (nextest profile, RED script, mutant backups, gate logs). I remove it after this SUMMARY is committed.

## User Setup Required

None.

## Next Phase Readiness

- **50-22** removes `monitor_pid`. It will then:
  - drop `publish_record`'s interim `state.monitor_pid` write and the WR-04 pre-clear;
  - rename `launch_stage_inner_clears_monitor_pid_on_early_failure` (its supervisor assertions are already in place).
- **50-17 (OPERATIONS)** may want to mention the `stop_forwarded` event and the RV-08 error's `devflow stop --launch` line. Do not document `DEVFLOW_TEST_STOPPED_OUTCOME_HOLD`: it is test-only.
- **50-07 follow-ups** (reported, not done here):
  - forward a signal received while handed off (T-50-74);
  - keep a stop's source on the token, which would make a source-aware forward possible (T-50-75).

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-18*

---
phase: 50-addressable-monitor-liveness
plan: 22
subsystem: cli
tags: [supervisor, liveness, monitor_pid-removal, wr-04, stray-census]

requires:
  - phase: 50-12
    provides: the /proc stray census pinned as 2.x-only (cannot match a 3.x supervisor)
  - phase: 50-14
    provides: stop addresses the socket; stop_e2e's planted-pid test
  - phase: 50-15
    provides: the launch guard and test plant helpers no longer read or write the monitor pid
  - phase: 50-23
    provides: record_failed_launch, refuse_launch's supervisor clear, publish_record
provides:
  - the devflow CLI (src and tests) neither reads nor writes State::monitor_pid and emits no monitor_pid JSON key
  - stage_launched carries supervisor_pid, socket_path, launch_id and launch
  - registry_reachable_pids is lock holders only
  - phase7_cli's MonitorReapGuard keyed on the socket-probed supervisor pid
affects: [50-16, 50-17]

actuals:
  tokens: 14031
  tasks: 2
  commits: 2
plan_head_before: ae687db20074defd1b7ca68d9655ae289daf679c
plan_head_after: 0174cb7e1ac70e8662b3cd9ef64842870b6820ab

tech-stack:
  added: []
  patterns:
    - "An integration-test reaper takes the supervisor's pid from its socket's ping reply, never from state"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/stop_e2e.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs

key-decisions:
  - "spawn_agent_and_record keeps its pre-spawn save_state (only the monitor-pid clear went): that save also puts launch_stage_inner's checkpoint_resumes reset and Validate dispatch stamp on disk before the supervisor spawns, so dropping it would have changed on-disk timing"
  - "stage_launched's pid key is renamed supervisor_pid (same value, published.supervisor_pid); the start line says 'supervisor pid'"
  - "phase7_cli's MonitorReapGuard reads the supervisor pid from a ping of state.supervisor's socket; before 50-22 the monitor pid held that same pid"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "No comment-stripped .monitor_pid field use and no \"monitor_pid\" JSON key anywhere in crates/devflow-cli (src and tests)"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "Task 2 <automated> gate (crate-wide rg scan, planted two-direction control in Task 1's gate); HEAD-before control: 25 matches in the four Task 1 files"
        status: pass
    human_judgment: false
  - id: D2
    description: "A launch records only state.supervisor and persists it for reload"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::launch_stage_persists_the_supervisor_record_for_reload"
        status: pass
    human_judgment: false
  - id: D3
    description: "WR-04 kept: a refused launch and a failed launch both clear state.supervisor"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::launch_stage_inner_clears_the_supervisor_record_on_early_failure"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::absent_canary_refuses_to_launch"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_failed_monitor_spawn_clears_the_supervisor_record"
        status: pass
    human_judgment: false
  - id: D4
    description: "The --until stop clears only the supervisor record"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::until_stop_clears_the_supervisor_record"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_until_plan_halts_cleanly"
        status: pass
    human_judgment: false
  - id: D5
    description: "registry_reachable_pids no longer reads the monitor pid; lock holders stay reachable and a supervisor cannot be matched by the census it filters"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::a_deleted_root_contributes_nothing_to_the_reachable_set"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent.rs#agent::tests::the_stray_census_matches_only_the_2x_process_shapes"
        status: pass
    human_judgment: false
  - id: D6
    description: "stage_launched and devflow start output carry socket_path, launch_id and launch"
    requirement: SUPV-01
    verification: []
    human_judgment: true
    rationale: "No test asserts the stage_launched key set or the start line's text; established by reading spawn_agent_and_record. Absence of the old key is covered by D1."
  - id: D7
    description: "phase7_cli's reap guard still reaps the chain's last supervisor and the suite audit still requires a verified reap"
    verification:
      - kind: integration
        ref: "env -u NEXTEST cargo test -p devflow --test phase7_cli (28 passed, suite_reap_audit ok); mutant guard with no pid fails suite_reap_audit"
        status: pass
    human_judgment: false

duration: 18min
completed: 2026-10-18
status: complete
---

# Phase 50 Plan 22: CLI monitor-pid removal Summary

**The `devflow` CLI now records, reads and reports only `state.supervisor`. The interim monitor-pid write, the `stage_launched` `monitor_pid` key, the `--until` clear and the stray-reap reachability input are gone, so 50-16 can delete the field.**

## Needs awareness

- **Commit `328501e` (Task 1 alone) breaks `phase7_cli`'s suite audit under `cargo test`.** Nothing writes the monitor pid after Task 1, so `MonitorReapGuard` finds no pid, `REAPED` stays 0, and `suite_reap_audit` times out. `0174cb7` fixes it. I did not run the suite at `328501e`. I did reproduce the exact condition with a mutant guard that never finds a pid: the audit failed. The plan placed `phase7_cli.rs` in Task 2, so the split was unavoidable without moving files between tasks. A bisect that lands on `328501e` will see this.
- **The renamed early-failure test does not reach `ensure_agent_binary`.** Its doc said it did. The child's PATH holds an `exit 0` stub `claude`, so an `Auto` Code launch ends at the canary refusal (`refuse_launch`). Hand mutants show this: removing `refuse_launch`'s clear fails the test, and removing `record_failed_launch`'s clear does not. `record_failed_launch` is pinned by 50-23's `a_failed_monitor_spawn_clears_the_supervisor_record`, and that test fails under the same mutant. I corrected the doc and the assertion message.
- **No test pins the `stage_launched` key set or the start line.** Only the crate-wide gate (no `"monitor_pid"` literal can exist in the CLI) and a code read support D6.
- **`cleanup`'s live-run refusal still names `devflow resume`.** This plan rewrote fixtures in two `phase7_cli` cleanup tests (the state plant only). It did not touch the refusal path or its text, and those tests still assert `devflow resume`. Left as noted by 50-12, 50-14 and 50-15.

## Performance

- **Duration:** about 18 min
- **Started:** 2026-10-18T17:45:28Z
- **Completed:** 2026-10-18T18:03Z
- **Tasks:** 2 (Task 1 tracer, Task 2 auto)
- **Files modified:** 7

## Accomplishments

- A launch writes one liveness handle, `state.supervisor`. `publish_record` no longer writes the monitor pid. `spawn_agent_and_record` no longer pre-clears it, and `refuse_launch` clears only the record.
- `stage_launched` carries `supervisor_pid`, `socket_path`, `launch_id` and `launch`. The start line reads `supervisor pid N`.
- The `--until` stop in `pipeline_gate.rs` clears only the supervisor record.
- `registry_reachable_pids` holds lock holders only. Its doc says why a supervisor needs no entry and names 50-12's census tests.
- Every CLI test that read or planted the monitor pid now asserts or plants `state.supervisor`, or was removed with a named covering test (see below).
- `phase7_cli`'s `MonitorReapGuard` takes the pid from a ping of the recorded socket, so it still reaps the chain's last supervisor.

## Removed reads and writes, and what answers each question now

| Removed use | What answers now | Proving test |
|---|---|---|
| `spawn_agent_and_record` WR-04 pre-clear (write) | `refuse_launch` and `record_failed_launch` clear `state.supervisor` | `launch_stage_inner_clears_the_supervisor_record_on_early_failure`, `absent_canary_refuses_to_launch` (`refuse_launch`), `a_failed_monitor_spawn_clears_the_supervisor_record` (`record_failed_launch`) |
| `publish_record` interim write | `state.supervisor = Some(record)` in the same save | `launch_stage_persists_the_supervisor_record_for_reload` (mutant: dropping that line fails it) |
| `refuse_launch` monitor-pid clear | `state.supervisor = None` beside it | Task 1 gate 2 (body check) plus `absent_canary_refuses_to_launch` (mutant fails it) |
| `stage_launched` `"monitor_pid"` key | `"supervisor_pid"`, same value | crate-wide gate; no key-set test (Needs awareness) |
| `--until` stop clear (`pipeline_gate.rs`) | `state.supervisor = None` | `pipeline_gate::tests::until_stop_clears_the_supervisor_record`, `phase7_cli::start_until_plan_halts_cleanly` |
| `registry_reachable_pids` read | lock holders only; the census cannot match a supervisor | `agent::tests::the_stray_census_matches_only_the_2x_process_shapes`, `supervisor_e2e::status_reports_each_liveness_state_with_its_repair_verb` (`census_never_sees_a_live_supervisor`) |
| `phase7_cli` `MonitorReapGuard` read | `live_supervisor_pid`: ping of `state.supervisor`'s socket | `phase7_cli` under `cargo test`: `suite_reap_audit` ok; the no-pid mutant fails it |

I searched for the message literals and the event key as well as the symbol. Outside comments, no `"monitor_pid"`, `(monitor pid` or `monitor pid N` text remains in the CLI. `start_lock_e2e`'s `"monitor pid"` is a negative assertion: the refusal must not say it.

## Tests rewritten, renamed or removed

| Test | Change |
|---|---|
| `pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload` | renamed `launch_stage_persists_the_supervisor_record_for_reload` (and its `NAME`). It asserts the reloaded `state.supervisor` equals the published record |
| `pipeline_launch::tests::launch_stage_inner_clears_monitor_pid_on_early_failure` | renamed `launch_stage_inner_clears_the_supervisor_record_on_early_failure` (and its `NAME`). Monitor-pid plant and asserts dropped; doc corrected |
| `pipeline_launch::tests::resume_with_agent_preserves_every_state_field_except_agent_and_monitor_pid` | renamed `..._except_agent`. The key-removal fixture is gone, so the whole-state compare now also covers that the handoff writes no monitor pid. The supervisor assertions (50-11) are unchanged |
| `resume_clears_stop_marker_and_advances_past_stop_point`, `resume_relaunches_without_consuming_a_pending_gate_answer`, `resume_preserves_unfired_until_cap`, `resume_without_a_cap_is_unchanged`, `resume_with_agent_hands_off_and_relaunches_under_the_new_driver`, `resume_with_agent_from_a_rate_limited_state_relaunches`, `resume_re_marks_stopped_when_launch_stage_fails_outright`, `launch_stage_inner_refuses_at_code_when_the_canary_cannot_confirm`, `advance_with_declared_checkpoint_and_gate_line_only_relaunches_and_records` | assert `state.supervisor` (`is_some`/`is_none`) where they asserted the monitor pid |
| `absent_canary_refuses_to_launch` | monitor-pid plant and asserts dropped; supervisor asserts kept |
| `pipeline_gate::tests::until_stop_clears_the_supervisor_record_with_the_monitor_pid` | renamed `until_stop_clears_the_supervisor_record`; monitor-pid plant and assert dropped |
| `commands::tests::monitor_pid_persisted_for_one_phase_does_not_disturb_a_sibling` | renamed `supervisor_record_persisted_for_one_phase_does_not_disturb_a_sibling`; plants and compares two distinct records |
| `commands::tests::status_reading_monitor_liveness_writes_no_state_and_no_event` | plants a record naming a socket that does not exist and the `u32::MAX` agent group |
| `commands::tests::stop_never_treats_monitor_pid_as_a_signalling_target` | **removed.** Covered by `stop_e2e::stop_never_signals_a_recorded_pid_it_addresses_the_socket`, which asserts more: success, "nothing is running", the planted separate process still alive, `stopped` set |
| `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` | the state-recorded fixture is removed with that input. The lock-holder and orphan cases are unchanged. Covered for supervisors by 50-12's census tests |
| `commands::tests::a_deleted_root_contributes_nothing_to_the_reachable_set` | the pid's reachable path is now a lock file naming it, in place of the monitor pid |
| `commands::tests::stage_launched_ts_reflects_event_age_not_phase_started_at` | event fixture key `supervisor_pid` |
| `preflight` tests (3 asserts, 2 comments) | `state.supervisor.is_some()` |
| `phase7_cli`: `start_until_plan_halts_cleanly`, `cleanup_force_refuses_on_live_agent_unknown_monitor`, `cleanup_force_refuses_on_dead_monitor_live_agent`, the dead-phase cleanup test, `unguarded_monitor_is_detected_by_the_registry`, `MonitorReapGuard` | supervisor record or socket-probed pid |
| `stop_e2e::stop_never_signals_a_recorded_pid_it_addresses_the_socket` | plants the process only as the record's `agent_pgid` (assertion unchanged) |
| `supervisor_e2e::start_publishes_a_live_supervisor_socket_for_each_launch_shape` | drops "stays populated until 50-22"; checks the ping's `supervisor_pid` against the sidecar's |

## Task Commits

1. **Task 1 (tracer): a launch records only `state.supervisor`** - `328501e` (refactor)
2. **Task 2: the read side and the CLI tests** - `0174cb7` (refactor)

Tracer feedback gate: interactive run, `human_verify_mode` unset (default `end-of-phase`), automated-only verify. I re-ran both Task 1 gates (exit 0) and continued with no checkpoint.

## Verification (commands run, actual exit codes)

- Task 1 gate 1 (planted control plus four-file absence plus named test): exit 0. Nextest `1 test run: 1 passed, 456 skipped`. Negative control: the same comment-stripped scan over HEAD-before's four files finds 25 matches.
- Task 1 gate 2 (three WR-04 tests, each `1 passed`, plus `refuse_launch` body has `supervisor = None`): exit 0.
- `pipeline_launch::tests::` and `pipeline_gate::tests::` modules: `123 tests run: 123 passed`.
- `preflight::tests::`: `68 passed`. Touched `commands` tests: 5/5 run individually, each `1 passed`.
- `phase7_cli`, `stop_e2e`, `supervisor_e2e`, `start_lock_e2e` under nextest: `88 passed`. Under nextest `suite_reap_audit` returns early (`NEXTEST` set), so that pass is not evidence. Under `env -u NEXTEST cargo test -p devflow --test phase7_cli`: `28 passed`, `suite_reap_audit ... ok`.
- Task 2 gate (crate-wide absence plus `scripts/check.sh all` under a sentinel HOME plus no `devflow` file or socket left under it): `GATE2_EXIT=0`, `check.sh: all OK`. fmt, clippy `-D warnings` and `cargo test --workspace` passed: 39 result lines, 1631 passed, 0 failed. No test was retried.
- `cargo clippy -p devflow --all-targets -- -D warnings` before each commit: exit 0.

## Mutation and discrimination evidence

| Mutant | Target test | Result |
|---|---|---|
| M1: `refuse_launch` keeps the record | `absent_canary_refuses_to_launch` | failed (killed) |
| M1 | `launch_stage_inner_clears_the_supervisor_record_on_early_failure` | failed (killed) |
| M2: `publish_record` never sets `state.supervisor` | `launch_stage_persists_the_supervisor_record_for_reload` | failed (killed) |
| M3: `record_failed_launch` keeps the record | `launch_stage_inner_clears_the_supervisor_record_on_early_failure` | **passed (survived)**: the test reaches `refuse_launch`, not this path |
| M3 | `a_failed_monitor_spawn_clears_the_supervisor_record` | failed (killed) |
| guard never finds a pid | `phase7_cli::suite_reap_audit` (cargo test) | failed: "monitor(s) leaked after the suite: ... registered=1" |

Each mutant ran on a backup copy. The file was restored and its sha256 checked.

What this does not establish: the mutants show these tests catch these single edits. They do not show the whole launch path is covered. D6 (the event key set) has no test.

## Decisions Made

- I kept the pre-spawn `save_state` in `spawn_agent_and_record` and dropped only the monitor-pid clear. Every exit from that function saves the state afterwards anyway. But that save is the only one that puts `checkpoint_resumes = 0` and the Validate dispatch stamp on disk *before* the supervisor spawns, so I did not remove it.
- `stage_launched`'s pid key became `supervisor_pid`, as the plan allowed. There is no shim, alias or serde rename for the old field (D-20).
- Comment-only historical mentions of `monitor_pid` stay where they describe what an earlier plan did: `record_failed_launch`'s doc, `start_lock_e2e`'s two plant docs, and `stop_e2e`'s "renamed from" line. The gates strip comments by design.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `supervisor_e2e.rs` also read the field and was not in the plan's file list**
- **Found during:** Task 1 (blast-radius scan)
- **Issue:** `start_publishes_for` asserted `state.monitor_pid.is_some()` ("stays populated until 50-22") and compared it with the ping's `supervisor_pid`. Task 2's crate-wide gate cannot pass with that in place.
- **Fix:** dropped both. The ping's `supervisor_pid` is now checked against the sidecar's.
- **Files modified:** `crates/devflow-cli/tests/supervisor_e2e.rs`
- **Verification:** `start_publishes_a_live_supervisor_socket_for_each_launch_shape` passed; the crate-wide gate passed.
- **Committed in:** `0174cb7`

**2. [Rule 1 - Bug] `phase7_cli`'s `MonitorReapGuard` would have reaped nothing**
- **Found during:** Task 2
- **Issue:** the guard and the leak-detection control read `state.monitor_pid`. With Task 1 in place, nothing writes it, so every guard would bind to `None`. Under `cargo test` the suite audit then fails.
- **Fix:** added `live_supervisor_pid`, which pings `state.supervisor`'s socket and takes the reply's `supervisor_pid`. That is the same pid the field used to hold (`published.supervisor_pid`).
- **Files modified:** `crates/devflow-cli/tests/phase7_cli.rs`
- **Verification:** `cargo test -p devflow --test phase7_cli` gives 28 passed with `suite_reap_audit` ok. The no-pid mutant fails the audit.
- **Committed in:** `0174cb7`

**3. [Rule 1 - Bug] The early-failure test's doc named the wrong failure path**
- **Found during:** Task 1 (mutant M3 survived)
- **Issue:** see Needs awareness. The doc and assertion message claimed `ensure_agent_binary` fails, but the canary refusal ends the launch first.
- **Fix:** the doc now names the path reached and the test that pins the other path. The assertion message says the stub `claude` cannot confirm the canary.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Committed in:** `328501e`

**4. [Scope note] Two more tests renamed beyond the plan's two**
- `until_stop_clears_the_supervisor_record_with_the_monitor_pid`, `monitor_pid_persisted_for_one_phase_does_not_disturb_a_sibling` and `resume_with_agent_preserves_every_state_field_except_agent_and_monitor_pid` were renamed, because their names described a field they no longer touch. No `.planning/` gate or VALIDATION row references the old names (searched).

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs) plus 1 scope note.
**Impact on plan:** all within the plan's goal. `test_support.rs`, `main.rs` and `start_lock_e2e.rs` needed no change: 50-14 and 50-15 had already cleared their field uses.

## Issues Encountered

- `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` is nextest-"leaky", as already logged in `deferred-items.md` and backlog 999.122. It now leaves two `sleep 30` children, not three. After the targeted run I confirmed both by `CARGO_MANIFEST_DIR` in `/proc/<pid>/environ` and killed them by PID. After `check.sh all` one had already exited on its own.

## Known Stubs

None.

## Cleanup

- After every test run, no process carrying this worktree's `CARGO_MANIFEST_DIR` was alive. The detector was checked against a planted control process.
- The gate's `/tmp/p50home.*` was removed by the gate itself. No `/tmp/dfs*`, `/tmp/dfh*` or `/tmp/dfu*` left.
- `~/.cache/devflow` file count is 1041, as before.
- The scratch directory `exec-50-22/` (logs, backups, leak script) is removed after this commit.

## User Setup Required

None. No external service configuration required.

## Next Phase Readiness

- **50-16** can delete `State::monitor_pid`: no comment-stripped field access or JSON key remains in `crates/devflow-cli`. Core's own uses (`state.rs` tests) are 50-16's.
- **50-17 (OPERATIONS)** may document that `stage_launched` now carries `supervisor_pid` in place of `monitor_pid`.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-18*

## Self-Check: PASSED

- All 7 modified files exist. Commits `328501e` and `0174cb7` are ancestors of HEAD. The old test names no longer exist. No TODO/FIXME/placeholder was added.

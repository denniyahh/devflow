---
phase: 50-addressable-monitor-liveness
verified: 2026-10-21T12:09:37Z
status: gaps_found
score: 7/8 roadmap success criteria verified (criterion 4 partial: one reproduced case where an accepted stop is never carried out)
covered_files:
  - ".planning/phases/50-addressable-monitor-liveness/50-01-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-01-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-02-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-02-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-03-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-03-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-04-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-04-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-05-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-05-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-06-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-06-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-07-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-07-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-08-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-08-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-09-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-09-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-10-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-10-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-11-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-11-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-12-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-12-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-13-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-13-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-14-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-14-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-15-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-15-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-16-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-16-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-17-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-17-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-18-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-18-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-19-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-19-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-20-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-20-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-21-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-21-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-22-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-22-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-23-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-23-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-24-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-24-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-25-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-25-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-LIVE-RUN.md"
  - ".planning/phases/50-addressable-monitor-liveness/tools/EVIDENCE-LAYOUT.md"
  - ".planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh"
  - ".planning/phases/50-addressable-monitor-liveness/tools/fault-arm.sh"
  - ".planning/phases/50-addressable-monitor-liveness/tools/semver-gate.sh"
  - "ARCHITECTURE.md"
  - "OPERATIONS.md"
  - "crates/devflow-cli/Cargo.toml"
  - "crates/devflow-cli/src/commands.rs"
  - "crates/devflow-cli/src/main.rs"
  - "crates/devflow-cli/src/pipeline_gate.rs"
  - "crates/devflow-cli/src/pipeline_launch.rs"
  - "crates/devflow-cli/src/pipeline_outcomes.rs"
  - "crates/devflow-cli/src/preflight.rs"
  - "crates/devflow-cli/src/staleness.rs"
  - "crates/devflow-cli/src/test_support.rs"
  - "crates/devflow-cli/tests/auto_chain_flag_e2e.rs"
  - "crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs"
  - "crates/devflow-cli/tests/common/mod.rs"
  - "crates/devflow-cli/tests/doctor_antigravity.rs"
  - "crates/devflow-cli/tests/fixtures/plan-bashisms/legit-PLAN.md"
  - "crates/devflow-cli/tests/gate_notify_persist_e2e.rs"
  - "crates/devflow-cli/tests/gate_sweep_e2e.rs"
  - "crates/devflow-cli/tests/gate_wedge_e2e.rs"
  - "crates/devflow-cli/tests/help_snapshot.rs"
  - "crates/devflow-cli/tests/hermetic_spawn_sites.rs"
  - "crates/devflow-cli/tests/log_format_env.rs"
  - "crates/devflow-cli/tests/phase7_cli.rs"
  - "crates/devflow-cli/tests/plan_bashism_scanner.rs"
  - "crates/devflow-cli/tests/reap_strays_e2e.rs"
  - "crates/devflow-cli/tests/recover_clean_e2e.rs"
  - "crates/devflow-cli/tests/release_check.rs"
  - "crates/devflow-cli/tests/snapshots/devflow-help.txt"
  - "crates/devflow-cli/tests/start_lock_e2e.rs"
  - "crates/devflow-cli/tests/start_reachability_e2e.rs"
  - "crates/devflow-cli/tests/stop_e2e.rs"
  - "crates/devflow-cli/tests/supervisor_e2e.rs"
  - "crates/devflow-cli/tests/test_home_isolation.rs"
  - "crates/devflow-cli/tests/yes_ship_config.rs"
  - "crates/devflow-core/src/agent.rs"
  - "crates/devflow-core/src/doc_check.rs"
  - "crates/devflow-core/src/gates.rs"
  - "crates/devflow-core/src/lib.rs"
  - "crates/devflow-core/src/lock.rs"
  - "crates/devflow-core/src/monitor.rs"
  - "crates/devflow-core/src/recover.rs"
  - "crates/devflow-core/src/registry.rs"
  - "crates/devflow-core/src/ship.rs"
  - "crates/devflow-core/src/state.rs"
  - "crates/devflow-core/src/supervisor.rs"
  - "crates/devflow-core/src/supervisor/backstop.rs"
  - "crates/devflow-core/src/supervisor/protocol.rs"
  - "crates/devflow-core/src/supervisor/runtime.rs"
  - "crates/devflow-core/src/test_support.rs"
  - "crates/devflow-core/tests/devflow_dir_gitignore.rs"
  - "crates/devflow-core/tests/monitor_e2e.rs"
  - "doc-check-allowlist.toml"
  - "docs/diagrams/agent-lifecycle.md"
  - "docs/guides/unattended-mode.md"
covered_digest: "v3:sha256:43ae49bced6267446f302713d348986334d6659826755b596ab627193f955962"
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "Roadmap criterion 4: `devflow stop --phase N` on an ALIVE supervisor sends `shutdown`; the supervisor kills its agent's process group (grace, then SIGKILL), records exit code 143, does not advance, removes its socket and exits."
    status: partial
    reason: >-
      Holds for a supervisor whose agent is running (tests and the live fault arm), but fails, reproduced against
      the built binary at HEAD 566c434, for a stream-shape supervisor whose agent completed while a process it
      started still holds the agent's stdout: the supervisor blocks in the reader-thread join after the agent's
      exit, stays ALIVE with owner `advancing`, answers a socket `shutdown` with `accepted` and then never sends
      the final line, never exits and never advances for as long as that process lives (observed 33 s; bounded only
      by the holder's lifetime). The code review's WR-03 (deferred to backlog 999.166 on 2026-10-21 as a
      code-read, unreproduced finding). It also defeats the phase goal's "recoverable by design" for this case:
      `devflow stop` cannot end it, and after a manual `kill -9` the STALE backstop refuses (leader already reaped,
      members alive).
    artifacts:
      - path: "crates/devflow-core/src/monitor.rs"
        issue: "run_supervised_stage (about lines 1340-1354): wait_agent then an unbounded `threads.join()` on the stdout reader; a Completed outcome never signals the agent's leftover group members"
      - path: "crates/devflow-core/src/supervisor/runtime.rs"
        issue: "StopToken::observe_exit (about lines 250-262): Completed moves the owner to Advancing, so a later stop is only flagged for a safe point that run_monitor reaches after run_supervised_stage returns, which it does not"
    missing:
      - "Bound the reader join for a Completed run, or end the group's leftover members inside observe_exit while the zombie leader still pins the pgid (the stop path already does this)"
      - "A test that fails first: a stream agent that emits its result line, backgrounds a process holding its stdout and exits; the supervisor must reach its advance, and a socket stop must end it"
---

# Phase 50: Addressable Monitor Liveness Verification Report

**Phase Goal:** "Is this run's monitor alive?" is answerable from disk state alone without a PID, in three
distinguishable states (GONE, STALE, ALIVE), so a stalled unattended run is recoverable by design rather than by
diagnosis. This is the milestone's breaking change.
**Verified:** 2026-10-21T12:09:37Z
**Status:** gaps_found
**Re-verification:** No (initial verification)

## Lead: what argues against the result

One gap, reproduced, not just read. A Claude or Antigravity (stream-shape) run whose agent finishes while
something it started still holds its stdout leaves a supervisor that answers ALIVE forever, accepts `devflow
stop`, and never carries the stop out. A `kill -9` then gives STALE, and the backstop refuses because the leader is
gone. The code review found this as WR-03, and the operator deferred it on 2026-10-21 to backlog 999.166 while it
was still unreproduced. It is now reproduced, with a negative control. Everything else in the contract holds.

How likely this is in practice was not measured. The reviewer judged it unlikely for Claude, whose tool shells
use their own pipes. The capture shape (every non-Claude agent and the checkpoint-resume relaunch) has no reader
thread and cannot hit it.

Reproduction (scratch script, cleaned up). `target/debug/devflow __monitor --launch stream --agent claude` was run
with a hermetic `DEVFLOW_CACHE_DIR`, against a fake agent that reads its turn, prints a top-level
`{"type":"result","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}`, runs `sleep 90 &` and exits 0:

```
--- t=8s: monitor alive? yes
--- exit-code file: 0
--- ping:      {... "owner":"advancing" ...}
--- shutdown (wait 25s for the final line):
[0.0s] {"owner":"advancing","accepted":true,"ok":true}
timeout after 25.0s with no further line
--- after shutdown: monitor alive? yes      (ping still answers "advancing")
```

The control used the same agent with the sleeper's stdio on `/dev/null`. The supervisor logged
`supervised stream agent … ended: Completed(0)`, went straight to its advance (which failed on the scratch project's
missing state, as expected) and exited, and the socket read GONE (`connect: No such file or directory`).

## Goal Achievement

### Observable Truths (roadmap contract)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Liveness resolves to exactly one of GONE/STALE/ALIVE from `state.json` plus the socket at the recorded path, no PID read; each fixture produces only its state | ✓ VERIFIED | `supervisor::probe` (supervisor.rs:338) does lstat, connect, ping, with no pid. `status`/`doctor` go through `probe_phase` → `phase_liveness` (commands.rs:1079-1125), no pid. `supervisor::tests::each_fixture_produces_exactly_one_liveness_state` asserts each fixture's single outcome and 3 distinct answers across the positive fixtures. `a_non_socket_or_unreadable_path_is_a_probe_error_not_stale` is the other half: it shows a non-socket is a ProbeError, never Stale. Both pass in this run |
| 2 | `PipeOwning` and `Legacy` answer liveness identically | ✓ VERIFIED | Both arms reach one `__monitor` → `run_supervised_stage` (monitor.rs:418-544, 1289). `monitor::tests::both_arms_answer_liveness_identically_for_each_condition` covers Running→ALIVE(label), SupervisorKilled→STALE and Finished→GONE per shape, with a label-swap control. `both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter` passes. Live: `ATTEMPT launches=6 stream=5 capture=1`, `both launch shapes ok` (re-run today) |
| 3 | Socket path persisted in `state.json`, under `registry::cache_dir()` (`DEVFLOW_CACHE_DIR` → `$XDG_CACHE_HOME/devflow` → `~/.cache/devflow`), never the project tree or `$XDG_RUNTIME_DIR`; over-long path refused at construction with a named error | ✓ VERIFIED | `State::supervisor: Option<SupervisorRecord>` (state.rs:293, 453). `registry::cache_dir` (registry.rs:53) reads no `XDG_RUNTIME_DIR`; `rg XDG_RUNTIME_DIR crates/` finds nothing. Production `launch_context` (pipeline_launch.rs:1503) uses it. `SocketPath::new` refuses with `SocketPathError::TooLong{len,max}` for the final and `.part` names (supervisor.rs:76). Tests that pass: `socket_path_of_107_bytes_binds_and_108_is_refused_by_name` and `supervisor_e2e::start_publishes_a_live_supervisor_socket_for_each_launch_shape` (under cache/sock, not under the root, ≤107 bytes). Live: six socket paths under `$HOME/.cache/devflow/sock` |
| 4 | `stop` on an ALIVE supervisor: shutdown → group killed (grace, SIGKILL), 143, no advance, socket removed, exits; lock-free run (999.138); a stop racing natural exit never launches the next stage | ✗ PARTIAL (gap) | Holds while the agent runs. `stop_e2e::stop_ends_a_lock_free_live_run_through_the_socket` (no lock holder; group gone; exit `143`; one `stage_launched`; GONE; stopped) passes, as does its control `without_stop_a_lock_free_run_completes_and_advances`. Also passing: `supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances` and `shutdown_over_the_socket_kills_the_agent_group_and_reports_stopped`. Live fault arm (fault-6, current source) `alive-stop-*`, `stop-races-completion` ok. **Fails** in the reproduced WR-03 case above |
| 5 | SIGTERM/SIGINT to either shape behaves like a socket `shutdown` (999.134) | ✓ VERIFIED | Self-pipe handler does only getpid+write (runtime.rs:386-441). `monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance` runs both shapes × both signals, and `supervisor_e2e::sigterm_to_a_live_supervisor_ends_the_run_without_advancing` and `runtime_source_has_none_of_the_forbidden_signal_or_exit_calls` pass. No live signal record (stated in 50-LIVE-RUN.md). In the WR-03 state a signal hangs exactly as a socket stop does, so "behaves like shutdown" still holds literally |
| 6 | STALE: signal only a non-empty group with matching boot id and a present leader with the recorded start time; previous boot removed, nothing signalled; leader-gone refused with members reported (F1 documented) | ✓ VERIFIED | `backstop::classify` (backstop.rs:314) applies the rules in the stated order, and the signal goes through a pidfd re-validated after opening. Passing tests: `classify_covers_every_rule_in_order`, `a_previous_boot_socket_is_removed_and_nothing_is_signalled`, `a_reused_pgid_is_never_signalled`, `backstop_refuses_and_reports_a_group_whose_leader_is_gone`, `the_backstop_never_targets_pgid_one_or_its_own_group`, `stop_e2e::stop_tears_down_a_stale_supervisor_through_the_backstop`. Live `kill9-stale-stop`, `kill9-at-gate` ok |
| 7 | Sockets/sidecars listed and probed with no state; `doctor` reports orphans; no read-only command kills or removes; `start`/`resume` refuse on unloadable state with an ALIVE or live-STALE sidecar (999.139) | ✓ VERIFIED | `sweep_for_in` (supervisor.rs:626) and `refuse_launch_over_a_live_run` (commands.rs:3277). Passing tests: `a_published_socket_is_found_by_root_and_phase_without_state`, `sweep_reports_without_removing_or_signalling`, `doctor_reports_an_orphan_socket_without_removing_it`, `doctor_is_read_only_on_a_mismatched_project`, `status_reading_monitor_liveness_writes_no_state_and_no_event`, `start_refuses_over_a_live_supervisor_when_the_state_will_not_load`, `resume_refuses_over_a_live_supervisor_when_the_state_will_not_load`, `only_a_provably_gone_group_lets_a_stale_socket_through_the_launch_guard`, and the control `start_proceeds_over_a_corrupt_state_with_no_live_supervisor` |
| 8 | Lock reclaim treats a live pid with a different start time as stale (999.128); every e2e test isolates HOME, XDG_CACHE_HOME, DEVFLOW_CACHE_DIR per child and reaps its supervisors (999.141) | ✓ VERIFIED | lock.rs `acquire_after_existing` now classifies by start time. Passing tests: `acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time`, the controls `acquire_never_reclaims_a_live_holder_whose_start_time_matches` and `…_with_no_recorded_start_time`, and `recover_clean_e2e::explicit_clean_at_a_recycled_lock_holder_reclaims_and_cleans`. `common::hermetic_devflow` sets all three variables. Also passing: `hermetic_spawn_sites::migrated_suites_spawn_devflow_only_through_the_hermetic_builder`, `only_common_names_the_devflow_binary`, and `test_home_isolation::a_child_writing_outside_its_tempdir_home_fails` (the negative control). `LaunchedRunReaper` shuts runs down through their sockets |

**Score:** 7/8 roadmap success criteria verified (0 present-but-behavior-unverified). Criteria 4-7 are behaviour
claims; each rests on a named test that passed in this verifier's own full run, not on code presence.

**Plan-level truths (159 across 25 plans).** I did not trace each one by hand. They are covered as follows. Those
that restate a roadmap criterion are folded into the table above. About 30 were checked directly in code or by a
named test, listed under Spot-checks below. The rest rest on their named tests, which all passed in the single full
run below. 50-VALIDATION.md's 54 automated rows were not re-run one by one.

**"The milestone's breaking change":** confirmed. `semver-gate.sh check --through 50-16` printed
`SEMVER verdict=match`, with this phase's breaks named one by one: `State.monitor_pid` removed,
`run_pipe_owning_monitor` removed, `spawn_monitor` arity changed, `MonitorError` made non-exhaustive.
`State::monitor_pid` is gone from the crates (`doc_check::no_monitor_pid_field_json_key_or_help_mention_remains`
passes). The workspace version is still 2.12.0; the 3.0.0 bump happens at release.

### Deferred Items

None qualify. 999.166 is a backlog entry, and Phase 50.1's success criteria (records never saved, T-50-90/91) do not
cover WR-03 or WR-01. So under the conservative rule the gap stays a gap.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/supervisor.rs` | socket path, publish, probe, sidecar, sweep, shutdown client | ✓ VERIFIED | 2209 lines, used by monitor.rs and commands.rs |
| `crates/devflow-core/src/supervisor/runtime.rs` | single owner, self-pipe signals, control thread | ✓ VERIFIED | used by `run_supervised_stage` and `run_monitor` |
| `crates/devflow-core/src/supervisor/backstop.rs` | STALE backstop, `assess`, pidfd group signal | ✓ VERIFIED | used by `stop`, the launch guard, `recover`, `doctor` and the publish-wait teardown |
| `crates/devflow-core/src/supervisor/protocol.rs` | wire protocol, peer uid check (cfg-gated) | ✓ VERIFIED | Linux `SO_PEERCRED`, macOS `getpeereid`, others reject every peer |
| `crates/devflow-core/src/monitor.rs` | both shapes on one supervisor; publish wait; agent group guard | ✓ VERIFIED (with the gap above) | shell monitor and `run_pipe_owning_monitor` gone |
| `crates/devflow-cli/src/commands.rs` | stop / status / doctor / cleanup / launch guard on the probe | ✓ VERIFIED | |
| `crates/devflow-cli/src/pipeline_launch.rs` | `__monitor` body, handoff, safe points, record clears | ✓ VERIFIED | CR-01 safe points 5 and 6 present; their 4 tests pass |
| `crates/devflow-cli/tests/common/mod.rs` | hermetic builder + socket reaper | ✓ VERIFIED | every e2e suite that spawns devflow uses it |
| `tools/semver-gate.sh`, `check-supervisor-evidence.sh`, `fault-arm.sh` | evidence and API gates | ✓ VERIFIED | re-run today, results below |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `spawn_monitor` (both arms) | `__monitor` → `run_monitor` → `run_supervised_stage` | `monitor_args` argv + `LaunchContext.exec` | WIRED |
| `run_supervised_stage` | `Supervisor::serve_written` | sidecar written before publish | WIRED |
| `launch_context` | `registry::cache_dir()` | non-test build | WIRED |
| `stop` | `supervisor::request_shutdown` / `backstop::teardown_stale` / `stop_via_lock` | probe result | WIRED |
| `status` / `doctor` | `supervisor::probe` | `probe_phase` | WIRED |
| `start` / `resume` / `recover --clean --phase` | `possibly_live_sockets` | `refuse_launch_over_a_live_run`, `refuse_a_live_supervisor` | WIRED |
| `lock::acquire_after_existing` | `classify_holder_record` | start-time comparison | WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace, once | `cargo nextest run --workspace --no-fail-fast` | 1648 passed, 0 failed, 3 skipped (the `#[ignore]` harness entries), 4 LEAK (the known open deferred items) | ✓ PASS |
| fmt / clippy | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings` | rc 0 / rc 0 | ✓ PASS |
| macOS compile gate (V-56) | `cargo check --target aarch64-apple-darwin -p devflow-core -p devflow --all-targets` | rc 0 | ✓ PASS (compile only) |
| Named API breaks | `semver-gate.sh check --through 50-16` | `SEMVER verdict=match` | ✓ PASS |
| WR-03 reproduction | scratch `__monitor` stream run, agent backgrounds a stdout holder | supervisor ALIVE `advancing`, shutdown `accepted`, no final line in 25 s, still alive | ✗ FAIL (gap) |
| WR-03 negative control | same, holder's stdio on `/dev/null` | supervisor reached its advance and exited; socket GONE | ✓ (control discriminates) |

### Probe Execution (evidence judges, read-only)

| Probe | Result | Status |
|-------|--------|--------|
| `check-supervisor-evidence.sh --fault-current ~/.cache/devflow-phase50` | 5 `ok live`, window `ok pointer`, `FAULT_CURRENT verdict=pass` (fault-6, `bc862759…`, current crate source) | PASS |
| `--attempt-gate ~/.cache/devflow-phase50` | `ATTEMPT_GATE verdict=shipped attempt=1` (fault-5 / `9b0ec70c…`) | PASS |
| `--published 50-evidence` | `PUBLISHED verdict=pass` | PASS |
| `--live-run 50-LIVE-RUN.md` | `LIVE_RUN verdict=pass` | PASS |
| `--selftest` (the judges' negative controls) | `SELFTEST passed=55 failed=0` | PASS |

The P50 home listing (depth 3, names, mtimes and sizes) was identical before and after, so the judges wrote
nothing there. Limit: live attempt 1 ran on `9b0ec70c…`, which predates the CR-01 fix, so the current Ship path has
fixture evidence (4 CR-01 tests) and a fault-arm re-run, but no live Ship run.

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| SUPV-01 | all 25 plans (50-01…50-25) | Monitor liveness answerable from disk state alone without a PID, in three distinguishable states; both shapes identical; `sun_path` bound; path in `state.json` under `~/.cache/devflow` | ✓ SATISFIED (requirement text) | Criteria 1-3 verified. The gap is against roadmap criterion 4, which the 2026-10-10 discussion added, not against the requirement's own text |

No orphaned requirement: REQUIREMENTS.md maps only SUPV-01 to Phase 50, and every plan declares it. Two plans also
list D-18 and D-24, which are decision ids, not requirement ids.

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| Lines this phase added (56 source/doc files) | `TBD`/`FIXME`/`XXX`/`TODO`/`HACK` | none found | scan validated with a positive control (it matched `SAFETY` lines in the same files) |
| `crates/devflow-core/src/monitor.rs` (run_supervised_stage) | unbounded join after a Completed agent | 🛑 Blocker (the gap) | reproduced above |

Env mutation: the 8 `set_var`/`remove_var` lines in the phase diff were moved, not added (8 added and 8 removed,
same text, the pre-existing ENV_MUTEX-guarded idle-timeout tests).

### Judgment-tier prohibitions (42, all `status: resolved` in the plans)

Settled from evidence, not escalated. None was found broken.

| Plans | Prohibition (short) | Basis |
|-------|---------------------|-------|
| 50-01 | no migration/schema/upgrade in ROADMAP; edits scoped to Phase 50 + six backlog entries | `e0750f1` hunks touch only the Phase 50 section and the folded entries; `rg schema_version` finds nothing in crates/ or the docs |
| 50-02, 03, 21 | no test touches the real `~/.cache/devflow`; no process-global env mutation; no real (non-dry-run) reap-strays | `test_home_isolation::*`, `hermetic_spawn_sites::*` and `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` pass; the env-mutation diff nets to zero |
| 50-04 | never reclaim a live start-time-matched holder | `acquire_never_reclaims_a_live_holder_whose_start_time_matches` passes |
| 50-05 | Stale only from a same-uid ECONNREFUSED; publish never replaces; no peer accepted without a uid read | Code read of `probe`/`own_socket_at` and `publish_with` (hard_link fails on an existing name); protocol.rs cfg arms; `publish_never_exposes_…_or_replaces_a_live_one` passes |
| 50-06, 12 | read-only functions and commands never remove or signal | `sweep_reports_without_removing_or_signalling`, `doctor_is_read_only_on_a_mismatched_project`, `doctor_reports_an_orphan_socket_without_removing_it`, `doctor_finds_a_real_stray_and_never_signals_it_across_two_runs` pass |
| 50-07, 08 | handler does only getpid+write; no sigprocmask; never pgid 0/1/own; no process::exit; agent never in launcher's group | `runtime_source_has_none_of_the_forbidden_signal_or_exit_calls`, `sigterm_is_delivered_as_a_stop_event_not_handled_in_the_handler`, `both_arms_put_the_agent_in_its_own_group` pass; `signal_group` guard |
| 50-09 | never signal an unproven group; off Linux never signal or remove | `a_reused_pgid_is_never_signalled`, `backstop_refuses_…_leader_is_gone` pass; non-Linux `gather` returns unread facts → `RefuseIdentityUnavailable` → `settle` keeps the files (code read; compile-checked for macOS only) |
| 50-10 | harness entry only under `test-support`; no new env mutation | `the_harness_entry_variant_is_declared_only_under_the_test_support_feature` passes; env diff nets to zero |
| 50-11 | no toggle back to the shell monitor; launcher never replaces a pinging socket; env values never on argv | both arms go to `__monitor` (code read); `monitor_args_carry_the_launch_and_only_adapter_env_names` passes |
| 50-13, 22, 23 | no abandoned state write or held lock; record never pre-cleared; no pid read from disk signalled; no 2.x compat | `the_supervisor_record_is_cleared_only_on_launch_failure_paths`, `the_handoff_observer_detects_a_pre_clear_gap`, `stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked` pass |
| 50-14 | `stop` never signals a state-recorded pid | `stop_never_signals_a_recorded_pid_it_addresses_the_socket` passes |
| 50-15 | never a second agent over an ALIVE or live-STALE run | start/resume refusal tests listed under criterion 7 pass |
| 50-16 | no unlisted API break; no 2.x compat | semver gate `match` |
| 50-17 | no doc claims macOS support or a migration path | OPERATIONS.md:155 says "unverified on macOS"; no migration text |
| 50-18 | tools never touch the real cache, never signal unstarted processes, never kill by pattern; Phase 49 tools unedited | `fault-arm.sh` runs under a mktemp HOME and cache; `git log a890232^..HEAD -- <phase-49>/tools` is empty |
| 50-19 | no autonomous GitHub repo operations; never targets this repo; no pre-written gate answer | the attempt targeted throwaway `denniyahh/p50-live-attempt-1`. Limit: `responded_by: denniyahh` cannot tell the operator from a process running as the same user; the 50-19 checkpoint record says the operator answered |
| 50-20 | no unsanitized evidence; verdicts from checker output | `--published` pass; the operator's email fragments are absent (`rg -uuu -i` on the address parts finds nothing; the positive control finds the `REDACTED-EMAIL-LOCAL` placeholders) |
| 50-24 | publish-wait teardown signals only its own child, snapshotted children and an assessed sidecar group | `a_supervisor_that_never_publishes_is_ended_with_its_agent_group`, `a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar` pass |
| 50-25 | no 50-18 mode or case changed | selftest 55/55 |

### Human Verification Required

None as a test step. The one open item is a decision (below), not a test.

V-53 (the 3.0.0 changelog macOS-note check) runs at the release cut by design (50-LIVE-RUN.md "Release handoff").
It is not a Phase 50 goal truth.

### Known residuals (not gaps; recorded elsewhere)

- **WR-01** (backlog 999.166): `stop` prints "did not answer … nothing was signalled or recorded" for a stop the
  supervisor accepted and will carry out later, when its next check is more than about 22 s away. Under-claims
  rather than over-claims. Not reproduced here.
- **999.164 #2**: `status`/`doctor` can read UNKNOWN (probe timeout) during a stop's 3 s kill grace.
- **deferred-items.md (open, not yet in backlog):** `stop --launch` and the corrupt-state `stop` do not check that
  `sock/` is listable (the launch guard does). The sanitizer does not redact email fragments (the published tree was
  hand-redacted and is clean today). The 11 s stop wait at safe point 3. 4 tests flagged LEAK by nextest. A
  `stop_e2e` flake, which passed here.
- Live evidence is one attempt. Criteria 5, 7 and 8 have no live record (50-LIVE-RUN.md says so).

### Gaps Summary

One root cause: a Completed stream run blocks on the stdout reader before any stop check can run. It breaks
criterion 4 for one reproducible condition, and with it the goal's "recoverable by design" for that condition.
Everything else (the three-state probe, shape parity, path placement and length bound, signals, the guarded STALE
backstop, read-only listing and doctor, the launch guard, recycled-pid reclaim, test hermeticity, and the named API
break) is verified by tests that pass here and by live evidence that re-judges green.

**Decision for the operator** (the prior deferral was made before this was reproduced):

1. Fix it in Phase 50 (`/gsd-plan-phase 50 --gaps`). This is the reviewer's suggested fix plus a fail-first test.
2. Keep it deferred. To pass the phase with it carried in 999.166, add to this file's frontmatter:

```yaml
overrides:
  - must_have: "Roadmap criterion 4: devflow stop --phase N on an ALIVE supervisor sends shutdown; the supervisor kills its agent's process group (grace, then SIGKILL), records exit code 143, does not advance, removes its socket and exits."
    reason: "WR-03 (a Completed stream run whose leftover process holds the agent's stdout blocks before any stop check) reproduced 2026-10-21; accepted as a residual and carried in backlog 999.166"
    accepted_by: "<operator>"
    accepted_at: "<ISO timestamp>"
```

---

_Verified: 2026-10-21T12:09:37Z_
_Verifier: Claude (gsd-verifier)_

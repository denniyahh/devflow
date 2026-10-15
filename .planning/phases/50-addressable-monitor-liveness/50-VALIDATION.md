---
phase: "50"
slug: "addressable-monitor-liveness"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-14"
---

# Phase 50 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: `50-RESEARCH.md` § Validation Architecture (rows V-01…V-29); V-30…V-33 added from plan-checker revision round 2 (plans 50-23, 50-24); V-34…V-38 added from the round-4 external review recorded in `e68cc2f` (RV-03, RV-04, RV-05, RV-08; plans 50-06, 50-13/50-23, 50-15, 50-24); V-39…V-43 added from the same review (RV-02, RV-06, RV-07, RV-09 and the RV-05b follow-up; plans 50-14, 50-17, 50-18); V-44…V-49 added from plan-checker revision round 5 on `249df55` (R5-W3, R5-W1, R5-W2, R5-I1, R5-I3; plans 50-10, 50-24, 50-18, 50-23, 50-12), and V-42 re-derived for R5-W1. Task IDs are assigned by the planner.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (rustc 1.97.1), `tempfile`, `insta`; TDD mode on |
| **Config file** | `scripts/check.sh`, `rust-toolchain.toml`, workspace `Cargo.toml` |
| **Quick run command** | `cargo test -p devflow-core --lib supervisor::` then `cargo test -p devflow --bin devflow commands::` |
| **Full suite command** | `scripts/check.sh all` (then `scripts/check-in-container.sh` before push) |
| **Estimated runtime** | quick: tens of seconds; full: several minutes (container 2–9 min cold) |

**Exact-name rule:** run a single test as
`cargo test -p <pkg> <target> <name> -- --exact 2>&1 | tee /dev/stderr | rg -q '1 passed'`.
A bare `--exact` name that matches nothing still exits 0, so qualify unit-test names by module and assert on
`1 passed`. The package is `devflow`, not `devflow-cli`, and it is binary-only: use `--bin devflow`, never `--lib`.

---

## Sampling Rate

- **After every task commit:** the task's named tests (exact, `1 passed`) plus `cargo test -p devflow-core --lib` or `cargo test -p devflow --bin devflow` for the touched crate
- **After every plan wave:** `scripts/check.sh all`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo mutants -v --file` on `supervisor/backstop.rs` and `supervisor/runtime.rs` (no surviving mutants in the decision functions)
- **Before `/gsd-verify-work`:** full suite green, `scripts/check-in-container.sh`, the V-27 semver comparison, then the live rig
- **Max feedback latency:** the quick command; the full suite runs at wave boundaries

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| V-01 | TBD | TBD | SUPV-01 crit 1 (GONE) | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_gone_only_when_no_socket_file_exists -- --exact` | ❌ W0 | ⬜ pending |
| V-02 | TBD | TBD | SUPV-01 crit 1 (STALE) | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_stale_only_for_a_socket_whose_listener_is_gone -- --exact` | ❌ W0 | ⬜ pending |
| V-03 | TBD | TBD | SUPV-01 crit 1 (ALIVE) | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_alive_only_after_a_ping_reply -- --exact` | ❌ W0 | ⬜ pending |
| V-04 | TBD | TBD | SUPV-01 crit 1 (discrimination) | — | N/A | unit + mutants | `cargo test -p devflow-core --lib supervisor::tests::each_fixture_produces_exactly_one_liveness_state -- --exact`; `cargo mutants -v --file crates/devflow-core/src/supervisor.rs` | ❌ W0 | ⬜ pending |
| V-05 | TBD | TBD | R-6 | backstop must not fire on a stray file | non-socket path is a probe error, never STALE | unit | `cargo test -p devflow-core --lib supervisor::tests::a_non_socket_or_unreadable_path_is_a_probe_error_not_stale -- --exact` | ❌ W0 | ⬜ pending |
| V-06 | TBD | TBD | SUPV-01 crit 3 / R-5 | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::socket_path_of_107_bytes_binds_and_108_is_refused_by_name -- --exact` | ❌ W0 | ⬜ pending |
| V-07 | TBD | TBD | SUPV-01 crit 3 | — | N/A | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state -- --exact` | ❌ W0 | ⬜ pending |
| V-08 | TBD | TBD | SUPV-01 crit 2 / R-11 | — | socket mode 0600 | unit (table) | `cargo test -p devflow-core --lib monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter -- --exact` | ❌ W0 | ⬜ pending |
| V-09 | TBD | TBD | SUPV-01 crit 2 | — | N/A | unit | `cargo test -p devflow-core --lib monitor::tests::both_arms_answer_liveness_identically_for_each_condition -- --exact` | ❌ W0 | ⬜ pending |
| V-10 | TBD | TBD | R-3 | group kill must not hit the launcher's group | `pgid == agent_pid` | unit | `cargo test -p devflow-core --lib monitor::tests::both_arms_put_the_agent_in_its_own_group -- --exact` | ❌ W0 | ⬜ pending |
| V-11 | TBD | TBD | R-1 (atomic publish) | live socket never replaced | publish refuses an existing name | unit | `cargo test -p devflow-core --lib supervisor::tests::publish_never_exposes_an_unlistening_socket_or_replaces_a_live_one -- --exact` | ❌ W0 | ⬜ pending |
| V-12 | TBD | TBD | R-1 (concurrency) | — | N/A | e2e | `cargo test -p devflow --test supervisor_e2e two_concurrent_starts_yield_exactly_one_live_supervisor -- --exact` | ❌ W0 | ⬜ pending |
| V-13 | TBD | TBD | R-1 (handoff) | — | N/A | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked -- --exact` | ❌ W0 | ⬜ pending |
| V-14 | TBD | TBD | R-2 | — | N/A | unit (+ control) | `cargo test -p devflow-core --lib supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances -- --exact` | ❌ W0 | ⬜ pending |
| V-15 | TBD | TBD | R-2 (gate) | — | lock released on abort | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock -- --exact` | ❌ W0 | ⬜ pending |
| V-16 | TBD | TBD | D-05 / 999.134 | — | N/A | unit | `cargo test -p devflow-core --lib monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance -- --exact` | ❌ W0 | ⬜ pending |
| V-17 | TBD | TBD | D-04 / 999.138 | — | N/A | e2e | `cargo test -p devflow --test stop_e2e stop_ends_a_lock_free_live_run_through_the_socket -- --exact` | ❌ W0 | ⬜ pending |
| V-18 | TBD | TBD | R-4 (leader present) | wrong process signalled | start-time-validated pidfd group signal | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::backstop_kills_a_validated_group_whose_leader_is_present -- --exact` | ❌ W0 | ⬜ pending |
| V-19 | TBD | TBD | R-4 (leader gone) | wrong process signalled | refuse and report; nothing signalled | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::backstop_refuses_and_reports_a_group_whose_leader_is_gone -- --exact` | ❌ W0 | ⬜ pending |
| V-20 | TBD | TBD | R-4 (reuse) | wrong process signalled | reused pgid never signalled | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::a_reused_pgid_is_never_signalled -- --exact` | ❌ W0 | ⬜ pending |
| V-21 | TBD | TBD | R-8 | — | previous-boot socket: nothing signalled | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::a_previous_boot_socket_is_removed_and_nothing_is_signalled -- --exact` | ❌ W0 | ⬜ pending |
| V-22 | TBD | TBD | R-7 / D-06 | — | sweep is read-only | unit | `cargo test -p devflow-core --lib supervisor::tests::sweep_reports_without_removing_or_signalling -- --exact` | ❌ W0 | ⬜ pending |
| V-23 | TBD | TBD | 999.139 / R-12 | — | N/A | e2e | `cargo test -p devflow --test start_lock_e2e start_refuses_over_a_live_supervisor_when_the_state_will_not_load -- --exact` | ❌ W0 | ⬜ pending |
| V-24 | TBD | TBD | 999.128 | — | N/A | unit | `cargo test -p devflow-core --lib lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time -- --exact` | ❌ W0 | ⬜ pending |
| V-25 | TBD | TBD | 999.128 (repair) | — | a `Live` holder still refuses | unit (rewritten) | `cargo test -p devflow-core --lib recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock -- --exact` (renamed by the plan) | ✅ (flip) | ⬜ pending |
| V-26 | TBD | TBD | 999.141 / D-07 | tests touch the real `~/.cache/devflow` | hermetic HOME | e2e | `cargo test -p devflow --test test_home_isolation a_child_writing_outside_its_tempdir_home_fails -- --exact` | ❌ W0 | ⬜ pending |
| V-27 | 50-01 (tool), 50-08 / 50-10 / 50-11 / 50-16 (staged runs) | TBD | R-9 | — | N/A | script gate | `tools/semver-gate.sh check --through <plan>` before each breaking commit; compares `cargo semver-checks check-release -p devflow-core` lints item by item to the named-break table (2 pre-existing Phase 49 lints + this phase's named breaks) | ❌ W0 | ⬜ pending |
| V-30 | 50-23 | TBD | WR-04 equivalent (launch failure paths) | stale record misdirects to `resume` | refused or failed launch clears `state.supervisor` | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_failed_monitor_spawn_clears_the_supervisor_record -- --exact` (+ `absent_canary_refuses_to_launch`, `launch_stage_inner_clears_monitor_pid_on_early_failure` extended) | ❌ W0 | ⬜ pending |
| V-31 | 50-23 | TBD | R-1 (failed publish save) | unpublished supervisor survives | failed save shuts the new supervisor down | unit (+ control) | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_failed_publish_save_shuts_the_new_supervisor_down -- --exact`; control `a_successful_publish_save_records_and_keeps_the_new_supervisor` | ❌ W0 | ⬜ pending |
| V-32 | 50-23 | TBD | R-2 (stop before record saved) | — | a stop reaching N+1 before its record is saved is recorded | unit (+ control) | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_stop_sent_straight_to_the_next_supervisor_before_its_record_is_saved_is_recorded -- --exact`; control `without_a_stop_to_the_next_supervisor_the_held_publish_hands_off` | ❌ W0 | ⬜ pending |
| V-33 | 50-24 | TBD | publish-wait timeout | agent orphaned by a bare SIGKILL | a never-publishing `__monitor` is ended with its agent group | unit (+ control) | test `a_supervisor_that_never_publishes_is_ended_with_its_agent_group` (exact module path in 50-24) | ❌ W0 | ⬜ pending |
| V-34 | 50-24 | 7 | RV-03 (pre-publish SIGKILL of `__monitor`) | T-50-69 | the launcher ends the agent group the sidecar records, only after `backstop::assess` validates it | unit (+ 2 controls: sidecar removed → agent survives; wrong start time → nothing signalled) | `cargo test -p devflow-core --lib monitor::tests::a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar -- --exact` | ❌ W0 | ⬜ pending |
| V-35 | 50-23 (on 50-13's `Advancing`-until-return change) | 10 | RV-04 (stop after safe point 3) | T-50-73 | a signal or socket stop reaching N after safe point 3 is forwarded and N+1 stops; a socket caller is answered `stopped` | unit (+ control) | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_stop_reaching_the_supervisor_after_safe_point_3_is_forwarded_to_the_next_supervisor -- --exact`; control `without_a_stop_after_safe_point_3_the_next_supervisor_runs_on` | ❌ W0 | ⬜ pending |
| V-36 | 50-06 | 3 | RV-05 (sweep half) / D-09 | T-50-71 | a socket with a missing or broken sidecar is still found for its root and phase, and not for another | unit (+ controls) | `cargo test -p devflow-core --lib supervisor::tests::a_socket_whose_sidecar_will_not_parse_is_still_found_by_its_name -- --exact` | ❌ W0 | ⬜ pending |
| V-37 | 50-15 | 11 | RV-05 (guard half) / D-09 / 999.139 | T-50-72 | corrupt state plus a corrupt live sidecar → `start` refuses, and the printed `devflow stop --phase N` ends the run | e2e (+ control) | `cargo test -p devflow --test start_lock_e2e start_refuses_when_the_state_and_the_live_sidecar_both_will_not_load -- --exact`; control `a_live_socket_named_for_another_root_does_not_block_a_corrupt_state_start` | ❌ W0 | ⬜ pending |
| V-38 | 50-23 | 10 | RV-08 (unconfirmed shutdown after a failed publish save) | T-50-63 | the record is kept and the error names `devflow stop --phase N --launch <id>` | unit (+ control) | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_failed_publish_save_whose_shutdown_is_not_confirmed_names_the_stop_launch_repair -- --exact`; control `a_failed_publish_save_shuts_the_new_supervisor_down` | ❌ W0 | ⬜ pending |
| V-39 | 50-14 | 10 | RV-05b (`stop --launch` on a socket with no usable sidecar) / D-09 | T-50-76 | an ALIVE socket whose sidecar does not parse is stopped by the launch id in its file name; the several-sockets refusal prints that id; a non-matching id and a wrong phase change nothing | e2e (+ controls) | `cargo test -p devflow --test stop_e2e stop_with_a_launch_id_stops_a_live_socket_whose_sidecar_will_not_parse -- --exact` | ❌ W0 | ⬜ pending |
| V-40 | 50-14 | 10 | RV-05b (STALE with no usable sidecar) / R-4 | T-50-77 | a STALE socket whose sidecar is missing or does not parse is refused with nothing signalled or removed | e2e (+ control: a STALE socket with a parsed sidecar in the same fixture is torn down) | `cargo test -p devflow --test stop_e2e stop_with_a_launch_id_refuses_a_stale_socket_without_a_usable_sidecar -- --exact` | ❌ W0 | ⬜ pending |
| V-41 | 50-18 | 14 | RV-02 / D-16 | T-50-78 | the attempt judge passes a correct run to Ship with zero leftover sidecars and fails a launch id missing from its socket name | script selftest | `bash .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh --selftest` must print `  ok   attempt passes with zero leftover sidecars` and `  ok   attempt fails when a launch id is not in its socket name` (50-18 Task 2 gate) | ❌ W0 | ⬜ pending |
| V-42 | 50-18 | 14 | RV-07 / RV-09 / D-17 / R-10 | T-50-79, T-50-80 | the fault verdict is judged on the directory recorded from the current committed crate source, with live cases and the fixture pointer counted apart | script gate | 50-18 Task 3 gate: no uncommitted crate source; judge the newest `fault-*` whose `src_tree` equals HEAD's `crates`, `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` object ids (with `src_matches_checkout` `true`) with `check-supervisor-evidence.sh --fault`; five `FAULT case=<name> ok live` lines and `FAULT case=publish-unlink-window ok pointer` (selected by source, not by `bin_sha256`, since R5-W1; see V-46) | ❌ W0 | ⬜ pending |
| V-43 | 50-17 | 14 | RV-06 / D-02 | — | N/A | unit (doc check) | `cargo test -p devflow-core --lib doc_check::source_devflow_env_vars_and_subcommands_are_documented -- --exact` after the OPERATIONS.md rewrite (50-17 Task 1's `subcommand[devflow monitor]` and `subcommand[devflow advance]` checks catch the dropped phrase first) | ✅ (exists) | ⬜ pending |
| V-44 | 50-10 | 6 | R5-W3 / CLAUDE.md doc check | — | N/A | unit (doc check) + gate | 50-10 Task 2 gate: `doc-check-allowlist.toml` holds one reasoned `source_to_docs` entry each for `DEVFLOW_MONITOR_ARGV` and `DEVFLOW_TEST_MONITOR_FULL`, and `exact-test.sh -p devflow-core --lib -- doc_check::source_devflow_env_vars_and_subcommands_are_documented doc_check::allowlist_entries_require_reasons` passes (control, tested: literal reads with no entries fail the doc check) | ✅ (tests exist) | ⬜ pending |
| V-45 | 50-24 | 7 | R5-W3 / CLAUDE.md doc check | — | N/A | unit (doc check) + gate | 50-24 Task 2 gate: the same check for `DEVFLOW_TEST_HOLD`, which 50-13's CLI `test_hold` also reads and relies on | ✅ (tests exist) | ⬜ pending |
| V-46 | 50-18 | 14 | R5-W1 / RV-07 | T-50-79, T-50-81 | a correct fault run still passes V-42 after later commits that leave crate source unchanged; a directory from other crate source fails | script gate | 50-18 Task 1 gate (`src_tree_is_head_crate_source=1`, and `src_matches_checkout` equal to what the crate-source status implies) and Task 3 gate (`other_source_control=fault-1`, the inline other-source control) | ❌ W0 | ⬜ pending |
| V-47 | 50-18 | 14 | R5-W2 / D-17 / R-10 | T-50-82 | `--attempt-gate` gives `shipped` only with a fault directory holding the full case set on the attempt's binary | script selftest | `bash .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh --selftest` must print `  ok   fault require-all fails a partial directory` and `  ok   attempt gate refuses a partial fault directory` (50-18 Task 2 gate) | ❌ W0 | ⬜ pending |
| V-48 | 50-23 | 10 | R5-I1 / RV-04 | T-50-83 | a stop flagged while N is still `Advancing`, held just before its handed-off marking, is forwarded and N+1 stops | unit (run (d) of V-35's test, + V-35's no-stop control) + order gate | `bash .planning/phases/50-addressable-monitor-liveness/tools/exact-test.sh -p devflow --bin devflow -- pipeline_launch::tests::a_stop_reaching_the_supervisor_after_safe_point_3_is_forwarded_to_the_next_supervisor`; 50-23 Task 3 order gate over `run_monitor` (`take_successor(`, then the `before_handoff_mark` hold, then `OwnerPhase::HandedOff`, then `stop_requested()`) | ❌ W0 | ⬜ pending |
| V-49 | 50-12 | 9 | R5-I3 / D-06 | T-50-84 | doctor lists this root's ALIVE broken-sidecar and STALE missing-sidecar sockets by the launch id in their names, and not another root's | unit (+ controls: another root's sidecar-less socket; a state-matched one) | `bash .planning/phases/50-addressable-monitor-liveness/tools/exact-test.sh -p devflow --bin devflow -- commands::tests::doctor_reconciliation::doctor_reports_an_orphan_socket_without_removing_it` (fixtures (e)-(h)); 50-12 Task 3 gate requires a `sweep_for_in(` call | ❌ W0 | ⬜ pending |
| V-28 | TBD | TBD | D-16 | — | N/A | manual (live rig) | runbook | manual | ⬜ pending |
| V-29 | TBD | TBD | D-17 / R-10 | — | N/A | manual + script | `tools/fault-arm.sh` record | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-cli/tests/common/mod.rs` — hermetic `HOME`/`XDG_*`/`DEVFLOW_CACHE_DIR` child builder plus reaper (999.141; lands **first**, D-07)
- [ ] an injectable cache dir for in-crate `--bin` tests (research Finding 7c)
- [ ] `crates/devflow-cli/tests/test_home_isolation.rs` — V-26
- [ ] `crates/devflow-cli/tests/supervisor_e2e.rs` — V-12 and fault-arm fixtures
- [ ] `crates/devflow-core/src/supervisor*.rs` test modules — V-01…V-06, V-11, V-14, V-18…V-22
- [ ] `.planning/phases/50-addressable-monitor-liveness/tools/fault-arm.sh` and the Phase 50 copies of the Phase 49 rig tools

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live run to Ship that crosses both monitor arms | D-16 (SUPV-01 crit 2) | needs a real agent, a disposable GitHub repo and the operator's preflight approvals | Phase 49 rig runbook, one `checkpoint:human-action` per attempt, capped at 3 attempts (49 D-10/D-11/D-18/D-19) |
| Fault arm: ALIVE→stop→GONE; kill -9→STALE→stop→GONE; SIGKILL while advancing; stop racing completion; the publish/unlink window as a fixture pointer (`live: false`, reported `ok pointer` and never counted as a live case, RV-09) | D-17 / R-10 | real process faults against a live supervisor | `tools/fault-arm.sh`; each live record names the launch shape, socket path, probe result, group members before and after, and whether advance ran |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < quick-command runtime
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

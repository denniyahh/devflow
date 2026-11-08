---
phase: "51"
slug: "rate-limit-agent-failover"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-11-05"
revised: "2026-11-06 (plan check after the 0d80aac replan: the evidence checker resolves cited files, lines and tests; the phase gate's supply-chain scan fails on a missing or failing tool, not on an advisory finding; earlier: reviews round 2, 0d80aac: a failed failover relaunch ends at a gate (R2-D1), evidence rows checked by Rust tests, resume lock probes withheld, `scripts/check.sh deps` in the phase gate; earlier: plan check iteration 2: 51-06-03 runs in a real supervisor; 51-10-03 and 51-13-03 gate on their own headings; earlier: reviews round 1, ae690dd, plan set 51-01..07, 51-09..14, 51-08 dropped)"
---

# Phase 51 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness, run through cargo-nextest (installed version recorded in RESEARCH: 0.9.150) |
| **Config file** | none required (the CLAUDE.md gate form) |
| **Quick run command** | one named test per command: `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=module::path::name)'` (CORE), `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=module::path::name)'` (CLI), `cargo nextest run -p devflow --test <suite> --no-tests=fail -E 'test(=name)'` (E2E) |
| **Full suite command** | `scripts/check.sh all` (fmt, clippy `-D warnings`, the whole workspace suite; no dependency scan), the supply-chain scan `all` does not run — the tools `scripts/check.sh deps` runs, split so a missing or failing tool fails and an advisory finding is a recorded warning: `cargo deny --version && cargo machete --version && cargo deny check bans licenses sources && cargo deny check -W vulnerability -W unmaintained -W unsound -W notice -W yanked advisories && cargo machete` — then `scripts/check-in-container.sh` |
| **Semver gate** | `cargo semver-checks check-release -p devflow-core` (every plan that touches `devflow-core`) |
| **Ignored evidence tests** | `pipeline_launch::tests::the_supv01_evidence_trace_rows_are_complete` and `…::the_supv01_evidence_is_complete` read `.planning/` and are `#[ignore]`d (develop and main carry no `.planning/`); run each with `cargo nextest run -p devflow --bin devflow --run-ignored only --no-tests=fail -E 'test(=…)'`. `scripts/check.sh all` does not run them |
| **Estimated runtime** | Not measured at planning time. A single named gate is dominated by the incremental build of the touched crate. The 50-13 handoff-harness tests wait up to `HANDOFF_STEP_WAIT` (45 s) per step and the binary e2e tests up to 120 s, so a failing one can take that long before it reports |

---

## Sampling Rate

- **After every task commit:** that task's `<automated>` gate — its named CORE/CLI/E2E tests, one per command, chained with `&&`
- **After every plan wave:** `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow`, plus `cargo semver-checks check-release -p devflow-core` when the wave touched `devflow-core`, plus any `--test` suite the wave added (`failover_e2e`, `fallback_agent_config`)
- **Before `/gsd-verify-work`:** 51-11 Task 2's phase gate — `scripts/check.sh all`, the split supply-chain scan (advisory findings recorded, not gating), the semver check and `scripts/check-in-container.sh` — must be green
- **Max feedback latency:** not measured (see Estimated runtime); bounded by the waits named there

---

## Per-Task Verification Map

Notation: CORE / CLI / E2E as in the quick run command. Each row names the task's primary new test; the task's `<automated>` block in its PLAN.md runs it together with the other tests named in parentheses and the regression tests it keeps green. "❌ new" means the test does not exist at `ae690dd` and the task writes it first (TDD); "✅" means an existing test.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 51-01-01 | 01 | 1 | SUPV-02 | T-51-01, T-51-02, T-51-33 | failover only from `RateLimited`, only at Code, one hop | e2e (handoff harness) + unit | CLI `pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage` (+ CORE `state::tests::fallback_fields_round_trip_and_tolerate_states_predating_them`, CLI `pipeline_launch::tests::failover_target_selects_the_fallback_only_at_code`) | ✅ | ✅ green |
| 51-01-02 | 01 | 1 | SUPV-02 | T-51-03, T-51-33 | a refused handoff is announced and takes the cron arm; a Plan rate limit never hands off | e2e + agent-free child | CLI `pipeline_launch::tests::a_refused_handoff_takes_the_cron_arm_and_names_the_refusal` (+ `…::a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off`, `…::a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm`) | ✅ | ✅ green |
| 51-02-01 | 02 | 1 | SUPV-02 | T-51-05 | N/A (source scan) | source scan | CORE `doc_check::the_removed_sequentagent_handoff_stays_dead` | ✅ | ✅ green |
| 51-02-02 | 02 | 1 | SUPV-02 | T-51-05 | N/A (source scan) | source scan | CORE `doc_check::the_removed_sequentagent_handoff_stays_dead` (extended, incl. exact-element decoys such as `name = "__monitor"`; + `doc_check::no_monitor_pid_field_json_key_or_help_mention_remains`) | ✅ | ✅ green |
| 51-03-01 | 03 | 1 | SUPV-02 | T-51-06 | the cause comes only from the last top-level `result` | unit + stream child | CORE `monitor::tests::the_idle_timeout_record_carries_the_last_result_cause` (+ `agent_result::tests::the_idle_timeout_cause_comes_from_the_last_top_level_result`, `…::an_idle_timeout_reason_names_the_last_result_cause`, `…::an_idle_timeout_without_cause_fields_keeps_todays_message`) | ✅ | ✅ green |
| 51-03-02 | 03 | 1 | SUPV-02 | T-51-06, T-51-07, T-51-08 | agent-authored cause text bounded, control-free, capped inside the gate context; on-disk keys pinned | unit | CORE `agent_result::tests::idle_timeout_cause_text_is_bounded_and_control_free` (+ `…::a_pre_51_idle_timeout_record_still_parses`, `…::the_on_disk_idle_timeout_record_key_set_is_pinned`, `…::the_idle_timeout_clause_is_capped_at_the_cause_bound`, CLI `pipeline_outcomes::tests::an_idle_timeout_gate_context_names_the_last_result_cause`) | ✅ | ✅ green |
| 51-04-01 | 04 | 2 | SUPV-02 | T-51-10, T-51-11 | closed-set flag value; arming announced | parse + agent-free child | CLI `commands::tests::start_persists_the_fallback_agent_in_the_run_state` (+ `tests::start_accepts_a_fallback_agent_flag`) | ✅ | ✅ green |
| 51-04-02 | 04 | 2 | SUPV-02 | T-51-09 | auto mode refuses a fallback outside the unattended launch shape, before any write | unit + child | CLI `commands::tests::start_in_auto_mode_refuses_a_fallback_outside_the_unattended_launch_shape` (+ `preflight::tests::the_fallback_launch_shape_check_refuses_only_in_auto_mode`, `commands::tests::start_refuses_a_fallback_equal_to_the_agent`) | ✅ | ✅ green |
| 51-04-03 | 04 | 2 | SUPV-02 | T-51-09 | a refused resume writes nothing (notify command unchanged) | child | CLI `pipeline_launch::tests::resume_refuses_a_fallback_equal_to_the_run_agent_before_any_write` (+ `tests::resume_accepts_a_fallback_agent_flag`, `pipeline_launch::tests::resume_with_a_fallback_agent_rearms_a_consumed_fallback`, `…::resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run`, `…::resume_without_a_fallback_flag_keeps_a_consumed_fallback_spent`, `…::resume_refuses_an_agent_equal_to_the_saved_unused_fallback`) | ✅ | ✅ green |
| 51-05-01 | 05 | 2 | SUPV-02 | T-51-12 | a torn line is never classified `RateLimited`; only top-level events decide a denial | unit | CORE `agent_result::tests::only_top_level_rate_limit_events_and_results_decide_a_denial` (+ `…::a_torn_final_line_is_not_read_as_a_clean_silence`, `…::a_torn_clause_leads_the_capped_clause_at_the_cause_bound`, `…::the_on_disk_idle_timeout_record_key_set_is_pinned`) | ✅ | ✅ green |
| 51-05-02 | 05 | 2 | SUPV-02 | T-51-12 | a torn tail at the timeout is written and flagged; a denial is still suppressed | stream child | CORE `monitor::tests::a_torn_final_line_at_the_idle_timeout_is_flagged_on_the_record` (+ ✅ `monitor::tests::a_quota_denial_is_not_recorded_as_an_idle_timeout`) | ✅ | ✅ green |
| 51-06-01 | 06 | 3 | SUPV-02 | T-51-14 | the fallback runs its own delivery canary | unit + child | CLI `pipeline_launch::tests::a_handoff_reruns_the_delivery_canary_for_the_new_agent` (+ `…::the_handoff_event_records_the_tree_it_hands_over`, `…::a_rate_limit_before_any_commit_still_hands_off`) | ✅ | ✅ green |
| 51-06-02 | 06 | 3 | SUPV-02 | T-51-16 | failover bounded by its one-hop mark, outside the infra ceiling | agent-free child | CLI `pipeline_launch::tests::an_unused_fallback_hands_off_one_below_the_infra_ceiling` (+ `…::a_rate_limit_after_the_one_hop_takes_the_cron_arm`, `…::an_ambiguous_outcome_with_an_unused_fallback_re_drives_without_handing_off`) | ✅ | ✅ green |
| 51-06-03 | 06 | 3 | SUPV-02 | T-51-15, T-51-39, T-51-40 | a failed relaunch after the commit ends at the never-silent gate inside a running supervisor (no cron instructions); a supervisor replaced before its relaunch writes nothing | handoff harness (real supervisor, fence active) + source scan | CLI `pipeline_launch::tests::a_handoff_whose_relaunch_fails_ends_at_a_gate` (+ `…::a_failover_superseded_mid_relaunch_writes_nothing`, `…::the_supervisor_record_is_cleared_only_on_launch_failure_paths` (extended, code part only), ✅ `…::launch_stage_inner_clears_the_supervisor_record_on_early_failure`; old-ordering mutation run recorded) | ✅ | ✅ green |
| 51-14-01 | 14 | 3 | SUPV-02 | T-51-35, T-51-36 | a key-supplied fallback is typed, announced with its source, and persisted by a real start | unit + binary e2e | E2E (`--test fallback_agent_config`) `config_key_alone_arms_the_fallback_and_names_its_source` (+ CORE `config::tests::fallback_agent_key_resolves_from_devflow_toml`, E2E `fallback_flag_wins_over_the_config_key`, `no_flag_and_no_key_arms_no_fallback`, `config_key_alone_is_persisted_in_the_run_state`; mutation run recorded) | ✅ | ✅ green |
| 51-14-02 | 14 | 3 | SUPV-02 | T-51-34, T-51-35 | the key is refused exactly like the flag; no environment variable selects a fallback | binary e2e + source scan | E2E `config_key_equal_to_the_agent_refuses_start_and_names_the_key` (+ `config_key_with_an_invalid_value_refuses_start`, `a_valid_flag_does_not_mask_an_invalid_config_key`, CORE `doc_check::no_environment_variable_selects_the_fallback_agent`) | ✅ | ✅ green |
| 51-07-01 | 07 | 4 | SUPV-02 | T-51-18 | the fallback receives a prompt from its own renderer | agent-free child (argv) + source scan | CLI `pipeline_launch::tests::a_code_failover_during_an_audit_fix_pass_replays_audit_fix` (+ CORE `state::tests::code_fix_round_trips_and_defaults_to_none`, CLI `pipeline_launch::tests::every_no_override_launch_renders_through_default_stage_prompt`) | ✅ | ✅ green |
| 51-07-02 | 07 | 4 | SUPV-02 | T-51-17 | a fix type cannot outlive its Code pass | child | CLI `pipeline_gate::tests::a_transition_clears_the_persisted_code_fix` (+ `pipeline_launch::tests::a_resume_mid_audit_fix_replays_audit_fix`) | ✅ | ✅ green |
| 51-12-01 | 12 | 5 | SUPV-02 | T-51-29, T-51-30 | hermetic children, every launch reaped; a refused fallback canary ends at a gate | binary e2e | E2E (`--test failover_e2e`) `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code` (+ `a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate`) | ✅ | ✅ green |
| 51-12-02 | 12 | 5 | SUPV-02 | T-51-29 | the new suite stays hermetic | binary e2e + source scan | E2E `a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions` (+ ✅ `--test test_home_isolation` `suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment`, ✅ `--test hermetic_spawn_sites` `only_common_names_the_devflow_binary`) | ✅ | ✅ green |
| 51-09-01 | 09 | 5 | SUPV-02 | T-51-23 | the saved notify command survives an unset variable | binary e2e (+ recorded mutant run) | E2E (`--test gate_notify_persist_e2e`) `resume_with_the_variable_unset_keeps_the_saved_command` (+ `resume_with_an_empty_variable_is_seen_cleared_by_the_post_commit_waiter`, ✅ two existing resume tests; `git diff --exit-code HEAD -- crates/devflow-cli/src/pipeline_launch.rs`) | ✅ | ✅ green |
| 51-09-02 | 09 | 5 | SUPV-02 | T-51-21, T-51-22 | an unloadable state is never rewritten; the on-disk notify command is the one fired | unit | CLI `pipeline_gate::tests::gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded` (+ `…::gate_entry_over_an_unparseable_state_does_not_rewrite_it`) | ✅ | ✅ green |
| 51-09-03 | 09 | 5 | SUPV-02 | T-51-23 | every strip site of the notify command is pinned | source scan | CORE `doc_check::strip_notify_env_doc_names_exactly_its_call_sites` (+ `doc_check::gate_notify_cmd_doc_cites_the_tests_that_pin_it`) | ✅ | ✅ green |
| 51-10-01 | 10 | 6 | SUPV-02 | T-51-24, T-51-41 | the test seam is compiled out; exactly one publish wait; no agent group outlives a withheld publish | handoff harness + source scan | CLI `pipeline_launch::tests::failover_with_the_publish_probe_withheld_ends_at_a_gate` (+ CORE `monitor::tests::the_publish_probe_seam_is_compiled_out_of_non_test_builds`, `monitor::tests::non_test_code_waits_for_publish_exactly_once`) | ✅ | ✅ green |
| 51-10-02 | 10 | 6 | SUPV-02 | T-51-25 | each withheld read has an ordinary-launch control; the ordinary control asserts the observable outcome only | handoff harness + child | CLI `pipeline_launch::tests::an_ordinary_transition_with_the_publish_probe_withheld_fails_the_same_way` (+ `…::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_recorded_probe_cannot_answer`) | ✅ | ✅ green |
| 51-10-03 | 10 | 6 | SUPV-02 | T-51-25 | the evidence file's rows are checked and every cited file, line and test resolves; a stub and a fabricated well-formed text are rejected | Rust row checker (planted texts) + ignored real-file test | CLI `pipeline_launch::tests::the_supv01_evidence_check_rejects_a_stub` && CLI (`--run-ignored only`) `…::the_supv01_evidence_trace_rows_are_complete` && CLI `…::failover_with_the_publish_probe_withheld_ends_at_a_gate` | ✅ | ✅ green |
| 51-13-01 | 13 | 7 | SUPV-02 | T-51-37 | the CLI test seam is compiled out; the withheld answer (`Gone`) takes the branch that changes the outcome | handoff harness + source scan | CLI `pipeline_launch::tests::failover_with_the_post_save_probe_answering_gone_records_the_run_stopped` (+ `…::an_ordinary_transition_with_the_post_save_probe_answering_gone_records_the_run_stopped`, `…::the_successor_probe_seam_is_compiled_out_of_non_test_builds`) | ✅ | ✅ green |
| 51-13-02 | 13 | 7 | SUPV-02 | T-51-38 | each withheld read has an ordinary-launch (or ordinary-resume) control | handoff harness + child | CLI `pipeline_launch::tests::failover_over_an_unprobeable_pending_record_ends_at_a_gate` (+ `…::an_ordinary_transition_over_an_unprobeable_pending_record_is_refused_the_same_way`, `…::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_recorded_lock_cannot_be_probed`, `…::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_pending_lock_cannot_be_probed`) | ✅ | ✅ green |
| 51-13-03 | 13 | 7 | SUPV-02 | T-51-38 | the evidence file is complete row by row, with both arm conclusions naming tests, and every citation resolves | Rust row checker (complete mode) + ignored real-file tests | CLI `pipeline_launch::tests::the_supv01_evidence_check_rejects_a_stub` (extended) && CLI (`--run-ignored only`) `…::the_supv01_evidence_is_complete` && CLI (`--run-ignored only`) `…::the_supv01_evidence_trace_rows_are_complete` && CLI `…::failover_with_the_post_save_probe_answering_gone_records_the_run_stopped` && CLI `…::failover_over_an_unprobeable_pending_record_ends_at_a_gate` | ✅ | ✅ green |
| 51-11-01 | 11 | 8 | SUPV-02 | T-51-26 | the docs never promise more than the code | doc scan | CORE `doc_check::operations_md_documents_the_fallback_agent_and_its_events` (+ ✅ `doc_check::doc_referenced_identifiers_exist_in_source`, `…::source_devflow_env_vars_and_subcommands_are_documented`, `…::operations_md_documents_run_liveness_and_the_stop_arms`, `…::docs_describe_the_socket_supervisor_not_the_shell_monitor`) | ✅ | ✅ green |
| 51-11-02 | 11 | 8 | SUPV-02 | T-51-SC | the supply-chain scan runs in the phase gate (`all` has none): a missing or failing tool fails it, an advisory finding is a recorded warning | full suite + dependency scan | `scripts/check.sh all && cargo deny --version && cargo machete --version && cargo deny check bans licenses sources && cargo deny check -W vulnerability -W unmaintained -W unsound -W notice -W yanked advisories && cargo machete && cargo semver-checks check-release -p devflow-core` (+ `scripts/check-in-container.sh`) | ✅ | ✅ green (51-11 SUMMARY run at 1e61089; no code change since) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

Criterion coverage: criterion 1 → 51-01-01, 51-12-01 (and 51-06, 51-07 for what crosses the hop); criterion 2 → 51-01-02, 51-12-02; criterion 3 → 51-10, 51-13 (evidence file); criterion 4 → 51-02. Folded backlog: 999.102 → 51-03; 999.71 (classifier only, R-4) → 51-05; 999.154 → 51-09. Operator rulings: R-1 → 51-03; R-2 → 51-01, 51-06, 51-10, 51-12, 51-13; R-3 → 51-14 (+ 51-01, 51-04, 51-11); R-4 → 51-05; R-5 → no live forced rate-limit run is planned.

---

## Wave 0 Requirements

No separate Wave 0 plan. Existing infrastructure covers the phase: cargo-nextest, the 50-13 handoff harness
(`run_handoff_child`, `HandoffRun`), `run_hermetic_test_in_child`, `agent_free_dir_with_agent_stub`,
`common::hermetic_devflow`. Each plan's own first task writes its tests first and builds the harness extension it needs:

- [x] Second agent stub in the 50-13 handoff child's PATH dir — 51-01 Task 1
- [x] WR-13 waiter on the first `stage_launched` event — 51-09 Task 1
- [x] Publish-wait withholding seam (`DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE`, test-support-gated) + allowlist entry — 51-10 Task 1
- [x] Post-save probe withholding seam (`DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE`, `#[cfg(test)]`, answers `Gone`) + allowlist entry — 51-13 Task 1
- [x] Evidence-row checker `supv01_evidence_violations` (test module of `pipeline_launch.rs`) with its planted-stub test — 51-10 Task 3; complete mode extended by 51-13 Task 3

---

## Manual-Only Verifications

All phase behaviors have automated verification. D-13's natural-rate-limit observation from the phase's own dogfood run is
optional evidence, read (read-only) from the main checkout's `.devflow/events.jsonl` in 51-13 Task 3, not a gate; R-5
rules out a live forced rate-limit run.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (no Wave 0 plan; each MISSING test was written first by its own task and exists at validation)
- [x] No watch-mode flags
- [x] Feedback latency measured (validate-phase 2026-11-07: all 88 mapped tests, incl. the 2 ignored evidence tests, ran in 12.95 s after an incremental build)
- [x] `nyquist_compliant: true` set in frontmatter (set by validate-phase, not by the planner)

**Approval:** validated 2026-11-07 by gsd-validate-phase (88/88 mapped tests green: `cargo nextest run --workspace --run-ignored all` over the map's exact names)

**Re-validation 2026-11-08 (after the code-review fix round `9886a95`..`c6e29e0`, at `fe1971a`):** 88/88 mapped tests green again. `cargo nextest run --workspace --no-fail-fast` ran 2111 tests: 2110 passed and 1 failed. The failure is `pending_supervisor_e2e::stop_ends_a_launch_its_running_launcher_promotes_during_the_wait`, a Phase 50.1 test outside this map. It passed 10/10 alone and is filed as backlog 999.191. The two ignored evidence tests passed with `--run-ignored only`. Every mapped name was checked against `cargo nextest list`, and every one is in the run's PASS lines. The fix round's own tests also passed: `fallback_agent_config::parallel_ignores_the_config_key`, `failover_e2e::approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on` and `failover_e2e::rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on`. Out of scope here: the README.md:110 over-promise for `devflow parallel` (51-VERIFICATION.md gap). It is a docs-accuracy gap, not a missing test.

**Re-validation 2026-11-08 (after the second fix round `5ced4d8`..`2213057`, at `2213057`):** no gaps. `cargo nextest run --workspace --no-fail-fast` ran 2120 tests: 2120 passed, 6 skipped (ignored). This run does not reproduce 999.191, which is a load-dependent flake: one green run does not close it. The two ignored evidence tests passed with `--run-ignored only`. Each behavior change in the round carries its own test, and each is in the run's PASS lines: `parallel_refuses_a_config_key_that_names_no_agent` (WR-02), `after_the_refused_relaunch_status_and_resume_lead_to_another_agent` and `recovery_hints_name_another_agent_when_the_recorded_canary_refuses_the_run` (WR-01), `recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome` (IN-01), `a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work`, `gate_list_shows_the_context_as_far_as_the_banner_does` and `a_denial_followed_by_a_torn_line_is_torn_for_the_monitor_and_the_verdict`. A made-up test name matched no PASS line, which is the control. IN-03 extends an existing e2e assertion. IN-04 changes log wording only, and its monitor test asserts the new wording.

**Re-validation 2026-11-08 (after the third fix round `ff649ff`..`744739a`, at `21c7c70`):** no gaps. `cargo nextest run --workspace --no-fail-fast` ran 2123 tests: 2123 passed, 6 skipped (ignored), rc 0. `--run-ignored only` on the CLI binary passed 4/4, including both evidence tests. Each behavior change in the round has its own test, and each is in the run's PASS lines: `the_failure_gate_way_on_survives_the_banner_and_gate_list_for_a_long_reason` (cce00cc), `recovery_hints_judge_the_environment_legacy_opt_out_as_resume_does` (d149da8) and `the_budgeted_idle_timeout_clause_closes_inside_the_gate_display_bound` (744739a). ff649ff and fc92c6b change comments only. A made-up test name matched no PASS line, which is the control. One green run does not close the 999.191 flake.

## Validation Audit 2026-11-08

| Metric | Count |
|---|---|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

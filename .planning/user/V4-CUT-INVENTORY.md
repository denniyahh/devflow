# V4 Cut Inventory

**Written:** 2026-11-08, from the tree at `90a9a5e` (workspace/denniyahh). Decision aid for the v4.0 cut: every operator-facing
command, flag, stage, hook and background subsystem, with the evidence behind it and a proposed disposition.

**The v4.0 core (operator, 2026-11-08):** one detached agent launch under a supervisor, stream capture, result classification
(success / failed / rate-limited / idle timeout / torn), delivery canary and health checks; failover on quota denial (later any
stage, rotation, utilization-aware routing); shell-answerable gates with notification plus a new MCP decision tool (auto / Jev /
operator); phase worktree on a branch; event log; `status`, `stop`, `cleanup`, `doctor`. GSD owns the workflow.

**Tiers.** L = exercised with a real agent, record cited. E = integration test drives the real binary/supervisor with a stub agent
(`crates/*/tests/*.rs`). U = `#[test]` inside `src/` only. N = none found (search named). **E\*** = the test file contains the
command/flag literal but I did not isolate one named test for it; treat as weaker than E.
**Dispositions.** `core` keep as is; `core-rework` keep, changed as stated; `cut` delete; `defer` keep code, decide later.
"operator 2026-11-08" marks a disposition the operator already took; every other one is my proposal.

Live records cited: **[49]** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-LIVE-RUN.md` (+ `49-RUNBOOK.md`,
`49-evidence/attempt-3/`); **[50.1]** `.planning/phases/50.1-split-lock-state-handling-overhaul/50.1-LIVE-RUN.md` (+ `50.1-28-SUMMARY.md`,
`50.1-evidence/attempt-2/`); **[51]** `.planning/phases/51-rate-limit-agent-failover/51-RUN-NOTES.md`; **[probe]**
`.planning/audits/2026-11-08-live-failover-probe.md`; **[RM-N]** `.planning/ROADMAP.md` `### Phase 999.N` (dogfood records).

## 1. Commands and flags (`crates/devflow-cli/src/main.rs`)

| Item | Tier | Evidence | Proposed | Reason |
|---|---|---|---|---|
| `start --phase --agent --mode [PROJECT]` | L | [probe] `devflow start --phase 1 --agent claude --mode supervise --fallback-agent codex`; [49] attempt 3 shipped; [50.1] attempt 2 | core | The launch verb |
| `start --fallback-agent` | L | [probe] claude→codex handoff, `agent_handoff` event; E `failover_e2e.rs` `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code` | core | Failover is core |
| `start --until <stage>` | E | `phase7_cli.rs` `start_until_plan_halts_cleanly`; live only as `--dry-run` [RM-183] | core-rework | Keep as "stop after stage"; stage set shrinks |
| `start --no-worktree` | E | `phase7_cli.rs` `start_no_worktree_uses_feature_branch`; used by 11 e2e files | core | Test harness and non-worktree runs depend on it |
| `start --force` | E\* | literal in `start_lock_e2e.rs`, `phase7_cli.rs` | core | Replace-state path; cheap |
| `start --dry-run` | E | `phase7_cli.rs` `start_dry_run_annotates_until_stage`; [RM-183] live dry run | defer | Small; 999.183 vanishes with Define |
| `start --yes-ship` | L | [49] `ship gate answered by --yes-ship`; E `yes_ship_config.rs` `flag_overrides_false_config` | cut | operator 2026-11-08 (Ship) |
| `start --legacy-claude-launch` | E\* | literal in `supervisor_e2e.rs`, `start_lock_e2e.rs`, `test_home_isolation.rs` | cut | Pre-31 single-document launch; stream is core; 999.64 fixed |
| `start --worktree` (hidden) | E\* | literal in `phase7_cli.rs`; doc says "no-op kept one release" | cut | Deprecated no-op |
| `resume --phase [--agent] [--fallback-agent] [--legacy-claude-launch]` | L | [49] attempt 2 `resume.stdout` Ship relaunch after quota pause; E `start_lock_e2e.rs` `resume_prints_the_answer_it_set_aside`, `failover_e2e.rs` | core-rework | Keep "relaunch the stage"; drop the cron coupling and `--legacy-claude-launch` |
| `advance` (hidden) | E | `gate_sweep_e2e.rs` `sweep_ends_a_real_advance_process_through_its_own_abort_path`; `gate_wedge_e2e.rs` | defer | Supervisor advances in-process; the verb is a test shape, decide with the test rewrite |
| `__monitor` (hidden supervisor body) | L | every live launch ([50.1] 12 launches, `supervisor_superseded=0`); E `supervisor_e2e.rs` `start_publishes_a_live_supervisor_socket_for_each_launch_shape` | core | The supervisor |
| `gate list [--all-roots]` | U | `commands.rs` `gate_list_all_roots`, `gate_list_and_show_print_the_gate_id`; E\* literal in `stop_e2e.rs` | core | Decision channel |
| `gate approve <phase> [--stage] [--note] [--gate-id]` | L | [49] `49-RUNBOOK.md` L121/L129 `gate approve 1 --stage define`, `gate_response_written` in `attempt-3/.../events.jsonl`; [50.1] "approval through the CLI"; [51] Code gates approved | core | Decision channel |
| `gate reject <phase> --note [--stage] [--gate-id]` | L | [51] run end: abort-reject of Validate gate `18e6310585119d8e`; [49] runbook L242; E `gate_wedge_e2e.rs` `advance_self_resolving_arm_live_advance_consumes_the_code_rejection` | core-rework | Note must reach the agent (999.189); "abort" by word is fragile |
| `gate approve/reject` legacy positional `STAGE_OR_PROJECT`/`PROJECT` | N | `rg legacy_project` and `rg STAGE_OR_PROJECT` over `crates/devflow-cli/tests` → none; only `main.rs`/`commands.rs` | cut | Compat arg form, untested |
| `gate show <phase> [--stage]` | U | `commands.rs` `gate_show_renders_full_untruncated_sanitized_context` | core | Needed once reasons exceed 300 chars (999.206) |
| `gate sweep [--max-age-secs] [--dry-run] [--root]` | E | `gate_sweep_e2e.rs` `sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort` | core-rework | Keep aging; simplify once Ship gates are gone |
| `gate sweep --reap-strays` | E | `reap_strays_e2e.rs` `reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see` | cut | Matches 2.x process shapes only; 3.x never starts them |
| `logs [-f] [--phase]` | U | `commands.rs` `logs_follow_ends_when_the_followed_state_is_gone_and_no_exit_file_will_come`, `default_logs_phase_prefers_single_active_state` | core | Watching the capture |
| `logs --stderr` | N | `rg '"--stderr' crates/*/tests` → none; no `commands.rs` test names `stderr` | core | Trivial switch; add a test or fold into `logs` |
| `history [N]` | U | `devflow-core/src/history.rs` `timeline_orders_events_and_correlates_retained_captures`; `rg '"history"' crates/*/tests` → none | defer | A view over the event log; keep if cheap |
| `parallel --phases [--agents] [--mode] [--force]` | E | `phase7_cli.rs` `parallel_creates_two_worktrees_and_spawns_two_monitors`; `fallback_agent_config.rs` `parallel_ignores_the_config_key` | cut | operator 2026-11-08; one run per phase; GSD owns multi-phase; closes 999.181, retires 999.26 |
| `reference [--branch] [--refresh]` | E | `phase7_cli.rs` `reference_and_cleanup_worktree_cli_flow`; flags: `rg '"--branch'` and `rg '"--refresh'` over `crates/*/tests` → none | cut | A static snapshot worktree is `git worktree add` |
| `cleanup [--force]` | L | [probe] 999.213 (`cleanup` after `stop` misreports; `--force` leaves state); E `phase7_cli.rs` `cleanup_force_removes_worktree_for_until_stopped_phase` | core-rework | Fix 999.213: remove state, say why it keeps |
| `status` | L | [RM-186] `devflow status` after the real rate limit; [probe] 999.213; E `supervisor_e2e.rs` `status_reports_each_liveness_state_with_its_repair_verb` | core-rework | Show stop reason and fallback state (999.190, 999.202) |
| `list` | U | `devflow-core/src/git.rs` `list_feature_branches_reports_ahead_and_behind_semantics`; no e2e isolated | cut | `git branch -vv` does this |
| `recover [--clean] [--phase]` | E | `recover_clean_e2e.rs` `explicit_clean_of_a_phase_with_nothing_live_cleans_and_says_so`, `sweep_removes_and_names_an_orphan_cron_record` | core-rework | Fold `--clean` into `cleanup`, inspect into `doctor` |
| `test` | E | `ci_parity_guards.rs` `devflow_test_clippy_matches_ci_scope`; `test_cmd` itself has no unit test | cut | `scripts/check.sh` is the gate; a wrapper in the product is noise |
| `doctor [--json]` | L | [probe] 999.217 (red ✗ for unset `RUST_LOG`); E `doctor_json_e2e.rs` `doctor_json_names_the_open_gates_waiter`, `doctor_antigravity.rs` | core-rework | Drop non-DevFlow-project warnings (999.217); keep orphan sweep |
| `release --check` / `--verify` | E | `release_check.rs` `release_check_passes_when_pins_match`, `release_without_check_is_rejected` | cut | Release engineering belongs to `scripts/cut-release.sh`/wsx, not the runner |
| `ship --phase [--force]` | E | `stop_e2e.rs` `a_real_stop_lets_a_real_ship_finish_the_pass_it_started`; U `pipeline_gate.rs` `ship_override_advances_via_written_response` | cut | operator 2026-11-08 |
| `stop --phase [--launch] [--root]` | L | [probe] "First launch stopped with `devflow stop`"; [51] 12 mentions; E `stop_e2e.rs` `stop_never_signals_a_recorded_pid_it_addresses_the_socket`, `pending_supervisor_e2e.rs` `stop_ends_a_pending_supervisor_through_its_socket` | core | Listed core; 999.209 residual stays |
| `evidence --phase [--json] [--require-shipped] [--root]` | U | `commands.rs` `evidence_require_shipped_exits_ok_iff_the_phase_has_shipped`; `ship_evidence.rs` 7 tests; `rg '"evidence"' crates/*/tests` → none | cut | A `workflow_shipped` oracle; Ship is gone |

## 2. Environment variables (OPERATIONS.md table, L398-418, plus undocumented reads)

| Variable | Tier | Evidence | Proposed | Reason |
|---|---|---|---|---|
| `DEVFLOW_GATE_NOTIFY_CMD` (+ `DEVFLOW_GATE_PHASE/STAGE/CONTEXT`, `DEVFLOW_NON_SILENT_GATE`) | L | `notify_fired` x3 [49], x5 [50.1] `events.jsonl`; E `gate_notify_persist_e2e.rs` `advance_gate_runs_the_persisted_command_when_the_variable_is_unset` | core | Gate notification |
| `DEVFLOW_GATE_TIMEOUT_SECS` | E\* | literal in 6 e2e files; U `config_parse.rs` | core | Gate wait bound |
| `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` | U | `config_parse.rs` `sweep_threshold_is_not_shorter_than_the_gate_poll_timeout` | core | Sweep threshold |
| `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` | U | read in `pipeline_gate.rs`/`config_parse.rs` (file-level) | cut | `devflow ship` only |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | U | `lock.rs` `acquire_creates_lock_and_records_pid` (the lock it bounds) | cut | Serializes the after-ship hook batch only |
| `DEVFLOW_CACHE_DIR` | U | `registry.rs` `register_in_two_different_pairs_both_survive_and_load_sorted` | core | Machine-global root registry for `gate list --all-roots`/sweep |
| `DEVFLOW_CAPTURE_RETENTION` | L | `capture_archived` x11 [50.1]; U `config.rs` | core | Capture archive |
| `DEVFLOW_REVIEW_ANGLES` | U | read in `config.rs` (file-level) | cut | Ship review angles |
| `DEVFLOW_EXTERNAL_VERIFY_ENABLED`, `DEVFLOW_TRUST_EXTERNAL_VERIFY` | U | `verify.rs` `approval_parser_accepts_only_nonempty_json_command_arrays` | defer | Layer-0 probes are classification input; decide with the MCP channel |
| `DEVFLOW_BASE_BRANCH` (and `devflow.toml` `base_branch`) | L | [49] criterion 1 `base_branch is integration` (toml form); env form E\* `start_reachability_e2e.rs` | core | Worktree fork point |
| `DEVFLOW_YES_SHIP` (and toml `yes_ship`) | E | `yes_ship_config.rs` `config_true_no_flag_is_preauthorized_and_announces_source` | cut | operator 2026-11-08 (Ship) |
| `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`, `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` | U | `monitor.rs` `idle_timeout_does_not_fire_while_a_background_task_is_open`, `a_quota_denial_is_not_recorded_as_an_idle_timeout` | core | Idle-timeout verdict; no live idle timeout recorded |
| `DEVFLOW_CLAUDE_LEGACY_LAUNCH` | E\* | literal in `supervisor_e2e.rs` etc. | cut | With the flag |
| `DEVFLOW_LOG_FORMAT` | E | `log_format_env.rs` `rust_log_debug_is_honored_under_json_log_format` | core | Cheap |
| `DEVFLOW_DELIVERY_CANARY_*` (prefix, `canary.rs`) | L | `claude_delivery_canary_confirmed` [49][50.1]; U `canary.rs` `canary_confirmed_when_token_returns_in_a_top_level_result` | core | Canary |
| `DEVFLOW_BINARY_NAME` | U | read in `agent.rs` (file-level); not in OPERATIONS.md | defer | Test override |
| `DEVFLOW_TEST_*`, `DEVFLOW_PIPELINE_LAUNCH_*_ROOT`, `DEVFLOW_27_0*`, `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` | — | test-only hooks (named in `doc-check-allowlist.toml`/OPERATIONS) | defer | Follow their subjects |
| `DEVFLOW_CI_CPUS`, `DEVFLOW_ALLOW_OFF_WORKTREE` | — | `scripts/` only, not read by the binary | defer | Repo tooling, out of the product cut |

## 3. Stages, hooks and pipeline policy (`devflow-core/src/stage.rs`, `hooks.rs`, `ship.rs`, CLI `pipeline_*.rs`)

| Item | Tier | Evidence | Proposed | Reason |
|---|---|---|---|---|
| Define stage | L | [probe] 999.214: paid no-op, 12-15 s, ~$0.33, twice | cut | operator 2026-11-08 |
| Plan stage | L | [probe] (accepted pre-written plan, 8-9 s); [50.1] | core-rework | GSD owns the command; DevFlow runs "one stage command" |
| Code stage | L | [probe], [49], [50.1], [51] | core | The workload |
| Validate stage (`verdict: pass`/`gaps` loop-back) | L | [50.1] timeline: `gaps` x2, `loop_back`, `phase_validate_failures: 2` | core-rework | Loop-back decision to GSD/MCP channel (999.189/192/193/205/208) |
| Ship stage | L | [50.1] `workflow_shipped`, PR #1, tag `v0.1.0` | cut | operator 2026-11-08 |
| `DocsUpdate` hook (Validate→Ship, `cargo doc`) | L | `hook_run` x5 [50.1] = DocsUpdate + 4 after-ship | cut | Fires on the Ship transition |
| After-ship `Merge`, `VersionBump`, `ChangelogAppend`, `BranchCleanup` | L | [50.1] "four finalization hooks (each `ok: true`)", `merge_result merged: true` | cut | operator 2026-11-08 |
| Finalization retry gate | U | `pipeline_gate.rs` `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`; never fired live ([50.1] "no retry after a hook failure") | cut | With the hooks |
| `BranchCreate` hook / phase worktree on `feature/phase-NN` | L | every run; [probe] `feature/phase-01` | core | Listed core |
| Rate-limit failover (one hop, Code only) | L | [probe] classified `ratelimited`, `agent_handoff` claude→codex, codex finished Code | core-rework | Generalize to any stage + rotation (operator) |
| Cron-resume instructions (`cron-instructions-NN.json`, `hermes_cron`) | L | [RM-186]: written on a real rate limit, nothing acted, run died | cut | operator 2026-11-08; closes 999.185/186 |
| Modes `auto` / `supervise` | L | auto [49][50.1]; supervise [51][probe] | core-rework | Redefine without the Ship gate |
| Unattended-launch prerequisites (preflight D-07: config, launch shape, planned checkpoint) | L | [50.1] "Define gate (seeded preflight refusal) fires"; U `preflight.rs` `unattended_check_reports_every_condition_in_both_modes` | core-rework | Checkpoint policy moves to the MCP decision channel |
| Checkpoint auto-decide (`checkpoint_auto_decided` capture relaunch) | L | x1 [49], x3 [50.1] | core-rework | Replaced by the MCP tool; keep until it exists |
| Preflight: agent binary + driver health | L | [probe] "health check — Real Claude"; U `preflight.rs` `ensure_agent_binary_diagnoses_missing_program` | core | Listed core |
| Preflight: phase reachable on base, base ref currency | E | `start_reachability_e2e.rs` `start_refuses_a_phase_promoted_only_on_the_working_branch_and_scaffolds_nothing` | core | Worktree correctness |
| Preflight: `gh auth`, major-bump refusal | U | `preflight.rs` `run_preflight_major_bump_gates_and_never_ships_unattended` | cut | Ship-only concerns |
| Infra ceiling / `MAX_CONSECUTIVE_FAILURES` forced gate | U | `pipeline_launch.rs` `an_ambiguous_outcome_reads_the_infra_ceiling_from_the_committed_counter` | core-rework | Keep "never silent"; thresholds become policy |
| Outcome policy table (`outcome_policy.rs`) | U | `success_advances` | core-rework | Shrinks with the stage set |

## 4. Background subsystems and drivers

| Item | Tier | Evidence | Proposed | Reason |
|---|---|---|---|---|
| Supervisor socket + liveness (ALIVE/STALE/GONE, pending-launch `flock`, backstop) | L | [50.1] 12 launches `supervisor_superseded=0`; E `supervisor_e2e.rs` `sigterm_to_a_live_supervisor_ends_the_run_without_advancing`, `backstop.rs` U | core | Listed core; real takeover never driven live |
| Stream launch shape (pipes, idle timeout) | L | [50.1] 9 stream launches; U `monitor.rs` `pipe_owning_monitor_delivers_prompt_via_stdin_and_captures_stream` | core | Listed core |
| Capture launch shape (codex/opencode/checkpoint resume) | L | [probe] codex Code+Validate; [50.1] 3 capture launches; E `supervisor_e2e.rs` `capture_supervisor_owns_a_fake_agent_and_records_devflow_result` | core | Non-Claude agents |
| Result classification: success / failed / rate-limited | L | [RM-186] real Claude denial → `ratelimited`; [probe] synthetic denial | core | Listed core; fixtures lag live shape (999.212) |
| Result classification: idle timeout, torn capture | U | `monitor.rs` `the_idle_timeout_record_carries_the_last_result_cause`; `agent_result.rs` `blocking_human_checkpoint_reported_false_when_init_is_torn` | core | Never seen live; 999.207/209 open |
| Delivery canary | L | `claude_delivery_canary_confirmed` [49][50.1]; E `failover_e2e.rs` `a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate` | core-rework | Re-runnable per agent (999.194) |
| Gate records + root registry + set-aside answers | L | `gate_fired/resolved` x5, `gate_claimed_answer_removed` x5 [50.1]; E `start_lock_e2e.rs` `resume_prints_the_answer_it_set_aside`; U `gates.rs` 96 tests | core | Decision channel substrate |
| Events log `events.jsonl` | L | every record above; U `events.rs` `emit_appends_parseable_lines_with_envelope_fields` | core | Listed core |
| Capture archive (`history/`, retention) | L | `capture_archived` [49][50.1]; U `history.rs` | core | Evidence of each launch |
| Build provenance (`build.rs`) + staleness self-dogfood block | L | [50.1] `build_dirty=true` recorded; E `build_provenance.rs` `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` | core-rework | Show the commit in `--version` (999.216) |
| `doc_check` source-scan tests | U | `doc_check.rs` `source_devflow_env_vars_and_subcommands_are_documented`, `allowlist_entries_require_reasons` | core-rework | Keep; allowlist and docs shrink with the cut |
| Auto-chain flag writer (`gsd_config.rs`, `.planning/config.json`) + leak repair | E | `auto_chain_flag_e2e.rs` `auto_mode_code_stage_child_observes_the_flag_set`; `auto_chain_leak_repair_e2e.rs` | defer | GSD owns the workflow; decide with `gsd-autonomous` |
| Legacy Claude launch path | E\* | see `--legacy-claude-launch` | cut | Above |
| `parallel` orchestration (`parallel.rs`) | E | see `parallel` | cut | operator 2026-11-08 |
| Shared checkout lock (`lock.rs`) | U | `acquire_creates_lock_and_records_pid` | cut | Serves the hook batch only |
| Version detection (`version.rs`), `ship_evidence.rs`, `ship.rs` | U | `version.rs` `detect_prefers_cargo_then_pyproject_then_package_json` | cut | Serve VersionBump / Ship only |
| Durable file ops (`durable.rs`) | U | `replace_fsyncs_the_file_and_its_directory` | core | State and gate records |
| Planning-doc staleness report (`doctor` `planning_doc_staleness`) | U | `staleness.rs` 30 tests | defer | GSD's concern; cheap to keep |
| Graphify hooks | N | `rg -l graphify crates/` → none; only `scripts/cut-pr-branch.sh` and the operator's global post-commit hook | defer | Not in the binary; repo tooling |
| Repo hooks (`scripts/hooks/`: gitleaks, branch guard, signing/container pre-push, bashism lint) | E | `pre_commit_secret_scan.rs` `a_commit_is_refused_when_gitleaks_reports_a_finding`; `pre_push_signing_policy.rs` | defer | Repo policy, not product; untouched by the cut |
| Driver `claude` | L | all live runs | core | Primary |
| Driver `codex` | L | [probe] Code + Validate on codex 0.167.0 | core | Fallback proven once |
| Driver `antigravity` | E | `failover_e2e.rs` `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code`; `phase7_cli.rs` `antigravity_parses_devflow_result_from_stream` | core | Only other unattended (stream) shape; never live |
| Driver `opencode` | U | `agents/opencode.rs` `probe_proc_snapshot_separates_a_live_process_from_a_reaped_one` (30 tests); no e2e (`rg opencode crates/devflow-cli/tests/*.rs` → none) | defer | Never live; rotation may want it |
| Drivers `pi`, `hermes` | E | `phase7_cli.rs` `pi_hung_process_is_detected_not_left_running`, `hermes_marker_less_run_does_not_advance` | defer | Never live; keep only if rotation needs them |

## 5. Counts

Counted by parsing the table rows above (96 rows, each with five cells).
Rows: 96 (commands/flags 34, env 18, stages/hooks 19, subsystems 25).
Tier: L 40, E 26 (of which E\* 6), U 25, N 3, test-only/scripts (no tier) 2.
Disposition: core 36, core-rework 19, cut 28, defer 13. Operator-decided cuts: 7 rows (Define, Ship, the four after-ship hooks, cron file, `ship`, `--yes-ship`, `DEVFLOW_YES_SHIP`); the other 21 cuts are proposals.

## 6. Backlog entries (`### Phase 999.N`, N ≥ 180) a `cut` closes outright

- **Define cut:** 999.183 (dry run says Define runs discuss), 999.214 (Define is a paid no-op).
- **Ship + after-ship hooks + `ship` cut:** 999.180 (approved Validate gate not closed before Ship), 999.210 (no way to end a supervise run at Validate except abort).
- **Cron instructions cut:** 999.185 (superseded supervisor writes the cron file), 999.186 (nothing acts on the cron file).
- **`parallel` cut:** 999.181 (`stop` signal to `parallel` stops later phases).
- Not closed by any cut (still real in the core): 999.187, 189, 190, 192-209, 211-213, 215-217. 999.192/193/205/208 move to GSD's side of the seam under the Validate rework but are not closed by deletion.

## 7. What this inventory does not establish

- **L means "ran at least once with a record", not "works reliably".** Phase 49 is one shipped attempt after two that did not (`49-LIVE-RUN.md` "Verdict"); 50.1 attempt 2 was not unattended; the failover probe is one run with a **synthetic** denial; real denials were classified live ([RM-186] and eight Phase 43 verdicts in `events.jsonl`) but no raw capture of one survives (999.212). Nothing here was run twice under the same conditions.
- **A test's existence is not its strength.** E\* rows cite a literal in a file, not a behaviour. Counts of `#[test]` (e.g. `gates.rs` 96) say nothing about what the tests discriminate; no mutation or coverage data was consulted. 999.188/191 record two e2e tests that fail under full-suite load.
- **Not inventoried:** `devflow.toml` keys other than `base_branch`, `yes_ship`, `fallback_agent`; the 6 agent drivers' internal command shapes; `.devflow/` file formats (OPERATIONS.md L420-445); GSD-side behaviour.
- **Dispositions are proposals except where marked.** "core" here means "the operator's core list names it or it is a precondition of something named"; it is not a claim the code is right (999.209, 999.213, 999.215 are open against core rows).
- **The test-file mapping is by literal match.** A command literal like `"list"` or `"test"` in a test file can be a different command (`gate list`) or a word; where I could not isolate a test I said so.

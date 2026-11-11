---
phase: 52-subtract-and-stabilize
plan: 15
subsystem: pipeline
tags: [cut-02, cut-04, ship-stage, stage-machine, prompt, config, clap, semver]

requires:
  - phase: 52-14
    provides: nothing pre-authorizes a Ship gate, so handle_ship_outcome was the last caller of run_gate_with_timeout's auto_response
provides:
  - "`Stage` is Define → Plan → Code → Validate; `\"ship\"` does not parse and the parse error offers only kept stages"
  - "`StageIntent::Ship`, the Ship prompts, `SHIP_REVIEW_ANGLES` and `FixType::AuditFix` are gone"
  - "`DEVFLOW_REVIEW_ANGLES`, `config::review_angles`, `DevflowConfig::review_angles` and its toml key are gone; a leftover `review_angles` key in devflow.toml is ignored"
  - "`devflow start --until ship` is refused by clap as an invalid value"
  - "CLI help names no Ship stage and no cron arm, pinned by `help_snapshot::help_names_no_removed_surface`"
  - "`handle_ship_outcome`, `handle_ship_failure`, `is_ship_review_failure`, `run_gate_with_timeout`'s `auto_response` and safe point 5 are gone"
  - "`gate approve`/`reject` write nothing at any gate with no registered waiter (the Ship bypass is gone)"
affects: [52-16, 52-17, 52-18, 52-26, 52-27, 52-33]

actuals:
  tokens: 49920
  tasks: 3
  commits: 3
plan_head_before: e1d5f3a83907e23654e5c56495d8743c146da216
plan_head_after: d8d8b9f32f9f19dabd6e3671a8526b2441af8f27

tech-stack:
  added: []
  patterns:
    - "A removed-value list the next cut plan extends is a named slice const (`REMOVED_UNTIL_STAGES`, like `REMOVED_SUBCOMMANDS`), not an inline array literal: clippy's `single_element_loop` refuses a one-element literal loop"
    - "A test that used the Ship gate's no-waiter bypass to make an answer land now registers a waiter (`Gates::open_registered`) and finishes the registration at its end"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/stage.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/help_snapshot.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - crates/devflow-cli/tests/stop_e2e.rs
    - ARCHITECTURE.md
    - OPERATIONS.md
    - README.md

key-decisions:
  - "FixType::AuditFix is deleted: rg found one producer, handle_ship_failure's review loop-back. The persisted-fix tests that used it now use GapsOnly."
  - "StageIntent::for_stage_in_project and prompt::stage_prompt_for_project are kept with a project_root nothing reads: 52-26, 52-27 and 52-VALIDATION.md name the one for_stage_in_project( call the source scan pins, and removing a pub fn is a semver change the plan did not ask for."
  - "run_gate_with_timeout's auto_response parameter and safe point 5 are removed here, as 52-14's deferred item proposed; safe points 4 and 6 keep their numbers."
  - "The Code prompt's 'the authoritative review happens during Ship' is reworded to 'this self-check is not the authoritative review of the phase', and the 8 prompt snapshots that carry it are updated."

patterns-established:
  - "When a removed stage was a test's stand-in, the test moves to Validate (or Code where Validate is already taken in the same fixture); when the removed stage's special case was the subject, the test is deleted"

requirements-completed: [CUT-02, CUT-04]

coverage:
  - id: D1
    description: "`\"ship\"` does not parse as a Stage, and the parse error lists only define, plan, code and validate"
    requirement: CUT-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/stage.rs#stage::tests::removed_stage_names_do_not_parse"
        status: pass
    human_judgment: false
  - id: D2
    description: "`devflow start --until ship --dry-run` is an `invalid value` clap error; `--until plan|code|validate --dry-run` exit 0"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#until_rejects_removed_stages"
        status: pass
    human_judgment: false
  - id: D3
    description: "`devflow --help`, `start --help` and `resume --help` name no Ship and no cron arm, and the committed help snapshot matches"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_names_no_removed_surface"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D4
    description: "DEVFLOW_REVIEW_ANGLES and the review_angles key are gone from source and scoped docs; a leftover key in devflow.toml is ignored"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#config::tests::a_leftover_review_angles_key_is_ignored"
        status: pass
    human_judgment: false
  - id: D5
    description: "The e2e suites and the whole check suite pass without the Ship stage"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p devflow --test stop_e2e / gate_wedge_e2e / log_format_env / gate_sweep_e2e"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 45min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 15: Remove the Ship Stage Summary

**The stage machine is Define → Plan → Code → Validate: `Stage::Ship`, `StageIntent::Ship`, the Ship prompts and review angles, `FixType::AuditFix`, the Ship outcome and failure handlers and the auto-answered gate path are deleted, clap refuses `--until ship`, and no help output names Ship.**

## Performance

- **Duration:** about 45 min
- **Started:** 2026-11-11T01:18:25Z
- **Completed:** 2026-11-11T02:04Z
- **Tasks:** 3
- **Files modified:** 42 (546 insertions, 1476 deletions)

## Accomplishments

- Core: `Stage::Ship` and every core arm (`next`, `is_gate`, `gsd_command`, `Display`, `FromStr`, the parse error text), `StageIntent::Ship`, `ship_stage_prompt`, `workflow_ship_prompt`, `ship_pr_base_instruction`, `SHIP_REVIEW_ANGLES`, `FixType::AuditFix` and its prompt arms, `DevflowConfig::review_angles` (field, Default entry, accessor), `config::review_angles` (the only `DEVFLOW_REVIEW_ANGLES` read) and `Mode::should_gate`'s Ship arm are deleted. `gates::STAGES` has four entries.
- CLI: `handle_ship_outcome`, `handle_ship_failure`, `is_ship_review_failure`, the `--until ship` refusal and the Ship entries of `STREAM_JSON_STAGES`, `close_gates_for_resume` and `set_aside_leftover_gates` are deleted. `refuse_a_gate_with_no_waiter` and doctor's open-gate repair lost their Ship special case. `run_gate_with_timeout` lost `auto_response`, the safe-point-5 check and the auto-answer write.
- Help: `start`'s summary is `Begin the workflow for a phase: Define → Plan → Code → Validate.`, `--mode` says `auto` runs unattended and finishes on a passing Validate, and `--until` no longer mentions `ship`. The `--fallback-agent` help already named no cron arm (52-09 rewrote it), and `stop`'s line already named no Ship gate (52-11). The snapshot diff is that one `start` line.
- Docs: the review-angles mentions went in the same commit as the code (OPERATIONS.md row, README toml line and env sentence, the `StageIntent` bullet, two ARCHITECTURE.md mentions, the configuration guide's toml line and row). Task 3 rewrote the stage lists and diagrams in README, OPERATIONS and ARCHITECTURE, ARCHITECTURE's "Git and ship model" (now "Git model"), the `--mode` rows, the dogfood session example, the quickstart's Shipping section, CONTRIBUTING's review-before-Ship sentence and the unattended-mode guide's "through Ship".

## Task Commits

1. **RED for Tasks 1 and 2:** `6ac5493` (test)
2. **Tasks 1 and 2 (core, then CLI), one commit after Task 2's gate passed:** `65f4bf0` (feat!)
3. **Task 3: e2e suites and docs:** `d8d8b9f` (feat!)

**Plan metadata:** the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch nextest config, `--profile red`). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `stage::tests::removed_stage_names_do_not_parse` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on its planned assertion (`expect_err` at stage.rs:173): `"ship"` parsed as `Stage::Ship`. Not a build or load fault. |
| `phase7_cli::until_rejects_removed_stages` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on the `invalid value` assertion (phase7_cli.rs:545): stderr held DevFlow's own refusal, `--until ship is a no-op: Ship is already the pipeline's terminal stage`, so clap had accepted the value. The `plan`/`code`/`validate` controls were not reached in RED. |
| `help_snapshot::help_names_no_removed_surface` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | The planted-line controls passed (both 3.0.0 lines and the cron-arm line were reported; `shipped`, `relationship`, `finished` were not), then it failed at help_snapshot.rs:122 because `devflow --help` named `Ship` in the start summary. |

Gate sequence: `test(52-15)` 6ac5493 → `feat(52-15)!` 65f4bf0 → `feat(52-15)!` d8d8b9f. No REFACTOR commit. The RED commit holds all three tests (deviation 1).

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode `end-of-phase`, and Task 1's verify is automated-only. Its chain was run before the commit and passed after the doc fix in deviation 3; expansion then continued.

## Tests deleted or changed

- **Deleted, subject was Ship:**
  - core `prompt`: `ship_prompt_sequences_code_review_before_ship`, `ship_prompt_defines_critical_gate_and_review_failed_contract`, `ship_prompt_instructs_pr_targeting_main`, `ship_prompts_name_the_trunk_and_no_fixture_branch`, `ship_prompt_includes_multi_angle_conditional_review`, `ship_prompt_uses_project_review_angle_override`; core `mode::ship_always_gates_in_both_modes`; core `config::env_overrides_file_review_angles`.
  - CLI `pipeline_outcomes`: `ship_loop_back_still_issues_gaps_only_when_verification_absent`, `ship_agent_failed_fires_gate`, `ship_review_failed_loops_to_code`, `ship_review_failed_uses_audit_fix`, `handle_ship_outcome_writes_gate_but_no_response`.
  - CLI `pipeline_gate`: `an_approved_ship_gate_finishes_the_run_without_running_a_hook`, `a_pre_authorized_answer_left_unconfirmed_still_decides_its_own_gate`, `a_stopped_auto_answered_run_sends_no_ship_notify`, with helpers `auto_answered_ship_gate`, `notify_marker` and `RecordedSupervisor`.
  - CLI `commands::gate_approve_with_no_waiter_at_the_ship_gate_still_writes`; e2e `phase7_cli::start_until_ship_is_rejected` (replaced by `until_rejects_removed_stages`).
- **Assertions removed from kept tests:** the Ship line in `pipeline_launch::resume_reports_the_answers_it_set_aside_for_the_closed_gate`; the Ship segment of `commands::gate_respond_answers_a_registered_waiter_and_refuses_when_none_is_registered`; the AuditFix rows of `prompt::fix_prompts_select_the_right_command`, `fix_prompts_carry_the_chain_flag_token_only_where_it_reaches_execute_phase`, `code_policy_is_absent_from_prompts_that_must_not_carry_it` and `claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it`; the Ship row in `agents::tests::workflow_render_preserves_stage_contracts`.
- **Re-pointed from Ship (or AuditFix) to a kept value, subject unchanged:**
  - CLI `commands`: `gate_sweep_preview_uses_the_no_waiter_matrix`, `gate_list_and_show_print_the_gate_id`, `gate_approve_arg_parsing_takes_the_stage_option`, `gate_show_arg_parsing_accepts_phase_and_optional_stage`, `gate_show_renders_full_untruncated_sanitized_context`, `gate_show_errors_asking_for_stage_with_several_open_gates` (Code + Validate), `gate_show_auto_resolves_single_open_gate`, `gate_sweep_dry_run_does_not_write_a_response`, `gate_sweep_skips_already_responded_gate_without_clobbering`, `gate_sweep_emits_gate_reaped_event_on_reap`, `status_shows_pending_gate_prominently`, `all_roots_row_includes_gate_with_non_numeric_timestamp`, `recovery_hints_for_a_gone_gate_pending_phase_name_resume_not_advance`.
  - CLI `commands`, now with a registered waiter because no gate stores an answer without one: `gate_respond_auto_resolves_single_open_gate`, `gate_respond_requires_stage_when_ambiguous_and_errors_when_none_open`.
  - CLI `commands`, renamed: `stop_via_gate_with_no_waiter_at_a_non_ship_gate_writes_nothing` → `stop_via_gate_with_no_waiter_writes_nothing`, `gate_sweep_leaves_a_no_waiter_non_ship_gate_alone` → `gate_sweep_leaves_a_no_waiter_gate_alone`, `gate_respond_with_no_registered_waiter_at_a_non_ship_gate_writes_nothing` → `gate_respond_with_no_registered_waiter_writes_nothing`, `stop_at_a_non_ship_gate_with_an_unconfirmable_holder_refuses_success` → `stop_at_a_gate_with_an_unconfirmable_holder_refuses_success`, `stop_reaches_the_holder_past_a_claimed_ship_gate` → `stop_reaches_the_holder_past_a_claimed_gate` (helper `claimed_ship_gate` → `claimed_gate`).
  - CLI `pipeline_gate::a_foreground_claim_gives_up_on_a_held_write_lock` (fixture `ship_ready_fixture` → `validate_parked_fixture`).
  - CLI `pipeline_outcomes::grafted_failure_shape_gates_instead_of_shipping`: "never advance to Ship" became `assert!(!state.finished)`.
  - CLI `pipeline_launch`: `auto_chain_eligibility_is_code_and_auto_mode_only`, `recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome`, `failover_target_selects_the_fallback_only_at_code`; `a_code_failover_during_an_audit_fix_pass_replays_audit_fix` → `a_code_failover_during_a_gaps_only_pass_replays_gaps_only` and `a_resume_mid_audit_fix_replays_audit_fix` → `a_resume_mid_gaps_only_pass_replays_gaps_only` (GapsOnly instead of AuditFix).
  - CLI `preflight::unattended_check_does_not_apply_outside_define_and_code`, `staleness::dirty_flag_arm_ignores_non_build_files_but_still_flags_sources`.
  - Core: `gates` (14 `Stage::Ship` stand-ins), `workflow::clear_only_touches_its_own_phase`, `workflow::list_states_enumerates_sorted_by_phase`, `recover::write_phase_artifacts` (Code + Validate), `agent_result` Layer 2 stage loop, `state` `code_fix` round-trip (GapsOnly), `events` and `history` fixtures (`ship` → `validate`, D-30), `mode::consecutive_reset_fires_on_every_other_transition`, the two conformance counts in `agents` (7 → 6), `outcome_policy::idle_timeout_is_never_auto_resumed` (5 → 4 stages), `recover::a_recover_decisions_worst_case_is_counted_on_its_real_paths` (the control's `gate * 4` became `gate * (STAGES.len() - 1)`, the same bound).
  - e2e: `stop_e2e::stop_says_a_claimed_gate_was_not_answered_again`, `stop_marks_state_stopped_and_records_reason`, `gate_approve_takes_the_id_gate_list_shows_and_refuses_a_replaced_one` (g2 now registered), `stop_against_a_hand_written_response_is_a_success_no_op`; `gate_wedge_e2e::dry_run_sweep_reports_no_waiter_gates_as_left_alone`; `gate_sweep_e2e::sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort`; `log_format_env::rust_log_unset_still_shows_info_level_logs_by_default` (registered waiter).
  - e2e `stop_e2e::gate_approve_with_no_registered_waiter_does_not_claim_advance`: its subject was the stored Ship answer; it now asserts the refusal (non-zero exit, no answer file, `no registered waiter` and `no response was written` on stderr, no "workflow will advance").
- **Added:** `stage::tests::removed_stage_names_do_not_parse`, `phase7_cli::until_rejects_removed_stages` (with `REMOVED_UNTIL_STAGES`), `help_snapshot::help_names_no_removed_surface` (with `REMOVED_HELP_TERMS`), `config::tests::a_leftover_review_angles_key_is_ignored` (deviation 4).

## Decisions Made

See `key-decisions`. `cargo semver-checks check-release -p devflow-core` (baseline: published 3.0.0) exits 100 with 9 major lints, run before the `65f4bf0` commit. This plan's items among them: `enum_variant_missing` for `Stage::Ship`, `StageIntent::Ship` and `FixType::AuditFix`; `enum_no_repr_variant_discriminant_changed` for `FixType::GapsOnly` (1 → 0) and `FixType::FullExecute` (2 → 1), a lint new in this plan; `function_missing` for `config::review_angles`; `inherent_method_missing` for `DevflowConfig::review_angles`; `struct_pub_field_missing` for `DevflowConfig.review_angles`. All are intended (D-01). The other lints come from items earlier plans removed.

## Deviations from Plan

**1. [Convention] One RED commit for Tasks 1 and 2, and `feat(52-15)!` subjects**
- **Issue:** the plan writes RED per task and names `feat(core)!: remove the Ship stage`. Task 2's RED tests need the CLI built, and after Task 1 the CLI does not build, so they could only run RED before any implementation. GSD's TDD gate looks for `feat(52-15)`.
- **Fix:** the three RED tests were written and recorded together on the untouched tree and committed as `6ac5493`. Subjects are `feat(52-15)!: remove the Ship stage` and `feat(52-15)!: drop the Ship stage from the e2e suites and docs`, each with a `BREAKING CHANGE:` footer.

**2. [Rule 3 - Blocking] The Task 1+2 commit leaves four e2e test targets uncompilable until Task 3**
- **Issue:** the plan puts the e2e suites in Task 3, but `gate_sweep_e2e.rs` (in no task's file list), `log_format_env.rs`, `gate_wedge_e2e.rs` and `stop_e2e.rs` use `Stage::Ship`. The build errors for the first three were observed before Task 3's edits; `stop_e2e.rs` had the same uses.
- **Fix:** the plan's split was kept. At `65f4bf0` the bin, the core lib and the two Task 2 test targets build and pass; those four e2e targets do not compile. `d8d8b9f` fixes them, and `gate_sweep_e2e.rs` was added to Task 3.

**3. [Rule 3 - Blocking] ARCHITECTURE.md's Ship prose had to land with the code**
- **Issue:** Task 1's `doc_check::doc_referenced_identifiers_exist_in_source` gate failed with "documented Rust identifier `handle_ship_outcome` does not exist in Rust source": ARCHITECTURE.md named it twice (the `advance()` bullet and the "Git and ship model" section), which the plan left to Task 3.
- **Fix:** ARCHITECTURE.md's whole Ship rewrite went into `65f4bf0`. README.md, OPERATIONS.md and the configuration guide were staged hunk by hunk, so `65f4bf0` holds only their review-angles lines and Task 3 holds their Ship prose. The failing run is the negative control: it passes after the fix.

**4. [Rule 2 - Missing check] A 3.x devflow.toml with `review_angles` must still load**
- **Issue:** the plan pins no behaviour for a leftover key. The operator's projects may carry it, and `DevflowConfig` has no `deny_unknown_fields`, so it loads, but nothing pinned it.
- **Fix:** `config::tests::a_leftover_review_angles_key_is_ignored`, the same shape 52-14 used for `yes_ship`. The `fallback_agent` parse-error test that used a malformed `review_angles` line now uses a malformed `capture_retention` line. No migration code.

**5. [Rule 1 - Consistency] The Code prompt told agents a Ship review follows**
- **Issue:** `CODE_STAGE_POLICY` said "the authoritative review happens during Ship", which every Code prompt delivers. It names no `Stage::Ship` or `"ship"`, so no scan sees it.
- **Fix:** reworded to "this self-check is not the authoritative review of the phase". 7 core snapshots and 1 CLI snapshot (`turn_one_code_prompt`) changed by that one line; the CLI one was found by the bin suite failing on it.

**6. [Scope] Deferred items handled here, and the two `pub` prompt functions kept**
- **Handled from `deferred-items.md`:** the stored-but-unconsumed Ship gate answer (the bypass and its tests) and `run_gate_with_timeout`'s `auto_response` with safe point 5. Both are marked resolved there.
- **Kept, with a parameter nothing reads:** `StageIntent::for_stage_in_project` and `prompt::stage_prompt_for_project` (see key-decisions); logged as a deferred item.

**7. [Rule 1 - Lint] clippy refused the one-element loop in `until_rejects_removed_stages`**
- **Issue:** `scripts/check.sh all` failed on `clippy::single_element_loop` at `for removed in ["ship"]`.
- **Fix:** the list is `const REMOVED_UNTIL_STAGES`, which 52-17 extends. The fix was amended into the unpushed `65f4bf0` so no commit carries the lint failure.

**8. [Plan premise] Help text and snapshot files**
- **Issue:** the plan expected `--fallback-agent` help naming a cron arm and a `stop` clause naming a Ship gate, and ship-prompt `.snap` files to delete. None existed: 52-09 and 52-11 had already rewritten those help lines, and `git ls-files '*.snap'` lists 9 snapshots, none for a Ship prompt (the Ship prompt tests used `assert!`, not insta).
- **Fix:** nothing to change for those; `help_names_no_removed_surface` pins that both terms stay absent.

**Total deviations:** 8 (1 convention, 2 blocking, 1 missing check, 2 consistency/lint, 1 scope, 1 plan premise). **Impact:** one extra test; the comment and prompt rewording is text only; no behaviour beyond the plan.

## Issues Encountered

- New deferred items logged in `deferred-items.md`: the capture-retention arithmetic still counts a five-stage run (fix after 52-17); `devflow doctor` checks for `gh`, which nothing runs (README and quickstart now say so); prose comments and three test names still describe Ship as the stage after Validate (for 52-18); and the kept `project_root` parameter.
- The post-commit hook warned that `CONTRIBUTING.md` changed without `DEV-SETUP-CHECKLIST.md`. The change was the review-enforcement sentence, not a setup step, so the checklist was not changed.

## Verification

- Task 1 `<verify>` chain: exit 0 after deviation 3 (the core lib had 1015 run, 1015 passed, 2 skipped). Before the fix it failed only on `doc_referenced_identifiers_exist_in_source`.
- Task 2 `<verify>` chain: exit 0; the three named tests `1 passed` each; `--bin devflow` 598 run, 598 passed, 4 skipped. An earlier run failed `the_gate_rule_holds_in_both_delivered_turns` on the CLI snapshot (deviation 5) and passed after.
- Task 3 `<verify>`: `stop_e2e` 41 passed, `gate_wedge_e2e` 5, `log_format_env` 3, `gate_sweep_e2e` 5, both `doc_check` tests `1 passed`. `scripts/check.sh all` printed "all OK" with exit 0: 44 `test result: ok` lines (as in 52-14) and no `FAILED` or `panicked`. The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) passed; no rerun was needed. The first `check.sh` run failed on the clippy lint in deviation 7.
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0 after deviation 7.
- Absence checks: `rg -n 'REVIEW_ANGLES|review_angles|review angle'` over README, OPERATIONS, ARCHITECTURE, CONTRIBUTING, `docs/guides` and the allowlist exits 1; the same search found 7 lines before the edit. `rg 'Stage::Ship|StageIntent::Ship|AuditFix' crates` finds nothing.
- What this does not establish:
  - The gates ran on the working tree, not on each commit's own tree. At `65f4bf0` the working tree also held the Task 3 edits, which none of the Task 1 and 2 targets compile; `doc_check` reads README and OPERATIONS, whose committed versions at `65f4bf0` hold only Ship prose with no backticked identifier (checked with `git show`). The full suite ran on the tree committed as `d8d8b9f` plus `deferred-items.md`.
  - `check.sh all` ran on this host, not in the pinned container; the pre-push hook runs the container check.
  - No live `devflow start` was made. The `--until` refusal is shown by the clap test, the dry runs by `--dry-run`.
  - A 3.x state file whose stage is `ship` now fails to load with the parse error (T-52-18, accepted under the no-migration rule). No test pins that message.
  - The `rg` absence checks are shell checks; the comment-stripped source scan comes in 52-18, and prose mentions of Ship remain (logged).

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-16 and 52-17, which remove Define. `REMOVED_UNTIL_STAGES` and `REMOVED_HELP_TERMS` are the lists 52-17 extends with `define`/`Define`.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

- This SUMMARY exists; no file was planned for creation.
- Commits 6ac5493, 65f4bf0 and d8d8b9f are ancestors of HEAD.
- `check evaluation-scope --plan 52-15 --commits-only` resolved the plan's three commits on this branch.

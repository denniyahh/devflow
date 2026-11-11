---
phase: 52-subtract-and-stabilize
plan: 16
subsystem: pipeline
tags: [cut-01, define-stage, stage-machine, preflight, doctor, never-prepared]

requires:
  - phase: 52-15
    provides: the stage machine is Define → Plan → Code → Validate, with no Ship stage
provides:
  - "`State::new` starts a run at Plan; `devflow start --dry-run` lists exactly plan, code, validate"
  - "`run_never_prepared(state)` is `!state.verification_baseline_captured` alone: no stage and no event-log read"
  - "doctor's `check_missing_branch` skips a run only when `PhaseFacts.unprepared_restart` is set, and still takes only `&PhaseFacts`"
  - "start's CONTEXT.md probe (`start_requires_phase_context`) asks the driver about Plan"
  - "preflight's first-stage checks key on Plan: the auto-mode CONTEXT.md check for a `RequiresExistingArtifact` driver, the D-07 unattended-launch check (now Plan and Code), and its not-yet-planned case"
  - "every test whose outcome depended on a fresh run starting at Define starts at Plan; `Stage::Define` still compiles and nothing starts or enters it"
affects: [52-17, 52-18]

actuals:
  tokens: 20800
  tasks: 3
  commits: 2
plan_head_before: 65f8abc3f305240cd3ec70ddcd1158141aaf9924
plan_head_after: cc68322a2bcf37e91d3ff79d4a01574104c59fd8

tech-stack:
  added: []
  patterns:
    - "A fixture that models a prepared run sets `verification_baseline_captured = true`; only a fixture that models a start killed during preparation leaves it false"
    - "A handoff fixture whose subject is stage N → N+1 names its stage N explicitly (`state.stage = Stage::Define`) rather than relying on where `State::new` starts"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-cli/tests/fallback_agent_config.rs
    - crates/devflow-cli/tests/pending_supervisor_e2e.rs
    - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-cli/tests/gate_sweep_e2e.rs
    - crates/devflow-cli/tests/start_lock_e2e.rs
    - crates/devflow-cli/tests/stop_e2e.rs
    - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs

key-decisions:
  - "preflight.rs's three `Stage::Define` behaviour sites were re-keyed to Plan in this plan, though preflight.rs is not in its file list: the plan's acceptance criterion forbids a behaviour comparison to Define, and leaving them would make 52-17's deletion silently drop the launch-time unattended refusal and the CONTEXT.md check."
  - "A Codex handoff or resume at Plan in auto mode is now refused (D-07 refuses a capture-shape agent, and the CONTEXT.md check applies at the first stage). In 3.0.0 the same refusals happened at Define. `resume_with_agent_allows_plan_stage` and `resume_clears_stop_marker_and_advances_past_stop_point` moved to supervise mode to keep their subjects."
  - "The pipeline_launch handoff fixture (`HandoffRun::launch_with`) keeps stage N = Define, set explicitly, instead of moving ~45 tests to N = Plan, N+1 = Code. 52-17 must move it when it deletes the variant."
  - "start's probe is a new private fn `start_requires_phase_context(&dyn AgentDriver)`, extracted in the RED commit with its old behaviour so the stage it asks about is testable with a fake driver."
  - "`cargo semver-checks` was not run: no pub item or signature changed. `State::new`'s starting stage is a behaviour change in devflow-core."

patterns-established:
  - "Tests that used Plan as the 'unaffected' stage for preflight checks now use Validate, because Plan is the first stage and gets the first-stage checks"

requirements-completed: [CUT-01]

coverage:
  - id: D1
    description: "`devflow start --dry-run --phase 1 --mode supervise` lists exactly plan, code, validate; the comparison refuses an empty, a two-stage and the 3.0.0 output"
    requirement: CUT-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_dry_run_lists_exactly_plan_code_validate"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_dry_run_annotates_until_stage"
        status: pass
    human_judgment: false
  - id: D2
    description: "`State::new` starts at Plan"
    requirement: CUT-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::new_state_starts_at_plan"
        status: pass
    human_judgment: false
  - id: D3
    description: "'Never launched' is the run's own flag: a fresh state is never prepared, a prepared Plan run is not, and an earlier run's events do not change that; doctor skips the missing-branch warning only for a never-prepared run; start's probe asks about Plan"
    requirement: CUT-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::run_never_prepared_is_true_for_a_fresh_state_only"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::run_never_prepared_ignores_an_earlier_runs_events"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::check_missing_branch_skips_a_never_launched_run_only"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::start_context_probe_asks_the_driver_about_plan"
        status: pass
    human_judgment: false
  - id: D4
    description: "The whole suite passes with runs starting at Plan and Define still compiled"
    requirement: CUT-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p devflow --test supervisor_e2e / fallback_agent_config / failover_e2e / gate_notify_persist_e2e / pending_supervisor_e2e (the plan's Task 3 gates)"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 57min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 16: Start Every Run at Plan Summary

**A fresh run starts at Plan and its dry run lists exactly plan, code and validate. "Never launched" now means the run never captured its verification baseline, at any stage. Preflight's first-stage checks moved from Define to Plan, and every test that relied on a fresh run starting at Define was moved, so `scripts/check.sh all` passes with `Stage::Define` still compiled.**

## Performance

- **Duration:** about 57 min
- **Started:** 2026-11-11T02:14:22Z
- **Completed:** 2026-11-11T03:11Z
- **Tasks:** 3
- **Files modified:** 16 (473 insertions, 197 deletions across both commits)

## Accomplishments

- `State::new` sets `stage: Stage::Plan`, and `print_dry_run` starts at Plan. The dry run of this tree for phase 1 prints `plan`, `code`, `validate` and nothing else.
- `run_never_prepared` is `!state.verification_baseline_captured`. It reads no event file. Its doc says why neither the stage nor the event log can decide this.
- `check_missing_branch` returns `None` when `facts.unprepared_restart.is_some()`, not when `facts.stage == Stage::Define`. It still takes only `&PhaseFacts`. The doc now says "past preparation".
- start's CONTEXT.md probe goes through `start_requires_phase_context`, which asks the driver about `Stage::Plan`. Codex declares both Define and Plan `RequiresExistingArtifact`, so `codex` without a CONTEXT.md is still refused at start (`phase7_cli::start_codex_without_context_fails_preflight` passes).
- preflight: `preflight_interactivity_check` refuses a `RequiresExistingArtifact` driver in auto mode without CONTEXT.md at Plan; `unattended_launch_check_applies` is Plan and Code; `unattended_planned_checkpoint_condition` treats "no plans yet" as pending at Plan.

## Task Commits

1. **RED for Tasks 1 and 2:** `65cc99c` (test)
2. **Tasks 1, 2 and 3, one commit after Task 3's gate and `scripts/check.sh all` passed:** `cc68322` (feat!)

**Plan metadata:** the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch nextest config, `--profile red`). Each record went through `gsd_run check tdd-red-evidence`, and every verdict was `RED_EVIDENCE_OK` / `target_test_failed` at exit 100.

| Test | Tree it failed on | Semantic assessment |
|------|-------------------|---------------------|
| `phase7_cli::start_dry_run_lists_exactly_plan_code_validate` | untouched | The three planted controls passed first. It then failed at phase7_cli.rs:801 on the exact-list assertion: `listed ["define", "plan", "code", "validate"]`. |
| `phase7_cli::start_dry_run_annotates_until_stage` | untouched | Failed at the new three-stage list assertion (phase7_cli.rs:844), with the same listed value. The `--until` assertions were not reached. |
| `state::tests::new_state_starts_at_plan` | untouched | `left: Define, right: Plan` at state.rs:783. |
| `commands::tests::run_never_prepared_is_true_for_a_fresh_state_only` | Task 1 applied | The fresh-state assertion failed (commands.rs:6779): with `State::new` at Plan, `stage == Define && …` returned false. |
| `commands::tests::run_never_prepared_ignores_an_earlier_runs_events` | Task 1 applied | The on-disk-events control passed first. The fresh-state assertion then failed (commands.rs:6820). |
| `commands::tests::check_missing_branch_skips_a_never_launched_run_only` | Task 1 applied | Failed at the `is_none()` assertion (commands.rs:6837): stage Plan is not Define, so the Warn finding came back. |
| `commands::tests::start_context_probe_asks_the_driver_about_plan` | Task 1 applied | Failed at the Plan assertion (commands.rs:6883): the probe asked about Define. |

The two `run_never_prepared` tests and the probe test pass on the untouched tree, because a fresh state was at Define there. Their RED was recorded with Task 1 applied, as the plan says. Gate sequence: `test(52-16)` 65cc99c → `feat(52-16)!` cc68322. No REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode `end-of-phase`, and Task 1's verify is automated-only. Its chain was re-run after GREEN and exited 0, and expansion continued.

## Behaviour sites: Define dispositions

**Re-keyed (behaviour):**
- `crates/devflow-core/src/state.rs` `State::new`: Plan.
- `crates/devflow-cli/src/pipeline_gate.rs` `print_dry_run`: starts at Plan.
- `crates/devflow-cli/src/commands.rs`: `run_never_prepared`, `check_missing_branch`, the start probe.
- `crates/devflow-cli/src/preflight.rs`: `preflight_interactivity_check`, `unattended_launch_check_applies`, `unattended_planned_checkpoint_condition` (deviation 1).

**`recover.rs` and `agent_result.rs`:** all 7 Define uses are in tests, and each names Define explicitly as a stand-in. Left for 52-17:
- `recover.rs` L1261/L1263: `clean_phase_report_preserves_earlier_gate_removal_when_later_gate_removal_fails`, the early gate.
- `recover.rs` L2148: run B's gate.
- `agent_result.rs` L8358 and L8782: `state.stage = Stage::Define` in two external-verify tests.
- `agent_result.rs` L9900 and L9924: the Layer 2 stage loops.

**Left for 52-17 (variant arms and stage lists, not comparisons):** `stage.rs` arms, `prompt.rs` intent mapping, `agents/codex.rs` interactivity arm, `agents/mod.rs` contract loop, `commands.rs` `set_aside_leftover_gates` list, `pipeline_launch.rs` `STREAM_JSON_STAGES`, its gate-id list and the `Stage::Define => transition(.., Stage::Plan)` arm.

**Test-only Define uses that remain**, by file (`rg -c 'Stage::Define|"define"|stage define'` after the commit):
- `pipeline_launch.rs` 45: the handoff fixture's explicit stage N and the tests built on it; `auto_chain_flag_eligible` and `failover_target` stage tables; `advance_with(.., Some(Stage::Define))` mismatch tests on explicit Define states.
- `commands.rs` 23: doctor JSON and gate tests that open a Define gate explicitly; the `set_aside_leftover_gates` list.
- `preflight.rs` 3: the Define control in `preflight_interactivity_check_flags_auto_plan_without_context_md` and in `unattended_check_does_not_apply_outside_plan_and_code`.
- e2e: `doctor_json_e2e.rs` 3 (an explicit Define gate), `failover_e2e.rs` 2 (the `stage define` stub branch and its doc), `stop_e2e.rs` 1 (a planted Define gate file), `phase7_cli.rs` 1 (the 3.0.0 control literal).
- core: the counts listed above, plus `gates.rs` 3, `mode.rs` 2, `outcome_policy.rs` 1, `events.rs` 1.

## Tests migrated (old → new expectation)

**Named by the plan:**
- `supervisor_e2e::finishing_run_advances_for` → `--until plan` became `--until code`; the expected launches went from `define`, `plan` to `plan`, `code`.
- `supervisor_e2e::capture_supervisor_owns_a_fake_agent_and_records_devflow_result` → `--until define` became `--until plan`.
- `fallback_agent_config::config_key_alone_is_persisted_in_the_run_state` → `--until define` became `--until plan`.
- `failover_e2e::a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code` → invocations `canary, stage define, stage plan, stage code` became `canary, stage plan, stage code`.
- `failover_e2e::after_the_refused_relaunch_status_and_resume_lead_to_another_agent` → the same shift (no `stage define`).
- `pending_supervisor_e2e::spawn_held_launcher` → held at `define` became held at `plan`.
- `gate_notify_persist_e2e`: `agent_environment_after_define` was renamed `agent_environment_after_plan_gate` and approves `--stage plan`. `spawn_parked_start_with` waits for the Plan gate.
- `commands::tests::phase_validate_failures_survive_a_forced_restart` → the fresh stage is Plan.
- `commands::tests::resume_refuses_a_start_killed_during_preparation` → the unchanged stage is Plan.
- `commands::tests::stop_signals_a_hand_run_advance_through_its_phase_holder` → `advance_command(.., Some(Stage::Plan))`; the stage stays Plan.
- `pipeline_launch::tests::advance_refuses_a_stopped_state` → `stop_until` and `advance_with` use Plan.
- `pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state` → the full-entry mismatch moves the state to Code and expects `expected: "plan"`.

**Further failures caused by the start-stage change** (the plan's "migrate any further failure" rule):
- `pipeline_launch` `HandoffRun::launch_with` (used by about 45 handoff, promotion, supersession and safe-point tests): it relied on a fresh run starting at Define with `stop_until = Plan`. It now sets `state.stage = Stage::Define` explicitly; nothing else changed.
- **Prepared-run fixtures that never set `verification_baseline_captured`.** Before this plan, `stage != Define` alone made such a state read as prepared; now `resume` refuses it. These now set the flag:
  - pipeline_launch tests: `resume_clears_stop_marker_and_advances_past_stop_point` (also moved to supervise, see below), `resume_reports_a_claimed_pair_whose_gate_file_is_gone`, `resume_with_agent_hands_off_and_relaunches_under_the_new_driver`, `resume_without_agent_leaves_the_saved_agent_untouched`, `resume_with_agent_refuses_auto_mode_handoff_that_would_fail_the_later_unattended_launch_check`, `resume_with_same_agent_is_an_ordinary_idempotent_resume`, `resume_with_agent_allows_plan_stage`, `resume_with_agent_preserves_every_state_field_except_agent`, `resume_with_agent_from_a_rate_limited_state_relaunches`, `resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_run_whose_supervisor_exited`, `a_hand_run_advance_refuses_while_a_supervisor_of_the_phase_is_live`, `resume_commits_its_unstop_and_keeps_a_concurrent_field`, `resume_re_marks_stopped_when_launch_stage_fails_outright`.
  - pipeline_launch helpers: `seed_gate_with_pending_answer`, `save_stopped_code_run`, `build_rescan_fixture`, `resume_code_run`, `code_run_recording`.
  - commands tests: `recovery_hints_for_a_gone_gate_pending_phase_name_resume_not_advance`, `doctor_reconciliation::doctor_reports_gate_pending_without_gate_file`.
  - e2e: `gate_notify_persist_e2e::spawn_parked_advance`, `gate_wedge_e2e::spawn_gated_advance`, `gate_sweep_e2e::sweep_ends_a_real_advance_process_through_its_own_abort_path`, `stop_e2e::stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent`, `start_lock_e2e::plant_leftover_run` and `plant_resumable_run`, and `auto_chain_leak_repair_e2e`'s state fixture.
- **A fresh start that parks at its first-stage gate** now parks at Plan:
  - `gate_wedge_e2e`: the parked-start helpers and two tests were renamed. `reject_define` → `reject_plan`, `define_gate_id` → `plan_gate_id`, `assert_parked_start_is_the_define_gates_waiter` → `…_plan_gates_waiter`, `start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers` → `…_a_plan_gate_nothing_answers`, `start_self_resolving_arm_live_start_consumes_the_define_rejection` → `…_the_plan_rejection`.
  - `start_lock_e2e::start_clears_leftover_gate_files_after_reserving`: the planted legacy answer is `-plan.response.json` and the test waits for the Plan gate.
  - `commands::tests::a_signal_to_a_start_parked_at_a_preflight_gate_keeps_the_resumable_state`: the Plan gate.
  - `stop_e2e::stop_after_a_supervisor_killed_at_a_gate_reaps_and_reports_gone`: the Plan gate.
  - `stop_e2e::stop_with_a_launch_id_tears_down_only_that_socket`: no Plan answer.
- **First-stage hold or stop:**
  - `commands::tests` `start_refuses_…` and `resume_refuses_over_a_pending_supervisor_whose_launcher_died`: hold `define:` → `plan:`.
  - `pending_supervisor_e2e::handoff_window`: `start("plan", --until plan)` → `start("code", --until code)`.
  - `stop_e2e::LiveRun`: `--until plan` → `--until code`, so its V-17 control (`without_stop_a_lock_free_run_completes_and_advances`) still sees a second launch.
- **The stage stays at the first stage:** `phase7_cli` `pi_nonzero_exit_does_not_advance`, `pi_hung_process_is_detected_not_left_running`, `hermes_nonzero_exit_does_not_advance` and `hermes_hung_process_is_detected_not_left_running` (6 assertions) expect Plan, not Define.
- `phase7_cli::start_until_plan_halts_cleanly`: a fresh run already sits at Plan, so the persisted stage no longer proves Plan ran. The test now also asserts a `stage_launched` event for `plan`.
- **preflight tests** (deviation 1):
  - Define → Plan as the first stage: `preflight_interactivity_check_flags_auto_define_without_context_md` (renamed `…_auto_plan_…`; Define is now its "not the first stage" control), `preflight_interactivity_check_probes_the_runs_persisted_base`, `run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`, `run_preflight_advance_skips_recheck_on_idempotently_failing_check`, `run_preflight_loopback_bounds_recursion`, `generic_preflight_checks_surfaces_the_unattended_reason_alongside_an_earlier_failure` (expects `cannot run the plan stage headlessly`).
  - Renamed: `unattended_check_does_not_apply_outside_define_and_code` → `unattended_check_does_not_apply_outside_plan_and_code`, and `unattended_check_treats_an_unplanned_phase_as_pending_at_define_and_undetermined_at_code` → `…_pending_at_plan_…`.
  - Plan → Validate as the stage the generic checks leave alone: `run_preflight_adapter_hook_override_fires`, `run_preflight_advance_gate_launches_agent_exactly_once`, `run_preflight_loopback_gate_launches_agent_exactly_once`, `preflight_retries_reset_on_pass`. The last two hung on an unanswered Plan gate before this change.
- **Moved to supervise mode** (Codex at Plan in auto is now refused): `pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point`, `resume_with_agent_allows_plan_stage`. `resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage` moved from Define to Plan.

## Decisions Made

See `key-decisions`.

## Deviations from Plan

**1. [Rule 2 - Missing critical] preflight.rs's Define behaviour sites re-keyed to Plan**
- **Found during:** Task 2, checking the acceptance criterion "no behaviour site in `crates/*/src` compares a stage to `Stage::Define`"
- **Issue:** preflight.rs is in no task's file list, but it has three such sites: the auto-mode CONTEXT.md check, `unattended_launch_check_applies` (Define and Code) and the "no plans yet is pending at Define" case. With runs starting at Plan, the D-07 unattended check would no longer run at launch, only at Code. 52-17's deletion would then drop these arms silently.
- **Fix:** the three sites and their docs now name Plan. 14 preflight tests were moved (listed above). A side effect: a Codex handoff or resume at Plan in auto mode is now refused, as it was at Define before.
- **Files modified:** crates/devflow-cli/src/preflight.rs, plus the tests listed above
- **Verification:** all 60 `preflight::` tests pass, the bin suite passes, and `scripts/check.sh all` passes
- **Committed in:** cc68322

**2. [Convention] One RED commit for Tasks 1 and 2, and a `feat(52-16)!` subject**
- **Issue:** the plan names the commit `feat(cli)!: start every run at Plan` and asks for one commit for Tasks 1 to 3. GSD's TDD gate looks for `test(52-16)` and `feat(52-16)`.
- **Fix:** the RED tests were committed as `65cc99c`. It builds; its new tests fail by design, and its two `run_never_prepared` tests pass until Task 1 is applied. That commit also extracts `start_requires_phase_context` with its old behaviour (asking about Define), so the probe test fails on an assertion, not a build error. The implementation for all three tasks is one commit, `cc68322 feat(52-16)!: start every run at Plan`, with a `BREAKING CHANGE:` footer.

**3. [Rule 3 - Blocking] The start-interactivity assertion is a new unit test, not a line in the existing start test**
- **Issue:** the plan asks to "assert in the existing start interactivity test" that the probe asks about Plan. The existing test is `phase7_cli::start_codex_without_context_fails_preflight`, an e2e run of Codex, and Codex declares Define and Plan alike, so no assertion there can tell which stage was asked.
- **Fix:** `commands::tests::start_context_probe_asks_the_driver_about_plan` uses a fake driver that needs an artifact at exactly one stage, with a Define-only driver as the control.

**4. [Rule 3 - Blocking] Plan-mandated e2e command uses `--mode supervise`**
- **Issue:** the behaviour text says `devflow start --dry-run --phase 1`, but clap requires `--mode` for `start` (exit 2, `the following required arguments were not provided: --mode <MODE>`, observed on this tree).
- **Fix:** the test runs `start --dry-run --phase 1 --mode supervise`.

---

**Total deviations:** 4 (1 missing critical, 2 blocking, 1 convention)
**Impact on plan:** deviation 1 widens the plan's scope by one source file and 14 tests, and it changes one behaviour (deciding note 2). The rest are wording and commit shape.

## Issues Encountered

- **A runaway nextest process is still running. I started it, and I was not permitted to stop it.** Before the preflight tests moved to Validate, a plain `cargo nextest run -p devflow --bin devflow` (no slow-timeout) hung on two of them, each waiting on an unanswered Plan gate with the 7-day default gate timeout. Its PIDs: nextest `165039`, test processes `191765` (`preflight_retries_reset_on_pass`) and `191941` (`run_preflight_loopback_gate_launches_agent_exactly_once`). Two `kill` attempts on those recorded PIDs were refused by the auto-mode classifier. They hold no lock this repo uses, but they will run until killed. Later runs used a scratch nextest profile with `slow-timeout = { period = "20s", terminate-after = 3 }`.
- `commands::tests::status_and_doctor_name_a_held_state_write_lock` (999.221) did not fail in this plan's `scripts/check.sh all` run, so no rerun was needed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 52-17 can delete `Stage::Define`. The handoff fixture's explicit `state.stage = Stage::Define` and the preflight Define controls are the stand-ins it must move. Compiler errors will point at each of them.
- `--until define` still parses and can never fire. No test relies on it.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

- Both commits are on `feature/phase-52` (`git merge-base --is-ancestor` for 65cc99c and cc68322).
- The named gates of Tasks 1, 2 and 3 each exit 0, and `scripts/check.sh all` exits 0 (`check.sh: all OK`). All were run after the last source edit, before `cc68322`.
- No file was created. All 16 modified files exist.

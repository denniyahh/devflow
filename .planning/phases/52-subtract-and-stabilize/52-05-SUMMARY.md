---
phase: 52-subtract-and-stabilize
plan: 05
subsystem: cli
tags: [rust, devflow-cli, devflow-core, launch-shape, breaking-change, tdd]

requires:
  - phase: 52-subtract-and-stabilize
    provides: "52-04: the e2e suites reach the capture shape through a fake codex, not the legacy flag"
provides:
  - "`devflow start` and `devflow resume` reject `--legacy-claude-launch` (clap: unexpected argument)"
  - "`DEVFLOW_CLAUDE_LEGACY_LAUNCH` is read nowhere; `config::claude_legacy_launch` and `State.legacy_claude_launch` are gone from devflow-core"
  - "`stream_launch_enabled(agent: AgentKind, stage: Stage) -> bool`: Claude and Antigravity stream at every stream stage, with no opt-out"
  - "`MonitorLaunch::Legacy` (capture shape) unchanged; codex, opencode, pi, hermes and `relaunch_checkpoint_session` still use it"
  - "`test_support::go_waiting_agent_dir(program)` replaces `go_waiting_claude_dir()`"
affects: [52-17, release-4.0.0]

actuals:
  tokens: 41942
  tasks: 2
  commits: 3
plan_head_before: a2bb981a14dfec65a277675319e3be2087560e96
plan_head_after: 9574b7c0cab94706fe8c48a95f101e3c7312da7b

tech-stack:
  added: []
  patterns:
    - "A unit test that needs the capture shape runs `AgentKind::Codex` with a `codex` stub on the child PATH"
    - "A unit test whose subject needs Claude as the primary (rate-limit envelope, claude-to-codex handoff) keeps Claude and records `state.canary = Some(Confirmed)`, so the stream launch runs no canary"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/agents/claude.rs
    - OPERATIONS.md
    - ARCHITECTURE.md
    - docs/guides/unattended-mode.md

key-decisions:
  - "Tests where Claude as the primary agent is the subject (fallback re-arm, rate-limit failover, the HandoffRun fixture) keep Claude and pre-record a Confirmed canary instead of moving to Codex: the rate-limit envelope is Claude-format and the assertions name the claude-to-codex handoff. This is the suite's existing pattern (preflight.rs, pipeline_launch.rs launch_stage_inner_resets_checkpoint_resumes_counter)"
  - "The `commands::start` test helpers run Codex, and `init_start_repo` now commits a phase CONTEXT.md, because `start` refuses Codex without one; a Claude start cannot pre-record a canary (start builds a fresh state)"
  - "The breaking marker is a `BREAKING CHANGE:` footer on `feat(52-05): ...`, not `feat(cli)!:`, so the GSD TDD gate's `^feat\\(52-05\\):` match still finds the GREEN commit"
  - "`resume_canary_refusal` is deleted; `recovery_hints` and `resume` both call `recorded_canary_refusal` directly, so they still judge through one function"
  - "The dead Claude arm of `resolve_launch_shape` and `ClaudeDriver::exec_command_single_document` are kept (plan: keep every capture-shape path); their comments now say nothing reaches them through `launch_stage` today"

patterns-established:
  - "Capture-shape unit test: `AgentKind::Codex` plus `agent_free_dir_with_agent_stub(\"codex\")` or `go_waiting_agent_dir(\"codex\")`"

requirements-completed: [CUT-04]

coverage:
  - id: D1
    description: "`start` and `resume` reject --legacy-claude-launch"
    requirement: CUT-04
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_flags_are_unknown"
        status: pass
    human_judgment: false
  - id: D2
    description: "stream_launch_enabled takes two parameters: claude and antigravity true at every stream stage, codex/opencode/pi/hermes false"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::stream_launch_includes_antigravity_on_stream_stages"
        status: pass
    human_judgment: false
  - id: D3
    description: "The unit tests that borrowed the flag keep their assertions on Codex (or Claude with a recorded canary); legacy-subject tests are deleted"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "cargo nextest run -p devflow --bin devflow --no-fail-fast (673 run, 673 passed)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Docs no longer name the flag or the env var; doc_check passes both directions"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 31min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 05: Remove the legacy Claude launch path Summary

**`--legacy-claude-launch`, `DEVFLOW_CLAUDE_LEGACY_LAUNCH` and `State.legacy_claude_launch` are gone. Claude always takes the stream launch, `stream_launch_enabled` has two parameters, and the capture shape still serves codex, opencode, pi and hermes.**

## Performance

- **Duration:** 31 min
- **Started:** 2026-11-10T19:51:10Z
- **Completed:** 2026-11-10T20:22:54Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments

- The flag is gone from `start` and `resume`, along with the env read in `config.rs`, the `State` field, `LegacyLaunchSource`, `apply_legacy_launch_opt_out`, `forced_legacy_launch_notice`, `announce_forced_legacy_launch` and `resume_canary_refusal`. No shim was left behind.
- `stream_launch_enabled(agent, stage)` is now the only launch-shape predicate for the launch, the canary gate and the unattended preflight.
- The 35 test sites that set the field were each migrated or deleted (table below). The full CLI unit suite passes: 673 run, 673 passed.
- The `DEVFLOW_CLAUDE_LEGACY_LAUNCH` row is removed from OPERATIONS.md. The flag is removed from OPERATIONS.md, ARCHITECTURE.md and docs/guides/unattended-mode.md.

## Task Commits

1. **Task 1 (tracer), RED:** `ad7066d` test(52-05): require clap to reject --legacy-claude-launch
2. **Task 1 (tracer), GREEN:** `39f6edc` feat(52-05): remove the legacy Claude launch path (`BREAKING CHANGE:` footer)
3. **Task 2:** `9574b7c` docs(52-05): drop the legacy Claude launch from docs and comments

Task 1's commit also holds the test-site migration. The plan asked for this, because `cargo build --workspace --tests` cannot pass while any test still sets the deleted field.

## TDD Gate Compliance

- **RED** (`ad7066d`): `REMOVED_FLAG_INVOCATIONS` gained `start --phase 1 --legacy-claude-launch --dry-run` and `resume --phase 1 --legacy-claude-launch`. The test was run with `cargo nextest run --profile red -p devflow --test phase7_cli --no-tests=fail -E 'test(=removed_flags_are_unknown)'`, using JUnit from a scratch nextest profile. It exited 100. `gsd check tdd-red-evidence` returned `RED_EVIDENCE_OK` (`target_test_failed`, junit, 1 test, 1 fail). Semantic assessment: the target test ran and failed on its planned assertion. The binary parsed the flag, and clap complained only about the missing `--mode`, so `unexpected argument '--legacy-claude-launch'` was absent.
- **GREEN** (`39f6edc`): `removed_flags_are_unknown` passed (exit 0), and `cargo build --workspace --tests` exited 0.
- **Not covered by RED:** the second `<behavior>` bullet, the two-parameter `stream_launch_enabled` test. It could not be written red, because a call with two arguments does not compile against the three-parameter function. It was rewritten in the GREEN commit (`stream_launch_includes_antigravity_on_stream_stages`) and passes.
- **Tracer gate:** the run was interactive, in `end-of-phase` mode, with an `<automated>`-only verify. The tracer verify was re-run and passed before Task 2 started.
- **MVP+TDD gate:** satisfied for Task 1, with a `test(52-05)` commit touching `tests/`, RED evidence OK and no earlier `feat(52-05)`. Task 2 has no `<behavior>` block, so the gate does not apply to it.

## Test-site inventory

**Count at the start commit `a2bb981`:** 35 field sites (`state.legacy_claude_launch = true`): 23 in pipeline_launch.rs, 10 in preflight.rs, 1 in commands.rs and 1 in pipeline_gate.rs. The plan counted 36 at b8df457. The difference is the `tests/start_lock_e2e.rs` site (~L1005), which 52-04 removed in `a0077fd`. Lines are at `a2bb981`.

| # | File:line | Enclosing test or helper | Disposition |
|---|-----------|--------------------------|-------------|
| 1 | pipeline_launch.rs:4486 | launch_stage_persists_the_supervisor_record_for_reload | migrated to Codex |
| 2 | pipeline_launch.rs:4608 | launch_publishes_the_socket_path_under_the_cache_dir_in_state | migrated to Codex |
| 3 | pipeline_launch.rs:5057 | resume_clears_stop_marker_and_advances_past_stop_point | migrated to Codex |
| 4 | pipeline_launch.rs:5116 | seed_gate_with_pending_answer (helper; 5 resume gate tests) | migrated to Codex |
| 5 | pipeline_launch.rs:5501 | resume_reports_a_claimed_pair_whose_gate_file_is_gone | migrated to Codex |
| 6 | pipeline_launch.rs:5652 | resume_preserves_unfired_until_cap | migrated to Codex |
| 7 | pipeline_launch.rs:5726 | resume_without_a_cap_is_unchanged | migrated to Codex |
| 8 | pipeline_launch.rs:5827 | resume_without_agent_leaves_the_saved_agent_untouched | migrated to Codex |
| 9 | pipeline_launch.rs:5933 | resume_with_same_agent_is_an_ordinary_idempotent_resume | migrated to Codex |
| 10 | pipeline_launch.rs:6155 | save_stopped_code_run (helper; 4 fallback re-arm tests) | Claude kept, canary recorded Confirmed |
| 11 | pipeline_launch.rs:6213 | resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run | Claude kept, canary recorded Confirmed |
| 12 | pipeline_launch.rs:7015 | a_validate_launch_commits_its_dispatch_stamp | migrated to Codex |
| 13 | pipeline_launch.rs:7155 | canary_gate_only_applies_to_the_stream_launch_path | migrated to Codex (the discriminator is now the agent, with Claude as the control) |
| 14 | pipeline_launch.rs:9689 | legacy_launch_skips_the_delivery_canary | deleted (legacy subject; #13 covers capture-skips-canary) |
| 15 | pipeline_launch.rs:10109 | HandoffRun::launch_with (helper; ~15 handoff/failover tests) | Claude kept, canary recorded Confirmed |
| 16 | pipeline_launch.rs:11728 | save_rate_limited_code_run (helper; 4 failover tests) | Claude kept, canary recorded Confirmed |
| 17 | pipeline_launch.rs:13619 | a_next_stage_spawn_that_fails_clears_its_pending_record | migrated to Codex |
| 18 | pipeline_launch.rs:13852 | a_launched_supervisor_holds_its_liveness_lock_after_the_launcher_drops_it | migrated to Codex |
| 19 | pipeline_launch.rs:13973 | a_failed_monitor_spawn_clears_the_supervisor_record | migrated to Codex |
| 20 | pipeline_launch.rs:14253 | resume_code_run (helper; a_resume_mid_audit_fix_replays_audit_fix) | migrated to Codex (argv read from the `codex` stub; Codex takes the prompt in argv) |
| 21 | pipeline_launch.rs:14471 | PublishFixture::spawn (3 publish-save tests) | migrated to Codex |
| 22 | pipeline_launch.rs:16696 | resume_commits_its_unstop_and_keeps_a_concurrent_field | migrated to Codex |
| 23 | pipeline_launch.rs:17279 | code_run_recording (helper; cron/ordinary resume refusal tests) | migrated to Codex |
| 24 | preflight.rs:2307 | run_preflight_advance_gate_launches_agent_exactly_once | migrated to Codex |
| 25 | preflight.rs:2383 | run_preflight_loopback_gate_launches_agent_exactly_once | migrated to Codex |
| 26 | preflight.rs:3050 | a_supervise_code_refusal_approval_records_the_pre_agent_set_once | migrated to Codex |
| 27 | preflight.rs:3142 | assert_approving_a_refusal_never_widens_a_recorded_set (helper; 3 approving_* tests) | migrated to Codex |
| 28 | preflight.rs:4385 | unattended_check_does_not_apply_outside_define_and_code | line dropped (the state was already Codex) |
| 29 | preflight.rs:4407 | unattended_check_reports_every_condition_in_both_modes | migrated: the failing condition is now `agent = Codex` |
| 30 | preflight.rs:4508 | unattended_check_refuses_a_legacy_or_non_claude_launch_shape | legacy arm deleted; renamed `unattended_check_refuses_a_non_stream_launch_shape`; the Codex/OpenCode arms are kept |
| 31 | preflight.rs:4599 | unattended_check_reports_but_does_not_refuse_in_supervise_mode | migrated: the failing condition is now `agent = Codex` |
| 32 | preflight.rs:4788 | unattended_launch_shape_condition_claude_unchanged | legacy arm deleted; the "Claude holds" half is kept |
| 33 | preflight.rs:4835 | unattended_launch_shape_refusal_names_the_capture_supervisor | the legacy state is replaced by a Pi state; the Codex state is kept |
| 34 | commands.rs:16514 | second_launch_refuses_over_a_dead_launchers_supervisor | migrated to Codex |
| 35 | pipeline_gate.rs:3202 | a_transition_clears_the_persisted_code_fix | migrated to Codex (`waiting_claude_dir` became `waiting_codex_dir`) |

Totals: 25 migrated to Codex, 4 kept on Claude with a recorded `Confirmed` canary, 1 deleted, and 5 partly edited (a legacy arm dropped or the failing condition swapped). Total 35, which equals the start-commit count.

**Other uses the field count did not include.** The compiler surfaced these, and each was handled:
- `commands::start(.., legacy_claude_launch = true, ..)` in 5 test helpers: `start_for_test`, `start_with_fallback`, `start_again_with_force`, `start_as_refusal_names`, and the direct call in `a_signal_to_a_start_parked_at_a_preflight_gate_keeps_the_resumable_state`. The first three of these plus `start_as_refusal_names` now run Codex. `start_with_fallback` takes its agent from the caller: `start_persists_the_fallback_agent_in_the_run_state` became Codex primary with a Claude fallback, and its printed line was swapped. The other two callers are refused before any launch and were left alone. The parked-start test keeps Claude, because its preflight gate fires before the canary.
- `resume(.., true)` / `resume(.., false)`: all 32 calls dropped the argument. That includes `commands::tests::launch_for_test` and four HandoffRun tests that passed `true`.
- Deleted legacy-subject tests: `legacy_launch_flag_forces_the_single_document_path`, `legacy_launch_is_off_by_default`, `legacy_launch_use_is_recorded_in_provenance`, `legacy_launch_env_var_is_parsed_as_a_bool`, `resume_does_not_clear_a_persisted_legacy_launch`, `stream_launch_includes_antigravity_ignores_claude_legacy_opt_out` and `commands::tests::recovery_hints_judge_the_environment_legacy_opt_out_as_resume_does`. The helpers `LegacyEnvOverride` and `legacy_state` were also deleted. With site #14, that makes 8 deleted tests.
- `parse_failure_does_not_trigger_a_fallback` was kept. It lost its env-var mutation (and the `#[expect(clippy::disallowed_methods)]` that came with it), and it lost its check for the removed `claude_legacy_launch_forced` event.
- `recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome` no longer loops over a legacy bool. It still enumerates every agent, stage and outcome.
- 52-04 listed no flag-subject e2e tests to delete ("None was kept"), so `tests/start_lock_e2e.rs` needed no change.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The plan's per-site rule did not fit four Claude-primary fixtures**
- **Found during:** Task 1
- **Issue:** `save_stopped_code_run`, the cron-paused re-arm test, `HandoffRun::launch_with` and `save_rate_limited_code_run` exist to test a Claude primary that fails over to a Codex fallback. The rate-limit capture is Claude-format, and the assertions name `claude → codex`. Moving them to Codex breaks their subject. Deleting them would remove failover coverage that has nothing to do with the legacy launch.
- **Fix:** keep Claude and record `state.canary = Some(Confirmed)`, which was the actual reason the field was set ("a stub `claude` cannot answer the stream launch's canary"). These tests now launch Claude on the stream shape.
- **Verification:** all 673 CLI unit tests pass.
- **Committed in:** 39f6edc

**2. [Rule 3 - Blocking] `start`-based test helpers passed the flag, and Codex needs a CONTEXT.md**
- **Found during:** Task 1 (compiler)
- **Fix:** the helpers run Codex, and `init_start_repo` commits `.planning/phases/NN-fixture/NN-CONTEXT.md`. The 12 parent PATH stubs became `go_waiting_agent_dir("codex")`.
- **Committed in:** 39f6edc

**3. [Rule 1 - Stale docs] Comments this removal made false**
- **Fix:** the comments in `resolve_launch_shape`, `canary_gate`, `ClaudeDriver::exec_command_single_document` (devflow-core `agents/claude.rs`) and its reachability test, `AutoChainGuard`'s doc (it referenced the deleted `LegacyEnvOverride`) and several test comments were reworded. `claude.rs` is outside the plan's file list.
- **Committed in:** 39f6edc, 9574b7c

**4. [Scope] Files outside `files_modified`:** `parallel.rs` (passed the flag to `start`), `test_support.rs` (`go_waiting_agent_dir`) and `agents/claude.rs` (see 3). `tests/start_lock_e2e.rs` was listed but needed no change.

**5. [Convention] The breaking marker is a footer, not `feat(cli)!:`**
- **Why:** the GSD TDD gate looks for `^feat\(52-05\):`. A `!` in the subject would hide the GREEN commit from it. The `BREAKING CHANGE:` footer is the other conventional-commit form of the marker.

**Total deviations:** 5 (2 blocking, 1 stale docs, 1 scope, 1 convention). **Impact:** test churn was larger than the plan's field count suggested, but every assertion was kept apart from the deleted legacy-subject ones.

## Verification

- Task 1: `cargo nextest run -p devflow --test phase7_cli --no-tests=fail -E 'test(=removed_flags_are_unknown)' && cargo build --workspace --tests` exited 0.
- Task 2: `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)' && ... -E 'test(=doc_check::source_devflow_env_vars_and_subcommands_are_documented)' && cargo nextest run -p devflow --test start_lock_e2e --no-tests=fail && scripts/check.sh all` exited 0. The doc_check tests ran 1 test each, `start_lock_e2e` passed 20/20, and `check.sh` printed `all OK` with 44 `test result: ok` lines.
- `cargo nextest run -p devflow --bin devflow --no-fail-fast`: 673 run, 673 passed.
- `cargo clippy --workspace --all-targets -- -D warnings` exited 0.
- `crates/devflow-core/src/monitor.rs` is unchanged since a2bb981 and still has `MonitorLaunch::Legacy`. `state.rs` has no `legacy_claude_launch`.
- **Negative controls:**
  - (a) On the first full run the `codex` stubs were still missing from 10 callers, and those 10 tests failed with `agent binary 'codex' not found`. That shows the migrated tests really launch Codex, and that the agent-free child PATH cannot reach the host's real `~/.local/bin/codex`.
  - (b) With a `DEVFLOW_CLAUDE_LEGACY_LAUNCH` line put back in OPERATIONS.md, `doc_referenced_identifiers_exist_in_source` exited 100 with `documented environment variable 'DEVFLOW_CLAUDE_LEGACY_LAUNCH' does not exist in Rust source`. The file was then restored.
- `cargo semver-checks check-release -p devflow-core` exits 100 as expected. It reports this plan's two removals (`config::claude_legacy_launch`, `State.legacy_claude_launch`) and two earlier phase-52 removals (`worktree::add_detached`, `worktree::reference_path`). All need the planned 4.0.0 major bump, which has not been made yet.
- **What this does not establish:**
  - Each suite passed once. That shows the migrated outcomes hold, not that they hold reliably.
  - The four Claude-kept fixtures now exercise the stream shape with a pre-recorded canary, not a live canary run.
  - No live `devflow start` run was made with the rebuilt binary.
  - A stale `DEVFLOW_CLAUDE_LEGACY_LAUNCH` in an operator's shell is now silently ignored. That is the accepted no-migration rule, and it goes in the CHANGELOG at release.

## Issues Encountered

- The `gsd` shim on PATH failed under mise (`No version is set for shim: gsd`). `gsd-tools.cjs` was run with `node` directly.

## User Setup Required

None.

## Next Phase Readiness

One way to launch Claude remains. The release that carries this needs devflow-core 4.0.0 and a CHANGELOG line for the removed flag and env var.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

## Self-Check: PASSED

Commits ad7066d, 39f6edc and 9574b7c are ancestors of HEAD. `fn stream_launch_enabled(agent: AgentKind, stage: Stage) -> bool` is in pipeline_launch.rs. `MonitorLaunch::Legacy` is in monitor.rs, which is unchanged. Every verification above was run in this session, and the exit codes are recorded.

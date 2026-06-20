---
phase: 11
slug: refactor-gsd-native
status: partial
nyquist_compliant: false
wave_0_complete: true
created: 2026-06-20
state_detected: B-adapted
---

# Phase 11 - Validation Strategy

State B adapted: no `11-VALIDATION.md` and no `11-SUMMARY.md` existed, but Phase 11 was already executed on `feature/phase-11`. This validation reconstructs coverage from `11-PLAN.md`, `CONTEXT.md`, implementation files, inline Rust tests, CLI integration tests, and a fresh `cargo test` run.

## Test Infrastructure

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness via Cargo |
| Config file | `Cargo.toml`, `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml` |
| Test locations | Inline `#[cfg(test)]` modules; `crates/devflow-core/tests/monitor_e2e.rs`; `crates/devflow-cli/tests/phase7_cli.rs` |
| Quick run command | `cargo test -p devflow-core --lib` |
| Full suite command | `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check` |
| Audit command run | `cargo test` |
| Audit result | Latest `cargo test`: 157 passed, 0 failed; one warning for unused `write_last_ship` test helper |
| Flake observation | One immediate rerun failed once in `parallel_creates_two_worktrees_and_spawns_two_monitors` waiting for `.devflow/phase-08-stdout`; the next rerun passed |

## Sampling Rate

- After every task commit: run `cargo test -p devflow-core --lib` for library tasks or the targeted CLI integration test for CLI tasks.
- After every plan subsection: run `cargo test`.
- Before ship: run `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`.
- Max feedback latency observed for `cargo test`: about 2 seconds in the warm local build.

## Requirement Coverage

| Requirement | Coverage | Evidence |
|-------------|----------|----------|
| CORE-01 | PARTIAL | Five-stage `Stage` exists and is tested, but `State::advance()` remains despite the plan's removal requirement. |
| CORE-02 | PARTIAL | `Mode` behavior is tested, but `consecutive_failures` is persisted in `State` although the plan required runtime-only tracking. |
| CORE-03 | COVERED | Prompt generation and adapter prompt forwarding are tested in `prompt.rs` and `agents/mod.rs`. |
| CLI-01 | COVERED | CLI `start --mode`, `--dry-run`, monitor advancement, and status paths are implemented; CLI command parser tests and integration tests pass. |
| CLI-02 | COVERED | Removed old CLI subcommands from `Command`; remaining CLI includes `start`, `parallel`, `sequentagent`, `reference`, `cleanup`, `status`, `list`, `recover`, `doctor`, and `test`. |
| GATE-01 | COVERED | Gate file schemas, write, poll, ack, cleanup, and action parsing are tested in `gates.rs`. |
| GATE-02 | COVERED | Validate/Ship gate decisions are wired in CLI and tested through `mode.rs`; live Hermes interaction remains manual-only. |
| HOOK-01 | COVERED | Hook map, branch create/cleanup, changelog, and version bump are tested in `hooks.rs`; docs hook is fail-soft and not directly asserted. |
| VERSION-01 | COVERED | Hybrid Git-based SemVer detection, tag counting, commit counting, and write-back are tested in `version.rs` and hook tests. |

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 11a-1 | 11a | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib stage::tests::display_is_lowercase` | yes | green |
| 11a-2 | 11a | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib stage::tests::next_walks_linear_chain_then_terminates` | yes | green |
| 11a-3 | 11a | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib stage::tests::gate_stages_are_validate_and_ship stage::tests::agent_stages_are_define_plan_code` | yes | green |
| 11a-4 | 11a | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib stage::tests::gsd_commands_match_stage` | yes | green |
| 11a-5 | 11a | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib state::tests::new_state_starts_at_define state::tests::state_serde_round_trips` | yes | green |
| 11a-6 | 11a | 1 | CORE-01 | source audit | `rg -n "Step\\b|advance_skipping|pub fn advance" crates/devflow-core/src/state.rs` | yes | partial - `Step` and skip logic are gone, but `State::advance()` remains |
| 11a-7 | 11a | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib state::tests::new_state_starts_at_define` | yes | green |
| 11a-8 | 11a | 1 | CORE-01 | compile | `cargo test -p devflow-core --lib stage::tests` | yes | green |
| 11a-9 | 11a | 1 | CORE-01 | compile/source audit | `cargo test -p devflow-core --lib && rg -n "Step\\b" crates --glob "*.rs"` | yes | green |
| 11b-1 | 11b | 1 | CORE-02 | unit | `cargo test -p devflow-core --lib mode::tests::from_str_accepts_canonical_and_alias` | yes | green |
| 11b-2 | 11b | 1 | CORE-02 | unit | `cargo test -p devflow-core --lib mode::tests::auto_does_not_gate_validate_until_failure_threshold mode::tests::supervise_always_gates_validate` | yes | green |
| 11b-3 | 11b | 1 | CORE-02 | unit | `cargo test -p devflow-core --lib mode::tests::auto_loops_validate_supervise_does_not` | yes | green |
| 11b-4 | 11b | 1 | CORE-02 | source audit | `rg -n "consecutive_failures" crates/devflow-core/src/state.rs crates/devflow-cli/src/main.rs` | yes | partial - implemented, but persisted in `State` instead of runtime-only context |
| 11b-5 | 11b | 1 | CORE-02 | compile | `cargo test -p devflow-core --lib mode::tests` | yes | green |
| 11c-1 | 11c | 1 | GATE-01 | unit | `cargo test -p devflow-core --lib gates::tests::gate_file_round_trips_through_serde` | yes | green |
| 11c-2 | 11c | 1 | GATE-01 | unit | `cargo test -p devflow-core --lib gates::tests::write_gate_creates_file_with_correct_path` | yes | green |
| 11c-3 | 11c | 1 | GATE-01 | unit | `cargo test -p devflow-core --lib gates::tests::poll_response_returns_when_file_appears gates::tests::poll_response_times_out_when_absent` | yes | green |
| 11c-4 | 11c | 1 | GATE-01 | unit | `cargo test -p devflow-core --lib gates::tests::ack_writes_received_true` | yes | green |
| 11c-5 | 11c | 1 | GATE-01 | unit | `cargo test -p devflow-core --lib gates::tests::cleanup_removes_all_three_files_idempotently` | yes | green |
| 11c-6 | 11c | 1 | GATE-02 | unit | `cargo test -p devflow-core --lib gates::tests::gate_action_advances_on_approval gates::tests::gate_action_loops_back_on_fixable_rejection gates::tests::gate_action_aborts_when_note_says_abort` | yes | green |
| 11c-7 | 11c | 1 | GATE-01 | compile | `cargo test -p devflow-core --lib gates::tests` | yes | green |
| 11d-1 | 11d | 1 | CORE-03 | unit | `cargo test -p devflow-core --lib prompt::tests::each_stage_prompt_carries_its_gsd_command_and_marker` | yes | green |
| 11d-2 | 11d | 1 | CORE-03 | unit | `cargo test -p devflow-core --lib prompt::tests::fix_prompts_select_the_right_command` | yes | green |
| 11d-3 | 11d | 1 | CORE-03 | source audit | `rg -n "phase_prompt" crates/devflow-core/src/agents/mod.rs` | yes | green |
| 11d-4 | 11d | 1 | CORE-03 | unit | `cargo test -p devflow-core --lib agents::tests::claude_and_codex_share_identical_prompt_text agents::tests::claude_wraps_prompt_in_noninteractive_flags agents::tests::codex_wraps_prompt_in_exec_and_json` | yes | green |
| 11d-5 | 11d | 1 | CORE-03 | compile | `cargo test -p devflow-core --lib agents::tests` | yes | green |
| 11d-6 | 11d | 1 | CORE-03 | compile | `cargo test -p devflow-core --lib prompt::tests` | yes | green |
| 11d-7 | 11d | 1 | CORE-03 | unit | `cargo test -p devflow-core --lib agents::tests` | yes | green |
| 11e-1 | 11e | 1 | CLI-02 | source audit | `rg -n "AutomationConfig|auto_|continue_on_error" crates/devflow-core/src/config.rs` | yes | green |
| 11e-2 | 11e | 1 | VERSION-01 | source audit | `rg -n "VersionConfig" crates/devflow-core/src/config.rs` | yes | green |
| 11e-3 | 11e | 1 | CLI-02 | unit | `cargo test -p devflow-core --lib config::tests::default_uses_hardcoded_constants` | yes | green |
| 11e-4 | 11e | 1 | CLI-02 | source audit | `rg -n "fn load|fn parse_config|fn should_skip|fn to_yaml|ConfigError|fn clean_value|fn parse_bool" crates/devflow-core/src/config.rs` | yes | green |
| 11e-5 | 11e | 1 | CLI-02 | test suite | `cargo test -p devflow-core --lib config::tests` | yes | green |
| 11e-6 | 11e | 1 | CLI-02 | unit | `cargo test -p devflow-core --lib git::tests::feature_start_branches_from_develop config::tests::default_uses_hardcoded_constants` | yes | green |
| 11e-7 | 11e | 1 | CLI-02 | source audit | `rg -n "Command::Init|Command::Config" crates/devflow-cli/src/main.rs` | yes | green |
| 11e-8 | 11e | 1 | CLI-02 | source audit | `rg -n "fn init\\b|fn show_config\\b" crates/devflow-cli/src/main.rs` | yes | green |
| 11f-1 | 11f | 1 | HOOK-01 | compile | `cargo test -p devflow-core --lib hooks::tests` | yes | green |
| 11f-2 | 11f | 1 | HOOK-01 | unit | `cargo test -p devflow-core --lib hooks::tests::transition_map_finalizes_docs_and_changelog_before_ship hooks::tests::after_ship_runs_version_and_cleanup` | yes | green |
| 11f-3 | 11f | 1 | HOOK-01 | unit | `cargo test -p devflow-core --lib hooks::tests::branch_create_makes_feature_branch` | yes | green |
| 11f-4 | 11f | 1 | HOOK-01 | unit | `cargo test -p devflow-core --lib hooks::tests::branch_cleanup_is_fail_soft_when_branch_absent` | yes | green |
| 11f-5 | 11f | 1 | HOOK-01 | source audit | `rg -n "cargo doc --no-deps|docs_update" crates/devflow-core/src/hooks.rs` | yes | green |
| 11f-6 | 11f | 1 | HOOK-01 | unit | `cargo test -p devflow-core --lib hooks::tests::changelog_append_writes_entry` | yes | green |
| 11f-7 | 11f | 1 | HOOK-01 VERSION-01 | unit | `cargo test -p devflow-core --lib hooks::tests::version_bump_tags_repo` | yes | green |
| 11f-8 | 11f | 1 | HOOK-01 | unit | `cargo test -p devflow-core --lib hooks::tests::transition_map_finalizes_docs_and_changelog_before_ship hooks::tests::after_ship_runs_version_and_cleanup` | yes | green |
| 11f-9 | 11f | 1 | HOOK-01 | compile | `cargo test -p devflow-core --lib hooks::tests` | yes | green |
| 11g-1 | 11g | 1 | CLI-01 | compile/source audit | `cargo test -p devflow-cli --bin devflow && rg -n "mode: Mode|dry_run" crates/devflow-cli/src/main.rs` | yes | green |
| 11g-2 | 11g | 1 | CLI-01 | unit/integration | `cargo test -p devflow-cli && cargo test -p devflow-core --test monitor_e2e` | yes | green |
| 11g-3 | 11g | 1 | CLI-01 | source audit | `rg -n "print_dry_run|dry run" crates/devflow-cli/src/main.rs` | yes | green |
| 11g-4 | 11g | 1 | CLI-02 | source audit | `rg -n "Command::Test|test_cmd" crates/devflow-cli/src/main.rs` | yes | green |
| 11g-5 | 11g | 1 | CLI-02 | source audit | `rg -n "Command::Check|Command::Verify|Command::Lint|Command::Docs|Command::Ship|Command::Confirm|Command::Rejectpr" crates/devflow-cli/src/main.rs` | yes | green |
| 11g-6 | 11g | 1 | CLI-02 | compile | `cargo test -p devflow-cli` | yes | green |
| 11g-7 | 11g | 1 | CLI-01 | source audit | `rg -n "Stage:|Mode:|Gate:" crates/devflow-cli/src/main.rs` | yes | green |
| 11g-8 | 11g | 1 | CLI-02 | source audit | `rg -n "fn check\\b|fn verify\\b|fn lint\\b|fn docs\\b|fn ship\\b|fn confirm\\b|fn rejectpr\\b|fn init\\b|fn show_config\\b" crates/devflow-cli/src/main.rs` | yes | green |
| 11h-1 | 11h | 1 | CLI-01 GATE-02 | source audit | `rg -n "ship_phase|gsd-ship|gsd-code-review" crates/devflow-core/src/ship.rs` | yes | missing - no `ship_phase()` implementation found |
| 11h-2 | 11h | 1 | CLI-02 | source audit | `rg -n "LastShip|confirm|rejectpr|gh pr" crates/devflow-core/src/ship.rs` | yes | partial - old bookkeeping remains, though old CLI commands are removed |
| 11h-3 | 11h | 1 | GATE-02 | source audit | `rg -n "gsd-code-review|LoopBack|ReviewFailed" crates/devflow-core/src/ship.rs crates/devflow-cli/src/main.rs` | yes | partial - loop-back exists in CLI gate handling, not in a rewritten ship module |
| 11h-4 | 11h | 1 | GATE-02 | source audit | `rg -n "ReviewFailed|AgentFailed" crates/devflow-core/src/ship.rs` | yes | missing |
| 11i-1 | 11i | 1 | CLI-02 | source audit | `ls crates/devflow-core/src/verify.rs` | no | green |
| 11i-2 | 11i | 1 | CLI-02 | source audit | `rg -n "pub mod verify|verify::" crates/devflow-core/src/lib.rs crates` | yes | green |
| 11i-3 | 11i | 1 | CORE-01 | source audit | `rg -n "Step\\b" crates --glob "*.rs"` | yes | green |
| 11i-4 | 11i | 1 | CLI-02 | source audit | `rg -n "should_skip|advance_skipping" crates --glob "*.rs"` | yes | green |
| 11i-5 | 11i | 1 | CORE-03 | source audit | `rg -n "capture_agent_output" crates/devflow-core/src/agent.rs crates/devflow-cli/src/main.rs` | yes | missing - function remains public and used by `sequentagent` |
| 11i-6 | 11i | 1 | CLI-02 | source audit | `rg -n "continue_on_error" crates --glob "*.rs"` | yes | green |
| 11i-7 | 11i | 1 | CLI-02 | source audit | `rg -n "fn check\\b" crates/devflow-cli/src/main.rs` | yes | green |
| 11j-1 | 11j | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib version::tests::count_tags_and_commits_drive_minor_and_patch` | yes | green |
| 11j-2 | 11j | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib version::tests::detect_prefers_cargo_then_pyproject_then_package_json` | yes | green |
| 11j-3 | 11j | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib version::tests::read_major_from_workspace_package version::tests::read_major_from_package_json` | yes | green |
| 11j-4 | 11j | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib version::tests::count_tags_and_commits_drive_minor_and_patch` | yes | green |
| 11j-5 | 11j | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib version::tests::count_tags_and_commits_drive_minor_and_patch` | yes | green |
| 11j-6 | 11j | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib hooks::tests::version_bump_tags_repo` | yes | green |
| 11j-7 | 11j | 1 | VERSION-01 | source audit | `rg -n "VersionConfig|calver|build_number|scheme" crates/devflow-core/src/version.rs crates/devflow-core/src/config.rs` | yes | green |
| 11k-1 | 11k | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib state::tests stage::tests` | yes | green |
| 11k-2 | 11k | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib stage::tests` | yes | green |
| 11k-3 | 11k | 1 | CORE-02 | unit | `cargo test -p devflow-core --lib mode::tests` | yes | green |
| 11k-4 | 11k | 1 | GATE-01 | unit | `cargo test -p devflow-core --lib gates::tests` | yes | green |
| 11k-5 | 11k | 1 | CORE-03 | unit | `cargo test -p devflow-core --lib prompt::tests` | yes | green |
| 11k-6 | 11k | 1 | HOOK-01 | unit | `cargo test -p devflow-core --lib hooks::tests` | yes | green |
| 11k-7 | 11k | 1 | CORE-01 | unit | `cargo test -p devflow-core --lib workflow::tests` | yes | green |
| 11k-8 | 11k | 1 | CORE-03 | unit | `cargo test -p devflow-core --lib agents::tests` | yes | green |
| 11k-9 | 11k | 1 | CLI-02 | unit | `cargo test -p devflow-core --lib git::tests` | yes | green |
| 11k-10 | 11k | 1 | VERSION-01 | unit | `cargo test -p devflow-core --lib version::tests` | yes | green |
| 11k-11 | 11k | 1 | all | full suite | `cargo test` | yes | green - 157 passed, one warning |
| 11k-12 | 11k | 1 | CORE-01 | source audit | `sed -n '1,80p' crates/devflow-core/src/lib.rs; test -f AGENTS.md` | partial | partial - `AGENTS.md` absent and `lib.rs` docs still mention old `devflow check`/`ship`/step events |
| 11k-13 | 11k | 1 | CLI-02 | source audit | `test ! -e .devflow.yaml` | no | missing - `.devflow.yaml` still exists |
| 11k-14 | 11k | 1 | CORE-01 | source audit | `rg -n "Step\\b" crates --glob "*.rs"` | yes | green |

## Coverage Gaps

| Gap | Affected Tasks | Classification | Evidence | Suggested Follow-Up |
|-----|----------------|----------------|----------|---------------------|
| `State::advance()` remains even though 11a-6 required removing it. | 11a-6 | PARTIAL | `crates/devflow-core/src/state.rs` defines `pub fn advance`. | Either delete it and use monitor/CLI transition logic exclusively, or amend the phase plan to allow a narrow helper. |
| Validate failure count is persisted in `State`, not runtime-only context. | 11b-4 | PARTIAL | `State` includes `consecutive_failures`. | Move to runtime monitor/session context or update the design if persistence is intentional across monitor restarts. |
| Ship stage was not rewritten into a GSD-native `ship_phase()` module. | 11h-1, 11h-2, 11h-3, 11h-4 | MISSING/PARTIAL | `ship.rs` still contains `LastShip`, confirm/reject wording, PR-body helpers, and no `ship_phase`, `ReviewFailed`, or `AgentFailed`. | Implement the planned `/gsd-ship` then `/gsd-code-review` flow or re-scope `ship.rs` as legacy utility code. |
| Blocking capture path remains public and in use. | 11i-5 | MISSING | `agent::capture_agent_output()` is used by `sequentagent`. | Decide whether `sequentagent` is allowed to keep a synchronous path; otherwise move capture behind monitor-owned execution. |
| v2.0.0 documentation update is incomplete. | 11k-12 | PARTIAL | `lib.rs` still references `devflow check`, `devflow ship`, and `step_*` events; no repo `AGENTS.md` exists. | Update docs to Describe Stage/Mode/Gate terminology and current commands. |
| `.devflow.yaml` remains in the project root. | 11k-13 | MISSING | `ls .devflow.yaml` succeeds. | Delete the file or document why the repository keeps it despite runtime ignoring it. |
| CLI parallel monitor integration appears timing-sensitive. | 11g-2, 11k-11 | PARTIAL | One `cargo test` rerun failed because `phase-08-stdout` was absent; the next full rerun passed all 157 tests. | Harden the integration test wait condition or monitor completion synchronization. |

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Hermes gate delivery from `.devflow/gates/*.json` to human and response back to DevFlow | GATE-02 | Unit tests cover file protocol, not the external Hermes cron/poller. | Start a supervised run, confirm Hermes observes `NN-validate.json`, write/receive `NN-validate.response.json`, and verify DevFlow writes `NN-validate.ack.json`. |
| Real agent launch through Claude/Codex/OpenCode CLIs | CORE-03 CLI-01 | Tests use fake shell agents and command construction; they do not consume real paid/credentialed agent CLIs. | Run `devflow start --phase 11 --agent codex --mode auto --dry-run`, then a controlled real-agent smoke run in a disposable phase/worktree. |
| Full Ship review/merge workflow | CLI-01 GATE-02 | Planned `ship.rs` rewrite is incomplete; no automated test can validate the intended `/gsd-ship` + `/gsd-code-review` contract yet. | Implement the ship rewrite, then run a dry-run or mocked agent flow that exercises review-pass and review-fail loop-back paths. |
| Docs hook side effects in the real workspace | HOOK-01 | Hook test verifies map/other hooks; `DocsUpdate` is intentionally fail-soft and can skip commit when `cargo doc` fails. | Run the Validate->Ship transition in a clean repo and verify generated docs are committed or an explicit warning is recorded. |

## Validation Sign-Off

- [x] Nyquist config checked: local GSD returned `true`.
- [x] Input state detected: adapted State B, reconstructing from executed artifacts.
- [x] Plan and context read.
- [x] Test infrastructure detected.
- [x] Requirements and tasks mapped to implementation/tests.
- [x] `cargo test` latest run: 157 passed, 0 failed.
- [ ] No flaky validation observed.
- [ ] No plan-contract gaps remain.
- [ ] `nyquist_compliant: true` set in frontmatter.

Approval: partial 2026-06-20. Automated tests are green, but the phase is not Nyquist-compliant because multiple planned implementation tasks remain partial or missing.

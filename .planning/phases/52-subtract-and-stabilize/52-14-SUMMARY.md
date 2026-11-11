---
phase: 52-subtract-and-stabilize
plan: 14
subsystem: pipeline
tags: [cut-02, cut-04, yes-ship, config, state, dry-run, semver]

requires:
  - phase: 52-13
    provides: the Ship preflight checks and ship_evidence are gone, so yes_ship's only remaining consumer was the Ship gate
provides:
  - "`devflow start --yes-ship` is rejected by clap as an unexpected argument"
  - "`DEVFLOW_YES_SHIP` is not read and not documented"
  - "DevflowConfig has no yes_ship field, Default entry or accessor; config::yes_ship is deleted; a leftover `yes_ship` key in devflow.toml is ignored"
  - "State has no yes_ship field; a 3.x state file carrying it still loads and the key is dropped on save"
  - "the dry run prints no `ship gate:` line"
  - "handle_ship_outcome always waits for a human"
  - "tests/yes_ship_config.rs and its HERMETIC_SUITES entry are deleted"
affects: [52-15, 52-18]

actuals:
  tokens: 16784
  tasks: 2
  commits: 3
plan_head_before: b8db4b51347d0cf37277332f557f12c58319e535
plan_head_after: 45a5c7297f76c97fbe7214c7d7218ee6c8937afd

tech-stack:
  added: []
  patterns:
    - "A concurrent-write test that used a deleted field as its stand-in now uses `base_branch`: the caller's copy keeps the default `None`, the other writer sets `Some(\"workspace/concurrent\")`, and the test asserts the write survives"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/hermetic_spawn_sites.rs
    - crates/devflow-cli/tests/fallback_agent_config.rs
    - OPERATIONS.md
  deleted:
    - crates/devflow-cli/tests/yes_ship_config.rs

key-decisions:
  - "Tests that used yes_ship only as a field another writer changes now use base_branch: start is its only production writer, so transition, loop-back, launch and resume never set it."
  - "run_gate_with_timeout's auto_response parameter and safe point 5 are kept. No production call passes Some any more; 52-15 removes them with handle_ship_outcome (logged in deferred-items.md)."
  - "The two safe-point-5 tests in pipeline_launch went through advance and needed yes_ship to answer the gate, so they are deleted. The pipeline_gate tests that pass the auto-response directly are kept; one is renamed a_stopped_auto_answered_run_sends_no_ship_notify."

patterns-established:
  - "When a removed field was a test's stand-in rather than its subject, the test is re-pointed at another field, not deleted"

requirements-completed: [CUT-02, CUT-04]

coverage:
  - id: D1
    description: "`devflow start --yes-ship` is refused by clap as an unexpected argument"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_flags_are_unknown"
        status: pass
    human_judgment: false
  - id: D2
    description: "A devflow.toml that still sets yes_ship loads without error and the key changes nothing; a 3.x state file carrying yes_ship still loads"
    requirement: CUT-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#config::tests::a_leftover_yes_ship_key_is_ignored"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::a_state_file_with_a_leftover_yes_ship_key_still_loads"
        status: pass
    human_judgment: false
  - id: D3
    description: "The yes-ship suite, its hermetic entry and the DEVFLOW_YES_SHIP doc row are gone, and the doc and hermetic scans pass"
    requirement: CUT-04
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/hermetic_spawn_sites.rs#migrated_suites_spawn_devflow_only_through_the_hermetic_builder"
        status: pass
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

duration: 22min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 14: Remove the Ship Pre-authorization Summary

**Nothing can pre-authorize a Ship any more: `--yes-ship` is an unknown flag, `DEVFLOW_YES_SHIP` is not read, `DevflowConfig` and `State` have no `yes_ship` field, the dry run's ship-gate line is gone, and a leftover `yes_ship` key in `devflow.toml` or in a 3.x state file is ignored.**

## Performance

- **Duration:** about 22 min
- **Started:** 2026-11-11T00:48:18Z
- **Completed:** 2026-11-11T01:10Z
- **Tasks:** 2
- **Files modified:** 13 (128 insertions, 724 deletions)

## Accomplishments

- CLI: the `--yes-ship` arg on `Command::Start` and the `yes_ship` parameter of `commands::start` are removed, along with the config OR-combine and the "pre-authorized by devflow.toml" notice.
- Core config: `DevflowConfig.yes_ship`, its `Default` entry, `DevflowConfig::yes_ship()` and `config::yes_ship()` (the only `DEVFLOW_YES_SHIP` read) are deleted. The doc comments on `base_branch` and `fallback_agent` that used `yes_ship` as their contrast now use `external_verify_enabled`.
- Core state: `State.yes_ship` is deleted. Three field docs that said "persisted for the reason [`Self::yes_ship`] gives" now link to `Self::base_branch`, whose doc gives the same separate-process reason.
- `print_dry_run` no longer prints `ship gate: …`. `handle_ship_outcome` passes `None` to `run_gate_with_timeout`.
- `tests/yes_ship_config.rs` (5 tests) is deleted with its `HERMETIC_SUITES` entry. `fallback_agent_config.rs` no longer says it mirrors that suite. The `DEVFLOW_YES_SHIP` row is gone from OPERATIONS.md.

## Task Commits

1. **Task 1 (tracer): `--yes-ship`, `DEVFLOW_YES_SHIP`, the toml key and the state field are gone end to end:** `9fb40ac` (test, RED), `feba1fd` (feat!, GREEN)
2. **Task 2: the yes-ship test suite, its hermetic entry and its docs are removed:** `45a5c72` (feat!)

**Plan metadata:** the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written with a scratch nextest config (`--profile red`). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `phase7_cli::removed_flags_are_unknown` (with the `--yes-ship` row appended) | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on its planned assertion (phase7_cli.rs:680): clap still accepted `--yes-ship`, and stderr named the missing `--mode` instead of an unexpected argument. Not a build, load or fixture fault. It passes after `feba1fd`. |
| `config::tests::a_leftover_yes_ship_key_is_ignored` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on its planned equality (config.rs:667): the loaded config had `yes_ship: true` where the default has `false`. `capture_retention: 9` matched on both sides, so the file had parsed. It passes after `feba1fd`. |
| Task 2 (`migrated_suites_spawn_devflow_only_through_the_hermetic_builder`, the two `doc_check` tests) | No RED commit | Task 2 deletes things and is not `tdd="true"`. Negative control instead: with `yes_ship_config.rs` deleted and nothing else changed, the hermetic scan failed with "cannot read suite …/yes_ship_config.rs", and `doc_referenced_identifiers_exist_in_source` failed with "documented environment variable `DEVFLOW_YES_SHIP` does not exist in Rust source". Both pass after the entry and the OPERATIONS.md row were removed. |

Gate sequence: `test(52-14)` 9fb40ac → `feat(52-14)!` feba1fd → `feat(52-14)!` 45a5c72. No REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode was `end-of-phase`, and the verify block is automated-only. The Task 1 `<verify>` chain was run before the commit (rc 0) and again on the committed `feba1fd` tree with a clean worktree (`1 passed` for each test). Expansion then continued.

## Tests deleted or changed

- Deleted with their subject (the pre-authorization):
  - `config`: `yes_ship_defaults_to_false`, `yes_ship_missing_file_returns_false`, `yes_ship_file_sets_true`, `yes_ship_file_sets_false`, `yes_ship_unrelated_keys_returns_default`, `yes_ship_unparseable_env_falls_back_to_file`, `env_overrides_file_yes_ship`.
  - `state`: `yes_ship_round_trips_through_serde`, `yes_ship_absent_from_json_defaults_to_false`, and the `!state.yes_ship` assertion in the `State::new` defaults test.
  - `pipeline_outcomes`: `state_new_alone_never_derives_yes_ship_from_config`, `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution`.
  - `pipeline_launch`: `a_stop_after_safe_point_1_is_honoured_before_the_yes_ship_approval`, `without_a_stop_after_safe_point_1_the_yes_ship_run_finishes`, and their helpers `held_yes_ship_advance` and `branch_exists`.
  - `preflight`: `unattended_check_is_not_bypassed_by_yes_ship`.
  - All 5 tests in `tests/yes_ship_config.rs`.
- Added: `config::tests::a_leftover_yes_ship_key_is_ignored` (planned) and `state::tests::a_state_file_with_a_leftover_yes_ship_key_still_loads` (see deviation 3).
- Renamed, subject unchanged: `pipeline_gate::a_stopped_yes_ship_run_sends_no_ship_notify` became `a_stopped_auto_answered_run_sends_no_ship_notify`. `pipeline_outcomes::handle_ship_outcome_without_yes_ship_writes_gate_but_no_response` became `handle_ship_outcome_writes_gate_but_no_response`.
- Re-pointed from `yes_ship` to `base_branch` as the field another writer changes: `pipeline_gate::transition_commits_only_its_fields`, `pipeline_gate::a_loop_back_commits_its_fields_and_the_verification_observation`, `pipeline_outcomes::a_validate_failure_commits_its_counters_and_keeps_a_concurrent_field`, `pipeline_launch::a_validate_launch_commits_its_dispatch_stamp`, `pipeline_launch::resume_commits_its_unstop_and_keeps_a_concurrent_field`. `commands::persist_stopped_state_appends_its_reason_on_the_current_state` used `yes_ship = true` only as a non-default field; it now sets `base_branch`.
- `phase7_cli::REMOVED_FLAG_INVOCATIONS` now lists `start --phase 1 --yes-ship --dry-run`.

## Decisions Made

See `key-decisions` above. `cargo semver-checks check-release -p devflow-core` (baseline: published 3.0.0) exits 100 with 7 major lint failures. This plan's items among them are `function devflow_core::config::yes_ship`, the inherent method `DevflowConfig::yes_ship` (a lint new in this plan: `inherent_method_missing`), the field `DevflowConfig.yes_ship` and the field `State.yes_ship`. All are intended (D-01). The other lints come from items earlier plans removed.

## Deviations from Plan

**1. [Convention] Commit subjects**
- **Issue:** the plan gave `feat(cli)!: remove --yes-ship`. GSD's TDD gate looks for `feat(52-14)`, as in 52-08 to 52-13.
- **Fix:** `feat(52-14)!: remove --yes-ship` and `feat(52-14)!: remove the yes-ship test suite and its docs`, each with a `BREAKING CHANGE:` footer.

**2. [Plan premise] README.md and docs/guides/configuration.md had no yes-ship mention**
- **Issue:** the plan lists both for removal of `--yes-ship`/`yes_ship` mentions. A case-insensitive `rg 'yes_ship|yes-ship|YES_SHIP'` over the repo (excluding `.planning/` and CHANGELOG.md) matched only OPERATIONS.md, CLAUDE.md and code. The negative control for that search: the same search matched the OPERATIONS.md row before it was removed.
- **Fix:** neither file changed. The CLAUDE.md mention is an operator file, left alone as the plan says.

**3. [Rule 2 - Missing check] A 3.x state file with `yes_ship` must still load**
- **Issue:** the plan pins only the toml key. The operator's own machine has state files written by 3.x with `"yes_ship"` in them, and `State` deserializes them. `State` has no `deny_unknown_fields`, so they load, but nothing pinned it.
- **Fix:** `a_state_file_with_a_leftover_yes_ship_key_still_loads` replaces the two deleted state tests. It loads such a file and asserts the key is not written back on save. No migration code was added.
- **Committed in:** feba1fd

**4. [Rule 1 - Consistency] Comments and doc links that named the deleted field**
- **Issue:** comments in `commands.rs` (five), `pipeline_gate.rs` (two), `config.rs` (three doc comments) and `state.rs` (four, three of them intra-doc links to `Self::yes_ship`) named `yes_ship` or `--yes-ship`.
- **Fix:** each was reworded in `feba1fd`. The intra-doc links now point at `Self::base_branch`. No code changed for this.

**5. [Ordering] Between `feba1fd` and `45a5c72` the five `yes_ship_config` tests fail**
- **Issue:** the plan puts the suite's deletion in Task 2, so the Task 1 commit has a suite that tests a removed flag. A workspace run on that tree had 1929 tests: 1924 passed and exactly those 5 failed.
- **Fix:** none. The plan's split was kept, and `45a5c72` deletes the suite.

**Total deviations:** 5 (1 convention, 1 plan premise, 1 missing check, 1 consistency, 1 ordering). **Impact:** no behaviour beyond the plan; one extra test.

## Issues Encountered

- `run_gate_with_timeout`'s `auto_response` parameter and safe point 5 now have no production caller that passes `Some`. The pipeline_gate tests that exercise them directly are kept. Logged in `deferred-items.md` for 52-15, which removes `handle_ship_outcome`.
- The 52-13 deferred item "No test now pins that `yes_ship` cannot auto-approve a preflight gate" is marked resolved: the field is gone, so the property has no subject.

## Verification

- Task 1 `<verify>`: exit 0 on the edited tree, and again on the clean `feba1fd` tree (`1 passed` each).
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0 after Task 1's edits.
- Workspace `cargo nextest run --workspace --no-fail-fast` after Task 1's edits: 1929 run, 1924 passed, 5 failed (all in `yes_ship_config`, deleted in Task 2), 6 skipped.
- Task 2 `<verify>` chain, run on the tree committed as `45a5c72` (only `deferred-items.md` was also modified, and that is not read by any test): exit 0. The three named tests each had `1 passed`. `scripts/check.sh all` printed "all OK". Its log has 44 `test result: ok` lines (52-13 had 45; one test binary, `yes_ship_config`, is gone) and no `FAILED` or `panicked`. The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) passed, so no rerun was needed.
- Acceptance checks: `crates/devflow-cli/tests/yes_ship_config.rs` does not exist. `rg -n 'pub yes_ship' crates/` exits 1. The only `yes_ship` text left in `state.rs` is the new leftover-key test.
- What this does not establish:
  - `check.sh all` ran on this host, not in the pinned container. The pre-push hook runs the container check.
  - No live `devflow start` was made. The flag refusal is shown by the clap test; the removed config path by the unit test.
  - The re-pointed concurrent-write tests prove a `base_branch` change survives. They no longer prove that for a bool field. No production code path treats the two differently, but no test checks that.
  - The `rg` absence checks are shell checks. The comment-stripped source scan comes in 52-18.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-15, which removes `Stage::Ship`. That plan should also drop `run_gate_with_timeout`'s `auto_response` parameter and safe point 5 (see `deferred-items.md`).

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

- `yes_ship_config.rs` is absent; this SUMMARY exists.
- Commits 9fb40ac, feba1fd and 45a5c72 are ancestors of HEAD.
- `check evaluation-scope --plan 52-14 --commits-only` resolved the plan's three commits on this branch.

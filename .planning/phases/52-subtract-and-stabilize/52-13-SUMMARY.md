---
phase: 52-subtract-and-stabilize
plan: 13
subsystem: pipeline
tags: [cut-02, cut-04, evidence, ship_evidence, version, ship, preflight, semver, cargo-lock]

requires:
  - phase: 52-12
    provides: the after-ship hooks and finalization path are gone, so version.rs and ship.rs had no production caller left
provides:
  - "`devflow evidence` is an unknown subcommand; `Command::Evidence`, `commands::evidence` and its three tests are deleted"
  - "devflow_core::ship_evidence (collect, is_stopped_at, ShipEvidence) is deleted"
  - "devflow_core::version and devflow_core::ship (prepend_changelog, ShipError) are deleted, with the CLI's CliError::Ship variant"
  - "the gh-auth and major-bump preflight checks and breaking_commit_subjects are deleted with their tests; generic_preflight_checks runs the unattended-launch and interactivity checks only"
  - "devflow-core no longer depends on semver or git-conventional; Cargo.lock drops git-conventional and unicase"
  - ".gitignore's .devflow/last-ship*.json line and its gitignore_coverage expectation are gone"
affects: [52-14, 52-15, 52-18]

actuals:
  tokens: 48992
  tasks: 2
  commits: 3
plan_head_before: aea9b6d4ffa7728db20283fc64558927b2a3c0e4
plan_head_after: 52a9488d9baae4cd50f12a962685e95faa57c51e

tech-stack:
  added: []
  patterns:
    - "A kept test that read state through a deleted module now reads the event log directly (events::last_event_of_kind_for_phase)"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - crates/devflow-cli/tests/workspace_version_pin.rs
    - crates/devflow-cli/tests/gitignore_coverage.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/events.rs
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/workflow.rs
    - crates/devflow-core/Cargo.toml
    - Cargo.lock
    - .gitignore
    - OPERATIONS.md

key-decisions:
  - "pipeline_gate's until-stop test is kept and renamed until_stop_never_emits_workflow_shipped: it still asserts no workflow_shipped, and it now checks the stopped_at reason on the workflow_finished event directly instead of through ship_evidence."
  - "workspace_version_pin.rs never called devflow_core::version, so only its comments changed; it still parses the root Cargo.toml by hand. A negative control (pin set to 2.9.9 and the built test binary run against it) failed with the expected message, then the file was restored."
  - "semver stays in Cargo.lock: wasmparser and wit-parser depend on it. Only git-conventional and unicase were removed, and winnow lost its memchr edge."
  - "generic_preflight_checks now orders the unattended-launch reason first, then interactivity; the ordering comment no longer describes Ship checks."

patterns-established:
  - "A kept test that reached its subject through a deleted reader is re-pointed at the underlying record, not deleted"

requirements-completed: [CUT-02, CUT-04]

coverage:
  - id: D1
    description: "`devflow evidence` is rejected by clap and absent from --help; ship_evidence.rs is deleted"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_subcommands_are_unknown"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D2
    description: "The gh-auth and major-bump preflight checks, version.rs, ship.rs, semver and git-conventional are removed; the workspace self-pin guard still bites"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/workspace_version_pin.rs#workspace_member_pins_match_the_workspace_version"
        status: pass
      - kind: other
        ref: "scripts/check.sh deps (cargo deny + cargo machete)"
        status: pass
    human_judgment: false
  - id: D3
    description: "No doc, ignore pattern or doc_check identifier names the removed command, modules or the last-ship file"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/gitignore_coverage.rs#gitignore_covers_devflow_runtime_state_paths"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 13: Remove ship, version and ship_evidence Summary

**devflow-core has no Ship module left: `version`, `ship` and `ship_evidence` are deleted, `devflow evidence` is an unknown subcommand, the gh-auth and major-bump preflight checks are gone, and devflow-core no longer depends on `semver` or `git-conventional`.**

## Performance

- **Duration:** about 20 min
- **Started:** 2026-11-11T00:19:13Z
- **Completed:** 2026-11-11T00:38:45Z
- **Tasks:** 2
- **Files modified:** 23 (72 insertions, 3949 deletions)

## Accomplishments

- CLI: `Command::Evidence` and `commands::evidence` are gone, along with its three `evidence_require_shipped_*` tests and the OPERATIONS.md row. The help snapshot was regenerated; the command column is one character narrower because `evidence` was the longest name.
- Core: `ship_evidence.rs` (404 lines), `version.rs` (2507 lines) and `ship.rs` (88 lines) are deleted with their `pub mod` lines. `CliError::Ship` goes with `ShipError`.
- Preflight: `gh_auth_check_applies`, `preflight_gh_auth_check`, `major_bump_check_applies`, `preflight_major_bump_check` and `breaking_commit_subjects` are deleted, with their entries in `generic_preflight_checks`, their 14 tests and the fixtures only they used. The `version` import is gone.
- Dependencies: `semver` and `git-conventional` are removed from `crates/devflow-core/Cargo.toml`. `Cargo.lock` drops `git-conventional` and `unicase`.
- `.gitignore`'s `.devflow/last-ship*.json` line and the matching `gitignore_coverage` path are removed.
- Comments that pointed at the deleted modules or checks were reworded in `events.rs`, `test_support.rs`, `agent_result.rs`, `git.rs`, `monitor.rs`, `workflow.rs`, `pipeline_launch.rs`, `preflight.rs` and `workspace_version_pin.rs`.

## Task Commits

1. **Task 1 (tracer): `devflow evidence` and `ship_evidence.rs` are gone end to end:** `d1018ad` (test, RED), `458a773` (feat!, GREEN)
2. **Task 2: the Ship preflight checks, `version.rs` with its crates, and `ship.rs` are removed:** `52a9488` (feat!)

**Plan metadata:** recorded in the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from a nextest JUnit report, written with a scratch nextest config (`--profile red`). The record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `phase7_cli::removed_subcommands_are_unknown` (with `evidence` appended) | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on its planned assertion (phase7_cli.rs:596, "must be rejected by clap as unknown"). Clap still knew the verb: stderr said `the following required arguments were not provided: --phase <PHASE>`. This was not a build, load or fixture fault. The test passes after `458a773`. |
| Task 2 (`workspace_member_pins_match_the_workspace_version`, `gitignore_covers_devflow_runtime_state_paths`, kept `preflight` tests) | No RED | Task 2's stated behaviour is that kept tests stay green, and no test can fail first on that. Both named tests passed before any Task 2 edit and again after it. A negative control for the self-pin guard: with the root `Cargo.toml` pin changed to `2.9.9`, the built test binary failed with "`devflow-core` is pinned to \"2.9.9\" … but [workspace.package] version is \"3.0.0\"". `Cargo.toml` was restored (`git diff --quiet` clean) and the test passed again. |

Gate sequence: `test(52-13)` d1018ad → `feat(52-13)!` 458a773 → `feat(52-13)!` 52a9488. Task 2 has no RED commit, for the reason in the table. No REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode was `end-of-phase`, and the verify block is automated-only. The Task 1 `<verify>` chain was run before the commit (rc 0) and again on the committed `458a773` tree with a clean worktree (rc 0, `1 passed` for each of the three tests). Expansion then continued.

## Tests deleted or changed

- Deleted with their subject:
  - `commands`: `evidence_require_shipped_exits_ok_iff_the_phase_has_shipped`, `evidence_require_shipped_names_stopped_at_rather_than_generic_not_shipped`, `evidence_require_shipped_failure_message_is_single_line_and_names_phase`.
  - `preflight`: `gh_auth_check_applies_only_to_ship_stage`, `major_bump_check_applies_only_to_ship_stage`, `major_bump_short_circuits_for_non_ship_stage`, `major_bump_ok_for_patch_or_minor_bump_at_ship`, `major_bump_errs_naming_bump_baseline_and_version_for_major_at_ship`, `major_bump_surfaces_unreachable_baseline_refusal`, the worktree-HEAD major-bump tests, `run_preflight_major_bump_gates_and_never_ships_unattended`, `run_preflight_major_bump_gate_not_auto_approved_by_yes_ship`, and `generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first`. The fixtures only they used went too: `commit_msg`, `tag`, `major_bump_fixture`, `major_bump_worktree_fixture`, `git_only_path_dir_with_failing_gh`.
  - Every test inside `ship_evidence.rs`, `version.rs` and `ship.rs`, deleted with the modules.
- Changed: `pipeline_gate::until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped` became `until_stop_never_emits_workflow_shipped`. It still asserts that no `workflow_shipped` event is written, and it reads the `workflow_finished` event's `reason` (`stopped_at`) with `events::last_event_of_kind_for_phase` instead of `ship_evidence::collect`.
- Changed: `phase7_cli::REMOVED_SUBCOMMANDS` now lists `evidence`.

## Decisions Made

See `key-decisions` above. `cargo semver-checks check-release -p devflow-core` (baseline: published 3.0.0) exits 100 and reports 6 major lint failures: `enum_missing`, `function_missing`, `module_missing`, `pub_module_level_const_missing`, `struct_missing` and `struct_pub_field_missing`. This plan's items among them are `mod devflow_core::ship`, `mod devflow_core::version` and `mod devflow_core::ship_evidence`, with their items (`ShipError`, `prepend_changelog`, `Version`, `VersionError`, `Bump`, `compute_version`, `write_version`, `ShipEvidence`, `collect`, `is_stopped_at` and the rest). All of these are intended (D-01, D-23). The report also lists `hooks`, `lock` and the cron-instruction items, which earlier plans removed.

## Deviations from Plan

**1. [Convention] Commit subjects**
- **Issue:** the plan gave the subject `feat(core)!: remove ship, version and ship_evidence`. GSD's TDD gate looks for `feat(52-13)`, as in 52-08 to 52-12.
- **Fix:** `feat(52-13)!: …` for both commits, each with a `BREAKING CHANGE:` footer.

**2. [Plan premise] No `version` import in commands.rs, and no OPERATIONS.md preflight rows for the two checks**
- **Issue:** the plan says to remove "the `version` import in `commands.rs`" and "the OPERATIONS.md preflight rows for the two checks". Neither exists. The only `version` import was in `preflight.rs` (`use devflow_core::{agents, events, verify, version}`), and it is removed. OPERATIONS.md's "Preflight refusals" table lists three `start` checks, none of them gh-auth or major-bump. An `rg` over README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md and `docs/` found no mention of either check. The negative control for that search: `docs/index.md` still matched `hooks`.
- **Fix:** nothing to remove. Recorded here.

**3. [Plan premise] README had no evidence mention**
- **Issue:** the plan asks for the README `evidence` mentions to be removed. The only match was "retained attempt evidence" in the `history` row, which is about `history`.
- **Fix:** README unchanged.

**4. [Rule 1 - Consistency] Comments outside the plan's file list that named the deleted modules**
- **Issue:** besides `events.rs`, `agent_result.rs` and `test_support.rs`, comments in `git.rs` ("see `version.rs`"), `monitor.rs` (`version.rs:1102`), `workflow.rs` (`ShipError` in a list of error enums), `pipeline_launch.rs` (the generic bundle named "major-bump" and "gh-auth") and `workspace_version_pin.rs` (`version::write_version`, "see `version.rs`") pointed at what this plan deleted.
- **Fix:** each was reworded in `52a9488`. No code changed in those files.

**5. [Ordering] The self-pin guard was re-run after the module declarations were removed, not before `version.rs` was deleted from disk**
- **Issue:** the plan asks for the guard to run green before `version.rs` is deleted. The guard never used `devflow_core::version`, so there was nothing to rewrite in code first.
- **Fix:** the guard passed before any Task 2 edit, and again after `pub mod version;` was removed but before `git rm`. It passed a third time inside the Task 2 verify chain, and the negative control above was run against the edited tree.

**Total deviations:** 5 (1 convention, 3 plan premises, 1 consistency). **Impact:** no behaviour added; the plan's outcome is unchanged.

## Issues Encountered

- Nothing now proves that `yes_ship` cannot auto-approve a preflight gate. `run_preflight_major_bump_gate_not_auto_approved_by_yes_ship` was the test for that, and it needed the major-bump check. 52-14 deletes `yes_ship`, so this is logged in `deferred-items.md` and not re-seeded here.
- `crates/devflow-cli/src/test_support.rs` `init_repo`'s doc comment still names the `VersionBump` and `BranchCleanup` hooks that 52-12 removed. That file was not in this plan's list, so this is logged in `deferred-items.md` for 52-18.
- `events::last_event_of_kind_for_phase` lost its core caller but is still used about twenty times in `pipeline_outcomes.rs`, so it stays.

## Verification

- Task 1 `<verify>`: exit 0, run on the edited tree and again on the clean `458a773` tree (`1 passed` for each of the three tests).
- Workspace `cargo nextest run --workspace --no-fail-fast` after Task 1's edits: 2001 tests run, 2001 passed (2 leaky), 6 skipped.
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0 after Task 1 and again after Task 2's edits.
- Task 2 `<verify>` chain, run on the tree committed as `52a9488`: exit 0. The three named tests each had `1 passed`. `scripts/check.sh all` printed "all OK", and its log has 45 `test result: ok` lines and no `FAILED` or `panicked`. The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) passed, so no rerun was needed. `scripts/check.sh deps` printed `advisories ok, bans ok, licenses ok, sources ok` and machete "didn't find any unused dependencies", then "deps OK". The advisory database fetch worked.
- Acceptance checks: `ship_evidence.rs`, `version.rs` and `ship.rs` do not exist. `rg 'semver|git-conventional' crates/devflow-core/Cargo.toml` exits 1. `rg 'semver::|git_conventional' crates` outside `version.rs` exits 1 (run before the deletion). The diff of `Cargo.lock` removes only `git-conventional`, `unicase` and winnow's `memchr` edge. `cargo tree -i memchr` still shows `serde_json` using it.
- What this does not establish:
  - `check.sh all` ran on this host, not in the pinned container. The pre-push hook runs the container check.
  - The `rg` absence checks are shell checks. The comment-stripped source scan comes in 52-18.
  - No live DevFlow run was made. The preflight path is covered only by the kept unit tests.
  - The reworded comments were checked for the identifiers they name, not read for accuracy against the code.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-14, which removes the remaining Ship config (`--yes-ship`, `DEVFLOW_YES_SHIP`, `State.yes_ship`).

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

---
phase: 52-subtract-and-stabilize
plan: 12
subsystem: pipeline
tags: [cut-02, cut-04, hooks, finalization, checkout-lock, recover, semver]

requires:
  - phase: 52-08
    provides: no run reaches Ship from Validate; finish_run ends a passing run and keeps its state
  - phase: 52-11
    provides: devflow ship removed, so finish_workflow had no caller but the Ship gate approval
provides:
  - "finish_workflow, finish_workflow_with_gate_timeout and the finalization retry gate are deleted; an approved Ship gate (reachable only from an older state) ends the run through finish_run"
  - "devflow_core::hooks is deleted, with run_checkout_hooks, run_checkout_hooks_from, hook_context_root, BatchProgress and the hook call in transition"
  - "devflow_core::lock is deleted, with recover's stale-lock sweep arms in clean_report and clean_phase_report"
  - "DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS is not read; its OPERATIONS.md, ARCHITECTURE.md and docs/guides/configuration.md mentions are gone"
  - "the --no-deps and --no-ff allowlist entries, the ARCHITECTURE.md Hooks section and lock bullet, the OPERATIONS.md lock rows and .gitignore's .devflow/lock-* line are gone"
affects: [52-13, 52-14, 52-15, 52-18, 52-25]

actuals:
  tokens: 55700
  tasks: 2
  commits: 4
plan_head_before: e9153fc01f2952520a748413a3110bc90a4da6ad
plan_head_after: 92c00d87352cc07f14dfa0e37514c0e3f7d6a0b3

tech-stack:
  added: []
  patterns:
    - "A Ship gate approval in a kept-until-52-15 handler routes to finish_run, so an older state at Ship ends the way a passing Validate does"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/config_parse.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-cli/tests/gitignore_coverage.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-core/src/doc_check.rs
    - ARCHITECTURE.md
    - OPERATIONS.md
    - doc-check-allowlist.toml
    - .gitignore

key-decisions:
  - "handle_ship_outcome's approval arm closes the Ship gate and calls finish_run (state kept, finished, branch kept) rather than erroring. Only a state written by a 3.x binary reaches it, and 52-15 deletes the handler with Stage::Ship."
  - "The DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS reader and its doc rows went in Task 1, not Task 2: deleting the hook batch left the reader dead, and doc_check fails while a doc names a variable the source no longer reads."
  - "The ARCHITECTURE.md Hooks section went in Task 1: doc_check::doc_referenced_identifiers_exist_in_source failed on `BranchCreate` once hooks.rs was gone."
  - "resume no longer adds 'a merge, version bump or tag may already have run' to a claimed Ship answer: nothing in this binary runs after a Ship approval."
  - "git.rs functions that served only the hooks (commit_all) are kept: they are pub in the published crate and outside this plan's list. Logged in deferred-items.md."

patterns-established:
  - "A deleted path's tests are listed by name in the SUMMARY; a kept test whose control asserted the deleted behaviour is re-pointed at the kept one (the --yes-ship control now expects a finish, not a ship)"

requirements-completed: [CUT-02, CUT-04]

coverage:
  - id: D1
    description: "An approved Ship gate ends the run without any hook: no hook_run, no workflow_shipped, one workflow_finished {reason: finished}, state kept finished, feature branch kept"
    requirement: CUT-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::an_approved_ship_gate_finishes_the_run_without_running_a_hook"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every kept transition behaves as before: the Validate finish and the --until halt are unchanged"
    requirement: CUT-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_run_that_passes_validate_finishes_and_keeps_its_worktree"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#an_until_halt_finishes_with_stopped_at_and_notifies_once"
        status: pass
    human_judgment: false
  - id: D3
    description: "The checkout lock and its recover arms are gone; recover's kept tests and the kept fixtures pass without lock files"
    requirement: CUT-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/gitignore_coverage.rs#gitignore_covers_devflow_runtime_state_paths"
        status: pass
      - kind: unit
        ref: "cargo nextest run -p devflow-core --lib -E 'test(/^recover::/) | test(/^doc_check::/)' (65 passed before the deletion; included in check.sh all after it)"
        status: pass
    human_judgment: false
  - id: D4
    description: "No scoped doc or allowlist entry names a removed identifier, flag or env var; every remaining DEVFLOW_ read is documented; fmt, clippy and the whole workspace pass"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
      - kind: other
        ref: "scripts/check.sh all (exit 0, \"all OK\")"
        status: pass
    human_judgment: false
  - id: D5
    description: "The rewritten ARCHITECTURE.md and OPERATIONS.md prose reads correctly"
    verification: []
    human_judgment: true
    rationale: "doc_check verifies only that named identifiers exist; nobody read the edited prose for accuracy"

duration: 32min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 12: Remove the after-ship hooks and the checkout lock Summary

**The finalization path, its retry gate, `devflow_core::hooks` and `devflow_core::lock` are deleted. Nothing in DevFlow merges, bumps a version, tags, writes a changelog entry or deletes a branch any more. A Ship gate approval, which only a 3.x state can still reach, now ends the run the way a passing Validate does.**

## Performance

- **Duration:** 32 min
- **Started:** 2026-11-10T23:37:50Z
- **Completed:** 2026-11-11T00:09:59Z
- **Tasks:** 2
- **Files modified:** 24 (142 insertions, 3736 deletions)

## Accomplishments

- `finish_workflow`, `finish_workflow_with_gate_timeout` and the finalization retry gate are deleted from `pipeline_gate.rs`. So are the `before_finalization` test hold and the `BEFORE_RUN_STATE_CLEAR` probe.
- `transition` no longer runs a hook batch. The dry run no longer prints `↳ hooks:` or `after ship:`.
- `run_checkout_hooks`, `run_checkout_hooks_from`, `hook_context_root` and `BatchProgress` are deleted from `pipeline_outcomes.rs`, and `crates/devflow-core/src/hooks.rs` with `pub mod hooks;`.
- `handle_ship_outcome`'s approval arm closes the Ship gate and calls `finish_run`. Nothing emits `workflow_shipped` now. `ship_evidence` still reads that event; 52-13 deletes it.
- `crates/devflow-core/src/lock.rs` and `pub mod lock;` are deleted, with both `remove_stale_locks_with_outcomes` loops in `recover.rs`.
- `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` is no longer read. Its rows in OPERATIONS.md and `docs/guides/configuration.md`, and its two ARCHITECTURE.md mentions, are gone.
- ARCHITECTURE.md loses the Hooks section (with the DocsUpdate row), the checkout-lock bullet, the `lock-NN` paragraph, the `hooks.rs` and `lock.rs` logging lines, `hook_run` and `checkout_lock_timeout` from the event list, and the `feature_finish … --no-ff` clause. The `--no-deps` and `--no-ff` allowlist entries are gone. `rg -n -- '--no-deps|--no-ff'` over README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md, `docs/guides` and the allowlist finds nothing, and the same search found both in the pre-plan tree.
- OPERATIONS.md loses the `lock-NN` and `lock-project` inventory rows, the recover row's lock clause, and the resume sentence about a merge after a Ship answer. `.gitignore` loses `.devflow/lock-*`, and `gitignore_coverage` no longer expects it.

## Task Commits

1. **Task 1 (tracer): the finish-workflow path, the retry gate, the hook batch and `hooks.rs` are gone:** `1b69c7d` (test, RED), `ae16397` (feat!, GREEN)
2. **Task 2: the checkout lock, its env var, its recover arms and the docs and allowlist entries are gone:** `2d26714` (test, kept fixtures), `92c00d8` (feat!)

**Plan metadata:** recorded in the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from a nextest JUnit report, using a scratch nextest config with `--profile red`. The record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `pipeline_gate::tests::an_approved_ship_gate_finishes_the_run_without_running_a_hook` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on its first planned assertion (pipeline_gate.rs:1023, "no after-ship hook runs"). The captured stdout read `phase 21 shipped`, so the after-ship batch had run. Not a build, load or fixture fault. It passes after `ae16397`. |
| Task 2's kept fixtures (`gitignore_coverage`, `doc_check`'s path fixture, `gate_wedge_e2e`) | No RED | Task 2's behaviour is that kept tests stay green, which no test can fail first. The fixtures were changed to stop expecting lock files and committed first (`2d26714`). They passed before the deletion (gitignore_coverage + gate_wedge_e2e 7/7; `recover::` + `doc_check::` 65/65) and again inside `scripts/check.sh all` after it. |

Gate sequence: `test(52-12)` 1b69c7d → `feat(52-12)!` ae16397 → `test(52-12)` 2d26714 → `feat(52-12)!` 92c00d8. No REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode was `end-of-phase`, and the verify block is automated-only. The Task 1 `<verify>` chain was run before any edit (both tests green), after GREEN, and again on the committed `ae16397` tree (rc 0, 1 and 1 passed). Expansion then continued.

## Tests deleted or changed

- Deleted because their subject was the removed path:
  - `pipeline_gate`: `advance_ship_success_emits_workflow_shipped_and_ship_evidence_reports_shipped`, `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`, `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`, `concurrent_ship_advances_finish_both_phases_independently`, `a_finalization_retry_does_not_repeat_the_version_bump`, `a_finalization_retry_merges_a_repair_without_a_second_tag`, `workflow_shipped_is_recorded_before_the_state_is_cleared`, `a_stop_during_a_finalization_retry_is_honoured_before_the_next_pass`, `without_a_stop_an_approved_finalization_retry_runs_the_next_pass`, `the_safe_point_6_comment_states_what_a_resumed_run_does` (it pinned a comment inside `finish_workflow`), and the helpers `approve_a_gate_with_a_waiter`, `finalization_that_fails_each_pass`, `spawn_engaged_finalization`, `approve_the_retry_gate_and_hold_its_claim`, `hook_runs` and `comment_normal_form`.
  - `pipeline_outcomes`: `checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout`, `terminal_hook_failure_stops_before_branch_cleanup`, `run_checkout_hooks_keeps_changelog_in_sync_with_tag_when_no_version_file`, `checkout_hooks_merge_into_the_persisted_base_not_the_ambient_default`, `content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root`.
  - `pipeline_launch`: `a_stop_before_finalization_is_honoured_before_the_merge`, `without_a_stop_before_finalization_the_run_ships` (both used the deleted `before_finalization` hold; `finish_run`'s safe point 6 has its own test from 52-08), and `a_superseded_supervisor_runs_no_finalization_hooks`.
  - `config_parse`: `parse_checkout_lock_timeout_defaults_and_parses`. `test_support`: `init_repo_no_version_file` (only the deleted hook test used it).
  - `recover`: `clean_phase_report_flags_a_stale_lock_with_unusable_coordination`, `clean_report_flags_a_stale_lock_with_unusable_coordination`, `clean_report_does_not_flag_a_live_stale_lock_notice_as_a_removal_failure`.
  - All 18 tests in `hooks.rs` and every test in `lock.rs`, deleted with their modules.
- Replaced: `advance_ship_success_runs_finish_workflow` became `an_approved_ship_gate_finishes_the_run_without_running_a_hook`, the RED test above.
- Changed because the outcome they asserted was the deleted behaviour:
  - `pipeline_outcomes::handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution` expects a kept, finished state instead of a removed one.
  - `pipeline_launch`'s `held_yes_ship_advance`: the control expects `workflow_finished`, a finished state and a kept branch, and the stop case expects no `workflow_finished`. `without_a_stop_after_safe_point_1_the_yes_ship_run_ships` is renamed `…_finishes`.
  - `pipeline_launch::resume_reports_the_answers_it_set_aside_for_the_closed_gate` expects no Ship caution.
  - `gate_wedge_e2e`'s two recover tests no longer plant a `lock-NN` or expect recover to remove it.
- Kept unchanged and still green: `recover::a_leftover_recycled_pid_lock_file_does_not_block_an_explicit_clean`. A leftover file still blocks nothing.

## Decisions Made

See `key-decisions` above. `cargo semver-checks check-release -p devflow-core` (baseline: published 3.0.0) exits 100 and reports 5 major lint failures. This plan's items among them are `mod devflow_core::hooks` (`Hook`, `HookError`, `HookContext`, `hooks_for_transition`, `hooks_after_ship`, `has_version_file`) and `mod devflow_core::lock` (`LockError`, `LockGuard`, `acquire_project`, `acquire_project_blocking`, `remove_stale_locks`). All of these are intended (D-01, D-23). The other items in the report came from earlier plans: cron-instructions, `legacy_claude_launch`, `reference_path`, `add_detached`, and `orphan_cron_records_removed`.

## Deviations from Plan

**1. [Convention] Commit subjects**
- **Issue:** the plan gave `feat(core)!: remove the after-ship hooks and the checkout lock`. GSD's TDD gate looks for `feat(52-12)`, as in 52-08 to 52-11.
- **Fix:** `feat(52-12)!: …` for both GREEN commits, each with a `BREAKING CHANGE:` footer. The first test commit's subject is 74 characters; the commit-msg hook warned and did not block.

**2. [Rule 3 - Blocking] The env var reader and docs moved into Task 1**
- **Found during:** Task 1. After the hook batch was deleted, `config_parse::checkout_lock_timeout` had no caller (a dead-code warning). Removing it made `doc_check::doc_referenced_identifiers_exist_in_source` fail on `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`.
- **Fix:** the reader, its test and the four doc mentions were removed in `ae16397`, not in Task 2.

**3. [Rule 3 - Blocking] The ARCHITECTURE.md Hooks section moved into Task 1**
- **Found during:** Task 1's workspace run. `doc_referenced_identifiers_exist_in_source` failed: "documented Rust identifier `BranchCreate` does not exist".
- **Fix:** the whole Hooks section, which contains the DocsUpdate row, went in `ae16397`, together with the `--no-deps` allowlist entry it needed. The lock prose and `--no-ff` stayed in Task 2.

**4. [Rule 1 - Consistency] Stale behaviour claims outside the plan's file list**
- **Issue:** `pipeline_launch.rs`'s resume line told the operator a merge or tag may have run after a Ship answer. Comments in `state.rs`, `commands.rs`, `main.rs`, `phase7_cli.rs`, `workflow.rs` and `test_support.rs` named `finish_workflow`, `hooks::`, `lock.rs` or `LockError`. The `state.rs` and `commands.rs` comments said a phase completion resets `phase_validate_failures`, which has not been true since 52-08.
- **Fix:** the resume caution and its OPERATIONS.md sentence are removed, and the comments are reworded. `git.rs` and `preflight.rs` held only comments that used `hooks::`, and those were reworded the same way. No code in either file used a hook type.

**5. [Rule 1 - Coverage] `pipeline_launch` tests outside the plan's list**
- **Issue:** three tests drove the deleted `before_finalization` hold, and the `--yes-ship` control expected a ship.
- **Fix:** see "Tests deleted or changed" above.

**Total deviations:** 5 (1 convention, 2 blocking, 1 consistency, 1 coverage). **Impact:** no behaviour added; the plan's outcome is unchanged.

## Issues Encountered

- A finished run keeps `phase_validate_failures`, and the next `start` for that phase carries the total forward: `carried_phase_failures` copies it from any loadable state. This comes from 52-08, which keeps the state of a finished run. It was read from source and not reproduced end to end. It is logged in `deferred-items.md` for 52-25.
- `GitFlow::commit_all` is now uncalled, beside six git.rs functions that were already uncalled. This is logged for 52-18's breaking-change pass.
- The docs-site pages under `docs/` (other than `docs/guides`) still describe the hooks. 52-18 rewrites them; this is logged.
- `recover --clean` no longer removes `lock-*` files that a 3.x binary left. They block nothing, and the operator rule is that no migration work is done.

## Verification

- Task 1 `<verify>`: exit 0, run before the edits and on `ae16397`.
- Workspace `cargo nextest run --workspace --no-fail-fast` after Task 1's edits: 2036 passed, 0 failed. An earlier run found the two failures that became deviations 3 and 4 (`doc_referenced_identifiers_exist_in_source`, `handle_ship_outcome_with_yes_ship_…`).
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0, before the Task 1 commit.
- Task 2 `<verify>` chain, run on the tree committed as `92c00d8`: exit 0. The three named tests each had `1 passed`, then `scripts/check.sh all` printed "all OK". Its log has 45 `test result: ok` lines and no `FAILED` or `panicked`. The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) passed.
- Acceptance checks: `hooks.rs` and `lock.rs` do not exist. `rg` finds no `pub mod hooks;` in lib.rs (rc 1), no `fn finish_workflow` in pipeline_gate.rs (rc 1), and no `no-deps|no-ff` in the allowlist (rc 1). The pre-plan allowlist had 2 matches.
- What this does not establish:
  - `check.sh all` ran on this host, not in the pinned container. The pre-push hook runs the container check.
  - The `rg` absence checks are shell checks. The comment-stripped source scan comes in 52-18.
  - No live DevFlow run was made. The Ship-approval path is covered only by unit and handoff tests, and only an old state can reach it.
  - The rewritten prose was checked for named identifiers, not read for accuracy.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-13, which deletes `ship_evidence.rs` (its `workflow_shipped` reader now has no writer), the Ship preflight checks, `version.rs` and the rest of `ship.rs`.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

- Commits `1b69c7d`, `ae16397`, `2d26714` and `92c00d8` are ancestors of HEAD; `git rev-list --count e9153fc..HEAD` before this docs commit was 4, and `gsd_run check evaluation-scope --plan 52-12 --commits-only` resolved them.
- `crates/devflow-core/src/hooks.rs` and `crates/devflow-core/src/lock.rs` do not exist.

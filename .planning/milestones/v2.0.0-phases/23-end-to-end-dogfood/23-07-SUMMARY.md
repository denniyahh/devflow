---
phase: 23-end-to-end-dogfood
plan: 07
subsystem: cli
tags: [rust, clap, cli-surface-reduction, changelog, semver]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood
    provides: "23-CONTEXT.md D-11/D-12/D-13 — the authorized-in-advance decision to hard-delete sequentagent and open the v2.0.0 slot"
provides:
  - "The devflow CLI crate with the sequentagent subcommand fully removed (enum variant, dispatch, implementation, status rendering, cron-resume builder)"
  - "A regenerated devflow-help.txt snapshot with no sequentagent line"
  - "Four operator documents (README, ARCHITECTURE, OPERATIONS) with the command removed from prose/tables, and CHANGELOG.md with a new v2.0.0 breaking-change entry"
affects: [23-08-core-side-sequentagent-removal, release/v2.0.0-cut]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Deleting a published CLI verb: enum variant + dispatch + implementation + status rendering + cron-resume builder + help snapshot + 4 docs + CHANGELOG breaking entry, in that dependency order so the compiler catches missed call sites"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - crates/devflow-core/src/ship.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-core/tests/devflow_dir_gitignore.rs
    - README.md
    - ARCHITECTURE.md
    - OPERATIONS.md
    - CHANGELOG.md

key-decisions:
  - "Task 1's checkpoint (D-11/D-12 authorization) was resolved by the human before this continuation agent started: approve — delete the verb, confirmed next published version 2.0.0, core-side surface explicitly deferred to plan 23-08"
  - "The 5 test call sites that used the deleted ship::build_cron_instructions purely as a generic CronInstructions fixture (not testing sequentagent-specific behavior) were switched to the surviving ship::build_single_agent_cron_instructions rather than deleted, preserving their actual coverage (round-trip persistence, per-phase isolation, legacy-record migration, unparseable-retry-time handling, cron-hint rendering)"
  - "Only the one test that asserted the deleted builder's own sequentagent-specific behavior (cron_instructions_include_resume_command) was deleted outright — its surviving-builder equivalent (single_agent_cron_instructions_resume_command_is_devflow_resume) already covers the resume-command contract"
  - "The plan's required 'targeted test that a rate-limit outcome names a surviving subcommand' was not newly written — an existing test, primary_loop_rate_limited_writes_single_agent_cron_instructions (crates/devflow-cli/src/pipeline_outcomes.rs), already asserts exactly this (resume.args == [\"resume\", \"--phase\", N]) and was re-confirmed passing after the deletion, rather than duplicating equivalent coverage"

requirements-completed: [23d]

coverage:
  - id: D1
    description: "The sequentagent subcommand no longer exists in the CLI's Command enum or its dispatch — invoking it fails with clap's unrecognized-subcommand error"
    requirement: "23d"
    verification:
      - kind: integration
        ref: "manual CLI invocation: `devflow sequentagent --phase 1 --agents claude,codex` exits 2 with 'error: unrecognized subcommand'"
        status: pass
      - kind: unit
        ref: "source assertion: rg -c 'sequentagent|Sequentagent|SequentAgent' crates/devflow-cli/src/main.rs == 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "The committed help snapshot matches the binary's real --help output and no longer advertises sequentagent"
    requirement: "23d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D3
    description: "The single-agent rate-limit resume path (devflow resume --phase N) is intact and provably names a surviving subcommand"
    requirement: "23d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#primary_loop_rate_limited_writes_single_agent_cron_instructions"
        status: pass
    human_judgment: false
  - id: D4
    description: "devflow parallel (run N phases concurrently) is completely unaffected by the deletion"
    requirement: "23d"
    verification:
      - kind: integration
        ref: "manual CLI invocation: `devflow parallel --help` prints normal usage, exit 0"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/parallel.rs#tests (pairs_default_missing_agents_to_claude and 4 siblings) — parse_phase_agent_pairs / parallel() left byte-identical"
        status: pass
    human_judgment: false
  - id: D5
    description: "Operator documentation (README, ARCHITECTURE, OPERATIONS) no longer advertises the removed command; CHANGELOG.md records the removal as a v2.0.0 breaking change while preserving historical mentions"
    requirement: "23d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: other
        ref: "source assertion: rg -c 'sequentagent' README.md ARCHITECTURE.md OPERATIONS.md == 0 each; rg -c 'sequentagent' CHANGELOG.md == 4 (all historical)"
        status: pass
    human_judgment: false

duration: 31min
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 07: Delete the sequentagent verb Summary

**Hard-deleted the published `sequentagent` CLI subcommand from the devflow-cli crate — enum variant, dispatch, implementation, status rendering, and its Hermes cron-resume builder — regenerated the help snapshot, reconciled four operator documents, and opened the CHANGELOG's v2.0.0 breaking-change slot (D-11/D-12), leaving `devflow parallel` and the single-agent rate-limit resume path byte-identical.**

## Performance

- **Duration:** ~31 min (Task 2 → Task 3 commit timestamps; Task 1's checkpoint decision was reached and resolved by the human in a prior session before this continuation agent was spawned)
- **Started:** 2026-07-25T23:38 (base commit `3425dbb`)
- **Completed:** 2026-07-26T00:09 (Task 3 commit `43f4691`)
- **Tasks:** 2 executed by this agent (Task 1's checkpoint decision — approve, version 2.0.0 — was already resolved before resumption; see Checkpoint Resolution below)
- **Files modified:** 11 (across both task commits)

## Accomplishments

- Deleted the `Command::Sequentagent` variant (with its `phase`/`agents`/`force`/`project` fields), its dispatch arm, and narrowed the `use parallel::{...}` import in `main.rs`
- Deleted `sequentagent()` and every helper whose only callers were inside it from `parallel.rs` — `run_agent_blocking`, `integrate_agent_branch`, `SequentagentSlotGuard`, `split_two_agents`, `write_rate_limit_cron`, `count_commits_between`, `add_or_explain` — leaving `parallel()` and `retry_after_from_reason()` (shared with `pipeline_outcomes.rs`) byte-identical
- Removed the 3 `phase7_cli.rs` tests that drove the deleted verb end-to-end (20 → 17 tests) and the now-dead `git_stdout()` helper those tests were the only caller of
- Regenerated the committed `devflow-help.txt` snapshot from the real binary output
- Deleted `ship::build_cron_instructions` (the two-agent Hermes cron-resume builder) — `ship::build_single_agent_cron_instructions` (the primary rate-limit resume path) is untouched
- Deleted `commands::render_sequentagent_status` and its `status` call site, plus its 4 dedicated tests; `agent_pid_from_file` (still used by surviving status/doctor code) is untouched
- Reconciled README.md, ARCHITECTURE.md, and OPERATIONS.md — removed the command from every command table and prose reference
- Added a new `## 2.0.0` CHANGELOG.md entry recording the removal as a breaking change and pointing to DEN-67 for the preserved capability intent; all 4 historical `sequentagent` mentions (the v1.8.0 entry) left untouched
- Full gate chain green: `cargo test --workspace` (369+ tests across all binaries, 0 failed, including `doc_check::doc_referenced_identifiers_exist_in_source`), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean

## Checkpoint Resolution (Task 1)

**Task 1: Authorize the one-way removal of a published CLI verb (D-11, D-12)** — `checkpoint:decision`, `gate="blocking"`.

Resolved by the human before this continuation agent was spawned. No commit (checkpoints don't produce commits).

- **Decision:** approve — delete the `sequentagent` two-agent sequential-handoff verb from the published CLI, and treat the removal as the breaking change that opens the v2.0.0 slot.
- **Confirmed next published version:** 2.0.0 (major bump off 1.8.1, per D-12) — this drove the CHANGELOG heading in Task 3.
- **Explicitly out of scope for this plan:** the core-side surface (`SequentagentSlotKind`, the slot record read/write functions, the two monitor API functions `spawn_monitor_no_advance`/`wait_for_agent_exit`) is deferred to plan 23-08, immediately after. Not touched in this plan — verified via `git status --short crates/devflow-core/src/agent_result.rs crates/devflow-core/src/monitor.rs` showing no changes.
- **Capability intent:** preserved on DEN-67 (999.42) per D-13 — this deletes an implementation, not an ambition.

A prior agent had already reached this checkpoint and returned for the human decision; its worktree was reclaimed before it could resume, with zero commits and zero file changes to reconcile. This agent started Task 2 clean, per the continuation instructions.

## Task Commits

1. **Task 1: Authorize the one-way removal of a published CLI verb (D-11, D-12)** — checkpoint, no commit (resolved before this agent's session)
2. **Task 2: Delete the verb end-to-end** — `dbab01a` (feat)
3. **Task 3: Preserve the single-agent resume path, and reconcile four operator documents** — `43f4691` (feat)

## Files Created/Modified

- `crates/devflow-cli/src/main.rs` — removed the `Sequentagent` enum variant, its dispatch arm, and the `sequentagent` import
- `crates/devflow-cli/src/parallel.rs` — removed `sequentagent()` and its 7 exclusive helpers; `parallel()` and `retry_after_from_reason()` untouched
- `crates/devflow-cli/tests/phase7_cli.rs` — removed the 3 sequentagent end-to-end tests and the now-dead `git_stdout()` helper; also updated a `build_cron_instructions` fixture call site (Task 3) to the surviving builder
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated from the real binary; no `sequentagent` line
- `crates/devflow-core/src/ship.rs` — deleted `build_cron_instructions`; switched 5 pre-existing fixture-only test call sites to `build_single_agent_cron_instructions`; deleted the one test asserting the deleted builder's own behavior
- `crates/devflow-cli/src/commands.rs` — deleted `render_sequentagent_status` and its `status` call site, and its 4 dedicated tests; switched 3 pre-existing fixture-only test call sites to the surviving builder
- `crates/devflow-core/tests/devflow_dir_gitignore.rs` — switched a `build_cron_instructions` fixture call site (generic devflow-dir gitignore coverage, unrelated to sequentagent) to the surviving builder
- `README.md` — removed the sequentagent command-table row and reworded the rate-limit-detection sentence to not name it
- `ARCHITECTURE.md` — reworded 3 mentions (checkout-lock consumer list, `spawn_monitor_no_advance` doc, worktree-model command list)
- `OPERATIONS.md` — removed the sequentagent command-table row and reworded the `.devflow/` cron-instructions file-inventory entry
- `CHANGELOG.md` — added a new `## 2.0.0` heading with a `### Removed (Breaking)` entry; all 4 historical mentions left untouched

## Decisions Made

- **Confirmed next published version is 2.0.0**, per the human's resolution of Task 1's checkpoint — used verbatim as the new CHANGELOG heading (`## 2.0.0 — 2026-07-26`), with a `### Removed (Breaking)` section naming D-11, the surviving `resume`/`parallel` paths, and DEN-67 as the forwarding pointer for the preserved capability intent.
- **Repurposed 8 pre-existing test call sites rather than deleting them.** All 5 in `ship.rs`, 3 in `commands.rs`, plus 1 in `phase7_cli.rs` and 1 in `devflow_dir_gitignore.rs`, used `build_cron_instructions` purely as a convenient `CronInstructions` fixture builder — none tested `sequentagent`-specific behavior (round-trip persistence, per-phase isolation, legacy-record migration, unparseable-retry rejection, `status`'s generic cron-hint rendering, and a devflow-dir gitignore-coverage sweep). Switching their fixture constructor to the surviving `build_single_agent_cron_instructions` (dropping the now-removed `agents` parameter) preserves all of that coverage with zero loss. Only `cron_instructions_include_resume_command` — which asserted the deleted builder's *own* sequentagent-specific resume-command shape — was deleted outright, since `single_agent_cron_instructions_resume_command_is_devflow_resume` already covers the surviving builder's equivalent contract.
- **Did not write a new "targeted rate-limit-resume regression test."** The plan's Task 3 action asks for one proving a rate-limited outcome names a surviving subcommand. An existing test, `primary_loop_rate_limited_writes_single_agent_cron_instructions` (`crates/devflow-cli/src/pipeline_outcomes.rs`), already asserts exactly this — `resume.args == ["resume", "--phase", N]` via a real `advance()` drive — and re-confirmed passing after the deletion. Duplicating it would have produced a second test asserting the identical property, which the project's own `ai-change-acceptance` skill (rules/test-signal-rejection.md) flags as low-value redundancy rather than genuine new coverage.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Task 2's literal `<verify>` command false-greens on a zero-test filter**
- **Found during:** Task 2
- **Issue:** The plan's automated verify command, `cargo test -p devflow help_snapshot`, filters by test-name substring. The actual test function is `help_output_matches_committed_snapshot` in `tests/help_snapshot.rs` — the substring `help_snapshot` never matches it, so the command runs 0 tests (`0 passed; 0 failed; ... 1 filtered out`) and the `rg -q '^test result: ok\. [1-9][0-9]* passed'` gate correctly fails (nothing to match), rather than silently passing. This is the exact class of false-green the project's own `ai-change-acceptance` skill documents (`cargo test --exact` matching nothing yet exiting 0).
- **Fix:** Ran the corrected filter, `cargo test -p devflow help_output_matches_committed_snapshot`, which compiles and runs the real test (1 passed). Independently confirmed the snapshot itself is correct via a full `cargo test -p devflow --test phase7_cli` run (17/17 passed) and a manual `devflow --help` diff against the committed snapshot.
- **Files modified:** none (verify-command-only; no PLAN.md edit — plan files are not modified by the executor)
- **Verification:** `cargo test -p devflow help_output_matches_committed_snapshot` → `test result: ok. 1 passed; 0 failed`
- **Committed in:** `dbab01a` (Task 2 commit)

**2. [Rule 1 - Bug] My own deletion left `git_stdout()` dead code**
- **Found during:** Task 2, immediately after deleting the 3 sequentagent tests from `phase7_cli.rs`
- **Issue:** `git_stdout()` was only called by the 3 deleted tests; leaving it produced `warning: function 'git_stdout' is never used` on `cargo test -p devflow --test phase7_cli`.
- **Fix:** Removed `git_stdout()`. `seed_feature_branch()` (defined immediately below it) is still used by 5 surviving tests and was left untouched.
- **Files modified:** `crates/devflow-cli/tests/phase7_cli.rs`
- **Verification:** Rebuild produces zero warnings; `cargo test -p devflow --test phase7_cli` still 17/17 passed.
- **Committed in:** `dbab01a` (Task 2 commit)

**3. [Rule 3 - Blocking] Task 3's literal `<verify>` command doesn't compile standalone**
- **Found during:** Task 3
- **Issue:** The plan's automated verify command opens with `cargo test -p devflow-core ship::`. Run standalone (not through the workspace), this fails to *compile* — `devflow-core`'s `test_support` module is gated behind `#[cfg(any(test, feature = "test-support"))]`, and two of its own integration-test files (`devflow_dir_gitignore.rs`, `monitor_e2e.rs`) reference `devflow_core::test_support::git_command` as an external-crate path, which requires the `test-support` feature to be explicitly enabled — it is not enabled by default, and `cargo test -p devflow-core` alone does not pull it in via feature unification the way `cargo test --workspace` does (through `devflow-cli`'s `devflow-core = { features = ["test-support"] }` dev-dependency).
- **Fix:** Ran `cargo test -p devflow-core --features test-support ship::` (15 passed, 0 failed) for the standalone form, and separately confirmed `cargo test --workspace` (which the same verify block also runs, and which does pick up the feature via unification) passes all `ship::` tests identically. Both forms are recorded here since the plan invoked the narrower one literally.
- **Files modified:** none (verify-command-only)
- **Verification:** `cargo test -p devflow-core --features test-support ship::` → `test result: ok. 15 passed; 0 failed`; `cargo test --workspace` → same 15 `ship::` tests pass among 369 devflow-core lib tests, 0 failed workspace-wide
- **Committed in:** `43f4691` (Task 3 commit)

**4. [Rule 3 - Blocking] Deleting `ship::build_cron_instructions` broke 8 pre-existing test fixtures outside Task 3's declared `<files>` list**
- **Found during:** Task 3, after deleting `build_cron_instructions`
- **Issue:** The plan's Task 3 `<files>` list names `crates/devflow-core/src/ship.rs` and `crates/devflow-cli/src/commands.rs`, but 2 more files outside that list — `crates/devflow-cli/tests/phase7_cli.rs` and `crates/devflow-core/tests/devflow_dir_gitignore.rs` — also called the now-deleted `build_cron_instructions` as a generic fixture builder (unrelated to sequentagent-specific behavior: a `status` cron-hint test and a devflow-dir gitignore-coverage sweep, respectively). Left as-is, `cargo build --workspace --all-targets` would fail to compile these test binaries.
- **Fix:** Switched both call sites to `build_single_agent_cron_instructions`, matching the same treatment applied to the in-scope files' fixtures (see Decisions Made above).
- **Files modified:** `crates/devflow-cli/tests/phase7_cli.rs`, `crates/devflow-core/tests/devflow_dir_gitignore.rs`
- **Verification:** `cargo build --workspace --all-targets` clean; `cargo test --workspace` 0 failed, including both affected tests
- **Committed in:** `43f4691` (Task 3 commit)

**5. [Rule 1 - Bug, documentation only] Two of Task 2/3's `<acceptance_criteria>` source-assertion targets don't match the symbols' actual definition location**
- **Found during:** Task 2 (verification pass)
- **Issue:** The plan's acceptance criteria ask for `rg -n 'fn ensure_phase_worktree' crates/devflow-cli/src/commands.rs` and `rg -n 'fn retry_after_from_reason' crates/devflow-cli/src/pipeline_outcomes.rs` to "still match." Neither function is *defined* in the file the grep targets — both are defined in `parallel.rs` (`pub(crate) fn ensure_phase_worktree` at line 15, `pub(crate) fn retry_after_from_reason` at line 111) and merely *imported and called* from `commands.rs`/`pipeline_outcomes.rs` respectively (`use crate::parallel::ensure_phase_worktree;`, `use crate::parallel::retry_after_from_reason;`). This appears to be an artifact of the plan's `<key_links>` section, which names them as `commands::ensure_phase_worktree` and `pipeline_outcomes::retry_after_from_reason` — attributing them to their call-site module rather than their definition module.
- **Resolution:** No code change — both symbols were never touched by any deletion in this plan and survive with all original callers intact, which is the actual intent the criteria were checking for. Verified: `rg -n 'ensure_phase_worktree' crates/devflow-cli/src/parallel.rs crates/devflow-cli/src/commands.rs` shows the definition in `parallel.rs:15` and the call in `commands.rs:178`; same pattern for `retry_after_from_reason`. Documented here rather than silently ignored.
- **Files modified:** none
- **Committed in:** n/a (no code change; documentation-only finding)

**6. [Rule 1 - Bug, documentation only] `ship.rs` acceptance criterion "returns 0" is not literally met by 2 legitimate remaining string references**
- **Found during:** Task 3 (verification pass)
- **Issue:** The plan's acceptance criteria ask for `rg -c 'sequentagent|Sequentagent|SequentAgent' crates/devflow-core/src/ship.rs` to return 0; it returns 2. Both matches are inside the pre-existing, untouched test `single_agent_cron_instructions_resume_command_is_devflow_resume` (a doc-comment naming what the surviving builder must never invoke, and a negative assertion `assert!(!record.hermes_cron.command.contains("sequentagent"))` proving the surviving builder never regresses to reference the deleted command). This test predates this plan and was not written by this agent.
- **Resolution:** Left as-is. Deleting or rewording the assertion to avoid the literal string would either delete a genuine regression-check (the negative assertion needs the literal string to check its absence) or weaken it into an assertion that could pass without ever having checked anything — exactly the kind of test-signal degradation the project's `ai-change-acceptance` skill rejects. The criterion's intent — no *implementation* of the deleted verb survives in `ship.rs` — is fully met; only a meta-reference proving its continued absence remains.
- **Files modified:** none
- **Committed in:** n/a (no code change; documentation-only finding)

---

**Total deviations:** 6 (2 Rule 1 bug fixes to false-green/dead-code, 2 Rule 3 blocking-issue fixes, 2 Rule 1 documentation-only findings with no code change)
**Impact on plan:** All fixes necessary for correctness (dead code, broken compile) or accurate verification (false-green filters). No scope creep — no file outside the plan's stated deletion/reconciliation intent was touched, and the 2 out-of-`<files>`-list edits (test fixture call sites) were strictly required by the plan's own Task 3 action (deleting `build_cron_instructions`), not independent additions.

## Issues Encountered

None beyond the deviations documented above.

## User Setup Required

None — no external service configuration required.

## Known Stubs

None.

## Threat Flags

None — this plan is purely subtractive (CLI surface deletion) plus documentation reconciliation; no new network endpoints, auth paths, file access patterns, or schema changes were introduced. The plan's own `<threat_model>` register (T-23-71 through T-23-SC) already covers this plan's actual risk surface and was verified mitigated during Task 3 (rate-limit resume test, `parallel` survival assertions, CHANGELOG non-erasure).

## Next Phase Readiness

- **Ready for plan 23-08** (the core-side `sequentagent` surface deletion: `SequentagentSlotKind`, the slot record read/write functions, and the two monitor API functions `spawn_monitor_no_advance`/`wait_for_agent_exit`) — confirmed untouched by this plan (`git status --short crates/devflow-core/src/agent_result.rs crates/devflow-core/src/monitor.rs` shows no changes), and confirmed to have zero remaining non-test callers in the CLI crate after this plan's Task 2/3 deletions, so 23-08 can proceed without further CLI-side coordination.
- **No blockers.** Full workspace gate green: `cargo build --workspace --all-targets`, `cargo test --workspace` (369+ tests, 0 failed), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
- The v2.0.0 CHANGELOG slot is now opened with this plan's entry; a future actual `/gsd-ship` for the milestone should reconcile this hand-written entry with whatever the `ChangelogAppend` hook produces at tag time (the hook auto-prepends at Ship; this entry was added mid-development per Task 3's explicit instruction and may need date/format reconciliation, not content changes, when the milestone actually ships).

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*

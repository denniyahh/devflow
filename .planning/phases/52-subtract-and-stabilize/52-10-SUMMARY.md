---
phase: 52-subtract-and-stabilize
plan: 10
subsystem: pipeline
tags: [cut-03, cut-04, cron, recover, ship, rate-limit, semver]

requires:
  - phase: 52-09
    provides: the rate-limit gate replaced the only cron-instructions writer
  - phase: 52-01
    provides: retry/reset parsing moved into reset_time.rs
provides:
  - "No code under crates/ reads, writes, lists, consumes or cleans the cron-instructions file"
  - "ship.rs holds only ShipError and prepend_changelog"
  - "recover's sweep and explicit clean have no cron arm; CleanReport has no orphan_cron_records_removed"
  - "workflow::clear_state_if has no after_clear step"
  - "test rename devflow_dir_gitignore::all_eight_devflow_constructors_produce_the_gitignore"
affects: [52-13, 52-18, 52-31, 52-32]

actuals:
  tokens: 26160
  tasks: 2
  commits: 5
plan_head_before: 06bcdfaee1dd4566d1c7fbadd123111324dbd3a5
plan_head_after: 7cd7ee63533c140f0a984339d43a5eecbe63874e

tech-stack:
  added: []
  patterns:
    - "A removed file's absence control keeps the file name as a literal path, so it still fails if a writer comes back"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/ship.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-core/src/workflow.rs
    - crates/devflow-core/src/doc_check.rs
    - crates/devflow-core/tests/devflow_dir_gitignore.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/tests/recover_clean_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-cli/tests/gitignore_coverage.rs
    - .gitignore
    - OPERATIONS.md
    - README.md

key-decisions:
  - "The constructor test is named for eight, not six: after the cron writer goes it drives eight .devflow/ entry points (case 3 checks the launch reservation and the monitor spawn). The old name said seven while it drove nine."
  - "workflow::clear_state_if loses its after_clear closure and Cleared loses its after field. The cron removal was its only production use, and the leftover field failed clippy as never read."
  - "failover_e2e keeps one cron-file absence check, through a literal path, because 52-18 lists it as a control site. The two absence checks in the handoff tests are removed: nothing can write the file, so they could not fail."
  - "The SUPV-01 test names that say cron (a_cron_resume_and_an_ordinary_resume_refuse_alike_*) are kept, because the archived 51 evidence file cites them. Their fixture no longer writes a cron file."

patterns-established:
  - "When a feature is removed, a test whose subject was the feature is deleted; a kept test that only seeded or asserted it drops that line"

requirements-completed: [CUT-03, CUT-04]

coverage:
  - id: D1
    description: "The CLI never reads or reports a cron file: launch and Ship finalization do not consume it, status prints no cron hints, and the recover sweep and explicit clean do not mention it"
    requirement: CUT-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/recover_clean_e2e.rs#explicit_clean_of_a_phase_with_nothing_to_clean_says_so"
        status: pass
      - kind: e2e
        ref: "cargo nextest run -p devflow --test recover_clean_e2e --no-tests=fail (13 passed)"
        status: pass
      - kind: unit
        ref: "cargo nextest run -p devflow --bin devflow --no-tests=fail (662 passed)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The core cron functions, types and recover's cron cleanup are gone; recover still cleans state, gate files, stale locks and abandoned markers"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail (1128 passed)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/recover.rs#recover::tests::clean_phase_report_does_not_count_a_failed_removal"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false
  - id: D3
    description: "The kept fixtures assert the kept surface: constructor gitignore test, OPERATIONS fallback-row pin, gitignore path lists"
    requirement: CUT-04
    verification:
      - kind: integration
        ref: "crates/devflow-core/tests/devflow_dir_gitignore.rs#all_eight_devflow_constructors_produce_the_gitignore"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::operations_md_documents_the_fallback_agent_and_its_events"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/gitignore_coverage.rs#gitignore_covers_devflow_runtime_state_paths"
        status: pass
    human_judgment: false
  - id: D4
    description: "OPERATIONS.md, README.md and .gitignore no longer name or describe the cron-instructions file"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)'"
        status: pass
    human_judgment: true
    rationale: "doc_check proves the docs name no missing identifier, not that the reworded OPERATIONS paragraph and README sentence read correctly; a reader should check them against the rate-limit gate"

duration: 23min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 10: Remove the cron-instructions file Summary

**The cron-instructions file has no reader, writer, cleaner, doc or ignore pattern left. The CLI no longer consumes it on launch or Ship finalization, `status` prints no cron hints, and `recover` neither cleans nor reports it. In devflow-core, `ship.rs` is down to `ShipError` and `prepend_changelog`, and `recover.rs` has no cron arm.**

## Performance

- **Duration:** about 23 min
- **Started:** 2026-11-10T22:33:23Z
- **Completed:** 2026-11-10T22:55:57Z
- **Tasks:** 2
- **Files modified:** 16 (96 insertions, 1570 deletions)

## Accomplishments

- CLI: `consume_cron_instructions` and its `cron_instructions_consumed` event are gone from `spawn_agent_and_record` (pipeline_launch.rs) and `finish_workflow_with_gate_timeout` (pipeline_gate.rs). `status` loses `cron_instruction_hints`/`cron_hint_line`. The recover sweep's "removed orphan cron records" line is gone. An empty explicit clean now says `nothing to clean for phase N: no workflow state or gate files`.
- Core: `ship.rs` drops `CronInstructions`, `ResumeCommand`, `HermesCronJob`, `CronInstructionPathKind`, `cron_instructions_path`, `legacy_cron_instructions_path`, `write/load/list/delete/consume_cron_instructions`, `delete_cron_instructions_with_partial_result`, `remove_file_if_still_present`, `build_single_agent_cron_instructions`, `hermes_schedule_from_retry_after` and `shell_quote`. `recover.rs` drops `sweep_orphan_cron_records`, `remove_orphan_cron_record`, `CleanReport::orphan_cron_records_removed`, the explicit clean's cron removal, and the test hook points `CronListed` and `ExplicitClearReturned`.
- `workflow::clear_state_if` loses its `after_clear` step. Its only production use was the cron removal.
- Docs: the OPERATIONS.md `.devflow/` file row for `cron-instructions-NN.json` is removed, including its false "written when…" text. Its prose in three places goes too: the "No `cron-instructions-NN.json` is written" sentence, "never by cron instructions", and "A scheduled rate-limit resume is itself a `devflow resume`…". The README rate-limit sentence now says the run waits at the gate. The `.gitignore` line `.devflow/cron-instructions*.json` is removed.

## Task Commits

1. **Task 1 (tracer): the CLI never reads or reports a cron file:** `c8392f3` (test, RED), `95e81d6` (feat!, GREEN)
2. **Task 2: core functions, recover's cleanup and docs go:** `e39d04d` (test, kept fixtures rewritten), `5a805d2` (feat!)
3. **Follow-up:** `7cd7ee6` (docs: two test doc comments that still claimed a removed absence assertion)

## TDD Gate Compliance

RED evidence came from a nextest JUnit report, using a scratch nextest config with `--profile red`. The record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `recover_clean_e2e::explicit_clean_of_a_phase_with_nothing_to_clean_says_so` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on the new exact-line assertion. The output was `nothing to clean for phase 66: no workflow state, gate files or cron record`, which names the behaviour being removed. Not a build, load or fixture fault. It passes after `95e81d6`. |
| Task 2's three kept fixtures | No RED, as the plan says | They were rewritten first and run before the deletion, and all passed (`devflow_dir_gitignore` 2/2, the doc_check pin, `gitignore_covers_devflow_runtime_state_paths`, and also `doc_check::gitignore_covers_all_devflow_paths`). They were run again after the deletion and all passed. |

Gate sequence: `test(52-10)` c8392f3 → `feat(52-10)!` 95e81d6 → `test(52-10)` e39d04d → `feat(52-10)!` 5a805d2. No REFACTOR commit.

## Tests deleted (subject was the cron file)

- `crates/devflow-cli/tests/recover_clean_e2e.rs`: `sweep_removes_and_names_an_orphan_cron_record`, `sweep_keeps_a_cron_record_for_a_phase_that_still_has_state`
- `crates/devflow-cli/tests/phase7_cli.rs`: `status_prints_cron_hint_when_cron_instructions_exist`
- `crates/devflow-cli/src/commands.rs`: `cron_instruction_hints_include_hermes_command_per_phase`, `cron_hint_line_appends_sanitized_reset_when_retry_after_present`, `cron_hint_line_omits_reset_fragment_when_retry_after_empty`, `cron_hint_line_never_emits_the_unsupported_devflow_intake_flag`, `cron_hint_line_command_quoting_roundtrips_through_shell_for_space_and_apostrophe_paths`
- `crates/devflow-cli/src/pipeline_launch.rs`: `failed_relaunch_preserves_the_phase_cron_instructions_record`
- `crates/devflow-core/src/ship.rs`: `cron_instructions_save_load_round_trips`, `delete_cron_instructions_is_idempotent`, `delete_cron_instructions_counts_a_legacy_record_it_removes`, `cron_instructions_are_per_phase`, `legacy_cron_instructions_are_read_and_deleted`, `consume_cron_instructions_deletes_per_phase_record`, `consume_cron_instructions_deletes_matching_legacy_record`, `consume_cron_instructions_reports_both_deleted_records`, `consume_cron_instructions_preserves_foreign_legacy_record`, `consume_cron_instructions_preserves_unreadable_legacy_record`, `consume_cron_instructions_tolerates_a_racing_concurrent_consumer`, `hermes_schedule_rounds_up_to_nearest_minute`, `hermes_schedule_normalizes_negative_offset`, `cron_schedule_parses_all_iso8601_offset_forms`, `hermes_schedule_formats_unix_seconds`, `shell_quote_leaves_common_safe_chars_unquoted`, `shell_quote_quotes_unsafe_input`, `single_agent_cron_instructions_resume_command_is_devflow_resume`, `cron_instructions_reject_unparseable_retry_time`. The offset forms are still pinned in `reset_time::tests::parse_offset_minutes_bounds_and_forms`.
- `crates/devflow-core/src/recover.rs`: `clean_still_deletes_unconsumed_cron_instructions`, `clean_phase_deletes_only_the_named_phase_cron_record`, `clean_report_retains_a_removed_orphan_cron_record`, `clean_phase_report_counts_a_lone_cron_record`, `explicit_clean_removes_the_cron_record_before_it_releases_the_write_lock`, `the_orphan_cron_sweep_keeps_the_record_of_a_run_that_reserved_after_its_listing`

## Kept tests rewritten

- `pipeline_gate::tests::advance_ship_success_emits_workflow_shipped_and_ship_evidence_reports_shipped`: no cron seeding, no consume-event assertion.
- `pipeline_launch::tests::resume_with_agent_hands_off_and_relaunches_under_the_new_driver` and `resume_with_agent_from_a_rate_limited_state_relaunches`: no cron seeding, no absence assertion.
- `pipeline_launch` fixture `save_rate_limit_paused` (used by the three `a_cron_resume_and_an_ordinary_resume_refuse_alike_*` tests): keeps the not-stopped, one-infra-fault state and writes no cron file.
- Seven absence assertions (`!cron_instructions_path(..).exists()`) were removed from `pipeline_launch` handoff and relaunch tests, two from `pipeline_outcomes`, and two from `failover_e2e`. With no writer anywhere, none of them could fail.
- `failover_e2e::a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` keeps its absence check through `Rig::cron_file`, which is now a literal path.
- `recover::tests::write_phase_artifacts` no longer writes a cron record. `recover::tests::clean_phase_report_does_not_count_a_failed_removal` now uses a state file that cannot be removed (a directory in its place). It asserts the warning `could not clear phase 8's state`, which is the reworded message of the kept failure path.
- `devflow_dir_gitignore`: `all_seven_…` is renamed `all_eight_devflow_constructors_produce_the_gitignore`, and its cron case is removed.
- `doc_check`: `FALLBACK_ROW_FRAGMENTS` drops the cron row. The missing-row planted control now uses ``| `devflow status``. `gitignore_covers_all_devflow_paths` drops the two cron paths.
- `gitignore_coverage::RUNTIME_PATHS` drops both cron paths.

There was no JSON body for recover or doctor: `CleanReport` has no `Serialize` and is printed as text only. So the plan's "JSON-shape test" has nothing to update. The text renderer is what changed.

## Decisions Made

See key-decisions.

## Deviations from Plan

**1. [Rule 3 - Blocking] `workflow::clear_state_if`'s `after_clear` removed**
- **Found during:** Task 2
- **Issue:** After the explicit clean dropped its cron removal, every caller passed `|| ()`. `Cleared::after` was read only in tests, so `cargo clippy -D warnings` failed on "field `after` is never read".
- **Fix:** The closure parameter and the field were removed (`pub(crate)`, not a semver change). The primitive test `workflow::tests::clear_state_if_decides_on_a_fresh_read_under_the_write_lock` keeps its decision, error, timeout and removal cases. It no longer checks that a step runs after the removal while the lock is still held, because no such step exists.
- **Files modified:** `crates/devflow-core/src/workflow.rs` (not in `files_modified`)
- **Committed in:** 5a805d2

**2. [Rule 2 - Doc accuracy] README rate-limit sentence**
- **Found during:** Task 2
- **Issue:** README.md said DevFlow "writes a per-phase `.devflow/cron-instructions-{phase:02}.json` for rescheduling". That has been false since 52-09.
- **Fix:** It now says the run waits at the stage's gate, which names `devflow resume --phase N` and the reset time. README.md was not in `files_modified`.
- **Committed in:** 5a805d2

**3. [Rule 1 - Shape] Constructor test count**
- **Found during:** Task 2
- **Issue:** The plan expected six constructors. The test drives eight after the removal: seven numbered cases, with case 3 checking both `monitor::Reservation::new` and `monitor::spawn_monitor`. The old name said seven, but the test drove nine.
- **Fix:** The test is named `all_eight_…`, as the plan allows ("name it for the real count and say so").
- **Committed in:** e39d04d

**4. [Rule 3 - Convention] Commit subjects**
- **Issue:** The plan gave the subject `feat(core)!: remove the cron-instructions file`. GSD's TDD gate looks for `feat(52-10)`, as 52-08 and 52-09 did.
- **Fix:** `feat(52-10)!: remove the cron-instructions file` (Task 2) and `feat(52-10)!: stop reading and reporting the cron-instructions file` (Task 1). Both carry BREAKING CHANGE footers.

**Total deviations:** 4 (1 blocking fix, 1 doc accuracy, 1 count, 1 convention). **Impact:** no scope change. workflow.rs and README.md were touched outside the plan's list.

## Issues Encountered

- `cargo semver-checks check-release -p devflow-core` exits 100, as expected for the planned 4.0.0. It reports this plan's removals: the 13 `ship::` items listed in the `5a805d2` footer and `CleanReport.orphan_cron_records_removed`. It also reports the earlier phase-52 removals (`worktree::add_detached`, `worktree::reference_path`, `config::claude_legacy_launch`, `State.legacy_claude_launch`). The major bump has not been made yet.
- Comments and test wording that still describe a "resume cron" or "cron arm" remain in agent_result.rs, monitor.rs, state.rs, reset_time.rs, staleness.rs, pipeline_outcomes.rs and pipeline_launch.rs tests. They are logged in `deferred-items.md` for 52-18 or 52-31/52-32. None is outside a comment except test names and messages, and the `Cron arm: ` label of the SUPV-01 evidence check.

## Verification

- Task 1 verify: `cargo nextest run -p devflow --test recover_clean_e2e --no-tests=fail` exit 0 (13 passed; 15 before, minus the 2 deleted); `… --test failover_e2e …` exit 0 (7 passed); `… --bin devflow …` exit 0 (662 passed, 4 skipped). `phase7_cli` exit 0 (28 passed). The tracer gate re-ran the verify after the change, in interactive mode with end-of-phase verification and only `<automated>` checks, so it continued.
- Task 2 verify: `devflow_dir_gitignore` exit 0 (2 passed); `doc_check::operations_md_documents_the_fallback_agent_and_its_events` exit 0; `gitignore_covers_devflow_runtime_state_paths` exit 0; `doc_check::doc_referenced_identifiers_exist_in_source` exit 0; `cargo nextest run -p devflow-core --lib` exit 0 (1128 passed); `scripts/check.sh all` exit 0 ("all OK") on the first run. The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) passed.
- Acceptance criteria, each checked against the commit before the plan (`HEAD~4`) as a negative control: in `ship.rs`, `fn write_cron_instructions` occurs 1 time before and 0 now, `fn shell_quote` 3 before and 0 now, and `fn hermes_schedule_from_retry_after` 1 before and 0 now. `.gitignore`: 1 cron line before, 0 now. CLI `src` files naming `consume_cron_instructions` or `orphan_cron_records_removed`: 3 before, 0 now. Under `crates/`, the remaining `cron-instructions` text is two doc comments and the literal path in `failover_e2e`.
- What this does not establish: `check.sh all` ran on this host, not in the pinned container (the pre-push hook runs that). A `cron-instructions-NN.json` that 3.x left on an operator's disk is now never cleaned. It stays git-ignored only through `.devflow/.gitignore` (`*`), since the root pattern is gone. The plan's VALIDATION manual-only row has close-out check the operator's `.devflow/`. The rewritten README and OPERATIONS prose is checked only for identifiers, not read for accuracy.

## User Setup Required

None.

## Next Phase Readiness

`ship.rs` holds only `ShipError` and `prepend_changelog`, which keeps 52-13's Ship-module removal small. 52-18's token scan should find `cron-instructions` only in the `failover_e2e` control it already lists, once comments are stripped.

## Self-Check: PASSED

- Files: every modified file listed exists. The renamed test ran by exact name: `devflow_dir_gitignore` ran 2 of 2.
- Commits c8392f3, 95e81d6, e39d04d, 5a805d2 and 7cd7ee6 are ancestors of HEAD. The count from `plan_head_before` is 5.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

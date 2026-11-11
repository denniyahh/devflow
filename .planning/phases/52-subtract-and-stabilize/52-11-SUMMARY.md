---
phase: 52-subtract-and-stabilize
plan: 11
subsystem: cli
tags: [cut-02, cut-04, ship, stop, hints, help-snapshot]

requires:
  - phase: 52-08
    provides: no run reaches Stage::Ship, so the ship command had nothing left to drive
  - phase: 52-10
    provides: the cron-instructions file was gone (previous plan in the cut order)
provides:
  - "`devflow ship` is an unknown subcommand and absent from `--help` (REMOVED_SUBCOMMANDS gains ship)"
  - "pipeline_gate::ship_override, claim_ship_answer, commands::ship_answer_stored and refuse_ship_over_a_live_supervisor are deleted"
  - "stop no longer stores an abort at a Ship gate with no waiter; it writes nothing and names the no-waiter repair, as at every other stage"
  - "DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS is not read; its OPERATIONS.md and docs/guides/configuration.md rows are gone"
  - "No string under crates/ or in the scoped docs names `devflow ship`"
affects: [52-12, 52-13, 52-15, 52-18, 52-19]

actuals:
  tokens: 40230
  tasks: 2
  commits: 3
plan_head_before: 213e51712ec1a5a11a2ab3cf5a41a9cb22553d7e
plan_head_after: 3c257b40206d7d0bbb96dbf270fc9b6c31c36284

tech-stack:
  added: []
  patterns:
    - "A stop test that needs a waiter at a non-Ship gate registers one in the test process with Gates::open_registered and finishes it at the end"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/config_parse.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/tests/stop_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/recover.rs
    - OPERATIONS.md
    - README.md
    - ARCHITECTURE.md
    - docs/guides/configuration.md

key-decisions:
  - "gate approve keeps storing a Ship answer when no waiter is registered (refuse_a_gate_with_no_waiter's Ship bypass). About a dozen tests seed generic gate behaviour at that path; it goes with the Stage::Ship variant in 52-15 (logged in deferred-items.md). Its messages already name resume/recover, not ship."
  - "finish_workflow_with_gate_timeout is kept: 52-12 deletes the finalization path. workflow_shipped_is_recorded_before_the_state_is_cleared now calls it directly instead of through ship_override, so that ordering stays pinned until then."
  - "no_waiter_repair, stop_no_waiter_repair, response_pickup_message and resume_gate_lines lose the parameter that only chose their Ship arm."

patterns-established:
  - "A hint whose state can occur only through Ship is deleted, with the test whose subject it was; a kept test that only asserted the hint drops that assertion"

requirements-completed: [CUT-02, CUT-04]

coverage:
  - id: D1
    description: "`devflow ship` is rejected by clap as an unrecognized subcommand and is not listed in `devflow --help`"
    requirement: CUT-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_subcommands_are_unknown"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D2
    description: "stop at a gate with no waiter writes nothing at any stage; the kept stop e2e tests stay green (41/41), two re-seeded at a Validate gate"
    requirement: CUT-02
    verification:
      - kind: e2e
        ref: "cargo nextest run -p devflow --test stop_e2e --no-tests=fail (41 passed)"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stop_reaches_the_holder_past_a_claimed_ship_gate"
        status: pass
    human_judgment: false
  - id: D3
    description: "DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS is not read, every remaining DEVFLOW_ read and Command variant is documented, and no scoped doc names `devflow ship`"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every operator message that named `devflow ship` names an existing command instead or is deleted, and the whole workspace passes fmt, clippy and tests"
    requirement: CUT-04
    verification:
      - kind: other
        ref: "scripts/check.sh all (exit 0, \"all OK\")"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::recovery_hints_for_a_gone_gate_pending_phase_name_resume_not_advance"
        status: pass
    human_judgment: false
  - id: D5
    description: "The rewritten OPERATIONS.md, README.md and ARCHITECTURE.md prose reads correctly"
    verification: []
    human_judgment: true
    rationale: "doc_check verifies only that named identifiers exist; nobody read the edited prose for accuracy"

duration: 25min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 11: Remove `devflow ship` Summary

**`devflow ship` is now an unknown subcommand. Its override path, the foreground gate timeout variable, `stop`'s Ship special case and every message that sent the operator to it are gone.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-11-10T23:04:49Z
- **Completed:** 2026-11-10T23:29:57Z
- **Tasks:** 2
- **Files modified:** 17 (137 insertions, 1893 deletions)

## Accomplishments

- `Command::Ship` and its dispatch are deleted. `devflow ship` fails as "unrecognized subcommand", and the help snapshot is regenerated: the `ship` line is gone, and the `stop` line no longer names `devflow ship`.
- `ship_override`, `claim_ship_answer`, `ship_answer_stored`, `refuse_ship_over_a_live_supervisor`, `foreground_gate_timeout_secs` and its parser are deleted, along with the `ship_override_refused` event.
- `stop` treats a Ship gate like any other. With no live waiter it writes nothing and names the no-waiter repair.
- `status` and `doctor` no longer suggest `devflow ship`: the stored-Ship-answer facts field, detail line and repairs are deleted.
- OPERATIONS.md loses the `devflow ship` row, the env var row and the `stop` row's Finalization paragraph. README, ARCHITECTURE and `docs/guides/configuration.md` lose their mentions.

## Task Commits

1. **Task 1 (tracer): `devflow ship` is gone end to end:** `8c4e873` (test, RED), `e1345c1` (feat!, GREEN)
2. **Task 2: no message sends the operator to `devflow ship`:** `3c257b4` (fix)

**Plan metadata:** recorded in the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from a nextest JUnit report, using a scratch nextest config with `--profile red`. The record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `phase7_cli::removed_subcommands_are_unknown` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | It failed on the planned assertion (phase7_cli.rs:588, "must be rejected by clap as unknown"). Clap's stderr was `the following required arguments were not provided: --phase <PHASE>`, which shows `ship` was still a subcommand. Not a build, load or fixture fault. It passes after `e1345c1`. |

Tracer gate: the run was interactive (auto_advance false), the mode was `end-of-phase`, and the verify block is automated-only. The Task 1 `<verify>` chain was re-run after GREEN and exited 0 (1, 1, 41, 1 and 1 passed). Expansion then continued.

## Hint sites (Task 2 acceptance)

| Site | Old text | New text |
|------|----------|----------|
| `commands.rs::recovery_hints`, GONE with a stored Ship answer | `devflow ship --phase N` instead of resume | `devflow resume --phase N` |
| `recovery_hints`, GONE at a pending Ship gate | extra hint `devflow ship --phase N` | deleted: state cannot occur without Ship |
| `recovery_hints`, STALE with a stored Ship answer | `devflow stop --phase N`, `devflow ship --phase N` | `devflow stop --phase N`, `devflow resume --phase N` |
| `doctor` gate-pending finding detail | `… and its gate holds a stored answer` | deleted: always `… but no gate file is open` |
| `doctor::repair_instead_of_resume` | `devflow ship --phase N` when a Ship answer is stored | deleted arm; falls through to `devflow resume --phase N` |
| `doctor` STALE supervisor repair | `devflow stop --phase N && devflow ship --phase N` when stored | always `devflow stop --phase N && devflow resume --phase N` |
| `no_waiter_repair` at Ship (gate approve/reject, gate sweep, stop, doctor) | `run \`devflow ship --phase N\` when you are ready to complete Ship` | `run \`devflow resume --phase N\` or \`devflow recover --clean --phase N\` to establish recovery` (the text every other stage already used) |
| `response_pickup_message` at Ship, no waiter | `no confirmed waiter remains; use \`devflow ship --phase\` for Ship recovery` | `no confirmed waiter remains; the response may be stale, so use the named recovery` |
| `stop`'s holder-still-running line | `…records the stop itself, except a \`devflow ship\` that has started its finalization pass: it runs the pass to the end and, if the pass succeeds, ships instead of stopping` | `…records the stop itself` |
| `stop`'s holder-exited, no-state line | `…rather than at a safe point (a \`devflow ship\` that had started its finalization pass runs it to the end and ships), so nothing was marked stopped` | `…rather than at a safe point, so nothing was marked stopped` |
| `pipeline_launch.rs::resume_gate_lines`, Ship answer set aside | `resume: the rerun's Ship gate asks again — answer it with \`devflow gate approve N ship\`; next time, … run \`devflow ship --phase N\` instead of \`resume\`` | deleted: state cannot occur without Ship |
| `pipeline_gate.rs` gate wait, answer already consumed | `another consumer (\`devflow ship\`) acted on it` | `another consumer acted on it` |
| `ship_override`'s messages, `claim_ship_answer`'s refusals, `refuse_ship_over_a_live_supervisor` (`ship refused: …`) | various, naming `devflow ship` | deleted with the command |
| `main.rs` `Stop` help | `(a Ship gate with no waiter stores the rejection for \`devflow ship\`)`; holder list `…, \`devflow ship\`, a hand-run \`advance\`` | clause deleted; `a \`start\` or \`resume\` before its spawn, a hand-run \`advance\`` |

The plan said not to name `gate abort` (52-19), and no new text does.

## Tests deleted, re-seeded or changed

- Deleted because their subject was the ship command or `stop`'s Ship special case:
  - `pipeline_gate`: the nine `ship_override_*` tests (advances, bounds the foreground wait, abort routes, refuses not at Ship, refuses no response, refuses when claimed, refuses unbound, refuses beside a holder, ships a gate-approve approval), `ship_override_and_a_live_waiter_cannot_both_consume_one_approval`, `ship_waits_out_a_recovery_past_its_liveness_check`, and `a_stop_inside_the_ship_finalization_pass_lands_after_the_pass_ends` with its MidMerge fixture.
  - `stop_e2e`: `a_real_stop_lets_a_real_ship_finish_the_pass_it_started` (and `ShipChild`, `git_stdout`), and `stop_at_a_ship_gate_stores_the_abort_and_signals_no_recycled_holder`.
  - `commands`: `stop_at_a_ship_gate_without_a_holder_keeps_manual_recovery_available`, `status_and_doctor_name_ship_for_a_stored_ship_answer`, `reconcile_phase_open_ship_gate_with_no_waiter_names_ship` (it became identical to the Validate case above it).
  - `pipeline_launch`: `devflow_ship_refuses_while_a_supervisor_of_the_phase_is_live`, `resume_names_only_the_actions_left_for_a_ship_answer_it_set_aside`.
  - `config_parse`: `parse_foreground_gate_timeout_env_override`.
- Re-seeded at a Validate gate, because their outcome changed once `stop`'s Ship case was gone:
  - `stop_e2e::stop_is_idempotent_against_an_already_answered_gate`. Seeded at Ship with no waiter, the first stop wrote nothing (`left: 0, right: 1`). It now registers a Validate waiter in the test process.
  - The control half of `commands::stop_reaches_the_holder_past_a_claimed_ship_gate`. It was an unclaimed Ship gate whose abort was stored. It is now an unclaimed Validate gate with a registered waiter: the abort is written and the holder is not signalled.
- Kept without change, outcome checked: `stop_e2e::stop_says_a_claimed_gate_was_not_answered_again` (the claim check runs before the removed case), `stop_e2e::stop_refuses_to_signal_a_holder_whose_start_time_does_not_match` (still covers the recycled-holder property).
- Re-driven: `pipeline_gate::workflow_shipped_is_recorded_before_the_state_is_cleared` now calls `finish_workflow_with_gate_timeout` directly.
- Renamed or reworded: `recovery_hints_for_a_gone_gate_pending_phase_name_resume_or_ship_not_advance` became `…_name_resume_not_advance`, and its Ship case now expects only resume. `gate_approve_with_no_waiter_at_the_ship_gate_writes_and_names_ship` became `…_still_writes`. `gate_respond_answers_a_registered_waiter_and_refuses_when_none_is_registered` drops its ship-hint assertion.

## Decisions Made

See `key-decisions` above. Also: the core edits (`gates.rs`, `git.rs`, `monitor.rs`, `recover.rs`) change comments only, so no `pub` item changed and `cargo semver-checks` was not run for this plan.

## Deviations from Plan

**1. [Convention] Commit subject**
- **Issue:** the plan gave `feat(cli)!: remove devflow ship`. GSD's TDD gate looks for `feat(52-11)`, as 52-08 to 52-10 did.
- **Fix:** `feat(52-11)!: remove devflow ship`, with a `BREAKING CHANGE:` footer.

**2. [Rule 3 - Blocking] The doc mentions moved into Task 1**
- **Found during:** Task 1's verify, `doc_check::doc_referenced_identifiers_exist_in_source`, which failed with "documented command `devflow ship` does not exist in the CLI enum surface".
- **Fix:** the README, ARCHITECTURE and OPERATIONS mentions of `devflow ship` were removed in `e1345c1`, not in Task 2 as the plan ordered. They are listed under Accomplishments.

**3. [Rule 1 - Consistency] Hints that read the deleted `ship_answer_stored` changed in Task 1**
- **Issue:** `recovery_hints` and the doctor findings read `ship_answer_stored`, so deleting it in Task 1 forced their Ship branches out in the same commit.
- **Fix:** those hint rows in the table above landed in `e1345c1`. The rest landed in `3c257b4`.

**4. [Rule 1 - Coverage] Two Ship-seeded tests re-seeded, one control rewritten**
- See "Re-seeded" above. The plan named `stop_says_a_claimed_gate_was_not_answered_again` as a possible re-seed. It did not change outcome, so it was kept. The tests that did change outcome are the two listed.

**5. [Scope] Comment-only edits in four devflow-core files**
- **Issue:** comments in `gates.rs`, `git.rs`, `monitor.rs` and `recover.rs` still described `devflow ship` as a live command.
- **Fix:** reworded those comments. These files were not in the plan's list.

**Total deviations:** 5 (1 convention, 1 blocking, 1 consistency, 1 coverage, 1 scope). **Impact:** no scope added beyond the plan's stated outcome.

## Issues Encountered

- One stored-answer path is left on purpose. `gate approve|reject` at a Ship gate with no waiter still writes the answer, and now nothing consumes it. It is logged in `deferred-items.md` for 52-15. The message the operator sees names resume/recover.

## Verification

- Task 1 `<verify>` chain: exit 0 (removed_subcommands 1 passed; help snapshot 1 passed; stop_e2e 41 passed; both doc_check pins 1 passed).
- `cargo nextest run -p devflow --bin devflow --no-tests=fail --no-fail-fast` before the GREEN commit: 646 passed.
- Task 2: the `doc_check::doc_referenced_identifiers_exist_in_source` pin passed, and then `scripts/check.sh all` exited 0 ("all OK") on the first run, on the tree that was committed as `3c257b4`. The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) did not show.
- `rg -n 'devflow ship' crates README.md OPERATIONS.md ARCHITECTURE.md CONTRIBUTING.md docs/guides` finds nothing. `rg` on `FOREGROUND_GATE_TIMEOUT` in `config_parse.rs` exits 1, which means no match.
- What this does not establish:
  - `check.sh all` ran on this host, not in the pinned container. The pre-push hook runs the container check.
  - The `rg` scans are shell checks, not the comment-stripped source scan that 52-18 adds.
  - The docs site (`docs/index.md`, `docs/architecture/*`) was not swept. That belongs to 52-18.
  - The rewritten prose was checked only for identifiers, not read for accuracy.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-12, which deletes the finalization path (`finish_workflow*`, hooks, checkout lock). The remaining tests that drive it (`workflow_shipped_is_recorded_before_the_state_is_cleared`, the finalization-retry tests) go with it.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*

## Self-Check: PASSED

- Commits `8c4e873`, `e1345c1` and `3c257b4` are ancestors of HEAD. `git rev-list --count 213e517..HEAD` before this docs commit was 3.
- `crates/devflow-cli/tests/phase7_cli.rs` contains `"ship"` in `REMOVED_SUBCOMMANDS`.

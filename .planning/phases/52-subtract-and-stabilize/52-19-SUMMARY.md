---
phase: 52-subtract-and-stabilize
plan: 19
subsystem: gates
tags: [gates, cli, gate-abort, serde, semver-break]

requires:
  - phase: 52-18
    provides: removed-surface scan and the Plan → Code → Validate pipeline the gates answer for
provides:
  - "GateResponse.abort: bool (#[serde(default)]); GateAction::from_response reads fields, never the note"
  - "GateError::ApprovedAndAborted: Gates::answer refuses an answer that approves and aborts"
  - "devflow gate abort <phase> [--stage] [--gate-id] [--note] [--project] (GateCmd::Abort)"
  - "GateAnswerKind (Approve, Reject, Abort) as gate_respond's answer input"
  - "Gates::reap_answer (gate sweep, devflow stop) writes abort: true"
affects: [52-20, 52-21, 52-32, gate answers, devflow stop, gate sweep]

actuals:
  tokens: 12900
  tasks: 3
  commits: 6
plan_head_before: 958d1aec5576f12a4cfef0b094a5227f02791234
plan_head_after: 3d785521ac54316b5c3792959c70fd192a081c8f

tech-stack:
  added: []
  patterns:
    - "A gate answer's action comes from its fields only; a note is a reason, never a command"
    - "Approve+abort is refused in the one shared writer (answer_with_hook), and read as an abort if found on disk"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/gates.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - OPERATIONS.md
    - ARCHITECTURE.md

key-decisions:
  - "gate_respond takes a GateAnswerKind enum (Approve, Reject, Abort) instead of two bools, so the CLI cannot even express approve+abort; Gates::answer still refuses it for every other writer"
  - "The approve+abort refusal lives in answer_with_hook, the writer Gates::answer and Gates::reap_answer share, so no writer can skip it; the new variant is GateError::ApprovedAndAborted"
  - "from_response checks abort before approved, so no record can both abort and advance; a hand-written approve+abort reads as Abort with a fixed reason naming the invalid answer"
  - "reap_answer's abort: true landed in Task 1, not Task 2: once the note stopped meaning abort, leaving it for Task 2 would have made gate sweep and devflow stop relaunch agents between commits"
  - "The sweep and stop notes keep their `abort: …` reason text: they are now only reasons, and changing them was not needed"

patterns-established:
  - "Abort fixtures set the field: JSON fixtures write \"abort\":true, struct literals abort: true, pipeline_launch tests use abort_response(note)"

requirements-completed: [GATE-01]

coverage:
  - id: D1
    description: "A rejection never ends a run, whatever its note says; `devflow gate abort` ends a live gated advance"
    requirement: GATE-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_note_saying_do_not_abort_keeps_the_run_alive"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#gate_abort_ends_the_run"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#gate_abort_writes_an_answer_with_the_abort_field_set"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#cleanup_worktree_cli_flow"
        status: pass
    human_judgment: false
  - id: D2
    description: "The mapping table, the approve+abort refusal and the reap writer are pinned by unit tests"
    requirement: GATE-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_note_containing_abort_does_not_abort"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::an_explicit_abort_field_aborts"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::an_answer_that_approves_and_aborts_is_refused"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_hand_written_approve_and_abort_reads_as_abort"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_record_without_the_abort_field_is_not_an_abort"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::reap_answer_writes_an_abort_for_the_observed_gate_only"
        status: pass
    human_judgment: false
  - id: D3
    description: "`gate abort` prints `aborted` and its gate_response_written event carries \"abort\"; OPERATIONS.md documents the three answers"
    requirement: GATE-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_respond_reports_an_abort"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 19: Explicit Gate Abort Summary

**A gate answer now aborts only through its new `abort` field, written by `devflow gate abort`, `gate sweep` or `devflow stop`. A rejection whose note says "do not abort, fix the test" now loops back where it used to end the run. `Gates::answer` refuses an answer that both approves and aborts, and such an answer, if written by hand, reads as an abort.**

## Performance

- **Duration:** about 25 min
- **Started:** 2026-11-11T04:36:49Z
- **Completed:** 2026-11-11T05:01:26Z
- **Tasks:** 3
- **Files modified:** 17

## Accomplishments

- `GateResponse` gains `#[serde(default)] pub abort: bool`. `from_response` checks `abort`, then `approved`, then loops back. The note is never read for the action.
- `devflow gate abort` (`GateCmd::Abort`) writes `abort: true`. Its pickup line opens with `aborted`, and its `gate_response_written` event carries `"abort": true`.
- `Gates::reap_answer` writes `abort: true`, so `gate sweep` and `devflow stop` still end runs. `stop_with_existing_response` decides through `from_response`, which now reads the field.
- An answer that approves and aborts is refused with `GateError::ApprovedAndAborted`, and nothing is written. If one is written by hand, it reads as `Abort` with the reason `the gate answer both approves and aborts, which is invalid; it is read as an abort`.
- OPERATIONS.md documents the three answers in the command table, in "Answering gates" and in the hand-written answer body. The rate-limit, failover, stop and sweep text no longer mention an abort note. ARCHITECTURE.md's description of `from_response` is corrected.

## Task Commits

1. **Task 1 RED:** `45766c7` test(52-19): add failing e2e tests for an explicit gate abort
2. **Task 1 GREEN:** `4e3f2af` feat(52-19)!: an explicit abort field and the devflow gate abort verb
3. **Task 2 RED:** `dfe82ef` test(52-19): pin the gate answer mapping and the approve+abort refusal
4. **Task 2 GREEN:** `c9f3b5b` feat(52-19)!: refuse an answer that both approves and aborts
5. **Task 3 RED:** `c60dc0d` test(52-19): gate abort must report itself as an abort
6. **Task 3 GREEN:** `3d78552` feat(52-19): gate abort reports the abort; docs name the three answers

**Plan metadata:** the docs commit that adds this file.

## Places that relied on an "abort" note to end a run (the plan's sweep)

Each now sets the field or uses the verb:

- `gate_wedge_e2e.rs`: `reject()` became `abort_code_gate()` (`gate abort … --stage code`), and `reject_plan()` became `abort_plan_gate()` (`gate abort … --stage plan`). The two self-resolving arms and the two wedge arms use them.
- `phase7_cli.rs::cleanup_worktree_cli_flow`: the teardown is `gate abort 8 --stage validate --note "test teardown"`, and the comment that cited the substring rule is rewritten (review concern C4).
- JSON fixtures `{"approved":false,"note":"abort: …"}` gained `"abort":true`: 16 in `pipeline_outcomes.rs`, 3 in `pipeline_launch.rs`, 2 in `preflight.rs` and 1 in `pipeline_gate.rs`.
- Struct literals with an abort note now set `abort: true`. They are in `cli/test_support.rs`, `pipeline_gate.rs`, `pipeline_outcomes.rs` (two, one of them the `ABORT_NOTE` fixture), `pipeline_launch.rs` (two), `commands.rs` (one), `start_lock_e2e.rs` (two), `core/recover.rs` (one) and `gates.rs` (the round-trip test).
- In `pipeline_launch.rs`, `gate_response(false, Some("abort…"))` calls became `abort_response(note)`, and `preanswer_gate_response` now takes a `&GateResponse`.
- The `answered_gate` and `answer_with` closures in `commands.rs` tests take an explicit `abort` argument.
- Every other `GateResponse` literal (about 50) gained `abort: false`.

## Decisions Made

See `key-decisions` in the frontmatter. The main one is the `GateAnswerKind` enum. The plan said to dispatch `gate_respond` with `abort = true, approved = false`. The enum carries the same information and cannot express approve+abort at the CLI.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Correctness] Split the e2e answer-file check into its own test**
- **Found during:** Task 1 RED
- **Issue:** The plan's `gate_abort_ends_the_run` was to check both `workflow_aborted` and that the answer file holds `"abort": true`. A live advance consumes the answer and its abort removes the file, so the file cannot be read reliably in the same run.
- **Fix:** Added `gate_wedge_e2e::gate_abort_writes_an_answer_with_the_abort_field_set`. It opens a gate and registers a waiter in the test process, runs `gate abort`, and reads the file. `gate_abort_ends_the_run` keeps the live-run claim. Separate tests also follow the operator's atomic-test rule.
- **Commit:** `45766c7`

**2. [Rule 3 - Blocking] Files outside `files_modified`: `preflight.rs` and `ARCHITECTURE.md`**
- `preflight.rs` held two abort-note JSON fixtures. Without the field they would have looped back and broken their tests.
- `ARCHITECTURE.md` stated the substring rule as current behaviour.
- **Commits:** `4e3f2af` and `3d78552`

**3. [Rule 1 - Bug] User-facing text that sent the operator to an abort note**
- `gate list`'s answer hint, the rate-limit gate context (`rejecting with "abort" ends the run`), `GateCmd::Reject`'s help and `stop`'s `wrote a rejection` line all described the old rule. They now name `devflow gate abort`, and `stop` says `wrote an abort`.
- **Commit:** `4e3f2af`

**4. [TDD ordering] `reap_answer`'s `abort: true` and the field-based mapping landed in Task 1 GREEN**
- Four of Task 2's six tests were already green when Task 2 RED ran (`unexpected_green`). Each checks behaviour the tracer had to ship: the mapping, the serde default and the reap writer. Without them, the tracer's e2e could not pass, and `gate sweep` and `devflow stop` would have relaunched agents between commits. Their RED evidence is Task 1's e2e RED: the `do not abort` note aborted the run. `a_note_containing_abort_does_not_abort` also carries the old rule as a test-local control, `old_substring_rule`, which returns `Abort` for the same note.
- `gate_action_aborts_when_note_says_abort` was replaced in Task 1 GREEN, not Task 2, because the GREEN change made it false.

**5. [TDD stub] Task 3 RED commit moved the pickup word into `GateAnswerKind::past_tense`**
- The behaviour was unchanged (abort still printed `rejected`), so the RED test could compile. GREEN changed the word.

---

**Total deviations:** 5 (1 test split, 1 out-of-list files, 1 stale user text, 2 TDD ordering notes)
**Impact on plan:** No scope added. Every change follows from making the field the only abort signal.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch nextest config with `--profile red`. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `gate_wedge_e2e::a_note_saying_do_not_abort_keeps_the_run_alive` | `RED_EVIDENCE_OK` (exit 100) | The target ran and failed on the planned assertion: `gate_resolved.action` was `"abort"` where `"loop_back"` was expected. |
| `gate_wedge_e2e::gate_abort_writes_an_answer_with_the_abort_field_set` | `RED_EVIDENCE_OK` (exit 100) | The target failed on its first assertion: `gate abort` exited with `unrecognized subcommand 'abort'`, which is the missing verb. |
| `gate_wedge_e2e::gate_abort_ends_the_run` | `RED_EVIDENCE_OK` (exit 100) | Same failure: the verb did not exist. |
| `gates::tests::an_answer_that_approves_and_aborts_is_refused` | `RED_EVIDENCE_OK` (exit 100) | `Gates::answer` wrote the approve+abort answer and returned its path. |
| `gates::tests::a_hand_written_approve_and_abort_reads_as_abort` | `RED_EVIDENCE_OK` (exit 100) | It read as `Abort`, but the reason was the note (`ship it`) and did not name the invalid answer. |
| `gates::tests::{a_note_containing_abort_does_not_abort, an_explicit_abort_field_aborts, a_record_without_the_abort_field_is_not_an_abort, reap_answer_writes_an_abort_for_the_observed_gate_only}` | `INVALID_RED` / `unexpected_green` | Already true after Task 1 GREEN; see deviation 4. |
| `commands::tests::gate_respond_reports_an_abort` | `RED_EVIDENCE_OK` (exit 100) | The event had no `abort` key (`Null`), and the word was `rejected`. |

The e2e assertion that `gate abort` stdout starts with `aborted` was also run red (exit 100, stdout `rejected gate for phase 4823 code …`).

Gate sequence: `test` 45766c7 → `feat` 4e3f2af; `test` dfe82ef → `feat` c9f3b5b; `test` c60dc0d → `feat` 3d78552. There is no REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode was `end-of-phase`, and Task 1's verify is automated only. Its three-test chain was re-run after GREEN and exited 0, so expansion continued.

## Verification run

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (3 named nextest runs, each `1 passed`) | 0 |
| `gate_wedge_e2e::gate_abort_writes_an_answer_with_the_abort_field_set` (added test) | 0 |
| Task 2 `<verify>` chain (6 named runs) | 0 |
| `commands::tests::gate_respond_reports_an_abort` and `doc_check::doc_referenced_identifiers_exist_in_source` | 0 |
| `cargo nextest run -p devflow-core --lib` (1017 tests, after Task 1) | 0 |
| `cargo nextest run -p devflow --bin devflow` (602 tests, after Task 1) | 0 |
| gate_wedge, gate_sweep, stop, start_lock, recover_clean, phase7_cli and failover e2e suites (123 tests, after Task 1) | 0 |
| `scripts/check.sh all` (fmt, clippy `-D warnings`, `cargo test --workspace`), on the final tree | 0 |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks reports one new break, `constructible_struct_adds_field` for `GateResponse.abort`. The plan expects it, and 4.0.0 covers it. The other failures come from 52-01..52-18. Adding a variant to the exhaustive `GateError` is also a break, so `c9f3b5b` is marked `!`. The version was not bumped.

Known flake 999.221 did not occur.

**What these checks do not establish:**
- The "do not abort" e2e checks the gate's resolved action. It does not follow the relaunched Code stage. The stub agent exits at once, and the test ends the advance after the loop-back.
- No live DevFlow run (installed binary) has been through `gate abort`. Coverage is the hermetic e2e only.
- Answers written by a 3.x binary that relied on an `abort:` note are now rejections, not aborts. There is no migration (D-28), so close-out has to check the operator's machine for pending 3.x answers.

## Issues Encountered

- In the first RED run of `gate_abort_ends_the_run`, the test panicked before it reaped its advance child. pid 749880 was left running for its 15 s gate timeout and was killed by that recorded pid. Both live-advance tests now hold the child in `ReapOnDrop`, and the later RED runs left nothing behind.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-20.

## Self-Check: PASSED

- Commits `45766c7`, `4e3f2af`, `dfe82ef`, `c9f3b5b`, `c60dc0d` and `3d78552` are ancestors of HEAD, and `git rev-list --count 958d1ae..HEAD` is 6.
- `from_response` contains no `contains("abort")`. Control: the same range does contain `response.abort`.
- `gate_action_aborts_when_note_says_abort` no longer exists in `crates/`.
- `phase7_cli::cleanup_worktree_cli_flow` calls `gate abort`.
- `scripts/check.sh all` exited 0.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

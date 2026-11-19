---
phase: 53-the-question-channel
plan: 04
subsystem: cli
tags: [gates, question, cli, doctor, status, sweep]

requires:
  - phase: 53-the-question-channel
    plan: 01
    provides: GateKind::Question, Gates::open_question_registered / read_question / answer_question / read_question_choice, `gate answer --option`
provides:
  - "`devflow gate answer <phase> --gate-id <id> (--option <id> | --text <answer>)`, exactly one required (clap ArgGroup)"
  - "approve/reject refusal on a question gate naming the exact `gate answer` line for that phase and gate id"
  - "`gate show` question block: question, context, options in payload order, `(recommended)` marker, descriptions"
  - "`gate list` footer naming `gate answer`"
  - "doctor orphan-gate exemption for question gates (PhaseFacts::question_gate_stages)"
  - "`status` line `gate: question <id>` plus `answer: devflow gate answer ...` while gate_pending is false"
  - "`gate sweep` kind guard: a question gate is LeftAlone at any age"
affects: [53-05, 53-07, 53-09]

actuals:
  tokens: 12472
  tasks: 3
  commits: 6
plan_head_before: 910fea6a4218fbf6cdf291e762ddc23e47adb00f
plan_head_after: bd7c05700ac071204f9722459ddf4e34155944d5

tech-stack:
  added: []
  patterns:
    - "A question gate's refusal and status line share one renderer, question_answer_line(phase, gate_id)"
    - "Agent-authored question strings reach the terminal only through render_gate_context (T-53-10); status prints only the gate id and phase"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - OPERATIONS.md

key-decisions:
  - "The refusal's exact verb line is rendered in gate_respond_report, not screen_answer: Refusal holds &'static str and screen_answer has no phase or gate id; the static Question refusal text now names `--option <id> | --text <answer>`"
  - "gate show replaces a question gate's raw context with the structured block; the raw context is derived from the same spec, so nothing is lost"
  - "gate sweep treats a gate whose question key cannot be read as LeftAlone (fail-safe: never abort a run on an unreadable question)"
  - "gate_response_written carries `text` for a text answer and `option_id` for an option, matching the keys history.rs reads"

patterns-established:
  - "Tests that backdate a gate rewrite it through serde_json::Value so keys GateFile does not name survive"

requirements-completed: [ASK-02]

coverage:
  - id: D1
    description: "gate answer takes exactly one of --option and --text"
    requirement: ASK-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::gate_answer_requires_exactly_one_of_option_and_text"
        status: pass
    human_judgment: false
  - id: D2
    description: "A wrong gate id is refused and the question stays open; an option id matches exactly; a non-question gate names approve/reject; an empty text is an answer"
    requirement: ASK-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_answer_with_another_gate_id_leaves_the_question_open"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_answer_refuses_an_option_not_offered"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_answer_on_a_non_question_gate_is_refused_naming_approve"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_answer_records_an_empty_text_answer"
        status: pass
    human_judgment: false
  - id: D3
    description: "Verb separation, question rendering in show/list, doctor exemption"
    requirement: ASK-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_approve_on_a_question_gate_is_refused_naming_gate_answer"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_abort_on_a_question_gate_is_accepted"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_show_prints_a_questions_options_and_recommended_marker"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_list_names_gate_answer_for_a_question_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::doctor_does_not_report_a_live_question_gate_as_orphaned"
        status: pass
    human_judgment: false
  - id: D4
    description: "status names an open question gate; gate sweep leaves it alone"
    requirement: ASK-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::status_reports_an_open_question_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_sweep_leaves_a_question_gate_alone"
        status: pass
    human_judgment: false

duration: 19min
completed: 2026-11-19
status: complete
---

# Phase 53 Plan 04: The operator's side of the question gate Summary

**`devflow gate answer` takes `--option <id>` or `--text <answer>` (exactly one), every wrong verb or wrong gate id is refused naming the line that does answer, `gate show`/`gate list`/`status` show an open question, and neither `doctor` nor `gate sweep` treats a question gate as abandoned.**

## Performance

- **Duration:** 19 min
- **Started:** 2026-11-19T14:17:35Z
- **Completed:** 2026-11-19T14:36:34Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments
- `--text` joins `--option` in a required, single-choice clap group; a text answer (empty included) skips the option check; option ids still match by exact `==` and the refusal lists the offered ids in payload order.
- `gate approve` / `gate reject` on a question gate now end with `` `devflow gate answer <phase> --gate-id <id> (--option <id> | --text <answer>)` or `devflow gate abort <phase> --gate-id <id>` `` for the gate actually open; abort is accepted.
- `gate show` on a question gate prints `question:`, `context:`, `options:` and `  <id>: <label>[ (recommended)]` lines with descriptions indented, each through `render_gate_context`; a non-question gate's output is unchanged. `gate list`'s footer names `gate answer`.
- `doctor`: `PhaseFacts::question_gate_stages` lets `check_orphan_gate` skip question gates; a stage-failure gate in the same shape is still a Problem with repair `devflow gate approve`.
- `status`: with `gate_pending` false and a question gate open at the run's stage, the stage line reads `gate: question <id>` and an `answer:` line follows; no agent text is printed. A read error logs `warn!` and keeps `gate: none`.
- `gate sweep`: `sweep_one_gate` returns `LeftAlone` for a question gate before the waiter check (dry run included), printing why; `reap_answer` is unchanged.

## Task Commits

1. **Task 1: `gate answer --text`, exactly-one-of, binding refusals** - `873c4c8` (test, RED), `720e424` (feat, GREEN)
2. **Task 2: Refusal texts, question rendering, doctor exemption** - `221009f` (test, RED), `ab6dcb7` (feat, GREEN)
3. **Task 3: `status` question line, sweep kind guard** - `fbd01f2` (test, RED), `bd7c057` (feat, GREEN)

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - `--text`, the `choice` ArgGroup, `answer_choice`, the parse test
- `crates/devflow-cli/src/commands.rs` - `gate_answer(_report)` over `QuestionChoice`, `question_answer_line`, refusal rendering, `render_gate_show(gate, question)` + `render_question_block`, `GATE_LIST_ANSWERS`, `question_gate_stages`, `open_question_gate_id`, the sweep guard, 11 tests and the `open_budget_question` fixture
- `crates/devflow-cli/src/pipeline_outcomes.rs` - the Question refusal's `why`/`applies` name `--text`
- `OPERATIONS.md` - `gate answer` row with `--text` and exact matching; the sweep row's question-gate exemption

## Verification

- Task 1 gate: 5/5 named tests pass (`tests::` is main.rs's real module path). Task 2 gate: 5/5 pass. Task 3 gate: 3/3 pass, including the existing `gate_sweep_reaps_only_a_gate_with_a_registered_waiter`.
- `cargo nextest run -p devflow --bin devflow` after Task 3: 509 run, 509 passed, 3 skipped.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `scripts/check.sh all` on `bd7c057`: `==> check.sh: all OK`, exit 0.
- No `devflow-core` pub item changed, so no semver-checks run was needed.

## TDD Gate Compliance

RED -> GREEN: `873c4c8` -> `720e424`, `221009f` -> `ab6dcb7`, `fbd01f2` -> `bd7c057`. No REFACTOR commit. RED evidence came from nextest JUnit reports (scratch `red` profile via `--config-file`) checked with `gsd-tools check tdd-red-evidence`; records are in `target/red-evidence/53-04-*.json` (gitignored). Task 1's RED commit carried a compiling stub (`gate_answer_report` took a `QuestionChoice` and refused text); Task 2's carried `render_gate_show`'s new parameter unused and the footer as a const.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `tests::gate_answer_requires_exactly_one_of_option_and_text` | RED_EVIDENCE_OK | kind UnknownArgument vs ArgumentConflict | Intended: no `--text` yet |
| `commands::tests::gate_answer_records_an_empty_text_answer` | RED_EVIDENCE_OK | expect on "an empty text is an answer" | Intended: stub refused text |
| `commands::tests::gate_answer_on_a_non_question_gate_is_refused_naming_approve` | RED_EVIDENCE_OK | message had `devflow gate              reject` | Intended: 53-01's lost line continuations broke the verb name (fixed in GREEN) |
| `commands::tests::gate_answer_with_another_gate_id_leaves_the_question_open` | INVALID_RED (unexpected_green) | n/a | Expected: 53-01 built the binding; mutation below fails it |
| `commands::tests::gate_answer_refuses_an_option_not_offered` | INVALID_RED (unexpected_green) | n/a | Expected: 53-01 built the exact match; mutation below fails it |
| `commands::tests::gate_approve_on_a_question_gate_is_refused_naming_gate_answer` | RED_EVIDENCE_OK | refusal named the `<phase>`/`<id>` placeholder line | Intended |
| `commands::tests::gate_abort_on_a_question_gate_is_accepted` | INVALID_RED (unexpected_green) | n/a | Expected: a control; 53-01's screen test covers abort acceptance |
| `commands::tests::gate_show_prints_a_questions_options_and_recommended_marker` | RED_EVIDENCE_OK | `question: Pick a budget` missing | Intended: stub ignored the spec |
| `commands::tests::gate_list_names_gate_answer_for_a_question_gate` | RED_EVIDENCE_OK | footer without `gate answer` | Intended |
| `commands::tests::doctor_does_not_report_a_live_question_gate_as_orphaned` | RED_EVIDENCE_OK | the orphan Problem for phase 5349 | Intended |
| `commands::tests::status_reports_an_open_question_gate` | RED_EVIDENCE_OK | stage line read `gate: none` | Intended |
| `commands::tests::gate_sweep_leaves_a_question_gate_alone` | RED_EVIDENCE_OK | left Reaped, right LeftAlone | First RED also had a fixture defect (see Deviations); re-verified by the mutation below after the fixture fix |

**Mutation runs** (each restored from a saved copy and confirmed with `cmp`; each record RED_EVIDENCE_OK):
- Option match by `eq_ignore_ascii_case`: the option test failed on "a case-folded option id must be refused" (it answered `Budget-120`).
- `gate_answer_report` answering the open gate's id whatever id was given: the wrong-id test failed on "another gate's id must be refused".
- Sweep guard removed, fixture fixed: the sweep test failed with left Reaped, right LeftAlone.

## Decisions Made
- The exact verb line is built in `gate_respond_report` from the gate kind on disk; the runner-side refusal (pipeline_gate) keeps the static text, which now names `--text` too.
- A question gate's `gate show` body replaces the raw context with the structured block.
- An unreadable question key leaves the gate alone in the sweep, and reads as `gate: none` in status with a warning.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] 53-01's `gate answer` refusals had runs of spaces mid-sentence**
- **Found during:** Task 1 (RED of the non-question refusal test)
- **Issue:** three string literals lost their `\` line continuations, so messages read `devflow gate              reject`.
- **Fix:** restored the continuations.
- **Files modified:** crates/devflow-cli/src/commands.rs
- **Commit:** 720e424

**2. [Rule 1 - Bug] The `backdate_gate` test helper dropped a question gate's `question` key**
- **Found during:** Task 3 GREEN
- **Issue:** it rewrote the gate file through `GateFile`, which does not name `question`, so a backdated question gate read as a plain gate.
- **Fix:** rewrite through `serde_json::Value`; the RED was then re-verified by mutation.
- **Files modified:** crates/devflow-cli/src/commands.rs
- **Commit:** bd7c057

**3. [Rule 2 - Docs] OPERATIONS.md updated for `--text` and the sweep exemption** (operator docs must match the CLI; file not in the plan's list). Commits 720e424, bd7c057.

## Issues Encountered
None beyond the deviations.

## Next Phase Readiness
53-05/53-07/53-09 can rely on `gate answer --text`, `question_answer_line`, and a sweep that never aborts a pending question.

## Self-Check: PASSED
- FOUND: crates/devflow-cli/src/main.rs, crates/devflow-cli/src/commands.rs, crates/devflow-cli/src/pipeline_outcomes.rs, OPERATIONS.md
- FOUND commits: 873c4c8, 720e424, 221009f, ab6dcb7, fbd01f2, bd7c057 (all in `git log 910fea6..HEAD`)

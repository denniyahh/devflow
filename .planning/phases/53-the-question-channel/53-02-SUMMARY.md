---
phase: 53-the-question-channel
plan: 02
subsystem: events, history
tags: [events, history, question, audit]

requires: []
provides:
  - events::QUESTION_ASKED / QUESTION_ANSWERED / QUESTION_CLOSED / ANSWERING_POLICY_OPERATOR_VIA_GATE with the field contract in their doc comment (D-15)
  - question rows in history::render_timeline, paired by question_id (D-16, ASK-04)
affects: [53-07]

actuals:
  tokens: 3790
  tasks: 2
  commits: 4
plan_head_before: 10a4545317cfce148fa7ccc0a657d52396b3ef89
plan_head_after: 019cac62c9e7c96a9e467e9373b1dd363a2ef3f8

tech-stack:
  added: []
  patterns:
    - "History sub-lines: a question event's detail prints as indented `  question|options|recommended|answer|closed|ignored` rows under its `[ts] describe` line"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/events.rs
    - crates/devflow-core/src/history.rs

key-decisions:
  - "A second answer or close for one question_id keeps its own `[ts] describe` line plus `  ignored <id>: already answered or closed by an earlier event`; only the first resolution's answer/closed sub-line is shown"
  - "An asked question counts as resolved when any answer or close for its id is in the timeline, not only a later one"
  - "An absent `recommended` key prints `  recommended: <missing field>`; null means no option is recommended; an empty options array prints `(none)`"

patterns-established:
  - "Agent-authored history text goes through gates::render_gate_context(text, usize::MAX): control characters replaced, never truncated"

requirements-completed: [ASK-04]

coverage:
  - id: D1
    description: "The four question event constants carry their wire values and a question_asked event describes by its stage"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/events.rs#events::tests::describe_names_a_question_event_by_its_stage"
        status: pass
    human_judgment: false
  - id: D2
    description: "An open question renders its text, options with (recommended), and not yet answered"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#history::tests::an_open_question_renders_not_yet_answered"
        status: pass
    human_judgment: false
  - id: D3
    description: "An empty text answer renders as text \"\" (empty), never as not yet answered"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#history::tests::an_empty_text_answer_renders_distinct_from_not_yet_answered"
        status: pass
    human_judgment: false
  - id: D4
    description: "A run without question events renders no question rows"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#history::tests::a_run_with_no_questions_has_no_question_rows"
        status: pass
    human_judgment: false
  - id: D5
    description: "Answers pair to questions by question_id, not position"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#history::tests::question_rows_pair_by_question_id_not_position"
        status: pass
    human_judgment: false
  - id: D6
    description: "Question text renders terminal-safe"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#history::tests::question_text_is_rendered_terminal_safe"
        status: pass
    human_judgment: false
  - id: D7
    description: "For one question_id only the first answer or close is applied; a second is shown as ignored"
    requirement: ASK-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/history.rs#history::tests::a_second_resolution_for_one_question_is_shown_but_not_applied"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-11-19
status: complete
---

# Phase 53 Plan 02: Question rows in devflow history Summary

**`devflow history` now shows each question with its options (recommended marked) and, on the event that recorded it, its answer or close reason, paired by `question_id`; an open question reads `not yet answered` and an empty text answer reads `text "" (empty)`.**

## Performance

- **Duration:** about 6 min
- **Completed:** 2026-11-19
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- `events.rs` fixes the event names and fields 53-07's server will emit (D-15), in one doc comment above the constants.
- `history::render_timeline` adds indented question sub-lines from events alone; every agent string is sanitized and untruncated; a missing field prints `<missing field>`.

## Task Commits

1. **Task 1: Question event contract** - `9825ee0` (test, RED), `aa66b8f` (feat, GREEN)
2. **Task 2: Question rows in history** - `9d6ed83` (test, RED), `019cac6` (feat, GREEN)

## Files Created/Modified

- `crates/devflow-core/src/events.rs` - the four constants, their field contract, one test
- `crates/devflow-core/src/history.rs` - question sub-lines in `render_timeline` (helpers `resolved_question_ids`, `question_rows`, `options_line`, `answer_text`, `text_field`), six tests

## Verification

- The plan's named-test chains for Task 1 and Task 2 each ran `1 test run: 1 passed`.
- `cargo nextest run -p devflow-core --lib`: 969 passed. `cargo clippy -p devflow-core --all-targets -- -D warnings`: clean. `scripts/check.sh all` was not run (no env read or `Command` variant added, so doc_check has nothing new).
- Negative control for the no-question test: with the non-question arm of `question_rows` mutated to print `  answer: not yet answered`, the test failed naming `question row "  answer: not yet answered" in attempt history for phase 16`; the file was restored and confirmed with `cmp`.
- `cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor`: `202 checks: 201 pass, 1 fail`; the one failure is `enum_variant_added` on `GateKind::Question` (53-01's planned major change). No item names the four new constants.

## TDD Gate Compliance

RED -> GREEN: `9825ee0` -> `aa66b8f`, then `9d6ed83` -> `019cac6`. No REFACTOR commit. RED evidence came from nextest JUnit reports (scratch `red` profile via `--config-file`) checked with `gsd-tools check tdd-red-evidence`; records are in `target/red-evidence/53-02-*.json` (gitignored). The Task 1 RED commit carried the constants as empty-string stubs so the test compiled and failed on its assertion.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `events::tests::describe_names_a_question_event_by_its_stage` | RED_EVIDENCE_OK | left `" (code)"`, right `"question_asked (code)"` | Intended: stub constant was empty |
| `history::tests::an_open_question_renders_not_yet_answered` | RED_EVIDENCE_OK | the `  question q-1: Keep the cache?` assertion | Intended: no question sub-lines yet |
| `history::tests::an_empty_text_answer_renders_distinct_from_not_yet_answered` | RED_EVIDENCE_OK | the `text "" (empty)` assertion | Intended: no answer sub-line yet |
| `history::tests::question_rows_pair_by_question_id_not_position` | RED_EVIDENCE_OK | the `answer q-2: option drop` assertion | Intended: same |
| `history::tests::question_text_is_rendered_terminal_safe` | RED_EVIDENCE_OK | the sanitized question-text assertion | Intended: question text not rendered yet |
| `history::tests::a_second_resolution_for_one_question_is_shown_but_not_applied` | RED_EVIDENCE_OK | the `answer q-1: option keep` assertion | Intended: same |
| `history::tests::a_run_with_no_questions_has_no_question_rows` | INVALID_RED (unexpected_green) | n/a | Expected: a control that must pass before and after; the mutation run above shows it can fail |

## Decisions Made

- Duplicate resolution for one `question_id` (must_haves adjacency edge): the second event keeps its `[ts] describe` line and gets `  ignored <id>: already answered or closed by an earlier event`, so the view never shows both an answer and a close. Covered by the added test `a_second_resolution_for_one_question_is_shown_but_not_applied`.
- Resolution is computed over the whole timeline rather than only events after the ask; an answer logged before its ask is a defect either way and is still shown.
- `recommended` absent prints `  recommended: <missing field>` (null means none); an empty `options` array prints `(none)`.

## Deviations from Plan

### Auto-added

**1. [Rule 2 - Missing test] A test for the adjacency edge in must_haves**
- **Found during:** Task 2
- **Issue:** must_haves names the "answer and close for one id" edge but the task listed no test for it
- **Fix:** added `history::tests::a_second_resolution_for_one_question_is_shown_but_not_applied` and the `  ignored` row
- **Files modified:** crates/devflow-core/src/history.rs
- **Commit:** 9d6ed83, 019cac6

Otherwise the plan executed as written.

## Issues Encountered

None.

## User Setup Required

None.

## Next Phase Readiness

53-07 can emit `question_asked` / `question_answered` / `question_closed` with the fields in `events.rs`, and `devflow history` will render them.

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/events.rs, crates/devflow-core/src/history.rs
- FOUND commits: 9825ee0, aa66b8f, 9d6ed83, 019cac6 (ancestors of HEAD)

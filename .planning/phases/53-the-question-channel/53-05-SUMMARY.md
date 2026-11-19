---
phase: 53-the-question-channel
plan: 05
subsystem: question, ask-server
tags: [mcp, validation, question, gates]
status: complete

requires:
  - phase: 53-01
    provides: QuestionSpec, the devflow __ask server and its parse_ask_arguments hook, AskFixture + AskClient
provides:
  - QuestionError with one named refusal per malformed field
  - QuestionSpec::from_arguments with character-counted bounds (QUESTION_MAX_CHARS, CONTEXT_MAX_CHARS, OPTION_ID_MAX_CHARS, OPTION_LABEL_MAX_CHARS, OPTION_DESCRIPTION_MAX_CHARS, MAX_OPTIONS)
  - the server's refusal of a malformed devflow_ask call as an isError tool result, before any gate is touched
affects: [53-07, 53-08]

actuals:
  tokens: 5173
  tasks: 2
  commits: 4
plan_head_before: 3652cdf7d69f5a2240763c515e639a035d62f9df
plan_head_after: fe5ebffab9e9b94c7be650b83bcfe0dff79e1cd5

tech-stack:
  added: []
  patterns:
    - "Field-by-field parse of an untrusted serde_json::Value: each field read by name, no defaults for required fields, the error carrying the field path (`options[1].id`)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/question.rs
    - crates/devflow-cli/src/ask_server.rs
    - crates/devflow-cli/tests/ask_e2e.rs

key-decisions:
  - "A JSON null for an optional field (context, recommended, description) reads as absent; a wrong-typed value is refused as WrongType naming the field"
  - "An option id that is only whitespace is refused as Blank, the same predicate as the question; option labels are required strings but may be empty"
  - "`recommended` gets no length bound of its own: it must equal an offered id, which is already bounded"
  - "`kind` is read through QuestionKind's serde wire names, so the accepted strings cannot drift from the enum"

patterns-established:
  - "Refusals of the ask payload are QuestionError values whose Display names the field in backticks"

requirements-completed: [ASK-01]

coverage:
  - id: D1
    description: "Each malformed payload (empty/whitespace question, missing/empty options, duplicate or empty option id, unoffered recommended, missing/unknown kind) is refused with a typed error naming its field"
    requirement: ASK-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/question.rs#question::tests::each_malformed_payload_is_refused_naming_its_field"
        status: pass
    human_judgment: false
  - id: D2
    description: "One option, no context, no recommended, kind discuss, plus an unknown extra key, parses"
    requirement: ASK-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/question.rs#question::tests::a_well_formed_payload_with_one_option_is_accepted"
        status: pass
    human_judgment: false
  - id: D3
    description: "Option ids differing only by case are distinct and accepted"
    requirement: ASK-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/question.rs#question::tests::option_ids_differing_only_by_case_are_distinct"
        status: pass
    human_judgment: false
  - id: D4
    description: "A question of exactly QUESTION_MAX_CHARS multibyte characters is accepted, one more is refused as TooLong"
    requirement: ASK-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/question.rs#question::tests::lengths_are_counted_in_characters"
        status: pass
    human_judgment: false
  - id: D5
    description: "Criterion 1 control: options [] returns isError naming options, no gate file at any poll, ping answered after, one option opens a gate"
    requirement: ASK-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/ask_e2e.rs#a_malformed_call_returns_is_error_and_opens_no_gate"
        status: pass
    human_judgment: false
  - id: D6
    description: "An unoffered recommended id returns isError naming recommended and opens no gate"
    requirement: ASK-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/ask_e2e.rs#a_call_offering_a_recommended_id_not_in_options_is_refused"
        status: pass
    human_judgment: false

duration: 70min
completed: 2026-11-19
---

# Phase 53 Plan 05: devflow_ask payload validation Summary

**`QuestionSpec::from_arguments` refuses every malformed `devflow_ask` field by name with character-counted bounds, and the `__ask` server returns that refusal as an `isError` tool result before any gate opens.**

## Performance

- **Duration:** about 70 min
- **Completed:** 2026-11-19T14:50:04Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- `QuestionError` (11 variants, `thiserror`, each Display naming its field) and six `pub const` bounds counted with `chars().count()`; over-long text is refused, never truncated.
- `QuestionSpec::from_arguments` reads `question`, `options` (each `id`, `label`, `description`), `context`, `kind` and `recommended` by name, refuses control characters in option ids, assigns a fresh `question_id`, ignores unknown keys.
- `parse_ask_arguments` in the server now delegates to it; a refusal reads `devflow_ask refused: <error>` with `isError: true`.

## Task Commits

1. **Task 1: `QuestionSpec::from_arguments` with named refusals and bounds** - `cb9919a` (test, RED), `459a60f` (feat, GREEN)
2. **Task 2: The server refuses a malformed call without opening a gate** - `99a5ae6` (test, RED), `fe5ebff` (feat, GREEN)

## Files Created/Modified
- `crates/devflow-core/src/question.rs` - bounds, `QuestionError`, `from_arguments` and its field helpers, four tests
- `crates/devflow-cli/src/ask_server.rs` - `parse_ask_arguments` delegates to `from_arguments`
- `crates/devflow-cli/tests/ask_e2e.rs` - `refused_call` (polls `.devflow/gates/` for emptiness until the reply), `assert_refused_naming`, two e2e tests

## Verification

- Plan gate, Task 1 (the four chained `cargo nextest ... --no-tests=fail -E 'test(=question::tests::...)'`): each `1 test run: 1 passed`.
- Plan gate, Task 2 (the two chained `cargo nextest -p devflow --test ask_e2e ...`): each `1 test run: 1 passed`; the whole `ask_e2e` suite `4 tests run: 4 passed` (the two 53-01 tests still pass).
- `scripts/check.sh all`: exit 0, `==> check.sh: all OK`, no non-zero `failed` count in its output.
- `cargo clippy -p devflow-core --all-targets -- -D warnings` and `-p devflow`: clean.
- `cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor`: exit 100, `202 checks: 199 pass, 3 fail`. The three major failures are `enum_variant_added` (`GateKind::Question`, 53-01), `function_missing` (`verify::phase_has_blocking_human_checkpoint`, `agent_result::blocking_human_checkpoint_reported`, `agent_result::checkpoint_reported_in_capture`) and `inherent_method_missing` (`ClaudeDriver::exec_resume_command`), all other plans' planned changes. None names `QuestionError`, `QuestionSpec::from_arguments` or the new bounds, so this plan's additions read as additive.

## TDD Gate Compliance

RED -> GREEN: `cb9919a` -> `459a60f`, `99a5ae6` -> `fe5ebff`. No REFACTOR commit. RED evidence came from nextest JUnit reports (scratch `red` profile via `--config-file`) checked with `gsd-tools check tdd-red-evidence`; records are in `target/red-evidence/53-05-*.json` (gitignored). Task 1's RED commit carried `QuestionError`, the bounds and a compiling pass-through `from_arguments` stub (the 53-01 serde deserialization).

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `question::tests::each_malformed_payload_is_refused_naming_its_field` | RED_EVIDENCE_OK | "case `empty question` parsed when it should be refused" | Intended: stub validated nothing |
| `question::tests::lengths_are_counted_in_characters` | RED_EVIDENCE_OK | "one character past the bound is refused, not truncated" (left `Ok(..)`) | Intended: stub had no bound |
| `question::tests::a_well_formed_payload_with_one_option_is_accepted` | INVALID_RED (unexpected_green) | n/a | Expected: a control the pass-through already satisfied; mutation below fails it |
| `question::tests::option_ids_differing_only_by_case_are_distinct` | INVALID_RED (unexpected_green) | n/a | Expected: the stub had no duplicate check at all; mutation below fails it |
| `ask_e2e::a_malformed_call_returns_is_error_and_opens_no_gate` | RED_EVIDENCE_OK | "a gate file appeared for a malformed call" (`.devflow/gates/01-code.json`) | Intended: the pass-through opened a gate for `options: []` |
| `ask_e2e::a_call_offering_a_recommended_id_not_in_options_is_refused` | RED_EVIDENCE_OK | "a gate file appeared for a malformed call" | Intended |

**Mutation runs** (each restored from a saved copy and confirmed with `cmp`; each record RED_EVIDENCE_OK):
- Duplicate check by `eq_ignore_ascii_case`: the case test failed on its `expect("`a` and `A` are distinct ids")`.
- `recommended` made required: the one-option test failed on its `expect("a well-formed payload parses")`.

## Decisions Made
- Null optional fields read as absent; wrong-typed fields are refused as `WrongType` naming the field.
- Whitespace-only option ids are refused (`Blank`), matching the question's predicate; labels are required strings, emptiness not checked.
- `recommended` has no own length bound, since it must equal an offered (bounded) id.
- `kind` parses through `QuestionKind`'s serde names, so the accepted strings track the enum.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- `.planning/user/errors/state-stale.json` was already modified in the working tree when this plan started and is not this plan's; it was left unstaged.

## Next Phase Readiness
- 53-07/53-08 can rely on every gate the server opens carrying a validated spec: at least one option, unique non-blank ids an operator can type, and a `recommended` that is offered.

## Self-Check: PASSED
- FOUND: crates/devflow-core/src/question.rs, crates/devflow-cli/src/ask_server.rs, crates/devflow-cli/tests/ask_e2e.rs
- FOUND (ancestors of HEAD): cb9919a, 459a60f, 99a5ae6, fe5ebff

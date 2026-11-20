---
phase: 53-the-question-channel
plan: 01
subsystem: gates, cli
tags: [mcp, json-rpc, stdio, gates, question]

requires:
  - phase: 52.2-foreground-runner
    provides: the waiter-first gate open, the runner lock and the gate verbs this plan extends
provides:
  - GateKind::Question with a question spec key in the gate file and a choice key in the answer body
  - Gates::open_question_registered / read_question / answer_question / read_question_choice
  - the hidden `devflow __ask` stdio MCP server with one tool, devflow_ask
  - `devflow gate answer <phase> --gate-id <id> --option <id>`
  - AskFixture + AskClient, the shared e2e client every later ask suite builds on
affects: [53-04, 53-05, 53-07, 53-08]

actuals:
  tokens: 20392
  tasks: 3
  commits: 5
plan_head_before: 65a7675dbe1fa69c45cc5340f6a42e4bc51d8e97
plan_head_after: 1145967a86bf6833e356aabc2d7adf213a1481e0

tech-stack:
  added: []
  patterns:
    - "Additive on-disk keys: the gate file and the answer body carry extra top-level keys through #[serde(flatten)] wrappers, so GateFile and GateAnswer literals stay untouched and old readers ignore the keys"
    - "Two-thread stdio server: the reader answers inline methods, one worker takes tools/call in arrival order, one Mutex writer emits one JSON line per message"

key-files:
  created:
    - crates/devflow-core/src/question.rs
    - crates/devflow-cli/src/ask_server.rs
    - crates/devflow-cli/tests/ask_e2e.rs
    - crates/devflow-cli/tests/common/ask_client.rs
  modified:
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/common/mod.rs
    - OPERATIONS.md
    - ARCHITECTURE.md

key-decisions:
  - "An abort answered on a question gate is claimed and closed by the server, which returns a tool error telling the agent the run was aborted; ending the run itself stays with the runner's stop path"
  - "On stdin EOF the server returns without joining a held call; cleanup of what an ended client leaves is 53-07's"
  - "parse_ask_arguments deserializes the arguments straight into QuestionSpec with a fresh question_id inserted (no serde dependency added to the CLI crate)"

patterns-established:
  - "Question gate wire format: gate file key `question` (QuestionSpec), answer body key `choice` ({option_id} or {text})"

requirements-completed: [ASK-01, ASK-02]

coverage:
  - id: D1
    description: "A question gate keeps its spec through answer and claim, and its chosen option is read back"
    requirement: ASK-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_question_gate_keeps_its_spec_through_answer_and_claim"
        status: pass
    human_judgment: false
  - id: D2
    description: "gate approve and gate reject on a question gate are refused naming gate answer; abort is accepted"
    requirement: ASK-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::approve_and_reject_on_a_question_gate_are_refused_naming_gate_answer"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/ask_e2e.rs#gate_approve_on_a_question_gate_is_refused_and_writes_no_answer"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow __ask speaks MCP JSON-RPC on stdio: version negotiation, -32601, ping during a held call, -32602 for another tool"
    requirement: ASK-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/ask_server.rs#ask_server::tests (4 named tests)"
        status: pass
    human_judgment: false
  - id: D4
    description: "A devflow_ask call blocks until `gate answer --option` and returns the option to the same client"
    requirement: ASK-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/ask_e2e.rs#a_call_blocks_until_answered_and_returns_the_option"
        status: pass
    human_judgment: false

duration: 28min
completed: 2026-11-19
status: complete
---

# Phase 53 Plan 01: The question channel tracer slice Summary

**A client process calls `devflow_ask` on the hidden `devflow __ask` stdio MCP server, the call becomes a `question` gate, the operator answers it with `devflow gate answer --gate-id <id> --option <id>`, and the same client gets `{question_id, answer: {option_id}, answered_by, answered_at}` back.**

## Performance

- **Duration:** 28 min
- **Started:** 2026-11-19T12:57:47Z
- **Completed:** 2026-11-19T13:25:24Z
- **Tasks:** 3
- **Files modified:** 13

## Accomplishments
- `GateKind::Question`: the gate file carries the question's spec under a `question` key in its one atomic write, the answer body carries the chosen option under a `choice` key; both survive the answer and the claim. Approve and reject are refused on it by the same screen the runner uses, naming `devflow gate answer`; abort is accepted.
- `devflow __ask`: initialize (echoes 2025-11-25, 2025-06-18 or 2025-03-26, else 2025-11-25), ping, tools/list (one tool), tools/call; -32601 for any other request, -32602 for another tool, -32700 for a non-JSON line; stdout carries JSON-RPC lines only.
- `devflow gate answer`: requires `--gate-id`, refuses a non-question gate and an option the question did not offer before writing, emits `gate_response_written` with the option id.
- `AskFixture` / `AskClient` pass the fixture's saved `started_at` as `--run-started-at`; `spawn_with` overrides it (review concern 14).

## Task Commits

1. **Task 1: The question gate** - `5241f8a` (test, RED), `54e5bdd` (feat, GREEN)
2. **Task 2: The stdio server, `__ask` and `gate answer`, docs** - `fab3206` (test, RED), `8f9333b` (feat, GREEN)
3. **Task 3: End-to-end tracer** - `1145967` (test)

## Files Created/Modified
- `crates/devflow-core/src/question.rs` - QuestionSpec, QuestionOption, QuestionKind (kebab-case), QuestionChoice, AskAnswer, new_question_id, render_question_context
- `crates/devflow-core/src/gates.rs` - the Question variant and the four question gate functions; open_gate_as/write_gate_file take an optional spec; answer_with_hook delegates to publish_answer_with_hook with an optional choice
- `crates/devflow-cli/src/ask_server.rs` - the server (serve, run_server, handle_line, the worker's gate lifecycle) and its protocol tests
- `crates/devflow-cli/src/main.rs` - `Command::Ask` (`__ask`, hidden) and `GateCmd::Answer`
- `crates/devflow-cli/src/commands.rs` - gate_answer / gate_answer_report; Question classified in two every-kind tests
- `crates/devflow-cli/src/pipeline_outcomes.rs` - AnswerFamily::Question, question_refusal, every match site; parity tests gained a Question case
- `crates/devflow-cli/src/test_support.rs` - Question in every_gate_kind
- `crates/devflow-cli/tests/common/ask_client.rs`, `tests/common/mod.rs`, `tests/ask_e2e.rs` - the shared client and the e2e suite
- `OPERATIONS.md`, `ARCHITECTURE.md` - `devflow ask` / `__ask`, `gate answer`, the `question` gate-kind row, the server as a gate waiter

## Verification

- Task 1 gate: 3/3 named tests pass. Task 2 gate: 7/7 named tests pass (4 protocol, 2 doc_check, help snapshot). Task 3 gate: 2/2 e2e pass, re-run after the mutation restores (tracer feedback gate, interactive end-of-phase mode with automated-only verify: passed, no checkpoint).
- `scripts/check.sh all` on `1145967`: `==> check.sh: all OK`, exit 0; its log shows the four ask_server tests and ask_e2e ran.
- Full unit suites after Task 1: devflow-core lib 962/962, devflow bin 517 run / 3 skipped, all passed, including the every-kind parity tests now carrying a Question row.
- doc_check negative control: with OPERATIONS.md and ARCHITECTURE.md reverted to HEAD, `source_devflow_env_vars_and_subcommands_are_documented` failed naming "CLI subcommand `devflow ask` is missing from scoped operator docs"; restored and confirmed with `cmp`.
- `cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor` (exit 100):
  ```
  Checked [   0.118s] 202 checks: 201 pass, 1 fail, 0 warn, 58 skip
  --- failure enum_variant_added: enum variant added on exhaustive enum ---
  Failed in:
    variant GateKind:Question in crates/devflow-core/src/gates.rs:165
  Summary semver requires new major version: 1 major and 0 minor checks failed
  ```
  The one major item is the expected `GateKind` variant (accepted inside unpublished 4.0.0); no other major item.

## TDD Gate Compliance

RED -> GREEN: `5241f8a` -> `54e5bdd`, then `fab3206` -> `8f9333b`; each GREEN descends from its RED. No REFACTOR commit. RED evidence came from nextest JUnit reports written with a scratch `red` profile (`--config-file` in the session scratch dir) and checked with `gsd-tools check tdd-red-evidence`; records are in `target/red-evidence/53-01-*.json` (gitignored). Each RED commit carried compiling stubs, so every test failed on its assertion and not on a build error.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `gates::tests::a_question_gate_keeps_its_spec_through_answer_and_claim` | RED_EVIDENCE_OK | "the gate file publishes the spec under its `question` key": left None | Intended: stub read_question returned None |
| `question::tests::the_blocking_human_kind_serializes_as_kebab_case` | RED_EVIDENCE_OK | wire value "BlockingHuman" vs "blocking-human" | Intended: no rename attribute yet |
| `pipeline_outcomes::tests::approve_and_reject_on_a_question_gate_are_refused_naming_gate_answer` | RED_EVIDENCE_OK | expect_err on the approve | Intended: Question was classified with the checkpoint family, which accepts an approve |
| `ask_server::tests::initialize_negotiates_a_supported_version` | RED_EVIDENCE_OK | "expected a reply now, got Nothing" | Intended: stub routed only tools/call |
| `ask_server::tests::an_unknown_request_gets_method_not_found` | RED_EVIDENCE_OK | "expected a reply now, got Nothing" | Intended: same stub |
| `ask_server::tests::ping_is_answered_while_a_call_is_held` | RED_EVIDENCE_OK | "ping not answered while a call was held" | Intended: stub ran calls inline on the reader |
| `ask_server::tests::a_call_for_another_tool_is_a_protocol_error` | RED_EVIDENCE_OK | error code left Null (the worker replied) | Intended: stub sent every tools/call to the worker |
| `ask_e2e::a_call_blocks_until_answered_and_returns_the_option` | INVALID_RED (unexpected_green) | n/a | Expected: Tasks 1-2 built the path this tracer proves; mutations below show it can fail |
| `ask_e2e::gate_approve_on_a_question_gate_is_refused_and_writes_no_answer` | INVALID_RED (unexpected_green) | n/a | Expected, as above |

**Mutation runs** (each restored from a saved copy and confirmed with `cmp` and `git diff --quiet`; each mutation record RED_EVIDENCE_OK):
- No wait and no close in the server: test 1 failed on "reply arrived before the gate was answered". (No wait alone failed earlier on "no gate file appeared": the gate opened and closed before the test saw it.)
- `gate answer` writing `budget-30` whatever the flag: test 1 failed on `option_id`, left "budget-30".
- The screen accepting an approve on a question gate: test 2 failed on "approve wrote an answer slot".

## Decisions Made
- Abort on a question gate: the server claims it, finishes its registration, closes the gate and returns a tool error saying the operator aborted the run. Ending the run itself is not wired here (the runner's stop path, later plans).
- Stdin EOF returns without joining a held call; a gate left by an ended client is 53-07's cleanup.
- `handle_line` takes only the line: no `ServerState` yet, since nothing in this plan needs reader state; 53-07's cancel flag adds what it needs.
- `__ask --project` is used as given, not walked up through `project_root()`: the launch registration passes the run's root.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Question classified in two commands.rs tests during Task 1**
- **Found during:** Task 1 (GREEN)
- **Issue:** adding `AnswerFamily::Question` broke the exhaustive matches in `gate_reject_prints_the_route_it_will_run` and the approve print test in commands.rs, a Task 2 file.
- **Fix:** Question joins the refused arm in both (reject refused naming `devflow gate abort`, approve refused).
- **Files modified:** crates/devflow-cli/src/commands.rs
- **Commit:** 54e5bdd

**2. [Rule 3 - Blocking] No serde dependency in the CLI crate**
- **Found during:** Task 2 (RED)
- **Issue:** a private `#[derive(Deserialize)]` struct in ask_server.rs needs the `serde` crate, which devflow-cli does not depend on.
- **Fix:** `parse_ask_arguments` inserts a fresh `question_id` into the arguments object and deserializes it as core's `QuestionSpec`; no dependency added.
- **Files modified:** crates/devflow-cli/src/ask_server.rs
- **Commit:** fab3206

---

**Total deviations:** 2 auto-fixed (both blocking). **Impact:** none on scope.

## Issues Encountered
None.

## User Setup Required
None.

## Next Phase Readiness
- 53-05 fills `parse_ask_arguments`; 53-07 adds the cancel closure (currently `|| false`), EOF/stop cleanup and the `--run-started-at` check; 53-08 passes the five `__ask` flags in the launch registration; 53-04 adds `--text`.

## Self-Check: PASSED

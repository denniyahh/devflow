---
phase: "53"
slug: "the-question-channel"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-18"
---

# Phase 53 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from 53-RESEARCH.md § Validation Architecture; test names there are proposals the planner may rename.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` via cargo-nextest 0.9.152; e2e in `crates/devflow-cli/tests/*.rs` with `tests/common` |
| **Config file** | workspace `Cargo.toml`; `scripts/check.sh all` |
| **Quick run command** | `cargo nextest run -p devflow-core --lib` / `cargo nextest run -p devflow --bin devflow` |
| **Full suite command** | `scripts/check.sh all` (then `scripts/check-in-container.sh`) |
| **Estimated runtime** | ~300 seconds (full suite, warm) |

Gate each requirement on one named test: `cargo nextest run -p <pkg> <--lib|--bin devflow|--test ask_e2e> --no-tests=fail -E 'test(=<module::path::name>)'`.

---

## Sampling Rate

- **After every task commit:** the task's named test(s), plus the quick run command for the touched package
- **After every plan wave:** `scripts/check.sh all`
- **Before DevFlow's Validate stage (`/gsd-validate-phase 53`):** Full suite must be green, container parity
- **After Validate passes, at close-out (agent session, before `scripts/cut-pr-branch.sh`):** the Manual-Only live proofs (claude + codex) and the D-17 closing run below
- **Max feedback latency:** 300 seconds

---

## Per-Task Verification Map

Filled by the planner per task. Requirement → test map from research:

| Requirement | Behavior | Test Type | Proposed test | Negative control | File Exists | Status |
|-------------|----------|-----------|---------------|------------------|-------------|--------|
| ASK-01 | initialize negotiates version; unknown request → -32601 | unit (cli) | `ask_server::tests::initialize_negotiates_a_supported_version`, `ask_server::tests::an_unknown_request_gets_method_not_found` | unsupported version gets `2025-11-25` | ❌ W0 | ⬜ pending |
| ASK-01 | malformed payloads refused before any gate opens | unit (core) | `question::tests::each_malformed_payload_is_refused_naming_its_field` | well-formed payload passes | ❌ W0 | ⬜ pending |
| ASK-01 | call blocks until answered, returns option in same process | e2e | `ask_e2e::a_call_blocks_until_answered_and_returns_the_option` | `ask_e2e::a_malformed_call_returns_is_error_and_opens_no_gate` | ❌ W0 | ⬜ pending |
| ASK-01 | driver argv carries the registration | unit (core) | `agents::claude::tests::launch_args_name_the_ask_registration`, `agents::codex::tests::launch_args_set_tool_timeout_and_approval_before_the_prompt` | canary/preflight argv has none | ❌ W0 | ⬜ pending |
| ASK-01 | cancel/EOF close the gate | unit/e2e | `ask_server::tests::a_cancelled_call_closes_its_gate_and_records_question_closed` | answered call records no `question_closed` | ❌ W0 | ⬜ pending |
| ASK-01 | OD-1 (b): a capped call returns `pending`; a re-call by `question_id` gets the answer with no new question (53-13) | e2e / unit (core) | `ask_e2e::a_capped_call_returns_pending_and_a_recall_gets_the_answer`, `agents::codex::tests::codex_holds_up_to_the_cap_and_its_client_waits_longer` | `ask_e2e::an_uncapped_call_holds_past_a_cap_value` | ❌ W0 | ⬜ pending |
| ASK-01 | OD-2: pi registered on 53-14's BUILD verdict, still refused by name on REFUSE | unit (core) / e2e | BUILD: `agents::pi::tests::launch_args_carry_the_pi_registration`, `ask_launch_e2e::a_pi_stage_agent_asks_through_its_registration_and_finishes`; REFUSE: `agents::tests::every_driver_declares_its_ask_registration` with pi refused | `pipeline_launch::tests::a_project_owned_registration_file_refuses_the_launch` | ❌ W0 | ⬜ pending |
| ASK-02 | list/show print question and options | unit (cli) | `commands::tests::gate_show_prints_a_questions_options_and_recommended_marker` | non-question gate shows no options | ❌ W0 | ⬜ pending |
| ASK-02 | answer binds to named gate only; verb separation | unit (cli) | `commands::tests::gate_answer_with_another_gate_id_leaves_the_question_open`, `…gate_approve_on_a_question_gate_is_refused_naming_gate_answer` | right id resolves it | ❌ W0 | ⬜ pending |
| ASK-02 | notify fires, `notify_fired` recorded | e2e | `ask_e2e::a_question_fires_the_notify_hook_and_records_notify_fired` | notify disabled → no marker | ❌ W0 | ⬜ pending |
| ASK-03 | positional self-resolution gone; prompts name `devflow_ask` | unit (core) | `prompt::tests::no_prompt_template_carries_positional_self_resolution`, `prompt::tests::every_stage_prompt_routes_questions_through_devflow_ask` | planted copy must trip with the intended message | ❌ W0 | ⬜ pending |
| ASK-04 | events + history rows | e2e / unit (core) | inside ASK-01 e2e; `history::tests::an_open_question_renders_not_yet_answered` | run with no questions has no rows | ❌ W0 | ⬜ pending |
| ASK-05 | blocking-human checkpoint answered in same session; runner backstop | e2e / unit (cli) | `ask_e2e::a_blocking_human_checkpoint_continues_in_the_same_session`, `pipeline_launch::tests::an_agent_exit_closes_its_stage_question_gate` | one Code `stage_launched`, zero `checkpoint_auto_decided` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-cli/tests/ask_e2e.rs` + POSIX sh stub helper in `tests/common`
- [ ] `crates/devflow-core/src/question.rs` test module
- [ ] `crates/devflow-cli/src/ask_server.rs` test module

---

## Manual-Only Verifications

These are not a PLAN: DevFlow's Code stage runs `/gsd-execute-phase 53` over every `*-PLAN.md`, on the pre-phase
binary, which has no question channel (a blocking checkpoint reached there is the 999.57 defect this phase closes).
So they run **after DevFlow's Validate for phase 53 has passed**, in an agent session at close-out, with the operator
answering the gates, before the PR is cut. They make four or more paid agent launches. Evidence goes to
`.planning/phases/53-the-question-channel/53-PROOF-RUN.md`. Run M1 to M4 in order.

Rules for every row: no `/gsd-*` command, no hand edit of `.devflow/` and no hand-run stage is used to make a run
pass; a DevFlow defect found is filed as a 999.x entry and reported, not routed around (CLAUDE.md dogfooding rule,
2026-11-08). Every PASS line cites the events, history or command output it rests on (T-53-31).

| ID | Behavior | Requirement | Why Manual | Test Instructions |
|----|----------|-------------|------------|-------------------|
| M1 | The runs use the installed binary built from the phase head | CLAUDE.md dogfooding | `cargo install` of the head, outside any DevFlow run | Precondition: `devflow status` in the DevFlow checkout shows no live runner for phase 53 (halt and say so if one is live), the tree is clean, and every phase plan's SUMMARY (53-01 to 53-14) is committed. Note `git rev-parse HEAD`, then run `cargo install --path crates/devflow-cli --locked --force` **before creating 53-PROOF-RUN.md or any other proof file**: `build.rs` sets the dirty flag from `git status --porcelain`, which counts untracked files, so a proof file created first makes the binary read `dirty`. Then create 53-PROOF-RUN.md and record `installed-head: <sha>`; check `test "$(devflow --version)" = "devflow 4.0.0-alpha.1 ($(git rev-parse HEAD), clean)"` exits 0; record `devflow --version`, `claude --version`, `codex --version`; record the full output of `cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor` and list its major items against the major changes the plan SUMMARYs record (every plan that changes devflow-core's pub API records its semver-checks output: 53-01, 53-02, 53-03, 53-05, 53-06, 53-07, 53-08, 53-09, 53-10, 53-13, and 53-14 on its BUILD verdict; an item no SUMMARY records is a defect to report); record `codex-config-before: <sha256 of ~/.codex/config.toml>` (T-53-30). |
| M2 | Live claude: the fixture's `blocking-human` checkpoint is asked through `devflow_ask`, held, answered at a gate, and Code continues in the same session | ASK-01, ASK-02, ASK-04, ASK-05 (roadmap criterion 1) | needs a real agent CLI and an operator answer | Build a fresh fixture with `scripts/scratch-dogfood-repo.sh --live-payload`; in it run `devflow start --phase 1 --agent claude --mode supervise`. When `devflow gate list` shows a `question` gate, `devflow gate show 1` must list the fixture's budget options. Hold the question at least 180 s after its `question_asked` before answering: DevFlow's stream idle monitor fires after 120 s of agent silence unless background tasks are open (`crates/devflow-core/src/monitor.rs` 64-121 and 941-985); during a held call claude emits a `tool_progress` heartbeat every 30 s in stream-json (53-MCP-SPIKE.md Item 3), and from 53-13 an open question gate also stretches the idle window, so the hold checks that a call held past 120 s is not killed, which an answer inside 120 s would not. Answer with `devflow gate answer 1 --gate-id <id> --option <id>`, then approve the later Validate gate as usual. Record the `gate list`/`gate show` output, the answer command, the seconds held, the events showing exactly one Code `stage_launched`, a `question_asked` and its `question_answered`, that no idle timeout was recorded for the stage, and the `devflow history 1` excerpt showing the question, its options, the answer, policy `operator_via_gate`, the operator and the time. Write `claude-live: PASS` only when all of these hold; otherwise `claude-live: FAIL` with the reason and a 999.x entry (an idle-monitor kill of the held call is such a defect). |
| M3 | Live codex on operator answer OD-1 (b): the call held to the 900 s cap returns `pending`, codex re-calls with the same `question_id`, the operator's answer reaches it on the re-call, and Code continues in the same session | ASK-01, ASK-02, ASK-04, ASK-05 (roadmap criterion 1) | needs a real agent CLI, an operator answer and a hold past the cap | As M2 with `--agent codex` on a fresh fixture, except the hold (operator decision (a), 2026-11-18, which accepts about 1.35 M input tokens per 15 min held, mostly cached, and 15 to 20 min of run time): answer no sooner than 1,020 s after the question's `question_asked` (the 900 s `ASK_HOLD_CAP_SECS`, codex's 60 s client margin, and 60 s for the re-call to arrive). One `pending` followed by a re-call with the same `question_id` before the answer is required: (1) shows it directly, and (2) with (3) shows it from DevFlow's own records and codex's output. (1) In `devflow logs --phase 1`, quote the codex capture's `devflow_ask` call whose result carries `status` `pending` and a `question_id`, and the next `devflow_ask` call carrying that `question_id`, whose result is the answer; if codex's capture does not show tool-call arguments or results, say so in 53-PROOF-RUN.md and file it as a 999.x observability gap. (2) In events.jsonl, for that question id: exactly one `question_asked`, one `gate_fired`, one `question_answered` and no `question_closed`, with `question_answered` at least 1,020 s after `question_asked`. (2) rests on 53-13's tested semantics: no call is held past the cap, a cancelled or dropped call closes the question (`question_closed`), and a call without the id asks a second question. (3) Answer with an option that is neither the first listed nor marked recommended, and quote where codex's output uses that option after `question_answered`: the answer reaches codex only as a `devflow_ask` result, so with (2) this shows the re-call by `question_id` delivered it, and a codex that chose by itself would not name it. Also record codex's input tokens, and how many were cached, from the capture's `turn.completed` usage, the seconds held, then `codex-config-after: <sha256>`, which must equal `codex-config-before` (T-53-30). Write `codex-live: PASS` only when M2's conditions, (2) and (3) hold, with (1) quoted or its absence recorded; an answer that reached codex on its first call, a `question_closed`, or a second `question_asked` is `codex-live: FAIL` with the reason and a 999.x entry. |
| M4 | D-17 closing run (operator answer "2 a"): a real GSD plan-phase run on a project that is not DevFlow sends its questions through the channel, and they are recorded | ASK-04, D-17 | live run on a throwaway project; feeds Phase 54's corpus | Create a throwaway git project under the session scratch directory that is not DevFlow, with a GSD `.planning/` holding a one-phase ROADMAP and no CONTEXT.md. Run `devflow start --phase 1 --agent claude --mode supervise --until plan`. Answer each `question` gate with `devflow gate answer 1 --gate-id <id> (--option <id> | --text <answer>)`. Record each question's text and options, the answers, whether any was a real GSD discuss question (observed, not assumed), the `devflow history 1` output, and the run's token use from claude's result `usage`. Write `closing-run: PASS` only when every question asked appears in `devflow history` with its answer and policy; if the run asked nothing, write `closing-run: NO-QUESTIONS` with the history excerpt and report it to the operator rather than counting it as a pass. Remove the scratch project afterwards and say so. |

Done when 53-PROOF-RUN.md holds `installed-head:`, `claude-live: PASS`, `codex-live: PASS`, matching codex config
hashes, and `closing-run: PASS` (or a reported `NO-QUESTIONS`).

pi has no row here: on 53-14's BUILD verdict its proof is the registration spike against a stub MCP server (53-MCP-SPIKE.md, pi addendum) and the stub-pi e2e, which is all the rewritten criterion 1 claims for pi; on REFUSE pi stays refused. No live DevFlow pi run is planned.

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 300s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

---
phase: 53-the-question-channel
verified: 2026-11-19T19:30:00Z
status: passed
score: 4/4 must-haves verified
covered_files: [".gitignore",".planning/phases/53-the-question-channel/53-01-PLAN.md",".planning/phases/53-the-question-channel/53-01-SUMMARY.md",".planning/phases/53-the-question-channel/53-02-PLAN.md",".planning/phases/53-the-question-channel/53-02-SUMMARY.md",".planning/phases/53-the-question-channel/53-03-PLAN.md",".planning/phases/53-the-question-channel/53-03-SUMMARY.md",".planning/phases/53-the-question-channel/53-04-PLAN.md",".planning/phases/53-the-question-channel/53-04-SUMMARY.md",".planning/phases/53-the-question-channel/53-05-PLAN.md",".planning/phases/53-the-question-channel/53-05-SUMMARY.md",".planning/phases/53-the-question-channel/53-06-PLAN.md",".planning/phases/53-the-question-channel/53-06-SUMMARY.md",".planning/phases/53-the-question-channel/53-07-PLAN.md",".planning/phases/53-the-question-channel/53-07-SUMMARY.md",".planning/phases/53-the-question-channel/53-08-PLAN.md",".planning/phases/53-the-question-channel/53-08-SUMMARY.md",".planning/phases/53-the-question-channel/53-09-PLAN.md",".planning/phases/53-the-question-channel/53-09-SUMMARY.md",".planning/phases/53-the-question-channel/53-10-PLAN.md",".planning/phases/53-the-question-channel/53-10-SUMMARY.md",".planning/phases/53-the-question-channel/53-11-PLAN.md",".planning/phases/53-the-question-channel/53-11-SUMMARY.md",".planning/phases/53-the-question-channel/53-12-PLAN.md",".planning/phases/53-the-question-channel/53-12-SUMMARY.md",".planning/phases/53-the-question-channel/53-13-PLAN.md",".planning/phases/53-the-question-channel/53-13-SUMMARY.md",".planning/phases/53-the-question-channel/53-14-PLAN.md",".planning/phases/53-the-question-channel/53-14-SUMMARY.md","ARCHITECTURE.md","OPERATIONS.md","crates/devflow-cli/src/ask_server.rs","crates/devflow-cli/src/commands.rs","crates/devflow-cli/src/main.rs","crates/devflow-cli/src/pipeline_gate.rs","crates/devflow-cli/src/pipeline_launch.rs","crates/devflow-cli/src/pipeline_outcomes.rs","crates/devflow-cli/src/preflight.rs","crates/devflow-cli/src/test_support.rs","crates/devflow-cli/tests/ask_checkpoint_e2e.rs","crates/devflow-cli/tests/ask_e2e.rs","crates/devflow-cli/tests/ask_launch_e2e.rs","crates/devflow-cli/tests/common/ask_client.rs","crates/devflow-cli/tests/common/mod.rs","crates/devflow-cli/tests/failover_e2e.rs","crates/devflow-cli/tests/fallback_agent_config.rs","crates/devflow-cli/tests/gitignore_coverage.rs","crates/devflow-cli/tests/phase7_cli.rs","crates/devflow-cli/tests/start_refusals_e2e.rs","crates/devflow-core/src/agent_result.rs","crates/devflow-core/src/agents/antigravity.rs","crates/devflow-core/src/agents/claude.rs","crates/devflow-core/src/agents/codex.rs","crates/devflow-core/src/agents/mod.rs","crates/devflow-core/src/agents/pi.rs","crates/devflow-core/src/doc_check.rs","crates/devflow-core/src/events.rs","crates/devflow-core/src/gates.rs","crates/devflow-core/src/history.rs","crates/devflow-core/src/lib.rs","crates/devflow-core/src/liveness.rs","crates/devflow-core/src/mode.rs","crates/devflow-core/src/monitor.rs","crates/devflow-core/src/prompt.rs","crates/devflow-core/src/question.rs","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap","crates/devflow-core/src/state.rs","crates/devflow-core/src/verify.rs","scripts/hooks/post-commit"]
covered_digest: "v4:sha256:d1e9013f664cf5249530a2606f33722282e8bb2f81817d22debc752b073a5ed6"
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "verify::phase_checkpoint_declarations with its CheckpointDeclaration type are deleted with their tests"
    reason: "Not orphans: they back verify::phase_has_human_only_checkpoint, the unattended-launch refusal at preflight.rs:1010; deleting them removes that refusal"
    accepted_by: "denniyahh"
    accepted_at: "2026-11-19T20:41:00Z"
human_verification:
  - test: "53-VALIDATION.md M1-M3: install the phase head, then run the live claude (M2) and live codex (M3, held past the 900 s cap, pending then re-call) proofs and record them in 53-PROOF-RUN.md"
    expected: "53-PROOF-RUN.md holds installed-head, claude-live: PASS, codex-live: PASS and matching codex-config-before/after hashes"
    why_human: "Roadmap criterion 1 names 53-PROOF-RUN.md as the live proof for claude and codex; the file does not exist, the runs need real agent CLIs, paid tokens and an operator answer, and the verifier was told not to launch live agents"
    resolution: "53-UAT.md item 1 result: pass. Live proofs M1-M3 recorded in 53-PROOF-RUN.md (commit c60cb1d): installed-head d74a788, claude-live: PASS, codex-live: PASS, codex-config-before equals codex-config-after."
  - test: "53-VALIDATION.md M4 (D-17 closing run): devflow start --phase 1 --agent claude --mode supervise --until plan on a throwaway non-DevFlow GSD project with no CONTEXT.md; answer each question gate"
    expected: "closing-run: PASS in 53-PROOF-RUN.md, every asked question in devflow history with its answer and policy (or NO-QUESTIONS reported to the operator)"
    why_human: "The roadmap sequencing note makes this run part of closing the phase and Phase 54's corpus; it is a live, paid run"
    resolution: "53-UAT.md item 2 result: pass. D-17 closing run M4 recorded in 53-PROOF-RUN.md (commit c60cb1d): closing-run: PASS; both asked questions are in devflow history with answer and policy operator_via_gate."
  - test: "Decide whether to accept the 53-10 deviation: verify::phase_checkpoint_declarations and verify::CheckpointDeclaration were kept, not deleted"
    expected: "Operator accepts an override (they back verify::phase_has_human_only_checkpoint, the unattended-launch refusal at crates/devflow-cli/src/preflight.rs:1010), or asks for that refusal to be redesigned"
    why_human: "The must-have says these are deleted; the code keeps them for a live caller, so the plan's premise that they were orphans was wrong. Accepting a must-have deviation needs the operator's name on an override"
    resolution: "53-UAT.md item 3 result: pass. Override accepted by the operator (denniyahh, 2026-11-19T20:41:00Z), recorded in 53-UAT.md item 3 by commit d74a788 and in this file's overrides entry."
---

# Phase 53: The Question Channel Verification Report

**Phase Goal:** An agent running under DevFlow asks DevFlow instead of a human. DevFlow serves an MCP tool, `devflow_ask`, over stdio; a call carries a question, typed options and context, blocks until an answer exists, and returns it to the agent in the same session. The one policy this phase ships, "operator via gate", turns each question into a gate the operator answers from the shell, with the notify hook firing as for any gate. Every stage prompt sends decision checkpoints, `blocking-human` gates and GSD discuss questions through the tool, and every question and answer is on the event log.
**Verified:** 2026-11-19T19:30:00Z
**Status:** passed
**Re-verification:** No — initial verification

Evidence base: the orchestrator's full workspace `cargo test` on HEAD f02ec75 (exit 0, 0 failed); `cargo nextest list --workspace` run by this verifier, in which every test named below appears once; no `#[ignore]` in the phase's ask test files or the core question/history/prompt modules; test bodies read for the criterion controls. No live agent was launched.

## Goal Achievement

### Observable Truths (roadmap contract)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | An agent launched by DevFlow calls `devflow_ask` over stdio, the call blocks until answered, the answer returns to the same session; proven live for claude and codex (53-PROOF-RUN.md), for pi by its spike, and by stub e2e for claude, codex and pi; opencode, hermes, antigravity refused by name (999.248). Control: a malformed payload is refused with a typed error and the agent is not left blocked | ✓ VERIFIED | Stub half verified: `ask_launch_e2e::a_stage_agent_asks_through_its_registration_and_finishes` (codex argv `-c mcp_servers.devflow.*`), `a_claude_stage_agent_reaches_the_server_through_its_mcp_config_file` (with a stub-without-flag control), `a_pi_stage_agent_asks_through_its_registration_and_finishes`; each asserts the operator's answer reached the stub and exactly one `stage_launched`. Refusal: `start_refusals_e2e::start_refuses_an_agent_without_an_ask_registration`, `ask_launch_e2e::an_unregistered_agent_is_refused_before_it_starts`. Control: `ask_e2e::a_malformed_call_returns_is_error_and_opens_no_gate`, `question::tests::each_malformed_payload_is_refused_naming_its_field`, `ask_e2e::a_call_offering_a_recommended_id_not_in_options_is_refused`. pi spike addendum in 53-MCP-SPIKE.md reads `pi verdict: BUILD`, held 900 s. Live half: 53-PROOF-RUN.md M1-M3 (commit c60cb1d) records installed-head d74a788, claude-live: PASS and codex-live: PASS with equal codex-config hashes; 53-UAT.md item 1 result: pass. |
| 2 | Under operator-via-gate a question appears in `gate list`/`gate show` with options; `gate answer --option` or free text resolves it; the notify hook fires and `notify_fired` is recorded as for any gate. Control: an answer naming another gate id leaves the question open | ✓ VERIFIED | `commands::tests::gate_show_prints_a_questions_options_and_recommended_marker`, `gate_answer_with_another_gate_id_leaves_the_question_open`, `gate_approve_on_a_question_gate_is_refused_naming_gate_answer`; `ask_e2e::a_question_fires_the_notify_hook_and_records_notify_fired`. The server fires through `pipeline_gate::fire_gate_notify_events` (ask_server.rs:1061), the same helper the runner path uses (pipeline_gate.rs:810), bounded by `gates::NOTIFY_HOOK_TIMEOUT` (gates.rs:2092) |
| 3 | Every stage prompt tells the agent to call the tool for decision checkpoints, `blocking-human` gates and GSD discuss questions; a `blocking-human` checkpoint under a one-shot launch is answered through the channel and the stage continues in the same session. Controls: source scan trips on a planted copy; the Phase 49 checkpoint fixture ends with the stage continued | ✓ VERIFIED | `ASK_POLICY` (prompt.rs:58-88) names all four question kinds and forbids positional choice. `prompt::tests::every_stage_prompt_routes_questions_through_devflow_ask` asserts 6 drivers x 8 intents each carry it exactly once before the completion contract; `no_prompt_template_carries_positional_self_resolution` scans rendered prompts and non-test source; `the_positional_scan_trips_on_a_planted_copy` is the control. `ask_checkpoint_e2e::a_blocking_human_checkpoint_continues_in_the_same_session` asserts the committed `budget.txt` holds `budget-120`, one Code `stage_launched`, one `question_asked` of kind blocking-human and its `question_answered` with `operator_via_gate`; control `a_checkpoint_reported_without_asking_reaches_the_failure_gate`. Relaunch symbols (`relaunch_checkpoint_session`, `exec_resume_command`, `checkpoint_auto_decide_prompt`, `GATE_RESOLUTION_RULE`, `checkpoint_auto_decided`, ...) have no match under `crates/` outside doc_check |
| 4 | Every question and answer is on `events.jsonl` with policy or operator and time; `devflow history` shows them in order. Control: an open question reads "not yet answered", distinct from an empty answer; a run that asked nothing shows no question rows | ✓ VERIFIED | `ask_e2e::question_events_record_the_question_and_its_answer`; `history::tests::an_open_question_renders_not_yet_answered`, `an_empty_text_answer_renders_distinct_from_not_yet_answered`, `a_run_with_no_questions_has_no_question_rows`, `question_rows_pair_by_question_id_not_position`, `a_second_resolution_for_one_question_is_shown_but_not_applied`; the ASK-05 e2e reads the rendered history for text, options, recommended marker, answer and policy |

**Score:** 4/4 roadmap truths verified (0 present, behavior-unverified). Criterion 1's live half is proven by 53-PROOF-RUN.md M1-M3 (53-UAT.md item 1 result: pass).

### Plan must-haves (14 plans)

Checked against the 53-VALIDATION.md test map (13 rows) and the named tests above, plus targeted source reads. One plan truth does not hold as written:

| Plan | Truth | Status | Evidence |
|------|-------|--------|----------|
| 53-10 | "`verify::phase_checkpoint_declarations` with its `CheckpointDeclaration` type are deleted with their tests" | PASSED (override) | Both still exist (crates/devflow-core/src/verify.rs:138, :294). 53-10-SUMMARY.md records the deviation: they back `verify::phase_has_human_only_checkpoint`, called by the unattended-launch refusal at crates/devflow-cli/src/preflight.rs:1010, so the plan's premise that they were orphans of the removed relaunch is false. Every other item in that truth is deleted (no match in crates/ or OPERATIONS.md). Override: Not orphans: they back verify::phase_has_human_only_checkpoint, the unattended-launch refusal at preflight.rs:1010; deleting them removes that refusal — accepted by denniyahh on 2026-11-19T20:41:00Z (53-UAT.md item 3). |
| 53-11 | Backstop truth: probe row ASK-05 "unclassified" edges covered by test evidence | ✓ VERIFIED | Each edge has a behavioral test that drives the binary: answered in-session and unasked fallback (`ask_checkpoint_e2e`, both), abort (`ask_launch_e2e::gate_abort_on_a_question_ends_the_run`), agent-exit backstop (`pipeline_launch::tests::an_agent_exit_closes_its_stage_question_gate`) |
| 53-14 | Ownership rule for a registration file written outside `.devflow/` | ✓ VERIFIED (not applicable) | pi's registration is `.devflow/ask-NN.mcp.ts` (53-MCP-SPIKE.md pi addendum; 53-14-SUMMARY deviation 1), so no file is written outside `.devflow/` |

**Override applied.** The 53-10 deviation is accepted through the overrides entry in this file's frontmatter (accepted by denniyahh, 2026-11-19T20:41:00Z; 53-UAT.md item 3, commit d74a788).

### Operator-accepted process deviation

TDD gate: 53-11 has no GREEN commit and 53-13 has no separate RED commit. The operator accepted both: "53-11 is test-only (feature built by 53-03/07/08/09; mutation controls show the failing direction), 53-13 observed RED before each change without a separate commit." Recorded as accepted, not a gap.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/ask_server.rs` | `__ask` stdio JSON-RPC server | ✓ VERIFIED | 1421 lines; initialize/tools/list/tools/call/ping tests; wired from main.rs and the launch registration |
| `crates/devflow-core/src/question.rs` | `QuestionSpec` validation | ✓ VERIFIED | 550 lines, 6 tests incl. character-count bounds and case-distinct ids |
| `crates/devflow-core/src/history.rs` | question rows | ✓ VERIFIED | 6 question tests listed above |
| `crates/devflow-core/src/prompt.rs` | shared `ask_policy!()` | ✓ VERIFIED | one macro, one const, interpolated into every rendered prompt (test-asserted) |
| `crates/devflow-core/src/agents/{claude,codex,pi}.rs` | per-launch registrations | ✓ VERIFIED | `launch_args_name_the_ask_registration`, `launch_args_set_tool_timeout_and_approval_before_the_prompt`, `launch_args_carry_the_pi_registration`, `every_driver_declares_its_ask_registration` |
| `crates/devflow-cli/tests/ask_e2e.rs`, `ask_launch_e2e.rs`, `ask_checkpoint_e2e.rs` | real-binary e2e | ✓ VERIFIED | 23 + 5 + 2 tests listed |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| stage launch (claude/codex/pi) | `devflow __ask` | `--mcp-config` file / `-c mcp_servers.devflow.*` / `-e .devflow/ask-NN.mcp.ts` | ✓ WIRED | three stub e2e tests read the registration from real argv and reach the server |
| `__ask` tools/call | question gate | gate open + `gate answer` | ✓ WIRED | `ask_e2e::a_call_blocks_until_answered_and_returns_the_option` |
| `__ask` | notify hook | `pipeline_gate::fire_gate_notify_events` | ✓ WIRED | ask_server.rs:1061 |
| `__ask` | events.jsonl -> `devflow history` | `question_asked`/`question_answered`/`question_closed` | ✓ WIRED | ASK-05 e2e reads history rendered from the server's events |

### Behavioral Spot-Checks

No test re-run by the verifier: the orchestrator's full workspace run on f02ec75 passed with 0 failed, and every test cited here appears exactly once in `cargo nextest list --workspace`, so each ran in that green run.

### Probe Execution

No phase-declared `scripts/*/tests/probe-*.sh`; the phase's probe edges are encoded as named Rust tests. SKIPPED.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| ASK-01 | 53-01, 05, 07, 08, 13, 14 | ✓ SATISFIED | stub e2e for claude, codex, pi and refusal tests pass; live claude/codex proofs M2/M3 PASS in 53-PROOF-RUN.md (53-UAT.md item 1 result: pass). |
| ASK-02 | 53-01, 04, 07 | ✓ SATISFIED | truth 2 |
| ASK-03 | 53-06 | ✓ SATISFIED | truth 3 |
| ASK-04 | 53-02, 07, 09, 11 | ✓ SATISFIED | truth 4 |
| ASK-05 | 53-03, 09, 11 | ✓ SATISFIED | truth 3 (stub of the Phase 49 fixture); live M2/M3 also exercise it |

All five IDs are claimed by plans; REQUIREMENTS.md maps no other ID to Phase 53. REQUIREMENTS.md already marks all five `[x]` Complete, ahead of the M2/M3 live proofs that criterion 1 names.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (phase diff, added lines) | - | TBD/FIXME/XXX/TODO/HACK/todo!/unimplemented! | none found | - |
| 53-REVIEW-DISPOSITION.md | - | 11 code-review findings (6 warning, 5 info), all `open`, none triaged | ⚠️ Warning | none contradicts a roadmap criterion; the verifier confirmed in source that WR-01 (`abort_requested` is read only at pipeline_launch.rs:2764, `resume` never clears it) and WR-03 (`Unanswered::Failed` emits nothing at ask_server.rs:927, so a question failed after its gate opened reads "not yet answered" forever) are real. Each needs a fix-or-defer disposition before close-out |

### Human Verification Required

### 1. Live claude and codex proofs (53-VALIDATION.md M1-M3)

**Test:** Install the phase head, then run M2 (claude, held at least 180 s) and M3 (codex, answered no sooner than 1,020 s after `question_asked`, pending then re-call), recording into 53-PROOF-RUN.md.
**Expected:** `installed-head:`, `claude-live: PASS`, `codex-live: PASS`, equal `codex-config-before`/`after`.
**Why human:** Roadmap criterion 1 names this file as the live proof; it needs real agent CLIs, paid tokens and an operator answer.
**Result:** pass. 53-UAT.md item 1; 53-PROOF-RUN.md M1-M3, commit c60cb1d.

### 2. D-17 closing run (53-VALIDATION.md M4)

**Test:** `devflow start --phase 1 --agent claude --mode supervise --until plan` on a throwaway non-DevFlow GSD project with no CONTEXT.md; answer each question gate.
**Expected:** `closing-run: PASS` (or `NO-QUESTIONS` reported), every question in `devflow history` with answer and policy.
**Why human:** Live, paid run that the roadmap makes part of closing the phase.
**Result:** pass. 53-UAT.md item 2; 53-PROOF-RUN.md M4 closing-run: PASS, commit c60cb1d.

### 3. 53-10 deviation

**Test:** Decide whether to accept the override above.
**Expected:** override recorded with the operator's name, or a request to redesign the unattended-launch refusal.
**Why human:** A must-have deviation needs the operator's acceptance.
**Result:** pass. 53-UAT.md item 3; override accepted by denniyahh at 2026-11-19T20:41:00Z, commit d74a788.

### Gaps Summary

No blocking gap found. The channel, the operator-via-gate policy, the prompts, the event log and history, and the removal of the checkpoint relaunch are present, wired, and exercised by real-binary e2e tests with negative controls. Closed after verification: the live claude/codex proofs and the D-17 closing run (53-PROOF-RUN.md, commit c60cb1d; 53-UAT.md items 1-2) and the 53-10 deviation (override accepted, 53-UAT.md item 3, commit d74a788). Open before close-out: dispositions for the 11 open code-review findings (WR-01 and WR-03 confirmed real).

---

_Verified: 2026-11-19T19:30:00Z_
_Verifier: Claude (gsd-verifier)_

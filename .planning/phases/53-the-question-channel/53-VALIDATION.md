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
- **Before `/gsd-verify-work`:** Full suite must be green, container parity, live proofs (claude + codex) and the closing run
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

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live claude and codex sessions call `devflow_ask` and continue after an answer | ASK-01, ASK-05 | needs real agent CLIs and an operator answer | dogfood run per CONTEXT.md; answer with `devflow gate answer` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 300s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

---
phase: "53"
slug: "the-question-channel"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-11-19"
---

# Phase 53 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Register authored at plan time (all 14 PLANs carry a `<threat_model>`); verified by gsd-security-auditor, block_on: high.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| agent → `devflow __ask` | the stage agent calls `devflow_ask` over stdio JSON-RPC | agent-authored question text and options (untrusted) |
| operator CLI → gate files | `devflow gate show/answer/abort` read and resolve the question gate | option id or free-text answer, gate id |
| runner → agent launch | the runner writes the per-launch registration (`.devflow/ask-NN.*`) and argv | server command, run binding, hold cap |
| gate → notify hook | an open question fires the operator's notify command | gate context as env vars |

---

## Threat Register

Evidence paths: `commands.rs`, `ask_server.rs`, `pipeline_launch.rs`, `pipeline_outcomes.rs`, `preflight.rs`, `main.rs` under `crates/devflow-cli/src`; `question.rs`, `gates.rs`, `history.rs`, `prompt.rs`, `monitor.rs`, `liveness.rs`, `workflow.rs`, `agents/*.rs` under `crates/devflow-core/src`; e2e tests under `crates/devflow-cli/tests`.

| Threat ID | Plan | Category | Severity | Disposition | Mitigation / evidence | Status |
|-----------|------|----------|----------|-------------|-----------------------|--------|
| T-53-01 | 53-01 | Spoofing | high | mitigate | commands.rs:3560 GateId::parse; :3566-3567 gate-names-id + live-waiter checks; test :7884 | closed |
| T-53-02 | 53-01 | Tampering | medium | mitigate | ask_server.rs:465-473 one locked writer; main.rs:442,455 logs to stderr | closed |
| T-53-03 | 53-01 | Elevation | medium | mitigate | pipeline_outcomes.rs:981,995; commands.rs:3472 screen before Gates::answer | closed |
| T-53-04 | 53-01 | Spoofing | low | accept | see Accepted Risks | closed |
| T-53-05 | 53-02 | Tampering | medium | mitigate | history.rs:229 every field through render_gate_context; test :516 | closed |
| T-53-06 | 53-02 | Info disclosure | low | accept | see Accepted Risks | closed |
| T-53-07 | 53-03 | Elevation | high | mitigate | relaunch route deleted (doc_check.rs:2275 token pin); pipeline_launch.rs:6069 one launch | closed |
| T-53-08 | 53-03 | Repudiation | medium | mitigate | pipeline_launch.rs:2838-2850 failure gate, reason :2653; tests :6069, :6125 | closed |
| T-53-09 | 53-04 | Spoofing | high | mitigate | commands.rs:3593 exact option-id match; tests :7884, :7915 | closed |
| T-53-10 | 53-04 | Tampering | medium | mitigate | commands.rs:4352-4374 (show), :3268 (list) through render_gate_context | closed |
| T-53-11 | 53-04 | DoS | low | mitigate | commands.rs:5177-5184 question stages exempt; test :8124 with control | closed |
| T-53-34 | 53-04 | DoS | medium | mitigate | commands.rs:4069-4085 sweep leaves question gate; test :8211 | closed |
| T-53-35 | 53-04 | Repudiation | low | mitigate | commands.rs:3103,3113 status line + answer verb; test :8177 | closed |
| T-53-12 | 53-05 | Tampering | medium | mitigate | question.rs:118-142 typed per-field parse; ask_server.rs:617-623 refused before gate; ask_e2e.rs:193 | closed |
| T-53-13 | 53-05 | DoS | medium | mitigate | question.rs:12-22 bounds, :197-207 refusal, :228 MAX_OPTIONS; tests :527-547 | closed |
| T-53-14 | 53-06 | Elevation | high | mitigate | prompt.rs:1548 scan + :1571 planted control; no-self-resolve fallback :141, test :1409 | closed |
| T-53-15 | 53-06 | Repudiation | medium | mitigate | 53-06-SUMMARY.md snapshot diff and headings recorded | closed |
| T-53-16 | 53-07 | DoS | high | mitigate | ask_server.rs:111-126 endings, :969-981 stop/runner-gone; ask_e2e.rs:467,495,511,536,556 | closed |
| T-53-17 | 53-07 | Spoofing | medium | mitigate | ask_server.rs:622 check_run_binding (:816-843) before any gate; ask_e2e.rs:568 | closed |
| T-53-18 | 53-07 | Tampering | medium | mitigate | ask_server.rs:363-371 removable FIFO; :936-966 per-stage ask lock; ask_e2e.rs:712,724,742 | closed |
| T-53-19 | 53-07 | Tampering | medium | mitigate | gates.rs:2117-2126 context passed as env vars, never interpolated | closed |
| T-53-36 | 53-07 | Tampering | high | mitigate | gates.rs:1136-1152 close_if_current under gate lock; ask_e2e.rs:608 | closed |
| T-53-37 | 53-07 | DoS | high | mitigate | gates.rs:2092 30 s bound, :2130 own pgrp, :2255 killpg; ask_e2e.rs:374 | closed |
| T-53-38 | 53-07 | Tampering | medium | mitigate | liveness.rs:423-430 flock lock never removed; test :626 two contenders x50 | closed |
| T-53-39 | 53-07 | DoS | medium | mitigate | ask_server.rs:628 T from arrival, :948 bounded lock wait; ask_e2e.rs:777,813 | closed |
| T-53-20 | 53-08 | Tampering | high | mitigate | codex.rs:107 every value via escape_toml_basic_string; quote test :287 | closed |
| T-53-21 | 53-08 | Elevation | medium | mitigate | pipeline_launch.rs:9998 one-call-site scan + planted control; preflight.rs:102 | closed |
| T-53-22 | 53-08 | Tampering | medium | mitigate | pipeline_launch.rs:199-200,249 writes only .devflow/ask-NN; codex argv only (codex.rs:132) | closed |
| T-53-23 | 53-08 | DoS | medium | mitigate | preflight.rs:99-103; start_refusals_e2e.rs:906 refusal by name | closed |
| T-53-40 | 53-08 | Info disclosure | low | mitigate | .gitignore ask-*.mcp.json/.ts; gitignore_coverage.rs; commands.rs:1990-1995 discard | closed |
| T-53-41 | 53-08 | Tampering | low | accept | see Accepted Risks | closed |
| T-53-24 | 53-09 | Tampering | high | mitigate | ask_server.rs:1239-1242 update_state field-only closure; workflow.rs:443 write lock | closed |
| T-53-25 | 53-09 | DoS | medium | mitigate | pipeline_launch.rs:2371 question-only backstop, :2461 resume; test :7365 | closed |
| T-53-42 | 53-09 | Repudiation | high | mitigate | ask_server.rs:1192 abort committed before claim/close; pipeline_launch.rs:2425-2452; test :7414 | closed |
| T-53-43 | 53-09 | DoS | medium | mitigate | pipeline_launch.rs:2764-2766 abort removes state; test :7277 | closed |
| T-53-26 | 53-09 | Info disclosure | low | accept | see Accepted Risks | closed |
| T-53-27 | 53-10 | Elevation | medium | mitigate | CheckpointDeclaration kind/family and State::checkpoint_approval removed from crates/ | closed |
| T-53-28 | 53-10 | DoS | low | accept | see Accepted Risks | closed |
| T-53-29 | 53-11 | Elevation | medium | mitigate | doc_check.rs:2275-2286 relaunch tokens; test :2488 | closed |
| T-53-33 | 53-12 | Repudiation | medium | mitigate | 53-12-SUMMARY.md lists each deletion; pinning tests in phase7_cli.rs, antigravity.rs | closed |
| T-53-32 | 53-13 | DoS | medium | mitigate | monitor.rs:972-990 continuous-silence bound; tests :2854-2942; pipeline_launch.rs:10116 | closed |
| T-53-44 | 53-13 | DoS | medium | accept | see Accepted Risks | closed |
| T-53-45 | 53-13 | Spoofing | low | mitigate | ask_server.rs:631-640 unknown question_id refused; ask_e2e.rs:990 | closed |
| T-53-46 | 53-14 | Tampering | high | mitigate | pi driver writes nothing under ~/.pi/agent (pi.rs fs writes test-only); directory checked: no devflow entry | closed |
| T-53-47 | 53-14 | Tampering | high | mitigate | superseded by 53-14 deviation 1, does not arise: no code writes the project's .pi/; ask_launch_e2e.rs:504 | closed |
| T-53-48 | 53-14 | Info disclosure | medium | mitigate | superseded by 53-14 deviation 1: file is .devflow/ask-NN.mcp.ts (pi.rs:33), removed on discard | closed |
| T-53-49 | 53-14 | Tampering | medium | mitigate | pi.rs:106 config via serde_json::to_string; quote test pi.rs:302 | closed |
| T-53-50 | 53-14 | Info disclosure | medium | mitigate | superseded by 53-14 deviation 1, does not arise: nothing written outside .devflow/ (`*` .gitignore, workflow.rs:144) | closed |
| T-53-SC | 53-01 … 53-14 (13 plans) | Tampering | low | accept | see Accepted Risks | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-53-01 | T-53-04 | the agent runs as the same user with skip-permissions; `answered_by` records USER (commands.rs:3568) | planner (53-01 threat model) | 2026-11-18 |
| AR-53-02 | T-53-06, T-53-26 | full question text in local events.jsonl; D-16 requires it; documented in OPERATIONS.md | planner (53-02, 53-09) | 2026-11-18 |
| AR-53-03 | T-53-41 | global MCP servers left unchanged; the `devflow` server name is reserved (OPERATIONS.md) | planner (53-08) | 2026-11-18 |
| AR-53-04 | T-53-28 | removed gate kind gets no state migration: no users (operator no-migration rule, 2026-10-12) | operator rule | 2026-10-12 |
| AR-53-05 | T-53-44 | OD-1 (b) hold cap chosen; per-hold token cost documented in OPERATIONS.md | operator (OD-1, 2026-11-18) | 2026-11-18 |
| AR-53-06 | T-53-SC (13 plans) | no package installed; Cargo.toml/Cargo.lock unchanged vs origin/develop | planner (each plan) | 2026-11-18 |

*Accepted risks do not resurface in future audit runs.*

---

## Audit Notes (non-blocking)

- T-53-16: the runner-gone ending is in code (ask_server.rs:974-976) but no test exercises it during a held call; the run-stopped test shares `run_ending` but not that branch. Small follow-up test.
- T-53-46: the spike's before/after sha256 lists no longer exist (executor scratch); closure rests on the driver code and the current `~/.pi/agent/` contents.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-19 | 62 | 62 | 0 | gsd-security-auditor via /gsd-secure-phase 53 |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-19

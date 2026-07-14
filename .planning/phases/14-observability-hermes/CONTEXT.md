# Phase 14: Observability + Hermes Support

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

## Goal

Give a running loop visibility instead of a black box between launch and
exit, settle the remaining agent-execution architecture question
(`capture_agent_output()` sync path), and add first-class Hermes support —
agent adapter, skill-file rewrite, and the Hermes plugin session mode.

Source: external code review of the codebase (2026-07-08) for the
observability items; Hermes items moved here from Phase 15 on 2026-07-14
(workload balance + synergy: the plugin's gate watcher and status display
consume this phase's `events.jsonl` and the Phase 13 notify hook, so
building them together avoids a cross-phase integration seam).

**Rescoped 2026-07-14 (MVP restructure):** the reliability/correctness
items originally scoped here — verdict-vs-ran split, native envelope
parsing, worktree-by-default, gate notify hook, configurable
`GATE_TIMEOUT_SECS`, WR-11 silent-halt fix — moved to **Phase 13 (MVP Core
Loop)** because the core loop isn't usable unattended without them.

`devflow.toml` / configurable pipeline (branch model, stage/agent command
templates) is explicitly **out of scope** for this phase — shelved for a
future phase pending a deliberate decision to reverse the Phase 11 "config
eliminated" call.

---

## 14a — Observability

- [ ] `devflow logs [--follow]` — tail the existing capture file
      (stdout/stderr) for the running/most-recent agent.
- [ ] Append-only `.devflow/events.jsonl` — one line per state
      transition, gate fire/response/ack, and hook run. Makes any future
      frontend (TUI, Hermes plugin, web) a reader instead of requiring a
      new integration. (The gate notify hook itself ships in Phase 13;
      events.jsonl should record its firings.)
- [ ] `devflow status` shows more than stage + PID where practical (last
      known action, elapsed time).

## 14b — capture_agent_output() Sync-Path Decision

Previously flagged in `12-CONTEXT.md` as **unclaimed by any phase**;
claimed here.

- [ ] Decide: should `sequentagent` keep a synchronous
      `capture_agent_output()` path (11i-5, still public/in use), or move
      behind monitor-owned execution like everything else?
- [ ] Implement the decision; remove the dead path if sequentagent moves
      to the monitor.

## 14c — Hermes Agent Adapter *(moved from Phase 15)*

- [ ] `HermesAgent` adapter — `hermes exec --non-interactive --json`
- [ ] AgentKind variant + parser + display + adapter_for()
- [ ] Extend Phase 13b's native-envelope parsing to Hermes's `--json`
      output so it gets the same authoritative completion signal as
      Claude/Codex (DEVFLOW_RESULT marker as fallback)
- [ ] Tests: parser aliases, shared prompt, adapter name, envelope parsing

## 14d — Hermes Skill-File Rewrite *(moved from Phase 15)*

Prerequisite for 14e's plugin work — the current gate-response path.

- [ ] Hermes skill file (`skills/hermes/devflow/SKILL.md`) is stale: says
      `v0.5.0+`, references `--max-turns 50` (not a real adapter flag) and
      `devflow check` (now `advance`), and has machine-specific paths
      (`~/Github/devflow`, distrobox notes). Rewrite against current
      CLI/adapter behavior, including the Phase 13 notify hook and
      configurable gate timeout.

## 14e — Hermes Plugin *(moved from Phase 15)*

A first-class DevFlow session mode for Hermes. When active, Hermes operates
as DevFlow's human interface rather than a general assistant — preventing
confusion between devflow commands and general LLM prompts, and
facilitating tighter integration with gate responses, stage transitions,
and state inspection.

### Mode Behavior

When DevFlow mode is active, Hermes:

- **Interprets gate responses directly** — "approved", "reject", "review"
  trigger gate response file writes without the user needing to address
  Hermes explicitly
- **Surfaces devflow state** — `devflow status` auto-runs on session start,
  active gates shown prominently
- **Prevents prompt confusion** — general questions ("what's the capital of
  France?") are redirected or handled separately from devflow operations
- **Auto-loads devflow context** — project state, active phase, open gates

### Implementation

- [ ] Hermes plugin: `~/.hermes/plugins/devflow/` — session mode, tools,
      hooks
- [ ] Toggle: `/devflow on` / `/devflow off`
- [ ] Gate watcher integration: replaces cron poll with plugin-native push
      (build on the Phase 13 gate notify hook and this phase's
      `events.jsonl` rather than a bespoke integration)
- [ ] Status display: active phase, current stage, open gates in session
      header
- [ ] Auto-response: intercept "approved"/"reject"/"review" in Telegram
      and write response files
- [ ] Plugin docs: installation, usage, configuration

## Explicitly Out of Scope (this phase)

- Antigravity agent adapter — stays in Phase 15 with the rest of the OSS
  work.
- Verdict-vs-ran split, native envelope parsing (Claude/Codex), worktree
  default, gate notify hook, configurable gate timeout, WR-11 — Phase 13
  (MVP Core Loop).
- `devflow.toml` / configurable stage-agent pipeline, branch model,
  verify/lint command config — shelved for a future phase; requires a
  deliberate re-decision on the Phase 11 "config eliminated" call.
- Publishing to crates.io — publish-prep done in Phase 12; actual publish
  belongs with Phase 15 OSS readiness.
- ARCHITECTURE.md rewrite, `.devflow.yaml` decoy removal, `--help`
  snapshot CI test — routed to Phase 15 (OSS Readiness; same class of
  doc-accuracy work).

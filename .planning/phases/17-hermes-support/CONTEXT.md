# Phase 17: Hermes Support

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

> Split out of Phase 14 on 2026-07-16 — Phase 14 had absorbed both the
> Hermes work (moved in from Phase 15 on 2026-07-14 for workload balance)
> and, a day later, the deferred CR-03 parallel-safety flaw, making it the
> heaviest phase. Hermes is personal-infrastructure and should not gate
> parallel-safety correctness, so it moves to its own phase after Phase 15
> (OSS Readiness). Content below carried over from Phase 14's 14c/14d/14e
> unchanged apart from renumbering.

## Goal

Add first-class Hermes support: `HermesAgent` adapter (17a), skill-file
rewrite (17b), and the Hermes plugin session mode (17c).

**Depends on:** Phase 14 — the plugin's gate watcher and status display
consume Phase 14's `events.jsonl` and Phase 13's gate notify hook. Phase 15
(OSS Readiness) precedes this phase in roadmap order.

---

## 17a — Hermes Agent Adapter *(was 14c)*

- [ ] `HermesAgent` adapter — `hermes exec --non-interactive --json`
- [ ] AgentKind variant + parser + display + adapter_for()
- [ ] Extend Phase 13b's native-envelope parsing to Hermes's `--json`
      output so it gets the same authoritative completion signal as
      Claude/Codex (DEVFLOW_RESULT marker as fallback)
- [ ] Tests: parser aliases, shared prompt, adapter name, envelope parsing

## 17b — Hermes Skill-File Rewrite *(was 14d)*

Prerequisite for 17c's plugin work — the current gate-response path.

- [ ] Hermes skill file (`skills/hermes/devflow/SKILL.md`) is stale: says
      `v0.5.0+`, references `--max-turns 50` (not a real adapter flag) and
      `devflow check` (now `advance`), and has machine-specific paths
      (`~/Github/devflow`, distrobox notes). Rewrite against current
      CLI/adapter behavior, including the Phase 13 notify hook and
      configurable gate timeout.

## 17c — Hermes Plugin *(was 14e)*

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
      (build on the Phase 13 gate notify hook and Phase 14's
      `events.jsonl` rather than a bespoke integration)
- [ ] Status display: active phase, current stage, open gates in session
      header
- [ ] Auto-response: intercept "approved"/"reject"/"review" in Telegram
      and write response files
- [ ] Plugin docs: installation, usage, configuration

## Explicitly Out of Scope (this phase)

- Antigravity agent adapter — Phase 15 (OSS Readiness).
- Observability (`devflow logs`, `events.jsonl`, richer `status`) and
  CR-03 parallel-safety — Phase 14 (Parallel Safety + Observability).

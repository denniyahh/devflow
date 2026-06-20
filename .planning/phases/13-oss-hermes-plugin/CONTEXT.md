# Phase 13: OSS Readiness + Hermes Plugin

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

## Goal

Make DevFlow ready for public consumption: dev container, contribution docs,
Hermes + Antigravity agent support, and a first-class Hermes plugin that gives
Hermes a dedicated DevFlow session mode.

---

## 13a — Dev Container

- [ ] `.devcontainer/devcontainer.json` — Rust + git + fish
- [ ] Dockerfile: Fedora 41 base, cargo, claude, codex CLI
- [ ] Mount `~/Github` for project access
- [ ] Container test: `cargo build && cargo test && cargo clippy` passes
- [ ] `devflow devcontainer` subcommand: `enter`, `build`, `destroy`

---

## 13b — Open Source Contributing

- [ ] CONTRIBUTING.md: fork → branch → test → PR workflow
- [ ] ARCHITECTURE.md: extension point docs for agent adapters
- [ ] README: installation, quickstart, agent support table
- [ ] CODE_OF_CONDUCT.md, SECURITY.md (if missing)
- [ ] CI badge + PR gate status

---

## 13c — Hermes + Antigravity Agent Adapters

- [ ] `HermesAgent` adapter — `hermes exec --non-interactive --json`
- [ ] `AntigravityAgent` adapter — launch protocol for Antigravity CLI
- [ ] AgentKind variants + parser + display + adapter_for()
- [ ] Tests: parser aliases, shared prompt, adapter name

---

## 13d — Hermes Plugin

A first-class DevFlow session mode for Hermes. When active, Hermes operates as
DevFlow's human interface rather than a general assistant — preventing confusion
between devflow commands and general LLM prompts, and facilitating tighter
integration with gate responses, stage transitions, and state inspection.

### Mode Behavior

When DevFlow mode is active, Hermes:

- **Interprets gate responses directly** — "approved", "reject", "review" trigger
  gate response file writes without the user needing to address Hermes explicitly
- **Surfaces devflow state** — `devflow status` auto-runs on session start,
  active gates shown prominently
- **Prevents prompt confusion** — general questions ("what's the capital of
  France?") are redirected or handled separately from devflow operations
- **Auto-loads devflow context** — project state, active phase, open gates

### Implementation

- [ ] Hermes plugin: `~/.hermes/plugins/devflow/` — session mode, tools, hooks
- [ ] Toggle: `/devflow on` / `/devflow off`
- [ ] Gate watcher integration: replaces cron poll with plugin-native push
- [ ] Status display: active phase, current stage, open gates in session header
- [ ] Auto-response: intercept "approved"/"reject"/"review" in Telegram and write
  response files
- [ ] Plugin docs: installation, usage, configuration

---

## Deferred From Phase 11

- Dev container (was capacity-permitting in Phase 11, now Phase 13)
- Hermes agent adapter (was Phase 12, pushed to 13)

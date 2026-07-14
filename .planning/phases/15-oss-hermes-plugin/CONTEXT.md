# Phase 15: OSS Readiness + Hermes Plugin

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

> Renumbered from Phase 13 on 2026-07-14 — Phase 13 was repurposed as
> "MVP Core Loop" per the MVP restructure decision. Content unchanged
> except section renumbering, the crates.io publish item, and the gate
> watcher note in 15d.

## Goal

Make DevFlow ready for public consumption: dev container, contribution docs,
Hermes + Antigravity agent support, and a first-class Hermes plugin that gives
Hermes a dedicated DevFlow session mode.

---

## 15a — Dev Container

- [ ] `.devcontainer/devcontainer.json` — Rust + git + fish
- [ ] Dockerfile: Fedora 41 base, cargo, claude, codex CLI
- [ ] Mount `~/Github` for project access
- [ ] Container test: `cargo build && cargo test && cargo clippy` passes
- [ ] `devflow devcontainer` subcommand: `enter`, `build`, `destroy`

---

## 15b — Open Source Contributing

- [ ] CONTRIBUTING.md: fork → branch → test → PR workflow
- [ ] ARCHITECTURE.md: full rewrite against current code — the existing
      file still describes the deleted v1 step machine (`Idle → Branching →
      Planning → Executing → Verifying → Docsing → Shipping → Cleaning`),
      `.devflow.yaml` parsing, `phase_prompt()`, `auto_verify`/`auto_docs`,
      `feature_finish`/`release_finish`. Replace with the actual Stage enum
      (`stage.rs`), GSD slash-command prompts (`prompt.rs`), and hooks
      (`hooks.rs`). Include extension point docs for agent adapters.
- [ ] README: full command-table rewrite against `main.rs` — currently
      documents `init`/`config`/`verify`/`lint`/`ship`/`confirm`/
      `rejectpr`, none of which exist, and `--mode auto|manual` instead of
      the real `auto|supervise`. Also installation, quickstart, agent
      support table.
- [ ] Delete the stray `.devflow.yaml` at repo root — decoy file left over
      from before config was eliminated in Phase 11; contradicts the doc
      comment in `config.rs` stating v2.0.0 has no `.devflow.yaml`. Note:
      `11-VALIDATION.md` (11k-13) originally deferred this specific item to
      Phase 12 ("delete the file or move it to a docs/ example directory");
      routed here instead since it's the same class of work as the rest of
      13b. Don't duplicate in Phase 12.
- [ ] **(IN-01, Phase 11 code review)** `crates/devflow-core/src/lib.rs:26`
      module-level rustdoc still shows `devflow check` and `devflow ship`
      as example commands — both removed in Phase 11. Replace with valid
      examples (`devflow start`, `devflow status`). Confirmed still present
      (2026-07-08).
- [ ] `--help` snapshot test — commit a snapshot of `devflow --help` output
      and assert it in CI, so README/CLI drift can't recur silently.
      (Build on the phase7 CLI test harness.)
- [ ] Hermes skill file (`skills/hermes/devflow/SKILL.md`) is independently
      stale: says `v0.5.0+`, references `--max-turns 50` (not a real
      adapter flag) and `devflow check` (now `advance`), and has
      machine-specific paths (`~/Github/devflow`, distrobox notes). Rewrite
      against current CLI/adapter behavior as part of this pass, ahead of
      15d's plugin work.
- [ ] CODE_OF_CONDUCT.md, SECURITY.md (if missing)
- [ ] CI badge + PR gate status
- [ ] Publish `devflow` to crates.io — metadata + dry-run/package landed
      in Phase 12 (12-06); the actual publish belongs here, once docs
      describe the real product

---

## 15c — Hermes + Antigravity Agent Adapters

- [ ] `HermesAgent` adapter — `hermes exec --non-interactive --json`
- [ ] `AntigravityAgent` adapter — launch protocol for Antigravity CLI
- [ ] AgentKind variants + parser + display + adapter_for()
- [ ] Tests: parser aliases, shared prompt, adapter name

---

## 15d — Hermes Plugin

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
      (build on the Phase 13 gate notify hook and Phase 14 `events.jsonl`
      rather than a bespoke integration)
- [ ] Status display: active phase, current stage, open gates in session header
- [ ] Auto-response: intercept "approved"/"reject"/"review" in Telegram and write
  response files
- [ ] Plugin docs: installation, usage, configuration

---

## Deferred From Phase 11

- Dev container (was capacity-permitting in Phase 11, now Phase 13)
- Hermes agent adapter (was Phase 12, pushed to 13)

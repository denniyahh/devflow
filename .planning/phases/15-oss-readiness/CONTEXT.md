# Phase 15: OSS Readiness

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

> Renumbered from Phase 13 on 2026-07-14 — Phase 13 was repurposed as
> "MVP Core Loop" per the MVP restructure decision. Later the same day,
> all Hermes work (agent adapter, skill-file rewrite, plugin) moved to
> Phase 14 (Observability + Hermes Support) for workload balance and
> synergy with `events.jsonl`. This phase keeps the pure OSS packaging
> plus the Antigravity adapter.

## Goal

Make DevFlow ready for public consumption: dev container, contribution
docs, README/ARCHITECTURE rewrite, Antigravity agent support, and the
actual crates.io publish.

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
- [ ] CODE_OF_CONDUCT.md, SECURITY.md (if missing)
- [ ] CI badge + PR gate status
- [ ] Publish `devflow` to crates.io — metadata + dry-run/package landed
      in Phase 12 (12-06); the actual publish belongs here, once docs
      describe the real product

---

## 15c — Antigravity Agent Adapter

- [ ] `AntigravityAgent` adapter — launch protocol for Antigravity CLI
- [ ] AgentKind variant + parser + display + adapter_for()
- [ ] Tests: parser aliases, shared prompt, adapter name

---

## Moved to Phase 14 (2026-07-14)

- `HermesAgent` adapter (was 15c)
- Hermes skill-file rewrite (was in 15b)
- Hermes plugin (was 15d)

## Deferred From Phase 11

- Dev container (was capacity-permitting in Phase 11, now this phase)

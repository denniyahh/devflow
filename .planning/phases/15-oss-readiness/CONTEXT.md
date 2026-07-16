# Phase 15: Dogfood Enablement + OSS Readiness

**Status:** 15a Complete (2026-07-16) | 15b Scoped | **Priority:** HIGH (15a) / MEDIUM (15b) | **Target:** TBD

> Renumbered from Phase 13 on 2026-07-14; Hermes work moved to Phase 14
> (later split to Phase 16). **Rescoped 2026-07-16 (dogfood-first):** the
> operator's top priority is a fully functional MVP for dogfooding, so this
> phase now leads with a small "dogfood enablement" wave (15a) — the
> `devflow gate` subcommand that removes the last hand-edited-JSON step,
> an accurate operator reference, and the doc-accuracy quick hits pulled
> forward from old 15b. The OSS packaging (15b) follows, and is intended to
> be executed **through DevFlow itself** as the first post-MVP dogfood run.
> The Antigravity adapter (old 15c) is deferred out of the phase entirely —
> it serves neither dogfooding nor OSS readiness urgently.

## Goal

15a: remove the remaining dogfooding friction — answering gates via CLI
instead of hand-written response files, one truthful operator page, and the
small accuracy items (decoy config, stale rustdoc, `--help` snapshot guard).
Exit criterion: a real phase runs end-to-end with every gate answered via
`devflow gate` + the notify hook.

15b: make DevFlow ready for public consumption — README/ARCHITECTURE
rewrite against v2 reality, CONTRIBUTING, dev container, crates.io publish.
Run as a DevFlow dogfood.

---

## 15a — Dogfood Enablement *(added 2026-07-16)*

- [x] `devflow gate` subcommand:
      `gate list` (open gates: phase, stage, age, context),
      `gate approve <phase> [--stage S] [--note ...]`,
      `gate reject <phase> --note ... [--stage S]` (note containing
      "abort" aborts, per `GateAction::from_response`). Stage
      auto-resolves when the phase has exactly one open gate. Core side:
      `Gates::list_open` + `Gates::respond` (atomic write; refuses when no
      gate is open or a response already exists). Response writes emit a
      `gate_response_written` event.
- [x] `OPERATIONS.md` — accurate operator reference: pipeline, real command
      table, gate protocol + subcommand, env vars
      (`DEVFLOW_GATE_NOTIFY_CMD`, `DEVFLOW_GATE_TIMEOUT_SECS`,
      `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, `RUST_LOG`,
      `DEVFLOW_LOG_FORMAT`), `.devflow/` file inventory, notify-hook recipe.
- [x] Delete the stray `.devflow.yaml` at repo root (decoy; contradicts
      config.rs's "v2.0.0 has no .devflow.yaml"; 11k-13).
- [x] **(IN-01)** Fix stale `lib.rs` module rustdoc examples against the
      real CLI (start requires `--mode`; no `check`/`ship`).
- [x] `--help` snapshot test — commit `devflow --help` output and assert it
      in tests so CLI/doc drift can't recur silently.

## 15b — Open Source Packaging *(run as a DevFlow dogfood)*

- [ ] README: full rewrite against `main.rs` — currently documents
      `init`/`config`/`verify`/`lint`/`ship`/`confirm`/`rejectpr` (none
      exist) and `--mode auto|manual` instead of `auto|supervise`.
      Installation, quickstart, agent support table; link OPERATIONS.md.
- [ ] ARCHITECTURE.md: full rewrite against current code (Stage enum, GSD
      prompts, hooks, per-phase state + two-level locking, events.jsonl,
      monitor ownership). Extension-point docs for agent adapters.
- [ ] CONTRIBUTING.md: fork → branch → test → PR workflow.
- [ ] Dev container: `.devcontainer/devcontainer.json`, Dockerfile,
      container test (`cargo build && cargo test && cargo clippy`).
      (`devflow devcontainer` subcommand only if it earns its keep.)
- [ ] CI badge + PR gate status; CODE_OF_CONDUCT/SECURITY refresh if needed.
- [ ] Publish `devflow` to crates.io (publish-prep landed in 12-06; publish
      once docs describe the real product).

## Deferred out of this phase (2026-07-16)

- **Antigravity agent adapter** (old 15c) — unscheduled backlog; revisit
  after Phase 16 when there's a concrete need.

## Moved to Phase 14 → 16 (2026-07-14 / 2026-07-16)

- `HermesAgent` adapter, Hermes skill-file rewrite, Hermes plugin — now
  Phase 16 (Hermes Support).

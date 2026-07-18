# DevFlow

## What This Is

DevFlow is a Rust CLI that automates the mechanical workflow steps an AI
coding agent needs to drive a development phase end-to-end: branch creation,
agent launch, completion detection, gated human checkpoints, versioning,
docs/changelog updates, and cleanup. It runs a 5-stage pipeline
(Define → Plan → Code → Validate → Ship) against any of several agent-agnostic
adapters (Claude Code, OpenAI Codex, OpenCode), in either `auto` (unattended)
or `supervise` (gated) mode.

## Core Value

A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never
silently corrupt its own state or lose a human's gate decision, even under a
mid-run crash or kill.

## Requirements

### Validated

- ✓ 5-stage GSD-native pipeline (Define→Plan→Code→Validate→Ship), `Mode`
  (auto/supervise) with forced-gate-on-repeated-failure — Phase 11
- ✓ File-based human gate protocol (write/poll/ack, 7-day timeout) — Phase 11
- ✓ Agent-agnostic adapters (Claude Code, Codex, OpenCode) — Phase 11
- ✓ Hybrid git-based SemVer (`version.rs`), hardened against workspace +
  array-of-tables `Cargo.toml` shapes — Phase 11, hardened in Phase 12
- ✓ Crash-safe state persistence (atomic temp+rename `save_state`) — Phase 12
- ✓ Argv-based agent spawn (no shell interpolation of agent-controlled
  data) — Phase 12
- ✓ crates.io publish-readiness (metadata, `--dry-run`, `cargo package`) —
  Phase 12, publish itself intentionally held until Phase 15 (OSS readiness)
- ✓ Reliable terminal finalization, reviewed external post-condition probes,
  retained attempt evidence, deterministic doc/runtime invariants,
  worktree-aware CLI behavior, attempt history, and persistent gates — Phase 16

### Active

- Phase 17 — **Pipeline Dogfood Follow-Up:** reconcile the terminal Ship
  event evidence and scope confirmed reliability repairs before feature work.
- Phase 18 — **Hermes Support:** HermesAgent adapter, current CLI skill file,
  and an `events.jsonl`-driven gate-watcher plugin. Phases 13–16 are complete.

### Out of Scope

- Bootstrap tooling (`new-project`, `map-codebase`) — deferred to its own
  future phase; no detailed requirements exist yet (Phase 12 CONTEXT.md,
  2026-07-08)

## Context

- Originally built around `tmux` for agent launching; Phase 11's GSD-native
  refactor replaced this entirely with direct process spawning + a monitor
  daemon (`monitor.rs`) that captures stdout/stderr/exit/pid to files and
  invokes `devflow advance` on completion. `tmux` is no longer a runtime
  dependency.
- The CLI surface was substantially cut and rebuilt in Phase 11, then expanded
  through Phase 16. Current operator commands include `start`, `gate`, `logs`,
  `history`, `parallel`, `sequentagent`, `reference`, `cleanup`, `status`,
  `list`, `recover`, `doctor`, and `test`; `advance` remains hidden/internal.
- Workspace version is `1.2.0`. Code/docs historically over-claimed
  "v2.0.0" as current; Phase 12 corrected this — 2.0.0 is the *target*
  version for the Phase 11–15 arc, not yet shipped, and will only be bumped
  when that line actually ships.
- No `.planning/REQUIREMENTS.md` exists in this project; requirements are
  tracked per-phase in each phase's `CONTEXT.md`, not via formal REQ-IDs.

## Constraints

- **Tech stack**: Rust 2024 edition, workspace of `devflow-core` (lib) +
  `devflow-cli` (binary). Dependencies: serde, clap, thiserror, tracing (zero
  network deps).
- **Runtime**: `git` required; no `tmux` dependency since Phase 11.
- **Build**: `cargo build --release` → single static binary (~20MB).
- **Versioning**: hybrid git-based SemVer via `version.rs` — do not
  reintroduce commit-message-based versioning (deprecated, reorganized June
  2026).

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Replace tmux-based agent launch with direct process spawn + monitor daemon | tmux launcher had a monitor deadlock bug; direct spawn + file-based capture is simpler and testable | ✓ Good |
| File-based gate protocol instead of a live RPC/socket | Human response can come from any interface (Hermes, manual file drop, future UI) without DevFlow depending on any one of them | ✓ Good |
| Hold `cargo publish` until Phase 15 (OSS readiness) — after MVP loop (13) + observability (14) | Publishing is irreversible — a version can never be reused or unpublished; first public release should be reliability-hardened and documented | — Pending |
| MVP restructure (2026-07-14): Phase 13 → MVP Core Loop, old 13 → 15 | Priority is dogfooding the core loop on real projects again; OSS packaging is worthless until the loop it packages works end-to-end | — Pending |
| Reintroduce a minimal `devflow.toml` | Phase 16 required typed reliability knobs while preserving hardcoded git-flow branch constants; environment variables override project values | ✓ Good |
| Defer bootstrap (`new-project`/`map-codebase`) out of Phase 12 | Genuinely unscoped — no detailed requirements exist yet; inventing them would be speculative | — Pending |

## Key Files

| File | Purpose |
|---|---|
| `.planning/ROADMAP.md` | Phase plan source of truth (current — not the stale pre-GSD `ROADMAP.md` at repo root, which predates the GSD reorg) |
| `.planning/codebase/` | Codebase map (7 documents, 2026-06-17 — predates Phases 1-12; consider `/gsd-map-codebase` before Phase 13) |
| `.planning/CONCERNS.md` | Top findings from the original pre-Phase-1 codebase audit |

---
*Last updated: 2026-07-18 after Phase 17 dogfood spike*

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
- ✓ Fail-closed outcome pipeline: typed agent outcomes incl. ResourceKilled/
  AgentUnavailable with exhaustive outcome→action policy, `Unknown`
  non-advance, per-loop infra-failure counter, preflight readiness gate, and
  build provenance + self-dogfood staleness gate — Phase 17 (AC-4 narrowed:
  security-artifact + reviewer-set preflight checks deferred to Phase 18)
- ✓ Dogfood reliability hardening: project-aware `doctor` reconciliation,
  monitor liveness (`State.monitor_pid` + `liveness()` predicate consumed by
  both `status` and `doctor`), Code↔Validate safety-gate reachability
  (`transition_resets_consecutive_failures`), Layer 0/Validate verdict
  reconciliation, worktree-aware build staleness enforcement, preflight-gate
  re-run wedge fix (bounded `preflight_retries`) — Phase 18, v1.5.0
- ✓ Release integrity + `main.rs` decomposition: `.devflow/` artifact hygiene
  (path/username redaction via a single `ensure_devflow_dir()` chokepoint),
  `commit_path` no-longer-allows-empty commits, `main.rs` split 8,467 → 7
  focused modules with zero behavioral change (single shared `ENV_MUTEX`
  preserved across the split), AI change acceptance contract — Phase 19,
  v1.6.0
- ✓ Release correctness + operator control: `VersionBump` rewrites workspace
  member self-pins by construction (20a), `cleanup --force` is fail-closed on
  any live agent/monitor with bounded-backoff retry (20b), `devflow start
  --until <stage>` gives a clean stop point short of Ship (20c), `devflow
  release --check` read-only preflight (self-pin, divergence, publish order,
  signing viability) (20d), `devflow ship --phase N [--force]` manual
  override reusing `finish_workflow` when the monitor is dead (20e) — Phase
  20, v1.7.0

### Active

*(none currently in flight. Phase 21 (Operator Legibility & Observability)
shipped as v1.8.0, 2026-07-24 (PR #23 → main, signed tag, GitHub Release,
published to crates.io). Phase 20 shipped as v1.7.0,
2026-07-23. The v2.0.0 milestone stays open — it does NOT close at Phase 20 or any other
fixed phase; numbering continues (21, 22, …) until a genuinely breaking
change earns the 2.0 slot. `/gsd-complete-milestone` is not run here.
Hermes Support, previously slotted as "Phase 18," was rescoped out during
the 2026-07-20 reprioritization to Dogfood Reliability Hardening and now
sits in the backlog as `999.1` — it is NOT automatically next; backlog
items require `/gsd-review-backlog` promotion.)*

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
- Workspace version is `1.8.0` (Phase 21, shipped 2026-07-24). Code/docs
  historically over-claimed "v2.0.0" as current; Phase 12 corrected this.
  The `v2.0.0` label names an open-ended milestone, not a bounded arc with
  a scheduled closing phase — decided 2026-07-23 (ROADMAP.md "Milestone
  stays open"): nothing across Phase 20's or Phase 21's units was inherently
  breaking, so each shipped as a minor bump and the milestone continues past
  both with no predetermined endpoint. `2.0.0` remains reserved for a
  future genuinely-breaking change, whenever that happens to land.
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
| Hoist `ENV_MUTEX` into one shared mutex during the `main.rs` split (Phase 19) | Three independent `static Mutex<()>` definitions were sound only by accident (each guarded a disjoint variable set); per-module mutexes would have silently broken the serialization 19i's fix depended on | ✓ Good |
| Split `main.rs` as flat sibling modules, not a `commands/` subdirectory (Phase 19) | Mapping Phase 18's plans onto proposed clusters showed pipeline state machine absorbed 3 of 7 plans vs. commands' 2 — a subdirectory buys zero wave reduction | ✓ Good |
| Tighten `cleanup --force`'s liveness guard to fail-closed on ANY live agent pid, not just Healthy/BetweenStages monitor states (Phase 20b, cross-AI review) | `Liveness::Unknown` (no recorded monitor) and `Stuck` (dead monitor) both still mean the agent process could be alive; a monitor-state-only guard left a real deletion-race hole | ✓ Good |
| Reuse `finish_workflow` verbatim for the manual `ship --phase` override rather than reimplementing Ship logic (Phase 20e) | The existing fail-closed terminal-Ship contract (retry-gate-reopen, `workflow_finished` emission) already does exactly what a second out-of-process trigger needs; reimplementing risks drift between the monitor-driven and manual paths | ✓ Good |

## Key Files

| File | Purpose |
|---|---|
| `.planning/ROADMAP.md` | Phase plan source of truth (current — not the stale pre-GSD `ROADMAP.md` at repo root, which predates the GSD reorg) |
| `.planning/codebase/` | Codebase map (7 documents, 2026-06-17 — predates Phases 1-12; consider `/gsd-map-codebase` before Phase 13) |
| `.planning/CONCERNS.md` | Top findings from the original pre-Phase-1 codebase audit |

---
*Last updated: 2026-07-24 after Phase 21 (Operator Legibility & Observability)
shipped as v1.8.0 — 4/4 plans, verified 21/21, signed tag + GitHub Release
published; the v2.0.0 milestone stays open (no fixed closing phase)*

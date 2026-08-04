# DevFlow

## What This Is

DevFlow is a Rust CLI that automates the mechanical workflow steps an AI
coding agent needs to drive a development phase end-to-end: branch creation,
agent launch, completion detection, gated human checkpoints, versioning,
docs/changelog updates, and cleanup. It runs a 5-stage pipeline
(Define → Plan → Code → Validate → Ship), today against three supported
agent adapters (Claude Code, OpenAI Codex, OpenCode) — opinionated by design,
not a universal agent platform — in either `auto` (unattended) or
`supervise` (gated) mode.

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
- ✓ End-to-end dogfood blockers: `compute_version` derives from the highest
  *reachable* semver tag plus conventional-commit classification and refuses on
  an unreachable baseline (25c), a major bump opens a human gate and never
  ships unattended, `enforce_build_staleness` is adjudicated once at `start`
  rather than per stage (25b), `ensure_base_ref_current` repairs a stale base
  via a compare-and-swap `git update-ref` behind a repository-wide
  checked-out predicate (25a), `doctor` / `gate sweep --reap-strays` filter the
  structural `/proc` census against a machine-wide registry-reachable pid set
  so a live registered monitor can no longer be reported or SIGKILLed as an
  orphan (25d), the 999.47 cmdline-inheritance CI flake is closed under an
  11-observation streak with human sign-off (25e), and CONTRIBUTING.md's
  release procedure no longer drifts from what 25c implements (25f) — Phase 25,
  verified 10/10, unreleased

### Active

*(none currently in flight. Phase 22's **light dogfooding trial** slice is
complete (22-01, 22-02, resolving 999.30/DEN-55) and integrated for release
alongside the urgent 999.37 sandbox-escape fix; the broader Concurrency &
Governance scope (999.4, 999.26, 999.28) remains unplanned. Phase 21
(Operator Legibility & Observability)
shipped as v1.8.0, 2026-07-24 (PR #23 → main, signed tag, GitHub Release,
published to crates.io). Phase 20 shipped as v1.7.0,
2026-07-23. **The v2.0.0 milestone was CLOSED 2026-08-02** after spanning releases 2.0.0,
2.1.0 and 2.2.0; the open-ended framing (it did not close at Phase 20 or any fixed phase)
governed it until then. The active milestone is now **v2.3.0**, declared 2026-08-02 and
deliberately **bounded** — it closes when the 999.64 arc lands (Phase 30 + Phase 31). See
ROADMAP.md. `/gsd-complete-milestone` has still not been run: v2.0.0 was closed by
documentation, because the archive step would have swept Phase 30's unexecuted plans into
`.planning/milestones/`.
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
- Workspace version is `2.2.0` (shipped 2026-07-31). Code/docs historically
  over-claimed "v2.0.0" as current; Phase 12 corrected this.
  The `v2.0.0` label named an **open-ended** milestone rather than a bounded
  arc — decided 2026-07-23 (ROADMAP.md "Milestone stays open") — and on that
  basis it spanned the 2.0.0, 2.1.0 and 2.2.0 releases, each a minor bump
  because nothing in them was inherently breaking. **That milestone was closed
  2026-08-02.** The active milestone is **v2.3.0**, and unlike its predecessor
  it is bounded: it closes when the 999.64 arc lands. Phase 31 changes
  DevFlow's internal launch path and agent adapter rather than the CLI surface,
  so v2.3.0 is expected to remain a minor bump; the `3.0.0` slot stays reserved
  for a genuinely breaking change, whenever that lands.
- No `.planning/REQUIREMENTS.md` exists in this project; requirements are
  tracked per-phase in each phase's `CONTEXT.md`, not via formal REQ-IDs.

## Constraints

- **Tech stack**: Rust 2024 edition, workspace of `devflow-core` (lib) +
  `devflow-cli` (binary). Dependencies: serde, clap, thiserror, tracing (zero
  network deps).
- **Runtime**: `git` required; no `tmux` dependency since Phase 11.
- **Build**: `cargo build --release` → single static binary (~20MB).
- **Versioning**: git-derived SemVer via `version.rs` — the version derives
  from the highest reachable semver tag (ancestry-checked, semver-ordered,
  never `git describe`) plus the conventional-commit intent of the commits
  added since that baseline was released. The version file (`Cargo.toml`) is
  a derived **output** that `VersionBump` writes, not an input `compute_version`
  reads. A major bump opens a human gate inside preflight and never ships
  unattended. (Phase 25, D-06/D-07/D-09/D-11 — supersedes the June 2026
  commit-message-derivation ban, lifted 2026-07-27.)

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
| Never honour an operator-set `GIT_DIR` — scrub the repository-local git vars unconditionally at `Command` construction (Phase 27, D-03) | `GIT_DIR` outranks `current_dir()`, so `mutating_project_root` — the guard added expressly to stop `release --execute`/`sync` acting on an unnamed repo — compared two paths, saw a match, and passed while the executor pushed and published against a different repository. Honouring the variable would have preserved that bypass | ✓ Good |
| Scrub at construction, apply `.envs(...)` after (Phase 27, WR-03 fix) | Ordering is load-bearing: it makes the scrub the default while still letting an adapter that *deliberately* sets one of these vars win — which is what keeps Codex's unsigned-commit override working | ✓ Good |

## Key Files

| File | Purpose |
|---|---|
| `.planning/ROADMAP.md` | Phase plan source of truth (current — not the stale pre-GSD `ROADMAP.md` at repo root, which predates the GSD reorg) |
| `.planning/codebase/` | Codebase map (7 documents, 2026-06-17 — predates Phases 1-12; consider `/gsd-map-codebase` before Phase 13) |
| `.planning/CONCERNS.md` | Top findings from the original pre-Phase-1 codebase audit |

---
*Last updated: 2026-07-28 after Phase 25 (End-to-End Dogfood Blockers)
shipped as v2.1.0 — 18/19 plans (25-10 superseded by 25-13), verified 10/10
across five gap-closure rounds, 129/129 threats closed, broken-windows ledger at
0 open / 1 waived / 4 fixed. PR #47 → develop, #50 squash-merged to main,
signed tag `v2.1.0` (maintainer key, fingerprint verified), main→develop sync
merge-committed (#51), GitHub Release published, and both crates published to
crates.io in order (devflow-core → devflow). The v2.0.0 milestone stayed open at
this point (no fixed closing phase); it was closed 2026-08-02*

*Phase 27 (Scrub Redirecting Git Environment From Production Calls) completed and
verified 2026-07-30 — 6/6 plans, 7/7 must-haves, all 41 production
`Command::new("git")` sites routed through `devflow_core::git::{hermetic_command,
git_command}`, Sweep A at 0, both hostile-`GIT_DIR` acceptance commands green at
HEAD (411/0 core, 188/0 cli). This unblocks 999.25 (release executor) and 999.52
(`devflow sync`), which named it prerequisite #1. Not yet shipped/merged.*

---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
<!-- refreshed: 2026-09-18 -->

# Architecture

**Analysis Date:** 2026-09-18

## System Overview

```text
┌─────────────────────────────────────────────────────────────┐
│              CLI frontend  (binary `devflow`)                │
│  arg parsing + dispatch: `crates/devflow-cli/src/main.rs`    │
├──────────────┬──────────────┬──────────────┬────────────────┤
│ commands.rs  │ pipeline_    │ pipeline_    │ pipeline_gate  │
│ (subcommand  │ launch.rs    │ outcomes.rs  │ .rs (transit., │
│  handlers)   │ (seam A)     │ (seam B)     │  gates, abort) │
│ preflight.rs │ staleness.rs │ parallel.rs  │ config_parse.rs│
└──────┬───────┴──────┬───────┴──────┬───────┴───────┬────────┘
       ▼              ▼              ▼               ▼
┌─────────────────────────────────────────────────────────────┐
│          devflow-core library  (`crates/devflow-core/src`)   │
│ stage/state/workflow │ agents/* (AgentDriver) │ monitor      │
│ agent_result + outcome_policy │ gates │ hooks/git/worktree   │
│ version/ship/ship_evidence │ lock/registry/events/recover    │
└──────┬──────────────────────────────────────────────────────┘
       ▼
┌─────────────────────────────────────────────────────────────┐
│ External processes + on-disk state                           │
│ `git`, agent CLIs (claude, codex, opencode, pi, hermes, agy) │
│ `.devflow/` (state-NN.json, events.jsonl, gates/, lock-NN)   │
│ `.planning/` (GSD-owned; config.json via gsd_config.rs only) │
└─────────────────────────────────────────────────────────────┘
```

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| CLI entry | tracing init, clap `Command` enum, `run()` dispatch, `CliError` | `crates/devflow-cli/src/main.rs` |
| Subcommand handlers | start, status, logs, history, gate family, recover, doctor | `crates/devflow-cli/src/commands.rs` |
| Pipeline seam A | `launch_stage`, `run_monitor`, `advance`, `resume` | `crates/devflow-cli/src/pipeline_launch.rs` |
| Pipeline seam B | `handle_*_outcome`, validate classification, checkout hooks | `crates/devflow-cli/src/pipeline_outcomes.rs` |
| Pipeline seam C | stage transitions, gate firing/resolution, loop-backs, finish/abort | `crates/devflow-cli/src/pipeline_gate.rs` |
| Preflight | readiness gate before `monitor::spawn_monitor` | `crates/devflow-cli/src/preflight.rs` |
| Staleness | binary provenance vs tree under test (self-dogfood guard) | `crates/devflow-cli/src/staleness.rs` |
| Parallel | multi-phase concurrent runs, one worktree each | `crates/devflow-cli/src/parallel.rs` |
| Stage machine | `Stage` enum Define→Plan→Code→Validate→Ship, GSD command mapping | `crates/devflow-core/src/stage.rs` |
| Run state | `State` (`#[non_exhaustive]`), `AgentKind` | `crates/devflow-core/src/state.rs` |
| Persistence | per-phase `.devflow/state-{phase:02}.json` load/save | `crates/devflow-core/src/workflow.rs` |
| Agent drivers | `AgentDriver` trait + one module per agent | `crates/devflow-core/src/agents/` |
| Monitor | detached daemon owning the agent process and capture files | `crates/devflow-core/src/monitor.rs` |
| Completion detection | DEVFLOW_RESULT parsing, four-layer decision engine | `crates/devflow-core/src/agent_result.rs` |
| Outcome policy | pure exhaustive `AgentStatus` → `Action` table | `crates/devflow-core/src/outcome_policy.rs` |
| Gates | file-protocol human handoff in `.devflow/gates/` | `crates/devflow-core/src/gates.rs` |
| Hooks | side effects at transitions (branch, docs, merge, changelog, version) | `crates/devflow-core/src/hooks.rs` |
| Git / worktree | plain `git` / `git worktree` subprocess wrappers | `crates/devflow-core/src/git.rs`, `crates/devflow-core/src/worktree.rs` |
| Versioning | git-derived SemVer | `crates/devflow-core/src/version.rs` |
| Ship | ship bookkeeping + structural "did it ship" oracle | `crates/devflow-core/src/ship.rs`, `crates/devflow-core/src/ship_evidence.rs` |
| Concurrency guards | per-phase PID lock; machine-global project registry | `crates/devflow-core/src/lock.rs`, `crates/devflow-core/src/registry.rs` |
| Event log | append-only `.devflow/events.jsonl` (schema v1) | `crates/devflow-core/src/events.rs` |
| GSD config writer | sole writer of `.planning/config.json` | `crates/devflow-core/src/gsd_config.rs` |
| Verify | operator-approved post-condition probes from PLAN.md frontmatter | `crates/devflow-core/src/verify.rs` |
| Canary | detects loss of the undocumented agent-CLI behaviour DevFlow relies on | `crates/devflow-core/src/canary.rs` |
| Config | minimal `devflow.toml` + fixed git-flow branch model | `crates/devflow-core/src/config.rs` |

## Pattern Overview

**Overall:** Library + thin-ish CLI frontend driving a linear, file-persisted stage machine; each stage is executed by an external AI coding agent under a detached monitor process that re-invokes `devflow advance` on exit.

**Key Characteristics:**

- Core returns structured types; the CLI formats output (`crates/devflow-core/src/lib.rs` module doc).
- All durable state is files under `.devflow/`, keyed per phase — no daemon or database.
- External tools (git, agents, gh) are invoked as subprocesses; no git library dependency (`crates/devflow-core/Cargo.toml` deps: libc, serde, serde_json, toml, thiserror, tracing, semver, git-conventional).
- Policy decisions are pure functions (`outcome_policy::decide_action`, `hooks::hooks_for_transition`) separated from I/O.

## Layers

**CLI layer:**

- Purpose: parse args, orchestrate the pipeline, render output, map errors to `CliError`
- Location: `crates/devflow-cli/src/`
- Contains: `pub(crate)` functions only; binary-only crate (no lib target)
- Depends on: `devflow-core`, clap, tracing-subscriber, serde_json, thiserror
- Used by: operators, the monitor daemon (re-invokes `devflow advance`/`monitor`)

**Core layer:**

- Purpose: stage machine, state, agent drivers, gates, hooks, git mechanics
- Location: `crates/devflow-core/src/`
- Contains: `pub mod` per concern; re-exports `Mode`, `Stage`, `State`, `AgentKind`
- Depends on: std + small crates; shells out to `git` and agent CLIs
- Used by: `devflow-cli` only (published separately to crates.io, core before cli)

## Data Flow

### Primary Request Path (`devflow start`)

1. Parse and dispatch `Command::Start` (`crates/devflow-cli/src/main.rs:531`)
2. Handler validates flags / creates worktree + state (`crates/devflow-cli/src/commands.rs`)
3. `launch_stage` runs preflight, renders the prompt via the agent's `AgentDriver::render_prompt`, and calls `monitor::spawn_monitor` (`crates/devflow-cli/src/pipeline_launch.rs:1234`, `crates/devflow-core/src/monitor.rs:296`)
4. Monitor owns the agent, captures stdout/exit into `.devflow/`, then invokes `devflow advance --phase N` (`crates/devflow-core/src/monitor.rs`)
5. `advance` acquires the per-phase lock, loads state, evaluates the result via `agent_result` (`crates/devflow-cli/src/pipeline_launch.rs:1510`)
6. `outcome_policy::decide_action` maps status to `Advance | AutoResume | GateInfra | GateReview` (`crates/devflow-core/src/outcome_policy.rs:38`)
7. Outcome handlers / gate seam transition, run hooks, write gates, or launch the next stage (`crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/src/pipeline_gate.rs`)
8. Ship: `handle_ship_outcome` finishes the workflow; post-ship hooks merge, bump version, append changelog, clean up branch (`crates/devflow-cli/src/pipeline_outcomes.rs:838`, `crates/devflow-core/src/hooks.rs`)

### Gate Resolution

1. Pipeline writes a gate request into `.devflow/gates/` and sets `State.gate_pending` (`crates/devflow-core/src/gates.rs`)
2. Operator or Hermes poller runs `devflow gate approve|reject|show|list|sweep` (`crates/devflow-cli/src/main.rs` `GateCommand`)
3. The blocked `advance` observes the response and continues or loops back (`crates/devflow-cli/src/pipeline_gate.rs`)

**State Management:**

- `.devflow/state-{phase:02}.json` per phase (`workflow.rs`); `.devflow/lock-{phase:02}` PID lock (`lock.rs`); `.devflow/events.jsonl` audit log (`events.rs`). `.devflow/` is self-ignored via its own `.gitignore` (`*`).

## Key Abstractions

**`AgentDriver`:**

- Purpose: per-agent prompt rendering, command building, completion parsing, health, env, sandbox
- Examples: `crates/devflow-core/src/agents/{claude,codex,opencode,pi,hermes,antigravity}.rs`
- Pattern: trait with defaulted methods (`crates/devflow-core/src/agents/mod.rs:70`)

**`Stage` / `State`:**

- Purpose: linear 5-stage chain and serialized run record
- Examples: `crates/devflow-core/src/stage.rs`, `crates/devflow-core/src/state.rs`
- Pattern: enum with `next()`; `#[non_exhaustive]` serde struct with `#[serde(default)]` for additive fields

**`Hook`:**

- Purpose: side effects at transitions (only `Validate→Ship` → `DocsUpdate` in `hooks_for_transition`; merge/version/changelog/cleanup via `hooks_after_ship`)
- Examples: `crates/devflow-core/src/hooks.rs`

**`PhaseId`:**

- Purpose: integer or decimal phase (`35`, `35.1`)
- Examples: `crates/devflow-core/src/phase_id.rs`

## Entry Points

**`devflow` binary:**

- Location: `crates/devflow-cli/src/main.rs` (`fn main`, line 506)
- Triggers: operator, monitor daemon, Hermes cron
- Responsibilities: subcommands Start, Advance, Monitor, Resume, Gate, Logs, History, Parallel, Reference, Cleanup, Status, List, Recover, Test, Doctor, Release, Ship, Stop, Evidence

**Hidden re-entry (`devflow monitor` / `devflow advance`):**

- Location: `crates/devflow-cli/src/pipeline_launch.rs` (`run_monitor`, `advance`)
- Triggers: spawned by `monitor::spawn_monitor`

## Architectural Constraints

- **Threading:** synchronous, no async runtime; concurrency is multi-process (detached monitor per stage, one per phase under `parallel`).
- **Global state:** none in-process of note; shared state is on disk (`.devflow/`) and the machine-global registry (`crates/devflow-core/src/registry.rs`).
- **Locking:** per-phase, not per-project (`lock.rs`); held by `advance` across long gate waits.
- **External file ownership:** `.planning/config.json` belongs to GSD — only `gsd_config.rs` writes it, preserving key order (`serde_json` `preserve_order` in root `Cargo.toml`).
- **Circular imports:** Not detected (CLI → core only).

## Anti-Patterns

### Wildcard arm in outcome dispatch

**What happens:** Adding a `_ =>` arm to an `AgentStatus` match.
**Why it's wrong:** A new status could silently advance; the policy is intentionally exhaustive.
**Do this instead:** Extend `decide_action` in `crates/devflow-core/src/outcome_policy.rs` with an explicit arm.

### Writing `.planning/config.json` directly

**What happens:** Serializing GSD config from another module.
**Why it's wrong:** Races live GSD readers and clobbers operator keys.
**Do this instead:** Go through `crates/devflow-core/src/gsd_config.rs`.

### Growing `main.rs`

**What happens:** Adding pipeline logic to `main.rs`.
**Why it's wrong:** The pipeline was deliberately split into seams A/B/C.
**Do this instead:** Place logic in the matching `pipeline_*.rs` seam or `commands.rs`.

## Error Handling

**Strategy:** `thiserror` enums in core per module (e.g. `lock::LockError`); CLI wraps into `CliError` (`crates/devflow-cli/src/main.rs:489`), `main` prints `error: {err}` and exits 1.

**Patterns:**

- Failures that would otherwise be invisible (monitor output to /dev/null) are recorded in `events.jsonl` via `events::emit`.
- Indeterminate agent outcomes gate for a human rather than advancing.

## Cross-Cutting Concerns

**Logging:** `tracing` to stderr; `RUST_LOG` filter (default `info`); `DEVFLOW_LOG_FORMAT=json` for JSON lines.
**Validation:** preflight (`preflight.rs`), staleness (`staleness.rs`), post-condition probes (`verify.rs`), ship evidence (`ship_evidence.rs`).
**Authentication:** delegated to agent CLIs and `gh auth` (checked in preflight).

---

*Architecture analysis: 2026-09-18*

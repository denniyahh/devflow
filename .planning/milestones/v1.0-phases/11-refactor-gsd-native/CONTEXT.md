# Phase 11: Refactor to GSD-Native Architecture

**Status:** Decisions Complete — Ready for Planning | **Priority:** HIGH | **Target:** v2.0.0 | **Decisions resolved:** 2026-06-19

## Goal

Rebuild devflow as an execution engine that wraps GSD, communicates with Hermes
via gate file protocol, and eliminates all skip/configurability cruft. Keep what
works (agent adapters, monitor daemon, git primitives, tracing) and throw away
the rest (old state machine, skip logic, check command, broken Planning step).

---

## Architecture Decisions

### Devflow's Role

Devflow is an **execution engine with stages, gates, and hooks.** GSD is one of
its tools. Hermes is the human interface. The relationship:

```
Human ──► Hermes ──► DevFlow (execution engine)
                        │
                        ├── Stages: Define → Plan → Code → Validate → Ship
                        ├── Gates: file-based protocol → Hermes → Human
                        ├── Tools: GSD (planning), Claude/Codex (coding),
                        │         cargo (verify), gh CLI (ship)
                        └── Hooks: branch mgmt, docs, changelogs, version bumps
```

### Two Modes of Operation

**Full Auto Mode:** Define and Plan are gospel — run once. Code ↔ Validate
loops until clean. Then Ship. No human gates unless validation fails repeatedly.

```
Define ──► Plan ──► Code ──► Validate ──► Ship
                        ▲          │
                        └──────────┘ (auto-loop until pass)
```

**Human Supervision Mode:** Same pipeline, but Validate fires a gate to Hermes.
Human picks from three responses:

```
Validate gate ──► Hermes ──► Human
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
        Ship       Code         Define
     ("approved") ("fix it")  ("totally wrong, restart")
```

- `approved` / "ship it" → Ship
- `reject` / "fix and retry" → Code → Validate → gate again
- `review` / "totally wrong, discuss" → Define → Plan → Code → Validate → gate

### Ship Stage Loop

Ship runs `/gsd-code-review` as a gate. If review finds issues, loops back to Code:

```
Code ──► Validate ──► Ship ──► done
  ▲          ▲          │
  │          │    code-review finds issues
  └──────────┴──────────┘
```

---

## State Machine

### Stages

| Stage | GSD Command | Purpose |
|---|---|---|
| **Define** | `/gsd-discuss-phase {N}` | Gather requirements via adaptive Q&A, write CONTEXT.md |
| **Define** (optional) | `/gsd-spec-phase {N}` | Pre-discuss clarity with ambiguity scoring |
| **Plan** | `/gsd-plan-phase {N}` | Research + plan + verify loop, write PLAN.md |
| **Code** | `/gsd-execute-phase {N}` | Wave-based parallel execution via coding agent |
| **Validate** | `/gsd-validate-phase {N}` | Nyquist coverage audit |
| **Ship** | `/gsd-ship {N}` | PR + review + merge prep |

### Loop Paths

| Path | Trigger | Devflow prompts agent with |
|---|---|---|
| Fix and retry | Validate finds mechanical issues | `/gsd-audit-fix` |
| Fix and retry | Validate finds structural gaps | `/gsd-execute-phase {N} --gaps-only` |
| Fix and retry | Both mechanical + structural | audit-fix first, then gaps-only |
| Full restart | Human says "totally wrong" | `/gsd-discuss-phase {N}` (fresh) |
| Ship retry | Code review finds issues | Back to Code → Validate → Ship |

### Validate Decision Tree

```
Validate runs
  ├─ Clean → Ship
  └─ Issues found
       ├─ Mechanical only → audit-fix → re-Validate
       ├─ Structural only → gaps-only → re-Validate
       └─ Both → audit-fix → gaps-only → re-Validate
```

---

## Gate File Protocol

Devflow pushes gates to Hermes via files. Hermes cron polls and delivers to
human. Protocol already spike-tested and working.

```
.devflow/gates/
  NN-{stage}.json            # devflow writes: {phase, gate, context, timestamp}
  NN-{stage}.response.json   # Hermes writes: {approved, note, responded_by}
  NN-{stage}.ack.json        # devflow writes: {received: true} → cleanup
```

### Gate Response Paths

| Human reply | response.approved | Devflow action |
|---|---|---|
| `approved` | `true` | Advance to next stage |
| `reject <reason>` | `false`, note=reason | Loop back to Code |
| `review <note>` | `false`, note=discuss | Loop back to Define |

---

## GSD Integration Mechanics

Devflow prompts the coding agent with specific GSD slash commands at each stage.
Commands are predefined per stage, not configurable. Flags vary by context.

### Stage Prompts

| Stage | Agent receives |
|---|---|
| **Define** (first run) | `Run /gsd-discuss-phase {N} to gather requirements.` |
| **Define** (optional pre) | `Run /gsd-spec-phase {N} --auto for ambiguity scoring.` |
| **Define** (restart) | `Restart. Run /gsd-discuss-phase {N} — disregard prior CONTEXT.md, start fresh.` |
| **Plan** | `Run /gsd-plan-phase {N} to create PLAN.md from CONTEXT.md.` |
| **Code** (first run) | `Run /gsd-execute-phase {N} for full wave-based execution.` |
| **Code** (fix mechanical) | `Validation found mechanical issues. Run /gsd-audit-fix.` |
| **Code** (fix structural) | `Validation found structural gaps. Run /gsd-execute-phase {N} --gaps-only.` |
| **Validate** | `Run /gsd-validate-phase {N} for Nyquist coverage audit.` |
| **Ship** | `Run /gsd-ship {N} then /gsd-code-review.` |

---

## Ownership Boundaries

### Devflow Owns

- State machine (Define → Plan → Code → Validate → Ship)
- Agent lifecycle (launch, monitor, 3-layer evaluation)
- Gate file protocol (write gate, poll response, write ack)
- Bootstrap (`new-project`, `map-codebase`, `graphify`, `ingest-docs`)
- Git flow primitives (branch create/merge, release branches)
- Worktrees (parallel agent isolation)
- Hooks (branch management, docs update, changelogs, version bumps)

### Hermes Owns

- Gate delivery (cron polls gates, delivers to human)
- Housekeeping (`progress`, `stats`, `health`, `phase`, `undo`)
- User interface (Telegram, CLI)
- Cross-session context (`thread`, `resume-work`, `pause-work`)

---

## What We Keep From Current Code

| Module | Reason |
|---|---|
| `agents/` (Claude, Codex, OpenCode adapters) | Agent-agnostic launch is core |
| `monitor.rs` | PID polling + auto-advance daemon |
| `agent_result.rs` | 3-layer evaluation (DEVFLOW_RESULT → exit code → heuristic) |
| `git.rs` | Git flow primitives (feature_start, release_start, merge, delete) |
| `worktree.rs` | Parallel agent isolation |
| `config.rs` | Parsing + schema (schema will change) |
| `tracing` instrumentation (Phase 10) | All modules instrumented |
| Gate file protocol (spike) | Proven — formalize into devflow code |

## What We Throw Away

| Item | Reason |
|---|---|
| Current `Step` enum + `next()` chain | Replaced by Define→Plan→Code→Validate→Ship |
| Branching stage | Becomes a hook, not a stage |
| Docsing stage | Becomes a hook (docs-update) |
| Cleaning stage | Becomes a hook (cleanup) |
| Planning-as-pause stage | Wrong design — replaced by GSD discuss-phase |
| `devflow check` command | Replaced by gate protocol + monitor auto-advance |
| All skip logic (`should_skip`, `advance_skipping`) | No step is skippable |
| All `auto_*` toggles (`auto_verify`, `auto_docs`, `auto_cleanup`, `auto_plan`) | No configurability |
| `continue_on_error` | Gates handle this now |
| Blocking agent launch mode | Monitor-only going forward |
| `verify.rs` / `devflow verify` / `devflow lint` / `devflow docs` commands | Handled by GSD validate-phase + hooks |
| `devflow ship` (old) | Replaced by GSD ship + code-review gate |

---

## Resolved Decisions (Phase 11 Planning)

1. **Mode switching** — `--mode auto` or `--mode supervise` CLI flag on `devflow start`.
   No config file, no per-phase toggling. Mode is per-session: if you start with
   `--mode auto`, the Define→Plan→Code→Validate loop runs without human gates until
   Ship (or until 3 consecutive Validate failures, which triggers a gate anyway).
   `--mode supervise` always fires a Validate gate to Hermes → Human before Ship.

2. **Agent prompt format** — Devflow generates a minimal 2-part prompt: (a) the GSD
   slash command as the primary instruction (e.g., `Run /gsd-execute-phase 11 for
   full wave-based execution.`), and (b) the DEVFLOW_RESULT completion protocol marker.
   The old 68-line prompt with CLAUDE.md/ROADMAP.md/CONTEXT.md reading instructions
   is deleted — agents already receive these as `<files_to_read>` from GSD commands.
   Each stage has its own prompt template (see Stage Prompts table above). Agent
   adapters format the prompt but content is GSD-command-driven.

3. **Config schema** — Nothing survives from `.devflow.yaml`. The file is not read.
   Git flow settings (main=main, develop=develop, feature_prefix=feature/) are
   hardcoded constants in `git.rs`. Mode is a CLI flag. Agent preference is a CLI
   flag (`--agent claude|codex|opencode`). Version file is auto-detected (Cargo.toml
   workspace.package.version). `devflow init` and `devflow config` commands are
   removed. The `config.rs` module is simplified to runtime structs only (no YAML
   parsing, no `should_skip`, no `auto_*` fields).

4. **Bootstrap scope** — Phased across 11-12. Phase 11 establishes the execution
   engine (state machine, gates, agent lifecycle, CLI, hooks). Phase 12 adds
   bootstrap commands (`devflow new`, `devflow map`, `devflow graphify`,
   `devflow ingest`). Git flow and versioning hooks are included in Phase 11 as
   part of the Ship stage.

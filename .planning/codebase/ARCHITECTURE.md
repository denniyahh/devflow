# DevFlow — Architecture

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

## Pattern

**Library + Thin CLI.** The architecture follows a standard Rust workspace pattern where all logic lives in `devflow-core` and `devflow-cli` is a thin wrapper around it. No async, no actors, no event loop — purely synchronous, procedural code.

## Crate Structure

```
┌─────────────────────────────────┐
│  devflow-cli (binary crate)     │
│  crates/devflow-cli/src/main.rs │  318 lines
│  • clap CLI parsing             │
│  • start / check / status       │
│  • ship / init / config          │
└────────────┬────────────────────┘
             │ depends on
             ▼
┌─────────────────────────────────┐
│  devflow-core (library crate)   │  1,278 lines total
│  crates/devflow-core/src/        │
│                                  │
│  config.rs     ── YAML parsing  │  294 lines
│  state.rs      ── state machine │  223 lines
│  version.rs    ── semver bump   │  164 lines
│  recover.rs    ── crash recovery│  127 lines
│  git.rs        ── git flow      │  119 lines
│  workflow.rs   ── orchestration │   99 lines
│  monitor.rs    ── bg daemon     │   87 lines
│  lock.rs       ── concurrency   │   83 lines
│  tmux.rs       ── agent launch  │   64 lines
│  lib.rs        ── re-exports    │   18 lines
└─────────────────────────────────┘
```

## State Machine (Central Abstraction)

```
IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING → SHIPPING → CLEANING → IDLE
```

- Defined in `state.rs`: `Step` enum, `State` struct, `Agent` enum
- Persisted to `.devflow/state.json` via `workflow.rs` (`save_state`/`load_state`)
- Advanced by `State::advance()` and `State::advance_skipping()` (respects config)
- Orchestrated by `workflow::advance_state()` which loops through steps

## Data Flow

```
User: devflow start --phase N --agent claude
  │
  ▼
main.rs: parse CLI → Config::load() → State::new()
  │
  ├─ git.rs: feature_start(N)       ── creates feature/phase-NN branch
  ├─ tmux.rs: launch_agent(&state)   ── tmux new-session -d -s <name> <cmd>
  ├─ monitor.rs: spawn_monitor()    ── forks child that polls tmux has-session
  └─ workflow.rs: save_state()      ── writes .devflow/state.json
                                    ── prints "monitor spawned (pid N)"

[Agent runs in tmux session...]

Monitor child (background):
  while tmux has-session; do sleep 30; done
  devflow check <project>    ──×5 calls to advance through all steps
  exit

devflow check:
  1. lock::acquire()          ── creates .devflow/state.lock
  2. workflow::load_state()   ── reads state.json
  3. tmux::agent_running()    ── checks tmux session exists
  4. if not running → state.advance_skipping(config)
  5. workflow::save_state()   ── writes updated state
  6. lock::release()          ── deletes lock file
```

## Key Abstractions

| Module | Responsibility | Key Types |
|---|---|---|
| `state.rs` | State machine, agent definitions | `Step`, `State`, `Agent` |
| `config.rs` | `.devflow.yaml` parsing | `Config`, `AutomationConfig`, `GitFlowConfig` |
| `workflow.rs` | State persistence + orchestration | `save_state()`, `load_state()`, `advance_state()` |
| `git.rs` | Branch lifecycle | `GitFlow` struct |
| `tmux.rs` | Agent session management | `launch_agent()`, `agent_running()`, `capture_output()` |
| `monitor.rs` | Background daemon | `spawn_monitor()` |
| `lock.rs` | Concurrency guard | `acquire()`, `release()`, `AcquireError` |
| `recover.rs` | Crash/stale detection | `recover()`, stale detection (>24h) |
| `version.rs` | Semver operations | `read_version()`, `bump()`, `write_version()` |

## Error Handling

- All modules use `thiserror` derive macros for their error types
- CLI wraps core errors into `CliError` with context
- No panics in library code — all fallible operations return `Result`
- `std::process::Command` failures propagated as typed errors (not raw exit codes)

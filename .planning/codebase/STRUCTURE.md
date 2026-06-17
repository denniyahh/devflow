# DevFlow — Directory Structure

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

```
devflow/
├── .devflow.yaml              # Self-hosted config (dogfooding)
├── Cargo.toml                 # Workspace root (v0.5.0, edition 2024)
├── README.md                  # Quick-start, state machine, config docs
├── ROADMAP.md                 # Version roadmap v0.1.0 → v1.1+
├── AGENTS.md                  # AI agent context (project overview)
├── CONTRIBUTING.md            # Dev setup, standards, PR process
├── LICENSE                    # MIT
│
├── crates/
│   ├── devflow-core/          # Library crate (1,278 lines)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs         # Module re-exports (18 lines)
│   │       ├── state.rs       # State machine + Agent enum (223 lines)
│   │       ├── config.rs      # .devflow.yaml parsing (294 lines)
│   │       ├── git.rs         # Git flow operations (119 lines)
│   │       ├── tmux.rs        # Tmux session management (64 lines)
│   │       ├── monitor.rs     # Background daemon (87 lines)
│   │       ├── lock.rs        # File-based mutex (83 lines)
│   │       ├── recover.rs     # Crash recovery (127 lines)
│   │       ├── version.rs     # Semver bumper (164 lines)
│   │       └── workflow.rs    # State persistence + orchestration (99 lines)
│   │
│   └── devflow-cli/           # Binary crate (318 lines)
│       ├── Cargo.toml
│       └── src/
│           └── main.rs        # CLI entry point, all commands
│
├── .planning/                 # GSD planning (created 2026-06-17)
│   └── codebase/              # Codebase map
│       ├── STACK.md
│       ├── INTEGRATIONS.md
│       ├── ARCHITECTURE.md
│       ├── STRUCTURE.md       # ← this file
│       ├── CONVENTIONS.md
│       ├── TESTING.md
│       └── CONCERNS.md
│
├── target/                    # Build artifacts (gitignored)
│   └── release/
│       └── devflow            # Release binary (~20MB)
│
└── .git/                      # Git repository
```

## Key Locations

| What | Where |
|---|---|
| **Entry point** | `crates/devflow-cli/src/main.rs` |
| **State machine** | `crates/devflow-core/src/state.rs` |
| **Config schema** | `crates/devflow-core/src/config.rs` |
| **Agent launch** | `crates/devflow-core/src/tmux.rs` |
| **Monitor daemon** | `crates/devflow-core/src/monitor.rs` |
| **Project roadmap** | `ROADMAP.md` |
| **Dogfood config** | `.devflow.yaml` |

## Naming Conventions

- **Files**: `snake_case.rs` (Rust standard)
- **Modules**: lowercase, single word (`state`, `config`, `git`)
- **Types**: `PascalCase` (`State`, `GitFlow`, `Config`)
- **Functions**: `snake_case` (`launch_agent`, `feature_start`)
- **Error types**: `ModuleNameError` (`TmuxError`, `MonitorError`, `GitError`)
- **Tmux sessions**: `devflow-{project_dir}-{phase:02}` (e.g., `devflow-trading_bot-08`)
- **Git branches**: `feature/phase-NN`, `release/vX.Y.Z`

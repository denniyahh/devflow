# DevFlow — Conventions

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

## Code Style

| Rule | Detail |
|---|---|
| **Edition** | Rust 2024 |
| **Formatter** | `rustfmt` (default settings) |
| **Linter** | No clippy config present (planned v0.4.0) |
| **Line length** | No explicit limit (Rust default: 100) |

## Naming Patterns

| Category | Convention | Examples |
|---|---|---|
| Modules | `snake_case`, single word | `state`, `config`, `git`, `tmux` |
| Structs | `PascalCase` | `State`, `GitFlow`, `Config` |
| Enums | `PascalCase` | `Step`, `Agent` |
| Functions | `snake_case` | `launch_agent()`, `feature_start()` |
| Error types | `ModuleNameError` | `TmuxError`, `MonitorError`, `GitError`, `CliError` |
| Constants | None used currently | — |

## Code Patterns

### Error Handling
```rust
// Every module defines its own error enum with thiserror
#[derive(Debug, thiserror::Error)]
pub enum TmuxError {
    #[error("failed to execute tmux: {0}")]
    Io(#[from] std::io::Error),
    #[error("tmux command failed: {0}")]
    Command(String),
}
```

### CLI Error Wrapping
```rust
// CLI wraps core errors
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("config: {0}")]
    Config(#[from] devflow_core::config::ConfigError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

### Shell Command Pattern
```rust
// All subprocess calls follow this pattern:
let status = Command::new("tmux")
    .args(["has-session", "-t", session_name])
    .status()?;
Ok(status.success())
```

### Serde Derives
```rust
// Config and state structs use serde for persistence
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub version: VersionConfig,
    pub automation: AutomationConfig,
    // ...
}
```

### Module Re-exports
```rust
// lib.rs only re-exports public modules
pub mod config;
pub mod git;
pub mod state;
// etc.
```

## Documentation

| Level | Style | Coverage |
|---|---|---|
| Module-level | `//!` doc comments | Every `.rs` file |
| Public API | `///` doc comments | Most public functions |
| Inline | `//` comments | Sparse, only for non-obvious logic |
| README | Full quick-start + state machine | `README.md` |
| Roadmap | Versioned checklist | `ROADMAP.md` |

## Project Conventions

| Convention | Detail |
|---|---|
| **Version** | Semver in `Cargo.toml` (`workspace.package.version`) |
| **License** | MIT OR Apache-2.0 |
| **Git flow** | `feature/phase-NN` → `develop` → `main` |
| **Branch naming** | `feature/phase-NN`, `release/vX.Y.Z` |
| **Commit style** | Conventional commits: `feat:`, `fix:`, `chore:` |
| **State dir** | `.devflow/` (gitignored) |
| **Config** | `.devflow.yaml` (git-tracked) |

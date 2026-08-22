# Coding Conventions

**Analysis Date:** 2026-07-17

## Naming Patterns

**Files:**
- Rust source files use `snake_case`: `git.rs`, `agent_result.rs`, `lock.rs`
- Test files in `tests/` directory use `snake_case`: `phase7_cli.rs`, `help_snapshot.rs`, `log_format_env.rs`
- Integration tests colocated in crate's `tests/` subdirectory, not inline `#[cfg(test)]` modules

**Functions:**
- Public functions use `snake_case`: `feature_start()`, `branch_exists()`, `parse_gate_timeout()`
- Internal helpers prefixed to indicate scope: `git_in()`, `git_output()`, `git()` for git wrapper variants
- Constructor pattern: `new()` factory (e.g., `GitFlow::new()`) or `default()` for structs
- Boolean query functions use `is_`/`has_` prefix: `branch_exists()`, `has_remote()`, `agent_running()`

**Variables:**
- Local variables use `snake_case`: `branch`, `phase`, `root`, `state`
- Constants use `SCREAMING_SNAKE_CASE`: `MAIN`, `DEVELOP`, `FEATURE_PREFIX`, `SCHEMA_VERSION`
- Module-level constants extracted to indicate intent: `STATE_FILE_PREFIX`, `SEVEN_DAYS`
- Generic lifetime parameters: `'de` for deserialization, `'a` for borrows

**Types:**
- Struct names use `PascalCase`: `GitFlow`, `BranchInfo`, `State`, `AgentResult`
- Enum variants use `PascalCase`: `AgentStatus::Success`, `Verdict::Pass`, `GitError::Command`
- Error type suffix: `Error` (e.g., `GitError`, `WorkflowError`, `LockError`)
- Custom error enum per module, deriving from `thiserror::Error`

## Code Style

**Formatting:**
- Rust edition 2024
- `rustfmt` (standard, no custom config in repo — uses stable defaults)
- Line length: follows Rust conventions (generally 100 cols for readability)
- Indentation: 4 spaces (enforced by rustfmt)

**Linting:**
- `cargo clippy -- -D warnings` (deny all warnings)
- Configuration: `rust-toolchain.toml` pins `stable` with `clippy` + `rustfmt` components
- No clippy overrides in source code; all warnings must be fixed
- Example: `crates/devflow-core/src/git.rs` uses `.map(|o| o.status.success()).unwrap_or(false)` (functional style), avoiding explicit conditionals

**Public API:**
- All public items (structs, functions, enums, mods) must include doc comments (`///`)
- Module-level documentation (crate root, submodules) via `//!` block comments
- Example from `crates/devflow-core/src/lib.rs`: comprehensive module-level documentation covering logging levels, JSON output, structured events
- Example from `crates/devflow-core/src/lock.rs`: brief purpose statement + conceptual notes in doc comments

## Import Organization

**Order:**
1. Standard library: `use std::{...}` grouped by module
2. External crates: `use serde::{...}`, `use clap::{...}`, `use thiserror`
3. Internal crates: `use devflow_core::{...}`, `use crate::{...}`
4. Re-exports at module end: `pub use` for convenience exports

**Example** from `crates/devflow-cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};
use devflow_core::agent;
use devflow_core::config::{DEVELOP, FEATURE_PREFIX, GitFlowConfig};
use devflow_core::gates::{self, GateAction, GateResponse, Gates};
// ... more internal imports
use std::path::{Path, PathBuf};
use tracing::info;
```

**Path Aliases:**
- No path aliases defined (uses full crate names)
- Crate hierarchy: `devflow` (binary) → `devflow_core` (library)

## Error Handling

**Pattern:**
- Use `thiserror::Error` for all error types: `#[derive(Debug, thiserror::Error)]`
- Define error enum per module (e.g., `GitError`, `WorkflowError`, `LockError`, `ResultError`)
- Result type alias not used; explicit `Result<T, E>` throughout

**Error Variants:**
- Variant per failure mode with context preserved
- Use `#[from]` for automatic conversion: `#[error("...{0}")] Io(#[from] std::io::Error)`
- Custom variants for domain logic: e.g., `GitError::Command(String)` for git command failures

**Example** from `crates/devflow-core/src/git.rs`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("failed to execute git: {0}")]
    Io(#[from] std::io::Error),
    #[error("git command failed: {0}")]
    Command(String),
}
```

**Propagation:**
- Use `?` operator in library code: `fn load_state(...) -> Result<State, WorkflowError> { ... }`
- No `unwrap()` in library code (`devflow-core`); only in tests or CLI initialization
- Result handling in CLI (`devflow` binary) uses explicit matching or `.expect()` with context

**Fallible Operations:**
- Best-effort operations (that must not abort the workflow) use `warn!()` logging and return silently
- Example from `crates/devflow-core/src/events.rs`: event logging fails soft with `warn!()`, never aborts
- Example from `crates/devflow-core/src/git.rs`: branch_exists() returns `false` on command failure (safe default)

## Logging

**Framework:** `tracing` crate (structured, level-aware logging)

**Output:**
- All log output goes to **stderr** (not stdout)
- stdout reserved for agent output, structured results, machine-readable data
- `RUST_LOG` environment variable controls verbosity (default: `info`)
- `DEVFLOW_LOG_FORMAT=json` for machine-readable JSON logs (one object per line)

**Patterns:**
- **State transitions & milestones:** `info!()` — workflow events, stage changes, git operations
- **Detailed I/O & operations:** `debug!()` — file reads/writes, command invocations
- **Recoverable anomalies:** `warn!()` — force operations, fallbacks, degraded conditions
- **Fatal conditions:** `error!()` — abort-level failures

**Examples** from codebase:
```rust
// info! for milestones
info!("creating feature branch: {branch}");
info!("finishing feature branch: {branch}");
info!("saving state: phase={} stage={}", state.phase, state.stage);

// debug! for detail
debug!("rebasing worktree at {} onto {onto}", dir.display());
debug!("loading state from {}", path.display());

// warn! for recoverable issues
warn!("force-creating feature branch: {branch}");
warn!("rebase conflict in {}; aborting", dir.display());

// error! for fatal issues (in CLI only; library uses Results)
```

**Structured Fields:**
- Use named fields in log macros: `info!(phase = phase, "event")`
- Not string interpolation: avoid `info!("phase {phase}: event")`
- Enables parsing by external tools (metrics, aggregation, filtering)

**Controlling Output:**
```bash
RUST_LOG=info devflow start --phase 3              # Default verbosity
RUST_LOG=debug devflow start --phase 3             # Detailed I/O
RUST_LOG=devflow_core=debug devflow status         # Specific crate
DEVFLOW_LOG_FORMAT=json RUST_LOG=info devflow start --phase 3 2>log.json
```

## Comments

**When to Comment:**
- Doc comments (`///`) on all public items — required, enforced
- Line comments (`//`) for non-obvious logic within functions
- Comments should explain WHY, not WHAT (code shows WHAT)
- Reference phase/decision records: e.g., `// WR-04 (13-REVIEW.md)`, `// CR-01 (15-REVIEW.md)`

**JSDoc/TSDoc:**
- Rust uses `///` doc comments with markdown formatting
- Structured as purpose statement + optional examples/notes
- Multi-line doc comments on complex types:

**Example** from `crates/devflow-core/src/agent_result.rs`:
```rust
/// Parsed agent completion result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentResult {
    pub status: AgentStatus,
    /// The Validate stage's self-reported verdict — distinct from `status`.
    /// `status` reports whether the stage's task completed; `verdict` reports
    /// whether validation ITSELF passed.
    #[serde(default, deserialize_with = "deserialize_verdict_lenient")]
    pub verdict: Option<Verdict>,
}
```

## Function Design

**Size:** 
- Max ~40-50 lines per function (subjective, but prefer small)
- Helpers extracted for readability: `git_output()`, `parse_marker_lines()`, `acquire_path()`
- Example: `crates/devflow-core/src/git.rs` splits git operations into single-purpose methods

**Parameters:**
- Prefer concrete types over generic trait bounds (unless polymorphism is needed)
- Use `&Path` not `&str` for filesystem paths
- Use `impl AsRef<Path>` for constructor/factory flexibility: `GitFlow::new(root: impl AsRef<Path>)`
- No default parameters; use builder pattern or `Option<T>` for optional behavior

**Return Values:**
- Fallible operations return `Result<T, E>`, never `Option<T>`
- Pure functions (no I/O, no mutation) return values directly
- Functions that check state return `bool` (e.g., `branch_exists()`, `agent_running()`)
- Option-returning functions indicate missing data, not failure: e.g., `None` for "no active gate"

## Module Design

**Exports:**
- Named exports preferred over glob imports
- Re-exports via `pub use` at module root for convenience
- Example from `crates/devflow-core/src/lib.rs`:
  ```rust
  pub mod agent;
  pub mod config;
  // ... other modules
  pub use mode::Mode;
  pub use stage::Stage;
  pub use state::{AgentKind, State};
  ```

**Barrel Files:**
- `agents/mod.rs` aggregates submodules: `pub mod claude;`, `pub use self::claude::*;`
- No re-export of private types through barrel files
- Single module entry point: `agents::adapter_for(agent_kind)` instead of exposing each adapter

**Module Organization:**
- One module per file: `src/git.rs` = `mod git`
- Submodules (e.g., `agents/`) as subdirectories with `mod.rs`
- All logic in crate root or dedicated modules, never in `lib.rs` after module declarations
- Example: `crates/devflow-core/` has 13 modules, each in its own file

## Serialization

**Format:** JSON (serde + serde_json)

**Patterns:**
- Structs derive `#[derive(Serialize, Deserialize)]`
- Custom serialization via `#[serde(rename_all = "lowercase")]` for case conversion
- Lenient deserialization for forward compatibility: `#[serde(default)]` + custom deserializers
- Example from `crates/devflow-core/src/agent_result.rs`:
  ```rust
  #[serde(default, deserialize_with = "deserialize_verdict_lenient")]
  pub verdict: Option<Verdict>,
  ```
  Handles absent, mis-cased, or unknown verdict values gracefully (falls back to `None` rather than error)

**Atomic Writes:**
- State persisted via sibling temp file + rename (not direct write)
- Example from `crates/devflow-core/src/workflow.rs`: `write_state_atomic()` prevents torn reads

## Process & System Interaction

**Exit Codes:**
- `0` for success, any non-zero for failure
- Specific codes not standardized; all failures exit non-zero

**Signals:**
- Process-existence checks use `libc::kill(pid, 0)` (POSIX standard)
- PID validation: reject 0 (process group signal), reject >i32::MAX (wrap risk)
- Example from `crates/devflow-core/src/agent.rs`: `agent_running()` guards against corrupt PIDs

**Environment Variables:**
- Configuration via env vars (no config files for automation flags)
- Parsing pure functions with env access only in CLI entry point
- Example: `parse_gate_timeout()` is pure; `gate_timeout_secs()` calls `std::env::var()`

---

*Convention analysis: 2026-07-17*

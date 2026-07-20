//! DevFlow — Agent-agnostic development workflow automation.
//!
//! The core library returns structured types and performs workflow mechanics;
//! frontends such as the CLI format output for humans or machines.
//!
//! ## Logging
//!
//! DevFlow uses the [`tracing`] crate for structured, level-aware logging. All
//! log output goes to **stderr** so stdout remains available for agent output,
//! structured results, and machine-readable data.
//!
//! ### Log Levels
//!
//! Set the `RUST_LOG` environment variable to control verbosity:
//!
//! | Level   | Use case |
//! |---------|----------|
//! | `error` | Fatal conditions that abort the workflow |
//! | `warn`  | Unexpected but recoverable conditions (e.g., force operations) |
//! | `info`  | Normal workflow events (state transitions, git operations) |
//! | `debug` | Detailed I/O and git commands |
//! | `trace` | Full tracing subscriber internals |
//!
//! ```bash
//! RUST_LOG=info devflow start --phase 3 --agent claude --mode auto   # Default: shows state transitions
//! RUST_LOG=debug devflow start --phase 3 --agent claude --mode auto  # Also shows git commands and file I/O
//! RUST_LOG=warn devflow status                           # Suppress info, show only warnings
//! ```
//!
//! The default log level is `info` when `RUST_LOG` is not set.
//!
//! ### JSON Output
//!
//! Set `DEVFLOW_LOG_FORMAT=json` for machine-readable JSON log lines on stderr:
//!
//! ```bash
//! DEVFLOW_LOG_FORMAT=json RUST_LOG=info devflow status 2>log.json
//! ```
//!
//! Each line is a JSON object with `timestamp`, `level`, `fields`, and
//! `target` keys. Structured events like `step_entered`/`step_exited` include
//! `phase` and step-name fields.
//!
//! ### Structured Events
//!
//! State machine transitions emit structured `tracing` events:
//!
//! - `step_exited` — emitted when leaving a step, with `phase` and step name
//! - `step_entered` — emitted when entering a step, with `phase` and step name
//!
//! These events are at `INFO` level and include the phase number as a named
//! field, making them filterable and parseable by external tooling.

pub mod agent;
pub mod agent_result;
pub mod agents;
pub mod config;
#[cfg(test)]
mod doc_check;
pub mod events;
pub mod gates;
pub mod git;
pub mod history;
pub mod hooks;
pub mod lock;
pub mod mode;
pub mod monitor;
pub mod outcome_policy;
pub mod prompt;
pub mod recover;
pub mod ship;
pub mod stage;
pub mod state;
pub mod verify;
pub mod version;
pub mod workflow;
pub mod worktree;

// Re-exports for convenience.
pub use mode::Mode;
pub use stage::Stage;
pub use state::{AgentKind, State};

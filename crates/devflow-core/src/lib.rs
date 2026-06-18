//! DevFlow — Agent-agnostic development workflow automation.
//!
//! The core library returns structured types and performs workflow mechanics;
//! frontends such as the CLI format output for humans or machines.

pub mod agent;
pub mod config;
pub mod git;
pub mod lock;
pub mod monitor;
pub mod recover;
pub mod state;
pub mod verify;
pub mod version;
pub mod workflow;

// Re-exports for convenience.
pub use config::Config;
pub use state::{Agent, State, Step};

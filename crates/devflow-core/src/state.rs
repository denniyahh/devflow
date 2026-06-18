//! DevFlow state machine.
//!
//! Drives the development workflow through a deterministic sequence of steps:
//! IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING → SHIPPING → CLEANING → IDLE

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent_result::AgentResult;

/// The current step in the development workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    /// No active workflow — waiting for `devflow start`.
    Idle,
    /// Creating the feature branch via git flow.
    Branching,
    /// Coding agent is running (spawned as child process).
    Executing,
    /// Running verification (tests, lint).
    Verifying,
    /// Updating documentation.
    Docsing,
    /// Creating release branch, bumping version, merging.
    Shipping,
    /// Deleting merged branches.
    Cleaning,
}

impl Step {
    /// Returns the next step in the workflow, or `None` if this is the terminal step.
    pub fn next(self) -> Option<Step> {
        match self {
            Step::Idle => Some(Step::Branching),
            Step::Branching => Some(Step::Executing),
            Step::Executing => Some(Step::Verifying),
            Step::Verifying => Some(Step::Docsing),
            Step::Docsing => Some(Step::Shipping),
            Step::Shipping => Some(Step::Cleaning),
            Step::Cleaning => None,
        }
    }

    /// Whether this step is waiting on an external agent (human or AI).
    pub fn is_waiting(self) -> bool {
        matches!(self, Step::Executing)
    }

    /// Whether this step can be skipped per config.
    pub fn is_skippable(self) -> bool {
        matches!(self, Step::Verifying | Step::Docsing | Step::Shipping)
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Step::Idle => "idle",
            Step::Branching => "branching",
            Step::Executing => "executing",
            Step::Verifying => "verifying",
            Step::Docsing => "docsing",
            Step::Shipping => "shipping",
            Step::Cleaning => "cleaning",
        };
        f.write_str(name)
    }
}

/// Full workflow state persisted to `.devflow/state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// Current workflow step.
    pub step: Step,
    /// Phase number being worked on.
    pub phase: u32,
    /// Which coding agent was launched.
    pub agent: Agent,
    /// PID of the spawned agent process (None if not yet launched).
    pub agent_pid: Option<u32>,
    /// PID of the background monitor (child process watching for agent exit).
    pub monitor_pid: Option<u32>,
    /// Human-readable label for the agent session (for status display).
    pub agent_label: Option<String>,
    /// When the phase started.
    pub started_at: String,
    /// Path to the project root.
    pub project_root: PathBuf,
    /// Parsed agent completion result (from DEVLOW_RESULT or exit code).
    #[serde(skip)]
    pub agent_result: Option<AgentResult>,
    /// Path where agent stdout was saved.
    #[serde(skip)]
    pub agent_stdout_path: Option<PathBuf>,
}

/// Supported coding agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    /// Anthropic Claude Code CLI.
    Claude,
    // Omx,  // OMX support disabled — preserved for potential future re-enable
    /// OpenAI Codex CLI.
    Codex,
    /// OpenCode CLI.
    OpenCode,
}

/// Type alias so the agents module can reference Agent without colliding
/// with its own Agent trait name.
pub type AgentKind = Agent;

impl Agent {
    /// Human-readable name — delegates to the agent trait.
    #[deprecated(
        since = "0.6.0",
        note = "use agents::adapter_for(kind).name() directly"
    )]
    pub fn name(self) -> &'static str {
        crate::agents::adapter_for(self).name()
    }

    /// The command and arguments to launch this agent in non-interactive mode.
    ///
    /// Delegates to the `agents::Agent` trait implementation for each agent kind.
    /// Returns `(program, args)` where the agent runs headless, produces
    /// structured output, and exits when done — never blocks waiting for user input.
    #[deprecated(
        since = "0.6.0",
        note = "use agents::adapter_for(kind).exec_command(phase) directly"
    )]
    pub fn exec_command(self, _project_root: &str, phase: u32) -> (&'static str, Vec<String>) {
        crate::agents::adapter_for(self).exec_command(phase)
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Agent::Claude => "claude",
            // Agent::Omx => "omx",  // OMX disabled
            Agent::Codex => "codex",
            Agent::OpenCode => "opencode",
        };
        f.write_str(name)
    }
}

impl FromStr for Agent {
    type Err = AgentParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "claude" => Ok(Agent::Claude),
            // "omx" | "oh-my-codex" => Ok(Agent::Omx),  // OMX disabled
            "codex" => Ok(Agent::Codex),
            "opencode" | "open-code" => Ok(Agent::OpenCode),
            other => Err(AgentParseError(other.to_string())),
        }
    }
}

/// Error returned when parsing an unsupported agent name.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unsupported agent `{0}`; expected claude, codex, or opencode")]
pub struct AgentParseError(String);

impl State {
    /// Create a new state for starting a phase.
    pub fn new(phase: u32, agent: Agent, project_root: PathBuf) -> Self {
        State {
            step: Step::Idle,
            phase,
            agent,
            agent_pid: None,
            monitor_pid: None,
            agent_label: None,
            started_at: timestamp_now(),
            project_root,
            agent_result: None,
            agent_stdout_path: None,
        }
    }

    /// Advance to the next step. Returns `None` if already at terminal step.
    pub fn advance(&mut self) -> Option<Step> {
        if let Some(next) = self.step.next() {
            self.step = next;
            Some(next)
        } else {
            None
        }
    }

    /// Advance past configured skip steps and return the current step.
    pub fn advance_skipping(&mut self, config: &crate::config::Config) -> Step {
        while config.should_skip(&self.step) {
            if self.advance().is_none() {
                self.step = Step::Idle;
                break;
            }
        }
        self.step
    }
}

fn timestamp_now() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}", duration.as_secs()),
        Err(_) => String::from("0"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::path::PathBuf;

    #[test]
    fn next_walks_full_chain_then_terminates() {
        assert_eq!(Step::Idle.next(), Some(Step::Branching));
        assert_eq!(Step::Branching.next(), Some(Step::Executing));
        assert_eq!(Step::Executing.next(), Some(Step::Verifying));
        assert_eq!(Step::Verifying.next(), Some(Step::Docsing));
        assert_eq!(Step::Docsing.next(), Some(Step::Shipping));
        assert_eq!(Step::Shipping.next(), Some(Step::Cleaning));
        assert_eq!(Step::Cleaning.next(), None);
    }

    #[test]
    fn only_executing_waits_on_an_agent() {
        assert!(Step::Executing.is_waiting());
        for step in [
            Step::Idle,
            Step::Branching,
            Step::Verifying,
            Step::Docsing,
            Step::Shipping,
            Step::Cleaning,
        ] {
            assert!(!step.is_waiting(), "{step} should not wait");
        }
    }

    #[test]
    fn skippable_steps_are_verify_docs_ship() {
        assert!(Step::Verifying.is_skippable());
        assert!(Step::Docsing.is_skippable());
        assert!(Step::Shipping.is_skippable());
        assert!(!Step::Idle.is_skippable());
        assert!(!Step::Branching.is_skippable());
        assert!(!Step::Executing.is_skippable());
        assert!(!Step::Cleaning.is_skippable());
    }

    #[test]
    fn step_display_is_lowercase() {
        assert_eq!(Step::Idle.to_string(), "idle");
        assert_eq!(Step::Branching.to_string(), "branching");
        assert_eq!(Step::Executing.to_string(), "executing");
        assert_eq!(Step::Verifying.to_string(), "verifying");
        assert_eq!(Step::Docsing.to_string(), "docsing");
        assert_eq!(Step::Shipping.to_string(), "shipping");
        assert_eq!(Step::Cleaning.to_string(), "cleaning");
    }

    #[test]
    fn agent_name_and_display() {
        assert_eq!(Agent::Claude.name(), "Claude Code");
        // Agent::Omx disabled
        assert_eq!(Agent::Codex.name(), "OpenAI Codex");
        assert_eq!(Agent::OpenCode.name(), "OpenCode");

        assert_eq!(Agent::Claude.to_string(), "claude");
        // Agent::Omx disabled
        assert_eq!(Agent::Codex.to_string(), "codex");
        assert_eq!(Agent::OpenCode.to_string(), "opencode");
    }

    #[test]
    fn agent_from_str_accepts_canonical_and_aliases() {
        assert_eq!("claude".parse::<Agent>().unwrap(), Agent::Claude);
        assert_eq!("CLAUDE".parse::<Agent>().unwrap(), Agent::Claude);
        // "omx" and "oh-my-codex" disabled — OMX support removed
        assert_eq!("codex".parse::<Agent>().unwrap(), Agent::Codex);
        assert_eq!("opencode".parse::<Agent>().unwrap(), Agent::OpenCode);
        assert_eq!("open-code".parse::<Agent>().unwrap(), Agent::OpenCode);
    }

    #[test]
    fn agent_from_str_rejects_unknown() {
        let err = "aider".parse::<Agent>().unwrap_err();
        assert!(err.to_string().contains("aider"));
    }

    #[test]
    fn exec_command_claude_uses_noninteractive_flags() {
        let (_prog, args) = Agent::Claude.exec_command("/repo", 3);
        let joined = args.join(" ");
        assert!(joined.contains("-p"));
        assert!(joined.contains("--output-format json"));
        assert!(joined.contains("--dangerously-skip-permissions"));
        assert!(joined.contains("phase 3"));
        assert!(joined.contains("ROADMAP.md"));
        assert!(joined.contains("CONTEXT.md"));
        assert!(joined.contains("cargo test"));
    }

    #[test]
    fn exec_command_codex_uses_exec_and_json() {
        let (_prog, args) = Agent::Codex.exec_command("/repo", 7);
        let joined = args.join(" ");
        assert!(joined.contains("exec"));
        assert!(joined.contains("--sandbox workspace-write"));
        assert!(joined.contains("--json"));
        assert!(joined.contains("phase 7"));
    }

    #[test]
    fn new_state_starts_idle_with_no_pid() {
        let state = State::new(2, Agent::Claude, PathBuf::from("/repo"));
        assert_eq!(state.step, Step::Idle);
        assert_eq!(state.phase, 2);
        assert_eq!(state.agent, Agent::Claude);
        assert!(state.agent_pid.is_none());
        assert!(state.monitor_pid.is_none());
        assert!(!state.started_at.is_empty());
    }

    #[test]
    fn advance_walks_chain_then_returns_none_at_terminal() {
        let mut state = State::new(1, Agent::Claude, PathBuf::from("/repo"));
        assert_eq!(state.advance(), Some(Step::Branching));
        state.step = Step::Cleaning;
        assert_eq!(state.advance(), None);
        assert_eq!(state.step, Step::Cleaning);
    }

    #[test]
    fn advance_skipping_jumps_over_disabled_steps() {
        let mut config = Config::default();
        config.automation.auto_verify = false;
        config.automation.auto_docs = false;

        let mut state = State::new(1, Agent::Claude, PathBuf::from("/repo"));
        state.step = Step::Executing;
        state.advance(); // -> Verifying (skipped) -> Docsing (skipped) -> Shipping
        let landed = state.advance_skipping(&config);
        assert_eq!(landed, Step::Shipping);
    }

    #[test]
    fn advance_skipping_returns_to_idle_when_all_remaining_skipped() {
        let mut config = Config::default();
        config.automation.auto_cleanup = false;

        let mut state = State::new(1, Agent::Claude, PathBuf::from("/repo"));
        state.step = Step::Shipping;
        state.advance(); // -> Cleaning
        let landed = state.advance_skipping(&config);
        assert_eq!(landed, Step::Idle);
    }

    #[test]
    fn state_serde_round_trips() {
        let state = State::new(9, Agent::Codex, PathBuf::from("/repo"));
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase, 9);
        assert_eq!(back.agent, Agent::Codex);
        assert_eq!(back.step, Step::Idle);
        assert!(back.agent_pid.is_none());
    }
}

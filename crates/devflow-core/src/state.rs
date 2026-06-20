//! DevFlow state machine.
//!
//! Drives the development workflow through a single linear chain of five stages:
//! Define → Plan → Code → Validate → Ship. See [`crate::stage::Stage`].

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent_result::AgentResult;
use crate::mode::Mode;
use crate::stage::Stage;

/// Full workflow state persisted to `.devflow/state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// Current workflow stage.
    pub stage: Stage,
    /// Phase number being worked on.
    pub phase: u32,
    /// Which coding agent was launched.
    pub agent: Agent,
    /// How the pipeline is driven (auto vs. supervise).
    pub mode: Mode,
    /// Whether a gate has been written and is awaiting a human response.
    #[serde(default)]
    pub gate_pending: bool,
    /// Consecutive Validate failures in this session — drives the Auto-mode
    /// forced gate after [`crate::mode::MAX_CONSECUTIVE_FAILURES`] failures.
    /// Not persisted: always starts at 0 on monitor restart (runtime-only).
    #[serde(skip)]
    pub consecutive_failures: u32,
    /// When the phase started (Unix seconds).
    pub started_at: String,
    /// Path to the project root.
    pub project_root: PathBuf,
    /// Working directory for the agent when running in a git worktree.
    ///
    /// `None` means the agent runs in `project_root`. State and capture files
    /// always live under the main `project_root`; only the agent's cwd changes.
    #[serde(default)]
    pub worktree_path: Option<PathBuf>,
    /// Parsed agent completion result (from DEVFLOW_RESULT or exit code).
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
    /// OpenAI Codex CLI.
    Codex,
    /// OpenCode CLI.
    OpenCode,
}

/// Type alias so the agents module can reference Agent without colliding
/// with its own Agent trait name.
pub type AgentKind = Agent;

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Agent::Claude => "claude",
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
    /// Create a new state for starting a phase at the [`Stage::Define`] stage.
    pub fn new(phase: u32, agent: Agent, mode: Mode, project_root: PathBuf) -> Self {
        State {
            stage: Stage::Define,
            phase,
            agent,
            mode,
            gate_pending: false,
            consecutive_failures: 0,
            started_at: timestamp_now(),
            project_root,
            worktree_path: None,
            agent_result: None,
            agent_stdout_path: None,
        }
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
    use std::path::PathBuf;

    #[test]
    fn agent_name_and_display() {
        use crate::agents::adapter_for;
        assert_eq!(adapter_for(AgentKind::Claude).name(), "Claude Code");
        assert_eq!(adapter_for(AgentKind::Codex).name(), "OpenAI Codex");
        assert_eq!(adapter_for(AgentKind::OpenCode).name(), "OpenCode");

        assert_eq!(Agent::Claude.to_string(), "claude");
        assert_eq!(Agent::Codex.to_string(), "codex");
        assert_eq!(Agent::OpenCode.to_string(), "opencode");
    }

    #[test]
    fn agent_from_str_accepts_canonical_and_aliases() {
        assert_eq!("claude".parse::<Agent>().unwrap(), Agent::Claude);
        assert_eq!("CLAUDE".parse::<Agent>().unwrap(), Agent::Claude);
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
    fn new_state_starts_at_define() {
        let state = State::new(2, Agent::Claude, Mode::Auto, PathBuf::from("/repo"));
        assert_eq!(state.stage, Stage::Define);
        assert_eq!(state.phase, 2);
        assert_eq!(state.agent, Agent::Claude);
        assert_eq!(state.mode, Mode::Auto);
        assert!(!state.gate_pending);
        assert_eq!(state.consecutive_failures, 0);
        assert!(!state.started_at.is_empty());
    }

    #[test]
    fn state_serde_round_trips() {
        let state = State::new(9, Agent::Codex, Mode::Supervise, PathBuf::from("/repo"));
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase, 9);
        assert_eq!(back.agent, Agent::Codex);
        assert_eq!(back.stage, Stage::Define);
        assert_eq!(back.mode, Mode::Supervise);
    }

    #[test]
    fn consecutive_failures_is_runtime_only_not_persisted() {
        let mut state = State::new(1, Agent::Claude, Mode::Auto, PathBuf::from("/repo"));
        state.consecutive_failures = 7;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            !json.contains("consecutive_failures"),
            "runtime-only field must not appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.consecutive_failures, 0,
            "consecutive_failures must reset to 0 on state load"
        );
    }
}

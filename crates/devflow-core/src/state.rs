//! DevFlow state machine.
//!
//! Drives the development workflow through a single linear chain of five stages:
//! Define → Plan → Code → Validate → Ship. See [`crate::stage::Stage`].

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub agent: AgentKind,
    /// How the pipeline is driven (auto vs. supervise).
    pub mode: Mode,
    /// Whether a gate has been written and is awaiting a human response.
    #[serde(default)]
    pub gate_pending: bool,
    /// Consecutive Validate failures — drives the Auto-mode forced gate after
    /// [`crate::mode::MAX_CONSECUTIVE_FAILURES`] failures. Persisted across
    /// `devflow advance` invocations so the counter survives monitor restarts.
    #[serde(default)]
    pub consecutive_failures: u32,
    /// Consecutive infrastructure-class faults (`ResourceKilled`,
    /// `AgentUnavailable`) — distinct from [`Self::consecutive_failures`]
    /// (D-08, 17-01). Gates at [`crate::mode::MAX_INFRA_FAILURES`]. Any
    /// increment (wired in Plan 04) must use `saturating_add` so a
    /// long-running stuck loop cannot overflow `u32`. A serde-absent value
    /// (older persisted state) defaults to 0. Reset to 0 on every successful
    /// stage transition, alongside `consecutive_failures` (CR-01, 17-06 gap
    /// closure), so the ceiling bounds a stuck loop, not a phase's lifetime.
    #[serde(default)]
    pub infra_failures: u32,
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
    /// PID of the detached monitor process that owns the agent for the
    /// current stage, recorded by `launch_stage` at spawn time. `None` means
    /// no monitor has been spawned for this state yet, OR the state was
    /// written by a binary predating this field — in both cases the
    /// liveness probe reports Unknown, never Stuck.
    #[serde(default)]
    pub monitor_pid: Option<u32>,
}

/// Supported coding agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Anthropic Claude Code CLI.
    Claude,
    /// OpenAI Codex CLI.
    Codex,
    /// OpenCode CLI.
    OpenCode,
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::OpenCode => "opencode",
        };
        f.write_str(name)
    }
}

impl FromStr for AgentKind {
    type Err = AgentParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "claude" => Ok(AgentKind::Claude),
            "codex" => Ok(AgentKind::Codex),
            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
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
    pub fn new(phase: u32, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
        State {
            stage: Stage::Define,
            phase,
            agent,
            mode,
            gate_pending: false,
            consecutive_failures: 0,
            infra_failures: 0,
            started_at: timestamp_now(),
            project_root,
            worktree_path: None,
            monitor_pid: None,
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

        assert_eq!(AgentKind::Claude.to_string(), "claude");
        assert_eq!(AgentKind::Codex.to_string(), "codex");
        assert_eq!(AgentKind::OpenCode.to_string(), "opencode");
    }

    #[test]
    fn agent_from_str_accepts_canonical_and_aliases() {
        assert_eq!("claude".parse::<AgentKind>().unwrap(), AgentKind::Claude);
        assert_eq!("CLAUDE".parse::<AgentKind>().unwrap(), AgentKind::Claude);
        assert_eq!("codex".parse::<AgentKind>().unwrap(), AgentKind::Codex);
        assert_eq!(
            "opencode".parse::<AgentKind>().unwrap(),
            AgentKind::OpenCode
        );
        assert_eq!(
            "open-code".parse::<AgentKind>().unwrap(),
            AgentKind::OpenCode
        );
    }

    #[test]
    fn agent_from_str_rejects_unknown() {
        let err = "aider".parse::<AgentKind>().unwrap_err();
        assert!(err.to_string().contains("aider"));
    }

    #[test]
    fn new_state_starts_at_define() {
        let state = State::new(2, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
        assert_eq!(state.stage, Stage::Define);
        assert_eq!(state.phase, 2);
        assert_eq!(state.agent, AgentKind::Claude);
        assert_eq!(state.mode, Mode::Auto);
        assert!(!state.gate_pending);
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.infra_failures, 0);
        assert!(!state.started_at.is_empty());
        assert_eq!(state.monitor_pid, None);
    }

    #[test]
    fn state_serde_round_trips() {
        let state = State::new(9, AgentKind::Codex, Mode::Supervise, PathBuf::from("/repo"));
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase, 9);
        assert_eq!(back.agent, AgentKind::Codex);
        assert_eq!(back.stage, Stage::Define);
        assert_eq!(back.mode, Mode::Supervise);
    }

    #[test]
    fn consecutive_failures_persists_across_advance_calls() {
        let mut state = State::new(1, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
        state.consecutive_failures = 3;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("consecutive_failures"),
            "consecutive_failures must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.consecutive_failures, 3,
            "consecutive_failures must round-trip through serde"
        );
    }

    /// D-08 (17-01): a distinct infra-failure counter round-trips through
    /// serde and its own key appears in the persisted JSON.
    #[test]
    fn infra_failures_round_trips_through_serde() {
        let mut state = State::new(1, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
        state.infra_failures = 4;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("infra_failures"),
            "infra_failures must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.infra_failures, 4,
            "infra_failures must round-trip through serde"
        );
    }

    /// A serde-absent `infra_failures` (older persisted state.json without
    /// the field) must default to 0, not fail to deserialize.
    #[test]
    fn infra_failures_absent_from_json_defaults_to_zero() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.infra_failures, 0);
    }

    /// `monitor_pid` round-trips through serde as an exact `u32` (18b).
    #[test]
    fn monitor_pid_round_trips_through_serde() {
        let mut state = State::new(1, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
        state.monitor_pid = Some(4242);
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("monitor_pid"),
            "monitor_pid must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.monitor_pid,
            Some(4242),
            "monitor_pid must round-trip through serde"
        );
    }

    /// A serde-absent `monitor_pid` (state written by a pre-18b binary) must
    /// deserialize to `None`, not `Some(0)` — a `Some(0)` default would let a
    /// pre-18b state file render as a monitor at pid 0.
    #[test]
    fn monitor_pid_absent_from_json_defaults_to_none() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.monitor_pid, None);
    }
}

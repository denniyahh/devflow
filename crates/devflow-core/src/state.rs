//! DevFlow state machine.
//!
//! Drives the development workflow through a deterministic sequence of steps:
//! IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING → SHIPPING → CLEANING → IDLE

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// The current step in the development workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    /// No active workflow — waiting for `devflow start`.
    Idle,
    /// Creating the feature branch via git flow.
    Branching,
    /// Coding agent is running in tmux.
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
    /// Tmux session name the agent runs in.
    pub tmux_session: Option<String>,
    /// PID of the background monitor (child process waiting for agent exit).
    pub monitor_pid: Option<u32>,
    /// When the phase started.
    pub started_at: String,
    /// Path to the project root.
    pub project_root: PathBuf,
}

/// Supported coding agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    /// Anthropic Claude Code CLI.
    Claude,
    /// oh-my-codex CLI.
    Omx,
    /// OpenAI Codex CLI.
    Codex,
    /// OpenCode CLI.
    OpenCode,
}

impl Agent {
    /// Human-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Omx => "oh-my-codex",
            Agent::Codex => "OpenAI Codex",
            Agent::OpenCode => "OpenCode",
        }
    }

    /// The shell command to launch this agent inside a tmux session.
    pub fn launch_command(self, project_root: &str, phase: u32) -> String {
        let root = shell_quote(project_root);
        match self {
            Agent::Claude => format!("cd {root} && claude --dangerously-skip-permissions"),
            Agent::Omx => format!("cd {root} && omx exec --full-auto --sandbox danger-full-access"),
            Agent::Codex => format!(
                "cd {root} && codex exec --sandbox workspace-write \"Work on phase {phase} of this project. Read AGENTS.md, CLAUDE.md, and the .planning/ directory to understand the current state and what needs to be done.\""
            ),
            Agent::OpenCode => format!("cd {root} && opencode run"),
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Agent::Claude => "claude",
            Agent::Omx => "omx",
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
            "omx" | "oh-my-codex" => Ok(Agent::Omx),
            "codex" => Ok(Agent::Codex),
            "opencode" | "open-code" => Ok(Agent::OpenCode),
            other => Err(AgentParseError(other.to_string())),
        }
    }
}

/// Error returned when parsing an unsupported agent name.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unsupported agent `{0}`; expected claude, omx, codex, or opencode")]
pub struct AgentParseError(String);

impl State {
    /// Create a new state for starting a phase.
    pub fn new(phase: u32, agent: Agent, project_root: PathBuf) -> Self {
        State {
            step: Step::Idle,
            phase,
            agent,
            tmux_session: None,
            monitor_pid: None,
            started_at: timestamp_now(),
            project_root,
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

    /// The tmux session name for this phase.
    pub fn tmux_session_name(&self) -> String {
        let project = self
            .project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '-'
                }
            })
            .collect::<String>();
        format!("devflow-{project}-{:02}", self.phase)
    }
}

fn timestamp_now() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}", duration.as_secs()),
        Err(_) => String::from("0"),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

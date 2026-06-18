//! Agent adapter trait and implementations.
//!
//! Each adapter knows how to build its launch command for non-interactive
//! execution. The trait keeps DevFlow's workflow logic independent from
//! individual agent CLIs.

use crate::state::AgentKind;

/// Common behavior implemented by every supported coding-agent backend.
pub trait Agent {
    /// Human-readable adapter name.
    fn name(&self) -> &'static str;

    /// Build the command and arguments to launch this agent headless.
    /// Returns `(program, args)` — the agent runs, produces output, and exits.
    fn exec_command(&self, phase: u32) -> (&'static str, Vec<String>);

    /// Detect an agent-specific completion signal in captured output.
    fn completion_signal_detected(&self, output: &str) -> bool;
}

/// The shared phase-execution prompt handed to every coding agent.
///
/// One prompt source: Claude and Codex receive identical instruction text and
/// differ only in their CLI flags. The prompt instructs the agent to read the
/// phase context, implement + test + lint + format, commit per sub-task, and
/// emit the `DEVFLOW_RESULT` completion marker that DevFlow parses.
pub fn phase_prompt(phase: u32) -> String {
    format!(
        "Complete phase {phase} of this project.\n\
         \n\
         ## Required Reading\n\
         1. CLAUDE.md — project conventions, architecture, coding standards\n\
         2. .planning/ROADMAP.md — what to build and success criteria for phase {phase}\n\
         3. .planning/phases/{phase:02}-*/CONTEXT.md — phase-specific tasks\n\
         4. AGENTS.md — agent preferences and tooling\n\
         \n\
         ## Process\n\
         - Read the required files first to understand what needs to be built\n\
         - Implement the changes described in the phase plan\n\
         - Run `cargo test` before committing to verify nothing breaks\n\
         - Run `cargo clippy` to catch common mistakes\n\
         - Run `cargo fmt` to format code\n\
         - Commit with descriptive messages explaining what was done\n\
         - If the phase includes multiple sub-tasks, commit each sub-task separately\n\
         - When all tasks from CONTEXT.md are complete, commit a final status update\n\
         \n\
         ## Available Commands\n\
         - `cargo test` — run all tests\n\
         - `cargo clippy -- -D warnings` — lint with strict mode\n\
         - `cargo fmt -- --check` — verify formatting\n\
         - `cargo build --release` — production build\n\
         \n\
         ## Success\n\
         The phase is complete when all checklist items in CONTEXT.md are done\n\
         and all tests pass.\n\
         \n\
         ## Completion Protocol (REQUIRED)\n\
         \n\
         After finishing all work, your FINAL message must be exactly:\n\
         \n\
         DEVFLOW_RESULT: {{\"status\": \"success\"}}\n\
         \n\
         If something prevents completion, your final message must be:\n\
         \n\
         DEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"specific explanation\"}}\n\
         \n\
         DevFlow reads this to determine whether the phase succeeded. Do NOT output anything after this line."
    )
}

/// Return an adapter for a configured agent kind.
pub fn adapter_for(kind: AgentKind) -> Box<dyn Agent> {
    match kind {
        AgentKind::Claude => Box::new(ClaudeAgent),
        AgentKind::Codex => Box::new(CodexAgent),
        AgentKind::OpenCode => Box::new(OpenCodeAgent),
    }
}

pub mod claude;
pub mod codex;
pub mod opencode;

pub use claude::ClaudeAgent;
pub use codex::CodexAgent;
pub use opencode::OpenCodeAgent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_for_returns_correct_names() {
        assert_eq!(adapter_for(AgentKind::Claude).name(), "Claude Code");
        assert_eq!(adapter_for(AgentKind::Codex).name(), "OpenAI Codex");
        assert_eq!(adapter_for(AgentKind::OpenCode).name(), "OpenCode");
    }

    /// Extract the prompt argument — the one carrying the shared instruction
    /// text — from an agent's launch command.
    fn prompt_arg(kind: AgentKind, phase: u32) -> String {
        let (_program, args) = adapter_for(kind).exec_command(phase);
        args.into_iter()
            .find(|arg| arg.starts_with("Complete phase"))
            .expect("agent command should carry the shared phase prompt")
    }

    #[test]
    fn claude_and_codex_share_identical_prompt_text() {
        let claude = prompt_arg(AgentKind::Claude, 7);
        let codex = prompt_arg(AgentKind::Codex, 7);
        assert_eq!(
            claude, codex,
            "Claude and Codex must receive identical prompt text"
        );
        // And it is the shared phase_prompt verbatim.
        assert_eq!(claude, phase_prompt(7));
    }
}

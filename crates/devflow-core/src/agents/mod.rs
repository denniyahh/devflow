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

/// Return an adapter for a configured agent kind.
pub fn adapter_for(kind: AgentKind) -> Box<dyn Agent> {
    match kind {
        AgentKind::Claude => Box::new(ClaudeAgent),
        // AgentKind::Omx => Box::new(OmxAgent),  // OMX disabled
        AgentKind::Codex => Box::new(CodexAgent),
        AgentKind::OpenCode => Box::new(OpenCodeAgent),
    }
}

pub mod claude;
pub mod codex;
pub mod omx;
pub mod opencode;

pub use claude::ClaudeAgent;
pub use codex::CodexAgent;
pub use omx::OmxAgent;
pub use opencode::OpenCodeAgent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_for_returns_correct_names() {
        assert_eq!(adapter_for(AgentKind::Claude).name(), "Claude Code");
        // AgentKind::Omx disabled
        assert_eq!(adapter_for(AgentKind::Codex).name(), "OpenAI Codex");
        assert_eq!(adapter_for(AgentKind::OpenCode).name(), "OpenCode");
    }
}

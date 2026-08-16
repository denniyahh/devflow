//! OpenCode agent adapter.
//!
//! Launches `opencode run "<prompt>"` in non-interactive mode.

use super::{AgentAdapter, AgentDriver};
use crate::phase_id::PhaseId;

/// The modular driver for OpenCode (37-02): positional `opencode run <prompt>`
/// + legacy prompt rendering. `OpenCodeAgent` below delegates to it.
pub struct OpenCodeDriver;

impl super::AgentDriver for OpenCodeDriver {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        ("opencode", vec!["run".into(), prompt.to_string()])
    }
}

pub struct OpenCodeAgent;

impl AgentAdapter for OpenCodeAgent {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn exec_command(
        &self,
        phase: PhaseId,
        prompt: &str,
        extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        OpenCodeDriver.build_command(phase, prompt, extra_writable_roots)
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }
}

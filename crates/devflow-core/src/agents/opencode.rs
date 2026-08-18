//! OpenCode agent driver.
//!
//! Launches `opencode run "<prompt>"` in non-interactive mode.

use crate::phase_id::PhaseId;

/// The modular driver for OpenCode (37-02): positional `opencode run <prompt>`
/// + legacy prompt rendering.
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

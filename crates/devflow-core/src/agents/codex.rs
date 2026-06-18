//! OpenAI Codex agent adapter.
//!
//! Launches `codex exec "<prompt>"` in non-interactive mode with JSON output.

use super::Agent;

pub struct CodexAgent;

impl Agent for CodexAgent {
    fn name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn exec_command(&self, phase: u32) -> (&'static str, Vec<String>) {
        (
            "codex",
            vec![
                "exec".into(),
                "--sandbox".into(),
                "workspace-write".into(),
                "--json".into(),
                super::phase_prompt(phase),
            ],
        )
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

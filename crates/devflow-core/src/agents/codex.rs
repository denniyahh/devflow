//! OpenAI Codex agent adapter.
//!
//! Launches `codex exec "<prompt>"` in non-interactive mode with JSON output.

use super::AgentAdapter;

pub struct CodexAgent;

impl AgentAdapter for CodexAgent {
    fn name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn exec_command(&self, _phase: u32, prompt: &str) -> (&'static str, Vec<String>) {
        (
            "codex",
            vec![
                "exec".into(),
                "--sandbox".into(),
                "workspace-write".into(),
                "--json".into(),
                prompt.to_string(),
            ],
        )
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

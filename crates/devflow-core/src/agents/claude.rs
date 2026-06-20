//! Claude Code agent adapter.
//!
//! Launches `claude -p "<prompt>"` in non-interactive mode with structured
//! JSON output. Claude runs headless — no trust dialogs, no user prompts.

use super::Agent;

pub struct ClaudeAgent;

impl Agent for ClaudeAgent {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn exec_command(&self, _phase: u32, prompt: &str) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                prompt.to_string(),
                "--output-format".into(),
                "json".into(),
                "--dangerously-skip-permissions".into(),
            ],
        )
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        // Claude exits cleanly when done; monitor detects exit via kill -0.
        false
    }
}

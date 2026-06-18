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

    fn exec_command(&self, phase: u32) -> (&'static str, Vec<String>) {
        let prompt = super::phase_prompt(phase);
        (
            "claude",
            vec![
                "-p".into(),
                prompt,
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

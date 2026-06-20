//! OpenCode agent adapter.
//!
//! Launches `opencode run "<prompt>"` in non-interactive mode.

use super::Agent;

pub struct OpenCodeAgent;

impl Agent for OpenCodeAgent {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn exec_command(&self, _phase: u32, prompt: &str) -> (&'static str, Vec<String>) {
        ("opencode", vec!["run".into(), prompt.to_string()])
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

//! OpenCode agent adapter.
//!
//! Launches `opencode run "<prompt>"` in non-interactive mode.

use super::Agent;

pub struct OpenCodeAgent;

impl Agent for OpenCodeAgent {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn exec_command(&self, phase: u32) -> (&'static str, Vec<String>) {
        ("opencode", vec!["run".into(), simple_prompt(phase)])
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

fn simple_prompt(phase: u32) -> String {
    format!(
        "Complete phase {phase}. Read .planning/phases/{phase:02}-*/CONTEXT.md for tasks. \
         Implement, test, lint, format, and commit each sub-task separately.\n\
         \n\
         ## REQUIRED: Output one of these as your last message:\n\
         \n\
         Success → DEVFLOW_RESULT: {{\"status\": \"success\"}}\n\
         Failure → DEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"why\"}}\n\
         \n\
         Nothing after this line. DevFlow uses it to track completion."
    )
}

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
                simple_prompt(phase),
            ],
        )
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

fn simple_prompt(phase: u32) -> String {
    format!(
        "Complete phase {phase}. Read .planning/phases/{phase:02}-*/CONTEXT.md for tasks. \
         Implement, test with `cargo test`, lint with `cargo clippy`, format with `cargo fmt`, \
         and commit with descriptive messages.\n\
         \n\
         ## REQUIRED: Output one of these as your last message:\n\
         \n\
         Success → DEVLOW_RESULT: {{\"status\": \"success\"}}\n\
         Failure → DEVLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"why\"}}\n\
         \n\
         Nothing after this line. DevFlow uses it to track completion."
    )
}

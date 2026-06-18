//! oh-my-codex agent adapter.
//!
//! Launches `omx exec "<prompt>"` in non-interactive mode with JSON output.
//! OMX shares the same API rate limit bucket as Codex.

use super::Agent;

pub struct OmxAgent;

impl Agent for OmxAgent {
    fn name(&self) -> &'static str {
        "oh-my-codex"
    }

    fn exec_command(&self, phase: u32) -> (&'static str, Vec<String>) {
        (
            "omx",
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
         Implement, test, lint, format, and commit each sub-task separately."
    )
}

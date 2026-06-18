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
        let prompt = rich_prompt(phase);
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

fn rich_prompt(phase: u32) -> String {
    format!(
        "Complete phase {phase} of this project.\n\
         \n\
         ## Required Reading\n\
         1. CLAUDE.md — project conventions, architecture, coding standards\n\
         2. .planning/ROADMAP.md — what to build and success criteria for phase {phase}\n\
         3. .planning/phases/{phase:02}-*/CONTEXT.md — phase-specific tasks\n\
         4. AGENTS.md — agent preferences and tooling\n\
         \n\
         ## Process\n\
         - Read the required files first to understand what needs to be built\n\
         - Implement the changes described in the phase plan\n\
         - Run `cargo test` before committing to verify nothing breaks\n\
         - Run `cargo clippy` to catch common mistakes\n\
         - Run `cargo fmt` to format code\n\
         - Commit with descriptive messages explaining what was done\n\
         - If the phase includes multiple sub-tasks, commit each sub-task separately\n\
         - When all tasks from CONTEXT.md are complete, commit a final status update\n\
         \n\
         ## Available Commands\n\
         - `cargo test` — run all tests\n\
         - `cargo clippy -- -D warnings` — lint with strict mode\n\
         - `cargo fmt -- --check` — verify formatting\n\
         - `cargo build --release` — production build\n\
         \n\
         ## Success\n\
         The phase is complete when all checklist items in CONTEXT.md are done\n\
         and all tests pass.\n\
         \n\
         ## Completion Protocol (REQUIRED)\n\
         \n\
         After finishing all work, your FINAL message must be exactly:\n\
         \n\
         DEVLOW_RESULT: {{\"status\": \"success\"}}\n\
         \n\
         If something prevents completion, your final message must be:\n\
         \n\
         DEVLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"specific explanation\"}}\n\
         \n\
         DevFlow reads this to determine whether the phase succeeded. Do NOT output anything after this line."
    )
}

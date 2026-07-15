//! OpenAI Codex agent adapter.
//!
//! Launches `codex exec "<prompt>"` in non-interactive mode with JSON output.

use super::AgentAdapter;
use std::path::Path;

pub struct CodexAgent;

impl AgentAdapter for CodexAgent {
    fn name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn exec_command(
        &self,
        _phase: u32,
        prompt: &str,
        extra_writable_root: Option<&Path>,
    ) -> (&'static str, Vec<String>) {
        let mut args: Vec<String> = vec![
            "exec".into(),
            "--sandbox".into(),
            "workspace-write".into(),
            "--json".into(),
        ];
        // Linked-worktree commits write index.lock/refs under the main
        // repo's `.git/` — outside the workspace-write sandbox — so grant
        // that one directory explicitly (13-06 dogfood finding: Code stage
        // implemented and tested, then could not commit). The value is TOML:
        // escape backslashes and quotes in the path.
        if let Some(root) = extra_writable_root {
            let escaped = root
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            args.push("-c".into());
            args.push(format!(
                "sandbox_workspace_write.writable_roots=[\"{escaped}\"]"
            ));
        }
        args.push(prompt.to_string());
        ("codex", args)
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

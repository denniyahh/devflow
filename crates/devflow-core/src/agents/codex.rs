//! OpenAI Codex agent adapter.
//!
//! Launches `codex exec "<prompt>"` in non-interactive mode with JSON output.

use super::AgentAdapter;
use std::path::PathBuf;

pub struct CodexAgent;

impl AgentAdapter for CodexAgent {
    fn name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn exec_command(
        &self,
        _phase: u32,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        let mut args: Vec<String> = vec![
            "exec".into(),
            "--sandbox".into(),
            "workspace-write".into(),
            "--json".into(),
        ];
        // Linked-worktree commits write git metadata outside the
        // workspace-write sandbox (13-06 dogfood finding: Code stage
        // implemented and tested, then could not commit). Grant every extra
        // root in one TOML list value; escape backslashes and quotes in
        // paths.
        if !extra_writable_roots.is_empty() {
            let list = extra_writable_roots
                .iter()
                .map(|root| {
                    let escaped = root
                        .display()
                        .to_string()
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    format!("\"{escaped}\"")
                })
                .collect::<Vec<_>>()
                .join(",");
            args.push("-c".into());
            args.push(format!("sandbox_workspace_write.writable_roots=[{list}]"));
        }
        args.push(prompt.to_string());
        ("codex", args)
    }

    /// The sandbox has no route to the operator's signing agent, so signed
    /// commits/tags fail headless (`ssh-keygen -Y sign` → passphrase error).
    /// Disable signing via env, scoped to this agent's process tree only.
    fn extra_env(&self) -> Vec<(String, String)> {
        vec![
            ("GIT_CONFIG_COUNT".into(), "2".into()),
            ("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into()),
            ("GIT_CONFIG_VALUE_0".into(), "false".into()),
            ("GIT_CONFIG_KEY_1".into(), "tag.gpgsign".into()),
            ("GIT_CONFIG_VALUE_1".into(), "false".into()),
        ]
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
}

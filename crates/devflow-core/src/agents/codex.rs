//! OpenAI Codex agent adapter.
//!
//! Launches `codex exec "<prompt>"` in non-interactive mode with JSON output.

use super::AgentAdapter;
use crate::phase_id::PhaseId;
use std::path::PathBuf;

pub struct CodexAgent;

impl AgentAdapter for CodexAgent {
    fn name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn exec_command(
        &self,
        _phase: PhaseId,
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

    /// Codex-native stage instruction: reference the GSD workflow file and
    /// follow it directly, rather than receiving a `/gsd-*` slash command
    /// (which Codex renders as a literal shell string — the 999.31 dogfood
    /// defect). No `/gsd-*` token appears anywhere in the output.
    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        use crate::prompt::{COMPLETION_PROTOCOL, FixType, StageIntent};

        let (workflow, args) = match intent {
            StageIntent::Define { phase } => ("discuss-phase.md", format!("phase {phase}")),
            StageIntent::Plan { phase } => ("plan-phase.md", format!("phase {phase}")),
            StageIntent::Code { phase, fix } => {
                let flag = match fix {
                    Some(FixType::GapsOnly) => " --gaps-only",
                    Some(FixType::FullExecute) | None => "",
                    Some(FixType::AuditFix) => "",
                };
                ("execute-phase.md", format!("phase {phase} --auto{flag}"))
            }
            StageIntent::Validate { phase } => ("validate-phase.md", format!("phase {phase}")),
            StageIntent::Ship { phase, .. } => ("ship.md", format!("phase {phase}")),
        };
        format!(
            "You are executing one stage of a headless DevFlow run as the OpenAI Codex coding agent.\n\n\
            Read the GSD workflow file at $HOME/.codex/gsd-core/workflows/{workflow} and follow it for {args}. \
            Do not run GSD slash-command instructions — Codex does not support those; execute the workflow file's instructions directly. \
            The `--auto` flag where present is part of the workflow invocation and must be preserved verbatim.\n\n\
            {COMPLETION_PROTOCOL}"
        )
    }
}

//! OpenAI Codex agent driver + legacy adapter.
//!
//! Launches `codex -a never exec "<prompt>"` in non-interactive mode with JSON
//! output. `-a never` is the GLOBAL approval flag and must precede `exec` —
//! verified against the installed CLI (a `codex exec -a never` placement is
//! rejected as an unknown argument).

use super::{AgentAdapter, AgentDriver, InteractivityMode};
use crate::phase_id::PhaseId;
use std::path::PathBuf;

/// The modular driver for Codex (37-03): owns the launch argv, the JSONL
/// completion parsing, the signing-disable environment, and the Codex-native
/// workflow-reference prompt.
pub struct CodexDriver;

impl AgentDriver for CodexDriver {
    fn name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_workflow_style(intent, &self.workflow_root())
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        let mut args: Vec<String> = vec![
            // `-a never` is the GLOBAL non-interactive approval flag (must
            // precede `exec`); spawn-tested against the installed CLI.
            "-a".into(),
            "never".into(),
            "exec".into(),
            "--sandbox".into(),
            "workspace-write".into(),
            "--json".into(),
        ];
        // Linked-worktree commits write git metadata outside the
        // workspace-write sandbox (13-06 dogfood finding: Code stage
        // implemented and tested, then could not commit). Grant every extra
        // root in one TOML list value; escape backslashes and quotes in paths.
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

    /// Relocate the Codex JSONL completion parsing under driver ownership: the
    /// function body lives in `agent_result.rs` (where the result-evaluation
    /// path and its fixtures live), and this method is the driver's contract
    /// entry point for it.
    fn parse_completion(&self, output: &str) -> Option<crate::agent_result::AgentResult> {
        crate::agent_result::parse_codex_event_result(output)
    }

    fn environment(&self) -> Vec<(String, String)> {
        // The sandbox has no route to the operator's signing agent, so signed
        // commits/tags fail headless (`ssh-keygen -Y sign` → passphrase error).
        // Disable signing via env, scoped to this agent's process tree only.
        vec![
            ("GIT_CONFIG_COUNT".into(), "2".into()),
            ("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into()),
            ("GIT_CONFIG_VALUE_0".into(), "false".into()),
            ("GIT_CONFIG_KEY_1".into(), "tag.gpgsign".into()),
            ("GIT_CONFIG_VALUE_1".into(), "false".into()),
        ]
    }

    fn interactivity_mode(&self, stage: crate::stage::Stage) -> InteractivityMode {
        use crate::stage::Stage;
        match stage {
            // Codex cannot run the interactive discuss-phase interview or the
            // interactive plan-phase decision headless — its Define/Plan stages
            // need the artifact to pre-exist (13-06 dogfood finding).
            Stage::Define | Stage::Plan => InteractivityMode::RequiresExistingArtifact,
            _ => InteractivityMode::HeadlessSafe,
        }
    }
}

/// Legacy `AgentAdapter` face for Codex (D-11 removal point). Delegates to
/// [`CodexDriver`].
pub struct CodexAgent;

impl AgentAdapter for CodexAgent {
    fn name(&self) -> &'static str {
        CodexDriver.name()
    }

    fn exec_command(
        &self,
        phase: PhaseId,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        CodexDriver.build_command(phase, prompt, extra_writable_roots)
    }

    fn extra_env(&self) -> Vec<(String, String)> {
        CodexDriver.environment()
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        CodexDriver.render_prompt(intent)
    }
}

//! Agent adapter trait and implementations.
//!
//! Each adapter knows how to wrap a stage prompt into its CLI's non-interactive
//! launch command. The prompt text itself comes from [`crate::prompt`] — the
//! adapter only formats it into the right flags for its agent.

use crate::state::AgentKind;
use std::path::PathBuf;

/// Common behavior implemented by every supported coding-agent backend.
pub trait AgentAdapter {
    /// Human-readable adapter name.
    fn name(&self) -> &'static str;

    /// Build the command and arguments to launch this agent headless with the
    /// given `prompt` for `phase`. Returns `(program, args)`.
    ///
    /// `extra_writable_roots` are directories OUTSIDE the agent's working
    /// directory that its sandbox must still be allowed to write. Linked git
    /// worktrees keep their git metadata under the main repo's `.git/` — and
    /// Codex additionally read-only-mounts the cwd's resolved git dir, so
    /// BOTH the common `.git` and the worktree admin dir
    /// (`.git/worktrees/<name>`) must be granted explicitly (13-06 dogfood
    /// finding, verified with `codex sandbox` probes). Adapters without a
    /// sandbox ignore it.
    fn exec_command(
        &self,
        phase: u32,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>);

    /// Extra environment variables for the agent process tree. Codex uses
    /// this to disable commit/tag signing inside its sandbox: the operator's
    /// signing agent (ssh-agent/gpg-agent) is unreachable there, so signed
    /// commits fail headless with a passphrase error (13-06 dogfood finding
    /// — same rationale as the unsigned VersionBump tags). `GIT_CONFIG_*`
    /// env scoping keeps the override out of every repo/global config.
    fn extra_env(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Detect an agent-specific completion signal in captured output.
    fn completion_signal_detected(&self, output: &str) -> bool;
}

/// Return an adapter for a configured agent kind.
pub fn adapter_for(kind: AgentKind) -> Box<dyn AgentAdapter> {
    match kind {
        AgentKind::Claude => Box::new(ClaudeAgent),
        AgentKind::Codex => Box::new(CodexAgent),
        AgentKind::OpenCode => Box::new(OpenCodeAgent),
    }
}

pub mod claude;
pub mod codex;
pub mod opencode;

pub use claude::ClaudeAgent;
pub use codex::CodexAgent;
pub use opencode::OpenCodeAgent;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::stage_prompt;
    use crate::stage::Stage;

    #[test]
    fn adapter_for_returns_correct_names() {
        assert_eq!(adapter_for(AgentKind::Claude).name(), "Claude Code");
        assert_eq!(adapter_for(AgentKind::Codex).name(), "OpenAI Codex");
        assert_eq!(adapter_for(AgentKind::OpenCode).name(), "OpenCode");
    }

    /// Extract the prompt argument carrying the instruction text.
    fn prompt_arg(kind: AgentKind, prompt: &str) -> String {
        let (_program, args) = adapter_for(kind).exec_command(7, prompt, &[]);
        args.into_iter()
            .find(|arg| arg.contains("DEVFLOW_RESULT"))
            .expect("agent command should carry the prompt with the DEVFLOW_RESULT contract")
    }

    #[test]
    fn claude_and_codex_share_identical_prompt_text() {
        let prompt = stage_prompt(Stage::Code, 7);
        let claude = prompt_arg(AgentKind::Claude, &prompt);
        let codex = prompt_arg(AgentKind::Codex, &prompt);
        assert_eq!(
            claude, codex,
            "Claude and Codex must receive identical prompt text"
        );
        assert_eq!(claude, prompt);
    }

    #[test]
    fn claude_wraps_prompt_in_noninteractive_flags() {
        let prompt = stage_prompt(Stage::Code, 3);
        let (program, args) = adapter_for(AgentKind::Claude).exec_command(3, &prompt, &[]);
        assert_eq!(program, "claude");
        let joined = args.join(" ");
        assert!(joined.contains("-p"));
        assert!(joined.contains("--output-format json"));
        assert!(joined.contains("--dangerously-skip-permissions"));
    }

    #[test]
    fn codex_wraps_prompt_in_exec_and_json() {
        let prompt = stage_prompt(Stage::Code, 7);
        let (program, args) = adapter_for(AgentKind::Codex).exec_command(7, &prompt, &[]);
        assert_eq!(program, "codex");
        let joined = args.join(" ");
        assert!(joined.contains("exec"));
        assert!(joined.contains("--sandbox workspace-write"));
        assert!(joined.contains("--json"));
    }

    /// 13-06 dogfood regression (Codex leg): linked-worktree git metadata
    /// lives under the main repo's `.git/` — outside the workspace-write
    /// sandbox — and Codex read-only-mounts the cwd's resolved git dir, so
    /// BOTH the common `.git` and the worktree admin dir must be granted
    /// (verified with `codex sandbox` probes). Without roots, no override.
    #[test]
    fn codex_grants_writable_roots_for_worktree_git_metadata() {
        let prompt = stage_prompt(Stage::Code, 7);
        let roots = vec![
            PathBuf::from("/repo/.git"),
            PathBuf::from("/repo/.git/worktrees/phase-07"),
        ];
        let (_, args) = adapter_for(AgentKind::Codex).exec_command(7, &prompt, &roots);
        let joined = args.join(" ");
        assert!(
            joined.contains(
                r#"-c sandbox_workspace_write.writable_roots=["/repo/.git","/repo/.git/worktrees/phase-07"]"#
            ),
            "codex must whitelist the common .git AND the worktree admin dir: {joined}"
        );

        let (_, args) = adapter_for(AgentKind::Codex).exec_command(7, &prompt, &[]);
        assert!(
            !args.join(" ").contains("writable_roots"),
            "no override without an extra root"
        );
    }

    /// 13-06 dogfood regression: signed commits fail inside the Codex
    /// sandbox (no route to the operator's signing agent) — codex scopes an
    /// unsigned-commit override to its own process tree via GIT_CONFIG_*
    /// env; agents without a sandbox get no extra env.
    #[test]
    fn codex_disables_signing_via_env_others_do_not() {
        let env = adapter_for(AgentKind::Codex).extra_env();
        assert!(env.contains(&("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into())));
        assert!(env.contains(&("GIT_CONFIG_KEY_1".into(), "tag.gpgsign".into())));
        assert!(adapter_for(AgentKind::Claude).extra_env().is_empty());
        assert!(adapter_for(AgentKind::OpenCode).extra_env().is_empty());
    }
}

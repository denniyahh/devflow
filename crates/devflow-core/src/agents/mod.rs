//! Agent adapter trait and implementations.
//!
//! Each adapter knows how to wrap a stage prompt into its CLI's non-interactive
//! launch command. The prompt text itself comes from [`crate::prompt`] — the
//! adapter only formats it into the right flags for its agent.

use crate::state::AgentKind;
use std::path::Path;

/// Common behavior implemented by every supported coding-agent backend.
pub trait AgentAdapter {
    /// Human-readable adapter name.
    fn name(&self) -> &'static str;

    /// Build the command and arguments to launch this agent headless with the
    /// given `prompt` for `phase`. Returns `(program, args)`.
    ///
    /// `extra_writable_root` is a directory OUTSIDE the agent's working
    /// directory that its sandbox must still be allowed to write. Linked git
    /// worktrees keep their git metadata (index.lock, HEAD) under the main
    /// repo's `.git/` — outside the worktree — so sandboxed agents (Codex
    /// `--sandbox workspace-write`) cannot commit without it (13-06 dogfood
    /// finding). Adapters without a sandbox ignore it.
    fn exec_command(
        &self,
        phase: u32,
        prompt: &str,
        extra_writable_root: Option<&Path>,
    ) -> (&'static str, Vec<String>);

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
        let (_program, args) = adapter_for(kind).exec_command(7, prompt, None);
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
        let (program, args) = adapter_for(AgentKind::Claude).exec_command(3, &prompt, None);
        assert_eq!(program, "claude");
        let joined = args.join(" ");
        assert!(joined.contains("-p"));
        assert!(joined.contains("--output-format json"));
        assert!(joined.contains("--dangerously-skip-permissions"));
    }

    #[test]
    fn codex_wraps_prompt_in_exec_and_json() {
        let prompt = stage_prompt(Stage::Code, 7);
        let (program, args) = adapter_for(AgentKind::Codex).exec_command(7, &prompt, None);
        assert_eq!(program, "codex");
        let joined = args.join(" ");
        assert!(joined.contains("exec"));
        assert!(joined.contains("--sandbox workspace-write"));
        assert!(joined.contains("--json"));
    }

    /// 13-06 dogfood regression (Codex leg): linked-worktree git metadata
    /// lives under the main repo's `.git/` — outside the workspace-write
    /// sandbox — so the Code stage implemented and tested but could not
    /// commit. With an extra writable root, codex gets a scoped `-c`
    /// override; without one, no override is added.
    #[test]
    fn codex_grants_writable_root_for_worktree_git_metadata() {
        let prompt = stage_prompt(Stage::Code, 7);
        let (_, args) = adapter_for(AgentKind::Codex).exec_command(
            7,
            &prompt,
            Some(std::path::Path::new("/repo/.git")),
        );
        let joined = args.join(" ");
        assert!(
            joined.contains(r#"-c sandbox_workspace_write.writable_roots=["/repo/.git"]"#),
            "codex must whitelist the main repo .git dir: {joined}"
        );

        let (_, args) = adapter_for(AgentKind::Codex).exec_command(7, &prompt, None);
        assert!(
            !args.join(" ").contains("writable_roots"),
            "no override without an extra root"
        );
    }
}

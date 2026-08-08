//! Agent adapter trait and implementations.
//!
//! Each adapter knows how to wrap a stage prompt into its CLI's non-interactive
//! launch command. The prompt text itself comes from [`crate::prompt`] — the
//! adapter only formats it into the right flags for its agent.

use crate::phase_id::PhaseId;
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
        phase: PhaseId,
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

    /// Adapter-specific pre-launch readiness check (D-13/D-14 adapter hook,
    /// Phase 17c). The default is a no-op — most adapters have nothing extra
    /// to check, mirroring [`Self::extra_env`]'s empty-default shape. The
    /// `Err` variant is a human-readable failure reason that flows into the
    /// preflight gate's context (`run_preflight` in `devflow-cli/src/main.rs`).
    /// This is the trait surface Phase 18's Hermes adapter implements to
    /// enforce a non-empty reviewer/receiver set — no built-in adapter
    /// (Claude/Codex/OpenCode) overrides it in Phase 17 because no
    /// reviewer-set storage exists yet in `state.rs`/`config.rs` (review
    /// consensus #6).
    fn preflight(&self, _state: &crate::state::State) -> Result<(), String> {
        Ok(())
    }
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

    /// Extract the prompt text as this adapter actually DELIVERS it.
    ///
    /// Codex and OpenCode pass it positionally, so it is read back out of
    /// argv. Claude does not: under `--input-format stream-json` the initial
    /// user turn travels on the child's stdin, so it is read back out of the
    /// wire document [`crate::monitor::user_turn_line`] builds. Two lookups,
    /// one question — "what text did the agent receive?".
    fn delivered_prompt(kind: AgentKind, prompt: &str) -> String {
        if kind == AgentKind::Claude {
            let turn: serde_json::Value =
                serde_json::from_str(&crate::monitor::user_turn_line(prompt))
                    .expect("the stdin user turn must be one valid JSON document");
            return turn
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(serde_json::Value::as_str)
                .expect("the user turn must carry the prompt as message.content")
                .to_string();
        }
        let (_program, args) = adapter_for(kind).exec_command(PhaseId::new(7), prompt, &[]);
        args.into_iter()
            .find(|arg| arg.contains("DEVFLOW_RESULT"))
            .expect("agent command should carry the prompt with the DEVFLOW_RESULT contract")
    }

    /// The invariant survived a transport change; it was not deleted with the
    /// mechanism that used to carry it. Every adapter still receives the
    /// canonical stage prompt byte-for-byte — Codex and OpenCode in argv,
    /// Claude in the stdin user turn.
    ///
    /// The Claude leg additionally asserts the prompt is ABSENT from argv,
    /// because "identical text" would otherwise be satisfiable by an adapter
    /// that sent the prompt through both routes — which would double the
    /// initial turn.
    #[test]
    fn every_adapter_receives_identical_prompt_text() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        for kind in [AgentKind::Claude, AgentKind::Codex, AgentKind::OpenCode] {
            assert_eq!(
                delivered_prompt(kind, &prompt),
                prompt,
                "{kind} must receive the canonical stage prompt unchanged"
            );
        }

        let (_program, args) =
            adapter_for(AgentKind::Claude).exec_command(PhaseId::new(7), &prompt, &[]);
        assert!(
            !args.iter().any(|arg| arg.contains("DEVFLOW_RESULT")),
            "Claude's prompt must travel on stdin ONLY; a copy left in argv \
             would deliver the initial turn twice: {args:?}"
        );
    }

    /// The Phase 31 launch contract, asserted as one thing because getting
    /// only the flags right is the documented way to half-implement it: the
    /// transport is `stream-json` in BOTH directions, and the prompt is not a
    /// positional argument at all.
    #[test]
    fn claude_launches_headless_stream_json_without_positional_prompt() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(3));
        let (program, args) =
            adapter_for(AgentKind::Claude).exec_command(PhaseId::new(3), &prompt, &[]);
        assert_eq!(program, "claude");
        assert!(args.iter().any(|a| a == "-p"));
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--input-format" && w[1] == "stream-json"),
            "the INPUT format is what moves the initial turn onto stdin; \
             flipping only the output format leaves the CLI with no first \
             turn and it stalls headless: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--output-format" && w[1] == "stream-json"),
            "the OUTPUT format is what makes the capture a JSONL event stream \
             the Layer 1 stream parser can read: {args:?}"
        );
        assert!(args.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert!(
            !args.iter().any(|arg| arg.contains("DEVFLOW_RESULT")),
            "no positional prompt: the initial user turn travels on stdin, \
             written by the monitor: {args:?}"
        );
    }

    #[test]
    fn codex_wraps_prompt_in_exec_and_json() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        let (program, args) =
            adapter_for(AgentKind::Codex).exec_command(PhaseId::new(7), &prompt, &[]);
        assert_eq!(program, "codex");
        let joined = args.join(" ");
        assert!(joined.contains("exec"));
        assert!(joined.contains("--sandbox workspace-write"));
        assert!(joined.contains("--json"));
    }

    #[test]
    fn opencode_wraps_prompt_in_run() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        let (program, args) =
            adapter_for(AgentKind::OpenCode).exec_command(PhaseId::new(7), &prompt, &[]);
        assert_eq!(program, "opencode");
        assert_eq!(args, ["run", prompt.as_str()]);
    }

    /// 13-06 dogfood regression (Codex leg): linked-worktree git metadata
    /// lives under the main repo's `.git/` — outside the workspace-write
    /// sandbox — and Codex read-only-mounts the cwd's resolved git dir, so
    /// BOTH the common `.git` and the worktree admin dir must be granted
    /// (verified with `codex sandbox` probes). Without roots, no override.
    #[test]
    fn codex_grants_writable_roots_for_worktree_git_metadata() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        let roots = vec![
            PathBuf::from("/repo/.git"),
            PathBuf::from("/repo/.git/worktrees/phase-07"),
        ];
        let (_, args) =
            adapter_for(AgentKind::Codex).exec_command(PhaseId::new(7), &prompt, &roots);
        let joined = args.join(" ");
        assert!(
            joined.contains(
                r#"-c sandbox_workspace_write.writable_roots=["/repo/.git","/repo/.git/worktrees/phase-07"]"#
            ),
            "codex must whitelist the common .git AND the worktree admin dir: {joined}"
        );

        let (_, args) = adapter_for(AgentKind::Codex).exec_command(PhaseId::new(7), &prompt, &[]);
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

    /// D-13: `preflight`'s default body is `Ok(())` for every built-in
    /// adapter — none of Claude/Codex/OpenCode override it in Phase 17 (no
    /// reviewer-set storage exists yet in `state.rs`/`config.rs`, review
    /// consensus #6).
    #[test]
    fn default_preflight_is_ok_for_built_in_adapters() {
        let state = crate::state::State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            crate::mode::Mode::Auto,
            PathBuf::from("/repo"),
        );
        assert!(adapter_for(AgentKind::Claude).preflight(&state).is_ok());
        assert!(adapter_for(AgentKind::Codex).preflight(&state).is_ok());
        assert!(adapter_for(AgentKind::OpenCode).preflight(&state).is_ok());
    }
}

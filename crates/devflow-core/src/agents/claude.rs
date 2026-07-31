//! Claude Code agent adapter.
//!
//! Launches `claude -p "<prompt>"` in non-interactive mode with structured
//! JSON output. Claude runs headless — no trust dialogs, no user prompts.

use super::AgentAdapter;

pub struct ClaudeAgent;

impl AgentAdapter for ClaudeAgent {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn exec_command(
        &self,
        _phase: u32,
        prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                prompt.to_string(),
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

impl ClaudeAgent {
    /// Build the resume relaunch command for a confirmed checkpoint
    /// auto-decide (D-03/D-04, 28-03). NOT a trait method — `--resume` is a
    /// Claude-CLI-specific, documented feature with no equivalent on
    /// `AgentAdapter` (D-05: Claude-only, no Codex/OpenCode accommodation,
    /// `AgentAdapter` itself is untouched).
    ///
    /// Argv order (RESEARCH.md § "Architecture Patterns / Pattern 4",
    /// confirmed): the print flag, the instruction, the resume flag
    /// immediately followed by the session id (so the id is parsed as the
    /// flag's value, not a positional argument), the output-format flag with
    /// its JSON value, and the permission-bypass flag.
    ///
    /// **Pitfall 1 (RESEARCH.md, T-28-02) — load-bearing, do not "clean up":**
    /// a `claude --resume`d session restores NEITHER the permission mode NOR
    /// the output format from the original launch. Both are re-passed here
    /// explicitly even though they look redundant with `exec_command`'s
    /// launch above. Omitting either reintroduces the exact headless hang
    /// this phase exists to close: the resumed session halts on a
    /// permission prompt with no operator present to answer it, and the
    /// prompt is not guaranteed to even reach the captured stdout.
    /// `resume_command_includes_permission_bypass` is the named regression
    /// test guarding this specifically — do not delete it as "obviously
    /// redundant" with `claude_wraps_prompt_in_noninteractive_flags` above;
    /// it guards a DIFFERENT command construction path.
    pub fn exec_resume_command(
        _session_id: &str,
        _instruction: &str,
    ) -> (&'static str, Vec<String>) {
        unimplemented!("RED: 28-03 Task 1 — exec_resume_command not yet implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_command_names_claude_program() {
        let (program, _args) = ClaudeAgent::exec_resume_command("sess", "instr");
        assert_eq!(program, "claude");
    }

    #[test]
    fn resume_command_carries_print_flag_and_instruction() {
        let (_program, args) = ClaudeAgent::exec_resume_command("sess", "do the thing");
        assert!(args.iter().any(|a| a == "-p"));
        assert!(args.iter().any(|a| a == "do the thing"));
    }

    #[test]
    fn resume_command_resume_flag_immediately_precedes_session_id() {
        let (_program, args) = ClaudeAgent::exec_resume_command("sess-abc", "instr");
        let resume_idx = args
            .iter()
            .position(|a| a == "--resume")
            .expect("--resume flag must be present");
        assert_eq!(
            args.get(resume_idx + 1).map(String::as_str),
            Some("sess-abc"),
            "the session id must immediately follow --resume so it is parsed \
             as the flag's value, not a positional argument: {args:?}"
        );
    }

    /// Pitfall 1 (RESEARCH.md, T-28-02): the single highest-consequence
    /// regression this phase can ship is a resume relaunch that omits either
    /// the permission-bypass flag or the JSON output-format flag — a resumed
    /// Claude session restores neither, so omitting them reintroduces a
    /// silent headless hang on a permission prompt nobody can answer.
    #[test]
    fn resume_command_includes_permission_bypass() {
        let (program, args) = ClaudeAgent::exec_resume_command("sess-123", "do the thing");
        assert_eq!(program, "claude");
        assert!(
            args.iter().any(|a| a == "--dangerously-skip-permissions"),
            "a resumed Claude session restores neither the permission mode \
             nor the output format (RESEARCH Pitfall 1) — omitting this flag \
             reintroduces a silent headless hang with nobody able to answer \
             the resulting permission prompt: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--output-format" && w[1] == "json"),
            "the JSON output-format flag must also be re-passed explicitly, \
             for the same reason as the permission-bypass flag: {args:?}"
        );
    }
}

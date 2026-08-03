//! Claude Code agent adapter.
//!
//! Launches `claude -p` headless with a bidirectional `stream-json` transport:
//! the initial user turn travels on the child's **stdin**, and its events come
//! back on stdout one JSON object per line. Claude runs headless — no trust
//! dialogs, no user prompts.

use super::AgentAdapter;

pub struct ClaudeAgent;

impl AgentAdapter for ClaudeAgent {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    /// Build the headless `stream-json` launch (Phase 31, constraint 1).
    ///
    /// **The prompt is deliberately absent from the returned argv.** Under
    /// `--input-format stream-json` the CLI takes its initial user turn from
    /// stdin as a JSON document, not from a positional argument; the monitor
    /// writes that turn via [`crate::monitor::user_turn_line`]. The `prompt`
    /// parameter is kept in the signature because [`AgentAdapter`] is shared
    /// with adapters that DO pass it positionally (Codex, OpenCode) — it is
    /// unused here on purpose, not by oversight.
    ///
    /// Evidence: all three archived Phase 30 harnesses
    /// (`.planning/phases/30-keep-the-session-alive-past-turn-end/`,
    /// `30b`/`30c`/`30d`) launch with exactly this flag set and no positional
    /// prompt, then write
    /// `{"type":"user","message":{"role":"user","content":<prompt>}}` to the
    /// child's stdin. `30c-monitor-env-harness.py`'s `DEFAULT_CLI_ARGV` is the
    /// literal argv reproduced here.
    ///
    /// `--verbose` is load-bearing, not decoration: every archived trial that
    /// produced a usable capture carried it, and dropping it is untested
    /// territory. Do not "clean it up".
    ///
    /// The switch is unconditional and stage-blind — constraint 1 forbids
    /// predicting at launch time which stages will background work. The
    /// *sequencing* choice about which stages route here lives at the call
    /// site (`claude_stream_launch_enabled` in `pipeline_launch.rs`); the
    /// shape a not-yet-widened stage gets instead is
    /// [`ClaudeAgent::exec_command_single_document`], which is a live path
    /// rather than a deprecated one.
    fn exec_command(
        &self,
        _phase: u32,
        _prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                "--input-format".into(),
                "stream-json".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--verbose".into(),
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
    /// The pre-31 single-document launch: `-p <prompt>` positionally with
    /// `--output-format json`.
    ///
    /// **This is a live path, not a deprecated leftover.** Two things select
    /// it, and both are deliberate:
    ///
    /// - **D-09/D-10's sequencing gate.** The stream-json launch is rolled out
    ///   one stage at a time, starting at `Stage::Code`. Every stage not yet
    ///   widened launches through here. That is a sequencing choice about
    ///   rollout order, which constraint 1 permits — it is emphatically not a
    ///   prediction about which stages background work, which constraint 1
    ///   forbids.
    /// - **D-11's opt-out.** An explicit flag (off by default) can force this
    ///   shape back on for recovery without cutting a release. Automatic
    ///   fallback on parse failure is rejected: a silent downgrade is the same
    ///   invisible-degradation class as the bug Phase 31 exists to fix.
    ///
    /// The argv is the pre-31 [`AgentAdapter::exec_command`] body verbatim, so
    /// the shipped capture shape (`CaptureKind::SingleDocEnvelope`) and the
    /// 30b isolation tests that guard it (D-12) keep holding bit-for-bit.
    pub fn exec_command_single_document(prompt: &str) -> (&'static str, Vec<String>) {
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
    pub fn exec_resume_command(session_id: &str, instruction: &str) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                instruction.to_string(),
                "--resume".into(),
                session_id.to_string(),
                "--output-format".into(),
                "json".into(),
                "--dangerously-skip-permissions".into(),
            ],
        )
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

//! Claude Code agent driver.
//!
//! Launches `claude -p` headless with a bidirectional `stream-json` transport:
//! the initial user turn travels on the child's **stdin**, and its events come
//! back on stdout one JSON object per line. Claude runs headless — no trust
//! dialogs, no user prompts.

use super::{AgentDriver, ArgPlacement, AskRegistration, AskWiring};
use crate::phase_id::PhaseId;

/// The modular driver for Claude (37-02): owns the `stream-json` launch,
/// legacy prompt rendering, and the pre-31 single-document builder.
pub struct ClaudeDriver;

impl AgentDriver for ClaudeDriver {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    /// Build the headless `stream-json` launch (Phase 31, constraint 1).
    ///
    /// **The prompt is deliberately absent from the returned argv.** Under
    /// `--input-format stream-json` the CLI takes its initial user turn from
    /// stdin as a JSON document, not from a positional argument; the monitor
    /// writes that turn via `crate::monitor::user_turn_line`. The `prompt`
    /// parameter is kept in the signature for the shared `AgentDriver` shape —
    /// it is unused here on purpose, not by oversight.
    ///
    /// `--verbose` is load-bearing, not decoration: every archived Phase-30
    /// trial that produced a usable capture carried it, and dropping it is
    /// untested territory. Do not "clean it up".
    ///
    /// The switch is unconditional and stage-blind — which stages route here is
    /// a rollout-order choice made at the call site
    /// (`claude_stream_launch_enabled`), not a prediction this builder makes.
    fn build_command(
        &self,
        _phase: PhaseId,
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

    fn ask_support(&self) -> Result<(), String> {
        Ok(())
    }

    /// D-03: one stdio server `devflow` in a per-launch `--mcp-config` file,
    /// appended last. No `--strict-mcp-config`: the operator's own servers stay.
    /// The per-server `timeout` (ms) is the gate timeout plus the margin.
    fn ask_wiring(&self, reg: &AskRegistration) -> Result<AskWiring, String> {
        let (exe, _root) = reg.utf8_paths()?;
        let config_path = reg.config_path.to_str().ok_or_else(|| {
            format!(
                "the registration path {} is not UTF-8",
                reg.config_path.display()
            )
        })?;
        let timeout_ms = reg.client_timeout_secs().saturating_mul(1000);
        let config = serde_json::json!({
            "mcpServers": {
                "devflow": {
                    "type": "stdio",
                    "command": exe,
                    "args": reg.server_args(),
                    "timeout": timeout_ms,
                }
            }
        });
        let text = serde_json::to_string(&config).map_err(|err| err.to_string())?;
        Ok(AskWiring {
            args: vec!["--mcp-config".into(), config_path.to_string()],
            placement: ArgPlacement::Append,
            file: Some((reg.config_path.clone(), text)),
        })
    }
}

impl ClaudeDriver {
    /// The pre-31 single-document launch: `-p <prompt>` positionally with
    /// `--output-format json`.
    ///
    /// **Kept, not a deprecated leftover.** One thing selects it, deliberately:
    ///
    /// - **D-09/D-10's sequencing gate.** The stream-json launch is rolled out
    ///   one stage at a time, starting at `Stage::Code`. Every stage not yet
    ///   widened launches through here. That is a sequencing choice about
    ///   rollout order, which constraint 1 permits — it is emphatically not a
    ///   prediction about which stages background work, which constraint 1
    ///   forbids.
    ///
    /// D-11's opt-out (`--legacy-claude-launch`) was removed in Phase 52, so
    /// with every stage widened nothing reaches this builder through
    /// `launch_stage` today. Automatic fallback on parse failure stays
    /// rejected: a silent downgrade is the same invisible-degradation class as
    /// the bug Phase 31 exists to fix.
    ///
    /// The argv is the pre-31 single-document launch body verbatim, so
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the stage prompt's one invariant substring. Every real
    /// stage prompt carries the `DEVFLOW_RESULT` contract, so its presence in
    /// an argument is what identifies that argument as the prompt.
    const PROMPT: &str = "do the work, then emit DEVFLOW_RESULT: {...}";

    /// Both directions, or neither works. Flipping only `--output-format`
    /// leaves the CLI with no first turn (the prompt has left argv but nothing
    /// is writing it to stdin) and it stalls headless — the failure RESEARCH
    /// Pitfall 1 names, whose warning sign is an `init` event followed by the
    /// agent asking what to do.
    #[test]
    fn exec_command_uses_stream_json_on_both_input_and_output() {
        let (program, args) = ClaudeDriver.build_command(PhaseId::new(7), PROMPT, &[]);
        assert_eq!(program, "claude");
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--input-format" && w[1] == "stream-json"),
            "the input format is what moves the initial turn onto stdin: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--output-format" && w[1] == "stream-json"),
            "the output format is what makes the capture a JSONL event stream: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--verbose"),
            "every archived Phase 30 trial that produced a usable capture \
             carried --verbose; dropping it is untested territory: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--dangerously-skip-permissions"),
            "a headless launch with no operator present cannot answer a \
             permission prompt: {args:?}"
        );
    }

    /// The half of the change that is easy to miss. RESEARCH Pitfall 1: the
    /// ROADMAP and CONTEXT both describe Phase 31 as "the argv flip", which
    /// reads as flags-only — but leaving the prompt at `args[1]` under
    /// `--input-format stream-json` is not documented to work, and was never
    /// tested in Phase 30.
    #[test]
    fn exec_command_carries_no_positional_prompt() {
        let (_program, args) = ClaudeDriver.build_command(PhaseId::new(7), PROMPT, &[]);
        assert!(
            !args.iter().any(|arg| arg.contains("DEVFLOW_RESULT")),
            "the prompt must not appear in argv at all — it travels as a JSON \
             user turn on the child's stdin, written by the monitor: {args:?}"
        );
    }

    /// The pre-31 shape must stay REACHABLE, not merely present. The D-09/D-10
    /// sequencing gate (every stage the rollout has not reached yet) depends on
    /// it. If this builder silently drifted toward the stream-json shape, that
    /// gate would land on a launch that is not pre-31 at all, and the D-12
    /// isolation guarantee for the shipped single-document capture would go
    /// with it.
    #[test]
    fn single_document_command_preserves_pre31_shape() {
        let (program, args) = ClaudeDriver::exec_command_single_document(PROMPT);
        assert_eq!(program, "claude");
        assert!(
            args.windows(2).any(|w| w[0] == "-p" && w[1] == PROMPT),
            "the prompt must follow -p POSITIONALLY, as it did pre-31: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--output-format" && w[1] == "json"),
            "the single-document envelope is what makes this capture classify \
             as SingleDocEnvelope and keep the raw-scan path: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--dangerously-skip-permissions"),
            "the single-document path is still headless: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a == "--input-format"),
            "this builder must NOT drift toward the stream-json shape — that \
             would leave the sequencing gate with no pre-31 shape: {args:?}"
        );
    }

    fn ask_registration() -> crate::agents::AskRegistration {
        crate::agents::AskRegistration {
            exe: std::path::PathBuf::from("/opt/bin/devflow"),
            project_root: std::path::PathBuf::from("/work/project"),
            phase: PhaseId::new(7),
            stage: crate::stage::Stage::Code,
            timeout_secs: 120,
            run_started_at: "2026-11-18T00:00:00Z".into(),
            config_path: std::path::PathBuf::from("/work/project/.devflow/ask-07.mcp.json"),
        }
    }

    /// D-03: claude reads the server from a per-launch `--mcp-config` file
    /// appended last, whose one server `devflow` runs the runner's own binary
    /// as `__ask` and lets a call wait the gate timeout plus the margin.
    #[test]
    fn launch_args_name_the_ask_registration() {
        let reg = ask_registration();
        let wiring = ClaudeDriver
            .ask_wiring(&reg)
            .expect("claude registers the server");
        assert_eq!(
            wiring.args,
            ["--mcp-config", "/work/project/.devflow/ask-07.mcp.json"]
        );
        assert_eq!(wiring.placement, crate::agents::ArgPlacement::Append);
        let (path, text) = wiring.file.expect("claude writes a registration file");
        assert_eq!(path, reg.config_path);
        assert!(!text.contains('\n'), "one line: {text}");
        let config: serde_json::Value = serde_json::from_str(&text).expect("the file is JSON");
        let servers = config["mcpServers"].as_object().expect("mcpServers");
        assert_eq!(servers.len(), 1, "one server: {text}");
        let server = &servers["devflow"];
        assert_eq!(server["type"], "stdio", "{text}");
        assert_eq!(server["command"], "/opt/bin/devflow", "{text}");
        assert_eq!(
            server["args"],
            serde_json::json!([
                "__ask",
                "--project",
                "/work/project",
                "--phase",
                "7",
                "--stage",
                "code",
                "--timeout-secs",
                "120",
                "--run-started-at",
                "2026-11-18T00:00:00Z"
            ]),
            "{text}"
        );
        assert_eq!(server["timeout"], (120 + 60) * 1000, "timeout ms: {text}");
    }
}

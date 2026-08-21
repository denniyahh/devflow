//! Antigravity CLI agent driver (phase 41).
//!
//! Launches the operator's `agy` wrapper headless with a bidirectional
//! `stream-json` transport: the initial user turn travels on the child's
//! **stdin** as an `event`-key JSON line (`{"event":"user","message":{...}}`),
//! and its events come back on stdout one JSON object per line under an
//! `event` key (`init` -> `step_update` -> `result`).
//!
//! Every argv/schema fact here is deliberately review-derived and cited, not
//! assumed (41-CONTEXT round-3, `.planning/phases/41-antigravity-driver/`):
//!
//! - **D-01:** `agy` is a shell wrapper (`exec antigravity-cli
//!   --dangerously-skip-permissions "$@"`); the wrapper injects the
//!   skip-permissions flag itself, so the driver argv must NOT repeat it.
//! - **D-02:** no `-p`. `-p` is a Go-flag STRING flag requiring an argument;
//!   it swallows the next token and exits 0 silently, and it is mutually
//!   exclusive with `--input-format stream-json`. The prompt travels on stdin,
//!   never in argv.
//! - **F3:** `--print-timeout 60m` — the CLI default is 5m, below the
//!   documented DevFlow stage length (a healthy stage measured at 47m); every
//!   prior invocation in this repo overrides it. `60m` is the decided floor.
//! - **D-03:** the completion parser is the Antigravity stream parser
//!   (`agent_result::parse_antigravity_event_result`), reading the marker from
//!   the last `event: "result"` object's `result.response` STRING; the ERROR
//!   envelope is Layer-1 decisive.

use super::AgentDriver;
use crate::phase_id::PhaseId;

/// The modular driver for Antigravity (`agy`): the stream-json launch with the
/// reviewed argv, prompt delegation to the Claude-style renderer (D-05), and
/// the Antigravity completion parser.
pub struct AntigravityDriver;

impl AgentDriver for AntigravityDriver {
    fn name(&self) -> &'static str {
        "Antigravity"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    /// Build the headless `stream-json` launch (D-02/F3, round 3).
    ///
    /// **Exact argv — do not "improve" it without re-deriving against a live
    /// CLI.** No `-p` (Go-flag string flag, D-02), no
    /// `--dangerously-skip-permissions` (the `agy` wrapper injects it, D-01),
    /// no prompt in argv (stdin is the transport, D-02), and
    /// `--print-timeout 60m` explicitly above the 5m default (F3). The
    /// `prompt`/`phase` parameters are kept for the shared `AgentDriver`
    /// shape; they are unused here on purpose.
    fn build_command(
        &self,
        _phase: PhaseId,
        _prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "agy",
            vec![
                "--input-format".into(),
                "stream-json".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--print-timeout".into(),
                "60m".into(),
            ],
        )
    }

    /// Delegate completion parsing to the Antigravity stream parser
    /// (D-03/round-3): marker from `result.response`, ERROR envelope decisive,
    /// marker-less -> `None` (Layer 2 owns it).
    fn parse_completion(&self, output: &str) -> Option<crate::agent_result::AgentResult> {
        crate::agent_result::parse_antigravity_event_result(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_result::AgentStatus;

    #[test]
    fn antigravity_driver_build_command_is_exact() {
        let (program, args) = AntigravityDriver.build_command(PhaseId::new(0), "x", &[]);
        assert_eq!(program, "agy");
        assert_eq!(
            args,
            vec![
                "--input-format".to_string(),
                "stream-json".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--print-timeout".to_string(),
                "60m".to_string(),
            ],
            "exact argv: no -p, no --dangerously-skip-permissions, no prompt, \
             --print-timeout above the 5m default (D-01/D-02/F3)"
        );
    }

    #[test]
    fn antigravity_driver_render_prompt_delegates_to_claude_style() {
        let intent =
            crate::prompt::StageIntent::for_stage(crate::stage::Stage::Code, PhaseId::new(7));
        let rendered = AntigravityDriver.render_prompt(&intent);
        assert_eq!(
            rendered,
            crate::prompt::render_claude_style(&intent),
            "D-05"
        );
        assert!(
            rendered.contains("DEVFLOW_RESULT"),
            "contract: prompt must carry the marker instruction"
        );
    }

    #[test]
    fn antigravity_driver_parse_completion_delegates() {
        // Live shape: marker inside result.response -> Success.
        let capture = concat!(
            "{\"event\":\"init\",\"model\":\"stub\"}\n",
            "{\"event\":\"result\",\"result\":{\"status\":\"SUCCESS\",\"response\":\"DEVFLOW_RESULT: {\\\"status\\\":\\\"success\\\"}\"}}\n",
        );
        let got = AntigravityDriver
            .parse_completion(capture)
            .expect("marker must resolve");
        assert_eq!(got.status, AgentStatus::Success);

        // ERROR envelope -> decisive Failed with the CLI's reason (notice (c)).
        let error = concat!(
            "{\"event\":\"init\",\"model\":\"stub\"}\n",
            "{\"event\":\"result\",\"result\":{\"status\":\"ERROR\",\"response\":\"\",\"error\":\"stream input message is missing the \\\"event\\\" field\"}}\n",
        );
        let got = AntigravityDriver
            .parse_completion(error)
            .expect("ERROR envelope must resolve");
        assert_eq!(got.status, AgentStatus::Failed);
        assert!(
            got.reason
                .as_deref()
                .unwrap()
                .contains("missing the \"event\" field")
        );

        // Marker-less -> None (Layer 2 owns it).
        let marker_less = concat!(
            "{\"event\":\"init\",\"model\":\"stub\"}\n",
            "{\"event\":\"result\",\"result\":{\"status\":\"SUCCESS\",\"response\":\"all done\"}}\n",
        );
        assert!(AntigravityDriver.parse_completion(marker_less).is_none());
    }

    /// F7 — "argv spawn-tested, not assumed": run the driver's ACTUAL argv
    /// against a real child process with a stub `agy` on PATH. The stub
    /// records the argv it received and the stdin turn, then emits an
    /// antigravity-shaped stream; the test asserts both the argv round-trip
    /// AND that the emitted stream parses back through the driver.
    #[test]
    fn antigravity_driver_spawn_argv_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        let argv_file = dir.path().join("argv.txt");
        let turn_file = dir.path().join("turn.txt");
        std::fs::create_dir_all(&bin_dir).unwrap();

        let stub = format!(
            r#"#!/bin/sh
printf '%s\n' "$@" > '{}'
IFS= read -r turn
printf '%s\n' "$turn" > '{}'
printf '%s\n' '{{"event":"init","model":"stub","inputFormat":"stream-json","outputFormat":"stream-json"}}'
printf '%s\n' '{{"event":"result","result":{{"status":"SUCCESS","response":"DEVFLOW_RESULT: {{\"status\":\"success\"}}"}}}}'
"#,
            argv_file.display(),
            turn_file.display(),
        );
        std::fs::write(bin_dir.join("agy"), stub).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin_dir.join("agy"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }

        let (program, args) = AntigravityDriver.build_command(PhaseId::new(0), "x", &[]);
        let out = std::process::Command::new(program)
            .args(&args)
            .env("PATH", &bin_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(
                    b"{\"event\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"x\"}}\n",
                )?;
                child.wait_with_output()
            })
            .expect("stub agy must spawn");

        assert!(out.status.success(), "stub exited {:?}", out.status.code());

        // The stub received exactly the five reviewed tokens — not -p, not the
        // skip-permissions flag, not the prompt.
        let received: Vec<String> = std::fs::read_to_string(&argv_file)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(
            received, args,
            "spawned argv must equal build_command's argv: {received:?}"
        );

        // The stream round-trips through the driver's parser.
        let parsed = AntigravityDriver
            .parse_completion(&String::from_utf8_lossy(&out.stdout))
            .expect("stub stream must parse");
        assert_eq!(parsed.status, AgentStatus::Success);

        // The stdin turn arrived as an event-key line (schema probe at the
        // process boundary — the CLI would reject a type-key turn).
        let turn = std::fs::read_to_string(&turn_file).unwrap();
        assert!(
            turn.contains("\"event\":\"user\""),
            "first turn must be event-key: {turn}"
        );
        assert!(!turn.contains("\"type\":"), "no type key allowed: {turn}");
    }

    #[test]
    fn antigravity_driver_name_is_correct() {
        assert_eq!(AntigravityDriver.name(), "Antigravity");
    }
}

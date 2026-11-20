//! OpenAI Codex agent driver.
//!
//! Launches `codex -a never exec "<prompt>"` in non-interactive mode with JSON
//! output. `-a never` is the GLOBAL approval flag and must precede `exec` —
//! verified against the installed CLI (a `codex exec -a never` placement is
//! rejected as an unknown argument).

use super::{
    ASK_CLIENT_TIMEOUT_MARGIN_SECS, ASK_HOLD_CAP_SECS, AgentDriver, ArgPlacement, AskRegistration,
    AskWiring, InteractivityMode,
};
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
        // root in one TOML list value; escape backslashes, quotes, and control
        // characters so a hostile path cannot corrupt the array (999.107 #2).
        if !extra_writable_roots.is_empty() {
            let list = extra_writable_roots
                .iter()
                .map(|root| {
                    // Refuse (panic) on a non-UTF-8 path: TOML basic strings are
                    // UTF-8-only, so a non-UTF-8 root cannot be serialized
                    // losslessly. A lossy U+FFFD would name a different,
                    // nonexistent path and silently drop the sandbox write grant
                    // (999.107 #2 review).
                    let path = root
                        .to_str()
                        .expect("non-UTF-8 writable root path — refusing Codex launch");
                    format!("\"{}\"", escape_toml_basic_string(path))
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

    fn ask_support(&self) -> Result<(), String> {
        Ok(())
    }

    /// D-03: five `-c mcp_servers.devflow.*` overrides before the prompt. The
    /// operator's own servers stay on (as for claude) and the tool is approved
    /// without a prompt. OD-1 (b): the server holds one call at most
    /// [`ASK_HOLD_CAP_SECS`], then replies `pending`, so the tool timeout is
    /// the cap plus the margin (the spike saw codex cancel a held call at its
    /// 300 s default, so its client must outlast the cap). Every string value
    /// is escaped like the writable roots.
    fn ask_wiring(&self, reg: &AskRegistration) -> Result<AskWiring, String> {
        let (exe, _root) = reg.utf8_paths()?;
        let quoted = |value: &str| format!("\"{}\"", escape_toml_basic_string(value));
        let mut server_args = reg.server_args();
        server_args.extend(["--hold-cap-secs".to_string(), ASK_HOLD_CAP_SECS.to_string()]);
        let server_args = server_args
            .iter()
            .map(|arg| quoted(arg))
            .collect::<Vec<_>>()
            .join(",");
        let overrides = [
            format!("command={}", quoted(exe)),
            format!("args=[{server_args}]"),
            format!(
                "tool_timeout_sec={}",
                ASK_HOLD_CAP_SECS + ASK_CLIENT_TIMEOUT_MARGIN_SECS
            ),
            format!("default_tools_approval_mode={}", quoted("approve")),
            "enabled=true".to_string(),
        ];
        let args = overrides
            .into_iter()
            .flat_map(|value| ["-c".to_string(), format!("mcp_servers.devflow.{value}")])
            .collect();
        Ok(AskWiring {
            args,
            placement: ArgPlacement::BeforeLast,
            file: None,
        })
    }

    fn interactivity_mode(&self, stage: crate::stage::Stage) -> InteractivityMode {
        use crate::stage::Stage;
        match stage {
            // Codex cannot run the interactive discuss-phase interview or the
            // interactive plan-phase decision headless — its Plan stage needs
            // the artifact to pre-exist (13-06 dogfood finding).
            Stage::Plan => InteractivityMode::RequiresExistingArtifact,
            _ => InteractivityMode::HeadlessSafe,
        }
    }
}

/// Escape a string for embedding inside a TOML basic (double-quoted) string.
///
/// 999.107 #2: the previous serializer escaped only `\` and `"`, so a path
/// containing a newline or other control character produced malformed TOML and
/// a corrupt `sandbox_workspace_write.writable_roots` override. Control
/// characters are escaped as `\n`/`\t`/`\r` (or `\uXXXX` for the rest) so the
/// array value stays a valid TOML string no matter what a path contains.
fn escape_toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c == '\u{7F}' => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase_id::PhaseId;

    fn writable_roots_flag(args: &[String]) -> &str {
        let idx = args
            .iter()
            .position(|a| a == "-c")
            .expect("-c flag present");
        &args[idx + 1]
    }

    /// 999.107 #2: a path containing a quote, backslash, and newline must
    /// serialize to valid TOML — the newline becomes `\n`, the quote `\"`,
    /// the backslash `\\` — never a raw control character that would corrupt
    /// the `writable_roots` array value.
    #[test]
    fn codex_writable_roots_escape_hostile_paths() {
        let roots = vec![PathBuf::from("/repo/a\"b\\c\nd")];
        let (_, args) = CodexDriver.build_command(PhaseId::new(7), "prompt", &roots);
        let flag = writable_roots_flag(&args);
        assert!(
            !flag.contains('\n'),
            "raw newline must be escaped: {flag:?}"
        );
        assert!(flag.contains(r#"\n"#), "newline must be `\\n`: {flag:?}");
        assert!(flag.contains(r#"\""#), "quote must be escaped: {flag:?}");
        assert!(
            flag.contains(r#"\\"#),
            "backslash must be escaped: {flag:?}"
        );
    }

    /// 999.107 #2 / review: DEL (U+007F) must be escaped as `\u007F`, not
    /// emitted literally — TOML forbids a raw DEL in a basic string.
    #[test]
    fn codex_writable_roots_escape_del() {
        let roots = vec![PathBuf::from("/repo/a\u{7F}b")];
        let (_, args) = CodexDriver.build_command(PhaseId::new(7), "prompt", &roots);
        let flag = writable_roots_flag(&args);
        assert!(
            !flag.contains('\u{7F}'),
            "raw DEL must be escaped: {flag:?}"
        );
        assert!(
            flag.contains(r#"\u007F"#),
            "DEL must be `\\u007F`: {flag:?}"
        );
    }

    /// 999.107 #2: a non-UTF-8 path cannot be serialized losslessly, so the
    /// launch must refuse (panic) rather than emit a writable root that names
    /// a different path.
    #[cfg(unix)]
    #[test]
    #[should_panic(expected = "non-UTF-8 writable root")]
    fn codex_writable_roots_refuses_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        // 0xFF is not valid UTF-8.
        let raw = std::ffi::OsString::from_vec(vec![b'/', b'r', b'e', b'p', b'o', 0xFF, b'x']);
        let roots = vec![PathBuf::from(raw)];
        let _ = CodexDriver.build_command(PhaseId::new(7), "prompt", &roots);
    }

    fn ask_registration(project_root: &str) -> crate::agents::AskRegistration {
        crate::agents::AskRegistration {
            exe: PathBuf::from("/opt/bin/devflow"),
            project_root: PathBuf::from(project_root),
            phase: PhaseId::new(7),
            stage: crate::stage::Stage::Code,
            timeout_secs: 120,
            run_started_at: "2026-11-18T00:00:00Z".into(),
            config_path: PathBuf::from("/unused/.devflow/ask-07.mcp.json"),
        }
    }

    /// The value of the `-c` override for `key`, or a panic naming the key.
    fn override_value<'a>(args: &'a [String], key: &str) -> &'a str {
        let prefix = format!("mcp_servers.devflow.{key}=");
        args.windows(2)
            .find(|w| w[0] == "-c" && w[1].starts_with(&prefix))
            .map(|w| &w[1][prefix.len()..])
            .unwrap_or_else(|| panic!("no -c {prefix}… in {args:?}"))
    }

    /// D-03: codex gets the server through `-c mcp_servers.devflow.*` overrides
    /// placed before the prompt: the command, its args with the hold cap, a
    /// tool timeout of the cap plus the margin, approval without a prompt, and
    /// enabled.
    /// Every value is a TOML string escaped like the writable roots.
    #[test]
    fn launch_args_set_tool_timeout_and_approval_before_the_prompt() {
        let wiring = CodexDriver
            .ask_wiring(&ask_registration("/work/project"))
            .expect("codex registers the server");
        assert_eq!(wiring.placement, crate::agents::ArgPlacement::BeforeLast);
        assert_eq!(wiring.file, None, "codex writes no file");
        let args = &wiring.args;
        assert_eq!(args.len(), 10, "five -c pairs: {args:?}");
        assert_eq!(override_value(args, "command"), "\"/opt/bin/devflow\"");
        assert_eq!(
            override_value(args, "args"),
            "[\"__ask\",\"--project\",\"/work/project\",\"--phase\",\"7\",\"--stage\",\
             \"code\",\"--timeout-secs\",\"120\",\"--run-started-at\",\"2026-11-18T00:00:00Z\",\
             \"--hold-cap-secs\",\"900\"]"
        );
        assert_eq!(override_value(args, "tool_timeout_sec"), "960");
        assert_eq!(
            override_value(args, "default_tools_approval_mode"),
            "\"approve\""
        );
        assert_eq!(override_value(args, "enabled"), "true");

        let hostile = CodexDriver
            .ask_wiring(&ask_registration("/work/a\"b"))
            .expect("codex registers the server");
        let value = override_value(&hostile.args, "args");
        assert!(
            value.contains(r#""/work/a\"b""#),
            "the quote in the project path is escaped: {value}"
        );
    }

    /// OD-1 (b), production values: codex is held 900 s at most per call and its
    /// client waits 960 s, so the server's `pending` reaches it first.
    #[test]
    fn codex_holds_up_to_the_cap_and_its_client_waits_longer() {
        assert_eq!(ASK_HOLD_CAP_SECS, 900, "the hold cap");
        assert_eq!(ASK_CLIENT_TIMEOUT_MARGIN_SECS, 60, "the client margin");
        let client_timeout = ASK_HOLD_CAP_SECS + ASK_CLIENT_TIMEOUT_MARGIN_SECS;
        assert!(
            ASK_HOLD_CAP_SECS < client_timeout,
            "the client outlasts the cap"
        );
        let wiring = CodexDriver
            .ask_wiring(&ask_registration("/work/project"))
            .expect("codex registers the server");
        let args = override_value(&wiring.args, "args");
        assert!(
            args.ends_with(",\"--hold-cap-secs\",\"900\"]"),
            "the server args carry the cap: {args}"
        );
        assert_eq!(override_value(&wiring.args, "tool_timeout_sec"), "960");
    }

    /// A non-UTF-8 project root is refused with an error, never a panic.
    #[cfg(unix)]
    #[test]
    fn ask_wiring_refuses_a_non_utf8_project_root() {
        use std::os::unix::ffi::OsStringExt;
        let mut reg = ask_registration("/work/project");
        reg.project_root = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xFF]));
        let refused = CodexDriver
            .ask_wiring(&reg)
            .expect_err("non-UTF-8 is refused");
        assert!(refused.contains("not UTF-8"), "{refused}");
    }
}

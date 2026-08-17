//! OpenAI Codex agent driver.
//!
//! Launches `codex -a never exec "<prompt>"` in non-interactive mode with JSON
//! output. `-a never` is the GLOBAL approval flag and must precede `exec` —
//! verified against the installed CLI (a `codex exec -a never` placement is
//! rejected as an unknown argument).

use super::{AgentDriver, InteractivityMode};
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
                    let path = root.to_string_lossy();
                    format!("\"{}\"", escape_toml_basic_string(&path))
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
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

    /// 999.107 #2: a non-UTF-8 path is lossily converted to U+FFFD and still
    /// serializes as a valid TOML string (no raw invalid byte, no raw control
    /// character).
    #[cfg(unix)]
    #[test]
    fn codex_writable_roots_lossy_for_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;
        // 0xFF is not valid UTF-8; `to_string_lossy` maps it to U+FFFD.
        let raw = std::ffi::OsString::from_vec(vec![b'/', b'r', b'e', b'p', b'o', 0xFF, b'x']);
        let roots = vec![PathBuf::from(raw)];
        let (_, args) = CodexDriver.build_command(PhaseId::new(7), "prompt", &roots);
        let flag = writable_roots_flag(&args);
        assert!(
            flag.contains('\u{FFFD}'),
            "lossy replacement expected: {flag:?}"
        );
        assert!(!flag.contains('\n'), "no raw control chars: {flag:?}");
    }
}

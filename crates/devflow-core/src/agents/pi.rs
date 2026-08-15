//! Pi coding-agent harness adapter.
//!
//! Launches `pi -p "<prompt>"` in non-interactive print mode. The prompt is
//! POSITIONAL — `-p` is a boolean flag (not the prompt carrier, and not stdin
//! transport like Claude's stream-json). `--no-approve` is always passed:
//! `--approve` trusts project-local extensions/skills/settings that execute
//! UNSANDBOXED (Pi ships no sandbox), and a fresh per-phase worktree establishes
//! no trust decision — that is a security boundary, not a convenience. A bare
//! `--` precedes the prompt so a leading `-`/`- [ ]` (markdown lists in stage
//! prompts) is never parsed as a flag.
//!
//! No `--model`/`--provider` wiring here: model/provider selection is the
//! `AgentDriver` contract's job (Phase 37), and `AgentAdapter` has no config
//! surface to source it from. Pi uses its own defaults (provider `google`).

use super::AgentAdapter;
use crate::phase_id::PhaseId;
use std::path::PathBuf;

pub struct PiAgent;

impl AgentAdapter for PiAgent {
    fn name(&self) -> &'static str {
        "Pi"
    }

    fn exec_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "pi",
            vec![
                "-p".into(),
                "--no-approve".into(),
                "--".into(),
                prompt.to_string(),
            ],
        )
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        // `pi -p` exits cleanly when done; the monitor detects exit via kill -0.
        // (Mirrors ClaudeAgent — print-mode transport has no event stream to scan.)
        false
    }

    /// Credential readiness via `pi auth check` — Pi's own verb — rather than
    /// env-var sniffing. `DEVFLOW_PI_PROVIDER` is a provider *name*, not a
    /// credential; treating it as one is a false-green. The "binary absent"
    /// case is out of scope here: `ensure_agent_binary` runs first on the
    /// start path, so this only ever runs once `pi` exists.
    fn preflight(&self, _state: &crate::state::State) -> Result<(), String> {
        let output = std::process::Command::new("pi")
            .args(["auth", "check", "--json"])
            .output()
            .map_err(|e| format!("could not run `pi auth check`: {e}"))?;
        classify_auth_check(&String::from_utf8_lossy(&output.stdout), output.status.success())
    }
}

/// Map `pi auth check --json` output to a readiness verdict. Split out so the
/// classification is unit-testable without spawning a process.
fn classify_auth_check(stdout: &str, success: bool) -> Result<(), String> {
    // A successful exit code alone is not enough: a credentialless check still
    // prints `{"status":"not_ready",...}`. Require the `ready` status AND exit 0.
    let ready = success && stdout.contains("\"status\":\"ready\"");
    if ready {
        Ok(())
    } else {
        Err("no provider credential resolves — run `pi auth login`".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_command_shape() {
        let (program, args) = PiAgent.exec_command(PhaseId::new(1), "do the thing", &[]);
        assert_eq!(program, "pi");
        assert_eq!(args, vec!["-p", "--no-approve", "--", "do the thing"]);
    }

    #[test]
    fn exec_command_delimits_a_prompt_that_starts_with_a_dash() {
        // A positional prompt that begins with `- [ ]` must not be parsed as a
        // flag — the bare `--` separates it from pi's option parser.
        let (_, args) = PiAgent.exec_command(PhaseId::new(1), "- [ ] task", &[]);
        assert_eq!(
            args,
            vec!["-p", "--no-approve", "--", "- [ ] task"]
        );
    }

    #[test]
    fn classify_auth_check_rejects_not_ready() {
        assert!(classify_auth_check(
            r#"{"status":"not_ready","provider":"google","reason":"credentials_not_configured"}"#,
            false,
        )
        .is_err());
    }

    #[test]
    fn classify_auth_check_accepts_ready() {
        assert!(classify_auth_check(
            r#"{"status":"ready","provider":"google","authType":"api_key"}"#,
            true,
        )
        .is_ok());
    }

    #[test]
    fn classify_auth_check_rejects_ready_text_with_failed_exit() {
        // A failed exit must not be read as ready even if the body says "ready".
        assert!(classify_auth_check(r#"{"status":"ready"}"#, false).is_err());
    }
}

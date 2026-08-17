//! Pi coding-agent harness adapter.
//!
//! Launches `pi -p "<prompt>"` in non-interactive print mode. The prompt is
//! POSITIONAL — `-p` is a boolean flag (not the prompt carrier, and not stdin
//! transport like Claude's stream-json). `--no-approve` is always passed:
//! `--approve` trusts project-local extensions/skills/settings that execute
//! UNSANDBOXED (Pi ships no sandbox), and a fresh per-phase worktree establishes
//! no trust decision — that is a security boundary, not a convenience.
//!
//! No `--model`/`--provider` wiring here: model/provider selection is the
//! `AgentDriver` contract's job (Phase 37), and the launch surface has no
//! config to source it from. Pi uses its own defaults (provider `google`).
//!
//! Note: Pi has NO `--` end-of-options convention — passing `--` is rejected as
//! an unknown option, so the prompt is passed raw. DevFlow's own stage prompts
//! never begin with `-`, so the leading-dash hazard (a markdown `- [ ]` list) is
//! a Phase 37 concern, not something a `--` can guard here.

use super::AgentDriver;
use crate::phase_id::PhaseId;
use std::path::PathBuf;

/// The modular driver for Pi (37-03): print-mode `-p` launch, `pi auth check`
/// health, and the de-Claude-ified workflow-reference prompt. NO JSON unwrapper
/// or monitor/`CloseRule` integration here — that is 37.1/38 (CONTEXT D-04).
pub struct PiDriver;

impl AgentDriver for PiDriver {
    fn name(&self) -> &'static str {
        "Pi"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_workflow_style(intent, &self.workflow_root())
    }

    /// Pi installs its GSD workflows under `~/.pi/agent/gsd-core/workflows/`,
    /// NOT the Codex install the default points at (code-review finding #5).
    fn workflow_root(&self) -> String {
        "$HOME/.pi/agent/gsd-core/workflows".to_string()
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "pi",
            vec!["-p".into(), "--no-approve".into(), prompt.to_string()],
        )
    }

    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        // Credential readiness via `pi auth check` — Pi's own verb — rather
        // than env-var sniffing (see `classify_auth_check`).
        // `--no-refresh` prevents a stalled OAuth token refresh from hanging
        // preflight (code-review finding #8: `pi auth check` refreshes expired
        // credentials by default, and `.output()` has no timeout).
        let output = std::process::Command::new("pi")
            .args([
                "auth",
                "check",
                "--json",
                "--provider",
                "google",
                "--no-refresh",
            ])
            .output()
            .map_err(|e| format!("could not run `pi auth check`: {e}"))?;
        classify_auth_check(
            &String::from_utf8_lossy(&output.stdout),
            output.status.success(),
        )
    }
}

/// Map `pi auth check --json` output to a readiness verdict. Split out so the
/// classification is unit-testable without spawning a process. Parses the JSON
/// rather than substring-matching, so whitespace formatting can't defeat it.
fn classify_auth_check(stdout: &str, success: bool) -> Result<(), String> {
    // A successful exit code alone is not enough: a credentialless check still
    // prints `{"status":"not_ready",...}`. Require the `ready` status AND exit 0.
    let ready = success
        && serde_json::from_str::<serde_json::Value>(stdout)
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_owned))
            .is_some_and(|s| s == "ready");
    if ready {
        Ok(())
    } else {
        Err("no provider credential resolves — run `pi auth check` for details".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use crate::state::{AgentKind, State};
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-global `PATH` (`set_var` is
    /// process-wide; `cargo test` runs tests in parallel).
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn exec_command_shape() {
        let (program, args) = PiDriver.build_command(PhaseId::new(1), "do the thing", &[]);
        assert_eq!(program, "pi");
        assert_eq!(args, vec!["-p", "--no-approve", "do the thing"]);
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
        assert!(
            classify_auth_check(
                r#"{"status":"ready","provider":"google","authType":"api_key"}"#,
                true,
            )
            .is_ok()
        );
    }

    #[test]
    fn classify_auth_check_tolerates_formatted_json() {
        // Pretty-printed / whitespace-padded JSON must not defeat the parse.
        assert!(classify_auth_check("{\n  \"status\": \"ready\"\n}", true).is_ok());
    }

    #[test]
    fn classify_auth_check_rejects_ready_text_with_failed_exit() {
        // A failed exit must not be read as ready even if the body says "ready".
        assert!(classify_auth_check(r#"{"status":"ready"}"#, false).is_err());
    }

    /// A `State` value for `preflight`, which ignores it (`_state`) —
    /// constructed only to satisfy the trait signature.
    fn test_state() -> State {
        State::new(
            PhaseId::new(36),
            AgentKind::Pi,
            Mode::Auto,
            std::path::PathBuf::from("/tmp"),
        )
    }

    /// Writes an executable `pi` stub into a fresh tempdir. The stub records its
    /// arguments (`"$@"`, one per line) to `args.txt` in the same dir, prints
    /// `body` to stdout, and exits with `exit_code`. The returned tempdir is the
    /// only entry the test puts on `PATH`, so the operator's live `pi` is never
    /// consulted.
    fn stub_pi_on_path(body: &str, exit_code: i32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("pi");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\necho '{body}'\nexit {exit_code}\n",
            args = dir.path().join("args.txt").display(),
            body = body,
            exit_code = exit_code,
        );
        std::fs::write(&stub, script).expect("write pi stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
        }
        dir
    }

    /// RAII guard that replaces `PATH` with `path` and restores the previous
    /// value on `Drop` — including the panic path, so a failing test never
    /// hands the next test a mutated `PATH`.
    struct PathGuard {
        original: Option<std::ffi::OsString>,
    }

    impl PathGuard {
        fn set(path: &std::path::Path) -> Self {
            let original = std::env::var_os("PATH");
            // SAFETY: held under ENV_MUTEX; no other thread reads/writes PATH.
            unsafe { std::env::set_var("PATH", path) };
            Self { original }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(prev) => unsafe { std::env::set_var("PATH", prev) },
                None => unsafe { std::env::remove_var("PATH") },
            }
        }
    }

    /// The shell-out must actually spawn `pi auth check --json --provider
    /// google` — not just classify a pre-parsed string. The stub records its
    /// argv, so this proves the wiring end to end.
    #[test]
    fn preflight_invokes_pi_auth_check_and_accepts_ready() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path(r#"{"status":"ready"}"#, 0);
        let _path = PathGuard::set(stub_dir.path());

        PiDriver
            .health(&test_state())
            .expect("a `ready` stub should pass preflight");

        let argv = std::fs::read_to_string(stub_dir.path().join("args.txt")).unwrap();
        assert_eq!(
            argv,
            "auth\ncheck\n--json\n--provider\ngoogle\n--no-refresh\n"
        );
    }

    /// The negative control AC #1 requires: a `pi` binary that reports
    /// `not_ready` must yield the credentialless `Err`, proving the predicate
    /// tests credential readiness, not env-var presence.
    #[test]
    fn preflight_reports_credentialless_when_auth_check_says_not_ready() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path(
            r#"{"status":"not_ready","reason":"credentials_not_configured"}"#,
            0,
        );
        let _path = PathGuard::set(stub_dir.path());

        let err = PiDriver
            .health(&test_state())
            .expect_err("a `not_ready` stub should fail preflight");
        assert!(
            err.contains("no provider credential resolves"),
            "unexpected error: {err}"
        );
    }

    /// The exit code must be honored through the shell-out path, not just the
    /// pure classifier: a `ready` body with a failed exit is still a failure.
    #[test]
    fn preflight_rejects_ready_body_with_failed_exit() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path(r#"{"status":"ready"}"#, 1);
        let _path = PathGuard::set(stub_dir.path());

        assert!(
            PiDriver.health(&test_state()).is_err(),
            "a failed exit must not be read as ready even when the body says ready"
        );
    }
}

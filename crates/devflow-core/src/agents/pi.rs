//! Pi coding-agent harness adapter.
//!
//! Launches `pi -p "<prompt>"` in non-interactive print mode. The prompt is
//! POSITIONAL — `-p` is a boolean flag (not the prompt carrier, and not stdin
//! transport like Claude's stream-json). `--no-approve` is always passed:
//! `--approve` trusts project-local extensions/skills/settings that execute
//! UNSANDBOXED (Pi ships no sandbox), and a fresh per-phase worktree establishes
//! no trust decision — that is a security boundary, not a convenience.
//!
//! No `--model`/`--provider` wiring in the launch argv: model/provider
//! selection is Pi's own. The `health` check probes the provider a launch will
//! actually use — `settings.json`'s `defaultProvider` (this machine:
//! `litellm`), falling back to Pi's built-in `--provider` default (`google`)
//! when unset — never a hardcoded provider and never "any ready provider in
//! `models.json`".
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

    /// Pi declares subagent-dispatch capability when a subagent extension is
    /// installed in the user profile (see [`pi_subagent_dispatch_available`]).
    fn capabilities(&self) -> super::DriverCapabilities {
        super::DriverCapabilities {
            subagent_dispatch: pi_subagent_dispatch_available(),
        }
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
        // than env-var sniffing (see `classify_auth_check`). `--no-refresh`
        // prevents a stalled OAuth token refresh from hanging preflight
        // (code-review finding #8).
        //
        // Probe the provider a launch will ACTUALLY use: `settings.json`'s
        // `defaultProvider`. `build_command` passes no `--provider`, so the run
        // selects the default — probing "any ready provider in `models.json`"
        // false-greens a credential the run never touches, and refusing when
        // `models.json` is absent false-rejects every standard install
        // (built-in providers are credentialled from env vars / `auth.json`,
        // never listed in `models.json`). Fall back to Pi's `--provider`
        // default (`google`) when `settings.json` carries no `defaultProvider`.
        let provider = configured_pi_provider().unwrap_or_else(|| "google".to_string());
        let output = std::process::Command::new("pi")
            .args(["auth", "check", "--json", "--provider", &provider, "--no-refresh"])
            .output()
            .map_err(|e| format!("could not run `pi auth check`: {e}"))?;
        classify_auth_check(&String::from_utf8_lossy(&output.stdout), output.status.success())
            .map_err(|reason| {
                format!(
                    "{reason} for provider `{provider}` — `pi auth check --json --provider {provider}` reports it not ready"
                )
            })
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
        Err("no provider credential resolves".to_string())
    }
}

/// The provider a `pi -p` launch actually uses: `settings.json`'s
/// `defaultProvider`. `None` when the file is missing/unparseable or carries no
/// `defaultProvider` — the caller falls back to Pi's built-in `--provider`
/// default (`google`, per `pi --help`).
///
/// `models.json` is NOT the provider configuration: it is the custom-model
/// CATALOG (LiteLLM/vLLM endpoints). A standard Pi install (built-in provider
/// credentialled from `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`/OAuth in
/// `auth.json`) has no `models.json` at all — reading it here both
/// hard-refuses default installs and false-greens a provider the run never
/// selects (phase-39 code review, finding 1).
fn configured_pi_provider() -> Option<String> {
    let base = pi_config_dir()?;
    let path = base.join("settings.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    json.get("defaultProvider")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Resolve Pi's config dir: `PI_CODING_AGENT_DIR` if set, else `~/.pi/agent`.
/// A leading `~` is expanded the way Pi's own `getAgentDir` does, so a tilde
/// value resolves instead of yielding an unreadable literal path (phase-39
/// code review, claude LOW #7).
fn pi_config_dir() -> Option<std::path::PathBuf> {
    let raw = std::env::var_os("PI_CODING_AGENT_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join(".pi").join("agent"))
        })?;
    let raw_str = raw.to_string_lossy();
    if let Some(rest) = raw_str.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return Some(std::path::PathBuf::from(home).join(rest));
    }
    Some(raw)
}

/// Whether Pi's profile has the vetted `@bacnh85/pi-subagent` dispatch
/// extension installed. Probed via `pi list --no-approve` — Pi exposes no
/// `pi tools` command, so the installed-package name is the only cheap,
/// non-interactive signal. The match is the specific vetted package, NOT a
/// bare `*subagent*` substring: other `subagent`-named packages (`@mystilleef`,
/// `@dreki-gg`, `@smoose`) are unsafe/deferred and must not be reported
/// available (phase-39 code review, finding 2).
///
/// **Honest limit:** name-based, not a tool-registry proof — it does not
/// confirm the extension registers a working dispatch tool. Any probe failure
/// returns `false`, so an undetectable profile fails closed to the baseline
/// single-agent path rather than refusing a working run.
fn pi_subagent_dispatch_available() -> bool {
    let Ok(output) = std::process::Command::new("pi")
        .args(["list", "--no-approve"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .to_lowercase()
            .contains("@bacnh85/pi-subagent")
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

    /// Like [`stub_pi_on_path`], plus a `settings.json` naming the provider a
    /// launch would use — so the health check probes the configured provider
    /// instead of a hardcoded `google`.
    fn stub_pi_with_provider(body: &str, exit_code: i32, provider: &str) -> tempfile::TempDir {
        let dir = stub_pi_on_path(body, exit_code);
        std::fs::write(
            dir.path().join("settings.json"),
            format!(r#"{{"defaultProvider":"{provider}"}}"#),
        )
        .expect("write settings.json");
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

    /// RAII guard that sets an environment variable to `value` and restores it
    /// on `Drop` — the same panic-safe pattern as [`PathGuard`].
    struct EnvGuard {
        name: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(name: &'static str, value: &std::path::Path) -> Self {
            let original = std::env::var_os(name);
            // SAFETY: held under ENV_MUTEX; no other thread reads/writes this var.
            unsafe { std::env::set_var(name, value) };
            Self { name, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(prev) => unsafe { std::env::set_var(self.name, prev) },
                None => unsafe { std::env::remove_var(self.name) },
            }
        }
    }

    /// The shell-out must actually spawn `pi auth check --json --provider
    /// <configured>` — not just classify a pre-parsed string. The stub records
    /// its argv, proving the wiring end to end, and that the provider is read
    /// from `settings.json` (`litellm`), not hardcoded.
    #[test]
    fn preflight_invokes_pi_auth_check_and_accepts_ready() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_with_provider(r#"{"status":"ready"}"#, 0, "litellm");
        let _path = PathGuard::set(stub_dir.path());
        let _cfgdir = EnvGuard::set("PI_CODING_AGENT_DIR", stub_dir.path());

        PiDriver
            .health(&test_state())
            .expect("a `ready` stub should pass preflight");

        let argv = std::fs::read_to_string(stub_dir.path().join("args.txt")).unwrap();
        assert_eq!(
            argv,
            "auth\ncheck\n--json\n--provider\nlitellm\n--no-refresh\n"
        );
    }

    /// The negative control AC #1 requires: a `pi` binary that reports
    /// `not_ready` must yield the credentialless `Err`, proving the predicate
    /// tests credential readiness, not env-var presence.
    #[test]
    fn preflight_reports_credentialless_when_auth_check_says_not_ready() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_with_provider(
            r#"{"status":"not_ready","reason":"credentials_not_configured"}"#,
            0,
            "litellm",
        );
        let _path = PathGuard::set(stub_dir.path());
        let _cfgdir = EnvGuard::set("PI_CODING_AGENT_DIR", stub_dir.path());

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
        let stub_dir = stub_pi_with_provider(r#"{"status":"ready"}"#, 1, "litellm");
        let _path = PathGuard::set(stub_dir.path());
        let _cfgdir = EnvGuard::set("PI_CODING_AGENT_DIR", stub_dir.path());

        assert!(
            PiDriver.health(&test_state()).is_err(),
            "a failed exit must not be read as ready even when the body says ready"
        );
    }

    /// No `settings.json` (a standard install: built-in provider from env vars
    /// / `auth.json`, no custom `models.json`) must NOT be hard-refused — the
    /// health check falls back to Pi's `--provider` default (`google`) and lets
    /// `pi auth check` report readiness (phase-39 code review, finding 1a).
    #[test]
    fn preflight_falls_back_to_google_when_no_default_provider() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path(r#"{"status":"ready"}"#, 0);
        let _path = PathGuard::set(stub_dir.path());
        let _cfgdir = EnvGuard::set("PI_CODING_AGENT_DIR", stub_dir.path());

        PiDriver
            .health(&test_state())
            .expect("a default-provider stub must pass preflight via the google fallback");

        let argv = std::fs::read_to_string(stub_dir.path().join("args.txt")).unwrap();
        assert_eq!(argv, "auth\ncheck\n--json\n--provider\ngoogle\n--no-refresh\n");
    }

    /// The capability probe shells out to `pi list --no-approve` and matches on
    /// the installed-package name (there is no `pi tools` command). A stub
    /// reporting an installed subagent package flips the capability on, and the
    /// argv proves the probe is exactly `pi list --no-approve`.
    #[test]
    fn pi_capabilities_detect_subagent_dispatch() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path("npm:@bacnh85/pi-subagent@0.15.1 (user)", 0);
        let _path = PathGuard::set(stub_dir.path());

        assert!(PiDriver.capabilities().subagent_dispatch);

        let argv = std::fs::read_to_string(stub_dir.path().join("args.txt")).unwrap();
        assert_eq!(argv, "list\n--no-approve\n");
    }

    /// A `subagent`-named package that is NOT the vetted `@bacnh85/pi-subagent`
    /// (e.g. the unsafe/deferred `@mystilleef`) must NOT flip the capability on
    /// — the name-match is the specific package, not `*subagent*` (phase-39
    /// code review, finding 2).
    #[test]
    fn pi_capabilities_exclude_unvetted_subagent_packages() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path("npm:@mystilleef/pi-subagent@2.0.0 (user)", 0);
        let _path = PathGuard::set(stub_dir.path());

        assert!(!PiDriver.capabilities().subagent_dispatch);
    }

    /// No subagent package in `pi list` → capability stays off (baseline path).
    #[test]
    fn pi_capabilities_fail_closed_when_no_subagent() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path("No packages installed.", 0);
        let _path = PathGuard::set(stub_dir.path());

        assert!(!PiDriver.capabilities().subagent_dispatch);
    }

    /// A failing probe (non-zero exit) fails closed to baseline, never refuses.
    #[test]
    fn pi_capabilities_fail_closed_when_probe_fails() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_pi_on_path("", 1);
        let _path = PathGuard::set(stub_dir.path());

        assert!(!PiDriver.capabilities().subagent_dispatch);
    }
}

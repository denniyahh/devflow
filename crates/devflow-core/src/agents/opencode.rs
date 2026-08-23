//! OpenCode agent driver.
//!
//! Launches `opencode run "<prompt>" --auto --format json` in non-interactive
//! mode. `--auto` is opencode's own label for "auto-approve permissions not
//! explicitly denied (dangerous!)" — the spawned agent executes tool calls
//! with no human in the loop, the same posture as Pi's `--no-approve` /
//! Codex's `-a never` (T-43-01, P-01). This flag must appear ONLY in this
//! launch argv, never in a health or capability probe.
//!
//! `health` (43-02, OPCD-03/D-07) is a fail-closed credential check: it
//! spawns `opencode providers list`, strips its ANSI escape codes, and sums
//! the terminal `N credentials` / `N environment variables` count lines. The
//! subcommand's exit code is always 0 regardless of credential state
//! (verified live), so readiness is decided solely from the parsed count,
//! never from `output.status.success()`. `opencode models` is deliberately
//! never used as the readiness probe — it always lists opencode's own free
//! catalog entries and would false-green a machine with zero configured
//! credentials (D-09).
//!
//! `capabilities` (43-02, OPCD-03/D-10) probes `opencode agent list` for a
//! configured subagent- or all-mode agent. Any probe failure, non-zero exit,
//! or unparseable output fails closed to `subagent_dispatch: false` — this
//! probe must never refuse a launch; only `health` may.

use crate::phase_id::PhaseId;

/// The modular driver for OpenCode (37-02/43-01/43-02): headless
/// `--auto --format json` launch, JSONL completion parsing delegated to
/// `agent_result::parse_opencode_event_result`, legacy prompt rendering, a
/// fail-closed credential health check, and a fail-closed subagent-dispatch
/// capability probe.
pub struct OpenCodeDriver;

impl super::AgentDriver for OpenCodeDriver {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "opencode",
            vec![
                "run".into(),
                prompt.to_string(),
                "--auto".into(),
                "--format".into(),
                "json".into(),
            ],
        )
    }

    /// Relocate the OpenCode JSONL completion parsing under driver ownership:
    /// the function body lives in `agent_result.rs` (where the
    /// result-evaluation path and its fixtures live), and this method is the
    /// driver's contract entry point for it — matching Codex's delegation
    /// pattern (RESEARCH Pattern 2).
    fn parse_completion(&self, output: &str) -> Option<crate::agent_result::AgentResult> {
        crate::agent_result::parse_opencode_event_result(output)
    }

    /// Fail-closed credential check (OPCD-03, D-07/D-08/D-09, T-43-09).
    ///
    /// `opencode providers list` has no JSON output mode (D-08) and exits 0
    /// regardless of credential state (verified live), so readiness is
    /// decided by summing the ANSI-stripped output's terminal
    /// `N credentials` / `N environment variables` count lines — never by
    /// `output.status.success()`. A spawn failure (e.g. no `opencode` on
    /// `PATH`) also fails closed to `Err`, never a panic.
    ///
    /// The returned `Err` is a fixed message naming only the derived state.
    /// It never interpolates the probe's raw stdout, a provider name, an
    /// `auth.json` path, or an environment-variable name (P-04, T-43-11) —
    /// this repository has three prior instances of exactly this leak class
    /// (999.10, WR-02).
    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        let output = std::process::Command::new("opencode")
            .args(["providers", "list"])
            .output()
            .map_err(|e| format!("could not run `opencode providers list`: {e}"))?;
        if opencode_configured_provider_count(&String::from_utf8_lossy(&output.stdout)) > 0 {
            Ok(())
        } else {
            Err("no OpenCode provider credential configured".to_string())
        }
    }

    /// Fail-closed subagent-dispatch capability probe (OPCD-03, D-10,
    /// T-43-10). This probe can never refuse a launch — only `health` may;
    /// the return type carries that guarantee (`DriverCapabilities`, not a
    /// `Result`).
    fn capabilities(&self) -> super::DriverCapabilities {
        super::DriverCapabilities {
            subagent_dispatch: opencode_subagent_dispatch_available(),
        }
    }
}

/// Hand-rolled SGR escape-sequence scrubber: on `\u{1b}` followed by `[`,
/// consumes through the terminating `m`. No `regex` crate is added for this
/// — this workspace has no `regex` dependency anywhere, and
/// `strip_corruption_padding` (`agent_result.rs`) is the standing precedent
/// for a single-purpose manual text scrubber over pulling in a crate.
fn strip_ansi_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Sum every `<n> credentials` / `<n> environment variable(s)` terminal count
/// line in `opencode providers list`'s (ANSI-stripped) output. `0` means no
/// usable provider is configured — the fail-closed signal (D-07).
///
/// Summing terminal count lines rather than pattern-matching a section
/// header is deliberate: it returns `0` identically for an absent section,
/// an empty section, and an explicit zero count line — the widest safe
/// reading of the shapes a genuinely credential-less machine could produce.
///
/// **Honest limit (A1, P-05):** the exact stdout shape of `opencode
/// providers list` on a machine with ZERO configured credentials was never
/// observed live — no destructive test against a credential-less machine was
/// performed. This function's zero-credential behavior is proven only
/// against constructed fixtures reasoned from the live positive-credential
/// capture below, never against a real credential-less run.
fn opencode_configured_provider_count(stdout: &str) -> u32 {
    strip_ansi_escapes(stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start_matches(['└', '┌', '│', '●', ' ']);
            let (num, rest) = trimmed.split_once(' ')?;
            let n: u32 = num.parse().ok()?;
            (rest.starts_with("credential") || rest.starts_with("environment variable"))
                .then_some(n)
        })
        .sum()
}

/// Probe whether OpenCode has a genuinely dispatchable subagent configured.
/// Mirrors Hermes's `_with(output_fn)` split (`hermes.rs`) rather than Pi's
/// bare spawn, since no stub-binary spawn path itself needs testing here —
/// only the classification of an arbitrary `Output`.
fn opencode_subagent_dispatch_available() -> bool {
    opencode_subagent_dispatch_available_with(|| {
        std::process::Command::new("opencode")
            .args(["agent", "list"])
            .output()
    })
}

/// Mockable inner form of [`opencode_subagent_dispatch_available`]. Fails
/// closed to `false` on a spawn error, a non-zero exit, empty stdout, or no
/// matching header line — never a panic, and never a signal that could
/// refuse a launch (only `health` may refuse).
///
/// **Honest limit (A4):** the header-line form (`<name> (<mode>)`) is
/// inferred from `opencode agent create --help`'s documented `--mode`
/// choices (`primary`/`subagent`/`all`) plus one live single-agent baseline
/// (`build (primary)`) — no live capture of a real configured subagent
/// exists on this machine. A substring match tolerates some spacing drift
/// but is not proven against real subagent output; a miss costs a false
/// negative, which is the safe direction.
fn opencode_subagent_dispatch_available_with(
    output_fn: impl FnOnce() -> std::io::Result<std::process::Output>,
) -> bool {
    let Ok(output) = output_fn() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    parse_opencode_agent_list_for_subagent(&String::from_utf8_lossy(&output.stdout))
}

/// Pure classifier over `opencode agent list` stdout: a dispatchable
/// subagent is any header line carrying the `(subagent)` or `(all)` mode
/// marker. The default `build` agent is `(primary)` and must never count.
fn parse_opencode_agent_list_for_subagent(stdout: &str) -> bool {
    stdout
        .lines()
        .any(|line| line.contains("(subagent)") || line.contains("(all)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentDriver;
    use crate::mode::Mode;
    use crate::state::{AgentKind, State};
    use std::os::unix::process::ExitStatusExt;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-global `PATH` (`set_var` is
    /// process-wide; `cargo test` runs tests in parallel). Copied verbatim
    /// from `pi.rs`'s test harness.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// A `State` value for `health`, which ignores it (`_state`) —
    /// constructed only to satisfy the trait signature.
    fn test_state() -> State {
        State::new(
            PhaseId::new(43),
            AgentKind::OpenCode,
            Mode::Auto,
            std::path::PathBuf::from("/tmp"),
        )
    }

    /// Writes an executable `opencode` stub into a fresh tempdir. The stub
    /// records its arguments (`"$@"`, one per line) to `args.txt` in the same
    /// dir, prints `body` to stdout via the `printf` shell BUILTIN (never an
    /// external `cat`/`echo` binary — the tests that exercise this stub set
    /// `PATH` to point ONLY at this tempdir, so any command the script needs
    /// beyond its own shebang-resolved `/bin/sh` must be a builtin, not a
    /// `$PATH`-resolved external program), and exits with `exit_code`. The
    /// returned tempdir is the only entry the test puts on `PATH`, so the
    /// operator's live `opencode` is never consulted. Modeled on `pi.rs`'s
    /// `stub_pi_on_path`, renamed for `opencode`; `body` is embedded directly
    /// into the script inside single quotes (it contains no single-quote
    /// characters) rather than passed as a shell format-string argument, so
    /// its ANSI escapes and box-drawing glyphs pass through byte-for-byte.
    fn stub_opencode_on_path(body: &str, exit_code: i32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("opencode");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\nprintf '%s' '{body}'\nexit {exit_code}\n",
            args = dir.path().join("args.txt").display(),
            body = body,
            exit_code = exit_code,
        );
        std::fs::write(&stub, script).expect("write opencode stub");
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
    /// hands the next test a mutated `PATH`. Copied verbatim from `pi.rs`.
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

    /// The real, live-verified `opencode providers list` output captured
    /// this session (43-RESEARCH.md Pattern 3) — three credentials from
    /// `auth.json`, three provider environment variables, ANSI SGR codes and
    /// box-drawing glyphs intact.
    const LIVE_PROVIDER_LIST_OUTPUT: &str = "\x1b[0m\n┌  Credentials \x1b[90m~/.local/share/opencode/auth.json\x1b[0m\n│\n●  Google \x1b[90mapi\x1b[0m\n│\n●  OpenAI \x1b[90moauth\x1b[0m\n│\n●  DeepSeek \x1b[90mapi\x1b[0m\n│\n└  3 credentials\n\n┌  Environment\n│\n●  DeepSeek \x1b[90mDEEPSEEK_API_KEY\x1b[0m\n│\n●  Google \x1b[90mGOOGLE_API_KEY\x1b[0m\n│\n●  OpenRouter \x1b[90mOPENROUTER_API_KEY\x1b[0m\n│\n└  3 environment variables\n";

    #[test]
    fn provider_count_sums_credentials_and_environment() {
        assert_eq!(
            opencode_configured_provider_count(LIVE_PROVIDER_LIST_OUTPUT),
            6
        );
    }

    /// SYNTHETIC (A1, P-05) — the real zero-credential shape of `opencode
    /// providers list` has never been observed live. These three fixtures
    /// cover the plausible shapes the RESEARCH doc leaves open; none is a
    /// captured real run.
    #[test]
    fn provider_count_is_zero_for_constructed_credentialless_output() {
        // Shape 1: both sections absent entirely.
        let absent = "No providers configured. Run `opencode auth login` to add one.\n";
        assert_eq!(opencode_configured_provider_count(absent), 0);

        // Shape 2: section header/footer present but with no items and no
        // visible numeric count.
        let empty_sections = "┌  Credentials\n│\n└\n\n┌  Environment\n│\n└\n";
        assert_eq!(opencode_configured_provider_count(empty_sections), 0);

        // Shape 3: explicit zero-count terminal lines.
        let explicit_zero =
            "┌  Credentials\n└  0 credentials\n\n┌  Environment\n└  0 environment variables\n";
        assert_eq!(opencode_configured_provider_count(explicit_zero), 0);
    }

    #[test]
    fn provider_count_ignores_bullet_provider_lines() {
        let stray_bullet = "●  Google api\n";
        assert_eq!(opencode_configured_provider_count(stray_bullet), 0);
    }

    #[test]
    fn strip_ansi_escapes_removes_sgr_and_preserves_box_glyphs() {
        let input = "\x1b[90m┌│●└\x1b[0m plain \x1b[1;31mtext\x1b[0m";
        let stripped = strip_ansi_escapes(input);
        assert_eq!(stripped, "┌│●└ plain text");
    }

    #[test]
    fn preflight_accepts_configured_credentials() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_opencode_on_path(LIVE_PROVIDER_LIST_OUTPUT, 0);
        let _path = PathGuard::set(stub_dir.path());

        OpenCodeDriver
            .health(&test_state())
            .expect("configured credentials must pass preflight");
    }

    /// SYNTHETIC (A1, P-05) — negative control proving exit code 0 alone
    /// does not green the check (T-43-09): the stub exits 0 (matching the
    /// live-verified always-0 exit code) but the stdout reports zero total
    /// credentials, so `health` must still refuse.
    #[test]
    fn preflight_rejects_constructed_zero_credential_output() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let zero_body =
            "┌  Credentials\n└  0 credentials\n\n┌  Environment\n└  0 environment variables\n";
        let stub_dir = stub_opencode_on_path(zero_body, 0);
        let _path = PathGuard::set(stub_dir.path());

        let err = OpenCodeDriver
            .health(&test_state())
            .expect_err("zero configured credentials must refuse preflight even with exit 0");
        assert!(err.contains("no OpenCode provider credential configured"));
    }

    #[test]
    fn preflight_rejects_when_probe_cannot_run() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let empty_dir = tempfile::tempdir().expect("create empty dir");
        let _path = PathGuard::set(empty_dir.path());

        let err = OpenCodeDriver
            .health(&test_state())
            .expect_err("missing opencode binary must fail closed, not panic");
        assert!(!err.is_empty());
    }

    #[test]
    fn health_error_leaks_no_provider_detail() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let body = "┌  Credentials ~/.local/share/opencode/auth.json\n└  0 credentials\n\n┌  Environment\n│\n●  Google GOOGLE_API_KEY (expired)\n│\n└  0 environment variables\n";
        let stub_dir = stub_opencode_on_path(body, 0);
        let _path = PathGuard::set(stub_dir.path());

        let err = OpenCodeDriver
            .health(&test_state())
            .expect_err("zero total credentials must refuse preflight");

        for leaked in ["auth.json", "GOOGLE_API_KEY", "Google", "expired"] {
            assert!(
                !err.contains(leaked),
                "health error must not leak `{leaked}`, got: {err}"
            );
        }
    }

    #[test]
    fn health_probe_argv_is_providers_list() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let stub_dir = stub_opencode_on_path(LIVE_PROVIDER_LIST_OUTPUT, 0);
        let _path = PathGuard::set(stub_dir.path());

        OpenCodeDriver
            .health(&test_state())
            .expect("configured credentials must pass health");

        let argv = std::fs::read_to_string(stub_dir.path().join("args.txt")).unwrap();
        assert_eq!(argv, "providers\nlist\n");
    }

    // --- Task 2: subagent-dispatch capability probe (spawn-free, mockable
    // `_with(output_fn)` form only — the stub-binary harness above stays
    // confined to `health`, where the spawn path itself is under test) ---

    fn mock_output(exit_code: i32, stdout: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(exit_code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn agent_list_baseline_reports_no_subagent() {
        let baseline = "build (primary)\n  [\n  {\n    \"permission\": \"*\",\n";
        assert!(!parse_opencode_agent_list_for_subagent(baseline));
    }

    #[test]
    fn agent_list_with_subagent_mode_reports_true() {
        let with_subagent =
            "build (primary)\n  [\n  {\n    \"permission\": \"*\",\n  reviewer (subagent)\n";
        assert!(parse_opencode_agent_list_for_subagent(with_subagent));
    }

    #[test]
    fn agent_list_with_all_mode_reports_true() {
        let with_all = "build (primary)\n  [\n  {\n    \"permission\": \"*\",\n  helper (all)\n";
        assert!(parse_opencode_agent_list_for_subagent(with_all));
    }

    #[test]
    fn subagent_probe_fails_closed_on_spawn_error() {
        let result = opencode_subagent_dispatch_available_with(|| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            ))
        });
        assert!(!result);
    }

    #[test]
    fn subagent_probe_fails_closed_on_nonzero_exit() {
        let result = opencode_subagent_dispatch_available_with(|| {
            Ok(mock_output(1, "reviewer (subagent)\n"))
        });
        assert!(
            !result,
            "a non-zero exit must fail closed even when stdout would otherwise classify true"
        );
    }

    #[test]
    fn subagent_probe_fails_closed_on_empty_output() {
        let result = opencode_subagent_dispatch_available_with(|| Ok(mock_output(0, "")));
        assert!(!result);
    }

    /// Mirrors `OpenCodeDriver::capabilities`'s exact wrapping
    /// (`super::DriverCapabilities { subagent_dispatch: ... }`) across every
    /// failure mode above, proving the probe's return type can never be a
    /// `Result` that would let it refuse a launch — a spawn error, a
    /// non-zero exit, and empty output all resolve to a valid
    /// `DriverCapabilities` value, never a panic or an early return.
    #[test]
    fn capabilities_never_refuses_a_launch() {
        type OutputFn = Box<dyn FnOnce() -> std::io::Result<std::process::Output>>;
        let cases: Vec<(&str, OutputFn)> = vec![
            (
                "spawn error",
                Box::new(|| {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "not found",
                    ))
                }),
            ),
            (
                "non-zero exit",
                Box::new(|| Ok(mock_output(1, "reviewer (subagent)\n"))),
            ),
            ("empty output", Box::new(|| Ok(mock_output(0, "")))),
        ];
        for (label, case) in cases {
            let caps = super::super::DriverCapabilities {
                subagent_dispatch: opencode_subagent_dispatch_available_with(case),
            };
            assert!(!caps.subagent_dispatch, "{label} must fail closed");
        }
    }
}

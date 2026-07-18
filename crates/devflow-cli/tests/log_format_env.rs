//! Regression guard for WR-01 (15-REVIEW.md, this round): when
//! `DEVFLOW_LOG_FORMAT=json`, the JSON tracing branch previously built its
//! subscriber with a hardcoded `LevelFilter::INFO` and never consulted
//! `RUST_LOG`, while the plain-text branch did. Fixed in commit `8672172`
//! (`crates/devflow-cli/src/main.rs`) by building an `EnvFilter` from
//! `RUST_LOG` on both branches.
//!
//! This test drives `devflow status` against a project whose only state is
//! a legacy `.devflow/state.json`. `workflow::migrate_legacy_state` emits a
//! `debug!("migrated legacy state.json to ...")` line as a side effect of
//! that migration, giving a real DEBUG-level log line to assert on — not
//! just "the binary didn't crash".
//!
//! Regression guard for CR-02 (15-REVIEW.md, third round): `lib.rs` documents
//! that "log output goes to stderr so stdout remains available for agent
//! output", but `main.rs` built both `tracing_subscriber::fmt()` branches
//! without a `.with_writer(...)` override, so log lines actually landed on
//! stdout. Fixed by adding `.with_writer(std::io::stderr)` to both branches.
//! Tests below now assert log lines on stderr and assert their absence from
//! stdout.

use devflow_core::gates::Gates;
use devflow_core::mode::Mode;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::path::PathBuf;
use std::process::Command;

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Set up a project dir with a legacy `.devflow/state.json` (phase 1, no
/// matching per-phase file), so `devflow status` triggers the migration's
/// `debug!` log line unconditionally.
fn project_with_legacy_state() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    let devflow_dir = dir.path().join(".devflow");
    std::fs::create_dir_all(&devflow_dir).expect("create .devflow dir");

    let state = State {
        stage: Stage::Code,
        phase: 1,
        agent: AgentKind::Claude,
        mode: Mode::Auto,
        gate_pending: false,
        consecutive_failures: 0,
        infra_failures: 0,
        started_at: "0".to_string(),
        project_root: dir.path().to_path_buf(),
        worktree_path: None,
    };
    let json = serde_json::to_string_pretty(&state).expect("serialize legacy state");
    std::fs::write(devflow_dir.join("state.json"), json).expect("write legacy state.json");

    dir
}

fn run_status(dir: &PathBuf, log_format: Option<&str>, rust_log: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(devflow_bin());
    cmd.arg("status").arg(dir);

    // Start from a clean env so ambient RUST_LOG/DEVFLOW_LOG_FORMAT from the
    // test runner's shell can't leak in and mask the behavior under test.
    cmd.env_remove("RUST_LOG");
    cmd.env_remove("DEVFLOW_LOG_FORMAT");
    if let Some(fmt) = log_format {
        cmd.env("DEVFLOW_LOG_FORMAT", fmt);
    }
    if let Some(level) = rust_log {
        cmd.env("RUST_LOG", level);
    }

    // `tracing_subscriber::fmt()` writes to stderr (main.rs sets
    // `.with_writer(std::io::stderr)` — CR-02 fix), separate from the CLI's
    // own `println!` output on stdout. Return both streams so callers can
    // assert log lines land on stderr and never leak onto stdout.
    let output = cmd.output().expect("run devflow status");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn rust_log_debug_is_honored_under_json_log_format() {
    let dir = project_with_legacy_state();
    let (stdout, stderr) = run_status(&dir.path().to_path_buf(), Some("json"), Some("debug"));

    assert!(
        stderr.contains("migrated legacy state.json"),
        "with DEVFLOW_LOG_FORMAT=json and RUST_LOG=debug, expected a DEBUG-level \
         tracing log to reach stderr (proving RUST_LOG is consulted on the json \
         branch, per WR-01 / commit 8672172), but it did not appear.\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("migrated legacy state.json"),
        "log lines must not leak onto stdout (CR-02) — stdout is reserved for \
         agent output and structured results per lib.rs's documented contract.\n\
         stdout:\n{stdout}"
    );
}

#[test]
fn rust_log_default_suppresses_debug_under_json_log_format() {
    let dir = project_with_legacy_state();
    // No RUST_LOG set — tracing-subscriber's EnvFilter::from_default_env()
    // defaults to ERROR-level-only when the env var is absent, so this DEBUG
    // line must NOT appear. If it does, RUST_LOG is being ignored (i.e. a
    // fixed, too-verbose level is hardcoded again), the exact WR-01 bug class.
    let (stdout, stderr) = run_status(&dir.path().to_path_buf(), Some("json"), None);

    assert!(
        !stderr.contains("migrated legacy state.json"),
        "with DEVFLOW_LOG_FORMAT=json and RUST_LOG unset, no DEBUG-level log \
         should reach stderr, but one did — RUST_LOG is not being consulted.\n\
         stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("migrated legacy state.json"),
        "log lines must never leak onto stdout (CR-02), regardless of level.\n\
         stdout:\n{stdout}"
    );
}

/// Regression guard for CR-01 (15-REVIEW.md, third round, commit `50db857`):
/// `main.rs` previously built its `EnvFilter` with the bare
/// `EnvFilter::from_default_env()`, which defaults to ERROR-only when
/// `RUST_LOG` is unset. Fixed by falling back to `EnvFilter::new("info")`
/// via `try_from_default_env().unwrap_or_else(...)` on both branches, so
/// `devflow` now logs at INFO level by default with no env var set at all.
///
/// This drives `devflow gate approve` against a project with a single open
/// gate — `Gates::respond` emits `info!("gate response written for phase
/// {phase} {stage}: approved=...")` as an unconditional side effect of a
/// successful approval, giving a real INFO-level log line to assert on. If
/// commit 50db857 were reverted (back to the bare `from_default_env()`),
/// this line would default to being suppressed and this test would fail.
fn project_with_open_gate(phase: u32, stage: Stage) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    Gates::write_gate(dir.path(), phase, stage, "test gate").expect("write gate");
    dir
}

#[test]
fn rust_log_unset_still_shows_info_level_logs_by_default() {
    let dir = project_with_open_gate(15, Stage::Ship);

    let mut cmd = Command::new(devflow_bin());
    cmd.arg("gate")
        .arg("approve")
        .arg("15")
        .arg("--project")
        .arg(dir.path());
    // No RUST_LOG, no DEVFLOW_LOG_FORMAT — exercise the true CLI default.
    cmd.env_remove("RUST_LOG");
    cmd.env_remove("DEVFLOW_LOG_FORMAT");

    let output = cmd.output().expect("run devflow gate approve");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "devflow gate approve should succeed against a single open gate\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("gate response written for phase 15 ship: approved=true"),
        "with RUST_LOG unset, an INFO-level tracing log should still reach stderr \
         by default (CR-01 / commit 50db857 — default level is INFO, not \
         ERROR-only), but it did not appear.\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("gate response written for phase 15 ship: approved=true"),
        "the tracing log line must not leak onto stdout (CR-02) — only the CLI's \
         own printed confirmation message belongs there.\nstdout:\n{stdout}"
    );
}

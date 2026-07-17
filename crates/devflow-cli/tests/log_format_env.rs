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
        started_at: "0".to_string(),
        project_root: dir.path().to_path_buf(),
        worktree_path: None,
    };
    let json = serde_json::to_string_pretty(&state).expect("serialize legacy state");
    std::fs::write(devflow_dir.join("state.json"), json).expect("write legacy state.json");

    dir
}

fn run_status(dir: &PathBuf, log_format: Option<&str>, rust_log: Option<&str>) -> String {
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

    // `tracing_subscriber::fmt()` writes to stdout by default (main.rs does
    // not override the writer), so tracing output and the CLI's own printed
    // output share stdout — assert against stdout, not stderr.
    let output = cmd.output().expect("run devflow status");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn rust_log_debug_is_honored_under_json_log_format() {
    let dir = project_with_legacy_state();
    let stdout = run_status(&dir.path().to_path_buf(), Some("json"), Some("debug"));

    assert!(
        stdout.contains("migrated legacy state.json"),
        "with DEVFLOW_LOG_FORMAT=json and RUST_LOG=debug, expected a DEBUG-level \
         tracing log to reach stdout (proving RUST_LOG is consulted on the json \
         branch, per WR-01 / commit 8672172), but it did not appear.\nstdout:\n{stdout}"
    );
}

#[test]
fn rust_log_default_suppresses_debug_under_json_log_format() {
    let dir = project_with_legacy_state();
    // No RUST_LOG set — tracing-subscriber's EnvFilter::from_default_env()
    // defaults to ERROR-level-only when the env var is absent, so this DEBUG
    // line must NOT appear. If it does, RUST_LOG is being ignored (i.e. a
    // fixed, too-verbose level is hardcoded again), the exact WR-01 bug class.
    let stdout = run_status(&dir.path().to_path_buf(), Some("json"), None);

    assert!(
        !stdout.contains("migrated legacy state.json"),
        "with DEVFLOW_LOG_FORMAT=json and RUST_LOG unset, no DEBUG-level log \
         should reach stdout, but one did — RUST_LOG is not being consulted.\n\
         stdout:\n{stdout}"
    );
}

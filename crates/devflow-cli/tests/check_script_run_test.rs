//! Behavioural tests for `scripts/check.sh test`'s failure accounting.
//!
//! `run_test` runs `cargo test --workspace --no-fail-fast` and then the
//! worktree-guard harness. Its own comment says one run must report everything
//! that is broken, but under `set -e` a failing cargo run ended the script before
//! the harness ran (external review, 2026-09-12; also observed in a real run).
//!
//! These tests **execute** a copy of the real `check.sh`, with `cargo` and the
//! harness replaced by stubs. The property is behaviour (which commands run, and
//! what the script exits with), which source inspection cannot settle. Running the
//! real target from a test would recurse the whole workspace suite (see
//! `ci_parity_guards.rs`); the stubs make it a hermetic, sub-second run.
//!
//! `both_green_reports_ok` is the negative control: without it, a `check.sh` that
//! always failed would satisfy the other two tests.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("chmod {}: {e}", path.display()));
}

/// Run a copy of `scripts/check.sh test` whose `cargo` exits `cargo_exit` and
/// whose harness exits `harness_exit`. Returns the output and whether the harness
/// stub ran.
fn run_check_test(cargo_exit: i32, harness_exit: i32) -> (Output, bool) {
    let sandbox = tempfile::tempdir().expect("create sandbox");
    let root = sandbox.path();
    fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
    fs::create_dir_all(root.join("bin")).expect("create stub bin dir");
    fs::copy(
        repo_root().join("scripts/check.sh"),
        root.join("scripts/check.sh"),
    )
    .expect("copy check.sh");

    let marker = root.join("harness-ran");
    write_executable(
        &root.join("scripts/test-phase-worktree-guard.sh"),
        &format!(
            "#!/usr/bin/env bash\ntouch '{}'\nexit {harness_exit}\n",
            marker.display()
        ),
    );
    write_executable(
        &root.join("bin/cargo"),
        &format!("#!/usr/bin/env bash\nexit {cargo_exit}\n"),
    );

    let path = format!(
        "{}:{}",
        root.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .args(["scripts/check.sh", "test"])
        .current_dir(root)
        .env("PATH", path)
        .output()
        .expect("run check.sh");
    let harness_ran = marker.exists();
    (output, harness_ran)
}

fn describe(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn cargo_failure_still_runs_the_harness_and_fails() {
    let (output, harness_ran) = run_check_test(101, 0);
    assert!(
        harness_ran,
        "a failing cargo test must not stop the harness from running\n{}",
        describe(&output)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "a failing cargo test must fail check.sh with cargo's own status\n{}",
        describe(&output)
    );
}

#[test]
fn harness_failure_fails_check_after_green_cargo() {
    let (output, harness_ran) = run_check_test(0, 1);
    assert!(harness_ran, "the harness must run\n{}", describe(&output));
    assert_eq!(
        output.status.code(),
        Some(1),
        "a failing harness must fail check.sh with the harness's status\n{}",
        describe(&output)
    );
}

#[test]
fn both_green_reports_ok() {
    let (output, harness_ran) = run_check_test(0, 0);
    assert!(harness_ran, "the harness must run\n{}", describe(&output));
    assert!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("==> check.sh: test OK"),
        "green cargo and harness must report OK\n{}",
        describe(&output)
    );
}

//! Integration tests for `devflow release --check` (20d) — the read-only
//! release-cut preflight. Drives the real binary against temp-workspace
//! fixtures rather than calling internal handlers directly (the checks are
//! `pub(crate)` inside `devflow-cli`, and driving the actual CLI is what
//! proves the `--check` gate and the self-pin comparison end-to-end).

use std::path::Path;
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

fn run_release(project: &Path, args: &[&str]) -> Output {
    Command::new(devflow_bin())
        .arg("release")
        .args(args)
        .arg(project)
        .output()
        .expect("spawn devflow release")
}

/// A workspace Cargo.toml whose `[workspace.dependencies]` self-pin either
/// matches or diverges from `[workspace.package] version`.
fn write_workspace_fixture(dir: &Path, package_version: &str, pin_version: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[workspace]\nmembers = [\"crates/devflow-core\"]\n\n\
             [workspace.package]\nversion = \"{package_version}\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = {{ path = \"crates/devflow-core\", version = \"{pin_version}\" }}\n"
        ),
    )
    .unwrap();
}

#[test]
fn release_check_passes_when_pins_match() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace_fixture(dir.path(), "1.7.0", "1.7.0");

    let output = run_release(dir.path(), &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected release --check to pass on matching pins, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("release preflight passed"),
        "expected a passing report, got: {stdout}"
    );
}

#[test]
fn release_check_flags_self_pin_drift() {
    let dir = tempfile::tempdir().unwrap();
    // The exact defect class 20a fixes: the workspace version moved to
    // 1.7.0, but the self-pin was left on the previous release's 1.6.0.
    write_workspace_fixture(dir.path(), "1.7.0", "1.6.0");

    let output = run_release(dir.path(), &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected release --check to fail on a drifted self-pin, got: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("1.6.0") && stdout.contains("1.7.0"),
        "expected the drifted pin (1.6.0) and the workspace version (1.7.0) both named in \
         the report, got: {stdout}"
    );
}

#[test]
fn release_without_check_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // No Cargo.toml needed — the bare-release rejection happens before any
    // check runs.
    let output = run_release(dir.path(), &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected bare `devflow release` (no --check) to be rejected, got success. stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("DEN-50"),
        "expected the rejection to name the deferred release-cut executor (DEN-50), got: {stderr}"
    );
    assert!(
        stderr.contains("--check"),
        "expected the rejection to mention --check, got: {stderr}"
    );
}

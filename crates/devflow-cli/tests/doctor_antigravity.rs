//! PATH-based `devflow doctor` antigravity-entry tests (phase 41 Task 7, F7).
//!
//! These live in the INTEGRATION suite, not the bin's unit tests, because a
//! unit-test harness's `env!("CARGO_BIN_EXE_devflow")` resolves to the
//! harness binary itself (spawning it with `doctor` re-enters the test suite —
//! infinite recursion). In an integration test the macro resolves to the real
//! `devflow` binary, and PATH is scoped to each child via `Command::env` so
//! no test-process-global environment mutation can poison parallel tests.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

fn write_stub_agy(root: &Path) -> std::path::PathBuf {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("agy"),
        "#!/bin/sh\necho 'antigravity-cli 9.9.9-test'\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(bin.join("agy")).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(bin.join("agy"), perms).unwrap();
    bin
}

/// F7: with a stubbed `agy` on the child's PATH, `devflow doctor` reports the
/// antigravity check present with the stub's version line.
#[test]
fn doctor_reports_antigravity_present_with_stub_on_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = write_stub_agy(root);

    let out = Command::new(devflow_bin())
        .arg("doctor")
        .arg(root)
        .env("PATH", &bin)
        .output()
        .expect("spawn devflow doctor");
    assert!(
        out.status.success(),
        "doctor must exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("antigravity") && text.contains("antigravity-cli 9.9.9-test"),
        "the stub version must be surfaced under the antigravity entry: {text}"
    );
}

/// F7: with `agy` absent from the child's PATH, the check reports
/// missing/warn — never a hard failure, never a false green.
#[test]
fn doctor_reports_antigravity_absent_without_agy() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let empty = root.join("empty-bin");
    std::fs::create_dir_all(&empty).unwrap();

    let out = Command::new(devflow_bin())
        .arg("doctor")
        .arg(root)
        .env("PATH", &empty)
        .output()
        .expect("spawn devflow doctor");
    assert!(
        out.status.success(),
        "doctor must never hard-fail on a missing agy: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    // Scope the assertion to the ANTIGRAVITY entry, not the whole output: an
    // empty PATH puts `✗` on many lines (git, cargo, gh, ...), so an unscoped
    // `contains("✗")` passes even if the antigravity entry falsely reported
    // present. The present-case test carries the positive discrimination.
    let antg_line = text
        .lines()
        .find(|l| l.trim_start().starts_with("antigravity"))
        .unwrap_or_else(|| panic!("antigravity entry missing from doctor output:\n{text}"));
    assert!(
        antg_line.contains("✗") || antg_line.contains("missing"),
        "absent agy must read missing/warn on the antigravity entry, never a false green: {antg_line}"
    );
}

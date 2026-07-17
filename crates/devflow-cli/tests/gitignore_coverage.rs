//! Regression guard for CR-01 (15-REVIEW.md): `.gitignore`'s `.devflow/`
//! patterns previously dropped coverage of legacy `state.json`,
//! `events.jsonl`, and `gates/` during a rewrite (commit `d021e3a`), leaking
//! runtime telemetry into git. Fixed again in commit `9b2fac4`. This test
//! asserts `git check-ignore` actually matches these paths, so a future
//! `.gitignore` rewrite can't silently regress the same coverage again.
//!
//! `git check-ignore` performs pure pattern matching — none of these paths
//! need to exist on disk for the check to be meaningful.

use std::path::PathBuf;
use std::process::Command;

/// Cargo test binaries run with cwd = the crate dir, but `.gitignore`
/// patterns here are anchored to the repo root, so `git check-ignore` must
/// be run from there for the paths to resolve correctly.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

#[test]
fn gitignore_covers_devflow_runtime_state_paths() {
    let output = Command::new("git")
        .current_dir(repo_root())
        .args([
            "check-ignore",
            "-v",
            ".devflow/state.json",
            ".devflow/events.jsonl",
            ".devflow/gates/probe.json",
        ])
        .output()
        .expect("run git check-ignore");

    assert!(
        output.status.success(),
        ".gitignore does not cover all required DevFlow runtime-state paths \
         (.devflow/state.json, .devflow/events.jsonl, .devflow/gates/) — \
         see 15-REVIEW.md CR-01.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

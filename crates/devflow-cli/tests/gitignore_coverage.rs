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

/// One representative path per `.devflow/` pattern in `.gitignore`. WR-07
/// (17-REVIEW.md): the guard previously asserted only three of these, so a
/// rewrite could have dropped raw agent stdout — the highest-value leak
/// surface, since it captures whatever an agent printed — and stayed green.
const RUNTIME_PATHS: &[&str] = &[
    ".devflow/state.json",
    ".devflow/state-07.json",
    ".devflow/lock-07",
    ".devflow/lock-project",
    ".devflow/phase-01-stdout",
    ".devflow/phase-01-stderr.log",
    ".devflow/phase-01-exit",
    ".devflow/phase-01-agent-pid",
    ".devflow/last-ship.json",
    ".devflow/cron-instructions.json",
    ".devflow/cron-instructions-01.json",
    ".devflow/events.jsonl",
    ".devflow/gates/probe.json",
    ".devflow/history/phase-01-state.json",
];

#[test]
fn gitignore_covers_devflow_runtime_state_paths() {
    // Checked one path per invocation rather than as a single argv:
    // `git check-ignore` exits 0 when ANY argument matches, so a batched
    // call would stay green while individual paths silently lost coverage —
    // precisely the regression this guard exists to catch.
    let mut unignored = Vec::new();
    for path in RUNTIME_PATHS {
        let output = Command::new("git")
            .current_dir(repo_root())
            .args(["check-ignore", "-q", path])
            .output()
            .expect("run git check-ignore");
        if !output.status.success() {
            unignored.push(*path);
        }
    }

    assert!(
        unignored.is_empty(),
        ".gitignore does not cover these DevFlow runtime-state paths, which \
         would leak runtime telemetry into git — see 15-REVIEW.md CR-01 and \
         17-REVIEW.md WR-07:\n  {}",
        unignored.join("\n  ")
    );
}

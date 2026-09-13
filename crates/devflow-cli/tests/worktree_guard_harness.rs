//! Workspace-only gate for the phase-worktree guard's test harness.
//!
//! `scripts/test-phase-worktree-guard.sh` exercises `scripts/lint-phase-worktree.sh`
//! in both directions. Both scripts are personal tooling on `workspace/denniyahh`
//! and never reach `develop`, so the harness cannot be wired into
//! `scripts/check.sh`: `develop` shares that script and does not have the harness.
//! Running it from this test makes every `cargo test --workspace` on a branch that
//! carries the harness fail when the harness does, `scripts/check.sh test` and the
//! pre-push container gate included, without touching a shared file. This file
//! stays out of every `develop` PR cut for the same reason.
//!
//! What a pass does NOT establish: that `scripts/hooks/pre-commit` still calls the
//! guard, or that the harness's cases cover every way a commit can stage
//! `crates/**`. It establishes that the harness ran, counted at least one check,
//! and counted no failures.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Cargo test binaries run with cwd = the crate dir; these files live at the
/// repo root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Trimmed, non-blank, non-comment lines. Guards that count occurrences must
/// use this: a bare search over the raw source counts prose, so documentation
/// mentioning a forbidden value would fail a guard about code.
fn code_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn describe(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The harness's final `passed=N failed=M` line as `(N, M)`. A missing or
/// malformed summary panics rather than reading as zero: a harness that died
/// before counting must never look like a clean run.
fn harness_summary(output: &Output) -> (u32, u32) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("passed="))
        .unwrap_or_else(|| {
            panic!(
                "harness printed no `passed=N failed=M` line\n{}",
                describe(output)
            )
        });
    let malformed = || panic!("malformed harness summary {line:?}\n{}", describe(output));
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [passed, failed] = fields.as_slice() else {
        malformed()
    };
    let count = |field: &str, prefix: &str| -> u32 {
        field
            .strip_prefix(prefix)
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| malformed())
    };
    (count(passed, "passed="), count(failed, "failed="))
}

#[test]
fn worktree_guard_harness_passes_with_checks_counted() {
    let output = Command::new("bash")
        .arg("scripts/test-phase-worktree-guard.sh")
        .current_dir(repo_root())
        .output()
        .expect("run scripts/test-phase-worktree-guard.sh");
    let (passed, failed) = harness_summary(&output);

    assert!(
        output.status.success() && failed == 0 && passed > 0,
        "worktree-guard harness must exit 0 with passed > 0 and failed = 0 \
         (got passed={passed} failed={failed})\n{}",
        describe(&output)
    );
}

/// External review, 2026-09-12 (finding C1). CI's Test job runs as root inside the
/// pinned image, on a checkout owned by the runner, and git accepts that checkout
/// only through the GLOBAL `safe.directory` entry the workflow adds. The
/// worktree-guard harness nulls global git config so its scratch fixtures cannot
/// inherit hooks, signing or excludes. Doing that before its one query against the
/// real checkout (the recorded-executable-mode check) made that query fail git's
/// ownership check, and the harness exited 128 with no output. That was reproduced
/// in the pinned image; the harness from before the change, and the changed
/// harness with matching ownership, both passed 13/0.
///
/// Source-asserting: the failure needs a root process against a checkout owned by
/// another user, which a test cannot arrange.
#[test]
fn worktree_guard_harness_queries_the_checkout_before_nulling_global_config() {
    let path = repo_root().join("scripts/test-phase-worktree-guard.sh");
    let script = read(&path);
    let lines = code_lines(&script);

    let checkout_query = lines
        .iter()
        .position(|l| l.contains("ls-files --stage"))
        .expect("the harness must still check recorded executable modes against the checkout");
    let null_global_config = lines
        .iter()
        .position(|l| l.contains("GIT_CONFIG_GLOBAL=/dev/null"))
        .expect("the harness must still null global git config for its fixtures");

    assert!(
        checkout_query < null_global_config,
        "{} nulls global git config (code line {null_global_config}) before querying the \
         real checkout (code line {checkout_query}); as root on a runner-owned checkout \
         that query then fails git's safe.directory ownership check",
        path.display()
    );
}

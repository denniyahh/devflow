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
        // Deliberately targets the real repository, so it must reach it via
        // `repo_root()` and not via an inherited GIT_DIR pointing elsewhere.
        let output = devflow_core::test_support::git_command(&repo_root())
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

/// D-14 (47-CONTEXT.md): insta's `.snap.new` scratch must be ignored, and the
/// committed `.snap` baseline must NOT be — the baseline is the reviewable
/// artifact, so a prompt wording change has to show up in the PR diff. One path
/// per invocation, as above.
///
/// The `.snap` half is the negative control. Without it, a `.gitignore` that
/// ignored every snapshot file — or everything — would pass the `.snap.new`
/// half alone.
///
/// Two details keep that control from passing vacuously:
/// * `--no-index`. `git check-ignore` never reports a TRACKED path as ignored,
///   and the baseline is tracked, so without it a `*.snap` rule would still read
///   as "not ignored" for the real baseline.
/// * Exact exit codes. `check-ignore -q` exits 0 for ignored, 1 for not ignored
///   and 128 on a git error; asserting `!success()` for the negative half would
///   read a failed git invocation as "not ignored".
#[test]
fn gitignore_ignores_snapshot_scratch_but_not_the_baseline() {
    const BASELINE: &str = "crates/devflow-core/src/snapshots/\
         devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap";
    let scratch = format!("{BASELINE}.new");

    let root = repo_root();
    let check_ignore_exit = |path: &str| {
        devflow_core::test_support::git_command(&root)
            .args(["check-ignore", "-q", "--no-index", path])
            .output()
            .expect("run git check-ignore")
            .status
            .code()
    };

    assert_eq!(
        check_ignore_exit(&scratch),
        Some(0),
        ".gitignore must ignore insta's `.snap.new` scratch file ({scratch}) — it \
         is written beside a missing or mismatched baseline and must never be \
         committed (47-CONTEXT.md D-14). Exit 1 = not ignored, 128 = git error."
    );
    assert_eq!(
        check_ignore_exit(BASELINE),
        Some(1),
        ".gitignore must NOT ignore the committed `.snap` baseline ({BASELINE}) — \
         the baseline is the reviewable artifact, and ignoring it hides every \
         prompt wording change from the PR diff (47-CONTEXT.md D-14). Exit 0 = \
         ignored, 128 = git error."
    );
}

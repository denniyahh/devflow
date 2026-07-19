//! Asserts `build.rs`'s compile-time provenance env vars (D-20, Phase 17d)
//! resolve at test-target compile time via `env!`. Tolerant of the
//! degraded no-git build case (`DEVFLOW_BUILD_COMMIT` may be empty) per
//! D-20 — absence of provenance is not staleness.
//!
//! `DEVFLOW_BUILD_TIMESTAMP` is intentionally NOT asserted here — it was
//! removed entirely (CR-02, 17-11 gap closure). See the regression test
//! below for why.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn build_dirty_is_exactly_true_or_false() {
    let dirty = env!("DEVFLOW_BUILD_DIRTY");
    assert!(
        dirty == "true" || dirty == "false",
        "DEVFLOW_BUILD_DIRTY must be exactly \"true\" or \"false\", got {dirty:?}"
    );
}

#[test]
fn build_commit_is_accessible_and_does_not_panic() {
    // May be empty in a no-git (crates.io) build per D-20 — accessing it
    // must never panic either way.
    let commit = env!("DEVFLOW_BUILD_COMMIT");
    let _ = commit.len();
}

// ---------------------------------------------------------------------------
// CR-02 regression (17-REVIEW.md; fixed 17-11).
//
// `build.rs` previously declared `rerun-if-changed` only for
// `HEAD`/`refs`/`packed-refs`, but reads `git status --porcelain` (the whole
// working tree) — an input neither of those paths fingerprints. Editing a
// tracked source file without moving a ref left the build script's cached
// `DEVFLOW_BUILD_DIRTY` output stale (`false`) even though the tree had
// just gone dirty. It was masked locally by accident (`.git/packed-refs`
// doesn't exist in an ordinary checkout, and cargo treats a missing
// rerun-if-changed path as "always rerun") but was exposed by any
// `git gc` or CI checkout (`actions/checkout@v4` packs refs).
//
// The fix makes `build.rs` always rerun. This test reproduces the
// reviewer's exact scenario end-to-end: a fresh, packed-refs checkout,
// built, edited, and built again — and asserts the build script's own
// CACHED output actually changed. Reading `env!(...)` from this test
// binary would only tell us what got embedded into ITS OWN compile; the
// only way to observe whether `build.rs` re-ran is to inspect the
// `cargo:rustc-env=...` lines cargo persists to
// `target/debug/build/devflow-<hash>/output` and only rewrites when the
// build script is actually invoked again.
// ---------------------------------------------------------------------------

/// The workspace root — `CARGO_MANIFEST_DIR` is `crates/devflow-cli`, two
/// levels below it.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/devflow-cli has a workspace root two levels up")
        .to_path_buf()
}

fn run(dir: &Path, program: &str, args: &[&str]) {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {program} {args:?}: {e}"));
    assert!(
        output.status.success(),
        "{program} {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Copy every git-tracked file's CURRENT on-disk content (which may include
/// uncommitted edits — this must exercise the working tree exactly as it
/// stands, not the last commit) from the real workspace into `dest`.
fn copy_tracked_worktree_into(dest: &Path) {
    let root = workspace_root();
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&root)
        .output()
        .expect("git ls-files");
    assert!(output.status.success(), "git ls-files failed");
    for rel in output.stdout.split(|&b| b == 0).filter(|s| !s.is_empty()) {
        let rel = std::str::from_utf8(rel).expect("tracked path is utf8");
        let src = root.join(rel);
        let dst = dest.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir for {rel}: {e}"));
        }
        std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {rel}: {e}"));
    }
}

/// Read the `devflow` package's cached build-script run directory (the one
/// holding `output`, as opposed to the sibling dir cargo uses to compile
/// the build script binary itself) and extract `DEVFLOW_BUILD_DIRTY`'s
/// current cached value.
fn read_cached_dirty_flag(target_dir: &Path) -> String {
    let build_dir = target_dir.join("debug/build");
    let entries = std::fs::read_dir(&build_dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", build_dir.display()));
    let output_path = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("devflow-") && !n.starts_with("devflow-core-"))
        })
        .map(|dir| dir.join("output"))
        .find(|candidate| candidate.exists())
        .unwrap_or_else(|| {
            panic!(
                "no devflow-*/output build-script cache found under {}",
                build_dir.display()
            )
        });
    let contents = std::fs::read_to_string(&output_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", output_path.display()));
    contents
        .lines()
        .find_map(|line| line.strip_prefix("cargo:rustc-env=DEVFLOW_BUILD_DIRTY="))
        .unwrap_or_else(|| panic!("no DEVFLOW_BUILD_DIRTY line in {}", output_path.display()))
        .to_string()
}

/// The reviewer's exact reproduction (17-REVIEW.md CR-02), reduced to a
/// self-contained fixture: a synthetic single-commit repo built from the
/// CURRENT working tree (so this test always exercises whatever `build.rs`
/// actually says right now, not a stale prior commit), with `packed-refs`
/// forced so the fixture matches a CI checkout rather than this dev
/// checkout's accidental local masking.
#[test]
fn build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild() {
    let clone_dir = tempfile::tempdir().expect("tempdir for synthetic checkout");
    let target_dir = tempfile::tempdir().expect("tempdir for target-dir");

    copy_tracked_worktree_into(clone_dir.path());

    run(clone_dir.path(), "git", &["init", "-q"]);
    run(
        clone_dir.path(),
        "git",
        &["config", "user.email", "cr02-repro@example.com"],
    );
    run(
        clone_dir.path(),
        "git",
        &["config", "user.name", "cr02-repro"],
    );
    run(
        clone_dir.path(),
        "git",
        &["config", "commit.gpgsign", "false"],
    );
    run(clone_dir.path(), "git", &["add", "-A"]);
    run(clone_dir.path(), "git", &["commit", "-q", "-m", "snapshot"]);
    // `packed-refs` is required to reproduce CR-02: it does not exist in an
    // ordinary local checkout (cargo treats a missing rerun-if-changed path
    // as always-rerun, which is exactly what masked this bug locally), but
    // does exist in any `actions/checkout@v4` CI clone.
    run(clone_dir.path(), "git", &["pack-refs", "--all"]);

    let target_dir_str = target_dir.path().to_str().expect("target dir is utf8");

    run(
        clone_dir.path(),
        "cargo",
        &["build", "-p", "devflow", "--target-dir", target_dir_str],
    );
    let before = read_cached_dirty_flag(target_dir.path());
    assert_eq!(
        before, "false",
        "a freshly snapshotted, committed tree's first build must be clean"
    );

    // Edit a tracked .rs file — dirties the working tree without touching
    // HEAD/refs/packed-refs, exactly the reviewer's reproduction.
    let edited = clone_dir.path().join("crates/devflow-cli/src/main.rs");
    let mut contents = std::fs::read_to_string(&edited).expect("read main.rs");
    contents.push_str("\n// CR-02 regression test edit\n");
    std::fs::write(&edited, contents).expect("write main.rs");

    run(
        clone_dir.path(),
        "cargo",
        &["build", "-p", "devflow", "--target-dir", target_dir_str],
    );
    let after = read_cached_dirty_flag(target_dir.path());
    assert_eq!(
        after, "true",
        "build.rs must re-run on every `cargo build` so DEVFLOW_BUILD_DIRTY \
         reflects the working tree's CURRENT state — if this reads \
         \"false\", build.rs's rerun trigger went stale again (CR-02)"
    );
}

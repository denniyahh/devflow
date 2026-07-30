//! What this file pins: 26-REVIEW.md **C-06** and 26-CONTEXT.md **D-13**.
//!
//! `project_root` walks *up* to the nearest `.devflow` ancestor. A phase
//! worktree (`.worktrees/phase-NN/`) has no `.devflow`; the parent checkout
//! does. Phase 26 newly routed `release --execute` and `sync` — the first
//! *irreversible* commands ever to use that resolver — through it, so a
//! maintainer running the release command from a phase worktree, which is this
//! project's ordinary working posture, cut a release from the **main
//! checkout's** branch, commits, and manifest.
//!
//! The compounding fact is what makes this Critical rather than merely
//! surprising: the executor's four entry guards (clean tree, on-develop,
//! has-remote, pre-gate) all test the **redirected** root, so a dirty worktree
//! beside a clean parent made the executor *more* likely to proceed, not less.
//! The guards are only meaningful once they run against the repository the
//! operator is standing in — which is exactly what these tests assert.
//!
//! Every git call goes through `devflow_core::test_support::git_command`
//! (999.37): an inherited `GIT_DIR` would retarget the very resolution under
//! test, and `release_execute.rs` is already flagged (W-13) for not scrubbing
//! it. That omission is not repeated here.

use std::path::Path;
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Run the real binary from `cwd` with an ISOLATED `HOME` and no inherited
/// SSH agent, mirroring `release_check.rs`'s runner — the signing-adjacent git
/// config resolution the release commands can reach otherwise reads the
/// operator's own `~/.gitconfig` and makes these tests machine-dependent.
fn run_devflow(cwd: &Path, args: &[&str]) -> Output {
    let isolated_home = tempfile::tempdir().unwrap();
    Command::new(devflow_bin())
        .args(args)
        .current_dir(cwd)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow")
}

fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
}

/// A workspace Cargo.toml whose `[workspace.dependencies]` self-pin equals
/// `[workspace.package] version`, so `release_execute`'s own pre-gate passes
/// and the test proves something about root resolution rather than about the
/// pre-gate. Kept local — these integration test files are deliberately
/// self-contained in this crate.
fn write_workspace_fixture(dir: &Path, version: &str) {
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[workspace]\nmembers = [\"crates/devflow-core\"]\n\n\
             [workspace.package]\nversion = \"{version}\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = {{ path = \"crates/devflow-core\", version = \"{version}\" }}\n"
        ),
    )
    .unwrap();
}

/// The exact topology 26-REVIEW.md verified on disk: a parent checkout that is
/// clean, on `develop`, and owns the only `.devflow`; and a **linked** worktree
/// (`git worktree add`) on a `feature/phase-NN` branch, dirty, with no
/// `.devflow` of its own. Two independent repositories would not reproduce
/// C-06 — the redirect depends on the worktree being a descendant path of a
/// checkout that carries the marker.
///
/// The parent deliberately has NO remote, so even a run that redirected all
/// the way through could not push anything anywhere.
fn worktree_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().to_path_buf();
    init_repo(&parent);

    // `.gitignore` mirrors this project's own: the worktree directory and the
    // pipeline state directory are ignored, which is what keeps the parent
    // checkout clean while a phase worktree lives inside it.
    std::fs::write(parent.join(".gitignore"), ".worktrees/\n.devflow/\n").unwrap();
    write_workspace_fixture(&parent, "1.0.0");
    git(&parent, &["add", "."]);
    git(&parent, &["commit", "-q", "-m", "initial"]);

    std::fs::create_dir_all(parent.join(".devflow")).unwrap();

    git(
        &parent,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature/phase-99",
            ".worktrees/phase-99",
        ],
    );
    let worktree = parent.join(".worktrees/phase-99");
    assert!(
        !worktree.join(".devflow").exists(),
        "the linked worktree must NOT carry its own .devflow — that is the C-06 topology"
    );

    // Make the worktree dirty. The parent stays clean, which is precisely the
    // shape that made the unfixed executor MORE likely to proceed.
    std::fs::write(worktree.join("uncommitted.txt"), "work in progress\n").unwrap();
    assert!(
        git_stdout(&parent, &["status", "--porcelain"]).is_empty(),
        "the parent checkout must be clean for this fixture to mean anything"
    );

    (dir, worktree)
}

/// C-06's own scenario. Asserting merely "exited non-zero" is INSUFFICIENT
/// here and is explicitly rejected: the unfixed binary ALSO exits non-zero in
/// this scenario — it refuses *after* redirecting, having checked the parent
/// (which is clean, on `develop`, and has no remote, so it refuses with
/// "no git remote configured"). Only an assertion about *which* repository the
/// refusal concerns can distinguish the fix from the defect (D-13's own note).
#[test]
fn release_execute_from_a_worktree_refuses_on_the_worktree_not_the_parent() {
    let (dir, worktree) = worktree_fixture();
    let parent = dir.path();

    let head_before = git_stdout(parent, &["rev-parse", "HEAD"]);

    let output = run_devflow(&worktree, &["release", "--execute", "--yes-release"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected a refusal, got success. stdout: {stdout}\nstderr: {stderr}"
    );
    // The worktree's OWN state is what the guard saw: it is the dirty one.
    assert!(
        stderr.contains("working tree is not clean"),
        "expected the refusal to be about the WORKTREE's own dirty tree, got: {stderr}"
    );
    // ...and it cannot have been about the parent, which is clean.
    assert!(
        git_stdout(parent, &["status", "--porcelain"]).is_empty(),
        "the parent checkout is clean, so a dirty-tree refusal can only be about the worktree"
    );
    assert!(
        !stderr.contains("no git remote configured"),
        "the unfixed binary redirected to the parent and refused on ITS missing remote — that \
         refusal must not appear: {stderr}"
    );

    assert_eq!(
        head_before,
        git_stdout(parent, &["rev-parse", "HEAD"]),
        "the parent checkout's HEAD must be byte-identical across the call"
    );
    assert!(
        git_stdout(parent, &["status", "--porcelain"]).is_empty(),
        "the parent checkout must still be clean"
    );
    assert!(
        git_stdout(parent, &["tag", "--list"]).is_empty(),
        "the parent checkout must have gained no tag"
    );
}

/// `devflow sync` is the other mutating command D-13 names. Same fixture,
/// same requirement: the refusal is about the worktree, and the parent is
/// provably untouched.
#[test]
fn sync_from_a_worktree_does_not_mutate_the_parent() {
    let (dir, worktree) = worktree_fixture();
    let parent = dir.path();

    let head_before = git_stdout(parent, &["rev-parse", "HEAD"]);

    let output = run_devflow(&worktree, &["sync"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected a refusal, got success. stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("working tree is not clean"),
        "expected sync to refuse on the WORKTREE's own dirty tree, got: {stderr}"
    );

    assert_eq!(
        head_before,
        git_stdout(parent, &["rev-parse", "HEAD"]),
        "the parent checkout's HEAD must be byte-identical across the call"
    );
    assert!(
        git_stdout(parent, &["status", "--porcelain"]).is_empty(),
        "the parent checkout must still be clean"
    );
    assert!(
        git_stdout(parent, &["tag", "--list"]).is_empty(),
        "the parent checkout must have gained no tag"
    );
}

/// A plain subdirectory of a repository: the resolved repository root differs
/// from the directory the operator named, so the mutating command refuses and
/// names BOTH paths plus both remedies (D-13).
#[test]
fn release_execute_from_a_subdirectory_names_both_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    write_workspace_fixture(root, "1.0.0");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial"]);

    let nested = root.join("crates/devflow-cli");
    std::fs::create_dir_all(&nested).unwrap();

    let output = run_devflow(root, &["release", "--execute", "--yes-release"])
        .status
        .success();
    assert!(
        !output,
        "sanity: the repository root fixture itself refuses"
    );

    let output = run_devflow(
        root,
        &[
            "release",
            "--execute",
            "--yes-release",
            nested.to_str().unwrap(),
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected a subdirectory to be refused, got success"
    );
    let canonical_nested = nested.canonicalize().unwrap();
    let canonical_root = root.canonicalize().unwrap();
    assert!(
        stderr.contains(&canonical_nested.display().to_string()),
        "the refusal must name the invoking path, got: {stderr}"
    );
    assert!(
        stderr.contains(&canonical_root.display().to_string()),
        "the refusal must name the resolved repository root, got: {stderr}"
    );
    assert!(
        stderr.contains("cd "),
        "the refusal must offer the `cd` remedy, got: {stderr}"
    );
    assert!(
        stderr.contains("[PROJECT]"),
        "the refusal must offer the explicit-target remedy, got: {stderr}"
    );
}

/// D-13's read-only carve-out, proven by behavior rather than asserted in
/// prose: `release --check` still walks up to the owning `.devflow` from a
/// subdirectory and runs its preflight. Asserts the preflight's own output,
/// not merely the exit status — `--check`'s exit status alone would also be
/// produced by other outcomes.
#[test]
fn release_check_from_a_subdirectory_still_walks_up() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    write_workspace_fixture(root, "1.7.0");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial"]);
    std::fs::create_dir_all(root.join(".devflow")).unwrap();

    let nested = root.join("crates/devflow-cli");
    std::fs::create_dir_all(&nested).unwrap();

    let output = run_devflow(root, &["release", "--check", nested.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "the read-only preflight must still run from a subdirectory. stdout: {stdout}\n\
         stderr: {stderr}"
    );
    assert!(
        stdout.contains("release preflight passed"),
        "expected the preflight's own report, got: {stdout}"
    );
    assert!(
        !stderr.contains("refusing to act on a repository you did not name"),
        "a read-only command must never hit the mutating resolver's refusal: {stderr}"
    );
}

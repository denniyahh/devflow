//! Integration tests for `devflow release --execute --yes-release` (26-07,
//! D-03/D-10/999.25) — the real release-cut executor's CLI surface. Drives
//! the real binary against temp-workspace fixtures, modeled on
//! `release_check.rs`: the checks and `release_execute` are `pub(crate)`
//! inside `devflow-cli`, so driving the actual CLI is what proves the
//! wiring end-to-end rather than merely compiling against it.

use std::path::Path;
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Runs `devflow release <args> <project>` with an ISOLATED `HOME` (a fresh
/// empty directory, no `.gitconfig`) and no inherited `SSH_AUTH_SOCK`/
/// `SSH_AGENT_PID` — mirrors `release_check.rs`'s `run_release`, since the
/// executor's pre-gate can reach signing-adjacent git config resolution
/// through the same machinery.
fn run_release(project: &Path, args: &[&str]) -> Output {
    let isolated_home = tempfile::tempdir().unwrap();
    Command::new(devflow_bin())
        .arg("release")
        .args(args)
        .arg(project)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release")
}

fn git(root: &Path, args: &[&str]) {
    // Hermetic: pinning cwd alone does not stop an inherited GIT_DIR from
    // retargeting the real repository (999.37).
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

/// A fixture repo checked out on `main` (deliberately NOT `develop`) with a
/// committed, self-pin-consistent workspace `Cargo.toml` — the pre-gate
/// (self-pin, publish order) must pass so the test proves the CLI reached
/// `execute_release` itself, not that it was rejected earlier by the
/// pre-gate.
fn off_develop_fixture(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "main"]);
    write_workspace_fixture(root, "1.0.0", "1.0.0");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial"]);
}

/// A workspace Cargo.toml whose `[workspace.dependencies]` self-pin equals
/// `[workspace.package] version` — matches `release_check.rs`'s helper of
/// the same name, kept local so this file has no cross-test-binary
/// dependency.
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

fn rev_parse(root: &Path, rev: &str) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(["rev-parse", rev])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn tag_list(root: &Path) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(["tag", "--list"])
        .output()
        .expect("git tag --list");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Task 1: proves the CLI actually reaches
/// `devflow_core::release::execute_release` rather than merely compiling
/// against it. The fixture's checkout is on `main`, not `develop`, so
/// `execute_release`'s own entry guard refuses before any mutation — and the
/// refusal wording (naming the actual current branch) can only come from
/// the core executor itself, never from the CLI's own pre-gate or
/// authorization checks.
#[test]
fn execute_reaches_the_core_executor_and_refuses_off_develop() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    off_develop_fixture(root);
    let before_head = rev_parse(root, "HEAD");

    let output = run_release(root, &["--execute", "--yes-release"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected the off-develop fixture to be refused, got success. stdout: {stdout}"
    );
    assert!(
        stderr.contains("main") && stderr.contains("develop"),
        "expected the executor's own refusal to name the current branch ('main') and \
         'develop', got: {stderr}"
    );

    let after_head = rev_parse(root, "HEAD");
    assert_eq!(
        before_head, after_head,
        "the fixture must gain no commit from a refused run"
    );
    assert!(
        tag_list(root).is_empty(),
        "the fixture must gain no tag from a refused run"
    );
}

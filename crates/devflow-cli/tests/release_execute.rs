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

/// A fixture repo checked out on `develop` with a committed,
/// self-pin-consistent workspace `Cargo.toml` — used by Task 2's
/// authorization-contract tests, none of which are expected to reach any
/// git-touching logic at all (the dispatch arm rejects before calling
/// `project_root` in every rejected branch), but a real fixture still lets
/// the "gained no commit and no tag" assertions mean something.
fn develop_fixture(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    write_workspace_fixture(root, "1.0.0", "1.0.0");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial"]);
}

/// Task 2 (B10, Truths 7/10): `--execute` without `--yes-release` is
/// rejected before any mutation, naming the authorization flag.
#[test]
fn execute_without_yes_release_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    develop_fixture(root);
    let before_head = rev_parse(root, "HEAD");

    let output = run_release(root, &["--execute"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected --execute without --yes-release to be rejected, got success"
    );
    assert!(
        stderr.contains("--yes-release"),
        "expected the rejection to name --yes-release, got: {stderr}"
    );
    assert_eq!(
        before_head,
        rev_parse(root, "HEAD"),
        "the fixture must gain no commit"
    );
    assert!(tag_list(root).is_empty(), "the fixture must gain no tag");
}

/// Task 2 (row B10) — the runtime contract, mirroring `preflight.rs`'s
/// `run_preflight_major_bump_gate_not_auto_approved_by_yes_ship` proof: the
/// authorization is withheld from the only channel that can supply it (a
/// `devflow.toml` authorization key AND plausibly-named environment
/// variables, both set true), and the run must still refuse. This is the
/// primary proof of B10 — `yes_release_has_no_config_state_or_env_surface`
/// below only supplements it, per
/// `.claude/skills/ai-change-acceptance/rules/test-signal-rejection.md`
/// § "not every source-grep is rejected".
#[test]
fn yes_release_is_not_settable_via_config_or_env() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    develop_fixture(root);
    let before_head = rev_parse(root, "HEAD");

    std::fs::write(root.join("devflow.toml"), "yes_release = true\n").unwrap();

    let isolated_home = tempfile::tempdir().unwrap();
    let output = Command::new(devflow_bin())
        .arg("release")
        .arg("--execute")
        .arg(root)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .env("DEVFLOW_YES_RELEASE", "true")
        .env("YES_RELEASE", "true")
        .output()
        .expect("spawn devflow release");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected the run to still be rejected with the authorization withheld from every \
         channel that could otherwise supply it, got success"
    );
    assert!(
        stderr.contains("--yes-release"),
        "expected the rejection to still name --yes-release, got: {stderr}"
    );
    assert_eq!(
        before_head,
        rev_parse(root, "HEAD"),
        "the fixture must gain no commit"
    );
    assert!(tag_list(root).is_empty(), "the fixture must gain no tag");
}

/// Task 2: `--check` and `--execute` together are rejected as mutually
/// exclusive; neither mode's work runs.
#[test]
fn check_and_execute_together_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    develop_fixture(root);

    let output = run_release(root, &["--check", "--execute"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected --check and --execute together to be rejected, got success"
    );
    assert!(
        stderr.contains("mutually exclusive"),
        "expected the rejection to state the two modes cannot be combined, got: {stderr}"
    );
}

/// Task 2 (Truth 7's behavioral evidence): a bare `devflow release` (no mode
/// flag) names both modes and no longer cites the deferred-executor
/// phrasing or the pre-plan backlog identifier. Asserts the ABSENCES
/// explicitly, so a future regression that restores the old rejection
/// fails this test rather than passing silently.
#[test]
fn bare_release_names_both_modes_and_no_deferred_executor() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    develop_fixture(root);

    let output = run_release(root, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected a bare `devflow release` to be rejected, got success"
    );
    assert!(
        stderr.contains("--check"),
        "expected the rejection to name --check, got: {stderr}"
    );
    assert!(
        stderr.contains("--execute"),
        "expected the rejection to name --execute, got: {stderr}"
    );
    assert!(
        !stderr.contains("not yet built"),
        "expected the OLD deferred-executor phrasing to be ABSENT, got: {stderr}"
    );
    assert!(
        !stderr.contains("DEN-50"),
        "expected the OLD backlog identifier to be ABSENT, got: {stderr}"
    );
}

/// Supplementary source-surface guard (B10) — documented as supplementary
/// to, never a substitute for, `yes_release_is_not_settable_via_config_or_env`
/// above, per
/// `.claude/skills/ai-change-acceptance/rules/test-signal-rejection.md`
/// § "not every source-grep is rejected": the property genuinely under test
/// is the absence of a surface, which a runtime test can only sample and a
/// source check can state exhaustively. All three files are outside this
/// plan's `files_modified`, so this guard is region-scoped to code the plan
/// never writes and cannot be self-invalidated by a pasted comment.
#[test]
fn yes_release_has_no_config_state_or_env_surface() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/devflow-cli has a workspace root two levels up");
    for relative in [
        "crates/devflow-core/src/state.rs",
        "crates/devflow-core/src/config.rs",
        "crates/devflow-cli/src/config_parse.rs",
    ] {
        let path = workspace_root.join(relative);
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        assert!(
            !contents.contains("yes_release"),
            "{relative} must not mention yes_release — it has no config/state/env surface (D-03)"
        );
    }
}

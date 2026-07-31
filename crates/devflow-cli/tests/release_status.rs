//! Integration tests for `devflow release status <version>` (29a) — the
//! read-only, end-to-end release-cut observer. Drives the real binary
//! against temp-workspace git fixtures rather than calling internal
//! handlers directly (same discipline as `release_check.rs`), and
//! separately proves the pre-existing `devflow release --check` contract is
//! untouched by this plan.

use std::path::Path;
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Runs `devflow release <args> <project>` with an ISOLATED `HOME` (a fresh
/// empty directory, no `.gitconfig`) and no inherited `SSH_AUTH_SOCK`/
/// `SSH_AGENT_PID` — matches `release_check.rs`'s isolation discipline so
/// these tests are deterministic regardless of the operator's global git
/// config.
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

/// `devflow release status <version> <project>` puts `project` as the
/// SECOND positional under the `status` subcommand, not as the outer
/// `Release` variant's own trailing positional — build the argv
/// accordingly rather than reusing [`run_release`]'s trailing-project
/// convention.
fn run_release_status(project: &Path, version: &str) -> Output {
    let isolated_home = tempfile::tempdir().unwrap();
    Command::new(devflow_bin())
        .arg("release")
        .arg("status")
        .arg(version)
        .arg(project)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release status")
}

/// As [`run_release_status`], but deliberately does NOT isolate `HOME` —
/// [`signed_tag_live_smoke`] needs the operator's real, authenticated `gh`
/// credentials, which an isolated `HOME` would hide.
fn run_release_status_live(project: &Path, version: &str) -> Output {
    Command::new(devflow_bin())
        .arg("release")
        .arg("status")
        .arg(version)
        .arg(project)
        .output()
        .expect("spawn devflow release status")
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

fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
}

/// A workspace `Cargo.toml` whose self-pin matches (used by the untouched
/// `--check` regression test below — copies `release_check.rs`'s fixture
/// shape rather than importing it, since these are separate test binaries).
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

/// `devflow release --check` (20d's legacy contract) must keep working
/// exactly as it did before this plan — same pass/fail behavior on a
/// matching self-pin fixture.
#[test]
fn release_check_still_passes_on_matching_pins() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace_fixture(dir.path(), "1.7.0", "1.7.0");

    let output = run_release(dir.path(), &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected release --check to still pass on matching pins, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("release preflight passed"),
        "expected a passing report, got: {stdout}"
    );
}

/// `devflow release status <version>` against a fixture repo with no
/// `origin` remote: `git ls-remote` fails, which must surface as
/// Unreachable (the `✗` icon, non-zero exit) — never as `Absent` (the `⚠`
/// icon), proving unreachable is not collapsed into absent end-to-end.
#[test]
fn release_status_no_remote_is_unreachable_not_absent() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = run_release_status(dir.path(), "1.2.3");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "expected a repo with no remote to exit non-zero, got success. stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains('✗'),
        "expected the failing (unreachable) icon on the signed-tag row, got: {stdout}"
    );
    assert!(
        !stdout.contains('⚠'),
        "a network/tool failure must never render as the absent (warn) icon, got: {stdout}"
    );
}

/// `devflow release status <version>` against a fixture whose `origin` IS
/// reachable (a local bare repo, no network needed) but genuinely carries
/// no such tag: a real negative answer from a reachable oracle must warn
/// and exit 0 — distinct from the no-remote case above, which must fail.
#[test]
fn release_status_absent_tag_on_reachable_remote_warns_and_exits_zero() {
    let origin_dir = tempfile::tempdir().unwrap();
    git(origin_dir.path(), &["init", "-q", "--bare"]);

    let work_dir = tempfile::tempdir().unwrap();
    init_repo(work_dir.path());
    std::fs::write(work_dir.path().join("file.txt"), "hello").unwrap();
    git(work_dir.path(), &["add", "."]);
    git(work_dir.path(), &["commit", "-q", "-m", "init"]);
    git(
        work_dir.path(),
        &[
            "remote",
            "add",
            "origin",
            origin_dir.path().to_str().expect("utf-8 tempdir path"),
        ],
    );
    git(work_dir.path(), &["push", "-q", "origin", "develop"]);

    let output = run_release_status(work_dir.path(), "1.2.3");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected a reachable remote with no matching tag to exit 0, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains('⚠'),
        "expected the absent (warn) icon for a genuinely missing tag, got: {stdout}"
    );
    assert!(
        !stdout.contains('✗'),
        "a reachable oracle's real negative answer must never render as the unreachable (fail) icon, got: {stdout}"
    );
}

/// The trailing summary line names the requested version and the observed
/// count — not just the per-row output above it.
#[test]
fn release_status_summary_line_names_version_and_count() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = run_release_status(dir.path(), "9.9.9");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("9.9.9:"),
        "expected the summary line to name the requested version, got: {stdout}"
    );
    assert!(
        stdout.contains("0/1"),
        "expected the summary line to report 0 of 1 question observed complete \
         (the no-remote fixture makes the sole wired question Unreachable, not \
         Present), got: {stdout}"
    );
}

/// Real end-to-end run against this repository's own real origin. `v2.1.0`
/// is a historical fact that cannot change; `999.999.999` will never exist
/// — asserting on `2.2.0`'s absence would become a false failure the moment
/// someone creates that tag. Requires network reachability to github.com
/// and an authenticated `gh` (see this test's own precondition check).
#[test]
#[ignore]
fn signed_tag_live_smoke() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/devflow-cli has a workspace root two levels up")
        .to_path_buf();

    // Reachability against this repo's own real `origin`, not a bare
    // `https://github.com` (which has no repository at its root and would
    // fail `ls-remote` regardless of network state).
    let network_ok = devflow_core::test_support::git_command(&repo_root)
        .args(["ls-remote", "--exit-code", "origin", "HEAD"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    assert!(
        network_ok,
        "signed_tag_live_smoke requires network reachability to origin (github.com) — real failure, not skipped"
    );
    let gh_auth_ok = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    assert!(
        gh_auth_ok,
        "signed_tag_live_smoke requires an authenticated `gh` (`gh auth status` must exit 0) — real failure, not skipped"
    );

    let present = run_release_status_live(&repo_root, "2.1.0");
    let present_stdout = String::from_utf8_lossy(&present.stdout);
    assert!(
        present.status.success(),
        "expected v2.1.0's signed tag to observe Present, got: {present_stdout}\nstderr: {}",
        String::from_utf8_lossy(&present.stderr)
    );
    assert!(
        present_stdout.contains('✓'),
        "expected the present icon for v2.1.0, got: {present_stdout}"
    );

    let absent = run_release_status_live(&repo_root, "999.999.999");
    let absent_stdout = String::from_utf8_lossy(&absent.stdout);
    assert!(
        absent.status.success(),
        "expected v999.999.999's absence to still exit 0 (Absent, not Unreachable), got: {absent_stdout}\nstderr: {}",
        String::from_utf8_lossy(&absent.stderr)
    );
    assert!(
        absent_stdout.contains('⚠'),
        "expected the absent icon for v999.999.999, got: {absent_stdout}"
    );
}

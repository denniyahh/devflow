//! Integration tests for `devflow release status <version>` (29a) — the
//! read-only, end-to-end release-cut observer. Drives the real binary
//! against temp-workspace git fixtures rather than calling internal
//! handlers directly (same discipline as `release_check.rs`), and
//! separately proves the pre-existing `devflow release --check` contract is
//! untouched by this plan.
//!
//! Live-remote behavior — whether GitHub's compare and contents endpoints
//! keep their documented semantics — is deliberately NOT covered by this
//! hermetic suite. That coverage lives in the `#[ignore]`-gated smoke tests
//! below (`signed_tag_live_smoke`, `crates_published_live_smoke`) and in
//! `29-VALIDATION.md`'s Manual-Only Verifications table. This is a recorded
//! boundary, not a coverage gap: a hermetic fixture cannot observe whether a
//! third-party API's contract has drifted.

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

fn commit(root: &Path, name: &str) {
    std::fs::write(root.join(name), name).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", &format!("add {name}")]);
}

fn rev_parse(root: &Path, rev: &str) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(["rev-parse", rev])
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_status_porcelain(root: &Path) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(["status", "--porcelain"])
        .output()
        .expect("git status");
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Sorted `git for-each-ref` listing — changes if any fetch, tag creation,
/// or ref update occurs, regardless of what kind.
fn sorted_ref_listing(root: &Path) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .expect("git for-each-ref");
    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    lines.sort();
    lines.join("\n")
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

/// Returns the status icon (`✓`/`⚠`/`✗`) printed on the row whose name
/// contains `row_name`, if any — lets a test assert on one specific
/// observation's outcome without being coupled to how many OTHER
/// observations `release status` also happens to print.
fn row_icon(stdout: &str, row_name: &str) -> Option<char> {
    stdout
        .lines()
        .find(|line| line.contains(row_name))
        .and_then(|line| line.chars().find(|c| "✓⚠✗".contains(*c)))
}

/// `devflow release status <version>` against a fixture whose `origin` IS
/// reachable (a local bare repo, no network needed) but genuinely carries
/// no such tag: a real negative answer from a reachable oracle must warn on
/// that row — distinct from the no-remote case above, which must fail on
/// it. This fixture has no `Cargo.toml`, so the (unrelated) crates-published
/// row independently reports Unreachable — asserted on its own row, not
/// conflated with the signed-tag row this test targets.
#[test]
fn release_status_absent_tag_on_reachable_remote_warns() {
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
    assert_eq!(
        row_icon(&stdout, "signed tag"),
        Some('⚠'),
        "expected the absent (warn) icon on the signed-tag row for a genuinely \
         missing tag on a reachable remote, got: {stdout}"
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
        stdout.contains("0/6"),
        "expected the summary line to report 0 of 6 questions observed complete \
         (this fixture has no remote and no Cargo.toml, so all six rows — \
         version bumped, changelog written, release PR merged, signed tag, \
         sync merged, crates published — are Unreachable, not Present), got: {stdout}"
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

/// Real end-to-end run against the real crates.io registry — the one piece
/// of this phase with no analog anywhere else in the codebase.
/// `devflow-core@2.2.0` and `devflow-core@999.999.999` are the same two
/// live-verified probes `29-RESEARCH.md` ran, chosen because both answers
/// are stable facts. Only network reachability to crates.io is required
/// (no `gh` credential involved), so this uses the isolated-`HOME` runner
/// like the deterministic tests above.
#[test]
#[ignore]
fn crates_published_live_smoke() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/devflow-cli has a workspace root two levels up")
        .to_path_buf();

    let network_ok = Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "20",
            "https://crates.io",
        ])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    assert!(
        network_ok,
        "crates_published_live_smoke requires network reachability to crates.io — real failure, not skipped"
    );

    let present = run_release_status(&repo_root, "2.2.0");
    let present_stdout = String::from_utf8_lossy(&present.stdout);
    assert!(
        present_stdout.contains("devflow-core") && present_stdout.contains('✓'),
        "expected devflow-core@2.2.0 to classify Present against the real registry, got: {present_stdout}"
    );

    let absent = run_release_status(&repo_root, "999.999.999");
    let absent_stdout = String::from_utf8_lossy(&absent.stdout);
    assert!(
        absent_stdout.contains("devflow-core") && absent_stdout.contains('⚠'),
        "expected devflow-core@999.999.999 to classify Absent against the real registry, got: {absent_stdout}"
    );
}

// -- 29-02 Task 3: invariants that make unit 29a shippable on its own -----

/// `devflow release status` mutates NOTHING: HEAD, the working tree, the
/// index, the full ref listing (no fetch, no new tag), and every file under
/// `.devflow/`/`devflow.toml` are all byte-identical before and after a run.
/// This is the executable form of RD-8's "state is derived, never
/// recorded" — the assertion goes red the instant any future change
/// introduces a progress file, a cache, or a fetch.
#[test]
fn status_leaves_the_repository_untouched() {
    // A real `origin` remote (a local bare repo) is required so that a
    // regression reintroducing a `git fetch` would be observable in the ref
    // listing below (creating `refs/remotes/origin/*`) — a fixture with no
    // remote at all would make a `fetch` a silent no-op, defeating the
    // mutation-testing check this test exists to satisfy.
    let origin_dir = tempfile::tempdir().unwrap();
    git(origin_dir.path(), &["init", "-q", "--bare"]);

    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit(dir.path(), "README.md");
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            origin_dir.path().to_str().expect("utf-8 tempdir path"),
        ],
    );
    git(dir.path(), &["push", "-q", "origin", "develop"]);

    std::fs::write(dir.path().join("devflow.toml"), "# devflow config\n").unwrap();
    std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
    std::fs::write(
        dir.path().join(".devflow").join("state.json"),
        "{\"stage\":\"ship\"}",
    )
    .unwrap();

    // A fetch always writes `.git/FETCH_HEAD`, regardless of whether it
    // introduces any new ref — this catches a regression even in a fixture
    // shape where the fetched content happens to match what's already known
    // locally (where `for-each-ref` alone would not change).
    let fetch_head_path = dir.path().join(".git").join("FETCH_HEAD");

    let before_head = rev_parse(dir.path(), "HEAD");
    let before_status = git_status_porcelain(dir.path());
    let before_refs = sorted_ref_listing(dir.path());
    let before_fetch_head_exists = fetch_head_path.exists();
    let before_devflow_toml = std::fs::read(dir.path().join("devflow.toml")).unwrap();
    let before_state_json = std::fs::read(dir.path().join(".devflow").join("state.json")).unwrap();
    let before_devflow_entries: Vec<_> = std::fs::read_dir(dir.path().join(".devflow"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    let _ = run_release_status(dir.path(), "1.2.3");

    let after_head = rev_parse(dir.path(), "HEAD");
    let after_status = git_status_porcelain(dir.path());
    let after_refs = sorted_ref_listing(dir.path());
    let after_fetch_head_exists = fetch_head_path.exists();
    let after_devflow_toml = std::fs::read(dir.path().join("devflow.toml")).unwrap();
    let after_state_json = std::fs::read(dir.path().join(".devflow").join("state.json")).unwrap();
    let after_devflow_entries: Vec<_> = std::fs::read_dir(dir.path().join(".devflow"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    assert_eq!(before_head, after_head, "HEAD must not move");
    assert_eq!(before_status, after_status, "working tree must not change");
    assert_eq!(
        before_refs, after_refs,
        "ref listing must not change — no fetch, no new tag, no remote-tracking ref"
    );
    assert_eq!(
        before_fetch_head_exists, after_fetch_head_exists,
        "no fetch was ever expected — FETCH_HEAD must not appear"
    );
    assert!(
        !after_fetch_head_exists,
        "FETCH_HEAD must never exist after a release status run — a fetch occurred"
    );
    assert_eq!(
        before_devflow_toml, after_devflow_toml,
        "devflow.toml must be byte-identical"
    );
    assert_eq!(
        before_state_json, after_state_json,
        "every file under .devflow/ must be byte-identical"
    );
    assert_eq!(
        before_devflow_entries.len(),
        after_devflow_entries.len(),
        "no new file must appear under .devflow/"
    );
}

/// Observation grants no mandate and must not accept one — a flag accepted
/// here would be the first step toward observation implying permission.
#[test]
fn status_rejects_an_authorization_flag() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let isolated_home = tempfile::tempdir().unwrap();
    let output = Command::new(devflow_bin())
        .arg("release")
        .arg("status")
        .arg("1.2.3")
        .arg("--yes-release")
        .arg(dir.path())
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release status --yes-release");

    assert!(
        !output.status.success(),
        "expected --yes-release to be rejected by argument parsing, got success. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// With no `origin` configured at all, every one of the six questions is
/// unanswerable — asserts both the non-zero exit AND the absence of any `⚠`
/// row. Any absent-shaped row in this scenario would be exactly the
/// `unreachable != absent` collapse this phase exists to prevent.
#[test]
fn status_reports_unreachable_not_absent_without_a_remote() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = run_release_status(dir.path(), "1.2.3");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "expected a repo with no remote at all to exit non-zero, got success. stdout: {stdout}"
    );
    assert!(
        stdout.contains('✗'),
        "expected at least one unreachable row, got: {stdout}"
    );
    assert!(
        !stdout.contains('⚠'),
        "with no remote at all, every question is unanswerable — any absent-shaped \
         row would be the unreachable != absent collapse this phase exists to prevent, got: {stdout}"
    );
}

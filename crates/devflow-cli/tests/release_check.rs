//! Integration tests for `devflow release --check` (20d) — the read-only
//! release-cut preflight. Drives the real binary against temp-workspace
//! fixtures rather than calling internal handlers directly (the checks are
//! `pub(crate)` inside `devflow-cli`, and driving the actual CLI is what
//! proves the `--check` gate and the self-pin comparison end-to-end).

use std::path::Path;
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Runs `devflow release <args> <project>` with an ISOLATED `HOME` (a fresh
/// empty directory, no `.gitconfig`) and no inherited `SSH_AUTH_SOCK`/
/// `SSH_AGENT_PID` — the signing-viability check (Task 3) reads
/// `git config gpg.format`/`user.signingkey`, which git resolves through
/// the OPERATOR's global `~/.gitconfig` even inside a throwaway fixture
/// repo. Without this isolation, these tests would be non-deterministic on
/// any machine whose global config sets `gpg.format=ssh` (this project's
/// own dev machine does — the exact Pattern 4 research finding).
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
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
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

fn commit(root: &Path, name: &str) {
    std::fs::write(root.join(name), name).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", &format!("add {name}")]);
}

fn rev_parse(root: &Path, rev: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(root)
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A workspace Cargo.toml whose `[workspace.dependencies]` self-pin either
/// matches or diverges from `[workspace.package] version`.
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

#[test]
fn release_check_passes_when_pins_match() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace_fixture(dir.path(), "1.7.0", "1.7.0");

    let output = run_release(dir.path(), &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected release --check to pass on matching pins, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("release preflight passed"),
        "expected a passing report, got: {stdout}"
    );
}

#[test]
fn release_check_flags_self_pin_drift() {
    let dir = tempfile::tempdir().unwrap();
    // The exact defect class 20a fixes: the workspace version moved to
    // 1.7.0, but the self-pin was left on the previous release's 1.6.0.
    write_workspace_fixture(dir.path(), "1.7.0", "1.6.0");

    let output = run_release(dir.path(), &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected release --check to fail on a drifted self-pin, got: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("1.6.0") && stdout.contains("1.7.0"),
        "expected the drifted pin (1.6.0) and the workspace version (1.7.0) both named in \
         the report, got: {stdout}"
    );
}

#[test]
fn release_without_check_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // No Cargo.toml needed — the bare-release rejection happens before any
    // check runs.
    let output = run_release(dir.path(), &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected bare `devflow release` (no --check) to be rejected, got success. stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("DEN-50"),
        "expected the rejection to name the deferred release-cut executor (DEN-50), got: {stderr}"
    );
    assert!(
        stderr.contains("--check"),
        "expected the rejection to mention --check, got: {stderr}"
    );
}

/// Task 2: `origin/main` resolves locally but is NOT an ancestor of
/// `HEAD` — develop has diverged and `scripts/sync-main-to-develop.sh`
/// should be run before the next release PR. No `git fetch` is issued;
/// `refs/remotes/origin/main` is set directly to simulate an
/// already-fetched-but-diverged remote-tracking ref.
#[test]
fn release_check_reports_divergence_when_main_not_ancestor() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "base.txt");

    // Simulate origin/main at a commit that is NOT reachable from the
    // current branch — a sibling line of history, not an ancestor.
    git(root, &["checkout", "-q", "-b", "main-line"]);
    commit(root, "main-only.txt");
    let main_sha = rev_parse(root, "HEAD");
    git(root, &["update-ref", "refs/remotes/origin/main", &main_sha]);

    git(root, &["checkout", "-q", "develop"]);
    commit(root, "develop-only.txt");

    let output = run_release(root, &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected release --check to fail on a real divergence, got: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("sync-main-to-develop.sh"),
        "expected the divergence failure to name the sync script, got: {stdout}"
    );
}

/// Task 2 edge-probe (20d/empty, network independence): `origin/main` was
/// never fetched at all — no remote-tracking ref exists locally. The check
/// must degrade to an actionable message, never crash and never issue an
/// implicit `git fetch`.
#[test]
fn release_check_divergence_degrades_when_origin_main_absent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "base.txt");
    // No `refs/remotes/origin/main` created at all.

    let output = run_release(root, &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("origin/main not fetched"),
        "expected an actionable 'origin/main not fetched' message, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("git fetch"),
        "expected the degrade message to direct the operator to `git fetch`, got: {stdout}"
    );
}

/// Task 2: the publish-order check states `devflow-core` before `devflow`
/// (path dependency `devflow` -> `devflow-core`), derived from the
/// workspace's own `[workspace] members` list and each member's own
/// `[dependencies]` section — never a hardcoded prose string.
#[test]
fn release_check_states_publish_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\n    \"crates/devflow-core\",\n    \"crates/devflow-cli\",\n]\n\n\
         [workspace.package]\nversion = \"1.0.0\"\nedition = \"2024\"\n\n\
         [workspace.dependencies]\n\
         devflow-core = { path = \"crates/devflow-core\", version = \"1.0.0\" }\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("crates/devflow-core")).unwrap();
    std::fs::write(
        root.join("crates/devflow-core/Cargo.toml"),
        "[package]\nname = \"devflow-core\"\nversion.workspace = true\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("crates/devflow-cli")).unwrap();
    std::fs::write(
        root.join("crates/devflow-cli/Cargo.toml"),
        "[package]\nname = \"devflow\"\nversion.workspace = true\n\n\
         [dependencies]\ndevflow-core.workspace = true\n",
    )
    .unwrap();

    let output = run_release(root, &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("devflow-core -> devflow") && !stdout.contains("devflow -> devflow-core"),
        "expected the publish order to state devflow-core before devflow, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Task 3 (T-20-04, ASVS V6 / WR-02): the signing-viability check's
/// rendered output must never contain private-key material or a full
/// filesystem path — a REAL disposable ed25519 keypair (private + public)
/// is written directly inside the fixture, proving the check never echoes
/// either, regardless of what's sitting alongside the public key on disk.
/// `SSH_AUTH_SOCK` is removed (via `run_release`'s isolation) so this
/// resolves deterministically to the "no ssh-agent reachable" branch
/// rather than depending on any locally running agent.
#[test]
fn release_check_signing_output_leaks_no_key_material_or_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "base.txt");
    git(root, &["config", "gpg.format", "ssh"]);

    let key_path = root.join("release-signing-key");
    let keygen = Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-f",
            key_path.to_str().unwrap(),
            "-N",
            "",
            "-q",
        ])
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen fixture setup failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );
    let pub_key_path = root.join("release-signing-key.pub");
    git(
        root,
        &["config", "user.signingkey", pub_key_path.to_str().unwrap()],
    );

    let output = run_release(root, &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("PRIVATE KEY"),
        "signing check output must never contain private key material, got: {stdout}"
    );
    assert!(
        !stdout.contains(root.to_str().unwrap()),
        "signing check output must never contain a full filesystem path, got: {stdout}"
    );
    assert!(
        !stdout.contains("panicked"),
        "signing check must never panic, got: {stdout}"
    );
}

/// A `PATH` containing ONLY a symlink to the real `git` binary — unlike a
/// bare directory restriction (e.g. `/usr/bin`), this guarantees `ssh-add`/
/// `ssh-keygen` are genuinely absent regardless of the host (some distros,
/// including this one, ship both alongside `git` in `/usr/bin`).
fn git_only_path() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let which = Command::new("which")
        .arg("git")
        .output()
        .expect("locate git via `which`");
    assert!(which.status.success(), "`which git` failed");
    let real_git = String::from_utf8_lossy(&which.stdout).trim().to_string();
    std::os::unix::fs::symlink(real_git, dir.path().join("git"))
        .expect("symlink git into the minimal PATH fixture");
    dir
}

/// Task 3 fail-soft edge (20d/empty): `ssh-add` itself is unavailable. The
/// check must degrade to an actionable message, never crash.
#[test]
fn release_check_signing_degrades_when_ssh_add_absent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "base.txt");
    git(root, &["config", "gpg.format", "ssh"]);
    let key_path = root.join("fake-signing-key.pub");
    std::fs::write(&key_path, "ssh-ed25519 AAAAfixture placeholder\n").unwrap();
    git(
        root,
        &["config", "user.signingkey", key_path.to_str().unwrap()],
    );

    let isolated_home = tempfile::tempdir().unwrap();
    let path_dir = git_only_path();
    let output = Command::new(devflow_bin())
        .arg("release")
        .arg("--check")
        .arg(root)
        .env("HOME", isolated_home.path())
        .env("PATH", path_dir.path())
        .output()
        .expect("spawn devflow release");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("ssh-add not found"),
        "expected a fail-soft 'tool not found' message, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !stdout.contains("panicked"),
        "must not panic, got: {stdout}"
    );
}

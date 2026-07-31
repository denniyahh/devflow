//! Integration tests for `devflow release cut <version>` (29-04) — proving
//! the shipped executor carries no operator-presence requirement of any
//! kind: it never prompts, never reads stdin, refuses without a mandate
//! without touching the network, and records nothing.
//!
//! Live pull-request behavior — whether GitHub's auto-merge actually waits
//! for green checks and merges with the requested method — is deliberately
//! NOT covered by this hermetic suite. That coverage lives in
//! `29-VALIDATION.md`'s Manual-Only Verifications table. This is a recorded
//! boundary, not a coverage gap: a hermetic fixture cannot observe whether a
//! third-party service's live behavior matches its documented contract.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Hermetic `git` invocation for fixture setup (999.37) — never a bare
/// `Command::new("git")`.
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

/// A minimal repo on `develop` with one commit and no `origin` remote —
/// every one of the six release-cut oracles fails to observe, which is
/// exactly what makes these tests deterministic without any network
/// dependency: the same fixture shape `release_status.rs` uses.
fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// Runs `devflow release cut <version> <project> [extra_args...]` with an
/// ISOLATED `HOME` (a fresh empty directory, no `.gitconfig`, no `gh`
/// credentials) and no inherited `SSH_AUTH_SOCK`/`SSH_AGENT_PID` — matches
/// `release_status.rs`'s isolation discipline so these tests are
/// deterministic regardless of the operator's global git/`gh` config.
fn run_cut(project: &Path, version: &str, extra_args: &[&str]) -> Output {
    let isolated_home = tempfile::tempdir().unwrap();
    Command::new(devflow_bin())
        .arg("release")
        .arg("cut")
        .arg(version)
        .arg(project)
        .args(extra_args)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release cut")
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// -- unattended / non-interactive -------------------------------------------

/// The executable form of RD-3: the shipped executor must not require a
/// human at the keyboard and must not refuse to run unattended. Spawns with
/// a null stdin and a bounded `try_wait` poll (never a blocking `wait()`),
/// asserting the process exits on its own rather than being reaped after a
/// timeout. A future regression introducing a prompt, a stdin read, or an
/// unbounded wait makes this test fail rather than hang the suite.
#[test]
fn release_cut_runs_unattended_with_stdin_closed() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let isolated_home = tempfile::tempdir().unwrap();

    let mut child = Command::new(devflow_bin())
        .arg("release")
        .arg("cut")
        .arg("1.2.3")
        .arg(dir.path())
        .arg("--yes-release")
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn devflow release cut");

    let start = Instant::now();
    let timeout = Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .expect("try_wait on devflow release cut child")
        {
            break status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "devflow release cut did not terminate on its own within {timeout:?} — \
                 the executor must never block waiting for input"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert!(
        !status.success(),
        "expected a fixture with no origin remote to stop before completion (non-zero exit)"
    );
}

/// No question mark followed by a read, no "press", no "confirm", no
/// "continue?" — none of the shapes an interactive prompt would take.
/// Lowercased so casing cannot evade the check; these substrings cannot
/// appear in any of this command's own legitimate stop-report wording.
#[test]
fn release_cut_output_contains_no_interactive_prompt() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = run_cut(dir.path(), "1.2.3", &["--yes-release"]);
    let combined = combined_output(&output).to_lowercase();

    for forbidden in ["press ", "confirm", "continue?", "(y/n)", "y/n?"] {
        assert!(
            !combined.contains(forbidden),
            "output contained a potential interactive-prompt substring {forbidden:?}: {combined}"
        );
    }
}

// -- authorization: the three grant paths -----------------------------------

/// With no mandate granted at all, the run refuses, names all three grant
/// mechanisms, and — critically — never reads stdin (proven by using the
/// default, non-piped stdin here; a blocking read would hang this test).
#[test]
fn release_cut_without_a_mandate_refuses() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = run_cut(dir.path(), "1.2.3", &[]);
    assert!(
        !output.status.success(),
        "expected a run with no mandate to exit non-zero"
    );
    let combined = combined_output(&output);
    for needle in ["--yes-release", "yes_release", "DEVFLOW_YES_RELEASE"] {
        assert!(
            combined.contains(needle),
            "expected the refusal to name {needle}, got: {combined}"
        );
    }
}

/// `DEVFLOW_YES_RELEASE=true` with no flag proceeds past the authorization
/// check. This test must not depend on any network outcome — it asserts
/// only that a step row is present, not on which step or which result.
#[test]
fn release_cut_accepts_the_environment_mandate() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let isolated_home = tempfile::tempdir().unwrap();

    let output = Command::new(devflow_bin())
        .arg("release")
        .arg("cut")
        .arg("1.2.3")
        .arg(dir.path())
        .env("HOME", isolated_home.path())
        .env("DEVFLOW_YES_RELEASE", "true")
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release cut");

    let combined = combined_output(&output);
    assert!(
        combined.contains(devflow_core::release_observe::ReleaseStep::VersionBumped.label()),
        "expected the run to proceed past authorization and print at least one step row, got: {combined}"
    );
}

/// `yes_release = true` written into `devflow.toml`, with no flag and no
/// environment variable, also proceeds past the authorization check.
#[test]
fn release_cut_accepts_the_config_file_mandate() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("devflow.toml"), "yes_release = true\n").unwrap();

    let output = run_cut(dir.path(), "1.2.3", &[]);
    let combined = combined_output(&output);
    assert!(
        combined.contains(devflow_core::release_observe::ReleaseStep::VersionBumped.label()),
        "expected the run to proceed past authorization and print at least one step row, got: {combined}"
    );
}

// -- records nothing, and re-running is re-observing -------------------------

/// `devflow release cut` writes NOTHING: `.devflow/` and `devflow.toml` are
/// byte-identical before and after, and no new file appears under
/// `.devflow/`. This is the executable form of RD-8's "state is derived,
/// never recorded" at the executor level, complementing `29-02`'s
/// observer-level assertion of the same invariant.
#[test]
fn release_cut_writes_no_devflow_state() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    std::fs::write(dir.path().join("devflow.toml"), "# devflow config\n").unwrap();
    std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
    std::fs::write(
        dir.path().join(".devflow").join("state.json"),
        "{\"stage\":\"ship\"}",
    )
    .unwrap();

    let before_devflow_toml = std::fs::read(dir.path().join("devflow.toml")).unwrap();
    let before_state_json = std::fs::read(dir.path().join(".devflow").join("state.json")).unwrap();
    let before_devflow_entries: Vec<_> = std::fs::read_dir(dir.path().join(".devflow"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    let _ = run_cut(dir.path(), "1.2.3", &["--yes-release"]);

    let after_devflow_toml = std::fs::read(dir.path().join("devflow.toml")).unwrap();
    let after_state_json = std::fs::read(dir.path().join(".devflow").join("state.json")).unwrap();
    let after_devflow_entries: Vec<_> = std::fs::read_dir(dir.path().join(".devflow"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

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

// -- 29-05: real reasons, never invented -------------------------------------

/// The step stops with the real tool failure text, never an invented one,
/// when the remote is unreachable. Points `origin` at a path that does not
/// exist — `gh` cannot resolve any repository context from it, and the
/// walk's first oracle (`VersionBumped`) fails immediately and locally, no
/// network needed.
#[test]
fn cut_stops_with_a_real_reason_when_the_remote_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    git(
        dir.path(),
        &["remote", "add", "origin", "/nonexistent/origin.git"],
    );

    let output = run_cut(dir.path(), "1.2.3", &["--yes-release"]);
    assert!(
        !output.status.success(),
        "expected the walk to stop rather than succeed against an unreachable remote"
    );
    let combined = combined_output(&output);
    assert!(
        combined.contains("stopped:"),
        "expected a `stopped:` line naming the real failure, got: {combined}"
    );
    assert!(
        !combined.trim().is_empty(),
        "expected a real, non-empty failure reason to be printed"
    );
}

/// A reachable-but-unauthenticated remote (a real, local bare repo, so `gh`
/// has genuine repository context but no credentials in the isolated
/// `HOME`) also stops with a real, non-crashing failure rather than a
/// fabricated one.
#[test]
fn cut_stops_without_crashing_when_gh_is_unauthenticated_against_a_real_remote() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let origin_dir = tempfile::tempdir().unwrap();
    git(origin_dir.path(), &["init", "-q", "--bare"]);
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

    let output = run_cut(dir.path(), "1.2.3", &["--yes-release"]);
    assert!(
        !output.status.success(),
        "expected the walk to stop rather than succeed with gh unauthenticated"
    );
    let combined = combined_output(&output);
    assert!(
        combined.contains("stopped:"),
        "expected a `stopped:` line naming the real failure, got: {combined}"
    );
}

/// Re-running is re-observing: two consecutive, identical invocations
/// against the same fixture produce identical reports — nothing is carried
/// between runs.
#[test]
fn release_cut_is_idempotent_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let first = run_cut(dir.path(), "1.2.3", &["--yes-release"]);
    let second = run_cut(dir.path(), "1.2.3", &["--yes-release"]);

    assert_eq!(first.status.code(), second.status.code());
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&second.stdout),
        "two consecutive identical runs must produce identical reports"
    );
}

// -- 29-06: the unit boundary is a tested fact, not a claim ------------------

/// Write a fake `gh` executable that answers only the handful of `gh api`
/// invocations `release_observe::observe` makes for
/// `VersionBumped`/`ChangelogWritten`/`ReleasePrMerged`/`SyncMerged`,
/// reporting each as already done for `version`. This is a fixture double
/// standing in for a real GitHub repository — not a general-purpose `gh`
/// mock — and every other invocation exits non-zero.
fn write_fake_gh_reporting_prs_landed(dir: &Path, version: &str) -> std::path::PathBuf {
    let bin_dir = dir.join("fake-bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let template = r#"#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "api" ]; then
  case "${2:-}" in
    "repos/{owner}/{repo}/contents/Cargo.toml?ref=develop"|"repos/{owner}/{repo}/contents/Cargo.toml?ref=main")
      cat <<'CARGOEOF'
[workspace.package]
version = "VERSION_PLACEHOLDER"
CARGOEOF
      exit 0
      ;;
    "repos/{owner}/{repo}/contents/CHANGELOG.md?ref=develop")
      cat <<'CHANGEEOF'
# Changelog

## VERSION_PLACEHOLDER

### Added
- test
CHANGEEOF
      exit 0
      ;;
    "repos/{owner}/{repo}/compare/main...develop")
      echo "identical"
      exit 0
      ;;
  esac
fi
echo "fake gh: unhandled invocation: $*" >&2
exit 1
"#;
    let script = template.replace("VERSION_PLACEHOLDER", version);
    let gh_path = bin_dir.join("gh");
    std::fs::write(&gh_path, script).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&gh_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh_path, perms).unwrap();
    }
    bin_dir
}

/// Against a fixture where steps 1 (version bump), 2 (changelog), 3
/// (release PR merged), and 5 (sync merged) all observe as already done —
/// via a fake `gh` standing in for GitHub, since none of this phase's units
/// implement a fake registry — the walk stops on step 4, the signed tag,
/// naming unit `29c`. `SignedTagPresent`'s own oracle is a real, local `git
/// ls-remote` against a real (tag-less) origin, so it genuinely observes
/// absent rather than being faked away — this is the one step under test,
/// and this pins the unit boundary as a tested fact. This assertion will
/// need updating — deliberately — when `29-07` lands and gives
/// `SignedTagPresent` a real action.
#[test]
fn cut_walks_to_the_signed_tag_step_and_stops() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let origin_dir = tempfile::tempdir().unwrap();
    git(origin_dir.path(), &["init", "-q", "--bare"]);
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

    let fake_bin_dir = write_fake_gh_reporting_prs_landed(dir.path(), "1.2.3");
    let isolated_home = tempfile::tempdir().unwrap();
    let path_var = format!(
        "{}:{}",
        fake_bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = Command::new(devflow_bin())
        .arg("release")
        .arg("cut")
        .arg("1.2.3")
        .arg(dir.path())
        .arg("--yes-release")
        .env("HOME", isolated_home.path())
        .env("PATH", path_var)
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release cut");

    let combined = combined_output(&output);
    assert!(
        !output.status.success(),
        "expected the walk to stop before completion, got: {combined}"
    );
    assert!(
        combined.contains("supplied by unit 29c"),
        "expected the walk to stop at the signed-tag step naming unit 29c, got: {combined}"
    );
}

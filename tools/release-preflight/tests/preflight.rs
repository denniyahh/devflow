//! End-to-end tests for `release-preflight check` and `release-preflight
//! verify`, driving the real binary against throwaway fixture workspaces.
//!
//! Every check the tool runs has at least one fixture that passes it and one
//! that fails it (or, where the check can only warn, one that makes it warn),
//! so a check that stops biting fails a named test rather than passing
//! silently. These are ported from the `devflow release --check`/`--verify`
//! suite that lived in `crates/devflow-cli/tests/release_check.rs`.
//!
//! Hermeticity: every spawned process, the tool and each fixture `git`, is
//! built with `devflow_core::git::hermetic_command`, which removes the
//! repository-local `GIT_*` variables (a `GIT_DIR` inherited from a hook
//! would otherwise retarget the real checkout). `HOME` points at an empty
//! directory and the global and system git config are disabled, so the
//! operator's own `~/.gitconfig` cannot change an outcome.

use std::path::Path;
use std::process::{Command, Output};

/// A command for `program` in `dir` with the redirecting `GIT_*` variables
/// removed and the global and system git configuration switched off.
fn hermetic(program: &str, dir: &Path, home: &Path) -> Command {
    let mut cmd = devflow_core::git::hermetic_command(program, dir);
    cmd.env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID");
    cmd
}

/// One fixture: a temp project root plus an empty `HOME` beside it.
struct Fixture {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().expect("create project tempdir"),
            home: tempfile::tempdir().expect("create home tempdir"),
        }
    }

    fn root(&self) -> &Path {
        self.project.path()
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent dir");
        }
        std::fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    }

    fn git(&self, args: &[&str]) {
        let output = hermetic("git", self.root(), self.home.path())
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn rev_parse(&self, rev: &str) -> String {
        let output = hermetic("git", self.root(), self.home.path())
            .args(["rev-parse", rev])
            .output()
            .expect("spawn git rev-parse");
        assert!(output.status.success(), "git rev-parse {rev} failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Point a remote-tracking ref (`refs/remotes/origin/<branch>`) at `rev`,
    /// standing in for an already-fetched remote without a network.
    fn set_remote_ref(&self, branch: &str, rev: &str) {
        let sha = self.rev_parse(rev);
        self.git(&["update-ref", &format!("refs/remotes/origin/{branch}"), &sha]);
    }

    /// `git init` on a `develop` branch, with signing and hooks off.
    fn init_repo(&self) {
        self.git(&["init", "-q"]);
        self.git(&["config", "user.email", "test@example.com"]);
        self.git(&["config", "user.name", "Test"]);
        self.git(&["config", "commit.gpgsign", "false"]);
        self.git(&["config", "tag.gpgsign", "false"]);
        self.git(&["config", "core.hooksPath", "/dev/null"]);
        self.git(&["checkout", "-q", "-b", "develop"]);
    }

    fn commit(&self, name: &str) {
        self.write(name, name);
        self.git(&["add", "."]);
        self.git(&["commit", "-q", "-m", &format!("add {name}")]);
    }

    fn run(&self, subcommand: &str) -> Run {
        let output = hermetic(
            env!("CARGO_BIN_EXE_release-preflight"),
            self.root(),
            self.home.path(),
        )
        .arg(subcommand)
        .arg(self.root())
        .output()
        .expect("spawn release-preflight");
        Run(output)
    }
}

/// The outcome of one `release-preflight` run.
struct Run(Output);

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.0.stdout).into_owned()
    }

    fn passed(&self) -> bool {
        self.0.status.success()
    }

    /// Both streams, for an assertion message.
    fn transcript(&self) -> String {
        format!(
            "exit: {:?}\nstdout:\n{}\nstderr:\n{}",
            self.0.status.code(),
            self.stdout(),
            String::from_utf8_lossy(&self.0.stderr)
        )
    }

    /// The report line for the check whose name starts with `name`.
    fn line(&self, name: &str) -> String {
        self.stdout()
            .lines()
            .find(|line| line.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("no report line for {name:?}\n{}", self.transcript()))
            .to_string()
    }
}

/// A workspace `Cargo.toml` whose `[workspace.dependencies]` self-pin either
/// matches or diverges from `[workspace.package] version`, in a git repo with
/// one commit and no remote.
fn workspace_fixture(package_version: &str, pin_version: &str) -> Fixture {
    let fixture = Fixture::new();
    fixture.init_repo();
    fixture.write(
        "Cargo.toml",
        &format!(
            "[workspace]\nmembers = [\"crates/devflow-core\"]\n\n\
             [workspace.package]\nversion = \"{package_version}\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = {{ path = \"crates/devflow-core\", version = \"{pin_version}\" }}\n"
        ),
    );
    fixture.commit("base.txt");
    fixture
}

const SELF_PIN: &str = "self-pin";

#[test]
fn check_passes_when_pins_match() {
    let fixture = workspace_fixture("1.7.0", "1.7.0");

    let run = fixture.run("check");

    assert!(
        run.passed(),
        "matching pins must pass\n{}",
        run.transcript()
    );
    let line = run.line(SELF_PIN);
    assert!(
        line.contains('✓') && line.contains("match 1.7.0"),
        "the self-pin line must be ok and name the version, got: {line}"
    );
    assert!(
        run.stdout().contains("release preflight passed"),
        "expected a passing report\n{}",
        run.transcript()
    );
}

#[test]
fn check_flags_self_pin_drift() {
    // The defect class the check exists for: the workspace version moved to
    // 1.7.0 but the self-pin was left on the previous release's 1.6.0.
    let fixture = workspace_fixture("1.7.0", "1.6.0");

    let run = fixture.run("check");

    assert!(
        !run.passed(),
        "a drifted self-pin must fail\n{}",
        run.transcript()
    );
    let line = run.line(SELF_PIN);
    assert!(
        line.contains('✗') && line.contains("devflow-core pinned 1.6.0 != 1.7.0"),
        "the self-pin line must fail and name the drifted pin, got: {line}"
    );
}

/// A workspace every `check` passes on: matching pins, two members where
/// `devflow` depends on `devflow-core`, a `CHANGELOG.md` whose top heading is
/// `changelog_version`, and `origin/main` at `HEAD`. Each negative control
/// below changes one thing from this fixture, so the check it targets is the
/// only one that can fail.
fn release_ready_fixture(version: &str, changelog_version: &str) -> Fixture {
    let fixture = Fixture::new();
    fixture.init_repo();
    fixture.write(
        "Cargo.toml",
        &format!(
            "[workspace]\nmembers = [\n    \"crates/devflow-core\",\n    \"crates/devflow-cli\",\n]\n\n\
             [workspace.package]\nversion = \"{version}\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = {{ path = \"crates/devflow-core\", version = \"{version}\" }}\n"
        ),
    );
    fixture.write(
        "crates/devflow-core/Cargo.toml",
        "[package]\nname = \"devflow-core\"\nversion.workspace = true\n\n[dependencies]\n",
    );
    fixture.write(
        "crates/devflow-cli/Cargo.toml",
        "[package]\nname = \"devflow\"\nversion.workspace = true\n\n\
         [dependencies]\ndevflow-core.workspace = true\n",
    );
    fixture.write(
        "CHANGELOG.md",
        &format!("# Changelog\n\n## [{changelog_version}] - 2026-11-10\n\n- notes\n"),
    );
    fixture.commit("base.txt");
    fixture.set_remote_ref("main", "HEAD");
    fixture
}

const DIVERGENCE: &str = "develop/main divergence";
const PUBLISH_ORDER: &str = "crates.io publish order";
const CHANGELOG: &str = "changelog version";
const TAG_ON_MAIN: &str = "release tag on main";
const SYNC: &str = "main→develop sync";

/// The positive control for every `check` line: on a release-ready workspace
/// each check is ok and the run exits 0.
#[test]
fn check_passes_on_a_release_ready_fixture() {
    let fixture = release_ready_fixture("1.7.0", "1.7.0");

    let run = fixture.run("check");

    assert!(
        run.passed(),
        "a release-ready fixture must pass\n{}",
        run.transcript()
    );
    for name in [SELF_PIN, DIVERGENCE, PUBLISH_ORDER, CHANGELOG] {
        let line = run.line(name);
        assert!(line.contains('✓'), "{name} must be ok, got: {line}");
    }
}

/// `origin/main` resolves locally but is not an ancestor of `HEAD`: develop
/// has diverged, and the fix is `scripts/sync-main-to-develop.sh`. No fetch is
/// issued; the remote-tracking ref is set directly.
#[test]
fn check_reports_divergence_when_main_not_ancestor() {
    let fixture = release_ready_fixture("1.7.0", "1.7.0");
    fixture.git(&["checkout", "-q", "-b", "main-line"]);
    fixture.commit("main-only.txt");
    fixture.set_remote_ref("main", "HEAD");
    fixture.git(&["checkout", "-q", "develop"]);
    fixture.commit("develop-only.txt");

    let run = fixture.run("check");

    assert!(
        !run.passed(),
        "a real divergence must fail\n{}",
        run.transcript()
    );
    let line = run.line(DIVERGENCE);
    assert!(
        line.contains('✗') && line.contains("NOT an ancestor"),
        "the divergence line must fail, got: {line}"
    );
    assert!(
        run.stdout().contains("sync-main-to-develop.sh"),
        "the divergence failure must name the sync script\n{}",
        run.transcript()
    );
}

/// `origin/main` was never fetched: the check must degrade to a warning that
/// says to fetch, never report a false divergence and never fetch itself.
#[test]
fn check_divergence_degrades_when_origin_main_absent() {
    let fixture = release_ready_fixture("1.7.0", "1.7.0");
    fixture.git(&["update-ref", "-d", "refs/remotes/origin/main"]);

    let run = fixture.run("check");

    let line = run.line(DIVERGENCE);
    assert!(
        line.contains('⚠') && line.contains("origin/main not fetched"),
        "the divergence line must warn that origin/main is not fetched, got: {line}"
    );
    assert!(
        run.stdout().contains("git fetch"),
        "the warning must direct the operator to `git fetch`\n{}",
        run.transcript()
    );
    assert!(
        run.passed(),
        "a warning alone must not fail the run\n{}",
        run.transcript()
    );
}

/// The publish order comes from the workspace's own members and their
/// dependencies: `devflow` depends on `devflow-core`, so core goes first.
#[test]
fn check_states_publish_order() {
    let fixture = release_ready_fixture("1.0.0", "1.0.0");

    let run = fixture.run("check");

    let line = run.line(PUBLISH_ORDER);
    assert!(
        line.contains("publish in order: devflow-core -> devflow")
            && !line.contains("devflow -> devflow-core"),
        "the publish order must state devflow-core before devflow, got: {line}"
    );
}

/// When no member manifest can be read the order cannot be derived, and the
/// check must say so as a warning rather than state an empty order as ok.
#[test]
fn check_warns_when_the_publish_order_cannot_be_derived() {
    // `members` names `crates/devflow-core`, but the fixture writes no
    // manifest there.
    let fixture = workspace_fixture("1.7.0", "1.7.0");

    let run = fixture.run("check");

    let line = run.line(PUBLISH_ORDER);
    assert!(
        line.contains('⚠') && line.contains("could not determine workspace publish order"),
        "an underivable publish order must warn, got: {line}"
    );
    assert!(
        !run.stdout().contains("publish in order:"),
        "no order may be stated when none can be derived\n{}",
        run.transcript()
    );
}

/// The top `## <version>` heading of `CHANGELOG.md` must equal the workspace
/// version, so a forgotten bump cannot ship notes under the old version.
#[test]
fn check_flags_a_changelog_version_mismatch() {
    let fixture = release_ready_fixture("1.7.0", "1.6.0");

    let run = fixture.run("check");

    assert!(
        !run.passed(),
        "a changelog version mismatch must fail\n{}",
        run.transcript()
    );
    let line = run.line(CHANGELOG);
    assert!(
        line.contains('✗') && line.contains("changelog 1.6.0 != workspace 1.7.0"),
        "the changelog line must fail and name both versions, got: {line}"
    );
}

/// A released fixture: `v1.7.0` tagged on the commit `origin/main` points at,
/// and `origin/develop` at that same commit (the sync has run).
fn released_fixture() -> Fixture {
    let fixture = release_ready_fixture("1.7.0", "1.7.0");
    fixture.git(&["tag", "v1.7.0"]);
    fixture.set_remote_ref("develop", "HEAD");
    fixture
}

/// The positive control for both `verify` checks.
#[test]
fn verify_passes_on_a_tagged_synced_fixture() {
    let fixture = released_fixture();

    let run = fixture.run("verify");

    assert!(
        run.passed(),
        "a tagged, synced release must verify\n{}",
        run.transcript()
    );
    for name in [TAG_ON_MAIN, SYNC] {
        let line = run.line(name);
        assert!(line.contains('✓'), "{name} must be ok, got: {line}");
    }
    assert!(
        run.stdout().contains("release verification passed"),
        "expected a passing verification report\n{}",
        run.transcript()
    );
}

/// The v2.5.0 mistake: the tag sits on a develop-only commit that is not on
/// `origin/main`.
#[test]
fn verify_flags_a_tag_not_on_main() {
    let fixture = release_ready_fixture("1.7.0", "1.7.0");
    fixture.commit("develop-release-commit.txt");
    fixture.git(&["tag", "v1.7.0"]);
    fixture.set_remote_ref("develop", "HEAD");

    let run = fixture.run("verify");

    assert!(
        !run.passed(),
        "a tag off main must fail verification\n{}",
        run.transcript()
    );
    let line = run.line(TAG_ON_MAIN);
    assert!(
        line.contains('✗') && line.contains("v1.7.0 is NOT an ancestor of origin/main"),
        "the tag line must fail, got: {line}"
    );
}

/// `origin/main` carries a commit `origin/develop` does not: the main→develop
/// sync was skipped.
#[test]
fn verify_flags_an_unsynced_develop() {
    let fixture = release_ready_fixture("1.7.0", "1.7.0");
    fixture.git(&["checkout", "-q", "-b", "main-line"]);
    fixture.commit("squash-merge.txt");
    fixture.git(&["tag", "v1.7.0"]);
    fixture.set_remote_ref("main", "HEAD");
    fixture.git(&["checkout", "-q", "develop"]);
    fixture.commit("develop-only.txt");
    fixture.set_remote_ref("develop", "HEAD");

    let run = fixture.run("verify");

    assert!(
        !run.passed(),
        "a skipped sync must fail verification\n{}",
        run.transcript()
    );
    let line = run.line(SYNC);
    assert!(
        line.contains('✗') && line.contains("sync was skipped"),
        "the sync line must fail, got: {line}"
    );
}

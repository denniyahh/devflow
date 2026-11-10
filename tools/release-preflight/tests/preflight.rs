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

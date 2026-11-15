//! Behavioural tests for `scripts/cut-pr-branch.sh` in phase mode.
//!
//! Each test builds a scratch repository with a bare `origin`, a pushed
//! `develop` and a `feature/phase-52.1` branch, then runs this checkout's real
//! script there. Global and system git config are nulled on every spawned
//! command, the script's included: it runs `git commit -C <hash>`, so a
//! developer's `commit.gpgsign` must not reach it. No git command touches the
//! real checkout; the script file is only read from it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PHASE_BRANCH: &str = "feature/phase-52.1";
const PR_BRANCH: &str = "feature/phase-52.1-pr";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn describe(out: &Output) -> String {
    format!(
        "status {:?}, stdout {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A scratch `origin` and work repository, checked out on the phase branch
/// with nothing committed on it yet.
struct Fixture {
    scratch: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            scratch: tempfile::tempdir().expect("create scratch dir"),
        };
        let root = fixture.scratch.path();
        std::fs::create_dir_all(root.join("home/.config")).expect("create scratch home");
        let init = fixture
            .hermetic("git", root)
            .args(["init", "-q", "--bare", "origin.git"])
            .output()
            .expect("run git init");
        assert!(init.status.success(), "init origin: {}", describe(&init));
        let init = fixture
            .hermetic("git", root)
            .args(["init", "-q", "-b", "develop", "work"])
            .output()
            .expect("run git init");
        assert!(init.status.success(), "init work: {}", describe(&init));
        fixture.git(&["config", "user.email", "test@example.com"]);
        fixture.git(&["config", "user.name", "test"]);
        fixture.git(&["remote", "add", "origin", "../origin.git"]);
        fixture.commit("chore: base", &["base.txt"]);
        fixture.git(&["push", "-q", "origin", "develop"]);
        fixture.git(&["checkout", "-q", "-b", PHASE_BRANCH]);
        fixture
    }

    fn work(&self) -> PathBuf {
        self.scratch.path().join("work")
    }

    /// A command that reads no git config or repository state from outside
    /// the scratch directory.
    fn hermetic(&self, program: &str, cwd: &Path) -> Command {
        let home = self.scratch.path().join("home");
        let mut cmd = Command::new(program);
        cmd.current_dir(cwd)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("HOME", home)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        for name in devflow_core::test_support::REPO_LOCAL_GIT_VARS
            .iter()
            .chain(devflow_core::test_support::ALSO_REDIRECTING_GIT_VARS)
        {
            cmd.env_remove(name);
        }
        cmd
    }

    fn try_git(&self, args: &[&str]) -> Output {
        self.hermetic("git", &self.work())
            .args(args)
            .output()
            .expect("run git")
    }

    /// Runs a git command that must succeed and returns its trimmed stdout.
    fn git(&self, args: &[&str]) -> String {
        let out = self.try_git(args);
        assert!(out.status.success(), "git {args:?}: {}", describe(&out));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Commits `paths` (each written with the subject as its content) and
    /// returns the commit's full SHA.
    fn commit(&self, subject: &str, paths: &[&str]) -> String {
        for path in paths {
            let file = self.work().join(path);
            std::fs::create_dir_all(file.parent().expect("a file has a parent"))
                .expect("create the file's directory");
            std::fs::write(&file, format!("{subject}\n")).expect("write file");
            self.git(&["add", "--", path]);
        }
        self.git(&["commit", "-q", "-m", subject]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Runs the real script for phase 52.1 in the work repository.
    fn cut(&self, extra: &[&str]) -> Output {
        self.hermetic("bash", &self.work())
            .arg(repo_root().join("scripts/cut-pr-branch.sh"))
            .args(["--phase", "52.1"])
            .args(extra)
            .env_remove("PHASE")
            .env_remove("BASE_REMOTE")
            .env_remove("WORKSPACE_BASE")
            .output()
            .expect("run cut-pr-branch.sh")
    }

    /// Subjects on the PR branch that are not on `origin/develop`, oldest first.
    fn pr_subjects(&self) -> Vec<String> {
        let range = format!("origin/develop..{PR_BRANCH}");
        self.git(&["log", "--reverse", "--format=%s", &range])
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// Commits one code file per subject, in order, and returns each commit's SHA.
fn commit_code(fixture: &Fixture, subjects: &[&str]) -> Vec<String> {
    subjects
        .iter()
        .enumerate()
        .map(|(index, subject)| fixture.commit(subject, &[&format!("src/{index}.txt")]))
        .collect()
}

/// Scopes that resemble phase 52.1's but belong to another phase or use a
/// suffix the filter does not accept.
const LOOK_ALIKES: [&str; 5] = [
    "fix(52-review): f",
    "fix(152.1-review): g",
    "fix(52.10-review): h",
    "docs(52.1-docs): i",
    "fix(52.1-reviewx): j",
];

#[test]
fn review_and_fix_scoped_commits_of_the_phase_are_replayed() {
    let fixture = Fixture::new();
    let subjects = [
        "feat(52.1-01): a",
        "fix(52.1-review): b",
        "docs(52.1-fix): c",
        "feat(52.1): d",
        "feat(phase-52.1)!: e",
    ];
    commit_code(&fixture, &subjects);

    let out = fixture.cut(&[]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.pr_subjects(),
        subjects,
        "every commit scoped to the phase, its review and fix scopes included, \
         must be replayed in order: {}",
        describe(&out)
    );
}

#[test]
fn look_alike_scopes_are_not_replayed() {
    let fixture = Fixture::new();
    commit_code(&fixture, &LOOK_ALIKES);
    fixture.commit("feat(52.1-01): k", &["src/k.txt"]);

    let out = fixture.cut(&[]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.pr_subjects(),
        ["feat(52.1-01): k"],
        "only the commit scoped to phase 52.1 may be replayed: {}",
        describe(&out)
    );
}

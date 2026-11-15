//! Behavioural tests for `scripts/cut-pr-branch.sh` in phase mode.
//!
//! Each test builds a scratch repository with a bare `origin`, a pushed
//! `develop` and a `feature/phase-52.1` branch, then runs this checkout's real
//! script there. Global and system git config are nulled on every spawned
//! command, the script's included: it runs `git commit -C <hash>`, so a
//! developer's `commit.gpgsign` must not reach it. No git command touches the
//! real checkout; the script file is only read from it.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

const PHASE_BRANCH: &str = "feature/phase-52.1";
const PR_BRANCH: &str = "feature/phase-52.1-pr";
const PHASE_ARGS: [&str; 2] = ["--phase", "52.1"];
/// An in-scope commit, so a range under test always has something to replay.
const SENTINEL: &str = "feat(52.1-01): sentinel";
/// An out-of-scope commit that touches a code file.
const SKIPPED_CODE: &str = "fix(52-review): code";

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

    /// The real script, set to run in the work repository.
    fn script(&self, args: &[&str]) -> Command {
        let mut cmd = self.hermetic("bash", &self.work());
        cmd.arg(repo_root().join("scripts/cut-pr-branch.sh"))
            .args(args)
            .env_remove("PHASE")
            .env_remove("BASE_REMOTE")
            .env_remove("WORKSPACE_BASE");
        cmd
    }

    /// Runs the script for phase 52.1.
    fn cut(&self, extra: &[&str]) -> Output {
        self.script(&[&PHASE_ARGS[..], extra].concat())
            .output()
            .expect("run cut-pr-branch.sh")
    }

    /// Runs the script with stdout and stderr on one file, so the order the
    /// two streams were written in can be read back.
    fn cut_transcript(&self) -> (ExitStatus, String) {
        let path = self.scratch.path().join("transcript.log");
        let file = std::fs::File::create(&path).expect("create the transcript");
        let status = self
            .script(&PHASE_ARGS)
            .stdout(file.try_clone().expect("share the transcript"))
            .stderr(file)
            .status()
            .expect("run cut-pr-branch.sh");
        let transcript = std::fs::read_to_string(&path).expect("read the transcript");
        (status, transcript)
    }

    /// Subjects on the PR branch that are not on `origin/develop`, oldest first.
    fn pr_subjects(&self) -> Vec<String> {
        let range = format!("origin/develop..{PR_BRANCH}");
        self.git(&["log", "--reverse", "--format=%s", &range])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn pr_branch_exists(&self) -> bool {
        let reference = format!("refs/heads/{PR_BRANCH}");
        self.try_git(&["show-ref", "--verify", "--quiet", &reference])
            .status
            .success()
    }

    fn short(&self, sha: &str) -> String {
        self.git(&["rev-parse", "--short", sha])
    }
}

/// The script's `  skipped <short-sha> <subject>` stderr lines, as
/// `(short sha, subject)` pairs.
fn skipped(out: &Output) -> Vec<(String, String)> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|line| line.strip_prefix("  skipped "))
        .filter_map(|rest| rest.split_once(' '))
        .map(|(sha, subject)| (sha.to_owned(), subject.to_owned()))
        .collect()
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

/// A range whose middle commit is out of scope and touches a code file.
const OUT_OF_SCOPE_MIDDLE: [&str; 3] = ["feat(52.1-01): a", "chore(ci): b", "feat(52.1-02): c"];

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

#[test]
fn a_skipped_code_commit_is_named_before_the_replay() {
    let fixture = Fixture::new();
    fixture.commit(SENTINEL, &["src/sentinel.txt"]);
    let code = fixture.commit(SKIPPED_CODE, &["src/x.txt"]);

    let out = fixture.cut(&[]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.pr_subjects(),
        [SENTINEL],
        "the out-of-scope commit must not be replayed: {}",
        describe(&out)
    );
    assert_eq!(
        skipped(&out),
        [(fixture.short(&code), SKIPPED_CODE.to_owned())],
        "the dropped code commit must be named on a skipped line: {}",
        describe(&out)
    );
}

#[test]
fn the_skipped_listing_is_printed_before_the_pr_branch_is_initialized() {
    let fixture = Fixture::new();
    fixture.commit(SENTINEL, &["src/sentinel.txt"]);
    let code = fixture.commit(SKIPPED_CODE, &["src/x.txt"]);
    let skipped_line = format!("  skipped {} {SKIPPED_CODE}", fixture.short(&code));

    let (status, transcript) = fixture.cut_transcript();

    assert!(status.success(), "the cut must pass: {transcript}");
    let listed_at = transcript.lines().position(|line| line == skipped_line);
    let initialized_at = transcript
        .lines()
        .position(|line| line.starts_with("==> Initializing clean branch"));
    assert!(
        listed_at.is_some(),
        "the dropped code commit must be listed: {transcript}"
    );
    assert!(
        initialized_at.is_some(),
        "premise: the script announces the PR branch: {transcript}"
    );
    assert!(
        listed_at < initialized_at,
        "the listing must be printed before the PR branch is created: {transcript}"
    );
}

#[test]
fn look_alike_scopes_are_each_listed_as_skipped() {
    let fixture = Fixture::new();
    let shas = commit_code(&fixture, &LOOK_ALIKES);
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
    let listed = skipped(&out);
    for (sha, subject) in shas.iter().zip(LOOK_ALIKES) {
        assert!(
            listed.contains(&(fixture.short(sha), subject.to_owned())),
            "{subject} must be named on its own skipped line: {}",
            describe(&out)
        );
    }
}

#[test]
fn a_skipped_planning_only_commit_is_not_listed() {
    let fixture = Fixture::new();
    fixture.commit(SENTINEL, &["src/sentinel.txt"]);
    fixture.commit("chore(notes): planning", &[".planning/notes.md"]);

    let out = fixture.cut(&[]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.pr_subjects(),
        [SENTINEL],
        "the planning-only commit must not be replayed: {}",
        describe(&out)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("chore(notes): planning"),
        "a commit that touches only forbidden paths must not be listed: {}",
        describe(&out)
    );
}

#[test]
fn the_skipped_listing_and_the_replay_agree() {
    let fixture = Fixture::new();
    let range: [(&str, &[&str]); 6] = [
        ("feat(52.1-01): in scope code", &["src/a.txt"]),
        ("fix(52-review): out of scope code", &["src/b.txt"]),
        ("docs(52.1): in scope planning", &[".planning/a.md"]),
        ("chore(notes): out of scope planning", &[".planning/b.md"]),
        (
            "fix(52.1-review): in scope mixed",
            &["src/c.txt", ".planning/c.md"],
        ),
        (
            "chore(ci): out of scope mixed",
            &["src/d.txt", ".planning/d.md"],
        ),
    ];
    for (subject, paths) in range {
        fixture.commit(subject, paths);
    }

    let out = fixture.cut(&[]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    let replayed = fixture.pr_subjects();
    let listed = skipped(&out);
    for (subject, paths) in range {
        let outcomes = [
            replayed.iter().any(|replayed| replayed == subject),
            listed.iter().any(|(_, listed)| listed == subject),
            paths.iter().all(|path| path.starts_with(".planning/")),
        ];
        assert_eq!(
            outcomes.iter().filter(|held| **held).count(),
            1,
            "{subject} must be exactly one of replayed, listed and planning-only, \
             got {outcomes:?}: {}",
            describe(&out)
        );
    }
}

#[test]
fn an_all_skipped_range_lists_its_code_commits_then_refuses() {
    let fixture = Fixture::new();
    let code = fixture.commit(SKIPPED_CODE, &["src/x.txt"]);
    let skipped_line = format!("  skipped {} {SKIPPED_CODE}", fixture.short(&code));

    let out = fixture.cut(&[]);

    assert_eq!(
        out.status.code(),
        Some(1),
        "a range with nothing in scope must be refused: {}",
        describe(&out)
    );
    assert!(
        !fixture.pr_branch_exists(),
        "a refused cut must not create the PR branch: {}",
        describe(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let listed_at = stderr.lines().position(|line| line == skipped_line);
    let refused_at = stderr
        .lines()
        .position(|line| line.starts_with("error: no commits scoped to phase 52.1"));
    assert!(
        listed_at.is_some(),
        "the skipped code commit must be listed: {}",
        describe(&out)
    );
    assert!(
        refused_at.is_some(),
        "the refusal must say nothing is scoped to the phase: {}",
        describe(&out)
    );
    assert!(
        listed_at < refused_at,
        "the listing must come before the refusal: {}",
        describe(&out)
    );
}

#[test]
fn an_unreadable_commit_aborts_before_anything_is_replayed() {
    let fixture = Fixture::new();
    fixture.commit("feat(52.1-01): a", &["src/a.txt"]);
    let unreadable = fixture.commit("fix(52-review): b", &["src/b.txt"]);
    fixture.commit("feat(52.1-02): c", &["src/c.txt"]);
    let tree = fixture.git(&["rev-parse", &format!("{unreadable}^{{tree}}")]);
    let object = fixture
        .work()
        .join(".git/objects")
        .join(&tree[..2])
        .join(&tree[2..]);
    assert!(
        object.is_file(),
        "premise: commit b's tree must be a loose object at {}",
        object.display()
    );
    std::fs::remove_file(&object).expect("remove commit b's tree object");

    let out = fixture.cut(&[]);

    assert!(
        !out.status.success(),
        "a commit git cannot read must stop the cut: {}",
        describe(&out)
    );
    assert!(
        !fixture.pr_branch_exists(),
        "the cut must stop before the PR branch is created: {}",
        describe(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot list the files commit"),
        "the error must say the commit's files could not be listed: {}",
        describe(&out)
    );
    assert!(
        stderr.contains(&unreadable),
        "the error must name commit b ({unreadable}): {}",
        describe(&out)
    );
    assert!(
        !stderr.contains("skipped "),
        "an unreadable commit must not be reported as skipped: {}",
        describe(&out)
    );
}

/// Removes the loose object holding `commit`'s root tree, so git can no longer
/// list the files the commit touches.
fn remove_root_tree(fixture: &Fixture, commit: &str) {
    let tree = fixture.git(&["rev-parse", &format!("{commit}^{{tree}}")]);
    let object = fixture
        .work()
        .join(".git/objects")
        .join(&tree[..2])
        .join(&tree[2..]);
    assert!(
        object.is_file(),
        "premise: the commit's tree must be a loose object at {}",
        object.display()
    );
    std::fs::remove_file(&object).expect("remove the commit's tree object");
}

fn local_branches(fixture: &Fixture) -> String {
    fixture.git(&["for-each-ref", "--format=%(refname)", "refs/heads"])
}

/// Asserts the script stopped on `commit` with the error that names it.
fn assert_refused_as_unreadable(out: &Output, commit: &str) {
    assert!(
        !out.status.success(),
        "a commit git cannot read must stop the cut: {}",
        describe(out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!(
            "error: cannot list the files commit {commit} touches"
        )),
        "the error must name the commit ({commit}) whose files could not be listed: {}",
        describe(out)
    );
}

#[test]
fn an_unreadable_in_scope_commit_is_refused_before_the_pr_branch_is_created() {
    let fixture = Fixture::new();
    fixture.commit("feat(52.1-01): a", &["src/a.txt"]);
    let unreadable = fixture.commit("feat(52.1-02): b", &["src/b.txt"]);
    fixture.commit("feat(52.1-03): c", &["src/c.txt"]);
    remove_root_tree(&fixture, &unreadable);

    let out = fixture.cut(&[]);

    assert_refused_as_unreadable(&out, &unreadable);
    assert!(
        !fixture.pr_branch_exists(),
        "the cut must stop before the PR branch is created: {}",
        describe(&out)
    );
}

#[test]
fn an_unreadable_commit_is_refused_before_a_branch_is_created_without_a_phase() {
    let fixture = Fixture::new();
    fixture.git(&["checkout", "-q", "-b", "workspace/notes"]);
    fixture.commit("feat: a", &["src/a.txt"]);
    let unreadable = fixture.commit("feat: b", &["src/b.txt"]);
    fixture.commit("feat: c", &["src/c.txt"]);
    remove_root_tree(&fixture, &unreadable);
    let branches_before = local_branches(&fixture);

    let out = fixture.script(&[]).output().expect("run cut-pr-branch.sh");

    assert_refused_as_unreadable(&out, &unreadable);
    assert_eq!(
        local_branches(&fixture),
        branches_before,
        "the cut must stop before any branch is created: {}",
        describe(&out)
    );
}

#[test]
fn an_included_commit_is_replayed_in_its_original_order() {
    let fixture = Fixture::new();
    let shas = commit_code(&fixture, &OUT_OF_SCOPE_MIDDLE);

    let out = fixture.cut(&["--include", &shas[1]]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.pr_subjects(),
        OUT_OF_SCOPE_MIDDLE,
        "the included commit must be replayed between its neighbours: {}",
        describe(&out)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("chore(ci): b"),
        "an included commit must not be listed as skipped: {}",
        describe(&out)
    );
}

#[test]
fn an_include_given_with_an_equals_sign_is_replayed() {
    let fixture = Fixture::new();
    let shas = commit_code(&fixture, &OUT_OF_SCOPE_MIDDLE);

    let out = fixture.cut(&[&format!("--include={}", shas[1])]);

    assert!(
        out.status.success(),
        "the cut must pass: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.pr_subjects(),
        OUT_OF_SCOPE_MIDDLE,
        "the included commit must be replayed between its neighbours: {}",
        describe(&out)
    );
    let branches = fixture.git(&["for-each-ref", "--format=%(refname)", "refs/heads"]);
    assert!(
        branches
            .lines()
            .any(|branch| branch == format!("refs/heads/{PR_BRANCH}")),
        "the PR branch must keep its phase name, got {branches:?}"
    );
    assert!(
        !branches.contains("include"),
        "no branch may be named after the argument, got {branches:?}"
    );
}

#[test]
fn an_include_outside_the_range_is_refused() {
    let fixture = Fixture::new();
    let base = fixture.git(&["rev-parse", "develop"]);
    fixture.commit(SENTINEL, &["src/sentinel.txt"]);

    let out = fixture.cut(&["--include", &base]);

    assert!(
        !out.status.success(),
        "a commit outside the range must be refused: {}",
        describe(&out)
    );
    assert!(
        !fixture.pr_branch_exists(),
        "a refused cut must not create the PR branch: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("is not in origin/develop..feature/phase-52.1"),
        "the refusal must name the range: {}",
        describe(&out)
    );
}

#[test]
fn an_include_without_a_phase_is_refused() {
    let fixture = Fixture::new();
    let code = fixture.commit("chore(ci): b", &["src/b.txt"]);
    fixture.git(&["checkout", "-q", "-b", "workspace/notes"]);
    let branches_before = fixture.git(&["for-each-ref", "--format=%(refname)", "refs/heads"]);

    let out = fixture
        .script(&["--include", &code])
        .output()
        .expect("run cut-pr-branch.sh");

    assert!(
        !out.status.success(),
        "an include outside phase mode must be refused: {}",
        describe(&out)
    );
    assert_eq!(
        fixture.git(&["for-each-ref", "--format=%(refname)", "refs/heads"]),
        branches_before,
        "a refused cut must not create a branch: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--include needs a phase"),
        "the refusal must say an include needs a phase: {}",
        describe(&out)
    );
}

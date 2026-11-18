//! Behavioural tests for the workspace-divergence check in
//! `scripts/hooks/pre-push`.
//!
//! Each test builds a scratch repository and runs a byte copy of this
//! checkout's hook and of `scripts/check-workspace-divergence.sh` against it.
//! The copies live in a `hook-checkout` directory beside the scratch
//! repository, laid out like a checkout: that is how an absolute
//! `core.hooksPath` serves a push from another worktree. The hook therefore
//! resolves its drop-in directory and the divergence script beside the copy,
//! inside the scratch directory, so nothing a workspace adds to this
//! checkout's hook extension point can pass or fail a test. This checkout is
//! only read, and only for the three scripts the fixture copies.
//!
//! Every spawned command gets a scratch `HOME`, no global or system git
//! config and no inherited repository-local `GIT_*` variable.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use devflow_core::test_support::write_executable;

/// The all-zero object name git puts on a push line for a ref that does not
/// exist on that side.
const ZERO_SHA: &str = "0000000000000000000000000000000000000000";
const WORKSPACE_REF: &str = "refs/heads/workspace/x";
const FEATURE_REF: &str = "refs/heads/feature/y";
const MANIFEST: &str = ".workspace-divergence";
const DIVERGENCE_SCRIPT: &str = "scripts/check-workspace-divergence.sh";
/// A script the workspace branch adds, whose committed mode the check reads.
const ADDED_SCRIPT: &str = "scripts/x.sh";
/// The host check script the hook runs, as `scripts/check.sh service`, after
/// the container gate.
const CHECK_SCRIPT: &str = "scripts/check.sh";
/// The scripts `develop` carries in the scratch repository. The hook runs the
/// container script and then the host service target from the working
/// directory as its last steps.
const BASE_SCRIPTS: [&str; 4] = [
    "scripts/cut-pr-branch.sh",
    DIVERGENCE_SCRIPT,
    "scripts/check-in-container.sh",
    CHECK_SCRIPT,
];
const PASSING_STUB: &str = "#!/bin/sh\nexit 0\n";
/// A check script that names its arguments on stderr and fails.
const FAILING_CHECK_STUB: &str = "#!/bin/sh\necho \"check.sh stub failed: $*\" >&2\nexit 1\n";
/// The two modes git records for a regular file.
const EXECUTABLE: &str = "100755";
const NOT_EXECUTABLE: &str = "100644";
/// Every message the divergence script prints, a pass or a refusal, opens
/// with this, so its absence from both streams means the script never ran.
const CHECK_PREFIX: &str = "check-workspace-divergence:";
const EVERY_DIFFERENCE_DECLARED: &str =
    "check-workspace-divergence: every difference from origin/develop is declared.";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

/// The text of one of this checkout's scripts.
fn real_script(path: &str) -> String {
    let source = repo_root().join(path);
    std::fs::read_to_string(&source)
        .unwrap_or_else(|error| panic!("read {}: {error}", source.display()))
}

fn describe(out: &Output) -> String {
    format!(
        "status {:?}, stdout {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The push line git feeds the hook for a branch pushed to a remote that does
/// not have it yet.
fn first_push_of(reference: &str, sha: &str) -> String {
    format!("{reference} {sha} {reference} {ZERO_SHA}")
}

/// A scratch repository on `develop`, with `origin/develop` at its tip, and a
/// copy of the hook and its divergence script beside it.
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
        let hook_checkout = root.join("hook-checkout");
        std::fs::create_dir_all(hook_checkout.join("scripts/hooks"))
            .expect("create the hook checkout");
        for script in ["scripts/hooks/pre-push", DIVERGENCE_SCRIPT] {
            write_executable(&hook_checkout.join(script), &real_script(script));
        }

        let init = fixture
            .hermetic("git", root)
            .args(["init", "-q", "-b", "develop", "repo"])
            .output()
            .expect("run git init");
        assert!(init.status.success(), "init repo: {}", describe(&init));
        fixture.git(&["config", "user.email", "test@example.com"]);
        fixture.git(&["config", "user.name", "test"]);
        fixture.commit_base();
        fixture
    }

    /// Commits `develop`'s base: a README and the four scripts, each
    /// committed executable, then points `origin/develop` at it.
    fn commit_base(&self) {
        self.write("README.md", "base\n");
        self.git(&["add", "--", "README.md"]);
        self.add_executable(
            "scripts/cut-pr-branch.sh",
            &real_script("scripts/cut-pr-branch.sh"),
        );
        self.add_executable(DIVERGENCE_SCRIPT, &real_script(DIVERGENCE_SCRIPT));
        self.add_executable("scripts/check-in-container.sh", PASSING_STUB);
        self.add_executable(CHECK_SCRIPT, PASSING_STUB);
        self.git(&["commit", "-q", "-m", "chore: base"]);

        let not_executable: Vec<&str> = BASE_SCRIPTS
            .into_iter()
            .filter(|script| self.committed_mode("develop", script) != EXECUTABLE)
            .collect();
        assert!(
            not_executable.is_empty(),
            "the base commit must carry these at 100755, or the mode check fails every \
             push for a reason no test names: {not_executable:?}"
        );
        self.git(&["update-ref", "refs/remotes/origin/develop", "develop"]);
    }

    fn repo(&self) -> PathBuf {
        self.scratch.path().join("repo")
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

    /// Runs a git command that must succeed and returns its trimmed stdout.
    fn git(&self, args: &[&str]) -> String {
        let out = self
            .hermetic("git", &self.repo())
            .args(args)
            .output()
            .expect("run git");
        assert!(out.status.success(), "git {args:?}: {}", describe(&out));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Writes a file that is never executed.
    fn write(&self, path: &str, contents: &str) {
        let file = self.repo().join(path);
        std::fs::create_dir_all(file.parent().expect("a file has a parent"))
            .expect("create the file's directory");
        std::fs::write(&file, contents).expect("write file");
    }

    /// Writes a script and stages it executable, whatever the scratch
    /// repository's `core.fileMode` came out as.
    fn add_executable(&self, path: &str, contents: &str) {
        let file = self.repo().join(path);
        std::fs::create_dir_all(file.parent().expect("a file has a parent"))
            .expect("create the script's directory");
        write_executable(&file, contents);
        self.git(&["add", "--", path]);
        self.stage_mode(path, EXECUTABLE);
    }

    /// Stages `path` with `mode`, leaving the file on disk as it is.
    fn stage_mode(&self, path: &str, mode: &str) {
        let flag = if mode == EXECUTABLE {
            "--chmod=+x"
        } else {
            "--chmod=-x"
        };
        self.git(&["update-index", flag, "--", path]);
    }

    /// Commits what is staged and returns the commit's full SHA.
    fn commit(&self, subject: &str) -> String {
        self.git(&["commit", "-q", "-m", subject]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// The mode `path` is committed with at `rev`.
    fn committed_mode(&self, rev: &str, path: &str) -> String {
        let entry = self.git(&["ls-tree", rev, "--", path]);
        entry.split(' ').next().unwrap_or_default().to_owned()
    }

    /// The mode `path` is staged with.
    fn index_mode(&self, path: &str) -> String {
        let entry = self.git(&["ls-files", "-s", "--", path]);
        entry.split(' ').next().unwrap_or_default().to_owned()
    }

    /// Checks out a new `workspace/x` from `develop` whose manifest declares
    /// nothing while the branch edits `develop`'s README, and returns its
    /// commit. A check of this commit must refuse it.
    fn wrong_workspace_branch(&self) -> String {
        self.git(&["checkout", "-q", "-b", "workspace/x", "develop"]);
        self.write(MANIFEST, "# declares nothing\n");
        self.write("README.md", "edited on the workspace branch\n");
        self.git(&["add", "--", MANIFEST, "README.md"]);
        self.commit("chore: undeclared readme edit")
    }

    /// Checks out a new `workspace/x` from `develop` that adds one script,
    /// committed with `mode`, and declares it, and returns its commit. The
    /// manifest is itself a built-in path, so the script's mode is all a
    /// check can object to.
    fn workspace_branch_adding_a_script(&self, mode: &str) -> String {
        self.git(&["checkout", "-q", "-b", "workspace/x", "develop"]);
        self.write(MANIFEST, &format!("only {ADDED_SCRIPT}\n"));
        self.git(&["add", "--", MANIFEST]);
        self.add_executable(ADDED_SCRIPT, PASSING_STUB);
        self.stage_mode(ADDED_SCRIPT, mode);
        self.commit("chore: add a declared script")
    }

    /// Runs the hook copy in the scratch repository, as git runs it for a
    /// push to `origin`, with `line` as the one ref being pushed.
    fn push(&self, line: &str) -> Output {
        let hook = self
            .scratch
            .path()
            .join("hook-checkout/scripts/hooks/pre-push");
        let mut child = self
            .hermetic("bash", &self.repo())
            .arg(hook)
            .arg("origin")
            // The remote's URL. The hook hands it to drop-ins and reads
            // nothing from it, so no repository exists there.
            .arg(self.scratch.path().join("origin.git"))
            // An inherited `1` would run the real host checks in place of
            // the container stub.
            .env_remove("DEVFLOW_SKIP_CONTAINER_CHECK")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run the hook");
        child
            .stdin
            .take()
            .expect("the hook's stdin is piped")
            .write_all(format!("{line}\n").as_bytes())
            .expect("write the push line");
        child.wait_with_output().expect("wait for the hook")
    }
}

#[test]
fn a_wrong_workspace_branch_pushed_from_a_develop_checkout_is_refused() {
    let fixture = Fixture::new();
    let sha = fixture.wrong_workspace_branch();
    fixture.git(&["checkout", "-q", "develop"]);
    assert!(
        !fixture.repo().join(MANIFEST).exists(),
        "premise: a develop-based checkout has no manifest on disk"
    );
    // The checkout's own copy of the script now passes everything. A hook
    // that ran it, and not the copy beside the hook, would let the push through.
    write_executable(&fixture.repo().join(DIVERGENCE_SCRIPT), PASSING_STUB);

    let out = fixture.push(&first_push_of(WORKSPACE_REF, &sha));

    assert!(
        !out.status.success(),
        "a wrong workspace branch must be refused whatever the checkout holds: {}",
        describe(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "diverges from origin/develop in ways .workspace-divergence does not declare"
        ),
        "the refusal must come from the divergence check: {}",
        describe(&out)
    );
    assert!(
        stderr.contains("README.md"),
        "the refusal must name the undeclared file: {}",
        describe(&out)
    );
}

#[test]
fn a_clean_pr_branch_pushed_from_a_workspace_checkout_passes() {
    let fixture = Fixture::new();
    fixture.git(&["checkout", "-q", "-b", "feature/y", "develop"]);
    fixture.write("notes.txt", "a new file\n");
    fixture.git(&["add", "--", "notes.txt"]);
    let sha = fixture.commit("feat: a new file");
    fixture.wrong_workspace_branch();
    assert!(
        fixture.repo().join(MANIFEST).exists(),
        "premise: the checkout is on the workspace branch, with its manifest on disk"
    );

    let out = fixture.push(&first_push_of(FEATURE_REF, &sha));

    assert!(
        out.status.success(),
        "a pushed commit that carries no manifest must not be judged by the checkout's: {}",
        describe(&out)
    );
}

#[test]
fn a_pushed_commits_script_mode_is_judged_by_the_commit_not_the_index() {
    let fixture = Fixture::new();
    let sha = fixture.workspace_branch_adding_a_script(EXECUTABLE);
    fixture.stage_mode(ADDED_SCRIPT, NOT_EXECUTABLE);
    assert_eq!(
        fixture.committed_mode(&sha, ADDED_SCRIPT),
        EXECUTABLE,
        "premise: the pushed commit carries the script executable"
    );
    assert_eq!(
        fixture.index_mode(ADDED_SCRIPT),
        NOT_EXECUTABLE,
        "premise: the index stages it non-executable"
    );

    let out = fixture.push(&first_push_of(WORKSPACE_REF, &sha));

    assert!(
        out.status.success(),
        "the index's mode must not refuse a commit that carries the script executable: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(EVERY_DIFFERENCE_DECLARED),
        "the check must have run and passed, not been skipped: {}",
        describe(&out)
    );
}

#[test]
fn a_non_executable_script_at_the_pushed_commit_is_refused() {
    let fixture = Fixture::new();
    let sha = fixture.workspace_branch_adding_a_script(NOT_EXECUTABLE);
    fixture.stage_mode(ADDED_SCRIPT, EXECUTABLE);
    assert_eq!(
        fixture.committed_mode(&sha, ADDED_SCRIPT),
        NOT_EXECUTABLE,
        "premise: the pushed commit carries the script non-executable"
    );
    assert_eq!(
        fixture.index_mode(ADDED_SCRIPT),
        EXECUTABLE,
        "premise: the index stages it executable"
    );

    let out = fixture.push(&first_push_of(WORKSPACE_REF, &sha));

    assert!(
        !out.status.success(),
        "a script the pushed commit carries non-executable must be refused, whatever the \
         index stages: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("not committed executable (100644): scripts/x.sh"),
        "the refusal must name the script and the mode the commit carries: {}",
        describe(&out)
    );
}

#[test]
fn a_delete_push_is_skipped() {
    let fixture = Fixture::new();
    let sha = fixture.wrong_workspace_branch();
    let direct = fixture
        .hermetic("bash", &fixture.repo())
        .arg(DIVERGENCE_SCRIPT)
        .args(["origin/develop", "HEAD"])
        .output()
        .expect("run the divergence script");
    let direct_stderr = String::from_utf8_lossy(&direct.stderr);
    assert!(
        !direct.status.success(),
        "premise: a check of this checkout refuses it: {}",
        describe(&direct)
    );
    assert!(
        direct_stderr.contains(CHECK_PREFIX),
        "premise: the refusal carries the script's prefix: {}",
        describe(&direct)
    );
    assert!(
        direct_stderr.contains("README.md"),
        "premise: the refusal is the undeclared file: {}",
        describe(&direct)
    );

    let out = fixture.push(&format!("(delete) {ZERO_SHA} {WORKSPACE_REF} {sha}"));

    assert!(
        out.status.success(),
        "a delete pushes no commit, so there is nothing to check: {}",
        describe(&out)
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains(CHECK_PREFIX),
        "the divergence script must not run for a delete (stdout): {}",
        describe(&out)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains(CHECK_PREFIX),
        "the divergence script must not run for a delete (stderr): {}",
        describe(&out)
    );
}

#[test]
fn a_pushed_commit_is_judged_by_its_own_manifest_and_built_in_list() {
    let fixture = Fixture::new();
    let sha = fixture.workspace_branch_adding_a_script(EXECUTABLE);
    // Uncommitted edits to the two files the check reads its rules from. Read
    // from the checkout, the first leaves the added script undeclared and the
    // second stops the manifest being a built-in path.
    fixture.write(MANIFEST, "# declares nothing\n");
    fixture.write(
        "scripts/cut-pr-branch.sh",
        "#!/bin/sh\nFORBIDDEN_PATHS=(.nothing)\n",
    );

    let out = fixture.push(&first_push_of(WORKSPACE_REF, &sha));

    assert!(
        out.status.success(),
        "the checkout's uncommitted manifest and built-in list must not refuse a commit \
         that declares every difference: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(EVERY_DIFFERENCE_DECLARED),
        "the check must have run and passed, not been skipped: {}",
        describe(&out)
    );
}

#[test]
fn a_failing_service_target_refuses_the_push() {
    let fixture = Fixture::new();
    fixture.git(&["checkout", "-q", "-b", "feature/y", "develop"]);
    fixture.write("notes.txt", "a new file\n");
    fixture.git(&["add", "--", "notes.txt"]);
    let sha = fixture.commit("feat: a new file");
    // The push a_clean_pr_branch_pushed_from_a_workspace_checkout_passes
    // accepts, with only the checkout's host check script now failing.
    write_executable(&fixture.repo().join(CHECK_SCRIPT), FAILING_CHECK_STUB);

    let out = fixture.push(&first_push_of(FEATURE_REF, &sha));

    assert!(
        !out.status.success(),
        "a failing `scripts/check.sh service` must refuse the push: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("pre-push: scripts/check.sh service"),
        "the hook must announce the service target: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("check.sh stub failed: service"),
        "the hook must run `scripts/check.sh service` itself: {}",
        describe(&out)
    );
}

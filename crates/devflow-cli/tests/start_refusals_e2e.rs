//! `devflow start`'s refusals over work an earlier run left (52.1 D-09.1, D-10, D-11):
//! `start` has no `--force`, removes nothing, and names `devflow discard --phase N`
//! for a leftover phase worktree or branch.

mod common;

use common::HermeticPath;
use devflow_core::phase_id::PhaseId;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

/// The phase every test here starts; the fixture pre-bakes its CONTEXT.md and PLAN.md.
const PHASE: u32 = 12;

/// A fake agent that reports success, so a start that launches reaches a gate.
const SUCCESS_AGENT: &str = "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n";

fn git(root: &Path, args: &[&str]) -> Output {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git(root, args).stdout)
        .trim()
        .to_string()
}

/// A repository on `develop` with phase 12's GSD context and a project config, as a real
/// GSD project has.
fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    let dir = root.join(".planning/phases/12-test");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("12-CONTEXT.md"), "ctx\n").unwrap();
    fs::write(dir.join("12-01-PLAN.md"), "plan\n").unwrap();
    fs::write(
        root.join(".planning/config.json"),
        "{\n  \"workflow\": {\n    \"auto_advance\": false\n  }\n}\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// `feature/phase-12` with one commit not merged into `develop`; `develop` stays checked out.
fn seed_unmerged_phase_branch(root: &Path) -> String {
    let branch = phase_branch();
    git(root, &["checkout", "-q", "-b", &branch]);
    fs::write(root.join("initial.txt"), "initial phase work\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial phase work"]);
    git(root, &["checkout", "-q", "develop"]);
    branch
}

fn phase_branch() -> String {
    format!("feature/phase-{}", PhaseId::new(PHASE).padded())
}

/// `branch`'s tip, or `None` when it does not exist.
fn branch_tip(root: &Path, branch: &str) -> Option<String> {
    let out = devflow_core::test_support::git_command(root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .expect("spawn git rev-parse");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fake_codex() -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("codex");
    fs::write(&path, SUCCESS_AGENT).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let path = dir.path().to_path_buf();
    FakeBin { _dir: dir, path }
}

/// `devflow start --phase 12 --agent codex --mode supervise` plus `extra` over `root`, with
/// its exit status left to the caller. A reaper is bound before the launch, so a start
/// that does launch has its run ended when the test ends.
fn start_phase(
    home: &common::HermeticHome,
    root: &Path,
    extra: &[&str],
) -> (Output, common::LaunchedRunReaper) {
    let reaper = common::LaunchedRunReaper::for_home(home, root, PhaseId::new(PHASE));
    let fake_bin = fake_codex();
    let parent = std::env::var_os("PATH").unwrap_or_default();
    let output = common::hermetic_devflow(home)
        .args(["start", "--phase", &PHASE.to_string()])
        .args(["--agent", "codex", "--mode", "supervise"])
        .args(extra)
        .arg(root)
        .hermetic_path(format!(
            "{}:{}",
            fake_bin.path.display(),
            parent.to_string_lossy()
        ))
        .env("DEVFLOW_TEST_ROOT", root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "60")
        .current_dir(root)
        .output()
        .expect("run devflow start");
    (output, reaper)
}

fn combined(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr)
}

/// Assert `start` refused, named `devflow discard --phase 12`, offered no `--force` and no
/// raw delete, and wrote no run state.
fn assert_refused_naming_discard(root: &Path, out: &Output) {
    let text = combined(out);
    assert!(
        !out.status.success(),
        "start over a leftover must refuse: {text}"
    );
    assert!(
        text.contains(&format!("devflow discard --phase {PHASE}")),
        "refusal does not name discard: {text}"
    );
    assert!(
        !text.contains("--force"),
        "the refusal offers --force: {text}"
    );
    assert!(
        !text.contains("git branch -D"),
        "the refusal offers a raw branch delete: {text}"
    );
    assert!(
        !devflow_core::workflow::state_path(root, PhaseId::new(PHASE)).exists(),
        "a refused start leaves no run state: {text}"
    );
}

/// 52.1 D-11: `start` has no `--force`; clap rejects it as an unknown argument.
#[test]
fn start_rejects_the_removed_force_flag() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    let (out, _reaper) = start_phase(&home, root, &["--force"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && stderr.contains("unexpected argument '--force'"),
        "force accepted: exit {:?}\n{}",
        out.status.code(),
        combined(&out)
    );
}

/// 52.1 D-09.1, D-10: a phase worktree with no run state makes `start` refuse naming
/// `devflow discard --phase N`; the worktree and its unmerged branch are left as they were.
#[test]
fn start_over_a_leftover_worktree_names_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let branch = seed_unmerged_phase_branch(root);
    let wt = root.join(".worktrees/phase-12");
    devflow_core::worktree::add(root, &wt, &branch, &branch, false).unwrap();
    fs::write(wt.join("uncommitted.txt"), "work in progress\n").unwrap();
    let tip = branch_tip(root, &branch).expect("premise: the branch exists");

    let (out, _reaper) = start_phase(&home, root, &[]);
    assert_refused_naming_discard(root, &out);
    let text = combined(&out);
    assert!(
        wt.join("uncommitted.txt").is_file(),
        "the worktree and its uncommitted file are left: {text}"
    );
    assert_eq!(
        branch_tip(root, &branch).as_deref(),
        Some(tip.as_str()),
        "branch tip moved: {text}"
    );
}

/// 52.1 D-09.1, D-10: the `--no-worktree` arm: a phase branch with no run state makes
/// `start` refuse naming `devflow discard --phase N`, with the branch at its tip and the
/// primary checkout still on `develop`.
#[test]
fn start_no_worktree_over_a_leftover_branch_names_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let branch = seed_unmerged_phase_branch(root);
    let tip = branch_tip(root, &branch).expect("premise: the branch exists");

    let (out, _reaper) = start_phase(&home, root, &["--no-worktree"]);
    assert_refused_naming_discard(root, &out);
    let text = combined(&out);
    assert_eq!(
        branch_tip(root, &branch).as_deref(),
        Some(tip.as_str()),
        "branch tip moved: {text}"
    );
    assert_eq!(
        git_stdout(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "develop",
        "a refused start leaves the primary checkout on its branch: {text}"
    );
}

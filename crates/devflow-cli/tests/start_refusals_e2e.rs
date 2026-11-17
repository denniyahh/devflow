//! `devflow start`'s refusals over work an earlier run left (52.1 D-09.1, D-10, D-11):
//! `start` has no `--force`, removes nothing, and names `devflow discard --phase N`
//! for a leftover phase worktree or branch. Every refusal comes from one guard sequence
//! (D-12, D-26): live, finished, incomplete, unloadable, orphan gate files, leftovers.

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

/// Wait until phase 12's run settles at a gate or a stop, so the reaper ends a quiet run.
fn wait_for_settled(root: &Path) {
    for _ in 0..400 {
        if let Ok(state) = devflow_core::workflow::load_state(root, PhaseId::new(PHASE))
            && (state.gate_pending || state.stopped)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("timed out waiting for phase {PHASE} to settle (gate or stop)");
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

/// How far behind the base the checkout `start --no-worktree` runs from is: past the
/// removed check's 50-commit refusal.
const COMMITS_BEHIND: usize = 51;

/// 52.1 D-24: `start --no-worktree` from a branch more than 50 commits behind the base
/// succeeds with no "behind" warning, and cuts the phase branch from the base: its tip is
/// the base's tip, not the checkout's (control: the two tips differ).
#[test]
fn start_no_worktree_far_behind_the_base_cuts_the_branch_from_the_base() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    git(root, &["branch", "behind"]);
    for n in 0..COMMITS_BEHIND {
        git(
            root,
            &["commit", "-q", "--allow-empty", "-m", &format!("base {n}")],
        );
    }
    git(root, &["checkout", "-q", "behind"]);
    let base_tip = branch_tip(root, "develop").expect("premise: the base exists");
    let checkout_tip = branch_tip(root, "behind").expect("premise: the checkout's branch");
    assert_ne!(
        base_tip, checkout_tip,
        "control: the checkout is not at the base's tip"
    );
    assert_eq!(
        git_stdout(root, &["rev-list", "--count", "behind..develop"]),
        COMMITS_BEHIND.to_string(),
        "premise: the checkout is {COMMITS_BEHIND} commits behind the base"
    );

    let (out, _reaper) = start_phase(&home, root, &["--no-worktree"]);
    let text = combined(&out);
    assert!(
        out.status.success(),
        "start refused as too far behind the base: {text}"
    );
    assert!(
        !text.contains("commits ahead"),
        "start printed the removed behind-the-base warning: {text}"
    );
    assert_eq!(
        branch_tip(root, &phase_branch()).as_deref(),
        Some(base_tip.as_str()),
        "branch tip differs from base: {text}"
    );
    wait_for_settled(root);
}

/// Save phase 12's state in `root`, as `edit` leaves it over a fresh one.
fn save_phase_state(root: &Path, edit: impl FnOnce(&mut devflow_core::state::State)) {
    let mut state = devflow_core::state::State::new(
        PhaseId::new(PHASE),
        devflow_core::state::AgentKind::Codex,
        devflow_core::mode::Mode::Supervise,
        root.to_path_buf(),
    );
    edit(&mut state);
    devflow_core::workflow::save_state(&state).expect("save the planted state");
}

/// Phase 12's state file bytes; the state must exist.
fn state_bytes(root: &Path) -> Vec<u8> {
    fs::read(devflow_core::workflow::state_path(
        root,
        PhaseId::new(PHASE),
    ))
    .expect("read the state file")
}

/// Assert `start` refused, offered no `--force`, and left phase 12's state file with
/// `before`'s bytes; returns the refusal text.
fn assert_refused_leaving_the_state(root: &Path, out: &Output, before: &[u8]) -> String {
    let text = combined(out);
    assert!(
        !text.contains("--force"),
        "the refusal offers --force: {text}"
    );
    assert_eq!(
        state_bytes(root),
        before,
        "a refused start changed the state file: {text}"
    );
    text
}

/// 52.1 D-09.1: `start` over a finished run refuses, says the phase finished and names
/// the close-out and `devflow discard --phase N`; the state is left byte-identical.
#[test]
fn start_refuses_a_finished_phase_naming_close_out_or_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    save_phase_state(root, |state| {
        state.verification_baseline_captured = true;
        state.finished = true;
    });
    let before = state_bytes(root);

    let (out, _reaper) = start_phase(&home, root, &[]);
    assert!(
        !out.status.success(),
        "finished phase restarted: {}",
        combined(&out)
    );
    let text = assert_refused_leaving_the_state(root, &out, &before);
    assert!(
        text.contains("finished") && text.contains("close the phase out"),
        "the refusal does not say the phase finished and name the close-out: {text}"
    );
    assert!(
        text.contains(&format!("devflow discard --phase {PHASE}")),
        "refusal does not name discard: {text}"
    );
}

/// 52.1 D-09.1: `start` over an incomplete run refuses and names
/// `devflow resume --phase N`; the state is left byte-identical.
#[test]
fn start_refuses_an_incomplete_phase_naming_resume() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    save_phase_state(root, |state| {
        state.verification_baseline_captured = true;
        state.stage = devflow_core::stage::Stage::Code;
    });
    let before = state_bytes(root);

    let (out, _reaper) = start_phase(&home, root, &[]);
    assert!(
        !out.status.success(),
        "an incomplete phase restarted: {}",
        combined(&out)
    );
    let text = assert_refused_leaving_the_state(root, &out, &before);
    assert!(
        text.contains(&format!("devflow resume --phase {PHASE}")),
        "the incomplete-run refusal does not name resume: {text}"
    );
}

/// Bytes no state deserializes.
const UNLOADABLE_STATE: &[u8] = b"{ this is not a state";

/// 52.1 D-20: a state file that does not load makes `start` refuse naming
/// `devflow discard --phase N`; the file stays as it was and no copy of it is written.
#[test]
fn start_refuses_an_unloadable_state_naming_discard_and_leaves_it_in_place() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let state_file = devflow_core::workflow::state_path(root, PhaseId::new(PHASE));
    fs::create_dir_all(state_file.parent().unwrap()).unwrap();
    fs::write(&state_file, UNLOADABLE_STATE).unwrap();

    let (out, _reaper) = start_phase(&home, root, &[]);
    assert!(
        !out.status.success(),
        "start replaced an unloadable state: {}",
        combined(&out)
    );
    let text = assert_refused_leaving_the_state(root, &out, UNLOADABLE_STATE);
    assert!(
        text.contains(&format!("devflow discard --phase {PHASE}")),
        "the unloadable-state refusal does not name discard: {text}"
    );
    let copies: Vec<String> = fs::read_dir(root.join(".devflow"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("unloadable-state-"))
        .collect();
    assert!(
        copies.is_empty(),
        "a copy of the state was written: {copies:?}\n{text}"
    );
}

/// 52.1 D-20: a state that only a killed `start`'s reservation left (it never prepared
/// the run) counts as no run, so `start` proceeds and prepares a new one.
#[test]
fn start_proceeds_over_a_reservation_only_state() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    save_phase_state(root, |state| {
        assert!(
            !state.verification_baseline_captured,
            "premise: a fresh state is reservation-only"
        );
    });

    let (out, _reaper) = start_phase(&home, root, &[]);
    let text = combined(&out);
    assert!(
        out.status.success(),
        "start refused a reservation-only state: {text}"
    );
    let state = devflow_core::workflow::load_state(root, PhaseId::new(PHASE))
        .expect("the new run's state loads");
    assert!(
        state.verification_baseline_captured,
        "the start did not prepare a new run over the reservation: {text}"
    );
    wait_for_settled(root);
}

/// D-09a: gate files with no run make `start` refuse naming `devflow discard --phase N`;
/// the gate file is left in place and no run state is written.
#[test]
fn start_refuses_orphan_gate_files_naming_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let gate = devflow_core::gates::Gates::open_gate(
        root,
        PhaseId::new(PHASE),
        devflow_core::stage::Stage::Code,
        devflow_core::gates::GateKind::StageFailure,
        "planted orphan Code gate",
    )
    .expect("plant an orphan gate");

    let (out, _reaper) = start_phase(&home, root, &[]);
    assert_refused_naming_discard(root, &out);
    assert!(
        gate.path.is_file(),
        "the orphan gate file was removed: {}",
        combined(&out)
    );
}

/// `devflow resume --phase 12` over `root`, with the fake agent on `PATH` and a reaper
/// bound before it, so a resume that does launch has its run ended when the test ends.
fn resume_phase(home: &common::HermeticHome, root: &Path) -> (Output, common::LaunchedRunReaper) {
    let reaper = common::LaunchedRunReaper::for_home(home, root, PhaseId::new(PHASE));
    let fake_bin = fake_codex();
    let parent = std::env::var_os("PATH").unwrap_or_default();
    let output = common::hermetic_devflow(home)
        .args(["resume", "--phase", &PHASE.to_string()])
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
        .expect("run devflow resume");
    (output, reaper)
}

/// Assert a refusal names only verbs that exist: no `--force` and no removed `recover`.
fn assert_no_removed_verb(text: &str) {
    assert!(
        !text.contains("--force"),
        "the refusal offers --force: {text}"
    );
    assert!(
        !text.contains("devflow recover"),
        "the refusal names the removed recover: {text}"
    );
}

/// 52.1 D-09.1: `resume` over a finished run refuses, saying the phase finished and
/// naming the close-out and `devflow discard --phase N`, not `start`; the state is left
/// byte-identical.
#[test]
fn resume_refuses_a_finished_run_naming_close_out_or_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    save_phase_state(root, |state| {
        state.verification_baseline_captured = true;
        state.finished = true;
    });
    let before = state_bytes(root);

    let (out, _reaper) = resume_phase(&home, root);
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "resume relaunched a finished run: {text}"
    );
    assert_eq!(
        state_bytes(root),
        before,
        "a refused resume changed the state: {text}"
    );
    assert_no_removed_verb(&text);
    assert!(
        text.contains("close the phase out"),
        "the refusal does not name the close-out: {text}"
    );
    assert!(
        text.contains(&format!("devflow discard --phase {PHASE}")),
        "the refusal does not name discard: {text}"
    );
    assert!(
        !text.contains("devflow start"),
        "the refusal names start, which refuses a finished phase: {text}"
    );
}

/// 52.1 D-09.1: `resume` with no run state refuses, naming `devflow start --phase N`,
/// and writes no state.
#[test]
fn resume_with_no_state_names_start() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    let (out, _reaper) = resume_phase(&home, root);
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "resume with no state succeeded: {text}"
    );
    assert_no_removed_verb(&text);
    assert!(
        text.contains(&format!("devflow start --phase {PHASE}")),
        "the refusal does not name start: {text}"
    );
    assert!(
        !devflow_core::workflow::state_path(root, PhaseId::new(PHASE)).exists(),
        "a refused resume wrote a state: {text}"
    );
}

/// Every file under `root`'s `.devflow/` with its bytes, sorted by path; empty when the
/// directory does not exist.
fn devflow_tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let bytes = fs::read(&path).unwrap_or_default();
                out.push((path.strip_prefix(base).unwrap().to_path_buf(), bytes));
            }
        }
    }
    let mut files = Vec::new();
    let dir = root.join(".devflow");
    walk(&dir, &dir, &mut files);
    files.sort();
    files
}

/// `.devflow/`'s files and every ref in `root`, the two things a start writes.
fn repo_snapshot(root: &Path) -> (Vec<(PathBuf, Vec<u8>)>, String) {
    (devflow_tree(root), git_stdout(root, &["for-each-ref"]))
}

/// 52.1 D-26: `start --dry-run` over a finished phase prints the refusal a real
/// `start` prints, and fails as it does.
#[test]
fn a_dry_run_over_a_finished_phase_prints_starts_refusal() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    save_phase_state(root, |state| {
        state.verification_baseline_captured = true;
        state.finished = true;
    });

    let (real, _reaper) = start_phase(&home, root, &[]);
    assert!(
        !real.status.success(),
        "premise: start refuses a finished phase: {}",
        combined(&real)
    );
    let (dry, _dry_reaper) = start_phase(&home, root, &["--dry-run"]);
    assert!(
        !dry.status.success(),
        "dry run text differs from start: the dry run passed a phase start refuses\n\
         start:\n{}\ndry run:\n{}",
        combined(&real),
        combined(&dry)
    );
    assert_eq!(
        String::from_utf8_lossy(&dry.stderr),
        String::from_utf8_lossy(&real.stderr),
        "dry run text differs from start"
    );
}

/// 52.1 D-26: a dry run that passes writes nothing: `.devflow/` and every ref are as
/// they were. Control: a real start over the same repository changes both.
#[test]
fn a_dry_run_writes_nothing() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    let before = repo_snapshot(root);
    let (dry, _dry_reaper) = start_phase(&home, root, &["--dry-run"]);
    assert!(
        dry.status.success(),
        "premise: the dry run passes: {}",
        combined(&dry)
    );
    let after = repo_snapshot(root);
    assert_eq!(
        after.0,
        before.0,
        "dry run wrote under .devflow/: {}",
        combined(&dry)
    );
    assert_eq!(after.1, before.1, "dry run wrote a ref: {}", combined(&dry));

    let (real, _reaper) = start_phase(&home, root, &[]);
    assert!(
        real.status.success(),
        "control: a real start launches: {}",
        combined(&real)
    );
    wait_for_settled(root);
    let started = repo_snapshot(root);
    assert_ne!(
        started.0, before.0,
        "control: a real start wrote no .devflow/ file"
    );
    assert_ne!(started.1, before.1, "control: a real start wrote no ref");
}

/// `devflow start --phase 12 --agent codex --mode supervise` plus `extra` over `root`,
/// with `path` after the fake service manager on `PATH` and `envs` set.
fn start_phase_on_path(
    home: &common::HermeticHome,
    root: &Path,
    extra: &[&str],
    path: &std::ffi::OsStr,
    envs: &[(&str, &str)],
) -> (Output, common::LaunchedRunReaper) {
    let reaper = common::LaunchedRunReaper::for_home(home, root, PhaseId::new(PHASE));
    let output = common::hermetic_devflow(home)
        .args(["start", "--phase", &PHASE.to_string()])
        .args(["--agent", "codex", "--mode", "supervise"])
        .args(extra)
        .arg(root)
        .hermetic_path(path)
        .envs(envs.iter().copied())
        .env("DEVFLOW_TEST_ROOT", root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "60")
        .current_dir(root)
        .output()
        .expect("run devflow start");
    (output, reaper)
}

/// A `PATH` dir holding only a `git` link: no agent binary.
fn git_only_dir() -> tempfile::TempDir {
    let parent = std::env::var_os("PATH").unwrap_or_default();
    let git = std::env::split_paths(&parent)
        .map(|dir| dir.join("git"))
        .find(|path| path.is_file())
        .expect("git on PATH");
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(git, dir.path().join("git")).unwrap();
    dir
}

/// Assert `dry` failed with `real`'s stderr, naming `check` when it did not.
fn assert_same_refusal(check: &str, real: &Output, dry: &Output) {
    assert!(
        !real.status.success(),
        "premise: start refuses on the {check}: {}",
        combined(real)
    );
    assert!(
        !dry.status.success(),
        "the {check} text differs: the dry run passed what start refuses\nstart:\n{}\ndry run:\n{}",
        combined(real),
        combined(dry)
    );
    assert_eq!(
        String::from_utf8_lossy(&dry.stderr),
        String::from_utf8_lossy(&real.stderr),
        "the {check} text differs between the dry run and start"
    );
}

/// 52.1 D-27: with no agent binary on `PATH`, the dry run refuses as `start` does.
#[test]
fn a_dry_run_refuses_a_missing_agent_binary_like_start() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let path_dir = git_only_dir();

    let path = path_dir.path().as_os_str();
    let (real, _reaper) = start_phase_on_path(&home, root, &[], path, &[]);
    let (dry, _dry_reaper) = start_phase_on_path(&home, root, &["--dry-run"], path, &[]);
    assert_same_refusal("agent binary check", &real, &dry);
    assert!(
        String::from_utf8_lossy(&dry.stderr).contains("agent binary `codex` not found"),
        "the agent binary check text is missing: {}",
        combined(&dry)
    );
}

/// 52.1 D-27: a configured base that is not a local branch refuses the dry run as it
/// refuses `start`.
#[test]
fn a_dry_run_refuses_an_unresolvable_base_like_start() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_codex();
    let parent = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), parent.to_string_lossy());
    let envs = [("DEVFLOW_BASE_BRANCH", "nonexistent-base-xyz")];

    let (real, _reaper) = start_phase_on_path(&home, root, &[], path.as_ref(), &envs);
    let (dry, _dry_reaper) = start_phase_on_path(&home, root, &["--dry-run"], path.as_ref(), &envs);
    assert_same_refusal("base resolution", &real, &dry);
    assert!(
        String::from_utf8_lossy(&dry.stderr).contains("nonexistent-base-xyz"),
        "the base resolution text does not name the base: {}",
        combined(&dry)
    );
}

/// 52.1 D-27: a dry run that passes prints one line naming every start check it skipped.
#[test]
fn a_dry_run_names_every_check_it_skipped() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    let (dry, _reaper) = start_phase(&home, root, &["--dry-run"]);
    let text = combined(&dry);
    assert!(dry.status.success(), "premise: the dry run passes: {text}");
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("not checked"))
        .collect();
    assert_eq!(lines.len(), 1, "no one skipped-checks line: {text}");
    for check in [
        "base currency",
        "phase reachability",
        "CONTEXT.md",
        "PLAN.md",
    ] {
        assert!(
            lines[0].contains(check),
            "the skipped line does not name {check}: {}",
            lines[0]
        );
    }
}

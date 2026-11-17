//! Effect tests of `cleanup`'s rule (52.1 D-09.5 as amended, D-16) and of the refusals
//! that keep an unmerged branch (52.1 D-10, D-23): each test checks what is on disk
//! after the verb, not only what it printed.
//!
//! Every devflow child is built by [`common::hermetic_devflow`], so `systemctl` is the
//! fake in [`common::HermeticHome::service_bin`]: it answers, and reads every unit
//! inactive unless a test puts a marker file there.

mod common;

use devflow_core::liveness;
use devflow_core::phase_id::PhaseId;
use devflow_core::state::State;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// The stop reason a finished run records (D-35), as the CLI writes it.
const FINISHED_REASON: &str = "finished: Validate passed; the worktree and branch are kept";

/// The stop reason a `--until plan` halt records.
const UNTIL_PLAN_REASON: &str = "stopped after plan completed (--until plan)";

/// How long a waiting cleanup is watched before the test lets its unit go: long
/// enough for several of its unit reads.
const WAIT_OBSERVED: Duration = Duration::from_millis(1500);

/// How long a cleanup that has stopped waiting may take to exit.
const EXITS_WITHIN: Duration = Duration::from_secs(30);

/// A base branch that is not the project's `develop`.
const OTHER_BASE: &str = "workspace/example";

/// The untracked file a dirty fixture leaves in its worktree.
const SENTINEL: &str = "sentinel.txt";

/// Run git in `dir`, asserting it succeeds; its stdout, trimmed.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = devflow_core::test_support::git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository on `develop` with one commit, and a `main` branch at it.
fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["branch", "main"]);
}

/// stdout and stderr of `out`, for failure messages.
fn text(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A repository, one phase in it, and the scratch home its devflow children use.
struct Case {
    root: PathBuf,
    phase: PhaseId,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the case.
    home: common::HermeticHome,
}

impl Case {
    /// An initialised repository for `phase` with nothing of the phase in it.
    fn new(phase: PhaseId) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes the project root, so the case must too.
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root);
        Self {
            root,
            phase,
            _repo: repo,
            home: common::HermeticHome::new(),
        }
    }

    /// A finished run whose branch has one commit not merged into `develop`, its
    /// clean worktree, a prepared state, and both lock files.
    fn finished_unmerged(phase: PhaseId) -> Self {
        let case = Self::new(phase);
        case.add_worktree("develop");
        case.commit_on_branch();
        case.save_state(finish);
        case.create_lock_files();
        case
    }

    /// [`Self::finished_unmerged`] with the branch merged into `develop`: merged,
    /// clean and finished, so cleanup removes it.
    fn finished_merged(phase: PhaseId) -> Self {
        let case = Self::finished_unmerged(phase);
        case.merge_into_develop();
        case
    }

    /// The phase's derived branch.
    fn branch(&self) -> String {
        format!("feature/phase-{}", self.phase.padded())
    }

    /// The phase's derived worktree.
    fn worktree(&self) -> PathBuf {
        devflow_core::worktree::phase_path(&self.root, self.phase)
    }

    /// The phase's run state file.
    fn state_file(&self) -> PathBuf {
        devflow_core::workflow::state_path(&self.root, self.phase)
    }

    /// Fork the phase branch from `from` and check it out in the phase worktree.
    fn add_worktree(&self, from: &str) {
        let branch = self.branch();
        git(&self.root, &["branch", &branch, from]);
        devflow_core::worktree::add(&self.root, &self.worktree(), &branch, &branch, false).unwrap();
    }

    /// Commit one file on the phase branch, in its worktree.
    fn commit_on_branch(&self) {
        let worktree = self.worktree();
        fs::write(worktree.join("work.txt"), "the run's work\n").unwrap();
        git(&worktree, &["add", "work.txt"]);
        git(&worktree, &["commit", "-q", "-m", "work"]);
    }

    /// Save a prepared state for the phase, recording its worktree when one exists,
    /// shaped by `shape`.
    fn save_state(&self, shape: impl FnOnce(&mut State)) {
        let mut state = State::new(
            self.phase,
            devflow_core::state::AgentKind::Claude,
            devflow_core::mode::Mode::Auto,
            self.root.clone(),
        );
        state.verification_baseline_captured = true;
        state.worktree_path = self.worktree().is_dir().then(|| self.worktree());
        shape(&mut state);
        devflow_core::workflow::save_state(&state).unwrap();
    }

    /// Leave an untracked file in the worktree.
    fn write_sentinel(&self) -> PathBuf {
        let sentinel = self.worktree().join(SENTINEL);
        fs::write(&sentinel, "uncommitted\n").unwrap();
        sentinel
    }

    /// Merge the phase branch into `develop` in the primary checkout.
    fn merge_into_develop(&self) {
        git(
            &self.root,
            &["merge", "-q", "--no-ff", "-m", "merge", &self.branch()],
        );
    }

    /// The phase's holder and runner lock files, as a run leaves them.
    fn lock_files(&self) -> [PathBuf; 2] {
        [
            liveness::phase_holder_path(&self.root, self.phase),
            liveness::runner_lock_path(&self.root, self.phase),
        ]
    }

    /// Create both lock files, held by nobody.
    fn create_lock_files(&self) {
        drop(liveness::claim_phase_holder(&self.root, self.phase).expect("claim the holder"));
        drop(liveness::claim_runner_lock(&self.root, self.phase).expect("claim the runner lock"));
        assert!(self.lock_files_remain(), "premise: both lock files exist");
    }

    /// Whether both lock files are still on disk.
    fn lock_files_remain(&self) -> bool {
        self.lock_files().iter().all(|lock| lock.is_file())
    }

    /// The tip of `branch`, `None` when it does not exist.
    fn tip_of(&self, branch: &str) -> Option<String> {
        let out = devflow_core::test_support::git_command(&self.root)
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

    /// The tip of the phase branch, which must exist.
    fn tip(&self) -> String {
        self.tip_of(&self.branch())
            .expect("the phase branch exists")
    }

    /// Whether the worktree and the state file are both still on disk.
    fn worktree_and_state_remain(&self) -> bool {
        self.worktree().is_dir() && self.state_file().is_file()
    }

    /// A hermetic `devflow ARGS ROOT` child running in the repository.
    fn devflow(&self, args: &[&str]) -> Command {
        let mut command = common::hermetic_devflow(&self.home);
        command.args(args).arg(&self.root).current_dir(&self.root);
        command
    }

    /// `devflow cleanup` on the repository.
    fn cleanup(&self) -> Output {
        self.devflow(&["cleanup"])
            .output()
            .expect("run devflow cleanup")
    }

    /// `devflow discard --phase N` plus `extra` on the repository.
    fn discard(&self, extra: &[&str]) -> Output {
        let phase = self.phase.to_string();
        let mut args = vec!["discard", "--phase", phase.as_str()];
        args.extend_from_slice(extra);
        self.devflow(&args).output().expect("run devflow discard")
    }

    /// `devflow cleanup` with no locatable user bus: both bus variables removed.
    fn cleanup_without_a_bus(&self) -> Output {
        self.devflow(&["cleanup"])
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .output()
            .expect("run devflow cleanup")
    }

    /// The phase's unit name as the binary derives it (`service::unit_name`).
    fn unit(&self) -> String {
        format!(
            "devflow-{:016x}-{}.service",
            devflow_core::registry::path_digest(&self.root),
            self.phase.padded()
        )
    }
}

/// Shape a state as a finished run (D-35).
fn finish(state: &mut State) {
    state.stopped = true;
    state.finished = true;
    state.stop_reason = Some(FINISHED_REASON.to_string());
}

/// A spawned devflow child, killed and reaped on drop.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn child(&mut self) -> &mut std::process::Child {
        self.0.as_mut().expect("the child is still owned")
    }

    /// Wait for the child to exit within `within`, returning its output.
    fn output_within(mut self, within: Duration) -> Output {
        let deadline = Instant::now() + within;
        while self.child().try_wait().expect("poll the child").is_none() {
            assert!(
                Instant::now() < deadline,
                "the child did not exit within {within:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        self.0
            .take()
            .unwrap()
            .wait_with_output()
            .expect("collect the child's output")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// D-16: a finished run on an unmerged branch keeps its worktree, an untracked file in
/// it, its state and its branch's exact tip, and cleanup names `devflow discard`.
#[test]
fn cleanup_keeps_a_finished_unmerged_run_with_its_exact_branch_tip() {
    let case = Case::finished_unmerged(PhaseId::new(81));
    let sentinel = case.write_sentinel();
    let tip = case.tip();
    let state_before = fs::read(case.state_file()).unwrap();

    let out = case.cleanup();
    assert!(
        case.worktree().is_dir(),
        "worktree removed while unmerged:\n{}",
        text(&out)
    );
    assert!(
        sentinel.is_file(),
        "the untracked file is gone:\n{}",
        text(&out)
    );
    assert_eq!(
        fs::read(case.state_file()).ok(),
        Some(state_before),
        "the state changed:\n{}",
        text(&out)
    );
    assert_eq!(case.tip(), tip, "branch tip moved:\n{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("devflow discard --phase 81"),
        "the keep line names `devflow discard`:\n{}",
        text(&out)
    );
    assert!(
        out.status.success(),
        "a kept run is not a failure:\n{}",
        text(&out)
    );
}

/// D-16 control: the same run, once its untracked file is gone and its branch merged
/// into the base, is removed with its state; the lock files are never removed.
#[test]
fn cleanup_removes_a_finished_run_after_its_branch_is_merged() {
    let case = Case::finished_unmerged(PhaseId::new(82));
    let sentinel = case.write_sentinel();
    fs::remove_file(&sentinel).unwrap();
    case.merge_into_develop();

    let out = case.cleanup();
    assert!(out.status.success(), "cleanup failed:\n{}", text(&out));
    assert!(
        !case.worktree().exists() && !case.state_file().exists(),
        "merged run kept:\n{}",
        text(&out)
    );
    assert_eq!(
        case.tip_of(&case.branch()),
        None,
        "the merged branch is deleted:\n{}",
        text(&out)
    );
    assert!(
        case.lock_files_remain(),
        "a lock file was removed:\n{}",
        text(&out)
    );
}

/// D-09.5's clean rule: a merged, finished run whose worktree holds an untracked file
/// is kept, naming the uncommitted change and `devflow discard`.
#[test]
fn cleanup_keeps_a_merged_run_whose_worktree_is_dirty() {
    let case = Case::finished_unmerged(PhaseId::new(83));
    let sentinel = case.write_sentinel();
    case.merge_into_develop();
    let tip = case.tip();
    let state_before = fs::read(case.state_file()).unwrap();

    let out = case.cleanup();
    assert!(
        case.worktree().is_dir() && sentinel.is_file(),
        "dirty worktree removed:\n{}",
        text(&out)
    );
    assert_eq!(
        fs::read(case.state_file()).ok(),
        Some(state_before),
        "the state changed:\n{}",
        text(&out)
    );
    assert_eq!(case.tip(), tip, "branch tip moved:\n{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1 uncommitted change") && stdout.contains("devflow discard --phase 83"),
        "the keep line names the uncommitted change and `devflow discard`:\n{}",
        text(&out)
    );
}

/// D-07, plan check W-3: with no locatable user bus and a manager that does not
/// answer, cleanup keeps the phase, prints the cause with systemctl's stderr and exits
/// non-zero.
#[test]
fn cleanup_keeps_a_phase_when_the_user_bus_cannot_be_located() {
    let case = Case::finished_merged(PhaseId::new(84));
    fs::write(case.home.service_bin().join("version.fail"), "").unwrap();

    let out = case.cleanup_without_a_bus();
    assert!(
        case.worktree_and_state_remain(),
        "phase removed while the user bus could not be located:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("XDG_RUNTIME_DIR") && stderr.contains("DBUS_SESSION_BUS_ADDRESS"),
        "bus variables not named:\n{}",
        text(&out)
    );
    assert!(
        stderr.contains("No medium found"),
        "systemctl stderr missing from the cleanup output:\n{}",
        text(&out)
    );
    assert!(
        !out.status.success(),
        "cleanup exited 0 with an unreachable manager:\n{}",
        text(&out)
    );
}

/// Control for the test above: the same fixture and environment with a manager that
/// answers, so the keep above comes from the unreachable manager.
#[test]
fn cleanup_removes_the_same_phase_when_the_user_manager_answers() {
    let case = Case::finished_merged(PhaseId::new(85));

    let out = case.cleanup_without_a_bus();
    assert!(out.status.success(), "cleanup failed:\n{}", text(&out));
    assert!(
        !case.worktree().exists() && !case.state_file().exists(),
        "merged run kept while the manager answered:\n{}",
        text(&out)
    );
    assert!(
        case.lock_files_remain(),
        "a lock file was removed:\n{}",
        text(&out)
    );
}

/// Review r3 codex-8 = agy-4: a dead start's reservation (a state no start prepared)
/// with no worktree is removed; the lock files stay.
#[test]
fn cleanup_removes_a_dead_starts_state_that_has_no_worktree() {
    let case = Case::new(PhaseId::new(86));
    case.save_state(|state| state.verification_baseline_captured = false);
    case.create_lock_files();

    let out = case.cleanup();
    assert!(out.status.success(), "cleanup failed:\n{}", text(&out));
    assert!(
        !case.state_file().exists(),
        "state-only dead start kept:\n{}",
        text(&out)
    );
    assert!(
        case.lock_files_remain(),
        "a lock file was removed:\n{}",
        text(&out)
    );
}

/// Control for the test above: the same reservation while its runner lock is held is
/// kept, naming `devflow stop --phase`.
#[test]
fn cleanup_keeps_a_state_only_phase_while_its_runner_lock_is_held() {
    let case = Case::new(PhaseId::new(87));
    case.save_state(|state| state.verification_baseline_captured = false);
    let hold = liveness::claim_runner_lock(&case.root, case.phase).expect("hold the runner lock");

    let out = case.cleanup();
    assert!(
        case.state_file().is_file(),
        "state-only phase removed under a held runner lock:\n{}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("devflow stop --phase 87"),
        "the keep line names `devflow stop --phase`:\n{}",
        text(&out)
    );
    drop(hold);
}

/// 52.1 D-09.5 amendment: an incomplete stopped run is kept even when its branch is
/// merged and its worktree clean, naming `devflow resume` and `devflow discard`.
#[test]
fn cleanup_keeps_an_incomplete_stopped_run_even_when_merged_naming_resume_and_discard() {
    let case = Case::new(PhaseId::new(88));
    case.add_worktree("develop");
    case.save_state(|state| {
        state.stopped = true;
        state.stop_reason = Some(UNTIL_PLAN_REASON.to_string());
    });
    let state_before = fs::read(case.state_file()).unwrap();

    let out = case.cleanup();
    assert!(
        case.worktree().is_dir(),
        "incomplete stopped run's worktree removed:\n{}",
        text(&out)
    );
    assert_eq!(
        fs::read(case.state_file()).ok(),
        Some(state_before),
        "the state changed:\n{}",
        text(&out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("devflow resume --phase 88")
            && stdout.contains("devflow discard --phase 88"),
        "the keep line names `devflow resume` and `devflow discard`:\n{}",
        text(&out)
    );
    assert!(
        stdout.contains(UNTIL_PLAN_REASON),
        "the stop reason:\n{}",
        text(&out)
    );
}

/// D-07a: with the runner lock free and the unit `deactivating`, cleanup waits and
/// removes nothing of the phase until the unit reads inactive; then it removes it.
#[test]
fn cleanup_waits_for_a_deactivating_unit_before_removing() {
    let case = Case::finished_merged(PhaseId::new(89));
    let marker = case
        .home
        .service_bin()
        .join(format!("{}.deactivating", case.unit()));
    fs::write(&marker, "").unwrap();
    let mut guard = ChildGuard(Some(
        case.devflow(&["cleanup"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn devflow cleanup"),
    ));

    std::thread::sleep(WAIT_OBSERVED);
    let still_waiting = guard.child().try_wait().expect("poll cleanup").is_none();
    assert!(
        case.worktree_and_state_remain(),
        "removed before the unit read inactive"
    );
    assert!(
        still_waiting,
        "cleanup exited while the unit read deactivating"
    );

    fs::remove_file(&marker).unwrap();
    let out = guard.output_within(EXITS_WITHIN);
    assert!(
        out.status.success(),
        "cleanup failed after the wait:\n{}",
        text(&out)
    );
    assert!(
        !case.worktree().exists() && !case.state_file().exists(),
        "merged run kept after the unit read inactive:\n{}",
        text(&out)
    );
}

/// Review pi-4: a clean phase worktree with no state, on a branch with commits not in
/// the base, is kept at its exact tip, naming the worktree and `devflow discard`.
#[test]
fn cleanup_keeps_a_stateless_worktree_on_an_unmerged_branch() {
    let case = Case::new(PhaseId::new(90));
    case.add_worktree("develop");
    case.commit_on_branch();
    let tip = case.tip();

    let out = case.cleanup();
    assert!(
        case.worktree().is_dir(),
        "stateless worktree removed while unmerged:\n{}",
        text(&out)
    );
    assert_eq!(case.tip(), tip, "branch tip moved:\n{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&case.worktree().display().to_string())
            && stdout.contains("devflow discard --phase 90"),
        "the keep line names the worktree and `devflow discard`:\n{}",
        text(&out)
    );
}

/// Control for the test above: the same worktree once its branch is merged is
/// removed, so the pass sees a worktree with no state; the lock files stay.
#[test]
fn cleanup_removes_a_stateless_worktree_after_its_branch_is_merged() {
    let case = Case::new(PhaseId::new(91));
    case.add_worktree("develop");
    case.commit_on_branch();
    case.create_lock_files();
    case.merge_into_develop();

    let out = case.cleanup();
    assert!(out.status.success(), "cleanup failed:\n{}", text(&out));
    assert!(
        !case.worktree().exists(),
        "merged stateless worktree kept:\n{}",
        text(&out)
    );
    assert!(
        case.lock_files_remain(),
        "a lock file was removed:\n{}",
        text(&out)
    );
}

/// D-10: no cleanup path deletes an unmerged branch: not the finished run's worktree
/// pass, not the removal of a dead start's state, and not a worktree no phase owns.
#[test]
fn cleanup_never_deletes_an_unmerged_branch() {
    // A finished run, clean, on an unmerged branch.
    let case = Case::finished_unmerged(PhaseId::new(92));
    // A dead start's state, whose phase branch has a commit and no worktree.
    let reserved = PhaseId::new(93);
    let reserved_branch = format!("feature/phase-{}", reserved.padded());
    let scratch = case.root.join("scratch");
    git(
        &case.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &reserved_branch,
            scratch.to_str().unwrap(),
        ],
    );
    fs::write(scratch.join("reserved.txt"), "work\n").unwrap();
    git(&scratch, &["add", "reserved.txt"]);
    git(&scratch, &["commit", "-q", "-m", "reserved work"]);
    git(
        &case.root,
        &["worktree", "remove", scratch.to_str().unwrap()],
    );
    let mut state = State::new(
        reserved,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        case.root.clone(),
    );
    state.verification_baseline_captured = false;
    devflow_core::workflow::save_state(&state).unwrap();
    // A worktree under .worktrees/ that names no phase, on an unmerged feature branch.
    let unowned_branch = "feature/unowned";
    let unowned = case.root.join(".worktrees").join("unowned");
    git(&case.root, &["branch", unowned_branch, "develop"]);
    devflow_core::worktree::add(&case.root, &unowned, unowned_branch, unowned_branch, false)
        .unwrap();
    fs::write(unowned.join("unowned.txt"), "work\n").unwrap();
    git(&unowned, &["add", "unowned.txt"]);
    git(&unowned, &["commit", "-q", "-m", "unowned work"]);
    let tips: Vec<(String, String)> = [
        case.branch(),
        reserved_branch.clone(),
        unowned_branch.to_string(),
    ]
    .into_iter()
    .map(|branch| {
        let tip = case.tip_of(&branch).expect("premise: the branch exists");
        (branch, tip)
    })
    .collect();

    let out = case.cleanup();
    assert!(
        !devflow_core::workflow::state_path(&case.root, reserved).exists(),
        "premise: the dead start's state was removed:\n{}",
        text(&out)
    );
    for (branch, tip) in &tips {
        assert_eq!(
            case.tip_of(branch).as_ref(),
            Some(tip),
            "unmerged branch {branch} deleted or moved:\n{}",
            text(&out)
        );
    }
}

/// D-10: discard without `--delete-unmerged-branch` refuses an unmerged branch and
/// removes nothing, naming the flag with the branch.
#[test]
fn discard_without_the_branch_name_refuses_an_unmerged_branch() {
    let case = Case::finished_unmerged(PhaseId::new(94));
    let tip = case.tip();

    let out = case.discard(&[]);
    assert!(!out.status.success(), "discard exited 0:\n{}", text(&out));
    assert!(
        case.worktree_and_state_remain(),
        "discard removed something:\n{}",
        text(&out)
    );
    assert_eq!(case.tip(), tip, "branch tip moved:\n{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--delete-unmerged-branch feature/phase-94"),
        "the refusal names the flag and the branch:\n{}",
        text(&out)
    );
}

/// D-23: discard with a name that is not the phase's branch refuses and removes nothing.
#[test]
fn discard_with_a_wrong_branch_name_refuses() {
    let case = Case::finished_unmerged(PhaseId::new(95));
    let tip = case.tip();

    let out = case.discard(&["--delete-unmerged-branch", "feature/phase-99"]);
    assert!(!out.status.success(), "discard exited 0:\n{}", text(&out));
    assert!(
        case.worktree_and_state_remain(),
        "discard removed something:\n{}",
        text(&out)
    );
    assert_eq!(case.tip(), tip, "branch tip moved:\n{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("names feature/phase-99"),
        "the refusal names the wrong branch:\n{}",
        text(&out)
    );
}

/// A finished phase of `case`'s repository whose branch is forked from [`OTHER_BASE`]:
/// merged into that base, with one commit `develop` lacks. The state records
/// `recorded_base`.
fn phase_on_other_base(case: &Case, phase: PhaseId, recorded_base: Option<&str>) -> PathBuf {
    let branch = format!("feature/phase-{}", phase.padded());
    git(&case.root, &["branch", &branch, OTHER_BASE]);
    let worktree = devflow_core::worktree::phase_path(&case.root, phase);
    devflow_core::worktree::add(&case.root, &worktree, &branch, &branch, false).unwrap();
    let mut state = State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        case.root.clone(),
    );
    state.verification_baseline_captured = true;
    state.worktree_path = Some(worktree.clone());
    state.base_branch = recorded_base.map(str::to_string);
    finish(&mut state);
    devflow_core::workflow::save_state(&state).unwrap();
    worktree
}

/// D-10: cleanup and discard count "merged" against the base the run recorded. A
/// branch merged into a recorded non-default base is removed by cleanup and discarded
/// without its name; the same branch with no recorded base counts against `develop`,
/// where it is unmerged, so cleanup keeps it.
#[test]
fn cleanup_and_discard_use_a_non_default_base() {
    let case = Case::new(PhaseId::new(96));
    git(&case.root, &["checkout", "-q", "-b", OTHER_BASE]);
    fs::write(case.root.join("planning.txt"), "only on the other base\n").unwrap();
    git(&case.root, &["add", "."]);
    git(&case.root, &["commit", "-q", "-m", "planning"]);
    git(&case.root, &["checkout", "-q", "develop"]);
    let removed = phase_on_other_base(&case, PhaseId::new(96), Some(OTHER_BASE));
    let kept = phase_on_other_base(&case, PhaseId::new(97), None);
    let discarded = phase_on_other_base(&case, PhaseId::new(98), Some(OTHER_BASE));
    fs::write(discarded.join(SENTINEL), "uncommitted\n").unwrap();
    let base_tip = case.tip_of(OTHER_BASE).expect("premise: the base exists");

    let out = case.cleanup();
    assert!(
        !removed.exists(),
        "a run merged into its recorded base was kept:\n{}",
        text(&out)
    );
    assert!(
        kept.is_dir() && case.tip_of("feature/phase-97").is_some(),
        "a run unmerged into develop was removed:\n{}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 commit not merged into `develop`"),
        "the keep line counts against develop:\n{}",
        text(&out)
    );
    assert!(
        discarded.is_dir(),
        "premise: the dirty run is kept:\n{}",
        text(&out)
    );

    let out = common::hermetic_devflow(&case.home)
        .args(["discard", "--phase", "98"])
        .arg(&case.root)
        .current_dir(&case.root)
        .output()
        .expect("run devflow discard");
    assert!(out.status.success(), "discard refused:\n{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("branch feature/phase-98 (merged into `workspace/example`)"),
        "discard counts against the recorded base:\n{}",
        text(&out)
    );
    assert!(
        !discarded.exists(),
        "discard kept the worktree:\n{}",
        text(&out)
    );
    assert_eq!(
        case.tip_of(OTHER_BASE),
        Some(base_tip),
        "the base branch moved:\n{}",
        text(&out)
    );
}

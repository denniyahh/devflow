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
use std::process::{Command, Output};

/// The stop reason a finished run records (D-35), as the CLI writes it.
const FINISHED_REASON: &str = "finished: Validate passed; the worktree and branch are kept";

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

    /// `devflow cleanup` with no locatable user bus: both bus variables removed.
    fn cleanup_without_a_bus(&self) -> Output {
        self.devflow(&["cleanup"])
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .output()
            .expect("run devflow cleanup")
    }
}

/// Shape a state as a finished run (D-35).
fn finish(state: &mut State) {
    state.stopped = true;
    state.finished = true;
    state.stop_reason = Some(FINISHED_REASON.to_string());
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

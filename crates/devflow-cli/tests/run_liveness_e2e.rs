//! Container tests of the runner liveness read (D-07, 999.237) and `discard`'s wait for
//! the phase's unit (D-07a).
//!
//! Every devflow child is built by [`common::hermetic_devflow`], so `systemctl` is the
//! fake in [`common::HermeticHome::service_bin`]; a test sets what it reports with
//! marker files there. Any `PATH` a test adds goes through
//! [`HermeticPath::hermetic_path`].

mod common;

use common::HermeticPath;
use devflow_core::liveness;
use devflow_core::phase_id::PhaseId;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

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

/// A `develop`-only repo carrying `phase`'s CONTEXT.md and one PLAN.md, and a GSD config.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    let padded = phase.padded();
    let dir = root.join(format!(".planning/phases/{padded}-test"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{padded}-CONTEXT.md")), "ctx\n").unwrap();
    fs::write(dir.join(format!("{padded}-01-PLAN.md")), "plan\n").unwrap();
    fs::write(
        root.join(".planning/config.json"),
        "{\n  \"workflow\": {\n    \"auto_advance\": false\n  }\n}\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["branch", "main"]);
}

/// A repository and the scratch home its devflow children use.
struct Case {
    root: PathBuf,
    phase: PhaseId,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the case.
    home: common::HermeticHome,
}

impl Case {
    /// An initialised repository for `phase` with no run state.
    fn new(phase: PhaseId) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes the project root, so the case must too.
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        Self {
            root,
            phase,
            _repo: repo,
            home: common::HermeticHome::new(),
        }
    }

    /// [`Self::new`] plus a finished run: a merged `feature/phase-NN`, its worktree
    /// and a loadable state that records no supervisor, so `discard` would remove all
    /// three.
    fn finished(phase: PhaseId) -> Self {
        let case = Self::new(phase);
        let branch = format!("feature/phase-{}", phase.padded());
        git(&case.root, &["branch", &branch, "develop"]);
        let worktree = case.worktree();
        devflow_core::worktree::add(&case.root, &worktree, &branch, &branch, false).unwrap();
        let mut state = devflow_core::state::State::new(
            phase,
            devflow_core::state::AgentKind::Claude,
            devflow_core::mode::Mode::Auto,
            case.root.clone(),
        );
        state.worktree_path = Some(worktree);
        state.stopped = true;
        state.finished = true;
        state.stop_reason =
            Some("finished: Validate passed; the worktree and branch are kept".to_string());
        devflow_core::workflow::save_state(&state).unwrap();
        case
    }

    /// The phase worktree `discard` removes.
    fn worktree(&self) -> PathBuf {
        self.root
            .join(".worktrees")
            .join(format!("phase-{}", self.phase.padded()))
    }

    /// The phase's run state file.
    fn state_file(&self) -> PathBuf {
        devflow_core::workflow::state_path(&self.root, self.phase)
    }

    /// Whether the worktree and the state file are both still on disk.
    fn worktree_and_state_remain(&self) -> bool {
        self.worktree().is_dir() && self.state_file().is_file()
    }

    /// A hermetic `devflow` child running in the repository, with an empty stub dir
    /// first on its `PATH` after the fake service manager.
    fn devflow(&self, args: &[&str], stub_dir: &Path) -> std::process::Command {
        let mut command = common::hermetic_devflow(&self.home);
        let parent = std::env::var_os("PATH").unwrap_or_default();
        let mut path = stub_dir.as_os_str().to_os_string();
        path.push(":");
        path.push(parent);
        command
            .args(args)
            .arg(&self.root)
            .current_dir(&self.root)
            .hermetic_path(path);
        command
    }

    /// `devflow discard --phase N` on the repository.
    fn discard(&self) -> Output {
        let stubs = tempfile::tempdir().unwrap();
        self.devflow(
            &["discard", "--phase", &self.phase.to_string()],
            stubs.path(),
        )
        .output()
        .expect("run devflow discard")
    }
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

/// 999.237, D-07: a held runner lock blocks `discard` even when the state names no
/// launch, and the refusal names `devflow stop`. Control: once the lock is free the
/// same discard removes the worktree and the state.
#[test]
fn discard_refuses_while_the_runner_lock_is_held_and_the_state_names_no_launch() {
    let case = Case::finished(PhaseId::new(61));
    let hold = liveness::claim_runner_lock(&case.root, case.phase).expect("hold the runner lock");

    let out = case.discard();
    assert!(
        case.worktree_and_state_remain(),
        "discard removed the worktree under a held runner lock:\n{}",
        text(&out)
    );
    assert!(!out.status.success(), "discard exited 0:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!("devflow stop --phase {}", case.phase)),
        "the refusal names `devflow stop --phase`:\n{}",
        text(&out)
    );
    assert!(
        !stderr.contains("recover"),
        "the refusal names no `recover`:\n{}",
        text(&out)
    );

    drop(hold);
    let out = case.discard();
    assert!(
        out.status.success(),
        "control: discard proceeds:\n{}",
        text(&out)
    );
    assert!(
        !case.worktree().exists() && !case.state_file().exists(),
        "control: discard removed the worktree and the state once the lock was free:\n{}",
        text(&out)
    );
}

/// 999.237, D-07: a held runner lock blocks `start` on a phase with no state, naming
/// `devflow stop`.
#[test]
fn start_refuses_while_the_runner_lock_is_held() {
    let case = Case::new(PhaseId::new(62));
    let hold = liveness::claim_runner_lock(&case.root, case.phase).expect("hold the runner lock");
    let _reaper = common::LaunchedRunReaper::new(&case.root, case.phase, Vec::new());
    let stubs = tempfile::tempdir().unwrap();

    let out = case
        .devflow(
            &[
                "start",
                "--phase",
                &case.phase.to_string(),
                "--agent",
                "codex",
                "--mode",
                "supervise",
                "--no-worktree",
            ],
            stubs.path(),
        )
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "1")
        .output()
        .expect("run devflow start");

    let launched = case.state_file().exists();
    assert!(
        !launched,
        "start launched beside a held runner lock:\n{}",
        text(&out)
    );
    assert!(!out.status.success(), "start exited 0:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!("devflow stop --phase {}", case.phase)),
        "the refusal names `devflow stop --phase`:\n{}",
        text(&out)
    );
    assert!(
        !stderr.contains("recover"),
        "the refusal names no `recover`:\n{}",
        text(&out)
    );
    drop(hold);
}

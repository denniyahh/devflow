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
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

/// How long a waiting discard is watched before the test lets its unit go: long
/// enough for several of its 200 ms unit reads.
const WAIT_OBSERVED: Duration = Duration::from_millis(1500);

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
/// The phase's unit name as the binary derives it (`service::unit_name`).
fn unit_name(root: &Path, phase: PhaseId) -> String {
    format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(root),
        phase.padded()
    )
}

impl Case {
    /// The phase's unit name.
    fn unit(&self) -> String {
        unit_name(&self.root, self.phase)
    }

    /// Make the fake `systemctl` report the phase's unit `deactivating`.
    fn mark_unit_deactivating(&self) -> PathBuf {
        let marker = self
            .home
            .service_bin()
            .join(format!("{}.deactivating", self.unit()));
        fs::write(&marker, "").unwrap();
        marker
    }

    /// Make the fake `systemctl`'s Version probe fail, as with no reachable manager.
    fn mark_manager_unanswering(&self) {
        fs::write(self.home.service_bin().join("version.fail"), "").unwrap();
    }

    /// `devflow discard --phase N` with `adjust` applied to the command first.
    fn discard_with(&self, adjust: impl FnOnce(&mut std::process::Command)) -> Output {
        let stubs = tempfile::tempdir().unwrap();
        let mut command = self.devflow(
            &["discard", "--phase", &self.phase.to_string()],
            stubs.path(),
        );
        adjust(&mut command);
        command.output().expect("run devflow discard")
    }
}

/// A spawned devflow child, killed and reaped by its recorded pid on drop.
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

/// D-07a: with the runner lock free and the unit `deactivating`, discard waits under
/// its holder and removes nothing until the unit reads inactive; then it completes.
#[test]
fn discard_waits_for_a_deactivating_unit_before_removing_anything() {
    let case = Case::finished(PhaseId::new(63));
    let marker = case.mark_unit_deactivating();
    let stubs = tempfile::tempdir().unwrap();
    let mut guard = ChildGuard(Some(
        case.devflow(
            &["discard", "--phase", &case.phase.to_string()],
            stubs.path(),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn devflow discard"),
    ));

    std::thread::sleep(WAIT_OBSERVED);
    let still_waiting = guard.child().try_wait().expect("poll discard").is_none();
    assert!(
        case.worktree_and_state_remain(),
        "removed before the unit read inactive"
    );
    assert!(
        still_waiting,
        "discard exited while the unit read deactivating"
    );

    fs::remove_file(&marker).unwrap();
    let out = guard.output_within(Duration::from_secs(30));
    assert!(
        out.status.success(),
        "discard completes once the unit reads inactive:\n{}",
        text(&out)
    );
    assert!(
        !case.worktree().exists() && !case.state_file().exists(),
        "discard removed the worktree and the state after the wait:\n{}",
        text(&out)
    );
}

/// D-07a: a unit that never reads inactive makes discard refuse after its bound,
/// naming the unit and its state, with nothing removed.
#[test]
fn discard_refuses_when_the_unit_never_reads_inactive() {
    let case = Case::finished(PhaseId::new(64));
    case.mark_unit_deactivating();

    let out = case.discard();
    assert!(
        !out.status.success(),
        "no refusal after the bound:\n{}",
        text(&out)
    );
    assert!(
        case.worktree_and_state_remain(),
        "discard removed something:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&case.unit()) && stderr.contains("deactivating"),
        "the refusal names the unit and its state:\n{}",
        text(&out)
    );
}

/// D-07, agy-6: a failed unit read once the manager answers is an error, never
/// inactive: discard refuses, names the user manager and prints systemctl's stderr.
/// Control: without the failure discard proceeds.
#[test]
fn a_systemctl_error_reads_as_an_error_never_inactive() {
    let case = Case::finished(PhaseId::new(65));
    let marker = case
        .home
        .service_bin()
        .join(format!("{}.fail", case.unit()));
    fs::write(&marker, "").unwrap();

    let out = case.discard();
    assert!(
        !out.status.success() && case.worktree_and_state_remain(),
        "systemctl error read as inactive:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("systemd user manager"),
        "the refusal names the user manager:\n{}",
        text(&out)
    );
    assert!(
        stderr.contains("Transport endpoint is not connected"),
        "systemctl stderr missing from the refusal:\n{}",
        text(&out)
    );

    fs::remove_file(&marker).unwrap();
    let out = case.discard();
    assert!(
        out.status.success(),
        "control: discard proceeds:\n{}",
        text(&out)
    );
}

/// D-07, review r3 codex-1/pi-5: an unanswering manager under a runtime dir that is not
/// this user's `/run/user/<uid>` is unreachable, not absent, so discard refuses beside
/// a deactivating unit rather than checking the lock alone.
#[test]
fn discard_refuses_a_deactivating_unit_when_the_runtime_dir_is_not_the_users_own() {
    let case = Case::finished(PhaseId::new(66));
    case.mark_unit_deactivating();
    case.mark_manager_unanswering();
    let runtime = tempfile::tempdir().unwrap();

    let out = case.discard_with(|command| {
        command.env("XDG_RUNTIME_DIR", runtime.path());
    });
    assert!(
        case.worktree_and_state_remain(),
        "discard acted on a runtime dir that is not the user's own:\n{}",
        text(&out)
    );
    assert!(!out.status.success(), "discard exited 0:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("XDG_RUNTIME_DIR") && stderr.contains("/run/user/"),
        "XDG_RUNTIME_DIR not named in the refusal:\n{}",
        text(&out)
    );
}

/// D-07: the runner lock is read before the manager, so with no reachable manager a
/// held lock still refuses, naming `devflow stop`.
#[test]
fn discard_without_a_user_manager_still_refuses_a_held_runner_lock() {
    let case = Case::finished(PhaseId::new(67));
    case.mark_manager_unanswering();
    let runtime = tempfile::tempdir().unwrap();
    let _hold = liveness::claim_runner_lock(&case.root, case.phase).expect("hold the runner lock");

    let out = case.discard_with(|command| {
        command.env("XDG_RUNTIME_DIR", runtime.path());
    });
    assert!(
        case.worktree_and_state_remain() && !out.status.success(),
        "discard acted under a held runner lock:\n{}",
        text(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains(&format!("devflow stop --phase {}", case.phase)),
        "the refusal names `devflow stop --phase`:\n{}",
        text(&out)
    );
}

/// D-07, review codex-2/pi-8: a manager whose socket exists but which does not answer
/// is not "no manager": discard refuses beside a deactivating unit and prints
/// systemctl's stderr.
#[test]
fn discard_refuses_a_deactivating_unit_behind_a_manager_that_does_not_answer() {
    let case = Case::finished(PhaseId::new(68));
    case.mark_unit_deactivating();
    case.mark_manager_unanswering();
    let runtime = tempfile::tempdir().unwrap();
    fs::create_dir(runtime.path().join("systemd")).unwrap();
    fs::write(runtime.path().join("systemd").join("private"), "").unwrap();

    let out = case.discard_with(|command| {
        command.env("XDG_RUNTIME_DIR", runtime.path());
    });
    assert!(
        case.worktree_and_state_remain() && !out.status.success(),
        "discard acted while the user manager did not answer:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("systemd user manager") && stderr.contains("No medium found"),
        "systemctl stderr missing from the refusal:\n{}",
        text(&out)
    );
}

/// D-07, review codex-2: with neither `XDG_RUNTIME_DIR` nor `DBUS_SESSION_BUS_ADDRESS`
/// set the user bus cannot be located: discard refuses naming both, nothing removed.
#[test]
fn discard_refuses_when_the_user_bus_cannot_be_located() {
    let case = Case::finished(PhaseId::new(69));
    case.mark_unit_deactivating();
    case.mark_manager_unanswering();

    let out = case.discard_with(|command| {
        command
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("DBUS_SESSION_BUS_ADDRESS");
    });
    assert!(
        case.worktree_and_state_remain() && !out.status.success(),
        "discard acted with no locatable user bus:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("XDG_RUNTIME_DIR") && stderr.contains("DBUS_SESSION_BUS_ADDRESS"),
        "bus variables not named:\n{}",
        text(&out)
    );
}

/// How long `start` through the fake launcher may take to return.
const START_RETURNS_WITHIN: Duration = Duration::from_secs(30);

impl Case {
    /// The `start` argv for this case: codex in supervise mode on the main checkout.
    fn start_args(&self) -> Vec<String> {
        [
            "start",
            "--phase",
            &self.phase.to_string(),
            "--agent",
            "codex",
            "--mode",
            "supervise",
            "--no-worktree",
        ]
        .map(str::to_string)
        .to_vec()
    }

    /// A `devflow start` child whose codex stub waits until it is ended.
    fn start_command(&self, stubs: &common::FakeBin) -> std::process::Command {
        let args = self.start_args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.devflow(&args, &stubs.path)
    }

    /// The pid the fake `systemd-run` recorded for the phase's unit, if any.
    fn recorded_runner_pid(&self) -> Option<u32> {
        fs::read_to_string(self.home.service_bin().join(format!("{}.pid", self.unit())))
            .ok()
            .and_then(|text| text.trim().parse().ok())
    }

    /// Whether the phase's state names no reservation: it is absent, or it loads
    /// with no `pending_supervisor`.
    fn reservation_free(&self) -> bool {
        match devflow_core::workflow::load_state(&self.root, self.phase) {
            Ok(state) => state.pending_supervisor.is_none(),
            Err(devflow_core::workflow::WorkflowError::MissingState(_)) => true,
            Err(_) => false,
        }
    }

    /// Whether the runner lock probes Live.
    fn runner_lock_live(&self) -> bool {
        liveness::probe(&liveness::runner_lock_path(&self.root, self.phase))
            .is_ok_and(|held| held == liveness::Held::Live)
    }
}

/// D-04a: a unit start that fails makes `start` exit non-zero naming the unit and
/// systemd-run's stderr, and the reservation is cleared.
#[test]
fn start_reports_a_failed_unit_start_naming_the_unit_and_clears_the_reservation() {
    let case = Case::new(PhaseId::new(73));
    let failure = format!(
        "Job for {} failed because the control process exited with error code.",
        case.unit()
    );
    fs::write(
        case.home.service_bin().join(common::FAKE_FAIL_NEXT),
        format!("{failure}\n"),
    )
    .unwrap();
    let logs = tempfile::tempdir().unwrap();
    let stubs = common::waiting_capture_agent_bin(logs.path());
    let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);

    let out = case
        .start_command(&stubs)
        .output()
        .expect("run devflow start");
    assert!(!out.status.success(), "start exited 0:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&case.unit()) && stderr.contains(&failure),
        "missing runner path not reported: the unit and systemd-run's text:\n{}",
        text(&out)
    );
    assert!(
        case.reservation_free(),
        "reservation left after a failed start:\n{}",
        text(&out)
    );
}

/// D-04c: with lingering off `start` warns that the run ends at logout and still
/// launches. Control: with lingering on it prints no such warning.
#[test]
fn start_warns_when_lingering_is_off() {
    for (phase, linger_off) in [(PhaseId::new(74), true), (PhaseId::new(75), false)] {
        let case = Case::new(phase);
        if linger_off {
            fs::write(case.home.service_bin().join("linger.no"), "").unwrap();
        }
        let logs = tempfile::tempdir().unwrap();
        let stubs = common::waiting_capture_agent_bin(logs.path());
        let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);

        let out = case
            .start_command(&stubs)
            .output()
            .expect("run devflow start");
        assert!(out.status.success(), "start failed:\n{}", text(&out));
        assert!(
            case.recorded_runner_pid().is_some(),
            "start did not launch through systemd-run:\n{}",
            text(&out)
        );
        let warned = String::from_utf8_lossy(&out.stderr).contains("logout");
        assert_eq!(
            warned,
            linger_off,
            "linger warning {} with lingering {}:\n{}",
            if warned { "printed" } else { "missing" },
            if linger_off { "off" } else { "on" },
            text(&out)
        );
    }
}

/// Review codex-6, pi-6: `start` through the fake launcher returns while the runner
/// lives, the runner lock is held right after, and the recorded pid runs `__runner`,
/// not `__monitor`.
#[test]
fn start_through_the_fake_launcher_returns_while_the_run_is_live() {
    let case = Case::new(PhaseId::new(76));
    let logs = tempfile::tempdir().unwrap();
    let stubs = common::waiting_capture_agent_bin(logs.path());
    let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);
    let mut guard = ChildGuard(Some(
        case.start_command(&stubs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn devflow start"),
    ));

    common::wait_for(
        || guard.child().try_wait().expect("poll start").is_some(),
        START_RETURNS_WITHIN,
        "start did not return while the runner lives",
    );
    let out = guard.output_within(Duration::from_secs(1));
    assert!(out.status.success(), "start failed:\n{}", text(&out));
    assert!(
        case.runner_lock_live(),
        "runner lock not live after start:\n{}",
        text(&out)
    );
    let pid = case
        .recorded_runner_pid()
        .unwrap_or_else(|| panic!("the fake did not start the runner verb:\n{}", text(&out)));
    let args = common::process_args(pid);
    assert!(
        args.iter().any(|arg| arg == "__runner"),
        "the fake did not start the runner verb: {args:?}"
    );
    assert!(
        !args.iter().any(|arg| arg == "__monitor"),
        "the fake started __monitor: {args:?}"
    );
}

/// D-07a, D-12, review agy-1: a unit still deactivating with the runner lock free is
/// waited on, not refused; once it reads inactive `start` launches.
#[test]
fn start_waits_for_a_deactivating_unit_then_launches() {
    let case = Case::new(PhaseId::new(77));
    let marker = case.mark_unit_deactivating();
    let logs = tempfile::tempdir().unwrap();
    let stubs = common::waiting_capture_agent_bin(logs.path());
    let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);
    let mut guard = ChildGuard(Some(
        case.start_command(&stubs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn devflow start"),
    ));

    std::thread::sleep(Duration::from_secs(1));
    let still_waiting = guard.child().try_wait().expect("poll start").is_none();
    if !still_waiting {
        let out = guard.output_within(Duration::from_secs(1));
        panic!(
            "start refused a stopping unit instead of waiting:\n{}",
            text(&out)
        );
    }
    assert!(
        case.recorded_runner_pid().is_none(),
        "start refused a stopping unit instead of waiting: it launched while the unit \
         read deactivating"
    );

    fs::remove_file(&marker).unwrap();
    let out = guard.output_within(START_RETURNS_WITHIN);
    assert!(
        out.status.success(),
        "start did not launch once the unit read inactive:\n{}",
        text(&out)
    );
    assert!(
        case.runner_lock_live(),
        "runner lock not live after start:\n{}",
        text(&out)
    );
}

/// D-03, review codex-2, agy-6: with no locatable user bus `start` refuses naming
/// both bus variables and printing systemctl's stderr, and gives no lingering hint,
/// which is Absent's alone.
#[test]
fn start_refuses_when_the_user_bus_cannot_be_located_naming_the_variables() {
    let case = Case::new(PhaseId::new(78));
    case.mark_manager_unanswering();
    let logs = tempfile::tempdir().unwrap();
    let stubs = common::waiting_capture_agent_bin(logs.path());
    let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);

    let out = case
        .start_command(&stubs)
        .env_remove("XDG_RUNTIME_DIR")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .output()
        .expect("run devflow start");
    assert!(!out.status.success(), "start exited 0:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("XDG_RUNTIME_DIR") && stderr.contains("DBUS_SESSION_BUS_ADDRESS"),
        "bus variables not named:\n{}",
        text(&out)
    );
    assert!(
        stderr.contains("No medium found"),
        "systemctl stderr missing from the refusal:\n{}",
        text(&out)
    );
    assert!(
        !stderr.contains("enable-linger"),
        "lingering hint on an unreachable bus:\n{}",
        text(&out)
    );
    assert!(
        case.recorded_runner_pid().is_none(),
        "start launched without a reachable manager:\n{}",
        text(&out)
    );
}

/// D-07, review agy-6: a failed unit read once the manager answers refuses `start`,
/// printing systemctl's stderr, and launches nothing.
#[test]
fn start_refuses_when_the_unit_read_fails_printing_systemctls_error() {
    let case = Case::new(PhaseId::new(79));
    fs::write(
        case.home
            .service_bin()
            .join(format!("{}.fail", case.unit())),
        "",
    )
    .unwrap();
    let logs = tempfile::tempdir().unwrap();
    let stubs = common::waiting_capture_agent_bin(logs.path());
    let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);

    let out = case
        .start_command(&stubs)
        .output()
        .expect("run devflow start");
    assert!(!out.status.success(), "start exited 0:\n{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Transport endpoint is not connected"),
        "systemctl stderr missing from the refusal:\n{}",
        text(&out)
    );
    assert!(
        case.recorded_runner_pid().is_none(),
        "start launched over a failed unit read:\n{}",
        text(&out)
    );
}

impl Case {
    /// `devflow stop --phase N` on the repository.
    fn stop(&self) -> Output {
        let stubs = tempfile::tempdir().unwrap();
        self.devflow(&["stop", "--phase", &self.phase.to_string()], stubs.path())
            .output()
            .expect("run devflow stop")
    }

    /// The fake `systemctl`'s record that `stop` was called on the phase's unit.
    fn stop_called(&self) -> bool {
        self.home
            .service_bin()
            .join(format!("{}.stop-called", self.unit()))
            .exists()
    }
}

/// Ends, on drop, the process group whose leader's pid the stub agent wrote to `file`,
/// so an agent a SIGKILLed runner could not end does not outlive the test.
struct AgentGroupGuard(PathBuf);

impl Drop for AgentGroupGuard {
    fn drop(&mut self) {
        if let Some(pid) = fs::read_to_string(&self.0)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
        {
            common::signal_group(pid, libc::SIGKILL);
        }
    }
}

/// D-05: `stop` on a phase with no run is a no-op that exits 0 and says so.
#[test]
fn stop_on_a_phase_with_no_run_is_a_no_op() {
    let case = Case::new(PhaseId::new(85));
    let out = case.stop();
    assert!(out.status.success(), "no-op stop failed:\n{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("no live run"),
        "no-op stop failed to say there is no live run:\n{}",
        text(&out)
    );
    assert!(!case.stop_called(), "stop stopped a unit with no run");
}

/// D-05, T-52.2-15: a runner the manager has to SIGKILL records no stop, so `stop`
/// reports it crashed, names `devflow resume` and `devflow discard`, and exits non-zero.
#[test]
fn stop_reports_crashed_when_the_runner_was_killed() {
    let case = Case::new(PhaseId::new(86));
    let logs = tempfile::tempdir().unwrap();
    let agent_pid = logs.path().join("agent.pid");
    let stubs = common::capture_agent_bin_then(
        logs.path(),
        &format!("echo $$ > '{}'\nsleep 120", agent_pid.display()),
    );
    let _agent = AgentGroupGuard(agent_pid.clone());
    let _reaper = common::LaunchedRunReaper::for_home(&case.home, &case.root, case.phase);
    let started = case
        .start_command(&stubs)
        .output()
        .expect("run devflow start");
    assert!(
        started.status.success(),
        "start failed:\n{}",
        text(&started)
    );
    common::wait_for(
        || agent_pid.is_file(),
        START_RETURNS_WITHIN,
        "the stub agent to start",
    );
    fs::write(
        case.home
            .service_bin()
            .join(format!("{}.kill", case.unit())),
        "",
    )
    .unwrap();

    let out = case.stop();
    let stdout_and_stderr = text(&out);
    assert!(
        !out.status.success(),
        "crashed run reported stopped: stop exited 0:\n{stdout_and_stderr}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("crashed")
            && stderr.contains(&format!("devflow resume --phase {}", case.phase))
            && stderr.contains(&format!("devflow discard --phase {}", case.phase)),
        "crashed run reported stopped:\n{stdout_and_stderr}"
    );
}

/// D-05, D-07, review pi-10, pi-5: with the runner lock free and the phase holder held,
/// `stop` stops nothing, says another verb holds the phase, naming its pid and that it
/// is in progress, and exits non-zero.
#[test]
fn stop_while_another_verb_holds_the_phase_holder_says_it_is_in_progress() {
    let case = Case::new(PhaseId::new(87));
    let _holder =
        liveness::claim_phase_holder(&case.root, case.phase).expect("hold the phase holder");

    let out = case.stop();
    assert!(
        !out.status.success(),
        "stop answered no live run while the holder was held:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("another DevFlow verb holds phase") && stderr.contains("in progress"),
        "stop answered no live run while the holder was held:\n{}",
        text(&out)
    );
    assert!(
        stderr.contains(&format!("pid {}", std::process::id())),
        "holder pid missing from the refusal:\n{}",
        text(&out)
    );
    assert!(
        !case.stop_called(),
        "stop stopped a unit while the holder was held"
    );
}

/// D-05, review agy-10: a unit stopped while activating, before its runner recorded
/// anything, leaves only the reservation's state: `stop` says it stopped during launch,
/// exits 0, and never says crashed. `stop_reports_crashed_when_the_runner_was_killed`
/// is the control on a prepared state.
#[test]
fn stop_of_a_unit_stopped_before_the_runner_recorded_anything_says_stopped_during_launch() {
    let case = Case::new(PhaseId::new(88));
    let state = devflow_core::state::State::new(
        case.phase,
        devflow_core::state::AgentKind::Codex,
        devflow_core::mode::Mode::Supervise,
        case.root.clone(),
    );
    assert!(
        !state.verification_baseline_captured,
        "the fixture is a state only a reservation wrote"
    );
    devflow_core::workflow::save_state(&state).unwrap();
    fs::write(
        case.home
            .service_bin()
            .join(format!("{}.activating", case.unit())),
        "",
    )
    .unwrap();

    let out = case.stop();
    assert!(
        out.status.success(),
        "launch-window stop reported crashed: stop failed:\n{}",
        text(&out)
    );
    let all = text(&out);
    assert!(
        all.contains("stopped during launch") && !all.contains("crashed"),
        "launch-window stop reported crashed:\n{all}"
    );
    assert!(case.stop_called(), "stop did not stop the activating unit");
}

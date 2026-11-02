//! End-to-end proof that `devflow stop` reaches a pending supervisor (D-02): a live
//! supervisor the state names only as a reservation, because its launcher or stage N's
//! supervisor died before promoting it (`50-CRASH-TABLE.md` rows A7 and C4).
//!
//! Each test drives the compiled binary through `common::hermetic_devflow`, with
//! `DEVFLOW_TEST_AGENT_SPAWN_HOLD` holding the supervisor after it adopted its liveness
//! lock and before it has an agent or a socket. Only the test's own kills are signals:
//! `stop` is asserted to end the supervisor through its socket.

mod common;

use devflow_core::liveness;
use devflow_core::phase_id::PhaseId;
use devflow_core::state::{PendingSupervisor, State};
use devflow_core::supervisor::{self, Liveness};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

const HOLD_ENV: &str = "DEVFLOW_TEST_AGENT_SPAWN_HOLD";
/// How long a launch gets to reach a point the test waits on.
const REACH_WITHIN: Duration = Duration::from_secs(45);
/// Long enough for a `stop` started now to pass its socket step and start waiting for
/// the publish. It only makes the wait the likely path; the assertions hold either way.
const STOP_REACHES_ITS_WAIT: Duration = Duration::from_secs(1);

/// A fake `claude` that answers `--version` and otherwise stays running.
const WAITING_CLAUDE: &str = r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
sleep 120"#;

/// A fake `claude` whose first run finishes, so stage N hands off, and whose later runs
/// stay running.
const FINISH_ONCE_CLAUDE: &str = r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
if [ ! -e .first-agent-done ]; then
  : > .first-agent-done
  git commit -q --allow-empty -m "fake agent work"
  printf 'work done\nDEVFLOW_RESULT: {"status":"success","commits":1}\n'
  exit 0
fi
sleep 120"#;

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

/// A repository on `develop` with a roadmap entry for `phase`.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(root.join(".gitignore"), ".first-agent-done\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: pending supervisor fixture\n"),
    )
    .unwrap();
    git(
        root,
        &["add", "README.md", ".gitignore", ".planning/ROADMAP.md"],
    );
    git(root, &["commit", "-q", "-m", "base"]);
}

/// The release file of the agent-spawn hold. Dropping it releases the hold, so a failing
/// assertion leaves no supervisor waiting out its bound.
struct SpawnHold {
    _dir: tempfile::TempDir,
    release: PathBuf,
}

impl SpawnHold {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let release = dir.path().join("release");
        Self { _dir: dir, release }
    }

    /// The hold's value for `stage`'s supervisor.
    fn spec(&self, stage: &str) -> String {
        format!("{stage}:{}", self.release.display())
    }

    /// Wait until a supervisor reports it reached the hold.
    fn wait_held(&self) {
        let mut marker = self.release.clone().into_os_string();
        marker.push(".held");
        let marker = PathBuf::from(marker);
        common::wait_for(
            || marker.exists(),
            REACH_WITHIN,
            "the supervisor to reach its hold",
        );
    }

    fn release(&self) {
        fs::write(&self.release, "").unwrap();
    }
}

impl Drop for SpawnHold {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, "");
    }
}

/// One fixture: a repo, its hermetic home and the reaper for whatever it launches. The
/// fields drop in order, so the hold is released before the reaper waits on the run, and
/// the reaper ends the run before its root and home go away.
struct Fixture {
    hold: SpawnHold,
    _reaper: common::LaunchedRunReaper,
    fake_bin: common::FakeBin,
    home: common::HermeticHome,
    phase: PhaseId,
    root: PathBuf,
    _repo: tempfile::TempDir,
}

impl Fixture {
    fn new(phase: PhaseId, agent: &str) -> Self {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        Self {
            hold: SpawnHold::new(),
            _reaper: common::LaunchedRunReaper::new(&root, phase, vec![]),
            fake_bin: common::fake_bin_dir(agent),
            home: common::HermeticHome::new(),
            phase,
            root,
            _repo: repo,
        }
    }

    /// `devflow start` in the capture shape, its supervisor held at `hold_stage`.
    fn start(&self, hold_stage: &str, extra: &[&str]) -> Command {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let path = format!(
            "{}:{}",
            self.fake_bin.path.display(),
            existing.to_string_lossy()
        );
        let mut command = common::hermetic_devflow(&self.home);
        command
            .args([
                "start",
                "--phase",
                &self.phase.to_string(),
                "--agent",
                "claude",
                "--mode",
                "supervise",
                "--no-worktree",
                "--legacy-claude-launch",
            ])
            .args(extra)
            .arg(&self.root)
            .current_dir(&self.root)
            .env("PATH", path)
            .env(HOLD_ENV, self.hold.spec(hold_stage));
        command
    }

    /// A launcher for stage Define, held before its supervisor's agent spawn.
    fn spawn_held_launcher(&self) -> Child {
        let launcher = self
            .start("define", &[])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn devflow start");
        self.hold.wait_held();
        launcher
    }

    fn state(&self) -> State {
        devflow_core::workflow::load_state(&self.root, self.phase).expect("the state loads")
    }

    fn pending(&self) -> PendingSupervisor {
        self.state()
            .pending_supervisor
            .expect("the state names the launch as pending")
    }

    fn stop_command(&self) -> Command {
        let mut command = common::hermetic_devflow(&self.home);
        command
            .args(["stop", "--phase", &self.phase.to_string()])
            .arg(&self.root)
            .current_dir(&self.root);
        command
    }

    /// `devflow stop`, started while the hold stands and with the hold released once it
    /// is waiting: the supervisor has no socket when `stop` first looks. The `stop` is
    /// asserted still running at the release, so it did not return early.
    fn stop_across_the_release(&self) -> Output {
        let mut stop = self
            .stop_command()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn devflow stop");
        std::thread::sleep(STOP_REACHES_ITS_WAIT);
        let early = stop.try_wait().expect("poll devflow stop");
        self.hold.release();
        let output = stop.wait_with_output().expect("wait for devflow stop");
        assert_eq!(
            early,
            None,
            "stop returned before the supervisor published\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn status(&self) -> String {
        let output = common::hermetic_devflow(&self.home)
            .arg("status")
            .arg(&self.root)
            .current_dir(&self.root)
            .output()
            .expect("run devflow status");
        assert!(
            output.status.success(),
            "status failed\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn stop_verb(&self) -> String {
        format!("devflow stop --phase {}", self.phase)
    }

    fn resume_verb(&self) -> String {
        format!("devflow resume --phase {}", self.phase)
    }

    /// While the pending supervisor lives, `status` names `stop` and never `resume`.
    fn assert_status_names_stop_only(&self, pending: &PendingSupervisor) {
        let status = self.status();
        assert!(status.contains(&self.stop_verb()), "{status}");
        assert!(status.contains(&pending.launch_id), "{status}");
        assert!(!status.contains("devflow resume"), "{status}");
    }

    /// `stop` succeeded and named no `resume`; the pending supervisor is GONE, its
    /// reservation is cleared, the phase is stopped, and `status` now suggests `resume`.
    fn assert_stopped(&self, output: &Output, pending: &PendingSupervisor) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "stop failed\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            !stdout.contains("devflow resume") && !stderr.contains("devflow resume"),
            "stop must not suggest resume\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert_eq!(
            supervisor::probe(&pending.socket_path).expect("probe its socket"),
            Liveness::Gone,
            "stop ends the pending supervisor\nstdout: {stdout}\nstderr: {stderr}"
        );
        let state = self.state();
        assert!(state.stopped, "the phase is stopped: {state:?}");
        assert_eq!(state.pending_supervisor, None, "its reservation is cleared");
        assert_eq!(
            liveness::probe(&pending.liveness_path).expect("probe its lock"),
            liveness::Held::Missing,
            "its liveness file is gone"
        );
        let status = self.status();
        assert!(status.contains(&self.resume_verb()), "{status}");
        assert!(!status.contains(&self.stop_verb()), "{status}");
    }
}

/// Row A7: a launcher SIGKILLed during the publish wait leaves a supervisor the state
/// names only as pending, which publishes once released. `stop` ends it through its
/// socket; afterwards `status` suggests `resume`.
#[test]
fn stop_ends_a_pending_supervisor_through_its_socket() {
    let fixture = Fixture::new(PhaseId::new(181), WAITING_CLAUDE);
    let mut launcher = fixture.spawn_held_launcher();
    common::kill_and_reap(&mut launcher);
    let pending = fixture.pending();
    assert_eq!(fixture.state().supervisor, None, "no record was saved");
    assert_eq!(
        liveness::probe(&pending.liveness_path).unwrap(),
        liveness::Held::Live,
        "the supervisor kept the lock after its launcher died"
    );
    fixture.assert_status_names_stop_only(&pending);

    fixture.hold.release();
    common::wait_for(
        || {
            matches!(
                supervisor::probe(&pending.socket_path),
                Ok(Liveness::Alive(_))
            )
        },
        REACH_WITHIN,
        "the pending supervisor to publish",
    );
    fixture.assert_status_names_stop_only(&pending);

    let output = fixture.stop_command().output().expect("run devflow stop");
    fixture.assert_stopped(&output, &pending);
}

/// The repair the launch guard names over a pending supervisor with no socket: `stop`
/// first finds none, waits for the publish and ends what appears.
#[test]
fn stop_waits_for_a_pending_supervisor_to_publish_and_ends_it() {
    let fixture = Fixture::new(PhaseId::new(182), WAITING_CLAUDE);
    let mut launcher = fixture.spawn_held_launcher();
    common::kill_and_reap(&mut launcher);
    let pending = fixture.pending();

    let output = fixture.stop_across_the_release();
    fixture.assert_stopped(&output, &pending);
}

/// A launcher still running may promote its launch while `stop` waits, which ends the
/// pending record without ending the supervisor: `stop` must still end it.
#[test]
fn stop_ends_a_launch_its_running_launcher_promotes_during_the_wait() {
    let fixture = Fixture::new(PhaseId::new(183), WAITING_CLAUDE);
    let mut launcher = fixture.spawn_held_launcher();
    let pending = fixture.pending();

    let output = fixture.stop_across_the_release();
    fixture.assert_stopped(&output, &pending);
    common::wait_for(
        || launcher.try_wait().expect("poll the launcher").is_some(),
        REACH_WITHIN,
        "the launcher to exit",
    );
}

/// SIGKILL the supervisor `socket`'s sidecar names, identity-checked first, and wait for
/// the socket to read STALE. Returns that sidecar.
fn kill_supervisor_at(socket: &Path) -> supervisor::Sidecar {
    let sidecar = supervisor::read_sidecar(&socket.with_extension("json")).expect("its sidecar");
    let pid = sidecar.supervisor_pid;
    assert!(
        devflow_core::agent::is_same_process(pid, sidecar.supervisor_start_time),
        "the sidecar's supervisor {pid} is still that process"
    );
    let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
    assert!(signed > 1, "refusing to signal pid {signed}");
    // SAFETY: `signed > 1` names the supervisor just identity-checked.
    assert_eq!(unsafe { libc::kill(signed, libc::SIGKILL) }, 0);
    common::wait_for(
        || matches!(supervisor::probe(socket), Ok(Liveness::Stale)),
        REACH_WITHIN,
        "the socket to read STALE",
    );
    sidecar
}

/// Row C4's state: N finished and spawned N+1, which is held before its agent spawn,
/// and N has been SIGKILLed. Returns N+1's reservation.
fn handoff_window(fixture: &Fixture) -> PendingSupervisor {
    let started = fixture
        .start("plan", &["--until", "plan"])
        .output()
        .expect("run devflow start");
    assert!(
        started.status.success(),
        "start failed\nstderr: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    fixture.hold.wait_held();
    let record = fixture.state().supervisor.expect("N is recorded");
    let pending = fixture.pending();
    assert_ne!(
        pending.launch_id, record.launch_id,
        "the pending one is N+1"
    );
    kill_supervisor_at(&record.socket_path);
    fixture.assert_status_names_stop_only(&pending);
    pending
}

/// Row C4: stage N's supervisor SIGKILLed after spawning N+1 and before promoting it.
/// `stop` removes N's stale socket without suggesting `resume`, since N+1 still lives,
/// then ends N+1 through its socket.
#[test]
fn stop_in_the_handoff_window_ends_the_next_supervisor_and_names_no_resume() {
    let fixture = Fixture::new(PhaseId::new(184), FINISH_ONCE_CLAUDE);
    let pending = handoff_window(&fixture);

    let output = fixture.stop_across_the_release();
    fixture.assert_stopped(&output, &pending);
}

/// N+1 published and then died too: its socket is STALE, its lock is released and its
/// agent still runs. Nothing but the pending record names that socket once N's is gone,
/// so `stop` reaches its backstop through the record, which ends the agent group.
#[test]
fn stop_ends_the_agent_of_a_pending_supervisor_that_died() {
    let fixture = Fixture::new(PhaseId::new(185), FINISH_ONCE_CLAUDE);
    let pending = handoff_window(&fixture);
    fixture.hold.release();
    common::wait_for(
        || {
            matches!(
                supervisor::probe(&pending.socket_path),
                Ok(Liveness::Alive(_))
            )
        },
        REACH_WITHIN,
        "N+1 to publish",
    );
    let sidecar = kill_supervisor_at(&pending.socket_path);
    let agent_group = sidecar.agent_pgid;
    assert!(
        !devflow_core::test_support::live_group_members(agent_group).is_empty(),
        "the agent outlived its supervisor"
    );
    common::wait_for(
        || {
            matches!(
                liveness::probe(&pending.liveness_path),
                Ok(liveness::Held::Abandoned)
            )
        },
        REACH_WITHIN,
        "N+1's lock to be released",
    );

    let output = fixture.stop_command().output().expect("run devflow stop");
    fixture.assert_stopped(&output, &pending);
    assert_eq!(
        devflow_core::test_support::live_group_members(agent_group),
        Vec::<u32>::new(),
        "the backstop ended the agent group"
    );
}

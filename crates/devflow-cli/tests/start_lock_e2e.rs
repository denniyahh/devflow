//! End-to-end coverage for `start`'s per-phase writer lock.
//!
//! These tests deliberately hold the lock from the test process, then drive
//! the compiled CLI as a separate process. That makes the refusal observable
//! at the same boundary an operator uses, rather than coupling the test to
//! `commands::start`'s implementation details.
//!
//! Every `devflow` child runs through `common::hermetic_devflow`, so its
//! registry entries land in a tempdir `HOME`, never the operator's real
//! `~/.cache/devflow` (999.141).

mod common;

use devflow_core::gates::{GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::supervisor::{self, Liveness, ShutdownOutcome};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::Duration;

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

fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: lock fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

fn child(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    mode: &str,
) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = common::hermetic_devflow(home);
    command
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            mode,
            "--no-worktree",
            "--legacy-claude-launch",
        ])
        .arg(root)
        .current_dir(root)
        .env("PATH", path);
    command
}

#[test]
fn start_refuses_while_another_process_holds_the_phase_lock() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(90);
    init_repo(root, phase);
    let fake_bin = common::fake_bin_dir("exit 0");
    let holder = devflow_core::lock::acquire(root, phase).expect("test holds phase lock");
    let holder_pid = std::process::id();
    let worktrees_before = devflow_core::test_support::git_command(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("list baseline worktrees");
    assert!(worktrees_before.status.success());

    let output = child(&home, root, phase, &fake_bin, "supervise")
        .output()
        .expect("run devflow start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "start must refuse a held lock\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains(&phase.to_string()),
        "stderr must name phase: {stderr}"
    );
    assert!(
        stderr.contains(&holder_pid.to_string()),
        "stderr must name holder pid {holder_pid}: {stderr}"
    );
    assert!(
        !root
            .join(format!(".devflow/state-{}.json", phase.padded()))
            .exists(),
        "a refused start must not write phase state"
    );
    let worktrees = devflow_core::test_support::git_command(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("list worktrees");
    assert!(worktrees.status.success());
    assert_eq!(
        worktrees.stdout, worktrees_before.stdout,
        "a refused start must not add a worktree"
    );
    let branches = devflow_core::test_support::git_command(root)
        .args([
            "branch",
            "--list",
            &format!("feature/phase-{padded}", padded = phase.padded()),
        ])
        .output()
        .expect("list phase branch");
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
    drop(holder);
}

#[test]
fn start_proceeds_when_no_process_holds_the_phase_lock() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(91);
    init_repo(root, phase);
    let fake_bin = common::fake_bin_dir("exit 0");
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);

    let output = child(&home, root, phase, &fake_bin, "supervise")
        .output()
        .expect("run devflow start");
    assert!(
        output.status.success(),
        "uncontended start failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let state = root.join(format!(".devflow/state-{}.json", phase.padded()));
    let events = devflow_core::events::events_path(root);
    assert!(
        state.exists()
            || fs::read_to_string(events)
                .unwrap_or_default()
                .contains("\"workflow_started\""),
        "an uncontended start must write state or its workflow_started event"
    );
}

#[test]
fn start_clears_leftover_gate_files_after_taking_the_lock() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(92);
    init_repo(root, phase);
    let fake_bin = common::fake_bin_dir("sleep 60");
    let response_path = Gates::response_path(root, phase, Stage::Define);
    fs::create_dir_all(response_path.parent().unwrap()).unwrap();
    fs::write(
        &response_path,
        serde_json::to_vec(&GateResponse {
            approved: false,
            note: Some("abort: planted stale response".into()),
            responded_by: Some("start-lock-e2e-planted-response".into()),
        })
        .unwrap(),
    )
    .unwrap();
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);

    let mut process = child(&home, root, phase, &fake_bin, "auto")
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30")
        .spawn()
        .expect("spawn devflow start");
    common::wait_for(
        || {
            Gates::gate_path(root, phase, Stage::Define).exists()
                || process.try_wait().expect("poll start child").is_some()
        },
        Duration::from_secs(10),
        "Define gate or start exit",
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        process.try_wait().expect("poll start child").is_none(),
        "planted response must not abort a fresh start"
    );
    assert!(
        root.join(format!(".devflow/state-{}.json", phase.padded()))
            .exists(),
        "a fresh start parked at Define must keep state"
    );
    let events = fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
    assert!(
        !events.contains("start-lock-e2e-planted-response"),
        "the planted response must not resolve the fresh gate\nevents:\n{events}"
    );
    assert!(
        !response_path.exists(),
        "the planted response must be removed"
    );
    common::kill_and_reap(&mut process);
}

// ---------------------------------------------------------------------------
// 48-19 (criterion 2, D-01, D-02): `start` must not overwrite a live run.
//
// Each arm plants the same leftover run (Code-stage state, a Code gate request
// and response, optionally an agent pid file) and differs from the others only
// in whether the recorded pids name a live process, plus `--force` on the agent
// arm. The live process is a test-owned `sleep`, reaped in `Drop` so a failing
// assertion leaks nothing.
// ---------------------------------------------------------------------------

/// Existing never-live pid idiom (phase7_cli.rs): above Linux's maximum
/// `pid_max` of 2^22, so no process can hold it.
const NEVER_LIVE_PID: u32 = 0x7FFF_FFFE;

/// Bounds any gate a launched monitor parks at, so a start that proceeds
/// (every control, and the refusal arms at RED) leaves no immortal monitor.
const GATE_TIMEOUT_SECS: &str = "15";

/// A test-owned live process whose pid the fixtures record as a monitor or
/// agent. Killed and waited on drop, including while a failed assertion
/// unwinds.
struct LiveProcess {
    child: Child,
}

impl LiveProcess {
    fn spawn() -> Self {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn test-owned live process");
        Self { child }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for LiveProcess {
    fn drop(&mut self) {
        // No `expect` here: a panic inside drop during unwinding aborts.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A fake `claude` that answers `--version` and otherwise sleeps until it is
/// ended, so a launched run stays live for the whole test.
const SLEEPING_CLAUDE: &str = "if [ \"$1\" = \"--version\" ]; then echo \"2.1.220 (Claude Code)\"; exit 0; fi\nexec sleep 120";

/// How long [`LiveRun`]'s drop waits for its supervisor's final shutdown line.
const LIVE_RUN_SHUTDOWN_WAIT: Duration = Duration::from_secs(15);

/// A run a real `devflow start` launched (capture shape, supervise mode) with
/// a sleeping fake agent: its supervisor answers ALIVE on `socket`, and
/// `sidecar` is what it published, read before any test corrupts it.
///
/// Dropping it ends the run through that socket, then SIGKILLs the agent
/// group and the supervisor if each is still the process the sidecar names
/// (`agent::is_same_process`), so a run whose state or sidecar a test
/// corrupted is still reaped. Bind a `LaunchedRunReaper` before
/// [`LiveRun::launch`] as the backstop for a panic inside it.
struct LiveRun {
    socket: PathBuf,
    sidecar: supervisor::Sidecar,
    _fake_bin: common::FakeBin,
}

impl LiveRun {
    fn launch(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Self {
        let fake_bin = common::fake_bin_dir(SLEEPING_CLAUDE);
        let output = child(home, root, phase, &fake_bin, "supervise")
            .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS)
            .output()
            .expect("run devflow start for the live-run fixture");
        assert!(
            output.status.success(),
            "the live-run fixture's start failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let record = devflow_core::workflow::load_state(root, phase)
            .expect("the fixture's start saved its state")
            .supervisor
            .expect("the fixture's start recorded state.supervisor");
        let sidecar = supervisor::read_sidecar(&record.socket_path.with_extension("json"))
            .expect("read the fixture's sidecar");
        let run = Self {
            socket: record.socket_path,
            sidecar,
            _fake_bin: fake_bin,
        };
        assert_eq!(run.sidecar.launch_id, record.launch_id, "sidecar launch id");
        assert!(
            matches!(supervisor::probe(&run.socket), Ok(Liveness::Alive(_))),
            "the fixture's supervisor must answer ALIVE"
        );
        let pid_file = devflow_core::agent_result::agent_pid_path(root, phase);
        common::wait_for(
            || pid_file.exists(),
            Duration::from_secs(10),
            "the fixture's agent pid file",
        );
        run
    }

    fn launch_id(&self) -> &str {
        &self.sidecar.launch_id
    }

    fn sidecar_path(&self) -> PathBuf {
        self.socket.with_extension("json")
    }

    /// Remove the agent pid file, so the guard's pid half sees nothing and
    /// only the supervisor socket can name this run: 999.139's shape, where
    /// the pid half let `start --force` launch a second agent.
    fn hide_agent_pid_file(&self, root: &Path, phase: PhaseId) {
        fs::remove_file(devflow_core::agent_result::agent_pid_path(root, phase))
            .expect("remove the fixture's agent pid file");
    }
}

impl Drop for LiveRun {
    fn drop(&mut self) {
        // No `expect` here: a panic inside drop during unwinding aborts.
        let _ = supervisor::request_shutdown(&self.socket, LIVE_RUN_SHUTDOWN_WAIT);
        let group = (self.sidecar.agent_pgid, self.sidecar.agent_start_time);
        if devflow_core::agent::agent_running(group.0)
            && devflow_core::agent::is_same_process(group.0, group.1)
        {
            common::signal_group(group.0, libc::SIGKILL);
        }
        let monitor = (
            self.sidecar.supervisor_pid,
            self.sidecar.supervisor_start_time,
        );
        if devflow_core::agent::agent_running(monitor.0)
            && devflow_core::agent::is_same_process(monitor.0, monitor.1)
            && let Ok(pid) = libc::pid_t::try_from(monitor.0)
        {
            // SAFETY: one process, identity-checked against its start time.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

/// The planted Code gate files, returned so a control can assert they are gone.
struct PlantedGate {
    request: PathBuf,
    response: PathBuf,
}

/// A leftover Code-stage state with no supervisor record, an optional agent
/// pid file, and a planted Code gate. The launch guard reads no
/// `monitor_pid` (50-15), so none is planted; a test that needs a live
/// supervisor launches one with [`LiveRun::launch`].
fn plant_leftover_run(root: &Path, phase: PhaseId, agent_pid: Option<u32>) -> PlantedGate {
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).expect("plant leftover state");
    if let Some(pid) = agent_pid {
        write_agent_pid(root, phase, pid);
    }
    plant_code_gate(root, phase)
}

/// A Code gate request and an aborting response, as a leftover run leaves
/// them.
fn plant_code_gate(root: &Path, phase: PhaseId) -> PlantedGate {
    let request = Gates::write_gate(root, phase, Stage::Code, "planted leftover Code gate")
        .expect("plant Code gate request");
    let response = Gates::response_path(root, phase, Stage::Code);
    fs::create_dir_all(response.parent().unwrap()).unwrap();
    fs::write(
        &response,
        serde_json::to_vec(&GateResponse {
            approved: false,
            note: Some("abort: planted leftover response".into()),
            responded_by: Some("start-lock-e2e-planted-leftover".into()),
        })
        .unwrap(),
    )
    .expect("plant Code gate response");
    PlantedGate { request, response }
}

fn write_agent_pid(root: &Path, phase: PhaseId, pid: u32) {
    let path = devflow_core::agent_result::agent_pid_path(root, phase);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, pid.to_string()).expect("write agent pid file");
}

/// Every file a live run for `phase` depends on, with its exact bytes, sorted
/// by path: the state file, the agent pid file, and each gate-directory entry
/// named `{padded}-…`. Only existing files are listed, so a deleted file shows
/// up as a different snapshot rather than a read panic.
fn phase_file_snapshot(root: &Path, phase: PhaseId) -> Vec<(PathBuf, Vec<u8>)> {
    let mut paths = vec![
        devflow_core::workflow::state_path(root, phase),
        devflow_core::agent_result::agent_pid_path(root, phase),
    ];
    let prefix = format!("{}-", phase.padded());
    if let Ok(entries) = fs::read_dir(Gates::dir(root)) {
        for entry in entries {
            let entry = entry.expect("read gates dir entry");
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                paths.push(entry.path());
            }
        }
    }
    paths.retain(|path| path.exists());
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).expect("read snapshot file");
            (path, bytes)
        })
        .collect()
}

fn snapshot_paths(snapshot: &[(PathBuf, Vec<u8>)]) -> Vec<&Path> {
    snapshot.iter().map(|(path, _)| path.as_path()).collect()
}

fn run_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    extra: &[&str],
) -> Output {
    child(home, root, phase, fake_bin, "supervise")
        .args(extra)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS)
        .output()
        .expect("run devflow start")
}

fn events_contain_workflow_started(root: &Path) -> bool {
    // An absent events file counts as not containing the event.
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .contains("\"workflow_started\"")
}

/// Sentences the live-run refusals carried for the deleted shell monitor, or
/// that 50-14's `stop` made false. Each must be gone, not merely joined by
/// new text, from every arm of both the start and the resume refusal.
fn removed_refusal_sentences(phase: PhaseId) -> Vec<String> {
    vec![
        "would only mark the state stopped".to_string(),
        "it signals nothing and does not end this run".to_string(),
        "monitor pid".to_string(),
        "an `sh -c` script".to_string(),
        format!("`.devflow/phase-{}-`", phase.padded()),
        "send SIGTERM to the confirmed monitor first".to_string(),
        "signalling the agent first lets the monitor launch the next stage".to_string(),
        "may be running `devflow advance` as a foreground child".to_string(),
        "`ps -ww -o pid=,args= --ppid <monitor pid>`".to_string(),
        "once it takes the lock after this refusal".to_string(),
    ]
}

/// The fragments a refusal must carry for a live supervisor socket that
/// `stop --launch` can address: its launch id and liveness, its socket, and
/// the exact repair line (50-14's `--launch` form works whether or not the
/// state loads).
fn supervisor_arm_fragments(phase: PhaseId, run: &LiveRun) -> Vec<String> {
    let id = run.launch_id();
    vec![
        format!("supervisor launch {id} ALIVE"),
        format!("socket {}", run.socket.display()),
        format!("run `devflow stop --phase {phase} --launch {id}` first"),
        "`devflow stop` ends a supervisor through its socket, whether or not the state file loads"
            .to_string(),
        "on a STALE socket it signals the agent group only after proving that group is this launch's"
            .to_string(),
    ]
}

/// The fragments a refusal must carry for a live agent its pid file names.
/// Every sentence that keeps the operator from signalling the wrong process
/// or launching a second agent is pinned, not only the repair commands
/// (T-48-19-06, T-48-19-07).
fn agent_arm_fragments(phase: PhaseId, pid: u32) -> Vec<String> {
    vec![
        format!("agent pid {pid} alive"),
        "`devflow stop` never signals a pid from the agent pid file".to_string(),
        "Check each pid before signalling it".to_string(),
        // Identity, not the bare executable name `ps -p` prints (48-REVIEW
        // CR-01); `-ww` keeps a long `args` from being cut.
        "`ps -p` shows only the executable name".to_string(),
        format!("agent pid {pid}: ps -ww -o pid=,lstart=,args= -p {pid}"),
        "`__monitor`".to_string(),
        format!("`--phase {phase}`"),
        "`ps -o ppid= -p <pid>`".to_string(),
        "an agent whose supervisor has exited cannot be tied to this phase that way".to_string(),
        "so do not signal it on the pid alone".to_string(),
        "Do neither while this phase's processes are live".to_string(),
        "a later `start` would launch a second agent beside it".to_string(),
    ]
}

/// Drives `start` against a live run and asserts the refusal plus the
/// absence of every side effect. `present` and `absent` are the arm's own
/// fragments; the verb's common fragments and every removed sentence are
/// checked here. `refusal` is the first assertion's message and names the
/// arm.
fn assert_start_refuses_live_run(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    extra: &[&str],
    (present, absent): (&[String], &[String]),
    refusal: &str,
) {
    let fake_bin = common::fake_bin_dir("exit 0");
    let before = phase_file_snapshot(root, phase);
    // Negative control on the snapshot itself: it must cover the planted
    // state and both planted gate files, or byte-identity would prove nothing.
    for required in [
        devflow_core::workflow::state_path(root, phase),
        Gates::gate_path(root, phase, Stage::Code),
        Gates::response_path(root, phase, Stage::Code),
    ] {
        assert!(
            snapshot_paths(&before).contains(&required.as_path()),
            "fixture snapshot must cover {}; it has {:?}",
            required.display(),
            snapshot_paths(&before)
        );
    }
    let branches_before = phase_branches(root, phase);
    let started_before = workflow_started_count(root);

    let output = run_start(home, root, phase, &fake_bin, extra);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{refusal}\nstdout: {stdout}\nstderr: {stderr}"
    );

    let common = [
        phase.to_string(),
        "refusing to start; nothing was written".to_string(),
        "Once every supervisor and process named above is gone, run `devflow start` again."
            .to_string(),
    ];
    for fragment in common.iter().chain(present) {
        assert!(
            stderr.contains(fragment.as_str()),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    // The superseded guidance must be gone, not merely joined by new text.
    for stale in removed_refusal_sentences(phase)
        .into_iter()
        .chain([
            "ps -p ".to_string(),
            "or after `ps -p` shows they are no longer this phase's processes".to_string(),
            "(stop does end a run that holds the lock, for example one parked at a gate.)"
                .to_string(),
        ])
        .chain(absent.iter().cloned())
    {
        assert!(
            !stderr.contains(&stale),
            "refusal must not contain superseded guidance {stale:?}\nstderr: {stderr}"
        );
    }

    let after = phase_file_snapshot(root, phase);
    assert!(
        after == before,
        "a refused start must leave the phase's state, agent pid and gate files \
         byte-identical\nbefore: {:?}\nafter:  {:?}",
        snapshot_paths(&before),
        snapshot_paths(&after)
    );
    let lock = root.join(format!(".devflow/lock-{}", phase.padded()));
    assert!(
        !lock.exists(),
        "a refused start must not leave {}",
        lock.display()
    );
    assert_eq!(
        phase_branches(root, phase),
        branches_before,
        "a refused start must not create or change the phase branch"
    );
    assert_eq!(
        workflow_started_count(root),
        started_before,
        "a refused start must not emit workflow_started"
    );
}

/// `git branch --list feature/phase-<padded>`'s output.
fn phase_branches(root: &Path, phase: PhaseId) -> String {
    let branches = devflow_core::test_support::git_command(root)
        .args([
            "branch",
            "--list",
            &format!("feature/phase-{padded}", padded = phase.padded()),
        ])
        .output()
        .expect("list phase branch");
    assert!(branches.status.success());
    String::from_utf8_lossy(&branches.stdout).into_owned()
}

/// How many `workflow_started` events the project's event log holds.
fn workflow_started_count(root: &Path) -> usize {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .matches("\"workflow_started\"")
        .count()
}

#[test]
fn start_refuses_a_phase_whose_recorded_monitor_is_alive() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(95);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    // Only the supervisor can refuse: the agent pid file is gone.
    run.hide_agent_pid_file(root, phase);
    plant_code_gate(root, phase);
    assert_eq!(workflow_started_count(root), 1, "the live run's own start");

    assert_start_refuses_live_run(
        &home,
        root,
        phase,
        &[],
        (
            &supervisor_arm_fragments(phase, &run),
            &["agent pid".to_string()],
        ),
        "start must refuse a phase whose recorded supervisor is alive",
    );
    assert!(
        matches!(supervisor::probe(&run.socket), Ok(Liveness::Alive(_))),
        "a refused start must leave the live supervisor running"
    );
}

#[test]
fn start_refuses_a_phase_whose_recorded_agent_is_alive() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(96);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    plant_leftover_run(root, phase, Some(agent.pid()));
    let mut present = agent_arm_fragments(phase, agent.pid());
    present.extend([
        // A live recycled pid refuses start again; only recover --clean
        // then start gets past it (48-REVIEW WR-01).
        "If a named pid is live but is not this phase's process (a recycled pid), `start` refuses again"
            .to_string(),
        format!("run `devflow recover --clean --phase {phase}` first, then `devflow start`"),
    ]);

    assert_start_refuses_live_run(
        &home,
        root,
        phase,
        &["--force"],
        (
            &present,
            &[
                "supervisor launch".to_string(),
                format!("devflow stop --phase {phase} --launch"),
            ],
        ),
        "start must refuse a phase whose recorded agent is alive",
    );
}

#[test]
fn start_replaces_a_leftover_state_whose_recorded_processes_are_dead() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(97);
    init_repo(root, phase);
    let planted = plant_leftover_run(root, phase, Some(NEVER_LIVE_PID));
    let fake_bin = common::fake_bin_dir("exit 0");
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);

    let output = run_start(&home, root, phase, &fake_bin, &[]);
    assert!(
        output.status.success(),
        "a leftover state whose recorded processes are dead must be replaced as today\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !planted.request.exists(),
        "the leftover Code gate request must be cleared: {}",
        planted.request.display()
    );
    assert!(
        !planted.response.exists(),
        "the leftover Code gate response must be cleared: {}",
        planted.response.display()
    );
    assert!(
        events_contain_workflow_started(root),
        "replacing a dead leftover must emit workflow_started"
    );
}

#[test]
fn start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(98);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    write_agent_pid(root, phase, agent.pid());
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "the carve-out fixture must have no phase state"
    );
    let fake_bin = common::fake_bin_dir("exit 0");
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![agent.pid()]);

    let output = run_start(&home, root, phase, &fake_bin, &[]);
    assert!(
        output.status.success(),
        "without phase state, start must proceed even if the agent pid file names a live process\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        events_contain_workflow_started(root),
        "the no-state carve-out must emit workflow_started"
    );
}

// ---------------------------------------------------------------------------
// 48-20 (criterion 2, D-01, D-02): `resume` must not overwrite a live run.
// ---------------------------------------------------------------------------

/// The whole start refusal for a live run at phase 94 whose state loads: its
/// supervisor answers ALIVE and its agent pid file names its live agent.
/// `{launch}`, `{socket}` and `{pid}` are that run's.
const START_REFUSAL_GOLDEN_PHASE_94: &str = concat!(
    "phase 94: a run is still live (supervisor launch {launch} ALIVE, agent pid {pid} alive) — ",
    "refusing to start; nothing was written\n",
    "`devflow stop` ends a supervisor through its socket, whether or not the state file loads; ",
    "on a STALE socket it signals the agent group only after proving that group is this ",
    "launch's. Run each line below first:\n",
    "  launch {launch} (ALIVE, socket {socket}): run `devflow stop --phase 94 --launch {launch}` first\n",
    "The agent pid file names a live process, and `devflow stop` never signals a pid from the ",
    "agent pid file. Check each pid before signalling it. `ps -p` shows only the executable name, ",
    "which cannot tell phases apart; `-ww` keeps a long command line from being cut at the ",
    "terminal width:\n",
    "  agent pid {pid}: ps -ww -o pid=,lstart=,args= -p {pid}\n",
    "This phase's agent has its supervisor, a `__monitor` process with this project's path and ",
    "`--phase 94` in its args, as its parent (`ps -o ppid= -p <pid>`); an agent whose supervisor ",
    "has exited cannot be tied to this phase that way, so do not signal it on the pid alone. ",
    "If a named pid is live but is not this phase's process (a recycled pid), `start` refuses ",
    "again: run `devflow recover --clean --phase 94` first, then `devflow start`. Do neither ",
    "while this phase's processes are live: `recover --clean` clears state even while an agent ",
    "runs, and a later `start` would launch a second agent beside it.\n",
    "Once every supervisor and process named above is gone, run `devflow start` again.",
);

fn events_contain(root: &Path, event: &str) -> bool {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .contains(&format!("\"{event}\""))
}

/// A resumable Code-stage state with no supervisor record and an optional
/// agent pid file. No `monitor_pid` is planted: the launch guard reads none
/// (50-15).
fn plant_resumable_run(root: &Path, phase: PhaseId, agent_pid: Option<u32>, stopped: bool) {
    let mut state = State::new(
        phase,
        AgentKind::Claude,
        Mode::Supervise,
        root.to_path_buf(),
    );
    state.stage = Stage::Code;
    state.legacy_claude_launch = true;
    state.stopped = stopped;
    state.stop_reason =
        stopped.then(|| "planted: marked stopped by devflow stop while the run was live".into());
    devflow_core::workflow::save_state(&state).expect("plant resumable state");
    if let Some(pid) = agent_pid {
        write_agent_pid(root, phase, pid);
    }
}

/// `devflow resume` for `phase`; its `DEVFLOW_CACHE_DIR` is `home`'s cache dir,
/// set by `common::hermetic_devflow`.
fn resume_command(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = common::hermetic_devflow(home);
    command
        .args([
            "resume",
            "--phase",
            &phase.to_string(),
            "--legacy-claude-launch",
        ])
        .arg(root)
        .current_dir(root)
        .env("PATH", path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS);
    command
}

struct RefusalCase {
    required: Vec<PathBuf>,
    /// The arm's own fragments.
    present: Vec<String>,
    /// Text this arm must not contain.
    absent: Vec<String>,
    /// A live agent pid the refusal must leave running.
    spared_agent: Option<u32>,
    refusal: &'static str,
}

fn assert_resume_refuses_live_run(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    case: &RefusalCase,
) {
    let before = phase_file_snapshot(root, phase);
    for required in &case.required {
        assert!(
            snapshot_paths(&before).contains(&required.as_path()),
            "fixture snapshot must cover {}; it has {:?}",
            required.display(),
            snapshot_paths(&before)
        );
    }
    let launched_before = stage_launched_count(root);

    let output = resume_command(home, root, phase, fake_bin)
        .output()
        .expect("run devflow resume");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{}\nstdout: {stdout}\nstderr: {stderr}",
        case.refusal
    );

    let padded = phase.padded();
    let common = [
        "refusing to resume; nothing was written".to_string(),
        format!(
            "Once every supervisor and process named above is gone, run `devflow resume --phase {phase}` again."
        ),
    ];
    for fragment in common.iter().chain(&case.present) {
        assert!(
            stderr.contains(fragment.as_str()),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    for stale in removed_refusal_sentences(phase)
        .into_iter()
        .chain([
            "refusing to start".to_string(),
            "run `devflow start` again".to_string(),
            "as soon as this start exits".to_string(),
            "ps -p ".to_string(),
        ])
        .chain(case.absent.iter().cloned())
    {
        assert!(
            !stderr.contains(&stale),
            "refusal must not contain start-only or superseded text {stale:?}\nstderr: {stderr}"
        );
    }

    let after = phase_file_snapshot(root, phase);
    assert_eq!(
        after, before,
        "a refused resume must leave phase files byte-identical"
    );
    assert!(
        !root.join(format!(".devflow/lock-{padded}")).exists(),
        "a refused resume must not leave a phase lock"
    );
    assert_eq!(
        stage_launched_count(root),
        launched_before,
        "a refused resume must not emit stage_launched"
    );
    if let Some(pid) = case.spared_agent {
        assert!(
            devflow_core::agent::agent_running(pid),
            "a refused resume must not signal the recorded agent {pid}"
        );
    }
}

/// How many `stage_launched` events the project's event log holds.
fn stage_launched_count(root: &Path) -> usize {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .matches("\"stage_launched\"")
        .count()
}

/// The resume agent arm's fragments beyond [`agent_arm_fragments`].
fn resume_agent_fragments(phase: PhaseId) -> Vec<String> {
    vec![
        "A state already marked stopped does not mean these processes have exited".to_string(),
        "`devflow status` names `devflow resume` for a phase whose supervisor is GONE even while its agent is still alive".to_string(),
        "resume refuses until every named process has exited".to_string(),
        "If a named pid is live but is not this phase's process (a recycled pid), `resume` refuses again".to_string(),
        "no DevFlow verb clears a recorded pid while keeping the phase's work".to_string(),
        format!("`devflow recover --clean --phase {phase}` removes the phase state, after which `resume` has nothing to resume, and `devflow start` refuses while the phase's worktree or branch exists"),
        "`devflow start --force` recreates them from the base and discards the phase's work".to_string(),
    ]
}

#[test]
fn resume_refuses_a_phase_whose_recorded_monitor_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let home = common::HermeticHome::new();
    let root = repo.path();
    let phase = PhaseId::new(85);
    init_repo(root, phase);
    let fake_bin = common::fake_bin_dir("exec sleep 60");
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    // Only the supervisor can refuse: the agent pid file is gone.
    run.hide_agent_pid_file(root, phase);

    assert_resume_refuses_live_run(
        &home,
        root,
        phase,
        &fake_bin,
        &RefusalCase {
            required: vec![devflow_core::workflow::state_path(root, phase)],
            present: supervisor_arm_fragments(phase, &run),
            absent: vec!["agent pid".to_string()],
            spared_agent: None,
            refusal: "resume must refuse a phase whose recorded supervisor is alive",
        },
    );
    assert!(
        matches!(supervisor::probe(&run.socket), Ok(Liveness::Alive(_))),
        "a refused resume must leave the live supervisor running"
    );
}

#[test]
fn resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let home = common::HermeticHome::new();
    let root = repo.path();
    let phase = PhaseId::new(86);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    let fake_bin = common::fake_bin_dir("exec sleep 60");
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![agent.pid()]);
    plant_resumable_run(root, phase, Some(agent.pid()), true);
    let mut present = agent_arm_fragments(phase, agent.pid());
    present.extend(resume_agent_fragments(phase));
    let case = RefusalCase {
        required: vec![
            devflow_core::workflow::state_path(root, phase),
            devflow_core::agent_result::agent_pid_path(root, phase),
        ],
        present,
        absent: vec![
            "supervisor launch".to_string(),
            format!("devflow stop --phase {phase} --launch"),
        ],
        spared_agent: Some(agent.pid()),
        refusal: "resume must refuse a phase whose recorded agent is alive",
    };

    assert_resume_refuses_live_run(&home, root, phase, &fake_bin, &case);
    assert_resume_refuses_live_run(&home, root, phase, &fake_bin, &case);
    assert!(
        devflow_core::workflow::load_state(root, phase)
            .expect("load planted state")
            .stopped,
        "the stop mark must survive a refused resume"
    );
}

#[test]
fn resume_relaunches_a_phase_whose_recorded_processes_are_dead() {
    for (phase, stopped) in [(PhaseId::new(87), false), (PhaseId::new(88), true)] {
        let repo = tempfile::tempdir().unwrap();
        let home = common::HermeticHome::new();
        let root = repo.path();
        init_repo(root, phase);
        let fake_bin = common::fake_bin_dir("exec sleep 60");
        let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
        plant_resumable_run(root, phase, Some(NEVER_LIVE_PID), stopped);

        let output = resume_command(&home, root, phase, &fake_bin)
            .output()
            .expect("run devflow resume");
        assert!(
            output.status.success(),
            "a resume over recorded processes that are dead must relaunch as today (stopped={stopped})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let state = devflow_core::workflow::load_state(root, phase).expect("load relaunched state");
        assert!(!state.stopped, "resume must clear the stop mark");
        assert!(
            state.supervisor.is_some(),
            "the relaunch must record its supervisor: {state:?}"
        );
        assert!(
            events_contain(root, "stage_launched"),
            "a resumed dead leftover must emit stage_launched"
        );
    }
}

#[test]
fn resume_without_phase_state_reports_the_missing_state_not_a_live_run() {
    let repo = tempfile::tempdir().unwrap();
    let home = common::HermeticHome::new();
    let root = repo.path();
    let phase = PhaseId::new(84);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    let fake_bin = common::fake_bin_dir("exec sleep 60");
    write_agent_pid(root, phase, agent.pid());
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "the carve-out fixture must have no phase state"
    );
    let before = phase_file_snapshot(root, phase);

    let output = resume_command(&home, root, phase, &fake_bin)
        .output()
        .expect("run devflow resume");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "resume without phase state must still fail\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !stderr.contains("refusing to resume"),
        "without phase state, resume must report the missing state, not a live run\nstderr: {stderr}"
    );
    assert_eq!(phase_file_snapshot(root, phase), before);
    assert!(devflow_core::agent::agent_running(agent.pid()));
    assert!(!events_contain(root, "stage_launched"));
}

#[test]
fn start_refusal_text_stays_byte_identical_for_a_live_monitor() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(94);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    let agent = run.sidecar.agent_pid;
    let fake_bin = common::fake_bin_dir("exit 0");

    let output = run_start(&home, root, phase, &fake_bin, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "start must still refuse a phase whose recorded supervisor is alive\nstderr: {stderr}"
    );
    let expected = format!(
        "error: {}\n",
        START_REFUSAL_GOLDEN_PHASE_94
            .replace("{launch}", run.launch_id())
            .replace("{socket}", &run.socket.display().to_string())
            .replace("{pid}", &agent.to_string())
    );
    assert!(
        stderr.contains(&expected),
        "the start refusal must match the golden byte for byte\nexpected: {expected}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains(&expected.replacen("refusing to start", "refusing to resume", 1)),
        "the golden comparison must be sensitive to the launch verb\nstderr: {stderr}"
    );
    for placeholder in ["{launch}", "{socket}", "{pid}"] {
        assert!(
            START_REFUSAL_GOLDEN_PHASE_94.contains(placeholder),
            "the golden must carry {placeholder}, or the substitution proves nothing"
        );
    }
    assert!(START_REFUSAL_GOLDEN_PHASE_94.ends_with(
        "Once every supervisor and process named above is gone, run `devflow start` again."
    ));
}

// ---------------------------------------------------------------------------
// 50-15 (D-09, R-12, 999.139): a state file that will not load must not hide
// a live run from `start` or `resume`. Each fixture is a real launched run
// whose agent pid file is removed, so only its supervisor socket can refuse.
// ---------------------------------------------------------------------------

/// Bytes no `State` deserializes.
const CORRUPT_JSON: &[u8] = b"{ this does not parse";

/// Overwrite `phase`'s state file with invalid JSON; returns the bytes.
fn corrupt_state(root: &Path, phase: PhaseId) -> Vec<u8> {
    let path = devflow_core::workflow::state_path(root, phase);
    fs::write(&path, CORRUPT_JSON).expect("corrupt the state file");
    assert!(
        devflow_core::workflow::load_state(root, phase).is_err(),
        "the corrupted state must not load"
    );
    CORRUPT_JSON.to_vec()
}

/// Every `.sock` file under `home`'s socket directory, sorted.
fn sockets_under(home: &common::HermeticHome) -> Vec<PathBuf> {
    let mut sockets: Vec<PathBuf> = fs::read_dir(home.cache_dir().join("sock"))
        .map(|entries| {
            entries
                .map(|entry| entry.expect("read sock dir entry").path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "sock"))
                .collect()
        })
        .unwrap_or_default();
    sockets.sort();
    sockets
}

/// `devflow start --force` for `phase`, as the 48-19 refusal tests run it.
fn force_start(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Output {
    let fake_bin = common::fake_bin_dir("exit 0");
    run_start(home, root, phase, &fake_bin, &["--force"])
}

/// Run `launch` (a `start` or `resume` over a corrupt state) and assert it is
/// refused with no side effect: no new socket, the corrupt state bytes
/// unchanged, no `workflow_started` or `stage_launched` appended. Returns
/// its stderr.
fn assert_refused_over_a_corrupt_state(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    launch: impl FnOnce() -> Output,
) -> String {
    let state_file = devflow_core::workflow::state_path(root, phase);
    let state_before = fs::read(&state_file).expect("read the corrupt state");
    let sockets_before = sockets_under(home);
    let started_before = workflow_started_count(root);
    let launched_before = stage_launched_count(root);

    let output = launch();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "a launch over a live run whose state will not load must refuse\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        sockets_under(home),
        sockets_before,
        "a refused launch must publish no second socket\nstderr: {stderr}"
    );
    assert_eq!(
        fs::read(&state_file).expect("read the state after the refusal"),
        state_before,
        "a refused launch must leave the corrupt state bytes unchanged"
    );
    assert_eq!(
        workflow_started_count(root),
        started_before,
        "a refused launch must not emit workflow_started"
    );
    assert_eq!(
        stage_launched_count(root),
        launched_before,
        "a refused launch must not emit stage_launched"
    );
    stderr
}

/// The backticked `devflow stop --phase …` command in `stderr`, split into
/// its words without the leading `devflow`.
fn printed_stop_command(stderr: &str, phase: PhaseId) -> Vec<String> {
    let head = format!("`devflow stop --phase {phase}");
    let at = stderr
        .find(&head)
        .unwrap_or_else(|| panic!("no {head}… command in the refusal\nstderr: {stderr}"));
    let command = &stderr[at + 1..];
    let end = command
        .find('`')
        .expect("the printed command closes its backtick");
    command[..end]
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect()
}

/// Run a printed `devflow stop` command exactly as printed, from `root`.
fn run_printed_stop(home: &common::HermeticHome, root: &Path, words: &[String]) -> Output {
    common::hermetic_devflow(home)
        .args(words)
        .current_dir(root)
        .output()
        .expect("run the printed devflow stop")
}

/// Whether process group `pgid` has no member left (`kill(-pgid, 0)` fails
/// with ESRCH).
fn group_is_gone(pgid: u32) -> bool {
    let pgid = libc::pid_t::try_from(pgid).expect("pgid fits pid_t");
    // SAFETY: signal 0 only checks for existence; nothing is delivered.
    let status = unsafe { libc::kill(-pgid, 0) };
    status == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[test]
fn start_refuses_over_a_live_supervisor_when_the_state_will_not_load() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(70);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    run.hide_agent_pid_file(root, phase);
    corrupt_state(root, phase);

    let stderr =
        assert_refused_over_a_corrupt_state(&home, root, phase, || force_start(&home, root, phase));
    let id = run.launch_id();
    for fragment in [
        "refusing to start; nothing was written".to_string(),
        format!("supervisor launch {id} ALIVE"),
        format!("run `devflow stop --phase {phase} --launch {id}` first"),
    ] {
        assert!(
            stderr.contains(&fragment),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    assert!(
        !stderr.contains("agent pid"),
        "only the socket may refuse here (the pid file is gone)\nstderr: {stderr}"
    );
    assert!(
        matches!(supervisor::probe(&run.socket), Ok(Liveness::Alive(_))),
        "a refused start must leave the live supervisor running"
    );
}

#[test]
fn the_corrupt_state_refusal_names_a_stop_that_ends_the_run() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(71);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    run.hide_agent_pid_file(root, phase);
    let corrupt = corrupt_state(root, phase);
    let stderr =
        assert_refused_over_a_corrupt_state(&home, root, phase, || force_start(&home, root, phase));
    let words = printed_stop_command(&stderr, phase);
    assert_eq!(
        words,
        [
            "stop",
            "--phase",
            &phase.to_string(),
            "--launch",
            run.launch_id()
        ],
        "the refusal must print the `--launch` form\nstderr: {stderr}"
    );

    let stop = run_printed_stop(&home, root, &words);
    assert!(
        stop.status.success(),
        "the printed stop must end the run\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    common::wait_for(
        || matches!(supervisor::probe(&run.socket), Ok(Liveness::Gone)),
        Duration::from_secs(5),
        "the stopped supervisor's socket to probe Gone",
    );
    common::wait_for(
        || group_is_gone(run.sidecar.agent_pgid),
        Duration::from_secs(5),
        "the stopped run's agent group to be gone",
    );
    assert_eq!(
        fs::read(devflow_core::workflow::state_path(root, phase)).unwrap(),
        corrupt,
        "stop must leave the corrupt state bytes unchanged"
    );

    let again = force_start(&home, root, phase);
    let again_stderr = String::from_utf8_lossy(&again.stderr);
    assert!(
        !again_stderr.contains("a run is still live"),
        "once the run is stopped, start must not refuse over it\nstderr: {again_stderr}"
    );
}

#[test]
fn resume_refuses_over_a_live_supervisor_when_the_state_will_not_load() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(72);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    run.hide_agent_pid_file(root, phase);
    corrupt_state(root, phase);
    let fake_bin = common::fake_bin_dir("exec sleep 60");

    let stderr = assert_refused_over_a_corrupt_state(&home, root, phase, || {
        resume_command(&home, root, phase, &fake_bin)
            .output()
            .expect("run devflow resume")
    });
    let id = run.launch_id();
    for fragment in [
        "refusing to resume; nothing was written".to_string(),
        format!("supervisor launch {id} ALIVE"),
        format!("run `devflow stop --phase {phase} --launch {id}` first"),
    ] {
        assert!(
            stderr.contains(&fragment),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    assert!(
        matches!(supervisor::probe(&run.socket), Ok(Liveness::Alive(_))),
        "a refused resume must leave the live supervisor running"
    );
}

#[test]
fn start_proceeds_over_a_corrupt_state_with_no_live_supervisor() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(73);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    run.hide_agent_pid_file(root, phase);
    match supervisor::request_shutdown(&run.socket, LIVE_RUN_SHUTDOWN_WAIT)
        .expect("shut the fixture's run down")
    {
        ShutdownOutcome::Stopped { .. } => {}
        other => panic!("the fixture's shutdown must stop the run, got {other:?}"),
    }
    common::wait_for(
        || !run.socket.exists() && !run.sidecar_path().exists(),
        Duration::from_secs(5),
        "the shut-down run's socket and sidecar to be removed",
    );
    corrupt_state(root, phase);
    let started_before = workflow_started_count(root);

    let output = force_start(&home, root, phase);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "with no live supervisor, start over a corrupt state proceeds as today\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !stderr.contains("a run is still live"),
        "no refusal over a dead run\nstderr: {stderr}"
    );
    assert_eq!(
        workflow_started_count(root),
        started_before + 1,
        "the start over the corrupt state must emit workflow_started"
    );
}

#[test]
fn start_refuses_when_the_state_and_the_live_sidecar_both_will_not_load() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(74);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    run.hide_agent_pid_file(root, phase);
    let (pgid, started) = (run.sidecar.agent_pgid, run.sidecar.agent_start_time);
    corrupt_state(root, phase);
    fs::write(run.sidecar_path(), CORRUPT_JSON).expect("corrupt the live sidecar");
    assert!(supervisor::read_sidecar(&run.sidecar_path()).is_err());

    let stderr =
        assert_refused_over_a_corrupt_state(&home, root, phase, || force_start(&home, root, phase));
    for fragment in [
        "refusing to start; nothing was written".to_string(),
        format!("socket {} (ALIVE", run.socket.display()),
        format!("launch {}", run.launch_id()),
        "does not parse".to_string(),
        format!("run `devflow stop --phase {phase}` first"),
        "one that does not answer cannot be validated without its sidecar".to_string(),
    ] {
        assert!(
            stderr.contains(&fragment),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    assert!(
        !stderr.contains("--launch"),
        "a socket with no usable sidecar is not addressed by `--launch`\nstderr: {stderr}"
    );
    let words = printed_stop_command(&stderr, phase);
    assert_eq!(words, ["stop", "--phase", &phase.to_string()]);

    assert!(
        devflow_core::agent::is_same_process(pgid, started),
        "the agent group's leader must still be the launched agent before the stop"
    );
    let stop = run_printed_stop(&home, root, &words);
    assert!(
        stop.status.success(),
        "the printed stop must end the run\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    common::wait_for(
        || matches!(supervisor::probe(&run.socket), Ok(Liveness::Gone)),
        Duration::from_secs(5),
        "the stopped supervisor's socket to probe Gone",
    );
    common::wait_for(
        || group_is_gone(pgid),
        Duration::from_secs(5),
        "the stopped run's agent group to be gone",
    );
}

#[test]
fn a_live_socket_named_for_another_root_does_not_block_a_corrupt_state_start() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let other_repo = tempfile::tempdir().unwrap();
    let other = other_repo.path();
    let phase = PhaseId::new(75);
    init_repo(root, phase);
    init_repo(other, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let _other_reaper = common::LaunchedRunReaper::new(other, phase, vec![]);
    let run = LiveRun::launch(&home, root, phase);
    run.hide_agent_pid_file(root, phase);
    corrupt_state(root, phase);
    fs::write(run.sidecar_path(), CORRUPT_JSON).expect("corrupt the live sidecar");
    fs::create_dir_all(other.join(".devflow")).unwrap();
    corrupt_state(other, phase);

    let output = force_start(&home, other, phase);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("devflow stop --phase"),
        "a socket named for another root must not refuse this root's start\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("a run is still live"),
        "no live-run refusal for another root's socket\nstderr: {stderr}"
    );
    assert!(
        matches!(supervisor::probe(&run.socket), Ok(Liveness::Alive(_))),
        "the other root's start must leave this root's supervisor running"
    );
    match supervisor::request_shutdown(&run.socket, LIVE_RUN_SHUTDOWN_WAIT)
        .expect("shut this root's run down")
    {
        ShutdownOutcome::Stopped { .. } => {}
        other => panic!("the shutdown must stop the run, got {other:?}"),
    }
}

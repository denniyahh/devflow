//! End-to-end coverage for `start`'s per-phase writer lock.
//!
//! These tests deliberately hold the lock from the test process, then drive
//! the compiled CLI as a separate process. That makes the refusal observable
//! at the same boundary an operator uses, rather than coupling the test to
//! `commands::start`'s implementation details.

use devflow_core::gates::{GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

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

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fake_bin_dir(agent_body: &str) -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::write(&claude, format!("#!/bin/sh\n{agent_body}\n")).unwrap();
    let mut permissions = fs::metadata(&claude).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&claude, permissions).unwrap();
    for program in ["git", "sh"] {
        symlink(format!("/usr/bin/{program}"), dir.path().join(program)).unwrap();
    }
    FakeBin {
        path: dir.path().to_path_buf(),
        _dir: dir,
    }
}

fn child(root: &Path, phase: PhaseId, fake_bin: &FakeBin, mode: &str) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = Command::new(devflow_bin());
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

fn wait_for(mut predicate: impl FnMut() -> bool, timeout: Duration, what: &str) {
    let started = Instant::now();
    while !predicate() {
        assert!(
            started.elapsed() < timeout,
            "timed out after {timeout:?} waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn kill_and_reap(child: &mut Child) {
    if child.try_wait().expect("poll start child").is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[test]
fn start_refuses_while_another_process_holds_the_phase_lock() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(90);
    init_repo(root, phase);
    let fake_bin = fake_bin_dir("exit 0");
    let holder = devflow_core::lock::acquire(root, phase).expect("test holds phase lock");
    let holder_pid = std::process::id();
    let worktrees_before = devflow_core::test_support::git_command(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("list baseline worktrees");
    assert!(worktrees_before.status.success());

    let output = child(root, phase, &fake_bin, "supervise")
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(91);
    init_repo(root, phase);
    let fake_bin = fake_bin_dir("exit 0");

    let output = child(root, phase, &fake_bin, "supervise")
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(92);
    init_repo(root, phase);
    let fake_bin = fake_bin_dir("sleep 60");
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

    let mut process = child(root, phase, &fake_bin, "auto")
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30")
        .spawn()
        .expect("spawn devflow start");
    wait_for(
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
    kill_and_reap(&mut process);
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

/// The planted Code gate files, returned so a control can assert they are gone.
struct PlantedGate {
    request: PathBuf,
    response: PathBuf,
}

fn plant_leftover_run(
    root: &Path,
    phase: PhaseId,
    monitor_pid: Option<u32>,
    agent_pid: Option<u32>,
) -> PlantedGate {
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state.monitor_pid = monitor_pid;
    devflow_core::workflow::save_state(&state).expect("plant leftover state");
    if let Some(pid) = agent_pid {
        write_agent_pid(root, phase, pid);
    }
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

fn run_start(root: &Path, phase: PhaseId, fake_bin: &FakeBin, extra: &[&str]) -> Output {
    child(root, phase, fake_bin, "supervise")
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

/// Drives `start` against a planted live run and asserts the refusal plus
/// the absence of every side effect. `refusal` is the first assertion's
/// message and names the arm.
fn assert_start_refuses_live_run(
    root: &Path,
    phase: PhaseId,
    extra: &[&str],
    live_pid: u32,
    refusal: &str,
) {
    let fake_bin = fake_bin_dir("exit 0");
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

    let output = run_start(root, phase, &fake_bin, extra);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{refusal}\nstdout: {stdout}\nstderr: {stderr}"
    );

    for fragment in [
        phase.to_string(),
        live_pid.to_string(),
        "refusing to start; nothing was written".to_string(),
        format!("devflow stop --phase {phase}"),
        "would only mark the state stopped".to_string(),
        // stop is not a no-op once this run's own advance holds the lock
        // (48-REVIEW WR-02).
        "once it takes the lock after this refusal".to_string(),
        // Identity, not the bare executable name `ps -p` prints (48-REVIEW
        // CR-01). `-ww`: at a terminal `args` is cut at the screen width,
        // which drops the Legacy script's phase paths.
        format!("ps -ww -o pid=,lstart=,args= -p {live_pid}"),
        "`ps -p` shows only the executable name".to_string(),
        format!("`--phase {phase}`"),
        "`__monitor`".to_string(),
        format!("`.devflow/phase-{}-`", phase.padded()),
        "`ps -o ppid= -p <pid>`".to_string(),
        // A Legacy monitor in its advance tail defers SIGTERM behind a
        // foreground `devflow advance` child (48-REVIEW WR-02).
        "may be running `devflow advance` as a foreground child".to_string(),
        "`ps -ww -o pid=,args= --ppid <monitor pid>`".to_string(),
        "signal that child, not only the monitor".to_string(),
        // A live recycled pid refuses start again; only recover --clean
        // then start gets past it (48-REVIEW WR-01).
        "If the named processes have exited, run `devflow start` again".to_string(),
        format!("run `devflow recover --clean --phase {phase}` first, then `devflow start`"),
        // Every sentence that keeps the operator from signalling the wrong
        // process or launching a second agent is pinned, not only the
        // repair commands (T-48-19-06, T-48-19-07): a text that drops these
        // but keeps the commands must fail.
        "Check each pid before signalling it".to_string(),
        "so do not signal it on the pid alone".to_string(),
        "send SIGTERM to the confirmed monitor first".to_string(),
        "signalling the agent first lets the monitor launch the next stage".to_string(),
        "If a named pid is live but is not this phase's process (a recycled pid), `start` refuses again"
            .to_string(),
        "Do neither while this phase's processes are live".to_string(),
        "a later `start` would launch a second agent beside it".to_string(),
    ] {
        assert!(
            stderr.contains(&fragment),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    // The superseded guidance must be gone, not merely joined by new text.
    for stale in [
        format!("ps -p {live_pid}"),
        "or after `ps -p` shows they are no longer this phase's processes".to_string(),
        "(stop does end a run that holds the lock, for example one parked at a gate.)".to_string(),
    ] {
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
    let branches = devflow_core::test_support::git_command(root)
        .args([
            "branch",
            "--list",
            &format!("feature/phase-{padded}", padded = phase.padded()),
        ])
        .output()
        .expect("list phase branch");
    assert!(branches.status.success());
    assert!(
        String::from_utf8_lossy(&branches.stdout).trim().is_empty(),
        "a refused start must not create the phase branch: {}",
        String::from_utf8_lossy(&branches.stdout)
    );
    assert!(
        !events_contain_workflow_started(root),
        "a refused start must not emit workflow_started"
    );
}

#[test]
fn start_refuses_a_phase_whose_recorded_monitor_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(95);
    init_repo(root, phase);
    let monitor = LiveProcess::spawn();
    plant_leftover_run(root, phase, Some(monitor.pid()), None);

    assert_start_refuses_live_run(
        root,
        phase,
        &[],
        monitor.pid(),
        "start must refuse a phase whose recorded monitor is alive",
    );
}

#[test]
fn start_refuses_a_phase_whose_recorded_agent_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(96);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    plant_leftover_run(root, phase, None, Some(agent.pid()));

    assert_start_refuses_live_run(
        root,
        phase,
        &["--force"],
        agent.pid(),
        "start must refuse a phase whose recorded agent is alive",
    );
}

#[test]
fn start_replaces_a_leftover_state_whose_recorded_processes_are_dead() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(97);
    init_repo(root, phase);
    let planted = plant_leftover_run(root, phase, Some(NEVER_LIVE_PID), Some(NEVER_LIVE_PID));
    let fake_bin = fake_bin_dir("exit 0");

    let output = run_start(root, phase, &fake_bin, &[]);
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
    let fake_bin = fake_bin_dir("exit 0");

    let output = run_start(root, phase, &fake_bin, &[]);
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

const START_REFUSAL_GOLDEN_PHASE_94: &str = "phase 94: a run is still live (monitor pid {pid} alive) — refusing to start; nothing was written\n\
         No process holds the phase lock right now, so `devflow stop --phase 94` would only mark the state stopped: it signals nothing and does not end this run. stop ends a run through the process that holds the lock: one parked at a gate, or this run's own `devflow advance` once it takes the lock after this refusal.\n\
         Check each pid before signalling it. `ps -p` shows only the executable name, which cannot tell phases apart; `-ww` keeps a long command line from being cut at the terminal width:\n\
         \x20\x20monitor pid {pid}: ps -ww -o pid=,lstart=,args= -p {pid}\n\
         This phase's monitor is either a `__monitor` process with this project's path and `--phase 94` in its args, or an `sh -c` script that names `.devflow/phase-94-` files and ends in `advance` with `--phase 94`. This phase's agent has that monitor as its parent (`ps -o ppid= -p <pid>`); an agent whose monitor has exited cannot be tied to this phase that way, so do not signal it on the pid alone.\n\
         To end this run now, send SIGTERM to the confirmed monitor first — signalling the agent first lets the monitor launch the next stage. If the agent has already exited, an `sh` monitor may be running `devflow advance` as a foreground child: the monitor defers SIGTERM until that child returns, and the child may take the lock as soon as this start exits. Find it with `ps -ww -o pid=,args= --ppid <monitor pid>` and signal that child, not only the monitor.\n\
         If the named processes have exited, run `devflow start` again. If a named pid is live but is not this phase's process (a recycled pid), `start` refuses again: run `devflow recover --clean --phase 94` first, then `devflow start`. Do neither while this phase's processes are live: `recover --clean` clears state even while an agent runs, and a later `start` would launch a second agent beside it.";

fn events_contain(root: &Path, event: &str) -> bool {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .contains(&format!("\"{event}\""))
}

fn plant_resumable_run(
    root: &Path,
    phase: PhaseId,
    monitor_pid: Option<u32>,
    agent_pid: Option<u32>,
    stopped: bool,
) {
    let mut state = State::new(
        phase,
        AgentKind::Claude,
        Mode::Supervise,
        root.to_path_buf(),
    );
    state.stage = Stage::Code;
    state.monitor_pid = monitor_pid;
    state.legacy_claude_launch = true;
    state.stopped = stopped;
    state.stop_reason =
        stopped.then(|| "planted: marked stopped by devflow stop while the run was live".into());
    devflow_core::workflow::save_state(&state).expect("plant resumable state");
    if let Some(pid) = agent_pid {
        write_agent_pid(root, phase, pid);
    }
}

fn resume_command(root: &Path, phase: PhaseId, fake_bin: &FakeBin, cache_dir: &Path) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = Command::new(devflow_bin());
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
        .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS)
        .env("DEVFLOW_CACHE_DIR", cache_dir);
    command
}

struct LaunchedRunReaper {
    root: PathBuf,
    phase: PhaseId,
    spare: Vec<u32>,
}

impl LaunchedRunReaper {
    fn new(root: &Path, phase: PhaseId, spare: Vec<u32>) -> Self {
        Self {
            root: root.to_path_buf(),
            phase,
            spare,
        }
    }
}

impl Drop for LaunchedRunReaper {
    fn drop(&mut self) {
        let monitor = devflow_core::workflow::load_state(&self.root, self.phase)
            .ok()
            .and_then(|state| state.monitor_pid);
        let agent = fs::read_to_string(devflow_core::agent_result::agent_pid_path(
            &self.root, self.phase,
        ))
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok());
        for pid in [monitor, agent].into_iter().flatten() {
            if !self.spare.contains(&pid) && devflow_core::agent::agent_running(pid) {
                let _ = devflow_core::agent::terminate_and_verify(
                    pid,
                    devflow_core::agent::TERMINATE_VERIFY_WAIT,
                    devflow_core::agent::TERMINATE_VERIFY_POLL,
                );
            }
        }
    }
}

struct RefusalCase {
    role: &'static str,
    live_pid: u32,
    required: Vec<PathBuf>,
    never_named: Vec<String>,
    refusal: &'static str,
}

fn assert_resume_refuses_live_run(
    root: &Path,
    phase: PhaseId,
    fake_bin: &FakeBin,
    cache_dir: &Path,
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

    let output = resume_command(root, phase, fake_bin, cache_dir)
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
    for fragment in [
        format!(
            "a run is still live ({} pid {} alive) — refusing to resume; nothing was written",
            case.role, case.live_pid
        ),
        format!("`devflow stop --phase {phase}` would only mark the state stopped: it signals nothing and does not end this run"),
        "once it takes the lock after this refusal".to_string(),
        "A state already marked stopped does not mean these processes have exited".to_string(),
        "`devflow status` and `devflow doctor` name `devflow resume` for a phase whose monitor is dead even while its agent is still alive".to_string(),
        "resume refuses until every named process has exited".to_string(),
        "Check each pid before signalling it".to_string(),
        "`ps -p` shows only the executable name".to_string(),
        format!("{} pid {}: ps -ww -o pid=,lstart=,args= -p {}", case.role, case.live_pid, case.live_pid),
        "`__monitor`".to_string(),
        format!("`--phase {phase}`"),
        format!("`.devflow/phase-{padded}-`"),
        "`ps -o ppid= -p <pid>`".to_string(),
        "so do not signal it on the pid alone".to_string(),
        "send SIGTERM to the confirmed monitor first".to_string(),
        "signalling the agent first lets the monitor launch the next stage".to_string(),
        "may be running `devflow advance` as a foreground child".to_string(),
        "the child may take the lock as soon as this resume exits".to_string(),
        "`ps -ww -o pid=,args= --ppid <monitor pid>`".to_string(),
        "signal that child, not only the monitor".to_string(),
        format!("If the named processes have exited, run `devflow resume --phase {phase}` again"),
        "If a named pid is live but is not this phase's process (a recycled pid), `resume` refuses again".to_string(),
        "no DevFlow verb clears a recorded pid while keeping the phase's work".to_string(),
        format!("`devflow recover --clean --phase {phase}` removes the phase state, after which `resume` has nothing to resume, and `devflow start` refuses while the phase's worktree or branch exists"),
        "`devflow start --force` recreates them from the base and discards the phase's work".to_string(),
        "Do neither while this phase's processes are live".to_string(),
        "a later `start` would launch a second agent beside it".to_string(),
    ] {
        assert!(
            stderr.contains(&fragment),
            "refusal must contain {fragment:?}\nstderr: {stderr}"
        );
    }
    for stale in [
        "refusing to start".to_string(),
        "run `devflow start` again".to_string(),
        "as soon as this start exits".to_string(),
        format!("ps -p {}", case.live_pid),
    ]
    .into_iter()
    .chain(case.never_named.iter().cloned())
    {
        assert!(
            !stderr.contains(&stale),
            "refusal must not contain start-only or dead-pid text {stale:?}\nstderr: {stderr}"
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
    assert!(
        !events_contain(root, "stage_launched"),
        "a refused resume must not emit stage_launched"
    );
    assert!(
        devflow_core::agent::agent_running(case.live_pid),
        "a refused resume must not signal the recorded {}",
        case.role
    );
}

#[test]
fn resume_refuses_a_phase_whose_recorded_monitor_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(85);
    init_repo(root, phase);
    let monitor = LiveProcess::spawn();
    let fake_bin = fake_bin_dir("exec sleep 60");
    let _reaper = LaunchedRunReaper::new(root, phase, vec![monitor.pid()]);
    plant_resumable_run(
        root,
        phase,
        Some(monitor.pid()),
        Some(NEVER_LIVE_PID),
        false,
    );

    assert_resume_refuses_live_run(
        root,
        phase,
        &fake_bin,
        cache.path(),
        &RefusalCase {
            role: "monitor",
            live_pid: monitor.pid(),
            required: vec![
                devflow_core::workflow::state_path(root, phase),
                devflow_core::agent_result::agent_pid_path(root, phase),
            ],
            never_named: vec![format!("agent pid {NEVER_LIVE_PID}")],
            refusal: "resume must refuse a phase whose recorded monitor is alive",
        },
    );
}

#[test]
fn resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(86);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    let fake_bin = fake_bin_dir("exec sleep 60");
    let _reaper = LaunchedRunReaper::new(root, phase, vec![agent.pid()]);
    plant_resumable_run(root, phase, Some(NEVER_LIVE_PID), Some(agent.pid()), true);
    let case = RefusalCase {
        role: "agent",
        live_pid: agent.pid(),
        required: vec![
            devflow_core::workflow::state_path(root, phase),
            devflow_core::agent_result::agent_pid_path(root, phase),
        ],
        never_named: vec![format!("monitor pid {NEVER_LIVE_PID}")],
        refusal: "resume must refuse a phase whose recorded agent is alive",
    };

    assert_resume_refuses_live_run(root, phase, &fake_bin, cache.path(), &case);
    assert_resume_refuses_live_run(root, phase, &fake_bin, cache.path(), &case);
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
        let cache = tempfile::tempdir().unwrap();
        let root = repo.path();
        init_repo(root, phase);
        let fake_bin = fake_bin_dir("exec sleep 60");
        let _reaper = LaunchedRunReaper::new(root, phase, vec![]);
        plant_resumable_run(
            root,
            phase,
            Some(NEVER_LIVE_PID),
            Some(NEVER_LIVE_PID),
            stopped,
        );

        let output = resume_command(root, phase, &fake_bin, cache.path())
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
        assert_ne!(state.monitor_pid, Some(NEVER_LIVE_PID));
        assert!(
            events_contain(root, "stage_launched"),
            "a resumed dead leftover must emit stage_launched"
        );
    }
}

#[test]
fn resume_without_phase_state_reports_the_missing_state_not_a_live_run() {
    let repo = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(84);
    init_repo(root, phase);
    let agent = LiveProcess::spawn();
    let fake_bin = fake_bin_dir("exec sleep 60");
    write_agent_pid(root, phase, agent.pid());
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "the carve-out fixture must have no phase state"
    );
    let before = phase_file_snapshot(root, phase);

    let output = resume_command(root, phase, &fake_bin, cache.path())
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(94);
    init_repo(root, phase);
    let monitor = LiveProcess::spawn();
    let fake_bin = fake_bin_dir("exit 0");
    plant_leftover_run(root, phase, Some(monitor.pid()), None);

    let output = run_start(root, phase, &fake_bin, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "start must still refuse a phase whose recorded monitor is alive\nstderr: {stderr}"
    );
    let expected = format!(
        "error: {}\n",
        START_REFUSAL_GOLDEN_PHASE_94.replace("{pid}", &monitor.pid().to_string())
    );
    assert!(
        stderr.contains(&expected),
        "the start refusal must stay byte-identical to 48-19's text\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains(&expected.replacen("refusing to start", "refusing to resume", 1)),
        "the golden comparison must be sensitive to the launch verb\nstderr: {stderr}"
    );
    assert!(
        START_REFUSAL_GOLDEN_PHASE_94.starts_with(
            "phase 94: a run is still live (monitor pid {pid} alive) — refusing to start; nothing was written"
        )
    );
    assert!(
        START_REFUSAL_GOLDEN_PHASE_94
            .ends_with("a later `start` would launch a second agent beside it.")
    );
}

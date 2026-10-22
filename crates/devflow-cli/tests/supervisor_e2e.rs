//! End-to-end proof that a stage launch runs under an addressed Rust
//! supervisor (50-11, criteria 2 and 3).
//!
//! The launch path is `commands::start` → `launch_stage` →
//! `monitor::spawn_monitor` → the hidden `__monitor` subcommand, all inside
//! the `devflow` BINARY crate (no `lib.rs`), so integration tests cannot call
//! any of it in-process. Like `stop_e2e.rs`, this drives the real compiled
//! binary through `common::hermetic_devflow` and observes what an operator
//! can: the persisted state, the socket it names, and the files the
//! supervisor writes.
//!
//! Every `devflow` child gets a tempdir `HOME` and `DEVFLOW_CACHE_DIR`, so the
//! sockets these launches bind live under that tempdir, never under the
//! operator's real `~/.cache/devflow`.

mod common;

use devflow_core::phase_id::PhaseId;
use devflow_core::supervisor::{self, Liveness, ShutdownOutcome};
use std::fs;
use std::path::Path;
use std::process::Output;
use std::time::{Duration, Instant};

/// How long a shutdown waits for the supervisor's final line.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(30);

/// Hermetic `git` for fixture setup (999.37), never a bare `Command::new("git")`.
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
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: supervisor fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// A fake `claude` that answers `--version` (the launch's health check), the
/// delivery canary's planted token (so a stream launch passes
/// `canary_gate`), and otherwise stays running until it is ended. Under the
/// capture shape its stdin is `/dev/null`, so the `read` yields nothing and
/// it goes straight to waiting.
const WAITING_CLAUDE: &str = r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    exit 0
    ;;
esac
sleep 120"#;

/// One launch shape as `devflow start` selects it.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// The default Claude launch: the stream shape.
    Stream,
    /// `--legacy-claude-launch`: the capture shape.
    Capture,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Shape::Stream => "stream",
            Shape::Capture => "capture",
        }
    }

    fn start_flags(self) -> &'static [&'static str] {
        match self {
            Shape::Stream => &[],
            Shape::Capture => &["--legacy-claude-launch"],
        }
    }
}

/// `devflow start` for `phase` in supervise mode, with `fake_bin` first on PATH.
fn run_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    shape: Shape,
) -> Output {
    run_start_with(home, root, phase, fake_bin, shape, &[])
}

/// [`run_start`] with `extra` start flags (for example `--until plan`).
fn run_start_with(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    shape: Shape,
    extra: &[&str],
) -> Output {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    common::hermetic_devflow(home)
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--no-worktree",
        ])
        .args(shape.start_flags())
        .args(extra)
        .arg(root)
        .current_dir(root)
        .env("PATH", path)
        .output()
        .expect("run devflow start")
}

/// Criterion 3 for one shape: after `devflow start` returns, the state names
/// a socket under the hermetic cache dir that answers ALIVE with the
/// shape's label, its sidecar names this root and phase, and a socket
/// shutdown stops the run and leaves the socket Gone.
fn start_publishes_for(shape: Shape, phase: PhaseId) {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    let fake_bin = common::fake_bin_dir(WAITING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    // Control: nothing names a supervisor before the launch, so an Alive
    // answer below can only come from this launch.
    assert!(
        devflow_core::workflow::load_state(&root, phase).is_err(),
        "{shape:?}: no state may exist before start"
    );

    let output = run_start(&home, &root, phase, &fake_bin, shape);
    assert!(
        output.status.success(),
        "{shape:?}: start failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let state = devflow_core::workflow::load_state(&root, phase).expect("start saved state");
    let record = state
        .supervisor
        .clone()
        .unwrap_or_else(|| panic!("{shape:?}: start must record state.supervisor: {state:?}"));
    let socket = record.socket_path.clone();
    let sock_dir = home.cache_dir().join("sock");
    assert!(
        socket.starts_with(&sock_dir),
        "{shape:?}: socket {} must be under the child's cache dir {}",
        socket.display(),
        sock_dir.display()
    );
    assert!(
        !socket.starts_with(&root),
        "{shape:?}: the socket must not live in the project tree"
    );
    assert!(
        socket.as_os_str().len() <= 107,
        "{shape:?}: socket path is {} bytes",
        socket.as_os_str().len()
    );
    let reply = match supervisor::probe(&socket).expect("probe the recorded socket") {
        Liveness::Alive(reply) => reply,
        other => panic!("{shape:?}: the recorded socket must answer ALIVE, got {other:?}"),
    };
    assert_eq!(reply.launch, shape.label(), "{shape:?}: launch label");
    assert_eq!(reply.launch_id, record.launch_id, "{shape:?}: launch id");
    assert_eq!(reply.agent_pgid, record.agent_pgid, "{shape:?}: agent pgid");

    let sidecar_path = socket.with_extension("json");
    let sidecar = supervisor::read_sidecar(&sidecar_path).expect("read the sidecar");
    assert_eq!(sidecar.project_root, root, "{shape:?}: sidecar root");
    assert_eq!(sidecar.phase, phase, "{shape:?}: sidecar phase");
    assert_eq!(sidecar.launch, shape.label(), "{shape:?}: sidecar launch");
    assert_eq!(
        sidecar.supervisor_pid, reply.supervisor_pid,
        "{shape:?}: the recorded socket answers for the supervisor its sidecar names"
    );

    match supervisor::request_shutdown(&socket, SHUTDOWN_WAIT).expect("shutdown the run") {
        ShutdownOutcome::Stopped { .. } => {}
        other => panic!("{shape:?}: shutdown must stop the run, got {other:?}"),
    }
    assert_eq!(
        supervisor::probe(&socket).expect("probe after shutdown"),
        Liveness::Gone,
        "{shape:?}: the socket is gone once the supervisor stopped"
    );
}

#[test]
fn start_publishes_a_live_supervisor_socket_for_each_launch_shape() {
    start_publishes_for(Shape::Stream, PhaseId::new(61));
    start_publishes_for(Shape::Capture, PhaseId::new(62));
}

/// How long the tests below wait for a supervisor to act on a signal, or a
/// stage chain to reach its stop point.
const RUN_WAIT: Duration = Duration::from_secs(60);

/// The stop reason `run_monitor` records for a run its supervisor stopped.
const SUPERVISOR_STOP_REASON: &str =
    "stopped through the supervisor (socket shutdown or SIGTERM/SIGINT)";

/// A fake `claude` that answers `--version` and the canary like
/// [`WAITING_CLAUDE`], commits one empty commit as its "work", prints the
/// success marker and exits 0 on its own.
const FINISHING_CLAUDE: &str = r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    exit 0
    ;;
esac
git commit -q --allow-empty -m "fake stage work"
printf 'work done\nDEVFLOW_RESULT: {"status":"success","commits":1}\n'
exit 0"#;

/// The `stage_launched` events recorded for `phase`.
fn stage_launched_count(root: &Path, phase: PhaseId) -> usize {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| {
            phase.matches_json(event.get("phase"))
                && event["event"].as_str() == Some("stage_launched")
        })
        .count()
}

/// The pids of the live members of process group `pgid`. Zombies do not
/// count: a CI job container's PID 1 never reaps an orphaned one
/// (`test_support::live_group_members`).
fn group_members(pgid: u32) -> Vec<u32> {
    devflow_core::test_support::live_group_members(pgid)
}

/// Poll `done` every 25 ms until it holds or `within` elapses.
fn holds_within(within: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + within;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// D-05 at the CLI level (999.134) for one shape: SIGTERM to the live
/// supervisor named in its sidecar ends the run. The agent group is gone,
/// `state.stopped` carries the supervisor stop reason, no further
/// `stage_launched` is recorded, and the socket is Gone.
fn sigterm_stops_for(shape: Shape, phase: PhaseId) {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    let fake_bin = common::fake_bin_dir(WAITING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = run_start(&home, &root, phase, &fake_bin, shape);
    assert!(
        output.status.success(),
        "{shape:?}: start failed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record = devflow_core::workflow::load_state(&root, phase)
        .expect("start saved state")
        .supervisor
        .unwrap_or_else(|| panic!("{shape:?}: start must record state.supervisor"));
    let socket = record.socket_path.clone();
    let sidecar = supervisor::read_sidecar(&socket.with_extension("json")).expect("sidecar");
    assert_eq!(
        stage_launched_count(&root, phase),
        1,
        "{shape:?}: one launch"
    );
    assert!(
        !group_members(record.agent_pgid).is_empty(),
        "{shape:?}: the agent group is running before the signal"
    );

    // The pid to signal comes from the sidecar and is confirmed against its
    // recorded start time first, so a recycled pid is never signalled.
    let pid = sidecar.supervisor_pid;
    assert!(
        devflow_core::agent::is_same_process(pid, sidecar.supervisor_start_time),
        "{shape:?}: the sidecar's supervisor {pid} is the recorded process"
    );
    let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
    assert!(signed > 1, "refusing to signal pid {signed}");
    // SAFETY: `signed > 1` names the supervisor just identity-checked.
    assert_eq!(
        unsafe { libc::kill(signed, libc::SIGTERM) },
        0,
        "SIGTERM {pid}"
    );

    assert!(
        holds_within(RUN_WAIT, || !devflow_core::agent::agent_running(pid)),
        "{shape:?}: the supervisor {pid} exits after SIGTERM"
    );
    assert_eq!(
        supervisor::probe(&socket).expect("probe after the signal"),
        Liveness::Gone,
        "{shape:?}: the socket is gone"
    );
    assert_eq!(
        group_members(record.agent_pgid),
        Vec::<u32>::new(),
        "{shape:?}: the agent group {} is gone",
        record.agent_pgid
    );
    let state = devflow_core::workflow::load_state(&root, phase).expect("reload state");
    assert!(state.stopped, "{shape:?}: state.stopped after the signal");
    assert_eq!(
        state.stop_reason.as_deref(),
        Some(SUPERVISOR_STOP_REASON),
        "{shape:?}: the supervisor stop reason"
    );
    assert_eq!(
        stage_launched_count(&root, phase),
        1,
        "{shape:?}: no stage_launched after the signal"
    );
}

/// The control for [`sigterm_stops_for`]: the same launch, whose fake agent
/// exits 0 on its own, advances in-process and launches the next stage, so a
/// following `stage_launched` names Plan (the launched stage was passed to the
/// advance). `--until plan` stops the chain at the Plan boundary.
fn finishing_run_advances_for(shape: Shape, phase: PhaseId) {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    let fake_bin = common::fake_bin_dir(FINISHING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = run_start_with(&home, &root, phase, &fake_bin, shape, &["--until", "plan"]);
    assert!(
        output.status.success(),
        "{shape:?}: start failed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        holds_within(RUN_WAIT, || stage_launched_count(&root, phase) >= 2),
        "{shape:?} control: a completed agent must advance and launch the next stage\n{}",
        fs::read_to_string(devflow_core::events::events_path(&root)).unwrap_or_default()
    );
    let events = fs::read_to_string(devflow_core::events::events_path(&root)).unwrap();
    let launched: Vec<serde_json::Value> = events
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"].as_str() == Some("stage_launched"))
        .collect();
    assert_eq!(launched[0]["stage"], "define", "{shape:?}: {launched:?}");
    assert_eq!(launched[1]["stage"], "plan", "{shape:?}: {launched:?}");
    assert_eq!(
        launched[1]["launch"],
        shape.label(),
        "{shape:?}: {launched:?}"
    );
    assert!(
        holds_within(RUN_WAIT, || devflow_core::workflow::load_state(
            &root, phase
        )
        .is_ok_and(|state| state.stopped)),
        "{shape:?} control: the chain stops at --until plan"
    );
}

#[test]
fn sigterm_to_a_live_supervisor_ends_the_run_without_advancing() {
    sigterm_stops_for(Shape::Stream, PhaseId::new(63));
    sigterm_stops_for(Shape::Capture, PhaseId::new(64));
    finishing_run_advances_for(Shape::Stream, PhaseId::new(65));
    finishing_run_advances_for(Shape::Capture, PhaseId::new(66));
}

/// Re-homed from `devflow-core/tests/monitor_e2e.rs` (50-11): a library test
/// binary cannot re-exec `__monitor`, so the capture-result check runs on the
/// real binary. A capture-shape run whose fake agent prints a
/// `DEVFLOW_RESULT` marker and exits 0: Layers 1–2 evaluate to Success from
/// the files the supervisor wrote, and the supervisor's own advance recorded
/// the same verdict.
#[test]
fn capture_supervisor_owns_a_fake_agent_and_records_devflow_result() {
    let phase = PhaseId::new(67);
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    let fake_bin = common::fake_bin_dir(FINISHING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = run_start_with(
        &home,
        &root,
        phase,
        &fake_bin,
        Shape::Capture,
        &["--until", "define"],
    );
    assert!(
        output.status.success(),
        "start failed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        holds_within(RUN_WAIT, || devflow_core::workflow::load_state(
            &root, phase
        )
        .is_ok_and(|state| state.stopped)),
        "the run stops at --until define once its supervisor advanced"
    );

    let agent_pid: u32 =
        fs::read_to_string(devflow_core::agent_result::agent_pid_path(&root, phase))
            .expect("the supervisor records the agent pid")
            .trim()
            .parse()
            .expect("a pid");
    assert!(agent_pid > 0);
    let stdout = fs::read_to_string(devflow_core::agent_result::stdout_path(&root, phase)).unwrap();
    assert!(
        stdout.contains("DEVFLOW_RESULT"),
        "captured stdout missing marker: {stdout:?}"
    );
    let exit = fs::read_to_string(devflow_core::agent_result::exit_code_path(&root, phase))
        .expect("the supervisor records the exit code");
    assert_eq!(exit.trim(), "0");

    let state = devflow_core::workflow::load_state(&root, phase).unwrap();
    let git_flow = devflow_core::config::git_flow_for_run(&root, state.base_branch.as_deref());
    let result = devflow_core::agent_result::evaluate_agent_result(&root, &state, &git_flow)
        .expect("evaluate the supervisor's files");
    assert_eq!(
        result.status,
        devflow_core::agent_result::AgentStatus::Success,
        "{result:?}"
    );
    let events = fs::read_to_string(devflow_core::events::events_path(&root)).unwrap();
    assert!(
        events
            .lines()
            .any(|line| line.contains("\"advance_evaluated\"")
                && line.contains("\"status\":\"success\"")),
        "the supervisor's in-process advance evaluated the stage as success:\n{events}"
    );
}

/// `devflow status` for `root`, as an operator runs it; its stdout.
fn run_status(home: &common::HermeticHome, root: &Path) -> String {
    let output = common::hermetic_devflow(home)
        .arg("status")
        .arg(root)
        .current_dir(root)
        .output()
        .expect("run devflow status");
    assert!(
        output.status.success(),
        "status failed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// 50-12 criterion 1: `devflow status` answers ALIVE, STALE and GONE from the
/// recorded socket, and each non-ALIVE answer names the verb that repairs it.
/// The run is made STALE by a SIGKILL of its supervisor (the pid comes from
/// the sidecar and is identity-checked first), and GONE by removing the stale
/// socket file. The reaper ends the agent group the SIGKILL orphaned.
#[test]
fn status_reports_each_liveness_state_with_its_repair_verb() {
    let phase = PhaseId::new(68);
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    let fake_bin = common::fake_bin_dir(WAITING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = run_start(&home, &root, phase, &fake_bin, Shape::Stream);
    assert!(
        output.status.success(),
        "start failed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record = devflow_core::workflow::load_state(&root, phase)
        .expect("start saved state")
        .supervisor
        .expect("start must record state.supervisor");
    let socket = record.socket_path.clone();
    let stop_verb = format!("devflow stop --phase {phase}");
    let resume_verb = format!("devflow resume --phase {phase}");

    let alive = run_status(&home, &root);
    assert!(
        alive.contains("ALIVE") && alive.contains("owner: agent_running"),
        "a live run reads ALIVE with its owner:\n{alive}"
    );
    assert!(
        alive.contains(&format!("supervisor: {}", socket.display())),
        "status names the recorded socket:\n{alive}"
    );
    assert!(
        !alive.contains(&stop_verb) && !alive.contains(&resume_verb),
        "an ALIVE run needs no repair:\n{alive}"
    );

    let sidecar = supervisor::read_sidecar(&socket.with_extension("json")).expect("sidecar");
    let pid = sidecar.supervisor_pid;
    census_never_sees_a_live_supervisor(pid);

    assert!(
        devflow_core::agent::is_same_process(pid, sidecar.supervisor_start_time),
        "the sidecar's supervisor {pid} is the recorded process"
    );
    let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
    assert!(signed > 1, "refusing to signal pid {signed}");
    // SAFETY: `signed > 1` names the supervisor just identity-checked.
    assert_eq!(
        unsafe { libc::kill(signed, libc::SIGKILL) },
        0,
        "SIGKILL {pid}"
    );
    assert!(
        holds_within(RUN_WAIT, || !devflow_core::agent::agent_running(pid)),
        "the supervisor {pid} is gone after SIGKILL"
    );
    assert_eq!(
        supervisor::probe(&socket).expect("probe the orphaned socket"),
        Liveness::Stale,
        "a SIGKILLed supervisor leaves its socket STALE"
    );

    let stale = run_status(&home, &root);
    assert!(stale.contains("STALE"), "the run reads STALE:\n{stale}");
    let stop_at = stale
        .find(&stop_verb)
        .unwrap_or_else(|| panic!("STALE names `{stop_verb}`:\n{stale}"));
    let resume_at = stale
        .find(&resume_verb)
        .unwrap_or_else(|| panic!("STALE names `{resume_verb}` after the stop:\n{stale}"));
    assert!(stop_at < resume_at, "stop comes before resume:\n{stale}");
    assert!(
        !stale.contains("ALIVE"),
        "a STALE run is not ALIVE:\n{stale}"
    );

    fs::remove_file(&socket).expect("remove the stale socket");
    let gone = run_status(&home, &root);
    assert!(gone.contains("GONE"), "the run reads GONE:\n{gone}");
    assert!(
        gone.contains(&resume_verb),
        "GONE while active names `{resume_verb}`:\n{gone}"
    );
    assert!(
        !gone.contains(&stop_verb),
        "GONE names no stop, there is nothing left to stop:\n{gone}"
    );
}

/// A 2.x-shaped monitor wrapper (`sh -c` with the trap line the 2.x script
/// carried), in its own process group, which drop SIGKILLs and reaps.
struct PlantedWrapper(std::process::Child);

impl PlantedWrapper {
    fn spawn() -> Self {
        use std::os::unix::process::CommandExt as _;
        // The trailing `:` keeps `sh` from exec-ing `sleep` in its place, so
        // the wrapper's own argv stays visible in `/proc`.
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg("cleanup() { :; }\ntrap cleanup TERM INT\nsleep 30\n:")
            .process_group(0)
            .spawn()
            .expect("spawn the planted 2.x wrapper");
        Self(child)
    }
}

impl Drop for PlantedWrapper {
    fn drop(&mut self) {
        common::signal_group(self.0.id(), libc::SIGKILL);
        let _ = self.0.wait();
    }
}

/// 50-12 (T-50-52): the `/proc` stray census, run against a real 3.x
/// supervisor, never lists it. The control is a planted 2.x-shaped wrapper
/// that the same census run must list, so an empty census cannot pass.
fn census_never_sees_a_live_supervisor(supervisor_pid: u32) {
    let wrapper = PlantedWrapper::spawn();
    let wrapper_pid = wrapper.0.id();
    let mut census = Vec::new();
    assert!(
        holds_within(RUN_WAIT, || {
            census = devflow_core::agent::discover_stray_devflow_processes();
            census.iter().any(|stray| stray.pid == wrapper_pid)
        }),
        "control: the census must list the planted 2.x wrapper {wrapper_pid}"
    );
    assert!(
        devflow_core::agent::agent_running(supervisor_pid),
        "the supervisor {supervisor_pid} is alive while the census runs"
    );
    assert!(
        census.iter().all(|stray| stray.pid != supervisor_pid),
        "the 2.x census must never list the 3.x supervisor {supervisor_pid}: {census:?}"
    );
    drop(wrapper);
}

// ---------------------------------------------------------------------------
// 50-15 (R-1, V-12): two `devflow start` launches racing for one phase yield
// exactly one live supervisor and one agent group.
// ---------------------------------------------------------------------------

/// Bounds any gate a launched run parks at, as the 48-19 refusal tests do.
const RACE_GATE_TIMEOUT_SECS: &str = "15";

/// A spawned `devflow start` for `phase` with the arguments the 48-19
/// live-run refusal tests use (supervise mode, `--no-worktree`, the capture
/// shape), so a loser fails only at the per-phase lock or at the live-run
/// guard, never at an earlier refusal.
fn spawn_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
) -> std::process::Child {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    common::hermetic_devflow(home)
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--no-worktree",
            "--legacy-claude-launch",
        ])
        .arg(root)
        .current_dir(root)
        .env("PATH", path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", RACE_GATE_TIMEOUT_SECS)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn devflow start")
}

/// What a sweep of `home`'s socket directory sees: the ALIVE sockets, and the
/// distinct agent pgids its parsed sidecars name.
fn live_supervisors(home: &common::HermeticHome) -> (Vec<std::path::PathBuf>, Vec<u32>) {
    let entries = supervisor::sweep_in(&home.cache_dir());
    let alive = entries
        .iter()
        .filter(|entry| matches!(entry.liveness, Ok(Liveness::Alive(_))))
        .map(|entry| entry.socket_path.clone())
        .collect();
    let mut pgids: Vec<u32> = entries
        .iter()
        .filter_map(|entry| entry.sidecar.as_ref().map(|sidecar| sidecar.agent_pgid))
        .collect();
    pgids.sort_unstable();
    pgids.dedup();
    (alive, pgids)
}

#[test]
fn two_concurrent_starts_yield_exactly_one_live_supervisor() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(63);
    init_repo(&root, phase);
    let fake_bin = common::fake_bin_dir(WAITING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let first = spawn_start(&home, &root, phase, &fake_bin);
    let second = spawn_start(&home, &root, phase, &fake_bin);
    let outputs = [
        first.wait_with_output().expect("wait for the first start"),
        second
            .wait_with_output()
            .expect("wait for the second start"),
    ];

    let report = |output: &Output| {
        format!(
            "status {:?}\nstdout: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    };
    let winners = outputs
        .iter()
        .filter(|output| output.status.success())
        .count();
    assert_eq!(
        winners,
        1,
        "exactly one racing start must succeed\nfirst: {}\nsecond: {}",
        report(&outputs[0]),
        report(&outputs[1])
    );
    let loser = outputs
        .iter()
        .find(|output| !output.status.success())
        .expect("one start lost");
    let stderr = String::from_utf8_lossy(&loser.stderr);
    let lock_refusal = stderr.contains("holds the per-phase lock");
    let live_refusal = stderr.contains(&format!("devflow stop --phase {phase}"))
        && stderr.contains("a run is still live");
    assert!(
        lock_refusal || live_refusal,
        "the loser must be refused at the lock or at the live-run guard, nothing else\n{}",
        report(loser)
    );
    println!(
        "V-12 loser refusal: {}",
        if lock_refusal {
            "per-phase lock"
        } else {
            "live-supervisor guard"
        }
    );

    let (alive, pgids) = live_supervisors(&home);
    assert_eq!(
        alive.len(),
        1,
        "exactly one socket may probe ALIVE: {alive:?}\nloser: {}",
        report(loser)
    );
    assert_eq!(
        pgids.len(),
        1,
        "the sidecars must name exactly one agent group: {pgids:?}"
    );
    assert!(
        !group_members(pgids[0]).is_empty(),
        "the one agent group {} must be live",
        pgids[0]
    );
}

#[test]
fn two_starts_for_different_phases_yield_two_live_supervisors() {
    let home = common::HermeticHome::new();
    let repos = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
    let roots: Vec<std::path::PathBuf> = repos
        .iter()
        .map(|repo| repo.path().canonicalize().unwrap())
        .collect();
    let phases = [PhaseId::new(64), PhaseId::new(65)];
    for (root, phase) in roots.iter().zip(phases) {
        init_repo(root, phase);
    }
    let fake_bin = common::fake_bin_dir(WAITING_CLAUDE);
    let _reapers: Vec<common::LaunchedRunReaper> = roots
        .iter()
        .zip(phases)
        .map(|(root, phase)| common::LaunchedRunReaper::new(root, phase, vec![]))
        .collect();

    let first = spawn_start(&home, &roots[0], phases[0], &fake_bin);
    let second = spawn_start(&home, &roots[1], phases[1], &fake_bin);
    for (child, phase) in [first, second].into_iter().zip(phases) {
        let output = child.wait_with_output().expect("wait for start");
        assert!(
            output.status.success(),
            "phase {phase}: a start with no rival for its phase must succeed\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let (alive, pgids) = live_supervisors(&home);
    assert_eq!(
        alive.len(),
        2,
        "the counting must see both live supervisors: {alive:?}"
    );
    assert_eq!(
        pgids.len(),
        2,
        "the sidecars must name two distinct agent groups: {pgids:?}"
    );
}

// ---- 50-26: WR-03 on the real `devflow __monitor` binary -----------------
//
// A completed stream run whose agent left a process holding its stdout must
// reach its advance, and a `devflow stop` sent while it is `advancing` must
// end it at safe point 1. Each run is a `__monitor` started directly (no
// launcher), over a seeded Code-stage state that halts at `--until code`.

/// A repository on `develop` with `feature/phase-NN` one commit ahead of it.
/// Named apart from [`init_repo`] (develop only, with a ROADMAP entry, used by
/// the `start` tests): the commit ahead is what lets Layer 2 classify the
/// Code run `Success`, so the advance reaches `transition`'s `--until` halt
/// (as `auto_chain_flag_e2e.rs`'s `init_repo` makes it).
fn init_repo_with_feature_branch(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);

    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

/// Mirrors `pipeline_launch::ADVANCE_STOP_REASON`, as
/// [`SUPERVISOR_STOP_REASON`] mirrors the supervisor's; `devflow stop`
/// appends its own clause after it.
const ADVANCE_STOP_REASON: &str = "stopped through the supervisor during advance";

/// The Claude top-level result line the held-stdout agent prints.
const HELD_RESULT_LINE: &str = r#"{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#;

/// The holder record `<pid> <start>` at `record`, when it exists and parses.
fn read_holder_record(record: &Path) -> Option<(u32, u64)> {
    let text = fs::read_to_string(record).ok()?;
    let mut fields = text.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let start = fields.next()?.parse().ok()?;
    fields.next().is_none().then_some((pid, start))
}

/// A `devflow __monitor` started directly over a seeded Code-stage state
/// (`Mode::Supervise`, `stop_until = Some(Stage::Code)`), with production's
/// 120 s idle timeout, supervising `sh <script>`: the script reads one line,
/// prints [`HELD_RESULT_LINE`], starts its holder as a background job,
/// records it (`<pid> <start>` while the holder is its unreaped child, as the
/// core tests' fixture rule says) and exits 0.
///
/// Dropped, it SIGKILLs and reaps the monitor through its own `Child` if it
/// still runs, then SIGKILLs the holder by its recorded pid only while
/// [`devflow_core::agent::is_same_process`] holds. It signals nothing else.
struct HeldStdoutRun {
    monitor: Option<std::process::Child>,
    root: std::path::PathBuf,
    phase: PhaseId,
    socket: supervisor::SocketPath,
    record: std::path::PathBuf,
    hold: std::path::PathBuf,
    log: std::path::PathBuf,
    aux_path: std::path::PathBuf,
    _reaper: common::LaunchedRunReaper,
    _repo: tempfile::TempDir,
    _aux: tempfile::TempDir,
    /// Every `devflow` child's `HOME` and cache. Declared last so it drops
    /// after the rest of the fixture.
    home: common::HermeticHome,
}

impl HeldStdoutRun {
    /// Seed the repository and state for `phase`, then spawn the monitor
    /// with `holder` as the agent's leftover. `hold` sets
    /// `DEVFLOW_TEST_STREAM_WAIT_HOLD` on the monitor.
    fn spawn(phase: PhaseId, holder: &str, hold: bool) -> Self {
        let repo = tempfile::tempdir().unwrap();
        let aux = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too.
        let root = repo.path().canonicalize().unwrap();
        init_repo_with_feature_branch(&root, phase);
        fs::create_dir_all(root.join(".planning")).unwrap();
        fs::write(
            root.join(".planning/config.json"),
            r#"{"workflow":{"_auto_chain_active":false}}"#,
        )
        .unwrap();
        let mut state = devflow_core::state::State::new(
            phase,
            devflow_core::state::AgentKind::Claude,
            devflow_core::mode::Mode::Supervise,
            root.clone(),
        );
        state.stage = devflow_core::stage::Stage::Code;
        state.stop_until = Some(devflow_core::stage::Stage::Code);
        state.stopped = false;
        devflow_core::workflow::save_state(&state).unwrap();

        let aux_path = aux.path().to_path_buf();
        let record = aux_path.join("holder");
        let hold_path = aux_path.join("hold");
        let log = aux_path.join("monitor.stderr");
        let prompt = aux_path.join("prompt.txt");
        fs::write(&prompt, "run the code stage\n").unwrap();
        let script = aux_path.join("agent.sh");
        fs::write(
            &script,
            format!(
                "PATH=/usr/bin:/bin\n\
                 IFS= read -r _turn || exit 91\n\
                 printf '%s\\n' '{HELD_RESULT_LINE}'\n\
                 {holder} &\n\
                 holder=$!\n\
                 echo \"$holder $(cut -d' ' -f22 /proc/$holder/stat)\" > '{record}.tmp'\n\
                 mv '{record}.tmp' '{record}'\n\
                 exit 0\n",
                record = record.display()
            ),
        )
        .unwrap();

        let home = common::HermeticHome::new();
        let launch_id = supervisor::new_launch_id();
        let socket =
            supervisor::SocketPath::for_launch(&home.cache_dir(), &root, phase, &launch_id)
                .expect("the hermetic socket path fits sun_path");
        let mut command = common::hermetic_devflow(&home);
        command
            .arg("__monitor")
            .arg("--project")
            .arg(&root)
            .arg("--phase")
            .arg(phase.to_string())
            .arg("--workdir")
            .arg(&root)
            .arg("--prompt-file")
            .arg(&prompt)
            .arg("--idle-timeout-secs")
            .arg("120")
            .arg("--agent")
            .arg("claude")
            .arg("--launch")
            .arg("stream")
            .arg("--launch-id")
            .arg(&launch_id)
            .arg("--socket")
            .arg(socket.as_path())
            .arg("--")
            .arg("sh")
            .arg(&script)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(fs::File::create(&log).expect("create the monitor's stderr log"));
        if hold {
            command.env("DEVFLOW_TEST_STREAM_WAIT_HOLD", &hold_path);
        }
        let monitor = command.spawn().expect("spawn devflow __monitor");
        let reaper = common::LaunchedRunReaper::new(&root, phase, Vec::new());
        Self {
            monitor: Some(monitor),
            root,
            phase,
            socket,
            record,
            hold: hold_path,
            log,
            aux_path,
            _reaper: reaper,
            _repo: repo,
            _aux: aux,
            home,
        }
    }

    /// The holder the agent recorded, waited for up to 10 s and required to
    /// name a live process.
    fn holder(&self) -> (u32, u64) {
        if !holds_within(Duration::from_secs(10), || {
            read_holder_record(&self.record).is_some()
        }) {
            panic!(
                "FIXTURE: no holder record at {} within 10 s; monitor stderr: {}",
                self.record.display(),
                self.stderr_log()
            );
        }
        let (pid, start) = read_holder_record(&self.record).expect("the record parses");
        if pid <= 1 || devflow_core::agent::process_start_time(pid) != Some(start) {
            panic!("FIXTURE: the holder record ({pid} {start}) names no live process");
        }
        (pid, start)
    }

    /// The monitor's exit status, polled for up to `within`.
    fn wait_exit(&mut self, within: Duration) -> Option<std::process::ExitStatus> {
        let monitor = self.monitor.as_mut().expect("the monitor is still owned");
        let mut status = None;
        holds_within(within, || {
            status = monitor.try_wait().expect("poll the monitor");
            status.is_some()
        });
        status
    }

    /// The owner a ping reports, or what the probe saw instead.
    fn owner(&self) -> String {
        match supervisor::probe(self.socket.as_path()) {
            Ok(Liveness::Alive(reply)) => reply.owner,
            other => format!("{other:?}"),
        }
    }

    fn stderr_log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn state(&self) -> devflow_core::state::State {
        devflow_core::workflow::load_state(&self.root, self.phase).expect("load the phase's state")
    }

    fn exit_file(&self) -> String {
        fs::read_to_string(devflow_core::agent_result::exit_code_path(
            &self.root, self.phase,
        ))
        .expect("read the exit file")
        .trim()
        .to_string()
    }

    /// Every `name` event for this phase, each as its JSON object. Fields are
    /// indexed on the map, so a missing one panics instead of reading as a
    /// default.
    fn events(&self, name: &str) -> Vec<serde_json::Map<String, serde_json::Value>> {
        let phase = serde_json::json!(self.phase);
        fs::read_to_string(devflow_core::events::events_path(&self.root))
            .unwrap_or_default()
            .lines()
            .map(
                |line| match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(serde_json::Value::Object(map)) => map,
                    other => panic!("an event line is not a JSON object: {line:?} ({other:?})"),
                },
            )
            .filter(|map| map["event"] == name && map["phase"] == phase)
            .collect()
    }

    /// Exactly one `stream_wait_bounded` event, naming only the reader, with
    /// the capture closed.
    fn assert_one_reader_bound(&self) {
        let bounded = self.events("stream_wait_bounded");
        assert!(
            bounded.len() == 1,
            "WR-03: expected exactly one stream_wait_bounded event, found {bounded:?}"
        );
        assert_eq!(bounded[0]["unfinished"], serde_json::json!(["reader"]));
        assert_eq!(bounded[0]["capture_closed"], serde_json::json!(true));
    }

    /// E1 and E2 after the monitor exited 0: the run reached its advance and
    /// halted at `--until code`.
    fn assert_reached_advance(&self, holder: (u32, u64), stdout_held: bool) {
        assert!(
            devflow_core::agent::is_same_process(holder.0, holder.1),
            "the holder still runs: the supervisor's wait ended, not the holder"
        );
        assert_eq!(self.exit_file(), "0");
        let state = self.state();
        assert!(state.stopped, "the run halted at --until code: {state:?}");
        assert_eq!(
            state.stop_reason,
            Some(format!(
                "stopped after {} completed (--until {})",
                devflow_core::stage::Stage::Code,
                devflow_core::stage::Stage::Code
            ))
        );
        let finished: Vec<_> = self
            .events("workflow_finished")
            .into_iter()
            .filter(|event| event["reason"] == "stopped_at")
            .collect();
        assert_eq!(finished.len(), 1, "one stopped_at halt: {finished:?}");
        if stdout_held {
            self.assert_one_reader_bound();
        } else {
            let bounded = self.events("stream_wait_bounded");
            assert!(
                bounded.is_empty(),
                "a released stdout is not a bounded wait: {bounded:?}"
            );
        }
        assert!(matches!(
            supervisor::probe(self.socket.as_path()),
            Ok(Liveness::Gone)
        ));
    }
}

impl Drop for HeldStdoutRun {
    fn drop(&mut self) {
        if let Some(mut monitor) = self.monitor.take()
            && matches!(monitor.try_wait(), Ok(None))
        {
            let _ = monitor.kill();
            let _ = monitor.wait();
        }
        if let Some((pid, start)) = read_holder_record(&self.record)
            && let Ok(signed) = libc::pid_t::try_from(pid)
            && signed > 1
            && devflow_core::agent::is_same_process(pid, start)
        {
            // SAFETY: `signed > 1` is the holder this run's agent recorded,
            // confirmed by its start time to still be that process.
            unsafe { libc::kill(signed, libc::SIGKILL) };
        }
    }
}

/// E1 (fail-first): a completed stream run whose agent left `sleep 300`
/// holding its stdout reaches its advance on the real binary, under the
/// production idle timeout.
#[test]
fn a_completed_stream_run_reaches_its_advance_while_a_leftover_process_holds_stdout() {
    let mut run = HeldStdoutRun::spawn(PhaseId::new(72), "sleep 300", false);
    let holder = run.holder();
    let Some(status) = run.wait_exit(Duration::from_secs(30)) else {
        panic!(
            "WR-03: the __monitor still runs 30 s after its agent exited while holder {} holds \
             its stdout; owner: {}; stderr: {}",
            holder.0,
            run.owner(),
            run.stderr_log()
        );
    };
    assert!(
        status.success(),
        "monitor exited {status:?}; stderr: {}",
        run.stderr_log()
    );
    run.assert_reached_advance(holder, true);
}

/// E2 (control): with the holder on `/dev/null`, the hold before the stream
/// thread wait is reached on the unfixed and the fixed code alike, so this
/// proves the binary under test honours the test-support hold E3 relies on
/// (agy-4), and the run then reaches its advance with no bounded wait.
#[test]
fn a_completed_stream_run_reaches_its_advance_once_its_leftover_process_released_stdout() {
    let mut run = HeldStdoutRun::spawn(
        PhaseId::new(73),
        "sleep 300 </dev/null >/dev/null 2>&1",
        true,
    );
    let holder = run.holder();
    let held = run.aux_path.join("hold.held");
    if !holds_within(Duration::from_secs(30), || held.exists()) {
        panic!(
            "HOLD-UNREACHABLE: the monitor never reached the stream-wait hold within 30 s, so \
             this binary lacks the test-support hold; owner: {}; stderr: {}",
            run.owner(),
            run.stderr_log()
        );
    }
    fs::write(&run.hold, "").expect("release the hold");
    let Some(status) = run.wait_exit(Duration::from_secs(30)) else {
        panic!(
            "WR-03: the released __monitor still runs 30 s later; owner: {}; stderr: {}",
            run.owner(),
            run.stderr_log()
        );
    };
    assert!(
        status.success(),
        "monitor exited {status:?}; stderr: {}",
        run.stderr_log()
    );
    run.assert_reached_advance(holder, false);
}

/// E3 (fail-first): `devflow stop` sent while a completed run is `advancing`
/// with its agent's stdout held ends the run at advance safe point 1: no
/// `--until` halt, no next stage, the socket removed, the supervisor gone.
/// The hold parks the supervisor in `advancing` and releases on the stop
/// request, so the stop lands there deterministically. The exit file keeps
/// the agent's own `0` (backlog 999.166 item 6), asserted so the record says
/// what happened.
#[test]
fn a_stop_sent_while_a_completed_run_waits_on_a_held_stdout_ends_it_at_safe_point_1() {
    let phase = PhaseId::new(74);
    let mut run = HeldStdoutRun::spawn(phase, "sleep 300", true);
    let holder = run.holder();
    let held = run.aux_path.join("hold.held");
    if !holds_within(Duration::from_secs(30), || held.exists()) {
        panic!(
            "WR-03: the supervisor did not reach its stream-thread wait within 30 s of the \
             agent's exit; owner: {}; stderr: {}",
            run.owner(),
            run.stderr_log()
        );
    }
    let owner = run.owner();
    if owner != "advancing" {
        panic!("FIXTURE: the held supervisor reports owner {owner}, not advancing");
    }

    let stop_out = run.aux_path.join("stop.stdout");
    let stop_err = run.aux_path.join("stop.stderr");
    let mut stop = common::hermetic_devflow(&run.home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(&run.root)
        .stdin(std::process::Stdio::null())
        .stdout(fs::File::create(&stop_out).unwrap())
        .stderr(fs::File::create(&stop_err).unwrap())
        .spawn()
        .expect("spawn devflow stop");
    let mut stop_status = None;
    holds_within(Duration::from_secs(40), || {
        stop_status = stop.try_wait().expect("poll devflow stop");
        stop_status.is_some()
    });
    if stop_status.is_none() {
        common::kill_and_reap(&mut stop);
    }
    let stop_stdout = fs::read_to_string(&stop_out).unwrap_or_default();
    let stop_stderr = fs::read_to_string(&stop_err).unwrap_or_default();
    if !stop_status.is_some_and(|status| status.success()) {
        panic!(
            "WR-03: `devflow stop` sent in advancing did not exit 0 within 40 s ({stop_status:?})\n\
             stdout: {stop_stdout}\nstderr: {stop_stderr}\nmonitor stderr: {}",
            run.stderr_log()
        );
    }
    assert!(
        stop_stdout.contains(&format!(
            "stop: stopped phase {phase} through its supervisor"
        )),
        "stdout: {stop_stdout}\nstderr: {stop_stderr}"
    );

    let Some(status) = run.wait_exit(Duration::from_secs(20)) else {
        panic!(
            "WR-03: the stopped __monitor still runs 20 s after `devflow stop`; stderr: {}",
            run.stderr_log()
        );
    };
    assert!(
        devflow_core::agent::is_same_process(holder.0, holder.1),
        "the holder still runs at the monitor's exit (status {status:?})"
    );
    let state = run.state();
    assert!(state.stopped, "{state:?}");
    let reason = state.stop_reason.clone().unwrap_or_default();
    assert!(
        reason.starts_with(ADVANCE_STOP_REASON),
        "the stop was honoured at an advance safe point: {reason:?}"
    );
    let advance_stopped = run.events("advance_stopped");
    assert_eq!(advance_stopped.len(), 1, "{advance_stopped:?}");
    assert_eq!(advance_stopped[0]["safe_point"], serde_json::json!(1));
    let halted: Vec<_> = run
        .events("workflow_finished")
        .into_iter()
        .filter(|event| event["reason"] == "stopped_at")
        .collect();
    assert!(
        halted.is_empty(),
        "a stopped advance never reaches the --until halt: {halted:?}"
    );
    run.assert_one_reader_bound();
    assert!(matches!(
        supervisor::probe(run.socket.as_path()),
        Ok(Liveness::Gone)
    ));
    assert_eq!(
        run.exit_file(),
        "0",
        "the exit file keeps the agent's own code (999.166 item 6)"
    );
}

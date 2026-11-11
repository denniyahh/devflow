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

use devflow_core::gsd_reader::GsdStatus;
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

/// A repository on `develop` with a roadmap entry for `phase` and its
/// CONTEXT.md, which `start` requires before it launches codex.
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
    let context = common::write_phase_context(root, phase);
    git(
        root,
        &["add", "README.md", ".planning/ROADMAP.md", &context],
    );
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
    /// A capture-shape agent: `codex`, which gets its prompt as argv.
    Capture,
}

/// What a shape's fake agent does once launched.
#[derive(Debug, Clone, Copy)]
enum AgentRun {
    /// Stays running until it is ended.
    Waiting,
    /// Commits one empty commit, reports success and exits 0.
    Finishing,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Shape::Stream => "stream",
            Shape::Capture => "capture",
        }
    }

    /// The `--agent` value that selects this shape.
    fn agent(self) -> &'static str {
        match self {
            Shape::Stream => "claude",
            Shape::Capture => "codex",
        }
    }

    /// This shape's fake agent doing `run`. A capture agent logs its argv and
    /// stdin under `logs`.
    fn fake_bin(self, logs: &Path, run: AgentRun) -> common::FakeBin {
        match (self, run) {
            (Shape::Stream, AgentRun::Waiting) => common::fake_bin_dir(WAITING_CLAUDE),
            (Shape::Stream, AgentRun::Finishing) => common::fake_bin_dir(FINISHING_CLAUDE),
            (Shape::Capture, AgentRun::Waiting) => common::waiting_capture_agent_bin(logs),
            (Shape::Capture, AgentRun::Finishing) => common::capture_agent_bin(logs),
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
            shape.agent(),
            "--mode",
            "supervise",
            "--no-worktree",
        ])
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
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = shape.fake_bin(logs.path(), AgentRun::Waiting);
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
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = shape.fake_bin(logs.path(), AgentRun::Waiting);
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
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = shape.fake_bin(logs.path(), AgentRun::Finishing);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = run_start_with(&home, &root, phase, &fake_bin, shape, &["--until", "code"]);
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
    assert_eq!(launched[0]["stage"], "plan", "{shape:?}: {launched:?}");
    assert_eq!(launched[1]["stage"], "code", "{shape:?}: {launched:?}");
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
        "{shape:?} control: the chain stops at --until code"
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
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = Shape::Capture.fake_bin(logs.path(), AgentRun::Finishing);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = run_start_with(
        &home,
        &root,
        phase,
        &fake_bin,
        Shape::Capture,
        &["--until", "plan"],
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
        "the run stops at --until plan once its supervisor advanced"
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
    // The assertions above hold for a stream launch too: a stream-shape
    // supervisor also writes the stdout and exit-code files. This one is the
    // shape check.
    let launches: Vec<serde_json::Value> = events
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"].as_str() == Some("stage_launched"))
        .map(|event| event["launch"].clone())
        .collect();
    assert_eq!(
        launches,
        vec![serde_json::Value::from(Shape::Capture.label())],
        "the one launch ran in the capture shape:\n{events}"
    );

    // The fake codex ran with the codex driver's argv, and the capture shape
    // gave it no stdin: the prompt travels as argv.
    let argv = fs::read_to_string(logs.path().join(common::AGENT_ARGV_LOG))
        .expect("the fake codex logs its argv");
    let argv: Vec<&str> = argv.lines().collect();
    assert!(
        argv.contains(&"exec") && argv.contains(&"--json"),
        "the fake codex got the codex driver's argv: {argv:?}"
    );
    let stdin = fs::read_to_string(logs.path().join(common::AGENT_STDIN_LOG))
        .expect("the fake codex logs its stdin");
    assert_eq!(stdin, "", "the capture shape gives the agent no stdin");
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
    // D-19: the SIGKILL orphaned the agent, so `resume` would refuse.
    let orphaned = run_status(&home, &root);
    assert!(orphaned.contains("GONE"), "the run reads GONE:\n{orphaned}");
    let kill_line = format!("kill -TERM -- -{}", record.agent_pgid);
    assert!(
        orphaned.contains(&kill_line),
        "GONE beside a live agent names `{kill_line}`:\n{orphaned}"
    );
    assert!(
        !orphaned.contains(&resume_verb),
        "GONE beside a live agent names no resume:\n{orphaned}"
    );

    assert!(
        devflow_core::agent::is_same_process(record.agent_pgid, record.agent_start_time),
        "the record's agent {} is the orphan",
        record.agent_pgid
    );
    common::signal_group(record.agent_pgid, libc::SIGKILL);
    assert!(
        holds_within(RUN_WAIT, || group_members(record.agent_pgid).is_empty()),
        "the agent group {} is gone after SIGKILL",
        record.agent_pgid
    );
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

/// D-01 through the real binary: the `__monitor` re-exec adopts the reserved lock, so
/// it reads live after the `devflow start` that took it has exited, and it reads
/// not-live once that supervisor is gone (its last act removes the file).
#[test]
fn a_real_supervisor_holds_its_reserved_lock_after_its_launcher_exits() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(64);
    init_repo(&root, phase);
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = Shape::Capture.fake_bin(logs.path(), AgentRun::Waiting);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);

    let output = spawn_start(&home, &root, phase, &fake_bin)
        .wait_with_output()
        .expect("wait for the start");
    assert!(
        output.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let state = devflow_core::workflow::load_state(&root, phase).unwrap();
    assert_eq!(state.pending_supervisor, None, "the publish promoted it");
    let record = state.supervisor.expect("the start recorded its supervisor");
    let liveness_path =
        devflow_core::liveness::launch_liveness_path(&root, &record.launch_id).unwrap();
    assert_eq!(
        devflow_core::liveness::probe(&liveness_path).unwrap(),
        devflow_core::liveness::Held::Live,
        "the launcher has exited, so only the supervisor can hold this"
    );

    let outcome = supervisor::request_shutdown(&record.socket_path, RUN_WAIT);
    assert!(
        matches!(outcome, Ok(ShutdownOutcome::Stopped { .. })),
        "the supervisor stops through its socket: {outcome:?}"
    );
    assert!(
        holds_within(RUN_WAIT, || {
            devflow_core::liveness::probe(&liveness_path).unwrap()
                != devflow_core::liveness::Held::Live
        }),
        "control: the lock does not outlive its supervisor"
    );
}

// ---------------------------------------------------------------------------
// 50-15 (R-1, V-12): two `devflow start` launches racing for one phase yield
// exactly one live supervisor and one agent group.
// ---------------------------------------------------------------------------

/// Bounds any gate a launched run parks at, as the 48-19 refusal tests do.
const RACE_GATE_TIMEOUT_SECS: &str = "15";

/// A spawned `devflow start` for `phase` with the arguments the 48-19
/// live-run refusal tests use (supervise mode, `--no-worktree`, the capture
/// shape), so a loser fails only at the reservation's live-run guard, never at
/// an earlier refusal.
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
            Shape::Capture.agent(),
            "--mode",
            "supervise",
            "--no-worktree",
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
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = Shape::Capture.fake_bin(logs.path(), AgentRun::Waiting);
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
    assert!(
        stderr.contains(&format!("devflow stop --phase {phase}")),
        "the loser's refusal must name `devflow stop`\n{}",
        report(loser)
    );
    // While the winner is before its spawn its holder decides; once its supervisor is live
    // the reservation's guard does. Either is a correct refusal, and which one depends on
    // the timing of this run.
    let refused_by = if stderr.contains("is held by a foreground DevFlow process") {
        "the holder"
    } else if stderr.contains("a run is still live") {
        "the reservation's live-run guard"
    } else {
        panic!(
            "the loser must be refused by the holder or the live-run guard\n{}",
            report(loser)
        );
    };
    println!("the loser was refused by {refused_by}");

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
    let logs = tempfile::tempdir().unwrap();
    let fake_bin = Shape::Capture.fake_bin(logs.path(), AgentRun::Waiting);
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
        // The state must record this launch, as a launcher's publish does: the monitor
        // started directly has no launcher to promote it (D-15).
        state.supervisor = Some(devflow_core::state::SupervisorRecord::new(
            socket.as_path().to_path_buf(),
            launch_id.clone(),
            4_294_967_000,
            1,
            String::new(),
        ));
        devflow_core::workflow::save_state(&state).unwrap();
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
/// request, so the stop lands there deterministically. The exit file records
/// the stop outcome, `143`, in place of the agent's own `0`, and `stop`'s line
/// says nothing was signalled (D-16).
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
            "stop: stopped phase {phase} through its supervisor (honoured at an advance safe \
             point; nothing was signalled; exit code 143)"
        )),
        "stdout: {stop_stdout}\nstderr: {stop_stderr}"
    );
    assert!(
        !stop_stdout.contains("agent group ended"),
        "nothing was signalled: {stop_stdout}"
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
        "143",
        "the exit file records the stop outcome, not the agent's own code (D-16)"
    );
}

// ---- 52-08: a run whose Validate passes ends as finished (CUT-02) ---------
//
// Validate is the last stage. A passing Validate ends the run through
// `finish_run`: one `workflow_finished` with `{"reason":"finished",
// "shipped":false}`, one notify, the state kept and marked finished, the
// worktree and branch kept. The `--until plan` halt is the opposite case.

/// [`FINISHING_CLAUDE`] whose marker also reports `"verdict":"pass"`, so its
/// Validate pass reads as Layer 1 Success plus a Pass verdict.
fn passing_claude() -> String {
    FINISHING_CLAUDE.replace(
        r#"{"status":"success","commits":1}"#,
        r#"{"status":"success","commits":1,"verdict":"pass"}"#,
    )
}

/// How long a finished run's supervisors get to exit before the events are
/// counted. Counting earlier could miss a second notify still to come.
const EXIT_WAIT: Duration = Duration::from_secs(30);

/// One auto run's fixture: a hermetic home, a repository with the
/// `.planning/config.json` auto mode's unattended check reads, a passing fake
/// `claude` first on PATH, a notify command that appends one line per call to
/// a log outside the repository, and a `gsd_run` stub under the home that
/// answers GSD's verification status `passed` (52-22: Validate passes only
/// when GSD reports `passed` too).
struct FinishRig {
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    root: std::path::PathBuf,
    phase: PhaseId,
    fake_bin: common::FakeBin,
    notify: tempfile::TempDir,
    gsd_logs: tempfile::TempDir,
}

impl FinishRig {
    fn new(phase: PhaseId) -> Self {
        Self::with_claude(phase, |_| passing_claude())
    }

    /// [`new`](Self::new) with the fake `claude` body `agent(notify_dir)`
    /// returns, for an agent that writes a log beside the notify log.
    fn with_claude(phase: PhaseId, agent: impl FnOnce(&Path) -> String) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        fs::write(
            root.join(".planning/config.json"),
            "{\n  \"workflow\": {\n    \"auto_advance\": true,\n    \"_auto_chain_active\": false\n  }\n}\n",
        )
        .unwrap();
        // A pre-written plan with no checkpoint, so auto mode's unattended
        // Code preflight has a plan to read.
        let padded = phase.padded();
        let plan = format!(".planning/phases/{padded}-fixture/{padded}-01-PLAN.md");
        fs::write(
            root.join(&plan),
            format!(
                "---\nphase: {padded}-fixture\nplan: 01\ntype: execute\nautonomous: true\n---\n\n<tasks>\n<task type=\"auto\"><name>stub</name></task>\n</tasks>\n"
            ),
        )
        .unwrap();
        git(&root, &["add", ".planning/config.json", &plan]);
        git(&root, &["commit", "-q", "-m", "auto config and plan"]);
        let notify = tempfile::tempdir().unwrap();
        let rig = Self {
            home,
            _repo: repo,
            root,
            phase,
            fake_bin: common::fake_bin_dir(&agent(notify.path())),
            notify,
            gsd_logs: tempfile::tempdir().unwrap(),
        };
        rig.stub_gsd(&[devflow_core::test_support::gsd_status_json(
            GsdStatus::Passed,
        )]);
        rig
    }

    /// Replace the run's `gsd_run` stub with one answering `responses`, one per
    /// verification-status read.
    fn stub_gsd(&self, responses: &[&str]) {
        common::stub_gsd_run(&self.home, responses, self.gsd_logs.path());
    }

    /// The calls the `gsd_run` stub logged, as `(working directory, argv)`.
    fn gsd_calls(&self) -> Vec<(std::path::PathBuf, String)> {
        common::gsd_run_calls(self.gsd_logs.path())
    }

    fn notify_log(&self) -> std::path::PathBuf {
        self.notify.path().join("notify.log")
    }

    /// `devflow start --phase N --agent claude --mode auto`, plus `extra`, in
    /// a phase worktree. Asserts the start itself succeeded.
    fn start(&self, extra: &[&str]) -> Output {
        self.start_in("auto", extra)
    }

    /// [`start`](Self::start) in `mode` (`auto` or `supervise`).
    fn start_in(&self, mode: &str, extra: &[&str]) -> Output {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let path = format!(
            "{}:{}",
            self.fake_bin.path.display(),
            existing.to_string_lossy()
        );
        let notify_cmd = format!(
            "printf 'notify stage=%s\\n' \"$DEVFLOW_GATE_STAGE\" >> '{}'",
            self.notify_log().display()
        );
        let output = common::hermetic_devflow(&self.home)
            .args([
                "start",
                "--phase",
                &self.phase.to_string(),
                "--agent",
                "claude",
                "--mode",
                mode,
            ])
            .args(extra)
            .arg(&self.root)
            .current_dir(&self.root)
            .env("PATH", path)
            .env("DEVFLOW_GATE_NOTIFY_CMD", notify_cmd)
            .output()
            .expect("run devflow start");
        assert!(
            output.status.success(),
            "start failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn events(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(devflow_core::events::events_path(&self.root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .collect()
    }

    fn events_text(&self) -> String {
        fs::read_to_string(devflow_core::events::events_path(&self.root)).unwrap_or_default()
    }

    fn positions(&self, name: &str) -> Vec<usize> {
        self.events()
            .iter()
            .enumerate()
            .filter(|(_, event)| event["event"] == name)
            .map(|(index, _)| index)
            .collect()
    }

    /// Wait until the run ends (a `workflow_finished` or a gate), then until
    /// every supervisor it started has exited, so nothing can add an event. A
    /// run that ends at a gate fails here: no run under test should gate.
    fn wait_for_end(&self, reaper: &common::LaunchedRunReaper) {
        common::wait_for(
            || {
                self.events().iter().any(|event| {
                    event["event"] == "workflow_finished" || event["event"] == "gate_fired"
                })
            },
            RUN_WAIT,
            &format!("the run of phase {} to end", self.phase),
        );
        assert!(
            self.positions("gate_fired").is_empty(),
            "the run ended at a gate, not finished; events:\n{}",
            self.events_text()
        );
        assert!(
            reaper.supervisors_exited_within(EXIT_WAIT),
            "the run's supervisors did not exit after it ended; events:\n{}",
            self.events_text()
        );
    }

    fn notify_lines(&self) -> Vec<String> {
        fs::read_to_string(self.notify_log())
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn state(&self) -> devflow_core::state::State {
        devflow_core::workflow::load_state(&self.root, self.phase).expect("the run's state")
    }

    fn git_out(&self, args: &[&str]) -> String {
        let output = devflow_core::test_support::git_command(&self.root)
            .args(args)
            .output()
            .expect("spawn git");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}

#[test]
fn a_run_that_passes_validate_finishes_and_keeps_its_worktree() {
    let phase = PhaseId::new(81);
    let rig = FinishRig::new(phase);
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    rig.wait_for_end(&reaper);
    let log = rig.events_text();
    let events = rig.events();

    let finished = rig.positions("workflow_finished");
    assert_eq!(finished.len(), 1, "one workflow_finished; events:\n{log}");
    let finish = &events[finished[0]];
    let payload: serde_json::Map<String, serde_json::Value> = finish
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| !["v", "ts", "phase", "event"].contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    assert_eq!(
        serde_json::Value::Object(payload),
        serde_json::json!({"reason": "finished", "shipped": false}),
        "the terminal payload; events:\n{log}"
    );

    assert_eq!(
        rig.notify_lines().len(),
        1,
        "notify fired exactly once: {:?}\nevents:\n{log}",
        rig.notify_lines()
    );
    let notified = rig.positions("notify_fired");
    assert_eq!(notified.len(), 1, "one notify_fired; events:\n{log}");
    assert!(
        notified[0] > finished[0],
        "notify_fired follows workflow_finished; events:\n{log}"
    );
    assert_eq!(events[notified[0]]["reason"], "finished", "{log}");

    for absent in ["workflow_shipped", "hook_run"] {
        assert!(
            rig.positions(absent).is_empty(),
            "no {absent} event; events:\n{log}"
        );
    }

    let state_file = devflow_core::workflow::state_path(&rig.root, phase);
    let raw: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_file).unwrap_or_else(|err| {
            panic!("the state file {} is kept: {err}", state_file.display())
        }))
        .expect("the kept state file parses");
    assert_eq!(raw["stopped"], true, "the kept state is stopped: {raw}");
    assert_eq!(raw["finished"], true, "the kept state is finished: {raw}");
    let state = rig.state();
    let worktree = state
        .worktree_path
        .clone()
        .expect("the run used a phase worktree");
    assert!(
        worktree.is_dir(),
        "the worktree {} is kept",
        worktree.display()
    );
    let branch = format!("feature/phase-{}", phase.padded());
    assert_eq!(
        rig.git_out(&["branch", "--list", &branch])
            .trim_start_matches(['*', '+', ' ']),
        branch,
        "the branch is kept"
    );

    let cache = rig.home.cache_dir();
    assert!(
        devflow_core::registry::roots_dir_in(&cache).is_dir(),
        "control: the launch registered the phase in this registry directory"
    );
    assert!(
        !devflow_core::registry::entry_path_in(&cache, &rig.root, phase).exists(),
        "the finished phase is deregistered"
    );
}

/// The `workflow_finished` events of `rig`'s run, payload fields only.
fn finished_payloads(rig: &FinishRig) -> Vec<serde_json::Value> {
    rig.events()
        .into_iter()
        .filter(|event| event["event"] == "workflow_finished")
        .map(|event| {
            serde_json::Value::Object(
                event
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(key, _)| !["v", "ts", "phase", "event"].contains(&key.as_str()))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            )
        })
        .collect()
}

/// The kept state file of `rig`'s run, read as JSON so an absent field fails
/// instead of reading as a default.
fn raw_state(rig: &FinishRig) -> serde_json::Value {
    let state_file = devflow_core::workflow::state_path(&rig.root, rig.phase);
    serde_json::from_str(
        &fs::read_to_string(&state_file)
            .unwrap_or_else(|err| panic!("the state file {} is kept: {err}", state_file.display())),
    )
    .expect("the kept state file parses")
}

/// The opposite case of the finish: `--until plan` halts with `stopped_at`,
/// notifies exactly once after `workflow_finished`, and is not finished.
#[test]
fn an_until_halt_finishes_with_stopped_at_and_notifies_once() {
    let phase = PhaseId::new(82);
    let rig = FinishRig::new(phase);
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&["--until", "plan"]);
    rig.wait_for_end(&reaper);
    let log = rig.events_text();

    assert_eq!(
        finished_payloads(&rig),
        vec![serde_json::json!({"reason": "stopped_at", "stage": "plan"})],
        "one --until halt; events:\n{log}"
    );
    assert_eq!(
        rig.notify_lines().len(),
        1,
        "the halt notified exactly once: {:?}\nevents:\n{log}",
        rig.notify_lines()
    );
    let finished = rig.positions("workflow_finished");
    let notified = rig.positions("notify_fired");
    assert_eq!(notified.len(), 1, "one notify_fired; events:\n{log}");
    assert!(
        notified[0] > finished[0],
        "notify_fired follows workflow_finished; events:\n{log}"
    );
    assert_eq!(rig.events()[notified[0]]["reason"], "stopped_at", "{log}");
    let raw = raw_state(&rig);
    assert_eq!(raw["stopped"], true, "{raw}");
    assert_eq!(raw["finished"], false, "a halt is not a finish: {raw}");
}

/// Validate is the last stage, so `--until validate` on a run whose Validate
/// passes ends as finished, exactly like a run without `--until`: it never
/// reports `stopped_at` for Validate. [`an_until_halt_finishes_with_stopped_at_and_notifies_once`]
/// is its opposite case on the same harness.
#[test]
fn an_until_validate_run_that_passes_finishes_as_finished() {
    let phase = PhaseId::new(83);
    let rig = FinishRig::new(phase);
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&["--until", "validate"]);
    rig.wait_for_end(&reaper);
    let log = rig.events_text();

    assert_eq!(
        finished_payloads(&rig),
        vec![serde_json::json!({"reason": "finished", "shipped": false})],
        "one finish and no stopped_at; events:\n{log}"
    );
    assert_eq!(
        rig.notify_lines().len(),
        1,
        "notify fired exactly once: {:?}\nevents:\n{log}",
        rig.notify_lines()
    );
    let raw = raw_state(&rig);
    assert_eq!(raw["finished"], true, "{raw}");
}

/// D-35: a finished run stays inspectable. `cleanup` without `--force` keeps
/// its worktree, and `resume` refuses it, naming `devflow start`, without
/// launching a stage that already passed.
#[test]
fn a_finished_run_is_kept_by_cleanup_and_refused_by_resume() {
    let phase = PhaseId::new(84);
    let rig = FinishRig::new(phase);
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    rig.wait_for_end(&reaper);
    let state = rig.state();
    assert!(state.finished, "premise: the run finished: {state:?}");
    let worktree = state
        .worktree_path
        .clone()
        .expect("the run used a phase worktree");

    let cleanup = common::hermetic_devflow(&rig.home)
        .arg("cleanup")
        .arg(&rig.root)
        .current_dir(&rig.root)
        .output()
        .expect("run devflow cleanup");
    assert!(
        cleanup.status.success(),
        "cleanup exits 0\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&cleanup.stdout),
        String::from_utf8_lossy(&cleanup.stderr)
    );
    assert!(
        worktree.is_dir(),
        "cleanup without --force keeps the finished run's worktree {}\nstdout: {}",
        worktree.display(),
        String::from_utf8_lossy(&cleanup.stdout)
    );

    let launched_before = stage_launched_count(&rig.root, phase);
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!(
        "{}:{}",
        rig.fake_bin.path.display(),
        existing.to_string_lossy()
    );
    let resume = common::hermetic_devflow(&rig.home)
        .args(["resume", "--phase", &phase.to_string()])
        .arg(&rig.root)
        .current_dir(&rig.root)
        .env("PATH", path)
        .output()
        .expect("run devflow resume");
    let stderr = String::from_utf8_lossy(&resume.stderr).to_string();
    assert!(
        !resume.status.success(),
        "resume refuses a finished run\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&resume.stdout)
    );
    assert!(
        stderr.contains("devflow start"),
        "the refusal names `devflow start`: {stderr}"
    );
    assert_eq!(
        stage_launched_count(&rig.root, phase),
        launched_before,
        "resume launched nothing; events:\n{}",
        rig.events_text()
    );
    assert!(
        rig.state().finished,
        "the refused resume leaves the finished state as it was"
    );
}

// ---- 52-22: Validate passes only when GSD reports `passed` (GATE-02, GATE-03)

/// GATE-03's never-lies rule for one gate context: a context names GSD's
/// `status`, and says `passed` only when GSD returned `passed`. `Err` names
/// the lie.
fn never_lies(context: &str, status: &str) -> Result<(), String> {
    if !context.contains(status) {
        return Err(format!(
            "the context does not name GSD's status {status}: {context}"
        ));
    }
    if status != "passed" && context.contains("passed") {
        return Err(format!(
            "the context says passed while GSD reports {status}: {context}"
        ));
    }
    Ok(())
}

/// D-08 / GATE-03: a supervise run whose Validate agent reports pass while GSD
/// reports `gaps_found` does not finish. The Validate gate names `gaps_found`
/// and never says passed, and GSD was asked about the phase directory under
/// the worktree, from the worktree. Control in the same test: a planted
/// context that says passed beside the same `gaps_found` status fails the
/// never-lies check.
#[test]
fn a_gaps_found_report_never_reads_as_passed() {
    let phase = PhaseId::new(85);
    let rig = FinishRig::new(phase);
    rig.stub_gsd(&[devflow_core::test_support::gsd_status_json(
        GsdStatus::GapsFound,
    )]);
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_in("supervise", &[]);
    common::wait_for(
        || {
            rig.events().iter().any(|event| {
                event["event"] == "workflow_finished"
                    || (event["event"] == "gate_fired" && event["stage"] == "validate")
            })
        },
        RUN_WAIT,
        "the run to finish or reach the Validate gate",
    );
    let log = rig.events_text();
    assert!(
        rig.positions("workflow_finished").is_empty(),
        "a gaps_found report must not finish the run; events:\n{log}"
    );

    let gate_path = devflow_core::gates::Gates::gate_path(
        &rig.root,
        phase,
        devflow_core::stage::Stage::Validate,
    );
    let gate: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&gate_path)
            .unwrap_or_else(|err| panic!("the Validate gate file {}: {err}", gate_path.display())),
    )
    .expect("the gate file parses");
    let context = gate["context"]
        .as_str()
        .expect("a gate context")
        .to_string();
    never_lies(&context, "gaps_found").unwrap_or_else(|lie| panic!("{lie}\nevents:\n{log}"));
    for event in rig.events() {
        if event["event"] == "gate_fired" || event["event"] == "gsd_verification" {
            assert!(
                !event.to_string().contains("passed"),
                "no event may say passed while GSD reports gaps_found: {event}"
            );
        }
    }

    let worktree = rig
        .state()
        .worktree_path
        .expect("the run used a phase worktree")
        .canonicalize()
        .expect("the worktree exists");
    let phase_dir = worktree.join(format!(".planning/phases/{}-fixture", phase.padded()));
    let calls = rig.gsd_calls();
    let read = format!("query verification status {}", phase_dir.display());
    assert!(
        calls
            .iter()
            .any(|(cwd, argv)| argv == &read && cwd == &worktree),
        "GSD was asked `{read}` from {}; calls: {calls:?}",
        worktree.display()
    );

    // Control: the check bites on a planted lie.
    assert!(
        never_lies("GSD verification: passed", "gaps_found").is_err(),
        "a context saying passed beside gaps_found must fail the never-lies check"
    );

    let abort = common::hermetic_devflow(&rig.home)
        .args([
            "gate",
            "abort",
            &phase.to_string(),
            "--stage",
            "validate",
            "--note",
            "test teardown",
        ])
        .arg("--project")
        .arg(&rig.root)
        .output()
        .expect("run devflow gate abort");
    assert!(
        abort.status.success(),
        "gate abort exits 0\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&abort.stdout),
        String::from_utf8_lossy(&abort.stderr)
    );
    assert!(
        reaper.supervisors_exited_within(EXIT_WAIT),
        "the aborted run's supervisors exit; events:\n{}",
        rig.events_text()
    );
}

/// A fake `claude` for the gap loop (GATE-03): it logs every turn it receives
/// to `prompts.log` in `log_dir`, and acts on the stage its prompt names. The
/// Validate pass writes the phase's `NN-VERIFICATION.md`; the gaps Plan pass
/// (`/gsd-plan-phase N --gaps`) writes and commits `NN-02-PLAN.md`, a gap
/// plan with `gap_closure: true` and no checkpoint, newer than the report;
/// every other pass commits an empty commit. Each reports success with a pass
/// verdict, so GSD's stub alone decides each Validate's route.
fn gap_loop_claude(log_dir: &Path, phase: PhaseId) -> String {
    let log = log_dir.join("prompts.log");
    let log = log.to_str().expect("a UTF-8 log path");
    assert!(!log.contains('\''), "the log path is single-quoted: {log}");
    let padded = phase.padded();
    format!(
        r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}}\n' "$token"
    exit 0
    ;;
esac
printf '%s\n' "$turn" >> '{log}'
dir='.planning/phases/{padded}-fixture'
case "$turn" in
  *gsd-validate-phase*)
    printf -- '---\nstatus: gaps_found\n---\n' > "$dir/{padded}-VERIFICATION.md"
    git add "$dir/{padded}-VERIFICATION.md"
    git commit -q -m "fake verification report"
    ;;
  *"gsd-plan-phase {phase} --gaps"*)
    printf -- '---\nphase: {padded}-fixture\nplan: 02\ntype: execute\nautonomous: true\ngap_closure: true\n---\n\n<tasks>\n<task type="auto"><name>gap</name></task>\n</tasks>\n' > "$dir/{padded}-02-PLAN.md"
    git add "$dir/{padded}-02-PLAN.md"
    git commit -q -m "fake gap plan"
    ;;
  *)
    git commit -q --allow-empty -m "fake stage work"
    ;;
esac
printf 'work done\nDEVFLOW_RESULT: {{"status":"success","commits":1,"verdict":"pass"}}\n'
exit 0"#
    )
}

/// The turns [`gap_loop_claude`] logged, one per launched stage pass, in order.
fn logged_prompts(rig: &FinishRig) -> Vec<String> {
    fs::read_to_string(rig.notify.path().join("prompts.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The GSD replies of one gap round: `gaps_found` at the first Validate, the
/// gap plan's `gap_closure` read after the gaps Plan pass (the only plan newer
/// than the report), `passed` at the second Validate.
fn gap_round_replies() -> [&'static str; 3] {
    [
        devflow_core::test_support::gsd_status_json(GsdStatus::GapsFound),
        r#"{"gap_closure": true}"#,
        devflow_core::test_support::gsd_status_json(GsdStatus::Passed),
    ]
}

/// The index of the first prompt in `prompts` holding `needle`.
fn first_prompt_with(prompts: &[String], needle: &str) -> Option<usize> {
    prompts.iter().position(|prompt| prompt.contains(needle))
}

/// GATE-03 / D-09: an auto run whose first Validate reads `gaps_found` goes to
/// Plan with `/gsd-plan-phase N --gaps`, then to Code with
/// `/gsd-execute-phase N --gaps-only` (and not the full execute command), then
/// to Validate, which reads `passed`, and the run finishes inside the same run.
#[test]
fn a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes() {
    let phase = PhaseId::new(86);
    let rig = FinishRig::with_claude(phase, |dir| gap_loop_claude(dir, phase));
    rig.stub_gsd(&gap_round_replies());
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start(&[]);
    rig.wait_for_end(&reaper);
    let log = rig.events_text();
    let prompts = logged_prompts(&rig);

    let first_validate = first_prompt_with(&prompts, "/gsd-validate-phase 86")
        .unwrap_or_else(|| panic!("a Validate ran; prompts: {prompts:?}\nevents:\n{log}"));
    let gaps_plan = first_prompt_with(&prompts, "/gsd-plan-phase 86 --gaps")
        .unwrap_or_else(|| panic!("the gaps Plan ran; prompts: {prompts:?}\nevents:\n{log}"));
    let gaps_code = first_prompt_with(&prompts, "/gsd-execute-phase 86 --gaps-only")
        .unwrap_or_else(|| panic!("the gaps-only Code ran; prompts: {prompts:?}\nevents:\n{log}"));
    assert!(
        first_validate < gaps_plan && gaps_plan < gaps_code,
        "Validate, then Plan --gaps, then Code --gaps-only; prompts: {prompts:?}"
    );
    assert!(
        !prompts[gaps_code].contains("/gsd-execute-phase 86 --auto"),
        "the gaps Code pass runs no full execute: {}",
        prompts[gaps_code]
    );
    assert!(
        prompts[gaps_code + 1..]
            .iter()
            .any(|prompt| prompt.contains("/gsd-validate-phase 86")),
        "Validate runs again after the gaps Code pass; prompts: {prompts:?}"
    );
    assert_eq!(
        finished_payloads(&rig),
        vec![serde_json::json!({"reason": "finished", "shipped": false})],
        "the run finishes once; events:\n{log}"
    );
    assert!(
        !raw_state(&rig)
            .get("gap_closure_pass")
            .is_some_and(|value| value == true),
        "the passed Validate cleared the gap-closure marker: {}",
        raw_state(&rig)
    );
}

/// The `--until validate` notice's wording, as `devflow start` prints it.
const UNTIL_VALIDATE_NOTICE: &str = "--until validate halts nothing";

/// `--until validate` halts nothing in v4 (Validate is the last stage): an
/// auto run started with it whose first Validate reads `gaps_found` runs the
/// gap loop and finishes, records no `stopped_at`, prints one notice line and
/// records one `until_noop` event. Control in the same test, on a fresh rig
/// with the same stubs: `--until plan` halts at Plan, launches no Code, prints
/// no notice and records no `until_noop` event.
#[test]
fn an_until_validate_run_with_gaps_runs_the_gap_loop() {
    let phase = PhaseId::new(87);
    let rig = FinishRig::with_claude(phase, |dir| gap_loop_claude(dir, phase));
    rig.stub_gsd(&gap_round_replies());
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    let output = rig.start(&["--until", "validate"]);
    rig.wait_for_end(&reaper);
    let log = rig.events_text();
    let prompts = logged_prompts(&rig);
    let first_validate = first_prompt_with(&prompts, "/gsd-validate-phase 87")
        .unwrap_or_else(|| panic!("a Validate ran; prompts: {prompts:?}\nevents:\n{log}"));
    let gaps_plan = first_prompt_with(&prompts, "/gsd-plan-phase 87 --gaps")
        .unwrap_or_else(|| panic!("the gaps Plan ran; prompts: {prompts:?}\nevents:\n{log}"));
    let gaps_code = first_prompt_with(&prompts, "/gsd-execute-phase 87 --gaps-only")
        .unwrap_or_else(|| panic!("the gaps-only Code ran; prompts: {prompts:?}\nevents:\n{log}"));
    assert!(
        first_validate < gaps_plan && gaps_plan < gaps_code,
        "Validate, then Plan --gaps, then Code --gaps-only; prompts: {prompts:?}"
    );
    assert!(
        !log.contains("\"stopped_at\""),
        "no workflow_finished is stopped_at; events:\n{log}"
    );
    assert_eq!(
        finished_payloads(&rig),
        vec![serde_json::json!({"reason": "finished", "shipped": false})],
        "exactly one finish; events:\n{log}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.matches(UNTIL_VALIDATE_NOTICE).count(),
        1,
        "start prints the notice once:\n{stdout}"
    );
    let noop: Vec<_> = rig
        .events()
        .into_iter()
        .filter(|event| event["event"] == "until_noop")
        .collect();
    assert_eq!(noop.len(), 1, "one until_noop event; events:\n{log}");
    assert_eq!(noop[0]["until"], "validate", "{}", noop[0]);

    // Control: `--until plan` halts at Plan, with no notice.
    let control_phase = PhaseId::new(88);
    let control = FinishRig::with_claude(control_phase, |dir| gap_loop_claude(dir, control_phase));
    control.stub_gsd(&gap_round_replies());
    let control_reaper = common::LaunchedRunReaper::new(&control.root, control_phase, vec![]);
    let control_output = control.start(&["--until", "plan"]);
    control.wait_for_end(&control_reaper);
    let control_log = control.events_text();
    assert_eq!(
        finished_payloads(&control),
        vec![serde_json::json!({"reason": "stopped_at", "stage": "plan"})],
        "control: --until plan halts at Plan; events:\n{control_log}"
    );
    let control_prompts = logged_prompts(&control);
    assert!(
        first_prompt_with(&control_prompts, "/gsd-execute-phase").is_none(),
        "control: no Code launched; prompts: {control_prompts:?}"
    );
    assert!(
        !String::from_utf8_lossy(&control_output.stdout).contains(UNTIL_VALIDATE_NOTICE),
        "control: --until plan prints no notice"
    );
    assert!(
        !control_log.contains("until_noop"),
        "control: no until_noop event; events:\n{control_log}"
    );
}

// ---- 52-29 (D-45 rules 1 and 2, D-46): a finish needs a Validate PASS on HEAD ----

impl FinishRig {
    /// The run's phase worktree.
    fn worktree(&self) -> std::path::PathBuf {
        self.state()
            .worktree_path
            .expect("the run used a phase worktree")
            .canonicalize()
            .expect("the worktree exists")
    }

    /// `devflow gate <answer> N --stage validate`, asserting it exits 0.
    fn answer_validate(&self, answer: &str) {
        let output = common::hermetic_devflow(&self.home)
            .args([
                "gate",
                answer,
                &self.phase.to_string(),
                "--stage",
                "validate",
            ])
            .arg("--project")
            .arg(&self.root)
            .output()
            .expect("run devflow gate");
        assert!(
            output.status.success(),
            "gate {answer} exits 0\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The Validate gates fired so far.
    fn validate_gates(&self) -> usize {
        self.events()
            .iter()
            .filter(|event| event["event"] == "gate_fired" && event["stage"] == "validate")
            .count()
    }

    /// Wait until `count` Validate gates have fired.
    fn wait_for_validate_gates(&self, count: usize) {
        common::wait_for(
            || self.validate_gates() >= count || !self.positions("workflow_finished").is_empty(),
            RUN_WAIT,
            &format!("Validate gate {count} of phase {}", self.phase),
        );
        assert_eq!(
            self.validate_gates(),
            count,
            "Validate gate {count} fired; events:\n{}",
            self.events_text()
        );
    }

    /// The `gate_resolved` events, in order.
    fn resolved(&self) -> Vec<serde_json::Value> {
        self.events()
            .into_iter()
            .filter(|event| event["event"] == "gate_resolved")
            .collect()
    }

    /// The Validate gate file's JSON.
    fn validate_gate(&self) -> serde_json::Value {
        let path = devflow_core::gates::Gates::gate_path(
            &self.root,
            self.phase,
            devflow_core::stage::Stage::Validate,
        );
        serde_json::from_str(&fs::read_to_string(&path).expect("the Validate gate file"))
            .expect("the gate file parses")
    }

    /// The Validate agent launches so far.
    fn validate_launches(&self) -> usize {
        self.events()
            .iter()
            .filter(|event| event["event"] == "stage_launched" && event["stage"] == "validate")
            .count()
    }
}

/// GSD's `passed` for every status read a supervise run makes.
fn always_passed() -> Vec<&'static str> {
    vec![devflow_core::test_support::gsd_status_json(GsdStatus::Passed); 12]
}

/// D-45 rule 1: a supervise run whose GSD stub answers `passed` to every
/// read and whose Validate agent reports pass. At the first Validate gate the
/// test commits one file on the phase branch in the run's worktree, then
/// approves: no `workflow_finished`, and a second Validate launch. The second
/// gate is approved: one `workflow_finished` `{"reason":"finished",
/// "shipped":false}`. The first approve's `gate_resolved` names the
/// Validate-again route with `agent_pass_on_head: false`, the second names
/// finish with `true`. Control in the same test: the second approve, over the
/// same `passed` stub with the PASS on HEAD, finishes, so the no-finish
/// assertion is one that can fail.
#[test]
fn approving_a_clean_pass_after_a_new_commit_runs_validate_again() {
    let phase = PhaseId::new(91);
    let rig = FinishRig::new(phase);
    rig.stub_gsd(&always_passed());
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_in("supervise", &[]);
    rig.wait_for_validate_gates(1);
    let worktree = rig.worktree();
    fs::write(worktree.join("after-validate.txt"), "new\n").unwrap();
    git(&worktree, &["add", "after-validate.txt"]);
    git(
        &worktree,
        &["commit", "-q", "-m", "a commit after Validate"],
    );
    rig.answer_validate("approve");
    rig.wait_for_validate_gates(2);
    let log = rig.events_text();
    assert!(
        rig.positions("workflow_finished").is_empty(),
        "an approve after a new commit does not finish; events:\n{log}"
    );
    assert_eq!(
        rig.validate_launches(),
        2,
        "Validate runs again; events:\n{log}"
    );
    let first = rig.resolved()[0].clone();
    assert_eq!(first["route"], "validate_again", "{first}");
    assert_eq!(first["agent_pass_on_head"], false, "{first}");
    assert_eq!(first["gsd_status"], "passed", "{first}");

    // Control: the same approve over the PASS on HEAD finishes.
    rig.answer_validate("approve");
    common::wait_for(
        || !rig.positions("workflow_finished").is_empty(),
        RUN_WAIT,
        "the second approve to finish the run",
    );
    let log = rig.events_text();
    assert_eq!(
        finished_payloads(&rig),
        vec![serde_json::json!({"reason": "finished", "shipped": false})],
        "events:\n{log}"
    );
    let second = rig.resolved()[1].clone();
    assert_eq!(second["route"], "finish", "{second}");
    assert_eq!(second["agent_pass_on_head"], true, "{second}");
    assert!(
        reaper.supervisors_exited_within(EXIT_WAIT),
        "the finished run's supervisors exit; events:\n{log}"
    );
}

/// D-46 (U3) and its 2026-11-10 amendment: the same supervise run; at the
/// first Validate gate the test writes one untracked file in the worktree and
/// commits nothing, then approves: no finish, a second Validate launch, and
/// the approve's `gate_resolved` names Validate again with
/// `agent_pass_on_head: false`. The second gate names the untracked path in a
/// `[never-silent]` line, and the state at that gate holds no
/// `validate_pass_head` (that Validate read the dirty tree, so it recorded no
/// PASS). The test deletes the file and approves: still no finish, a third
/// Validate launch, Validate again with `false` (cleaning the tree does not
/// revive a PASS). The third gate, whose Validate ran over the clean tree, is
/// approved: one `workflow_finished`. Control in the same test: that last
/// approve, over the same stub, HEAD and clean tree, finishes because a
/// Validate examined the clean tree.
#[test]
fn approving_a_clean_pass_over_an_uncommitted_change_runs_validate_again() {
    let phase = PhaseId::new(92);
    let rig = FinishRig::new(phase);
    rig.stub_gsd(&always_passed());
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_in("supervise", &[]);
    rig.wait_for_validate_gates(1);
    let worktree = rig.worktree();
    let loose = worktree.join("uncommitted.txt");
    fs::write(&loose, "not committed\n").unwrap();
    rig.answer_validate("approve");
    rig.wait_for_validate_gates(2);
    let log = rig.events_text();
    assert!(
        rig.positions("workflow_finished").is_empty(),
        "an approve over an uncommitted file does not finish; events:\n{log}"
    );
    assert_eq!(
        rig.validate_launches(),
        2,
        "Validate runs again; events:\n{log}"
    );
    let first = rig.resolved()[0].clone();
    assert_eq!(first["route"], "validate_again", "{first}");
    assert_eq!(first["agent_pass_on_head"], false, "{first}");
    let context = rig.validate_gate()["context"]
        .as_str()
        .expect("a gate context")
        .to_string();
    assert!(
        context
            .lines()
            .chain(context.split(". "))
            .any(|line| line.contains("[never-silent]") && line.contains("uncommitted.txt")),
        "the second gate names the untracked path never-silently: {context}"
    );
    assert!(
        rig.state().validate_pass_head.is_none(),
        "a Validate over a dirty tree records no PASS: {:?}",
        rig.state()
    );

    fs::remove_file(&loose).unwrap();
    rig.answer_validate("approve");
    rig.wait_for_validate_gates(3);
    let log = rig.events_text();
    assert!(
        rig.positions("workflow_finished").is_empty(),
        "cleaning the tree does not revive a PASS; events:\n{log}"
    );
    assert_eq!(
        rig.validate_launches(),
        3,
        "a third Validate; events:\n{log}"
    );
    let second = rig.resolved()[1].clone();
    assert_eq!(second["route"], "validate_again", "{second}");
    assert_eq!(second["agent_pass_on_head"], false, "{second}");

    // Control: after a Validate over the clean tree, the same approve finishes.
    rig.answer_validate("approve");
    common::wait_for(
        || !rig.positions("workflow_finished").is_empty(),
        RUN_WAIT,
        "the approve after a clean Validate to finish the run",
    );
    let log = rig.events_text();
    assert_eq!(
        finished_payloads(&rig),
        vec![serde_json::json!({"reason": "finished", "shipped": false})],
        "events:\n{log}"
    );
    assert!(
        reaper.supervisors_exited_within(EXIT_WAIT),
        "the finished run's supervisors exit; events:\n{log}"
    );
}

/// GATE-02 / D-10: a supervise run whose GSD stub answers `passed`; its
/// Validate gate is rejected with a note. The next Code prompt runs
/// `/gsd-quick` with the note, and runs neither the gaps-only nor the full
/// execute command; Validate runs after it.
#[test]
fn rejecting_a_clean_validate_pass_runs_gsd_quick_with_the_note() {
    const NOTE: &str = "rename the quiet flag to silent and keep the old name";
    let phase = PhaseId::new(93);
    let rig = FinishRig::with_claude(phase, |dir| gap_loop_claude(dir, phase));
    rig.stub_gsd(&always_passed());
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);

    rig.start_in("supervise", &[]);
    rig.wait_for_validate_gates(1);
    let output = common::hermetic_devflow(&rig.home)
        .args(["gate", "reject", &phase.to_string(), "--stage", "validate"])
        .args(["--note", NOTE])
        .arg("--project")
        .arg(&rig.root)
        .output()
        .expect("run devflow gate reject");
    assert!(
        output.status.success(),
        "gate reject exits 0\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    rig.wait_for_validate_gates(2);
    let log = rig.events_text();
    let prompts = logged_prompts(&rig);
    let quick = prompts
        .iter()
        .find(|prompt| prompt.contains("/gsd-quick"))
        .unwrap_or_else(|| panic!("a Code prompt runs /gsd-quick; prompts: {prompts:?}"));
    assert!(quick.contains(NOTE), "the note, verbatim: {quick}");
    assert!(!quick.contains("--gaps-only"), "not the gaps pass: {quick}");
    assert!(
        !quick.contains("/gsd-execute-phase"),
        "not the full execute command: {quick}"
    );
    assert_eq!(
        rig.validate_launches(),
        2,
        "Validate runs after it; events:\n{log}"
    );
    assert!(rig.positions("workflow_finished").is_empty(), "{log}");

    let abort = common::hermetic_devflow(&rig.home)
        .args(["gate", "abort", &phase.to_string(), "--stage", "validate"])
        .args(["--note", "test teardown"])
        .arg("--project")
        .arg(&rig.root)
        .output()
        .expect("run devflow gate abort");
    assert!(abort.status.success(), "gate abort exits 0: {abort:?}");
    assert!(
        reaper.supervisors_exited_within(EXIT_WAIT),
        "the aborted run's supervisors exit; events:\n{}",
        rig.events_text()
    );
}

/// The sentence the way-on helper renders for every verification gate
/// (GATE-04, U10): approve states the finish rule rather than a route chosen
/// from the evidence the gate was opened over, reject runs `/gsd-quick` with
/// the note, abort ends the run. The unit test
/// `pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run`
/// pins the helper to the routers; this copy pins what a live gate file says.
const VERIFICATION_WAY_ON: &str = "Answers — approve: reads GSD's verification status again \
     and finishes the run only on `passed` with a Validate pass on this commit over a clean \
     tree, otherwise taking the route the fresh read names; reject with a note: runs \
     `/gsd-quick` with the note, then Validate; abort: ends the run.";

/// The answer files written under `rig`'s gates directory.
fn response_files(rig: &FinishRig) -> Vec<String> {
    fs::read_dir(devflow_core::gates::Gates::dir(&rig.root))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".response.json"))
                .collect()
        })
        .unwrap_or_default()
}

/// D-10, GATE-04: a supervise run whose GSD stub answers `passed` and whose
/// Validate agent passes reaches a `clean_pass` gate whose context leads with
/// the way-on sentence. A reject with a whitespace-only note is refused before
/// anything is written: it exits non-zero, names the answers that apply,
/// writes no response file, and the gate stays pending. That refused answer is
/// the control, the same command with only the note changed. The reject with
/// a note then exits 0 and prints the `/gsd-quick` route with the note, and the
/// next Code prompt runs `/gsd-quick` with that note byte for byte: the
/// printed route is the route run.
#[test]
fn a_validate_gate_names_the_route_its_reject_prints_and_runs() {
    const NOTE: &str = "keep the --quiet alias and document it in the help";
    let phase = PhaseId::new(94);
    let rig = FinishRig::with_claude(phase, |dir| gap_loop_claude(dir, phase));
    rig.stub_gsd(&always_passed());
    let reaper = common::LaunchedRunReaper::new(&rig.root, phase, vec![]);
    let reject = |note: &str| {
        common::hermetic_devflow(&rig.home)
            .args(["gate", "reject", &phase.to_string(), "--stage", "validate"])
            .args(["--note", note])
            .arg("--project")
            .arg(&rig.root)
            .output()
            .expect("run devflow gate reject")
    };

    rig.start_in("supervise", &[]);
    rig.wait_for_validate_gates(1);
    let gate = rig.validate_gate();
    assert_eq!(gate["gate_kind"], "clean_pass", "{gate}");
    let context = gate["context"].as_str().unwrap_or_default();
    assert!(
        context.starts_with(VERIFICATION_WAY_ON),
        "the gate leads with its way on: {context}"
    );

    let refused = reject(" \t ");
    let refused_err = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success(),
        "a whitespace-only reject exits non-zero\nstdout: {}\nstderr: {refused_err}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(
        refused_err.contains("refused") && refused_err.contains("devflow gate reject --note"),
        "the refusal names the answers that apply: {refused_err}"
    );
    assert_eq!(
        response_files(&rig),
        Vec::<String>::new(),
        "a refused reject writes no response file"
    );
    assert!(
        rig.positions("gate_response_written").is_empty(),
        "{}",
        rig.events_text()
    );
    assert_eq!(
        rig.validate_gate()["gate_id"],
        gate["gate_id"],
        "the gate stays pending"
    );

    let output = reject(NOTE);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "gate reject exits 0\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("route: quick") && stdout.contains("`/gsd-quick`"),
        "the reject prints the /gsd-quick route: {stdout}"
    );
    assert!(
        stdout.contains(NOTE),
        "the print carries the note: {stdout}"
    );
    rig.wait_for_validate_gates(2);
    let prompts = logged_prompts(&rig);
    let quick = prompts
        .iter()
        .find(|prompt| prompt.contains("/gsd-quick"))
        .unwrap_or_else(|| panic!("a Code prompt runs /gsd-quick; prompts: {prompts:?}"));
    assert!(quick.contains(NOTE), "the note, byte for byte: {quick}");

    let abort = common::hermetic_devflow(&rig.home)
        .args(["gate", "abort", &phase.to_string(), "--stage", "validate"])
        .args(["--note", "test teardown"])
        .arg("--project")
        .arg(&rig.root)
        .output()
        .expect("run devflow gate abort");
    assert!(abort.status.success(), "gate abort exits 0: {abort:?}");
    assert!(
        reaper.supervisors_exited_within(EXIT_WAIT),
        "the aborted run's supervisors exit; events:\n{}",
        rig.events_text()
    );
}

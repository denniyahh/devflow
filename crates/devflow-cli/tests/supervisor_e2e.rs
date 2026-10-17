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
    assert!(
        state.monitor_pid.is_some(),
        "{shape:?}: monitor_pid stays populated until 50-22"
    );

    let reply = match supervisor::probe(&socket).expect("probe the recorded socket") {
        Liveness::Alive(reply) => reply,
        other => panic!("{shape:?}: the recorded socket must answer ALIVE, got {other:?}"),
    };
    assert_eq!(reply.launch, shape.label(), "{shape:?}: launch label");
    assert_eq!(reply.launch_id, record.launch_id, "{shape:?}: launch id");
    assert_eq!(reply.agent_pgid, record.agent_pgid, "{shape:?}: agent pgid");
    assert_eq!(
        Some(reply.supervisor_pid),
        state.monitor_pid,
        "{shape:?}: monitor_pid names the supervisor"
    );

    let sidecar_path = socket.with_extension("json");
    let sidecar = supervisor::read_sidecar(&sidecar_path).expect("read the sidecar");
    assert_eq!(sidecar.project_root, root, "{shape:?}: sidecar root");
    assert_eq!(sidecar.phase, phase, "{shape:?}: sidecar phase");
    assert_eq!(sidecar.launch, shape.label(), "{shape:?}: sidecar launch");

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

/// The pids whose process group (`/proc/<pid>/stat` field 5) is `pgid`.
fn group_members(pgid: u32) -> Vec<u32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|stat| {
                    let rest = &stat[stat.rfind(')')? + 1..];
                    rest.split_whitespace().nth(2)?.parse::<u32>().ok()
                })
                == Some(pgid)
        })
        .collect()
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

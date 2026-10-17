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
use std::time::Duration;

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

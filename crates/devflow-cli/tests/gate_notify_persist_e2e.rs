//! 49-REVIEW.md WR-01/WR-02/WR-07: the persisted gate notify command, pinned
//! at the CLI boundary.
//!
//! - `start_*` arms: `devflow start` captures `DEVFLOW_GATE_NOTIFY_CMD` into the
//!   state file, and an empty value captures nothing.
//! - `advance_*` arms: a gate fired by `devflow advance` runs the persisted
//!   command when the variable is unset, an empty variable disables it, and a
//!   set variable overrides it.
//! - `agent_*` arms: the coding agent never receives the variable. The legacy
//!   arm reaches the agent through the detached `sh` monitor, so it also shows
//!   that chain does not carry it; the default (pipe-owning) arm reaches the
//!   first agent process `start` launches, which is the delivery canary.
//!
//! Not covered here: the pipe-owning `__monitor` re-exec and its agent spawn
//! (reached only after the canary, which this fixture's agent never answers)
//! and the build-staleness gate's
//! call site (`staleness.rs`), which only fires against a stale self-dogfood
//! build.
//!
//! The variable is set or removed on each spawned child only (TEST-01), so the
//! developer's own shell hook never reaches these runs.

use devflow_core::gates::{GATE_NOTIFY_CMD_ENV, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
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

fn base_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
}

/// A `develop`-only repo whose ROADMAP names `phase` and which has no
/// `.planning/config.json`, so `start --mode auto` parks at its Define
/// preflight gate. Same shape as gate_wedge_e2e.rs `init_start_repo`.
fn init_start_repo(root: &Path, phase: PhaseId) {
    base_repo(root);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: notify fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// A repo on the phase branch with one agent commit, so `advance` from Code
/// parks at a Code gate. Same shape as gate_wedge_e2e.rs `init_repo`.
fn init_advance_repo(root: &Path, phase: PhaseId) {
    base_repo(root);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
    let branch = format!("feature/phase-{}", phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fake_bin_dir() -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::write(&claude, "#!/bin/sh\nsleep 60\n").unwrap();
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

/// Owns a spawned `devflow` process so a panicking assertion never leaks a
/// parked process for the length of its gate timeout, and the private cache
/// directory it registers in (`Drop::drop` reaps the child before the field
/// is removed).
struct ReapOnDrop(
    Child,
    #[expect(dead_code, reason = "held only so the cache dir outlives the child")]
    tempfile::TempDir,
);

/// A private `DEVFLOW_CACHE_DIR` for one spawned child, so its root
/// registration never lands in the developer's real `~/.cache/devflow`.
fn isolated_cache(command: &mut Command) -> tempfile::TempDir {
    let cache = tempfile::tempdir().unwrap();
    command.env("DEVFLOW_CACHE_DIR", cache.path());
    cache
}

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// How a test sets `DEVFLOW_GATE_NOTIFY_CMD` on the spawned child.
enum NotifyEnv<'a> {
    Unset,
    Set(&'a str),
}

fn apply_notify_env(command: &mut Command, notify: NotifyEnv<'_>) {
    match notify {
        NotifyEnv::Unset => command.env_remove(GATE_NOTIFY_CMD_ENV),
        NotifyEnv::Set(value) => command.env(GATE_NOTIFY_CMD_ENV, value),
    };
}

fn wait_until_parked(child: &mut ReapOnDrop, gate: &Path, what: &str) {
    let waited = Instant::now();
    while !gate.exists() {
        if let Some(status) = child.0.try_wait().expect("poll child") {
            panic!("{what} exited ({status:?}) before parking at its gate");
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "timed out after 20s waiting for {what}'s gate"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `notify_fired` is emitted after the hook has run (it runs synchronously),
/// so once the event is on disk a sentinel the hook would create must exist.
fn wait_for_notify_fired(root: &Path) {
    let events = root.join(".devflow").join("events.jsonl");
    let waited = Instant::now();
    loop {
        if fs::read_to_string(&events).is_ok_and(|text| text.contains("\"notify_fired\"")) {
            return;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "timed out after 20s waiting for notify_fired in {}",
            events.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn spawn_parked_start(
    root: &Path,
    phase: PhaseId,
    fake_bin: &FakeBin,
    notify: NotifyEnv<'_>,
) -> ReapOnDrop {
    spawn_parked_start_with(root, phase, fake_bin, notify, true)
}

fn spawn_parked_start_with(
    root: &Path,
    phase: PhaseId,
    fake_bin: &FakeBin,
    notify: NotifyEnv<'_>,
    legacy_launch: bool,
) -> ReapOnDrop {
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
            "auto",
            "--no-worktree",
        ])
        .args(legacy_launch.then_some("--legacy-claude-launch"))
        .arg(root)
        .current_dir(root)
        .env("PATH", path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30")
        .env(TEST_MARKER_ENV, root);
    apply_notify_env(&mut command, notify);
    let cache = isolated_cache(&mut command);
    let mut start = ReapOnDrop(command.spawn().expect("spawn foreground start"), cache);
    wait_until_parked(
        &mut start,
        &Gates::gate_path(root, phase, Stage::Define),
        "start",
    );
    start
}

fn spawn_parked_advance(
    root: &Path,
    phase: PhaseId,
    persisted: Option<&str>,
    notify: NotifyEnv<'_>,
) -> ReapOnDrop {
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state.gate_notify_cmd = persisted.map(str::to_owned);
    devflow_core::workflow::save_state(&state).unwrap();
    let mut command = Command::new(devflow_bin());
    command
        .args(["advance", "--phase", &phase.to_string()])
        .arg(root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30");
    apply_notify_env(&mut command, notify);
    let cache = isolated_cache(&mut command);
    let mut advance = ReapOnDrop(command.spawn().expect("spawn foreground advance"), cache);
    wait_until_parked(
        &mut advance,
        &Gates::gate_path(root, phase, Stage::Code),
        "advance",
    );
    advance
}

fn persisted_cmd(root: &Path, phase: PhaseId) -> Option<String> {
    devflow_core::workflow::load_state(root, phase)
        .expect("load state")
        .gate_notify_cmd
}

#[test]
fn start_persists_the_notify_command_from_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(94);
    init_start_repo(root, phase);
    let sentinel = tempfile::tempdir().unwrap();
    let cmd = format!("touch {}", sentinel.path().join("fired").display());
    let fake_bin = fake_bin_dir();

    let _start = spawn_parked_start(root, phase, &fake_bin, NotifyEnv::Set(&cmd));

    assert_eq!(persisted_cmd(root, phase).as_deref(), Some(cmd.as_str()));
}

#[test]
fn start_with_an_empty_notify_variable_persists_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(94);
    init_start_repo(root, phase);
    let fake_bin = fake_bin_dir();

    let _start = spawn_parked_start(root, phase, &fake_bin, NotifyEnv::Set(""));

    assert_eq!(persisted_cmd(root, phase), None);
}

#[test]
fn advance_gate_runs_the_persisted_command_when_the_variable_is_unset() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);
    init_advance_repo(root, phase);
    let sentinel_dir = tempfile::tempdir().unwrap();
    let sentinel = sentinel_dir.path().join("fired");
    let cmd = format!("touch {}", sentinel.display());

    let _advance = spawn_parked_advance(root, phase, Some(&cmd), NotifyEnv::Unset);
    wait_for_notify_fired(root);

    assert!(sentinel.exists(), "the persisted notify command must run");
}

#[test]
fn advance_gate_prefers_a_set_variable_over_the_persisted_command() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);
    init_advance_repo(root, phase);
    let sentinel_dir = tempfile::tempdir().unwrap();
    let persisted = sentinel_dir.path().join("persisted");
    let overriding = sentinel_dir.path().join("override");
    let persisted_cmd = format!("touch {}", persisted.display());
    let override_cmd = format!("touch {}", overriding.display());

    let _advance = spawn_parked_advance(
        root,
        phase,
        Some(&persisted_cmd),
        NotifyEnv::Set(&override_cmd),
    );
    wait_for_notify_fired(root);

    assert!(overriding.exists(), "the set variable must run");
    assert!(
        !persisted.exists(),
        "the persisted command must not also run"
    );
}

#[test]
fn advance_gate_with_an_empty_variable_skips_the_persisted_command() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);
    init_advance_repo(root, phase);
    let sentinel_dir = tempfile::tempdir().unwrap();
    let sentinel = sentinel_dir.path().join("fired");
    let cmd = format!("touch {}", sentinel.display());

    let _advance = spawn_parked_advance(root, phase, Some(&cmd), NotifyEnv::Set(""));
    wait_for_notify_fired(root);

    assert!(
        !sentinel.exists(),
        "an empty DEVFLOW_GATE_NOTIFY_CMD must disable the persisted command"
    );
}

/// Set on every spawned `start` to the fixture root, so the processes that
/// test launched can be found (and reaped) by their environment.
const TEST_MARKER_ENV: &str = "DEVFLOW_NOTIFY_E2E_MARKER";

/// Every live process whose environment carries `marker`, with its argv and
/// environment entries.
fn marked_processes(marker: &str) -> Vec<(u32, String, Vec<String>)> {
    let wanted = format!("{TEST_MARKER_ENV}={marker}");
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc").expect("read /proc").flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(environ) = fs::read(entry.path().join("environ")) else {
            continue;
        };
        let entries: Vec<String> = environ
            .split(|byte| *byte == 0)
            .map(|item| String::from_utf8_lossy(item).into_owned())
            .collect();
        if entries.contains(&wanted) {
            let argv = fs::read(entry.path().join("cmdline"))
                .map(|raw| String::from_utf8_lossy(&raw).replace('\0', " "))
                .unwrap_or_default();
            found.push((pid, argv, entries));
        }
    }
    found
}

/// Approve the parked Define gate, wait for the monitor chain to launch the
/// fake agent, and return the agent's environment from `/proc`. Every marked
/// process is killed before returning.
fn agent_environment_after_define(root: &Path, phase: PhaseId, fake_bin: &FakeBin) -> Vec<String> {
    let approve = Command::new(devflow_bin())
        .args(["gate", "approve", &phase.to_string(), "define"])
        .arg(root)
        .output()
        .expect("run gate approve");
    assert!(
        approve.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let marker = root.to_string_lossy().into_owned();
    let agent = fake_bin.path.join("claude").to_string_lossy().into_owned();
    let waited = Instant::now();
    let environment = loop {
        if let Some((_, _, entries)) = marked_processes(&marker)
            .into_iter()
            .find(|(_, argv, _)| argv.contains(&agent))
        {
            break entries;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "timed out waiting for the monitor chain to launch the agent"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = Command::new(devflow_bin())
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output();
    for (pid, _, _) in marked_processes(&marker) {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    environment
}

fn assert_chain_has_no_notify_variable(legacy_launch: bool) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(96);
    init_start_repo(root, phase);
    let fake_bin = fake_bin_dir();
    let _start = spawn_parked_start_with(
        root,
        phase,
        &fake_bin,
        NotifyEnv::Set("touch /nonexistent/notify-sentinel"),
        legacy_launch,
    );

    let environment = agent_environment_after_define(root, phase, &fake_bin);

    // Control: a variable set on `start` the same way does reach the agent,
    // so an absent entry below means stripped, not unread.
    assert!(
        environment
            .iter()
            .any(|entry| entry == "DEVFLOW_GATE_TIMEOUT_SECS=30"),
        "control variable missing from the agent environment"
    );
    let prefix = format!("{GATE_NOTIFY_CMD_ENV}=");
    assert!(
        !environment.iter().any(|entry| entry.starts_with(&prefix)),
        "the agent must not carry {GATE_NOTIFY_CMD_ENV}"
    );
}

#[test]
fn agent_via_legacy_monitor_never_receives_the_notify_variable() {
    assert_chain_has_no_notify_variable(true);
}

#[test]
fn agent_via_default_launch_canary_never_receives_the_notify_variable() {
    assert_chain_has_no_notify_variable(false);
}

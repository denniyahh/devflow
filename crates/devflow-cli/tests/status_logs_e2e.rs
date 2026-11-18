//! End-to-end proof that `devflow status` and `devflow logs -f` read the one liveness
//! answer (D-09, D-09a, D-10): a run launched as a runner unit through the home's fake
//! `systemd-run` shows as live naming `stop` and `logs`, every phase found from its
//! state, runner-lock and gate files gets one row, and `logs -f` ends with the run's
//! state line.
//!
//! `commands` is `pub(crate)` inside the `devflow` binary crate, so these drive the
//! compiled binary, like every other devflow-cli integration test.

mod common;

use common::HermeticPath;
use devflow_core::phase_id::PhaseId;
use std::path::{Path, PathBuf};

/// Hermetic `git` invocation for fixture setup (999.37).
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
    std::fs::create_dir_all(root.join(".planning")).unwrap();
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    std::fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: status fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// The fake `claude`'s health check and delivery canary; `body` runs for the stage turn.
fn fake_claude(body: &str) -> String {
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
{body}"#
    )
}

/// A stage turn that keeps running until `trigger` exists, then runs `released`.
fn triggered_agent(trigger: &Path, released: &str) -> String {
    fake_claude(&format!(
        "while [ ! -e '{trigger}' ]; do sleep 0.05; done\n{released}",
        trigger = trigger.display()
    ))
}

/// The phase's unit name as the binary derives it (`service::unit_name`).
fn unit_name(root: &Path, phase: PhaseId) -> String {
    format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(root),
        phase.padded()
    )
}

/// One run launched by `devflow start --mode supervise --until code --no-worktree` as a
/// runner unit through the home's fake `systemd-run`, whose agent waits for its
/// trigger file. The reaper is declared first so it drops before the home and repo.
struct Rig {
    _reaper: common::LaunchedRunReaper,
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    _fake_bin: common::FakeBin,
    root: PathBuf,
    phase: PhaseId,
    /// The runner the fake `systemd-run` recorded for the phase's unit.
    runner_pid: u32,
}

impl Rig {
    /// Launch a run whose agent runs `released` once [`Self::release`] is called.
    fn launch(phase: PhaseId, released: &str) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        let fake_bin = common::fake_bin_dir(&triggered_agent(&Self::trigger_path(&root), released));
        let reaper = common::LaunchedRunReaper::for_home(&home, &root, phase);
        init_repo(&root, phase);
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
        let output = common::hermetic_devflow(&home)
            .args(["start", "--phase", &phase.to_string()])
            .args([
                "--agent",
                "claude",
                "--mode",
                "supervise",
                "--until",
                "code",
            ])
            .arg("--no-worktree")
            .arg(&root)
            .current_dir(&root)
            .hermetic_path(path)
            .output()
            .expect("run devflow start");
        assert!(
            output.status.success(),
            "start failed: {}",
            describe(&output)
        );
        let runner_pid = std::fs::read_to_string(
            home.service_bin()
                .join(format!("{}.pid", unit_name(&root, phase))),
        )
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .expect("start launched a runner through the fake systemd-run");
        Self {
            _reaper: reaper,
            home,
            _repo: repo,
            _fake_bin: fake_bin,
            root,
            phase,
            runner_pid,
        }
    }

    fn trigger_path(root: &Path) -> PathBuf {
        root.join(".git").join("devflow-test-trigger")
    }

    /// Let the agent's stage turn run its `released` body.
    fn release(&self) {
        std::fs::write(Self::trigger_path(&self.root), "").unwrap();
    }

    /// `devflow status` over the rig's root.
    fn status(&self) -> std::process::Output {
        status_of(&self.home, &self.root)
    }
}

/// `devflow status ROOT` under `home`.
fn status_of(home: &common::HermeticHome, root: &Path) -> std::process::Output {
    common::hermetic_devflow(home)
        .arg("status")
        .arg(root)
        .current_dir(root)
        .output()
        .expect("run devflow status")
}

/// A command's exit code, stdout and stderr, for an assertion message.
fn describe(output: &std::process::Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The line `status` prints for `phase`'s row: `phase N: <word> …`.
fn phase_row(stdout: &str, phase: PhaseId) -> Option<String> {
    let head = format!("phase {phase}: ");
    stdout
        .lines()
        .find(|line| line.starts_with(&head))
        .map(str::to_string)
}

/// D-09: a run whose runner holds its lock shows as live, naming `stop` and `logs`, and
/// `status` prints no agent pid line.
#[test]
fn status_shows_a_running_unit_as_live_naming_stop_and_logs() {
    let rig = Rig::launch(PhaseId::new(181), "exit 0");
    let phase = rig.phase;
    assert!(
        common::runs_the_runner_verb(rig.runner_pid),
        "the run's runner {} is running",
        rig.runner_pid
    );

    let output = rig.status();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "status failed: {}",
        describe(&output)
    );
    let row = phase_row(&stdout, phase).unwrap_or_default();
    assert!(
        row.starts_with(&format!("phase {phase}: live")),
        "status did not say live: {}",
        describe(&output)
    );
    assert!(
        row.contains(&format!("devflow stop --phase {phase}"))
            && row.contains(&format!("devflow logs --phase {phase} -f")),
        "the live row names stop and logs: {row}"
    );
    assert!(
        !stdout.contains("agent_pid"),
        "agent pid line still printed: {stdout}"
    );
    assert!(
        !stdout.contains("supervisor:"),
        "supervisor probe line still printed: {stdout}"
    );
}

/// `status` over a bare directory with nothing but what the test plants under
/// `.devflow/`: its stdout, after asserting it exited 0.
fn status_over(home: &common::HermeticHome, root: &Path) -> String {
    let output = status_of(home, root);
    assert!(
        output.status.success(),
        "status failed: {}",
        describe(&output)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// D-09 (review codex-8): a state file that does not parse is listed as unloadable
/// naming `devflow discard`, not dropped into `stage: idle`.
#[test]
fn status_lists_an_unloadable_state_as_unloadable_naming_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(182);
    let state = devflow_core::workflow::state_path(&root, phase);
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::write(&state, "{ not json").unwrap();

    let stdout = status_over(&home, &root);
    let row =
        phase_row(&stdout, phase).unwrap_or_else(|| panic!("phase missing from status: {stdout}"));
    assert!(
        !stdout.contains("stage: idle"),
        "status printed idle over a phase: {stdout}"
    );
    assert!(
        row.starts_with(&format!("phase {phase}: unloadable")),
        "{row}"
    );
    assert!(
        row.contains(&format!("devflow discard --phase {phase}")),
        "{row}"
    );
}

/// D-09a (review codex-8): a gate request file with no state is debris naming
/// `devflow discard`.
#[test]
fn status_lists_gate_files_with_no_state_as_debris_naming_discard() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(183);
    devflow_core::gates::Gates::write_gate(
        &root,
        phase,
        devflow_core::stage::Stage::Plan,
        devflow_core::gates::GateKind::StageFailure,
        "a gate its run left behind",
    )
    .unwrap();

    let stdout = status_over(&home, &root);
    let row =
        phase_row(&stdout, phase).unwrap_or_else(|| panic!("phase missing from status: {stdout}"));
    assert!(
        !stdout.contains("stage: idle"),
        "status printed idle over a phase: {stdout}"
    );
    assert!(row.starts_with(&format!("phase {phase}: debris")), "{row}");
    assert!(
        row.contains(&format!("devflow discard --phase {phase}")),
        "{row}"
    );
}

/// D-09 (review codex-8): a held runner lock with no state is a live run naming
/// `devflow stop`.
#[test]
fn status_lists_a_held_runner_lock_with_no_state_as_live() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(184);
    let _hold = devflow_core::liveness::claim_runner_lock(&root, phase).expect("hold the lock");

    let stdout = status_over(&home, &root);
    let row =
        phase_row(&stdout, phase).unwrap_or_else(|| panic!("phase missing from status: {stdout}"));
    assert!(
        !stdout.contains("stage: idle"),
        "status printed idle over a phase: {stdout}"
    );
    assert!(row.starts_with(&format!("phase {phase}: live")), "{row}");
    assert!(
        row.contains(&format!("devflow stop --phase {phase}")),
        "{row}"
    );
}

/// The history rule's control: a runner lock file nobody holds, with no state, no gate
/// file and no `.devflow/gates/` directory, is not listed, and `status` exits 0 with
/// `stage: idle` (review r3 agy-3: a missing gates directory is an empty set).
#[test]
fn status_omits_a_free_runner_lock_with_nothing_else() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(185);
    drop(devflow_core::liveness::claim_runner_lock(&root, phase).expect("claim the lock"));
    let lock = devflow_core::liveness::runner_lock_path(&root, phase);
    assert!(lock.exists(), "the lock file outlives its holder");
    assert!(
        !devflow_core::gates::Gates::dir(&root).exists(),
        "no gates directory"
    );

    let stdout = status_over(&home, &root);
    assert!(
        phase_row(&stdout, phase).is_none(),
        "a free runner lock was listed: {stdout}"
    );
    assert!(stdout.contains("stage: idle"), "{stdout}");
}

/// How long a `logs -f` child may take to end once its run has.
const LOGS_END_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// A stage turn that fails, so a supervised run opens its stage-failure gate.
const FAILS: &str = r#"printf 'stage failed on purpose\nDEVFLOW_RESULT: {"status":"failed"}\n'
exit 1"#;

/// `devflow logs --phase N -f ROOT` spawned under `home`, its output piped.
fn spawn_logs_follow(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
) -> std::process::Child {
    common::hermetic_devflow(home)
        .args(["logs", "--phase", &phase.to_string(), "-f"])
        .arg(root)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn devflow logs -f")
}

/// Wait for a `logs -f` child to exit within [`LOGS_END_WAIT`]; on timeout kill it and
/// panic with `what`, never hang. Its output once it has exited.
fn logs_output(mut child: std::process::Child, what: &str) -> std::process::Output {
    let started = std::time::Instant::now();
    while child.try_wait().expect("poll logs -f").is_none() {
        if started.elapsed() > LOGS_END_WAIT {
            common::kill_and_reap(&mut child);
            panic!("{what}");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    child.wait_with_output().expect("collect logs -f output")
}

/// The last non-empty line of a command's stderr, where `logs` prints its `==` lines.
fn last_stderr_line(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .to_string()
}

/// D-10: `logs -f` on a run whose agent exits at a failed stage ends with the run's
/// state line, naming the pending gate and the positional `devflow gate approve N`.
#[test]
fn logs_follow_ends_with_the_runs_state_line() {
    let rig = Rig::launch(PhaseId::new(186), FAILS);
    let phase = rig.phase;
    let logs = spawn_logs_follow(&rig.home, &rig.root, phase);
    rig.release();

    let output = logs_output(logs, "logs -f did not end after the agent exited");
    let last = last_stderr_line(&output);
    assert!(
        output.status.success(),
        "logs -f failed: {}",
        describe(&output)
    );
    assert!(
        last.contains("gate") && last.contains(&format!("devflow gate approve {phase}`")),
        "no state line after follow ended: {}",
        describe(&output)
    );
}

/// D-10, control on another row: `logs -f` over a finished run's capture and exit file
/// ends with the finished line.
#[test]
fn logs_follow_on_a_finished_run_ends_with_finished() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(187);
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Supervise,
        root.clone(),
    );
    state.verification_baseline_captured = true;
    state.stage = devflow_core::stage::Stage::Validate;
    state.stopped = true;
    state.finished = true;
    devflow_core::workflow::save_state(&state).unwrap();
    std::fs::write(
        devflow_core::agent_result::stdout_path(&root, phase),
        "validate output\n",
    )
    .unwrap();
    std::fs::write(
        devflow_core::agent_result::exit_code_path(&root, phase),
        "0",
    )
    .unwrap();

    let output = logs_output(
        spawn_logs_follow(&home, &root, phase),
        "logs -f did not end on a finished run",
    );
    let last = last_stderr_line(&output);
    assert!(
        output.status.success(),
        "logs -f failed: {}",
        describe(&output)
    );
    assert!(
        last.contains("finished"),
        "no state line after follow ended: {}",
        describe(&output)
    );
}

/// D-10 (review codex-9): a runner SIGKILLed with its agent group writes no exit file,
/// and `logs -f` still ends within a bound once its output is quiescent and the runner
/// reads NotLive, its last line naming crashed with `devflow resume` and `devflow
/// discard`.
#[test]
fn logs_follow_ends_with_crashed_after_a_hard_kill_with_no_exit_file() {
    let phase = PhaseId::new(188);
    let rig = Rig::launch(
        phase,
        "echo $$ > \"$(dirname \"$0\")/agent.pid\"\nwhile :; do sleep 0.1; done",
    );
    rig.release();
    let agent_pid_file = rig._fake_bin.path.join("agent.pid");
    common::wait_for(
        || {
            std::fs::read_to_string(&agent_pid_file)
                .is_ok_and(|text| text.trim().parse::<u32>().is_ok())
        },
        std::time::Duration::from_secs(20),
        "the agent's pid file",
    );
    let agent_pid: u32 = std::fs::read_to_string(&agent_pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let logs = spawn_logs_follow(&rig.home, &rig.root, phase);
    std::thread::sleep(std::time::Duration::from_millis(700));

    // SAFETY: the runner pid the fake systemd-run recorded for this test's own unit.
    unsafe { libc::kill(rig.runner_pid as libc::pid_t, libc::SIGKILL) };
    // Stands in for KillMode=control-group, which the fake manager does not apply.
    common::signal_group(agent_pid, libc::SIGKILL);

    let output = logs_output(logs, "logs -f did not end after a hard kill");
    let last = last_stderr_line(&output);
    assert!(
        last.contains("crashed")
            && last.contains(&format!("devflow resume --phase {phase}"))
            && last.contains(&format!("devflow discard --phase {phase}")),
        "no crashed line after a hard kill: {}",
        describe(&output)
    );
    assert!(
        !devflow_core::agent_result::exit_code_path(&rig.root, phase).exists(),
        "an exit file exists for the stage"
    );
}

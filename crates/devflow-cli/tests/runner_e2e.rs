//! Container tests of the foreground runner verb, `devflow __runner`, run directly
//! with no user manager and `NOTIFY_SOCKET` unset (D-02, D-07b).
//!
//! Every devflow child is built by [`common::hermetic_devflow`] and gets its stub
//! agent's dir only through [`HermeticPath::hermetic_path`], so the fake service
//! manager stays first on its `PATH`. Every runner child is reaped by a
//! [`RunnerChild`] guard holding its recorded pid.

mod common;

use common::HermeticPath;
use devflow_core::gsd_reader::GsdStatus;
use devflow_core::liveness::{self, Held};
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output};
use std::time::{Duration, Instant};

/// How long a runner may take to run its stubbed stages and exit.
const RUNNER_WAIT: Duration = Duration::from_secs(90);

/// How often a waiting runner is polled.
const RUNNER_POLL: Duration = Duration::from_millis(50);

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

/// The `fake_bin:parent PATH` value phase7_cli's `path_with_fake_bin` builds.
fn path_with_fake_bin(fake_bin: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    format!("{}:{}", fake_bin.display(), existing.to_string_lossy())
}

/// A repository whose saved state for `phase` sits at Plan, driven by a stub `codex`,
/// and the scratch home its devflow children use.
struct RunnerFixture {
    root: PathBuf,
    phase: PhaseId,
    fake_bin: common::FakeBin,
    gsd_logs: tempfile::TempDir,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the fixture.
    home: common::HermeticHome,
}

impl RunnerFixture {
    /// A Codex/Supervise state at Plan with `stop_until`, and a `codex` stub whose body
    /// is `agent_body`.
    fn new(phase: PhaseId, stop_until: Option<Stage>, agent_body: &str) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too.
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        let fake_bin = common::fake_bin_dir_for("codex", agent_body);
        let mut state = devflow_core::state::State::new(
            phase,
            devflow_core::state::AgentKind::Codex,
            devflow_core::mode::Mode::Supervise,
            root.clone(),
        );
        state.stage = Stage::Plan;
        state.stop_until = stop_until;
        state.stopped = false;
        devflow_core::workflow::save_state(&state).unwrap();
        let fixture = Self {
            root,
            phase,
            fake_bin,
            gsd_logs: tempfile::tempdir().unwrap(),
            _repo: repo,
            home: common::HermeticHome::new(),
        };
        // Validate passes only when GSD reports `passed` too (52-22).
        let passed = devflow_core::test_support::gsd_status_json(GsdStatus::Passed);
        common::stub_gsd_run(&fixture.home, &[passed; 16], fixture.gsd_logs.path());
        fixture
    }

    /// A file the stub agent writes beside itself.
    fn beside_agent(&self, name: &str) -> PathBuf {
        self.fake_bin.path.join(name)
    }

    /// The lines of [`Self::beside_agent`]`(name)`, empty when it does not exist.
    fn lines_beside_agent(&self, name: &str) -> Vec<String> {
        fs::read_to_string(self.beside_agent(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// `devflow gate <args>` as a separate process in the fixture root; asserts it
    /// exited 0.
    fn gate(&self, args: &[&str]) -> Output {
        let output = self
            .devflow()
            .arg("gate")
            .args(args)
            .output()
            .expect("run devflow gate");
        assert!(
            output.status.success(),
            "devflow gate {args:?} failed: {}",
            shown(&output)
        );
        output
    }

    /// Whether `stage`'s gate file is open.
    fn gate_open(&self, stage: Stage) -> bool {
        devflow_core::gates::Gates::gate_path(&self.root, self.phase, stage).exists()
    }

    /// A hermetic devflow child in the fixture root, with the stub agent's dir on its
    /// `PATH` behind the fake service manager.
    fn devflow(&self) -> Command {
        let mut command = common::hermetic_devflow(&self.home);
        command
            .hermetic_path(path_with_fake_bin(&self.fake_bin.path))
            .current_dir(&self.root);
        command
    }

    /// `devflow __runner --project ROOT --phase N` with `NOTIFY_SOCKET` unset.
    fn runner(&self) -> Command {
        let mut command = self.devflow();
        command
            .env_remove("NOTIFY_SOCKET")
            .arg("__runner")
            .arg("--project")
            .arg(&self.root)
            .arg("--phase")
            .arg(self.phase.to_string());
        command
    }

    /// Spawn [`Self::runner`] with its output captured, under a [`RunnerChild`] guard.
    fn spawn_runner(&self) -> RunnerChild {
        let child = self
            .runner()
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn devflow __runner");
        RunnerChild::new(child)
    }

    fn runner_lock(&self) -> PathBuf {
        liveness::runner_lock_path(&self.root, self.phase)
    }

    fn state(&self) -> Option<devflow_core::state::State> {
        devflow_core::workflow::load_state(&self.root, self.phase).ok()
    }

    /// Every event line of `kind` in the project's event log.
    fn events_of(&self, kind: &str) -> Vec<serde_json::Value> {
        fs::read_to_string(devflow_core::events::events_path(&self.root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"] == kind)
            .collect()
    }
}

/// A runner child, killed and reaped by its recorded pid when dropped.
struct RunnerChild {
    pid: u32,
    child: Option<Child>,
}

impl RunnerChild {
    fn new(child: Child) -> Self {
        Self {
            pid: child.id(),
            child: Some(child),
        }
    }

    /// Wait for the runner to exit, at most [`RUNNER_WAIT`]; its output once it has.
    fn wait(&mut self) -> Output {
        let deadline = Instant::now() + RUNNER_WAIT;
        let child = self
            .child
            .as_mut()
            .expect("the runner was not waited on yet");
        while child.try_wait().expect("poll the runner").is_none() {
            assert!(
                Instant::now() < deadline,
                "runner pid {} did not exit within {RUNNER_WAIT:?}",
                self.pid
            );
            std::thread::sleep(RUNNER_POLL);
        }
        self.child
            .take()
            .expect("a runner child")
            .wait_with_output()
            .expect("collect the runner's output")
    }

    /// Whether the runner is still running.
    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().expect("poll the runner").is_none())
    }
}

impl Drop for RunnerChild {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            common::kill_and_reap(child);
        }
    }
}

/// The runner's output, for a failure message.
fn shown(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn exited_zero(status: ExitStatus) -> bool {
    status.code() == Some(0)
}

/// A `codex` stub that reports a successful turn.
const PASSING_CODEX: &str = "printf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'";

/// D-02: `devflow __runner` run directly, with `NOTIFY_SOCKET` unset, over a Plan-stage
/// state with `stop_until = Plan`, runs Plan, exits 0, records the Plan verdict and
/// halts at `--until`; its runner lock probes abandoned once it has exited.
#[test]
fn a_runner_run_directly_records_the_plan_verdict() {
    let phase = PhaseId::new(81);
    let fixture = RunnerFixture::new(phase, Some(Stage::Plan), PASSING_CODEX);

    let output = fixture.spawn_runner().wait();
    assert!(
        exited_zero(output.status),
        "runner failed: {}",
        shown(&output)
    );

    let evaluated = fixture.events_of("advance_evaluated");
    assert!(
        evaluated
            .iter()
            .any(|event| event["stage"] == "plan" && event["status"] == "success"),
        "no Plan verdict recorded: {evaluated:?}\n{}",
        shown(&output)
    );
    let state = fixture
        .state()
        .expect("the state is kept after an --until halt");
    assert_eq!(
        (state.stage, state.stopped),
        (Stage::Plan, true),
        "the run halts after Plan as stop_until asks"
    );
    assert_eq!(
        liveness::probe(&fixture.runner_lock()).expect("probe the runner lock"),
        Held::Abandoned,
        "the runner lock is not abandoned after the runner exited"
    );
}

/// A `codex` stub that waits for a `release` file beside it (bounded), then reports a
/// successful turn.
fn codex_waiting_for(release: &Path) -> String {
    format!(
        "i=0; while [ ! -e '{release}' ] && [ \"$i\" -lt 900 ]; do sleep 0.1; i=$((i+1)); done\n\
         {PASSING_CODEX}",
        release = release.display()
    )
}

/// D-02, D-07: a second `__runner` for a phase whose first runner holds the runner lock
/// exits non-zero naming the lock and launches nothing; the first still finishes.
#[test]
fn a_second_runner_refuses_while_the_first_holds_the_lock() {
    let phase = PhaseId::new(82);
    let release_dir = tempfile::tempdir().unwrap();
    let release = release_dir.path().join("release");
    let fixture = RunnerFixture::new(phase, Some(Stage::Plan), &codex_waiting_for(&release));

    let mut first = fixture.spawn_runner();
    common::wait_for(
        || !fixture.events_of("stage_launched").is_empty(),
        RUNNER_WAIT,
        "the first runner to launch Plan",
    );
    assert_eq!(
        liveness::probe(&fixture.runner_lock()).expect("probe the runner lock"),
        Held::Live,
        "the first runner does not hold its lock while Plan runs"
    );

    let second = fixture.runner().output().expect("run a second runner");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        !exited_zero(second.status) && stderr.contains("runner lock"),
        "second runner started: {}",
        shown(&second)
    );
    assert!(first.running(), "the first runner ended before the release");
    assert_eq!(
        fixture.events_of("stage_launched").len(),
        1,
        "the second runner launched a stage"
    );

    fs::write(&release, "").unwrap();
    let output = first.wait();
    assert!(
        exited_zero(output.status),
        "the first runner did not finish: {}",
        shown(&output)
    );
    let evaluated = fixture.events_of("advance_evaluated");
    assert!(
        evaluated
            .iter()
            .any(|event| event["stage"] == "plan" && event["status"] == "success"),
        "no Plan verdict recorded for the first runner: {evaluated:?}"
    );
}

/// A `codex` stub for a full run: on every stage it appends `$PPID` to a `ppids` file
/// beside itself, commits its "work" and reports a passing turn.
const FULL_RUN_CODEX: &str = r#"here=$(dirname "$0")
printf '%s\n' "$PPID" >> "$here/ppids"
git commit -q --allow-empty -m "stub stage work"
printf 'DEVFLOW_RESULT: {"status":"success","commits":1,"verdict":"pass"}\n'"#;

/// The indexes of `lines` (one recorded agent parent pid per line) that name a process
/// other than `runner_pid`: the stages that ran outside the runner.
fn runs_outside_the_runner(runner_pid: u32, lines: &[String]) -> Vec<usize> {
    let runner = runner_pid.to_string();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() != runner)
        .map(|(index, _)| index)
        .collect()
}

/// End every pid `lines` names other than `runner_pid` (a detached `__monitor` a stage
/// ran under), by the recorded pid.
fn end_recorded_non_runner_parents(runner_pid: u32, lines: &[String]) {
    for index in runs_outside_the_runner(runner_pid, lines) {
        if let Ok(pid) = lines[index].trim().parse::<i32>() {
            // SAFETY: a plain kill(2) on a pid this test's own run recorded.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

/// Run the full-run fixture up to its Validate gate: the runner child, once the gate
/// file is open. The wait does not end when the runner exits: a stage launched outside
/// the runner can still open the gate, and the caller's parent-pid check reports it.
fn run_to_the_validate_gate(fixture: &RunnerFixture) -> RunnerChild {
    let runner = fixture.spawn_runner();
    let deadline = Instant::now() + RUNNER_WAIT;
    while !fixture.gate_open(Stage::Validate) {
        if Instant::now() >= deadline {
            let pid = runner.pid;
            let lines = fixture.lines_beside_agent("ppids");
            end_recorded_non_runner_parents(pid, &lines);
            panic!("gate never opened (ppids {lines:?}, runner pid {pid})");
        }
        std::thread::sleep(RUNNER_POLL);
    }
    runner
}

/// D-02, D-11, D-07: one runner runs Plan, Code and Validate as its own children, waits
/// at the Validate gate without taking the phase holder, and finishes once a second
/// process approves the gate with the positional `devflow gate approve N`.
#[test]
fn a_runner_drives_plan_code_validate_and_finishes_after_a_gate_approve_from_a_second_process() {
    let phase = PhaseId::new(83);
    let fixture = RunnerFixture::new(phase, None, FULL_RUN_CODEX);
    let mut runner = run_to_the_validate_gate(&fixture);
    let runner_pid = runner.pid;

    let lines = fixture.lines_beside_agent("ppids");
    let outside = runs_outside_the_runner(runner_pid, &lines);
    if !outside.is_empty() {
        end_recorded_non_runner_parents(runner_pid, &lines);
    }
    assert_eq!(
        (lines.len(), outside.clone()),
        (3, vec![]),
        "a stage ran outside the runner: ppids {lines:?}, runner pid {runner_pid}, outside {outside:?}"
    );
    // Control: a non-runner parent in the Code line is reported.
    let sleeper = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let mut sleeper = scopeguard_child(sleeper);
    let mut planted = lines.clone();
    planted[1] = sleeper.id().to_string();
    let reported = runs_outside_the_runner(runner_pid, &planted);
    common::kill_and_reap(&mut sleeper);
    assert_eq!(
        reported,
        vec![1],
        "the planted non-runner parent was not reported"
    );

    assert_eq!(
        liveness::probe(&fixture.runner_lock()).expect("probe the runner lock"),
        Held::Live,
        "the runner does not hold its lock at the gate"
    );
    let holder = liveness::phase_holder_path(&fixture.root, phase);
    assert_ne!(
        liveness::probe(&holder).expect("probe the phase holder"),
        Held::Live,
        "runner holds the phase holder"
    );

    let number = phase.to_string();
    fixture.gate(&["approve", &number]);
    let output = runner.wait();
    assert!(
        exited_zero(output.status),
        "runner did not finish: {}",
        shown(&output)
    );
    let state = fixture.state().expect("a finished run keeps its state");
    assert!(state.finished, "runner did not finish: state {state:?}");
    assert_eq!(
        fixture.events_of("stage_launched").len(),
        3,
        "three stage_launched events"
    );
    assert_eq!(
        fixture.events_of("workflow_finished").len(),
        1,
        "one workflow_finished event"
    );

    // Control: a held phase holder probes Live through the same probe.
    let _held = liveness::claim_phase_holder(&fixture.root, phase).expect("claim the holder");
    assert_eq!(
        liveness::probe(&holder).expect("probe the phase holder"),
        Held::Live,
        "holder probe missed a held holder"
    );
}

/// A child process, reaped by [`common::kill_and_reap`] by its caller.
fn scopeguard_child(child: Child) -> Child {
    child
}

/// D-11, the control for the approve case: a `devflow gate reject N --note TEXT` from a
/// second process does not finish the run; the runner loops back and waits again.
#[test]
fn a_runner_at_a_rejected_gate_does_not_finish() {
    let phase = PhaseId::new(84);
    let fixture = RunnerFixture::new(phase, None, FULL_RUN_CODEX);
    let mut runner = run_to_the_validate_gate(&fixture);
    let number = phase.to_string();
    let fired = |fixture: &RunnerFixture| {
        fixture
            .events_of("gate_fired")
            .iter()
            .filter(|event| event["stage"] == "validate")
            .count()
    };
    assert_eq!(fired(&fixture), 1, "one Validate gate before the reject");

    fixture.gate(&["reject", &number, "--note", "fix the thing"]);
    common::wait_for(
        || fired(&fixture) >= 2 || !runner.running(),
        RUNNER_WAIT,
        "the Validate gate after the reject",
    );
    let state = fixture.state().expect("a rejected run keeps its state");
    assert!(!state.finished, "rejected run read finished: {state:?}");
    assert!(runner.running(), "the runner ended after a reject");

    fixture.gate(&["abort", &number]);
    let output = runner.wait();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an aborted run's runner exits 1: {}",
        shown(&output)
    );
    assert!(
        fixture.state().is_none_or(|state| !state.finished),
        "rejected run read finished after the abort"
    );
}

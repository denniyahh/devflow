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
    /// The runner child's `PATH` after the fake service manager, when it is not
    /// [`path_with_fake_bin`]'s.
    runner_path: Option<String>,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the fixture.
    home: common::HermeticHome,
}

impl RunnerFixture {
    /// A Codex/Supervise state at Plan with `stop_until`, and a `codex` stub whose body
    /// is `agent_body`.
    fn new(phase: PhaseId, stop_until: Option<Stage>, agent_body: &str) -> Self {
        Self::with_agent(phase, stop_until, |_| agent_body.to_string())
    }

    /// [`Self::new`] with the stub body `agent(root)` returns, for a stub that reads a
    /// file under the fixture root.
    fn with_agent(
        phase: PhaseId,
        stop_until: Option<Stage>,
        agent: impl FnOnce(&Path) -> String,
    ) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too.
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        let fake_bin = common::fake_bin_dir_for("codex", &agent(&root));
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
            runner_path: None,
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
        if let Some(path) = &self.runner_path {
            command.hermetic_path(path);
        }
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
    let mut sleeper = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
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
    let answers = fixture.events_of("gate_response_written");
    assert!(
        answers.len() == 1 && answers[0]["approved"] == true,
        "exactly one gate approve answer: {answers:?}"
    );
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

/// Every event line in the project's event log, in order.
fn all_events(fixture: &RunnerFixture) -> Vec<serde_json::Value> {
    fs::read_to_string(devflow_core::events::events_path(&fixture.root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

/// The index in [`all_events`] of the first event named `kind`, if any.
fn first_position(events: &[serde_json::Value], kind: &str) -> Option<usize> {
    events.iter().position(|event| event["event"] == kind)
}

/// Wait until the runner exits or a gate file for any stage is open, at most
/// [`RUNNER_WAIT`].
fn wait_for_exit_or_gate(fixture: &RunnerFixture, runner: &mut RunnerChild) {
    let deadline = Instant::now() + RUNNER_WAIT;
    while runner.running()
        && ![Stage::Plan, Stage::Code, Stage::Validate]
            .into_iter()
            .any(|stage| fixture.gate_open(stage))
    {
        assert!(
            Instant::now() < deadline,
            "the runner neither exited nor gated within {RUNNER_WAIT:?}"
        );
        std::thread::sleep(RUNNER_POLL);
    }
}

/// A `codex` stub whose first act is to record, as one `exit=… idle=…` line in a `seen`
/// file beside it, whether the exit file `exit` and the idle-timeout record `idle` are
/// present, then reports a passing turn.
fn codex_recording_side_channels(exit: &Path, idle: &Path) -> String {
    format!(
        "e=absent; [ -e '{exit}' ] && e=present\n\
         i=absent; [ -e '{idle}' ] && i=present\n\
         printf 'exit=%s idle=%s\\n' \"$e\" \"$i\" >> \"$(dirname \"$0\")/seen\"\n\
         {PASSING_CODEX}",
        exit = exit.display(),
        idle = idle.display()
    )
}

/// What a monitor writes as an idle-timeout record (the keys of agent_result.rs's
/// `IdleTimeoutRecord`).
const STALE_IDLE_TIMEOUT_RECORD: &str = r#"{"status":"idle_timeout","idle_secs":30,"agent_pid":4242,"written_at":1700000000,"commits":[]}"#;

/// D-02 (review r2 codex-1 = pi-1): in a two-stage run each launch archives the previous
/// capture into history first, and no stale `-exit` or `-idle-timeout` file is present
/// when a stage's agent starts; each `stage_launched` names the runner's pid.
#[test]
fn each_runner_launch_archives_the_previous_capture_and_clears_its_side_channels() {
    use devflow_core::agent_result;
    let phase = PhaseId::new(85);
    let fixture = RunnerFixture::with_agent(phase, Some(Stage::Code), |root| {
        codex_recording_side_channels(
            &agent_result::exit_code_path(root, phase),
            &agent_result::idle_timeout_path(root, phase),
        )
    });
    let stdout = agent_result::stdout_path(&fixture.root, phase);
    let exit = agent_result::exit_code_path(&fixture.root, phase);
    let idle = agent_result::idle_timeout_path(&fixture.root, phase);
    fs::create_dir_all(stdout.parent().expect("a .devflow dir")).unwrap();
    fs::write(&stdout, "stale capture\n").unwrap();
    fs::write(&exit, "0\n").unwrap();
    fs::write(&idle, STALE_IDLE_TIMEOUT_RECORD).unwrap();
    assert!(
        stdout.is_file() && exit.is_file() && idle.is_file(),
        "a planted stale file is missing before the run"
    );

    let mut runner = fixture.spawn_runner();
    let runner_pid = runner.pid;
    wait_for_exit_or_gate(&fixture, &mut runner);
    let seen = fixture.lines_beside_agent("seen");
    for (index, line) in seen.iter().enumerate() {
        let stage = ["Plan", "Code"].get(index).copied().unwrap_or("a later");
        assert!(
            !line.contains("exit=present"),
            "stale exit file seen by the {stage} agent: {seen:?}"
        );
        assert!(
            !line.contains("idle=present"),
            "stale idle-timeout record seen by the {stage} agent: {seen:?}"
        );
    }
    let output = runner.wait();
    assert!(
        exited_zero(output.status),
        "runner failed: {}",
        shown(&output)
    );
    assert_eq!(
        seen,
        vec!["exit=absent idle=absent"; 2],
        "the Plan and Code agents each ran once"
    );

    let archived: Vec<String> = fs::read_dir(agent_result::history_dir(&fixture.root, phase))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with("-stdout"))
                .map(|entry| fs::read_to_string(entry.path()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        archived.len() == 2
            && archived.iter().any(|text| text.contains("stale capture"))
            && archived.iter().any(|text| text.contains("DEVFLOW_RESULT")),
        "no archived capture in history (the stale one and Plan's): {archived:?}"
    );
    let captures = fixture.events_of("capture_archived");
    assert!(
        captures.len() == 2 && captures[1]["stage"] == "plan" && captures[1]["to_stage"] == "code",
        "expected two capture_archived, the second from plan to code: {captures:?}"
    );
    let launched = fixture.events_of("stage_launched");
    let stages: Vec<&str> = launched
        .iter()
        .map(|event| event["stage"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(stages, ["plan", "code"], "stage_launched in run order");
    assert!(
        launched
            .iter()
            .all(|event| event["runner_pid"].as_u64() == Some(u64::from(runner_pid))),
        "stage_launched without runner_pid {runner_pid}: {launched:?}"
    );
    let evaluated = fixture.events_of("advance_evaluated");
    assert!(
        evaluated
            .iter()
            .any(|event| event["stage"] == "plan" && event["status"] == "success"),
        "stale idle-timeout record seen: Plan's verdict is not a pass: {evaluated:?}"
    );
}

/// Symlinks, in a new scratch dir, to only the tools a run invokes by name, resolved
/// from this test's own `PATH`.
fn tools_dir(tools: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    for tool in tools {
        let found = std::env::split_paths(&path)
            .map(|entry| entry.join(tool))
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("`{tool}` is not on this test's PATH"));
        std::os::unix::fs::symlink(found, dir.path().join(tool)).unwrap();
    }
    dir
}

/// The tools a run with a capture-shape stub invokes by name.
const RUN_TOOLS: [&str; 3] = ["git", "sh", "rm"];

/// D-02, never silent (review r2 codex-6): a `codex` missing at the next stage makes
/// launch_stage_next's binary check refuse inside the advance. The runner emits
/// `runner_advance_failed` for Code, opens the infra gate, and exits 1 after a
/// `devflow gate abort N` from a second process. Control: the same PATH with the stub
/// kept launches Code.
#[test]
fn a_missing_agent_binary_at_the_next_stage_opens_the_infra_gate_with_an_event() {
    let tools = tools_dir(&RUN_TOOLS);
    assert!(
        !tools.path().join("codex").exists(),
        "a codex is on the test PATH"
    );

    // Control first: the stub kept, stop_until Code, on the same PATH shape.
    let control_phase = PhaseId::new(86);
    let mut control = RunnerFixture::new(control_phase, Some(Stage::Code), PASSING_CODEX);
    control.runner_path = Some(format!(
        "{}:{}",
        control.fake_bin.path.display(),
        tools.path().display()
    ));
    let output = control.spawn_runner().wait();
    assert!(
        exited_zero(output.status),
        "the kept-stub control failed: {}",
        shown(&output)
    );
    assert_eq!(
        control.events_of("stage_launched").len(),
        2,
        "the kept-stub control did not launch Code"
    );
    assert!(
        control.events_of("runner_advance_failed").is_empty(),
        "control run wrote runner_advance_failed"
    );

    let phase = PhaseId::new(87);
    let removing = format!("rm -f -- \"$0\"\n{PASSING_CODEX}");
    let mut fixture = RunnerFixture::new(phase, Some(Stage::Code), &removing);
    fixture.runner_path = Some(format!(
        "{}:{}",
        fixture.fake_bin.path.display(),
        tools.path().display()
    ));
    let mut runner = fixture.spawn_runner();
    wait_for_exit_or_gate(&fixture, &mut runner);
    assert!(runner.running(), "runner exited at a failed advance");
    assert!(
        !fixture.beside_agent("codex").exists(),
        "the stub did not remove itself"
    );
    let events = all_events(&fixture);
    let failed = fixture.events_of("runner_advance_failed");
    assert!(
        failed.len() == 1
            && failed[0]["stage"] == "code"
            && failed[0]["agent"] == "codex"
            && failed[0]["error"]
                .as_str()
                .is_some_and(|error| error.contains("not found — is it installed?")),
        "no runner_advance_failed event for Code: {failed:?}"
    );
    let failed_at = first_position(&events, "runner_advance_failed");
    let gated_at = events
        .iter()
        .rposition(|event| event["event"] == "gate_fired" && event["stage"] == "code");
    assert!(
        matches!((failed_at, gated_at), (Some(failed), Some(gated)) if gated > failed),
        "no infra gate after a failed advance"
    );
    let state = fixture.state().expect("the state at the infra gate");
    assert_eq!(state.infra_failures, 1, "infra_failures at the gate");
    let stages: Vec<String> = fixture
        .events_of("stage_launched")
        .iter()
        .map(|event| event["stage"].to_string())
        .collect();
    assert_eq!(stages, ["\"plan\""], "only Plan was launched");

    fixture.gate(&["abort", &phase.to_string()]);
    let output = runner.wait();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an aborted run's runner exits 1: {}",
        shown(&output)
    );
}

/// D-02, never silent (W-2): a `codex` stub the binary check accepts (a regular file with
/// its execute bit) whose `#!` names an interpreter that does not exist fails at the
/// exec inside the runner's `monitor::run_stage`. The runner emits `stage_launch_failed`,
/// opens the infra gate, and exits 1 after a `devflow gate abort N` from a second
/// process. Control: a working stub runs Plan with no such event.
#[test]
fn a_stage_the_runner_cannot_start_opens_the_infra_gate_with_an_event() {
    // No other `codex` may be on the runner's PATH: a failed exec of the stub would
    // otherwise go on to the next `codex` the PATH search finds.
    let tools = tools_dir(&RUN_TOOLS);
    assert!(
        !tools.path().join("codex").exists(),
        "a codex is on the test PATH"
    );
    let only_the_stub = |fixture: &mut RunnerFixture| {
        fixture.runner_path = Some(format!(
            "{}:{}",
            fixture.fake_bin.path.display(),
            tools.path().display()
        ));
    };
    let mut control = RunnerFixture::new(PhaseId::new(88), Some(Stage::Plan), PASSING_CODEX);
    only_the_stub(&mut control);
    let output = control.spawn_runner().wait();
    assert!(
        exited_zero(output.status),
        "the working-stub control failed: {}",
        shown(&output)
    );
    assert!(
        control
            .events_of("advance_evaluated")
            .iter()
            .any(|event| event["stage"] == "plan" && event["status"] == "success"),
        "the control recorded no Plan verdict"
    );
    assert!(
        control.events_of("stage_launch_failed").is_empty(),
        "control run wrote stage_launch_failed"
    );

    let phase = PhaseId::new(89);
    let mut fixture = RunnerFixture::new(phase, Some(Stage::Plan), PASSING_CODEX);
    only_the_stub(&mut fixture);
    let stub = fixture.beside_agent("codex");
    fs::write(&stub, "#!/nonexistent/devflow-test-interpreter\n").unwrap();
    fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let mut runner = fixture.spawn_runner();
    wait_for_exit_or_gate(&fixture, &mut runner);
    assert!(runner.running(), "runner exited at a failed start");
    let events = all_events(&fixture);
    let failed = fixture.events_of("stage_launch_failed");
    assert!(
        failed.len() == 1
            && failed[0]["stage"] == "plan"
            && failed[0]["agent"] == "codex"
            && failed[0]["error"]
                .as_str()
                .is_some_and(|error| error.contains("spawn")),
        "no stage_launch_failed event: {failed:?}"
    );
    let failed_at = first_position(&events, "stage_launch_failed");
    let gated_at = events
        .iter()
        .rposition(|event| event["event"] == "gate_fired" && event["stage"] == "plan");
    assert!(
        matches!((failed_at, gated_at), (Some(failed), Some(gated)) if gated > failed),
        "no infra gate after a failed start"
    );
    assert_eq!(
        fixture
            .state()
            .expect("the state at the infra gate")
            .infra_failures,
        1,
        "infra_failures at the gate"
    );

    fixture.gate(&["abort", &phase.to_string()]);
    let output = runner.wait();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an aborted run's runner exits 1: {}",
        shown(&output)
    );
}

// ---- 52.2-05: the runner stops (D-05, D-04b) ----

/// How long a signalled runner may take to end its agent, record the stop and exit.
const STOP_WAIT: Duration = Duration::from_secs(20);

/// The exit code of a runner that recorded a stop.
const STOPPED_EXIT_CODE: i32 = devflow_core::supervisor::runtime::STOPPED_EXIT_CODE;

/// A `codex` stub that, after `prelude`, writes its pid (`$$`, its group's pgid) to a
/// `pid` file beside itself and waits in a short-sleep loop (bounded, about 60 s).
fn codex_waiting_with_pid(prelude: &str) -> String {
    format!(
        "here=$(dirname \"$0\")\n{prelude}\nprintf '%s\\n' \"$$\" > \"$here/pid.tmp\"\n\
         mv \"$here/pid.tmp\" \"$here/pid\"\n\
         i=0; while [ \"$i\" -lt 600 ]; do sleep 0.1; i=$((i+1)); done\n{PASSING_CODEX}"
    )
}

/// SIGKILLs the process groups it names on drop, when they still have a live member:
/// a failing stop test leaves no stub agent behind.
struct GroupReaper(Vec<u32>);

impl Drop for GroupReaper {
    fn drop(&mut self) {
        for &pgid in &self.0 {
            if !devflow_core::test_support::live_group_members(pgid).is_empty() {
                common::signal_group(pgid, libc::SIGKILL);
            }
        }
    }
}

/// The pid a [`codex_waiting_with_pid`] stub wrote to `name` beside it, once it has.
fn agent_pid(fixture: &RunnerFixture, name: &str, runner: &mut RunnerChild) -> u32 {
    let path = fixture.beside_agent(name);
    common::wait_for(
        || path.exists() || !runner.running(),
        RUNNER_WAIT,
        "the stub agent's pid file",
    );
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("no pid file {} ({err}); the runner exited", path.display()))
        .trim()
        .parse()
        .expect("a pid")
}

/// Send `signal` to the runner by its recorded pid.
fn signal_runner(runner: &RunnerChild, signal: libc::c_int) {
    let pid = libc::pid_t::try_from(runner.pid).expect("a pid_t");
    // SAFETY: a plain kill(2) on the runner child this test spawned and has not reaped.
    assert_eq!(unsafe { libc::kill(pid, signal) }, 0, "signal the runner");
}

/// Wait at most [`STOP_WAIT`] for a signalled runner to exit; its output once it has.
fn wait_stopped(runner: &mut RunnerChild, what: &str) -> Output {
    let deadline = Instant::now() + STOP_WAIT;
    while runner.running() {
        assert!(
            Instant::now() < deadline,
            "{what}: the runner's exit code: none, still running {STOP_WAIT:?} after the stop"
        );
        std::thread::sleep(RUNNER_POLL);
    }
    runner.wait()
}

/// D-05: a SIGTERM to the runner while its agent runs ends the agent's process group,
/// records `stopped` with a reason in the state and as a `runner_stopped` event, and the
/// runner exits with STOPPED_EXIT_CODE.
#[test]
fn a_sigterm_to_the_runner_mid_stage_records_stopped() {
    let phase = PhaseId::new(90);
    let fixture = RunnerFixture::new(phase, None, &codex_waiting_with_pid(""));
    let mut runner = fixture.spawn_runner();
    let pgid = agent_pid(&fixture, "pid", &mut runner);
    let _reaper = GroupReaper(vec![pgid]);

    signal_runner(&runner, libc::SIGTERM);
    let output = wait_stopped(&mut runner, "a_sigterm_to_the_runner_mid_stage");

    let live = devflow_core::test_support::live_group_members(pgid);
    assert!(live.is_empty(), "agent group still live: {live:?}");
    let state = fixture.state().expect("a stopped run keeps its state");
    assert!(
        state.stopped && !state.finished && state.stop_reason.is_some(),
        "state not stopped: {state:?}\n{}",
        shown(&output)
    );
    assert!(
        !fixture.events_of("runner_stopped").is_empty(),
        "no stop event in the log: {:?}",
        all_events(&fixture)
    );
    assert_eq!(
        output.status.code(),
        Some(STOPPED_EXIT_CODE),
        "the runner's exit code: {}",
        shown(&output)
    );
}

/// A `codex` stub that counts its runs in a `runs` file beside it. Its first run (Plan)
/// commits and passes; its second (Code) ignores SIGTERM, writes its pid to `code-pid`
/// and waits (bounded, about 60 s).
const TERM_IGNORING_SECOND_RUN_CODEX: &str = r#"here=$(dirname "$0")
n=$(cat "$here/runs" 2>/dev/null || echo 0); n=$((n+1)); printf '%s\n' "$n" > "$here/runs"
if [ "$n" -ge 2 ]; then
  trap '' TERM
  printf '%s\n' "$$" > "$here/code-pid.tmp"; mv "$here/code-pid.tmp" "$here/code-pid"
  i=0; while [ "$i" -lt 600 ]; do sleep 0.1; i=$((i+1)); done
fi
git commit -q --allow-empty -m "stub stage work"
printf 'DEVFLOW_RESULT: {"status":"success","commits":1,"verdict":"pass"}\n'"#;

/// D-05 (review codex-1): the run-long stop token is re-armed for each stage. A stop
/// while the Code agent (the second stage, reached from Plan with no gate) ignores
/// SIGTERM still ends its group, by SIGKILL after the grace, and the run reads
/// `stopped`. Without the per-stage reset Plan's observed exit is still recorded, the
/// group is never signalled, and the bound below turns that into a panic.
#[test]
fn a_stop_in_the_second_stage_ends_a_term_ignoring_agent_and_records_stopped() {
    let phase = PhaseId::new(91);
    let fixture = RunnerFixture::new(phase, None, TERM_IGNORING_SECOND_RUN_CODEX);
    let mut runner = fixture.spawn_runner();
    let pgid = agent_pid(&fixture, "code-pid", &mut runner);
    let _reaper = GroupReaper(vec![pgid]);
    assert_eq!(
        fixture.lines_beside_agent("runs"),
        ["2"],
        "the pid is the Code run's"
    );

    signal_runner(&runner, libc::SIGTERM);
    let deadline = Instant::now() + STOP_WAIT;
    while runner.running() || !devflow_core::test_support::live_group_members(pgid).is_empty() {
        assert!(
            Instant::now() < deadline,
            "stage-2 agent group not ended after a stop: members {:?}, runner running {}",
            devflow_core::test_support::live_group_members(pgid),
            runner.running()
        );
        std::thread::sleep(RUNNER_POLL);
    }
    let output = runner.wait();
    let state = fixture.state().expect("a stopped run keeps its state");
    assert!(
        state.stopped && !state.finished && state.stage == Stage::Code,
        "state not stopped: {state:?}\n{}",
        shown(&output)
    );
    assert_eq!(
        output.status.code(),
        Some(STOPPED_EXIT_CODE),
        "the runner's exit code: {}",
        shown(&output)
    );
}

/// The longest gap a stop test leaves between signalling the agent's group and the
/// runner: under the runner's grace for a stop that reaches the agent first.
const RUNNER_SIGNAL_LAG: Duration = Duration::from_millis(100);

/// The `advance_evaluated` events whose status is `failed`: a stage recorded failed.
fn stage_failures(fixture: &RunnerFixture) -> Vec<serde_json::Value> {
    fixture
        .events_of("advance_evaluated")
        .into_iter()
        .filter(|event| event["status"] == "failed")
        .collect()
}

/// D-04b's order under `KillMode=control-group`: SIGTERM the agent's group first, then
/// the runner within [`RUNNER_SIGNAL_LAG`]. Returns the runner's output.
fn stop_agent_first(fixture: &RunnerFixture, runner: &mut RunnerChild, what: &str) -> Output {
    let pgid = agent_pid(fixture, "pid", runner);
    let _reaper = GroupReaper(vec![pgid]);
    common::signal_group(pgid, libc::SIGTERM);
    std::thread::sleep(RUNNER_SIGNAL_LAG);
    signal_runner(runner, libc::SIGTERM);
    wait_stopped(runner, what)
}

/// D-04b: a stop whose SIGTERM reaches the agent before the runner (the agent dies by
/// SIGTERM) still records `stopped`, and no stage-failure event is written.
#[test]
fn a_stop_whose_sigterm_reaches_the_agent_first_records_stopped() {
    let fixture = RunnerFixture::new(PhaseId::new(92), None, &codex_waiting_with_pid(""));
    let mut runner = fixture.spawn_runner();
    let output = stop_agent_first(&fixture, &mut runner, "agent first");
    let failures = stage_failures(&fixture);
    assert!(
        failures.is_empty(),
        "stage_failed after a stop: {failures:?}\n{}",
        shown(&output)
    );
    let state = fixture.state().expect("a stopped run keeps its state");
    assert!(
        state.stopped && !state.finished,
        "state not stopped: {state:?}"
    );
    assert_eq!(
        output.status.code(),
        Some(STOPPED_EXIT_CODE),
        "the runner's exit code: {}",
        shown(&output)
    );
}

/// D-04b (review r2 codex-4): an agent that traps SIGTERM and exits 143 (a normal exit,
/// not a death by signal) during a stop that reaches it first still records `stopped`,
/// and no stage-failure event is written.
#[test]
fn an_agent_that_traps_term_and_exits_143_during_a_stop_records_stopped() {
    let fixture = RunnerFixture::new(
        PhaseId::new(93),
        None,
        &codex_waiting_with_pid("trap 'exit 143' TERM"),
    );
    let mut runner = fixture.spawn_runner();
    let output = stop_agent_first(&fixture, &mut runner, "trapped exit 143");
    let failures = stage_failures(&fixture);
    assert!(
        failures.is_empty(),
        "a trapped exit 143 during a stop read as a failure: {failures:?}\n{}",
        shown(&output)
    );
    let state = fixture.state().expect("a stopped run keeps its state");
    assert!(
        state.stopped && !state.finished,
        "state not stopped: {state:?}"
    );
    assert_eq!(
        output.status.code(),
        Some(STOPPED_EXIT_CODE),
        "the runner's exit code: {}",
        shown(&output)
    );
}

/// D-04b, the control for the two tests above: an agent that exits 143 by itself, with
/// no stop requested within the runner's grace, is a stage failure, not `stopped`.
#[test]
fn an_agent_exiting_143_without_a_stop_is_a_stage_failure() {
    let phase = PhaseId::new(94);
    let fixture = RunnerFixture::new(phase, None, "exit 143");
    let mut runner = fixture.spawn_runner();
    wait_for_exit_or_gate(&fixture, &mut runner);
    let failures = stage_failures(&fixture);
    let state = fixture.state();
    assert!(
        failures.iter().any(|event| event["stage"] == "plan")
            && state.as_ref().is_some_and(|state| !state.stopped),
        "143 without a stop read as stopped: failures {failures:?}, state {state:?}, events {:?}",
        all_events(&fixture)
    );
    if runner.running() {
        fixture.gate(&["abort", &phase.to_string()]);
    }
    let output = runner.wait();
    assert_ne!(
        output.status.code(),
        Some(STOPPED_EXIT_CODE),
        "143 without a stop read as stopped: {}",
        shown(&output)
    );
}

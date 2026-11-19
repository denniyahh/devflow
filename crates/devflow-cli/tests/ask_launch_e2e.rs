//! Phase 53, criterion 1 and D-03: a stage agent the runner launches reaches the
//! `devflow` MCP server through its per-launch registration and gets the operator's
//! answer in the same process; an agent with no registration is refused before it
//! starts (operator answer 2).
//!
//! Every devflow child is built by [`common::hermetic_devflow`] and gets its stub
//! agent's dir only through [`HermeticPath::hermetic_path`]. The stubs are POSIX sh
//! that read their registration with `case` and `sed` (no jq, no python), which holds
//! because the scratch paths hold no space or quote; each test asserts that first.

mod common;

use common::HermeticPath;
use devflow_core::gates::Gates;
use devflow_core::gsd_reader::GsdStatus;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::AgentKind;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// The phase every test here runs.
const PHASE: u32 = 1;

/// How long a runner may take to run its stubbed stage and exit, or to gate.
const RUNNER_WAIT: Duration = Duration::from_secs(90);

/// How often a waiting test polls.
const POLL: Duration = Duration::from_millis(50);

/// The option the operator picks.
const CHOSEN: &str = "budget-120";

/// The MCP exchange a stub agent writes to the server: initialize, initialized and
/// one `devflow_ask` call with id 2.
const MCP_SCRIPT: &str = r#"init='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"stub-agent","version":"0"}}}'
initialized='{"jsonrpc":"2.0","method":"notifications/initialized"}'
call='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"devflow_ask","arguments":{"question":"Pick a budget","options":[{"id":"budget-30","label":"thirty"},{"id":"budget-120","label":"one twenty"}],"kind":"decision"}}}'
out="$here/server.out"
: > "$out"
{
  printf '%s\n%s\n%s\n' "$init" "$initialized" "$call"
  i=0
  while ! grep -q '"id":2[,}]' "$out" && [ "$i" -lt 1800 ]; do sleep 0.05; i=$((i+1)); done
} | "$cmd" $args > "$out"
sed -n 's/.*"option_id":"\([^"]*\)".*/\1/p' "$out" | head -n 1 > "$here/answer.txt"
"#;

/// A `codex` stub: takes the server's command and args from its own
/// `-c mcp_servers.devflow.*` argv entries, asks through it, and reports success.
fn codex_asking_stub() -> String {
    format!(
        r#"here=$(dirname "$0")
cmd=''
args=''
for a in "$@"; do
  case "$a" in
    mcp_servers.devflow.command=*) cmd=$(printf '%s' "${{a#mcp_servers.devflow.command=}}" | sed 's/^"//; s/"$//') ;;
    mcp_servers.devflow.args=*) args=$(printf '%s' "${{a#mcp_servers.devflow.args=}}" | sed 's/^\[//; s/\]$//; s/"//g; s/,/ /g') ;;
  esac
done
if [ -z "$cmd" ] || [ -z "$args" ]; then echo "no mcp_servers.devflow in argv" >&2; exit 3; fi
{MCP_SCRIPT}
printf 'DEVFLOW_RESULT: {{"status":"success"}}\n'
exit 0"#
    )
}

/// A `codex` stub that asks as [`codex_asking_stub`] does, then reports a failed stage
/// when the server's reply carries `aborted_by_operator` (marking it in
/// `aborted.txt`), and success otherwise.
fn codex_abort_aware_stub() -> String {
    const SUCCESS_TAIL: &str = "printf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\nexit 0";
    let asking = codex_asking_stub();
    assert!(
        asking.ends_with(SUCCESS_TAIL),
        "precondition: the asking stub ends with its success report: {asking}"
    );
    asking.replace(
        SUCCESS_TAIL,
        r#"if grep -q '"isError":true' "$out" && grep -q 'aborted_by_operator' "$out"; then
  : > "$here/aborted.txt"
  printf 'DEVFLOW_RESULT: {"status":"failed","reason":"the operator aborted at the question"}\n'
  exit 0
fi
printf 'DEVFLOW_RESULT: {"status":"success"}\n'
exit 0"#,
    )
}

/// A `claude` stub: answers `--version` and the delivery canary as the supervisor
/// suite's `FINISHING_CLAUDE` does; for a stage turn it records its argv, takes the
/// value after `--mcp-config`, reads the server's command and args from that one-line
/// file, asks through it, and prints a claude-shaped success result.
fn claude_asking_stub() -> String {
    format!(
        r#"here=$(dirname "$0")
if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}}\n' "$token"
    exit 0
    ;;
esac
: > "$here/argv.txt"
file=''
take=''
for a in "$@"; do
  printf '%s\n' "$a" >> "$here/argv.txt"
  if [ -n "$take" ]; then file=$a; take=''; fi
  case "$a" in --mcp-config) take=1 ;; esac
done
if [ -z "$file" ]; then echo "no --mcp-config in argv" >&2; exit 4; fi
cmd=$(sed -n 's/.*"command":"\([^"]*\)".*/\1/p' "$file")
args=$(sed -n 's/.*"args":\[\([^]]*\)\].*/\1/p' "$file" | sed 's/"//g; s/,/ /g')
{MCP_SCRIPT}
printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"DEVFLOW_RESULT: {{\"status\":\"success\"}}"}}'
exit 0"#
    )
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

/// A `develop`-only repo carrying the phase's CONTEXT.md and one PLAN.md, and a GSD
/// config.
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
}

/// A repository whose saved state sits at Plan with `stop_until` Plan for `agent`,
/// driven by a stub `program` whose body is `body`.
struct Fixture {
    root: PathBuf,
    phase: PhaseId,
    fake_bin: common::FakeBin,
    _gsd_logs: tempfile::TempDir,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the fixture.
    home: common::HermeticHome,
}

impl Fixture {
    fn new(agent: AgentKind, program: &str, body: &str) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too.
        let root = repo.path().canonicalize().unwrap();
        let phase = PhaseId::new(PHASE);
        init_repo(&root, phase);
        let fake_bin = common::fake_bin_dir_for(program, body);
        let mut state = devflow_core::state::State::new(
            phase,
            agent,
            devflow_core::mode::Mode::Supervise,
            root.clone(),
        );
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = false;
        devflow_core::workflow::save_state(&state).unwrap();
        let fixture = Self {
            root,
            phase,
            fake_bin,
            _gsd_logs: tempfile::tempdir().unwrap(),
            _repo: repo,
            home: common::HermeticHome::new(),
        };
        let passed = devflow_core::test_support::gsd_status_json(GsdStatus::Passed);
        common::stub_gsd_run(&fixture.home, &[passed; 4], fixture._gsd_logs.path());
        fixture.assert_paths_need_no_quoting();
        fixture
    }

    /// The stubs split the server's args on spaces and read them between quotes, so
    /// no path they carry may hold a space or a quote.
    fn assert_paths_need_no_quoting(&self) {
        let exe = PathBuf::from(common::devflow_bin());
        for path in [&self.root, &self.fake_bin.path, &exe] {
            let text = path.to_string_lossy();
            assert!(
                !text.contains([' ', '"', '\'', '\\']),
                "precondition: the stub's sed parsing needs paths with no space or quote: {text}"
            );
        }
    }

    fn beside_agent(&self, name: &str) -> PathBuf {
        self.fake_bin.path.join(name)
    }

    /// A hermetic devflow child in the fixture root, with the stub agent's dir on its
    /// `PATH` behind the fake service manager.
    fn devflow(&self) -> Command {
        let parent = std::env::var_os("PATH").unwrap_or_default();
        let mut command = common::hermetic_devflow(&self.home);
        command
            .hermetic_path(format!(
                "{}:{}",
                self.fake_bin.path.display(),
                parent.to_string_lossy()
            ))
            .current_dir(&self.root);
        command
    }

    fn spawn_runner(&self) -> Runner {
        let child = self
            .devflow()
            .env_remove("NOTIFY_SOCKET")
            .arg("__runner")
            .arg("--project")
            .arg(&self.root)
            .arg("--phase")
            .arg(self.phase.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn devflow __runner");
        Runner { child: Some(child) }
    }

    fn gate_path(&self) -> PathBuf {
        Gates::gate_path(&self.root, self.phase, Stage::Plan)
    }

    /// Wait for the Plan gate file, then its id.
    fn wait_for_gate_id(&self, runner: &mut Runner) -> String {
        let deadline = Instant::now() + RUNNER_WAIT;
        while !self.gate_path().exists() {
            assert!(
                runner.running(),
                "the runner exited before any gate opened: {}",
                shown(&runner.wait())
            );
            assert!(
                Instant::now() < deadline,
                "no Plan gate within {RUNNER_WAIT:?}"
            );
            std::thread::sleep(POLL);
        }
        let text = fs::read_to_string(self.gate_path()).expect("read the gate file");
        let gate: serde_json::Value = serde_json::from_str(&text).expect("the gate is JSON");
        gate["gate_id"]
            .as_str()
            .unwrap_or_else(|| panic!("the gate names no id: {text}"))
            .to_string()
    }

    /// `devflow gate <args>` from a second process; asserts it exited 0.
    fn gate(&self, args: &[&str]) {
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
    }

    fn events_of(&self, kind: &str) -> Vec<serde_json::Value> {
        fs::read_to_string(devflow_core::events::events_path(&self.root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"] == kind)
            .collect()
    }

    /// Answer the stage's question with [`CHOSEN`] and return the runner's output.
    fn answer_and_finish(&self, mut runner: Runner) -> Output {
        let gate_id = self.wait_for_gate_id(&mut runner);
        self.gate(&[
            "answer",
            &self.phase.to_string(),
            "--gate-id",
            &gate_id,
            "--option",
            CHOSEN,
        ]);
        runner.wait()
    }

    /// The asserts both stub launches share: the answer reached the stub, one Plan
    /// launch, and a runner exit of 0.
    fn assert_answered_in_one_launch(&self, output: &Output) {
        assert_eq!(
            output.status.code(),
            Some(0),
            "runner exit: {}",
            shown(output)
        );
        let answer = fs::read_to_string(self.beside_agent("answer.txt")).unwrap_or_default();
        assert_eq!(
            answer.trim(),
            CHOSEN,
            "answer.txt does not hold the operator's answer; server said: {}\n{}",
            fs::read_to_string(self.beside_agent("server.out")).unwrap_or_default(),
            shown(output)
        );
        let launched = self.events_of("stage_launched");
        assert!(
            launched.len() == 1 && launched[0]["stage"] == "plan",
            "stage_launched: one Plan launch expected: {launched:?}"
        );
    }
}

/// A runner child, killed and reaped when dropped.
struct Runner {
    child: Option<Child>,
}

impl Runner {
    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().expect("poll the runner").is_none())
    }

    fn wait(&mut self) -> Output {
        let deadline = Instant::now() + RUNNER_WAIT;
        while self.running() {
            assert!(
                Instant::now() < deadline,
                "the runner did not exit within {RUNNER_WAIT:?}"
            );
            std::thread::sleep(POLL);
        }
        self.child
            .take()
            .expect("a runner child")
            .wait_with_output()
            .expect("collect the runner's output")
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            common::kill_and_reap(child);
        }
    }
}

fn shown(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Criterion 1: a codex stage agent reads `-c mcp_servers.devflow.*` from its argv,
/// runs that server, asks, and gets the operator's `gate answer` from a second process
/// back in the same process; the stage finishes with one `stage_launched`.
#[test]
fn a_stage_agent_asks_through_its_registration_and_finishes() {
    let fixture = Fixture::new(AgentKind::Codex, "codex", &codex_asking_stub());
    let output = fixture.answer_and_finish(fixture.spawn_runner());
    fixture.assert_answered_in_one_launch(&output);
}

/// Review concern 16: a claude stage agent finds `--mcp-config <file>` as its last two
/// argv entries, reads the server from that file, and gets the operator's answer.
/// Control: the same stub, run without the flag, exits non-zero naming it, so a launch
/// without the splice fails this test.
#[test]
fn a_claude_stage_agent_reaches_the_server_through_its_mcp_config_file() {
    let fixture = Fixture::new(AgentKind::Claude, "claude", &claude_asking_stub());

    let control = Command::new(fixture.beside_agent("claude"))
        .args(["-p", "--verbose"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .expect("the stub's stdin")
                .write_all(b"{\"type\":\"user\"}\n")?;
            child.wait_with_output()
        })
        .expect("run the stub without --mcp-config");
    assert!(
        !control.status.success()
            && String::from_utf8_lossy(&control.stderr).contains("no --mcp-config in argv"),
        "control: the stub without the flag must refuse: {}",
        shown(&control)
    );

    let output = fixture.answer_and_finish(fixture.spawn_runner());
    fixture.assert_answered_in_one_launch(&output);
    let argv = fs::read_to_string(fixture.beside_agent("argv.txt")).expect("the stub's argv");
    let argv: Vec<&str> = argv.lines().collect();
    let registration = devflow_core::workflow::devflow_dir(&fixture.root)
        .join(format!("ask-{}.mcp.json", fixture.phase.padded()));
    assert_eq!(
        argv[argv.len().saturating_sub(2)..],
        ["--mcp-config", &*registration.to_string_lossy()],
        "--mcp-config and its path are not the last two argv entries: {argv:?}"
    );
}

/// Operator answer (2): a run whose agent has no registration is refused before its
/// agent starts: the stub never runs and the refusal names the agent and
/// `devflow_ask`.
#[test]
fn an_unregistered_agent_is_refused_before_it_starts() {
    let marker_body = r#"touch "$(dirname "$0")/ran"
printf 'DEVFLOW_RESULT: {"status":"success"}\n'"#;
    let fixture = Fixture::new(AgentKind::OpenCode, "opencode", marker_body);
    let mut runner = fixture.spawn_runner();
    let deadline = Instant::now() + RUNNER_WAIT;
    while runner.running() && !fixture.gate_path().exists() {
        assert!(
            Instant::now() < deadline,
            "the runner neither exited nor gated within {RUNNER_WAIT:?}"
        );
        std::thread::sleep(POLL);
    }
    if runner.running() {
        fixture.gate(&["abort", &fixture.phase.to_string()]);
    }
    let output = runner.wait();
    assert!(
        !fixture.beside_agent("ran").exists(),
        "stub opencode ran: {}",
        shown(&output)
    );
    let events =
        fs::read_to_string(devflow_core::events::events_path(&fixture.root)).unwrap_or_default();
    let text = format!("{}\n{events}", shown(&output));
    assert!(
        text.contains("opencode") && text.contains("devflow_ask"),
        "the refusal does not name opencode and devflow_ask: {text}"
    );
    assert!(
        fixture.events_of("stage_launched").is_empty(),
        "a refused agent was launched"
    );
}

/// D-10: `gate abort` on a stage agent's question ends the run. The server replies
/// `isError` naming `aborted_by_operator`, the stub reports a failed stage, and the
/// runner then aborts the run instead of routing that failure: one
/// `workflow_aborted`, no saved state, no further launch and no stage-failure gate.
/// The aborted question has a `question_closed` and no `question_answered`.
#[test]
fn gate_abort_on_a_question_ends_the_run() {
    let fixture = Fixture::new(AgentKind::Codex, "codex", &codex_abort_aware_stub());
    let mut runner = fixture.spawn_runner();
    let question_gate = fixture.wait_for_gate_id(&mut runner);
    fixture.gate(&[
        "abort",
        &fixture.phase.to_string(),
        "--gate-id",
        &question_gate,
        "--note",
        "stop",
    ]);
    let deadline = Instant::now() + RUNNER_WAIT;
    while runner.running() {
        if let Ok(text) = fs::read_to_string(fixture.gate_path())
            && let Ok(gate) = serde_json::from_str::<serde_json::Value>(&text)
            && gate["gate_id"] != question_gate.as_str()
        {
            panic!("the run did not abort: a gate opened after the question: {text}");
        }
        assert!(
            Instant::now() < deadline,
            "the runner did not exit within {RUNNER_WAIT:?}"
        );
        std::thread::sleep(POLL);
    }
    let output = runner.wait();

    assert!(
        fixture.beside_agent("aborted.txt").exists(),
        "the stub did not get an isError reply naming aborted_by_operator; server said: {}\n{}",
        fs::read_to_string(fixture.beside_agent("server.out")).unwrap_or_default(),
        shown(&output)
    );
    let aborted = fixture.events_of("workflow_aborted");
    assert_eq!(
        aborted.len(),
        1,
        "one abort event expected: {aborted:?}\n{}",
        shown(&output)
    );
    assert!(
        matches!(
            devflow_core::workflow::load_state(&fixture.root, fixture.phase),
            Err(devflow_core::workflow::WorkflowError::MissingState(_))
        ),
        "an aborted run leaves no saved state, as `gate abort` leaves one"
    );
    let launched = fixture.events_of("stage_launched");
    assert_eq!(
        launched.len(),
        1,
        "a stage launched after the abort: {launched:?}"
    );
    let closed: Vec<_> = fixture
        .events_of("question_closed")
        .into_iter()
        .filter(|event| event["gate_id"] == question_gate.as_str())
        .collect();
    assert!(
        closed.len() == 1 && closed[0]["reason"] == "aborted_by_operator",
        "one question_closed{{aborted_by_operator}} expected: {closed:?}"
    );
    assert!(
        fixture
            .events_of("question_answered")
            .iter()
            .all(|event| event["gate_id"] != question_gate.as_str()),
        "question_answered recorded for an aborted question"
    );
}

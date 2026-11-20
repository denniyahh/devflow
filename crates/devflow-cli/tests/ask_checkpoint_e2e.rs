//! Phase 53, ASK-05 and criterion 3: a `blocking-human` checkpoint a Code-stage agent
//! meets under a one-shot runner launch is asked through `devflow_ask`, answered by
//! the operator, and the stage finishes in the same process; the D-13 fallback (the
//! agent reports the checkpoint without asking) is the control and reaches the Code
//! failure gate. The checkpoint copies the Phase 49 fixture's shape
//! (`scripts/scratch-dogfood-repo.sh`): one decision with options budget-30,
//! budget-120 and budget-300.
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

/// The question the stub asks, from the Phase 49 fixture's checkpoint.
const QUESTION: &str = "Which value should DEFAULT_RETRY_BUDGET_SECS hold, in seconds?";

/// The fixture's three option ids, in the order the stub offers them.
const OPTION_IDS: [&str; 3] = ["budget-30", "budget-120", "budget-300"];

/// The option the stub recommends and the operator picks.
const CHOSEN: &str = "budget-120";

/// The subject of the commit the asking stub makes with the answer.
const BUDGET_COMMIT: &str = "feat: choose the default retry budget";

/// The text the Code failure gate carries for an unasked checkpoint (D-13).
const UNASKED_CHECKPOINT_TEXT: &str = "could not ask through devflow_ask";

/// The prefixes `devflow_core::history::render_timeline` gives the rows under a
/// question event: a question, its options, a missing recommendation, an open answer,
/// an answer, a close and an ignored second resolution.
const QUESTION_ROW_PREFIXES: [&str; 7] = [
    "  question ",
    "  options: ",
    "  recommended: ",
    "  answer: ",
    "  answer ",
    "  closed ",
    "  ignored ",
];

/// A `codex` stub that takes the server's command and args from its own
/// `-c mcp_servers.devflow.*` argv entries, asks the fixture's blocking-human
/// question through it, writes the option it got to `budget.txt`, commits that file
/// and reports success.
fn codex_checkpoint_stub() -> String {
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
init='{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-11-25","capabilities":{{}},"clientInfo":{{"name":"stub-agent","version":"0"}}}}}}'
initialized='{{"jsonrpc":"2.0","method":"notifications/initialized"}}'
call='{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"devflow_ask","arguments":{{"question":"{QUESTION}","options":[{{"id":"budget-30","label":"30 seconds"}},{{"id":"budget-120","label":"120 seconds"}},{{"id":"budget-300","label":"300 seconds"}}],"kind":"blocking-human","recommended":"{CHOSEN}"}}}}}}'
out="$here/server.out"
: > "$out"
{{
  printf '%s\n%s\n%s\n' "$init" "$initialized" "$call"
  i=0
  while ! grep -q '"id":2[,}}]' "$out" && [ "$i" -lt 1800 ]; do sleep 0.05; i=$((i+1)); done
}} | "$cmd" $args > "$out"
answer=$(sed -n 's/.*"option_id":"\([^"]*\)".*/\1/p' "$out" | head -n 1)
if [ -z "$answer" ]; then echo "no option_id in the server's reply" >&2; exit 5; fi
printf '%s\n' "$answer" > budget.txt
git add budget.txt && git commit -q -m '{BUDGET_COMMIT}' || exit 6
printf 'DEVFLOW_RESULT: {{"status":"success"}}\n'
exit 0"#
    )
}

/// A `codex` stub that meets the same checkpoint and reports it without asking: the
/// fallback an agent takes when `devflow_ask` fails (D-13).
const CODEX_UNASKED_STUB: &str = r#"printf 'DEVFLOW_RESULT: {"status":"failed","checkpoint":"blocking-human","reason":"Task 1 needs the operator to choose the retry budget"}\n'
exit 0"#;

fn git_output(root: &Path, args: &[&str]) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A `develop`-only repo carrying the phase's CONTEXT.md and one PLAN.md, and a GSD
/// config.
fn init_repo(root: &Path, phase: PhaseId) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "devflow@example.com"],
        &["config", "user.name", "DevFlow Tests"],
        &["config", "commit.gpgsign", "false"],
        &["config", "core.hooksPath", "/dev/null"],
        &["checkout", "-q", "-b", "develop"],
    ] {
        git_output(root, args);
    }
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
    git_output(root, &["add", "."]);
    git_output(root, &["commit", "-q", "-m", "base"]);
}

/// A repository whose saved state sits at Code with `stop_until` Code for codex,
/// driven by a stub `codex` whose body is `body`.
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
    fn new(body: &str) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too.
        let root = repo.path().canonicalize().unwrap();
        let phase = PhaseId::new(PHASE);
        init_repo(&root, phase);
        let fake_bin = common::fake_bin_dir_for("codex", body);
        let mut state = devflow_core::state::State::new(
            phase,
            AgentKind::Codex,
            devflow_core::mode::Mode::Supervise,
            root.clone(),
        );
        state.stage = Stage::Code;
        state.stop_until = Some(Stage::Code);
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

    /// The stub splits the server's args on spaces and reads them between quotes, so
    /// no path it carries may hold a space or a quote.
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
        Gates::gate_path(&self.root, self.phase, Stage::Code)
    }

    /// Wait for the Code gate file, then return it as JSON.
    fn wait_for_gate(&self, runner: &mut Runner) -> serde_json::Value {
        let deadline = Instant::now() + RUNNER_WAIT;
        while !self.gate_path().exists() {
            assert!(
                runner.running(),
                "the runner exited before any gate opened: {}",
                shown(&runner.wait())
            );
            assert!(
                Instant::now() < deadline,
                "no Code gate within {RUNNER_WAIT:?}"
            );
            std::thread::sleep(POLL);
        }
        let text = fs::read_to_string(self.gate_path()).expect("read the gate file");
        serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("the gate is not JSON ({err}): {text}"))
    }

    /// `devflow <args>` from a second process; asserts it exited 0.
    fn run(&self, args: &[&str]) -> Output {
        let output = self.devflow().args(args).output().expect("run devflow");
        assert!(
            output.status.success(),
            "devflow {args:?} failed: {}",
            shown(&output)
        );
        output
    }

    /// `devflow history <phase> <root>`'s stdout; asserts it rendered the run.
    fn history(&self) -> String {
        let root = self.root.to_string_lossy().into_owned();
        let output = self.run(&["history", &self.phase.to_string(), &root]);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            stdout.contains(&format!("attempt history for phase {}", self.phase)),
            "history did not render the run: {}",
            shown(&output)
        );
        stdout
    }

    fn events_of(&self, kind: &str) -> Vec<serde_json::Value> {
        fs::read_to_string(devflow_core::events::events_path(&self.root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"] == kind)
            .collect()
    }

    /// The asserts both runs share: exactly one Code `stage_launched`.
    fn assert_one_code_launch(&self, output: &str) {
        let launched = self.events_of("stage_launched");
        assert!(
            launched.len() == 1 && launched[0]["stage"] == "code",
            "stage_launched count: one Code launch expected: {launched:?}\n{output}"
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

/// ASK-05, criterion 3: the Code agent asks the blocking-human checkpoint through
/// `devflow_ask`, gets the operator's `budget-120`, commits it and finishes Code in
/// the one launch; the run records the question as `blocking-human` and its answer as
/// `operator_via_gate`, and `devflow history` shows both.
#[test]
fn a_blocking_human_checkpoint_continues_in_the_same_session() {
    let fixture = Fixture::new(&codex_checkpoint_stub());
    let mut runner = fixture.spawn_runner();
    let gate = fixture.wait_for_gate(&mut runner);
    let gate_id = gate["gate_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the gate names no id: {gate}"));
    fixture.run(&[
        "gate",
        "answer",
        &fixture.phase.to_string(),
        "--gate-id",
        gate_id,
        "--option",
        CHOSEN,
    ]);
    let output = runner.wait();
    let shown_run = format!(
        "{}\nserver said: {}",
        shown(&output),
        fs::read_to_string(fixture.beside_agent("server.out")).unwrap_or_default()
    );
    assert_eq!(output.status.code(), Some(0), "runner exit: {shown_run}");

    let subject = git_output(&fixture.root, &["log", "-1", "--format=%s"]);
    assert_eq!(
        subject.trim(),
        BUDGET_COMMIT,
        "the stub's commit: {shown_run}"
    );
    let budget = git_output(&fixture.root, &["show", "HEAD:budget.txt"]);
    assert_eq!(
        budget.trim(),
        CHOSEN,
        "budget.txt in the commit holds the operator's answer: {shown_run}"
    );
    fixture.assert_one_code_launch(&shown_run);

    let asked = fixture.events_of("question_asked");
    assert!(
        asked.len() == 1 && asked[0]["kind"] == "blocking-human" && asked[0]["stage"] == "code",
        "one Code question_asked of kind blocking-human expected: {asked:?}"
    );
    let answered = fixture.events_of("question_answered");
    assert!(
        answered.len() == 1
            && answered[0]["question_id"] == asked[0]["question_id"]
            && answered[0]["answer"]["option_id"] == CHOSEN
            && answered[0]["policy"] == "operator_via_gate",
        "one question_answered with {CHOSEN} and policy operator_via_gate expected: {answered:?}"
    );

    let history = fixture.history();
    assert!(
        history.contains(&format!(": {QUESTION}\n")),
        "history lacks the question text:\n{history}"
    );
    for id in OPTION_IDS {
        assert!(
            history.contains(&format!("{id} \"")),
            "history lacks option {id}:\n{history}"
        );
    }
    assert!(
        history.contains(&format!("{CHOSEN} \"120 seconds\" (recommended)")),
        "history does not mark {CHOSEN} recommended:\n{history}"
    );
    assert!(
        history.contains(&format!(": option {CHOSEN} by "))
            && history.contains("via operator_via_gate"),
        "history lacks the answer {CHOSEN} with operator_via_gate:\n{history}"
    );
}

/// D-13 control: the same checkpoint reported without asking reaches the Code failure
/// gate naming the unasked checkpoint, with one Code launch and nothing relaunched;
/// the run asked nothing, so `devflow history` shows no question row.
#[test]
fn a_checkpoint_reported_without_asking_reaches_the_failure_gate() {
    let fixture = Fixture::new(CODEX_UNASKED_STUB);
    let mut runner = fixture.spawn_runner();
    let gate = fixture.wait_for_gate(&mut runner);
    assert_eq!(gate["gate_kind"], "stage_failure", "the Code gate: {gate}");
    let context = gate["context"]
        .as_str()
        .unwrap_or_else(|| panic!("the gate has no context: {gate}"));
    assert!(
        context.contains(UNASKED_CHECKPOINT_TEXT),
        "the failure gate must name the unasked checkpoint: {context}"
    );
    fixture.assert_one_code_launch(&gate.to_string());

    fixture.run(&["gate", "abort", &fixture.phase.to_string()]);
    let output = runner.wait();
    fixture.assert_one_code_launch(&shown(&output));
    assert!(
        fixture.events_of("question_asked").is_empty(),
        "the fallback run recorded a question_asked"
    );

    let history = fixture.history();
    for line in history.lines() {
        assert!(
            !QUESTION_ROW_PREFIXES
                .iter()
                .any(|prefix| line.starts_with(prefix)),
            "history shows question row {line:?} for a run that asked nothing:\n{history}"
        );
    }
}

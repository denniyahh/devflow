//! The fixture and JSON-RPC client every `devflow __ask` e2e suite shares
//! (Phase 53).
//!
//! [`AskFixture`] is a scratch project whose phase has a saved run state at a
//! stage and whose runner lock the test process holds for the fixture's life,
//! as a live run's would be. [`AskClient`] spawns `devflow __ask` for that run
//! through [`super::hermetic_devflow`], passing the saved state's own
//! `started_at` as `--run-started-at`, writes JSON-RPC lines to its stdin and
//! reads its stdout lines on a thread. A stdout line that is not JSON fails the
//! test, printing the line.

use super::{HermeticHome, hermetic_devflow, kill_and_reap, wait_for};
use devflow_core::gates::Gates;
use devflow_core::liveness;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The phase every ask fixture runs.
pub const ASK_PHASE: u32 = 1;

/// The `--timeout-secs` a client passes unless a test overrides it.
pub const ASK_TIMEOUT_SECS: u64 = 120;

/// How long a fixture waits for the question's gate file to appear.
pub const GATE_APPEAR_WAIT: Duration = Duration::from_secs(30);

/// A scratch project with a saved run state and its runner lock held.
pub struct AskFixture {
    home: HermeticHome,
    root: tempfile::TempDir,
    phase: PhaseId,
    stage: Stage,
    started_at: String,
    _runner_lock: liveness::Hold,
}

impl AskFixture {
    /// A fixture whose phase's saved state is at `stage`.
    pub fn new(stage: Stage) -> Self {
        let home = HermeticHome::new();
        let root = tempfile::tempdir().expect("create the fixture's project root");
        let phase = PhaseId::new(ASK_PHASE);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.path().to_path_buf(),
        );
        state.stage = stage;
        devflow_core::workflow::save_state(&state).expect("save the fixture's run state");
        let runner_lock = liveness::claim_runner_lock(root.path(), phase)
            .expect("hold the fixture's runner lock");
        Self {
            home,
            root,
            phase,
            stage,
            started_at: state.started_at,
            _runner_lock: runner_lock,
        }
    }

    /// The project root.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// The phase.
    pub fn phase(&self) -> PhaseId {
        self.phase
    }

    /// The stage the saved state is at.
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// The saved state's `started_at`.
    pub fn started_at(&self) -> &str {
        &self.started_at
    }

    /// A hermetic `devflow` command run in the project root.
    pub fn devflow(&self) -> Command {
        let mut command = hermetic_devflow(&self.home);
        command.current_dir(self.root());
        command
    }

    /// The stage's gate file.
    pub fn gate_path(&self) -> PathBuf {
        Gates::gate_path(self.root(), self.phase, self.stage)
    }

    /// The id of the stage's open gate, waiting for its file to appear.
    pub fn wait_for_gate_id(&self) -> String {
        wait_for(
            || self.gate_path().exists(),
            GATE_APPEAR_WAIT,
            "no gate file appeared",
        );
        let text = std::fs::read_to_string(self.gate_path()).expect("read the gate file");
        let gate: Value = serde_json::from_str(&text).expect("the gate file is JSON");
        gate["gate_id"]
            .as_str()
            .unwrap_or_else(|| panic!("the gate file names no gate id: {text}"))
            .to_string()
    }
}

/// What [`AskClient::spawn_with`] passes instead of the fixture's values.
#[derive(Debug, Default, Clone)]
pub struct AskOverrides {
    /// A `--run-started-at` other than the fixture's `started_at`.
    pub run_started_at: Option<String>,
    /// A `--timeout-secs` other than [`ASK_TIMEOUT_SECS`].
    pub timeout_secs: Option<u64>,
}

/// A running `devflow __ask` and the lines it has written to stdout.
pub struct AskClient {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Arc<Mutex<Vec<String>>>,
}

impl AskClient {
    /// `devflow __ask` for the fixture's run.
    pub fn spawn(fixture: &AskFixture) -> Self {
        Self::spawn_with(fixture, AskOverrides::default())
    }

    /// `devflow __ask` for the fixture's run, with `overrides` replacing its
    /// `--run-started-at` or `--timeout-secs`.
    pub fn spawn_with(fixture: &AskFixture, overrides: AskOverrides) -> Self {
        let run_started_at = overrides
            .run_started_at
            .unwrap_or_else(|| fixture.started_at().to_string());
        let timeout_secs = overrides.timeout_secs.unwrap_or(ASK_TIMEOUT_SECS);
        let mut child = fixture
            .devflow()
            .arg("__ask")
            .arg("--project")
            .arg(fixture.root())
            .args(["--phase", &fixture.phase().to_string()])
            .args(["--stage", &fixture.stage().to_string()])
            .args(["--timeout-secs", &timeout_secs.to_string()])
            .args(["--run-started-at", &run_started_at])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn devflow __ask");
        let stdout = child.stdout.take().expect("the server's stdout");
        let stdin = child.stdin.take();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let read_into = Arc::clone(&lines);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                read_into.lock().unwrap().push(line);
            }
        });
        Self {
            child,
            stdin,
            lines,
        }
    }

    /// Write `message` as one line.
    pub fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("the server's stdin is open");
        writeln!(stdin, "{message}").expect("write to the server's stdin");
        stdin.flush().expect("flush the server's stdin");
    }

    /// Send a request with `id`.
    pub fn request(&mut self, id: u64, method: &str, params: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    }

    /// The reply with `id`, waiting at most `wait` for it. Every line read so
    /// far must be JSON.
    pub fn reply(&self, id: u64, wait: Duration) -> Option<Value> {
        let started = Instant::now();
        loop {
            let lines = self.lines.lock().unwrap().clone();
            for line in &lines {
                let message: Value = serde_json::from_str(line)
                    .unwrap_or_else(|_| panic!("a stdout line is not JSON: {line:?}"));
                if message["id"] == id {
                    return Some(message);
                }
            }
            if started.elapsed() >= wait {
                return None;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// `initialize` (id 1), `notifications/initialized` and `tools/list`
    /// (id 2), asserting the one tool is `devflow_ask`.
    pub fn handshake(&mut self) {
        self.request(
            1,
            "initialize",
            json!({ "protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": { "name": "ask-e2e", "version": "0" } }),
        );
        let initialized = self.reply(1, GATE_APPEAR_WAIT).expect("initialize replies");
        assert_eq!(
            initialized["result"]["protocolVersion"], "2025-11-25",
            "{initialized}"
        );
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        self.request(2, "tools/list", json!({}));
        let listed = self.reply(2, GATE_APPEAR_WAIT).expect("tools/list replies");
        assert_eq!(
            listed["result"]["tools"][0]["name"], "devflow_ask",
            "{listed}"
        );
    }

    /// A `devflow_ask` call with `id`, asking `question` with one option per id.
    pub fn ask(&mut self, id: u64, question: &str, option_ids: &[&str]) {
        let options: Vec<Value> = option_ids
            .iter()
            .map(|option| json!({ "id": option, "label": format!("label of {option}") }))
            .collect();
        self.request(
            id,
            "tools/call",
            json!({ "name": "devflow_ask", "arguments": {
                "question": question, "options": options, "kind": "decision" } }),
        );
    }
}

impl Drop for AskClient {
    fn drop(&mut self) {
        drop(self.stdin.take());
        kill_and_reap(&mut self.child);
    }
}

//! End-to-end tests of `devflow __triage-score` (Phase 54, D-01, D-04, D-05,
//! D-06): synthetic projects are mined, each question is scored by a loopback
//! Jev fake, and a counts-only report lands under the user data dir.
//!
//! No test reaches TypeSafe: the hermetic command drops `TYPESAFE_*`, and each
//! run that scores gets `TYPESAFE_BASE_URL` pointing at a [`FakeJev`].

mod common;

use common::{HermeticHome, hermetic_devflow};
use devflow_core::corpus::normalize_question;
use devflow_core::test_support::{FakeJev, FakeReply, jev_answer};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

/// The bearer key the fake is sent; not a real key.
const TEST_KEY: &str = "test-key-not-secret";

/// The model the tool asks for by default and the fake answers as.
const MODEL: &str = "jev-1.14.0";

/// A token only `alpha`'s questions carry.
const ALPHA_TOKEN: &str = "zqxalphatoken";

/// A token only `beta`'s questions carry.
const BETA_TOKEN: &str = "zqxbetatoken";

/// A `question_asked` event offering options `a` and `b`, recommending `a`.
fn asked(qid: &str, question: &str, ts: u64) -> Value {
    json!({
        "v": 1, "ts": ts, "phase": 60, "event": "question_asked",
        "stage": "plan", "question_id": qid, "gate_id": format!("g-{qid}"),
        "kind": "decision", "question": question,
        "options": [{"id": "a", "label": "Thirty seconds"}, {"id": "b", "label": "Sixty seconds"}],
        "context": null, "recommended": "a", "run_started_at": 1,
    })
}

/// The operator's `question_answered` event for `qid`.
fn answered(qid: &str, answer: Value, ts: u64) -> Value {
    json!({
        "v": 1, "ts": ts, "phase": 60, "event": "question_answered",
        "stage": "plan", "question_id": qid, "gate_id": format!("g-{qid}"),
        "answer": answer, "policy": "operator_via_gate", "answered_by": "op",
    })
}

/// `alpha`: an event log with three operator-answered `decision` questions,
/// the last answered in the operator's own words.
fn alpha_project() -> TempDir {
    let dir = TempDir::new().expect("create alpha");
    let events = [
        asked("q1", &format!("Which retry budget {ALPHA_TOKEN} one"), 10),
        answered("q1", json!({"option_id": "a"}), 11),
        asked("q2", &format!("Which retry policy {ALPHA_TOKEN} two"), 20),
        answered("q2", json!({"option_id": "b"}), 21),
        asked("q3", &format!("Which retry limit {ALPHA_TOKEN} three"), 30),
        answered("q3", json!({"text": "back off twice then stop"}), 31),
    ];
    let devflow = dir.path().join(".devflow");
    std::fs::create_dir_all(&devflow).expect("create alpha .devflow");
    let text: String = events.iter().map(|e| format!("{e}\n")).collect();
    std::fs::write(devflow.join("events.jsonl"), text).expect("write alpha events");
    dir
}

/// `beta`: one DISCUSSION-LOG with two operator-answered questions.
fn beta_project() -> TempDir {
    let dir = TempDir::new().expect("create beta");
    let header = "| Option | Description | Selected |\n|--------|-------------|----------|\n";
    let text = format!(
        "# Phase 60: X - Discussion Log\n\n**Date:** 2026-01-01\n\n---\n\n\
         ## Layout {BETA_TOKEN} one\n\n{header}| Flat | one module | ✓ |\n| Nested | per stage | |\n\n\
         ## Naming {BETA_TOKEN} two\n\n{header}| Short | terse | |\n| Long | spelled out | ✓ |\n\n"
    );
    let logs = dir.path().join(".planning/phases/60-x");
    std::fs::create_dir_all(&logs).expect("create beta logs dir");
    std::fs::write(logs.join("60-DISCUSSION-LOG.md"), text).expect("write beta log");
    dir
}

/// A fake that answers `count` requests, each naming option `a`.
fn answering_fake(count: usize) -> FakeJev {
    let reply = jev_answer(MODEL, 0.8, "a", &[("a", 0.9), ("b", 0.1)]);
    FakeJev::serve((0..count).map(|_| FakeReply::json(reply.clone())).collect())
}

/// The two synthetic projects and a data dir for one run.
struct Fixture {
    home: HermeticHome,
    alpha: TempDir,
    beta: TempDir,
    data: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            home: HermeticHome::new(),
            alpha: alpha_project(),
            beta: beta_project(),
            data: TempDir::new().expect("create the data dir"),
        }
    }

    /// `<data>/triage`.
    fn triage_dir(&self) -> PathBuf {
        self.data.path().join("triage")
    }

    /// Run the tool over both projects against `fake`, sending `key` when
    /// given and no key otherwise.
    fn run(&self, fake: &FakeJev, key: Option<&str>) -> Output {
        let mut command = hermetic_devflow(&self.home);
        command
            .arg("__triage-score")
            .arg("--project")
            .arg(project_arg("alpha", self.alpha.path()))
            .arg("--project")
            .arg(project_arg("beta", self.beta.path()))
            .env("DEVFLOW_DATA_DIR", self.data.path())
            .env("TYPESAFE_BASE_URL", fake.url());
        if let Some(key) = key {
            command.env("TYPESAFE_API_KEY", key);
        }
        command.output().expect("run devflow __triage-score")
    }
}

fn project_arg(name: &str, path: &Path) -> String {
    format!("{name}={}", path.display())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Run against an answering fake and require success.
fn scored(fixture: &Fixture, fake: &FakeJev) -> Output {
    let output = fixture.run(fake, Some(TEST_KEY));
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        text(&output.stdout),
        text(&output.stderr)
    );
    output
}

#[test]
fn scoring_a_synthetic_corpus_writes_a_counts_only_report() {
    let fixture = Fixture::new();
    let fake = answering_fake(5);
    let output = scored(&fixture, &fake);

    let report_path = fixture.triage_dir().join("report.md");
    let report = std::fs::read_to_string(&report_path).expect("report.md is written");
    for line in [
        "- alpha: decision 3, discuss 0, blocking-human 0",
        "- beta: decision 0, discuss 2, blocking-human 0",
        "Free-text answers: 1",
        "Requests: 5",
        "Errors: 0",
        "Result: escalate-all",
    ] {
        assert!(report.contains(line), "report lacks `{line}`:\n{report}");
    }
    assert_eq!(fake.request_count(), 5);
    let stdout = text(&output.stdout);
    assert!(
        stdout.contains(&report_path.display().to_string()),
        "stdout does not name the report:\n{stdout}"
    );
}

/// Each captured request's body as JSON.
fn request_bodies(fake: &FakeJev) -> Vec<Value> {
    fake.requests()
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("a JSON request body"))
        .collect()
}

/// `path`'s lines, each parsed as JSON.
fn json_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .expect("read a JSON-lines file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect()
}

fn precedent_questions(body: &Value) -> Vec<String> {
    body["state"]["precedents"]
        .as_array()
        .expect("state.precedents is an array")
        .iter()
        .map(|precedent| {
            let question = precedent["question"]
                .as_str()
                .expect("a precedent question");
            normalize_question(question)
        })
        .collect()
}

#[test]
fn scoring_never_puts_a_question_among_its_own_precedents() {
    let fixture = Fixture::new();
    let fake = answering_fake(5);
    scored(&fixture, &fake);

    let lines = json_lines(&fixture.triage_dir().join("scores.jsonl"));
    assert_eq!(lines.len(), 5);
    for line in &lines {
        let id = &line["entry_id"];
        let precedent_ids = line["precedent_ids"].as_array().expect("precedent_ids");
        assert!(!precedent_ids.contains(id), "{line}");
    }
    let bodies = request_bodies(&fake);
    assert_eq!(bodies.len(), 5);
    for body in &bodies {
        let question = normalize_question(body["state"]["question"].as_str().expect("a question"));
        assert!(
            !precedent_questions(body).contains(&question),
            "a request carries its own question: {body}"
        );
    }
    assert!(
        bodies
            .iter()
            .any(|body| !precedent_questions(body).is_empty()),
        "no request carried a precedent, so the check above proves nothing"
    );
}

#[test]
fn scoring_takes_precedents_from_the_questions_own_project() {
    let fixture = Fixture::new();
    let fake = answering_fake(5);
    scored(&fixture, &fake);

    let bodies = request_bodies(&fake);
    let alpha: Vec<&Value> = bodies
        .iter()
        .filter(|body| {
            body["state"]["question"]
                .as_str()
                .unwrap_or_default()
                .contains(ALPHA_TOKEN)
        })
        .collect();
    assert_eq!(alpha.len(), 3);
    for body in &alpha {
        assert!(!body.to_string().contains(BETA_TOKEN), "{body}");
        assert!(
            !precedent_questions(body).is_empty(),
            "an alpha request carried no precedent: {body}"
        );
    }
    let beta = bodies
        .iter()
        .filter(|body| body.to_string().contains(BETA_TOKEN))
        .count();
    assert_eq!(beta, 2, "the token is visible in beta's own requests");
}

#[test]
fn the_corpus_files_are_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::new();
    let fake = answering_fake(5);
    scored(&fixture, &fake);

    let mode = |path: &Path| {
        std::fs::metadata(path)
            .unwrap_or_else(|err| panic!("stat {}: {err}", path.display()))
            .permissions()
            .mode()
            & 0o777
    };
    let dir = fixture.triage_dir();
    assert_eq!(mode(&dir), 0o700, "{}", dir.display());
    for name in ["corpus.jsonl", "scores.jsonl", "report.md"] {
        assert_eq!(mode(&dir.join(name)), 0o600, "{name}");
    }
}

#[test]
fn the_report_names_no_question_text() {
    let fixture = Fixture::new();
    let fake = answering_fake(5);
    let output = scored(&fixture, &fake);

    let report = std::fs::read_to_string(fixture.triage_dir().join("report.md")).expect("report");
    let stdout = text(&output.stdout);
    let corpus =
        std::fs::read_to_string(fixture.triage_dir().join("corpus.jsonl")).expect("corpus");
    for token in [ALPHA_TOKEN, BETA_TOKEN] {
        assert!(
            !report.contains(token),
            "report.md carries {token}:\n{report}"
        );
        assert!(!stdout.contains(token), "stdout carries {token}:\n{stdout}");
        assert!(
            corpus.contains(token),
            "corpus.jsonl lacks {token}, so the check proves nothing"
        );
    }
}

#[test]
fn scoring_without_a_key_is_refused() {
    let fixture = Fixture::new();
    let fake = answering_fake(5);
    let output = fixture.run(&fake, None);

    assert!(!output.status.success(), "the run without a key succeeded");
    let stderr = text(&output.stderr);
    assert!(stderr.contains("TYPESAFE_API_KEY"), "stderr:\n{stderr}");
    assert_eq!(fake.request_count(), 0);
    assert!(
        !fixture.triage_dir().exists(),
        "the refused run wrote {}",
        fixture.triage_dir().display()
    );
    let stdout = text(&output.stdout);
    for line in [
        "mined alpha: event entries 3, log entries 0",
        "mined beta: event entries 0, log entries 2",
    ] {
        assert!(stdout.contains(line), "stdout lacks `{line}`:\n{stdout}");
    }
}

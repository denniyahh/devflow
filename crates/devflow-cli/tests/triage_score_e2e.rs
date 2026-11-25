//! End-to-end tests of `devflow __triage-score` (Phase 54, D-01, D-04, D-05,
//! D-06): synthetic projects are mined, each question is scored by a loopback
//! Jev fake, and a counts-only report lands under the user data dir.
//!
//! No test reaches TypeSafe: the hermetic command drops `TYPESAFE_*`, and each
//! run that scores gets `TYPESAFE_BASE_URL` pointing at a [`FakeJev`].

mod common;

use common::{HermeticHome, hermetic_devflow};
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

    /// Run the tool over both projects, against `fake` when given (with the
    /// test key) and with no key otherwise.
    fn run(&self, fake: Option<&FakeJev>) -> Output {
        let mut command = hermetic_devflow(&self.home);
        command
            .arg("__triage-score")
            .arg("--project")
            .arg(project_arg("alpha", self.alpha.path()))
            .arg("--project")
            .arg(project_arg("beta", self.beta.path()))
            .env("DEVFLOW_DATA_DIR", self.data.path());
        if let Some(fake) = fake {
            command
                .env("TYPESAFE_BASE_URL", fake.url())
                .env("TYPESAFE_API_KEY", TEST_KEY);
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
    let output = fixture.run(Some(fake));
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

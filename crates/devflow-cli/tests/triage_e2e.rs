//! End-to-end tests of decision triage in `devflow __ask` (Phase 54,
//! TRIAGE-01): a question is scored by a loopback Jev fake and either answered
//! at once, with no gate, or sent to the operator's question gate.
//!
//! No test reaches TypeSafe: the hermetic command drops `TYPESAFE_*`, and
//! each server here gets `TYPESAFE_BASE_URL` pointing at a [`FakeJev`] or a
//! refused loopback port.

mod common;

use common::ask_client::{AskClient, AskFixture, AskOverrides};
use devflow_core::stage::Stage;
use devflow_core::test_support::{FakeJev, FakeReply, jev_answer};
use serde_json::{Value, json};
use std::time::Duration;

/// The id of the `devflow_ask` call in these tests.
const CALL_ID: u64 = 3;

/// The bearer key the fake is sent; not a real key.
const TEST_KEY: &str = "test-key-not-secret";

/// The model every fixture configures and every fake answers as.
const MODEL: &str = "jev-1.14.0";

/// How long a reply that must come is waited for.
const REPLY_WAIT: Duration = Duration::from_secs(30);

/// `[triage]` as a fixture's `devflow.toml` carries it.
struct TriageToml {
    threshold: f64,
    pre_check_threshold: f64,
    timeout_secs: u64,
}

impl TriageToml {
    /// Both thresholds at `threshold`, a 2 s model timeout.
    fn at(threshold: f64) -> Self {
        Self {
            threshold,
            pre_check_threshold: threshold,
            timeout_secs: 2,
        }
    }

    fn text(&self) -> String {
        format!(
            "[triage]\nenabled = true\nthreshold = {:?}\npre_check_threshold = {:?}\n\
             model = \"{MODEL}\"\ntimeout_secs = {}\n",
            self.threshold, self.pre_check_threshold, self.timeout_secs
        )
    }
}

/// An ask fixture at Code whose `devflow.toml` turns triage on with `triage`.
fn triage_fixture(triage: &TriageToml) -> AskFixture {
    let fixture = AskFixture::new(Stage::Code);
    std::fs::write(fixture.root().join("devflow.toml"), triage.text())
        .expect("write the fixture's devflow.toml");
    fixture
}

/// The server environment that sends Jev requests to `base_url`.
fn jev_env(base_url: &str) -> Vec<(String, String)> {
    vec![
        ("TYPESAFE_BASE_URL".to_string(), base_url.to_string()),
        ("TYPESAFE_API_KEY".to_string(), TEST_KEY.to_string()),
    ]
}

/// A handshaken `devflow __ask` for `fixture` whose Jev is `base_url`.
fn triage_client(fixture: &AskFixture, base_url: &str, overrides: AskOverrides) -> AskClient {
    let mut client = AskClient::spawn_with(
        fixture,
        AskOverrides {
            env: jev_env(base_url),
            ..overrides
        },
    );
    client.handshake();
    client
}

/// A fake Jev reply scoring option `a` at `score`, its pre-check at `score`.
fn scoring_a(score: f64) -> FakeReply {
    FakeReply::json(jev_answer(
        MODEL,
        score,
        "a",
        &[("a", score), ("b", 1.0 - score)],
    ))
}

/// The two options every question here offers.
fn options() -> Value {
    json!([
        { "id": "a", "label": "Thirty seconds" },
        { "id": "b", "label": "Sixty seconds" },
    ])
}

/// Ask `question` of `kind` under `id`.
fn ask(client: &mut AskClient, id: u64, kind: &str, question: &str) {
    client.request(
        id,
        "tools/call",
        json!({ "name": "devflow_ask", "arguments": {
            "question": question, "options": options(), "kind": kind } }),
    );
}

/// The `question_asked` line of the fixture's only question.
fn only_question_asked(fixture: &AskFixture) -> Value {
    let asked = fixture.events_named("question_asked");
    assert_eq!(asked.len(), 1, "one question_asked: {asked:#?}");
    asked[0].clone()
}

#[test]
fn an_above_threshold_question_is_answered_without_a_gate() {
    let fixture = triage_fixture(&TriageToml::at(0.5));
    let fake = FakeJev::serve(vec![FakeReply::json(jev_answer(
        MODEL,
        0.93,
        "a",
        &[("a", 0.9), ("b", 0.1)],
    ))]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());

    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let reply = client
        .reply(CALL_ID, REPLY_WAIT)
        .expect("triage answers the call");
    let result = &reply["result"];
    assert_ne!(result["isError"], true, "{reply}");
    assert_eq!(
        result["structuredContent"]["answered_by"], "triage:jev",
        "{reply}"
    );
    assert_eq!(
        result["structuredContent"]["answer"]["option_id"], "a",
        "{reply}"
    );
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("DevFlow triage"),
        "the reply names triage: {text}"
    );
    assert!(
        !text.contains("The operator chose"),
        "the reply credits the operator: {text}"
    );
    assert!(!fixture.gate_path().exists(), "a gate file was written");
    let asked = only_question_asked(&fixture);
    assert_eq!(asked["triage"]["outcome"], "decided", "{asked}");
    assert!(
        asked.get("gate_id").is_none(),
        "a gate id was recorded: {asked}"
    );
    assert_eq!(asked["run_started_at"], fixture.started_at(), "{asked}");
    let answered = fixture.events_named("question_answered");
    assert_eq!(answered.len(), 1, "{answered:#?}");
    assert_eq!(answered[0]["policy"], "triage", "{answered:#?}");
    assert_eq!(answered[0]["answered_by"], "triage:jev", "{answered:#?}");
    assert_eq!(fake.request_count(), 1, "one Jev request");
}

#[test]
fn the_hermetic_command_drops_the_typesafe_environment() {
    let fixture = AskFixture::new(Stage::Code);
    let command = fixture.devflow();
    let envs: Vec<_> = command.get_envs().collect();
    for name in ["TYPESAFE_API_KEY", "TYPESAFE_BASE_URL"] {
        let entry = envs
            .iter()
            .find(|(var, _)| *var == name)
            .unwrap_or_else(|| panic!("{name} is not touched by the hermetic command"));
        assert_eq!(entry.1, None, "{name} is removed, not set: {envs:?}");
    }
}

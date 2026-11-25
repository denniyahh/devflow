//! End-to-end tests of triage's fallback in `devflow __ask` (Phase 54,
//! TRIAGE-04, D-19): a model that is missing or fails sends the question to
//! the operator's gate and appends a `triage_fallback` event naming why, and
//! an answering model decides the same fixture.
//!
//! No test reaches TypeSafe: the hermetic command drops `TYPESAFE_*`, and
//! each server here gets `TYPESAFE_BASE_URL` pointing at a [`FakeJev`] or a
//! refused loopback port.

mod common;

use common::ask_client::{AskClient, AskFixture, AskOverrides};
use devflow_core::stage::Stage;
use devflow_core::test_support::refused_url;
use serde_json::{Value, json};
use std::time::Duration;

/// The id of the `devflow_ask` call in these tests.
const CALL_ID: u64 = 3;

/// The bearer key the fake is sent; not a real key.
const TEST_KEY: &str = "test-key-not-secret";

/// The model every fixture configures.
const MODEL: &str = "jev-1.14.0";

/// How long an event or reply that must come is waited for.
const EVENT_WAIT: Duration = Duration::from_secs(30);

/// An ask fixture at Code whose `devflow.toml` turns triage on with both
/// thresholds at 0.5 and a model timeout of `timeout_secs`.
fn triage_fixture(timeout_secs: u64) -> AskFixture {
    let fixture = AskFixture::new(Stage::Code);
    let toml = format!(
        "[triage]\nenabled = true\nthreshold = 0.5\npre_check_threshold = 0.5\n\
         model = \"{MODEL}\"\ntimeout_secs = {timeout_secs}\n"
    );
    std::fs::write(fixture.root().join("devflow.toml"), toml)
        .expect("write the fixture's devflow.toml");
    fixture
}

/// The server environment that sends Jev requests to `base_url` with the
/// test key.
fn jev_env(base_url: &str) -> Vec<(String, String)> {
    vec![
        ("TYPESAFE_BASE_URL".to_string(), base_url.to_string()),
        ("TYPESAFE_API_KEY".to_string(), TEST_KEY.to_string()),
    ]
}

/// A handshaken `devflow __ask` for `fixture` with `overrides`.
fn client_with(fixture: &AskFixture, overrides: AskOverrides) -> AskClient {
    let mut client = AskClient::spawn_with(fixture, overrides);
    client.handshake();
    client
}

/// A handshaken `devflow __ask` for `fixture` whose Jev is `base_url`.
fn triage_client(fixture: &AskFixture, base_url: &str) -> AskClient {
    client_with(
        fixture,
        AskOverrides {
            env: jev_env(base_url),
            ..AskOverrides::default()
        },
    )
}

/// Ask a `decision` question under [`CALL_ID`].
fn ask(client: &mut AskClient, question: &str) {
    client.request(
        CALL_ID,
        "tools/call",
        json!({ "name": "devflow_ask", "arguments": {
            "question": question,
            "options": [
                { "id": "a", "label": "Thirty seconds" },
                { "id": "b", "label": "Sixty seconds" },
            ],
            "kind": "decision" } }),
    );
}

/// The fixture's `triage_fallback` lines, waiting until there are `count`.
fn wait_for_fallbacks(fixture: &AskFixture, count: usize) -> Vec<Value> {
    common::wait_for(
        || fixture.events_named("triage_fallback").len() >= count,
        EVENT_WAIT,
        "the triage_fallback lines",
    );
    fixture.events_named("triage_fallback")
}

/// `question_answered` lines whose policy is `triage`.
fn triage_answers(fixture: &AskFixture) -> Vec<Value> {
    fixture
        .events_named("question_answered")
        .into_iter()
        .filter(|line| line["policy"] == "triage")
        .collect()
}

/// Assert the fixture's one question went to the operator's gate because
/// triage fell back with `reason`, and return its `triage_fallback` line:
/// the gate opened, its `question_asked` names the gate, one fallback names
/// that question and this run, and triage answered nothing.
fn assert_fell_back(fixture: &AskFixture, reason: &str) -> Value {
    let gate_id = fixture.wait_for_gate_id();
    let fallbacks = wait_for_fallbacks(fixture, 1);
    let asked = fixture.events_named("question_asked");
    assert_eq!(asked.len(), 1, "one question_asked: {asked:#?}");
    assert_eq!(asked[0]["gate_id"], gate_id.as_str(), "{asked:#?}");
    assert_eq!(fallbacks.len(), 1, "one triage_fallback: {fallbacks:#?}");
    let fallback = fallbacks[0].clone();
    assert_eq!(fallback["reason"], reason, "{fallback}");
    assert_eq!(
        fallback["question_id"], asked[0]["question_id"],
        "{fallback}"
    );
    assert_eq!(
        fallback["run_started_at"],
        fixture.started_at(),
        "{fallback}"
    );
    assert!(triage_answers(fixture).is_empty(), "triage answered");
    fallback
}

#[test]
fn an_unreachable_endpoint_falls_back_to_the_operator() {
    let fixture = triage_fixture(2);
    let mut client = triage_client(&fixture, &refused_url());
    ask(&mut client, "How long should the budget be?");

    let fallback = assert_fell_back(&fixture, "unreachable");
    assert!(
        fallback["detail"].as_str().is_some_and(|d| !d.is_empty()),
        "a detail names the failure: {fallback}"
    );
}

#[test]
fn a_question_that_never_got_the_ask_lock_writes_no_fallback() {
    let fixture = triage_fixture(2);
    let mut first = triage_client(&fixture, &refused_url());
    ask(&mut first, "First question");
    fixture.wait_for_gate_id();
    let first_fallbacks = wait_for_fallbacks(&fixture, 1);

    let mut second = client_with(
        &fixture,
        AskOverrides {
            timeout_secs: Some(2),
            env: jev_env(&refused_url()),
            ..AskOverrides::default()
        },
    );
    ask(&mut second, "Second question");
    let reply = second
        .reply(CALL_ID, EVENT_WAIT)
        .expect("the waiting call replies");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        text.starts_with("devflow_ask closed: timed_out"),
        "the waiting call timed out: {reply}"
    );

    let asked = fixture.events_named("question_asked");
    assert_eq!(
        asked.len(),
        1,
        "only the first question was asked: {asked:#?}"
    );
    assert_eq!(asked[0]["question"], "First question", "{asked:#?}");
    let fallbacks = fixture.events_named("triage_fallback");
    assert_eq!(
        fallbacks, first_fallbacks,
        "the waiting call wrote a triage_fallback"
    );
    assert_eq!(
        fallbacks[0]["question_id"], asked[0]["question_id"],
        "the first question's fallback: {fallbacks:#?}"
    );
}

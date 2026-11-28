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
use devflow_core::test_support::{FakeJev, FakeReply, jev_answer, refused_url};
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

/// A fake Jev reply as `model`, picking option `a` with probability `score`,
/// its pre-check also at `score`.
fn answer_as(model: &str, score: f64) -> FakeReply {
    FakeReply::json(jev_answer(
        model,
        score,
        "a",
        &[("a", score), ("b", 1.0 - score)],
    ))
}

/// Ask one question of a server whose Jev is a fake serving `reply`, and
/// return the fake once the question's fallback is on the log.
fn fall_back_on(reply: FakeReply, reason: &str) -> (AskFixture, FakeJev, Value) {
    let fixture = triage_fixture(1);
    let fake = FakeJev::serve(vec![reply]);
    let mut client = triage_client(&fixture, &fake.url());
    ask(&mut client, "How long should the budget be?");
    let fallback = assert_fell_back(&fixture, reason);
    (fixture, fake, fallback)
}

#[test]
fn a_missing_api_key_falls_back_to_the_operator() {
    let fixture = triage_fixture(2);
    let fake = FakeJev::serve(vec![answer_as(MODEL, 0.9)]);
    let mut client = client_with(
        &fixture,
        AskOverrides {
            env: vec![("TYPESAFE_BASE_URL".to_string(), fake.url())],
            ..AskOverrides::default()
        },
    );
    ask(&mut client, "How long should the budget be?");

    assert_fell_back(&fixture, "no_api_key");
    assert_eq!(fake.request_count(), 0, "a request was sent without a key");
}

#[test]
fn a_server_error_falls_back_to_the_operator() {
    let (_fixture, _fake, fallback) = fall_back_on(FakeReply::status(500, "{}"), "http_status");
    assert_eq!(
        fallback["detail"], "HTTP 500 unexpected status",
        "{fallback}"
    );
}

#[test]
fn a_rate_limit_falls_back_to_the_operator() {
    let (_fixture, _fake, fallback) = fall_back_on(FakeReply::status(429, "{}"), "http_status");
    assert_eq!(fallback["detail"], "HTTP 429 rate limited", "{fallback}");
}

#[test]
fn a_timeout_falls_back_to_the_operator() {
    let reply = answer_as(MODEL, 0.9).delayed(Duration::from_secs(3));
    let (_fixture, fake, _fallback) = fall_back_on(reply, "timeout");
    assert_eq!(fake.request_count(), 1, "the request was sent");
}

#[test]
fn an_unparsable_response_falls_back_to_the_operator() {
    fall_back_on(FakeReply::json("not json"), "unparsable");
}

#[test]
fn a_response_missing_probabilities_falls_back_to_the_operator() {
    let mut body: Value = serde_json::from_str(&answer_as(MODEL, 0.9).body).expect("JSON");
    body["answers"]["pick"]
        .as_object_mut()
        .expect("the pick answer")
        .remove("probabilities")
        .expect("the fake answer had probabilities");
    let (_fixture, _fake, fallback) = fall_back_on(FakeReply::json(body.to_string()), "unparsable");
    assert!(
        fallback["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("probabilities")),
        "the detail names the missing field: {fallback}"
    );
}

#[test]
fn a_model_mismatch_falls_back_to_the_operator() {
    fall_back_on(answer_as("jev-1.13.0", 0.9), "model_mismatch");
}

#[test]
fn an_answering_model_decides_the_same_fixture() {
    let fixture = triage_fixture(1);
    let fake = FakeJev::serve(vec![answer_as(MODEL, 0.9)]);
    let mut client = triage_client(&fixture, &fake.url());
    ask(&mut client, "How long should the budget be?");

    let reply = client.reply(CALL_ID, EVENT_WAIT).expect("the call replies");
    assert_ne!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(triage_answers(&fixture).len(), 1, "triage answered once");
    assert!(!fixture.gate_path().exists(), "a gate opened");
    assert!(
        fixture.events_named("triage_fallback").is_empty(),
        "a fallback was recorded"
    );
}

/// A string only the fake's error bodies carry.
const PLANTED: &str = "PRIVATE-PRECEDENT-7f3a";

#[test]
fn no_fallback_event_carries_the_key() {
    for status in [500, 429] {
        let body = format!(r#"{{"error": "{PLANTED}", "key": "{TEST_KEY}"}}"#);
        assert!(body.contains(PLANTED) && body.contains(TEST_KEY), "{body}");
        let (fixture, fake, _fallback) =
            fall_back_on(FakeReply::status(status, body), "http_status");
        let requests = fake.requests();
        assert_eq!(requests.len(), 1, "the fake served its body");
        assert!(
            requests[0]
                .header("authorization")
                .is_some_and(|value| value.contains(TEST_KEY)),
            "the key reached the server: {:?}",
            requests[0].headers
        );

        let log = std::fs::read_to_string(devflow_core::events::events_path(fixture.root()))
            .expect("read events.jsonl");
        let status_out = fixture
            .devflow()
            .arg("status")
            .output()
            .expect("run devflow status");
        let printed = String::from_utf8_lossy(&status_out.stdout).into_owned()
            + &String::from_utf8_lossy(&status_out.stderr);
        assert!(
            printed.contains("triage: fell back to the operator 1 time(s)"),
            "status shows the fallback: {printed}"
        );
        for (name, text) in [("events.jsonl", &log), ("status", &printed)] {
            for line in text.lines() {
                assert!(!line.contains(TEST_KEY), "{name} carries the key: {line}");
                assert!(!line.contains(PLANTED), "{name} carries the body: {line}");
            }
        }
    }
}

/// The test key followed by a terminal clear-screen sequence, as an endpoint
/// that echoes its request might return it.
const ECHOED: &str = concat!("test-key-not-secret", "\u{1b}[2J");

/// Assert that neither events.jsonl nor `devflow status` output carries the
/// key or an escape character, and that `status` still reports one fallback
/// for `reason`.
fn assert_log_and_status_hold_no_echo(fixture: &AskFixture, reason: &str) {
    let log = std::fs::read_to_string(devflow_core::events::events_path(fixture.root()))
        .expect("read events.jsonl");
    let status_out = fixture
        .devflow()
        .arg("status")
        .output()
        .expect("run devflow status");
    let printed = String::from_utf8_lossy(&status_out.stdout).into_owned()
        + &String::from_utf8_lossy(&status_out.stderr);
    assert!(
        printed.contains("triage: fell back to the operator 1 time(s)"),
        "status shows the fallback: {printed}"
    );
    assert!(printed.contains(reason), "status names {reason}: {printed}");
    for line in log.lines() {
        assert!(
            !line.contains(TEST_KEY),
            "events.jsonl carries the key: {line}"
        );
        assert!(
            !line.contains("\\u001b[2J"),
            "events.jsonl carries the escape: {line}"
        );
    }
    for line in printed.lines() {
        assert!(!line.contains(TEST_KEY), "status carries the key: {line}");
        assert!(
            !line.contains('\u{1b}'),
            "status carries the escape: {line:?}"
        );
    }
}

/// Assert the fallback's `detail` names `field` and holds neither the key
/// nor an escape character.
fn assert_detail_names_only(fallback: &Value, field: &str) {
    let detail = fallback["detail"].as_str().expect("the detail is a string");
    assert!(detail.contains(field), "the detail names {field}: {detail}");
    assert!(
        !detail.contains(TEST_KEY),
        "the detail carries the key: {detail:?}"
    );
    assert!(
        !detail.contains('\u{1b}'),
        "the detail carries the escape: {detail:?}"
    );
}

#[test]
fn a_response_model_echoing_the_key_stays_out_of_the_log_and_status() {
    assert_eq!(ECHOED, format!("{TEST_KEY}\u{1b}[2J"));
    let reply = answer_as(ECHOED, 0.9);
    assert!(reply.body.contains(TEST_KEY), "the fake echoes the key");
    let (fixture, fake, fallback) = fall_back_on(reply, "model_mismatch");
    assert_eq!(fake.request_count(), 1, "the fake served its body");

    assert_detail_names_only(&fallback, "model");
    assert_detail_names_only(&fallback, MODEL);
    assert_log_and_status_hold_no_echo(&fixture, "model_mismatch");
}

#[test]
fn a_response_choice_echoing_the_key_stays_out_of_the_log_and_status() {
    let reply = FakeReply::json(jev_answer(MODEL, 0.9, ECHOED, &[("a", 0.5), ("b", 0.5)]));
    assert!(reply.body.contains(TEST_KEY), "the fake echoes the key");
    let (fixture, fake, fallback) = fall_back_on(reply, "unparsable");
    assert_eq!(fake.request_count(), 1, "the fake served its body");

    assert_detail_names_only(&fallback, "answers.pick.choice");
    assert_log_and_status_hold_no_echo(&fixture, "unparsable");
}

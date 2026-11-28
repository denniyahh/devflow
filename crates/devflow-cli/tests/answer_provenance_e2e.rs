//! End-to-end provenance of a gate answer (#343): an answer given with
//! `DEVFLOW_ANSWERED_BY=agent:<name>` is recorded as that agent's in the tool
//! reply, the `question_answered` event and `devflow history`; the same answer
//! without the marker is recorded as the operator's. The marker is set on the
//! spawned `devflow` child only, never on this process.

mod common;

use common::ask_client::{AskClient, AskFixture};
use devflow_core::stage::Stage;
use serde_json::Value;
use std::process::Command;
use std::time::{Duration, Instant};

/// How long a reply or event that must come is waited for.
const REPLY_WAIT: Duration = Duration::from_secs(30);

/// The id of the `devflow_ask` call in these tests.
const CALL_ID: u64 = 3;

/// The marker variable a delegated agent session sets.
const ANSWERED_BY_ENV: &str = "DEVFLOW_ANSWERED_BY";

/// Ask a question, answer it with `configure` applied to the `gate answer`
/// child, and return the reply's `structuredContent` and the
/// `question_answered` event.
fn answer_with(fixture: &AskFixture, configure: impl FnOnce(&mut Command)) -> (Value, Value) {
    let mut client = AskClient::spawn(fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &["a", "b"]);
    let gate_id = fixture.wait_for_gate_id();

    let mut answer = fixture.devflow();
    answer.args([
        "gate",
        "answer",
        "1",
        "--gate-id",
        &gate_id,
        "--option",
        "a",
    ]);
    configure(&mut answer);
    let output = answer.output().expect("run devflow gate answer");
    assert!(
        output.status.success(),
        "devflow gate answer failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let reply = client
        .reply(CALL_ID, REPLY_WAIT)
        .expect("the call replies once the gate is answered");
    let content = reply["result"]["structuredContent"].clone();
    (content, wait_for_answered_event(fixture))
}

/// The one `question_answered` event, waiting for it to be logged.
fn wait_for_answered_event(fixture: &AskFixture) -> Value {
    let started = Instant::now();
    loop {
        let found = fixture.events_named("question_answered");
        if let [one] = found.as_slice() {
            return one.clone();
        }
        assert!(
            found.is_empty() && started.elapsed() < REPLY_WAIT,
            "expected one question_answered within {REPLY_WAIT:?}: {:?}",
            fixture.events()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// `devflow history 1`'s stdout, asserting it exited 0.
fn history(fixture: &AskFixture) -> String {
    let output = fixture
        .devflow()
        .args(["history", "1"])
        .output()
        .expect("run devflow history");
    assert!(
        output.status.success(),
        "devflow history failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn an_agent_marked_answer_records_the_agent() {
    let fixture = AskFixture::new(Stage::Code);
    let (content, event) = answer_with(&fixture, |command| {
        command.env(ANSWERED_BY_ENV, "agent:claude");
    });

    assert_eq!(content["answered_by"], "agent:claude", "reply: {content}");
    assert_eq!(event["answered_by"], "agent:claude", "event: {event}");
    let shown = history(&fixture);
    assert!(shown.contains("by agent:claude"), "history: {shown}");
}

#[test]
fn an_unmarked_answer_records_the_operator() {
    let fixture = AskFixture::new(Stage::Code);
    let (content, event) = answer_with(&fixture, |command| {
        command
            .env_remove(ANSWERED_BY_ENV)
            .env("USER", "operator-fixture");
    });

    assert_eq!(
        content["answered_by"], "operator-fixture",
        "reply: {content}"
    );
    assert_eq!(event["answered_by"], "operator-fixture", "event: {event}");
}

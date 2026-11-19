//! Server-direct e2e tests of `devflow __ask` (Phase 53, ASK-01, ASK-02): a
//! client process asks through `devflow_ask`, the operator answers the question
//! gate from another process, and the same client gets the option back.

mod common;

use common::ask_client::{AskClient, AskFixture};
use devflow_core::gates::{GateId, Gates};
use devflow_core::stage::Stage;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a reply that must not come is watched for after the gate opens.
const NO_REPLY_WATCH: Duration = Duration::from_secs(2);

/// How long a reply that must come is waited for once the gate is answered.
const REPLY_WAIT: Duration = Duration::from_secs(30);

/// The id of the `devflow_ask` call in these tests.
const CALL_ID: u64 = 3;

const OPTIONS: [&str; 3] = ["budget-30", "budget-120", "budget-300"];

/// `devflow gate <args>` in the fixture root, asserting it exited 0.
fn gate(fixture: &AskFixture, args: &[&str]) {
    let output = fixture
        .devflow()
        .arg("gate")
        .args(args)
        .output()
        .expect("run devflow gate");
    assert!(
        output.status.success(),
        "devflow gate {args:?} failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The option the reply to the call carries.
fn chosen_option(client: &AskClient) -> String {
    let reply = client
        .reply(CALL_ID, REPLY_WAIT)
        .expect("the call replies once the gate is answered");
    reply["result"]["structuredContent"]["answer"]["option_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no option_id in the reply: {reply}"))
        .to_string()
}

#[test]
fn a_call_blocks_until_answered_and_returns_the_option() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    let gate_id = fixture.wait_for_gate_id();
    assert!(
        client.reply(CALL_ID, NO_REPLY_WATCH).is_none(),
        "reply arrived before the gate was answered"
    );

    gate(
        &fixture,
        &[
            "answer",
            "1",
            "--gate-id",
            &gate_id,
            "--option",
            "budget-120",
        ],
    );

    let reply = client
        .reply(CALL_ID, REPLY_WAIT)
        .expect("the call replies once the gate is answered");
    let answer = &reply["result"]["structuredContent"];
    assert_eq!(
        answer["answer"]["option_id"], "budget-120",
        "option_id: {reply}"
    );
    for field in ["question_id", "answered_by", "answered_at"] {
        assert!(
            answer[field]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "{field} is a non-empty string: {reply}"
        );
    }
    assert!(
        !fixture.gate_path().exists(),
        "the question's gate file is gone once its answer is returned"
    );
}

#[test]
fn gate_approve_on_a_question_gate_is_refused_and_writes_no_answer() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    let gate_id = fixture.wait_for_gate_id();

    let approve = fixture
        .devflow()
        .args(["gate", "approve", "1", "--gate-id", &gate_id])
        .output()
        .expect("run devflow gate approve");
    let slot = Gates::answer_path(
        fixture.root(),
        fixture.phase(),
        fixture.stage(),
        &GateId::parse(&gate_id).expect("the gate id parses"),
    );
    assert!(!slot.exists(), "approve wrote an answer slot");
    let stderr = String::from_utf8_lossy(&approve.stderr);
    assert!(
        !approve.status.success(),
        "gate approve was refused: {stderr}"
    );
    assert!(
        stderr.contains("gate answer"),
        "the refusal names `gate answer`: {stderr}"
    );
    assert!(
        client.reply(CALL_ID, NO_REPLY_WATCH).is_none(),
        "reply arrived before the gate was answered"
    );

    // Control: the right verb resolves the same question.
    gate(
        &fixture,
        &[
            "answer",
            "1",
            "--gate-id",
            &gate_id,
            "--option",
            "budget-30",
        ],
    );
    assert_eq!(chosen_option(&client), "budget-30");
}

/// Send a `devflow_ask` call with `arguments` and return its reply, asserting
/// at every poll until the reply that no gate file has appeared.
fn refused_call(fixture: &AskFixture, client: &mut AskClient, arguments: Value) -> Value {
    client.request(
        CALL_ID,
        "tools/call",
        json!({ "name": "devflow_ask", "arguments": arguments }),
    );
    let gates_dir = fixture
        .gate_path()
        .parent()
        .expect("a gates dir")
        .to_path_buf();
    let started = Instant::now();
    loop {
        let entries: Vec<_> = std::fs::read_dir(&gates_dir)
            .map(|dir| {
                dir.filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            entries.is_empty(),
            "a gate file appeared for a malformed call: {entries:?}"
        );
        if let Some(reply) = client.reply(CALL_ID, Duration::ZERO) {
            return reply;
        }
        assert!(
            started.elapsed() < REPLY_WAIT,
            "no reply to a malformed call within {REPLY_WAIT:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Assert `reply` is a tool result with `isError: true` whose text names `field`.
fn assert_refused_naming(reply: &Value, field: &str) {
    assert_eq!(reply["result"]["isError"], true, "isError: {reply}");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in the refusal: {reply}"));
    assert!(text.contains(field), "the refusal names `{field}`: {text}");
}

#[test]
fn a_malformed_call_returns_is_error_and_opens_no_gate() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    let reply = refused_call(
        &fixture,
        &mut client,
        json!({ "question": "Pick a budget", "options": [], "kind": "decision" }),
    );
    assert_refused_naming(&reply, "options");

    client.request(4, "ping", json!({}));
    let ping = client.reply(4, REPLY_WAIT);
    assert!(ping.is_some(), "ping not answered after a refusal");

    // Control: the same call with one option opens a gate.
    client.ask(5, "Pick a budget", &OPTIONS[..1]);
    fixture.wait_for_gate_id();
}

#[test]
fn a_call_offering_a_recommended_id_not_in_options_is_refused() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    let reply = refused_call(
        &fixture,
        &mut client,
        json!({
            "question": "Pick a budget",
            "options": [{ "id": "budget-30", "label": "thirty" }],
            "kind": "decision",
            "recommended": "budget-999",
        }),
    );
    assert_refused_naming(&reply, "recommended");
}

/// A gate notify command that writes the hook's `DEVFLOW_GATE_CONTEXT` to
/// `marker`.
fn marker_hook(marker: &std::path::Path) -> String {
    format!(
        "printf '%s' \"$DEVFLOW_GATE_CONTEXT\" > '{}'",
        marker.display()
    )
}

/// Wait until `event` is on the fixture's log, returning its lines.
fn wait_for_event(fixture: &AskFixture, event: &str) -> Vec<Value> {
    let started = Instant::now();
    loop {
        let found = fixture.events_named(event);
        if !found.is_empty() {
            return found;
        }
        assert!(
            started.elapsed() < REPLY_WAIT,
            "no `{event}` on events.jsonl within {REPLY_WAIT:?}: {:?}",
            fixture.events()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The index of the first `event` line on the fixture's log.
fn position_of(fixture: &AskFixture, event: &str) -> usize {
    fixture
        .events()
        .iter()
        .position(|line| line["event"] == event)
        .unwrap_or_else(|| panic!("no `{event}` on events.jsonl"))
}

#[test]
fn a_question_fires_the_notify_hook_and_records_notify_fired() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let marker = scratch.path().join("notified");
    let fixture = AskFixture::new(Stage::Code);
    fixture.edit_state(|state| state.gate_notify_cmd = Some(marker_hook(&marker)));
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < REPLY_WAIT {
        std::thread::sleep(Duration::from_millis(25));
    }
    let notified = std::fs::read_to_string(&marker).expect("notify marker written by the hook");
    assert!(
        notified.contains("Pick a budget"),
        "the notify marker holds the question: {notified:?}"
    );
    let fired = wait_for_event(&fixture, "gate_fired");
    assert_eq!(fired[0]["gate_kind"], "question", "gate_kind: {fired:?}");
    wait_for_event(&fixture, "notify_fired");
    assert!(
        position_of(&fixture, "gate_fired") < position_of(&fixture, "notify_fired"),
        "gate_fired precedes notify_fired: {:?}",
        fixture.events()
    );
    drop(client);

    // Control: with no notify command the hook writes nothing, and both
    // events are still recorded.
    let scratch = tempfile::tempdir().expect("scratch dir");
    let marker = scratch.path().join("notified");
    let control = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&control);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    control.wait_for_gate_id();
    wait_for_event(&control, "gate_fired");
    wait_for_event(&control, "notify_fired");
    assert!(!marker.exists(), "no hook ran, so no notify marker");
}

#[test]
fn question_events_record_the_question_and_its_answer() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    let gate_id = fixture.wait_for_gate_id();
    gate(
        &fixture,
        &[
            "answer",
            "1",
            "--gate-id",
            &gate_id,
            "--option",
            "budget-120",
        ],
    );
    assert_eq!(chosen_option(&client), "budget-120");

    let asked = wait_for_event(&fixture, "question_asked");
    assert_eq!(asked.len(), 1, "one question_asked: {asked:?}");
    let question_id = asked[0]["question_id"].as_str().expect("a question_id");
    let offered: Vec<&str> = asked[0]["options"]
        .as_array()
        .expect("options is an array")
        .iter()
        .map(|option| option["id"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        offered, OPTIONS,
        "option order as in the payload: {asked:?}"
    );
    assert_eq!(asked[0]["gate_id"], gate_id.as_str(), "{asked:?}");
    assert_eq!(asked[0]["kind"], "decision", "{asked:?}");

    let answered = wait_for_event(&fixture, "question_answered");
    assert_eq!(answered.len(), 1, "one question_answered: {answered:?}");
    assert_eq!(answered[0]["question_id"], question_id, "{answered:?}");
    assert_eq!(
        answered[0]["policy"], "operator_via_gate",
        "policy: {answered:?}"
    );
    assert_eq!(
        answered[0]["answer"]["option_id"], "budget-120",
        "{answered:?}"
    );
    assert!(
        answered[0]["answered_by"]
            .as_str()
            .is_some_and(|by| !by.is_empty()),
        "answered_by: {answered:?}"
    );
    let closed: Vec<Value> = fixture
        .events_named("question_closed")
        .into_iter()
        .filter(|line| line["question_id"] == question_id)
        .collect();
    assert!(
        closed.is_empty(),
        "question_closed recorded for an answered question: {closed:?}"
    );
}

#[test]
fn a_hook_that_never_exits_does_not_delay_the_question() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let marker = scratch.path().join("hook-started");
    let fixture = AskFixture::new(Stage::Code);
    fixture.edit_state(|state| {
        state.gate_notify_cmd = Some(format!(
            "printf started > '{}'; sleep 600",
            marker.display()
        ));
    });
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    let asked_at = Instant::now();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    let gate_id = fixture.wait_for_gate_id();
    assert!(
        asked_at.elapsed() < Duration::from_secs(5),
        "the gate took {:?} to appear",
        asked_at.elapsed()
    );
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(marker.exists(), "the notify hook was started");

    gate(
        &fixture,
        &[
            "answer",
            "1",
            "--gate-id",
            &gate_id,
            "--option",
            "budget-30",
        ],
    );
    let answered_at = Instant::now();
    let reply = client.reply(CALL_ID, Duration::from_secs(5));
    assert!(
        reply.is_some(),
        "a call not resolved while the hook still sleeps (waited {:?})",
        answered_at.elapsed()
    );
}

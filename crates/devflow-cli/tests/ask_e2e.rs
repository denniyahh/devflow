//! Server-direct e2e tests of `devflow __ask` (Phase 53, ASK-01, ASK-02): a
//! client process asks through `devflow_ask`, the operator answers the question
//! gate from another process, and the same client gets the option back.

mod common;

use common::ask_client::{AskClient, AskFixture, AskOverrides};
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
/// `marker`. It writes a temp file beside the marker and renames it into
/// place, so a reader polling `marker.exists()` never sees it half-written.
fn marker_hook(marker: &std::path::Path) -> String {
    marker_hook_with_delay(marker, 0)
}

/// [`marker_hook`] with `delay_secs` of sleep between the temp file's
/// creation and the write, so a reader that could see an unpublished marker
/// would read it empty.
fn marker_hook_with_delay(marker: &std::path::Path, delay_secs: u32) -> String {
    let marker = marker.display();
    format!(
        "{{ sleep {delay_secs}; printf '%s' \"$DEVFLOW_GATE_CONTEXT\"; }} > '{marker}.tmp' \
         && mv '{marker}.tmp' '{marker}'"
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

/// How long an ending has to be observed after its trigger.
const ENDING_WAIT: Duration = Duration::from_secs(5);

/// Wait until the fixture's gate file is gone.
fn wait_for_gate_gone(fixture: &AskFixture) {
    let started = Instant::now();
    while fixture.gate_path().exists() {
        assert!(
            started.elapsed() < ENDING_WAIT,
            "the gate file still present {ENDING_WAIT:?} after the ending's trigger"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The `question_closed` reasons recorded so far.
fn closed_reasons(fixture: &AskFixture) -> Vec<String> {
    fixture
        .events_named("question_closed")
        .iter()
        .map(|line| line["reason"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Wait until a `question_closed` with `reason` is recorded.
fn wait_for_closed_reason(fixture: &AskFixture, reason: &str) {
    let started = Instant::now();
    while !closed_reasons(fixture).iter().any(|found| found == reason) {
        assert!(
            started.elapsed() < ENDING_WAIT,
            "no question_closed reason `{reason}`; recorded: {:?}",
            closed_reasons(fixture)
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The text of `reply`'s tool result, asserting it is an error.
fn error_text(reply: &Value) -> String {
    assert_eq!(reply["result"]["isError"], true, "isError: {reply}");
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in the reply: {reply}"))
        .to_string()
}

#[test]
fn a_cancelled_call_closes_its_gate_and_records_question_closed() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();

    // Control: a string id is another request, so the numeric call stays held.
    client.cancel(json!("3"));
    assert!(
        client.reply(CALL_ID, NO_REPLY_WATCH).is_none(),
        "a string-id cancel ended the numeric call"
    );
    assert!(
        fixture.gate_path().exists(),
        "a string-id cancel closed the numeric call's gate"
    );

    client.cancel(json!(CALL_ID));
    wait_for_gate_gone(&fixture);
    wait_for_closed_reason(&fixture, "cancelled_by_client");
    assert!(
        client.reply(CALL_ID, NO_REPLY_WATCH).is_none(),
        "a reply sent for a cancelled id"
    );
}

#[test]
fn stdin_eof_closes_the_gate() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    client.close_stdin();
    wait_for_gate_gone(&fixture);
    wait_for_closed_reason(&fixture, "client_disconnected");
    let status = client
        .wait_exit(ENDING_WAIT)
        .expect("the server exits once its stdin ends");
    assert!(status.success(), "a non-zero server exit on EOF: {status}");
}

#[test]
fn a_timed_out_call_returns_closed_timed_out() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn_with(
        &fixture,
        AskOverrides {
            timeout_secs: Some(2),
            ..AskOverrides::default()
        },
    );
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    let reply = client
        .reply(CALL_ID, ENDING_WAIT)
        .expect("a timed-out call replies");
    let text = error_text(&reply);
    assert!(
        text.starts_with("devflow_ask closed: timed_out"),
        "the `devflow_ask closed:` prefix: {text}"
    );
    assert!(!fixture.gate_path().exists(), "the gate file still present");
    wait_for_closed_reason(&fixture, "timed_out");
}

#[test]
fn a_stopped_run_closes_the_question() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    fixture.edit_state(|state| state.stopped = true);
    let reply = client
        .reply(CALL_ID, ENDING_WAIT)
        .expect("a stopped run's question replies");
    let text = error_text(&reply);
    assert!(
        text.starts_with("devflow_ask closed: run_stopped"),
        "the `devflow_ask closed:` prefix: {text}"
    );
    wait_for_gate_gone(&fixture);
    wait_for_closed_reason(&fixture, "run_stopped");
}

#[test]
fn sigterm_closes_the_gate_and_records_terminated() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    client.signal(libc::SIGTERM);
    wait_for_gate_gone(&fixture);
    wait_for_closed_reason(&fixture, "terminated");
}

#[test]
fn a_call_with_no_live_runner_is_refused_and_opens_no_gate() {
    let mut fixture = AskFixture::new(Stage::Code);
    fixture.release_runner_lock();
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    let arguments = json!({ "question": "Pick a budget",
        "options": [{ "id": "budget-30", "label": "30 s" }], "kind": "decision" });
    let text = error_text(&refused_call(&fixture, &mut client, arguments.clone()));
    assert!(
        text.starts_with("devflow_ask refused:") && text.contains("runner"),
        "the `devflow_ask refused:` prefix, naming the runner: {text}"
    );
    drop(client);

    // A client bound to another run's started_at is refused the same way.
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn_with(
        &fixture,
        AskOverrides {
            run_started_at: Some("1".to_string()),
            ..AskOverrides::default()
        },
    );
    client.handshake();
    let text = error_text(&refused_call(&fixture, &mut client, arguments));
    assert!(
        text.starts_with("devflow_ask refused:"),
        "the `devflow_ask refused:` prefix: {text}"
    );
    drop(client);

    // Control: the fixture's own started_at and live runner open a gate.
    let control = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&control);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    control.wait_for_gate_id();
}

#[test]
fn a_late_server_cleanup_leaves_the_runners_replacement_gate() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    let (failure, _waiter) = Gates::open_registered(
        fixture.root(),
        fixture.phase(),
        fixture.stage(),
        devflow_core::gates::GateKind::StageFailure,
        "the agent exited",
    )
    .expect("open the runner's failure gate");
    client.close_stdin();
    wait_for_closed_reason(&fixture, "client_disconnected");
    client.wait_exit(ENDING_WAIT).expect("the server exits");
    assert_eq!(
        fixture.wait_for_gate_id(),
        failure.gate_id.as_str(),
        "the replacement gate removed or replaced"
    );
}

/// The id of a second `devflow_ask` call.
const SECOND_ID: u64 = 4;

/// The id of a `ping` sent to order a test after the lines before it.
const PING_ID: u64 = 9;

/// The stage's open gate id, if its gate file exists.
fn open_gate_id(fixture: &AskFixture) -> Option<String> {
    let text = std::fs::read_to_string(fixture.gate_path()).ok()?;
    let gate: Value = serde_json::from_str(&text).ok()?;
    gate["gate_id"].as_str().map(str::to_string)
}

/// Answer the question gate `gate_id` with `option`.
fn answer(fixture: &AskFixture, gate_id: &str, option: &str) {
    gate(
        fixture,
        &["answer", "1", "--gate-id", gate_id, "--option", option],
    );
}

/// The option `reply` carries.
fn option_of(reply: &Value) -> &str {
    reply["result"]["structuredContent"]["answer"]["option_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no option_id in the reply: {reply}"))
}

/// Watch for `NO_REPLY_WATCH` that the stage's gate stays `first` and only one
/// question has been asked.
fn assert_only_the_first_gate_stays(fixture: &AskFixture, first: &str) {
    let started = Instant::now();
    while started.elapsed() < NO_REPLY_WATCH {
        let open = open_gate_id(fixture);
        assert_eq!(
            open.as_deref(),
            Some(first),
            "second gate opened early: the stage's gate is now {open:?}"
        );
        let asked = fixture.events_named("question_asked").len();
        assert_eq!(asked, 1, "two gates open: {asked} question_asked events");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait for a gate other than `previous` to open, and return its id.
fn wait_for_next_gate_id(fixture: &AskFixture, previous: &str) -> String {
    let started = Instant::now();
    loop {
        if let Some(open) = open_gate_id(fixture)
            && open != previous
        {
            return open;
        }
        assert!(
            started.elapsed() < ENDING_WAIT,
            "no gate opened after `{previous}` closed"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Answer the queued question's gate and check each reply carries its own answer.
fn answer_both_in_turn(fixture: &AskFixture, first: &str, replies: [(&AskClient, u64); 2]) {
    answer(fixture, first, "budget-120");
    let [(first_client, first_id), (second_client, second_id)] = replies;
    let reply = first_client
        .reply(first_id, REPLY_WAIT)
        .expect("the first call replies once answered");
    assert_eq!(option_of(&reply), "budget-120", "{reply}");
    let second = wait_for_next_gate_id(fixture, first);
    assert_ne!(second, first, "a reused gate id");
    answer(fixture, &second, "budget-300");
    let reply = second_client
        .reply(second_id, REPLY_WAIT)
        .expect("the second call replies once answered");
    assert_eq!(option_of(&reply), "budget-300", "{reply}");
}

#[test]
fn a_second_call_waits_for_the_first_question_to_close() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "First question", &OPTIONS);
    let first = fixture.wait_for_gate_id();
    client.ask(SECOND_ID, "Second question", &OPTIONS);
    assert_only_the_first_gate_stays(&fixture, &first);
    answer_both_in_turn(&fixture, &first, [(&client, CALL_ID), (&client, SECOND_ID)]);
}

#[test]
fn a_second_server_on_the_same_stage_waits_for_the_ask_lock() {
    let fixture = AskFixture::new(Stage::Code);
    let mut first_client = AskClient::spawn(&fixture);
    first_client.handshake();
    first_client.ask(CALL_ID, "First question", &OPTIONS);
    let first = fixture.wait_for_gate_id();
    let mut second_client = AskClient::spawn(&fixture);
    second_client.handshake();
    second_client.ask(CALL_ID, "Second question", &OPTIONS);
    assert_only_the_first_gate_stays(&fixture, &first);
    answer_both_in_turn(
        &fixture,
        &first,
        [(&first_client, CALL_ID), (&second_client, CALL_ID)],
    );
}

#[test]
fn a_cancelled_queued_call_never_opens_a_gate() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn(&fixture);
    client.handshake();
    client.ask(CALL_ID, "First question", &OPTIONS);
    let first = fixture.wait_for_gate_id();
    client.ask(SECOND_ID, "Second question", &OPTIONS);
    client.cancel(json!(SECOND_ID));
    // The reader handles lines in order: once the ping is answered, the cancel was read.
    client.request(PING_ID, "ping", json!({}));
    client
        .reply(PING_ID, ENDING_WAIT)
        .expect("the ping is answered");
    answer(&fixture, &first, "budget-120");
    let reply = client
        .reply(CALL_ID, REPLY_WAIT)
        .expect("the first call replies once answered");
    assert_eq!(option_of(&reply), "budget-120", "{reply}");
    assert!(
        client.reply(SECOND_ID, NO_REPLY_WATCH).is_none(),
        "a reply sent for the cancelled queued call"
    );
    let asked = fixture.events_named("question_asked");
    assert_eq!(
        asked.len(),
        1,
        "a question_asked for the cancelled queued call: {asked:?}"
    );
    assert!(
        !fixture.gate_path().exists(),
        "the cancelled queued call opened a gate"
    );
}

#[test]
fn a_queued_call_whose_time_runs_out_opens_no_gate() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn_with(
        &fixture,
        AskOverrides {
            timeout_secs: Some(2),
            ..AskOverrides::default()
        },
    );
    client.handshake();
    let sent = Instant::now();
    client.ask(CALL_ID, "First question", &OPTIONS);
    client.ask(SECOND_ID, "Second question", &OPTIONS);
    for id in [CALL_ID, SECOND_ID] {
        let reply = client.reply(id, ENDING_WAIT).expect("each call replies");
        let text = error_text(&reply);
        assert!(
            text.starts_with("devflow_ask closed: timed_out"),
            "call {id}: the `devflow_ask closed:` prefix: {text}"
        );
    }
    let elapsed = sent.elapsed();
    assert!(
        elapsed < Duration::from_secs(4),
        "a queued call still waiting after its deadline: both replies took {elapsed:?}"
    );
    let asked = fixture.events_named("question_asked");
    assert_eq!(
        asked.len(),
        1,
        "a question_asked for the timed-out queued call: {asked:?}"
    );
    assert!(!fixture.gate_path().exists(), "a second gate file appeared");
}

#[test]
fn a_call_waiting_on_another_servers_ask_lock_times_out() {
    let fixture = AskFixture::new(Stage::Code);
    let mut first_client = AskClient::spawn(&fixture);
    first_client.handshake();
    first_client.ask(CALL_ID, "First question", &OPTIONS);
    let first = fixture.wait_for_gate_id();
    let mut second_client = AskClient::spawn_with(
        &fixture,
        AskOverrides {
            timeout_secs: Some(2),
            ..AskOverrides::default()
        },
    );
    second_client.handshake();
    let sent = Instant::now();
    second_client.ask(CALL_ID, "Second question", &OPTIONS);
    let reply = second_client
        .reply(CALL_ID, Duration::from_secs(4))
        .unwrap_or_else(|| panic!("no reply within 4 s ({:?})", sent.elapsed()));
    let text = error_text(&reply);
    assert!(
        text.starts_with("devflow_ask closed: timed_out"),
        "the `devflow_ask closed:` prefix: {text}"
    );
    assert_eq!(
        open_gate_id(&fixture).as_deref(),
        Some(first.as_str()),
        "the waiting call opened a gate"
    );
    let asked = fixture.events_named("question_asked");
    assert_eq!(
        asked.len(),
        1,
        "a question_asked for the call that never got the ask lock: {asked:?}"
    );
}

/// The id of a re-call of a question that came back `pending`.
const RECALL_ID: u64 = 4;

/// The arguments [`AskClient::ask`] sends for `OPTIONS`, with `question_id`
/// added when given.
fn ask_arguments(question_id: Option<&str>) -> Value {
    let options: Vec<Value> = OPTIONS
        .iter()
        .map(|option| json!({ "id": option, "label": format!("label of {option}") }))
        .collect();
    let mut arguments =
        json!({ "question": "Pick a budget", "options": options, "kind": "decision" });
    if let Some(question_id) = question_id {
        arguments["question_id"] = json!(question_id);
    }
    arguments
}

/// Call `devflow_ask` under `id` with the arguments of the first call plus `question_id`.
fn recall(client: &mut AskClient, id: u64, question_id: &str) {
    client.request(
        id,
        "tools/call",
        json!({ "name": "devflow_ask", "arguments": ask_arguments(Some(question_id)) }),
    );
}

/// A capped server for the fixture's run.
fn capped_client(fixture: &AskFixture, hold_cap_secs: u64, timeout_secs: u64) -> AskClient {
    AskClient::spawn_with(
        fixture,
        AskOverrides {
            hold_cap_secs: Some(hold_cap_secs),
            timeout_secs: Some(timeout_secs),
            ..AskOverrides::default()
        },
    )
}

/// The question id of a `pending` reply, asserting it is a non-error pending result.
fn pending_question_id(reply: &Value) -> String {
    assert_ne!(
        reply["result"]["isError"], true,
        "a pending reply is not an error: {reply}"
    );
    let structured = &reply["result"]["structuredContent"];
    assert_eq!(structured["status"], "pending", "status: {reply}");
    structured["question_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| panic!("no question_id in the pending reply: {reply}"))
        .to_string()
}

#[test]
fn a_capped_call_returns_pending_and_a_recall_gets_the_answer() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = capped_client(&fixture, 2, 60);
    client.handshake();
    let sent = Instant::now();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    let gate_id = fixture.wait_for_gate_id();
    let pending = client
        .reply(CALL_ID, Duration::from_secs(10))
        .expect("a capped call replies at its cap");
    let waited = sent.elapsed();
    assert!(
        waited >= Duration::from_millis(1500),
        "the pending reply came before the 2 s cap: {waited:?}"
    );
    let question_id = pending_question_id(&pending);
    assert_eq!(
        open_gate_id(&fixture).as_deref(),
        Some(gate_id.as_str()),
        "the question's gate changed or closed at the cap"
    );

    // The operator answers while no call is held.
    answer(&fixture, &gate_id, "budget-120");
    let recalled = Instant::now();
    recall(&mut client, RECALL_ID, &question_id);
    let reply = client
        .reply(RECALL_ID, REPLY_WAIT)
        .expect("the re-call replies with the answer");
    let took = recalled.elapsed();
    assert!(
        took < Duration::from_secs(5),
        "the re-call waited for an answer already given: {took:?}"
    );
    assert_eq!(option_of(&reply), "budget-120", "{reply}");
    assert_eq!(
        reply["result"]["structuredContent"]["question_id"],
        question_id.as_str(),
        "the answer is for the same question: {reply}"
    );
    let count = |event: &str| fixture.events_named(event).len();
    assert_eq!(count("question_asked"), 1, "question_asked events");
    assert_eq!(count("gate_fired"), 1, "gate_fired events");
    assert_eq!(count("question_answered"), 1, "question_answered events");
    assert_eq!(
        count("question_closed"),
        0,
        "a question_closed for an answered question"
    );
}

#[test]
fn a_pending_question_times_out_without_a_held_call() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = capped_client(&fixture, 1, 3);
    client.handshake();
    let sent = Instant::now();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    fixture.wait_for_gate_id();
    let pending = client
        .reply(CALL_ID, Duration::from_secs(3))
        .expect("a capped call replies at its cap");
    let question_id = pending_question_id(&pending);

    while fixture.gate_path().exists() {
        assert!(
            sent.elapsed() < Duration::from_secs(5),
            "a pending question still open 5 s after its first call (T is 3 s)"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    wait_for_closed_reason(&fixture, "timed_out");

    recall(&mut client, RECALL_ID, &question_id);
    let reply = client
        .reply(RECALL_ID, ENDING_WAIT)
        .expect("a re-call of a closed question replies");
    let text = error_text(&reply);
    assert!(
        text.starts_with("devflow_ask closed: timed_out"),
        "the `devflow_ask closed:` prefix: {text}"
    );
}

#[test]
fn a_recall_with_an_unknown_question_id_is_refused() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = capped_client(&fixture, 2, 60);
    client.handshake();
    let unknown = "0123456789abcdef";
    recall(&mut client, RECALL_ID, unknown);
    let reply = client
        .reply(RECALL_ID, ENDING_WAIT)
        .expect("a re-call with an unknown id replies");
    let text = error_text(&reply);
    assert!(
        text.starts_with("devflow_ask refused:"),
        "the `devflow_ask refused:` prefix: {text}"
    );
    assert!(text.contains(unknown), "the refusal names the id: {text}");
    assert!(!fixture.gate_path().exists(), "an unknown id opened a gate");
    assert!(
        fixture.events_named("question_asked").is_empty(),
        "an unknown id recorded a question"
    );
}

#[test]
fn an_uncapped_call_holds_past_a_cap_value() {
    let fixture = AskFixture::new(Stage::Code);
    let mut client = AskClient::spawn_with(
        &fixture,
        AskOverrides {
            timeout_secs: Some(60),
            ..AskOverrides::default()
        },
    );
    client.handshake();
    client.ask(CALL_ID, "Pick a budget", &OPTIONS);
    let gate_id = fixture.wait_for_gate_id();
    assert!(
        client.reply(CALL_ID, Duration::from_secs(4)).is_none(),
        "an uncapped call replied before its question was answered"
    );
    assert_eq!(
        open_gate_id(&fixture).as_deref(),
        Some(gate_id.as_str()),
        "the question is still open"
    );
    answer(&fixture, &gate_id, "budget-300");
    let reply = client
        .reply(CALL_ID, REPLY_WAIT)
        .expect("the held call replies once answered");
    assert_eq!(option_of(&reply), "budget-300", "{reply}");
}

#[test]
fn the_marker_hook_publishes_the_marker_whole() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let marker = scratch.path().join("notified");
    let mut hook = std::process::Command::new("sh")
        .arg("-c")
        .arg(marker_hook_with_delay(&marker, 1))
        .env("DEVFLOW_GATE_CONTEXT", "Pick a budget")
        .spawn()
        .expect("spawn the marker hook");
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < REPLY_WAIT {
        std::thread::sleep(Duration::from_millis(10));
    }
    let notified = std::fs::read_to_string(&marker).expect("notify marker written by the hook");
    let status = hook.wait().expect("wait for the marker hook");
    assert!(status.success(), "the marker hook failed: {status:?}");
    assert!(
        notified.contains("Pick a budget"),
        "the notify marker holds the question: {notified:?}"
    );
}

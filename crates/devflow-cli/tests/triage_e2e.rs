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
use devflow_core::test_support::{FakeJev, FakeReply, jev_answer, refused_url};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

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

/// A fake Jev reply picking option `a` with probability `score`, its
/// pre-check also at `score`.
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

/// How long a reply that must not come is watched for.
const NO_REPLY_WATCH: Duration = Duration::from_secs(2);

/// The id of a second call or a re-call.
const SECOND_ID: u64 = 4;

/// Wait until the fixture's log holds `count` `question_asked` lines, and
/// return them.
fn wait_for_questions_asked(fixture: &AskFixture, count: usize) -> Vec<Value> {
    common::wait_for(
        || fixture.events_named("question_asked").len() >= count,
        REPLY_WAIT,
        "the question_asked lines",
    );
    fixture.events_named("question_asked")
}

/// The `triage` object of the fixture's only `question_asked`, once its gate
/// has opened, asserting the line names the gate.
fn gated_triage(fixture: &AskFixture) -> Value {
    let gate_id = fixture.wait_for_gate_id();
    let asked = wait_for_questions_asked(fixture, 1);
    assert_eq!(asked.len(), 1, "one question_asked: {asked:#?}");
    assert_eq!(asked[0]["gate_id"], gate_id.as_str(), "{asked:#?}");
    asked[0]["triage"].clone()
}

/// `question_answered` lines whose policy is `triage`.
fn triage_answers(fixture: &AskFixture) -> Vec<Value> {
    fixture
        .events_named("question_answered")
        .into_iter()
        .filter(|line| line["policy"] == "triage")
        .collect()
}

/// Answer the question gate `gate_id` with `option`, asserting the command
/// exited 0.
fn answer_gate(fixture: &AskFixture, gate_id: &str, option: &str) {
    let output = fixture
        .devflow()
        .args([
            "gate",
            "answer",
            "1",
            "--gate-id",
            gate_id,
            "--option",
            option,
        ])
        .output()
        .expect("run devflow gate answer");
    assert!(
        output.status.success(),
        "devflow gate answer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The `structuredContent` of the reply to `id`, waiting for it.
fn answered(client: &AskClient, id: u64) -> Value {
    let reply = client.reply(id, REPLY_WAIT).expect("the call replies");
    assert_ne!(reply["result"]["isError"], true, "{reply}");
    reply["result"]["structuredContent"].clone()
}

#[test]
fn a_threshold_above_the_models_scores_sends_the_question_to_a_gate() {
    let fixture = triage_fixture(&TriageToml::at(1.0));
    let fake = FakeJev::serve(vec![scoring_a(0.99)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let triage = gated_triage(&fixture);
    assert_eq!(triage["outcome"], "escalated", "{triage}");
    assert_eq!(triage["reason"], "pre_check_below_threshold", "{triage}");
    answer_gate(&fixture, &fixture.wait_for_gate_id(), "b");
    let answer = answered(&client, CALL_ID);
    assert_eq!(answer["answer"]["option_id"], "b", "{answer}");
    assert_ne!(answer["answered_by"], "triage:jev", "{answer}");
    assert_eq!(fake.request_count(), 1);

    // Control: a threshold at the scores decides (D-24's `>=`).
    let control = triage_fixture(&TriageToml::at(0.99));
    let fake = FakeJev::serve(vec![scoring_a(0.99)]);
    let mut client = triage_client(&control, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );
    let answer = answered(&client, CALL_ID);
    assert_eq!(answer["answered_by"], "triage:jev", "{answer}");
    assert_eq!(
        only_question_asked(&control)["triage"]["outcome"],
        "decided"
    );
}

#[test]
fn a_zero_threshold_never_opens_a_gate() {
    let fixture = triage_fixture(&TriageToml::at(0.0));
    let fake = FakeJev::serve(vec![scoring_a(0.3), scoring_a(0.3)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());

    for (id, kind) in [(CALL_ID, "discuss"), (SECOND_ID, "decision")] {
        ask(&mut client, id, kind, &format!("A {kind} question"));
        let answer = answered(&client, id);
        assert_eq!(answer["answered_by"], "triage:jev", "{kind}: {answer}");
        assert!(!fixture.gate_path().exists(), "{kind} wrote a gate file");
    }
    let asked = fixture.events_named("question_asked");
    assert_eq!(asked.len(), 2, "{asked:#?}");
    for line in &asked {
        assert!(line.get("gate_id").is_none(), "a gate id: {line}");
        assert_eq!(line["triage"]["outcome"], "decided", "{line}");
    }
    assert!(
        fixture.events_named("gate_fired").is_empty(),
        "a gate fired: {:#?}",
        fixture.events()
    );
    assert_eq!(fake.request_count(), 2);
}

#[test]
fn a_blocking_human_question_is_never_triaged() {
    let fixture = triage_fixture(&TriageToml::at(0.0));
    let fake = FakeJev::serve(vec![scoring_a(1.0)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "blocking-human",
        "May I delete the release tag?",
    );

    let triage = gated_triage(&fixture);
    assert_eq!(triage["outcome"], "escalated", "{triage}");
    assert_eq!(triage["reason"], "blocking_human", "{triage}");
    assert!(
        client.reply(CALL_ID, NO_REPLY_WATCH).is_none(),
        "the call was answered"
    );
    assert_eq!(fake.request_count(), 0, "Jev was asked");
}

/// Whether `value` has a key `key` at any depth.
fn has_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(name, inner)| name == key || has_key(inner, key)),
        Value::Array(items) => items.iter().any(|item| has_key(item, key)),
        _ => false,
    }
}

#[test]
fn jev_never_sees_the_recommended_option() {
    let fixture = triage_fixture(&TriageToml::at(0.5));
    let fake = FakeJev::serve(vec![scoring_a(0.93)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    client.request(
        CALL_ID,
        "tools/call",
        json!({ "name": "devflow_ask", "arguments": {
            "question": "Should the cache stay?", "kind": "decision", "recommended": "b",
            "options": [
                { "id": "a", "label": "Drop it" },
                { "id": "b", "label": "Keep it (Recommended)" },
            ] } }),
    );
    answered(&client, CALL_ID);

    let requests = fake.requests();
    assert_eq!(requests.len(), 1, "{requests:#?}");
    let text = String::from_utf8_lossy(&requests[0].body).into_owned();
    let body: Value = serde_json::from_str(&text).expect("the request is JSON");
    assert!(!has_key(&body, "recommended"), "a recommended key: {text}");
    assert!(
        !text.contains("(Recommended)"),
        "a recommended marker: {text}"
    );
    assert!(text.contains("Keep it"), "the option was sent: {text}");
    // Control: the question did carry a recommendation, on the log.
    let asked = only_question_asked(&fixture);
    assert!(has_key(&asked, "recommended"), "{asked}");
    assert_eq!(asked["recommended"], "b", "{asked}");
}

#[test]
fn triage_off_by_default_sends_no_request() {
    let fixture = AskFixture::new(Stage::Code);
    let fake = FakeJev::serve(vec![scoring_a(0.93)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let triage = gated_triage(&fixture);
    assert!(triage.is_null(), "a triage object: {triage}");
    assert_eq!(fake.request_count(), 0, "Jev was asked");
}

#[test]
fn triage_off_for_the_run_sends_no_request() {
    let fixture = triage_fixture(&TriageToml::at(0.5));
    fixture.edit_state(|state| state.triage_off = true);
    let fake = FakeJev::serve(vec![scoring_a(0.93)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let triage = gated_triage(&fixture);
    assert!(triage.is_null(), "a triage object: {triage}");
    assert_eq!(fake.request_count(), 0, "Jev was asked");
}

#[test]
fn an_escalated_question_fires_the_notify_hook() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let marker = scratch.path().join("notified");
    let fixture = triage_fixture(&TriageToml::at(1.0));
    let hook = format!(
        "printf '%s' \"$DEVFLOW_GATE_CONTEXT\" > '{0}.tmp' && mv '{0}.tmp' '{0}'",
        marker.display()
    );
    fixture.edit_state(|state| state.gate_notify_cmd = Some(hook));
    let fake = FakeJev::serve(vec![scoring_a(0.5)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    assert_eq!(gated_triage(&fixture)["outcome"], "escalated");
    common::wait_for(|| marker.exists(), REPLY_WAIT, "the notify marker");
    let notified = std::fs::read_to_string(&marker).expect("read the notify marker");
    assert!(
        notified.contains("How long should the budget be?"),
        "the marker holds the question: {notified:?}"
    );
}

#[test]
fn a_failing_model_escalates_the_question() {
    let fixture = triage_fixture(&TriageToml::at(0.5));
    let mut client = triage_client(&fixture, &refused_url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let triage = gated_triage(&fixture);
    assert_eq!(triage["outcome"], "fallback", "{triage}");
    assert_eq!(triage["reason"], "unreachable", "{triage}");
    assert!(
        client.reply(CALL_ID, NO_REPLY_WATCH).is_none(),
        "the call was answered"
    );
    assert!(triage_answers(&fixture).is_empty(), "triage answered");
}

#[test]
fn a_slow_model_cannot_overrun_the_ask_budget() {
    let fixture = triage_fixture(&TriageToml {
        timeout_secs: 10,
        ..TriageToml::at(0.5)
    });
    let fake = FakeJev::serve(vec![scoring_a(0.93).delayed(Duration::from_secs(6))]);
    let mut client = triage_client(
        &fixture,
        &fake.url(),
        AskOverrides {
            timeout_secs: Some(8),
            ..AskOverrides::default()
        },
    );
    let sent = Instant::now();
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let triage = gated_triage(&fixture);
    let opened = sent.elapsed();
    assert!(
        opened < Duration::from_secs(8),
        "the gate opened at {opened:?}"
    );
    assert_eq!(triage["outcome"], "fallback", "{triage}");
    assert_eq!(triage["reason"], "timeout", "{triage}");
    assert!(triage_answers(&fixture).is_empty(), "triage answered");
    assert_eq!(fake.request_count(), 1);
}

#[test]
fn a_question_with_no_budget_left_is_not_sent_to_the_model() {
    let fixture = triage_fixture(&TriageToml::at(0.5));
    let fake = FakeJev::serve(vec![scoring_a(0.93)]);
    let mut client = triage_client(
        &fixture,
        &fake.url(),
        AskOverrides {
            timeout_secs: Some(5),
            ..AskOverrides::default()
        },
    );
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    let triage = gated_triage(&fixture);
    assert_eq!(triage["outcome"], "fallback", "{triage}");
    assert_eq!(triage["reason"], "ask_budget_spent", "{triage}");
    assert_eq!(fake.request_count(), 0, "Jev was asked");
}

#[test]
fn a_question_cancelled_during_the_model_call_is_not_decided() {
    let fixture = triage_fixture(&TriageToml {
        timeout_secs: 10,
        ..TriageToml::at(0.5)
    });
    let fake = FakeJev::serve(vec![scoring_a(0.93).delayed(Duration::from_secs(3))]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );
    common::wait_for(|| fake.request_count() == 1, REPLY_WAIT, "the Jev request");
    std::thread::sleep(Duration::from_secs(1));
    client.cancel(json!(CALL_ID));

    // The model answers 2 s later; watch past that for a reply.
    assert!(
        client.reply(CALL_ID, Duration::from_secs(5)).is_none(),
        "the cancelled call got a reply"
    );
    assert!(
        fixture.events_named("question_answered").is_empty(),
        "an answer was recorded: {:#?}",
        fixture.events()
    );
    assert!(
        fixture.events_named("question_asked").is_empty(),
        "the question was recorded: {:#?}",
        fixture.events()
    );
    assert!(!fixture.gate_path().exists(), "a gate was written");
    assert_eq!(fake.request_count(), 1);
}

#[test]
fn a_decision_that_cannot_be_recorded_is_not_returned() {
    let fixture = triage_fixture(&TriageToml::at(0.5));
    std::fs::create_dir_all(devflow_core::events::events_path(fixture.root()))
        .expect("make events.jsonl a directory");
    let fake = FakeJev::serve(vec![scoring_a(0.93)]);
    let mut client = triage_client(&fixture, &fake.url(), AskOverrides::default());
    ask(
        &mut client,
        CALL_ID,
        "decision",
        "How long should the budget be?",
    );

    fixture.wait_for_gate_id();
    assert_eq!(fake.request_count(), 1, "Jev was asked");
    if let Some(reply) = client.reply(CALL_ID, NO_REPLY_WATCH) {
        assert_ne!(
            reply["result"]["structuredContent"]["answered_by"], "triage:jev",
            "an unrecorded decision was returned: {reply}"
        );
    }
}

#[test]
fn a_capped_call_keeps_the_triage_result_for_the_recall() {
    let fixture = triage_fixture(&TriageToml::at(1.0));
    let first_fake = FakeJev::serve(vec![scoring_a(0.5)]);
    let mut first_client = triage_client(&fixture, &first_fake.url(), AskOverrides::default());
    ask(&mut first_client, CALL_ID, "decision", "First question");
    let first_gate = fixture.wait_for_gate_id();

    let second_fake = FakeJev::serve(vec![scoring_a(0.5), scoring_a(0.5)]);
    let mut second_client = triage_client(
        &fixture,
        &second_fake.url(),
        AskOverrides {
            hold_cap_secs: Some(1),
            timeout_secs: Some(60),
            ..AskOverrides::default()
        },
    );
    ask(&mut second_client, CALL_ID, "decision", "Second question");
    let pending = answered(&second_client, CALL_ID);
    assert_eq!(pending["status"], "pending", "{pending}");
    let question_id = pending["question_id"]
        .as_str()
        .expect("the pending reply names its question")
        .to_string();
    assert_eq!(second_fake.request_count(), 1, "Jev scored the first call");

    answer_gate(&fixture, &first_gate, "a");
    answered(&first_client, CALL_ID);
    second_client.request(
        SECOND_ID,
        "tools/call",
        json!({ "name": "devflow_ask", "arguments": {
            "question": "Second question", "options": options(), "kind": "decision",
            "question_id": question_id } }),
    );

    let asked = wait_for_questions_asked(&fixture, 2);
    let recalled = asked
        .iter()
        .find(|line| line["question_id"] == question_id.as_str())
        .unwrap_or_else(|| panic!("no question_asked for {question_id}: {asked:#?}"));
    assert!(recalled["gate_id"].is_string(), "{recalled}");
    assert_eq!(recalled["triage"]["outcome"], "escalated", "{recalled}");
    assert_eq!(
        recalled["triage"]["reason"], "pre_check_below_threshold",
        "{recalled}"
    );
    assert_eq!(
        second_fake.request_count(),
        1,
        "the re-call asked Jev again"
    );
}

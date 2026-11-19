//! `devflow __ask`: the stdio MCP server an agent DevFlow launched spawns, with
//! one tool, `devflow_ask` (Phase 53, D-03..D-06).
//!
//! Newline-delimited JSON-RPC on stdin and stdout, one message per line. The
//! reader answers `initialize`, `ping` and `tools/list` itself and hands each
//! `tools/call` to one worker thread, in arrival order, so a call held on its
//! gate never stops `ping` being answered. The worker opens the question as a
//! gate of kind `question`, waits for its answer as every gate waiter does,
//! and replies with the operator's choice. Only JSON-RPC lines reach stdout;
//! every log line goes to stderr.

use crate::pipeline_gate::fire_gate_notify_events;
use devflow_core::gates::{AwaitOutcome, Claim, GateAnswer, GateId, GateKind, Gates};
use devflow_core::phase_id::PhaseId;
use devflow_core::question::{AskAnswer, QuestionChoice, QuestionSpec, render_question_context};
use devflow_core::stage::Stage;
use devflow_core::{events, workflow};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use tracing::info;

/// The MCP protocol versions this server speaks, newest first. A client asking
/// for another gets the first (the spec's rule for an unsupported request).
pub(crate) const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] =
    ["2025-11-25", "2025-06-18", "2025-03-26"];

/// The one tool this server offers.
pub(crate) const ASK_TOOL: &str = "devflow_ask";

/// JSON-RPC: the line is not JSON.
const PARSE_ERROR: i64 = -32700;
/// JSON-RPC: a request for a method this server does not have.
const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC: a request whose params this server refuses.
const INVALID_PARAMS: i64 = -32602;

/// What `__ask` is bound to: the run that launched the agent (D-03).
#[derive(Debug, Clone)]
pub(crate) struct AskArgs {
    /// Project root.
    pub(crate) project: PathBuf,
    /// The phase the question belongs to.
    pub(crate) phase: PhaseId,
    /// The stage whose gate the question opens.
    pub(crate) stage: Stage,
    /// How long a question waits for its answer.
    pub(crate) timeout_secs: u64,
    /// The launching run's `started_at`; accepted here, checked from 53-07.
    pub(crate) run_started_at: String,
}

/// What the reader does with one line.
#[derive(Debug, PartialEq)]
pub(crate) enum Dispatch {
    /// Write this message now.
    Reply(Value),
    /// Hand this `tools/call` to the worker, which replies under `id`.
    Call {
        /// The request's id.
        id: Value,
        /// The request's params.
        params: Value,
    },
    /// Write nothing (a notification, or a client's response).
    Nothing,
}

/// Serve `args` on stdin and stdout until stdin ends. Returns the exit code.
pub(crate) fn serve(args: AskArgs) -> i32 {
    info!(
        "devflow __ask: phase {} {} for the run started at {}",
        args.phase, args.stage, args.run_started_at
    );
    let input = std::io::BufReader::new(std::io::stdin());
    let output = Arc::new(Mutex::new(std::io::stdout()));
    run_server(input, output, move |params| ask(&args, params))
}

/// The server loop over any input and output, so tests drive it without stdio.
/// `call` runs each `tools/call`'s params on the worker thread and returns the
/// tool result.
pub(crate) fn run_server<R, W>(
    input: R,
    output: Arc<Mutex<W>>,
    mut call: impl FnMut(&Value) -> Value + Send + 'static,
) -> i32
where
    R: BufRead,
    W: Write + Send + 'static,
{
    let (calls, queued) = mpsc::channel::<(Value, Value)>();
    let worker_output = Arc::clone(&output);
    let worker = std::thread::Builder::new()
        .name("ask-worker".to_string())
        .spawn(move || {
            for (id, params) in queued {
                let result = call(&params);
                write_message(&worker_output, &response(&id, result));
            }
        });
    if let Err(error) = worker {
        eprintln!("devflow __ask: could not start the worker thread: {error}");
        return 1;
    }
    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                write_message(
                    &output,
                    &error_response(&Value::Null, PARSE_ERROR, "parse error"),
                );
                continue;
            }
            Err(error) => {
                eprintln!("devflow __ask: stdin read failed: {error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match handle_line(&line) {
            Dispatch::Reply(message) => write_message(&output, &message),
            Dispatch::Call { id, params } => {
                if calls.send((id, params)).is_err() {
                    eprintln!("devflow __ask: the worker thread is gone");
                    return 1;
                }
            }
            Dispatch::Nothing => {}
        }
    }
    // Stdin ended. A call still held is not waited for: what an ended client
    // leaves behind is 53-07's cleanup.
    0
}

/// Route one input line: reply now, hand a `tools/call` to the worker, or
/// nothing for a notification or a client's response.
pub(crate) fn handle_line(line: &str) -> Dispatch {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Dispatch::Reply(error_response(&Value::Null, PARSE_ERROR, "parse error"));
    };
    let id = message.get("id").filter(|id| !id.is_null()).cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Dispatch::Nothing;
    };
    let Some(id) = id else {
        // A notification: `notifications/initialized` and any other get no reply.
        return Dispatch::Nothing;
    };
    match method {
        "initialize" => {
            let requested = params["protocolVersion"].as_str().unwrap_or("");
            Dispatch::Reply(response(
                &id,
                json!({
                    "protocolVersion": negotiate(requested),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "devflow", "version": env!("CARGO_PKG_VERSION") },
                }),
            ))
        }
        "ping" => Dispatch::Reply(response(&id, json!({}))),
        "tools/list" => Dispatch::Reply(response(&id, json!({ "tools": [ask_tool_definition()] }))),
        "tools/call" => match params["name"].as_str() {
            Some(ASK_TOOL) => Dispatch::Call { id, params },
            other => Dispatch::Reply(error_response(
                &id,
                INVALID_PARAMS,
                &format!("unknown tool `{}`", other.unwrap_or_default()),
            )),
        },
        _ => Dispatch::Reply(error_response(&id, METHOD_NOT_FOUND, "method not found")),
    }
}

/// Write `message` as one line and flush, under the one writer lock.
fn write_message<W: Write>(output: &Mutex<W>, message: &Value) {
    let line = message.to_string();
    let mut out = output
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
        eprintln!("devflow __ask: could not write a reply to stdout");
    }
}

/// A JSON-RPC result under `id`.
fn response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// A JSON-RPC error under `id`.
fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The protocol version to answer a client that asked for `requested`.
fn negotiate(requested: &str) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|version| *version == requested)
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0])
}

/// The `devflow_ask` tool as `tools/list` describes it (D-07).
fn ask_tool_definition() -> Value {
    json!({
        "name": ASK_TOOL,
        "description": "Ask the operator a question and wait for their answer. The call \
            returns the option the operator chose.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question." },
                "options": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "label": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["id", "label"]
                    }
                },
                "context": { "type": "string" },
                "kind": { "type": "string", "enum": ["decision", "blocking-human", "discuss"] },
                "recommended": { "type": "string", "description": "An offered option's id." }
            },
            "required": ["question", "options", "kind"]
        }
    })
}

/// A tool result the agent reads as a failure, naming why.
fn tool_error(why: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": why }], "isError": true })
}

/// The question a call asks, under a fresh question id, or why it was refused
/// (D-07, D-08). A refusal is returned before any gate is touched, so it blocks
/// neither this call nor the calls queued behind it.
fn parse_ask_arguments(arguments: &Value) -> Result<QuestionSpec, String> {
    QuestionSpec::from_arguments(arguments).map_err(|error| format!("devflow_ask refused: {error}"))
}

/// One `devflow_ask` call: open the question gate, wait for its answer, and
/// return the operator's choice as the tool result.
fn ask(args: &AskArgs, params: &Value) -> Value {
    let spec = match parse_ask_arguments(params.get("arguments").unwrap_or(&Value::Null)) {
        Ok(spec) => spec,
        Err(why) => return tool_error(&why),
    };
    match ask_gate(args, &spec) {
        Ok(answer) => {
            let text = match &answer.answer {
                QuestionChoice::Option { option_id } => {
                    format!("The operator chose option `{option_id}`.")
                }
                QuestionChoice::Text { text } => format!("The operator answered: {text}"),
            };
            json!({
                "content": [{ "type": "text", "text": text }],
                "structuredContent": answer,
            })
        }
        Err(why) => tool_error(&why),
    }
}

/// The gate half of [`ask`]: open, wait, read the choice, claim, finish the
/// registration and close the stage's gate. The registration is finished on
/// every exit path.
fn ask_gate(args: &AskArgs, spec: &QuestionSpec) -> Result<AskAnswer, String> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let (opened, registration) = Gates::open_question_registered(root, phase, stage, spec)
        .map_err(|error| format!("devflow_ask could not open the question gate: {error}"))?;
    let gate_id = opened.gate_id;
    eprintln!(
        "devflow __ask: question gate .devflow/gates/{}-{stage}.json written (gate {gate_id}); \
         awaiting `devflow gate answer`",
        phase.padded()
    );
    let notifier = Notifier::spawn(args, spec);
    emit_question_asked(args, spec, &gate_id);
    let outcome = await_bound_answer(args, &gate_id);
    let result = match outcome {
        Ok(Some(answer)) => consume(args, &gate_id, spec, &answer),
        Ok(None) => Err(format!(
            "devflow_ask: no answer to the question within {} s; the question was closed",
            args.timeout_secs
        )),
        Err(why) => Err(why),
    };
    registration.finish();
    if let Err(error) = Gates::close(root, phase, stage) {
        eprintln!("devflow __ask: could not close phase {phase} {stage}'s gate: {error}");
    }
    notifier.finish();
    if let Ok(answer) = &result {
        emit_question_answered(args, &gate_id, answer);
    }
    result
}

/// The notify thread of one question: the gate's `gate_fired`, notify hook and
/// `notify_fired`, run off the worker thread so a slow hook never delays the
/// answer wait (D-11).
struct Notifier {
    /// Set once the question has ended; the hook run is then killed.
    ended: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Notifier {
    /// Start notifying for `spec`'s just-opened gate, with the run's persisted
    /// notify command.
    fn spawn(args: &AskArgs, spec: &QuestionSpec) -> Self {
        let ended = Arc::new(AtomicBool::new(false));
        let notify_cmd = match workflow::load_state(&args.project, args.phase) {
            Ok(state) => state.gate_notify_cmd,
            Err(error) => {
                eprintln!(
                    "devflow __ask: could not read the run state for its notify command: {error}"
                );
                None
            }
        };
        let (project, phase, stage) = (args.project.clone(), args.phase, args.stage);
        let context = render_question_context(spec);
        let thread_ended = Arc::clone(&ended);
        let thread = std::thread::Builder::new()
            .name("ask-notify".to_string())
            .spawn(move || {
                // A question is an expected stop for an answer in every mode,
                // never a failure, so it is not an unexpected gate.
                fire_gate_notify_events(
                    &project,
                    phase,
                    stage,
                    &context,
                    &context,
                    GateKind::Question,
                    false,
                    notify_cmd.as_deref(),
                    &|| thread_ended.load(Ordering::SeqCst),
                );
            })
            .map_err(|error| {
                eprintln!("devflow __ask: could not start the notify thread: {error}");
            })
            .ok();
        Self { ended, thread }
    }

    /// The question has ended: kill a hook still running and wait for the
    /// thread, which returns within one hook poll.
    fn finish(mut self) {
        self.ended.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            eprintln!("devflow __ask: the notify thread panicked");
        }
    }
}

/// Record `question_asked` for `spec`'s open gate (D-15).
fn emit_question_asked(args: &AskArgs, spec: &QuestionSpec, gate_id: &GateId) {
    events::emit(
        &args.project,
        args.phase,
        events::QUESTION_ASKED,
        json!({
            "stage": args.stage.to_string(),
            "question_id": spec.question_id,
            "gate_id": gate_id.as_str(),
            "kind": spec.kind,
            "question": spec.question,
            "options": spec.options,
            "context": spec.context,
            "recommended": spec.recommended,
        }),
    );
}

/// Record `question_answered` for `answer` (D-15).
fn emit_question_answered(args: &AskArgs, gate_id: &GateId, answer: &AskAnswer) {
    events::emit(
        &args.project,
        args.phase,
        events::QUESTION_ANSWERED,
        json!({
            "stage": args.stage.to_string(),
            "question_id": answer.question_id,
            "gate_id": gate_id.as_str(),
            "answer": answer.answer,
            "policy": events::ANSWERING_POLICY_OPERATOR_VIA_GATE,
            "answered_by": answer.answered_by,
        }),
    );
}

/// [`Gates::await_answer`] with the time left, setting aside each file in the
/// slot that does not bind to `gate_id`. `None` on a timeout or a cancel.
fn await_bound_answer(args: &AskArgs, gate_id: &GateId) -> Result<Option<GateAnswer>, String> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let started = std::time::Instant::now();
    loop {
        let left = args
            .timeout_secs
            .saturating_sub(started.elapsed().as_secs());
        match Gates::await_answer(root, phase, stage, gate_id, left, || false) {
            AwaitOutcome::Unbound { reason } => {
                Gates::set_aside_unbound(root, phase, stage, gate_id).map_err(|error| {
                    format!(
                        "devflow_ask: the file in the question gate's answer slot does not bind \
                         to it ({reason}) and could not be set aside: {error}"
                    )
                })?;
            }
            AwaitOutcome::Answered(answer) => return Ok(Some(answer)),
            AwaitOutcome::TimedOut | AwaitOutcome::Cancelled => return Ok(None),
        }
    }
}

/// Read the answered gate's choice and claim the answer.
fn consume(
    args: &AskArgs,
    gate_id: &GateId,
    spec: &QuestionSpec,
    answer: &GateAnswer,
) -> Result<AskAnswer, String> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let choice = Gates::read_question_choice(root, phase, stage, gate_id)
        .map_err(|error| format!("devflow_ask could not read the answer's choice: {error}"))?;
    match Gates::claim(root, phase, stage, gate_id) {
        Ok(Claim::Claimed) => {}
        Ok(Claim::AlreadyConsumed) => {
            return Err("devflow_ask: the answer was already consumed by another process".into());
        }
        Err(error) => return Err(format!("devflow_ask could not claim the answer: {error}")),
    }
    if answer.response.abort {
        let reason = answer.response.note.as_deref().unwrap_or("no reason given");
        return Err(format!(
            "devflow_ask: the operator aborted the run at this question ({reason}); stop work"
        ));
    }
    let Some(choice) = choice else {
        return Err("devflow_ask: the question's answer carries no choice".into());
    };
    Ok(AskAnswer {
        question_id: spec.question_id.clone(),
        answer: choice,
        answered_by: answer
            .response
            .responded_by
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        answered_at: unix_now(),
    })
}

/// Now, as Unix epoch seconds.
fn unix_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// How long a test waits for a reply that should come.
    const REPLY_WAIT: Duration = Duration::from_secs(5);

    fn request(id: u64, method: &str, params: Value) -> String {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    fn reply_of(dispatch: Dispatch) -> Value {
        match dispatch {
            Dispatch::Reply(message) => message,
            other => panic!("expected a reply now, got {other:?}"),
        }
    }

    /// Every reply line written so far.
    fn replies(output: &Mutex<Vec<u8>>) -> Vec<Value> {
        let bytes = output.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).expect("every stdout line is JSON"))
            .collect()
    }

    /// The reply with `id`, waiting at most `wait` for it.
    fn reply_with_id(output: &Mutex<Vec<u8>>, id: u64, wait: Duration) -> Option<Value> {
        let started = Instant::now();
        loop {
            if let Some(reply) = replies(output).into_iter().find(|reply| reply["id"] == id) {
                return Some(reply);
            }
            if started.elapsed() >= wait {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn ask_call(id: u64, tool: &str) -> String {
        request(
            id,
            "tools/call",
            json!({
                "name": tool,
                "arguments": {
                    "question": "Pick a budget",
                    "options": [{ "id": "budget-30", "label": "30 s" }],
                    "kind": "decision"
                }
            }),
        )
    }

    #[test]
    fn initialize_negotiates_a_supported_version() {
        let reply = reply_of(handle_line(&request(
            1,
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {} }),
        )));
        assert_eq!(
            reply["result"]["protocolVersion"], "2025-06-18",
            "a supported version is echoed: {reply}"
        );
        assert_eq!(reply["result"]["serverInfo"]["name"], "devflow", "{reply}");
        let control = reply_of(handle_line(&request(
            2,
            "initialize",
            json!({ "protocolVersion": "1999-01-01", "capabilities": {} }),
        )));
        assert_eq!(
            control["result"]["protocolVersion"], "2025-11-25",
            "an unsupported version gets the newest supported one: {control}"
        );
    }

    #[test]
    fn an_unknown_request_gets_method_not_found() {
        let reply = reply_of(handle_line(&request(7, "server/discover", json!({}))));
        assert_eq!(reply["id"], 7, "{reply}");
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND, "{reply}");
        let notification =
            json!({ "jsonrpc": "2.0", "method": "notifications/unknown" }).to_string();
        assert_eq!(
            handle_line(&notification),
            Dispatch::Nothing,
            "an unknown notification gets no reply"
        );
    }

    #[test]
    fn ping_is_answered_while_a_call_is_held() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let (entered, call_entered) = mpsc::channel::<()>();
        let (release, released) = mpsc::channel::<()>();
        let server = std::thread::spawn({
            let output = Arc::clone(&output);
            move || {
                run_server(std::io::BufReader::new(reader), output, move |_| {
                    let _ = entered.send(());
                    let _ = released.recv();
                    json!({ "content": [] })
                })
            }
        });
        writeln!(writer, "{}", ask_call(1, ASK_TOOL)).unwrap();
        call_entered
            .recv_timeout(REPLY_WAIT)
            .expect("the call reached the worker");
        writeln!(writer, "{}", request(2, "ping", json!({}))).unwrap();
        let ping = reply_with_id(&output, 2, REPLY_WAIT);
        assert!(ping.is_some(), "ping not answered while a call was held");
        assert_eq!(ping.unwrap()["result"], json!({}));
        assert!(
            reply_with_id(&output, 1, Duration::ZERO).is_none(),
            "the held call has not replied yet"
        );
        release.send(()).unwrap();
        drop(writer);
        assert_eq!(server.join().unwrap(), 0);
        assert!(
            reply_with_id(&output, 1, REPLY_WAIT).is_some(),
            "the released call replies"
        );
    }

    #[test]
    fn a_call_for_another_tool_is_a_protocol_error() {
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let calls = Arc::new(Mutex::new(0_u32));
        let input = format!("{}\n", ask_call(3, "other"));
        let code = run_server(std::io::Cursor::new(input), Arc::clone(&output), {
            let calls = Arc::clone(&calls);
            move |_| {
                *calls.lock().unwrap() += 1;
                json!({ "content": [] })
            }
        });
        assert_eq!(code, 0);
        let reply = reply_with_id(&output, 3, REPLY_WAIT).expect("the call is answered");
        assert_eq!(reply["error"]["code"], INVALID_PARAMS, "{reply}");
        assert_eq!(
            *calls.lock().unwrap(),
            0,
            "a call for another tool never reaches the worker, so it opens no gate"
        );
    }
}

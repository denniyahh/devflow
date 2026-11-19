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
//!
//! A held call ends only by an answer, an abort, the client's cancel, stdin's
//! end, SIGTERM or SIGINT, the run stopped, its runner lock no longer live, or
//! the timeout (D-06). Every ending other than an answer closes the call's own
//! gate (never a gate that replaced it) and records `question_closed`.

use crate::pipeline_gate::fire_gate_notify_events;
use devflow_core::gates::{AwaitOutcome, Claim, GateAnswer, GateId, GateKind, Gates};
use devflow_core::liveness::{self, Held};
use devflow_core::phase_id::PhaseId;
use devflow_core::question::{AskAnswer, QuestionChoice, QuestionSpec, render_question_context};
use devflow_core::stage::Stage;
use devflow_core::{events, workflow};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tracing::info;

/// The SIGTERM or SIGINT this process caught, `0` before one arrives. The
/// handler only stores here, which is async-signal-safe.
static CAUGHT_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// How often the idle worker looks for a caught signal, which cannot wake it.
const IDLE_POLL: Duration = Duration::from_millis(100);

/// How often a held call re-reads the run state and probes the runner lock.
const RUN_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// How often a call retries the stage's ask lock while another question holds it.
const ASK_LOCK_RETRY: Duration = Duration::from_millis(250);

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
    /// The launching run's `started_at`; a call is refused unless the saved
    /// state still carries it.
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
    /// The client cancelled the request with this id (`notifications/cancelled`).
    Cancel {
        /// The cancelled request's id.
        request_id: Value,
    },
}

/// Why a question ended without an answer: its `question_closed` reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    /// The client cancelled the call; it gets no reply.
    CancelledByClient,
    /// The server's stdin ended.
    ClientDisconnected,
    /// The server caught SIGTERM or SIGINT.
    Terminated,
    /// The run's state says it was stopped.
    RunStopped,
    /// The phase's runner lock is no longer held.
    RunnerGone,
    /// No answer within `--timeout-secs`.
    TimedOut,
    /// The operator answered with an abort.
    AbortedByOperator,
}

impl Ending {
    /// The `question_closed` reason, which also leads the reply's text.
    fn reason(self) -> &'static str {
        match self {
            Self::CancelledByClient => "cancelled_by_client",
            Self::ClientDisconnected => "client_disconnected",
            Self::Terminated => "terminated",
            Self::RunStopped => "run_stopped",
            Self::RunnerGone => "runner_gone",
            Self::TimedOut => "timed_out",
            Self::AbortedByOperator => "aborted_by_operator",
        }
    }

    /// The plain sentence the reply gives after the reason.
    fn sentence(self) -> &'static str {
        match self {
            Self::CancelledByClient => "the client cancelled the call",
            Self::ClientDisconnected => "the client's connection ended",
            Self::Terminated => "the question server was told to stop; end the stage",
            Self::RunStopped => "the run was stopped; end the stage",
            Self::RunnerGone => "the run's runner is no longer live; end the stage",
            Self::TimedOut => "no answer within the gate timeout",
            Self::AbortedByOperator => "the operator aborted the run at this question; stop work",
        }
    }
}

/// One `tools/call` waiting for the worker.
struct PendingCall {
    /// The request's id.
    id: Value,
    /// The id as its JSON text, so `3` and `"3"` are different ids.
    id_text: String,
    /// The request's params.
    params: Value,
    /// When the reader read it: the call's timeout runs from here.
    arrived: Instant,
}

/// What the reader thread and the worker share.
#[derive(Default)]
struct CallFlags {
    /// Each cancelled request id, as its JSON text.
    cancelled: Mutex<HashSet<String>>,
    /// Set once stdin has ended.
    eof: AtomicBool,
    /// The calls not yet taken by the worker, in arrival order.
    queue: Mutex<VecDeque<PendingCall>>,
    /// Signalled on each push and at stdin's end.
    wake: Condvar,
}

/// Lock `mutex`, taking the data of a poisoned one.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The signal this process caught, if one has arrived.
fn caught_signal() -> Option<i32> {
    match CAUGHT_SIGNAL.load(Ordering::SeqCst) {
        0 => None,
        signal => Some(signal),
    }
}

/// One call's view of what ends it from the client's side.
pub(crate) struct CallControl<'a> {
    flags: &'a CallFlags,
    id_text: &'a str,
    /// When the call arrived, so queue time counts against its timeout.
    arrived: Instant,
}

impl CallControl<'_> {
    /// The client's cancel of this call, stdin's end, or a caught signal.
    pub(crate) fn ending(&self) -> Option<Ending> {
        if lock(&self.flags.cancelled).contains(self.id_text) {
            Some(Ending::CancelledByClient)
        } else if self.flags.eof.load(Ordering::SeqCst) {
            Some(Ending::ClientDisconnected)
        } else if caught_signal().is_some() {
            Some(Ending::Terminated)
        } else {
            None
        }
    }
}

/// Serve `args` on stdin and stdout until stdin ends or a stop signal arrives.
/// Returns the exit code: `0` at stdin's end, `128 + signal` on a signal.
pub(crate) fn serve(args: AskArgs) -> i32 {
    info!(
        "devflow __ask: phase {} {} for the run started at {}",
        args.phase, args.stage, args.run_started_at
    );
    if let Err(error) = install_stop_handler() {
        eprintln!("devflow __ask: could not install the SIGTERM/SIGINT handler: {error}");
        return 1;
    }
    let input = std::io::BufReader::new(std::io::stdin());
    let output = Arc::new(Mutex::new(std::io::stdout()));
    run_server(input, output, move |params, control| {
        ask(&args, params, control)
    })
}

/// Record SIGTERM and SIGINT in [`CAUGHT_SIGNAL`] instead of dying, so a held
/// question is closed first. This process is the agent's child, not the
/// runner, so it installs its own handler rather than `stop`'s signal pipe.
fn install_stop_handler() -> std::io::Result<()> {
    extern "C" fn on_stop_signal(signal: libc::c_int) {
        CAUGHT_SIGNAL.store(signal, Ordering::SeqCst);
    }
    for signal in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: an all-zero sigaction is valid; the fields that matter are set below.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = on_stop_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        // SAFETY: `action.sa_mask` is a live sigset_t.
        unsafe { libc::sigemptyset(&raw mut action.sa_mask) };
        // SAFETY: `action` is fully initialized; the old action is not wanted.
        if unsafe { libc::sigaction(signal, &raw const action, std::ptr::null_mut()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// The server loop over any input and output, so tests drive it without stdio.
/// A reader thread reads `input`; the calling thread is the worker, running
/// `call` on each `tools/call`'s params in arrival order. `call` returns the
/// tool result, or `None` to send no reply (a cancelled call). The loop ends at
/// stdin's end or a caught signal, after the call in hand has cleaned up.
pub(crate) fn run_server<R, W>(
    input: R,
    output: Arc<Mutex<W>>,
    mut call: impl FnMut(&Value, &CallControl<'_>) -> Option<Value>,
) -> i32
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let flags = Arc::new(CallFlags::default());
    let reader = std::thread::Builder::new()
        .name("ask-reader".to_string())
        .spawn({
            let (flags, output) = (Arc::clone(&flags), Arc::clone(&output));
            move || read_lines(input, &output, &flags)
        });
    if let Err(error) = reader {
        eprintln!("devflow __ask: could not start the reader thread: {error}");
        return 1;
    }
    while let Some(pending) = next_call(&flags) {
        let control = CallControl {
            flags: &flags,
            id_text: &pending.id_text,
            arrived: pending.arrived,
        };
        let result = call(&pending.params, &control);
        // After stdin's end the client is gone: nothing more goes to stdout.
        if let Some(result) = result
            && !flags.eof.load(Ordering::SeqCst)
        {
            write_message(&output, &response(&pending.id, result));
        }
        lock(&flags.cancelled).remove(&pending.id_text);
    }
    caught_signal().map_or(0, |signal| 128 + signal)
}

/// The next queued call, or `None` once stdin has ended or a signal arrived.
fn next_call(flags: &CallFlags) -> Option<PendingCall> {
    let mut queue = lock(&flags.queue);
    loop {
        if flags.eof.load(Ordering::SeqCst) || caught_signal().is_some() {
            return None;
        }
        if let Some(pending) = queue.pop_front() {
            return Some(pending);
        }
        queue = flags
            .wake
            .wait_timeout(queue, IDLE_POLL)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
}

/// Record a client's cancel of `request_id`. A call still queued is removed,
/// so it opens no gate, records no event and gets no reply; a call already
/// taken by the worker sees its id in the cancelled set.
fn cancel(flags: &CallFlags, request_id: &Value) {
    let id_text = request_id.to_string();
    let mut queue = lock(&flags.queue);
    let queued = queue.len();
    queue.retain(|pending| pending.id_text != id_text);
    if queue.len() == queued {
        lock(&flags.cancelled).insert(id_text);
    }
}

/// The reader thread: answer what the reader answers, queue each call, record
/// each cancel, and mark stdin's end.
fn read_lines<R: BufRead, W: Write>(input: R, output: &Mutex<W>, flags: &CallFlags) {
    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                write_message(
                    output,
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
            Dispatch::Reply(message) => write_message(output, &message),
            Dispatch::Call { id, params } => {
                let id_text = id.to_string();
                lock(&flags.queue).push_back(PendingCall {
                    id,
                    id_text,
                    params,
                    arrived: Instant::now(),
                });
                flags.wake.notify_all();
            }
            Dispatch::Cancel { request_id } => cancel(flags, &request_id),
            Dispatch::Nothing => {}
        }
    }
    // Stdin ended: a held call closes its gate as `client_disconnected`.
    flags.eof.store(true, Ordering::SeqCst);
    flags.wake.notify_all();
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
    if id.is_none() && method == "notifications/cancelled" {
        return match params.get("requestId").filter(|id| !id.is_null()) {
            Some(request_id) => Dispatch::Cancel {
                request_id: request_id.clone(),
            },
            None => Dispatch::Nothing,
        };
    }
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

/// How a question gate ended without an answer the agent can use.
#[derive(Debug)]
enum Unanswered {
    /// One of the D-06 endings: `question_closed` is recorded for it.
    Closed {
        ending: Ending,
        /// What the reply says after the reason.
        detail: String,
        /// Who closed it, when an operator did.
        closed_by: Option<String>,
    },
    /// The call ended before its gate opened: no gate, no question event.
    NotAsked(Ending),
    /// The question could not be served; the text is the reply.
    Failed(String),
}

impl Unanswered {
    /// The ending `ending`, with its plain sentence.
    fn closed(ending: Ending) -> Self {
        Self::Closed {
            ending,
            detail: ending.sentence().to_string(),
            closed_by: None,
        }
    }
}

/// One `devflow_ask` call: check it binds to its run, open the question gate,
/// wait for its answer, and return the operator's choice as the tool result.
/// `None` for a call the client cancelled, which gets no reply.
fn ask(args: &AskArgs, params: &Value, control: &CallControl<'_>) -> Option<Value> {
    let spec = match parse_ask_arguments(params.get("arguments").unwrap_or(&Value::Null)) {
        Ok(spec) => spec,
        Err(why) => return Some(tool_error(&why)),
    };
    if let Err(why) = check_run_binding(args) {
        return Some(tool_error(&why));
    }
    match ask_gate(args, &spec, control) {
        Ok(answer) => Some(answered_result(&answer)),
        Err(
            Unanswered::Closed {
                ending: Ending::CancelledByClient,
                ..
            }
            | Unanswered::NotAsked(Ending::CancelledByClient),
        ) => None,
        Err(Unanswered::Closed { ending, detail, .. }) => Some(closed_error(ending, &detail)),
        Err(Unanswered::NotAsked(ending)) => Some(closed_error(ending, ending.sentence())),
        Err(Unanswered::Failed(why)) => Some(tool_error(&why)),
    }
}

/// The tool result for a question that ended without an answer.
fn closed_error(ending: Ending, detail: &str) -> Value {
    tool_error(&format!(
        "devflow_ask closed: {} — {detail}",
        ending.reason()
    ))
}

/// The tool result for an answered question.
fn answered_result(answer: &AskAnswer) -> Value {
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

/// Refuse a call unless this server is bound to the live run it names (D-03):
/// the saved state exists, carries `--run-started-at` and `--stage`, and the
/// phase's runner lock is held.
fn check_run_binding(args: &AskArgs) -> Result<(), String> {
    let phase = args.phase;
    let state = workflow::load_state(&args.project, phase)
        .map_err(|error| format!("devflow_ask refused: phase {phase} has no run state: {error}"))?;
    if state.started_at != args.run_started_at {
        return Err(format!(
            "devflow_ask refused: this server belongs to the run started at {}, but phase \
             {phase}'s run started at {}",
            args.run_started_at, state.started_at
        ));
    }
    if state.stage != args.stage {
        return Err(format!(
            "devflow_ask refused: this server is for stage {}, but the run is at {}",
            args.stage, state.stage
        ));
    }
    match liveness::probe(&liveness::runner_lock_path(&args.project, phase)) {
        Ok(Held::Live) => Ok(()),
        Ok(Held::Abandoned | Held::Missing) => Err(format!(
            "devflow_ask refused: phase {phase}'s runner is not live, so no one would read \
             the question"
        )),
        Err(error) => Err(format!(
            "devflow_ask refused: could not probe phase {phase}'s runner lock: {error}"
        )),
    }
}

/// The gate half of [`ask`]: take the stage's ask lock, open, wait, read the
/// choice, claim, finish the registration and close the question's own gate.
/// The registration is finished and the gate closed on every exit path; the
/// close is identity checked, so a gate that replaced this one survives. The
/// ask lock is held until the question has closed.
fn ask_gate(
    args: &AskArgs,
    spec: &QuestionSpec,
    control: &CallControl<'_>,
) -> Result<AskAnswer, Unanswered> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let watch = RunWatch::new(args, control);
    let _ask_lock = take_ask_lock(args, spec, &watch)?;
    let (opened, registration) = Gates::open_question_registered(root, phase, stage, spec)
        .map_err(|error| {
            Unanswered::Failed(format!(
                "devflow_ask could not open the question gate: {error}"
            ))
        })?;
    let gate_id = opened.gate_id;
    eprintln!(
        "devflow __ask: question gate .devflow/gates/{}-{stage}.json written (gate {gate_id}); \
         awaiting `devflow gate answer`",
        phase.padded()
    );
    let notifier = Notifier::spawn(args, spec);
    emit_question_asked(args, spec, &gate_id);
    let result = match await_bound_answer(args, &gate_id, &watch) {
        Ok(Some(answer)) => consume(args, &gate_id, spec, &answer),
        Ok(None) => Err(Unanswered::closed(
            watch.ending.get().unwrap_or(Ending::TimedOut),
        )),
        Err(why) => Err(Unanswered::Failed(why)),
    };
    registration.finish();
    if let Err(error) = Gates::close_if_current(root, phase, stage, &gate_id) {
        eprintln!("devflow __ask: could not close phase {phase} {stage}'s gate {gate_id}: {error}");
    }
    notifier.finish();
    match &result {
        Ok(answer) => emit_question_answered(args, &gate_id, answer),
        Err(Unanswered::Closed {
            ending, closed_by, ..
        }) => emit_question_closed(args, spec, &gate_id, *ending, closed_by.as_deref()),
        Err(Unanswered::NotAsked(_) | Unanswered::Failed(_)) => {}
    }
    result
}

/// Take the stage's ask lock, retrying every [`ASK_LOCK_RETRY`] while another
/// question holds it, until the call ends or its time (counted from arrival)
/// runs out. A call that ends here opened no gate, so no question event is
/// recorded; a timed-out one logs its question id to stderr.
fn take_ask_lock(
    args: &AskArgs,
    spec: &QuestionSpec,
    watch: &RunWatch<'_>,
) -> Result<liveness::Hold, Unanswered> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    loop {
        if watch.ended() {
            return Err(Unanswered::NotAsked(
                watch.ending.get().unwrap_or(Ending::TimedOut),
            ));
        }
        let left = watch.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            eprintln!(
                "devflow __ask: question {} timed out before its gate opened",
                spec.question_id
            );
            return Err(Unanswered::NotAsked(Ending::TimedOut));
        }
        match liveness::claim_ask_lock(root, phase, stage) {
            Ok(Ok(hold)) => return Ok(hold),
            Ok(Err(_held)) => std::thread::sleep(ASK_LOCK_RETRY.min(left)),
            Err(error) => {
                return Err(Unanswered::Failed(format!(
                    "devflow_ask could not take phase {phase} {stage}'s ask lock: {error}"
                )));
            }
        }
    }
}

/// What ends a held question from outside, polled on every cancel check of the
/// answer wait: the client's endings at once, the run's state and runner lock
/// at most once per [`RUN_CHECK_INTERVAL`]. The first ending seen is kept.
struct RunWatch<'a> {
    args: &'a AskArgs,
    control: &'a CallControl<'a>,
    /// The call's arrival plus `--timeout-secs`.
    deadline: Instant,
    last_run_check: Cell<Option<Instant>>,
    ending: Cell<Option<Ending>>,
}

impl<'a> RunWatch<'a> {
    fn new(args: &'a AskArgs, control: &'a CallControl<'a>) -> Self {
        Self {
            args,
            control,
            deadline: control.arrived + Duration::from_secs(args.timeout_secs),
            last_run_check: Cell::new(None),
            ending: Cell::new(None),
        }
    }

    /// Whether the question has ended; the ending is kept in `ending`.
    fn ended(&self) -> bool {
        if self.ending.get().is_none() {
            let ending = self.control.ending().or_else(|| self.run_ending());
            self.ending.set(ending);
        }
        self.ending.get().is_some()
    }

    /// The run stopped or its runner gone, read at most once per interval.
    fn run_ending(&self) -> Option<Ending> {
        if self
            .last_run_check
            .get()
            .is_some_and(|checked| checked.elapsed() < RUN_CHECK_INTERVAL)
        {
            return None;
        }
        self.last_run_check.set(Some(Instant::now()));
        let (root, phase) = (self.args.project.as_path(), self.args.phase);
        if workflow::load_state(root, phase).is_ok_and(|state| state.stopped) {
            return Some(Ending::RunStopped);
        }
        match liveness::probe(&liveness::runner_lock_path(root, phase)) {
            Ok(Held::Live) => None,
            Ok(Held::Abandoned | Held::Missing) => Some(Ending::RunnerGone),
            Err(error) => {
                eprintln!("devflow __ask: could not probe phase {phase}'s runner lock: {error}");
                None
            }
        }
    }
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

/// Record `question_closed` for a question that ended without an answer (D-15).
fn emit_question_closed(
    args: &AskArgs,
    spec: &QuestionSpec,
    gate_id: &GateId,
    ending: Ending,
    closed_by: Option<&str>,
) {
    events::emit(
        &args.project,
        args.phase,
        events::QUESTION_CLOSED,
        json!({
            "stage": args.stage.to_string(),
            "question_id": spec.question_id,
            "gate_id": gate_id.as_str(),
            "reason": ending.reason(),
            "closed_by": closed_by,
        }),
    );
}

/// [`Gates::await_answer`] with the time left before the call's deadline, setting aside each file in the
/// slot that does not bind to `gate_id`. `None` on a timeout or on an ending
/// `watch` saw (kept in it).
fn await_bound_answer(
    args: &AskArgs,
    gate_id: &GateId,
    watch: &RunWatch<'_>,
) -> Result<Option<GateAnswer>, String> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    loop {
        // The time left before the call's deadline, to the nearest whole second.
        let left = (watch.deadline.saturating_duration_since(Instant::now())
            + Duration::from_millis(500))
        .as_secs();
        match Gates::await_answer(root, phase, stage, gate_id, left, || watch.ended()) {
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
) -> Result<AskAnswer, Unanswered> {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let choice = Gates::read_question_choice(root, phase, stage, gate_id).map_err(|error| {
        Unanswered::Failed(format!(
            "devflow_ask could not read the answer's choice: {error}"
        ))
    })?;
    match Gates::claim(root, phase, stage, gate_id) {
        Ok(Claim::Claimed) => {}
        Ok(Claim::AlreadyConsumed) => {
            return Err(Unanswered::Failed(
                "devflow_ask: the answer was already consumed by another process".into(),
            ));
        }
        Err(error) => {
            return Err(Unanswered::Failed(format!(
                "devflow_ask could not claim the answer: {error}"
            )));
        }
    }
    if answer.response.abort {
        let reason = answer.response.note.as_deref().unwrap_or("no reason given");
        return Err(Unanswered::Closed {
            ending: Ending::AbortedByOperator,
            detail: format!("the operator aborted the run at this question ({reason}); stop work"),
            closed_by: answer.response.responded_by.clone(),
        });
    }
    let Some(choice) = choice else {
        return Err(Unanswered::Failed(
            "devflow_ask: the question's answer carries no choice".into(),
        ));
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
    use std::sync::mpsc;

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
                run_server(std::io::BufReader::new(reader), output, move |_, _| {
                    let _ = entered.send(());
                    let _ = released.recv();
                    Some(json!({ "content": [] }))
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
        assert!(
            reply_with_id(&output, 1, REPLY_WAIT).is_some(),
            "the released call replies"
        );
        drop(writer);
        assert_eq!(server.join().unwrap(), 0);
    }

    #[test]
    fn a_call_for_another_tool_is_a_protocol_error() {
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let calls = Arc::new(Mutex::new(0_u32));
        let input = format!("{}\n", ask_call(3, "other"));
        let code = run_server(std::io::Cursor::new(input), Arc::clone(&output), {
            let calls = Arc::clone(&calls);
            move |_, _| {
                *calls.lock().unwrap() += 1;
                Some(json!({ "content": [] }))
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

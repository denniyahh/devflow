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
//!
//! With `--hold-cap-secs` (operator answer OD-1 (b)) a call is held at most
//! that long: at the cap it replies `pending` with its `question_id` and the
//! question stays open at its gate, and a re-call carrying that id continues
//! it. The timeout counts from the question's first call, held or not.
//!
//! With triage on (`[triage]` in `devflow.toml`, not turned off for the run),
//! a question's first call is first scored by Jev (Phase 54, TRIAGE-01). A
//! score at or above both thresholds answers it at once, with no gate, once
//! both of its event lines are on the log; anything else, including every
//! failure, sends it down the gate path above unchanged (D-12, D-19).

use crate::pipeline_gate::fire_gate_notify_events;
use crate::triage_client::{self, TransportFailure};
use devflow_core::gates::{
    AwaitOutcome, Claim, GateAnswer, GateId, GateKind, Gates, WaiterRegistration,
};
use devflow_core::liveness;
use devflow_core::phase_id::PhaseId;
use devflow_core::question::{
    AnswererKind, AskAnswer, QuestionChoice, QuestionSpec, answerer_kind, render_question_context,
};
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::triage::{FallbackReason, TriageConfig, TriageOutcome};
use devflow_core::{config, corpus, events, triage, workflow};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
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

/// The time a question keeps, after its Jev call, to take the stage's ask
/// lock and open its gate should triage not decide it.
const TRIAGE_GATE_RESERVE: Duration = Duration::from_secs(5);

/// The least time a Jev call is given; with less left before the question's
/// deadline the model is not asked.
const MIN_TRIAGE_BUDGET: Duration = Duration::from_secs(1);

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
    /// The longest one call is held, in seconds; `None` holds it until the
    /// question ends.
    pub(crate) hold_cap_secs: Option<u64>,
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
    /// The id, as its JSON text, of the call the worker has taken and not finished.
    /// Set and cleared under the `queue` lock, so a cancel that reads both under that
    /// lock sees a call as queued or in flight, never neither.
    in_flight: Mutex<Option<String>>,
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
    run_worker(input, output, AskServer::new(args))
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

/// What the server loop's worker thread runs.
pub(crate) trait Worker {
    /// One `tools/call`'s params: the tool result, or `None` to send no reply
    /// (a cancelled call).
    fn call(&mut self, params: &Value, control: &CallControl<'_>) -> Option<Value>;
    /// No call is held: end what has run out. Called about every [`IDLE_POLL`].
    fn idle(&mut self) {}
    /// The loop is ending, at stdin's end or a caught signal.
    fn shut_down(&mut self, _ending: Ending) {}
}

/// A worker that only answers calls.
#[cfg(test)]
struct CallFn<F>(F);

#[cfg(test)]
impl<F: FnMut(&Value, &CallControl<'_>) -> Option<Value>> Worker for CallFn<F> {
    fn call(&mut self, params: &Value, control: &CallControl<'_>) -> Option<Value> {
        (self.0)(params, control)
    }
}

/// [`run_worker`] with a worker that only answers calls.
#[cfg(test)]
pub(crate) fn run_server<R, W>(
    input: R,
    output: Arc<Mutex<W>>,
    call: impl FnMut(&Value, &CallControl<'_>) -> Option<Value>,
) -> i32
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    run_worker(input, output, CallFn(call))
}

/// The server loop over any input and output, so tests drive it without stdio.
/// A reader thread reads `input`; the calling thread is the worker, running
/// each `tools/call`'s params in arrival order and idling between calls. The
/// loop ends at stdin's end or a caught signal, after the call in hand has
/// cleaned up; the worker then shuts down with that ending.
pub(crate) fn run_worker<R, W>(input: R, output: Arc<Mutex<W>>, mut worker: impl Worker) -> i32
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
    while let Some(pending) = next_call(&flags, &mut worker) {
        let control = CallControl {
            flags: &flags,
            id_text: &pending.id_text,
            arrived: pending.arrived,
        };
        let result = worker.call(&pending.params, &control);
        {
            let _queue = lock(&flags.queue);
            *lock(&flags.in_flight) = None;
        }
        // After stdin's end the client is gone: nothing more goes to stdout.
        if let Some(result) = result
            && !flags.eof.load(Ordering::SeqCst)
        {
            write_message(&output, &response(&pending.id, result));
        }
        lock(&flags.cancelled).remove(&pending.id_text);
    }
    let signal = caught_signal();
    worker.shut_down(if signal.is_some() {
        Ending::Terminated
    } else {
        Ending::ClientDisconnected
    });
    signal.map_or(0, |signal| 128 + signal)
}

/// The next queued call, or `None` once stdin has ended or a signal arrived.
fn next_call(flags: &CallFlags, worker: &mut impl Worker) -> Option<PendingCall> {
    let mut queue = lock(&flags.queue);
    loop {
        if flags.eof.load(Ordering::SeqCst) || caught_signal().is_some() {
            return None;
        }
        if let Some(pending) = queue.pop_front() {
            *lock(&flags.in_flight) = Some(pending.id_text.clone());
            return Some(pending);
        }
        drop(queue);
        worker.idle();
        queue = lock(&flags.queue);
        if !queue.is_empty() {
            continue;
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
/// taken by the worker sees its id in the cancelled set. A cancel for an id
/// neither queued nor in flight is dropped (D-12): JSON-RPC ids may be reused,
/// and a recorded id nothing would remove would silence the next call with it.
/// Both reads happen under the queue lock, so a pop between them cannot drop a
/// real cancel.
fn cancel(flags: &CallFlags, request_id: &Value) {
    let id_text = request_id.to_string();
    let mut queue = lock(&flags.queue);
    let queued = queue.len();
    queue.retain(|pending| pending.id_text != id_text);
    if queue.len() == queued && lock(&flags.in_flight).as_deref() == Some(id_text.as_str()) {
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
            returns the option the operator chose, or a `pending` result naming a question_id \
            to call again with while the question stays open.",
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
                "recommended": { "type": "string", "description": "An offered option's id." },
                "question_id": {
                    "type": "string",
                    "description": "Only on a re-call: the question_id of a pending reply, \
                        to keep waiting on that same question."
                }
            },
            "required": ["question", "options", "kind"]
        }
    })
}

/// A tool result the agent reads as a failure, naming why.
fn tool_error(why: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": why }], "isError": true })
}

/// The question a call asks, under a fresh question id, beside the
/// `question_id` a re-call carries, or why it was refused (D-07, D-08). A
/// refusal is returned before any gate is touched, so it blocks neither this
/// call nor the calls queued behind it.
fn parse_ask_arguments(arguments: &Value) -> Result<(QuestionSpec, Option<String>), String> {
    QuestionSpec::from_call_arguments(arguments)
        .map_err(|error| format!("devflow_ask refused: {error}"))
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
    /// The operator aborted, but the abort could not be saved as the run's
    /// `abort_requested`. The answer is left unclaimed on its open gate, so the
    /// runner's question-gate backstop still applies it; the text is the
    /// reply's detail, which tells the agent to stop.
    AbortUnsaved(String),
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

/// A question this server opened: its gate, held until the question ends,
/// across every call that holds it (operator answer OD-1 (b)).
struct OpenGate {
    gate_id: GateId,
    registration: WaiterRegistration,
    notifier: Notifier,
    /// The stage's ask lock, released once the question has closed.
    ask_lock: liveness::Hold,
}

/// A question asked and not yet ended. A capped call that reaches its cap
/// leaves it here, and a re-call carrying its `question_id` takes it back.
struct OpenQuestion {
    spec: QuestionSpec,
    /// The first call's arrival plus `--timeout-secs` (T counts from there).
    deadline: Instant,
    /// `None` while the question waits for the stage's ask lock.
    gate: Option<OpenGate>,
    /// The `triage` object of `question_asked`, set once on the first call
    /// and recorded when the gate opens; `None` when triage did not run.
    triage: Option<Value>,
    /// Why triage could not score the question, and its detail, recorded as
    /// `triage_fallback` when the gate opens; `None` for any other outcome.
    fallback: Option<(FallbackReason, String)>,
}

/// What triage did with a question's first call.
enum Triaged {
    /// Triage answered the question and both of its lines are on the log.
    Answered(AskAnswer),
    /// The call ended, or was refused, while the model was asked: this is its
    /// reply, `None` for a call the client cancelled.
    Ended(Option<Value>),
    /// The question goes to the operator's gate.
    ToGate,
}

/// The `devflow_ask` worker: each call, and the questions left open between
/// calls by a hold cap.
struct AskServer {
    args: AskArgs,
    /// The questions no call holds, by question id.
    open: HashMap<String, OpenQuestion>,
    /// The reply each question this server ended gave, by question id, which
    /// a later re-call of that id gets again.
    finished: HashMap<String, Value>,
    /// When the idle worker last read the run state and the runner lock.
    last_run_check: Option<Instant>,
}

/// What became of a call's hold on its question.
enum CallOutcome {
    /// The question ended; this is its result.
    Ended(Result<AskAnswer, Unanswered>),
    /// The call's cap came first; the question stays open.
    Pending,
}

impl Worker for AskServer {
    fn call(&mut self, params: &Value, control: &CallControl<'_>) -> Option<Value> {
        let (spec, recall_id) =
            match parse_ask_arguments(params.get("arguments").unwrap_or(&Value::Null)) {
                Ok(parsed) => parsed,
                Err(why) => return Some(tool_error(&why)),
            };
        let state = match check_run_binding(&self.args) {
            Ok(state) => state,
            Err(why) => return Some(tool_error(&why)),
        };
        let question = match recall_id {
            None => {
                let mut question = OpenQuestion {
                    spec,
                    deadline: control.arrived + Duration::from_secs(self.args.timeout_secs),
                    gate: None,
                    triage: None,
                    fallback: None,
                };
                let reply = match self.triage(&mut question, &state, control) {
                    Triaged::ToGate => return self.hold(question, control),
                    Triaged::Answered(answer) => Some(answered_result(&answer)),
                    Triaged::Ended(reply) => reply,
                };
                self.finished.insert(
                    question.spec.question_id,
                    reply.clone().unwrap_or_else(|| {
                        closed_error(
                            Ending::CancelledByClient,
                            Ending::CancelledByClient.sentence(),
                        )
                    }),
                );
                return reply;
            }
            Some(id) => match self.open.remove(&id) {
                Some(question) => question,
                None => {
                    return Some(self.finished.get(&id).cloned().unwrap_or_else(|| {
                        tool_error(&format!(
                            "devflow_ask refused: question_id `{id}` names no question this \
                             server asked; call without a question_id to ask a new question"
                        ))
                    }));
                }
            },
        };
        self.hold(question, control)
    }

    fn idle(&mut self) {
        if self.open.is_empty() {
            return;
        }
        let now = Instant::now();
        let timed_out: Vec<String> = self
            .open
            .iter()
            .filter(|(_, question)| question.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in timed_out {
            self.close_unheld(&id, Ending::TimedOut);
        }
        if self
            .last_run_check
            .is_some_and(|checked| checked.elapsed() < RUN_CHECK_INTERVAL)
        {
            return;
        }
        self.last_run_check = Some(now);
        if let Some(ending) = run_ending(&self.args) {
            self.close_all(ending);
        }
    }

    fn shut_down(&mut self, ending: Ending) {
        self.close_all(ending);
    }
}

impl AskServer {
    fn new(args: AskArgs) -> Self {
        Self {
            args,
            open: HashMap::new(),
            finished: HashMap::new(),
            last_run_check: None,
        }
    }

    /// Score a first-time question with Jev, when triage is on for this run.
    /// A decision is returned only after the call is re-checked (ended,
    /// deadline, run binding) and both of its event lines are appended; every
    /// other outcome leaves the `triage` object on `question` for its gate.
    fn triage(
        &self,
        question: &mut OpenQuestion,
        state: &State,
        control: &CallControl<'_>,
    ) -> Triaged {
        let config = config::triage_config(&self.args.project);
        if !config.enabled || state.triage_off {
            return Triaged::ToGate;
        }
        let mut outcome = score_question(&self.args, &config, &question.spec, question.deadline);
        if let Some(ending) = control.ending().or_else(|| run_ending(&self.args)) {
            return Triaged::Ended(ended_reply(Err(Unanswered::NotAsked(ending))));
        }
        if Instant::now() < question.deadline {
            if let Err(why) = check_run_binding(&self.args) {
                return Triaged::Ended(Some(tool_error(&why)));
            }
            if let TriageOutcome::Decided { option_id, .. } = &outcome {
                let fields = outcome.event_fields(&config);
                match record_decision(&self.args, &question.spec, option_id, fields) {
                    Ok(answer) => return Triaged::Answered(answer),
                    Err(error) => {
                        eprintln!(
                            "devflow __ask: could not record triage's answer to question {} \
                             ({error}); asking the operator",
                            question.spec.question_id
                        );
                        outcome = TriageOutcome::Fallback {
                            reason: FallbackReason::LedgerUnwritable,
                            detail: "the event log could not be appended".to_string(),
                        };
                    }
                }
            }
        }
        // Past the deadline the question goes to `hold`, which ends it as
        // timed out before any gate opens.
        question.triage = Some(outcome.event_fields(&config));
        if let TriageOutcome::Fallback { reason, detail } = outcome {
            question.fallback = Some((reason, detail));
        }
        Triaged::ToGate
    }

    /// Hold `question` for this call: open its gate if it has none, wait for
    /// its answer until its T or this call's cap, and reply. `None` for a call
    /// the client cancelled, which gets no reply.
    fn hold(&mut self, mut question: OpenQuestion, control: &CallControl<'_>) -> Option<Value> {
        let cap = self
            .args
            .hold_cap_secs
            .map(|cap| control.arrived + Duration::from_secs(cap));
        let hold_until = cap.map_or(question.deadline, |cap| cap.min(question.deadline));
        let watch = RunWatch::new(&self.args, control, hold_until);
        let held = hold_question(&self.args, &mut question, &watch);
        let id = question.spec.question_id.clone();
        let reply = match held {
            CallOutcome::Pending => {
                let reply = pending_result(&id, question.gate.is_some());
                self.open.insert(id, question);
                return Some(reply);
            }
            CallOutcome::Ended(result) => {
                if let Some(gate) = question.gate.take() {
                    close_gate(&self.args, &question.spec, gate, &result);
                }
                ended_reply(result)
            }
        };
        self.finished.insert(
            id,
            reply.clone().unwrap_or_else(|| {
                closed_error(
                    Ending::CancelledByClient,
                    Ending::CancelledByClient.sentence(),
                )
            }),
        );
        reply
    }

    /// End the open question `id`, which no call holds, with `ending`.
    fn close_unheld(&mut self, id: &str, ending: Ending) {
        let Some(mut question) = self.open.remove(id) else {
            return;
        };
        match question.gate.take() {
            Some(gate) => close_gate(
                &self.args,
                &question.spec,
                gate,
                &Err(Unanswered::closed(ending)),
            ),
            None => eprintln!(
                "devflow __ask: question {id} ended ({}) before its gate opened",
                ending.reason()
            ),
        }
        self.finished
            .insert(id.to_string(), closed_error(ending, ending.sentence()));
    }

    /// End every open question no call holds with `ending`.
    fn close_all(&mut self, ending: Ending) {
        let ids: Vec<String> = self.open.keys().cloned().collect();
        for id in ids {
            self.close_unheld(&id, ending);
        }
    }
}

/// The reply for a question that ended, `None` for one the client cancelled.
fn ended_reply(result: Result<AskAnswer, Unanswered>) -> Option<Value> {
    match result {
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
        Err(Unanswered::AbortUnsaved(detail)) => {
            Some(closed_error(Ending::AbortedByOperator, &detail))
        }
    }
}

/// The reply at a call's cap: not an error, naming the question id the
/// re-call carries. `asked` says whether the question's gate is open yet.
fn pending_result(question_id: &str, asked: bool) -> Value {
    let text = if asked {
        format!(
            "No answer yet. Call devflow_ask again with the same arguments and question_id \
             {question_id}; the question stays open for the operator."
        )
    } else {
        format!(
            "Not asked yet: another question holds this stage. Call devflow_ask again with \
             the same arguments and question_id {question_id}."
        )
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": { "status": "pending", "question_id": question_id },
    })
}

/// The tool result for a question that ended without an answer.
fn closed_error(ending: Ending, detail: &str) -> Value {
    tool_error(&format!(
        "devflow_ask closed: {} — {detail}",
        ending.reason()
    ))
}

/// The tool result for an answered question, naming who answered it.
fn answered_result(answer: &AskAnswer) -> Value {
    let by = &answer.answered_by;
    let text = match (answerer_kind(by), &answer.answer) {
        (AnswererKind::Triage, QuestionChoice::Option { option_id }) => format!(
            "DevFlow triage (Jev) chose option `{option_id}` on the operator's triage policy."
        ),
        (AnswererKind::Triage, QuestionChoice::Text { text }) => {
            format!("DevFlow triage (Jev) answered on the operator's triage policy: {text}")
        }
        (AnswererKind::Agent, QuestionChoice::Option { option_id }) => {
            format!("An agent answering for the operator (`{by}`) chose option `{option_id}`.")
        }
        (AnswererKind::Agent, QuestionChoice::Text { text }) => {
            format!("An agent answering for the operator (`{by}`) answered: {text}")
        }
        (_, QuestionChoice::Option { option_id }) => {
            format!("The operator chose option `{option_id}`.")
        }
        (_, QuestionChoice::Text { text }) => format!("The operator answered: {text}"),
    };
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": answer,
    })
}

/// Refuse a call unless this server is bound to the live run it names (D-03):
/// the saved state exists, carries `--run-started-at` and `--stage`, and the
/// phase's runner lock is held. Returns the saved state it read.
fn check_run_binding(args: &AskArgs) -> Result<State, String> {
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
        Ok(liveness::Held::Live) => Ok(state),
        Ok(liveness::Held::Abandoned | liveness::Held::Missing) => Err(format!(
            "devflow_ask refused: phase {phase}'s runner is not live, so no one would read \
             the question"
        )),
        Err(error) => Err(format!(
            "devflow_ask refused: could not probe phase {phase}'s runner lock: {error}"
        )),
    }
}

/// One call's hold on `question`: take the stage's ask lock and open the gate
/// if the question has none, then wait for its answer and read the choice.
/// The call's time ends at `watch.deadline`: the question's T, or the call's
/// cap when that comes first, in which case the question is left open.
fn hold_question(args: &AskArgs, question: &mut OpenQuestion, watch: &RunWatch<'_>) -> CallOutcome {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let spec = &question.spec;
    let capped =
        |watch: &RunWatch<'_>| watch.ending.get().is_none() && Instant::now() < question.deadline;
    if question.gate.is_none() {
        let ask_lock = match take_ask_lock(args, spec, watch) {
            Ok(hold) => hold,
            Err(Unanswered::NotAsked(Ending::TimedOut)) if capped(watch) => {
                return CallOutcome::Pending;
            }
            Err(unanswered) => return CallOutcome::Ended(Err(unanswered)),
        };
        let (opened, registration) = match Gates::open_question_registered(root, phase, stage, spec)
        {
            Ok(opened) => opened,
            Err(error) => {
                return CallOutcome::Ended(Err(Unanswered::Failed(format!(
                    "devflow_ask could not open the question gate: {error}"
                ))));
            }
        };
        let gate_id = opened.gate_id;
        eprintln!(
            "devflow __ask: question gate .devflow/gates/{}-{stage}.json written (gate {gate_id}); \
             awaiting `devflow gate answer`",
            phase.padded()
        );
        let notifier = Notifier::spawn(args, spec);
        emit_question_asked(args, spec, &gate_id, question.triage.as_ref());
        if let Some((reason, detail)) = &question.fallback {
            emit_triage_fallback(args, spec, *reason, detail);
        }
        question.gate = Some(OpenGate {
            gate_id,
            registration,
            notifier,
            ask_lock,
        });
    }
    let Some(gate_id) = question.gate.as_ref().map(|gate| gate.gate_id.clone()) else {
        return CallOutcome::Ended(Err(Unanswered::Failed(
            "devflow_ask: the question has no open gate".into(),
        )));
    };
    match await_bound_answer(args, &gate_id, watch) {
        Ok(Some(answer)) => CallOutcome::Ended(consume(args, &gate_id, &question.spec, &answer)),
        Ok(None) if capped(watch) => CallOutcome::Pending,
        Ok(None) => CallOutcome::Ended(Err(Unanswered::closed(
            watch.ending.get().unwrap_or(Ending::TimedOut),
        ))),
        Err(why) => CallOutcome::Ended(Err(Unanswered::Failed(why))),
    }
}

/// End a question's gate: finish the registration, close the question's own
/// gate (identity checked, so a gate that replaced it survives), stop its
/// notifier, record how it ended, and release the stage's ask lock last.
fn close_gate(
    args: &AskArgs,
    spec: &QuestionSpec,
    gate: OpenGate,
    result: &Result<AskAnswer, Unanswered>,
) {
    let (root, phase, stage) = (args.project.as_path(), args.phase, args.stage);
    let OpenGate {
        gate_id,
        registration,
        notifier,
        ask_lock,
    } = gate;
    registration.finish();
    if !keeps_gate_open(result)
        && let Err(error) = Gates::close_if_current(root, phase, stage, &gate_id)
    {
        eprintln!("devflow __ask: could not close phase {phase} {stage}'s gate {gate_id}: {error}");
    }
    notifier.finish();
    match result {
        Ok(answer) => emit_question_answered(args, &gate_id, answer),
        Err(Unanswered::Closed {
            ending, closed_by, ..
        }) => emit_question_closed(args, spec, &gate_id, *ending, closed_by.as_deref()),
        Err(Unanswered::NotAsked(_) | Unanswered::Failed(_) | Unanswered::AbortUnsaved(_)) => {}
    }
    drop(ask_lock);
}

/// Whether a question's end leaves its gate open: only an abort that could not
/// be saved, whose unclaimed answer the runner's backstop applies.
fn keeps_gate_open(result: &Result<AskAnswer, Unanswered>) -> bool {
    matches!(result, Err(Unanswered::AbortUnsaved(_)))
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
                "devflow __ask: question {} reached its time before its gate opened",
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

/// The run stopped, or its runner lock no longer held, if either is so now.
fn run_ending(args: &AskArgs) -> Option<Ending> {
    let (root, phase) = (args.project.as_path(), args.phase);
    if workflow::load_state(root, phase).is_ok_and(|state| state.stopped) {
        return Some(Ending::RunStopped);
    }
    match liveness::probe(&liveness::runner_lock_path(root, phase)) {
        Ok(liveness::Held::Live) => None,
        Ok(liveness::Held::Abandoned | liveness::Held::Missing) => Some(Ending::RunnerGone),
        Err(error) => {
            eprintln!("devflow __ask: could not probe phase {phase}'s runner lock: {error}");
            None
        }
    }
}

/// What ends a held call from outside, polled on every cancel check of the
/// answer wait: the client's endings at once, the run's state and runner lock
/// at most once per [`RUN_CHECK_INTERVAL`]. The first ending seen is kept.
struct RunWatch<'a> {
    args: &'a AskArgs,
    control: &'a CallControl<'a>,
    /// When this call's hold ends: the question's T, or the call's cap.
    deadline: Instant,
    last_run_check: Cell<Option<Instant>>,
    ending: Cell<Option<Ending>>,
}

impl<'a> RunWatch<'a> {
    fn new(args: &'a AskArgs, control: &'a CallControl<'a>, deadline: Instant) -> Self {
        Self {
            args,
            control,
            deadline,
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
        run_ending(self.args)
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

/// Record `question_asked` for `spec`'s open gate (D-15), with its `triage`
/// object when triage ran.
fn emit_question_asked(
    args: &AskArgs,
    spec: &QuestionSpec,
    gate_id: &GateId,
    triage: Option<&Value>,
) {
    events::emit(
        &args.project,
        args.phase,
        events::QUESTION_ASKED,
        question_asked_fields(args, spec, Some(gate_id), triage),
    );
}

/// Record `triage_fallback` for `spec` (D-19), beside its `question_asked`
/// once its gate is open, so a question that never got the ask lock has
/// neither. `detail` is a failure name or `HTTP <code> <reason>`, never
/// response-body text or the key.
fn emit_triage_fallback(args: &AskArgs, spec: &QuestionSpec, reason: FallbackReason, detail: &str) {
    events::emit(
        &args.project,
        args.phase,
        events::TRIAGE_FALLBACK,
        json!({
            "stage": args.stage.to_string(),
            "question_id": spec.question_id,
            "run_started_at": args.run_started_at,
            "reason": reason.as_str(),
            "detail": detail,
        }),
    );
}

/// The fields of `question_asked`; `gate_id` is absent for a question triage
/// decided, which has no gate.
fn question_asked_fields(
    args: &AskArgs,
    spec: &QuestionSpec,
    gate_id: Option<&GateId>,
    triage: Option<&Value>,
) -> Value {
    let mut fields = json!({
        "stage": args.stage.to_string(),
        "question_id": spec.question_id,
        "kind": spec.kind,
        "question": spec.question,
        "options": spec.options,
        "context": spec.context,
        "recommended": spec.recommended,
        "run_started_at": args.run_started_at,
    });
    if let Some(gate_id) = gate_id {
        fields["gate_id"] = json!(gate_id.as_str());
    }
    if let Some(triage) = triage {
        fields["triage"] = triage.clone();
    }
    fields
}

/// Append triage's answer to `spec`, `question_asked` (no gate) then
/// `question_answered`, and return it; an append error means the decision
/// must not be returned (D-14: the log is the only ledger).
fn record_decision(
    args: &AskArgs,
    spec: &QuestionSpec,
    option_id: &str,
    triage_fields: Value,
) -> std::io::Result<AskAnswer> {
    let (root, phase) = (args.project.as_path(), args.phase);
    events::try_emit(
        root,
        phase,
        events::QUESTION_ASKED,
        question_asked_fields(args, spec, None, Some(&triage_fields)),
    )?;
    let answer = AskAnswer {
        question_id: spec.question_id.clone(),
        answer: QuestionChoice::Option {
            option_id: option_id.to_string(),
        },
        answered_by: triage::TRIAGE_ANSWERER.to_string(),
        answered_at: unix_now(),
    };
    events::try_emit(
        root,
        phase,
        events::QUESTION_ANSWERED,
        json!({
            "stage": args.stage.to_string(),
            "question_id": answer.question_id,
            "answer": answer.answer,
            "policy": events::ANSWERING_POLICY_TRIAGE,
            "answered_by": answer.answered_by,
        }),
    )?;
    Ok(answer)
}

/// Score `spec` with Jev under `config`. A `blocking-human` question, an
/// unset threshold or too little time left sends no request; every failure
/// is a fallback, which escalates (D-08, D-07, D-19). The call is given at
/// most the question's time left less [`TRIAGE_GATE_RESERVE`].
fn score_question(
    args: &AskArgs,
    config: &TriageConfig,
    spec: &QuestionSpec,
    deadline: Instant,
) -> TriageOutcome {
    if let Some(outcome) = triage::pre_decide(spec.kind, config) {
        return outcome;
    }
    let precedents = corpus::precedents_for(&args.project, config, spec);
    let request = triage::jev_request(spec, &precedents, &config.model);
    let left = deadline
        .saturating_duration_since(Instant::now())
        .saturating_sub(TRIAGE_GATE_RESERVE);
    let budget = config.timeout.min(left);
    if budget < MIN_TRIAGE_BUDGET {
        return TriageOutcome::Fallback {
            reason: FallbackReason::AskBudgetSpent,
            detail: format!(
                "{} ms left for the model call, under the {} s minimum",
                budget.as_millis(),
                MIN_TRIAGE_BUDGET.as_secs()
            ),
        };
    }
    let body = match triage_client::endpoint_from_env(budget)
        .and_then(|endpoint| triage_client::post_systemone(&endpoint, &request))
    {
        Ok(body) => body,
        Err(failure) => return transport_fallback(&failure),
    };
    match triage::parse_jev_response(&body, spec, &config.model) {
        Ok(scores) => triage::decide(&scores, config),
        Err((reason, detail)) => TriageOutcome::Fallback { reason, detail },
    }
}

/// A transport failure as its fallback; the failure's text never carries the
/// key or the endpoint's reply.
fn transport_fallback(failure: &TransportFailure) -> TriageOutcome {
    TriageOutcome::Fallback {
        reason: failure.fallback_reason(),
        detail: failure.to_string(),
    }
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
    if answer.response.abort
        && let Err(error) = record_abort_request(args, spec, answer)
    {
        return Err(Unanswered::AbortUnsaved(format!(
            "the operator aborted the run at this question, but the abort could not be saved \
             ({error}); it stays on the open gate for the runner to apply; stop work"
        )));
    }
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

/// Commit the operator's abort as the run's `abort_requested` (D-10), before
/// the answer is claimed, the gate closed or the reply sent, so a server killed
/// at any later point leaves the request for the runner to apply. A failed
/// commit is returned: the caller leaves the answer unclaimed on its open gate.
fn record_abort_request(
    args: &AskArgs,
    spec: &QuestionSpec,
    answer: &GateAnswer,
) -> Result<(), workflow::WorkflowError> {
    let note = abort_note(answer.response.note.as_deref(), &spec.question_id);
    let committed = workflow::update_state(&args.project, args.phase, |state| {
        state.abort_requested = Some(note);
        Ok::<(), workflow::WorkflowError>(())
    });
    if let Err(error) = &committed {
        eprintln!(
            "devflow __ask: phase {}: the operator's abort was not saved in the run state; \
             it stays on the open question gate for the runner to apply: {error}",
            args.phase
        );
    }
    committed.map(|_| ())
}

/// The note an abort from a question records: the operator's own, or one naming
/// the question.
pub(crate) fn abort_note(note: Option<&str>, question_id: &str) -> String {
    note.map_or_else(
        || format!("aborted from question {question_id}"),
        str::to_string,
    )
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

    /// How many replies with `id` have been written, waiting at most `wait` for `count`.
    fn replies_with_id(output: &Mutex<Vec<u8>>, id: u64, count: usize, wait: Duration) -> usize {
        let started = Instant::now();
        loop {
            let seen = replies(output)
                .iter()
                .filter(|reply| reply["id"] == id)
                .count();
            if seen >= count || started.elapsed() >= wait {
                return seen;
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

    /// D-12: JSON-RPC ids may be reused, so a cancel that arrives after its call
    /// finished must not silence a later call with the same id.
    #[test]
    fn a_late_cancel_does_not_silence_a_reused_id() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let server = std::thread::spawn({
            let output = Arc::clone(&output);
            move || {
                run_server(std::io::BufReader::new(reader), output, |_, control| {
                    control.ending().is_none().then(|| json!({ "content": [] }))
                })
            }
        });
        writeln!(writer, "{}", ask_call(7, ASK_TOOL)).unwrap();
        assert!(
            reply_with_id(&output, 7, REPLY_WAIT).is_some(),
            "the first call 7 replies"
        );
        // The worker takes 9 only after it finished 7, so 7 is no longer in flight.
        writeln!(writer, "{}", ask_call(9, ASK_TOOL)).unwrap();
        assert!(
            reply_with_id(&output, 9, REPLY_WAIT).is_some(),
            "call 9 replies"
        );
        let late_cancel = json!({
            "jsonrpc": "2.0",
            "method": "notifications/cancelled",
            "params": { "requestId": 7 }
        });
        writeln!(writer, "{late_cancel}").unwrap();
        // The reader handles lines in order, so the ping's reply means the cancel was read.
        writeln!(writer, "{}", request(10, "ping", json!({}))).unwrap();
        assert!(
            reply_with_id(&output, 10, REPLY_WAIT).is_some(),
            "the ping replies"
        );
        writeln!(writer, "{}", ask_call(7, ASK_TOOL)).unwrap();
        assert_eq!(
            replies_with_id(&output, 7, 2, REPLY_WAIT),
            2,
            "a late cancel for a finished id silenced a later call that reused it"
        );
        drop(writer);
        assert_eq!(server.join().unwrap(), 0);
    }

    fn pending_call(id: u64) -> PendingCall {
        PendingCall {
            id: json!(id),
            id_text: json!(id).to_string(),
            params: json!({}),
            arrived: Instant::now(),
        }
    }

    fn no_reply(_: &Value, _: &CallControl<'_>) -> Option<Value> {
        None
    }

    #[test]
    fn a_cancel_records_only_a_queued_or_in_flight_id() {
        let flags = CallFlags::default();
        // Control: a queued call is removed by its cancel and not recorded.
        lock(&flags.queue).push_back(pending_call(8));
        cancel(&flags, &json!(8));
        assert!(lock(&flags.queue).is_empty(), "the queued call is removed");
        assert!(
            !lock(&flags.cancelled).contains("8"),
            "a queued call's cancel is not recorded"
        );
        // Control: a call the worker has taken is in flight, and its cancel is recorded.
        lock(&flags.queue).push_back(pending_call(9));
        let taken = next_call(&flags, &mut CallFn(no_reply)).expect("the call is taken");
        assert_eq!(taken.id_text, "9");
        cancel(&flags, &json!(9));
        assert!(
            lock(&flags.cancelled).remove("9"),
            "an in-flight call's cancel is recorded"
        );
        *lock(&flags.in_flight) = None;
        cancel(&flags, &json!(7));
        let cancelled = lock(&flags.cancelled).clone();
        assert!(
            cancelled.is_empty(),
            "a cancel for an id that is neither queued nor in flight records nothing: \
             {cancelled:?}"
        );
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

    /// An abort the run state cannot save is left unclaimed on its still-open
    /// gate, for the runner's question-gate backstop, and the agent is still
    /// told to stop. Control: with the state saved, the same abort is recorded
    /// and its answer claimed.
    #[test]
    fn an_abort_the_state_cannot_save_stays_unclaimed_on_its_gate() {
        use devflow_core::gates::GateResponse;
        use devflow_core::mode::Mode;
        use devflow_core::state::{AgentKind, State};
        let _guard = crate::test_support::env_lock();
        for saved in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let (phase, stage) = (PhaseId::new(1311), Stage::Plan);
            if saved {
                let state = State::new(
                    phase,
                    AgentKind::Claude,
                    Mode::Supervise,
                    root.to_path_buf(),
                );
                workflow::save_state(&state).unwrap();
            }
            let spec = QuestionSpec::from_arguments(&json!({
                "question": "Pick a budget",
                "options": [{ "id": "a", "label": "A" }, { "id": "b", "label": "B" }],
                "kind": "decision",
            }))
            .unwrap();
            let (opened, _registration) =
                Gates::open_question_registered(root, phase, stage, &spec).unwrap();
            let response = GateResponse {
                approved: false,
                abort: true,
                note: Some("stop".into()),
                responded_by: Some("operator".into()),
            };
            Gates::answer(root, phase, stage, &opened.gate_id, &response).unwrap();
            let answer = GateAnswer {
                gate_id: opened.gate_id.as_str().to_string(),
                response,
            };
            let args = AskArgs {
                project: root.to_path_buf(),
                phase,
                stage,
                timeout_secs: 60,
                run_started_at: String::new(),
                hold_cap_secs: None,
            };

            let result = consume(&args, &opened.gate_id, &spec, &answer);

            let claimed = Gates::claim_path(root, phase, stage, &opened.gate_id).exists();
            if saved {
                assert!(
                    matches!(
                        result,
                        Err(Unanswered::Closed {
                            ending: Ending::AbortedByOperator,
                            ..
                        })
                    ),
                    "a saved abort closes as aborted: {result:?}"
                );
                assert!(claimed, "a saved abort claims its answer");
                assert!(!keeps_gate_open(&result), "a saved abort closes its gate");
                let state = workflow::load_state(root, phase).unwrap();
                assert_eq!(state.abort_requested.as_deref(), Some("stop"));
            } else {
                let Err(Unanswered::AbortUnsaved(detail)) = &result else {
                    panic!("an unsaved abort must stay on its gate: {result:?}");
                };
                assert!(
                    detail.contains("stop work"),
                    "the reply still says stop: {detail}"
                );
                assert!(!claimed, "an unsaved abort must leave its answer unclaimed");
                assert!(
                    keeps_gate_open(&result),
                    "an unsaved abort must keep its gate open"
                );
            }
        }
    }
}

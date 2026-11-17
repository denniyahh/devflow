//! The runner's stop primitives: one terminal-state owner per run, and the decision
//! that a stop is never a completion (R-2).
//!
//! **The one invariant.** Every stop and exit decision is taken under the
//! [`StopToken`]'s owner mutex: a stop request, the "the agent exited, do we
//! advance?" decision, and every advance safe point. A stop and the agent's natural
//! exit are therefore totally ordered, and a stop that wins the race always ends the
//! run as stopped. Group signals are sent under the same mutex, and only while the
//! exit has not been observed, so no group signal is sent after the leader is reaped.
//!
//! **Signals.** [`install_signal_pipe`] installs the process's one SIGTERM/SIGINT
//! handler, which only compares `getpid()` with the installing pid, stores the signal
//! number and `write`s one byte to a pipe; all three are async-signal-safe. No signal
//! is ever blocked: a blocked mask is inherited by every agent the process spawns
//! (RESEARCH Finding 3, probe A). [`spawn_signal_dispatcher`]'s thread reads the pipe
//! and turns each byte into [`StopToken::request_stop`].
//!
//! **Waiting.** The agent's exit is observed with `waitid(WNOWAIT)`
//! (`wait_for_exit_without_reaping`), which does not reap, so the zombie leader
//! still holds the agent's process group while leftover members are killed. Only then
//! is it reaped.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

/// The exit code a stopped run reports: the shell convention for a death by
/// SIGTERM, `128 + 15`. The monitor writes it to the exit file on
/// [`AgentOutcome::Stopped`], and the CLI after a stop honoured at an advance safe point.
pub const STOPPED_EXIT_CODE: i32 = 128 + libc::SIGTERM;

/// Who owns the run right now. Its [`as_str`](Self::as_str) label is the
/// `owner` field of a ping reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OwnerPhase {
    /// A stage's agent is running.
    AgentRunning,
    /// The agent completed and the CLI is advancing the pipeline.
    Advancing,
    /// The CLI is waiting on a gate answer.
    GateWait,
    /// The run was handed to another process for its next stage.
    HandedOff,
    /// The run is over.
    Finished,
}

impl OwnerPhase {
    /// The protocol's `owner` label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentRunning => "agent_running",
            Self::Advancing => "advancing",
            Self::GateWait => "gate_wait",
            Self::HandedOff => "handed_off",
            Self::Finished => "finished",
        }
    }
}

/// Where a stop request came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopSource {
    /// A `shutdown` request on the socket.
    Socket,
    /// A signal delivered to this process, by number.
    Signal(i32),
    /// This process found it does not own the run (D-03, D-15).
    Ownership,
}

/// What ending the agent's process group did (`end_group_while_running`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentGroupEnd {
    /// The agent was running: its group was signalled.
    Ended,
    /// The agent had already exited: nothing was signalled.
    AlreadyExited,
}

/// How the agent's run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentOutcome {
    /// The agent exited on its own with this code (`128 + n` for a death by
    /// signal `n`), and no stop was requested first. Only this outcome may
    /// advance the pipeline.
    Completed(i32),
    /// A stop was requested before the exit decision. Never advances.
    Stopped,
}

/// What a stop request does, decided by [`on_stop_request`] from the owner
/// phase at the moment the request is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopAction {
    /// The agent is running: end its process group.
    KillAgentGroup,
    /// The CLI owns the run: the stop is flagged and its next safe point
    /// stops.
    FlagForSafePoint,
    /// The run was handed off: reply `handed_off` and act on nothing.
    ReplyHandedOff,
    /// The run is already over: act on nothing.
    AlreadyFinished,
}

/// What an advance safe point does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SafePoint {
    /// No stop is requested: carry on.
    Proceed,
    /// A stop is requested: persist the stop and go no further.
    StopHere,
}

/// The decision for a stop request taken in `phase`.
pub fn on_stop_request(phase: OwnerPhase) -> StopAction {
    match phase {
        OwnerPhase::AgentRunning => StopAction::KillAgentGroup,
        OwnerPhase::Advancing | OwnerPhase::GateWait => StopAction::FlagForSafePoint,
        OwnerPhase::HandedOff => StopAction::ReplyHandedOff,
        OwnerPhase::Finished => StopAction::AlreadyFinished,
    }
}

/// The decision when the agent exits with `code`: a requested stop always
/// wins over the exit code.
pub fn on_agent_exit(stop_requested: bool, code: i32) -> AgentOutcome {
    if stop_requested {
        AgentOutcome::Stopped
    } else {
        AgentOutcome::Completed(code)
    }
}

/// The decision at an advance safe point.
pub fn at_safe_point(stop_requested: bool) -> SafePoint {
    if stop_requested {
        SafePoint::StopHere
    } else {
        SafePoint::Proceed
    }
}

/// The state the owner mutex guards.
#[derive(Debug)]
pub(crate) struct OwnerState {
    pub(crate) phase: OwnerPhase,
    pub(crate) stop_requested: bool,
    /// The first stop's source, for the log.
    pub(crate) stop_source: Option<StopSource>,
    /// The main thread has observed the agent's exit. Set under the lock
    /// before the leader is reaped, so a group signal sent under the lock
    /// while this is false always reaches an unreaped group.
    pub(crate) agent_exited: bool,
    /// The run ended as a stop: [`AgentOutcome::Stopped`] or a safe point
    /// that returned [`SafePoint::StopHere`].
    pub(crate) ended_stopped: bool,
    /// A stop signalled the agent's process group: the final `stopped` reply says so.
    pub(crate) group_signalled: bool,
    /// The process group of the agent the run's current stage spawned, for the runner's
    /// signal dispatcher ([`spawn_signal_dispatcher`]). Set by [`StopToken::begin_agent`].
    pub(crate) agent_pgid: Option<u32>,
}

/// The owner mutex and the condition variable a signalling thread waits on
/// for the agent's exit.
#[derive(Debug)]
pub(crate) struct Owner {
    pub(crate) state: Mutex<OwnerState>,
    pub(crate) exited: Condvar,
}

/// A handle to the run's single terminal-state owner. Cloning shares the
/// owner; every method takes the owner mutex, so their decisions are totally
/// ordered with each other.
#[derive(Debug, Clone)]
pub struct StopToken {
    pub(crate) owner: Arc<Owner>,
}

impl StopToken {
    pub(crate) fn new(phase: OwnerPhase) -> Self {
        Self {
            owner: Arc::new(Owner {
                state: Mutex::new(OwnerState {
                    phase,
                    stop_requested: false,
                    stop_source: None,
                    agent_exited: false,
                    ended_stopped: false,
                    group_signalled: false,
                    agent_pgid: None,
                }),
                exited: Condvar::new(),
            }),
        }
    }

    /// The token of one foreground run (D-02): the runner owns every stage's agent in
    /// its own process, so one token spans the whole run. It starts in
    /// [`OwnerPhase::Advancing`]; [`crate::monitor::run_stage`] moves it to
    /// [`OwnerPhase::AgentRunning`] while an agent runs.
    pub fn for_run() -> StopToken {
        StopToken::new(OwnerPhase::Advancing)
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, OwnerState> {
        self.owner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a stop has been requested.
    pub fn stop_requested(&self) -> bool {
        self.lock().stop_requested
    }

    /// The current owner phase.
    pub fn phase(&self) -> OwnerPhase {
        self.lock().phase
    }

    /// Move the run to `phase`, e.g. `GateWait` around a gate poll.
    pub fn enter(&self, phase: OwnerPhase) {
        self.lock().phase = phase;
    }

    /// Re-arm a run-long token for the agent a new stage just spawned, which leads its
    /// own process group `pgid` and is unreaped. Under the owner lock: the phase becomes
    /// [`OwnerPhase::AgentRunning`], the previous stage's observed exit and group signal
    /// are cleared, and `pgid` is recorded for [`spawn_signal_dispatcher`]. A stop already
    /// requested is kept, with its source, and is returned: `true` means the caller must
    /// end the new group at once, because the stop arrived while the stage was launched.
    pub fn begin_agent(&self, pgid: u32) -> bool {
        let mut state = self.lock();
        state.phase = OwnerPhase::AgentRunning;
        state.agent_exited = false;
        state.group_signalled = false;
        state.agent_pgid = Some(pgid);
        state.stop_requested
    }

    /// Where the first stop request came from, if one has been taken.
    pub fn stop_source(&self) -> Option<StopSource> {
        self.lock().stop_source
    }

    /// Take a stop request and return what it does. A request in
    /// `AgentRunning`, `Advancing` or `GateWait` sets the stop flag; one in
    /// `HandedOff` or `Finished` changes nothing. This only decides: the
    /// caller acts on the returned [`StopAction`] (the runner's signal
    /// dispatcher ends the agent group for [`StopAction::KillAgentGroup`]).
    pub fn request_stop(&self, source: StopSource) -> StopAction {
        request_stop_locked(&mut self.lock(), source)
    }

    /// An advance safe point: [`SafePoint::StopHere`] when a stop has been
    /// requested, which also records that the run ended as a stop.
    pub fn safe_point(&self) -> SafePoint {
        let mut state = self.lock();
        let decision = at_safe_point(state.stop_requested);
        if decision == SafePoint::StopHere {
            state.ended_stopped = true;
        }
        decision
    }

    /// Whether the run ended as a stop, so a parked shutdown caller may be
    /// told `stopped`.
    pub fn ended_stopped(&self) -> bool {
        self.lock().ended_stopped
    }

    /// Record that this run's stop ended an agent group another process owns. The final
    /// stop report then does not say nothing was signalled.
    pub fn note_group_signalled(&self) {
        self.lock().group_signalled = true;
    }

    /// The main thread's exit decision. Under the owner lock: mark the exit
    /// observed (waking a thread that waits for it), run `reap` with the stop flag,
    /// decide with [`on_agent_exit`], and leave the phase at `Advancing` for
    /// a completion or `Finished` for a stop.
    pub(crate) fn observe_exit(
        &self,
        reap: impl FnOnce(bool) -> io::Result<i32>,
    ) -> io::Result<AgentOutcome> {
        let mut state = self.lock();
        state.agent_exited = true;
        self.owner.exited.notify_all();
        let code = reap(state.stop_requested)?;
        let outcome = on_agent_exit(state.stop_requested, code);
        match outcome {
            AgentOutcome::Completed(_) => state.phase = OwnerPhase::Advancing,
            AgentOutcome::Stopped => {
                state.phase = OwnerPhase::Finished;
                state.ended_stopped = true;
            }
        }
        Ok(outcome)
    }
}

/// [`StopToken::request_stop`] on an already locked owner, so the caller
/// can act on the decision under the same lock.
pub(crate) fn request_stop_locked(state: &mut OwnerState, source: StopSource) -> StopAction {
    let action = on_stop_request(state.phase);
    if matches!(
        action,
        StopAction::KillAgentGroup | StopAction::FlagForSafePoint
    ) {
        state.stop_requested = true;
        state.stop_source.get_or_insert(source);
    }
    action
}

/// Signal the agent's process group. The one place a group signal is sent:
/// it refuses a pgid that does not fit `pid_t`, is `0` or `1`, or is this
/// process's own group, because `kill(0, …)` signals our own group and
/// `kill(-1, …)` signals every process we may signal (T-50-18).
pub(crate) fn signal_group(pgid: u32, signal: libc::c_int) -> bool {
    let Ok(signed) = libc::pid_t::try_from(pgid) else {
        tracing::warn!("agent pgid {pgid} does not fit pid_t; not signalling");
        return false;
    };
    if signed <= 1 {
        tracing::warn!("refusing to signal process group {signed}");
        return false;
    }
    // SAFETY: getpgrp has no preconditions and cannot fail.
    if signed == unsafe { libc::getpgrp() } {
        tracing::warn!("refusing to signal this process's own process group {signed}");
        return false;
    }
    // SAFETY: `signed > 1`, so `-signed < -1`: neither our own group (`0`)
    // nor every process (`-1`).
    unsafe { libc::kill(-signed, signal) == 0 }
}

/// Wait up to [`crate::agent::TERMINATE_VERIFY_WAIT`] for the main thread to observe
/// the agent's exit, then SIGKILL the group if it has not. Checked under the owner lock,
/// so the signal reaches only an unreaped leader's group: see
/// [`end_group_while_running`].
pub(crate) fn kill_group_unless_exit_observed(token: &StopToken, pgid: u32) {
    let state = token.lock();
    let (state, _) = token
        .owner
        .exited
        .wait_timeout_while(state, crate::agent::TERMINATE_VERIFY_WAIT, |state| {
            !state.agent_exited
        })
        .unwrap_or_else(PoisonError::into_inner);
    if !state.agent_exited {
        tracing::warn!("agent group {pgid} outlived SIGTERM; sending SIGKILL");
        signal_group(pgid, libc::SIGKILL);
    }
}

/// End the agent's process group while the agent still runs: for a stop that arrived
/// while the stage was launched, or for an owner that found it does not own the run
/// (D-03, D-15). The sequence is a signal stop's:
/// SIGTERM, then up to [`crate::agent::TERMINATE_VERIFY_WAIT`] for the exit, then
/// SIGKILL.
///
/// The rule (R-13): a group is signalled only under the owner lock, only while the
/// phase is `AgentRunning` and the main thread has not observed the exit. Until then the
/// leader is unreaped, so the kernel cannot have given its pid to another process, and
/// the number still names only the agent's own group. Once [`wait_owned_agent`]
/// has reaped the leader, the number may name anyone's group, so this signals nothing
/// and returns [`AgentGroupEnd::AlreadyExited`]. A process the agent left in its group is
/// then not signalled, as for every completed agent (backstop rule 7 reports a leaderless
/// group and never signals it).
pub(crate) fn end_group_while_running(token: &StopToken, pgid: u32) -> AgentGroupEnd {
    let mut state = token.lock();
    if state.phase != OwnerPhase::AgentRunning || state.agent_exited {
        return AgentGroupEnd::AlreadyExited;
    }
    request_stop_locked(&mut state, StopSource::Ownership);
    tracing::info!("ending agent group {pgid}: this process does not own the run");
    state.group_signalled |= signal_group(pgid, libc::SIGTERM);
    drop(state);
    kill_group_unless_exit_observed(token, pgid);
    AgentGroupEnd::Ended
}

/// Block until `pid` (a child of this process) has exited, without reaping
/// it: `waitid(P_PID, pid, WEXITED | WNOWAIT)`. The unreaped zombie keeps its
/// pid and process group, so a group signal sent afterwards still reaches
/// only that group. Its second caller is `monitor.rs`'s stream exit watch
/// (50-26, WR-03), which ends the stream at the agent's exit without
/// reaping it.
pub(crate) fn wait_for_exit_without_reaping(pid: u32) -> io::Result<()> {
    wait_without_reaping(pid).map(|_| ())
}

/// [`wait_for_exit_without_reaping`], returning how `pid` ended, read from the
/// unreaped child's `siginfo_t`.
fn wait_without_reaping(pid: u32) -> io::Result<AgentEnd> {
    let id = libc::id_t::from(pid);
    loop {
        // SAFETY: an all-zero siginfo_t is a valid out-parameter for waitid.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a live, correctly sized out-parameter.
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                id,
                &raw mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if rc == 0 {
            return Ok(agent_end(&info));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// How an agent ended, as `waitid` reported it before the reap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentEnd {
    /// A normal exit with this code.
    Exited(i32),
    /// A death by this signal.
    Signalled(i32),
    /// Not read on this target.
    Unknown,
}

#[cfg(target_os = "linux")]
fn agent_end(info: &libc::siginfo_t) -> AgentEnd {
    // SAFETY: `waitid` returned 0 for a WEXITED wait, so `info` describes a child's
    // exit and its `si_status` is set.
    let status = unsafe { info.si_status() };
    match info.si_code {
        libc::CLD_EXITED => AgentEnd::Exited(status),
        libc::CLD_KILLED | libc::CLD_DUMPED => AgentEnd::Signalled(status),
        _ => AgentEnd::Unknown,
    }
}

#[cfg(not(target_os = "linux"))]
fn agent_end(_info: &libc::siginfo_t) -> AgentEnd {
    AgentEnd::Unknown
}

/// How long the runner waits, after its agent ended the way a stop ends it, for its own
/// stop signal to arrive before it classifies the stage (D-04b). Under
/// `KillMode=control-group` systemd SIGTERMs the agent and the runner together, so the
/// agent may end first.
const STOP_SIGNAL_GRACE: Duration = Duration::from_millis(500);

/// The exit code of an agent that traps SIGTERM and exits as the shell convention says.
const SIGTERM_EXIT_CODE: i32 = 128 + libc::SIGTERM;
/// The exit code of an agent that traps SIGINT and exits as the shell convention says.
const SIGINT_EXIT_CODE: i32 = 128 + libc::SIGINT;

/// Whether `end` is how a stop's SIGTERM or SIGINT ends an agent: a death by either, or a
/// normal exit with `128 +` either (review r2 codex-4: an agent that traps the signal).
fn looks_like_a_stop(end: AgentEnd) -> bool {
    match end {
        AgentEnd::Signalled(signal) => signal == libc::SIGTERM || signal == libc::SIGINT,
        AgentEnd::Exited(code) => code == SIGTERM_EXIT_CODE || code == SIGINT_EXIT_CODE,
        AgentEnd::Unknown => false,
    }
}

/// Under the owner lock: request the stop for a signal the handler caught that the
/// dispatcher has not turned into one yet, and say whether a stop is requested. The
/// handler stores the signal number before it writes the pipe byte, so this sees a
/// signal the dispatcher has not read.
fn adopt_caught_stop(token: &StopToken) -> bool {
    let mut state = token.lock();
    if !state.stop_requested
        && let Some(signal) = caught_stop_signal()
    {
        request_stop_locked(&mut state, StopSource::Signal(signal));
    }
    state.stop_requested
}

/// Drain-then-classify (D-04b): before the exit decision, take this process's own
/// pending stop signal, and when the agent ended the way a stop ends it, wait up to
/// [`STOP_SIGNAL_GRACE`] for that signal. An agent that ends this way with no stop
/// arriving in the grace is classified as it ended.
fn take_a_pending_stop(token: &StopToken, end: AgentEnd) {
    if adopt_caught_stop(token) || !looks_like_a_stop(end) {
        return;
    }
    let deadline = std::time::Instant::now() + STOP_SIGNAL_GRACE;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(crate::agent::TERMINATE_VERIFY_POLL);
        if adopt_caught_stop(token) {
            return;
        }
    }
}

/// An exit status as a code: the exit code, or `128 + n` for a death by
/// signal `n` (the `monitor` convention: SIGTERM is 143, SIGKILL 137).
pub(crate) fn exit_code(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map_or(-1, |signal| 128 + signal)
    })
}

/// The signal pipe's read end, or `-1` before [`install_signal_pipe`].
pub(crate) static SIGNAL_READ_FD: AtomicI32 = AtomicI32::new(-1);
/// The signal pipe's write end, the handler's only output.
static SIGNAL_WRITE_FD: AtomicI32 = AtomicI32::new(-1);
/// The pid that installed the handler. A forked child that has not exec'd
/// yet runs the inherited handler under another pid and writes nothing.
static HANDLER_PID: AtomicI32 = AtomicI32::new(0);
/// The last SIGTERM/SIGINT the handler caught, `0` for none. Stored before the
/// pipe byte is written, so a reader that finds no byte yet still sees it.
static CAUGHT_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// The SIGTERM/SIGINT handler: the pid check, an atomic store and a one-byte
/// `write`, all async-signal-safe, and nothing else. The runner's signal
/// dispatcher ([`spawn_signal_dispatcher`]) turns the byte into a stop.
extern "C" fn on_stop_signal(signo: libc::c_int) {
    // SAFETY: getpid is async-signal-safe and has no preconditions.
    if unsafe { libc::getpid() } == HANDLER_PID.load(Ordering::Relaxed) {
        CAUGHT_SIGNAL.store(signo, Ordering::SeqCst);
        let byte = signo as u8;
        // SAFETY: write is async-signal-safe; `byte` is a live one-byte
        // buffer. A full pipe drops the byte, which is harmless: the first
        // byte already requested the stop.
        unsafe {
            libc::write(
                SIGNAL_WRITE_FD.load(Ordering::Relaxed),
                (&raw const byte).cast(),
                1,
            );
        }
    }
}

/// Turn SIGTERM and SIGINT into stop events for the runner: once per process,
/// create a close-on-exec, non-blocking pipe and install [`on_stop_signal`] with
/// `SA_RESTART` for both signals. [`spawn_signal_dispatcher`]'s thread reads the pipe
/// and requests the stop. The caught signal's number is also kept for
/// [`caught_stop_signal`].
///
/// This is the process's one SIGTERM/SIGINT handler: a foreground holder's
/// `liveness::install_holder_stop_handler` installs it too and reads the number.
///
/// Call it at runner entry, before the agent is spawned. It installs
/// process-global signal state, so only a process that owns a run calls it, and it
/// never blocks a signal: a blocked mask would be inherited by every agent
/// (RESEARCH Finding 3).
pub fn install_signal_pipe() -> io::Result<()> {
    static INSTALLED: OnceLock<Result<(), (io::ErrorKind, String)>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| install_once().map_err(|error| (error.kind(), error.to_string())))
        .clone()
        .map_err(|(kind, message)| io::Error::new(kind, message))
}

/// The last SIGTERM or SIGINT [`install_signal_pipe`]'s handler caught in this
/// process, if one has arrived. A later signal replaces an earlier one.
pub fn caught_stop_signal() -> Option<i32> {
    match CAUGHT_SIGNAL.load(Ordering::SeqCst) {
        0 => None,
        signal => Some(signal),
    }
}

fn install_once() -> io::Result<()> {
    let [read, write] = cloexec_nonblocking_pipe()?;
    SIGNAL_WRITE_FD.store(write, Ordering::SeqCst);
    // SAFETY: getpid has no preconditions.
    HANDLER_PID.store(unsafe { libc::getpid() }, Ordering::SeqCst);
    SIGNAL_READ_FD.store(read, Ordering::SeqCst);
    for signal in [libc::SIGTERM, libc::SIGINT] {
        // SAFETY: an all-zero sigaction is valid; the fields that matter
        // are set below.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = on_stop_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        // SAFETY: `action.sa_mask` is a live sigset_t.
        unsafe { libc::sigemptyset(&raw mut action.sa_mask) };
        // SAFETY: `action` is a fully initialized sigaction; the old action
        // is not wanted.
        if unsafe { libc::sigaction(signal, &raw const action, std::ptr::null_mut()) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// A pipe whose two ends are close-on-exec and non-blocking, so no agent
/// inherits it and a full pipe never blocks the handler.
#[cfg(target_os = "linux")]
fn cloexec_nonblocking_pipe() -> io::Result<[libc::c_int; 2]> {
    let mut fds = [-1; 2];
    // SAFETY: `fds` is a live two-element out-parameter.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fds)
}

/// [`cloexec_nonblocking_pipe`] where libc has no `pipe2` (Apple targets):
/// `pipe`, then `FD_CLOEXEC` and `O_NONBLOCK` on each end.
#[cfg(not(target_os = "linux"))]
fn cloexec_nonblocking_pipe() -> io::Result<[libc::c_int; 2]> {
    let mut fds = [-1; 2];
    // SAFETY: `fds` is a live two-element out-parameter.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    for fd in fds {
        // SAFETY: `fd` is an open descriptor this call created.
        let configured = unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) == 0 && {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                flags >= 0 && libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == 0
            }
        };
        if !configured {
            let error = io::Error::last_os_error();
            for fd in fds {
                // SAFETY: both descriptors were created above and are not
                // shared yet.
                unsafe { libc::close(fd) };
            }
            return Err(error);
        }
    }
    Ok(fds)
}

/// Wait for a stage's agent under the run's token: block until `child`,
/// which leads its own group `pgid`, exits, take a pending stop of this process's own
/// ([`take_a_pending_stop`]), then decide and reap it under `token`. The wait runs on the
/// caller's thread.
pub(crate) fn wait_owned_agent(
    token: &StopToken,
    pgid: u32,
    child: &mut std::process::Child,
) -> io::Result<AgentOutcome> {
    let end = wait_without_reaping(child.id())?;
    take_a_pending_stop(token, end);
    observe_agent_exit(token, pgid, child)
}

/// The exit decision for an agent already seen to exit, unreaped: under the owner
/// lock, SIGKILL the group's leftover members on a stop, while the unreaped leader
/// still holds the pgid (RESEARCH Pitfall 9), then reap it.
pub(crate) fn observe_agent_exit(
    token: &StopToken,
    pgid: u32,
    child: &mut std::process::Child,
) -> io::Result<AgentOutcome> {
    token.observe_exit(|stop_requested| {
        if stop_requested {
            // The zombie leader still holds the pgid, so this reaches
            // only the agent's own leftover members.
            signal_group(pgid, libc::SIGKILL);
        }
        child.wait().map(exit_code)
    })
}

/// Read every pending byte from the non-blocking signal pipe; each is one
/// delivered signal's number.
pub(crate) fn drain_signal_pipe(fd: libc::c_int) -> Vec<i32> {
    let mut signals = Vec::new();
    let mut buf = [0_u8; 64];
    loop {
        // SAFETY: `buf` is a live buffer of the length passed.
        let read = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        let Ok(read) = usize::try_from(read) else {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            // WouldBlock: the pipe is drained.
            return signals;
        };
        if read == 0 {
            return signals;
        }
        signals.extend(buf[..read].iter().map(|&byte| i32::from(byte)));
    }
}

/// The foreground runner's signal consumer for its whole run (D-05): a thread that
/// turns each caught SIGTERM/SIGINT into a stop on the run's token. Dropping it tells
/// the thread to quit and joins it.
#[derive(Debug)]
pub struct SignalDispatcher {
    quit: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for SignalDispatcher {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Start the runner's one reader of the signal pipe ([`install_signal_pipe`] must have
/// run). The thread, `runner-signals`, polls the pipe every
/// [`crate::agent::TERMINATE_VERIFY_POLL`] and requests a stop on `token` for each
/// signal. While an agent runs ([`StopAction::KillAgentGroup`]) it SIGTERMs the group
/// [`StopToken::begin_agent`] recorded, under the owner lock and only while the agent's
/// exit is unobserved, then SIGKILLs it if the exit is not observed within
/// [`crate::agent::TERMINATE_VERIFY_WAIT`]. During an advance or a gate wait the stop
/// flag is the whole effect: the gate poll and the next safe point read it.
///
/// # Errors
/// The thread could not be spawned.
pub fn spawn_signal_dispatcher(token: StopToken) -> io::Result<SignalDispatcher> {
    let quit = Arc::new(AtomicBool::new(false));
    let thread_quit = Arc::clone(&quit);
    let thread = std::thread::Builder::new()
        .name("runner-signals".to_string())
        .spawn(move || dispatch_signals(&token, &thread_quit))?;
    Ok(SignalDispatcher {
        quit,
        thread: Some(thread),
    })
}

/// [`spawn_signal_dispatcher`]'s loop: poll the signal pipe until told to quit.
fn dispatch_signals(token: &StopToken, quit: &AtomicBool) {
    let tick = libc::c_int::try_from(crate::agent::TERMINATE_VERIFY_POLL.as_millis())
        .unwrap_or(libc::c_int::MAX);
    while !quit.load(Ordering::Acquire) {
        // A negative fd (no pipe installed) is ignored by poll.
        let mut fd = libc::pollfd {
            fd: SIGNAL_READ_FD.load(Ordering::SeqCst),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `fd` is one live, initialized pollfd.
        let ready = unsafe { libc::poll(&raw mut fd, 1, tick) };
        if ready <= 0 || fd.revents & libc::POLLIN == 0 {
            continue;
        }
        for signal in drain_signal_pipe(fd.fd) {
            stop_on_signal(token, signal);
        }
    }
}

/// One caught signal's stop: request it, and while an agent runs end its group.
fn stop_on_signal(token: &StopToken, signal: i32) {
    let mut state = token.lock();
    let action = request_stop_locked(&mut state, StopSource::Signal(signal));
    let pgid = match (action, state.agent_exited, state.agent_pgid) {
        (StopAction::KillAgentGroup, false, Some(pgid)) => pgid,
        _ => return,
    };
    tracing::info!("stop requested by signal {signal}: terminating agent group {pgid}");
    state.group_signalled |= signal_group(pgid, libc::SIGTERM);
    drop(state);
    kill_group_unless_exit_observed(token, pgid);
}

#[cfg(test)]
pub(crate) mod tests {
    /// One `line N: <needle>` entry per needle on each non-comment line of
    /// `source` that holds one: a process exit or a signal-mask or
    /// signal-wait call.
    pub(crate) fn forbidden_calls(source: &str) -> Vec<String> {
        let needles = needles();
        let mut found = Vec::new();
        for (index, line) in source.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for needle in &needles {
                if line.contains(needle.as_str()) {
                    found.push(format!("line {}: {needle}", index + 1));
                }
            }
        }
        found
    }

    /// The needles, each joined from two fragments at run time so this test
    /// module never holds one verbatim.
    pub(crate) fn needles() -> [String; 4] {
        [
            ["process", "::exit"].concat(),
            ["sigproc", "mask"].concat(),
            ["pthread_sig", "mask"].concat(),
            ["sig", "wait"].concat(),
        ]
    }

    #[test]
    fn stop_source_has_none_of_the_forbidden_signal_or_exit_calls() {
        assert_eq!(
            forbidden_calls(include_str!("stop.rs")),
            Vec::<String>::new(),
            "stop.rs must not exit the process, block a signal or wait for one"
        );
        // Planted-positive control: each needle on a code line is flagged,
        // and the same needle after `//` is not.
        let planted: String = needles()
            .iter()
            .map(|needle| format!("    let _ = {needle}(0);\n    // {needle}\n"))
            .collect();
        let found = forbidden_calls(&planted);
        assert_eq!(found.len(), 4, "every planted call is flagged: {found:?}");
        for (index, needle) in needles().iter().enumerate() {
            let line = index * 2 + 1;
            assert_eq!(found[index], format!("line {line}: {needle}"));
        }
    }
}

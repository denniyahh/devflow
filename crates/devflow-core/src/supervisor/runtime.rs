//! The supervisor's process body: one terminal-state owner, one control
//! thread, and the decision that a stop is never a completion (R-2).
//!
//! **Threads.** Two participants, plus a signal handler that does no work:
//! - the **main thread** spawns the agent, calls [`Supervisor::serve`], then
//!   blocks in [`Supervisor::wait_agent`]. It observes the agent's exit with
//!   `waitid(WNOWAIT)`, which does not reap, so the zombie leader still holds
//!   the agent's process group while leftover members are killed. Only then
//!   does it reap.
//! - one **control thread**, started by [`Supervisor::serve`], `poll`s the
//!   listening socket and, when [`install_signal_pipe`] has run, the signal
//!   pipe's read end. It answers `ping`, turns a `shutdown` request or a
//!   signal byte into [`StopToken::request_stop`], and acts on the returned
//!   [`StopAction`].
//! - the SIGTERM/SIGINT **handler** only compares `getpid()` with the
//!   supervisor's pid and `write`s one byte to the pipe; both calls are
//!   async-signal-safe. The supervisor never blocks a signal: a blocked mask
//!   is inherited by every agent it spawns (RESEARCH Finding 3, probe A).
//!
//! **The one invariant.** Every stop and exit decision is taken under the
//! [`StopToken`]'s owner mutex: a stop request, the main thread's "the agent
//! exited, do we advance?" decision, and every advance safe point. A stop and
//! the agent's natural exit are therefore totally ordered, and a stop that
//! wins the race always ends the run as stopped. The group signals the
//! control thread sends are sent under the same mutex, and only while the
//! exit has not been observed, so no group signal is sent after the leader
//! is reaped.

use super::protocol::{self, PingReply, Reply, Request, ShutdownAccepted, ShutdownStopped};
use super::{PublishError, Sidecar, SocketPath};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::JoinHandle;

/// The exit code a stopped run reports: the shell convention for a death by
/// SIGTERM, `128 + 15`. The monitor writes it to the exit file on
/// [`AgentOutcome::Stopped`].
pub(crate) const STOPPED_EXIT_CODE: i32 = 128 + libc::SIGTERM;

/// Who owns the run right now. Its [`as_str`](Self::as_str) label is the
/// `owner` field of a ping reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OwnerPhase {
    /// The agent is running under this supervisor.
    AgentRunning,
    /// The agent completed and the CLI is advancing the pipeline.
    Advancing,
    /// The CLI is waiting on a gate answer.
    GateWait,
    /// The run was handed to the next stage's supervisor.
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
    /// A signal delivered to the supervisor, by number.
    Signal(i32),
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
struct OwnerState {
    phase: OwnerPhase,
    stop_requested: bool,
    /// The first stop's source, for the log.
    stop_source: Option<StopSource>,
    /// The main thread has observed the agent's exit. Set under the lock
    /// before the leader is reaped, so a group signal sent under the lock
    /// while this is false always reaches an unreaped group.
    agent_exited: bool,
    /// The run ended as a stop: [`AgentOutcome::Stopped`] or a safe point
    /// that returned [`SafePoint::StopHere`].
    ended_stopped: bool,
}

/// The owner mutex and the condition variable the control thread waits on
/// for the agent's exit.
#[derive(Debug)]
struct Owner {
    state: Mutex<OwnerState>,
    exited: Condvar,
}

/// A handle to the run's single terminal-state owner. Cloning shares the
/// owner; every method takes the owner mutex, so their decisions are totally
/// ordered with each other and with the supervisor's.
#[derive(Debug, Clone)]
pub struct StopToken {
    owner: Arc<Owner>,
}

impl StopToken {
    fn new(phase: OwnerPhase) -> Self {
        Self {
            owner: Arc::new(Owner {
                state: Mutex::new(OwnerState {
                    phase,
                    stop_requested: false,
                    stop_source: None,
                    agent_exited: false,
                    ended_stopped: false,
                }),
                exited: Condvar::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, OwnerState> {
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

    /// Take a stop request and return what it does. A request in
    /// `AgentRunning`, `Advancing` or `GateWait` sets the stop flag; one in
    /// `HandedOff` or `Finished` changes nothing. This only decides: the
    /// caller acts on the returned [`StopAction`] (the supervisor's control
    /// thread ends the agent group for [`StopAction::KillAgentGroup`]).
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
    fn ended_stopped(&self) -> bool {
        self.lock().ended_stopped
    }

    /// The main thread's exit decision. Under the owner lock: mark the exit
    /// observed (waking the control thread), run `reap` with the stop flag,
    /// decide with [`on_agent_exit`], and leave the phase at `Advancing` for
    /// a completion or `Finished` for a stop.
    fn observe_exit(&self, reap: impl FnOnce(bool) -> io::Result<i32>) -> io::Result<AgentOutcome> {
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

/// [`StopToken::request_stop`] on an already locked owner, so the control
/// thread can act on the decision under the same lock.
fn request_stop_locked(state: &mut OwnerState, source: StopSource) -> StopAction {
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
fn signal_group(pgid: u32, signal: libc::c_int) -> bool {
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
        tracing::warn!("refusing to signal the supervisor's own process group {signed}");
        return false;
    }
    // SAFETY: `signed > 1`, so `-signed < -1`: neither our own group (`0`)
    // nor every process (`-1`).
    unsafe { libc::kill(-signed, signal) == 0 }
}

/// Block until `pid` (a child of this process) has exited, without reaping
/// it: `waitid(P_PID, pid, WEXITED | WNOWAIT)`. The unreaped zombie keeps its
/// pid and process group, so a group signal sent afterwards still reaches
/// only that group. Its second caller is `monitor.rs`'s stream exit watch
/// (50-26, WR-03), which ends the stream at the agent's exit without
/// reaping it.
pub(crate) fn wait_for_exit_without_reaping(pid: u32) -> io::Result<()> {
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
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// How long [`stopped_outcome_test_hold`] waits for its release file: above
/// the bounded waits of the test that uses it, combined.
#[cfg(any(test, feature = "test-support"))]
const STOPPED_OUTCOME_HOLD_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Test-only hold (50-23, R6-E). When `stop_requested` and
/// `DEVFLOW_TEST_STOPPED_OUTCOME_HOLD` names a path: create `<path>.held`,
/// then wait, polling every 20 ms and bounded by
/// [`STOPPED_OUTCOME_HOLD_WAIT`], until `<path>` exists. [`Supervisor::wait_agent`]
/// calls it after the agent's exit is observed and before the owner lock is
/// taken for the exit decision, so a parked supervisor still answers pings
/// with owner `agent_running` and its outcome is undecided: a CLI test can
/// order that decision against the next stage's publish. Returns at once
/// for a supervisor whose stop was not requested. Compiled only under
/// `cfg(test)` or the `test-support` feature, together with its one call; a
/// release build has neither.
#[cfg(any(test, feature = "test-support"))]
fn stopped_outcome_test_hold(stop_requested: bool) {
    if !stop_requested {
        return;
    }
    let Some(release) = std::env::var_os("DEVFLOW_TEST_STOPPED_OUTCOME_HOLD") else {
        return;
    };
    let release = std::path::PathBuf::from(release);
    let mut held = release.clone().into_os_string();
    held.push(".held");
    let _ = std::fs::write(&held, "");
    let deadline = std::time::Instant::now() + STOPPED_OUTCOME_HOLD_WAIT;
    while !release.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
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
static SIGNAL_READ_FD: AtomicI32 = AtomicI32::new(-1);
/// The signal pipe's write end, the handler's only output.
static SIGNAL_WRITE_FD: AtomicI32 = AtomicI32::new(-1);
/// The pid that installed the handler. A forked child that has not exec'd
/// yet runs the inherited handler under another pid and writes nothing.
static SUPERVISOR_PID: AtomicI32 = AtomicI32::new(0);

/// The SIGTERM/SIGINT handler: the pid check and a one-byte `write`, both
/// async-signal-safe, and nothing else. The control thread turns the byte
/// into a stop.
extern "C" fn on_stop_signal(signo: libc::c_int) {
    // SAFETY: getpid is async-signal-safe and has no preconditions.
    if unsafe { libc::getpid() } == SUPERVISOR_PID.load(Ordering::Relaxed) {
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

/// Turn SIGTERM and SIGINT into stop events for this process's supervisor:
/// once per process, create a close-on-exec, non-blocking pipe and install
/// [`on_stop_signal`] with `SA_RESTART` for both signals. The control thread
/// [`Supervisor::serve`] starts reads the pipe and requests the stop.
///
/// Call it at supervisor-process entry, before the agent is spawned. It
/// installs process-global signal state, so only a supervisor process calls
/// it, and it never blocks a signal: a blocked mask would be inherited by
/// every agent (RESEARCH Finding 3).
pub fn install_signal_pipe() -> io::Result<()> {
    static INSTALLED: OnceLock<Result<(), (io::ErrorKind, String)>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| install_once().map_err(|error| (error.kind(), error.to_string())))
        .clone()
        .map_err(|(kind, message)| io::Error::new(kind, message))
}

fn install_once() -> io::Result<()> {
    let [read, write] = cloexec_nonblocking_pipe()?;
    SIGNAL_WRITE_FD.store(write, Ordering::SeqCst);
    // SAFETY: getpid has no preconditions.
    SUPERVISOR_PID.store(unsafe { libc::getpid() }, Ordering::SeqCst);
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

/// A running supervisor: its published socket, its sidecar and its control
/// thread. Dropping it without [`finish`](Self::finish) still stops the
/// thread and removes the launch files.
#[derive(Debug)]
pub struct Supervisor {
    socket: SocketPath,
    agent_pgid: u32,
    token: StopToken,
    quit: Arc<AtomicBool>,
    control: Option<JoinHandle<Vec<UnixStream>>>,
    closed: bool,
}

impl Supervisor {
    /// Write `sidecar`, publish `socket` (so the sidecar exists before the
    /// socket name appears, R-7), and start the control thread. The run
    /// starts in [`OwnerPhase::AgentRunning`]: call this after the agent is
    /// spawned, with the agent's identity in `sidecar`.
    ///
    /// An existing sidecar at the name is never replaced: the write fails
    /// with [`io::ErrorKind::AlreadyExists`] and nothing is published.
    pub fn serve(socket: SocketPath, sidecar: Sidecar) -> Result<Self, PublishError> {
        super::write_sidecar(&socket, &sidecar)?;
        Self::serve_written(socket, sidecar)
    }

    /// [`serve`](Self::serve) for a caller that has already written
    /// `sidecar` beside `socket` itself with `write_sidecar`, which never
    /// replaces one, so the sidecar there is this launch's. A publish that
    /// fails removes it, the name being taken included: left beside another
    /// socket, it would name this launch's agent as that socket's.
    pub(crate) fn serve_written(
        socket: SocketPath,
        sidecar: Sidecar,
    ) -> Result<Self, PublishError> {
        let listener = match super::publish(&socket) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = std::fs::remove_file(socket.sidecar_path());
                return Err(error);
            }
        };
        let started = listener.set_nonblocking(true).and_then(|()| {
            let token = StopToken::new(OwnerPhase::AgentRunning);
            let quit = Arc::new(AtomicBool::new(false));
            let control = Control {
                listener,
                sidecar: sidecar.clone(),
                token: token.clone(),
                quit: Arc::clone(&quit),
            };
            let handle = std::thread::Builder::new()
                .name("supervisor-control".to_string())
                .spawn(move || control.run())?;
            Ok(Self {
                socket: socket.clone(),
                agent_pgid: sidecar.agent_pgid,
                token,
                quit,
                control: Some(handle),
                closed: false,
            })
        });
        started.map_err(|error| {
            let _ = super::remove_launch_files(&socket);
            PublishError::Io(error)
        })
    }

    /// Block until `child` (the agent this supervisor serves) exits, decide
    /// under the owner lock whether the run completed or was stopped, and
    /// reap it. On [`AgentOutcome::Stopped`] the group's leftover members
    /// are SIGKILLed first, while the unreaped leader still holds the pgid
    /// (RESEARCH Pitfall 9).
    pub fn wait_agent(&self, child: &mut std::process::Child) -> io::Result<AgentOutcome> {
        wait_for_exit_without_reaping(child.id())?;
        // Test-only (50-23, R6-E): park a stopped supervisor between the
        // observed exit and its outcome decision. Compiled out of release
        // builds.
        #[cfg(any(test, feature = "test-support"))]
        stopped_outcome_test_hold(self.token.stop_requested());
        let pgid = self.agent_pgid;
        self.token.observe_exit(|stop_requested| {
            if stop_requested {
                // The zombie leader still holds the pgid, so this reaches
                // only the agent's own leftover members.
                signal_group(pgid, libc::SIGKILL);
            }
            child.wait().map(exit_code)
        })
    }

    /// A handle to this run's owner, for the CLI's safe points.
    pub fn token(&self) -> StopToken {
        self.token.clone()
    }

    /// Mark the run handed off to the next stage's supervisor: a later stop
    /// request is answered `handed_off` and acts on nothing.
    pub fn hand_off(&self) {
        self.token.enter(OwnerPhase::HandedOff);
    }

    /// End the supervisor: stop the control thread, remove the socket and
    /// sidecar, then send each parked shutdown caller its final line. That
    /// line is `stopped` only when the run ended as a stop
    /// ([`AgentOutcome::Stopped`] or a safe point that stopped), otherwise
    /// `{"ok":false,"error":"handed_off"}`, so no caller is told a stop
    /// happened that did not. The files go before the line, so a caller that
    /// reads `stopped` finds the socket already gone.
    pub fn finish(mut self) {
        let parked = self.close();
        let reply = if self.token.ended_stopped() {
            Reply::Stopped(ShutdownStopped {
                exit_code: STOPPED_EXIT_CODE,
                advance_suppressed: true,
            })
        } else {
            Reply::Error("handed_off".to_string())
        };
        self.token.enter(OwnerPhase::Finished);
        for stream in parked {
            if let Err(error) = protocol::write_reply(&stream, &reply) {
                tracing::warn!("could not send the final shutdown line: {error}");
            }
        }
    }

    /// Stop and join the control thread and remove the launch files, once.
    /// Returns the parked shutdown connections.
    fn close(&mut self) -> Vec<UnixStream> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        self.quit.store(true, Ordering::Release);
        let parked = self
            .control
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if let Err(error) = super::remove_launch_files(&self.socket) {
            tracing::warn!(
                "could not remove the launch files for {}: {error}",
                self.socket.as_path().display()
            );
        }
        parked
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // A parked caller is dropped without a final line: it reads a closed
        // connection, which is an error, never a stop.
        drop(self.close());
    }
}

/// Read every pending byte from the non-blocking signal pipe; each is one
/// delivered signal's number.
fn drain_signal_pipe(fd: libc::c_int) -> Vec<i32> {
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

/// The control thread's state.
struct Control {
    listener: UnixListener,
    sidecar: Sidecar,
    token: StopToken,
    quit: Arc<AtomicBool>,
}

impl Control {
    /// Poll the listener until told to quit; return the parked shutdown
    /// connections.
    fn run(self) -> Vec<UnixStream> {
        let mut parked = Vec::new();
        let tick = libc::c_int::try_from(crate::agent::TERMINATE_VERIFY_POLL.as_millis())
            .unwrap_or(libc::c_int::MAX);
        while !self.quit.load(Ordering::Acquire) {
            // A negative fd (no pipe installed) is ignored by poll.
            let mut fds = [
                self.listener.as_raw_fd(),
                SIGNAL_READ_FD.load(Ordering::SeqCst),
            ]
            .map(|fd| libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
            // SAFETY: `fds` is a live array of initialized pollfd entries and
            // its length is passed alongside it.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, tick) };
            if ready <= 0 {
                continue;
            }
            if fds[1].revents & libc::POLLIN != 0 {
                for signal in drain_signal_pipe(fds[1].fd) {
                    self.stop(StopSource::Signal(signal), None, &mut parked);
                }
            }
            if fds[0].revents & libc::POLLIN != 0 {
                self.accept_all(&mut parked);
            }
        }
        parked
    }

    /// Serve every pending connection.
    fn accept_all(&self, parked: &mut Vec<UnixStream>) {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => self.serve_connection(stream, parked),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    tracing::warn!("supervisor accept failed: {error}");
                    return;
                }
            }
        }
    }

    /// Answer one connection's single request.
    fn serve_connection(&self, stream: UnixStream, parked: &mut Vec<UnixStream>) {
        // An accepted socket may inherit the listener's non-blocking flag.
        if stream.set_nonblocking(false).is_err() || !protocol::peer_is_same_user(&stream) {
            return;
        }
        match protocol::read_request(&stream) {
            Ok(Request::Ping) => {
                let reply = Reply::Ping(self.ping_reply());
                if let Err(error) = protocol::write_reply(&stream, &reply) {
                    tracing::warn!("could not answer a ping: {error}");
                }
            }
            Ok(Request::Shutdown) => self.stop(StopSource::Socket, Some(stream), parked),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                let _ = protocol::write_reply(&stream, &Reply::Error("unknown_op".to_string()));
            }
            Err(_) => {}
        }
    }

    /// Take a stop request (with the requesting connection, for a socket
    /// `shutdown`) and act on [`StopToken::request_stop`]'s decision.
    ///
    /// - `KillAgentGroup`: under the owner lock, and only while the exit has
    ///   not been observed, SIGTERM the agent's group. Reply `accepted` and
    ///   park the connection for [`Supervisor::finish`]'s final line. Then
    ///   wait up to [`crate::agent::TERMINATE_VERIFY_WAIT`] for the main
    ///   thread to observe the exit, and SIGKILL the group if it has not.
    /// - `FlagForSafePoint`: reply `accepted` and park; the CLI's next safe
    ///   point stops the run.
    /// - `ReplyHandedOff`: reply `handed_off`.
    /// - `AlreadyFinished`: report the stop if the run ended as one,
    ///   otherwise reply `already_finished`.
    fn stop(&self, source: StopSource, stream: Option<UnixStream>, parked: &mut Vec<UnixStream>) {
        let pgid = self.sidecar.agent_pgid;
        let mut state = self.token.lock();
        let action = request_stop_locked(&mut state, source);
        let owner = state.phase.as_str().to_string();
        let ended_stopped = state.ended_stopped;
        if action == StopAction::KillAgentGroup && !state.agent_exited {
            tracing::info!("stop requested by {source:?}: terminating agent group {pgid}");
            signal_group(pgid, libc::SIGTERM);
        }
        drop(state);

        let accepted = Reply::Accepted(ShutdownAccepted { owner });
        let replies: &[Reply] = match action {
            StopAction::KillAgentGroup | StopAction::FlagForSafePoint => {
                std::slice::from_ref(&accepted)
            }
            StopAction::ReplyHandedOff => &[Reply::Error("handed_off".to_string())],
            StopAction::AlreadyFinished if ended_stopped => &[
                accepted.clone(),
                Reply::Stopped(ShutdownStopped {
                    exit_code: STOPPED_EXIT_CODE,
                    advance_suppressed: true,
                }),
            ],
            StopAction::AlreadyFinished => &[Reply::Error("already_finished".to_string())],
        };
        if let Some(stream) = stream {
            let sent = replies
                .iter()
                .try_for_each(|reply| protocol::write_reply(&stream, reply));
            match sent {
                Ok(())
                    if matches!(
                        action,
                        StopAction::KillAgentGroup | StopAction::FlagForSafePoint
                    ) =>
                {
                    parked.push(stream);
                }
                Ok(()) => {}
                Err(error) => tracing::warn!("could not answer a shutdown request: {error}"),
            }
        }

        if action == StopAction::KillAgentGroup {
            let state = self.token.lock();
            let (state, _) = self
                .token
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
    }

    /// The ping reply: the sidecar's identity plus the current owner phase.
    fn ping_reply(&self) -> PingReply {
        let sidecar = &self.sidecar;
        PingReply {
            launch_id: sidecar.launch_id.clone(),
            phase: sidecar.phase.to_string(),
            stage: sidecar.stage.clone().unwrap_or_default(),
            launch: sidecar.launch.clone(),
            owner: self.token.phase().as_str().to_string(),
            agent_pid: sidecar.agent_pid,
            agent_pgid: sidecar.agent_pgid,
            agent_start_time: sidecar.agent_start_time,
            boot_id: (!sidecar.boot_id.is_empty()).then(|| sidecar.boot_id.clone()),
            supervisor_pid: sidecar.supervisor_pid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase_id::PhaseId;
    use crate::supervisor::{Liveness, ShutdownOutcome, new_launch_id, probe, request_shutdown};
    use crate::test_support::{assert_child_ran_exactly_one_passing_test, spawn_test_in_child};
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::time::{Duration, Instant};

    /// The socket path the child supervisor serves at.
    const SOCKET_ENV: &str = "DEVFLOW_TEST_RUNTIME_SOCKET";
    /// Where the child supervisor writes its `AgentOutcome`.
    const OUTCOME_ENV: &str = "DEVFLOW_TEST_RUNTIME_OUTCOME";
    /// The `sh -c` script the child supervisor runs as its agent.
    const AGENT_ENV: &str = "DEVFLOW_TEST_RUNTIME_AGENT";
    /// `1` when the child supervisor installs the signal pipe first.
    const PIPE_ENV: &str = "DEVFLOW_TEST_RUNTIME_PIPE";

    /// How long the parent waits for a child supervisor to answer a ping or
    /// to exit.
    const CHILD_WAIT: Duration = Duration::from_secs(30);

    /// A short cache dir under `/tmp`, so socket paths stay far below
    /// `sun_path`.
    fn short_cache() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("dfs")
            .tempdir_in("/tmp")
            .expect("create a short cache dir under /tmp")
    }

    fn wait_until(within: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + within;
        loop {
            if done() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Whether any member of process group `pgid` still exists (zombies
    /// included). `EPERM` means a member exists that we may not signal.
    fn group_exists(pgid: u32) -> bool {
        let signed = libc::pid_t::try_from(pgid).expect("pgid fits pid_t");
        assert!(signed > 1, "refusing to probe process group {signed}");
        // SAFETY: `signed > 1`; signal 0 only checks for existence.
        let rc = unsafe { libc::kill(-signed, 0) };
        rc == 0 || io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    /// Kills and reaps a child process on every exit path.
    struct ChildGuard(Option<Child>);

    impl ChildGuard {
        fn child(&mut self) -> &mut Child {
            self.0.as_mut().expect("the child is still owned")
        }

        /// Wait for the child to exit on its own, then collect its output.
        fn wait_exit(&mut self) -> Output {
            let exited = wait_until(CHILD_WAIT, || {
                self.child().try_wait().expect("poll the child").is_some()
            });
            let child = self.0.take().expect("the child is still owned");
            if !exited {
                let mut child = ChildGuard(Some(child));
                let _ = child.child().kill();
                let output = child.0.take().expect("owned").wait_with_output();
                panic!("the child supervisor did not exit within {CHILD_WAIT:?}: {output:?}");
            }
            child
                .wait_with_output()
                .expect("collect the child's output")
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                // `Child::kill` is SIGKILL to the pid this test spawned and
                // still owns, never a pid read from disk.
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// SIGKILLs whatever is left of a process group on drop.
    struct GroupGuard(u32);

    impl Drop for GroupGuard {
        fn drop(&mut self) {
            if group_exists(self.0) {
                signal_group(self.0, libc::SIGKILL);
            }
        }
    }

    /// A sidecar for an agent this test process spawned.
    fn test_sidecar(socket: &SocketPath, agent_pid: u32) -> Sidecar {
        let stem = socket
            .as_path()
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .expect("socket stem");
        Sidecar {
            launch_id: stem.rsplit('-').next().expect("launch id").to_string(),
            project_root: PathBuf::from("/project/runtime"),
            phase: PhaseId::new(50),
            stage: Some("code".to_string()),
            agent: "test".to_string(),
            launch: "capture".to_string(),
            agent_pid,
            agent_pgid: agent_pid,
            agent_start_time: crate::agent::process_start_time(agent_pid).unwrap_or(0),
            boot_id: crate::supervisor::boot_id().unwrap_or_default(),
            supervisor_pid: std::process::id(),
            supervisor_start_time: crate::agent::process_start_time(std::process::id())
                .unwrap_or(0),
            created_at: crate::supervisor::rfc3339_now(),
        }
    }

    /// The child half shared by every runtime process test: optionally
    /// install the signal pipe, spawn the agent script in its own process
    /// group, serve, wait, finish, and record the outcome.
    fn supervise_in_child() {
        let socket = std::env::var_os(SOCKET_ENV).expect("the parent names the socket");
        let socket = SocketPath::new(PathBuf::from(socket)).expect("socket path fits");
        let outcome_path = std::env::var_os(OUTCOME_ENV).expect("the parent names the outcome");
        let script = std::env::var_os(AGENT_ENV).expect("the parent names the agent script");
        if std::env::var_os(PIPE_ENV).is_some_and(|value| value == "1") {
            install_signal_pipe().expect("install the signal pipe");
        }
        let mut agent = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("spawn the agent");
        // Covers a panic before the supervisor has decided the run. Once
        // `wait_agent` returns, ending the group is the supervisor's job, so
        // the guard is disarmed: a cleanup here would hide a member the
        // supervisor failed to kill. The parent's guard still reaps it.
        let group = GroupGuard(agent.id());
        let sidecar = test_sidecar(&socket, agent.id());
        let supervisor = Supervisor::serve(socket, sidecar).expect("serve the socket");
        let outcome = supervisor
            .wait_agent(&mut agent)
            .expect("wait for the agent");
        std::mem::forget(group);
        supervisor.finish();
        std::fs::write(outcome_path, format!("{outcome:?}")).expect("record the outcome");
    }

    /// A child supervisor the parent launched, once it answers a ping.
    struct Launched {
        child: ChildGuard,
        group: GroupGuard,
        socket: PathBuf,
        outcome: PathBuf,
        reply: PingReply,
    }

    impl Launched {
        fn outcome(&self) -> String {
            std::fs::read_to_string(&self.outcome).expect("the child recorded its outcome")
        }
    }

    /// Re-run `test_name` as a child supervisor for `script` and wait until
    /// its socket answers a ping.
    fn launch(test_name: &str, cache: &Path, script: &str, install_pipe: bool) -> Launched {
        let socket = SocketPath::for_launch(
            cache,
            Path::new("/project/runtime"),
            PhaseId::new(50),
            &new_launch_id(),
        )
        .expect("socket path fits sun_path");
        let outcome = cache.join("outcome");
        let pipe = if install_pipe { "1" } else { "0" };
        let mut child = ChildGuard(Some(spawn_test_in_child(
            test_name,
            cache,
            &[
                (SOCKET_ENV, socket.as_path().as_os_str()),
                (OUTCOME_ENV, outcome.as_os_str()),
                (AGENT_ENV, std::ffi::OsStr::new(script)),
                (PIPE_ENV, std::ffi::OsStr::new(pipe)),
            ],
        )));
        let deadline = Instant::now() + CHILD_WAIT;
        let reply = loop {
            if let Ok(Liveness::Alive(reply)) = probe(socket.as_path()) {
                break reply;
            }
            let exited = child.child().try_wait().expect("poll the child").is_some();
            if exited || Instant::now() >= deadline {
                let output = child.0.take().expect("owned").wait_with_output();
                panic!("the child supervisor never answered a ping (exited: {exited}): {output:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        Launched {
            group: GroupGuard(reply.agent_pgid),
            child,
            socket: socket.as_path().to_path_buf(),
            outcome,
            reply,
        }
    }

    const SHUTDOWN_TEST: &str = "supervisor::runtime::tests::shutdown_over_the_socket_kills_the_agent_group_and_reports_stopped";

    #[test]
    fn shutdown_over_the_socket_kills_the_agent_group_and_reports_stopped() {
        if crate::test_support::in_child_test(SHUTDOWN_TEST) {
            supervise_in_child();
            return;
        }
        let cache = short_cache();
        let mut run = launch(
            SHUTDOWN_TEST,
            cache.path(),
            "sleep 60 & sleep 60 & wait",
            false,
        );
        assert_eq!(run.reply.owner, "agent_running");
        let pgid = run.reply.agent_pgid;
        assert_eq!(pgid, run.reply.agent_pid, "the agent leads its own group");
        assert!(
            group_exists(pgid),
            "control: the agent group runs before the stop"
        );

        let answer = request_shutdown(&run.socket, CHILD_WAIT);
        assert_eq!(
            answer.as_ref().ok(),
            Some(&ShutdownOutcome::Stopped {
                exit_code: 143,
                advance_suppressed: true
            }),
            "a socket shutdown must report the stop, got {answer:?}"
        );
        let output = run.child.wait_exit();
        assert_child_ran_exactly_one_passing_test(&output, SHUTDOWN_TEST);
        assert!(
            wait_until(Duration::from_secs(5), || !group_exists(pgid)),
            "every member of the agent group must be gone"
        );
        assert!(
            matches!(probe(&run.socket), Ok(Liveness::Gone)),
            "the socket must be gone after the stop"
        );
        assert_eq!(run.outcome(), "Stopped");
        drop(run.group);
    }

    const COMPLETES_TEST: &str =
        "supervisor::runtime::tests::an_agent_that_exits_on_its_own_completes_with_its_code";

    #[test]
    fn an_agent_that_exits_on_its_own_completes_with_its_code() {
        if crate::test_support::in_child_test(COMPLETES_TEST) {
            supervise_in_child();
            return;
        }
        let cache = short_cache();
        let go = cache.path().join("go");
        let script = format!(
            "while [ ! -e '{}' ]; do sleep 0.05; done; exit 7",
            go.display()
        );
        let mut run = launch(COMPLETES_TEST, cache.path(), &script, false);
        assert_eq!(run.reply.owner, "agent_running");
        std::fs::write(&go, "").expect("let the agent exit");
        let output = run.child.wait_exit();
        assert_child_ran_exactly_one_passing_test(&output, COMPLETES_TEST);
        assert_eq!(run.outcome(), "Completed(7)");
        assert!(
            matches!(probe(&run.socket), Ok(Liveness::Gone)),
            "finish must remove the socket"
        );
    }

    /// One `line N: <needle>` entry per needle on each non-comment line of
    /// `source` that holds one: a process exit or a signal-mask or
    /// signal-wait call.
    fn forbidden_calls(source: &str) -> Vec<String> {
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
    fn needles() -> [String; 4] {
        [
            ["process", "::exit"].concat(),
            ["sigproc", "mask"].concat(),
            ["pthread_sig", "mask"].concat(),
            ["sig", "wait"].concat(),
        ]
    }

    #[test]
    fn runtime_source_has_none_of_the_forbidden_signal_or_exit_calls() {
        assert_eq!(
            forbidden_calls(include_str!("runtime.rs")),
            Vec::<String>::new(),
            "runtime.rs must not exit the process, block a signal or wait for one"
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

    /// The windows of one stage's life in which a stop can land (V-14).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Window {
        /// The agent is still running.
        BeforeExit,
        /// `waitid` has seen the exit, the leader is unreaped, and the exit
        /// decision has not been taken.
        ExitObservedBeforeDecision,
        /// The CLI is waiting on a gate.
        DuringGateWait,
        /// Safe point 1 has passed and the run is `Advancing` again, before
        /// safe point 2 (right before the next stage's spawn).
        AfterAdvancingBeforeSpawn,
        /// The next stage is spawned and answers ping, before it is
        /// published (safe point 3).
        BetweenSpawnAndPublish,
    }

    /// How one stage's life ended.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Verdict {
        /// The next stage was published: the pipeline advanced.
        Advanced,
        /// The run stopped at this decision and launched nothing further.
        Stopped(&'static str),
    }

    /// One stage's life against a real [`StopToken`]: a real agent process
    /// (`cat`, which exits when its stdin closes) is observed with `waitid`
    /// without reaping, the exit is decided under the owner lock, and the
    /// advance passes its gate wait and three safe points. `pause` runs at
    /// `window` and holds it open.
    fn run_stage(token: &StopToken, window: Option<Window>, pause: &dyn Fn()) -> Verdict {
        let at = |here: Window| {
            if window == Some(here) {
                pause();
            }
        };
        let mut agent = ChildGuard(Some(
            Command::new("/bin/cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .expect("spawn the agent"),
        ));
        at(Window::BeforeExit);
        drop(agent.child().stdin.take());
        wait_for_exit_without_reaping(agent.child().id()).expect("observe the exit");
        at(Window::ExitObservedBeforeDecision);
        let outcome = token
            .observe_exit(|_| agent.child().wait().map(exit_code))
            .expect("decide the exit");
        if outcome == AgentOutcome::Stopped {
            return Verdict::Stopped("exit decision");
        }
        assert_eq!(outcome, AgentOutcome::Completed(0));
        assert_eq!(token.phase(), OwnerPhase::Advancing);
        if token.safe_point() == SafePoint::StopHere {
            return Verdict::Stopped("safe point 1");
        }
        token.enter(OwnerPhase::GateWait);
        at(Window::DuringGateWait);
        if token.safe_point() == SafePoint::StopHere {
            return Verdict::Stopped("gate wait");
        }
        token.enter(OwnerPhase::Advancing);
        at(Window::AfterAdvancingBeforeSpawn);
        if token.safe_point() == SafePoint::StopHere {
            return Verdict::Stopped("safe point 2");
        }
        at(Window::BetweenSpawnAndPublish);
        if token.safe_point() == SafePoint::StopHere {
            return Verdict::Stopped("safe point 3");
        }
        Verdict::Advanced
    }

    #[test]
    fn a_stop_racing_natural_completion_never_advances() {
        let cases = [
            (
                Window::BeforeExit,
                StopAction::KillAgentGroup,
                "exit decision",
            ),
            (
                Window::ExitObservedBeforeDecision,
                StopAction::KillAgentGroup,
                "exit decision",
            ),
            (
                Window::DuringGateWait,
                StopAction::FlagForSafePoint,
                "gate wait",
            ),
            (
                Window::AfterAdvancingBeforeSpawn,
                StopAction::FlagForSafePoint,
                "safe point 2",
            ),
            (
                Window::BetweenSpawnAndPublish,
                StopAction::FlagForSafePoint,
                "safe point 3",
            ),
        ];
        for (window, expected_action, stopped_at) in cases {
            let token = StopToken::new(OwnerPhase::AgentRunning);
            let ready = Arc::new(std::sync::Barrier::new(2));
            let done = Arc::new(std::sync::Barrier::new(2));
            let stopper = {
                let (token, ready, done) = (token.clone(), Arc::clone(&ready), Arc::clone(&done));
                std::thread::spawn(move || {
                    ready.wait();
                    let action = token.request_stop(StopSource::Socket);
                    done.wait();
                    action
                })
            };
            let reached = std::cell::Cell::new(false);
            let verdict = run_stage(&token, Some(window), &|| {
                reached.set(true);
                ready.wait();
                done.wait();
            });
            // A run that ended before its window would leave the stopper
            // blocked on the barrier: fail here rather than hang on the join.
            assert!(
                reached.get(),
                "{window:?}: the run ended before its window was reached: {verdict:?}"
            );
            let action = stopper.join().expect("the stopper thread");
            assert_eq!(action, expected_action, "{window:?}: the stop's action");
            assert_eq!(
                verdict,
                Verdict::Stopped(stopped_at),
                "{window:?}: a stop must end the run, never advance it"
            );
            assert!(token.stop_requested(), "{window:?}: the stop is recorded");
            assert!(token.ended_stopped(), "{window:?}: the run ended as a stop");
        }
    }

    #[test]
    fn natural_completion_without_a_stop_advances() {
        let token = StopToken::new(OwnerPhase::AgentRunning);
        assert_eq!(run_stage(&token, None, &|| {}), Verdict::Advanced);
        assert!(!token.stop_requested());
        assert!(!token.ended_stopped(), "no stop, so no stopped line");
        assert_eq!(token.phase(), OwnerPhase::Advancing);
    }

    #[test]
    fn every_owner_phase_has_exactly_one_stop_decision() {
        use OwnerPhase::{Advancing, AgentRunning, Finished, GateWait, HandedOff};
        let table = [
            (
                AgentRunning,
                StopAction::KillAgentGroup,
                true,
                "agent_running",
            ),
            (Advancing, StopAction::FlagForSafePoint, true, "advancing"),
            (GateWait, StopAction::FlagForSafePoint, true, "gate_wait"),
            (HandedOff, StopAction::ReplyHandedOff, false, "handed_off"),
            (Finished, StopAction::AlreadyFinished, false, "finished"),
        ];
        for (phase, action, flags, label) in table {
            assert_eq!(on_stop_request(phase), action, "{phase:?}");
            assert_eq!(phase.as_str(), label);
            let token = StopToken::new(phase);
            assert_eq!(
                token.safe_point(),
                SafePoint::Proceed,
                "{phase:?}: before a stop"
            );
            assert!(!token.ended_stopped(), "{phase:?}: before a stop");
            assert_eq!(
                token.request_stop(StopSource::Signal(libc::SIGTERM)),
                action
            );
            assert_eq!(token.stop_requested(), flags, "{phase:?}: the stop flag");
            assert_eq!(
                token.phase(),
                phase,
                "{phase:?}: a stop never moves the phase"
            );
            let expected = if flags {
                SafePoint::StopHere
            } else {
                SafePoint::Proceed
            };
            assert_eq!(
                token.safe_point(),
                expected,
                "{phase:?}: the next safe point"
            );
            assert_eq!(token.ended_stopped(), flags, "{phase:?}: ended as a stop");
        }
    }

    #[test]
    fn exit_and_safe_point_decisions_cover_every_input() {
        for code in [0, 1, 7, 137, STOPPED_EXIT_CODE] {
            assert_eq!(on_agent_exit(false, code), AgentOutcome::Completed(code));
            assert_eq!(on_agent_exit(true, code), AgentOutcome::Stopped);
        }
        assert_eq!(at_safe_point(false), SafePoint::Proceed);
        assert_eq!(at_safe_point(true), SafePoint::StopHere);
        // The exit decision leaves the phase where the CLI expects it.
        let completed = StopToken::new(OwnerPhase::AgentRunning);
        assert_eq!(
            completed.observe_exit(|_| Ok(7)).expect("decide"),
            AgentOutcome::Completed(7)
        );
        assert_eq!(completed.phase(), OwnerPhase::Advancing);
        assert!(!completed.ended_stopped());
        let stopped = StopToken::new(OwnerPhase::AgentRunning);
        stopped.request_stop(StopSource::Socket);
        assert_eq!(
            stopped
                .observe_exit(|stop| Ok(if stop { 143 } else { 0 }))
                .expect("decide"),
            AgentOutcome::Stopped
        );
        assert_eq!(stopped.phase(), OwnerPhase::Finished);
        assert!(stopped.ended_stopped());
    }

    /// The named mask field of `/proc/<pid>/status` (`SigBlk`, `SigCgt`,
    /// `SigIgn`, ...). Panics when the field is absent, so an unreadable
    /// mask never reads as clear.
    #[cfg(target_os = "linux")]
    fn signal_mask_field(pid: u32, field: &str) -> u64 {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
            .unwrap_or_else(|error| panic!("/proc/{pid}/status must be readable: {error}"));
        let hex = status
            .lines()
            .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
            .unwrap_or_else(|| panic!("/proc/{pid}/status has no {field} line"))
            .trim();
        u64::from_str_radix(hex, 16).expect("signal masks are hex")
    }

    /// `signal`'s bit in a `/proc` signal mask.
    #[cfg(target_os = "linux")]
    fn mask_bit(signal: libc::c_int) -> u64 {
        1 << (signal - 1)
    }

    /// Whether `pid` is gone or a zombie: it can no longer run.
    #[cfg(target_os = "linux")]
    fn process_is_dead(pid: u32) -> bool {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(_) => true,
            Ok(stat) => stat
                .rfind(')')
                .and_then(|end| stat[end + 1..].split_whitespace().next())
                .is_some_and(|state| state == "Z"),
        }
    }

    /// Send `signal` to a child this test spawned and still owns.
    #[cfg(target_os = "linux")]
    fn signal_child(child: &mut ChildGuard, signal: libc::c_int) {
        let pid = libc::pid_t::try_from(child.child().id()).expect("pid fits pid_t");
        assert!(pid > 1, "refusing to signal pid {pid}");
        // SAFETY: `pid > 1` is a single, unreaped child of this process.
        assert_eq!(unsafe { libc::kill(pid, signal) }, 0, "signal the child");
    }

    #[cfg(target_os = "linux")]
    const SIGNAL_TEST: &str = "supervisor::runtime::tests::sigterm_is_delivered_as_a_stop_event_not_handled_in_the_handler";

    #[cfg(target_os = "linux")]
    #[test]
    fn sigterm_is_delivered_as_a_stop_event_not_handled_in_the_handler() {
        use crate::test_support::{EXEC_VISIBILITY_POLL, EXEC_VISIBILITY_WAIT};
        use std::os::unix::process::ExitStatusExt;
        if crate::test_support::in_child_test(SIGNAL_TEST) {
            supervise_in_child();
            return;
        }
        let script = "sleep 60 & exec sleep 60";
        for signal in [libc::SIGTERM, libc::SIGINT] {
            let cache = short_cache();
            let mut run = launch(SIGNAL_TEST, cache.path(), script, true);
            let (agent, pgid) = (run.reply.agent_pid, run.reply.agent_pgid);
            // Read the masks only once the agent runs `sleep`: before exec it
            // is a copy of the supervisor and carries its handler.
            assert!(
                crate::test_support::wait_for_exec_visibility(
                    agent,
                    "sleep",
                    EXEC_VISIBILITY_WAIT,
                    EXEC_VISIBILITY_POLL
                ),
                "the agent must exec sleep"
            );
            let supervisor = run.child.child().id();
            assert_ne!(
                signal_mask_field(supervisor, "SigCgt") & mask_bit(libc::SIGTERM),
                0,
                "control: the mask reader sees the supervisor's own TERM handler"
            );
            let term_and_int = mask_bit(libc::SIGTERM) | mask_bit(libc::SIGINT);
            assert_eq!(
                signal_mask_field(agent, "SigBlk") & term_and_int,
                0,
                "the agent must not inherit a blocked TERM or INT"
            );
            assert_eq!(
                signal_mask_field(agent, "SigCgt") & mask_bit(libc::SIGTERM),
                0,
                "the agent's TERM handler must be the default"
            );

            signal_child(&mut run.child, signal);
            let output = run.child.wait_exit();
            assert_child_ran_exactly_one_passing_test(&output, SIGNAL_TEST);
            assert_eq!(run.outcome(), "Stopped", "signal {signal} is a stop");
            assert!(
                wait_until(Duration::from_secs(5), || !group_exists(pgid)),
                "signal {signal}: the agent group must be gone"
            );
            assert!(
                matches!(probe(&run.socket), Ok(Liveness::Gone)),
                "signal {signal}: the socket must be gone"
            );
        }

        // Control: a supervisor that did not install the pipe dies on the
        // same SIGTERM and leaves its agent group running.
        let cache = short_cache();
        let mut run = launch(SIGNAL_TEST, cache.path(), script, false);
        let pgid = run.reply.agent_pgid;
        signal_child(&mut run.child, libc::SIGTERM);
        let output = run.child.wait_exit();
        assert_eq!(
            output.status.signal(),
            Some(libc::SIGTERM),
            "with no handler the supervisor dies on TERM: {output:?}"
        );
        assert!(
            group_exists(pgid),
            "with no handler the agent group outlives its supervisor"
        );
        // Reap that group by the pgid its ping reply named.
        assert!(
            signal_group(pgid, libc::SIGKILL),
            "SIGKILL the orphaned group"
        );
        assert!(
            wait_until(Duration::from_secs(5), || !group_exists(pgid)),
            "the orphaned group must be gone after SIGKILL"
        );
    }

    #[cfg(target_os = "linux")]
    const IGNORER_TEST: &str =
        "supervisor::runtime::tests::a_term_ignoring_group_member_is_killed_after_the_leader_exits";

    #[cfg(target_os = "linux")]
    #[test]
    fn a_term_ignoring_group_member_is_killed_after_the_leader_exits() {
        use crate::test_support::{EXEC_VISIBILITY_POLL, EXEC_VISIBILITY_WAIT};
        if crate::test_support::in_child_test(IGNORER_TEST) {
            supervise_in_child();
            return;
        }
        let cache = short_cache();
        let member_file = cache.path().join("member");
        let script = format!(
            "(trap '' TERM; exec sleep 60) & echo $! > '{}'; wait",
            member_file.display()
        );
        let mut run = launch(IGNORER_TEST, cache.path(), &script, false);
        let pgid = run.reply.agent_pgid;
        let mut member = None;
        assert!(
            wait_until(Duration::from_secs(5), || {
                member = std::fs::read_to_string(&member_file)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok());
                member.is_some()
            }),
            "the agent must record its member's pid"
        );
        let member = member.expect("member pid");
        assert!(
            crate::test_support::wait_for_exec_visibility(
                member,
                "sleep",
                EXEC_VISIBILITY_WAIT,
                EXEC_VISIBILITY_POLL
            ),
            "the member must exec sleep"
        );
        // The fixture is what it claims: the member is in the agent's group
        // and ignores TERM.
        // SAFETY: getpgid only reads; `member` is a live pid we just saw.
        let member_pgid = unsafe { libc::getpgid(libc::pid_t::try_from(member).expect("pid")) };
        assert_eq!(u32::try_from(member_pgid).ok(), Some(pgid));
        assert_ne!(
            signal_mask_field(member, "SigIgn") & mask_bit(libc::SIGTERM),
            0,
            "the member must ignore TERM"
        );

        let answer = request_shutdown(&run.socket, CHILD_WAIT);
        assert_eq!(
            answer.as_ref().ok(),
            Some(&ShutdownOutcome::Stopped {
                exit_code: 143,
                advance_suppressed: true
            }),
            "the stop is reported, got {answer:?}"
        );
        assert!(
            wait_until(crate::agent::TERMINATE_VERIFY_WAIT, || process_is_dead(
                member
            )),
            "the TERM-ignoring member must be gone within the grace window"
        );
        let output = run.child.wait_exit();
        assert_child_ran_exactly_one_passing_test(&output, IGNORER_TEST);
        assert_eq!(run.outcome(), "Stopped");
        assert!(
            wait_until(Duration::from_secs(5), || !group_exists(pgid)),
            "the agent group must be gone"
        );
    }

    /// A supervisor served from this test process for an agent that is
    /// never signalled: its sidecar pgid is `0`, which `signal_group`
    /// refuses, and no test here stops it in `AgentRunning`.
    fn in_process_supervisor(cache: &Path) -> (Supervisor, PathBuf) {
        let socket = SocketPath::for_launch(
            cache,
            Path::new("/project/runtime"),
            PhaseId::new(50),
            &new_launch_id(),
        )
        .expect("socket path fits sun_path");
        let mut sidecar = test_sidecar(&socket, std::process::id());
        sidecar.agent_pgid = 0;
        let path = socket.as_path().to_path_buf();
        let supervisor = Supervisor::serve(socket, sidecar).expect("serve the socket");
        (supervisor, path)
    }

    /// A launch never replaces an existing sidecar at its socket's name, and
    /// leaves none of its own beside a socket it could not publish. Without
    /// this, `serve` overwrote another launch's sidecar and only then found
    /// the name taken, so `stop` would validate that launch's group against
    /// this one's identity.
    #[test]
    fn serve_never_replaces_an_existing_sidecar_at_its_name() {
        let cache = short_cache();
        let socket_for = || {
            SocketPath::for_launch(
                cache.path(),
                Path::new("/project/runtime"),
                PhaseId::new(50),
                &new_launch_id(),
            )
            .expect("socket path fits sun_path")
        };

        // Another launch's socket and sidecar hold the name.
        let taken = socket_for();
        let theirs = test_sidecar(&taken, std::process::id());
        crate::supervisor::write_sidecar(&taken, &theirs).expect("plant their sidecar");
        let their_bytes =
            std::fs::read_to_string(taken.sidecar_path()).expect("read their sidecar");
        let _their_listener = crate::supervisor::publish(&taken).expect("plant their socket");
        let mut ours = theirs.clone();
        ours.agent_pgid = 0;
        ours.agent_start_time += 1;
        let result = Supervisor::serve(taken.clone(), ours.clone());
        assert!(result.is_err(), "the name is taken: {result:?}");
        assert_eq!(
            std::fs::read_to_string(taken.sidecar_path()).expect("their sidecar survives"),
            their_bytes,
            "their sidecar is unchanged"
        );

        // A socket with no sidecar holds the name: no sidecar of ours is
        // left beside it to misattribute it.
        let bare = socket_for();
        let _bare_listener = crate::supervisor::publish(&bare).expect("plant a bare socket");
        let result = Supervisor::serve(bare.clone(), ours);
        assert!(
            matches!(result, Err(PublishError::AlreadyExists(_))),
            "the name is taken: {result:?}"
        );
        assert!(
            !bare.sidecar_path().exists(),
            "no sidecar of ours is left beside a socket we did not publish"
        );

        // Control: a free name serves, with the sidecar written.
        let (supervisor, path) = in_process_supervisor(cache.path());
        assert!(path.with_extension("json").exists());
        supervisor.finish();
    }

    #[test]
    fn a_parked_shutdown_hears_stopped_only_when_the_run_stopped() {
        let cache = short_cache();
        let stopped = ShutdownOutcome::Stopped {
            exit_code: 143,
            advance_suppressed: true,
        };

        // Taken while the CLI advances, then stopped at a safe point: the
        // parked caller hears `stopped`, and the socket is already gone.
        let (supervisor, path) = in_process_supervisor(cache.path());
        let token = supervisor.token();
        token.enter(OwnerPhase::Advancing);
        let caller = {
            let path = path.clone();
            std::thread::spawn(move || request_shutdown(&path, CHILD_WAIT).ok())
        };
        assert!(
            wait_until(CHILD_WAIT, || token.stop_requested()),
            "the request is taken"
        );
        assert_eq!(token.safe_point(), SafePoint::StopHere);
        supervisor.finish();
        assert_eq!(caller.join().expect("caller"), Some(stopped));
        assert!(matches!(probe(&path), Ok(Liveness::Gone)));

        // Control: taken while advancing, but no safe point stopped the run
        // before it ended: the caller hears `handed_off`, never `stopped`.
        let (supervisor, path) = in_process_supervisor(cache.path());
        let token = supervisor.token();
        token.enter(OwnerPhase::Advancing);
        let caller = {
            let path = path.clone();
            std::thread::spawn(move || request_shutdown(&path, CHILD_WAIT).ok())
        };
        assert!(
            wait_until(CHILD_WAIT, || token.stop_requested()),
            "the request is taken"
        );
        supervisor.finish();
        assert_eq!(
            caller.join().expect("caller"),
            Some(ShutdownOutcome::HandedOff)
        );

        // Handed off before the request: answered at once, nothing flagged,
        // and a ping names the owner.
        let (supervisor, path) = in_process_supervisor(cache.path());
        supervisor.hand_off();
        match probe(&path) {
            Ok(Liveness::Alive(reply)) => assert_eq!(reply.owner, "handed_off"),
            other => panic!("a served socket answers ping, got {other:?}"),
        }
        assert_eq!(
            request_shutdown(&path, CHILD_WAIT).ok(),
            Some(ShutdownOutcome::HandedOff)
        );
        assert!(!supervisor.token().stop_requested());
        supervisor.finish();
        assert!(matches!(probe(&path), Ok(Liveness::Gone)));
    }

    // ---- 50-23 (R6-E): the stopped-outcome test hold stays test-only ----

    /// `source`'s non-test lines: the part before the first line that is
    /// exactly `mod tests {`, without its whole-line `//` comments.
    fn non_test_code_lines(source: &str) -> Vec<&str> {
        source
            .lines()
            .take_while(|line| *line != "mod tests {")
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect()
    }

    /// The lines of the item whose header line starts with `header`, through
    /// the next line starting with `closing`; `None` when no line starts with
    /// `header`.
    fn item_lines<'a>(lines: &[&'a str], header: &str, closing: &str) -> Option<Vec<&'a str>> {
        let start = lines.iter().position(|line| line.starts_with(header))?;
        let mut item = vec![lines[start]];
        for line in &lines[start + 1..] {
            item.push(*line);
            if line.starts_with(closing) {
                break;
            }
        }
        Some(item)
    }

    const HOLD_NO_LITERAL_READ: &str =
        "no literal std::env::var_os/var read of the stopped-outcome hold variable";
    const HOLD_FN_NOT_ONCE: &str = "fn stopped_outcome_test_hold( is not defined exactly once";
    const HOLD_NOT_GATED: &str = "stopped_outcome_test_hold is not directly under #[cfg(any(test, feature = \"test-support\"))]";
    const WAIT_AGENT_NOT_FOUND: &str = "Supervisor::wait_agent not found (or under 5 lines)";
    const WAIT_AGENT_NO_HOLD: &str =
        "Supervisor::wait_agent does not call stopped_outcome_test_hold";

    /// R6-E as a pure scan over `source`'s non-test, comment-stripped code:
    /// the hold variable is read literally (the form the doc check sees),
    /// `stopped_outcome_test_hold` is defined once directly under the
    /// test-support gate, and `wait_agent` calls it.
    fn stopped_outcome_hold_violations(source: &str) -> Vec<String> {
        let lines = non_test_code_lines(source);
        let mut violations = Vec::new();
        let reads = [
            r#"std::env::var_os("DEVFLOW_TEST_STOPPED_OUTCOME_HOLD")"#,
            r#"std::env::var("DEVFLOW_TEST_STOPPED_OUTCOME_HOLD")"#,
        ];
        if !lines
            .iter()
            .any(|line| reads.iter().any(|read| line.contains(read)))
        {
            violations.push(HOLD_NO_LITERAL_READ.to_string());
        }
        let defined: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.trim_start()
                    .starts_with("fn stopped_outcome_test_hold(")
            })
            .map(|(at, _)| at)
            .collect();
        match defined.as_slice() {
            [at] => {
                let gated = at.checked_sub(1).is_some_and(|above| {
                    lines[above].ends_with(r#"#[cfg(any(test, feature = "test-support"))]"#)
                });
                if !gated {
                    violations.push(HOLD_NOT_GATED.to_string());
                }
            }
            _ => violations.push(HOLD_FN_NOT_ONCE.to_string()),
        }
        match item_lines(&lines, "    pub fn wait_agent(", "    }").filter(|item| item.len() >= 5) {
            None => violations.push(WAIT_AGENT_NOT_FOUND.to_string()),
            Some(item) => {
                if !item
                    .iter()
                    .any(|line| line.contains("stopped_outcome_test_hold("))
                {
                    violations.push(WAIT_AGENT_NO_HOLD.to_string());
                }
            }
        }
        violations
    }

    /// T-50-85 (permanent source scan, R6-G4, R6-E): a release build never
    /// compiles the stopped-outcome hold. The real file yields no violation;
    /// each planted source yields the violation it plants.
    #[test]
    fn the_stopped_outcome_hold_is_test_only_and_read_literally() {
        let source = include_str!("runtime.rs");
        assert!(
            source.lines().any(|line| line == "mod tests {"),
            "runtime.rs must keep its `mod tests {{` line, or the scan would read its own controls"
        );
        let real = stopped_outcome_hold_violations(source);
        assert!(real.is_empty(), "runtime.rs: {real:?}");

        let gate = "#[cfg(any(test, feature = \"test-support\"))]\n";
        let read = "    let _path = std::env::var_os(\"DEVFLOW_TEST_STOPPED_OUTCOME_HOLD\");\n";
        let hold = |attribute: &str, body: &str| {
            format!("{attribute}fn stopped_outcome_test_hold(stop_requested: bool) {{\n{body}}}\n")
        };
        let pad = "        let _filler = 0;\n".repeat(4);
        let call = "        stopped_outcome_test_hold(true);\n";
        let wait_agent = |body: &str| {
            format!("impl Supervisor {{\n    pub fn wait_agent(&self) {{\n{pad}{body}    }}\n")
        };
        let other = |body: &str| format!("    pub fn token(&self) {{\n{body}    }}\n}}\n");
        let good = format!("{}{}{}", hold(gate, read), wait_agent(call), other(""));
        let only = |violation: &str| vec![violation.to_string()];

        assert_eq!(
            stopped_outcome_hold_violations(&good),
            Vec::<String>::new(),
            "a correct layout"
        );
        assert_eq!(
            stopped_outcome_hold_violations(&format!(
                "{}{}{}",
                hold("#[cfg(test)]\n", read),
                wait_agent(call),
                other("")
            )),
            only(HOLD_NOT_GATED),
            "gated by cfg(test) only"
        );
        assert_eq!(
            stopped_outcome_hold_violations(&format!(
                "{}{}{}",
                hold("", read),
                wait_agent(call),
                other("")
            )),
            only(HOLD_NOT_GATED),
            "no attribute"
        );
        assert_eq!(
            stopped_outcome_hold_violations(&format!(
                "const HOLD: &str = \"hold\";\n{}{}{}",
                hold(gate, "    let _path = std::env::var_os(HOLD);\n"),
                wait_agent(call),
                other("")
            )),
            only(HOLD_NO_LITERAL_READ),
            "a read through a constant"
        );
        assert_eq!(
            stopped_outcome_hold_violations(&format!(
                "{}{}{}",
                hold(gate, read),
                wait_agent("        // stopped_outcome_test_hold(true);\n"),
                other("")
            )),
            only(WAIT_AGENT_NO_HOLD),
            "a commented-out call"
        );
        assert_eq!(
            stopped_outcome_hold_violations(&format!(
                "{}{}{}",
                hold(gate, read),
                wait_agent(""),
                other(call)
            )),
            only(WAIT_AGENT_NO_HOLD),
            "the call moved to another method"
        );
        assert_eq!(
            stopped_outcome_hold_violations(&format!("mod tests {{\n{good}")),
            vec![
                HOLD_NO_LITERAL_READ.to_string(),
                HOLD_FN_NOT_ONCE.to_string(),
                WAIT_AGENT_NOT_FOUND.to_string()
            ],
            "the read, the hold and a decoy wait_agent only after the test-module cut"
        );
    }
}

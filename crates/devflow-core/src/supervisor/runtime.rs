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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

/// The exit code a stopped run reports: the shell convention for a death by
/// SIGTERM, `128 + 15`.
const STOPPED_EXIT_CODE: i32 = 128 + libc::SIGTERM;

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
/// only that group.
fn wait_for_exit_without_reaping(pid: u32) -> io::Result<()> {
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

/// An exit status as a code: the exit code, or `128 + n` for a death by
/// signal `n` (the `monitor` convention: SIGTERM is 143, SIGKILL 137).
fn exit_code(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map_or(-1, |signal| 128 + signal)
    })
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
    pub fn serve(socket: SocketPath, sidecar: Sidecar) -> Result<Self, PublishError> {
        super::write_sidecar(&socket, &sidecar)?;
        let listener = match super::publish(&socket) {
            Ok(listener) => listener,
            // Another launch's live socket holds the name: its sidecar is
            // not ours to remove.
            Err(PublishError::AlreadyExists(path)) => {
                return Err(PublishError::AlreadyExists(path));
            }
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
            let mut fds = [libc::pollfd {
                fd: self.listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            // SAFETY: `fds` is a live array of initialized pollfd entries and
            // its length is passed alongside it.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, tick) };
            if ready <= 0 {
                continue;
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

    /// The child half shared by every runtime process test: spawn the agent script in its own process
    /// group, serve, wait, finish, and record the outcome.
    fn supervise_in_child() {
        let socket = std::env::var_os(SOCKET_ENV).expect("the parent names the socket");
        let socket = SocketPath::new(PathBuf::from(socket)).expect("socket path fits");
        let outcome_path = std::env::var_os(OUTCOME_ENV).expect("the parent names the outcome");
        let script = std::env::var_os(AGENT_ENV).expect("the parent names the agent script");
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
        let _group = GroupGuard(agent.id());
        let sidecar = test_sidecar(&socket, agent.id());
        let supervisor = Supervisor::serve(socket, sidecar).expect("serve the socket");
        let outcome = supervisor
            .wait_agent(&mut agent)
            .expect("wait for the agent");
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
    fn launch(test_name: &str, cache: &Path, script: &str) -> Launched {
        let socket = SocketPath::for_launch(
            cache,
            Path::new("/project/runtime"),
            PhaseId::new(50),
            &new_launch_id(),
        )
        .expect("socket path fits sun_path");
        let outcome = cache.join("outcome");
        let mut child = ChildGuard(Some(spawn_test_in_child(
            test_name,
            cache,
            &[
                (SOCKET_ENV, socket.as_path().as_os_str()),
                (OUTCOME_ENV, outcome.as_os_str()),
                (AGENT_ENV, std::ffi::OsStr::new(script)),
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
        let mut run = launch(SHUTDOWN_TEST, cache.path(), "sleep 60 & sleep 60 & wait");
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
        let mut run = launch(COMPLETES_TEST, cache.path(), &script);
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
}

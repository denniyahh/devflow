//! The stale-socket backstop (D-04, R-4, R-8): what `devflow stop` does
//! when a launch's socket probes [`Liveness::Stale`]. The supervisor is
//! gone, but the agent's process group may still be running.
//!
//! This code signals a process group by a value read from disk, so it is
//! split in two. [`classify`] is a pure decision over gathered facts
//! ([`BackstopFacts`]). [`teardown_stale`] is a thin shell that acts on it.
//! The rule, in order:
//!
//! 0. A recorded pgid that does not fit `pid_t`, is `<= 1`, or is the
//!    caller's own process group is [`BackstopDecision::RefuseUnsafeTarget`],
//!    decided before any `/proc` read.
//! 1. Where process identity cannot be proved (off Linux, a boot id or
//!    process table that cannot be read, or a launch that recorded 0 or ""
//!    for the agent's start time or the boot id), the answer is
//!    [`BackstopDecision::RefuseIdentityUnavailable`] (MAC-2).
//! 2. The sidecar and the state record (when given) must agree on launch id,
//!    pgid, agent start time and boot id, or it is
//!    [`BackstopDecision::RefuseMismatch`].
//! 3. A sidecar from another boot is [`BackstopDecision::PreviousBoot`].
//! 4. No live (non-zombie) process of this uid in the group is
//!    [`BackstopDecision::EmptyGroup`].
//! 5. A leader (pid == pgid) with the recorded start time that still leads
//!    the group is [`BackstopDecision::LeaderPresentSignal`].
//! 6. A leader pid held by a process with another start time is
//!    [`BackstopDecision::LeaderRecycled`]: the kernel does not reuse a pid
//!    while it names a process group (RESEARCH A1), so the original group is
//!    gone.
//! 7. Otherwise the leader is gone (or has left the group) while members
//!    live: [`BackstopDecision::RefuseLeaderGone`]. They are reported, never
//!    signalled, because nothing proves they are the agent's (the residual
//!    F1 limit).
//!
//! Only `LeaderPresentSignal` signals. On Linux the signal goes through a
//! pidfd opened on the leader, with the start time re-read after opening, and
//! `PIDFD_SIGNAL_PROCESS_GROUP`. A kernel without that flag (`EINVAL`) falls
//! back to validate-then-`killpg` ([`SignalPath::ValidatedKillpg`]). Off Linux
//! no signalling path exists.

use super::{Liveness, Sidecar, SocketPath};
use crate::agent::ProcStat;
use crate::state::SupervisorRecord;
use std::io;

/// How long to wait for the group to empty after SIGKILL. SIGKILL cannot be
/// caught, but its delivery is not synchronous.
const KILL_VERIFY_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// One live process of the recorded process group, as gathered from
/// `/proc`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GroupMember {
    /// The member's pid.
    pub pid: u32,
    /// Its start time in clock ticks since boot.
    pub start_time: u64,
    /// Its one-letter `/proc` state.
    pub state: char,
    /// Its session id.
    pub session: i32,
    /// Its command line, arguments joined by spaces (empty when unreadable).
    pub cmdline: String,
}

/// The identity a launch recorded, read from its sidecar or its state
/// record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordedIdentity {
    pub(crate) launch_id: String,
    pub(crate) pgid: u32,
    pub(crate) start_time: u64,
    pub(crate) boot_id: String,
}

impl RecordedIdentity {
    fn of_sidecar(sidecar: &Sidecar) -> Self {
        Self {
            launch_id: sidecar.launch_id.clone(),
            pgid: sidecar.agent_pgid,
            start_time: sidecar.agent_start_time,
            boot_id: sidecar.boot_id.clone(),
        }
    }

    fn of_record(record: &SupervisorRecord) -> Self {
        Self {
            launch_id: record.launch_id.clone(),
            pgid: record.agent_pgid,
            start_time: record.agent_start_time,
            boot_id: record.boot_id.clone(),
        }
    }
}

/// Everything [`classify`] decides on, gathered once.
#[derive(Debug, Clone)]
pub(crate) struct BackstopFacts {
    /// The sidecar's identity.
    pub(crate) recorded: RecordedIdentity,
    /// The state record's identity, when state loaded.
    pub(crate) state_record: Option<RecordedIdentity>,
    /// The caller's own process group (`getpgrp`).
    pub(crate) caller_pgrp: i32,
    /// The current kernel boot id.
    pub(crate) current_boot_id: Option<String>,
    /// `/proc/<pgid>/stat` when an own-uid process holds that pid.
    pub(crate) leader: Option<ProcStat>,
    /// Own-uid, non-zombie processes whose pgrp is the recorded pgid.
    pub(crate) members: Vec<GroupMember>,
    /// Whether this platform could gather the facts that prove identity
    /// (MAC-2): `/proc` start times, the group census and the boot id.
    pub(crate) identity_provable: bool,
}

/// What the backstop decides for a STALE socket. See the module doc for the
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackstopDecision {
    /// The recorded pgid does not fit `pid_t`, is 0 or 1, or is the
    /// caller's own process group. Nothing is signalled or removed.
    RefuseUnsafeTarget,
    /// This platform gives DevFlow no process start time, group census or
    /// boot id (off Linux; on Linux, an unreadable boot id or process table,
    /// or a launch that could not record them), so the group's identity
    /// cannot be proved. Callers treat it like the
    /// other `Refuse*` decisions: signal nothing, keep the socket and
    /// sidecar, and count the run as possibly live (fail closed).
    RefuseIdentityUnavailable,
    /// The sidecar and the state record disagree; the text names the field.
    /// Nothing is signalled or removed.
    RefuseMismatch(String),
    /// The sidecar was written in a previous boot, so nothing it names can
    /// be running. The socket and sidecar are removed; nothing is signalled.
    PreviousBoot,
    /// No live process of this user is in the recorded group. The socket and
    /// sidecar are removed; nothing is signalled.
    EmptyGroup,
    /// The leader is the recorded agent and still leads the group: the group
    /// is signalled (SIGTERM, then SIGKILL).
    LeaderPresentSignal,
    /// The leader pid now names a different process, so the recorded group
    /// is gone. The socket and sidecar are removed; nothing is signalled.
    LeaderRecycled,
    /// The leader is gone (or has left the group) but these members live.
    /// They are reported, never signalled, and the socket and sidecar are
    /// kept.
    RefuseLeaderGone(Vec<GroupMember>),
}

impl BackstopDecision {
    /// The text the CLI prints for this decision about the launch whose
    /// socket is `socket` and whose sidecar is `sidecar`.
    ///
    /// A `RefuseLeaderGone` text lists each member's pid, start time, state,
    /// session and command line, states the residual F1 limit and gives an
    /// inspection command. A `RefuseIdentityUnavailable` text gives the
    /// manual repair and names no platform command (MAC-2, backlog 999.158).
    pub fn describe(&self, socket: &SocketPath, sidecar: &Sidecar) -> String {
        use std::fmt::Write as _;

        let pgid = sidecar.agent_pgid;
        let launch = &sidecar.launch_id;
        let files = format!(
            "{} and {}",
            socket.as_path().display(),
            socket.sidecar_path().display()
        );
        match self {
            Self::RefuseUnsafeTarget => format!(
                "refused: process group {pgid} of launch {launch} is not a safe target (it is 0 \
                 or 1, does not fit a pid, or is this process's own group); nothing was \
                 signalled or removed"
            ),
            Self::RefuseIdentityUnavailable => format!(
                "nothing was signalled or removed; DevFlow has no process start time or boot id \
                 for it (this platform gives none, or the launch could not record them), so it \
                 cannot prove process group {pgid} is still the agent launch {launch} started; if \
                 you have confirmed it is, end that group yourself, then delete {files}"
            ),
            Self::RefuseMismatch(why) => format!(
                "refused: the sidecar and the state record of launch {launch} disagree ({why}); \
                 nothing was signalled or removed"
            ),
            Self::PreviousBoot => format!(
                "launch {launch} was recorded in a previous boot (boot id {}), so process group \
                 {pgid} cannot still be its agent; nothing is signalled, and {files} are removed",
                sidecar.boot_id
            ),
            Self::EmptyGroup => format!(
                "process group {pgid} of launch {launch} has no live process of this user; \
                 nothing is signalled, and {files} are removed"
            ),
            Self::LeaderPresentSignal => format!(
                "process group {pgid} is still led by launch {launch}'s agent (start time {}); \
                 the group is ended with SIGTERM, then SIGKILL, and {files} are removed",
                sidecar.agent_start_time
            ),
            Self::LeaderRecycled => format!(
                "pid {pgid} now belongs to a different process than launch {launch}'s agent \
                 (start time {}), so the agent's group is gone; nothing is signalled, and {files} \
                 are removed",
                sidecar.agent_start_time
            ),
            Self::RefuseLeaderGone(members) => {
                let mut text = format!(
                    "refused: process group {pgid} of launch {launch} still has {} live \
                     member(s), but its leader, the recorded agent (pid {pgid}), is gone or has \
                     left the group. The leader is gone, so these may be the original agent's \
                     children or an unrelated group that reused the id; nothing was signalled, \
                     and {files} are kept. Inspect the group with: ps -o \
                     pid,pgid,sid,lstart,args -p \"$(pgrep -d, -g {pgid})\"",
                    members.len()
                );
                for member in members {
                    let _ = write!(
                        text,
                        "\n  pid {}, start time {}, state {}, session {}: {}",
                        member.pid, member.start_time, member.state, member.session, member.cmdline
                    );
                }
                text
            }
        }
    }
}

/// How a group was signalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignalPath {
    /// `pidfd_send_signal` with `PIDFD_SIGNAL_PROCESS_GROUP` on a pidfd of
    /// the validated leader.
    PidfdGroup,
    /// The kernel lacks `PIDFD_SIGNAL_PROCESS_GROUP` (`EINVAL`): the leader
    /// was re-validated through its pidfd and `/proc`, then `killpg`.
    ValidatedKillpg,
}

/// What [`teardown_stale`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackstopOutcome {
    /// The group was signalled and is empty; the socket and sidecar are
    /// removed. `members` is the group as gathered before signalling.
    Killed {
        /// How the group was signalled.
        via: SignalPath,
        /// The group's live members before the signal.
        members: Vec<GroupMember>,
    },
    /// Nothing was signalled and the socket and sidecar were removed
    /// (`PreviousBoot`, `EmptyGroup` or `LeaderRecycled`).
    RemovedNothingSignalled(BackstopDecision),
    /// The socket and sidecar were kept. Nothing was signalled, except in
    /// one case: after the SIGTERM, the leader exited and members survived
    /// it, which reads `RefuseLeaderGone` and those members got that one
    /// SIGTERM.
    Refused(BackstopDecision),
}

/// Why [`teardown_stale`] did not reach a decision.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackstopError {
    /// The socket did not probe STALE (it is alive, gone, or the probe
    /// failed); the backstop only acts on a STALE socket.
    #[error("the socket is not stale: {0}")]
    NotStale(String),
    /// Reading, signalling or removing failed.
    #[error("the stale-socket backstop failed: {0}")]
    Io(#[from] io::Error),
}

/// `pgid` as a `pid_t` when it is a safe group to signal: it fits, is above
/// 1, and is not `caller_pgrp`.
fn safe_target(pgid: u32, caller_pgrp: i32) -> Option<libc::pid_t> {
    let signed = libc::pid_t::try_from(pgid).ok()?;
    (signed > 1 && signed != caller_pgrp).then_some(signed)
}

/// The first identity field on which the sidecar and the state record
/// disagree, as text.
fn first_disagreement(sidecar: &RecordedIdentity, record: &RecordedIdentity) -> Option<String> {
    if sidecar.launch_id != record.launch_id {
        return Some(format!(
            "launch id: sidecar {}, state {}",
            sidecar.launch_id, record.launch_id
        ));
    }
    if sidecar.pgid != record.pgid {
        return Some(format!(
            "pgid: sidecar {}, state {}",
            sidecar.pgid, record.pgid
        ));
    }
    if sidecar.start_time != record.start_time {
        return Some(format!(
            "agent start time: sidecar {}, state {}",
            sidecar.start_time, record.start_time
        ));
    }
    if sidecar.boot_id != record.boot_id {
        return Some(format!(
            "boot id: sidecar {}, state {}",
            sidecar.boot_id, record.boot_id
        ));
    }
    None
}

/// The backstop's decision rule, pure. See the module doc for the order.
pub(crate) fn classify(facts: &BackstopFacts) -> BackstopDecision {
    let recorded = &facts.recorded;
    let Some(pgid) = safe_target(recorded.pgid, facts.caller_pgrp) else {
        return BackstopDecision::RefuseUnsafeTarget;
    };
    let current_boot = match (facts.identity_provable, facts.current_boot_id.as_deref()) {
        (true, Some(boot)) => boot,
        _ => return BackstopDecision::RefuseIdentityUnavailable,
    };
    // A launch that could not read its agent's start time or the boot id
    // records 0 or "" in their place; neither proves the group gone.
    if recorded.start_time == 0 || recorded.boot_id.is_empty() {
        return BackstopDecision::RefuseIdentityUnavailable;
    }
    if let Some(why) = facts
        .state_record
        .as_ref()
        .and_then(|record| first_disagreement(recorded, record))
    {
        return BackstopDecision::RefuseMismatch(why);
    }
    if recorded.boot_id != current_boot {
        return BackstopDecision::PreviousBoot;
    }
    if facts.members.is_empty() {
        return BackstopDecision::EmptyGroup;
    }
    match facts.leader {
        Some(leader) if leader.start_time != recorded.start_time => {
            BackstopDecision::LeaderRecycled
        }
        Some(leader) if leader.pgrp == pgid => BackstopDecision::LeaderPresentSignal,
        _ => BackstopDecision::RefuseLeaderGone(facts.members.clone()),
    }
}

/// The caller's own process group.
fn caller_pgrp() -> i32 {
    // SAFETY: getpgrp has no preconditions and cannot fail.
    unsafe { libc::getpgrp() }
}

/// Facts with nothing read about any process: the shape every platform
/// starts from, and the whole answer off Linux.
fn unread_facts(sidecar: &Sidecar, state_record: Option<&SupervisorRecord>) -> BackstopFacts {
    BackstopFacts {
        recorded: RecordedIdentity::of_sidecar(sidecar),
        state_record: state_record.map(RecordedIdentity::of_record),
        caller_pgrp: caller_pgrp(),
        current_boot_id: None,
        leader: None,
        members: Vec::new(),
        identity_provable: false,
    }
}

/// Gather the facts [`classify`] needs. An unsafe target reads nothing.
#[cfg(target_os = "linux")]
fn gather(sidecar: &Sidecar, state_record: Option<&SupervisorRecord>) -> BackstopFacts {
    let mut facts = unread_facts(sidecar, state_record);
    let Some(pgid) = safe_target(facts.recorded.pgid, facts.caller_pgrp) else {
        return facts;
    };
    let Some(members) = group_members(pgid) else {
        // The process table could not be listed: identity is unprovable.
        return facts;
    };
    facts.members = members;
    facts.leader = own_process_stat(facts.recorded.pgid);
    facts.current_boot_id = super::boot_id();
    facts.identity_provable = true;
    facts
}

/// Off Linux there is no `/proc` start time, group census or boot id
/// (backlog 999.158, 999.159), so nothing is read and identity is
/// unprovable: [`classify`] answers `RefuseIdentityUnavailable` (MAC-2).
#[cfg(not(target_os = "linux"))]
fn gather(sidecar: &Sidecar, state_record: Option<&SupervisorRecord>) -> BackstopFacts {
    unread_facts(sidecar, state_record)
}

/// `/proc/<pid>/stat` for a process this uid owns; `None` for another uid's
/// process or no process.
#[cfg(target_os = "linux")]
fn own_process_stat(pid: u32) -> Option<ProcStat> {
    let owner = std::fs::metadata(format!("/proc/{pid}")).ok()?;
    if std::os::unix::fs::MetadataExt::uid(&owner) != super::protocol::euid() {
        return None;
    }
    crate::agent::process_stat_fields(pid)
}

/// Own-uid, non-zombie processes whose process group is `pgid`, by pid.
/// The owner uid is checked before anything else is read (the
/// `discover_stray_devflow_processes` rule). `None` when `/proc` cannot be
/// listed, which is "cannot tell", never "empty".
pub(crate) fn group_members(pgid: libc::pid_t) -> Option<Vec<GroupMember>> {
    let entries = std::fs::read_dir("/proc").ok()?;
    let my_uid = super::protocol::euid();
    let mut members = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(owner) = std::fs::metadata(entry.path()) else {
            continue;
        };
        if std::os::unix::fs::MetadataExt::uid(&owner) != my_uid {
            continue;
        }
        let Some(stat) = crate::agent::process_stat_fields(pid) else {
            continue;
        };
        if stat.pgrp != pgid || stat.state == 'Z' {
            continue;
        }
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline"))
            .map(|raw| {
                raw.split(|&byte| byte == 0)
                    .filter(|arg| !arg.is_empty())
                    .map(String::from_utf8_lossy)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        members.push(GroupMember {
            pid,
            start_time: stat.start_time,
            state: stat.state,
            session: stat.session,
            cmdline,
        });
    }
    members.sort_by_key(|member| member.pid);
    Some(members)
}

/// Decide what the backstop would do for this launch, without acting: for
/// read-only callers (doctor, the launch guard) that classify a STALE socket.
pub fn assess(sidecar: &Sidecar, state_record: Option<&SupervisorRecord>) -> BackstopDecision {
    classify(&gather(sidecar, state_record))
}

/// Tear down the launch behind a STALE socket.
///
/// Re-probes first and returns [`BackstopError::NotStale`] unless the probe
/// is `Ok(Liveness::Stale)`. Then gathers facts, [`classify`]s them and acts:
/// `PreviousBoot`, `EmptyGroup` and `LeaderRecycled` remove the socket and
/// sidecar and signal nothing; `LeaderPresentSignal` ends the group (SIGTERM,
/// [`crate::agent::TERMINATE_VERIFY_WAIT`], SIGKILL) and then removes them;
/// every `Refuse*` keeps them and signals nothing.
pub fn teardown_stale(
    socket: &SocketPath,
    sidecar: &Sidecar,
    state_record: Option<&SupervisorRecord>,
) -> Result<BackstopOutcome, BackstopError> {
    match super::probe(socket.as_path()) {
        Ok(Liveness::Stale) => {}
        Ok(Liveness::Gone) => return Err(BackstopError::NotStale("it is gone".to_string())),
        Ok(Liveness::Alive(_)) => {
            return Err(BackstopError::NotStale(
                "its supervisor answered a ping".to_string(),
            ));
        }
        Err(error) => {
            return Err(BackstopError::NotStale(format!(
                "it could not be probed: {error}"
            )));
        }
    }
    let facts = gather(sidecar, state_record);
    match classify(&facts) {
        BackstopDecision::LeaderPresentSignal => kill_group(socket, sidecar, state_record, &facts),
        decision => settle(socket, decision),
    }
}

/// Act on a decision that signals nothing: remove the files for the three
/// decisions that prove the group gone, keep them for anything else.
fn settle(
    socket: &SocketPath,
    decision: BackstopDecision,
) -> Result<BackstopOutcome, BackstopError> {
    match decision {
        BackstopDecision::PreviousBoot
        | BackstopDecision::EmptyGroup
        | BackstopDecision::LeaderRecycled => {
            super::remove_launch_files(socket)?;
            Ok(BackstopOutcome::RemovedNothingSignalled(decision))
        }
        refusal => Ok(BackstopOutcome::Refused(refusal)),
    }
}

/// The result of one validated group signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "no signalling path exists off Linux (MAC-2)")
)]
enum GroupSignal {
    /// Sent to the whole group.
    Sent(SignalPath),
    /// The leader exited, or its pid no longer names the recorded leader of
    /// the group: nothing was sent.
    LeaderGone,
}

/// End a validated group: SIGTERM, wait for it to empty, then SIGKILL.
fn kill_group(
    socket: &SocketPath,
    sidecar: &Sidecar,
    state_record: Option<&SupervisorRecord>,
    facts: &BackstopFacts,
) -> Result<BackstopOutcome, BackstopError> {
    let recorded = &facts.recorded;
    let members = facts.members.clone();
    let pgid = safe_target(recorded.pgid, facts.caller_pgrp).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "unsafe process group target")
    })?;
    let via = match signal_validated_group(recorded.pgid, recorded.start_time, libc::SIGTERM)? {
        GroupSignal::Sent(via) => via,
        GroupSignal::LeaderGone => {
            // The leader changed between gathering and opening its pidfd;
            // nothing was sent. Decide again on fresh facts.
            let fresh = gather(sidecar, state_record);
            return match classify(&fresh) {
                BackstopDecision::LeaderPresentSignal => Err(BackstopError::Io(io::Error::other(
                    "the group leader changed while it was being validated; nothing was signalled",
                ))),
                decision => settle(socket, decision),
            };
        }
    };
    if group_empties_within(pgid, crate::agent::TERMINATE_VERIFY_WAIT) {
        super::remove_launch_files(socket)?;
        return Ok(BackstopOutcome::Killed { via, members });
    }
    if signal_validated_group(recorded.pgid, recorded.start_time, libc::SIGKILL)?
        == GroupSignal::LeaderGone
    {
        // The leader died during the grace period and members outlived the
        // SIGTERM. Without a leader nothing proves they are the agent's, so
        // decide again: an empty group is done, anything else is kept.
        return match classify(&gather(sidecar, state_record)) {
            BackstopDecision::EmptyGroup => {
                super::remove_launch_files(socket)?;
                Ok(BackstopOutcome::Killed { via, members })
            }
            decision => Ok(BackstopOutcome::Refused(decision)),
        };
    }
    if group_empties_within(pgid, KILL_VERIFY_WAIT) {
        super::remove_launch_files(socket)?;
        return Ok(BackstopOutcome::Killed { via, members });
    }
    Err(BackstopError::Io(io::Error::other(format!(
        "process group {pgid} still has members after SIGKILL; the socket and sidecar were kept"
    ))))
}

/// Poll the census until no live member of `pgid` remains, up to `wait`.
/// An unreadable process table never counts as empty.
fn group_empties_within(pgid: libc::pid_t, wait: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if group_members(pgid).is_some_and(|members| members.is_empty()) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(crate::agent::TERMINATE_VERIFY_POLL);
    }
}

/// Whether `/proc/<pgid>` is still the recorded leader (same start time)
/// and still leads group `pgid`.
#[cfg(target_os = "linux")]
fn is_original_leader(pgid: u32, start_time: u64) -> bool {
    own_process_stat(pgid).is_some_and(|stat| {
        stat.start_time == start_time && i64::from(stat.pgrp) == i64::from(pgid)
    })
}

/// Signal the group whose leader is `pgid` with the recorded `start_time`.
///
/// Opens a pidfd on the leader, which pins one process, then re-reads its
/// start time and group: if they still match, the fd names the original
/// leader (a recycled pid would show a later start time). Then
/// `pidfd_send_signal(fd, signal, NULL, PIDFD_SIGNAL_PROCESS_GROUP)`. On
/// `EINVAL` (a kernel before 6.9, RESEARCH A3) it re-validates and uses
/// `killpg` ([`SignalPath::ValidatedKillpg`]). `ESRCH` is
/// [`GroupSignal::LeaderGone`].
#[cfg(target_os = "linux")]
fn signal_validated_group(
    pgid: u32,
    start_time: u64,
    signal: libc::c_int,
) -> io::Result<GroupSignal> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let pid = safe_target(pgid, caller_pgrp()).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "unsafe process group target")
    })?;
    // SAFETY: pidfd_open(pid, 0) takes plain integers and returns a new fd
    // or -1 with errno set.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0 as libc::c_uint) };
    if raw < 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(GroupSignal::LeaderGone),
            _ => Err(error),
        };
    }
    let raw = libc::c_int::try_from(raw)
        .map_err(|_| io::Error::other("pidfd_open returned an out-of-range fd"))?;
    // SAFETY: `raw` is a pidfd this call just opened and nothing else owns.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    if !is_original_leader(pgid, start_time) {
        return Ok(GroupSignal::LeaderGone);
    }
    // SAFETY: a live pidfd, a valid signal, a null siginfo (the kernel fills
    // in a SI_USER one) and a flag the kernel validates.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            signal,
            std::ptr::null::<libc::siginfo_t>(),
            libc::PIDFD_SIGNAL_PROCESS_GROUP,
        )
    };
    if rc == 0 {
        return Ok(GroupSignal::Sent(SignalPath::PidfdGroup));
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(GroupSignal::LeaderGone),
        Some(libc::EINVAL) => validated_killpg(&pidfd, pid, pgid, start_time, signal),
        _ => Err(error),
    }
}

/// The `EINVAL` fallback: confirm through the pidfd that the pinned leader
/// is alive, re-read its start time and group, then `killpg`. The window
/// between the check and `killpg` is the documented TOCTOU this path keeps
/// (T-50-24); while the leader is alive its pid cannot name another group.
#[cfg(target_os = "linux")]
fn validated_killpg(
    pidfd: &std::os::fd::OwnedFd,
    pid: libc::pid_t,
    pgid: u32,
    start_time: u64,
    signal: libc::c_int,
) -> io::Result<GroupSignal> {
    use std::os::fd::AsRawFd;

    // SAFETY: signal 0 with no flags only checks that the pinned process is
    // alive and signallable.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            0 as libc::c_int,
            std::ptr::null::<libc::siginfo_t>(),
            0 as libc::c_uint,
        )
    };
    if rc != 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(GroupSignal::LeaderGone),
            _ => Err(error),
        };
    }
    if !is_original_leader(pgid, start_time) {
        return Ok(GroupSignal::LeaderGone);
    }
    // SAFETY: `pid` passed `safe_target` (> 1, not our own group), so
    // killpg reaches neither our group nor every process.
    if unsafe { libc::killpg(pid, signal) } != 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(GroupSignal::LeaderGone),
            _ => Err(error),
        };
    }
    Ok(GroupSignal::Sent(SignalPath::ValidatedKillpg))
}

/// Off Linux there is no pidfd and no way to prove the leader's identity, so
/// there is no signalling path: this sends nothing (MAC-2).
#[cfg(not(target_os = "linux"))]
fn signal_validated_group(
    _pgid: u32,
    _start_time: u64,
    _signal: libc::c_int,
) -> io::Result<GroupSignal> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "group signalling needs Linux pidfds; nothing was signalled",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase_id::PhaseId;
    use crate::supervisor::{new_launch_id, protocol, publish, write_sidecar};
    use std::path::Path;
    #[cfg(target_os = "linux")]
    use std::process::{Child, Command, Stdio};
    #[cfg(target_os = "linux")]
    use std::time::{Duration, Instant};

    const ROOT: &str = "/project/root";
    /// A group whose leader stays: it waits on its two sleeping members.
    #[cfg(target_os = "linux")]
    const LEADER_WAITS: &str = "sleep 30 & sleep 30 & wait";
    /// A group whose leader forks two sleeping members and exits at once.
    #[cfg(target_os = "linux")]
    const LEADER_EXITS: &str = "sleep 30 & sleep 30 & exit 0";
    /// How long a fixture may take to reach the state a test needs.
    #[cfg(target_os = "linux")]
    const SETTLE: Duration = Duration::from_secs(5);
    #[cfg(target_os = "linux")]
    const POLL: Duration = Duration::from_millis(20);

    /// A short cache dir under `/tmp`, so socket paths stay far below
    /// `sun_path` however long `$TMPDIR` is.
    fn short_cache() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("dfs")
            .tempdir_in("/tmp")
            .expect("create a short cache dir under /tmp")
    }

    fn socket_for(cache: &Path, launch_id: &str) -> SocketPath {
        SocketPath::for_launch(cache, Path::new(ROOT), PhaseId::new(50), launch_id)
            .expect("socket path fits sun_path")
    }

    /// A STALE socket: published, then its listener dropped.
    fn stale_socket(cache: &Path) -> (SocketPath, String) {
        let launch_id = new_launch_id();
        let socket = socket_for(cache, &launch_id);
        drop(publish(&socket).expect("publish the socket"));
        assert!(
            matches!(super::super::probe(socket.as_path()), Ok(Liveness::Stale)),
            "the fixture socket must probe STALE"
        );
        (socket, launch_id)
    }

    fn sidecar_for(launch_id: &str, pgid: u32, start_time: u64, boot_id: &str) -> Sidecar {
        Sidecar {
            launch_id: launch_id.to_string(),
            project_root: Path::new(ROOT).to_path_buf(),
            phase: PhaseId::new(50),
            stage: Some("code".to_string()),
            agent: "claude".to_string(),
            launch: "capture".to_string(),
            agent_pid: pgid,
            agent_pgid: pgid,
            agent_start_time: start_time,
            boot_id: boot_id.to_string(),
            supervisor_pid: std::process::id(),
            supervisor_start_time: 0,
            created_at: super::super::rfc3339_from_unix(1_700_000_000),
        }
    }

    /// A STALE socket whose sidecar records `pgid`, `start_time` and
    /// `boot_id`.
    fn stale_launch(
        cache: &Path,
        pgid: u32,
        start_time: u64,
        boot_id: &str,
    ) -> (SocketPath, Sidecar) {
        let (socket, launch_id) = stale_socket(cache);
        let sidecar = sidecar_for(&launch_id, pgid, start_time, boot_id);
        write_sidecar(&socket, &sidecar).expect("write the sidecar");
        (socket, sidecar)
    }

    fn files_exist(socket: &SocketPath) -> (bool, bool) {
        (socket.as_path().exists(), socket.sidecar_path().exists())
    }

    #[cfg(target_os = "linux")]
    /// Whether the process `(pid, start_time)` is alive (not a zombie).
    fn alive(pid: u32, start_time: u64) -> bool {
        crate::agent::process_stat_fields(pid)
            .is_some_and(|stat| stat.start_time == start_time && stat.state != 'Z')
    }

    #[cfg(target_os = "linux")]
    /// Live processes in `pgid`, read independently of the code under test.
    fn live_in_group(pgid: u32) -> Vec<(u32, u64)> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir("/proc").expect("list /proc").flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(stat) = crate::agent::process_stat_fields(pid) else {
                continue;
            };
            if i64::from(stat.pgrp) == i64::from(pgid) && stat.state != 'Z' {
                found.push((pid, stat.start_time));
            }
        }
        found.sort_unstable();
        found
    }

    #[cfg(target_os = "linux")]
    fn execed_sleep(pid: u32) -> bool {
        std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|raw| raw.starts_with(b"sleep\0"))
    }

    #[cfg(target_os = "linux")]
    /// A process group this test spawned. Bound before any assertion; its
    /// drop SIGKILLs the group while the leader is still this test's
    /// unreaped child (so the pgid cannot have been reused), then each
    /// recorded member by pid only if its start time still matches, then
    /// reaps the leader. It signals nothing the test did not create.
    struct TestGroup {
        leader: Child,
        leader_reaped: bool,
        pgid: u32,
        leader_start: u64,
        members: Vec<(u32, u64)>,
    }

    #[cfg(target_os = "linux")]
    impl TestGroup {
        fn spawn(script: &str) -> Self {
            use std::os::unix::process::CommandExt;
            let leader = Command::new("sh")
                .args(["-c", script])
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn the test group leader");
            let pgid = leader.id();
            let mut group = Self {
                leader,
                leader_reaped: false,
                pgid,
                leader_start: 0,
                members: Vec::new(),
            };
            group.leader_start =
                crate::agent::process_start_time(pgid).expect("read the leader's start time");
            group
        }

        /// Wait until the group has `sleepers` sleeping members (plus the
        /// leader while it is alive), recording each as it appears.
        fn settle(&mut self, sleepers: usize) {
            let deadline = Instant::now() + SETTLE;
            loop {
                let live = live_in_group(self.pgid);
                for member in &live {
                    if !self.members.contains(member) {
                        self.members.push(*member);
                    }
                }
                let sleeping = live.iter().filter(|(pid, _)| execed_sleep(*pid)).count();
                if sleeping == sleepers {
                    self.members = live;
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "group {} never reached {sleepers} sleeping members: {live:?}",
                    self.pgid
                );
                std::thread::sleep(POLL);
            }
        }

        /// Reap the leader (a script that exits on its own) and stop
        /// counting it as a member.
        fn reap_leader(&mut self) {
            self.leader.wait().expect("reap the leader");
            self.leader_reaped = true;
            self.members.retain(|(pid, _)| *pid != self.pgid);
        }

        fn all_alive(&self) -> bool {
            self.members.iter().all(|&(pid, start)| alive(pid, start))
        }

        fn wait_all_gone(&self) {
            let deadline = Instant::now() + SETTLE;
            while self.members.iter().any(|&(pid, start)| alive(pid, start)) {
                assert!(
                    Instant::now() < deadline,
                    "group {} members outlived the kill",
                    self.pgid
                );
                std::thread::sleep(POLL);
            }
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for TestGroup {
        fn drop(&mut self) {
            if !self.leader_reaped
                && let Ok(pgid) = libc::pid_t::try_from(self.pgid)
                && pgid > 1
            {
                // SAFETY: the leader is this test's unreaped child, so its
                // pid, and the group it names, cannot have been reused.
                unsafe { libc::killpg(pgid, libc::SIGKILL) };
            }
            for &(pid, start) in &self.members {
                if alive(pid, start)
                    && let Ok(signed) = libc::pid_t::try_from(pid)
                {
                    // SAFETY: a positive pid this test spawned, start time
                    // re-checked just above.
                    unsafe { libc::kill(signed, libc::SIGKILL) };
                }
            }
            let _ = self.leader.kill();
            let _ = self.leader.wait();
        }
    }

    #[cfg(target_os = "linux")]
    fn member_ids(members: &[GroupMember]) -> Vec<(u32, u64)> {
        let mut ids: Vec<(u32, u64)> = members
            .iter()
            .map(|member| (member.pid, member.start_time))
            .collect();
        ids.sort_unstable();
        ids
    }

    /// V-18 (D-04, R-4): a STALE socket over a group whose leader is
    /// present with the recorded start time and boot id is torn down
    /// through the leader's pidfd. Control: a recorded start time one tick
    /// off is a recycled leader, and nothing is signalled.
    #[cfg(target_os = "linux")]
    #[test]
    fn backstop_kills_a_validated_group_whose_leader_is_present() {
        let cache = short_cache();
        let boot = super::super::boot_id().expect("read the boot id");
        let mut group = TestGroup::spawn(LEADER_WAITS);
        group.settle(2);
        assert_eq!(group.members.len(), 3, "leader plus two sleeping members");

        // Control: the wrong start time signals nothing.
        let (socket, sidecar) =
            stale_launch(cache.path(), group.pgid, group.leader_start + 1, &boot);
        let outcome = teardown_stale(&socket, &sidecar, None).expect("teardown the control");
        assert_eq!(
            outcome,
            BackstopOutcome::RemovedNothingSignalled(BackstopDecision::LeaderRecycled),
            "a start time one tick off must read as a recycled leader"
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(group.all_alive(), "the control must signal nothing");
        assert_eq!(
            files_exist(&socket),
            (false, false),
            "a recycled leader removes the files"
        );

        // The recorded start time: the group is killed and the files go.
        let (socket, sidecar) = stale_launch(cache.path(), group.pgid, group.leader_start, &boot);
        let outcome = teardown_stale(&socket, &sidecar, None).expect("teardown the group");
        let BackstopOutcome::Killed { via, members } = outcome else {
            panic!("a validated leader-present group must be Killed, got {outcome:?}");
        };
        eprintln!("V-18 signal path: {via:?}");
        assert!(matches!(
            via,
            SignalPath::PidfdGroup | SignalPath::ValidatedKillpg
        ));
        assert_eq!(
            member_ids(&members),
            group.members,
            "Killed reports the group as gathered"
        );
        group.wait_all_gone();
        assert_eq!(
            files_exist(&socket),
            (false, false),
            "a killed group's files are removed"
        );
    }

    /// Answer one ping on `listener` from a thread, as a live supervisor
    /// would.
    fn answer_one_ping(
        listener: std::os::unix::net::UnixListener,
        launch_id: String,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept the probe");
            let _ = protocol::read_request(&stream);
            let reply = protocol::PingReply {
                launch_id,
                phase: "50".to_string(),
                stage: "code".to_string(),
                launch: "capture".to_string(),
                owner: "agent_running".to_string(),
                agent_pid: 4242,
                agent_pgid: 4242,
                agent_start_time: 1,
                boot_id: super::super::boot_id(),
                supervisor_pid: std::process::id(),
            };
            protocol::write_reply(&stream, &protocol::Reply::Ping(reply)).expect("write the reply");
        })
    }

    /// `teardown_stale` acts only on a STALE socket: an Alive one, a Gone
    /// one and a probe error are `NotStale`, with the files untouched.
    /// Control: a STALE socket passes the probe gate and reaches a decision.
    #[test]
    fn teardown_stale_acts_only_on_a_stale_socket() {
        let cache = short_cache();

        let launch_id = new_launch_id();
        let socket = socket_for(cache.path(), &launch_id);
        let listener = publish(&socket).expect("publish the socket");
        let sidecar = sidecar_for(&launch_id, 4242, 1, "boot-test");
        write_sidecar(&socket, &sidecar).expect("write the sidecar");
        let server = answer_one_ping(listener, launch_id);
        let outcome = teardown_stale(&socket, &sidecar, None);
        server.join().expect("ping server thread");
        assert!(
            matches!(outcome, Err(BackstopError::NotStale(_))),
            "an Alive socket is NotStale, got {outcome:?}"
        );
        assert_eq!(
            files_exist(&socket),
            (true, true),
            "an Alive socket keeps its files"
        );

        let launch_id = new_launch_id();
        let not_a_socket = socket_for(cache.path(), &launch_id);
        std::fs::write(not_a_socket.as_path(), b"not a socket").expect("plant a regular file");
        let sidecar = sidecar_for(&launch_id, 4242, 1, "boot-test");
        let outcome = teardown_stale(&not_a_socket, &sidecar, None);
        assert!(
            matches!(outcome, Err(BackstopError::NotStale(_))),
            "a ProbeError is NotStale, got {outcome:?}"
        );
        assert!(
            not_a_socket.as_path().exists(),
            "a probe error removes nothing"
        );

        let gone = socket_for(cache.path(), &new_launch_id());
        assert!(matches!(
            teardown_stale(&gone, &sidecar, None),
            Err(BackstopError::NotStale(_))
        ));

        // Control: STALE passes the gate (pgid 1 then refuses as unsafe).
        let (stale, stale_sidecar) = stale_launch(cache.path(), 1, 1, "boot-test");
        assert_eq!(
            teardown_stale(&stale, &stale_sidecar, None).expect("a STALE socket is acted on"),
            BackstopOutcome::Refused(BackstopDecision::RefuseUnsafeTarget)
        );
        assert_eq!(
            files_exist(&stale),
            (true, true),
            "a refusal keeps the files"
        );
    }

    /// A short pause after a call that must signal nothing, so a wrongly
    /// sent signal has time to land before the survivors are checked.
    #[cfg(target_os = "linux")]
    const SIGNAL_LANDS: Duration = Duration::from_millis(200);

    /// Tear down a fixture group whose leader is present with the recorded
    /// identity, and require it gone: the opposite-direction control of
    /// V-19, V-20 and V-21.
    #[cfg(target_os = "linux")]
    fn assert_validated_group_is_killed(cache: &Path, boot: &str) {
        let mut group = TestGroup::spawn(LEADER_WAITS);
        group.settle(2);
        let (socket, sidecar) = stale_launch(cache, group.pgid, group.leader_start, boot);
        let outcome = teardown_stale(&socket, &sidecar, None).expect("teardown the control");
        assert!(
            matches!(outcome, BackstopOutcome::Killed { .. }),
            "the control group must be killed, got {outcome:?}"
        );
        group.wait_all_gone();
        assert_eq!(files_exist(&socket), (false, false));
    }

    /// V-19 (R-4, RESEARCH Finding 1): the leader is gone and its members
    /// live, so nothing proves they are the agent's. They are reported,
    /// never signalled, and the files are kept. Control: the same shape with
    /// the leader alive is killed.
    #[cfg(target_os = "linux")]
    #[test]
    fn backstop_refuses_and_reports_a_group_whose_leader_is_gone() {
        let cache = short_cache();
        let boot = super::super::boot_id().expect("read the boot id");
        let mut group = TestGroup::spawn(LEADER_EXITS);
        group.reap_leader();
        group.settle(2);
        assert_eq!(group.members.len(), 2, "two members outlive the leader");

        let (socket, sidecar) = stale_launch(cache.path(), group.pgid, group.leader_start, &boot);
        let outcome = teardown_stale(&socket, &sidecar, None).expect("teardown");
        let BackstopOutcome::Refused(decision) = outcome else {
            panic!("a leader-gone group must be refused, got {outcome:?}");
        };
        let BackstopDecision::RefuseLeaderGone(members) = &decision else {
            panic!("expected RefuseLeaderGone, got {decision:?}");
        };
        assert_eq!(
            member_ids(members),
            group.members,
            "both members are reported"
        );
        assert!(
            members
                .iter()
                .all(|member| member.cmdline.starts_with("sleep 30")),
            "each member is reported with its cmdline: {members:?}"
        );
        std::thread::sleep(SIGNAL_LANDS);
        assert!(group.all_alive(), "a leader-gone group is never signalled");
        assert_eq!(
            files_exist(&socket),
            (true, true),
            "a refusal keeps the files"
        );

        let text = decision.describe(&socket, &sidecar);
        assert!(
            text.contains(&format!(
                "ps -o pid,pgid,sid,lstart,args -p \"$(pgrep -d, -g {})\"",
                group.pgid
            )),
            "{text}"
        );
        assert!(
            text.contains(
                "may be the original agent's children or an unrelated group that reused the id; \
                 nothing was signalled"
            ),
            "{text}"
        );
        for member in members {
            assert!(text.contains(&format!("pid {},", member.pid)), "{text}");
            assert!(
                text.contains(&format!("start time {},", member.start_time)),
                "{text}"
            );
            assert!(
                text.contains(&format!("session {}", member.session)),
                "{text}"
            );
        }

        assert_validated_group_is_killed(cache.path(), &boot);
    }

    /// V-20 (R-4, T-50-22): the recorded pgid names a live, unrelated group
    /// whose leader started at another time. That is a recycled pid, so the
    /// recorded group is gone: nothing is signalled and the files go.
    /// Control: the matching start time is signalled.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_reused_pgid_is_never_signalled() {
        let cache = short_cache();
        let boot = super::super::boot_id().expect("read the boot id");
        let mut unrelated = TestGroup::spawn(LEADER_WAITS);
        unrelated.settle(2);
        assert!(
            unrelated.leader_start > 0,
            "a start time an earlier agent could precede"
        );

        // The recorded agent started one tick before the group now at its pid.
        let recorded_start = unrelated.leader_start - 1;
        let (socket, sidecar) = stale_launch(cache.path(), unrelated.pgid, recorded_start, &boot);
        assert_eq!(
            teardown_stale(&socket, &sidecar, None).expect("teardown"),
            BackstopOutcome::RemovedNothingSignalled(BackstopDecision::LeaderRecycled)
        );
        std::thread::sleep(SIGNAL_LANDS);
        assert!(
            unrelated.all_alive(),
            "the unrelated group must not be signalled"
        );
        assert_eq!(
            files_exist(&socket),
            (false, false),
            "a recycled pgid is GONE"
        );

        assert_validated_group_is_killed(cache.path(), &boot);
    }

    /// V-21 (R-8): a sidecar from another boot over a live, otherwise valid
    /// group is removed and nothing is signalled. Control: the current boot
    /// id is signalled.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_previous_boot_socket_is_removed_and_nothing_is_signalled() {
        let cache = short_cache();
        let boot = super::super::boot_id().expect("read the boot id");
        let previous = "00000000-0000-4000-8000-000000000000";
        assert_ne!(
            boot, previous,
            "the fabricated boot id must differ from this boot's"
        );
        let mut group = TestGroup::spawn(LEADER_WAITS);
        group.settle(2);

        let (socket, sidecar) =
            stale_launch(cache.path(), group.pgid, group.leader_start, previous);
        assert_eq!(
            teardown_stale(&socket, &sidecar, None).expect("teardown"),
            BackstopOutcome::RemovedNothingSignalled(BackstopDecision::PreviousBoot)
        );
        std::thread::sleep(SIGNAL_LANDS);
        assert!(group.all_alive(), "a previous-boot socket signals nothing");
        assert_eq!(
            files_exist(&socket),
            (false, false),
            "a previous-boot socket is removed"
        );

        assert_validated_group_is_killed(cache.path(), &boot);
    }

    /// T-50-25: pgid 0, 1, the caller's own group and a value that does not
    /// fit `pid_t` are refused before anything is read or signalled. Were
    /// the caller's group signalled, this test would die. Control: an
    /// ordinary pgid is not refused as unsafe.
    #[test]
    fn the_backstop_never_targets_pgid_one_or_its_own_group() {
        let cache = short_cache();
        // SAFETY: getpgrp has no preconditions and cannot fail.
        let own = u32::try_from(unsafe { libc::getpgrp() }).expect("a positive pgrp");
        for pgid in [0, 1, own, u32::MAX] {
            let (socket, sidecar) = stale_launch(cache.path(), pgid, 1, "boot-test");
            assert_eq!(
                assess(&sidecar, None),
                BackstopDecision::RefuseUnsafeTarget,
                "pgid {pgid}"
            );
            assert_eq!(
                teardown_stale(&socket, &sidecar, None).expect("teardown"),
                BackstopOutcome::Refused(BackstopDecision::RefuseUnsafeTarget),
                "pgid {pgid}"
            );
            assert_eq!(
                files_exist(&socket),
                (true, true),
                "pgid {pgid} keeps its files"
            );
        }
        let ordinary = sidecar_for(
            "ordinary",
            u32::try_from(i32::MAX).expect("fits"),
            1,
            "boot",
        );
        assert_ne!(
            assess(&ordinary, None),
            BackstopDecision::RefuseUnsafeTarget
        );
    }

    /// T-50-25: gathering reads only this uid's processes. Group 1 (init's)
    /// is visible to an independent `/proc` scan but owned by root, so the
    /// census and the leader read find nothing there.
    #[cfg(target_os = "linux")]
    #[test]
    fn gathering_skips_processes_of_another_uid() {
        if super::super::protocol::euid() == 0 {
            eprintln!("skipped: running as root, so no process belongs to another uid");
            return;
        }
        assert!(
            !live_in_group(1).is_empty(),
            "control: an independent scan sees group 1"
        );
        assert_eq!(
            group_members(1),
            Some(Vec::new()),
            "another uid's group reads empty"
        );
        assert_eq!(
            own_process_stat(1),
            None,
            "another uid's leader reads absent"
        );
        let me = std::process::id();
        assert!(
            own_process_stat(me).is_some(),
            "control: this uid's process is read"
        );
    }

    /// MAC-2: the `RefuseIdentityUnavailable` text names the group, the
    /// launch, both files and the manual repair, and no platform command.
    /// Control: the leader-gone text does not claim identity is unprovable.
    #[test]
    fn the_identity_unavailable_refusal_names_the_group_and_the_manual_repair() {
        let launch_id = "aaaa0000aaaa0000";
        let socket = socket_for(Path::new("/tmp/dfs-text-fixture"), launch_id);
        let sidecar = sidecar_for(launch_id, 4242, 99, "boot-test");
        let text = BackstopDecision::RefuseIdentityUnavailable.describe(&socket, &sidecar);
        for needle in [
            "nothing was signalled or removed".to_string(),
            "cannot prove process group 4242".to_string(),
            launch_id.to_string(),
            socket.as_path().display().to_string(),
            socket.sidecar_path().display().to_string(),
            "end that group yourself".to_string(),
        ] {
            assert!(text.contains(&needle), "missing {needle:?} in {text}");
        }
        for command in ["ps ", "kill", "pkill", "launchctl"] {
            assert!(
                !text.contains(command),
                "names a command {command:?}: {text}"
            );
        }

        let member = GroupMember {
            pid: 4243,
            start_time: 100,
            state: 'S',
            session: 4000,
            cmdline: "sleep 30".to_string(),
        };
        let gone = BackstopDecision::RefuseLeaderGone(vec![member]).describe(&socket, &sidecar);
        assert!(!gone.contains("cannot prove process group"), "{gone}");
        assert!(
            gone.contains(r#"ps -o pid,pgid,sid,lstart,args -p "$(pgrep -d, -g 4242)""#),
            "{gone}"
        );
    }

    const TABLE_PGID: u32 = 4242;
    const TABLE_START: u64 = 100;
    const TABLE_BOOT: &str = "boot-now";
    const TABLE_CALLER: i32 = 7;

    fn table_member(pid: u32) -> GroupMember {
        GroupMember {
            pid,
            start_time: TABLE_START,
            state: 'S',
            session: 4000,
            cmdline: "agent".to_string(),
        }
    }

    fn table_identity() -> RecordedIdentity {
        RecordedIdentity {
            launch_id: "launch-a".to_string(),
            pgid: TABLE_PGID,
            start_time: TABLE_START,
            boot_id: TABLE_BOOT.to_string(),
        }
    }

    /// Facts for a validated, leader-present group: the one shape that
    /// signals. Each table row changes one or two facts from here.
    fn signalling_facts() -> BackstopFacts {
        BackstopFacts {
            recorded: table_identity(),
            state_record: Some(table_identity()),
            caller_pgrp: TABLE_CALLER,
            current_boot_id: Some(TABLE_BOOT.to_string()),
            leader: Some(ProcStat {
                state: 'S',
                pgrp: 4242,
                session: 4000,
                start_time: TABLE_START,
            }),
            members: vec![table_member(TABLE_PGID), table_member(TABLE_PGID + 1)],
            identity_provable: true,
        }
    }

    fn with(change: impl FnOnce(&mut BackstopFacts)) -> BackstopFacts {
        let mut facts = signalling_facts();
        change(&mut facts);
        facts
    }

    fn set_pgid(facts: &mut BackstopFacts, pgid: u32) {
        facts.recorded.pgid = pgid;
        if let Some(record) = facts.state_record.as_mut() {
            record.pgid = pgid;
        }
        if let Some(leader) = facts.leader.as_mut() {
            leader.pgrp = i32::try_from(pgid).unwrap_or(-1);
        }
    }

    fn mismatched(field: &str) -> BackstopFacts {
        with(|facts| {
            let record = facts.state_record.as_mut().expect("a state record");
            match field {
                "launch id" => record.launch_id = "launch-b".to_string(),
                "pgid" => record.pgid = TABLE_PGID + 9,
                "agent start time" => record.start_time = TABLE_START + 9,
                "boot id" => record.boot_id = "boot-other".to_string(),
                other => panic!("no field {other}"),
            }
        })
    }

    fn previous_boot() -> BackstopFacts {
        with(|facts| facts.current_boot_id = Some("boot-later".to_string()))
    }

    fn empty_group() -> BackstopFacts {
        with(|facts| facts.members.clear())
    }

    fn recycled_leader() -> BackstopFacts {
        with(|facts| {
            facts.leader = Some(ProcStat {
                state: 'S',
                pgrp: 4242,
                session: 5000,
                start_time: TABLE_START + 1,
            });
        })
    }

    fn leader_gone() -> BackstopFacts {
        with(|facts| facts.leader = None)
    }

    fn gone_members() -> BackstopDecision {
        BackstopDecision::RefuseLeaderGone(signalling_facts().members)
    }

    /// The decision rule, case by case and order by order, over synthetic
    /// facts. Runs on every target: `classify` has no `cfg`.
    #[test]
    fn classify_covers_every_rule_in_order() {
        use BackstopDecision as D;
        let own_group = u32::try_from(TABLE_CALLER).expect("positive");
        let largest_pid = u32::try_from(i32::MAX).expect("fits");
        let mut rows: Vec<(&str, BackstopFacts, BackstopDecision)> = vec![
            // Step 4 (the only signalling row), and its boundaries.
            (
                "validated leader present",
                signalling_facts(),
                D::LeaderPresentSignal,
            ),
            (
                "no state record",
                with(|f| f.state_record = None),
                D::LeaderPresentSignal,
            ),
            (
                "pgid 2 is a safe target",
                with(|f| set_pgid(f, 2)),
                D::LeaderPresentSignal,
            ),
            (
                "i32::MAX fits pid_t",
                with(|f| set_pgid(f, largest_pid)),
                D::LeaderPresentSignal,
            ),
            (
                "a zombie leader still leads",
                with(|f| {
                    f.leader.as_mut().expect("leader").state = 'Z';
                }),
                D::LeaderPresentSignal,
            ),
            // The unsafe-target guard, first of all.
            ("pgid 0", with(|f| set_pgid(f, 0)), D::RefuseUnsafeTarget),
            ("pgid 1", with(|f| set_pgid(f, 1)), D::RefuseUnsafeTarget),
            (
                "the caller's own group",
                with(|f| set_pgid(f, own_group)),
                D::RefuseUnsafeTarget,
            ),
            (
                "pgid beyond pid_t",
                with(|f| set_pgid(f, largest_pid + 1)),
                D::RefuseUnsafeTarget,
            ),
            (
                "u32::MAX",
                with(|f| set_pgid(f, u32::MAX)),
                D::RefuseUnsafeTarget,
            ),
            (
                "unsafe wins over unprovable identity",
                with(|f| {
                    set_pgid(f, 1);
                    f.identity_provable = false;
                }),
                D::RefuseUnsafeTarget,
            ),
            (
                "unsafe wins over a mismatch",
                {
                    let mut f = mismatched("launch id");
                    set_pgid(&mut f, 0);
                    f
                },
                D::RefuseUnsafeTarget,
            ),
            // An unreadable boot id is the identity refusal, never PreviousBoot.
            (
                "no current boot id",
                with(|f| f.current_boot_id = None),
                D::RefuseIdentityUnavailable,
            ),
            // A launch that could not read its agent's start time or the boot
            // id recorded 0 or "" in their place. Those placeholders prove
            // nothing: never PreviousBoot, EmptyGroup or LeaderRecycled.
            (
                "recorded start time 0",
                with(|f| {
                    f.recorded.start_time = 0;
                    f.state_record.as_mut().expect("record").start_time = 0;
                }),
                D::RefuseIdentityUnavailable,
            ),
            (
                "recorded start time 0, no state record",
                with(|f| {
                    f.recorded.start_time = 0;
                    f.state_record = None;
                }),
                D::RefuseIdentityUnavailable,
            ),
            (
                "recorded start time 0 over an empty group",
                with(|f| {
                    f.recorded.start_time = 0;
                    f.state_record.as_mut().expect("record").start_time = 0;
                    f.members.clear();
                }),
                D::RefuseIdentityUnavailable,
            ),
            (
                "recorded boot id empty",
                with(|f| {
                    f.recorded.boot_id = String::new();
                    f.state_record.as_mut().expect("record").boot_id = String::new();
                }),
                D::RefuseIdentityUnavailable,
            ),
            (
                "recorded boot id empty, no state record",
                with(|f| {
                    f.recorded.boot_id = String::new();
                    f.state_record = None;
                }),
                D::RefuseIdentityUnavailable,
            ),
            // Step 3: previous boot, and it wins over an empty group.
            ("previous boot", previous_boot(), D::PreviousBoot),
            (
                "previous boot wins over an empty group",
                with(|f| {
                    f.current_boot_id = Some("boot-later".to_string());
                    f.members.clear();
                }),
                D::PreviousBoot,
            ),
            (
                "previous boot wins over a gone leader",
                with(|f| {
                    f.current_boot_id = Some("boot-later".to_string());
                    f.leader = None;
                }),
                D::PreviousBoot,
            ),
            // Step 4: an empty group, and it wins over the leader checks.
            ("empty group", empty_group(), D::EmptyGroup),
            (
                "empty group wins over a recycled leader",
                with(|f| {
                    f.members.clear();
                    f.leader.as_mut().expect("leader").start_time = TABLE_START + 1;
                }),
                D::EmptyGroup,
            ),
            (
                "empty group with no leader",
                with(|f| {
                    f.members.clear();
                    f.leader = None;
                }),
                D::EmptyGroup,
            ),
            // Step 6: recycled leader, whatever its group.
            ("recycled leader", recycled_leader(), D::LeaderRecycled),
            (
                "recycled leader in another group",
                with(|f| {
                    let leader = f.leader.as_mut().expect("leader");
                    leader.start_time = TABLE_START - 1;
                    leader.pgrp = 9999;
                }),
                D::LeaderRecycled,
            ),
            // Step 7: leader gone, or the original leader left the group.
            ("leader gone, members live", leader_gone(), gone_members()),
            (
                "original leader left the group",
                with(|f| {
                    f.leader.as_mut().expect("leader").pgrp = 9999;
                }),
                gone_members(),
            ),
        ];
        // Step 2: each mismatched field refuses, and a mismatch wins over
        // everything after it.
        for field in ["launch id", "pgid", "agent start time", "boot id"] {
            rows.push((
                "mismatch",
                mismatched(field),
                D::RefuseMismatch(String::new()),
            ));
        }
        for (name, later) in [
            ("mismatch wins over previous boot", previous_boot()),
            ("mismatch wins over an empty group", empty_group()),
            ("mismatch wins over a recycled leader", recycled_leader()),
            ("mismatch wins over a gone leader", leader_gone()),
        ] {
            let mut facts = later;
            facts.state_record.as_mut().expect("record").launch_id = "launch-b".to_string();
            rows.push((name, facts, D::RefuseMismatch(String::new())));
        }
        // MAC-2: unprovable identity refuses before every step after the
        // unsafe-target guard, whatever those steps would have answered.
        for (name, base) in [
            ("unprovable over a mismatch", mismatched("pgid")),
            ("unprovable over a previous boot", previous_boot()),
            ("unprovable over an empty group", empty_group()),
            ("unprovable over a present leader", signalling_facts()),
            ("unprovable over a recycled leader", recycled_leader()),
            ("unprovable over a gone leader", leader_gone()),
        ] {
            let mut facts = base;
            facts.identity_provable = false;
            rows.push((name, facts, D::RefuseIdentityUnavailable));
        }

        for (name, facts, expected) in &rows {
            let actual = classify(facts);
            match (expected, &actual) {
                // A mismatch is matched by variant; its text is checked below.
                (D::RefuseMismatch(_), D::RefuseMismatch(_)) => {}
                _ => assert_eq!(&actual, expected, "row {name:?}"),
            }
        }

        // The mismatch text names the field that differs, and both values.
        for (field, sidecar_value, state_value) in [
            ("launch id", "launch-a", "launch-b"),
            ("pgid", "4242", "4251"),
            ("agent start time", "100", "109"),
            ("boot id", "boot-now", "boot-other"),
        ] {
            let D::RefuseMismatch(why) = classify(&mismatched(field)) else {
                panic!("{field} must be a mismatch");
            };
            assert_eq!(
                why,
                format!("{field}: sidecar {sidecar_value}, state {state_value}")
            );
        }
    }

    /// The kernel-capability check (RESEARCH A3): on a kernel with
    /// `PIDFD_SIGNAL_PROCESS_GROUP` the backstop must use it. The kernel is
    /// probed independently of the code under test (signal 0 with the flag
    /// on a pidfd of a group this test owns); a kernel without it is
    /// reported on stderr, never passed silently.
    #[cfg(target_os = "linux")]
    #[test]
    fn pidfd_group_signal_is_used_when_the_kernel_supports_it() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        let cache = short_cache();
        let boot = super::super::boot_id().expect("read the boot id");
        let mut group = TestGroup::spawn(LEADER_WAITS);
        group.settle(2);

        let leader = libc::pid_t::try_from(group.pgid).expect("fits pid_t");
        // SAFETY: pidfd_open on this test's own unreaped child.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, leader, 0 as libc::c_uint) };
        assert!(raw >= 0, "pidfd_open: {}", io::Error::last_os_error());
        // SAFETY: a pidfd this call just opened and nothing else owns.
        let pidfd = unsafe { OwnedFd::from_raw_fd(libc::c_int::try_from(raw).expect("an fd")) };
        // SAFETY: signal 0 only checks permission; the flag is what is probed.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd.as_raw_fd(),
                0 as libc::c_int,
                std::ptr::null::<libc::siginfo_t>(),
                libc::PIDFD_SIGNAL_PROCESS_GROUP,
            )
        };
        if rc != 0 {
            let error = io::Error::last_os_error();
            assert_eq!(
                error.raw_os_error(),
                Some(libc::EINVAL),
                "probe failed: {error}"
            );
            eprintln!("skipped: kernel lacks PIDFD_SIGNAL_PROCESS_GROUP");
            return;
        }

        let (socket, sidecar) = stale_launch(cache.path(), group.pgid, group.leader_start, &boot);
        let outcome = teardown_stale(&socket, &sidecar, None).expect("teardown");
        let BackstopOutcome::Killed { via, .. } = outcome else {
            panic!("a validated group must be Killed, got {outcome:?}");
        };
        assert_eq!(
            via,
            SignalPath::PidfdGroup,
            "a supporting kernel uses the pidfd group signal"
        );
        group.wait_all_gone();
    }

    /// T-50-24: after opening the leader's pidfd, the helper re-reads the
    /// leader and sends nothing unless it is still the recorded process and
    /// still leads the group its pid names. Signal 0 is used throughout, so
    /// a wrongly "sent" signal shows in the return value and harms nothing.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_group_signal_revalidates_the_leader_after_opening_its_pidfd() {
        struct Reaped(Child);
        impl Drop for Reaped {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        let mut group = TestGroup::spawn(LEADER_WAITS);
        group.settle(2);
        assert_eq!(
            signal_validated_group(group.pgid, group.leader_start + 1, 0).expect("signal"),
            GroupSignal::LeaderGone,
            "a different start time sends nothing"
        );

        // A process that does not lead a group of its own pid: it stays in
        // this test's group.
        let outsider = Reaped(
            Command::new("sleep")
                .arg("30")
                .stdin(Stdio::null())
                .spawn()
                .expect("spawn a process in this test's group"),
        );
        let outsider_pid = outsider.0.id();
        let outsider_start =
            crate::agent::process_start_time(outsider_pid).expect("read its start time");
        assert_eq!(
            signal_validated_group(outsider_pid, outsider_start, 0).expect("signal"),
            GroupSignal::LeaderGone,
            "a process that does not lead the group its pid names sends nothing"
        );

        // Control: the recorded leader of its own group is signalled.
        assert!(matches!(
            signal_validated_group(group.pgid, group.leader_start, 0).expect("signal"),
            GroupSignal::Sent(_)
        ));
        assert!(group.all_alive(), "signal 0 ends nothing");
    }
}

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
//! 1. Where process identity cannot be proved (off Linux, or a boot id or
//!    process table that cannot be read), the answer is
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

#[cfg(test)]
use super::Liveness;
use super::{Sidecar, SocketPath};
use crate::state::SupervisorRecord;
use std::io;

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

/// What the backstop decides for a STALE socket. See the module doc for the
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackstopDecision {
    /// The recorded pgid does not fit `pid_t`, is 0 or 1, or is the
    /// caller's own process group. Nothing is signalled or removed.
    RefuseUnsafeTarget,
    /// This platform gives DevFlow no process start time, group census or
    /// boot id (off Linux; on Linux, an unreadable boot id or process table),
    /// so the group's identity cannot be proved. Callers treat it like the
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

/// Decide what the backstop would do for this launch, without acting: for
/// read-only callers (doctor, the launch guard) that classify a STALE socket.
pub fn assess(_sidecar: &Sidecar, _state_record: Option<&SupervisorRecord>) -> BackstopDecision {
    // RED stub (50-09 Task 1): the decision rule lands in GREEN.
    BackstopDecision::RefuseIdentityUnavailable
}

/// Tear down the launch behind a STALE socket.
pub fn teardown_stale(
    _socket: &SocketPath,
    _sidecar: &Sidecar,
    _state_record: Option<&SupervisorRecord>,
) -> Result<BackstopOutcome, BackstopError> {
    // RED stub (50-09 Task 1): probing, the decision and acting land in GREEN.
    Ok(BackstopOutcome::Refused(
        BackstopDecision::RefuseIdentityUnavailable,
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
    #[cfg(target_os = "linux")]
    /// How long a fixture may take to reach the state a test needs.
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
}

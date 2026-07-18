//! Agent process helpers.
//!
//! All agents run in non-interactive mode (`claude -p`, `codex exec`) under a
//! detached monitor that owns the process and its capture files (see
//! [`crate::monitor`]). The old synchronous launch/capture path
//! (`launch_agent` + `capture_agent_output`) was removed in 14b once
//! `sequentagent` moved behind monitor-owned execution — the monitor is now
//! the single way an agent process is spawned.

/// Check whether a process with the given PID is still running.
///
/// The PID typically comes from parsing an on-disk file, so hostile or
/// corrupted values must be rejected, not reinterpreted: `kill(0, sig)`
/// signals the caller's own process group (a "0" PID file would read as
/// permanently alive), and a value above `i32::MAX` would wrap negative
/// through an `as libc::pid_t` cast — `kill(-1, 0)` probes every process
/// the caller may signal and virtually always succeeds.
pub fn agent_running(pid: u32) -> bool {
    // kill(pid, 0) is the standard POSIX way to check process existence
    // without sending an actual signal.
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    pid > 0 && unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_running_detects_self() {
        // The current process is, by definition, running.
        assert!(agent_running(std::process::id()));
    }

    #[test]
    fn agent_running_false_for_dead_pid() {
        // A PID near the top of the range is essentially never live.
        assert!(!agent_running(0x7FFF_FFFE));
    }

    #[test]
    fn agent_running_rejects_corrupt_pid_values() {
        // "0" from a truncated PID file: kill(0, 0) would signal our own
        // process group and report alive.
        assert!(!agent_running(0));
        // Above i32::MAX: `as libc::pid_t` would wrap to -1, and
        // kill(-1, 0) probes every signalable process — almost always "alive".
        assert!(!agent_running(u32::MAX));
        assert!(!agent_running(i32::MAX as u32 + 1));
    }
}

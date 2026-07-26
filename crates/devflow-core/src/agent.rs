//! Agent process helpers.
//!
//! All agents run in non-interactive mode (`claude -p`, `codex exec`) under a
//! detached monitor that owns the process and its capture files (see
//! [`crate::monitor`]). The old synchronous launch/capture path
//! (`launch_agent` + `capture_agent_output`) was removed in 14b — the monitor
//! is now the single way an agent process is spawned.

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

/// Send SIGTERM to `pid` — the crate's one process-termination call, used by
/// `devflow stop`'s signalling fallback (23c). Applies exactly the same
/// guards [`agent_running`] applies, for reasons that are catastrophic here
/// rather than merely wrong: signalling pid `0` would target the caller's
/// own process group (`kill(0, sig)` reaches every process in the group,
/// including this one), and a value above `i32::MAX` would wrap negative
/// through the `as libc::pid_t` cast — `kill(-1, sig)` sends the signal to
/// every process the caller may signal. Returns whether the signal was
/// delivered.
pub fn terminate(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    pid > 0 && unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
}

/// Best-effort, Linux-only identity check for `devflow stop`'s signalling
/// fallback (T-23-52, PID reuse in a stale lock file): does
/// `/proc/<pid>/cmdline` name a devflow process? Reads the NUL-separated
/// argv and reports whether any argument's file-name component starts with
/// `devflow`. Returns `false` when the file cannot be read (process exited
/// between the liveness check and this call, non-Linux, permission denied)
/// — the fail-closed direction. A `false` return means "identity could not
/// be confirmed," and callers must treat that as "do not signal," never as
/// "signal anyway."
pub fn looks_like_devflow_process(pid: u32) -> bool {
    let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    cmdline
        .split(|&byte| byte == 0)
        .filter(|arg| !arg.is_empty())
        .any(|arg| {
            let arg = String::from_utf8_lossy(arg);
            std::path::Path::new(arg.as_ref())
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("devflow"))
        })
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

    #[test]
    fn terminate_rejects_pid_zero() {
        // Would target the caller's own process group — never send it.
        assert!(!terminate(0));
    }

    #[test]
    fn terminate_rejects_pid_above_i32_max() {
        // Would wrap to -1 through the pid_t cast — kill(-1, SIGTERM) hits
        // every process the caller may signal.
        assert!(!terminate(u32::MAX));
        assert!(!terminate(i32::MAX as u32 + 1));
    }

    #[test]
    fn terminate_signals_a_live_child_and_it_exits() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        assert!(terminate(pid), "terminate must report the signal delivered");

        let status = child.wait().expect("wait on the terminated child");
        assert!(
            !status.success(),
            "a SIGTERM'd child must not report a successful exit, got {status:?}"
        );
    }

    #[test]
    fn looks_like_devflow_process_is_true_for_the_current_process() {
        // Cargo names this crate's test binary from its crate name
        // (`devflow-core` → `devflow_core-<hash>` under target/deps
        // naming) — a reliable positive fixture with no need to spawn a
        // real devflow binary.
        assert!(looks_like_devflow_process(std::process::id()));
    }

    /// Render `/proc/<pid>/cmdline` readably for failure diagnostics: the
    /// NUL-separated argv joined with ` | `, or a marker when it cannot be
    /// read. Test-only; never used in a decision, only in a message.
    fn debug_cmdline(pid: u32) -> String {
        match std::fs::read(format!("/proc/{pid}/cmdline")) {
            Ok(raw) if raw.iter().all(|&byte| byte == 0) => "<empty>".to_string(),
            Ok(raw) => raw
                .split(|&byte| byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect::<Vec<_>>()
                .join(" | "),
            Err(err) => format!("<unreadable: {err}>"),
        }
    }

    #[test]
    fn looks_like_devflow_process_is_false_for_a_non_devflow_process() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        // This assertion has failed intermittently in CI (first seen
        // 2026-07-26, on commits touching no Rust source) as a FALSE
        // POSITIVE: the predicate reported a plain `sleep` as a devflow
        // process. It does not reproduce locally — 40/40 under CPU load —
        // and a fork/exec cmdline-inheritance theory was disproved at
        // 0/3000. A bare `assert!` throws away the one artifact that could
        // name the mechanism, so bracket the predicate with reads of the
        // same /proc file and report everything on failure.
        //
        // Reading the verdict here rather than inside `assert!` keeps the
        // diagnostics adjacent to the call they describe, and lets the
        // child be reaped before any panic unwinds (a panic inside the
        // assert would otherwise leak the `sleep` for its full duration —
        // this repo already has an orphan-hygiene problem, see 999.44/46).
        let cmdline_before = debug_cmdline(pid);
        let verdict = looks_like_devflow_process(pid);
        let cmdline_after = debug_cmdline(pid);
        let exe = std::fs::read_link(format!("/proc/{pid}/exe"))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|err| format!("<unreadable: {err}>"));
        let self_pid = std::process::id();
        let self_cmdline = debug_cmdline(self_pid);

        let _ = child.kill();
        let _ = child.wait();

        assert!(
            !verdict,
            "looks_like_devflow_process({pid}) returned true for a spawned `sleep`.\n\
             \x20 child cmdline before: {cmdline_before}\n\
             \x20 child cmdline after:  {cmdline_after}\n\
             \x20 child /proc/{pid}/exe: {exe}\n\
             \x20 test process:         pid {self_pid} cmdline {self_cmdline}\n\
             If both child cmdlines name a devflow binary, the pid is not the \
             `sleep` we spawned (recycled/misattributed pid). If they differ from \
             each other, the cmdline changed under the predicate. If they name \
             `sleep`, the predicate's matching logic is at fault."
        );
    }

    #[test]
    fn looks_like_devflow_process_is_false_when_proc_cannot_be_read() {
        // A pid guaranteed not to exist: the fail-closed default must be
        // false, never true, when identity cannot be confirmed at all.
        assert!(!looks_like_devflow_process(0x7FFF_FFFE));
    }
}

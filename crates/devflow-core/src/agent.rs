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
///
/// **Zombies are NOT running.** `kill(pid, 0)` succeeds for a process that
/// has exited but not yet been reaped: the pid stays allocated until its
/// parent calls `wait`, so the bare POSIX check reports a dead agent as
/// alive. That is not academic here — when a monitor dies before its agent,
/// the agent reparents to PID 1, and inside a container PID 1 is whatever
/// the image runs (cargo, a shell, the test harness), none of which reap
/// orphans the way an init system does. The zombie then persists for the
/// life of the container and every liveness check keeps answering "yes".
///
/// That is exactly the "monitor over-durability" class Phase 23 exists to
/// close: an operator, `gate sweep`, or `stop` asking "is this phase still
/// running?" would be told yes forever about a process that is already dead.
/// Observed directly in CI (`sigterm_to_monitor_also_kills_the_agent`), where
/// both monitor and agent were `State=Z` and the agent had reparented to
/// PPid=1 while the bare check still reported them alive.
///
/// Reading `/proc` is Linux-only; where it cannot be read, this falls back to
/// the `kill(0)` answer rather than inventing one.
pub fn agent_running(pid: u32) -> bool {
    // kill(pid, 0) is the standard POSIX way to check process existence
    // without sending an actual signal.
    let Ok(signed) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if signed <= 0 || unsafe { libc::kill(signed, 0) } != 0 {
        return false;
    }
    !is_zombie(pid)
}

/// Whether `pid` has exited but not yet been reaped — `State: Z` in
/// `/proc/<pid>/status`.
///
/// Returns `false` when the status file cannot be read: an unreadable
/// `/proc` entry means "cannot tell", and the caller has already established
/// via `kill(0)` that the pid exists, so claiming zombie-hood here would
/// invent information.
fn is_zombie(pid: u32) -> bool {
    let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
        return false;
    };
    status
        .lines()
        .find(|line| line.starts_with("State:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .is_some_and(|state| state == "Z")
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

/// Default bounded wait for [`terminate_and_verify`]'s escalation to
/// `SIGKILL`. A few seconds is long enough for a well-behaved process to
/// shut down after `SIGTERM`, short enough that an unattended loop is not
/// stalled indefinitely waiting on one that won't.
pub const TERMINATE_VERIFY_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Default poll interval while [`terminate_and_verify`] waits for its target
/// to exit. Callers that need a different ceiling or granularity should pass
/// their own `wait`/`poll` rather than inventing new constants.
pub const TERMINATE_VERIFY_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Terminate `pid`, escalating to `SIGKILL` if it has not exited within
/// `wait`, and return a **verified fact** about whether it is dead —
/// never an assumption.
///
/// Sequence: send one `SIGTERM` via [`terminate`]. If that fails to signal
/// the process at all (already gone, or the pid is invalid), report whether
/// it is already dead — "could not signal it" and "already dead" are the
/// same outcome from the caller's perspective. Otherwise poll
/// [`agent_running`] at `poll` intervals until `wait` elapses, returning
/// `true` the moment it reports dead. On expiry, escalate with `SIGKILL` and
/// return the (inverted) liveness check one final time.
///
/// **`SIGKILL` escalation is not optional here.** 999.44's 2026-07-27
/// measurement found 15 of 15 orphaned monitor wrappers surviving `SIGTERM`
/// — the wrapper installs `trap cleanup TERM INT`, which evidently does not
/// fire, most likely because the shell is blocked in `wait` on a child it
/// can never reap. Per 25-RESEARCH.md Open Question 2 and 999.47's own
/// recorded lesson, this function deliberately does **not** depend on
/// explaining that mechanism — the escalation works regardless of *why*
/// `SIGTERM` alone fails. That is accepted unexplained behaviour this code
/// defends against, not a root cause this function resolves.
///
/// A non-positive `pid`, or one that does not fit `libc::pid_t`, returns
/// `false` immediately and signals nothing — the same wraparound/group-
/// signal hazard [`agent_running`] and [`terminate`] already guard against.
pub fn terminate_and_verify(
    pid: u32,
    wait: std::time::Duration,
    poll: std::time::Duration,
) -> bool {
    let Ok(signed) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if signed <= 0 {
        return false;
    }

    if !terminate(pid) {
        // Could not be signalled at all — already dead counts as success.
        return !agent_running(pid);
    }

    let term_deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < term_deadline {
        if !agent_running(pid) {
            return true;
        }
        std::thread::sleep(poll);
    }

    // TERM alone did not clear it within the bounded wait — escalate.
    unsafe {
        libc::kill(signed, libc::SIGKILL);
    }
    // SIGKILL is uncatchable but not synchronous: the kernel needs a moment
    // to actually deliver it, so poll again rather than checking exactly
    // once — a single immediate check can race the kernel and report a
    // just-killed process as still alive.
    let kill_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < kill_deadline {
        if !agent_running(pid) {
            return true;
        }
        std::thread::sleep(poll);
    }
    !agent_running(pid)
}

/// A process's start time — field 22 of `/proc/<pid>/stat`, in clock ticks
/// since boot.
///
/// This is the missing half of process identity. A PID alone is ambiguous:
/// the kernel reuses it after the process exits, so a stale record naming
/// pid 1234 may now refer to something entirely unrelated. `(pid, starttime)`
/// is unique for the life of a boot, because a recycled pid necessarily
/// starts later than the one it replaced.
///
/// Record this alongside a pid whenever the pid will be acted on later —
/// signalled, killed, reported as a holder — and require BOTH to match
/// before acting. That is the only check immune to the two ways `/proc`
/// lies about identity:
///
/// * **PID reuse.** cmdline/exe describe whoever holds the pid *now*.
/// * **The fork/exec window (999.47).** Between `Command::spawn()` returning
///   a pid and the child completing `execve`, the child is a copy of its
///   parent: `/proc/<pid>/cmdline` reports the PARENT's argv and
///   `/proc/<pid>/exe` the parent's binary. A devflow process's freshly
///   spawned child therefore looks exactly like devflow itself. Confirmed
///   directly in CI, where container overlayfs widens that window enough to
///   hit routinely.
///
/// `comm` is inherited across `fork` too, so it is no better. There is no
/// field that distinguishes a mid-`execve` child from its parent — they are
/// genuinely the same image at that instant. Identity must be *recorded*,
/// never inferred.
///
/// **Granularity caveat, measured not assumed.** The value is in clock ticks
/// since boot — `USER_HZ`, conventionally 100, so 10ms. Two processes created
/// within the same tick report the *same* start time; this was observed
/// directly while testing, where a test binary and a child it spawned
/// microseconds later were indistinguishable by this field alone.
///
/// That does not weaken the pid-recycling guarantee this exists for: for a
/// pid to be recycled the kernel must exhaust and wrap the pid space, which
/// takes vastly longer than 10ms. It does mean this must not be used to
/// distinguish a parent from a child it just spawned — for that, compare
/// pids, which differ by construction.
///
/// Returns `None` when the stat file cannot be read or parsed — the
/// fail-closed direction, meaning "identity could not be confirmed."
pub fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 2 (comm) is parenthesised and may itself contain spaces or
    // parentheses, so split after the FINAL ')' rather than tokenising the
    // whole line. After that point, field 3 is index 0, so field 22 is 19.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse::<u64>().ok()
}

/// Whether `pid` is the same process instance that recorded `expected_start`.
///
/// The identity check `devflow stop` and friends should use. See
/// [`process_start_time`] for why a pid alone — or any `/proc`-derived
/// description of it — cannot answer this.
pub fn is_same_process(pid: u32, expected_start: u64) -> bool {
    process_start_time(pid) == Some(expected_start)
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
///
/// **UNSOUND ON ITS OWN — see 999.47.** This returns `true` for any freshly
/// `fork`ed child of a devflow process that has not yet completed `execve`,
/// because such a child transiently carries its parent's cmdline. It is
/// retained only as a secondary, advisory signal; prefer
/// [`is_same_process`] with a recorded start time, which cannot be fooled
/// this way. Never let this function alone authorise a signal.
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

    /// A reaped-pending child is dead, not running. `kill(pid, 0)` succeeds
    /// on a zombie because the pid is still allocated, so the bare POSIX
    /// check reports it alive — which is how a container with no reaping
    /// init can make a dead agent look permanently live.
    #[test]
    fn agent_running_is_false_for_an_unreaped_zombie() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();

        // Wait for it to become a zombie WITHOUT reaping it: poll /proc for
        // State: Z rather than calling wait(), which would clear the pid.
        let mut became_zombie = false;
        for _ in 0..200 {
            if super::is_zombie(pid) {
                became_zombie = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(became_zombie, "child never became an unreaped zombie");

        // The bare POSIX check still says "alive" — that is the trap.
        assert_eq!(
            unsafe { libc::kill(pid as libc::pid_t, 0) },
            0,
            "kill(pid, 0) is expected to still succeed on a zombie — if this \
             fails the test is no longer exercising the case it was written for"
        );
        assert!(
            !agent_running(pid),
            "a zombie has exited and must not be reported as running"
        );

        let _ = child.wait();
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
    fn terminate_and_verify_rejects_pid_zero_and_out_of_range_without_signalling() {
        // Same wraparound/group-signal hazard `terminate` and `agent_running`
        // already guard against — never send anything for these values.
        assert!(!terminate_and_verify(
            0,
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(10)
        ));
        assert!(!terminate_and_verify(
            u32::MAX,
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(10)
        ));
        assert!(!terminate_and_verify(
            i32::MAX as u32 + 1,
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(10)
        ));
    }

    #[test]
    fn terminate_and_verify_returns_true_immediately_for_a_dead_pid() {
        // A pid essentially never live: `terminate` fails to signal it at
        // all, so the function must report "already dead" without waiting
        // out the ceiling.
        let start = std::time::Instant::now();
        let cleared = terminate_and_verify(
            0x7FFF_FFFE,
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(20),
        );
        let elapsed = start.elapsed();

        assert!(
            cleared,
            "a pid that cannot be signalled at all must count as already cleared"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "must not wait out the full ceiling when the signal itself fails, took {elapsed:?}"
        );
    }

    #[test]
    fn terminate_and_verify_clears_a_normal_child_before_the_wait_elapses() {
        // `sleep` has no TERM handler installed, so the default disposition
        // (terminate) applies — it must exit promptly, well before
        // escalation would ever be needed.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        let start = std::time::Instant::now();
        let cleared = terminate_and_verify(
            pid,
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(20),
        );
        let elapsed = start.elapsed();

        assert!(cleared, "a TERM-honouring child must be cleared");
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "clearing an ordinary child must complete well before the 5s wait \
             ceiling, took {elapsed:?} (SIGKILL escalation should not have \
             been needed)"
        );

        let _ = child.wait();
    }

    #[test]
    fn terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child() {
        // D-17's regression test: a child that installs an empty TERM
        // handler and then sleeps must still be cleared, via the SIGKILL
        // escalation, within the bounded wait.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap '' TERM; sleep 30")
            .spawn()
            .expect("spawn TERM-ignoring child");
        let pid = child.id();

        // Give the shell a moment to install its trap before signalling.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let cleared = terminate_and_verify(
            pid,
            std::time::Duration::from_millis(500),
            std::time::Duration::from_millis(20),
        );

        assert!(
            cleared,
            "a TERM-ignoring child must still be cleared via SIGKILL escalation"
        );
        assert!(
            !agent_running(pid),
            "child must be verified dead after escalation, not merely assumed"
        );

        let _ = child.wait();
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

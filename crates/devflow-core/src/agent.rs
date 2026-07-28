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

/// Resolve the kernel's clock tick rate (`USER_HZ`) via
/// `sysconf(_SC_CLK_TCK)`, rather than assuming the "conventionally 100"
/// value [`process_start_time`]'s own doc comment names as a convention,
/// not a guarantee — a wrong divisor would silently scale
/// [`process_age`]'s result. Returns `None` when the kernel reports a
/// non-positive value, so the caller can fail closed instead of dividing
/// by (or trusting) a nonsensical rate.
fn clock_ticks_per_second() -> Option<i64> {
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (ticks > 0).then_some(ticks)
}

/// How long `pid` has been running: `/proc/uptime` (seconds since boot)
/// minus [`process_start_time`] (ticks since boot, converted to seconds
/// via the kernel's own reported tick rate — never a hardcoded divisor).
///
/// This is the primitive `reap_stray_candidates`' age floor (25-12/999.47,
/// the production half of the defect class) is built on: a process
/// [`discover_stray_devflow_processes`] catches mid-`execve` is genuinely
/// the same process with genuinely the same recorded start time as its
/// parent — [`is_same_process`] cannot distinguish the two — but its age
/// is sub-millisecond, while a genuine orphan is minutes to hours old.
/// `reap_stray_candidates` (`devflow-cli::commands`) is the one caller
/// that consumes this to make a signalling decision; see its own doc
/// comment for how the separation is used.
///
/// Returns `None` when age could not be determined at all: `/proc/uptime`
/// is unreadable or unparseable, the tick rate cannot be resolved, or
/// [`process_start_time`] itself returns `None`. Callers MUST treat `None`
/// as "do not act" — never as "old enough" — matching the fail-closed
/// posture [`process_start_time`] documents for identity.
///
/// A negative difference — the two clocks read microseconds apart — is
/// clamped to zero rather than treated as an error: it is a rounding
/// artefact, not evidence of anything, and zero keeps the fail-closed
/// direction (an age of zero sits below any floor).
pub fn process_age(pid: u32) -> Option<std::time::Duration> {
    let uptime_raw = std::fs::read_to_string("/proc/uptime").ok()?;
    let uptime_secs: f64 = uptime_raw.split_whitespace().next()?.parse().ok()?;

    let ticks_per_sec = clock_ticks_per_second()?;
    let start_ticks = process_start_time(pid)?;
    let start_secs = start_ticks as f64 / ticks_per_sec as f64;

    let age_secs = (uptime_secs - start_secs).max(0.0);
    Some(std::time::Duration::from_secs_f64(age_secs))
}

/// The age floor `reap_stray_candidates` (`devflow-cli::commands`)
/// refuses to signal a candidate below. **An age floor, not a
/// classifier**: it refuses every candidate younger than this in BOTH
/// directions — a mid-`execve` false positive and a genuine stray younger
/// than the floor are both refused, because [`process_age`] cannot tell
/// the two apart and does not try to.
///
/// The two populations this separates are six orders of magnitude apart:
/// a `fork()`->`execve()` window, sub-millisecond even under the 2-core
/// pinned CI load measured in `25-CI-OBSERVATION.md`, versus a genuine
/// orphan of a *previous* run — a monitor wrapper lives for the duration
/// of an agent stage, minutes to hours. No value between those two
/// populations is contentious.
///
/// A candidate refused for youth is not lost: `gate_sweep`
/// (`devflow-cli::commands`) re-runs discovery after its reaping pass and
/// reports anything still discoverable, so a false refusal is deferred
/// cleanup — cleared on the next invocation — never a missed one.
pub const STRAY_MIN_AGE: std::time::Duration = std::time::Duration::from_secs(2);

/// Which structural layer of a DevFlow-spawned process tree
/// [`discover_stray_devflow_processes`] matched a candidate against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrayLayer {
    /// The monitor wrapper shell spawned by `monitor::spawn_monitor` — the
    /// `sh -c <script>` process that owns the agent and, on exit, runs
    /// `devflow advance`.
    MonitorWrapper,
    /// The trailing `devflow advance` invocation the wrapper's script runs
    /// as its last command once the agent exits.
    AdvanceChild,
}

/// A process discovered by [`discover_stray_devflow_processes`]: its pid,
/// the start time recorded at discovery time, and which layer matched it.
///
/// The recorded `start_time` is what lets a later caller re-confirm this is
/// still the same process — via [`is_same_process`] — immediately before
/// acting on it, closing the check-then-act window between discovery and
/// signalling (999.47's "Related TOCTOU").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrayProcess {
    /// The discovered process's pid.
    pub pid: u32,
    /// The pid's recorded start time (`/proc/<pid>/stat` field 22), captured
    /// at discovery time for later identity re-confirmation.
    pub start_time: u64,
    /// Which structural matcher identified this process.
    pub layer: StrayLayer,
}

/// The monitor wrapper's trap-installation line, copied byte-for-byte from
/// `monitor::spawn_monitor_inner`'s literal script text (see
/// `crates/devflow-core/src/monitor.rs`) — not paraphrased or reduced to a
/// single word, so a reader can grep both files for this exact string to
/// confirm they still agree. If the wrapper script's text ever changes, this
/// constant must change with it in the same commit.
const MONITOR_WRAPPER_MARKER: &str = "trap cleanup TERM INT";

/// The devflow CLI binary's name — `crates/devflow-cli/Cargo.toml`'s
/// `[package].name`, with no `[[bin]]` override, so cargo names the built
/// binary after the package. Matched against argv[0]'s basename for Layer 2.
const DEVFLOW_BINARY_NAME: &str = "devflow";

/// The advance subcommand's literal name (`Command::Advance` in
/// `devflow-cli/src/main.rs`), matched against argv[1] for Layer 2.
const ADVANCE_SUBCOMMAND: &str = "advance";

/// Census both of DevFlow's orphan-prone process layers directly from the OS
/// process table — the only remaining discovery surface once a project root
/// has been deleted off disk, taking every registry entry, lock file and
/// state file with it (999.44).
///
/// This is a pure, read-only survey: it never signals a process. Deciding
/// whether to act on a result, and re-confirming identity immediately
/// beforehand, is the caller's job.
///
/// Two structural matchers, deliberately narrower than the predicate
/// 999.47 disproved (which matched ANY argv element whose basename began
/// with the binary name, so `sleep /tmp/devflow-scratch/x` was a false
/// positive):
///
/// * **Layer 1 — the monitor wrapper.** `argv[0]` is `sh`, `argv[1]` is
///   `-c`, and `argv[2]` (the script) contains [`MONITOR_WRAPPER_MARKER`]
///   verbatim.
/// * **Layer 2 — the trailing advance child.** `argv[0]`'s basename equals
///   [`DEVFLOW_BINARY_NAME`] AND `argv[1]` equals [`ADVANCE_SUBCOMMAND`].
///
/// Neither matcher scans all argv elements or matches a prefix; both check
/// specific, named positions only.
///
/// Two hard constraints on the census, both load-bearing:
///
/// 1. **No parentage filter.** These orphans reparent to the user's
///    per-user service manager, not to the init process — a parent-identity
///    filter was directly measured against this repository (23-FINDINGS.md)
///    to report zero orphans while 14 genuinely existed. This function does
///    not consult parentage at all.
/// 2. **Never return a process owned by another user.** Each candidate's
///    owning uid is compared against the caller's effective uid, and
///    anything that does not match is skipped — the concrete hazard is a
///    caller later signalling a stranger's process on a shared machine.
/// 3. **Structural, not exec-confirmed (25-12/999.47, the production half
///    of the defect class).** This is a **structural** match over
///    `/proc/<pid>/cmdline` alone. During a process's own
///    `fork()`->`execve()` window — [`process_start_time`]'s doc comment
///    is this codebase's authoritative statement of the mechanism —
///    `/proc/<pid>/cmdline` transiently reports its PARENT's argv, not its
///    own. A transient child of the monitor wrapper, or of `devflow
///    advance`, therefore matches Layer 1 or Layer 2 respectively while
///    being neither. This census does not — and deliberately should not —
///    filter that case out: a census that guessed at exec status would
///    also drop genuine strays, and it has no reliable way to distinguish
///    the two (see [`process_age`]'s own doc comment for why). It is the
///    **caller's** obligation not to act on an unqualified census result —
///    and that obligation has TWO parts, bounding two DIFFERENT hazards,
///    neither of which discharges the other (CR-01, 999.44/DEN-68):
///
///    - **The age floor** ([`process_age`]/[`STRAY_MIN_AGE`]) bounds the
///      fork/exec cmdline-inheritance window above — "is this argv match
///      even real yet."
///    - **Registry-reachability** (`commands::unreachable_stray_candidates`,
///      `devflow-cli::commands`) bounds a different question — "is this
///      process alive AND OWNED by a live registry entry, lock file, or
///      state file" — which the age floor says nothing about: a monitor
///      wrapper minutes old sails straight past it while still being a
///      live, registered process, not a stray.
///
///    `reap_stray_candidates` (`devflow-cli::commands`) is the one caller
///    with a destructive consequence, and it discharges the first with
///    [`process_age`] and [`STRAY_MIN_AGE`]; `unreachable_stray_candidates`
///    (`devflow-cli::commands`), interposed before either `doctor` or
///    `reap_stray_candidates` acts, discharges the second — never this
///    function.
///
/// Every read failure is tolerated silently (a pid that vanishes between
/// the directory listing and the cmdline/stat read is normal churn, not an
/// error), and an unreadable `/proc` returns an empty list rather than
/// propagating an error.
pub fn discover_stray_devflow_processes() -> Vec<StrayProcess> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };

    let my_uid = unsafe { libc::geteuid() };
    let mut found = Vec::new();

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };

        // Shared-machine safety: skip anything not owned by us before
        // reading anything else about it.
        let Ok(owner_metadata) = std::fs::metadata(entry.path()) else {
            continue;
        };
        if std::os::unix::fs::MetadataExt::uid(&owner_metadata) != my_uid {
            continue;
        }

        let Ok(raw_cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let args: Vec<String> = raw_cmdline
            .split(|&byte| byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();

        let Some(layer) = classify_stray_layer(&args) else {
            continue;
        };

        // The candidate's identity, recorded now so a caller can
        // re-confirm it with `is_same_process` right before acting.
        let Some(start_time) = process_start_time(pid) else {
            continue; // exited between the directory listing and here
        };

        found.push(StrayProcess {
            pid,
            start_time,
            layer,
        });
    }

    found
}

/// Basename of an argv element, matching the idiom already used by
/// [`looks_like_devflow_process`].
///
/// `pub(crate)` (widened from private, 25-11/999.47) so
/// [`crate::test_support::wait_for_exec_visibility`] can reuse this exact
/// basename idiom instead of growing a second copy of it. Crate-internal
/// visibility only — not public API of this crate's normal build.
pub(crate) fn argv_basename(arg: &str) -> Option<&str> {
    std::path::Path::new(arg)
        .file_name()
        .and_then(|n| n.to_str())
}

/// Which layer (if any) an argv list structurally matches. See
/// [`discover_stray_devflow_processes`] for the two matchers' exact shape.
fn classify_stray_layer(args: &[String]) -> Option<StrayLayer> {
    let is_monitor_wrapper = args.len() >= 3
        && argv_basename(&args[0]) == Some("sh")
        && args[1] == "-c"
        && args[2].contains(MONITOR_WRAPPER_MARKER);
    if is_monitor_wrapper {
        return Some(StrayLayer::MonitorWrapper);
    }

    let is_advance_child = args
        .first()
        .and_then(|argv0| argv_basename(argv0))
        .is_some_and(|name| name == DEVFLOW_BINARY_NAME)
        && args.get(1).map(String::as_str) == Some(ADVANCE_SUBCOMMAND);
    if is_advance_child {
        return Some(StrayLayer::AdvanceChild);
    }

    None
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
#[deprecated(note = "unsound alone (999.47) -- use is_same_process with a recorded start time")]
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
    fn discover_stray_devflow_processes_finds_a_monitor_wrapper() {
        // A shell invoked with `-c` whose script argument contains the
        // wrapper's literal marker, verbatim from monitor.rs.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();

        // 999.47: cross the exec-visibility barrier before reading the
        // cmdline-derived census, or this test races the fixture's own
        // fork()->execve() window (25-11).
        assert!(
            crate::test_support::wait_for_exec_visibility(
                pid,
                "sh",
                crate::test_support::EXEC_VISIBILITY_WAIT,
                crate::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );

        let found = discover_stray_devflow_processes();
        let candidate = found.iter().find(|p| p.pid == pid);

        let candidate = candidate.expect("monitor wrapper fixture must be discovered");
        assert_eq!(candidate.layer, StrayLayer::MonitorWrapper);
        assert!(
            is_same_process(pid, candidate.start_time),
            "the recorded start time must re-confirm identity while the process is alive"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape() {
        // The exact false-positive class 999.47 measured: a process that
        // merely mentions a devflow-looking path as an argument, not
        // structurally shaped like either layer.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 30")
            .arg("/tmp/devflow-scratch/looks-like-devflow")
            .spawn()
            .expect("spawn 999.47-shaped fixture");
        let pid = child.id();

        // 999.47/25-11: without this barrier, the assertion below passes
        // during the fork()->execve() window for a reason unrelated to what
        // it claims to test — the census is reading the CALLER's (this test
        // binary's) argv, which matches neither Layer 1 nor Layer 2, so the
        // NOT-FIND assertion is vacuously true regardless of whether the
        // fixture's own shape is correctly rejected. Crossing the barrier
        // first makes the NOT-FIND assertion mean what it claims.
        assert!(
            crate::test_support::wait_for_exec_visibility(
                pid,
                "sh",
                crate::test_support::EXEC_VISIBILITY_WAIT,
                crate::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );

        let found = discover_stray_devflow_processes();

        let _ = child.kill();
        let _ = child.wait();

        assert!(
            !found.iter().any(|p| p.pid == pid),
            "a process merely mentioning a devflow-looking path must not be discovered"
        );
    }

    #[test]
    fn discover_stray_devflow_processes_rejects_devflow_named_argv0_with_wrong_argv1() {
        // argv[0]'s basename matches the binary name, but argv[1] is not
        // the advance subcommand — Layer 2 requires BOTH positions.
        let mut child = std::process::Command::new("sleep");
        std::os::unix::process::CommandExt::arg0(&mut child, "devflow");
        let mut child = child
            .arg("30")
            .spawn()
            .expect("spawn devflow-argv0 fixture");
        let pid = child.id();

        // 999.47/25-11: same reasoning as the false-positive-shape test
        // above — without this barrier, the NOT-FIND assertion below passes
        // vacuously during the fork()->execve() window (the caller's own
        // argv matches neither layer), which says nothing about whether
        // THIS fixture's argv[0]==devflow/argv[1]!=advance shape is
        // correctly rejected once its own exec has actually landed.
        assert!(
            crate::test_support::wait_for_exec_visibility(
                pid,
                "devflow",
                crate::test_support::EXEC_VISIBILITY_WAIT,
                crate::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );

        let found = discover_stray_devflow_processes();

        let _ = child.kill();
        let _ = child.wait();

        assert!(
            !found.iter().any(|p| p.pid == pid),
            "argv[0]==devflow with argv[1] != advance must not be discovered as Layer 2"
        );
    }

    #[test]
    fn discover_stray_devflow_processes_excludes_an_unrelated_process() {
        // This test binary's own process is neither the wrapper's `sh -c`
        // shape nor a `devflow advance` invocation, so it must never be
        // discovered — proving the census does not match by default and
        // completes a full /proc scan without error.
        let self_pid = std::process::id();
        let found = discover_stray_devflow_processes();
        assert!(
            !found.iter().any(|p| p.pid == self_pid),
            "the test binary itself must never be discovered as a stray process"
        );
    }

    #[test]
    #[allow(deprecated)] // D-13: retained, zero-cost-of-call regression coverage for a deprecated-but-not-removed public fn
    fn looks_like_devflow_process_is_true_for_the_current_process() {
        // Cargo names this crate's test binary from its crate name
        // (`devflow-core` → `devflow_core-<hash>` under target/deps
        // naming) — a reliable positive fixture with no need to spawn a
        // real devflow binary.
        assert!(looks_like_devflow_process(std::process::id()));
    }

    #[test]
    fn looks_like_devflow_process_is_false_for_a_non_devflow_process() {
        // Retargeted (D-13): this test used to assert the deprecated
        // `looks_like_devflow_process` predicate against a freshly spawned
        // `sleep`, which raced that child's `execve` and failed
        // intermittently in CI (999.47, "MECHANISM CONFIRMED 2026-07-26").
        // It now asserts the `(pid, starttime)` identity guard production
        // actually uses — `is_same_process` — which needs no `spawn()` and
        // therefore has no `execve` to race. This is what fixes the flake,
        // by construction, not by making the old test rarer.
        let self_pid = std::process::id();
        let real_start = process_start_time(self_pid)
            .expect("must be able to read this process's own recorded start time");

        assert!(
            is_same_process(self_pid, real_start),
            "the current process must match its own recorded start time"
        );

        let perturbed_start = real_start.wrapping_add(1);
        assert!(
            !is_same_process(self_pid, perturbed_start),
            "a deliberately wrong start time must not be treated as a match"
        );
    }

    #[test]
    #[allow(deprecated)] // D-13: retained, zero-cost-of-call regression coverage for a deprecated-but-not-removed public fn
    fn looks_like_devflow_process_is_false_when_proc_cannot_be_read() {
        // A pid guaranteed not to exist: the fail-closed default must be
        // false, never true, when identity cannot be confirmed at all.
        assert!(!looks_like_devflow_process(0x7FFF_FFFE));
    }

    // 25-12/999.47 (production half): `process_age`'s own test group. Test
    // names all begin `process_age_` so the count-based acceptance
    // criterion (`cargo test agent::tests::process_age` -> `3 passed`)
    // resolves unambiguously.

    #[test]
    fn process_age_returns_some_for_the_current_process() {
        // Measured directly (not assumed): `/proc/uptime` and this
        // process's own recorded start time share the same ~10ms USER_HZ
        // granularity `process_start_time`'s doc comment already caveats
        // — a process asked for its own age within one tick of its start
        // (plausible for a freshly launched, fast-starting test binary)
        // genuinely reads `Duration::ZERO`, reproduced deterministically
        // running this test in isolation. Sleep past one tick first so
        // the assertion below tests "age advances," not "the OS finished
        // its first tick before this line ran."
        std::thread::sleep(std::time::Duration::from_millis(20));
        let age = process_age(std::process::id()).expect("this process's own age must resolve");
        assert!(
            age > std::time::Duration::ZERO,
            "a running process must report nonzero age once at least one tick has elapsed"
        );
        assert!(
            age < std::time::Duration::from_secs(3600),
            "the test binary has not been running for an hour"
        );
    }

    #[test]
    fn process_age_returns_none_for_a_dead_pid() {
        // Same guaranteed-not-to-exist pid the fail-closed tests above use.
        assert_eq!(process_age(0x7FFF_FFFE), None);
    }

    #[test]
    fn process_age_is_below_the_floor_for_a_fresh_child_and_grows_monotonically_for_self() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn fixture");
        let pid = child.id();

        let child_age = process_age(pid).expect("a freshly spawned child's age must resolve");
        assert!(
            child_age < STRAY_MIN_AGE,
            "a process spawned microseconds ago must be younger than the floor"
        );

        let _ = child.kill();
        let _ = child.wait();

        let self_pid = std::process::id();
        let first = process_age(self_pid).expect("this process's own age must resolve");
        std::thread::sleep(std::time::Duration::from_millis(50));
        let second =
            process_age(self_pid).expect("this process's own age must resolve after the sleep too");
        assert!(
            second >= first,
            "age must grow monotonically across a sleep, never shrink"
        );
    }
}

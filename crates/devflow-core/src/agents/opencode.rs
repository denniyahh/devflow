//! OpenCode agent driver.
//!
//! Launches `opencode run "<prompt>" --auto --format json` in non-interactive
//! mode. `--auto` is opencode's own label for "auto-approve permissions not
//! explicitly denied (dangerous!)" — the spawned agent executes tool calls
//! with no human in the loop, the same posture as Pi's `--no-approve` /
//! Codex's `-a never` (T-43-01, P-01). This flag must appear ONLY in this
//! launch argv, never in a health or capability probe.
//!
//! `health` (43-02, OPCD-03/D-07) is a fail-closed credential check: it
//! spawns `opencode providers list`, strips its ANSI escape codes, and sums
//! the terminal `N credentials` / `N environment variables` count lines. The
//! subcommand's exit code is always 0 regardless of credential state
//! (verified live), so a zero-credential machine cannot be distinguished
//! from a non-zero exit by status alone — readiness therefore requires BOTH
//! `output.status.success()` AND a positive parsed count, narrowing the
//! false-green window without reintroducing the false-red risk a
//! status-only check would carry. `opencode models` is deliberately
//! never used as the readiness probe — it always lists opencode's own free
//! catalog entries and would false-green a machine with zero configured
//! credentials (D-09).
//!
//! `capabilities` (43-02, OPCD-03/D-10) probes `opencode agent list` for a
//! configured subagent- or all-mode agent. Any probe failure, non-zero exit,
//! or unparseable output fails closed to `subagent_dispatch: false` — this
//! probe must never refuse a launch; only `health` may.

use crate::phase_id::PhaseId;

/// The modular driver for OpenCode (37-02/43-01/43-02): headless
/// `--auto --format json` launch, JSONL completion parsing delegated to
/// `agent_result::parse_opencode_event_result`, legacy prompt rendering, a
/// fail-closed credential health check, and a fail-closed subagent-dispatch
/// capability probe.
pub struct OpenCodeDriver;

impl super::AgentDriver for OpenCodeDriver {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "opencode",
            vec![
                "run".into(),
                prompt.to_string(),
                "--auto".into(),
                "--format".into(),
                "json".into(),
            ],
        )
    }

    /// Relocate the OpenCode JSONL completion parsing under driver ownership:
    /// the function body lives in `agent_result.rs` (where the
    /// result-evaluation path and its fixtures live), and this method is the
    /// driver's contract entry point for it — matching Codex's delegation
    /// pattern (RESEARCH Pattern 2).
    fn parse_completion(&self, output: &str) -> Option<crate::agent_result::AgentResult> {
        crate::agent_result::parse_opencode_event_result(output)
    }

    /// Fail-closed credential check (OPCD-03, D-07/D-08/D-09, T-43-09).
    ///
    /// `opencode providers list` has no JSON output mode (D-08) and exits 0
    /// regardless of credential state (verified live), so exit status alone
    /// cannot decide readiness — but it is still consulted: readiness
    /// requires BOTH `output.status.success()` AND a positive sum of the
    /// ANSI-stripped output's terminal `N credentials` / `N environment
    /// variables` count lines. A spawn failure (e.g. no `opencode` on
    /// `PATH`) also fails closed to `Err`, never a panic.
    ///
    /// The returned `Err` is a fixed message naming only the derived state.
    /// It never interpolates the probe's raw stdout, a provider name, an
    /// `auth.json` path, or an environment-variable name (P-04, T-43-11) —
    /// this repository has three prior instances of exactly this leak class
    /// (999.10, WR-02).
    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        let mut cmd = std::process::Command::new("opencode");
        cmd.args(["providers", "list"]);
        let output = spawn_with_timeout(&mut cmd, PROBE_TIMEOUT)
            .map_err(|e| format!("could not run `opencode providers list`: {e}"))?;
        if output.status.success()
            && opencode_configured_provider_count(&String::from_utf8_lossy(&output.stdout)) > 0
        {
            Ok(())
        } else {
            Err("no OpenCode provider credential configured".to_string())
        }
    }

    /// Fail-closed subagent-dispatch capability probe (OPCD-03, D-10,
    /// T-43-10). This probe can never refuse a launch — only `health` may;
    /// the return type carries that guarantee (`DriverCapabilities`, not a
    /// `Result`).
    fn capabilities(&self) -> super::DriverCapabilities {
        super::DriverCapabilities {
            subagent_dispatch: opencode_subagent_dispatch_available(),
        }
    }
}

/// Bound on how long a `health`/`capabilities` probe subprocess may run
/// before being killed (43-REVIEW.md WR-02). `opencode providers list` and
/// `opencode agent list` are local, non-interactive, small-output commands —
/// 5s is generous headroom, not a tuned budget.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How often [`spawn_with_timeout`] polls the child for exit.
const PROBE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(20);

#[cfg(unix)]
fn kill_probe_group(pid: u32) -> std::io::Result<bool> {
    let pid = pid as libc::pid_t;
    if pid <= 0 {
        return Ok(false);
    }
    if unsafe { libc::kill(-pid, libc::SIGKILL) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(error)
    }
}

/// On Linux, make the process group established by `CommandExt::process_group`
/// an inescapable containment boundary for the probe and every descendant it
/// forks. A probe is only a small, non-interactive query; it has no legitimate
/// reason to daemonize or rearrange process groups. Denying these two syscalls
/// before `exec` means a later group kill reaches every descendant that could
/// retain one of the probe pipes.
///
/// This runs in `pre_exec`, after std has placed the child in its dedicated
/// process group and before the requested program starts. It deliberately
/// allows every other syscall, so it does not turn the probe into a general
/// sandbox. If the kernel rejects the filter, `spawn` fails closed instead of
/// launching an uncontained Linux probe.
#[cfg(target_os = "linux")]
fn install_linux_probe_process_group_guard() -> std::io::Result<()> {
    // The classic BPF program loads the syscall number, rejects `setsid` and
    // `setpgid` with EPERM, and allows every other syscall. `seccomp_data.nr`
    // is the first field in the kernel ABI, so its offset is zero.
    let mut filter = [
        libc::sock_filter {
            code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
            jt: 0,
            jf: 0,
            k: 0,
        },
        libc::sock_filter {
            code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            jt: 0,
            jf: 1,
            k: libc::SYS_setsid as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_RET | libc::BPF_K) as u16,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            jt: 0,
            jf: 1,
            k: libc::SYS_setpgid as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_RET | libc::BPF_K) as u16,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        },
        libc::sock_filter {
            code: (libc::BPF_RET | libc::BPF_K) as u16,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ALLOW,
        },
    ];
    let mut program = libc::sock_fprog {
        len: filter.len() as libc::c_ushort,
        filter: filter.as_mut_ptr(),
    };

    // `prctl` is async-signal-safe and this code performs no allocation or
    // locking, which is required for a `pre_exec` callback in a multithreaded
    // parent process.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &mut program as *mut libc::sock_fprog,
        ) == -1
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

type ProbePipeReceiver = std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>;

/// Begin consuming a probe pipe immediately. This keeps a verbose-but-valid
/// probe from blocking before it exits and, critically, leaves the caller with
/// a deadline even if an unsupported-platform descendant retains the pipe.
fn capture_probe_pipe(pipe: impl std::io::Read + Send + 'static) -> ProbePipeReceiver {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut pipe = pipe;
        let mut bytes = Vec::new();
        let result = pipe.read_to_end(&mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });
    receiver
}

fn collect_probe_pipe(
    receiver: ProbePipeReceiver,
    deadline: std::time::Instant,
    name: &str,
) -> std::io::Result<Vec<u8>> {
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    match receiver.recv_timeout(remaining) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("probe {name} pipe remained open after the deadline"),
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            format!("probe {name} reader disconnected"),
        )),
    }
}

/// Run `cmd` to completion, killing it if it does not exit within `timeout`
/// (43-REVIEW.md WR-02). Without this, a blocked `opencode` subprocess
/// (hung credential re-auth prompt, network stall) stalls `health()`/
/// `capabilities()` forever — for `capabilities()` specifically, an unbounded
/// hang is a de facto launch refusal that never surfaces as an `Err`, directly
/// contradicting the "this probe can never refuse a launch" contract.
///
/// On Linux, a pre-exec seccomp filter prevents the probe tree from calling
/// `setsid` or `setpgid`, so its dedicated process group remains a complete
/// kill boundary. Other platforms retain the group cleanup where available,
/// but cannot make that stronger claim without a platform-native job/tree
/// primitive. Their pipe readers are still deadline-bounded, so an escaped
/// pipe holder fails the probe closed rather than hanging `health()` or
/// `capabilities()`. Such an unsupported escape can leave a reader thread
/// waiting for the inherited descriptor; it cannot leave the caller waiting.
fn spawn_with_timeout(
    cmd: &mut std::process::Command,
    timeout: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(target_os = "linux")]
    unsafe {
        // SAFETY: the callback only invokes `prctl` and reads/writes its
        // stack-local BPF program. It allocates nothing and takes no locks.
        std::os::unix::process::CommandExt::pre_exec(cmd, || {
            install_linux_probe_process_group_guard()
        });
    }
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::other("probe stdout was not piped despite Stdio::piped configuration")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        std::io::Error::other("probe stderr was not piped despite Stdio::piped configuration")
    })?;
    let stdout_receiver = capture_probe_pipe(stdout);
    let stderr_receiver = capture_probe_pipe(stderr);
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            #[cfg(unix)]
            if kill_probe_group(child.id())? {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "probe parent exited before its process group drained",
                ));
            }
            let stdout = collect_probe_pipe(stdout_receiver, deadline, "stdout")?;
            let stderr = collect_probe_pipe(stderr_receiver, deadline, "stderr")?;
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if std::time::Instant::now() >= deadline {
            #[cfg(unix)]
            {
                if !kill_probe_group(child.id())? {
                    let _ = child.kill();
                }
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("probe did not exit within {timeout:?}"),
            ));
        }
        std::thread::sleep(PROBE_POLL_INTERVAL);
    }
}

/// Hand-rolled CSI escape-sequence scrubber: on `\u{1b}` followed by `[`,
/// consumes through the sequence's final byte (ECMA-48 range `0x40..=0x7E`)
/// — not just `m` (SGR/color codes), which would over-consume into
/// following text on any other CSI sequence (cursor movement, erase-line,
/// etc.) and could corrupt the very count lines this scrubber exists to
/// preserve. No `regex` crate is added for this — this workspace has no
/// `regex` dependency anywhere, and `strip_corruption_padding`
/// (`agent_result.rs`) is the standing precedent for a single-purpose manual
/// text scrubber over pulling in a crate.
fn strip_ansi_escapes(s: &str) -> String {
    // 43-REVIEW.md IN-01: a legitimate CSI sequence's parameter bytes are a
    // handful of characters in ordinary SGR output (e.g. `90` in `\x1b[90m`);
    // this bounds how far the scrubber will consume looking for a final byte
    // before giving up. Without a bound, an ESC `[` with no final byte
    // anywhere in the REST of a truncated capture (a genuinely malformed/
    // cut-off input this function is explicitly meant to be resilient to)
    // silently discards everything after it — including a real terminal
    // footer line the caller needs. 128 (not a tighter bound like 32) is
    // deliberately generous headroom above ordinary SGR sequences — a real
    // CSI sequence with more parameter characters than that is rare, but a
    // false-positive "malformed" classification of one is the cost of too
    // tight a bound, so this errs wide (codex review, quick pass on 823cc58).
    const MAX_CSI_PARAM_CHARS: usize = 128;

    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            let mut consumed = String::new();
            let mut terminated = false;
            while let Some(&next) = chars.peek() {
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    chars.next();
                    terminated = true;
                    break;
                }
                if consumed.chars().count() >= MAX_CSI_PARAM_CHARS {
                    break;
                }
                consumed.push(next);
                chars.next();
            }
            if !terminated {
                // Malformed or truncated — preserve the escape and whatever
                // was scanned literally rather than silently dropping it;
                // normal processing resumes from here for the rest of `s`.
                out.push('\u{1b}');
                out.push('[');
                out.push_str(&consumed);
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Sum every `<n> credentials` / `<n> environment variable(s)` terminal count
/// line in `opencode providers list`'s (ANSI-stripped) output. `0` means no
/// usable provider is configured — the fail-closed signal (D-07).
///
/// Summing terminal count lines rather than pattern-matching a section
/// header is deliberate: it returns `0` identically for an absent section,
/// an empty section, and an explicit zero count line — the widest safe
/// reading of the shapes a genuinely credential-less machine could produce.
///
/// **Honest limit (A1, P-05):** the exact stdout shape of `opencode
/// providers list` on a machine with ZERO configured credentials was never
/// observed live — no destructive test against a credential-less machine was
/// performed. This function's zero-credential behavior is proven only
/// against constructed fixtures reasoned from the live positive-credential
/// capture below, never against a real credential-less run.
///
/// Anchored to the `└` footer glyph specifically (not `┌`/`│`/`●`, which mark
/// header/body lines) so a coincidentally-matching substring elsewhere in the
/// output — a header, a body line, a future diagnostic line that happens to
/// start with a number and the word "credential" — can never be summed in.
fn opencode_configured_provider_count(stdout: &str) -> u32 {
    strip_ansi_escapes(stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start().strip_prefix('└')?.trim_start();
            let (num, rest) = trimmed.split_once(' ')?;
            let n: u32 = num.parse().ok()?;
            (rest.starts_with("credential") || rest.starts_with("environment variable"))
                .then_some(n)
        })
        .sum()
}

/// Probe whether OpenCode has a genuinely dispatchable subagent configured.
/// Mirrors Hermes's `_with(output_fn)` split (`hermes.rs`) rather than Pi's
/// bare spawn, since no stub-binary spawn path itself needs testing here —
/// only the classification of an arbitrary `Output`.
fn opencode_subagent_dispatch_available() -> bool {
    opencode_subagent_dispatch_available_with(|| {
        let mut cmd = std::process::Command::new("opencode");
        cmd.args(["agent", "list"]);
        spawn_with_timeout(&mut cmd, PROBE_TIMEOUT)
    })
}

/// Mockable inner form of [`opencode_subagent_dispatch_available`]. Fails
/// closed to `false` on a spawn error, a non-zero exit, empty stdout, or no
/// matching header line — never a panic, and never a signal that could
/// refuse a launch (only `health` may refuse).
///
/// **Honest limit (A4):** the header-line form (`<name> (<mode>)`) is
/// inferred from `opencode agent create --help`'s documented `--mode`
/// choices (`primary`/`subagent`/`all`) plus one live single-agent baseline
/// (`build (primary)`) — no live capture of a real configured subagent
/// exists on this machine. A substring match tolerates some spacing drift
/// but is not proven against real subagent output; a miss costs a false
/// negative, which is the safe direction.
fn opencode_subagent_dispatch_available_with(
    output_fn: impl FnOnce() -> std::io::Result<std::process::Output>,
) -> bool {
    let Ok(output) = output_fn() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    parse_opencode_agent_list_for_subagent(&String::from_utf8_lossy(&output.stdout))
}

/// Pure classifier over `opencode agent list` stdout: a dispatchable
/// subagent is any header line carrying the `(subagent)` or `(all)` mode
/// marker. The default `build` agent is `(primary)` and must never count.
///
/// Anchored to a trailing mode-marker on a non-JSON line — not a raw
/// substring scan — so the permission-rule JSON dump that follows each
/// agent's header in real output can never coincidentally flip this `true`
/// from body text (the real baseline capture's dump lines all start with
/// `[` or `{`).
/// 43-REVIEW.md IN-02 fix: beyond the JSON-dump-line exclusion above, the
/// marker must be preceded by exactly one space and the name token before
/// it must itself contain no whitespace (`<name> (mode)`, matching the real
/// `build (primary)` baseline shape) — so free-form prose that merely ends
/// with the literal marker text (e.g. a crafted agent description reading
/// "...acts like a fallback (subagent)") cannot flip this `true`; only a
/// genuine single-token header line can.
///
/// **Known tradeoff (codex quick-review on 823cc58):** a real OpenCode agent
/// whose configured name itself contains whitespace (e.g. `code reviewer
/// (subagent)`) would NOT match this anchor and would be reported as
/// unavailable. This is the accepted direction: this function's own contract
/// (see `capabilities()` above) is that it must NEVER cause a false
/// `subagent_dispatch: true` — a false negative here only under-detects a
/// real capability (safe, degrades to the baseline single-agent path); the
/// false positive this anchor prevents (arbitrary prose ending in the marker
/// text) is the actually dangerous direction. No live capture of a
/// multi-word configured agent name exists to confirm whether OpenCode
/// permits one at all (same A4 honest-limit as the rest of this probe).
fn parse_opencode_agent_list_for_subagent(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        let line = line.trim();
        if line.starts_with(['[', '{']) {
            return false;
        }
        ["(subagent)", "(all)"].into_iter().any(|marker| {
            line.strip_suffix(marker)
                .and_then(|prefix| prefix.strip_suffix(' '))
                .is_some_and(|name| !name.is_empty() && !name.contains(char::is_whitespace))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentDriver;
    use crate::mode::Mode;
    use crate::state::{AgentKind, State};
    use std::os::unix::process::ExitStatusExt;

    const OPENCODE_STUB_DIR_ENV: &str = "DEVFLOW_TEST_OPENCODE_STUB_DIR";

    /// A `State` value for `health`, which ignores it (`_state`) —
    /// constructed only to satisfy the trait signature.
    fn test_state() -> State {
        State::new(
            PhaseId::new(43),
            AgentKind::OpenCode,
            Mode::Auto,
            std::path::PathBuf::from("/tmp"),
        )
    }

    /// Writes an executable `opencode` stub into a fresh tempdir. The stub
    /// records its arguments (`"$@"`, one per line) to `args.txt` in the same
    /// dir, prints `body` to stdout via the `printf` shell BUILTIN (never an
    /// external `cat`/`echo` binary — the tests that exercise this stub set
    /// `PATH` to point ONLY at this tempdir, so any command the script needs
    /// beyond its own shebang-resolved `/bin/sh` must be a builtin, not a
    /// `$PATH`-resolved external program), and exits with `exit_code`. The
    /// returned tempdir is the only entry the test puts on `PATH`, so the
    /// operator's live `opencode` is never consulted. Modeled on `pi.rs`'s
    /// `stub_pi_on_path`, renamed for `opencode`; `body` is embedded directly
    /// into the script inside single quotes (it contains no single-quote
    /// characters) rather than passed as a shell format-string argument, so
    /// its ANSI escapes and box-drawing glyphs pass through byte-for-byte.
    fn stub_opencode_on_path(body: &str, exit_code: i32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("opencode");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\nprintf '%s' '{body}'\nexit {exit_code}\n",
            args = dir.path().join("args.txt").display(),
            body = body,
            exit_code = exit_code,
        );
        std::fs::write(&stub, script).expect("write opencode stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
        }
        dir
    }

    /// Like [`stub_opencode_on_path`], but the stub sleeps for `sleep_secs`
    /// before it would ever print or exit — for exercising
    /// [`spawn_with_timeout`]'s kill path (43-REVIEW.md WR-02) without
    /// depending on a real hung `opencode`.
    fn stub_hanging_opencode_on_path(sleep_secs: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("opencode");
        let script = format!(
            "#!/bin/sh\nsleep {sleep_secs} &\nchild=$!\nprintf '%s\\n' \"$child\" > \"$0.pid\"\nwait \"$child\"\n"
        );
        std::fs::write(&stub, script).expect("write hanging opencode stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("/usr/bin/sleep", dir.path().join("sleep"))
            .expect("link the required sleep utility into the isolated PATH");
        dir
    }

    fn stub_parent_exits_before_hanging_child(sleep_secs: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("opencode");
        let script = format!(
            "#!/bin/sh\nsleep {sleep_secs} &\nchild=$!\nprintf '%s\\n' \"$child\" > \"$0.pid\"\nexit 0\n"
        );
        std::fs::write(&stub, script).expect("write parent-exit stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("/usr/bin/sleep", dir.path().join("sleep"))
            .expect("link the required sleep utility into the isolated PATH");
        dir
    }

    /// Linux-only CR-01 fixture. Without the pre-exec guard, `setsid` moves
    /// the `sleep` child out of the probe group while it keeps the inherited
    /// stdout/stderr descriptors open after this shell exits. With the guard,
    /// the real `setsid` utility receives EPERM and exits instead.
    #[cfg(target_os = "linux")]
    fn stub_parent_exits_after_attempting_setsid(sleep_secs: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("opencode");
        let script = format!(
            "#!/bin/sh\nsetsid sleep {sleep_secs} &\nchild=$!\nprintf '%s\\n' \"$child\" > \"$0.pid\"\nwait \"$child\"\nprintf '%s\\n' \"$?\" > \"$0.setsid-status\"\nexit 0\n"
        );
        std::fs::write(&stub, script).expect("write setsid escape stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
        }
        std::os::unix::fs::symlink("/usr/bin/sleep", dir.path().join("sleep"))
            .expect("link the required sleep utility into the isolated PATH");
        std::os::unix::fs::symlink("/usr/bin/setsid", dir.path().join("setsid"))
            .expect("link the required setsid utility into the isolated PATH");
        dir
    }

    /// Linux-only CR-01 sibling fixture for a pure process-group escape.
    /// The Python child records whether `setpgid(0, 0)` succeeded, then would
    /// retain the inherited pipes after the shell parent exits. Unlike
    /// `setsid`, this proves the second syscall denied by the guard.
    #[cfg(target_os = "linux")]
    fn stub_parent_exits_after_attempting_setpgid(sleep_secs: u64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create stub dir");
        let stub = dir.path().join("opencode");
        let escape = dir.path().join("opencode.setpgid.py");
        let script = "#!/bin/sh\npython3 \"$0.setpgid.py\" \"$0.setpgid-status\" &\nchild=$!\nprintf '%s\\n' \"$child\" > \"$0.pid\"\nexit 0\n";
        let python = format!(
            "import os, sys, time\nstatus = sys.argv[1]\ntry:\n    os.setpgid(0, 0)\nexcept OSError as error:\n    open(status, 'w', encoding='utf-8').write(str(error.errno))\n    raise\nopen(status, 'w', encoding='utf-8').write('0')\ntime.sleep({sleep_secs})\n"
        );
        std::fs::write(&stub, script).expect("write setpgid escape stub");
        std::fs::write(&escape, python).expect("write setpgid escape helper");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
        }
        std::os::unix::fs::symlink("/usr/bin/python3", dir.path().join("python3"))
            .expect("link the required Python utility into the isolated PATH");
        dir
    }

    fn hanging_stub_child_pid(stub_dir: &tempfile::TempDir) -> u32 {
        std::fs::read_to_string(stub_dir.path().join("opencode.pid"))
            .expect("hanging stub records its sleep pid")
            .trim()
            .parse()
            .expect("hanging stub pid is numeric")
    }

    fn child_opencode_stub_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(
            std::env::var_os(OPENCODE_STUB_DIR_ENV)
                .expect("child must receive the OpenCode stub directory from its parent"),
        )
    }

    /// The real, live-verified `opencode providers list` output captured
    /// this session (43-RESEARCH.md Pattern 3) — three credentials from
    /// `auth.json`, three provider environment variables, ANSI SGR codes and
    /// box-drawing glyphs intact.
    // 43-REVIEW.md IN-03: real provider/env-var names swapped for fictional
    // ones — the original values (still verified live, just not this
    // operator's actual configured providers) revealed which providers this
    // machine has configured, which is metadata worth not committing even
    // though it is not a secret.
    const LIVE_PROVIDER_LIST_OUTPUT: &str = "\x1b[0m\n┌  Credentials \x1b[90m~/.local/share/opencode/auth.json\x1b[0m\n│\n●  Acme \x1b[90mapi\x1b[0m\n│\n●  Widgetcorp \x1b[90moauth\x1b[0m\n│\n●  Contoso \x1b[90mapi\x1b[0m\n│\n└  3 credentials\n\n┌  Environment\n│\n●  Contoso \x1b[90mCONTOSO_API_KEY\x1b[0m\n│\n●  Acme \x1b[90mACME_API_KEY\x1b[0m\n│\n●  Fabrikam \x1b[90mFABRIKAM_API_KEY\x1b[0m\n│\n└  3 environment variables\n";

    #[test]
    fn provider_count_sums_credentials_and_environment() {
        assert_eq!(
            opencode_configured_provider_count(LIVE_PROVIDER_LIST_OUTPUT),
            6
        );
    }

    /// SYNTHETIC (A1, P-05) — the real zero-credential shape of `opencode
    /// providers list` has never been observed live. These three fixtures
    /// cover the plausible shapes the RESEARCH doc leaves open; none is a
    /// captured real run.
    #[test]
    fn provider_count_is_zero_for_constructed_credentialless_output() {
        // Shape 1: both sections absent entirely.
        let absent = "No providers configured. Run `opencode auth login` to add one.\n";
        assert_eq!(opencode_configured_provider_count(absent), 0);

        // Shape 2: section header/footer present but with no items and no
        // visible numeric count.
        let empty_sections = "┌  Credentials\n│\n└\n\n┌  Environment\n│\n└\n";
        assert_eq!(opencode_configured_provider_count(empty_sections), 0);

        // Shape 3: explicit zero-count terminal lines.
        let explicit_zero =
            "┌  Credentials\n└  0 credentials\n\n┌  Environment\n└  0 environment variables\n";
        assert_eq!(opencode_configured_provider_count(explicit_zero), 0);
    }

    #[test]
    fn provider_count_ignores_bullet_provider_lines() {
        let stray_bullet = "●  Google api\n";
        assert_eq!(opencode_configured_provider_count(stray_bullet), 0);
    }

    /// WR-03 regression: a non-footer line that coincidentally starts with a
    /// number followed by "credential"/"environment variable" must not be
    /// summed — only a genuine `└`-prefixed footer line counts.
    #[test]
    fn provider_count_ignores_unanchored_matching_substring() {
        let coincidental =
            "┌  Credentials\n3 credential refresh events pending\n│\n└  1 credentials\n";
        assert_eq!(opencode_configured_provider_count(coincidental), 1);
    }

    #[test]
    fn strip_ansi_escapes_removes_sgr_and_preserves_box_glyphs() {
        let input = "\x1b[90m┌│●└\x1b[0m plain \x1b[1;31mtext\x1b[0m";
        let stripped = strip_ansi_escapes(input);
        assert_eq!(stripped, "┌│●└ plain text");
    }

    /// WR-02 regression: a non-SGR CSI sequence (erase-line, `\x1b[2K`) must
    /// not over-consume into following text — only SGR (`...m`) previously
    /// terminated the scrubber's hunt loop, corrupting adjacent count lines.
    #[test]
    fn strip_ansi_escapes_terminates_on_non_sgr_csi_sequence() {
        let input = "\x1b[2K└  3 environment variables\n";
        let stripped = strip_ansi_escapes(input);
        assert_eq!(stripped, "└  3 environment variables\n");
    }

    /// 43-REVIEW.md IN-01: a malformed/truncated CSI sequence with no final
    /// byte anywhere in the rest of the buffer must not silently discard
    /// everything after it — a real terminal footer line beyond the bounded
    /// scan point must survive. The sequence's own parameter bytes plus the
    /// interleaved digits/box-glyphs before that footer are ALL outside the
    /// `0x40..=0x7E` final-byte range, so this reproduces the pre-fix
    /// full-buffer data loss (a bare digit/box-drawing run is not itself
    /// evidence of the bug — the credentials line surviving past the 32-char
    /// bound is).
    #[test]
    fn strip_ansi_escapes_preserves_content_after_an_unterminated_sequence() {
        let junk = "0".repeat(150); // exceeds MAX_CSI_PARAM_CHARS unterminated
        let input = format!("\x1b[{junk}└  3 credentials\n");
        let stripped = strip_ansi_escapes(&input);
        assert!(
            stripped.ends_with("└  3 credentials\n"),
            "the footer line must survive an unterminated escape earlier in the buffer: {stripped:?}"
        );
    }

    #[test]
    fn preflight_accepts_configured_credentials() {
        const NAME: &str = "agents::opencode::tests::preflight_accepts_configured_credentials";
        if crate::test_support::in_child_test(NAME) {
            OpenCodeDriver
                .health(&test_state())
                .expect("configured credentials must pass preflight");
            return;
        }
        let stub_dir = stub_opencode_on_path(LIVE_PROVIDER_LIST_OUTPUT, 0);
        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    /// SYNTHETIC (A1, P-05) — negative control proving exit code 0 alone
    /// does not green the check (T-43-09): the stub exits 0 (matching the
    /// live-verified always-0 exit code) but the stdout reports zero total
    /// credentials, so `health` must still refuse.
    #[test]
    fn preflight_rejects_constructed_zero_credential_output() {
        const NAME: &str =
            "agents::opencode::tests::preflight_rejects_constructed_zero_credential_output";
        if crate::test_support::in_child_test(NAME) {
            let err = OpenCodeDriver
                .health(&test_state())
                .expect_err("zero configured credentials must refuse preflight even with exit 0");
            assert!(err.contains("no OpenCode provider credential configured"));
            return;
        }
        let zero_body =
            "┌  Credentials\n└  0 credentials\n\n┌  Environment\n└  0 environment variables\n";
        let stub_dir = stub_opencode_on_path(zero_body, 0);
        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    /// WR-01 regression: a non-zero exit must refuse preflight even when the
    /// stdout it flushed before failing happens to contain a well-formed,
    /// credential-bearing count line — the exit status is now consulted in
    /// addition to the parsed count, not ignored entirely.
    #[test]
    fn preflight_rejects_nonzero_exit_with_credential_bearing_stdout() {
        const NAME: &str = "agents::opencode::tests::preflight_rejects_nonzero_exit_with_credential_bearing_stdout";
        if crate::test_support::in_child_test(NAME) {
            let err = OpenCodeDriver.health(&test_state()).expect_err(
                "a non-zero exit must fail closed even when stdout would otherwise report credentials",
            );
            assert!(err.contains("no OpenCode provider credential configured"));
            return;
        }
        let stub_dir = stub_opencode_on_path(LIVE_PROVIDER_LIST_OUTPUT, 1);
        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    #[test]
    fn preflight_rejects_when_probe_cannot_run() {
        const NAME: &str = "agents::opencode::tests::preflight_rejects_when_probe_cannot_run";
        if crate::test_support::in_child_test(NAME) {
            let err = OpenCodeDriver
                .health(&test_state())
                .expect_err("missing opencode binary must fail closed, not panic");
            assert!(!err.is_empty());
            return;
        }
        let empty_dir = tempfile::tempdir().expect("create empty dir");
        let output = crate::test_support::run_test_in_child(
            NAME,
            empty_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, empty_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    /// 43-REVIEW.md WR-02: a hung probe subprocess must be killed within its
    /// bound, not stall the caller forever. Uses a short custom timeout
    /// (well under the stub's `sleep 10`) so the test itself stays fast —
    /// the production `PROBE_TIMEOUT` constant is exercised by the plain
    /// `health()`/`capabilities()` call sites, not re-tested at its full
    /// duration here.
    #[test]
    fn spawn_with_timeout_kills_a_hung_child() {
        let stub_dir = stub_hanging_opencode_on_path(10);
        let mut cmd = std::process::Command::new(stub_dir.path().join("opencode"));
        let start = std::time::Instant::now();
        let err = spawn_with_timeout(&mut cmd, std::time::Duration::from_millis(200))
            .expect_err("a child sleeping 10s must not be allowed to finish under a 200ms bound");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let pid = hanging_stub_child_pid(&stub_dir);
        assert!(
            !crate::agent::agent_running(pid),
            "the timed-out probe's sleep descendant must be dead: pid {pid}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "the timeout must actually bound the wait, not merely be advisory: took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn spawn_with_timeout_kills_a_descendant_after_its_parent_exits() {
        let stub_dir = stub_parent_exits_before_hanging_child(10);
        let mut cmd = std::process::Command::new(stub_dir.path().join("opencode"));
        let start = std::time::Instant::now();
        let err = spawn_with_timeout(&mut cmd, std::time::Duration::from_millis(200))
            .expect_err("a pipe-holding descendant must keep the probe from succeeding");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let pid = hanging_stub_child_pid(&stub_dir);
        let reaped_by = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while crate::agent::agent_running(pid) && std::time::Instant::now() < reaped_by {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            !crate::agent::agent_running(pid),
            "the parent-exit probe's sleep descendant must be dead: pid {pid}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "a parent-exit descendant must not make output draining unbounded: took {:?}",
            start.elapsed()
        );
    }

    /// CR-01 regression and negative control. The unguarded direct command
    /// proves this host's `setsid` utility can create a session; the same
    /// utility inside a probe must instead be denied before it can retain the
    /// probe pipes from a different process group. Both `health` and
    /// `capabilities` must return promptly and fail closed.
    #[cfg(target_os = "linux")]
    #[test]
    fn probe_rejects_a_setsid_pipe_holder_before_parent_exit() {
        const NAME: &str =
            "agents::opencode::tests::probe_rejects_a_setsid_pipe_holder_before_parent_exit";
        if crate::test_support::in_child_test(NAME) {
            let start = std::time::Instant::now();
            let health = OpenCodeDriver.health(&test_state());
            let capabilities = OpenCodeDriver.capabilities();
            assert!(
                health.is_err(),
                "a rejected session escape must fail health closed"
            );
            assert!(
                !capabilities.subagent_dispatch,
                "a rejected session escape must fail capabilities closed"
            );
            assert!(
                start.elapsed() < std::time::Duration::from_secs(2),
                "an attempted setsid escape must not consume the five-second probe timeout: took {:?}",
                start.elapsed()
            );
            return;
        }

        let stub_dir = stub_parent_exits_after_attempting_setsid(60);
        let control = std::process::Command::new(stub_dir.path().join("setsid"))
            .arg(stub_dir.path().join("sleep"))
            .arg("0")
            .status()
            .expect("the unguarded negative control must run setsid");
        assert!(
            control.success(),
            "the unguarded negative control must create a session; otherwise this fixture does not exercise an escape-capable utility"
        );

        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);

        let status = std::fs::read_to_string(stub_dir.path().join("opencode.setsid-status"))
            .expect("the guarded fixture records the setsid exit status");
        assert_ne!(
            status.trim(),
            "0",
            "the guarded probe must deny the escape syscall rather than merely timing out after it succeeds"
        );
        let pid = hanging_stub_child_pid(&stub_dir);
        assert!(
            !crate::agent::agent_running(pid),
            "the attempted escape process must be reaped: pid {pid}"
        );
    }

    /// CR-01's process-group sibling: `setpgid(0, 0)` creates a different
    /// group without creating a session. The unguarded control must work; the
    /// probe version must be denied and both public probe paths must return
    /// fail-closed before the production timeout elapses.
    #[cfg(target_os = "linux")]
    #[test]
    fn probe_rejects_a_setpgid_pipe_holder_before_parent_exit() {
        const NAME: &str =
            "agents::opencode::tests::probe_rejects_a_setpgid_pipe_holder_before_parent_exit";
        if crate::test_support::in_child_test(NAME) {
            let start = std::time::Instant::now();
            let health = OpenCodeDriver.health(&test_state());
            let capabilities = OpenCodeDriver.capabilities();
            assert!(
                health.is_err(),
                "a rejected process-group escape must fail health closed"
            );
            assert!(
                !capabilities.subagent_dispatch,
                "a rejected process-group escape must fail capabilities closed"
            );
            assert!(
                start.elapsed() < std::time::Duration::from_secs(2),
                "an attempted setpgid escape must not consume the five-second probe timeout: took {:?}",
                start.elapsed()
            );
            return;
        }

        let stub_dir = stub_parent_exits_after_attempting_setpgid(60);
        let control = std::process::Command::new(stub_dir.path().join("python3"))
            .args(["-c", "import os; os.setpgid(0, 0)"])
            .status()
            .expect("the unguarded negative control must run Python");
        assert!(
            control.success(),
            "the unguarded negative control must create a process group; otherwise this fixture does not exercise an escape-capable utility"
        );

        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);

        let status_path = stub_dir.path().join("opencode.setpgid-status");
        let status_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !status_path.exists() && std::time::Instant::now() < status_deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let status = std::fs::read_to_string(&status_path)
            .expect("the guarded fixture records the setpgid exit status");
        assert_ne!(
            status.trim(),
            "0",
            "the guarded probe must deny setpgid rather than merely timing out after it succeeds"
        );
        let pid = hanging_stub_child_pid(&stub_dir);
        let reaped_by = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while crate::agent::agent_running(pid) && std::time::Instant::now() < reaped_by {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            !crate::agent::agent_running(pid),
            "the attempted group-escape process must be reaped: pid {pid}"
        );
    }

    /// 43-REVIEW.md WR-02: `health()` itself must fail closed (not hang) when
    /// the real `opencode providers list` invocation stalls, using the small
    /// custom `PROBE_TIMEOUT` bound rather than blocking forever. Runs
    /// against the actual production timeout constant, so this test's
    /// runtime is bounded by `PROBE_TIMEOUT`, not instantaneous.
    #[test]
    fn health_fails_closed_on_a_hung_probe() {
        const NAME: &str = "agents::opencode::tests::health_fails_closed_on_a_hung_probe";
        if crate::test_support::in_child_test(NAME) {
            let err = OpenCodeDriver
                .health(&test_state())
                .expect_err("a hung `opencode providers list` must fail closed, not hang forever");
            assert!(!err.is_empty());
            return;
        }
        let stub_dir = stub_hanging_opencode_on_path(60);
        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
        let pid = hanging_stub_child_pid(&stub_dir);
        assert!(
            !crate::agent::agent_running(pid),
            "the timed-out health probe's sleep descendant must be dead: pid {pid}"
        );
    }

    #[test]
    fn health_error_leaks_no_provider_detail() {
        const NAME: &str = "agents::opencode::tests::health_error_leaks_no_provider_detail";
        if crate::test_support::in_child_test(NAME) {
            let err = OpenCodeDriver
                .health(&test_state())
                .expect_err("zero total credentials must refuse preflight");

            for leaked in ["auth.json", "GOOGLE_API_KEY", "Google", "expired"] {
                assert!(
                    !err.contains(leaked),
                    "health error must not leak `{leaked}`, got: {err}"
                );
            }
            return;
        }
        let body = "┌  Credentials ~/.local/share/opencode/auth.json\n└  0 credentials\n\n┌  Environment\n│\n●  Google GOOGLE_API_KEY (expired)\n│\n└  0 environment variables\n";
        let stub_dir = stub_opencode_on_path(body, 0);
        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    #[test]
    fn health_probe_argv_is_providers_list() {
        const NAME: &str = "agents::opencode::tests::health_probe_argv_is_providers_list";
        if crate::test_support::in_child_test(NAME) {
            OpenCodeDriver
                .health(&test_state())
                .expect("configured credentials must pass health");

            let argv = std::fs::read_to_string(child_opencode_stub_dir().join("args.txt")).unwrap();
            assert_eq!(argv, "providers\nlist\n");
            return;
        }
        let stub_dir = stub_opencode_on_path(LIVE_PROVIDER_LIST_OUTPUT, 0);
        let output = crate::test_support::run_test_in_child(
            NAME,
            stub_dir.path(),
            &[(OPENCODE_STUB_DIR_ENV, stub_dir.path().as_os_str())],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    // --- Task 2: subagent-dispatch capability probe (spawn-free, mockable
    // `_with(output_fn)` form only — the stub-binary harness above stays
    // confined to `health`, where the spawn path itself is under test) ---

    fn mock_output(exit_code: i32, stdout: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(exit_code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn agent_list_baseline_reports_no_subagent() {
        let baseline = "build (primary)\n  [\n  {\n    \"permission\": \"*\",\n";
        assert!(!parse_opencode_agent_list_for_subagent(baseline));
    }

    #[test]
    fn agent_list_with_subagent_mode_reports_true() {
        let with_subagent =
            "build (primary)\n  [\n  {\n    \"permission\": \"*\",\n  reviewer (subagent)\n";
        assert!(parse_opencode_agent_list_for_subagent(with_subagent));
    }

    #[test]
    fn agent_list_with_all_mode_reports_true() {
        let with_all = "build (primary)\n  [\n  {\n    \"permission\": \"*\",\n  helper (all)\n";
        assert!(parse_opencode_agent_list_for_subagent(with_all));
    }

    /// WR-04 regression: a permission-dump line that contains the literal
    /// text "(subagent)" as part of body content (not as a trailing header
    /// mode marker) must not flip the result — only an actual header line
    /// counts.
    #[test]
    fn agent_list_ignores_marker_text_inside_json_dump_line() {
        let body_text_only =
            "build (primary)\n  [\n  {\n    \"description\": \"acts like a (subagent) proxy\",\n";
        assert!(!parse_opencode_agent_list_for_subagent(body_text_only));
    }

    /// 43-REVIEW.md IN-02 fix: the previous test above never actually
    /// exercised the bracket-prefix guard — its crafted line didn't end with
    /// the literal marker text, so the ends-with anchor alone already
    /// rejected it regardless of the guard. This line does NOT start with
    /// `[`/`{` (so the bracket guard would NOT exclude it) but DOES end
    /// with the literal marker text with free-form multi-word prose before
    /// it — only the name-token anchor (43-REVIEW.md IN-02's actual fix)
    /// prevents this from flipping the result.
    #[test]
    fn agent_list_ignores_prose_line_ending_in_the_literal_marker_text() {
        let prose_line = "build (primary)\na fallback description mentions (subagent)\n";
        assert!(!parse_opencode_agent_list_for_subagent(prose_line));
    }

    #[test]
    fn subagent_probe_fails_closed_on_spawn_error() {
        let result = opencode_subagent_dispatch_available_with(|| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            ))
        });
        assert!(!result);
    }

    #[test]
    fn subagent_probe_fails_closed_on_nonzero_exit() {
        let result = opencode_subagent_dispatch_available_with(|| {
            Ok(mock_output(1, "reviewer (subagent)\n"))
        });
        assert!(
            !result,
            "a non-zero exit must fail closed even when stdout would otherwise classify true"
        );
    }

    #[test]
    fn subagent_probe_fails_closed_on_empty_output() {
        let result = opencode_subagent_dispatch_available_with(|| Ok(mock_output(0, "")));
        assert!(!result);
    }

    /// Mirrors `OpenCodeDriver::capabilities`'s exact wrapping
    /// (`super::DriverCapabilities { subagent_dispatch: ... }`) across every
    /// failure mode above, proving the probe's return type can never be a
    /// `Result` that would let it refuse a launch — a spawn error, a
    /// non-zero exit, and empty output all resolve to a valid
    /// `DriverCapabilities` value, never a panic or an early return.
    #[test]
    fn capabilities_never_refuses_a_launch() {
        type OutputFn = Box<dyn FnOnce() -> std::io::Result<std::process::Output>>;
        let cases: Vec<(&str, OutputFn)> = vec![
            (
                "spawn error",
                Box::new(|| {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "not found",
                    ))
                }),
            ),
            (
                "non-zero exit",
                Box::new(|| Ok(mock_output(1, "reviewer (subagent)\n"))),
            ),
            ("empty output", Box::new(|| Ok(mock_output(0, "")))),
        ];
        for (label, case) in cases {
            let caps = super::super::DriverCapabilities {
                subagent_dispatch: opencode_subagent_dispatch_available_with(case),
            };
            assert!(!caps.subagent_dispatch, "{label} must fail closed");
        }
    }
}

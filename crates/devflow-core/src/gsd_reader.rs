//! The one seam through which DevFlow asks GSD about a phase's verification.
//!
//! **DevFlow does not own the answer.** GSD writes `VERIFICATION.md`, decides
//! what its frontmatter means and computes whether it is stale. This module
//! never parses the report, never recomputes a digest (D-45 rule 3) and never
//! routes on GSD's runtime-projected `next_command`. It runs GSD's own readers
//! — `query verification status`, `query frontmatter get`,
//! `runtime-identity` — and reads their JSON (D-08).
//!
//! Three properties follow, and each is pinned by a stub-launcher test rather
//! than left to inspection (CI has no `node`, so no test runs the real GSD):
//!
//! 1. **Fail closed (D-24).** Every way the answer can be missing is its own
//!    [`ReaderFailure`], never a status. Exit 0 is not success: gsd-core
//!    1.20.1 exits 0 for `phase_dir_not_found` and for a frontmatter
//!    `{"error": ...}`, so the JSON is indexed and an absent field is a
//!    failure, never a benign default.
//! 2. **One hermetic, bounded call.** The launcher runs by argv (never
//!    `sh -c`), in the evidence root, with every repo-local `GIT_*` variable
//!    removed and `HOME` set to the home it was found under. The call is
//!    killed after [`GSD_READER_TIMEOUT`]; there is no env var or config key
//!    for it.
//! 3. **No path in an event.** GSD's messages hold absolute paths
//!    (Pitfall 11). [`ReaderFailure::label`] is the event-safe name; the
//!    message stays in the variant, for the gate context and stderr only.

use crate::git::hermetic_command_at;
use crate::phase_id::PhaseId;
use serde_json::Value;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one GSD reader call may run before it is killed.
pub const GSD_READER_TIMEOUT: Duration = Duration::from_secs(60);

/// How often a running reader is polled for exit.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Where each runtime install ships GSD's `gsd_run` launcher, relative to the
/// home, in the order they are tried.
const LAUNCHER_CANDIDATES: [&str; 3] = [
    ".claude/gsd-core/bin/gsd_run",
    ".codex/gsd-core/bin/gsd_run",
    ".pi/agent/gsd-core/bin/gsd_run",
];

/// A verification status GSD reported and DevFlow can act on.
///
/// `unparseable` and `phase_dir_not_found` are in GSD's set too, but they say
/// nothing was established, so they are [`ReaderFailure`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GsdStatus {
    /// `passed`: the report passed and no covered file moved since.
    Passed,
    /// `gaps_found`: the report found gaps (route `plan-phase`).
    GapsFound,
    /// `human_needed`: the report needs a human sign-off (route `verify-work`).
    HumanNeeded,
    /// `stale`: a covered file changed after the verifier ran.
    Stale,
    /// `missing`: the phase directory holds no report.
    Missing,
}

/// What `query verification status` said about a phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// The status, from GSD's closed set.
    pub status: GsdStatus,
    /// GSD's bare route command (`plan-phase`, `execute-phase`, ...), empty
    /// when GSD names none.
    pub route: String,
    /// The covered files GSD saw move (GSD omits the key when none did).
    pub moved_files: Vec<String>,
}

/// Every way a GSD reader can fail to give an answer.
///
/// The `Display` text may carry GSD's message and absolute paths; use
/// [`ReaderFailure::label`] wherever the text is persisted as an event.
#[derive(Debug, thiserror::Error)]
pub enum ReaderFailure {
    /// No launcher under the home.
    #[error("no GSD launcher (gsd_run) under {home}")]
    LauncherNotFound {
        /// The home that was searched.
        home: PathBuf,
    },
    /// The launcher could not be run, or waiting on it failed.
    #[error("the GSD reader could not be run: {message}")]
    Spawn {
        /// The OS error.
        message: String,
    },
    /// The reader did not finish in time; it was killed and reaped.
    #[error("the GSD reader did not finish within {after:?}")]
    TimedOut {
        /// The deadline that expired.
        after: Duration,
    },
    /// The reader exited 0 but its stdout is not JSON.
    #[error("the GSD reader printed something that is not JSON: {message}")]
    NotJson {
        /// The parse error.
        message: String,
    },
    /// GSD reported a status outside the known set.
    #[error("GSD reported a verification status outside the known set: {status}")]
    UnknownStatus {
        /// The status string GSD printed.
        status: String,
    },
    /// No phase directory matched.
    #[error("no phase directory: {message}")]
    PhaseDirNotFound {
        /// What was searched, or GSD's message.
        message: String,
    },
    /// More than one phase directory matched, so which one GSD should read
    /// is not established.
    #[error("more than one phase directory matches: {matches:?}")]
    AmbiguousPhaseDir {
        /// The matching directories.
        matches: Vec<PathBuf>,
    },
    /// The reader exited non-zero (gsd-core 1.20.1 does so for a report
    /// status outside its closed set).
    #[error("the GSD reader exited {code:?}: {message}")]
    Exit {
        /// The exit code, `None` when a signal ended it.
        code: Option<i32>,
        /// The reader's stderr.
        message: String,
    },
    /// GSD could not parse the report's frontmatter (`unparseable`).
    #[error("GSD could not parse the verification report: {message}")]
    Unparseable {
        /// GSD's message.
        message: String,
    },
    /// GSD could not determine whether the report is stale
    /// (`staleCheckIndeterminate: true`), so a `passed` is not established.
    #[error("GSD could not determine whether the verification report is stale")]
    Indeterminate,
    /// The JSON lacks a field the answer needs, or holds it with the wrong
    /// type.
    #[error("the GSD reader's JSON has no usable `{key}` field")]
    MissingField {
        /// The field.
        key: String,
    },
}

impl ReaderFailure {
    /// An event-safe name for the failure class: no path, no GSD message.
    #[must_use]
    pub fn label(&self) -> &'static str {
        ""
    }
}

/// The first `gsd_run` launcher under `home`: the Claude install, then the
/// Codex install, then the pi install.
pub fn locate_launcher(home: &Path) -> Result<PathBuf, ReaderFailure> {
    LAUNCHER_CANDIDATES
        .iter()
        .map(|relative| home.join(relative))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| ReaderFailure::LauncherNotFound {
            home: home.to_path_buf(),
        })
}

/// The phase's directory under `evidence_root`: the one entry of
/// `.planning/phases` named `{padded}-...` (`07-demo` for phase 7).
///
/// It returns the directory, not the report, so GSD can still answer
/// `missing` for a directory that holds no report. Pass the root the agent
/// wrote to (the worktree in worktree mode), never the main checkout: the
/// main checkout would read `missing` for a phase that has a report.
pub fn phase_dir(evidence_root: &Path, phase: PhaseId) -> Result<PathBuf, ReaderFailure> {
    let phases = evidence_root.join(".planning/phases");
    let prefix = format!("{padded}-", padded = phase.padded());
    let entries = std::fs::read_dir(&phases).map_err(|error| ReaderFailure::PhaseDirNotFound {
        message: format!("cannot read {}: {error}", phases.display()),
    })?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.starts_with(&prefix))
        })
        .ok_or_else(|| ReaderFailure::PhaseDirNotFound {
            message: format!("no {prefix}* directory under {}", phases.display()),
        })
}

/// The command one reader call runs: `launcher args...` by argv, in
/// `evidence_root`, with the repo-local `GIT_*` variables removed, `HOME` set
/// to the home the launcher was found under, stdin closed, stdout and stderr
/// captured, and (on Unix) its own process group so a timeout can end
/// everything it started.
pub(crate) fn reader_command(
    launcher: &Path,
    home: &Path,
    evidence_root: &Path,
    args: &[&OsStr],
) -> Command {
    let mut cmd = hermetic_command_at(launcher, evidence_root);
    cmd.args(args)
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// What a finished reader left behind.
struct ReaderOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

/// Run `cmd` to completion or until `timeout`, whichever is first.
///
/// stdout and stderr are drained on their own threads so a full pipe cannot
/// stall the child into a false timeout. On expiry the child's process group
/// is killed and the child reaped before [`ReaderFailure::TimedOut`] returns.
fn run_bounded(mut cmd: Command, timeout: Duration) -> Result<ReaderOutput, ReaderFailure> {
    let deadline = Instant::now() + timeout;
    let mut child = cmd.spawn().map_err(|error| ReaderFailure::Spawn {
        message: error.to_string(),
    })?;
    let stdout = drain(child.stdout.take());
    let _stderr = drain(child.stderr.take());
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                end(&mut child);
                return Err(ReaderFailure::TimedOut { after: timeout });
            }
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(error) => {
                end(&mut child);
                return Err(ReaderFailure::Spawn {
                    message: format!("waiting on the GSD reader failed: {error}"),
                });
            }
        }
    };
    let stdout = collect(&stdout, deadline, timeout)?;
    Ok(ReaderOutput { status, stdout })
}

/// Read `pipe` to its end on a thread; the bytes arrive on the receiver.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<std::io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match pipe {
            Some(mut pipe) => {
                let mut bytes = Vec::new();
                pipe.read_to_end(&mut bytes).map(|_| bytes)
            }
            None => Err(std::io::Error::other("the pipe was not captured")),
        };
        // The receiver is gone only when the call already failed.
        let _ = sender.send(result);
    });
    receiver
}

/// The drained bytes, waiting no later than `deadline`.
///
/// A grandchild that outlives the reader and keeps the pipe open would block
/// the drain; that reads as a timeout, not as an empty answer.
fn collect(
    receiver: &mpsc::Receiver<std::io::Result<Vec<u8>>>,
    deadline: Instant,
    timeout: Duration,
) -> Result<Vec<u8>, ReaderFailure> {
    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(ReaderFailure::Spawn {
            message: format!("reading the GSD reader's output failed: {error}"),
        }),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(ReaderFailure::TimedOut { after: timeout }),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(ReaderFailure::Spawn {
            message: "the GSD reader's output was lost".to_owned(),
        }),
    }
}

/// Kill the unreaped `child`'s process group, then the child, and reap it.
fn end(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id())
        && pid > 1
    {
        // SAFETY: `child` is not yet reaped, so `pid` is still this child and,
        // through `process_group(0)`, its own group id; `pid > 1`, so `-pid`
        // is neither our group (`0`) nor every process (`-1`).
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    // Both fail only when the child already exited, which the wait reaps.
    let _ = child.kill();
    let _ = child.wait();
}

/// Classify a `query verification status` result.
fn classify(value: &Value) -> Result<StatusReport, ReaderFailure> {
    let status = match value["status"].as_str() {
        Some("passed") => GsdStatus::Passed,
        other => {
            return Err(ReaderFailure::UnknownStatus {
                status: other.unwrap_or_default().to_owned(),
            });
        }
    };
    Ok(StatusReport {
        status,
        route: value["route"].as_str().unwrap_or_default().to_owned(),
        moved_files: Vec::new(),
    })
}

/// GSD's verification status for `phase_dir` (from [`phase_dir`]), read in
/// `evidence_root` through the launcher under `home`.
pub fn verification_status(
    home: &Path,
    evidence_root: &Path,
    phase_dir: &Path,
) -> Result<StatusReport, ReaderFailure> {
    verification_status_within(home, evidence_root, phase_dir, GSD_READER_TIMEOUT)
}

/// [`verification_status`] with the deadline as a parameter, so a test can
/// use a short one.
fn verification_status_within(
    home: &Path,
    evidence_root: &Path,
    phase_dir: &Path,
    timeout: Duration,
) -> Result<StatusReport, ReaderFailure> {
    let launcher = locate_launcher(home)?;
    let args = [
        OsStr::new("query"),
        OsStr::new("verification"),
        OsStr::new("status"),
        phase_dir.as_os_str(),
    ];
    let output = run_bounded(
        reader_command(&launcher, home, evidence_root, &args),
        timeout,
    )?;
    let _ = output.status;
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| ReaderFailure::NotJson {
            message: error.to_string(),
        })?;
    classify(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{ALSO_REDIRECTING_GIT_VARS, REPO_LOCAL_GIT_VARS};

    /// The recorded gsd-core 1.20.1 `passed` result (keys from
    /// `routeResult`, values from the probe table and `VERIFICATION_ROUTES`).
    const PASSED_JSON: &str = r#"{"status":"passed","next_action":"Verification passed — continue.","next_command":"","route":""}"#;

    /// A temp home with a stub launcher slot, a temp evidence root holding
    /// `.planning/phases/07-demo/`, and a log directory the stub writes to.
    struct Fixture {
        _dir: tempfile::TempDir,
        home: PathBuf,
        root: PathBuf,
        logs: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path().join("home");
            let root = dir.path().join("root");
            let logs = dir.path().join("logs");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(root.join(".planning/phases/07-demo")).unwrap();
            std::fs::create_dir_all(&logs).unwrap();
            Self {
                _dir: dir,
                home,
                root,
                logs,
            }
        }

        fn phase(&self) -> PathBuf {
            self.root.join(".planning/phases/07-demo")
        }

        /// Plant `body` as an executable `#!/bin/sh` launcher at
        /// `<home>/<relative>`.
        fn plant_at(&self, relative: &str, body: &str) -> PathBuf {
            let launcher = self.home.join(relative);
            std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
            std::fs::write(&launcher, format!("#!/bin/sh\n{body}\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
            launcher
        }

        /// A Claude-home launcher that logs its argv and working directory,
        /// prints `stdout` and exits `code`.
        fn plant_printing(&self, stdout: &str, code: i32) {
            assert!(
                !stdout.contains('\''),
                "fixture must not hold a single quote"
            );
            let body = format!(
                "printf '%s\\n' \"$@\" > '{argv}'\npwd -P > '{cwd}'\nprintf '%s\\n' '{stdout}'\nexit {code}",
                argv = self.logs.join("argv").display(),
                cwd = self.logs.join("cwd").display(),
            );
            self.plant_at(".claude/gsd-core/bin/gsd_run", &body);
        }

        fn argv(&self) -> Vec<String> {
            std::fs::read_to_string(self.logs.join("argv"))
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn cwd(&self) -> PathBuf {
            PathBuf::from(
                std::fs::read_to_string(self.logs.join("cwd"))
                    .unwrap()
                    .trim_end(),
            )
        }
    }

    #[test]
    fn a_passed_report_reads_as_passed() {
        let fx = Fixture::new();
        fx.plant_printing(PASSED_JSON, 0);

        let dir = phase_dir(&fx.root, PhaseId::new(7)).expect("the 07- directory is found");
        assert_eq!(dir, fx.phase());

        let report = verification_status(&fx.home, &fx.root, &dir).expect("passed is a status");
        assert_eq!(report.status, GsdStatus::Passed);
        assert_eq!(report.route, "");
        assert!(report.moved_files.is_empty());

        let expected_argv = vec![
            "query".to_owned(),
            "verification".to_owned(),
            "status".to_owned(),
            dir.display().to_string(),
        ];
        assert_eq!(fx.argv(), expected_argv);
        assert_eq!(fx.cwd(), fx.root.canonicalize().unwrap());
    }

    #[test]
    fn the_reader_command_removes_repo_local_git_vars() {
        let fx = Fixture::new();
        let cmd = reader_command(
            Path::new("/nonexistent/gsd_run"),
            &fx.home,
            &fx.root,
            &[OsStr::new("runtime-identity")],
        );
        let removed: Vec<&str> = cmd
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .filter_map(|(key, _)| key.to_str())
            .collect();
        for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
            assert!(
                removed.contains(var),
                "{var} is not cleared on the GSD reader command"
            );
        }
        assert_eq!(cmd.get_current_dir(), Some(fx.root.as_path()));
    }

    // ---- Statuses: recorded gsd-core 1.20.1 shapes (probe table, RESEARCH
    // § GSD Integration). Keys follow `routeResult`; `next_action` text is
    // `VERIFICATION_ROUTES`'. The stale result is the verbatim probe output.

    const STALE_JSON: &str = r#"{"status":"stale","next_action":"Verification is stale — covered source files changed after the verifier last ran: \"a.txt\". Re-run execute-phase for this phase: …","next_command":"/gsd-execute-phase 01","route":"execute-phase","moved_files":["a.txt"]}"#;
    const GAPS_FOUND_JSON: &str = r#"{"status":"gaps_found","next_action":"Gaps found. Plan the fixes, then re-run execute-phase before shipping.","next_command":"/gsd-plan-phase 01 --gaps","route":"plan-phase"}"#;
    const MISSING_JSON: &str = r#"{"status":"missing","next_action":"No verification report found — the verify step never completed.","next_command":"/gsd-execute-phase 01","route":"execute-phase"}"#;
    /// Not in the probe table: built from the enum and `VERIFICATION_ROUTES`
    /// (`human_needed` → `verify-work`).
    const HUMAN_NEEDED_JSON: &str = r#"{"status":"human_needed","next_action":"Human verification required.","next_command":"/gsd-verify-work 01","route":"verify-work"}"#;
    const PHASE_DIR_NOT_FOUND_JSON: &str = r#"{"status":"phase_dir_not_found","next_action":"Usage error: the phase directory does not exist","next_command":"","route":"","message":"Usage error: no phase directory at /var/home/x/.planning/phases/07-demo"}"#;
    /// Not in the probe table: built from the enum and `VERIFICATION_ROUTES`.
    const UNPARSEABLE_JSON: &str = r#"{"status":"unparseable","next_action":"The *-VERIFICATION.md frontmatter is not parseable YAML — fix the syntax error in the report itself.","next_command":"","route":""}"#;
    const INDETERMINATE_PASSED_JSON: &str = r#"{"status":"passed","next_action":"Verification passed — continue.","next_command":"","route":"","staleCheckIndeterminate":true}"#;
    /// The recorded stderr for a report whose status is `bogus` (exit 1, no
    /// stdout).
    const BOGUS_STDERR: &str = "Error: Verification report /var/home/x/.planning/phases/07-demo/07-VERIFICATION.md has status \"bogus\", which is outside the closed set";

    fn status_of(fx: &Fixture) -> Result<StatusReport, ReaderFailure> {
        verification_status(&fx.home, &fx.root, &fx.phase())
    }

    #[test]
    fn a_stale_report_reads_as_stale_with_its_moved_files() {
        let fx = Fixture::new();
        fx.plant_printing(STALE_JSON, 0);
        let report = status_of(&fx).expect("stale is a status");
        assert_eq!(report.status, GsdStatus::Stale);
        assert_eq!(report.route, "execute-phase");
        assert_eq!(report.moved_files, vec!["a.txt".to_owned()]);
    }

    #[test]
    fn a_gaps_found_report_reads_as_gaps_found() {
        let fx = Fixture::new();
        fx.plant_printing(GAPS_FOUND_JSON, 0);
        let report = status_of(&fx).expect("gaps_found is a status");
        assert_eq!(report.status, GsdStatus::GapsFound);
        assert_eq!(report.route, "plan-phase");
    }

    #[test]
    fn a_missing_report_reads_as_missing() {
        let fx = Fixture::new();
        fx.plant_printing(MISSING_JSON, 0);
        let report = status_of(&fx).expect("missing is a status");
        assert_eq!(report.status, GsdStatus::Missing);
        assert_eq!(report.route, "execute-phase");
    }

    #[test]
    fn a_human_needed_report_reads_as_human_needed() {
        let fx = Fixture::new();
        fx.plant_printing(HUMAN_NEEDED_JSON, 0);
        let report = status_of(&fx).expect("human_needed is a status");
        assert_eq!(report.status, GsdStatus::HumanNeeded);
        assert_eq!(report.route, "verify-work");
    }

    // ---- Failures: each is an `Err` of its own variant.

    #[test]
    fn no_launcher_is_a_reader_failure() {
        let fx = Fixture::new();
        let failure = status_of(&fx).expect_err("no launcher is not a status");
        assert!(
            matches!(failure, ReaderFailure::LauncherNotFound { ref home } if *home == fx.home),
            "{failure:?}"
        );
    }

    #[test]
    fn a_spawn_error_is_a_reader_failure() {
        let fx = Fixture::new();
        let launcher = fx.plant_at(".claude/gsd-core/bin/gsd_run", "exit 0");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o644)).unwrap();
        let failure = status_of(&fx).expect_err("an unexecutable launcher is not a status");
        assert!(
            matches!(failure, ReaderFailure::Spawn { .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn a_hung_reader_times_out() {
        let fx = Fixture::new();
        let pid_file = fx.logs.join("pid");
        fx.plant_at(
            ".claude/gsd-core/bin/gsd_run",
            &format!("echo $$ > '{}'\nexec sleep 30", pid_file.display()),
        );
        let deadline = Duration::from_millis(300);
        let started = Instant::now();
        let failure = verification_status_within(&fx.home, &fx.root, &fx.phase(), deadline)
            .expect_err("a hung reader is not a status");
        let elapsed = started.elapsed();
        assert!(
            matches!(failure, ReaderFailure::TimedOut { after } if after == deadline),
            "{failure:?}"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "the call waited {elapsed:?}, not the deadline"
        );
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // SAFETY: signal 0 only probes whether `pid` exists.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        assert!(!alive, "the hung reader {pid} was left running");
    }

    #[test]
    fn a_nonzero_exit_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_at(
            ".claude/gsd-core/bin/gsd_run",
            &format!("printf '%s\\n' '{BOGUS_STDERR}' >&2\nexit 1"),
        );
        let failure = status_of(&fx).expect_err("a non-zero exit is not a status");
        match failure {
            ReaderFailure::Exit { code, ref message } => {
                assert_eq!(code, Some(1));
                assert!(message.contains("\"bogus\""), "{message}");
            }
            other => panic!("expected Exit, got {other:?}"),
        }
    }

    #[test]
    fn a_nonzero_exit_wins_over_a_passed_json() {
        let fx = Fixture::new();
        fx.plant_printing(PASSED_JSON, 3);
        let failure = status_of(&fx).expect_err("a non-zero exit is not a status");
        assert!(
            matches!(failure, ReaderFailure::Exit { code: Some(3), .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn non_json_stdout_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing("Verification passed", 0);
        let failure = status_of(&fx).expect_err("prose is not a status");
        assert!(
            matches!(failure, ReaderFailure::NotJson { .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn an_unknown_status_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(r#"{"status":"bogus","route":""}"#, 0);
        let failure = status_of(&fx).expect_err("an unknown status is not a status");
        assert!(
            matches!(failure, ReaderFailure::UnknownStatus { ref status } if status == "bogus"),
            "{failure:?}"
        );
    }

    #[test]
    fn phase_dir_not_found_with_exit_0_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(PHASE_DIR_NOT_FOUND_JSON, 0);
        let failure = status_of(&fx).expect_err("phase_dir_not_found is not a status");
        match failure {
            ReaderFailure::PhaseDirNotFound { ref message } => {
                assert!(message.contains("Usage error"), "{message}");
            }
            other => panic!("expected PhaseDirNotFound, got {other:?}"),
        }
    }

    #[test]
    fn an_unparseable_status_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(UNPARSEABLE_JSON, 0);
        let failure = status_of(&fx).expect_err("unparseable is not a status");
        assert!(
            matches!(failure, ReaderFailure::Unparseable { .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn an_indeterminate_passed_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(INDETERMINATE_PASSED_JSON, 0);
        let failure = status_of(&fx).expect_err("an indeterminate passed is not a pass");
        assert!(
            matches!(failure, ReaderFailure::Indeterminate),
            "{failure:?}"
        );
    }

    #[test]
    fn a_json_object_without_status_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(r#"{"route":"","next_command":""}"#, 0);
        let failure = status_of(&fx).expect_err("no status is not a status");
        assert!(
            matches!(failure, ReaderFailure::MissingField { ref key } if key == "status"),
            "{failure:?}"
        );
    }

    #[test]
    fn a_passed_report_without_a_route_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(r#"{"status":"passed","next_command":""}"#, 0);
        let failure = status_of(&fx).expect_err("an absent route is not an empty one");
        assert!(
            matches!(failure, ReaderFailure::MissingField { ref key } if key == "route"),
            "{failure:?}"
        );
    }

    #[test]
    fn two_matching_phase_dirs_are_a_reader_failure() {
        let fx = Fixture::new();
        std::fs::create_dir_all(fx.root.join(".planning/phases/07-other")).unwrap();
        let failure = phase_dir(&fx.root, PhaseId::new(7)).expect_err("two 07- directories");
        match failure {
            ReaderFailure::AmbiguousPhaseDir { ref matches } => assert_eq!(matches.len(), 2),
            other => panic!("expected AmbiguousPhaseDir, got {other:?}"),
        }
    }

    /// One of every variant, each built from a message holding an absolute
    /// path. The exhaustive `match` makes a new variant fail to compile here
    /// until it is added to the list.
    fn every_failure(path: &str) -> Vec<ReaderFailure> {
        let all = vec![
            ReaderFailure::LauncherNotFound {
                home: PathBuf::from(path),
            },
            ReaderFailure::Spawn {
                message: path.to_owned(),
            },
            ReaderFailure::TimedOut {
                after: Duration::from_secs(1),
            },
            ReaderFailure::NotJson {
                message: path.to_owned(),
            },
            ReaderFailure::UnknownStatus {
                status: path.to_owned(),
            },
            ReaderFailure::PhaseDirNotFound {
                message: path.to_owned(),
            },
            ReaderFailure::AmbiguousPhaseDir {
                matches: vec![PathBuf::from(path)],
            },
            ReaderFailure::Exit {
                code: Some(1),
                message: path.to_owned(),
            },
            ReaderFailure::Unparseable {
                message: path.to_owned(),
            },
            ReaderFailure::Indeterminate,
            ReaderFailure::MissingField {
                key: path.to_owned(),
            },
        ];
        for failure in &all {
            match failure {
                ReaderFailure::LauncherNotFound { .. }
                | ReaderFailure::Spawn { .. }
                | ReaderFailure::TimedOut { .. }
                | ReaderFailure::NotJson { .. }
                | ReaderFailure::UnknownStatus { .. }
                | ReaderFailure::PhaseDirNotFound { .. }
                | ReaderFailure::AmbiguousPhaseDir { .. }
                | ReaderFailure::Exit { .. }
                | ReaderFailure::Unparseable { .. }
                | ReaderFailure::Indeterminate
                | ReaderFailure::MissingField { .. } => {}
            }
        }
        all
    }

    #[test]
    fn reader_failure_labels_carry_no_path() {
        let path = "/var/home/x/.planning";
        let failures = every_failure(path);
        let mut labels = Vec::new();
        for failure in &failures {
            let label = failure.label();
            assert!(!label.is_empty(), "{failure:?} has an empty label");
            assert!(
                !label.contains('/'),
                "{failure:?} label {label:?} holds a path"
            );
            assert!(
                !labels.contains(&label),
                "{failure:?} shares label {label:?}"
            );
            labels.push(label);
        }
        // Control: the Display text does carry the path, so the label is what
        // keeps it out of an event.
        assert!(
            failures
                .iter()
                .any(|failure| failure.to_string().contains(path))
        );
    }
}

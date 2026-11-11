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
}

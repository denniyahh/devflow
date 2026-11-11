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
///
/// Stored (in a gate file's `gsd_status`, 52-29) as GSD's own token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// GSD's `next_action` text, `None` when GSD's answer had none.
    pub next_action: Option<String>,
    /// The covered files GSD saw move (GSD omits the key when none did).
    pub moved_files: Vec<String>,
}

/// Every way a GSD reader can fail to give an answer.
///
/// The `Display` text may carry GSD's message and absolute paths; use
/// [`ReaderFailure::label`] wherever the text is persisted as an event.
/// `non_exhaustive`: a later gsd-core can add a failure class without a
/// major release of this published crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
    /// GSD answered with an `error` key (gsd-core 1.20.1 exits 0 for a
    /// frontmatter field that is not there).
    #[error("GSD reported an error: {message}")]
    GsdError {
        /// GSD's error text.
        message: String,
    },
}

impl ReaderFailure {
    /// An event-safe name for the failure class: no path, no GSD message.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::LauncherNotFound { .. } => "gsd_launcher_not_found",
            Self::Spawn { .. } => "gsd_reader_spawn_failed",
            Self::TimedOut { .. } => "gsd_reader_timed_out",
            Self::NotJson { .. } => "gsd_reader_not_json",
            Self::UnknownStatus { .. } => "gsd_unknown_status",
            Self::PhaseDirNotFound { .. } => "gsd_phase_dir_not_found",
            Self::AmbiguousPhaseDir { .. } => "gsd_phase_dir_ambiguous",
            Self::Exit { .. } => "gsd_reader_nonzero_exit",
            Self::Unparseable { .. } => "gsd_report_unparseable",
            Self::Indeterminate => "gsd_stale_check_indeterminate",
            Self::MissingField { .. } => "gsd_reader_missing_field",
            Self::GsdError { .. } => "gsd_reader_error",
        }
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
/// main checkout would read `missing` for a phase that has a report. Two
/// matching directories are a failure, not a guess: `read_dir` order is
/// unspecified, so "the first" would be arbitrary.
pub fn phase_dir(evidence_root: &Path, phase: PhaseId) -> Result<PathBuf, ReaderFailure> {
    let phases = evidence_root.join(".planning/phases");
    let prefix = format!("{padded}-", padded = phase.padded());
    let entries = std::fs::read_dir(&phases).map_err(|error| ReaderFailure::PhaseDirNotFound {
        message: format!("cannot read {}: {error}", phases.display()),
    })?;
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect();
    match matches.len() {
        0 => Err(ReaderFailure::PhaseDirNotFound {
            message: format!("no {prefix}* directory under {}", phases.display()),
        }),
        1 => Ok(matches.remove(0)),
        _ => {
            matches.sort();
            Err(ReaderFailure::AmbiguousPhaseDir { matches })
        }
    }
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
    stderr: Vec<u8>,
}

/// Run `cmd` to completion or until `timeout`, whichever is first.
///
/// stdout and stderr are drained on their own threads so a full pipe cannot
/// stall the child into a false timeout. On expiry the child's process group
/// is killed and the child reaped before [`ReaderFailure::TimedOut`] returns,
/// whether the child itself was still running or had exited while a process
/// it started held a pipe open past the deadline.
fn run_bounded(mut cmd: Command, timeout: Duration) -> Result<ReaderOutput, ReaderFailure> {
    let deadline = Instant::now() + timeout;
    let mut child = cmd.spawn().map_err(|error| ReaderFailure::Spawn {
        message: error.to_string(),
    })?;
    // Read before `try_wait` reaps the child: through `process_group(0)` the
    // child's pid is its group's id, which outlives the child while any
    // member does.
    let group = child.id();
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
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
    let collected = collect(&stdout, deadline, timeout)
        .and_then(|stdout| Ok((stdout, collect(&stderr, deadline, timeout)?)));
    let (stdout, stderr) = match collected {
        Ok(both) => both,
        Err(failure) => {
            if matches!(failure, ReaderFailure::TimedOut { .. }) {
                // The child is reaped; a member of its group that holds the
                // pipe is reaped by its new parent once it is killed.
                kill_group(group);
            }
            return Err(failure);
        }
    };
    Ok(ReaderOutput {
        status,
        stdout,
        stderr,
    })
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
    kill_group(child.id());
    // Both fail only when the child already exited, which the wait reaps.
    let _ = child.kill();
    let _ = child.wait();
}

/// SIGKILL the process group `group`, the id of a reader child spawned with
/// `process_group(0)`.
///
/// Called while the child is unreaped, or after it was reaped while a process
/// still holds one of its pipes open. That holder is a member of the group
/// unless it left with `setsid` (a `detached` spawn): such a holder is out of
/// reach of this kill, and once the group is empty its id could name another
/// group.
fn kill_group(group: u32) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(group)
        && pid > 1
    {
        // SAFETY: `group` is a reader child's own group id (see above);
        // `pid > 1`, so `-pid` is neither our group (`0`) nor every process
        // (`-1`).
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = group;
}

/// Run one reader under `home` in `evidence_root` and parse its stdout.
///
/// The exit status is checked before stdout is read, so a non-zero exit is a
/// failure even when stdout holds a well-formed answer.
fn read_json(
    home: &Path,
    evidence_root: &Path,
    args: &[&OsStr],
    timeout: Duration,
) -> Result<Value, ReaderFailure> {
    let launcher = locate_launcher(home)?;
    let output = run_bounded(
        reader_command(&launcher, home, evidence_root, args),
        timeout,
    )?;
    if !output.status.success() {
        return Err(ReaderFailure::Exit {
            code: output.status.code(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|error| ReaderFailure::NotJson {
        message: error.to_string(),
    })
}

/// GSD's own explanation in a result, for the gate context only.
fn gsd_message(value: &Value) -> String {
    value["message"]
        .as_str()
        .or_else(|| value["next_action"].as_str())
        .unwrap_or("GSD gave no message")
        .to_owned()
}

/// The string at `key`, or a failure naming the key.
fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, ReaderFailure> {
    value[key]
        .as_str()
        .ok_or_else(|| ReaderFailure::MissingField {
            key: key.to_owned(),
        })
}

/// Classify a `query verification status` result.
///
/// `staleCheckIndeterminate: true` is checked first: it means GSD could not
/// tell whether the status still holds, so no status is established.
fn classify(value: &Value) -> Result<StatusReport, ReaderFailure> {
    match value.get("staleCheckIndeterminate") {
        None | Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => return Err(ReaderFailure::Indeterminate),
        Some(_) => {
            return Err(ReaderFailure::MissingField {
                key: "staleCheckIndeterminate".to_owned(),
            });
        }
    }
    let status = match required_str(value, "status")? {
        "passed" => GsdStatus::Passed,
        "gaps_found" => GsdStatus::GapsFound,
        "human_needed" => GsdStatus::HumanNeeded,
        "stale" => GsdStatus::Stale,
        "missing" => GsdStatus::Missing,
        "unparseable" => {
            return Err(ReaderFailure::Unparseable {
                message: gsd_message(value),
            });
        }
        "phase_dir_not_found" => {
            return Err(ReaderFailure::PhaseDirNotFound {
                message: gsd_message(value),
            });
        }
        other => {
            return Err(ReaderFailure::UnknownStatus {
                status: other.to_owned(),
            });
        }
    };
    let route = required_str(value, "route")?.to_owned();
    // GSD writes `next_action` beside every status; absent, it is `None`,
    // and present, it must be a string.
    let next_action = match value.get("next_action") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err(ReaderFailure::MissingField {
                key: "next_action".to_owned(),
            });
        }
    };
    // GSD omits `moved_files` when no covered file moved; present, it must be
    // a list of strings.
    let moved_files = match value.get("moved_files") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ReaderFailure::MissingField {
                key: "moved_files".to_owned(),
            })?,
        Some(_) => {
            return Err(ReaderFailure::MissingField {
                key: "moved_files".to_owned(),
            });
        }
    };
    Ok(StatusReport {
        status,
        route,
        next_action,
        moved_files,
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
    let args = [
        OsStr::new("query"),
        OsStr::new("verification"),
        OsStr::new("status"),
        phase_dir.as_os_str(),
    ];
    classify(&read_json(home, evidence_root, &args, timeout)?)
}

/// A failure when GSD answered with an `error` key, whatever it exited.
fn refuse_gsd_error(value: &Value) -> Result<(), ReaderFailure> {
    match value.get("error") {
        None => Ok(()),
        Some(Value::String(message)) => Err(ReaderFailure::GsdError {
            message: message.clone(),
        }),
        Some(other) => Err(ReaderFailure::GsdError {
            message: other.to_string(),
        }),
    }
}

/// The value of frontmatter `field` in `file`, as GSD's
/// `query frontmatter get <file> --field <field>` reads it.
///
/// An `error` key is a failure even at exit 0, and so is a result that does
/// not carry `field`. A field present with a `null` value is returned as
/// `null`: present-and-empty is not absent.
pub fn frontmatter_field(
    home: &Path,
    evidence_root: &Path,
    file: &Path,
    field: &str,
) -> Result<Value, ReaderFailure> {
    let args = [
        OsStr::new("query"),
        OsStr::new("frontmatter"),
        OsStr::new("get"),
        file.as_os_str(),
        OsStr::new("--field"),
        OsStr::new(field),
    ];
    let value = read_json(home, evidence_root, &args, GSD_READER_TIMEOUT)?;
    refuse_gsd_error(&value)?;
    value
        .get(field)
        .cloned()
        .ok_or_else(|| ReaderFailure::MissingField {
            key: field.to_owned(),
        })
}

/// The installed GSD version (`runtime-identity --raw`'s `version`), so a
/// caller can record which GSD answered.
pub fn runtime_version(home: &Path, evidence_root: &Path) -> Result<String, ReaderFailure> {
    let args = [OsStr::new("runtime-identity"), OsStr::new("--raw")];
    let value = read_json(home, evidence_root, &args, GSD_READER_TIMEOUT)?;
    refuse_gsd_error(&value)?;
    required_str(&value, "version").map(str::to_owned)
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
        let recorded: Value = serde_json::from_str(STALE_JSON).unwrap();
        assert_eq!(
            report.next_action.as_deref(),
            recorded["next_action"].as_str(),
            "GSD's next action is read verbatim"
        );
        assert!(report.next_action.is_some());
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

    /// A launcher that exits at once while a process it started keeps stdout
    /// open times out on the drain, and that process (a member of the
    /// reader's process group) is killed before `TimedOut` returns. Control
    /// in the same test: the leftover is alive while the launcher has already
    /// exited, so the drain, not the launcher, is what timed out.
    #[test]
    fn a_reader_whose_child_holds_stdout_times_out_and_the_child_is_killed() {
        let fx = Fixture::new();
        let group_file = fx.logs.join("group");
        let leftover_file = fx.logs.join("leftover");
        fx.plant_at(
            ".claude/gsd-core/bin/gsd_run",
            &format!(
                "sleep 30 &\necho $! > '{leftover}'\necho $$ > '{group}'\nexit 0",
                leftover = leftover_file.display(),
                group = group_file.display(),
            ),
        );
        let deadline = Duration::from_millis(300);
        let started = Instant::now();
        let failure = verification_status_within(&fx.home, &fx.root, &fx.phase(), deadline)
            .expect_err("a reader whose output never closes is not a status");
        let elapsed = started.elapsed();
        let read_pid = |path: &Path| -> u32 {
            std::fs::read_to_string(path)
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        let (group, leftover) = (read_pid(&group_file), read_pid(&leftover_file));
        // SIGKILL is delivered asynchronously: allow a short window for the
        // leftover to leave the live set.
        let gone_by = Instant::now() + Duration::from_secs(2);
        while crate::test_support::process_is_live(leftover) && Instant::now() < gone_by {
            std::thread::sleep(Duration::from_millis(10));
        }
        let leftover_live = crate::test_support::process_is_live(leftover);
        let group_live = crate::test_support::live_group_members(group);
        if leftover_live {
            // Clean up the recorded leftover before the assertion fails.
            // SAFETY: `leftover` is the pid the stub recorded, still live.
            unsafe {
                libc::kill(i32::try_from(leftover).unwrap(), libc::SIGKILL);
            }
        }
        assert!(
            matches!(failure, ReaderFailure::TimedOut { after } if after == deadline),
            "{failure:?}"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "the call waited {elapsed:?}, not the deadline"
        );
        assert!(
            !leftover_live,
            "the process {leftover} holding the reader's stdout was left running"
        );
        assert!(
            group_live.is_empty(),
            "the reader's group {group} still has live members: {group_live:?}"
        );
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
            ReaderFailure::GsdError {
                message: path.to_owned(),
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
                | ReaderFailure::MissingField { .. }
                | ReaderFailure::GsdError { .. } => {}
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

    // ---- Frontmatter and version readers: recorded gsd-core 1.20.1 outputs.

    const FRONTMATTER_JSON: &str = r#"{"covered_files":["a.txt"]}"#;
    const FIELD_NOT_FOUND_JSON: &str = r#"{"error":"Field not found","field":"nope"}"#;
    const RUNTIME_IDENTITY_JSON: &str = r#"{"packageName":"@opengsd/gsd-core","version":"1.20.1"}"#;

    fn report(fx: &Fixture) -> PathBuf {
        fx.phase().join("07-VERIFICATION.md")
    }

    #[test]
    fn frontmatter_field_reads_the_requested_field() {
        let fx = Fixture::new();
        fx.plant_printing(FRONTMATTER_JSON, 0);
        let value = frontmatter_field(&fx.home, &fx.root, &report(&fx), "covered_files")
            .expect("a present field is read");
        assert_eq!(value, serde_json::json!(["a.txt"]));
        let expected_argv = vec![
            "query".to_owned(),
            "frontmatter".to_owned(),
            "get".to_owned(),
            report(&fx).display().to_string(),
            "--field".to_owned(),
            "covered_files".to_owned(),
        ];
        assert_eq!(fx.argv(), expected_argv);
        assert_eq!(fx.cwd(), fx.root.canonicalize().unwrap());
    }

    #[test]
    fn a_missing_frontmatter_field_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(FIELD_NOT_FOUND_JSON, 0);
        let failure = frontmatter_field(&fx.home, &fx.root, &report(&fx), "nope")
            .expect_err("an exit-0 error is not a value");
        match failure {
            ReaderFailure::GsdError { ref message } => assert_eq!(message, "Field not found"),
            other => panic!("expected GsdError, got {other:?}"),
        }
    }

    #[test]
    fn a_failing_frontmatter_read_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(FRONTMATTER_JSON, 1);
        let failure = frontmatter_field(&fx.home, &fx.root, &report(&fx), "covered_files")
            .expect_err("a non-zero exit is not a value");
        assert!(
            matches!(failure, ReaderFailure::Exit { code: Some(1), .. }),
            "{failure:?}"
        );
    }

    #[test]
    fn a_frontmatter_result_without_the_field_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(FRONTMATTER_JSON, 0);
        let failure = frontmatter_field(&fx.home, &fx.root, &report(&fx), "gap_closure")
            .expect_err("an absent key is not a value");
        assert!(
            matches!(failure, ReaderFailure::MissingField { ref key } if key == "gap_closure"),
            "{failure:?}"
        );
    }

    #[test]
    fn runtime_version_reads_the_gsd_version() {
        let fx = Fixture::new();
        fx.plant_printing(RUNTIME_IDENTITY_JSON, 0);
        let version = runtime_version(&fx.home, &fx.root).expect("the version is read");
        assert_eq!(version, "1.20.1");
        assert_eq!(
            fx.argv(),
            vec!["runtime-identity".to_owned(), "--raw".to_owned()]
        );
    }

    #[test]
    fn a_runtime_identity_without_a_version_is_a_reader_failure() {
        let fx = Fixture::new();
        fx.plant_printing(r#"{"packageName":"@opengsd/gsd-core"}"#, 0);
        let failure = runtime_version(&fx.home, &fx.root).expect_err("no version is not a version");
        assert!(
            matches!(failure, ReaderFailure::MissingField { ref key } if key == "version"),
            "{failure:?}"
        );
    }
}

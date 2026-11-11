//! The one seam through which DevFlow asks GSD about a phase's verification.
//!
//! **DevFlow does not own the answer.** GSD writes `VERIFICATION.md`, decides
//! what its frontmatter means and computes whether it is stale. This module
//! never parses the report, never recomputes a digest (D-45 rule 3) and never
//! routes on GSD's runtime-projected `next_command`. It runs GSD's own readers
//! and reads their JSON (D-08).

use crate::phase_id::PhaseId;
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

/// How long one GSD reader call may run before it is killed.
pub const GSD_READER_TIMEOUT: Duration = Duration::from_secs(60);

/// A verification status GSD reported and DevFlow can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GsdStatus {
    /// The report passed and no covered file moved since.
    Passed,
    /// The report found gaps.
    GapsFound,
    /// The report needs a human sign-off.
    HumanNeeded,
    /// A covered file changed after the verifier ran.
    Stale,
    /// The phase directory holds no report.
    Missing,
}

/// What `query verification status` said about a phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// The status, from GSD's closed set.
    pub status: GsdStatus,
    /// GSD's bare route command (`plan-phase`, `execute-phase`, ...).
    pub route: String,
    /// The covered files GSD saw move.
    pub moved_files: Vec<String>,
}

/// Every way a GSD reader can fail to give an answer.
#[derive(Debug, thiserror::Error)]
pub enum ReaderFailure {
    /// No launcher under the home.
    #[error("no GSD launcher (gsd_run) under {home}")]
    LauncherNotFound {
        /// The home that was searched.
        home: PathBuf,
    },
    /// The launcher could not be run.
    #[error("the GSD reader could not be run: {message}")]
    Spawn {
        /// The OS error.
        message: String,
    },
    /// The reader did not finish in time.
    #[error("the GSD reader did not finish within {after:?}")]
    TimedOut {
        /// The deadline that expired.
        after: Duration,
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
        /// What was searched.
        message: String,
    },
}

/// The launcher under `home`.
pub fn locate_launcher(home: &Path) -> Result<PathBuf, ReaderFailure> {
    Err(ReaderFailure::LauncherNotFound {
        home: home.to_path_buf(),
    })
}

/// The phase directory under `evidence_root`.
pub fn phase_dir(evidence_root: &Path, _phase: PhaseId) -> Result<PathBuf, ReaderFailure> {
    Err(ReaderFailure::PhaseDirNotFound {
        message: evidence_root.display().to_string(),
    })
}

/// The command one reader call runs.
pub(crate) fn reader_command(
    launcher: &Path,
    _home: &Path,
    _evidence_root: &Path,
    args: &[&OsStr],
) -> Command {
    let mut cmd = Command::new(launcher);
    cmd.args(args);
    cmd
}

fn run_bounded(mut cmd: Command, _timeout: Duration) -> Result<Output, ReaderFailure> {
    cmd.output().map_err(|error| ReaderFailure::Spawn {
        message: error.to_string(),
    })
}

fn classify(value: &Value) -> Result<StatusReport, ReaderFailure> {
    Err(ReaderFailure::UnknownStatus {
        status: value.to_string(),
    })
}

/// GSD's verification status for `phase_dir`.
pub fn verification_status(
    home: &Path,
    evidence_root: &Path,
    phase_dir: &Path,
) -> Result<StatusReport, ReaderFailure> {
    verification_status_within(home, evidence_root, phase_dir, GSD_READER_TIMEOUT)
}

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
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| ReaderFailure::UnknownStatus {
            status: error.to_string(),
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

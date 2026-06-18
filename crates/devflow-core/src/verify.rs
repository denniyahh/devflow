//! Command execution helpers for verify, lint, and docs automation.
//!
//! Provides a unified interface for running shell commands with output capture,
//! used by the CLI's `verify`, `lint`, and `docs` subcommands and by the state
//! machine's auto-advancement through VERIFYING and DOCSING steps.

use std::io;
use std::path::Path;

/// Result of running a shell command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Exit code (None if killed by signal).
    pub exit_code: Option<i32>,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
}

impl CommandResult {
    /// Whether the command exited successfully (code 0).
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Errors produced while running automation commands.
#[derive(Debug)]
pub enum VerifyError {
    /// The shell process could not be spawned.
    Spawn(io::Error),

    /// The command ran but failed (non-zero exit).
    CommandFailed {
        exit_code: Option<i32>,
        stderr: String,
    },
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::Spawn(e) => write!(f, "failed to spawn command: {e}"),
            VerifyError::CommandFailed { exit_code, stderr } => {
                let code = exit_code.map_or("signal".into(), |c| c.to_string());
                write!(f, "command exited with code {code}: {stderr}")
            }
        }
    }
}

impl std::error::Error for VerifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VerifyError::Spawn(e) => Some(e),
            VerifyError::CommandFailed { .. } => None,
        }
    }
}

impl From<io::Error> for VerifyError {
    fn from(e: io::Error) -> Self {
        VerifyError::Spawn(e)
    }
}

/// Run a shell command in the given working directory.
///
/// Uses `sh -c` for consistent shell-like behavior across platforms.
/// Returns the full output regardless of exit code — callers decide
/// whether a non-zero exit is fatal.
pub fn run_command(command: &str, cwd: &Path) -> Result<CommandResult, io::Error> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .output()?;

    Ok(CommandResult {
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Run a command and fail if its exit code is non-zero.
///
/// On failure, returns `VerifyError::CommandFailed` with the captured stderr.
pub fn run_or_fail(command: &str, cwd: &Path) -> Result<CommandResult, VerifyError> {
    let result = run_command(command, cwd)?;
    if !result.success() {
        return Err(VerifyError::CommandFailed {
            exit_code: result.exit_code,
            stderr: result.stderr.clone(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_true_succeeds() {
        let result = run_command("true", Path::new(".")).expect("run true");
        assert!(result.success());
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn run_command_false_fails_but_no_error() {
        let result = run_command("false", Path::new(".")).expect("run false");
        assert!(!result.success());
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn run_command_echo_captures_stdout() {
        let result = run_command("echo hello", Path::new(".")).expect("run echo");
        assert!(result.success());
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn run_or_fail_rejects_nonzero() {
        let err = run_or_fail("false", Path::new(".")).unwrap_err();
        match err {
            VerifyError::CommandFailed { exit_code, .. } => {
                assert_eq!(exit_code, Some(1));
            }
            other => panic!("expected CommandFailed, got {other}"),
        }
    }

    #[test]
    fn run_or_fail_accepts_zero() {
        let result = run_or_fail("true", Path::new(".")).expect("run_or_fail true");
        assert!(result.success());
    }

    #[test]
    fn command_with_stderr_capture() {
        let result = run_command("echo ok; echo err >&2", Path::new(".")).expect("run");
        assert!(result.success());
        assert!(result.stdout.contains("ok"));
        assert!(result.stderr.contains("err"));
    }
}

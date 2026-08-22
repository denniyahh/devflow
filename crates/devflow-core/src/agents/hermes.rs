//! Hermes coding-agent adapter (Phase 42).
//!
//! Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless-safe oneshot mode.
//! The prompt is passed via `-z`. Environment variable `HERMES_ACCEPT_HOOKS=1` is injected
//! to avoid interactive prompts on shell hooks.
//!
//! Slash commands (`/gsd-*`) are rendered via standard claude-style prompt rendering.
//! Subagent dispatch capability is dynamically probed via `hermes tools list` checking for
//! the enabled `delegation` toolset.

use super::AgentDriver;
use crate::phase_id::PhaseId;
use std::path::PathBuf;

/// The modular driver for Hermes (Phase 42): headless `-z` oneshot launch,
/// `HERMES_ACCEPT_HOOKS=1` environment, standard claude-style prompt rendering,
/// and dynamic delegation subagent dispatch probing.
pub struct HermesDriver;

impl AgentDriver for HermesDriver {
    fn name(&self) -> &'static str {
        "Hermes"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    fn capabilities(&self) -> super::DriverCapabilities {
        super::DriverCapabilities {
            subagent_dispatch: hermes_subagent_dispatch_available(),
        }
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "hermes",
            vec![
                "-z".into(),
                prompt.to_string(),
                "--yolo".into(),
                "--accept-hooks".into(),
            ],
        )
    }

    fn environment(&self) -> Vec<(String, String)> {
        vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]
    }

    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        // Presence-only probe of the `hermes` binary.
        let output = std::process::Command::new("hermes")
            .arg("--version")
            .output()
            .map_err(|e| format!("could not run `hermes --version`: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let detail = String::from_utf8_lossy(&output.stderr);
            Err(format!("`hermes --version` failed: {}", detail.trim()))
        }
    }
}

/// Dynamically probe whether Hermes has the `delegation` toolset enabled.
///
/// Runs `hermes tools list` and checks for both `enabled` and `delegation` in the output.
pub fn hermes_subagent_dispatch_available() -> bool {
    hermes_subagent_dispatch_available_with(|| {
        std::process::Command::new("hermes")
            .args(["tools", "list"])
            .output()
    })
}

/// Inner helper parameterized on output function for unit testing without invoking real CLI.
pub fn hermes_subagent_dispatch_available_with(
    output_fn: impl FnOnce() -> Result<std::process::Output, std::io::Error>,
) -> bool {
    let Ok(output) = output_fn() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_hermes_tools_list_for_delegation(&stdout)
}

/// Parse `hermes tools list` stdout to check if delegation toolset is enabled.
pub fn parse_hermes_tools_list_for_delegation(stdout: &str) -> bool {
    for line in stdout.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("delegation")
            && lower.contains("enabled")
            && !lower.contains("disabled")
            && !lower.contains("not enabled")
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase_id::PhaseId;
    use crate::stage::Stage;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn hermes_driver_name() {
        let driver = HermesDriver;
        assert_eq!(driver.name(), "Hermes");
    }

    #[test]
    fn hermes_driver_build_command() {
        let driver = HermesDriver;
        let (prog, args) = driver.build_command(PhaseId::new(42), "test prompt", &[]);
        assert_eq!(prog, "hermes");
        assert_eq!(
            args,
            vec![
                "-z".to_string(),
                "test prompt".to_string(),
                "--yolo".to_string(),
                "--accept-hooks".to_string(),
            ]
        );
    }

    #[test]
    fn hermes_driver_environment() {
        let driver = HermesDriver;
        let envs = driver.environment();
        assert_eq!(envs, vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]);
    }

    #[test]
    fn hermes_driver_render_prompt() {
        let driver = HermesDriver;
        let intent = crate::prompt::StageIntent::for_stage(Stage::Plan, PhaseId::new(42));
        let rendered = driver.render_prompt(&intent);
        assert!(rendered.contains("DEVFLOW_RESULT"));
        assert!(rendered.contains("/gsd-plan-phase 42"));
    }

    #[test]
    fn parse_hermes_tools_list_delegation_enabled() {
        let sample = "\
Available Toolsets:
  ✓ enabled delegation 👥 Task Delegation
  ✓ enabled terminal   💻 Terminal Execution
  ✗ disabled web       🌐 Web Search
";
        assert!(parse_hermes_tools_list_for_delegation(sample));
    }

    #[test]
    fn parse_hermes_tools_list_delegation_disabled() {
        let sample = "\
Available Toolsets:
  ✗ disabled delegation 👥 Task Delegation
  ✓ enabled terminal   💻 Terminal Execution
";
        assert!(!parse_hermes_tools_list_for_delegation(sample));
    }

    #[test]
    fn parse_hermes_tools_list_missing_delegation() {
        let sample = "\
Available Toolsets:
  ✓ enabled terminal   💻 Terminal Execution
";
        assert!(!parse_hermes_tools_list_for_delegation(sample));
    }

    #[test]
    fn parse_hermes_tools_list_disabled_delegation_with_enabled_word() {
        let sample = "\
Available Toolsets:
  ✗ disabled delegation 👥 Task Delegation (can be enabled in config)
";
        assert!(!parse_hermes_tools_list_for_delegation(sample));
    }

    #[test]
    fn hermes_subagent_dispatch_with_mock() {
        let success_output = || {
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"  \xe2\x9c\x93 enabled delegation \xf0\x9f\x91\xa5 Task Delegation\n"
                    .to_vec(),
                stderr: Vec::new(),
            })
        };
        assert!(hermes_subagent_dispatch_available_with(success_output));

        let failure_output = || {
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: b"error\n".to_vec(),
                stderr: Vec::new(),
            })
        };
        assert!(!hermes_subagent_dispatch_available_with(failure_output));

        let io_error = || {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            ))
        };
        assert!(!hermes_subagent_dispatch_available_with(io_error));
    }
}

//! DevFlow project configuration and fixed git-flow branch model.
//!
//! Phase 16 decision D-03 deliberately reopened the earlier no-config-file
//! decision for a minimal `devflow.toml` containing only Phase 16 knobs.
//! `DEVFLOW_*` environment variables remain the highest-precedence overrides.
//! The git-flow branch model remains hardcoded to the opinionated `main`,
//! `develop`, and `feature/` constants below.

use std::path::Path;

/// Number of capture generations retained when not otherwise configured.
pub const DEFAULT_CAPTURE_RETENTION: usize = 5;

/// Production/release branch name.
pub const MAIN: &str = "main";
/// Development/integration branch name.
pub const DEVELOP: &str = "develop";
/// Prefix for per-phase feature branches.
pub const FEATURE_PREFIX: &str = "feature/";

/// The fixed git-flow branch names used by the current pipeline.
///
/// Kept as a struct (rather than bare constants) so the modules that build
/// branch names — git, ship, agent-result evaluation — can take a single value
/// and stay readable. `default()` is the only constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFlowConfig {
    /// Main/production branch name.
    pub main: String,
    /// Development/integration branch name.
    pub develop: String,
    /// Prefix for feature branches.
    pub feature_prefix: String,
}

impl Default for GitFlowConfig {
    fn default() -> Self {
        GitFlowConfig {
            main: MAIN.to_string(),
            develop: DEVELOP.to_string(),
            feature_prefix: FEATURE_PREFIX.to_string(),
        }
    }
}

/// The minimal project configuration introduced by Phase 16 decision D-03.
///
/// Missing fields inherit their built-in defaults so operators can specify
/// only the knobs they need in `devflow.toml`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct DevflowConfig {
    /// Number of capture generations to retain per pipeline stage.
    pub capture_retention: usize,
    /// Custom Ship review angles; `None` keeps the built-in angle list.
    pub review_angles: Option<Vec<String>>,
    /// Whether declared external verification commands may run.
    pub external_verify_enabled: bool,
}

impl Default for DevflowConfig {
    fn default() -> Self {
        Self {
            capture_retention: DEFAULT_CAPTURE_RETENTION,
            review_angles: None,
            external_verify_enabled: true,
        }
    }
}

impl DevflowConfig {
    /// Return the configured capture-retention count.
    pub fn capture_retention(&self) -> usize {
        self.capture_retention
    }

    /// Return configured review angles, or `None` to use built-in angles.
    pub fn review_angles(&self) -> Option<&[String]> {
        self.review_angles.as_deref()
    }

    /// Return whether external verification is enabled.
    pub fn external_verify_enabled(&self) -> bool {
        self.external_verify_enabled
    }
}

/// Load the minimal Phase 16 configuration from `<project_root>/devflow.toml`.
///
/// A missing file preserves built-in behavior. Read or parse failures are
/// fail-soft: DevFlow warns and continues with defaults instead of aborting the
/// workflow.
pub fn load_config(project_root: &Path) -> DevflowConfig {
    let path = project_root.join("devflow.toml");
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DevflowConfig::default();
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to read devflow config; using defaults");
            return DevflowConfig::default();
        }
    };

    match toml::from_str(&contents) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to parse devflow config; using defaults");
            DevflowConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_hardcoded_constants() {
        let config = GitFlowConfig::default();
        assert_eq!(config.main, "main");
        assert_eq!(config.develop, "develop");
        assert_eq!(config.feature_prefix, "feature/");
    }
}

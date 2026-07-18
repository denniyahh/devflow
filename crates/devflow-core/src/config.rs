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

/// Resolve capture retention with `DEVFLOW_CAPTURE_RETENTION` taking
/// precedence over `devflow.toml` and the built-in default.
pub fn capture_retention(project_root: &Path) -> usize {
    if let Some(value) = env_value("DEVFLOW_CAPTURE_RETENTION") {
        match value.parse() {
            Ok(retention) => return retention,
            Err(error) => tracing::warn!(
                value,
                %error,
                "invalid DEVFLOW_CAPTURE_RETENTION; using devflow.toml or default"
            ),
        }
    }
    load_config(project_root).capture_retention
}

/// Resolve Ship review angles with `DEVFLOW_REVIEW_ANGLES` taking precedence
/// over `devflow.toml`. The environment value is a comma-separated list.
pub fn review_angles(project_root: &Path) -> Option<Vec<String>> {
    if let Some(value) = env_value("DEVFLOW_REVIEW_ANGLES") {
        let angles: Vec<_> = value
            .split(',')
            .map(str::trim)
            .filter(|angle| !angle.is_empty())
            .map(str::to_owned)
            .collect();
        if !angles.is_empty() {
            return Some(angles);
        }
        tracing::warn!("DEVFLOW_REVIEW_ANGLES contains no review angles; using devflow.toml");
    }
    load_config(project_root).review_angles
}

/// Resolve external verification with `DEVFLOW_EXTERNAL_VERIFY_ENABLED`
/// taking precedence over `devflow.toml` and the built-in default.
pub fn external_verify_enabled(project_root: &Path) -> bool {
    if let Some(value) = env_value("DEVFLOW_EXTERNAL_VERIFY_ENABLED") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(
                value,
                %error,
                "invalid DEVFLOW_EXTERNAL_VERIFY_ENABLED; using devflow.toml or default"
            ),
        }
    }
    load_config(project_root).external_verify_enabled
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvOverride(&'static str);

    impl EnvOverride {
        fn set(key: &'static str, value: &str) -> Self {
            // SAFETY: Tests that mutate this process-global variable are
            // serialized by ENV_MUTEX and the guard removes it on drop.
            unsafe { std::env::set_var(key, value) };
            Self(key)
        }
    }

    impl Drop for EnvOverride {
        fn drop(&mut self) {
            // SAFETY: See EnvOverride::set; the same mutex guard is still held.
            unsafe { std::env::remove_var(self.0) };
        }
    }

    #[test]
    fn default_uses_hardcoded_constants() {
        let config = GitFlowConfig::default();
        assert_eq!(config.main, "main");
        assert_eq!(config.develop, "develop");
        assert_eq!(config.feature_prefix, "feature/");
    }

    #[test]
    fn missing_file_uses_devflow_defaults() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(load_config(dir.path()), DevflowConfig::default());
    }

    #[test]
    fn file_overrides_capture_retention_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "capture_retention = 9\n").unwrap();

        assert_eq!(load_config(dir.path()).capture_retention(), 9);
    }

    #[test]
    fn env_overrides_file_capture_retention() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "capture_retention = 9\n").unwrap();
        let _env = EnvOverride::set("DEVFLOW_CAPTURE_RETENTION", "12");

        assert_eq!(capture_retention(dir.path()), 12);
    }

    #[test]
    fn env_overrides_file_review_angles() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("devflow.toml"),
            "review_angles = [\"file angle\"]\n",
        )
        .unwrap();
        let _env = EnvOverride::set("DEVFLOW_REVIEW_ANGLES", "security, docs accuracy");

        assert_eq!(
            review_angles(dir.path()),
            Some(vec!["security".into(), "docs accuracy".into()])
        );
    }

    #[test]
    fn env_overrides_file_external_verification() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("devflow.toml"),
            "external_verify_enabled = false\n",
        )
        .unwrap();
        let _env = EnvOverride::set("DEVFLOW_EXTERNAL_VERIFY_ENABLED", "true");

        assert!(external_verify_enabled(dir.path()));
    }

    #[test]
    fn malformed_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "capture_retention =\n").unwrap();

        assert_eq!(load_config(dir.path()), DevflowConfig::default());
    }
}

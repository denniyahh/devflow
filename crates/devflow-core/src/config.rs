//! DevFlow project configuration.
//!
//! Parses `.devflow.yaml` from the project root. The parser intentionally
//! supports DevFlow's small, stable YAML shape instead of a general YAML grammar.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Project-level configuration read from `.devflow.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub struct Config {
    /// Version management settings.
    #[serde(default)]
    pub version: VersionConfig,

    /// Automation toggles.
    #[serde(default)]
    pub automation: AutomationConfig,

    /// Git flow branch model settings.
    #[serde(default)]
    pub git_flow: GitFlowConfig,
}

/// Version management settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionConfig {
    /// Versioning scheme: "semver" or "calver".
    #[serde(default = "default_scheme")]
    pub scheme: String,

    /// Path to the file containing the version (relative to project root).
    #[serde(default = "default_version_file")]
    pub file: String,

    /// Dotted path to the version field (e.g., "project.version" for pyproject.toml).
    #[serde(default = "default_version_field")]
    pub field: String,

    /// Build number source: "git" (commit count) or "timestamp".
    #[serde(default = "default_build_number")]
    pub build_number: String,
}

/// Automation toggles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AutomationConfig {
    /// Auto-create feature branch on `devflow start`.
    #[serde(default = "default_true")]
    pub auto_branch: bool,

    /// Run verification after agent execution.
    #[serde(default = "default_true")]
    pub auto_verify: bool,

    /// Update docs after verification.
    #[serde(default = "default_true")]
    pub auto_docs: bool,

    /// Which version component to auto-bump: "none", "patch", "minor", or "major".
    #[serde(default = "default_auto_version")]
    pub auto_version: String,

    /// Auto-create release branch + merge. If false, ask first.
    #[serde(default = "default_false")]
    pub auto_ship: bool,

    /// Delete merged branches after shipping.
    #[serde(default = "default_true")]
    pub auto_cleanup: bool,
}

/// Git flow branch model settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GitFlowConfig {
    /// Main/production branch name.
    #[serde(default = "default_main")]
    pub main: String,

    /// Development/integration branch name.
    #[serde(default = "default_develop")]
    pub develop: String,

    /// Prefix for feature branches.
    #[serde(default = "default_feature_prefix")]
    pub feature_prefix: String,
}

fn default_scheme() -> String {
    "semver".into()
}
fn default_version_file() -> String {
    "pyproject.toml".into()
}
fn default_version_field() -> String {
    "project.version".into()
}
fn default_build_number() -> String {
    "git".into()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_main() -> String {
    "main".into()
}
fn default_develop() -> String {
    "develop".into()
}
fn default_feature_prefix() -> String {
    "feature/".into()
}
fn default_auto_version() -> String {
    "patch".into()
}


impl Default for VersionConfig {
    fn default() -> Self {
        VersionConfig {
            scheme: default_scheme(),
            file: default_version_file(),
            field: default_version_field(),
            build_number: default_build_number(),
        }
    }
}

impl Default for AutomationConfig {
    fn default() -> Self {
        AutomationConfig {
            auto_branch: default_true(),
            auto_verify: default_true(),
            auto_docs: default_true(),
            auto_version: default_auto_version(),
            auto_ship: default_false(),
            auto_cleanup: default_true(),
        }
    }
}

impl Default for GitFlowConfig {
    fn default() -> Self {
        GitFlowConfig {
            main: default_main(),
            develop: default_develop(),
            feature_prefix: default_feature_prefix(),
        }
    }
}

impl Config {
    /// Load config from `.devflow.yaml` in the given project root.
    pub fn load(project_root: &Path) -> Result<Config, ConfigError> {
        let path = project_root.join(".devflow.yaml");
        if !path.exists() {
            return Ok(Config::default());
        }

        let contents = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
        parse_config(&contents)
    }

    /// Render this config as DevFlow's canonical YAML shape.
    pub fn to_yaml(&self) -> String {
        format!(
            "version:\n  scheme: {}\n  file: {}\n  field: {}\n  build_number: {}\nautomation:\n  auto_branch: {}\n  auto_verify: {}\n  auto_docs: {}\n  auto_version: {}\n  auto_ship: {}\n  auto_cleanup: {}\ngit_flow:\n  main: {}\n  develop: {}\n  feature_prefix: {}\n",
            self.version.scheme,
            self.version.file,
            self.version.field,
            self.version.build_number,
            self.automation.auto_branch,
            self.automation.auto_verify,
            self.automation.auto_docs,
            self.automation.auto_version,
            self.automation.auto_ship,
            self.automation.auto_cleanup,
            self.git_flow.main,
            self.git_flow.develop,
            self.git_flow.feature_prefix,
        )
    }

    /// Whether the given step should be skipped per automation config.
    pub fn should_skip(&self, step: &crate::state::Step) -> bool {
        match step {
            crate::state::Step::Verifying => !self.automation.auto_verify,
            crate::state::Step::Docsing => !self.automation.auto_docs,
            crate::state::Step::Shipping => false,
            crate::state::Step::Cleaning => !self.automation.auto_cleanup,
            _ => false,
        }
    }
}

/// Errors produced while reading or parsing `.devflow.yaml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Config file could not be read.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// Config file uses an unsupported value.
    #[error("failed to parse config: {0}")]
    Parse(String),
}

fn parse_config(contents: &str) -> Result<Config, ConfigError> {
    let mut config = Config::default();
    let mut section = String::new();

    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or_default();
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && line.trim_end().ends_with(':') {
            section = line.trim().trim_end_matches(':').to_string();
            continue;
        }
        let Some((key, value)) = line.trim().split_once(':') else {
            return Err(ConfigError::Parse(format!("invalid line `{}`", raw_line)));
        };
        let value = clean_value(value);
        match (section.as_str(), key.trim()) {
            ("version", "scheme") => config.version.scheme = value,
            ("version", "file") => config.version.file = value,
            ("version", "field") => config.version.field = value,
            ("version", "build_number") => config.version.build_number = value,
            ("automation", "auto_branch") => config.automation.auto_branch = parse_bool(&value)?,
            ("automation", "auto_verify") => config.automation.auto_verify = parse_bool(&value)?,
            ("automation", "auto_docs") => config.automation.auto_docs = parse_bool(&value)?,
            ("automation", "auto_version") => config.automation.auto_version = value,
            ("automation", "auto_ship") => config.automation.auto_ship = parse_bool(&value)?,
            ("automation", "auto_cleanup") => config.automation.auto_cleanup = parse_bool(&value)?,
            ("git_flow", "main") => config.git_flow.main = value,
            ("git_flow", "develop") => config.git_flow.develop = value,
            ("git_flow", "feature_prefix") => config.git_flow.feature_prefix = value,
            _ => {}
        }
    }

    Ok(config)
}

fn clean_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parse_bool(value: &str) -> Result<bool, ConfigError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(ConfigError::Parse(format!(
            "expected boolean, got `{other}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_devflow_yaml_shape() {
        let config = parse_config(
            "version:\n  file: Cargo.toml\nautomation:\n  auto_verify: false\ngit_flow:\n  develop: dev\n",
        )
        .expect("parse config");
        assert_eq!(config.version.file, "Cargo.toml");
        assert!(!config.automation.auto_verify);
        assert_eq!(config.git_flow.develop, "dev");
    }
}

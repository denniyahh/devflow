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

    /// Auto-advance past the Planning step without pausing for review.
    #[serde(default = "default_false")]
    pub auto_plan: bool,

    /// Shell command for verification (e.g., "cargo test").
    #[serde(default = "default_verify_command")]
    pub verify_command: String,

    /// Shell command for linting (e.g., "cargo clippy -- -D warnings").
    #[serde(default = "default_lint_command")]
    pub lint_command: String,

    /// Shell command for docs generation (e.g., "cargo doc --no-deps").
    #[serde(default = "default_docs_command")]
    pub docs_command: String,

    /// Continue advancing even if verify/lint/docs fails.
    #[serde(default = "default_false")]
    pub continue_on_error: bool,

    /// Auto-commit documentation changes after docs_command succeeds.
    #[serde(default = "default_false")]
    pub docs_auto_commit: bool,
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
fn default_verify_command() -> String {
    "cargo test".into()
}
fn default_lint_command() -> String {
    "cargo clippy -- -D warnings".into()
}
fn default_docs_command() -> String {
    "cargo doc --no-deps 2>&1".into()
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
            auto_plan: default_false(),
            verify_command: default_verify_command(),
            lint_command: default_lint_command(),
            docs_command: default_docs_command(),
            continue_on_error: default_false(),
            docs_auto_commit: default_false(),
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
    /// Auto-detects version file format if not explicitly configured.
    pub fn load(project_root: &Path) -> Result<Config, ConfigError> {
        let path = project_root.join(".devflow.yaml");
        let mut config = if !path.exists() {
            Config::default()
        } else {
            let contents = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
            parse_config(&contents)?
        };
        // Auto-detect version file if using the default (pyproject.toml)
        // or if the configured file doesn't exist
        if config.version.file == "pyproject.toml"
            && !project_root.join(&config.version.file).exists()
        {
            config.version.auto_detect(project_root);
        }
        Ok(config)
    }

    /// Render this config as DevFlow's canonical YAML shape.
    pub fn to_yaml(&self) -> String {
        format!(
            "version:\n  scheme: {}\n  file: {}\n  field: {}\n  build_number: {}\nautomation:\n  auto_branch: {}\n  auto_verify: {}\n  auto_docs: {}\n  auto_version: {}\n  auto_ship: {}\n  auto_cleanup: {}\n  auto_plan: {}\n  verify_command: \"{}\"\n  lint_command: \"{}\"\n  docs_command: \"{}\"\n  continue_on_error: {}\n  docs_auto_commit: {}\ngit_flow:\n  main: {}\n  develop: {}\n  feature_prefix: {}\n",
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
            self.automation.auto_plan,
            self.automation.verify_command,
            self.automation.lint_command,
            self.automation.docs_command,
            self.automation.continue_on_error,
            self.automation.docs_auto_commit,
            self.git_flow.main,
            self.git_flow.develop,
            self.git_flow.feature_prefix,
        )
    }

    /// Whether the given step should be skipped per automation config.
    pub fn should_skip(&self, step: &crate::state::Step) -> bool {
        match step {
            crate::state::Step::Planning => self.automation.auto_plan,
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
            ("automation", "auto_plan") => config.automation.auto_plan = parse_bool(&value)?,
            ("automation", "verify_command") => config.automation.verify_command = value,
            ("automation", "lint_command") => config.automation.lint_command = value,
            ("automation", "docs_command") => config.automation.docs_command = value,
            ("automation", "continue_on_error") => {
                config.automation.continue_on_error = parse_bool(&value)?
            }
            ("automation", "docs_auto_commit") => {
                config.automation.docs_auto_commit = parse_bool(&value)?
            }
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

impl VersionConfig {
    /// Auto-detect the version file and field from the project root.
    ///
    /// Checks for common project files in order: Cargo.toml, pyproject.toml, package.json.
    pub fn auto_detect(&mut self, project_root: &Path) {
        let cargo = project_root.join("Cargo.toml");
        let pyproject = project_root.join("pyproject.toml");
        let package_json = project_root.join("package.json");

        if cargo.exists() {
            self.file = "Cargo.toml".into();
            // Check for workspace pattern first
            if let Ok(contents) = std::fs::read_to_string(&cargo)
                && contents.contains("[workspace.package]")
            {
                self.field = "workspace.package.version".into();
                return;
            }
            self.field = "package.version".into();
        } else if pyproject.exists() {
            self.file = "pyproject.toml".into();
            self.field = "project.version".into();
        } else if package_json.exists() {
            self.file = "package.json".into();
            self.field = "version".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Step;

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

    #[test]
    fn defaults_match_documented_schema() {
        let config = Config::default();
        assert_eq!(config.version.scheme, "semver");
        assert_eq!(config.version.file, "pyproject.toml");
        assert_eq!(config.version.field, "project.version");
        assert_eq!(config.version.build_number, "git");
        assert!(config.automation.auto_branch);
        assert!(config.automation.auto_verify);
        assert!(config.automation.auto_docs);
        assert_eq!(config.automation.auto_version, "patch");
        assert!(!config.automation.auto_ship);
        assert!(config.automation.auto_cleanup);
        assert_eq!(config.automation.verify_command, "cargo test");
        assert_eq!(
            config.automation.lint_command,
            "cargo clippy -- -D warnings"
        );
        assert_eq!(config.automation.docs_command, "cargo doc --no-deps 2>&1");
        assert!(!config.automation.continue_on_error);
        assert!(!config.automation.docs_auto_commit);
        assert_eq!(config.git_flow.main, "main");
        assert_eq!(config.git_flow.develop, "develop");
        assert_eq!(config.git_flow.feature_prefix, "feature/");
    }

    #[test]
    fn parse_keeps_defaults_for_omitted_fields() {
        let config = parse_config("version:\n  file: Cargo.toml\n").expect("parse");
        // Unspecified fields retain defaults.
        assert_eq!(config.version.scheme, "semver");
        assert_eq!(config.git_flow.main, "main");
        assert_eq!(config.version.file, "Cargo.toml");
    }

    #[test]
    fn parse_strips_comments_and_quotes() {
        let config = parse_config(
            "version:\n  file: \"Cargo.toml\"  # the manifest\n  field: 'package.version'\n",
        )
        .expect("parse");
        assert_eq!(config.version.file, "Cargo.toml");
        assert_eq!(config.version.field, "package.version");
    }

    #[test]
    fn parse_ignores_unknown_keys_and_sections() {
        let config = parse_config("unknown:\n  foo: bar\nversion:\n  mystery: 1\n  file: x.toml\n")
            .expect("parse");
        assert_eq!(config.version.file, "x.toml");
    }

    #[test]
    fn parse_rejects_line_without_colon() {
        let err = parse_config("version:\n  not-a-pair\n").unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    #[test]
    fn parse_rejects_non_boolean_toggle() {
        let err = parse_config("automation:\n  auto_ship: maybe\n").unwrap_err();
        match err {
            ConfigError::Parse(msg) => assert!(msg.contains("boolean")),
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn parse_accepts_all_boolean_toggles() {
        let config = parse_config(
            "automation:\n  auto_branch: false\n  auto_verify: false\n  auto_docs: false\n  auto_ship: true\n  auto_cleanup: false\n",
        )
        .expect("parse");
        assert!(!config.automation.auto_branch);
        assert!(!config.automation.auto_verify);
        assert!(!config.automation.auto_docs);
        assert!(config.automation.auto_ship);
        assert!(!config.automation.auto_cleanup);
    }

    #[test]
    fn parse_new_automation_fields() {
        let config = parse_config(
            "automation:\n  verify_command: \"cargo test --release\"\n  lint_command: \"cargo clippy\"\n  docs_command: \"cargo doc\"\n  continue_on_error: true\n  docs_auto_commit: true\n",
        )
        .expect("parse");
        assert_eq!(config.automation.verify_command, "cargo test --release");
        assert_eq!(config.automation.lint_command, "cargo clippy");
        assert_eq!(config.automation.docs_command, "cargo doc");
        assert!(config.automation.continue_on_error);
        assert!(config.automation.docs_auto_commit);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path()).expect("load");
        assert_eq!(config.version.file, "pyproject.toml");
    }

    #[test]
    fn load_reads_file_from_project_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".devflow.yaml"),
            "version:\n  file: Cargo.toml\n",
        )
        .unwrap();
        let config = Config::load(dir.path()).expect("load");
        assert_eq!(config.version.file, "Cargo.toml");
    }

    #[test]
    fn to_yaml_round_trips_through_parser() {
        let mut original = Config::default();
        original.version.file = "Cargo.toml".into();
        original.automation.auto_ship = true;
        original.git_flow.develop = "trunk".into();

        let yaml = original.to_yaml();
        let parsed = parse_config(&yaml).expect("re-parse rendered yaml");

        assert_eq!(parsed.version.file, "Cargo.toml");
        assert!(parsed.automation.auto_ship);
        assert_eq!(parsed.git_flow.develop, "trunk");
        assert_eq!(parsed.version.scheme, original.version.scheme);
    }

    #[test]
    fn should_skip_follows_automation_toggles() {
        let mut config = Config::default();
        // All enabled by default → nothing skipped except never-skip steps.
        assert!(!config.should_skip(&Step::Verifying));
        assert!(!config.should_skip(&Step::Docsing));
        assert!(!config.should_skip(&Step::Cleaning));
        // Shipping is never auto-skipped.
        assert!(!config.should_skip(&Step::Shipping));
        // Non-optional steps never skip.
        assert!(!config.should_skip(&Step::Idle));
        assert!(!config.should_skip(&Step::Branching));
        assert!(!config.should_skip(&Step::Executing));

        config.automation.auto_verify = false;
        config.automation.auto_docs = false;
        config.automation.auto_cleanup = false;
        assert!(config.should_skip(&Step::Verifying));
        assert!(config.should_skip(&Step::Docsing));
        assert!(config.should_skip(&Step::Cleaning));
        assert!(!config.should_skip(&Step::Shipping));
    }
}

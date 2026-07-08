//! Git-flow branch model.
//!
//! DevFlow has no `.devflow.yaml` and no automation toggles — all behavior is
//! driven by CLI flags (`--mode`, `--agent`, …). The only project configuration
//! left is the git-flow branch model, and that is hardcoded to opinionated
//! constants: `main`, `develop`, and the `feature/` prefix.

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

//! DevFlow project configuration and fixed git-flow branch model.
//!
//! Phase 16 decision D-03 deliberately reopened the earlier no-config-file
//! decision for a minimal `devflow.toml` containing only Phase 16 knobs.
//! `DEVFLOW_*` environment variables remain the highest-precedence overrides.
//! The git-flow branch model remains hardcoded to the opinionated `main`,
//! `develop`, and `feature/` constants below.

use std::path::Path;

/// Number of capture generations retained when not otherwise configured.
///
/// **The number is arithmetic, not a round guess** (ROADMAP criterion 7).
/// `archive_phase_files` runs once per launch and archives the *previous*
/// stage's files, so a clean five-stage Define→Plan→Code→Validate→Ship run
/// produces **4** archive events, and each Validate→Code loop-back adds **2**
/// more.
///
/// `12` therefore accommodates a clean run plus four loop-backs **exactly** —
/// 4 + (4 × 2) = 12, with **zero** headroom at four, because the next archive
/// event after the twelfth evicts. The bound that carries actual headroom is
/// **three** loop-backs: 4 + (3 × 2) = 10 ≤ 12. Do not restate this as
/// "survives four loop-backs with headroom"; it does not.
///
/// The prior value of `5` lost Define's capture on the **first** loop-back
/// (event 6 of 6), silently — `prune_history` deletes without an error or a
/// log, so the loss surfaces only when someone goes looking for a capture that
/// is already gone.
///
/// This is criterion 7's "changing the constant" branch, chosen over a
/// run-local `DEVFLOW_CAPTURE_RETENTION` export because the criterion requires
/// the mitigation leave an **inspectable artifact**: a committed source
/// constant is greppable and outlives the run, an exported environment
/// variable is neither. The env and `devflow.toml` overrides are unchanged and
/// still take precedence — they are simply no longer the mitigation.
pub const DEFAULT_CAPTURE_RETENTION: usize = 12;

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
    /// Whether the Ship gate is standing pre-authorized for this project
    /// (D-12, `28-CONTEXT.md`) — the gate's approval is supplied
    /// automatically and attributed in the gate ledger, exactly as if
    /// `--yes-ship` had been typed on every invocation. Deliberately
    /// defaults to `false`, unlike `external_verify_enabled`: an absent
    /// `devflow.toml`, or one that omits this key, must never pre-authorize
    /// a Ship. This is a deliberate reversal of Phase 23's own D-05
    /// (`--yes-ship` was a per-run flag only, never config-persistable, "so
    /// a standing unattended auto-merge can never become the silent
    /// default"). The reversal's stated cost, recorded twice: relaxing this
    /// later is easy, but tightening it after operators depend on a
    /// persisted setting is not. `commands::start` combines this value with
    /// the CLI flag via logical OR rather than replacing it, because the
    /// flag has no negative form — passing `--yes-ship` always wins.
    pub yes_ship: bool,
}

impl Default for DevflowConfig {
    fn default() -> Self {
        Self {
            capture_retention: DEFAULT_CAPTURE_RETENTION,
            review_angles: None,
            external_verify_enabled: true,
            yes_ship: false,
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

    /// Return whether the Ship gate is standing pre-authorized (D-12).
    pub fn yes_ship(&self) -> bool {
        self.yes_ship
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

/// Resolve the Ship gate's standing pre-authorization (D-12) with
/// `DEVFLOW_YES_SHIP` taking precedence over `devflow.toml` and the
/// built-in `false` default. Mirrors `external_verify_enabled`'s resolver
/// shape exactly. Note that this resolver is not the only path by which
/// `state.yes_ship` becomes `true` — `commands::start` also ORs in the
/// `--yes-ship` CLI flag; this function reports only the config/env-derived
/// half of that combination.
pub fn yes_ship(project_root: &Path) -> bool {
    if let Some(value) = env_value("DEVFLOW_YES_SHIP") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(
                value,
                %error,
                "invalid DEVFLOW_YES_SHIP; using devflow.toml or default"
            ),
        }
    }
    load_config(project_root).yes_ship
}

/// Resolve D-11's legacy-launch opt-out (31-04) from
/// `DEVFLOW_CLAUDE_LEGACY_LAUNCH`.
///
/// Environment only, deliberately: D-11 specifies one flag and one environment
/// variable, and nothing else. There is no `devflow.toml` key, so this takes no
/// `project_root` — a standing per-project default for an escape hatch is
/// exactly the "used routinely, erodes what it protects" shape D-11 warns
/// about. `--legacy-claude-launch` supplies the other half, OR-ed in
/// `commands::start` / `pipeline_launch::resume`.
///
/// **The value is PARSED as a bool, not merely tested for presence (W4).** A
/// naive `env::var(..).is_ok()` would make `DEVFLOW_CLAUDE_LEGACY_LAUNCH=false`
/// *enable* the legacy path — an accidental-reach path D-11 forbids. Garbage
/// warns and is ignored rather than enabling; the escape hatch fails CLOSED.
///
/// Read through [`env_value`] with the variable name as a literal, matching
/// [`yes_ship`] and [`external_verify_enabled`]. A const-mediated read compiles
/// and works identically but is INVISIBLE to
/// `doc_check::source_read_env_vars`, which would then pass green while the
/// variable went undocumented — the "by blindness" failure 31-02 recorded.
pub fn claude_legacy_launch() -> bool {
    if let Some(value) = env_value("DEVFLOW_CLAUDE_LEGACY_LAUNCH") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(
                value,
                %error,
                "invalid DEVFLOW_CLAUDE_LEGACY_LAUNCH; the legacy Claude launch stays OFF"
            ),
        }
    }
    false
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

    /// D-12: an absent `devflow.toml` must never pre-authorize a Ship — the
    /// deliberate asymmetry with `external_verify_enabled`'s `true` default.
    #[test]
    fn yes_ship_defaults_to_false() {
        assert!(!DevflowConfig::default().yes_ship());
    }

    /// D-12: no `devflow.toml` present → the resolver falls through to the
    /// built-in `false` default.
    #[test]
    fn yes_ship_missing_file_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!yes_ship(dir.path()));
    }

    /// D-12: `devflow.toml` setting the key `true` → the resolver returns
    /// `true`.
    #[test]
    fn yes_ship_file_sets_true() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_ship = true\n").unwrap();

        assert!(yes_ship(dir.path()));
    }

    /// D-12: `devflow.toml` setting the key `false` → the resolver returns
    /// `false`.
    #[test]
    fn yes_ship_file_sets_false() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_ship = false\n").unwrap();

        assert!(!yes_ship(dir.path()));
    }

    /// D-12: a `devflow.toml` with unrelated keys still loads, and the
    /// resolver returns the default.
    #[test]
    fn yes_ship_unrelated_keys_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "capture_retention = 9\n").unwrap();

        assert!(!yes_ship(dir.path()));
    }

    /// D-12: an unparseable `DEVFLOW_YES_SHIP` value warns and falls back to
    /// the file/default rather than panicking or returning true.
    #[test]
    fn yes_ship_unparseable_env_falls_back_to_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_ship = true\n").unwrap();
        let _env = EnvOverride::set("DEVFLOW_YES_SHIP", "not-a-bool");

        assert!(yes_ship(dir.path()));
    }

    /// D-12: env beats file, matching every sibling resolver.
    #[test]
    fn env_overrides_file_yes_ship() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_ship = false\n").unwrap();
        let _env = EnvOverride::set("DEVFLOW_YES_SHIP", "true");

        assert!(yes_ship(dir.path()));
    }
}

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
    /// Standing authorization mandate for the release-cut executor's whole
    /// sequence, including its irreversible steps (RD-2, RD-4) — granted
    /// once by the operator via `--yes-release`, this key, or
    /// `DEVFLOW_YES_RELEASE`, mirroring `yes_ship`'s three-source
    /// precedent (D-12) exactly. One mandate covers the whole sequence: a
    /// second, narrower flag for the irreversible steps would be a
    /// self-imposed gate no repository rule imposes, which RD-2 forbids.
    ///
    /// Four properties make this a mandate rather than a ledger, each
    /// asserted by a test in this module:
    /// - **Read-only to DevFlow.** No code path here, or anywhere else in
    ///   this crate, writes `devflow.toml` or any file under `.devflow/`
    ///   for release purposes (RD-8) — the structural reason this cannot
    ///   become a progress ledger.
    /// - **Not consumed.** Reading this value does not clear or decrement
    ///   it; it is a standing grant, so re-running is free.
    /// - **A single boolean carrying no progress.** It cannot express
    ///   "step 3 done", so it cannot drift into meaning that.
    /// - **Defaults to false.** An absent `devflow.toml`, or one that
    ///   omits this key, never authorizes a release — relaxing this later
    ///   is easy, tightening it after operators depend on a persisted
    ///   setting is not.
    ///
    /// Deliberately not a field on [`crate::state::State`]: `State` is a
    /// file DevFlow writes, and a release record living there would be
    /// the beginning of exactly the progress ledger this design exists to
    /// avoid.
    pub yes_release: bool,
}

impl Default for DevflowConfig {
    fn default() -> Self {
        Self {
            capture_retention: DEFAULT_CAPTURE_RETENTION,
            review_angles: None,
            external_verify_enabled: true,
            yes_ship: false,
            yes_release: false,
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

    /// Return whether the release-cut executor's whole sequence is
    /// standing pre-authorized (RD-2, RD-4). See the `yes_release` field's
    /// doc comment for the boundary this accessor exposes.
    pub fn yes_release(&self) -> bool {
        self.yes_release
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

/// Resolve the release-cut executor's standing authorization mandate
/// (RD-2, RD-4) with `DEVFLOW_YES_RELEASE` taking precedence over
/// `devflow.toml` and the built-in `false` default. Mirrors `yes_ship`'s
/// resolver shape exactly — same precedence, same fail-soft parse
/// warning, same false default. Unlike `yes_ship`, no CLI flag is ORed in
/// here; that combination happens at the release-cut command's own call
/// site, exactly as `commands::start` ORs in `--yes-ship`.
pub fn yes_release(project_root: &Path) -> bool {
    // RED stub: intentionally wrong so the behavior tests below fail for
    // the intended reason before the GREEN implementation lands.
    todo!("yes_release resolver not yet implemented")
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

    // -- 29-03 Task 1: the release authorization mandate ------------------
    //
    // Mirrors the yes_ship test block above field for field. The two cases
    // that carry the design's weight — independence from yes_ship, and
    // not-consumed-by-reading — are marked below.

    /// RD-8: an absent `devflow.toml`, or one omitting the key, must never
    /// authorize a release — the same asymmetry `yes_ship` established.
    #[test]
    fn yes_release_defaults_to_false() {
        assert!(!DevflowConfig::default().yes_release());
    }

    /// No `devflow.toml` present → the resolver falls through to the
    /// built-in `false` default.
    #[test]
    fn yes_release_missing_file_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!yes_release(dir.path()));
    }

    /// `devflow.toml` setting the key `true` → the resolver returns `true`.
    #[test]
    fn yes_release_file_sets_true() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_release = true\n").unwrap();

        assert!(yes_release(dir.path()));
    }

    /// `devflow.toml` setting the key `false` → the resolver returns
    /// `false`.
    #[test]
    fn yes_release_file_sets_false() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_release = false\n").unwrap();

        assert!(!yes_release(dir.path()));
    }

    /// A `devflow.toml` with only unrelated keys still loads, and the
    /// resolver returns the default.
    #[test]
    fn yes_release_unrelated_keys_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "capture_retention = 9\n").unwrap();

        assert!(!yes_release(dir.path()));
    }

    /// `DEVFLOW_YES_RELEASE=true` overrides a file value of `false`.
    #[test]
    fn env_overrides_file_yes_release() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_release = false\n").unwrap();
        let _env = EnvOverride::set("DEVFLOW_YES_RELEASE", "true");

        assert!(yes_release(dir.path()));
    }

    /// An unparseable `DEVFLOW_YES_RELEASE` value warns and falls back to
    /// the file value rather than panicking.
    #[test]
    fn yes_release_unparseable_env_falls_back_to_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_release = true\n").unwrap();
        let _env = EnvOverride::set("DEVFLOW_YES_RELEASE", "not-a-bool");

        assert!(yes_release(dir.path()));
    }

    /// Design weight #1: the two mandates are independent. Setting
    /// `yes_release` to `true` leaves `yes_ship` at its own resolved value
    /// (`false`, the default), and vice versa is exercised implicitly by
    /// every other `yes_ship` test in this file continuing to pass
    /// unaffected by this module's new field.
    #[test]
    fn yes_release_and_yes_ship_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("devflow.toml"), "yes_release = true\n").unwrap();

        assert!(yes_release(dir.path()));
        assert!(!yes_ship(dir.path()));
    }

    /// Design weight #2: reading the mandate does not consume it. Calling
    /// `yes_release(dir)` twice returns the same value both times, and
    /// `devflow.toml`'s bytes are unchanged afterward — proving the config
    /// module only reads this file for release purposes, never writes it.
    #[test]
    fn yes_release_reading_twice_does_not_consume_or_mutate_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("devflow.toml");
        let original_contents = "yes_release = true\n";
        std::fs::write(&config_path, original_contents).unwrap();

        let first = yes_release(dir.path());
        let second = yes_release(dir.path());

        assert!(first);
        assert_eq!(first, second);
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            original_contents
        );
    }
}

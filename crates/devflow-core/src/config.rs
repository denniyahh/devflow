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
    /// The branch this project's DevFlow phase lifecycle integrates on
    /// (45-01 decision D-01, AUTO-01). `None` means the built-in [`DEVELOP`]
    /// constant, which is what every project resolved before this key
    /// existed.
    ///
    /// **This is the project's whole integration trunk, not merely a
    /// worktree start point.** A phase worktree forks FROM this branch and
    /// the git-flow lifecycle merges back INTO it; both resolve from this
    /// one value via [`git_flow_for_project`], so they can never disagree.
    /// The rejected alternative — a separate start-point key with `develop`
    /// still the merge target — produces a feature branch forked from a
    /// personal branch and merged into `develop`, dragging unrelated history
    /// into the integration branch.
    ///
    /// Its reason for existing: `preflight_unattended_launch_check` reads
    /// `.planning/config.json` from the WORKTREE, and a worktree forked from
    /// `develop` does not carry a `.planning/` that lives only on a planning
    /// branch — so the unattended check refused every launch (999.110).
    pub base_branch: Option<String>,
}

/// Where a resolved base branch value came from.
///
/// Not decoration: `commands::start` names the source in its operator note,
/// and the local-branch existence check is scoped to the two non-`Default`
/// arms so the default path's existing fall-open behaviour is untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseBranchSource {
    /// Supplied by the `DEVFLOW_BASE_BRANCH` environment variable.
    Env,
    /// Supplied by the `base_branch` key in `devflow.toml`.
    ConfigFile,
    /// Neither was set; the built-in [`DEVELOP`] constant.
    Default,
}

/// A resolved base branch and the provenance of its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBaseBranch {
    /// The branch name.
    pub value: String,
    /// Where the value came from.
    pub source: BaseBranchSource,
}

impl Default for DevflowConfig {
    fn default() -> Self {
        Self {
            capture_retention: DEFAULT_CAPTURE_RETENTION,
            review_angles: None,
            external_verify_enabled: true,
            yes_ship: false,
            base_branch: None,
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

/// The TOML parse error for `<project_root>/devflow.toml`, if the file both
/// exists and fails to parse.
///
/// [`load_config`] deliberately swallows that error — every other key it
/// carries is fail-soft. [`base_branch`] is not, so it needs the error back
/// to honour its own fail-hard contract. Returns `None` for a missing file
/// (no configuration is a valid state) and for an unreadable one (that is
/// already `load_config`'s warn-and-default path, and is not evidence that a
/// base branch was configured).
fn config_parse_error(project_root: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(project_root.join("devflow.toml")).ok()?;
    toml::from_str::<DevflowConfig>(&contents)
        .err()
        .map(|error| error.to_string())
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

/// Reject a base-branch value that must never reach a `git` argv or become
/// this project's integration trunk (45-01, D-01).
///
/// Three refusals, each with its own reason:
///
/// - Equal to [`MAIN`]: making the trunk configurable creates a new way to
///   point an unattended phase run at the production branch, forking from and
///   merging into it while bypassing the release path entirely.
/// - Empty or entirely whitespace: not a branch name, and a blank positional
///   argument to `git worktree add` means something else again.
/// - First byte `-`: a flag-shaped value in an argv position is argument
///   injection (T-45-03).
///
/// Every message names the offending value and its reason, and contains no
/// absolute filesystem path and no host username — the WR-02 / 999.10
/// convention documented at `preflight.rs`'s own message helpers.
pub fn validate_base_branch(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("base branch is empty — set it to a branch name or remove the setting".into());
    }
    if value.starts_with('-') {
        return Err(format!(
            "base branch `{value}` begins with `-`; a flag-shaped value reaches `git` as an \
             option rather than a branch and is refused"
        ));
    }
    if value == MAIN {
        return Err(format!(
            "base branch `{value}` is the production branch; DevFlow refuses to fork phase \
             worktrees from it or merge phase work into it, because that bypasses the release \
             path. Use `{DEVELOP}` or a planning branch instead"
        ));
    }
    Ok(())
}

/// Resolve this project's DevFlow integration trunk (45-01 decision D-01,
/// AUTO-01), with `DEVFLOW_BASE_BRANCH` taking precedence over
/// `devflow.toml`'s `base_branch` key and the built-in [`DEVELOP`] default.
///
/// **This resolver is deliberately FAIL-HARD on an explicitly supplied
/// value, unlike its [`yes_ship`] sibling — do not "fix" it to match.** A bad
/// `yes_ship` bool falls back to `false`, which is the SAFER direction. A bad
/// base branch falling back to [`DEVELOP`] would silently redirect the trunk
/// to a value the operator did not ask for, and it would make this phase's
/// own `main`-refusal unobservable for the most direct way to configure it:
/// an operator writing the production branch name into `devflow.toml` would
/// get a `tracing::warn!` nobody reads and a silent substitution, and the
/// refusal naming the offending branch would never be emitted. Review round 2
/// found exactly that hole in an earlier fail-soft shape of this function.
///
/// Only the [`BaseBranchSource::Default`] arm — no environment variable, no
/// key — is infallible, and it always yields [`DEVELOP`].
///
/// The variable name is written as a **string literal** rather than through a
/// const: a const-mediated read is invisible to
/// `doc_check::source_read_env_vars`, which would then pass green while the
/// variable went undocumented (the failure recorded in
/// [`claude_legacy_launch`]'s doc comment).
pub fn base_branch(project_root: &Path) -> Result<ResolvedBaseBranch, String> {
    if let Some(value) = env_value("DEVFLOW_BASE_BRANCH") {
        return validate_base_branch(&value)
            .map_err(|reason| format!("DEVFLOW_BASE_BRANCH: {reason}"))
            .map(|()| ResolvedBaseBranch {
                value,
                source: BaseBranchSource::Env,
            });
    }
    // The fail-hard contract above covers a *parseable* bad value only.
    // `load_config` is fail-soft by design for every other key, so a
    // `devflow.toml` that does not parse — an unterminated string, a stray
    // bracket — hands back `DevflowConfig::default()`, whose `base_branch`
    // is `None`. That is indistinguishable here from "no key configured",
    // so the resolver would return the `Default`/`develop` arm with no
    // error: exactly the silent trunk redirect this function's doc comment
    // says it refuses, reached through the file rather than through the
    // value. A configured base that cannot be read is refused, not guessed.
    config_parse_error(project_root)
        .map_or(Ok(()), |error| Err(format!("devflow.toml: {error}")))?;
    if let Some(value) = load_config(project_root).base_branch {
        return validate_base_branch(&value)
            .map_err(|reason| format!("devflow.toml `base_branch`: {reason}"))
            .map(|()| ResolvedBaseBranch {
                value,
                source: BaseBranchSource::ConfigFile,
            });
    }
    Ok(ResolvedBaseBranch {
        value: DEVELOP.to_string(),
        source: BaseBranchSource::Default,
    })
}

/// The project's [`GitFlowConfig`] — identical to [`GitFlowConfig::default`]
/// except that `develop` is the resolved [`base_branch`]. This is the single
/// place the trunk substitution happens, which is what keeps the branch a
/// phase worktree forks FROM identical to the branch the lifecycle merges
/// INTO.
///
/// Returns a plain `GitFlowConfig` rather than a `Result` because it is
/// called from many non-CLI sites (hooks, the monitor, ship evidence) that
/// have no error channel; on a resolver `Err` it logs and returns the
/// defaults. **That fallback is not a hole only because `commands::start`
/// refuses on the same `Err` before any git mutation**, so no run reaches
/// those sites with an invalid configuration. If that refusal is ever
/// removed, this fallback becomes one.
pub fn git_flow_for_project(project_root: &Path) -> GitFlowConfig {
    match base_branch(project_root) {
        Ok(resolved) => GitFlowConfig {
            develop: resolved.value,
            ..GitFlowConfig::default()
        },
        Err(error) => {
            tracing::warn!(
                %error,
                "invalid base branch configuration; using the built-in git-flow defaults"
            );
            GitFlowConfig::default()
        }
    }
}

/// The trunk model for a run that already recorded its base at `start`.
///
/// CR-02 (45-REVIEW.md): [`git_flow_for_project`] re-resolves from ambient
/// configuration every time it is called, but `DEVFLOW_BASE_BRANCH` lives in
/// the environment of whichever shell ran `devflow start`. A monitor death
/// followed by the documented `devflow resume` recovery — from a fresh shell
/// without the export — resolved `develop` instead, merged the phase branch
/// there, and confirmed success against the wrong branch.
///
/// The value `start` resolved is persisted on `State::base_branch`, so prefer
/// it. Fall back to the resolver only when there is nothing persisted, which
/// is both "nothing configured" and "state written before the field existed".
///
/// Takes the persisted value rather than reading it from a `&State` so
/// `devflow-core`'s config layer stays independent of the state layer.
pub fn git_flow_for_run(project_root: &Path, persisted_base: Option<&str>) -> GitFlowConfig {
    match persisted_base {
        // `State` is a JSON file on disk, so the persisted value is not
        // automatically the one `start` validated: a hand-edited or
        // truncated `.devflow/state-NN.json` can carry anything. Re-checking
        // it here keeps `start`'s production-branch refusal from being
        // bypassable by editing state, and costs one string comparison on a
        // path that already shells out to `git`.
        Some(base) if validate_base_branch(base).is_ok() => GitFlowConfig {
            develop: base.to_string(),
            ..GitFlowConfig::default()
        },
        Some(base) => {
            tracing::warn!(
                base,
                "persisted base branch is not a usable trunk; re-resolving from configuration"
            );
            git_flow_for_project(project_root)
        }
        None => git_flow_for_project(project_root),
    }
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

        /// Remove an ambient value for the duration of a test.
        ///
        /// Needed by the 45-01 base-branch tests: unlike every earlier
        /// resolver here, `base_branch` has a `Default` arm whose whole
        /// contract is "no env var, no key". A developer with
        /// `DEVFLOW_BASE_BRANCH` exported in their shell would otherwise see
        /// the zero-regression control pass or fail for reasons unrelated to
        /// the code. Drop removes the variable, which is the correct final
        /// state for a test process that never legitimately owns one.
        fn clear(key: &'static str) -> Self {
            // SAFETY: See EnvOverride::set; the same mutex guard is held.
            unsafe { std::env::remove_var(key) };
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

    // -----------------------------------------------------------------
    // 45-01 / D-01 / AUTO-01: configurable base branch resolution.
    // -----------------------------------------------------------------

    /// THE NEGATIVE CONTROL FOR THE WHOLE PHASE: with no `devflow.toml` and
    /// no `DEVFLOW_BASE_BRANCH`, every existing project must resolve exactly
    /// what it resolved before this key existed. If this test breaks, the
    /// change silently moved every project's trunk.
    #[test]
    fn base_branch_defaults_to_develop_with_no_config() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // The guard removes any ambient value for the duration of the test.
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");

        let resolved = base_branch(dir.path()).expect("default resolution is infallible");
        assert_eq!(resolved.value, DEVELOP);
        assert_eq!(resolved.source, BaseBranchSource::Default);
    }

    #[test]
    fn base_branch_reads_devflow_toml() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();

        let resolved = base_branch(dir.path()).expect("a valid file value resolves");
        assert_eq!(resolved.value, "workspace/example");
        assert_eq!(resolved.source, BaseBranchSource::ConfigFile);
    }

    #[test]
    fn base_branch_env_beats_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        let _env = EnvOverride::set("DEVFLOW_BASE_BRANCH", "other/branch");

        let resolved = base_branch(dir.path()).expect("a valid env value resolves");
        assert_eq!(resolved.value, "other/branch");
        assert_eq!(resolved.source, BaseBranchSource::Env);
    }

    /// `env_value`'s documented empty-string filter: an exported-but-empty
    /// variable is not a configuration, so the file value still wins.
    #[test]
    fn base_branch_empty_env_falls_through_to_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        let _env = EnvOverride::set("DEVFLOW_BASE_BRANCH", "");

        let resolved = base_branch(dir.path()).expect("a valid file value resolves");
        assert_eq!(resolved.value, "workspace/example");
        assert_eq!(resolved.source, BaseBranchSource::ConfigFile);
    }

    /// THE POINT OF THE FALLIBLE RESOLVER (review round 2). A resolver that
    /// warned and fell through to `DEVELOP` would make this plan's
    /// `main`-refusal truth unobservable for the most direct way to
    /// configure it: the operator would get a `tracing::warn!` nobody reads
    /// and a silent trunk substitution. Assert on the resolver's OWN return
    /// value, not on a later refusal in `commands::start`.
    #[test]
    fn base_branch_errors_on_an_explicitly_configured_production_branch() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        std::fs::write(
            dir.path().join("devflow.toml"),
            format!("base_branch = \"{MAIN}\"\n"),
        )
        .unwrap();

        let err = base_branch(dir.path()).expect_err("the production branch is refused");
        assert!(
            err.contains(MAIN),
            "error must name the offending value: {err}"
        );
        assert!(
            err.contains("devflow.toml"),
            "error must identify the config file as the source: {err}"
        );

        // The same value through the environment instead. The source
        // attribution is asserted independently: an error naming the file
        // while the value came from the environment sends the operator to
        // the wrong place.
        let env = EnvOverride::set("DEVFLOW_BASE_BRANCH", MAIN);
        let err = base_branch(dir.path()).expect_err("the production branch is refused");
        assert!(
            err.contains(MAIN),
            "error must name the offending value: {err}"
        );
        assert!(
            err.contains("DEVFLOW_BASE_BRANCH"),
            "error must identify the environment variable as the source: {err}"
        );
        drop(env);

        // NEGATIVE CONTROL: with neither source set the resolver is `Ok`,
        // proving the refusal is about the VALUE and not about the presence
        // of a config file or an environment variable.
        std::fs::remove_file(dir.path().join("devflow.toml")).unwrap();
        let resolved = base_branch(dir.path()).expect("neither source set means no refusal");
        assert_eq!(resolved.value, DEVELOP);
        assert_eq!(resolved.source, BaseBranchSource::Default);
    }

    #[test]
    fn base_branch_errors_on_an_explicitly_configured_blank_or_flag_shaped_value() {
        let _lock = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        std::fs::write(dir.path().join("devflow.toml"), "base_branch = \"   \"\n").unwrap();
        assert!(
            base_branch(dir.path()).is_err(),
            "a whitespace-only file value must not fall back"
        );

        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"--upload-pack=touch /tmp/x\"\n",
        )
        .unwrap();
        assert!(
            base_branch(dir.path()).is_err(),
            "a flag-shaped file value must not fall back"
        );

        drop(_env);
        let clean = tempfile::tempdir().unwrap();
        let _env = EnvOverride::set("DEVFLOW_BASE_BRANCH", "--upload-pack=touch /tmp/x");
        assert!(
            base_branch(clean.path()).is_err(),
            "a flag-shaped env value must not fall back"
        );
    }

    /// NEGATIVE CONTROL is the whole test: `main` and `feature_prefix` must
    /// NOT move. Only the trunk is substituted.
    #[test]
    fn git_flow_for_project_replaces_develop_only() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();

        let config = git_flow_for_project(dir.path());
        assert_eq!(config.develop, "workspace/example");
        assert_eq!(config.main, MAIN);
        assert_eq!(config.feature_prefix, FEATURE_PREFIX);
    }

    /// CR-02 (45-REVIEW.md): a run's persisted base outranks whatever the
    /// CURRENT process would resolve, because the process that merges the
    /// phase branch is not the process that read `DEVFLOW_BASE_BRANCH`.
    #[test]
    fn git_flow_for_run_prefers_the_persisted_base_over_ambient_config() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        // Ambient configuration deliberately disagrees with the persisted
        // value, so the two sources are distinguishable.
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/from-file\"\n",
        )
        .unwrap();

        let persisted = git_flow_for_run(dir.path(), Some("workspace/persisted"));
        assert_eq!(persisted.develop, "workspace/persisted");
        assert_eq!(persisted.main, MAIN);
        assert_eq!(persisted.feature_prefix, FEATURE_PREFIX);

        // NEGATIVE CONTROL: the SAME call on the SAME root with nothing
        // persisted must fall through to the resolver and pick up the file
        // value. Without this half the test also passes against a function
        // that ignores the project root entirely, and against one that
        // simply echoes whatever it is handed.
        let ambient = git_flow_for_run(dir.path(), None);
        assert_eq!(ambient.develop, "workspace/from-file");
    }

    /// agy (external review, 2026-09-02): `base_branch`'s doc comment
    /// promises it is FAIL-HARD on an explicitly supplied value, and that
    /// promise held only for a value TOML could parse. A `devflow.toml` with
    /// a syntax error fell into `load_config`'s warn-and-default path, whose
    /// `base_branch: None` is indistinguishable here from "no key set" — so
    /// the resolver returned the `Default`/`develop` arm and the operator's
    /// configured trunk was silently substituted, which is exactly the hole
    /// review round 2 closed for the value and left open for the file.
    #[test]
    fn base_branch_refuses_an_unparseable_config_rather_than_defaulting() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        // An unterminated string: the key is present and clearly intended,
        // but the file does not parse.
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/example\n",
        )
        .unwrap();

        let err = base_branch(dir.path()).expect_err("an unparseable config must not fall back");
        assert!(
            err.contains("devflow.toml"),
            "message must name the file the operator has to fix: {err}"
        );

        // NEGATIVE CONTROL 1: the same key in a file that DOES parse must
        // still resolve. Without this the test also passes against a
        // `base_branch` that refuses every configured value outright.
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        let resolved = base_branch(dir.path()).expect("a parseable config still resolves");
        assert_eq!(resolved.value, "workspace/example");
        assert_eq!(resolved.source, BaseBranchSource::ConfigFile);

        // NEGATIVE CONTROL 2: no file at all is still the infallible
        // `Default` arm, not an error. Without this the test also passes
        // against a `base_branch` that treats any unreadable file as fatal.
        std::fs::remove_file(dir.path().join("devflow.toml")).unwrap();
        let defaulted = base_branch(dir.path()).expect("a missing config is not an error");
        assert_eq!(defaulted.value, DEVELOP);
        assert_eq!(defaulted.source, BaseBranchSource::Default);
    }

    /// codex (external review, 2026-09-02, CR-45-02): `State` is a JSON file
    /// on disk, so a persisted base is not automatically one `start`
    /// validated. Trusting it verbatim made `start`'s production-branch
    /// refusal bypassable by editing `.devflow/state-NN.json`, which then
    /// merged the phase branch into `main`.
    #[test]
    fn git_flow_for_run_refuses_a_persisted_base_that_start_would_have_rejected() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let _env = EnvOverride::clear("DEVFLOW_BASE_BRANCH");
        std::fs::write(
            dir.path().join("devflow.toml"),
            "base_branch = \"workspace/from-file\"\n",
        )
        .unwrap();

        // The production branch is what `validate_base_branch` exists to
        // refuse, and the merge target is the consequence that matters.
        let hijacked = git_flow_for_run(dir.path(), Some(MAIN));
        assert_ne!(
            hijacked.develop, MAIN,
            "a persisted `main` must never become the merge target"
        );
        assert_eq!(
            hijacked.develop, "workspace/from-file",
            "a refused persisted base falls back to the resolver, not to a hardcoded trunk"
        );

        // A flag-shaped value is argument injection (T-45-03) and is
        // refused on the same path.
        assert_eq!(
            git_flow_for_run(dir.path(), Some("--upload-pack=touch /tmp/pwn")).develop,
            "workspace/from-file"
        );

        // NEGATIVE CONTROL: a LEGITIMATE persisted base must still win over
        // ambient configuration. Without this half the test also passes
        // against a `git_flow_for_run` that ignores the persisted value
        // entirely — which would re-open CR-02, the defect the parameter
        // was added to fix.
        assert_eq!(
            git_flow_for_run(dir.path(), Some("workspace/persisted")).develop,
            "workspace/persisted"
        );
    }

    #[test]
    fn validate_base_branch_refuses_main() {
        let err = validate_base_branch(MAIN).expect_err("the production branch is refused");
        assert!(err.contains(MAIN), "message must name the branch: {err}");
        // WR-02 / 999.10: no absolute host path, no derived username.
        assert!(!err.contains("/home"), "message leaked a host path: {err}");
        assert!(!err.contains("/Users"), "message leaked a host path: {err}");
        if let Ok(user) = std::env::var("USER")
            && !user.is_empty()
        {
            assert!(
                !err.contains(&user),
                "message leaked the host username: {err}"
            );
        }
    }

    #[test]
    fn validate_base_branch_refuses_flag_shaped_and_blank() {
        assert!(validate_base_branch("--upload-pack=x").is_err());
        assert!(validate_base_branch("-x").is_err());
        assert!(validate_base_branch("").is_err());
        assert!(validate_base_branch("   ").is_err());
        // NEGATIVE CONTROL: without this the validator could reject
        // everything and every `Err` assertion above would still pass.
        assert!(validate_base_branch("workspace/example").is_ok());
        assert!(validate_base_branch(DEVELOP).is_ok());
    }
}

//! The single writer of GSD's `.planning/config.json` in this codebase.
//!
//! **DevFlow does not own this file.** Its other keys belong to GSD and to the
//! operator, and a live GSD process may read it at any moment during a run.
//! This module therefore acquires exactly ONE key of write authority —
//! `workflow._auto_chain_active`, the flag GSD's `checkpoint_handling` consults
//! to decide whether an ordinary `gate="blocking"` checkpoint may be
//! auto-approved — and must leave every other key, its position, and its
//! serialized form byte-identical (35.1-01, T-35.1-02).
//!
//! Three properties follow from that, and each is pinned by a test rather than
//! left to inspection:
//!
//! 1. **No typed struct.** The file is parsed as a bare [`serde_json::Value`].
//!    A typed `Config`/`Workflow` round trip would silently DROP the keys this
//!    crate does not model (`commit_docs`, `git`, `intel`, `review`,
//!    `model_overrides`, `mempalace`), which is precisely the contract this
//!    module exists to honour.
//! 2. **Key order is preserved.** `serde_json`'s `preserve_order` feature is
//!    enabled in the workspace `Cargo.toml` for this module's sake; without it
//!    every write re-sorts the top-level keys alphabetically and turns a
//!    tracked file into a full-file diff on every stage launch.
//! 3. **Writing a value the file already holds is a no-op.** The file's bytes
//!    and mtime are left untouched, so an INELIGIBLE stage launch — which
//!    actively asserts `false` rather than merely leaving the file alone — does
//!    not dirty a tracked file every time it runs (35.1-01 F-3).
//!
//! Reads are defensive (ASVS V5): a shape this module does not recognise
//! yields the inactive default, never a panic, and a malformed or absent file
//! is an `Err` the caller can log and skip rather than an abort that would kill
//! a long unattended run.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// The key this module owns, and the only one it may write.
const AUTO_CHAIN_KEY: &str = "_auto_chain_active";
/// The object that key lives under.
const WORKFLOW_KEY: &str = "workflow";

/// Errors produced while reading or writing GSD's project config.
///
/// Shape follows [`crate::workflow::WorkflowError`] — `Io`/`Json` `#[from]`
/// variants so every call site converts with `?`, plus a named variant for the
/// "there is no file at all" case so the message says which path was missing
/// instead of surfacing a bare `NotFound`.
#[derive(Debug, thiserror::Error)]
pub enum GsdConfigError {
    /// Filesystem operation failed.
    #[error("GSD config I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("GSD config JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// No GSD config exists at the expected path.
    #[error("no GSD config at {0}")]
    Missing(PathBuf),
}

/// Path of the GSD project config under a project (or worktree) root.
///
/// `root` is the directory whose `.planning/` is the tracked, committed one —
/// in worktree mode that is the WORKTREE, not the main checkout, because the
/// worktree copy is the one the agent's `check auto-mode` actually reads.
#[must_use]
pub fn config_path(root: &Path) -> PathBuf {
    root.join(".planning").join("config.json")
}

/// Read the config file into a [`Value`], distinguishing "absent" from
/// "malformed" so a caller can tell a project that never had GSD config from
/// one whose config it must not touch.
fn read_config(path: &Path) -> Result<Value, GsdConfigError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(GsdConfigError::Missing(path.to_path_buf()));
        }
        Err(err) => return Err(GsdConfigError::Io(err)),
    };
    Ok(serde_json::from_str(&contents)?)
}

/// Whether `workflow._auto_chain_active` is currently set under `root`.
///
/// Deliberately mirrors GSD's own defensive-default idiom
/// (`check-command-router.cjs:95-111`): any shape this module does not
/// recognise — no `workflow` object, a `workflow` that is not an object, a
/// non-boolean value — reads as the inactive default. **Never index**; indexing
/// a missing key panics, and a panic here would kill an unattended run over a
/// hand-edited config file (ASVS V5).
///
/// # Errors
///
/// Returns [`GsdConfigError::Missing`] when no config file exists and
/// [`GsdConfigError::Json`] when the file is not valid JSON. A file that parses
/// but lacks the key is NOT an error — that is the defensive default above.
pub fn auto_chain_active(root: &Path) -> Result<bool, GsdConfigError> {
    let value = read_config(&config_path(root))?;
    Ok(read_flag(&value))
}

/// The defensive read, factored out so the write path uses the exact same
/// interpretation it will later be asserted against.
fn read_flag(value: &Value) -> bool {
    value
        .get(WORKFLOW_KEY)
        .and_then(|workflow| workflow.get(AUTO_CHAIN_KEY))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Set `workflow._auto_chain_active` under `root`, returning whether the file
/// was actually changed.
///
/// Writing a value the file already holds is a genuine no-op: `Ok(false)` is
/// returned and the file's bytes and mtime are untouched (F-3). That is what
/// keeps the symmetric guard — which asserts `false` on every ineligible launch
/// rather than leaving whatever it finds — from rewriting a tracked file on
/// every stage of every run.
///
/// A missing `workflow` object is CREATED rather than rejected; a config that
/// simply has not grown that key yet is a normal shape, not a corrupt one.
///
/// # Errors
///
/// Returns [`GsdConfigError::Missing`] when no config file exists,
/// [`GsdConfigError::Json`] when the existing file is not valid JSON, and
/// [`GsdConfigError::Io`] when the atomic write fails. In every error case the
/// original file is left exactly as it was — the temp-write-then-`rename`
/// idiom means a failure never leaves a truncated config behind.
pub fn set_auto_chain_active(root: &Path, active: bool) -> Result<bool, GsdConfigError> {
    let path = config_path(root);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(GsdConfigError::Missing(path));
        }
        Err(err) => return Err(GsdConfigError::Io(err)),
    };
    let mut value: Value = serde_json::from_str(&raw)?;

    if read_flag(&value) == active {
        return Ok(false);
    }

    // Insert the `workflow` object when it is absent OR is not an object at
    // all. Replacing a non-object `workflow` is the only case in which this
    // module overwrites something it does not own, and it is unavoidable: there
    // is nowhere else the key can live, and leaving it would mean silently
    // reporting success while writing nothing.
    if !value.get(WORKFLOW_KEY).is_some_and(Value::is_object) {
        if let Some(map) = value.as_object_mut() {
            map.insert(
                WORKFLOW_KEY.to_string(),
                Value::Object(serde_json::Map::new()),
            );
        } else {
            // The document's root is not an object — there is no `workflow`
            // key to set and never was. Treat it as malformed rather than
            // replacing the operator's file wholesale.
            return Err(GsdConfigError::Json(serde::de::Error::custom(
                "GSD config root is not a JSON object",
            )));
        }
    }
    value[WORKFLOW_KEY][AUTO_CHAIN_KEY] = Value::Bool(active);

    let mut contents = serde_json::to_string_pretty(&value)?;
    // `to_string_pretty` emits no trailing newline; the operator's file has
    // one. Preserving whatever the file already used keeps the diff to the one
    // line this module owns.
    if raw.ends_with('\n') {
        contents.push('\n');
    }
    write_atomic(&path, &contents)?;
    Ok(true)
}

/// What [`force_clear_auto_chain`] actually did, so the CLI call sites can
/// decide whether to be loud without re-deriving any of it.
///
/// Three independent facts, deliberately not collapsed into one enum: a repair
/// can touch the working tree only, the working tree AND the branch tip, or the
/// working tree while explicitly DECLINING the branch tip. The third is not a
/// failure — it is the correct answer when committing would sweep in an edit
/// DevFlow does not own (F-8) — and a call site that could not tell it apart
/// from "nothing happened" would report the deferral as silence.
#[derive(Debug, Default)]
pub struct ClearOutcome {
    /// The file on disk carried a set flag and now does not.
    pub working_tree_repaired: bool,
    /// The branch tip carried a set flag and a commit was made so it no longer
    /// does. Confirmed by re-probing the tip after the commit, never inferred
    /// from `commit_path` returning `Ok`.
    pub committed_tree_repaired: bool,
    /// Why the branch-tip half was NOT attempted or did not land. Populated
    /// whenever the working tree was corrected but the committed copy was left
    /// alone for a reason the operator needs to hear.
    pub commit_refused: Option<String>,
}

impl ClearOutcome {
    /// Whether either half of the repair actually changed something.
    ///
    /// Deliberately excludes `commit_refused`: a refusal is a separate thing to
    /// be loud about, and folding it in here would make "we fixed something"
    /// and "we declined to fix something" indistinguishable at the call site.
    #[must_use]
    pub fn repaired_anything(&self) -> bool {
        self.working_tree_repaired || self.committed_tree_repaired
    }
}

/// The config's path as a git pathspec — relative, because that is what
/// `commit_path` and `git show HEAD:<path>` both need.
const CONFIG_PATHSPEC: &str = ".planning/config.json";

/// Clear `workflow._auto_chain_active` under `root` unconditionally, repairing
/// the branch tip too when that can be done without sweeping in anything else.
///
/// **This is the second, independent mechanism (35.1 D-01).** The in-process
/// [`crate::gsd_config`] guard held by `devflow`'s monitor covers a normal
/// return, a `?` early-return and a panic-unwind — and structurally cannot
/// cover a `SIGKILL`, because `Drop` never runs. A killed monitor therefore
/// leaves a set flag in a TRACKED file, from where `commit_docs` or any
/// sweeping `git add` can carry it onto `develop` and into the next phase's
/// `plan-phase` invocation, where the same boolean no longer means "approve
/// this checkpoint" but "chain into execute-phase". Rather than try to make
/// the guard cover the uncoverable, both launch entry points repair forward.
///
/// Emits nothing and prints nothing: this is `devflow-core` and it returns a
/// report. The operator-facing notice and the `events.jsonl` entry belong to
/// the CLI call sites, which is what lets this function's tests assert on an
/// outcome value rather than on captured stdout.
///
/// # Errors
///
/// Returns [`GsdConfigError::Json`] when the config exists but cannot be
/// parsed — a file DevFlow cannot read cannot be certified clear, so the caller
/// must hear about it. An ABSENT config is not an error: a project with no GSD
/// config has nothing to leak, and failing here would break `devflow start` for
/// every non-GSD project.
pub fn force_clear_auto_chain(root: &Path) -> Result<ClearOutcome, GsdConfigError> {
    let _ = root;
    Ok(ClearOutcome::default())
}

/// Write through a sibling temporary file so a live GSD process never observes
/// a truncated or partially written config (T-35.1-04) — the same idiom
/// [`crate::workflow`] already uses for `.devflow/state-{NN}.json`.
///
/// No parent-directory creation step: `.planning/` necessarily exists by the
/// time this runs, because the read above succeeded from inside it.
fn write_atomic(path: &Path, contents: &str) -> Result<(), GsdConfigError> {
    let tmp = path.with_extension("json.devflow-tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This project's REAL config shape — every top-level key the operator
    /// owns, in the order the file has them, plus `workflow.auto_advance` and
    /// the nested integer `workflow.subagent_timeout`.
    ///
    /// The top-level order is deliberately NOT alphabetical (`workflow` comes
    /// second, `git` third), because an alphabetical fixture would satisfy the
    /// key-order assertion below even under a `BTreeMap`-backed round trip and
    /// prove nothing about `preserve_order`.
    ///
    /// Written in `serde_json::to_string_pretty`'s exact rendering (two-space
    /// indent, one array element per line) so the whole-file byte comparison
    /// below can be an equality rather than a normalized diff.
    const REAL_SHAPE: &str = r#"{
  "commit_docs": true,
  "workflow": {
    "granularity": "medium",
    "auto_mode": true,
    "auto_advance": true,
    "commit_docs": true,
    "subagent_timeout": 300000,
    "_auto_chain_active": false,
    "nyquist_validation": true,
    "tdd_mode": true
  },
  "git": {
    "main": "main",
    "develop": "develop"
  },
  "intel": {
    "enabled": true
  },
  "review": {
    "default_reviewers": [
      "codex"
    ]
  },
  "model_overrides": {
    "gsd-executor": "inherit"
  },
  "mempalace": {
    "enabled": true
  }
}
"#;

    /// Write `contents` as a project's `.planning/config.json` and hand back
    /// the root (the temp dir is returned too, so it outlives the test body).
    fn project(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(config_path(&root), contents).unwrap();
        (dir, root)
    }

    /// [`REAL_SHAPE`] with the one owned value set either way, so a fixture can
    /// carry the leak without a second near-duplicate literal drifting from the
    /// first.
    fn real_shape(active: bool) -> String {
        let replaced = REAL_SHAPE.replace(
            "\"_auto_chain_active\": false",
            &format!("\"_auto_chain_active\": {active}"),
        );
        assert!(
            replaced.contains(&format!("\"_auto_chain_active\": {active}")),
            "the fixture must actually carry the requested flag value"
        );
        replaced
    }

    /// Hermetic git invocation pinned to `root` (999.37) — never a bare
    /// `Command::new(\"git\")`.
    fn git(root: &Path, args: &[&str]) {
        let output = crate::git::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_output(root: &Path, args: &[&str]) -> String {
        let output = crate::git::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// A real temp repository with one commit, so `HEAD` exists — every probe
    /// in `force_clear_auto_chain` is expressed against `HEAD`, and a repo with
    /// no commits would exercise the could-not-certify arm instead of the arm
    /// under test.
    fn git_project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "devflow@example.com"]);
        git(&root, &["config", "user.name", "DevFlow Tests"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        git(&root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "-q", "-m", "base"]);
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        (dir, root)
    }

    fn write_config(root: &Path, contents: &str) {
        std::fs::write(config_path(root), contents).unwrap();
    }

    fn commit_config(root: &Path, message: &str) {
        git(root, &["add", CONFIG_PATHSPEC]);
        git(root, &["commit", "-q", "-m", message]);
    }

    fn head_sha(root: &Path) -> String {
        git_output(root, &["rev-parse", "HEAD"])
    }

    /// The flag as the BRANCH TIP holds it — read out of git, never out of the
    /// working tree. A working-tree read cannot tell "committed the fix" from
    /// "wrote the fix and forgot to commit", which is the exact gap the
    /// committed half of this repair exists to close.
    fn flag_at_head(root: &Path) -> Value {
        let raw = git_output(root, &["show", &format!("HEAD:{CONFIG_PATHSPEC}")]);
        let value: Value = serde_json::from_str(&raw).unwrap();
        // Indexing, not `.get()`: an absent key must raise here rather than
        // quietly render as `false` and agree with a correct cleared result.
        value["workflow"]["_auto_chain_active"].clone()
    }

    /// The common case a killed monitor leaves behind: the leak is on disk but
    /// never reached a commit. The working tree is repaired; the branch tip is
    /// already in agreement afterwards, so nothing is committed.
    #[test]
    fn force_clear_repairs_a_leaked_working_tree_value() {
        let (_dir, root) = git_project();
        write_config(&root, &real_shape(false));
        commit_config(&root, "add gsd config");
        let head_before = head_sha(&root);
        // The leak: written into the working tree after the commit, exactly as
        // a SIGKILLed monitor leaves it.
        write_config(&root, &real_shape(true));

        let outcome = force_clear_auto_chain(&root).unwrap();

        assert!(
            outcome.working_tree_repaired,
            "a set flag on disk must be reported as a working-tree repair"
        );
        assert!(
            !outcome.committed_tree_repaired,
            "the branch tip never carried the leak, so nothing may be committed"
        );
        assert_eq!(outcome.commit_refused, None);
        assert!(
            !auto_chain_active(&root).unwrap(),
            "a subsequent read must see the cleared value"
        );
        assert_eq!(
            head_sha(&root),
            head_before,
            "a working-tree-only repair must not add a commit"
        );
    }

    /// Criterion 2's committed half: when the leak reached `HEAD`, the value
    /// Ship would merge into `develop` is the CLEARED one. Read back out of git
    /// rather than out of the working tree, for the reason [`flag_at_head`]
    /// gives.
    #[test]
    fn force_clear_commits_when_the_leak_reached_head() {
        let (_dir, root) = git_project();
        write_config(&root, &real_shape(true));
        commit_config(&root, "add gsd config carrying the leak");
        assert_eq!(
            flag_at_head(&root),
            Value::Bool(true),
            "the fixture must actually commit the leak, or the assertions below \
             are vacuous"
        );

        let outcome = force_clear_auto_chain(&root).unwrap();

        assert!(outcome.working_tree_repaired);
        assert!(
            outcome.committed_tree_repaired,
            "a leak that reached HEAD must be repaired in the commit too, not \
             only in the working tree — otherwise the branch → merge → develop \
             → next-phase-chains path stays open (35.1 D-01)"
        );
        assert_eq!(outcome.commit_refused, None);
        assert_eq!(flag_at_head(&root), Value::Bool(false));
    }

    /// F-8 / T-35.1-08: `commit_path` is path-scoped but still commits whatever
    /// else is dirty IN that path. This repository has already had an incident
    /// where an in-progress file was swept into an unrelated commit
    /// (`CLAUDE.md`), and the correct posture is to refuse rather than sweep.
    ///
    /// **The `HEAD` comparison is the load-bearing assertion.** Asserting only
    /// on the returned `commit_refused` would pass against an implementation
    /// that committed the operator's edit and then reported a refusal.
    #[test]
    fn force_clear_refuses_to_commit_when_the_file_carries_other_changes() {
        let (_dir, root) = git_project();
        write_config(&root, &real_shape(true));
        commit_config(&root, "add gsd config carrying the leak");
        let head_before = head_sha(&root);
        // An operator edit in flight, in the same file, beyond the one key
        // DevFlow owns.
        write_config(
            &root,
            &real_shape(true).replace("\"granularity\": \"medium\"", "\"granularity\": \"large\""),
        );

        let outcome = force_clear_auto_chain(&root).unwrap();

        assert!(
            outcome.working_tree_repaired,
            "the working-tree clear disarms the bypass for THIS run and must \
             happen even when the commit is declined"
        );
        assert!(
            !outcome.committed_tree_repaired,
            "the branch-tip repair must be deferred, not attempted"
        );
        let reason = outcome
            .commit_refused
            .expect("a declined commit must say why, loudly");
        assert!(
            reason.contains("beyond"),
            "the refusal must name the cause — got: {reason}"
        );
        assert!(
            !auto_chain_active(&root).unwrap(),
            "the working tree is still cleared"
        );
        assert!(
            std::fs::read_to_string(config_path(&root))
                .unwrap()
                .contains("\"granularity\": \"large\""),
            "the operator's in-flight edit must survive untouched"
        );
        assert_eq!(
            head_sha(&root),
            head_before,
            "nothing may be committed — this assertion, not the returned \
             refusal, is what distinguishes a genuine refusal from a commit \
             that reported one"
        );
    }

    /// The no-op control. Without it, an implementation that always reported a
    /// repair — or always committed — would satisfy every test above.
    #[test]
    fn force_clear_on_an_already_clean_config_reports_nothing_and_writes_nothing() {
        let (_dir, root) = git_project();
        write_config(&root, &real_shape(false));
        commit_config(&root, "add a clean gsd config");
        let head_before = head_sha(&root);
        let bytes_before = std::fs::read(config_path(&root)).unwrap();

        let outcome = force_clear_auto_chain(&root).unwrap();

        assert!(!outcome.working_tree_repaired);
        assert!(!outcome.committed_tree_repaired);
        assert_eq!(outcome.commit_refused, None);
        assert!(
            !outcome.repaired_anything(),
            "an ordinary clean launch must have nothing to be loud about"
        );
        assert_eq!(std::fs::read(config_path(&root)).unwrap(), bytes_before);
        assert_eq!(head_sha(&root), head_before);
    }

    /// A project that never had GSD config has nothing to leak. A hard error
    /// here would break `devflow start` for every non-GSD project, which is a
    /// far worse failure than the one this repair prevents.
    #[test]
    fn force_clear_on_a_project_without_a_gsd_config_is_a_clean_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();

        let outcome = force_clear_auto_chain(&root).expect("an absent config is not an error");

        assert!(!outcome.working_tree_repaired);
        assert!(!outcome.committed_tree_repaired);
        assert_eq!(outcome.commit_refused, None);
    }

    /// Absent and malformed are different facts and get different answers: a
    /// file that cannot be parsed cannot be certified clear, so it propagates
    /// rather than silently reading as a clean no-op.
    #[test]
    fn force_clear_on_a_malformed_config_is_an_error() {
        let (_dir, root) = git_project();
        let malformed = "{ \"workflow\": { \"_auto_chain_active\": tru";
        write_config(&root, malformed);

        assert!(matches!(
            force_clear_auto_chain(&root),
            Err(GsdConfigError::Json(_))
        ));
        assert_eq!(
            std::fs::read_to_string(config_path(&root)).unwrap(),
            malformed,
            "a failed certification must leave the operator's file exactly as \
             it was"
        );
    }

    /// A project whose `.planning/` exists but holds no config file.
    fn empty_project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        (dir, root)
    }

    /// Everything except the one key this module owns must survive a write
    /// unchanged — value, position, and serialized form.
    ///
    /// **The assertion is on the file's BYTES, not on a re-parse.** An earlier
    /// version of this test compared `keys()` from two `serde_json::Value`
    /// parses and passed with `preserve_order` REMOVED — because without that
    /// feature both parses go through a `BTreeMap`, so both key lists come out
    /// alphabetized and agree with each other. The comparison normalized away
    /// the exact property it claimed to measure. Comparing raw text is what
    /// makes this discriminating, and it subsumes numeric re-rendering
    /// (`subagent_timeout: 300000`) as well as ordering.
    #[test]
    fn writing_the_flag_leaves_every_other_key_byte_identical() {
        let (_dir, root) = project(REAL_SHAPE);
        let before = std::fs::read_to_string(config_path(&root)).unwrap();

        assert!(set_auto_chain_active(&root, true).unwrap());

        let after = std::fs::read_to_string(config_path(&root)).unwrap();
        let expected = before.replace(
            "\"_auto_chain_active\": false",
            "\"_auto_chain_active\": true",
        );
        assert_ne!(
            expected, before,
            "the fixture must actually contain the key this test flips, or the \
             comparison below is vacuous"
        );
        assert_eq!(
            after, expected,
            "the written file must differ from the original in EXACTLY the one \
             value this module owns — same key order, same number rendering, \
             same whitespace"
        );

        // And the flag really did flip, read back through this module's own
        // accessor rather than by re-reading the text just compared.
        assert!(auto_chain_active(&root).unwrap());
    }

    /// Criterion 3b / D-06: this phase buys checkpoint APPROVAL, not workflow
    /// chaining. Nothing in this codebase may write `workflow.auto_advance`.
    ///
    /// Why it matters concretely: GSD's `check auto-mode` ORs the two flags
    /// (`check-command-router.cjs:107`), so a write that clobbered
    /// `auto_advance` would silently enable the stage-chaining ROADMAP
    /// criterion 3 forbids — and it would do so through a key DevFlow never
    /// intended to touch.
    #[test]
    fn writing_the_flag_never_touches_auto_advance() {
        let (_dir, root) = project(REAL_SHAPE);
        // The fixture has `auto_advance: true` BEFORE the call, deliberately.
        // A fixture where it were `false` would prove nothing: `false` is also
        // what a dropped key deserializes to.
        assert_eq!(
            serde_json::from_str::<Value>(REAL_SHAPE).unwrap()["workflow"]["auto_advance"],
            Value::Bool(true)
        );

        set_auto_chain_active(&root, true).unwrap();
        set_auto_chain_active(&root, false).unwrap();

        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(config_path(&root)).unwrap()).unwrap();
        assert_eq!(
            after["workflow"]["auto_advance"],
            Value::Bool(true),
            "auto_advance is the operator's, and neither setting nor clearing \
             the chain flag may disturb it"
        );
    }

    /// F-3: the ineligible-launch path asserts `false` on every stage launch,
    /// so writing a value the file already holds must cost nothing — otherwise
    /// a tracked file is rewritten on every run of every stage.
    #[test]
    fn setting_the_value_it_already_holds_is_a_no_op() {
        let (_dir, root) = project(REAL_SHAPE);
        let path = config_path(&root);
        let before = std::fs::read(&path).unwrap();
        let before_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();

        // REAL_SHAPE holds `false`; ask for `false`.
        assert!(
            !set_auto_chain_active(&root, false).unwrap(),
            "a write that changes nothing must report that it changed nothing"
        );

        assert_eq!(before, std::fs::read(&path).unwrap());
        assert_eq!(
            before_mtime,
            std::fs::metadata(&path).unwrap().modified().unwrap()
        );

        // Negative control: the same call with the OTHER value must report a
        // change. Without this, a `set_auto_chain_active` that always returned
        // `false` and never wrote would satisfy the assertions above.
        assert!(set_auto_chain_active(&root, true).unwrap());
    }

    /// A config that simply has not grown a `workflow` object yet is a normal
    /// shape, not a corrupt one.
    #[test]
    fn a_missing_workflow_object_is_created_rather_than_rejected() {
        let (_dir, root) = project("{\n  \"commit_docs\": true\n}\n");

        assert!(set_auto_chain_active(&root, true).unwrap());
        assert!(auto_chain_active(&root).unwrap());

        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(config_path(&root)).unwrap()).unwrap();
        assert_eq!(
            after["commit_docs"],
            Value::Bool(true),
            "creating the workflow object must not disturb the keys already there"
        );
    }

    /// ASVS V5: a hand-edited config is untrusted input. It must produce an
    /// `Err` the caller can log and skip, never a panic that would kill a long
    /// unattended run — and the atomic write must not have truncated anything
    /// on the way to failing.
    #[test]
    fn a_malformed_config_is_an_error_not_a_panic() {
        let malformed = "{ \"workflow\": { \"auto_advance\": tru";
        let (_dir, root) = project(malformed);

        // Asserted on the returned Err, never on a caught panic.
        assert!(matches!(
            set_auto_chain_active(&root, true),
            Err(GsdConfigError::Json(_))
        ));
        assert!(matches!(
            auto_chain_active(&root),
            Err(GsdConfigError::Json(_))
        ));
        assert_eq!(
            std::fs::read_to_string(config_path(&root)).unwrap(),
            malformed,
            "a failed write must leave the operator's file exactly as it was"
        );
    }

    /// A JSON document whose root is not an object has nowhere for the key to
    /// live. Refuse rather than replace the operator's file wholesale.
    #[test]
    fn a_non_object_config_root_is_an_error_not_a_replacement() {
        let (_dir, root) = project("[1, 2, 3]\n");

        assert!(matches!(
            set_auto_chain_active(&root, true),
            Err(GsdConfigError::Json(_))
        ));
        assert_eq!(
            std::fs::read_to_string(config_path(&root)).unwrap(),
            "[1, 2, 3]\n"
        );
    }

    /// No file at all is an explicit `Err`, not a silent create — DevFlow does
    /// not own this file and must not conjure one into a project that has no
    /// GSD config.
    #[test]
    fn an_absent_config_is_an_error_not_a_panic() {
        let (_dir, root) = empty_project();

        assert!(matches!(
            set_auto_chain_active(&root, true),
            Err(GsdConfigError::Missing(_))
        ));
        assert!(matches!(
            auto_chain_active(&root),
            Err(GsdConfigError::Missing(_))
        ));
        assert!(
            !config_path(&root).exists(),
            "a failed write must not leave a file behind"
        );
    }

    /// The V5 defensive-default row: three shapes this module does not model,
    /// all of which must read as the INACTIVE value rather than panic.
    ///
    /// Reading via `value["workflow"]["_auto_chain_active"]` would panic on the
    /// first of these; that is exactly the indexing this module forbids on a
    /// read path.
    #[test]
    fn reading_the_flag_defaults_to_the_inactive_value_on_a_shape_it_does_not_recognise() {
        for shape in [
            // `workflow` absent entirely.
            "{ \"commit_docs\": true }",
            // `workflow` present but not an object.
            "{ \"workflow\": \"medium\" }",
            // the key present but not a boolean.
            "{ \"workflow\": { \"_auto_chain_active\": \"true\" } }",
        ] {
            let (_dir, root) = project(shape);
            assert!(
                !auto_chain_active(&root).unwrap(),
                "unrecognised shape must read inactive, not panic: {shape}"
            );
        }

        // Negative control: the shape this module DOES recognise still reads
        // active, so the three assertions above are discriminating rather than
        // a function that always returns `false`.
        let (_dir, root) = project("{ \"workflow\": { \"_auto_chain_active\": true } }");
        assert!(auto_chain_active(&root).unwrap());
    }

    /// The file's trailing-newline convention survives a write, so the diff
    /// stays one line instead of gaining a spurious no-newline-at-EOF marker.
    #[test]
    fn the_trailing_newline_convention_survives_a_write() {
        let (_dir, root) = project(REAL_SHAPE);
        set_auto_chain_active(&root, true).unwrap();
        assert!(
            std::fs::read_to_string(config_path(&root))
                .unwrap()
                .ends_with('\n')
        );

        let without = REAL_SHAPE.trim_end_matches('\n').to_string();
        let (_dir2, root2) = project(&without);
        set_auto_chain_active(&root2, true).unwrap();
        assert!(
            !std::fs::read_to_string(config_path(&root2))
                .unwrap()
                .ends_with('\n')
        );
    }

    /// The write leaves no temporary file behind for a `git add` to sweep up.
    #[test]
    fn the_atomic_write_leaves_no_temp_file_behind() {
        let (_dir, root) = project(REAL_SHAPE);
        set_auto_chain_active(&root, true).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(root.join(".planning"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "config.json")
            .collect();
        assert!(
            leftovers.is_empty(),
            "stray files in .planning: {leftovers:?}"
        );
    }
}

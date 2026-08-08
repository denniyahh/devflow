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

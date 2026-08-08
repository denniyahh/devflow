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

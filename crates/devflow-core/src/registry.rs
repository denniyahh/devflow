//! Machine-global registry of currently-active DevFlow project roots.
//!
//! `Gates::list_open` (`gates.rs`) is scoped to one `project_root`, and every
//! caller inherits that scope — there is nowhere in this codebase that
//! answers "what is DevFlow doing across every project on this machine?"
//! without shelling out to `ps` and `find` (see `23-ORPHAN-FORENSICS.md`).
//! This module is that answer: a `(project_root, phase)` pair is registered
//! on the same code path that already writes `state.monitor_pid`, so a
//! running phase cannot be missing from the registry.
//!
//! **Storage shape (23-03 revision, cross-AI review BLOCKER 4):** one file
//! per `(project_root, phase)` under a `roots/` subdirectory of the cache
//! dir, enumerated with `read_dir`. Registration writes only its own file —
//! there is no load-modify-write step and therefore no lost-update race to
//! defend. A corrupt or truncated entry costs one entry, never the whole
//! registry.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A registered `(project_root, phase)` pair — one DevFlow phase this
/// machine is (or recently was) running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredRoot {
    /// The project's root directory.
    pub project_root: PathBuf,
    /// Phase number registered.
    pub phase: u32,
    /// Unix timestamp (seconds) when this entry was written, as a string —
    /// matches `GateFile.timestamp`'s existing wire shape.
    pub registered_at: String,
}

/// Errors produced by registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Filesystem operation failed.
    #[error("registry I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("registry JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

/// Resolve the DevFlow cache directory. The ONLY env-reading function in
/// this module. Resolution order: `DEVFLOW_CACHE_DIR` (test/override hook),
/// then `XDG_CACHE_HOME/devflow`, then `HOME/.cache/devflow`. Returns `None`
/// when none of the three is set.
///
/// This workspace has no `dirs` crate and must not gain one
/// (`23-RESEARCH.md` Standard Stack: zero new dependencies).
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("DEVFLOW_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(dir).join("devflow"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".cache").join("devflow"))
}

/// The `roots/` subdirectory of a cache dir, where per-registration entry
/// files live.
pub fn roots_dir_in(cache_dir: &Path) -> PathBuf {
    cache_dir.join("roots")
}

/// The deterministic per-registration entry file path for `(project_root,
/// phase)`. The digest is only a filename disambiguator — the authoritative
/// `project_root` lives inside the file itself, and `load_roots_in` reads it
/// from there, so a digest collision costs at most one shadowed entry and
/// never a wrong path.
pub fn entry_path_in(cache_dir: &Path, project_root: &Path, phase: u32) -> PathBuf {
    let digest = path_digest(project_root);
    roots_dir_in(cache_dir).join(format!("{digest:016x}-{phase:02}.json"))
}

/// Inline FNV-1a 64-bit hash over `path`'s OS-string bytes, used only to
/// derive a stable per-entry filename. `std::collections::hash_map::
/// DefaultHasher` is deliberately not used here: its output is explicitly
/// not guaranteed stable across Rust releases, and an unstable filename
/// would orphan every existing entry on a toolchain bump.
fn path_digest(path: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in path.as_os_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Register `(project_root, phase)` into the machine-global registry under
/// `cache_dir`. Pure with respect to env. Creates the roots directory if
/// absent and writes only this registration's own file — there is no load
/// step, no merge step, and no rewrite of any other entry. Re-registering
/// the same pair simply overwrites its own file with a fresh
/// `registered_at`.
pub fn register_in(cache_dir: &Path, project_root: &Path, phase: u32) -> Result<(), RegistryError> {
    let dir = roots_dir_in(cache_dir);
    std::fs::create_dir_all(&dir)?;
    let entry = RegisteredRoot {
        project_root: project_root.to_path_buf(),
        phase,
        registered_at: unix_now(),
    };
    let path = entry_path_in(cache_dir, project_root, phase);
    std::fs::write(&path, serde_json::to_string_pretty(&entry)?)?;
    Ok(())
}

/// Every registered root, sorted by `(project_root, phase)` so output is
/// deterministic (`read_dir` order is not). `read_dir`s the roots
/// directory, parsing each `.json` entry and skipping any that is
/// unreadable or unparsable — exactly as `Gates::list_open` already skips
/// unparsable gate files. Returns an empty `Vec` when the directory is
/// absent. Never returns `Result`; enumeration must degrade, not die.
pub fn load_roots_in(cache_dir: &Path) -> Vec<RegisteredRoot> {
    let mut roots = Vec::new();
    let dir = roots_dir_in(cache_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return roots;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.ends_with(".json") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(root) = serde_json::from_str::<RegisteredRoot>(&contents) else {
            continue;
        };
        roots.push(root);
    }
    roots.sort_by(|a, b| (&a.project_root, a.phase).cmp(&(&b.project_root, b.phase)));
    roots
}

/// Register `(project_root, phase)` into the resolved machine-global cache
/// dir. A silent `Ok(())` no-op when [`cache_dir`] resolves to `None` —
/// registration is best-effort observability, never a reason to fail a
/// launch.
pub fn register(project_root: &Path, phase: u32) -> Result<(), RegistryError> {
    let Some(dir) = cache_dir() else {
        return Ok(());
    };
    register_in(&dir, project_root, phase)
}

/// Every registered root in the resolved machine-global cache dir. An empty
/// `Vec` when [`cache_dir`] resolves to `None`.
pub fn load_roots() -> Vec<RegisteredRoot> {
    let Some(dir) = cache_dir() else {
        return Vec::new();
    };
    load_roots_in(&dir)
}

fn unix_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_in_two_different_pairs_both_survive_and_load_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let root_a = PathBuf::from("/tmp/project-a");
        let root_b = PathBuf::from("/tmp/project-b");

        register_in(cache, &root_a, 5).unwrap();
        register_in(cache, &root_b, 7).unwrap();

        let roots = load_roots_in(cache);
        assert_eq!(roots.len(), 2);
        assert!(
            roots
                .iter()
                .any(|r| r.project_root == root_a && r.phase == 5)
        );
        assert!(
            roots
                .iter()
                .any(|r| r.project_root == root_b && r.phase == 7)
        );
        // Sorted by (project_root, phase).
        assert!(roots[0].project_root <= roots[1].project_root);
    }

    #[test]
    fn register_in_same_root_two_phases_survive_as_distinct_files() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let root = PathBuf::from("/tmp/project-multi-phase");

        register_in(cache, &root, 1).unwrap();
        register_in(cache, &root, 2).unwrap();

        let roots = load_roots_in(cache);
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().any(|r| r.phase == 1));
        assert!(roots.iter().any(|r| r.phase == 2));
    }

    #[test]
    fn load_roots_in_skips_one_corrupt_entry_and_keeps_its_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let root = PathBuf::from("/tmp/project-good");
        register_in(cache, &root, 3).unwrap();

        let junk_path = roots_dir_in(cache).join("junk-entry.json");
        std::fs::write(&junk_path, "{not json").unwrap();

        let roots = load_roots_in(cache);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].project_root, root);
        assert_eq!(roots[0].phase, 3);
    }

    #[test]
    fn load_roots_in_on_absent_directory_returns_empty_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("never-created");
        assert!(load_roots_in(&cache).is_empty());
    }

    #[test]
    fn register_in_same_pair_twice_results_in_exactly_one_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let root = PathBuf::from("/tmp/project-reregister");

        register_in(cache, &root, 9).unwrap();
        register_in(cache, &root, 9).unwrap();

        let roots = load_roots_in(cache);
        assert_eq!(roots.len(), 1);
    }

    /// Cross-AI review BLOCKER 4's required fix: two concurrent
    /// registrations for two DIFFERENT (project_root, phase) pairs must
    /// BOTH survive — the per-file storage shape has no read-modify-write
    /// step to lose one in.
    #[test]
    fn concurrent_registration_of_different_pairs_both_survive() {
        let cache = tempfile::tempdir().unwrap();
        let cache_path = cache.path().to_path_buf();
        let root_a = PathBuf::from("/tmp/concurrent-project-a");
        let root_b = PathBuf::from("/tmp/concurrent-project-b");

        std::thread::scope(|scope| {
            let a = scope.spawn(|| register_in(&cache_path, &root_a, 1));
            let b = scope.spawn(|| register_in(&cache_path, &root_b, 1));
            a.join().unwrap().unwrap();
            b.join().unwrap().unwrap();
        });

        let roots = load_roots_in(&cache_path);
        assert_eq!(roots.len(), 2, "both concurrent registrations must survive");
        assert!(roots.iter().any(|r| r.project_root == root_a));
        assert!(roots.iter().any(|r| r.project_root == root_b));
    }

    /// Two concurrent registrations for the SAME pair must never produce a
    /// torn file — write-temp-then-rename per entry protects against a
    /// torn read of the one file both writers target.
    #[test]
    fn concurrent_registration_of_same_pair_results_in_one_valid_entry() {
        let cache = tempfile::tempdir().unwrap();
        let cache_path = cache.path().to_path_buf();
        let root = PathBuf::from("/tmp/concurrent-project-same");

        std::thread::scope(|scope| {
            let a = scope.spawn(|| register_in(&cache_path, &root, 1));
            let b = scope.spawn(|| register_in(&cache_path, &root, 1));
            a.join().unwrap().unwrap();
            b.join().unwrap().unwrap();
        });

        let entry_path = entry_path_in(&cache_path, &root, 1);
        let contents = std::fs::read_to_string(&entry_path).unwrap();
        let parsed: RegisteredRoot =
            serde_json::from_str(&contents).expect("entry must not be torn");
        assert_eq!(parsed.project_root, root);

        let roots = load_roots_in(&cache_path);
        assert_eq!(roots.len(), 1);
    }

    /// T-23-33: the registry names every project this user is currently
    /// running — both the cache dir and the roots dir must be created
    /// private (0700), not inherit whatever the parent directory's mode is.
    #[test]
    fn register_in_creates_cache_and_roots_dirs_with_mode_0700() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let cache_path = base.path().join("nested-cache");
        let root = PathBuf::from("/tmp/project-perm");

        register_in(&cache_path, &root, 1).unwrap();

        let cache_mode = std::fs::metadata(&cache_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(cache_mode, 0o700, "cache dir must be created with mode 0700");

        let roots_mode = std::fs::metadata(roots_dir_in(&cache_path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(roots_mode, 0o700, "roots dir must be created with mode 0700");
    }

    #[test]
    fn prune_missing_in_removes_entry_for_deleted_root_and_reports_count() {
        let cache = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let project_path = project.path().to_path_buf();
        register_in(cache.path(), &project_path, 1).unwrap();
        drop(project); // deletes the project's directory from disk

        let removed = prune_missing_in(cache.path());

        assert_eq!(removed, 1);
        assert!(load_roots_in(cache.path()).is_empty());
    }

    #[test]
    fn prune_missing_in_keeps_entry_for_existing_root() {
        let cache = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        register_in(cache.path(), project.path(), 1).unwrap();

        let removed = prune_missing_in(cache.path());

        assert_eq!(removed, 0);
        assert_eq!(load_roots_in(cache.path()).len(), 1);
    }

    #[test]
    fn prune_missing_in_removes_and_counts_unparsable_entry() {
        let cache = tempfile::tempdir().unwrap();
        let dir = roots_dir_in(cache.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("junk.json"), "{not json").unwrap();

        let removed = prune_missing_in(cache.path());

        assert_eq!(removed, 1);
        assert!(load_roots_in(cache.path()).is_empty());
    }

    #[test]
    fn path_digest_is_stable_and_distinguishes_different_paths() {
        let a = Path::new("/tmp/project-a");
        let b = Path::new("/tmp/project-b");

        assert_eq!(path_digest(a), path_digest(a), "digest must be stable");
        assert_ne!(
            path_digest(a),
            path_digest(b),
            "different paths must yield different digests"
        );

        let cache = Path::new("/tmp/cache");
        assert_ne!(
            entry_path_in(cache, a, 1),
            entry_path_in(cache, b, 1),
            "different project roots must yield different entry paths"
        );
    }
}

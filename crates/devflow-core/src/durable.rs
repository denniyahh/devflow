//! Durable file operations for state and gate records (Phase 50.1 D-09).
//!
//! `replace_durably` is D-09's replace: fsync a temp file, rename it over the
//! target, fsync the directory, so a crash leaves the old bytes or the new bytes
//! and a power loss cannot lose the rename.
//!
//! Not established: these helpers make the calls, in order, and the `cfg(test)`
//! recorder proves that. Durability across a real power loss is not tested.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Fsync a directory, making a create, rename or unlink inside it durable.
pub(crate) fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()?;
    #[cfg(test)]
    record(Synced::Dir(dir.to_path_buf()));
    Ok(())
}

fn sync_file(file: &File, path: &Path) -> std::io::Result<()> {
    file.sync_all()?;
    #[cfg(test)]
    record(Synced::File(path.to_path_buf()));
    #[cfg(not(test))]
    let _ = path;
    Ok(())
}

/// The directory holding `path`; a bare file name lives in the current directory.
fn parent_of(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Create a sibling temp in DevFlow's shared `.{name}.{pid}.{n}.tmp` shape, which
/// every cleanup sweep matches on, write `contents` to it and fsync it. The temp
/// is removed on any error.
fn write_synced_temp(path: &Path, contents: &[u8]) -> std::io::Result<PathBuf> {
    let (tmp, mut file) = crate::workflow::create_unique_temp(path, std::process::id(), &TEMP_SEQ)?;
    if let Err(error) = file
        .write_all(contents)
        .and_then(|()| sync_file(&file, &tmp))
    {
        drop(file);
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(tmp)
}

/// Replace `path` with `contents`: temp, fsync, rename, directory fsync.
///
/// An error after the rename (the directory fsync failed) leaves the new bytes in
/// place: the caller is told the write is not known durable, not that it failed
/// to happen.
pub(crate) fn replace_durably(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = write_synced_temp(path, contents)?;
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    fsync_dir(parent_of(path))
}

/// RED scaffold: how a publish ended.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Publication {
    /// This call's bytes are at the target.
    Published,
    /// Another publisher got there first; the target was left alone.
    AlreadyExists,
}

/// RED scaffold: a hard-link publish with no directory fsync.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
pub(crate) fn publish_exclusive_durably(
    path: &Path,
    contents: &[u8],
) -> std::io::Result<Publication> {
    let tmp = write_synced_temp(path, contents)?;
    let publish = std::fs::hard_link(&tmp, path);
    std::fs::remove_file(&tmp)?;
    match publish {
        Ok(()) => Ok(Publication::Published),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(Publication::AlreadyExists)
        }
        Err(error) => Err(error),
    }
}

/// RED scaffold: an unlink with no directory fsync.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
pub(crate) fn remove_durably(path: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// RED scaffold: a rename with no directory fsync.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
pub(crate) fn rename_durably(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

/// RED scaffold: the walk R2-08 forbids, returning early on an existing target.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
pub(crate) fn create_dir_all_durably(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let missing: Vec<&Path> = dir
        .ancestors()
        .take_while(|ancestor| !ancestor.as_os_str().is_empty() && !ancestor.is_dir())
        .collect();
    for component in missing.iter().rev() {
        create_component_durably(component)?;
    }
    Ok(())
}

/// RED scaffold: a racing creator's `AlreadyExists` is an error here.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
fn create_component_durably(component: &Path) -> std::io::Result<()> {
    std::fs::create_dir(component)?;
    fsync_dir(parent_of(component))
}

/// RED scaffold: the replacing rename the helper exists to prevent.
#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
pub(crate) fn rename_to_unique_durably(
    from: &Path,
    dir: &Path,
    stem: &str,
    suffix: &str,
) -> std::io::Result<PathBuf> {
    static UNIQUE_SEQ: AtomicU64 = AtomicU64::new(0);
    rename_to_unique_durably_with(from, dir, stem, suffix, std::process::id(), &UNIQUE_SEQ)
}

#[allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]
fn rename_to_unique_durably_with(
    from: &Path,
    dir: &Path,
    stem: &str,
    suffix: &str,
    pid: u32,
    seq: &AtomicU64,
) -> std::io::Result<PathBuf> {
    let n = seq.fetch_add(1, Ordering::Relaxed);
    let candidate = dir.join(format!("{stem}.{pid}.{n}{suffix}"));
    std::fs::rename(from, &candidate)?;
    Ok(candidate)
}

/// One fsync the code under test made, in call order.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Synced {
    /// A file was synced.
    File(PathBuf),
    /// A directory was synced.
    Dir(PathBuf),
}

#[cfg(test)]
thread_local! {
    static SYNCED: std::cell::RefCell<Vec<Synced>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn record(entry: Synced) {
    SYNCED.with(|synced| synced.borrow_mut().push(entry));
}

/// Drain and return this thread's recorded fsyncs.
#[cfg(test)]
pub(crate) fn take_synced() -> Vec<Synced> {
    SYNCED.with(|synced| std::mem::take(&mut *synced.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// True when `name` is the temp shape `.{target}.{pid}.{n}.tmp` for this process.
    fn is_this_process_temp_for(name: &str, target: &str) -> bool {
        let Some(middle) = name
            .strip_prefix(&format!(".{target}."))
            .and_then(|rest| rest.strip_suffix(".tmp"))
        else {
            return false;
        };
        let Some((pid, sequence)) = middle.split_once('.') else {
            return false;
        };
        pid == std::process::id().to_string() && sequence.parse::<u64>().is_ok()
    }

    fn entry_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn replace_fsyncs_the_file_and_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.json");

        replace_durably(&target, b"one").unwrap();
        take_synced();
        replace_durably(&target, b"two").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two");
        assert_eq!(entry_names(dir.path()), ["target.json"], "no temp remains");
        let synced = take_synced();
        let [Synced::File(temp), Synced::Dir(parent)] = synced.as_slice() else {
            panic!("expected exactly [File(temp), Dir(parent)], got {synced:?}");
        };
        assert_eq!(parent, dir.path());
        assert_eq!(temp.parent().unwrap(), dir.path());
        let temp_name = temp.file_name().unwrap().to_string_lossy();
        assert!(
            is_this_process_temp_for(&temp_name, "target.json"),
            "temp keeps the shape every cleanup sweep matches on: {temp_name}"
        );
    }

    #[test]
    fn a_failed_replace_leaves_the_target_and_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        replace_durably(&missing.join("target.json"), b"one").unwrap_err();

        assert!(!missing.exists(), "a failed replace creates nothing");
        assert!(entry_names(dir.path()).is_empty());
        let synced = take_synced();
        assert!(
            !synced.iter().any(|entry| matches!(entry, Synced::Dir(_))),
            "no directory fsync for a replace that never happened: {synced:?}"
        );
    }

    #[test]
    fn exclusive_publish_never_replaces_and_fsyncs_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("response.json");

        assert_eq!(
            publish_exclusive_durably(&target, b"a").unwrap(),
            Publication::Published
        );
        let synced = take_synced();
        let [Synced::File(temp), Synced::Dir(parent)] = synced.as_slice() else {
            panic!("expected exactly [File(temp), Dir(parent)], got {synced:?}");
        };
        assert_eq!(parent, dir.path());
        assert!(is_this_process_temp_for(
            &temp.file_name().unwrap().to_string_lossy(),
            "response.json"
        ));

        assert_eq!(
            publish_exclusive_durably(&target, b"b").unwrap(),
            Publication::AlreadyExists
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "a");
        assert_eq!(
            entry_names(dir.path()),
            ["response.json"],
            "no temp remains"
        );
        let synced = take_synced();
        assert!(
            !synced.iter().any(|entry| matches!(entry, Synced::Dir(_))),
            "the loser published nothing, so it syncs no directory: {synced:?}"
        );
    }

    #[test]
    fn remove_fsyncs_the_directory_and_reports_a_missing_file_as_not_removed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("record.json");
        std::fs::write(&target, "x").unwrap();

        assert!(remove_durably(&target).unwrap());
        assert!(!target.exists());
        assert_eq!(take_synced(), [Synced::Dir(dir.path().to_path_buf())]);

        assert!(!remove_durably(&target).unwrap());
        assert_eq!(take_synced(), [], "nothing removed, nothing to sync");
    }

    #[test]
    fn rename_fsyncs_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a");
        let to = dir.path().join("b");
        std::fs::write(&from, "x").unwrap();

        rename_durably(&from, &to).unwrap();

        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "x");
        assert_eq!(take_synced(), [Synced::Dir(dir.path().to_path_buf())]);
    }

    #[test]
    fn creating_a_directory_fsyncs_its_parent() {
        let root = tempfile::tempdir().unwrap();
        let x = root.path().join("x");
        let y = x.join("y");

        create_dir_all_durably(&y).unwrap();
        assert!(y.is_dir());
        assert_eq!(
            take_synced(),
            [
                Synced::Dir(root.path().to_path_buf()),
                Synced::Dir(x.clone())
            ]
        );

        create_dir_all_durably(&y).unwrap();
        assert_eq!(
            take_synced(),
            [Synced::Dir(x.clone())],
            "the last component's parent is synced on every return"
        );

        // A creator that never synced its parent, the state one paused before
        // its fsync leaves: the next caller must sync it.
        let z = root.path().join("z");
        let w = z.join("w");
        std::fs::create_dir_all(&w).unwrap();
        create_dir_all_durably(&w).unwrap();
        assert_eq!(take_synced(), [Synced::Dir(z)]);
    }

    #[test]
    fn a_racing_creators_directory_still_has_its_parent_fsynced() {
        let root = tempfile::tempdir().unwrap();
        let raced = root.path().join("raced");
        std::fs::create_dir(&raced).unwrap();

        create_component_durably(&raced).unwrap();

        assert_eq!(take_synced(), [Synced::Dir(root.path().to_path_buf())]);
    }

    #[test]
    fn a_file_in_the_way_of_a_directory_is_an_error_and_syncs_nothing() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        std::fs::write(&file, "x").unwrap();

        let at_target = create_dir_all_durably(&file).unwrap_err();
        let below = create_dir_all_durably(&file.join("below")).unwrap_err();

        assert_eq!(at_target.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(below.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(take_synced(), []);
    }

    #[test]
    fn a_unique_rename_never_replaces_an_existing_name() {
        let dir = tempfile::tempdir().unwrap();
        let name = |n: u32| dir.path().join(format!("answer.4242.{n}.quarantined"));
        std::fs::write(name(0), "other-0").unwrap();
        std::fs::write(name(1), "other-1").unwrap();
        let source = dir.path().join("source");
        std::fs::write(&source, "mine").unwrap();
        take_synced();

        let moved = rename_to_unique_durably_with(
            &source,
            dir.path(),
            "answer",
            ".quarantined",
            4242,
            &AtomicU64::new(0),
        )
        .unwrap();

        assert_eq!(moved, name(2));
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(name(0)).unwrap(), "other-0");
        assert_eq!(std::fs::read_to_string(name(1)).unwrap(), "other-1");
        assert!(!source.exists(), "the source is gone");
        assert!(take_synced().contains(&Synced::Dir(dir.path().to_path_buf())));

        // Control: the replacing rename onto the same name destroys the earlier
        // file, which is the overwrite the helper exists to prevent.
        let second = dir.path().join("second");
        std::fs::write(&second, "second-bytes").unwrap();
        rename_durably(&second, &name(0)).unwrap();
        assert_eq!(std::fs::read_to_string(name(0)).unwrap(), "second-bytes");

        let third = dir.path().join("third");
        std::fs::write(&third, "third-bytes").unwrap();
        let wrapped =
            rename_to_unique_durably(&third, dir.path(), "answer", ".quarantined").unwrap();
        let wrapped_name = wrapped.file_name().unwrap().to_string_lossy();
        assert!(
            wrapped_name.contains(&format!(".{}.", std::process::id())),
            "{wrapped_name}"
        );
        assert!(wrapped_name.ends_with(".quarantined"), "{wrapped_name}");
        assert_eq!(std::fs::read_to_string(&wrapped).unwrap(), "third-bytes");
    }

    #[test]
    fn a_unique_rename_across_directories_fsyncs_both() {
        let root = tempfile::tempdir().unwrap();
        let from_dir = root.path().join("from");
        let to_dir = root.path().join("to");
        std::fs::create_dir(&from_dir).unwrap();
        std::fs::create_dir(&to_dir).unwrap();
        let source = from_dir.join("source");
        std::fs::write(&source, "mine").unwrap();

        let moved =
            rename_to_unique_durably_with(&source, &to_dir, "a", ".b", 1, &AtomicU64::new(0))
                .unwrap();

        assert_eq!(moved, to_dir.join("a.1.0.b"));
        assert_eq!(
            take_synced(),
            [Synced::Dir(to_dir), Synced::Dir(from_dir)],
            "the new name, then the removed one"
        );
    }
}

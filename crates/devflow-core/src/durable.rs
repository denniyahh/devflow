//! Durable file operations for state and gate records (Phase 50.1 D-09).
//!
//! Every write, publish and removal in `workflow.rs` and `gates.rs` goes through
//! one of D-09's three operations, which a source scan in this module's tests
//! enforces:
//! - replace ([`replace_durably`]): fsync a temp file, rename it over the target,
//!   fsync the directory;
//! - exclusive publish ([`publish_exclusive_durably`]): fsync a temp file, hard-link
//!   it to the target, fsync the directory, never falling back to `rename`
//!   ([`publish_exclusive_durably_or_withdraw`] also unlinks it again on a later
//!   failure, for a target only lock holders act on);
//! - remove ([`remove_durably`]): unlink, then fsync the directory.
//!
//! The advisory coordination files of Phase 50.1 (`supervisor-<id>.live`,
//! `NN-{stage}.{gate_id}.waiter`, `.devflow/.holder-NN`, `.devflow/.state-NN.lock`,
//! `.devflow/.gates-NN.lock`, `*.hold-tmp`, `.devflow/.gitignore`) are deliberately
//! outside these operations.
//! None of them holds a state or gate record, and a held `flock`, not their bytes
//! or existence, decides liveness. Fsync guards only against power loss, after
//! which no holder survives, so a lost or resurrected marker reads as not live,
//! which is the truth after a reboot.
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
    #[cfg(test)]
    if FAIL_DIR_FSYNC_OF.with(|fail| {
        let hit = fail.borrow().as_deref() == Some(dir);
        if hit {
            fail.borrow_mut().take();
        }
        hit
    }) {
        return Err(std::io::Error::other("injected directory fsync failure"));
    }
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

/// How [`publish_exclusive_durably`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Publication {
    /// This call's bytes are at the target.
    Published,
    /// Another publisher got there first; the target was left alone.
    AlreadyExists,
}

/// Publish `contents` at `path` exactly once: temp, fsync, hard link, directory
/// fsync. There is no fallback to `rename`, which would overwrite the first
/// publisher; a filesystem that cannot hard-link fails loudly (Phase 48 D-04).
pub(crate) fn publish_exclusive_durably(
    path: &Path,
    contents: &[u8],
) -> std::io::Result<Publication> {
    let tmp = write_synced_temp(path, contents)?;
    let publication = publication_of(std::fs::hard_link(&tmp, path));
    let cleanup = std::fs::remove_file(&tmp);
    let publication = publication?;
    if publication == Publication::Published {
        fsync_dir(parent_of(path))?;
        cleanup?;
    }
    Ok(publication)
}

/// [`publish_exclusive_durably`], except that an error after the link landed (the
/// directory fsync or the temp removal failed) withdraws the link before returning,
/// so an `Err` always means nothing is published at `path`.
///
/// Only for a target whose every publisher, and every reader that consumes it, holds
/// one lock the caller holds now, so no consumer can act on the link before it is
/// withdrawn: a gate's claim ack under the gate lock. Lock-free readers (`stop`'s
/// already-claimed check, `status`'s hints) can still see it for that moment and read
/// it as claimed. An answer is consumed without that lock, so it does not qualify. The
/// withdrawal is itself not known durable when the directory cannot be synced.
pub(crate) fn publish_exclusive_durably_or_withdraw(
    path: &Path,
    contents: &[u8],
) -> std::io::Result<Publication> {
    let tmp = write_synced_temp(path, contents)?;
    let publication = publication_of(std::fs::hard_link(&tmp, path));
    let cleanup = std::fs::remove_file(&tmp);
    let publication = publication?;
    if publication == Publication::Published
        && let Err(error) = fsync_dir(parent_of(path)).and(cleanup)
    {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(publication)
}

/// What a hard-link attempt means for a publish. Only `AlreadyExists` says another
/// publisher won; any other error, a filesystem without hard links for one, stays
/// an error rather than reading as an answer that is not there.
fn publication_of(link: std::io::Result<()>) -> std::io::Result<Publication> {
    match link {
        Ok(()) => Ok(Publication::Published),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(Publication::AlreadyExists)
        }
        Err(error) => Err(error),
    }
}

/// Unlink `path`, then fsync its directory. Returns whether this call removed
/// the file; a missing file is `Ok(false)` and syncs nothing.
///
/// An error from the directory fsync means the unlink happened but is not known
/// durable, and a retry then reads `Ok(false)`.
pub(crate) fn remove_durably(path: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => {
            fsync_dir(parent_of(path))?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Rename `from` to `to`, then fsync `to`'s directory. Callers keep both names in
/// one directory, so that fsync covers both entries.
#[cfg(test)]
pub(crate) fn rename_durably(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)?;
    fsync_dir(parent_of(to))
}

/// Create `dir` and any missing parents, fsyncing the parent of each component it
/// creates.
///
/// R2-08: every successful return has also fsynced `dir`'s own parent, whether
/// this call created `dir`, lost the race to create it, or found it already there,
/// because another process may have made the entry and not synced it yet.
///
/// Not established: an intermediate component another process made is not
/// re-synced. DevFlow has none: [`crate::workflow::ensure_devflow_dir`] passes the
/// `.devflow` component on its own first.
pub(crate) fn create_dir_all_durably(dir: &Path) -> std::io::Result<()> {
    // The empty path (a bare file name's parent) and a filesystem root have no
    // parent, and no entry of their own to create or sync.
    if dir.parent().is_none() {
        return Ok(());
    }
    if dir.is_dir() {
        return fsync_dir(parent_of(dir));
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

/// Create one directory and fsync its parent. A racing creator's directory counts
/// as created; a file in the way is an error.
fn create_component_durably(component: &Path) -> std::io::Result<()> {
    match std::fs::create_dir(component) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && component.is_dir() => {}
        Err(error) => return Err(error),
    }
    fsync_dir(parent_of(component))
}

static UNIQUE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Move `from` to a new name `{stem}.{pid}.{seq}{suffix}` in `dir`, by a no-replace
/// hard link, then remove the source and fsync the directory (R-11). Returns the
/// new name.
///
/// The destination is always a name nobody held: `pid` separates processes and the
/// `AlreadyExists` retry covers a reused pid with a restarted counter. A crash
/// between the link and the source removal leaves the file under both names, a
/// duplicate and never a loss.
pub(crate) fn rename_to_unique_durably(
    from: &Path,
    dir: &Path,
    stem: &str,
    suffix: &str,
) -> std::io::Result<PathBuf> {
    rename_to_unique_durably_with(from, dir, stem, suffix, std::process::id(), &UNIQUE_SEQ)
}

fn rename_to_unique_durably_with(
    from: &Path,
    dir: &Path,
    stem: &str,
    suffix: &str,
    pid: u32,
    seq: &AtomicU64,
) -> std::io::Result<PathBuf> {
    loop {
        let n = seq.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!("{stem}.{pid}.{n}{suffix}"));
        match std::fs::hard_link(from, &candidate) {
            Ok(()) => {
                remove_linked_source(from)?;
                fsync_dir(dir)?;
                let from_dir = parent_of(from);
                if from_dir != dir {
                    fsync_dir(from_dir)?;
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Remove a source already linked under its new name. `NotFound` means a concurrent
/// remover took it after the link; the bytes survive under the new name.
fn remove_linked_source(from: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(from) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
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
thread_local! {
    static FAIL_DIR_FSYNC_OF: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Make this thread's next [`fsync_dir`] of `dir` fail once, as a dying disk's `EIO`
/// would. Other directories sync as usual.
#[cfg(test)]
pub(crate) fn fail_next_dir_fsync_of(dir: &Path) {
    FAIL_DIR_FSYNC_OF.with(|fail| *fail.borrow_mut() = Some(dir.to_path_buf()));
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
    fn a_replace_whose_rename_fails_removes_its_temp() {
        let dir = tempfile::tempdir().unwrap();
        let occupied = dir.path().join("target.json");
        std::fs::create_dir(&occupied).unwrap();

        replace_durably(&occupied, b"one").unwrap_err();

        assert!(occupied.is_dir(), "the target is untouched");
        assert_eq!(entry_names(dir.path()), ["target.json"], "no temp remains");
        let synced = take_synced();
        assert!(
            !synced.iter().any(|entry| matches!(entry, Synced::Dir(_))),
            "no directory fsync for a rename that never happened: {synced:?}"
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

    #[test]
    fn a_bare_file_name_lives_in_the_current_directory() {
        assert_eq!(parent_of(Path::new("bare")), Path::new("."));
        assert_eq!(parent_of(Path::new("a/b")), Path::new("a"));
    }

    #[test]
    fn the_empty_path_and_the_root_have_no_entry_to_sync() {
        create_dir_all_durably(Path::new("")).unwrap();
        create_dir_all_durably(Path::new("/")).unwrap();
        assert_eq!(take_synced(), []);
    }

    #[test]
    fn only_already_exists_means_another_publisher_won() {
        let error = |kind: std::io::ErrorKind| Err(std::io::Error::from(kind));

        assert_eq!(publication_of(Ok(())).unwrap(), Publication::Published);
        assert_eq!(
            publication_of(error(std::io::ErrorKind::AlreadyExists)).unwrap(),
            Publication::AlreadyExists
        );
        assert_eq!(
            publication_of(error(std::io::ErrorKind::PermissionDenied))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::PermissionDenied,
            "a filesystem that cannot link must not read as a lost race"
        );
    }

    #[test]
    fn a_unique_rename_of_a_missing_source_is_an_error_not_a_retry() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        let error =
            rename_to_unique_durably_with(&missing, dir.path(), "a", ".b", 1, &AtomicU64::new(0))
                .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(entry_names(dir.path()).is_empty());
    }

    #[test]
    fn a_source_a_concurrent_remover_took_is_not_an_error_but_a_failed_removal_is() {
        let dir = tempfile::tempdir().unwrap();

        remove_linked_source(&dir.path().join("gone")).unwrap();
        // `remove_file` on a directory fails with something other than NotFound.
        let error = remove_linked_source(dir.path()).unwrap_err();

        assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
    }

    /// The file mutations D-09 reserves for this module.
    const DURABLE_BYPASS_OPS: [&str; 4] = ["fs::write(", "rename(", "hard_link(", "remove_file("];

    /// The numbered lines of `source` before its test module, whole-line comments
    /// dropped. A trailing comment stays part of its code line.
    fn non_test_code_lines(source: &str) -> Vec<(usize, &str)> {
        source
            .lines()
            .enumerate()
            .take_while(|(_, line)| *line != "mod tests {")
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .map(|(index, line)| (index + 1, line))
            .collect()
    }

    fn durable_bypass_violations(file: &str, source: &str) -> Vec<String> {
        let mut violations = Vec::new();
        for (number, line) in non_test_code_lines(source) {
            for op in DURABLE_BYPASS_OPS {
                if line.contains(op) {
                    violations.push(format!("{file}: line {number}: {op}"));
                }
            }
        }
        violations
    }

    /// D-09, criterion 3. The scope is exactly `workflow.rs` and `gates.rs`, D-09's
    /// state and gate modules. `durable.rs` is excluded because it is where the
    /// four operations live.
    ///
    /// Not established: this is a text scan, so a renamed import
    /// (`use std::fs as f; f::write(`) or an `OpenOptions` write is not seen. The
    /// `.gitignore` marker in `ensure_devflow_dir` is such a write, by design.
    #[test]
    fn state_and_gate_modules_mutate_files_only_through_the_durable_helpers() {
        let workflow = durable_bypass_violations("workflow.rs", include_str!("workflow.rs"));
        assert!(workflow.is_empty(), "direct mutations: {workflow:?}");
        let gates = durable_bypass_violations("gates.rs", include_str!("gates.rs"));
        assert!(gates.is_empty(), "direct mutations: {gates:?}");

        let scan = |source: &str| durable_bypass_violations("planted.rs", source);
        assert_eq!(
            scan("fn sneaky() {\n    std::fs::rename(a, b).unwrap();\n}\n"),
            ["planted.rs: line 2: rename("]
        );
        assert_eq!(
            scan("let _ = std::fs::remove_file(p); // cleanup\n"),
            ["planted.rs: line 1: remove_file("],
            "a trailing comment is part of its code line"
        );
        assert_eq!(
            scan("    // std::fs::write(p, b)\n"),
            Vec::<String>::new(),
            "a whole-line comment is not code"
        );
        assert_eq!(
            scan("mod tests {\n    std::fs::hard_link(a, b).unwrap();\n}\n"),
            Vec::<String>::new(),
            "test code is out of scope"
        );
        assert_eq!(
            scan("std::fs::write(p, b).unwrap();\n"),
            ["planted.rs: line 1: fs::write("]
        );
    }
}

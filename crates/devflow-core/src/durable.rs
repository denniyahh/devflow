//! Durable file operations for state and gate records (Phase 50.1 D-09).
#![allow(dead_code, reason = "RED scaffold: no caller until the GREEN commit")]

use std::io::Write;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// RED scaffold: the temp-and-rename the old writers did, with no fsync.
pub(crate) fn replace_durably(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let (tmp, mut file) = crate::workflow::create_unique_temp(path, std::process::id(), &TEMP_SEQ)?;
    file.write_all(contents)?;
    drop(file);
    std::fs::rename(&tmp, path)
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
}

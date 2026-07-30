//! The release executor's persisted step ledger (26-REVIEW.md **C-02**).
//!
//! **Scope limit.** This module exists for [`crate::release`]'s resume path
//! and nothing else (D-06a). It is not a licence for progress files elsewhere
//! in DevFlow: `workflow.rs`, `ship.rs`, `hooks.rs`, `sync.rs`, and `gates.rs`
//! neither read nor write this record, and adding such a reader would exceed
//! the amendment that authorized this file.
//!
//! **Live state wins.** Git and the registry remain authoritative on what is
//! actually true. Where this record and live state disagree, live state wins,
//! and this file may never be the reason a step is skipped — every step keeps
//! the live-state predicate it already has. The ledger supplies exactly two
//! facts no amount of git archaeology can recover: *which version this cut is
//! for*, and *whether a cut is in flight at all*.
//!
//! **It records; it never compensates.** Nothing here un-pushes, un-tags,
//! un-publishes, deletes, re-points, or force-updates anything (D-05). The
//! module does not even remove its own record — see the note on the
//! deliberately absent `clear` below.
//!
//! **The format is versioned on purpose.** D-06a rates this change *costly* to
//! reverse: once a released binary writes a ledger, every later version must be
//! able to read what it wrote. The on-disk shape therefore carries an explicit
//! [`LEDGER_VERSION`], and JSON via `serde_json` was chosen because it is
//! already a `devflow-core` dependency and the format every other persisted
//! DevFlow record uses (`state-NN.json`, gate files, the machine registry).
//!
//! **An unrecognized version refuses rather than being ignored.** Treating an
//! unreadable ledger as absent would silently restore exactly the C-02 behavior
//! this module removes — the executor would compute a fresh version and cut a
//! second release. So a corrupt, unreadable, or newer-format ledger is a loud
//! error naming the file; only a genuinely *absent* file reads as "no release is
//! in flight".

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::version::sanitize_changelog_subject;

/// The on-disk format version this build writes and is able to read.
///
/// Defined once and consulted by both [`write`] (which stamps it) and [`read`]
/// (which refuses anything else), so the supported version can never be
/// hardcoded at a second site that drifts from the first.
pub const LEDGER_VERSION: u32 = 1;

/// The ledger's file name inside the repository's git common directory.
const LEDGER_FILE_NAME: &str = "devflow-release-ledger.json";

/// Whether a release cut is in flight or has finished cleanly.
///
/// This distinction is the ledger's primary job, not an incidental detail
/// (D-06a): `VersionError::UnreachableBaseline` is by-construction true
/// mid-sequence, so without a persisted record a re-run after a *complete*
/// release is indistinguishable from a fresh start and quietly begins the next
/// one. [`ReleaseLedger::head_at_completion`] is how the executor corroborates
/// [`LedgerStatus::Complete`] against live git rather than trusting this field
/// alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerStatus {
    /// A release cut started and has not reached a terminal successful
    /// outcome. Its [`ReleaseLedger::version`] pins the identity of the cut a
    /// re-run must finish.
    InFlight,
    /// A release cut reached a terminal successful outcome at
    /// [`ReleaseLedger::head_at_completion`].
    Complete,
}

/// One step's persisted outcome.
///
/// Deliberately a separate type from [`crate::release::StepReport`] rather than
/// a serialization of it. Persisting the in-memory reporting type directly
/// would make every future change to that type a change to a released on-disk
/// format — precisely the coupling D-06a's *costly* reversibility rating warns
/// against. The fields are owned `String`s so the persisted shape is stable
/// even if the in-memory enums gain variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerStep {
    /// The step's operator-facing label (`ReleaseStep::label`).
    pub step: String,
    /// The step's status label, `"completed"` or `"skipped"`.
    pub status: String,
    /// The step's bounded, control-character-neutralized detail.
    pub detail: String,
}

/// What the release executor recorded about one release cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseLedger {
    /// The on-disk format version. See [`LEDGER_VERSION`].
    pub ledger_version: u32,
    /// Whether the cut is in flight or finished.
    pub status: LedgerStatus,
    /// The release version this cut is for, e.g. `"1.9.0"`. This is the field
    /// C-02 turns on: a re-run pins this instead of recomputing a version from
    /// git state the interrupted run has already moved.
    pub version: String,
    /// The release tag name, e.g. `"v1.9.0"`.
    pub tag: String,
    /// Seconds since the unix epoch when the cut started. Informational only —
    /// no logic branches on it, because a clock is not evidence about git
    /// state.
    pub started_unix: u64,
    /// Seconds since the unix epoch of the last write. Informational only, for
    /// the same reason.
    pub updated_unix: u64,
    /// The commit `HEAD` named when the run reached a terminal successful
    /// outcome; `None` while in flight. Corroborated against a live
    /// `git rev-parse HEAD` by the executor, so a stale `Complete` status alone
    /// can never refuse a legitimate new release.
    pub head_at_completion: Option<String>,
    /// One entry per reported step, in sequence order.
    pub steps: Vec<LedgerStep>,
}

impl ReleaseLedger {
    /// A fresh in-flight record for `version`/`tag`, with no steps yet.
    pub fn in_flight(version: &str, tag: &str) -> Self {
        let now = unix_now();
        Self {
            ledger_version: LEDGER_VERSION,
            status: LedgerStatus::InFlight,
            version: version.to_string(),
            tag: tag.to_string(),
            started_unix: now,
            updated_unix: now,
            head_at_completion: None,
            steps: Vec::new(),
        }
    }

    /// Refresh `updated_unix`. Informational only — nothing branches on it —
    /// but kept explicit rather than stamped inside [`write`] so a written
    /// record round-trips byte-for-byte with the value the caller holds.
    pub fn touch(&mut self) {
        self.updated_unix = unix_now();
    }

    /// Mark this record complete at the commit `head`, as read from live git by
    /// the caller at that moment.
    pub fn mark_complete(&mut self, head: &str) {
        self.status = LedgerStatus::Complete;
        self.head_at_completion = Some(head.to_string());
        self.updated_unix = unix_now();
    }
}

/// Errors this module refuses with. Every message names the file to inspect,
/// and none proposes a removal the tool performs — inspecting and resolving the
/// record is the operator's, and nothing here deletes it.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// A filesystem operation on the ledger failed.
    #[error("release ledger I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The repository's git common directory could not be resolved, so the
    /// ledger has no location. Refused rather than falling back to a
    /// working-tree path — see [`ledger_path`] for why that fallback would
    /// break the executor's own entry guard.
    #[error("could not resolve the git directory for the release ledger under {path}: {detail}")]
    GitDir {
        /// The project root the resolution was attempted from, sanitized.
        path: String,
        /// A bounded, sanitized description of the failure.
        detail: String,
    },
    /// The ledger exists but is not readable as this format. Never treated as
    /// absent: treating it as absent restores C-02.
    #[error(
        "release ledger at {path} could not be read: {detail} — inspect the file; devflow \
         will not rewrite or remove it"
    )]
    Parse {
        /// The ledger's path, sanitized.
        path: String,
        /// A bounded, sanitized parse detail.
        detail: String,
    },
    /// The ledger declares a format version this build does not understand,
    /// most likely written by a newer devflow.
    #[error(
        "release ledger at {path} declares format version {found}, but this build of devflow \
         supports version {supported} — inspect the file (upgrade devflow if it was written by \
         a newer build); devflow will not rewrite or remove it"
    )]
    UnsupportedVersion {
        /// The ledger's path, sanitized.
        path: String,
        /// The version found on disk.
        found: u64,
        /// The version this build supports ([`LEDGER_VERSION`]).
        supported: u32,
    },
}

/// Where the ledger for the repository at `project_root` lives:
/// `<git-common-dir>/devflow-release-ledger.json`.
///
/// **Why the git directory and not the working tree.** `run_release`'s first
/// entry guard requires `git status --porcelain` to be empty. A ledger written
/// into the working tree would be untracked output that makes the *resume* run
/// refuse with `ReleaseError::DirtyWorkingTree` — the C-02 fix defeating itself
/// on its own second invocation. Placing the record inside the git directory
/// keeps it invisible to that guard by construction rather than by a
/// `.gitignore` entry a fixture or a fresh clone might not carry.
///
/// **Why `--git-common-dir` and not `--git-dir`.** The common directory is
/// shared with linked worktrees, which is the correct scope for "a release of
/// this repository": a cut started from one worktree is the same cut when
/// resumed from another.
///
/// Refuses when git cannot answer. It never falls back to a working-tree path,
/// because that fallback is the failure mode described above.
pub fn ledger_path(project_root: &Path) -> Result<PathBuf, LedgerError> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(project_root)
        .output()
        .map_err(|err| LedgerError::GitDir {
            path: sanitize_changelog_subject(&project_root.display().to_string()),
            detail: sanitize_changelog_subject(&err.to_string()),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(LedgerError::GitDir {
            path: sanitize_changelog_subject(&project_root.display().to_string()),
            detail: sanitize_changelog_subject(&stderr),
        });
    }
    let reported = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if reported.is_empty() {
        return Err(LedgerError::GitDir {
            path: sanitize_changelog_subject(&project_root.display().to_string()),
            detail: "git reported an empty --git-common-dir".to_string(),
        });
    }
    // git returns a path relative to the working directory for the ordinary
    // case (`.git`), and an absolute one for a linked worktree.
    let common = PathBuf::from(&reported);
    let common = if common.is_absolute() {
        common
    } else {
        project_root.join(common)
    };
    Ok(common.join(LEDGER_FILE_NAME))
}

/// Read the ledger for `project_root`.
///
/// `Ok(None)` means the file does not exist, which is the only condition that
/// reads as "no release is in flight" — a release cut by an older binary, from
/// a second clone, or on another machine legitimately leaves no ledger, and
/// that case must keep behaving exactly as it does today.
///
/// Refuses when the file exists but does not parse, or parses with a
/// [`LEDGER_VERSION`] this build does not support. Refusing never rewrites or
/// removes the file.
pub fn read(project_root: &Path) -> Result<Option<ReleaseLedger>, LedgerError> {
    let path = ledger_path(project_root)?;
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(LedgerError::Io(err)),
    };
    let display = sanitize_changelog_subject(&path.display().to_string());

    // The format version is checked BEFORE the record is deserialized: a
    // newer format may well still deserialize into this build's struct while
    // meaning something different, and silently accepting it is the same class
    // of guess this module exists to refuse.
    let value: serde_json::Value =
        serde_json::from_str(&contents).map_err(|err| LedgerError::Parse {
            path: display.clone(),
            detail: sanitize_changelog_subject(&err.to_string()),
        })?;
    let found = value
        .get("ledger_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| LedgerError::Parse {
            path: display.clone(),
            detail: "no readable `ledger_version` field".to_string(),
        })?;
    if found != u64::from(LEDGER_VERSION) {
        return Err(LedgerError::UnsupportedVersion {
            path: display,
            found,
            supported: LEDGER_VERSION,
        });
    }

    let ledger: ReleaseLedger =
        serde_json::from_value(value).map_err(|err| LedgerError::Parse {
            path: display,
            detail: sanitize_changelog_subject(&err.to_string()),
        })?;
    Ok(Some(ledger))
}

/// Write `ledger` for `project_root`, atomically.
///
/// Every operator-visible string is bounded and control-character-neutralized
/// through [`sanitize_changelog_subject`] before it is persisted (T-26-37):
/// step details carry untrusted git and cargo stderr, and the CLI reads this
/// record back and prints it, so the round trip through the filesystem must not
/// reintroduce what the in-memory path already strips.
///
/// The write goes to a uniquely-named temp file in the same directory followed
/// by a `rename`, mirroring `gates.rs::write_atomic` and `registry.rs`, so a
/// reader never observes a partial file and no temp sibling is left behind.
///
/// There is deliberately **no** `clear`/`remove`: removing the record is how a
/// stale-ledger bug becomes silent, and every state this executor can be in is
/// expressible as in-flight or complete.
pub fn write(project_root: &Path, ledger: &ReleaseLedger) -> Result<(), LedgerError> {
    let path = ledger_path(project_root)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let bounded = ReleaseLedger {
        ledger_version: LEDGER_VERSION,
        status: ledger.status,
        version: sanitize_changelog_subject(&ledger.version),
        tag: sanitize_changelog_subject(&ledger.tag),
        started_unix: ledger.started_unix,
        updated_unix: ledger.updated_unix,
        head_at_completion: ledger
            .head_at_completion
            .as_deref()
            .map(sanitize_changelog_subject),
        steps: ledger
            .steps
            .iter()
            .map(|step| LedgerStep {
                step: sanitize_changelog_subject(&step.step),
                status: sanitize_changelog_subject(&step.status),
                detail: sanitize_changelog_subject(&step.detail),
            })
            .collect(),
    };

    let contents = serde_json::to_string_pretty(&bounded).map_err(|err| LedgerError::Parse {
        path: sanitize_changelog_subject(&path.display().to_string()),
        detail: sanitize_changelog_subject(&err.to_string()),
    })?;

    static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{n}", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Seconds since the unix epoch, derived the way `history.rs` does it — no
/// date dependency, and a clock failure degrades to `0` rather than panicking
/// (nothing branches on these values).
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::tests::init_repo;

    fn sample(version: &str) -> ReleaseLedger {
        let mut ledger = ReleaseLedger::in_flight(version, &format!("v{version}"));
        ledger.steps.push(LedgerStep {
            step: "version bump".to_string(),
            status: "completed".to_string(),
            detail: format!("wrote and committed version {version}"),
        });
        ledger.steps.push(LedgerStep {
            step: "signed release tag".to_string(),
            status: "skipped".to_string(),
            detail: "already an annotated, verified, pushed release tag".to_string(),
        });
        ledger
    }

    #[test]
    fn round_trips_through_the_git_directory() {
        let repo = init_repo();
        let root = repo.path();
        let written = sample("1.9.0");

        write(root, &written).expect("write the ledger");
        let read_back = read(root)
            .expect("read the ledger")
            .expect("a written ledger must read back as Some");

        assert_eq!(read_back, written, "the record must round-trip unchanged");
        assert_eq!(read_back.steps.len(), 2, "the step list must survive");
    }

    /// The assertion that keeps `run_release`'s clean-tree entry guard working.
    /// A ledger placed anywhere in the working tree fails this, even if it were
    /// `.gitignore`d in this repository — the property asserted is git's real
    /// answer, not the shape of the path.
    #[test]
    fn ledger_is_invisible_to_git_status() {
        let repo = init_repo();
        let root = repo.path();

        write(root, &sample("2.0.0")).expect("write the ledger");

        let output = crate::test_support::git_command(root)
            .args(["status", "--porcelain"])
            .output()
            .expect("spawn git status");
        assert!(output.status.success(), "git status must succeed");
        let porcelain = String::from_utf8_lossy(&output.stdout);
        assert!(
            porcelain.trim().is_empty(),
            "a ledger write must leave the working tree clean, got: {porcelain}"
        );
    }

    #[test]
    fn refuses_an_unsupported_ledger_version() {
        let repo = init_repo();
        let root = repo.path();
        let path = ledger_path(root).expect("resolve the ledger path");
        let future = LEDGER_VERSION + 1;
        let raw = format!(
            "{{\"ledger_version\":{future},\"status\":\"inflight\",\"version\":\"9.9.9\",\
             \"tag\":\"v9.9.9\",\"started_unix\":1,\"updated_unix\":1,\
             \"head_at_completion\":null,\"steps\":[]}}"
        );
        std::fs::write(&path, &raw).expect("write a future-format ledger");

        let err = read(root).expect_err("an unsupported ledger version must refuse");
        match &err {
            LedgerError::UnsupportedVersion {
                found, supported, ..
            } => {
                assert_eq!(*found, u64::from(future));
                assert_eq!(*supported, LEDGER_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
        let rendered = err.to_string();
        assert!(
            rendered.contains(&future.to_string())
                && rendered.contains(&LEDGER_VERSION.to_string()),
            "the refusal must name both version numbers: {rendered}"
        );

        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read"),
            raw,
            "refusing must not rewrite the file it cannot read"
        );
    }

    #[test]
    fn refuses_a_corrupt_ledger() {
        let repo = init_repo();
        let root = repo.path();
        let path = ledger_path(root).expect("resolve the ledger path");
        let raw = "{ this is not json";
        std::fs::write(&path, raw).expect("write a corrupt ledger");

        let err = read(root).expect_err("a corrupt ledger must refuse");
        assert!(
            matches!(err, LedgerError::Parse { .. }),
            "expected Parse, got {err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("re-read"),
            raw,
            "refusing must leave the file untouched"
        );
    }

    #[test]
    fn absent_ledger_is_not_an_error() {
        let repo = init_repo();
        assert_eq!(
            read(repo.path()).expect("an absent ledger must not error"),
            None,
            "an absent ledger reads as `no release is in flight`"
        );
    }

    #[test]
    fn write_is_atomic_and_leaves_no_temp_file() {
        let repo = init_repo();
        let root = repo.path();
        write(root, &sample("3.1.4")).expect("write the ledger");
        let path = ledger_path(root).expect("resolve the ledger path");
        let dir = path.parent().expect("the ledger has a parent directory");

        let siblings: Vec<String> = std::fs::read_dir(dir)
            .expect("read the git directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("devflow-release-ledger"))
            .collect();
        assert_eq!(
            siblings,
            vec![LEDGER_FILE_NAME.to_string()],
            "the write must leave exactly the ledger file and no temp sibling"
        );
    }
}

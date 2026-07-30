//! The release-cut executor (backlog `999.25`/`999.52`).
//!
//! [`execute_release`] composes, in order, the five steps of a release cut —
//! version bump and direct push to `develop`, the human-gated
//! `develop`->`main` boundary, the signed tag, the sync back, and the
//! crates.io publish — reusing every primitive this phase already built
//! (`GitFlow::push_ref`, `release_tag_state`, `create_signed_release_tag`
//! from 26-03; `sync_main_to_develop` from 26-04; `crate_already_published`/
//! `cargo_publish` from 26-05) rather than reimplementing any of them.
//!
//! Every step consults a live-state predicate rather than a persisted
//! progress file (D-06) — resume comes from real git and registry state
//! only. Nothing is ever rolled back on failure; the fix is always forward
//! (D-05), matching `hooks_after_ship`'s documented policy. The
//! `develop`->`main` pull request is opened and merged by a human; this
//! module never touches it (D-02) — no `gh` subprocess is invoked anywhere
//! in this file. The publish step is the one irreversible operation in the
//! sequence; the operator's authorization for running it unattended is
//! recorded in `26-01-SUMMARY.md` Decision 2 (`automate-publish`).
//!
//! Re-running this function after a release has fully completed (its tag
//! reachable from `develop` after the sync) computes the NEXT version and
//! begins a new release — this is not a "last release already finished"
//! no-op detector, for the same `compute_version` reason documented on
//! [`ReleaseError::TagCollision`]'s call sites below.

use std::path::Path;
use std::process::Command;

use crate::version::sanitize_changelog_subject;

/// One step of the release-cut sequence, in the order [`execute_release`]
/// runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStep {
    /// Compute the release version, write it into the version file, commit
    /// it, and push `develop` directly to `origin` (D-01/D-08).
    VersionBump,
    /// Create (or verify the pre-existence of) the signed release tag at
    /// `origin/main`'s tip, once `origin/main` declares the release version.
    Tag,
    /// Merge `origin/main` back into `develop` via
    /// [`crate::sync::sync_main_to_develop`] — D-07's second entry point.
    Sync,
    /// Publish each workspace member to crates.io in
    /// [`crate::git::publish_order`]'s sequence (D-04).
    Publish,
}

impl ReleaseStep {
    /// A stable, operator-facing label for this step.
    pub fn label(&self) -> &'static str {
        match self {
            ReleaseStep::VersionBump => "version bump",
            ReleaseStep::Tag => "signed release tag",
            ReleaseStep::Sync => "sync main back into develop",
            ReleaseStep::Publish => "crates.io publish",
        }
    }
}

/// Whether a [`ReleaseStep`] actually did something on this run, or found
/// nothing to do (D-06's live-state idempotency shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// The step performed a real action this run.
    Completed,
    /// Live state already satisfied this step; nothing was done.
    Skipped,
}

/// A single step's outcome, recorded in [`ReleaseReport::steps`].
#[derive(Debug, Clone)]
pub struct StepReport {
    /// Which step this report describes.
    pub step: ReleaseStep,
    /// Whether the step acted or was a no-op.
    pub status: StepStatus,
    /// A bounded, control-character-neutralized human-readable detail
    /// ([`sanitize_changelog_subject`]).
    pub detail: String,
}

/// The overall result of one [`execute_release`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseOutcome {
    /// The run stopped at the `develop`->`main` human gate (D-02):
    /// `origin/main` does not yet declare the release version. No tag was
    /// created. The operator must merge the release PR and re-run.
    HaltedAtHumanGate,
    /// All five steps ran (or were skipped as already-satisfied) to
    /// completion, **including** a publish step that either published every
    /// package in [`crate::git::publish_order`]'s sequence or found each one
    /// already live on the registry.
    Completed,
    /// Every step ran, but [`crate::git::publish_order`] resolved no packages,
    /// so the registry received nothing (C-04). Distinct from
    /// [`ReleaseOutcome::Completed`] on purpose: the tag is cut and pushed, so
    /// this is not a failure, but a caller that renders it as an unqualified
    /// "release cut complete" is reporting a false green on the one
    /// irreversible step. Callers must name the fact that nothing was
    /// published.
    CompletedWithoutPublish,
}

/// The full result of an [`execute_release`] call.
#[derive(Debug, Clone)]
pub struct ReleaseReport {
    /// The computed (or resumed) release version, e.g. `"1.9.0"`.
    pub version: String,
    /// The release tag name, e.g. `"v1.9.0"`.
    pub tag: String,
    /// One entry per step attempted, in sequence order.
    pub steps: Vec<StepReport>,
    /// Whether the run completed or halted at the human gate.
    pub outcome: ReleaseOutcome,
}

/// Errors produced by [`execute_release`]. Every string payload is bounded
/// through [`sanitize_changelog_subject`] before being stored (T-26-37) —
/// untrusted git/cargo/sync stderr can contain filesystem paths, remote
/// URLs, and key-file locations.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    /// A filesystem operation failed.
    #[error("release I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// A `GitFlow` operation failed.
    #[error("git operation failed: {0}")]
    Git(#[from] crate::git::GitError),
    /// A version operation failed.
    #[error("version operation failed: {0}")]
    Version(#[from] crate::version::VersionError),
    /// The sync-back step failed.
    #[error("sync step failed: {0}")]
    Sync(#[from] crate::sync::SyncError),
    /// The publish step failed.
    #[error("publish step failed: {0}")]
    Publish(#[from] crate::git::PublishError),
    /// The working tree has uncommitted changes. Refuses before any
    /// mutation, including the fetch.
    #[error("working tree is not clean — commit, stash, or discard changes first")]
    DirtyWorkingTree,
    /// The current checkout is not on `develop`.
    #[error("must be run from 'develop' (currently on '{current}')")]
    NotOnDevelop {
        /// The branch actually checked out, sanitized.
        current: String,
    },
    /// No git remote is configured.
    #[error("no git remote configured")]
    NoRemote,
    /// The release tag already exists in a shape that is neither the
    /// already-released state nor a shape safe to create over (D-05): a
    /// stray lightweight tag, a mismatched annotated tag, or a tag that
    /// could not be pushed and independently re-verified as `Released`.
    /// Never auto-resolved — no deletion, no re-pointing, no force.
    #[error("release tag `{tag}` collision: {detail}")]
    TagCollision {
        /// The release tag's name.
        tag: String,
        /// A bounded, sanitized description of the collision.
        detail: String,
    },
    /// The highest semver tag is unreachable from `develop` — which would
    /// normally mean "a release is in flight, resume it" — but the tag does
    /// not name `origin/main`'s tip, so it is a stray tag rather than a
    /// release being resumed (C-01). Refused before any mutation: adopting it
    /// would write, commit, and push a version nobody asked for to the shared
    /// `develop` branch. Never auto-resolved — this module deletes and
    /// re-points nothing.
    #[error(
        "highest semver tag `{tag}` is unreachable from develop but names {target}, not \
         origin/main's tip {main_tip} — refusing to adopt it as an in-flight release. If a \
         release really is in flight, merge the develop -> main release PR first; if this is a \
         leftover tag from a squash-merged branch, it is not a release to resume"
    )]
    StrayBaselineTag {
        /// The unreachable tag's name, sanitized.
        tag: String,
        /// `origin/main`'s current tip, sanitized.
        main_tip: String,
        /// The commit the tag actually names, sanitized.
        target: String,
    },
}

/// A failed [`execute_release`] run: the error that stopped it **plus** the
/// step ledger accumulated before it stopped (C-03, phase 26 review).
///
/// The ledger is not decoration. Nothing in this sequence is ever rolled
/// back (D-05), so on any failure the steps that already reported are
/// precisely the set of external-state mutations — pushed commits, a pushed
/// signed tag, published crates — that the operator must NOT redo. Returning
/// a bare [`ReleaseError`] told them only about the step that failed, which
/// makes a blind re-run the natural next move.
#[derive(Debug)]
pub struct ReleaseFailure {
    /// Why the run stopped.
    pub error: ReleaseError,
    /// Every step that reported before the failure, in sequence order.
    pub steps: Vec<StepReport>,
}

impl std::fmt::Display for ReleaseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ReleaseFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

fn git_output(project_root: &Path, args: &[&str]) -> Result<String, ReleaseError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(ReleaseError::Git(crate::git::GitError::Command(
            sanitize_changelog_subject(&stderr),
        )))
    }
}

fn git_raw(project_root: &Path, args: &[&str]) -> Result<(), ReleaseError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(ReleaseError::Git(crate::git::GitError::Command(
            sanitize_changelog_subject(&stderr),
        )))
    }
}

/// Whether `ancestor` is an ancestor of (or equal to) `descendant` — a thin
/// wrapper over `git merge-base --is-ancestor`. Never panics; a spawn
/// failure degrades to `false` (the caller then treats the step as "not yet
/// satisfied", which is the safe direction — it means "act", never "skip a
/// real step").
fn is_ancestor(project_root: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// The commit a tag names, peeling an annotated tag to its target
/// (`<tag>^{commit}`). `None` when the tag does not resolve at all. Used to
/// decide whether an unreachable baseline tag is a release in flight or a
/// stray leftover (C-01) — asked before any mutation, so a `None` here must
/// refuse rather than proceed.
fn tag_target_commit(project_root: &Path, tag: &str) -> Option<String> {
    let spec = format!("{tag}^{{commit}}");
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &spec])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!commit.is_empty()).then_some(commit)
}

/// Parse a bare `MAJOR.MINOR.PATCH` string (e.g. the un-prefixed contents of
/// an `UnreachableBaseline` tag name) via the same parse path
/// [`crate::version::version_in_contents`] uses for a real version file —
/// wrapping the text as a synthetic top-level `version = "..."` assignment,
/// which is exactly what `field_for` resolves for any path whose file name
/// is not `Cargo.toml`/`pyproject.toml` (falls through to the bare `"version"`
/// field at the top-level, empty-string section).
fn parse_bare_version(text: &str) -> Result<crate::version::Version, ReleaseError> {
    let synthetic = format!("version = \"{text}\"\n");
    crate::version::version_in_contents(Path::new("release-tag"), &synthetic)
        .map_err(ReleaseError::Version)
}

/// Run the release-cut sequence end to end: compute (or resume) the release
/// version; write it and push `develop` directly to `origin` (step 1,
/// D-01/D-08); halt cleanly at the `develop`->`main` human gate until
/// `origin/main` declares the release version (step 2, D-02); create, push,
/// and independently re-verify the real signed release tag (step 3, D-10);
/// sync `origin/main` back into `develop` via the identical
/// [`crate::sync::sync_main_to_develop`] the standalone `devflow sync`
/// subcommand calls (step 4, D-07); and publish each
/// [`crate::git::publish_order`] package to crates.io, gated by a live
/// registry existence check (step 5, D-04, the one irreversible operation in
/// the sequence — see `26-01-SUMMARY.md` Decision 2 for the operator's
/// standing authorization to run it unattended). Every step's own live-state
/// predicate makes it independently resumable (D-06); nothing is ever rolled
/// back on any failure (D-05) — see the module doc comment for the complete
/// contract.
pub fn execute_release(project_root: &Path) -> Result<ReleaseReport, ReleaseFailure> {
    let mut steps: Vec<StepReport> = Vec::new();
    match run_release(project_root, &mut steps) {
        Ok((version, tag, outcome)) => Ok(ReleaseReport {
            version,
            tag,
            steps,
            outcome,
        }),
        Err(error) => Err(ReleaseFailure { error, steps }),
    }
}

/// The sequence itself. Takes the step ledger by `&mut` rather than owning it
/// so that a failure at any `?` still hands every already-reported step back
/// to [`execute_release`]'s caller (C-03) — nothing here is rolled back, so
/// that ledger is the operator's only record of what already landed.
fn run_release(
    project_root: &Path,
    steps: &mut Vec<StepReport>,
) -> Result<(String, String, ReleaseOutcome), ReleaseError> {
    // Entry guards, before any mutation, mirroring
    // `crate::sync::sync_main_to_develop`'s own guard order.
    let status = git_output(project_root, &["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        return Err(ReleaseError::DirtyWorkingTree);
    }
    let current = git_output(project_root, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if current != crate::config::DEVELOP {
        return Err(ReleaseError::NotOnDevelop {
            current: sanitize_changelog_subject(&current),
        });
    }
    if !crate::git::GitFlow::new(project_root).has_remote() {
        return Err(ReleaseError::NoRemote);
    }
    git_raw(
        project_root,
        &["fetch", "origin", "main", "develop", "--quiet"],
    )?;

    // Version resolution. `UnreachableBaseline` is the resume signal for an
    // in-flight release (see module doc comment and `compute_version`'s own
    // doc comment): once the release tag lands on `origin/main`'s tip and
    // before the sync lands, a squash-merged `main` makes that tag
    // unreachable from `develop` by construction, so this guard fires on
    // exactly the release being resumed.
    //
    // C-01: that inference is only valid for a tag that really does name
    // `origin/main`'s tip, and it MUST be checked here, not deferred to step
    // 3. The original code deferred it on the grounds that "step 3
    // independently validates that the tag belongs to origin/main's tip" —
    // but step 1 writes, commits, and pushes the adopted version to
    // `origin/develop` before step 2 or 3 ever run, so by then the damage has
    // landed on a shared branch. Unreachable-but-unrelated tags are the
    // ordinary case, not an exotic one: `hooks_after_ship`'s `version_bump`
    // creates a `v{version}` tag at the end of every phase's Ship stage, and
    // squash-merging that phase's branch leaves the tag permanently
    // unreachable from `develop` and strictly higher than the reachable
    // baseline (this repository already carries one such orphan, `v1.3.69`).
    // Adopting one wrote and pushed a version nobody asked for and reported
    // "the version bump already landed in a prior invocation", which was
    // simply false.
    let mut resume_note: Option<String> = None;
    let version = match crate::version::compute_version(project_root) {
        Ok(version) => version,
        Err(crate::version::VersionError::UnreachableBaseline { tag }) => {
            let main_tip = git_output(project_root, &["rev-parse", "origin/main"])?
                .trim()
                .to_string();
            match tag_target_commit(project_root, &tag) {
                Some(target) if target == main_tip => {}
                other => {
                    return Err(ReleaseError::StrayBaselineTag {
                        tag: sanitize_changelog_subject(&tag),
                        main_tip: sanitize_changelog_subject(&main_tip),
                        target: sanitize_changelog_subject(
                            other.as_deref().unwrap_or("no resolvable commit"),
                        ),
                    });
                }
            }
            let stripped = tag.strip_prefix('v').unwrap_or(tag.as_str());
            let resumed = parse_bare_version(stripped)?;
            // W-18/WR-01: the note is folded into step 1's own report below
            // rather than pushed as a second `VersionBump` entry — the
            // original pushed one here AND fell through into step 1, so every
            // resumed run reported `VersionBump` twice, breaking
            // `ReleaseReport::steps`' documented one-entry-per-step contract.
            resume_note = Some(format!(
                "resuming the in-flight release identified by unreachable tag `{tag}`, \
                 which names origin/main's tip"
            ));
            resumed
        }
        Err(err) => return Err(ReleaseError::Version(err)),
    };
    let tag = format!("v{version}");

    // Step 1: version bump, with two independent sub-predicates so a
    // partially-completed step resumes correctly (D-06): writing/committing
    // and pushing are checked and acted on independently, so a run that
    // wrote+committed but failed to push (or vice versa) resumes correctly
    // next time.
    let on_disk = crate::version::read_version(project_root)?;
    let write_needed = (on_disk.major, on_disk.minor, on_disk.patch)
        < (version.major, version.minor, version.patch);
    let bumped = if write_needed {
        crate::version::write_version(project_root, &version)?;
        let version_file = crate::version::detect_version_file(project_root).ok_or_else(|| {
            ReleaseError::Version(crate::version::VersionError::Parse(
                "no version file found".into(),
            ))
        })?;
        let file_name = version_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Cargo.toml");
        // Scoped to the version file's own path, never `commit_all` — a
        // caller must never sweep unrelated dirty state into this commit.
        crate::git::GitFlow::new(project_root)
            .commit_path(file_name, &format!("chore: bump version to {version}"))?;
        true
    } else {
        false
    };

    let local_tip = git_output(project_root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    // Different question from `origin_main_ancestor_status` (`git.rs:508`),
    // which answers `origin/main` versus `HEAD` and is not parameterizable
    // — this is not a second implementation of the same check.
    let push_needed = !is_ancestor(project_root, &local_tip, "origin/develop");
    let pushed = if push_needed {
        crate::git::GitFlow::new(project_root).push_ref(crate::config::DEVELOP)?;
        true
    } else {
        false
    };

    let step1_status = if bumped || pushed {
        StepStatus::Completed
    } else {
        StepStatus::Skipped
    };
    let step1_detail = match (bumped, pushed) {
        (true, true) => format!("wrote and committed version {version}, pushed develop to origin"),
        (true, false) => format!(
            "wrote and committed version {version}; origin/develop already contains the tip"
        ),
        (false, true) => {
            format!("version file already declared {version}; pushed develop to origin")
        }
        (false, false) => format!(
            "version file already declared {version} and origin/develop already contains \
             the tip — nothing to do"
        ),
    };
    let step1_detail = match resume_note {
        Some(note) => format!("{note}; {step1_detail}"),
        None => step1_detail,
    };
    steps.push(StepReport {
        step: ReleaseStep::VersionBump,
        status: step1_status,
        detail: sanitize_changelog_subject(&step1_detail),
    });

    // Step 2: the human-gated boundary (D-02). Content-based, not
    // ancestry-based, because `main` squash-merges (an ancestry test would
    // never become true).
    let version_file = crate::version::detect_version_file(project_root).ok_or_else(|| {
        ReleaseError::Version(crate::version::VersionError::Parse(
            "no version file found".into(),
        ))
    })?;
    let relative = version_file
        .strip_prefix(project_root)
        .unwrap_or(version_file.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    let show_ref = format!("origin/main:{relative}");
    let main_declared = git_output(project_root, &["show", &show_ref])
        .ok()
        .and_then(|contents| crate::version::version_in_contents(&version_file, &contents).ok());

    let boundary_passed = matches!(
        &main_declared,
        Some(declared)
            if (declared.major, declared.minor, declared.patch)
                == (version.major, version.minor, version.patch)
    );

    if !boundary_passed {
        let declared_desc = main_declared
            .map(|v| v.to_string())
            .unwrap_or_else(|| "no readable version".to_string());
        steps.push(StepReport {
            step: ReleaseStep::Tag,
            status: StepStatus::Skipped,
            detail: sanitize_changelog_subject(&format!(
                "halted at the human gate: origin/main declares {declared_desc}, release \
                 version is {version} — open and merge the develop -> main release PR, then \
                 re-run devflow's release executor"
            )),
        });
        return Ok((version.to_string(), tag, ReleaseOutcome::HaltedAtHumanGate));
    }

    // Step 3: the signed release tag (D-10, D-06, B8). Reaches here only
    // once step 2's content-based boundary has confirmed `origin/main`
    // declares the release version. No signing-viability check of any kind
    // is added before, during, or after this step — this project's existing
    // SSH signing-viability predictor gains no new caller here (D-10).
    let main_tip = git_output(project_root, &["rev-parse", "origin/main"])?
        .trim()
        .to_string();
    let tag_state = crate::git::release_tag_state(project_root, &tag, &main_tip);
    match &tag_state {
        crate::git::ReleaseTagState::Released => {
            steps.push(StepReport {
                step: ReleaseStep::Tag,
                status: StepStatus::Skipped,
                detail: sanitize_changelog_subject(&format!(
                    "{tag} is already an annotated, verified, pushed release tag — nothing to do"
                )),
            });
        }
        crate::git::ReleaseTagState::Absent => {
            crate::git::create_signed_release_tag(project_root, &tag, &main_tip)?;
            crate::git::GitFlow::new(project_root).push_ref(&tag)?;
            let requery = crate::git::release_tag_state(project_root, &tag, &main_tip);
            if requery != crate::git::ReleaseTagState::Released {
                return Err(ReleaseError::TagCollision {
                    tag: tag.clone(),
                    detail: sanitize_changelog_subject(&format!(
                        "expected Released immediately after creating and pushing the tag, \
                         got {requery:?}"
                    )),
                });
            }
            steps.push(StepReport {
                step: ReleaseStep::Tag,
                status: StepStatus::Completed,
                detail: sanitize_changelog_subject(&format!(
                    "created and pushed the signed release tag {tag}"
                )),
            });
        }
        crate::git::ReleaseTagState::PresentUnverified { reason } => {
            if local_tag_is_verifiable(project_root, &tag) {
                crate::git::GitFlow::new(project_root).push_ref(&tag)?;
                let requery = crate::git::release_tag_state(project_root, &tag, &main_tip);
                if requery != crate::git::ReleaseTagState::Released {
                    return Err(ReleaseError::TagCollision {
                        tag: tag.clone(),
                        detail: sanitize_changelog_subject(&format!(
                            "expected Released immediately after pushing an existing \
                             verifiable tag, got {requery:?}"
                        )),
                    });
                }
                steps.push(StepReport {
                    step: ReleaseStep::Tag,
                    status: StepStatus::Completed,
                    detail: sanitize_changelog_subject(&format!(
                        "pushed the existing verifiable signed release tag {tag}"
                    )),
                });
            } else {
                return Err(ReleaseError::TagCollision {
                    tag: tag.clone(),
                    detail: sanitize_changelog_subject(reason),
                });
            }
        }
        crate::git::ReleaseTagState::StrayLightweight => {
            return Err(ReleaseError::TagCollision {
                tag: tag.clone(),
                detail: sanitize_changelog_subject(&format!(
                    "{tag} already exists as a lightweight (non-annotated) tag — refusing to \
                     delete or re-point it"
                )),
            });
        }
        crate::git::ReleaseTagState::Mismatched { tagged_commit } => {
            return Err(ReleaseError::TagCollision {
                tag: tag.clone(),
                detail: sanitize_changelog_subject(&format!(
                    "{tag} already exists and points at {tagged_commit}, not the released \
                     commit {main_tip} — refusing to delete or re-point it"
                )),
            });
        }
    }

    // Step 4: sync origin/main back into develop — D-07's second entry
    // point into the identical `sync_main_to_develop` implementation the
    // standalone `devflow sync` subcommand calls. No option is passed and no
    // part of the sync logic is reimplemented here.
    match crate::sync::sync_main_to_develop(project_root) {
        Ok(crate::sync::SyncOutcome::AlreadyAncestor) => {
            steps.push(StepReport {
                step: ReleaseStep::Sync,
                status: StepStatus::Skipped,
                detail: sanitize_changelog_subject(
                    "origin/main is already an ancestor of develop — nothing to sync",
                ),
            });
        }
        Ok(crate::sync::SyncOutcome::Merged { merge_commit }) => {
            steps.push(StepReport {
                step: ReleaseStep::Sync,
                status: StepStatus::Completed,
                detail: sanitize_changelog_subject(&format!(
                    "merged origin/main back into develop at {merge_commit}"
                )),
            });
        }
        Err(err) => return Err(ReleaseError::Sync(err)),
    }

    // Step 5: publish (D-04) — the one irreversible operation in the
    // sequence. `publish_order`'s sequence is consulted exactly once and
    // never sorted or reordered. An unclassifiable registry check stops the
    // run immediately, before publishing anything further — never treated
    // as permission to proceed (D-05).
    let packages = crate::git::publish_order(project_root);
    if packages.is_empty() {
        // C-04: an empty publish set is NOT a completed release. It is not an
        // error either — `publish_order` reads a Cargo workspace's `members`,
        // so a single-crate or non-Rust project legitimately resolves to
        // nothing, and this executor also serves those. What it must never do
        // is let the caller print an unqualified "release cut complete" after
        // tagging and pushing while crates.io received nothing, which is a
        // false green on the one irreversible step. The distinct outcome
        // forces the caller to say so.
        steps.push(StepReport {
            step: ReleaseStep::Publish,
            status: StepStatus::Skipped,
            detail: sanitize_changelog_subject(
                "no workspace members were resolved by publish_order — NOTHING was published",
            ),
        });
        return Ok((
            version.to_string(),
            tag,
            ReleaseOutcome::CompletedWithoutPublish,
        ));
    }

    for package in &packages {
        match crate::git::crate_already_published(project_root, package, &version.to_string()) {
            Ok(true) => {
                steps.push(StepReport {
                    step: ReleaseStep::Publish,
                    status: StepStatus::Skipped,
                    detail: sanitize_changelog_subject(&format!(
                        "{package}@{version} is already published"
                    )),
                });
            }
            Ok(false) => {
                crate::git::cargo_publish(project_root, package)?;
                steps.push(StepReport {
                    step: ReleaseStep::Publish,
                    status: StepStatus::Completed,
                    detail: sanitize_changelog_subject(&format!("published {package}@{version}")),
                });
            }
            Err(err) => return Err(ReleaseError::Publish(err)),
        }
    }

    Ok((version.to_string(), tag, ReleaseOutcome::Completed))
}

/// Independently answer two narrow questions about a LOCAL tag with two
/// cheap real git calls, rather than string-matching
/// `ReleaseTagState::PresentUnverified`'s `reason` (which deliberately
/// collapses two distinct causes — see that variant's doc comment):
/// is the tag object's type `tag` (annotated, not lightweight), and does
/// `git tag -v` exit successfully? Two real commands answer this directly —
/// the same "do it and read the real result" principle this project's
/// D-10 decision states for signing viability, applied here to a narrower
/// question this file's existing SSH signing-viability predictor does not
/// answer at all.
fn local_tag_is_verifiable(project_root: &Path, tag: &str) -> bool {
    let is_annotated = Command::new("git")
        .args(["cat-file", "-t", tag])
        .current_dir(project_root)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "tag")
        .unwrap_or(false);
    if !is_annotated {
        return false;
    }
    Command::new("git")
        .args(["tag", "-v", tag])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEVELOP;
    use crate::git::GitFlow;
    use crate::sync::tests::{init_bare_remote, init_repo};

    /// A minimal workspace `Cargo.toml` fixture carrying a
    /// `[workspace.package] version` and a `[workspace.dependencies]`
    /// self-pin, so `write_version`'s two-place rewrite is exercised. No
    /// `[workspace] members` list, so `publish_order` resolves to no
    /// members (Task 3's empty-publish-order case) — the literal substring
    /// "members" never appears in this fixture.
    fn workspace_cargo_toml(version: &str) -> String {
        format!(
            "[workspace.package]\nversion = \"{version}\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = {{ path = \"crates/devflow-core\", version = \"{version}\" }}\n"
        )
    }

    fn commit_file(root: &Path, name: &str, content: &str, message: &str) {
        std::fs::write(root.join(name), content).unwrap();
        crate::test_support::git_command(root)
            .args(["add", name])
            .status()
            .expect("git add");
        crate::test_support::git_command(root)
            .args(["commit", "-q", "-m", message])
            .status()
            .expect("git commit");
    }

    fn rev_parse(root: &Path, rev: &str) -> String {
        let output = crate::test_support::git_command(root)
            .args(["rev-parse", rev])
            .output()
            .expect("spawn git rev-parse");
        assert!(
            output.status.success(),
            "git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Build a fixture on `develop` with a bare `origin`, one `feat:` commit
    /// (so `compute_version` derives a deterministic minor bump from a zero
    /// baseline), and a version file declaring `old_version` on BOTH
    /// `origin/main` and `origin/develop` — older than whatever
    /// `compute_version` will derive, and identical on both branches so the
    /// human-gate boundary check (step 2) reads a real, matching value from
    /// `origin/main`. `develop`'s `feat:` commit lands only on `develop`, not
    /// pushed yet — `execute_release`'s own step 1 push is what lands it,
    /// which is the behavior under test.
    fn fixture_with_older_version(old_version: &str) -> (tempfile::TempDir, tempfile::TempDir) {
        let repo = init_repo();
        let root = repo.path();
        let bare = init_bare_remote(root);
        let flow = GitFlow::new(root);

        // develop: add the version file, push so origin/develop has it.
        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml(old_version),
            "chore: add version file",
        );
        flow.push_ref(DEVELOP)
            .expect("push develop with version file");

        // main: the identical version file content, as a parallel commit —
        // origin/main then declares old_version too.
        flow.checkout("main").expect("checkout main");
        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml(old_version),
            "chore: add version file",
        );
        flow.push_ref("main").expect("push main with version file");

        // Back to develop for the feat commit that drives the version bump.
        flow.checkout(DEVELOP).expect("checkout develop");
        commit_file(root, "feature.txt", "feature", "feat: add a feature");

        (repo, bare)
    }

    #[test]
    fn version_bump_pushes_develop() {
        let (repo, bare) = fixture_with_older_version("0.0.1");
        let root = repo.path();

        let remote_before = rev_parse(bare.path(), "refs/heads/develop");

        let report = execute_release(root).expect("execute_release must succeed");

        let version_step = report
            .steps
            .iter()
            .find(|s| s.step == ReleaseStep::VersionBump)
            .expect("a VersionBump step report must exist");
        assert_eq!(
            version_step.status,
            StepStatus::Completed,
            "version bump must be Completed when the version file was stale"
        );

        let remote_after = rev_parse(bare.path(), "refs/heads/develop");
        assert_ne!(
            remote_before, remote_after,
            "the remote develop ref must have advanced"
        );
        assert!(
            is_ancestor(root, &remote_before, &remote_after),
            "the remote ref must have advanced by fast-forward"
        );

        let tag_exists = crate::test_support::git_command(root)
            .args(["tag", "-l", &report.tag])
            .output()
            .map(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty())
            .unwrap_or(false);
        assert!(!tag_exists, "no local tag must exist yet");
        let remote_tag = crate::test_support::git_command(bare.path())
            .args(["tag", "-l", &report.tag])
            .output()
            .map(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty())
            .unwrap_or(false);
        assert!(!remote_tag, "no remote tag must exist yet");

        assert_eq!(report.outcome, ReleaseOutcome::HaltedAtHumanGate);
    }

    #[test]
    fn skips_push_when_already_ahead() {
        let (repo, bare) = fixture_with_older_version("0.0.1");
        let root = repo.path();

        // Run once to land the bump and push.
        let first = execute_release(root).expect("first execute_release must succeed");
        let release_version = first.version.clone();

        let remote_before = rev_parse(bare.path(), "refs/heads/develop");

        let second = execute_release(root).expect("second execute_release must succeed");
        assert_eq!(second.version, release_version);
        let version_step = second
            .steps
            .iter()
            .find(|s| s.step == ReleaseStep::VersionBump)
            .expect("a VersionBump step report must exist");
        assert_eq!(version_step.status, StepStatus::Skipped);

        let remote_after = rev_parse(bare.path(), "refs/heads/develop");
        assert_eq!(
            remote_before, remote_after,
            "the remote develop ref must be byte-identical on the skip path"
        );
    }

    #[test]
    fn halts_at_the_human_gate_when_main_does_not_declare_the_release() {
        let (repo, _bare) = fixture_with_older_version("0.0.1");
        let root = repo.path();

        let report = execute_release(root).expect("execute_release must succeed");
        assert_eq!(report.outcome, ReleaseOutcome::HaltedAtHumanGate);

        let detail = report
            .steps
            .iter()
            .map(|s| s.detail.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            detail.contains("0.0.1"),
            "detail must name the version origin/main declares: {detail}"
        );
        assert!(
            detail.contains(&report.version),
            "detail must name the release version: {detail}"
        );

        let tag_exists = crate::test_support::git_command(root)
            .args(["tag", "-l", &report.tag])
            .output()
            .map(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty())
            .unwrap_or(false);
        assert!(!tag_exists, "no tag must be created on a halted run");
    }

    /// Shared state for Task 2's tag-step fixtures: `develop` and
    /// `origin/main` both declare the same release version — a "human
    /// already merged the PR" state — and `origin/main`'s tip is NOT an
    /// ancestor of `develop` (a squash-shaped commit, mirroring this
    /// repository's real merge topology). `keys` is kept alive for its
    /// `Drop` side effect only (the throwaway keypair files it owns), never
    /// read directly — `#[allow(dead_code)]` records that deliberately.
    struct TagFixture {
        repo: tempfile::TempDir,
        bare: tempfile::TempDir,
        #[allow(dead_code)]
        keys: Option<tempfile::TempDir>,
        version: String,
        tag: String,
    }

    /// Build a [`TagFixture`]. `configure_signing` controls whether repo-local
    /// SSH tag signing is configured (`crate::git::tests::configure_ssh_tag_signing`
    /// — reused rather than building a second signing fixture, per this
    /// plan's resolved design note). `pre_bump_develop` controls whether
    /// `develop`'s version bump (write + commit + push) already landed
    /// before the fixture returns, versus leaving it for `execute_release`'s
    /// own step 1 to perform during the call under test. `pre_create_tag`
    /// controls whether the real signed release tag already exists at
    /// `origin/main`'s tip (via `create_signed_release_tag`, pushed).
    fn build_tag_fixture(
        configure_signing: bool,
        pre_bump_develop: bool,
        pre_create_tag: bool,
    ) -> TagFixture {
        let repo = init_repo();
        let root = repo.path();
        let bare = init_bare_remote(root);
        let keys = configure_signing.then(|| crate::git::tests::configure_ssh_tag_signing(root));
        let flow = GitFlow::new(root);

        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml("0.0.1"),
            "chore: add version file",
        );
        flow.push_ref(DEVELOP)
            .expect("push develop with version file");
        commit_file(root, "feature.txt", "feature", "feat: add a feature");

        let version = crate::version::compute_version(root).expect("compute_version");
        let version_str = version.to_string();
        let tag_name = format!("v{version_str}");

        // main: a squash-shaped commit declaring the SAME release version —
        // NOT an ancestor of develop, mirroring this repository's real
        // squash-merge topology (the resolved D-02 design note).
        flow.checkout("main").expect("checkout main");
        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml(&version_str),
            &format!("chore: release {version_str} (squash)"),
        );
        flow.push_ref("main")
            .expect("push main with release version");
        let main_tip = rev_parse(root, "main");

        if pre_create_tag {
            crate::git::create_signed_release_tag(root, &tag_name, &main_tip)
                .expect("create_signed_release_tag");
            flow.push_ref(&tag_name).expect("push release tag");
        }

        flow.checkout(DEVELOP).expect("checkout develop");
        if pre_bump_develop {
            crate::version::write_version(root, &version).expect("write_version");
            crate::git::GitFlow::new(root)
                .commit_path(
                    "Cargo.toml",
                    &format!("chore: bump version to {version_str}"),
                )
                .expect("commit version bump");
            flow.push_ref(DEVELOP)
                .expect("push develop with release version");
        }

        TagFixture {
            repo,
            bare,
            keys,
            version: version_str,
            tag: tag_name,
        }
    }

    /// B8: an already-released tag is a proven no-op — the tag object's SHA
    /// is byte-identical across the call, not merely "the step reported
    /// Skipped" — and the run proceeds past the tag step rather than
    /// halting.
    #[test]
    fn skips_tag_when_already_released() {
        let fixture = build_tag_fixture(true, true, true);
        let root = fixture.repo.path();

        let tag_sha_before = rev_parse(root, &format!("refs/tags/{}", fixture.tag));

        let report = execute_release(root).expect("execute_release must succeed");

        let tag_step = report
            .steps
            .iter()
            .find(|s| s.step == ReleaseStep::Tag)
            .expect("a Tag step report must exist");
        assert_eq!(tag_step.status, StepStatus::Skipped);

        let tag_sha_after = rev_parse(root, &format!("refs/tags/{}", fixture.tag));
        assert_eq!(
            tag_sha_before, tag_sha_after,
            "the tag object's SHA must be byte-identical across the call"
        );
        assert_ne!(
            report.outcome,
            ReleaseOutcome::HaltedAtHumanGate,
            "the run must proceed past the tag step rather than halting"
        );
    }

    /// B11: a mid-sequence failure (no signing key configured) leaves the
    /// already-completed version-bump step landed — the bump runs for real
    /// during THIS call (fixture leaves develop un-bumped), then the tag
    /// step fails, and nothing is rolled back (D-05).
    #[test]
    fn partial_failure_leaves_prior_steps_landed() {
        let fixture = build_tag_fixture(false, false, false);
        let root = fixture.repo.path();

        let result = execute_release(root);
        let err = result.expect_err("execute_release must fail without a signing key");
        let rendered = err.to_string();
        assert!(
            rendered.contains("devflow.releaseSigningKey"),
            "the rendered error must name the missing config key: {rendered}"
        );

        // C-03: the failure must carry the ledger of what already landed —
        // the version-bump step ran for real and was pushed, and a bare
        // error would leave the operator unable to tell.
        let version_step = err
            .steps
            .iter()
            .find(|s| s.step == ReleaseStep::VersionBump)
            .expect("the failure must report the VersionBump step that already landed");
        assert_eq!(
            version_step.status,
            StepStatus::Completed,
            "the landed version bump must be reported as Completed on the failure path"
        );
        assert!(
            !err.steps.iter().any(|s| s.step == ReleaseStep::Tag),
            "no Tag step may be reported — the tag step is what failed: {:?}",
            err.steps
        );

        let local_tip = rev_parse(root, "HEAD");
        let remote_tip = rev_parse(fixture.bare.path(), "refs/heads/develop");
        assert_eq!(
            local_tip, remote_tip,
            "the version-bump commit pushed by step 1 must remain on the remote — no rollback"
        );

        let on_disk = crate::version::read_version(root).expect("read_version");
        assert_eq!(
            on_disk.to_string(),
            fixture.version,
            "the version file must still declare the release version — step 1 really ran"
        );

        let tag_list = crate::test_support::git_command(root)
            .args(["tag", "-l"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_default();
        assert!(tag_list.is_empty(), "no tag must exist: {tag_list}");
    }

    /// A stray lightweight tag of the release name is refused, never
    /// auto-resolved (D-05) — the tag is untouched after the refusal and no
    /// annotated tag of that name is created.
    ///
    /// The stray tag is placed on `main` (off `develop`'s ancestry), not on
    /// `develop`'s own `HEAD`: any tag whose name parses as semver affects
    /// `compute_version`'s own tag scan (`highest_semver_tag` is
    /// reachability-blind by design — RESEARCH.md Pitfall 2) regardless of
    /// where it points, so placing it at develop's `HEAD` would make
    /// `compute_version` treat it as an unreachable-baseline resume signal
    /// with a DIFFERENT derived version, never reaching the tag step with
    /// the SAME tag name this test means to collide with. Placed on `main`
    /// instead, `compute_version` correctly resumes to the identical
    /// `fixture.tag` name via the same `UnreachableBaseline` path — a real,
    /// intentional interaction with step 1, not a workaround around it.
    #[test]
    fn refuses_a_stray_lightweight_tag_rather_than_skipping() {
        let fixture = build_tag_fixture(true, true, false);
        let root = fixture.repo.path();
        let flow = GitFlow::new(root);
        flow.checkout("main").expect("checkout main");
        flow.tag(&fixture.tag).expect("create lightweight tag");
        flow.checkout(DEVELOP).expect("checkout develop");
        let lightweight_sha = rev_parse(root, &fixture.tag);

        let result = execute_release(root);
        match result {
            Err(ReleaseFailure {
                error: ReleaseError::TagCollision { tag, .. },
                ..
            }) => {
                assert_eq!(tag, fixture.tag);
            }
            other => panic!("expected Err(ReleaseError::TagCollision {{ .. }}), got {other:?}"),
        }

        let after_sha = rev_parse(root, &fixture.tag);
        assert_eq!(
            lightweight_sha, after_sha,
            "the stray lightweight tag must be untouched"
        );
        let object_type = crate::test_support::git_command(root)
            .args(["cat-file", "-t", &fixture.tag])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_default();
        assert_eq!(
            object_type, "commit",
            "the tag must still be lightweight — never turned into an annotated tag"
        );
    }

    /// The full sequence, completed in one call: version bump (already
    /// satisfied — Skipped), tag (Absent — Completed), sync (Skipped — see
    /// below), publish (empty `publish_order` — Skipped).
    ///
    /// `main` is fast-forwarded to `develop`'s own tip (same commit, same
    /// content) rather than given a separate squash-shaped commit, and NO
    /// tag is pre-created. Both choices are load-bearing, not arbitrary:
    /// `compute_version`'s baseline scan is reachability-blind for
    /// `highest_semver_tag` but reachability-scoped for
    /// `reachable_semver_baseline` — a tag already reachable from `develop`
    /// (as any tag on a commit that IS `develop`'s ancestor would be) is
    /// treated as an ALREADY-COMPLETE prior release, and `compute_version`
    /// derives the NEXT version past it (see the module doc comment's
    /// documented consequence) — which `origin/main`'s still-unchanged
    /// content could then never match at step 2. Avoiding a pre-existing
    /// tag sidesteps that entirely: this fixture models the ordinary,
    /// non-squashed case (`develop` and `main` coincide, no release cut yet)
    /// where `origin/main` being an ancestor of `develop` and the tag being
    /// `Absent` are simultaneously true and internally consistent.
    #[test]
    fn completes_the_sequence_and_reports_every_step() {
        let repo = init_repo();
        let root = repo.path();
        let _bare = init_bare_remote(root);
        let _keys = crate::git::tests::configure_ssh_tag_signing(root);
        let flow = GitFlow::new(root);

        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml("0.0.1"),
            "chore: add version file",
        );
        flow.push_ref(DEVELOP)
            .expect("push develop with version file");
        commit_file(root, "feature.txt", "feature", "feat: add a feature");

        let version = crate::version::compute_version(root).expect("compute_version");
        let version_str = version.to_string();
        crate::version::write_version(root, &version).expect("write_version");
        crate::git::GitFlow::new(root)
            .commit_path(
                "Cargo.toml",
                &format!("chore: bump version to {version_str}"),
            )
            .expect("commit version bump");
        flow.push_ref(DEVELOP)
            .expect("push develop with release version");

        // main: fast-forward to develop's exact tip — origin/main is
        // trivially an ancestor of develop (self-ancestor), and no tag
        // exists yet anywhere in the repo.
        flow.checkout("main").expect("checkout main");
        crate::test_support::git_command(root)
            .args(["merge", "--ff-only", DEVELOP])
            .status()
            .expect("fast-forward main to develop");
        flow.push_ref("main").expect("push main");
        flow.checkout(DEVELOP).expect("checkout develop");

        let report = execute_release(root).expect("execute_release must succeed");
        // C-04: this fixture's `publish_order` resolves no members, so the
        // registry receives nothing. That must NOT report as `Completed` —
        // the whole sequence ran, but a caller rendering this as "release cut
        // complete" would be announcing a publish that never happened.
        assert_eq!(
            report.outcome,
            ReleaseOutcome::CompletedWithoutPublish,
            "an empty publish order must be reported distinctly, never as a complete release"
        );

        let step_order: Vec<ReleaseStep> = report.steps.iter().map(|s| s.step).collect();
        assert_eq!(
            step_order,
            vec![
                ReleaseStep::VersionBump,
                ReleaseStep::Tag,
                ReleaseStep::Sync,
                ReleaseStep::Publish,
            ],
            "expected one entry per ReleaseStep in sequence order: {step_order:?}"
        );
    }

    /// Reuse 26-04's own tree-mismatch construction (`aborts_on_tree_mismatch`
    /// in `sync.rs`) on top of Task 2's already-released-tag fixture shape:
    /// `origin/main` carries one extra file `develop` lacks, added BEFORE the
    /// release tag is created so the tag correctly names the tip the sync's
    /// `-X ours` merge will actually pull in (adding the file afterward would
    /// leave `origin/main`'s freshly-fetched tip ahead of the tag, making
    /// step 3 see `Mismatched` instead of `Released` and never reach sync at
    /// all).
    #[test]
    fn a_refused_sync_stops_the_run_before_publishing() {
        let repo = init_repo();
        let root = repo.path();
        let _bare = init_bare_remote(root);
        let _keys = crate::git::tests::configure_ssh_tag_signing(root);
        let flow = GitFlow::new(root);

        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml("0.0.1"),
            "chore: add version file",
        );
        flow.push_ref(DEVELOP)
            .expect("push develop with version file");
        commit_file(root, "feature.txt", "feature", "feat: add a feature");

        let version = crate::version::compute_version(root).expect("compute_version");
        let version_str = version.to_string();
        let tag_name = format!("v{version_str}");

        flow.checkout("main").expect("checkout main");
        commit_file(
            root,
            "Cargo.toml",
            &workspace_cargo_toml(&version_str),
            &format!("chore: release {version_str} (squash)"),
        );
        // The extra file develop lacks, added BEFORE the tag so the tag
        // names the tip the sync merge will actually pull in.
        commit_file(
            root,
            "new-from-main.txt",
            "brand new",
            "chore: main-only file",
        );
        flow.push_ref("main")
            .expect("push main with release version and extra file");
        let main_tip = rev_parse(root, "main");

        crate::git::create_signed_release_tag(root, &tag_name, &main_tip)
            .expect("create_signed_release_tag");
        flow.push_ref(&tag_name).expect("push release tag");

        flow.checkout(DEVELOP).expect("checkout develop");
        crate::version::write_version(root, &version).expect("write_version");
        crate::git::GitFlow::new(root)
            .commit_path(
                "Cargo.toml",
                &format!("chore: bump version to {version_str}"),
            )
            .expect("commit version bump");
        flow.push_ref(DEVELOP)
            .expect("push develop with release version");

        let result = execute_release(root);
        let err = result.expect_err("execute_release must fail on a tree-changing sync");
        match &err.error {
            ReleaseError::Sync(crate::sync::SyncError::TreeChanged {
                before_tree,
                after_tree,
            }) => {
                assert_ne!(before_tree, after_tree);
                let rendered = err.to_string();
                assert!(
                    rendered.contains(before_tree) && rendered.contains(after_tree),
                    "rendered TreeChanged refusal must name both SHAs: {rendered}"
                );
            }
            other => panic!(
                "expected Err(ReleaseError::Sync(SyncError::TreeChanged {{ .. }})), got {other:?}"
            ),
        }

        // C-03: the ledger now survives the failure, so "the run stopped
        // before publishing" is asserted directly against the reported steps
        // rather than inferred from `ReleaseError` having had no `steps`
        // field. The sync step is the one that failed, so it reports nothing
        // either.
        assert!(
            !err.steps
                .iter()
                .any(|s| matches!(s.step, ReleaseStep::Publish | ReleaseStep::Sync)),
            "no Sync or Publish step may be reported: {:?}",
            err.steps
        );
        // W-18/WR-01: exactly one entry per step, in sequence order — the
        // resume arm used to push a second `VersionBump` report and fall
        // through into step 1, and this fixture traverses that arm.
        let reported: Vec<ReleaseStep> = err.steps.iter().map(|s| s.step).collect();
        assert_eq!(
            reported,
            vec![ReleaseStep::VersionBump, ReleaseStep::Tag],
            "the two steps that landed before the refused sync must each be reported once"
        );
    }

    /// C-01: a semver tag that is unreachable from `develop` but does NOT
    /// name `origin/main`'s tip is a stray leftover (an ordinary
    /// squash-merged phase tag — `hooks_after_ship` creates one per phase),
    /// not an in-flight release. It must be refused BEFORE any mutation.
    ///
    /// The original code adopted such a tag's version as a resume, then
    /// wrote, committed, and pushed it to `origin/develop` before step 2's
    /// human gate ever ran. This asserts the observable consequences a
    /// deferred check could not satisfy: the remote `develop` ref and the
    /// on-disk version file are both byte-identical across the refusal.
    #[test]
    fn refuses_a_stray_unreachable_tag_instead_of_adopting_its_version() {
        let (repo, bare) = fixture_with_older_version("0.0.1");
        let root = repo.path();
        let flow = GitFlow::new(root);

        // A high semver tag on an abandoned branch: reachable from neither
        // `develop` nor `origin/main`, exactly what a squash-merged phase
        // branch leaves behind.
        flow.checkout("main").expect("checkout main");
        commit_file(root, "abandoned.txt", "abandoned", "chore: abandoned work");
        flow.tag("v9.9.9").expect("create the stray semver tag");
        crate::test_support::git_command(root)
            .args(["reset", "--hard", "HEAD~1"])
            .status()
            .expect("abandon the tagged commit");
        flow.checkout(DEVELOP).expect("checkout develop");

        let remote_before = rev_parse(bare.path(), "refs/heads/develop");
        let version_before = std::fs::read_to_string(root.join("Cargo.toml")).expect("read");

        let err = execute_release(root).expect_err("a stray unreachable tag must be refused");
        match &err.error {
            ReleaseError::StrayBaselineTag { tag, .. } => assert_eq!(tag, "v9.9.9"),
            other => panic!("expected StrayBaselineTag, got {other:?}"),
        }
        let rendered = err.to_string();
        assert!(
            rendered.contains("v9.9.9"),
            "the refusal must name the stray tag: {rendered}"
        );
        assert!(
            err.steps.is_empty(),
            "the refusal happens before step 1, so no step may report: {:?}",
            err.steps
        );

        assert_eq!(
            remote_before,
            rev_parse(bare.path(), "refs/heads/develop"),
            "origin/develop must be byte-identical — nothing may be pushed"
        );
        assert_eq!(
            version_before,
            std::fs::read_to_string(root.join("Cargo.toml")).expect("read"),
            "the version file must be byte-identical — nothing may be written"
        );
        assert!(
            !rev_parse(root, "HEAD").is_empty(),
            "sanity: the repo is still usable"
        );
    }
}

//! Stage-transition hooks.
//!
//! Branching, docs, changelog, and version bumps are no longer workflow stages
//! (as they were in v0.x). They are *hooks* that fire at specific stage
//! transitions. [`hooks_for_transition`] maps a `(from, to)` stage move to the
//! hooks that should run, and [`Hook::run`] executes one.

use crate::config::GitFlowConfig;
use crate::git::GitFlow;
use crate::stage::Stage;
use crate::version;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// A side-effecting action that fires at a stage transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    /// Create the `feature/phase-NN` branch from develop.
    BranchCreate,
    /// Delete the merged feature branch after Ship.
    BranchCleanup,
    /// Regenerate and commit docs.
    DocsUpdate,
    /// Merge the phase feature branch into develop before release bookkeeping.
    Merge,
    /// Append a CHANGELOG entry.
    ChangelogAppend,
    /// Compute and write the next version, then tag it.
    VersionBump,
}

/// Context passed to every hook.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Phase the workflow is on.
    pub phase: u32,
    /// Project root.
    pub project_root: PathBuf,
    /// Stage the workflow is entering.
    pub stage: Stage,
    /// Git-flow branch model.
    pub git_flow: GitFlowConfig,
    /// The version `VersionBump` actually tagged, set once it runs (GAP-7).
    /// `ChangelogAppend` reads this instead of re-deriving the version from
    /// disk, so the changelog heading and the git tag never desync — in
    /// particular when there is no version file and `version::read_version`
    /// would otherwise error and fall back to the `unreleased` literal.
    pub shipped_version: Option<String>,
    /// The Keep-a-Changelog-grouped body `VersionBump` computed, set once it
    /// runs (D-12, T-26-11). `ChangelogAppend` reads this instead of
    /// re-deriving it from live git state, for the same reason
    /// `shipped_version` must be handed forward rather than re-derived:
    /// once `VersionBump` has created the release tag, the range this body
    /// was computed over collapses to empty (the tag is now the baseline),
    /// so a re-derivation after the fact would silently produce an empty
    /// changelog entry.
    pub shipped_changelog_body: Option<String>,
}

/// Errors produced by hooks.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// A git-flow operation failed.
    #[error(transparent)]
    Git(#[from] crate::git::GitError),
    /// A version operation failed.
    #[error(transparent)]
    Version(#[from] version::VersionError),
    /// Filesystem operation failed.
    #[error("hook I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

impl Hook {
    /// Run this hook against the given context.
    pub fn run(&self, ctx: &mut HookContext) -> Result<(), HookError> {
        match self {
            Hook::BranchCreate => branch_create(ctx),
            Hook::BranchCleanup => branch_cleanup(ctx),
            Hook::DocsUpdate => docs_update(ctx),
            Hook::Merge => merge_feature(ctx),
            Hook::ChangelogAppend => changelog_append(ctx),
            Hook::VersionBump => version_bump(ctx),
        }
    }
}

/// Which hooks fire when moving `from` → `to`.
///
/// - Validate → Ship: docs are finalized before shipping.
/// - Ship → (done): merge + version bump + changelog + branch cleanup.
/// - everything else: none.
///
/// `ChangelogAppend` deliberately does NOT run here (WR-04, 17-12): a
/// changelog heading naming a release is only true once `VersionBump` has
/// actually cut the tag, and `VersionBump` runs in [`hooks_after_ship`],
/// strictly after this transition.
pub fn hooks_for_transition(from: Stage, to: Stage) -> Vec<Hook> {
    match (from, to) {
        (Stage::Validate, Stage::Ship) => vec![Hook::DocsUpdate],
        _ => Vec::new(),
    }
}

/// Hooks that fire after Ship completes (the workflow's terminal transition).
///
/// `ChangelogAppend` runs strictly after `VersionBump` (WR-04, 17-12) — the
/// entry must describe the version `VersionBump` actually wrote and tagged,
/// never a version computed independently of it. It runs before
/// `BranchCleanup` so a changelog failure still stops short of deleting the
/// feature branch (`run_checkout_hooks`' terminal-batch fail-fast breaks on
/// the first error in this batch).
pub fn hooks_after_ship() -> Vec<Hook> {
    vec![
        Hook::Merge,
        Hook::VersionBump,
        Hook::ChangelogAppend,
        Hook::BranchCleanup,
    ]
}

fn branch_create(ctx: &HookContext) -> Result<(), HookError> {
    let git = GitFlow::new(&ctx.project_root);
    let branch = git.feature_start(ctx.phase)?;
    info!("BranchCreate: created {branch}");
    Ok(())
}

fn branch_cleanup(ctx: &HookContext) -> Result<(), HookError> {
    let git = GitFlow::new(&ctx.project_root);
    let branch = format!("{}phase-{:02}", ctx.git_flow.feature_prefix, ctx.phase);
    if git.branch_exists(&branch) {
        // Non-force cleanup is intentional: never discard unmerged work.
        match git.delete_branch(&branch, false) {
            Ok(()) => info!("BranchCleanup: deleted {branch}"),
            Err(err) => {
                let message = err.to_string();
                if message.contains("not fully merged") || message.contains("not yet merged") {
                    warn!(
                        "BranchCleanup: feature branch {branch} is not merged yet — left in place"
                    );
                } else {
                    warn!("BranchCleanup: could not delete {branch}: {err}");
                }
            }
        }
    }
    Ok(())
}

/// Merge the phase's feature branch into develop, then re-assert ancestry
/// (23-06 / T-23-62) before reporting success.
///
/// **The post-merge ancestry re-check runs here — immediately after
/// `merge_feature_into_develop` returns `Ok`, while the feature branch still
/// exists — because this is the only place in `hooks_after_ship` where the
/// assertion is both meaningful and safe.** `BranchCleanup` runs later in the
/// same batch and deletes the branch; after that, an ancestry check fails
/// closed on an absent branch (`git.rs:89-92`: "an absent branch is not proof
/// of a merge") and would report `false` for every successfully shipped
/// phase, so this check can never be moved after the batch without inverting
/// its meaning.
///
/// **No-rollback policy, stated here because it must not be re-derived
/// later:** on the ancestry re-check's failure path below, `merge_feature`
/// does NOT undo the merge. `git merge --no-ff` has already committed on
/// `develop` by the time the re-check runs, and automatically resetting a
/// shared integration branch is a far more dangerous operation than the
/// inconsistency it would be papering over. Instead, this returns `Err`; the
/// containing `run_checkout_hooks` batch fails; `finish_workflow_with_gate_timeout`
/// reopens an actionable Ship gate whose context tells a human to resolve the
/// git error, and the operator decides. Plan 23-10's recovery-path artifact
/// must know this exact state.
fn merge_feature(ctx: &HookContext) -> Result<(), HookError> {
    let git = GitFlow::new(&ctx.project_root);
    let branch = format!("{}phase-{:02}", ctx.git_flow.feature_prefix, ctx.phase);
    if !git.branch_exists(&branch) {
        return Err(crate::git::GitError::Command(format!(
            "feature branch `{branch}` is missing; refusing to report an unproven merge"
        ))
        .into());
    }
    if git.is_merged_into_develop(ctx.phase) {
        info!("Merge: {branch} is already merged; nothing to merge");
        crate::events::emit(
            &ctx.project_root,
            ctx.phase,
            "merge_result",
            serde_json::json!({"merged": false, "branch": branch}),
        );
        return Ok(());
    }

    git.merge_feature_into_develop(ctx.phase)?;

    if !git.is_merged_into_develop(ctx.phase) {
        crate::events::emit(
            &ctx.project_root,
            ctx.phase,
            "merge_result",
            serde_json::json!({"merged": false, "branch": branch}),
        );
        return Err(crate::git::GitError::Command(format!(
            "merge of `{branch}` reported success but the branch is still not an ancestor of \
             develop; refusing to report an unproven merge"
        ))
        .into());
    }

    info!("Merge: merged {branch} into develop");
    crate::events::emit(
        &ctx.project_root,
        ctx.phase,
        "merge_result",
        serde_json::json!({"merged": true, "branch": branch}),
    );
    Ok(())
}

fn docs_update(ctx: &HookContext) -> Result<(), HookError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("cargo doc --no-deps 2>&1")
        .current_dir(&ctx.project_root)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            // Commit any doc changes; ignore "nothing to commit".
            let git = GitFlow::new(&ctx.project_root);
            if let Err(err) = git.commit_all("docs: update generated docs") {
                warn!("DocsUpdate: commit failed: {err}");
            } else {
                info!("DocsUpdate: docs regenerated and committed");
            }
        }
        Ok(_) => warn!("DocsUpdate: cargo doc reported a failure; skipping commit"),
        Err(err) => warn!("DocsUpdate: could not run cargo doc: {err}"),
    }
    Ok(())
}

fn changelog_append(ctx: &mut HookContext) -> Result<(), HookError> {
    // Prefer the version VersionBump (which runs immediately before this
    // hook in hooks_after_ship()) actually tagged, handed through
    // batch-scoped context state (GAP-7) — this is the only source that's
    // correct with no version file present. Fall back to
    // version::read_version (deliberately NOT version::compute_version,
    // which recomputes MINOR from the live git tag count that VersionBump's
    // own tag just incremented, yielding a version one higher than the tag
    // actually cut — WR-04, 17-12), then to the `unreleased` literal.
    let version = ctx.shipped_version.clone().unwrap_or_else(|| {
        version::read_version(&ctx.project_root)
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "unreleased".to_string())
    });
    let body = ctx.shipped_changelog_body.as_deref().unwrap_or("");
    let path = ctx.project_root.join("CHANGELOG.md");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = crate::ship::prepend_changelog(&existing, &version, &today(), body);
    std::fs::write(&path, updated)?;
    // Commit the write. Round 2's WR-04 finding: this hook used to write and
    // never commit, and docs_update — the only committing hook — ran first
    // in the old (Validate→Ship) batch order, so the entry was left dirty
    // and lost when Merge/BranchCleanup ran. Scoped to CHANGELOG.md (not
    // commit_all) so this hook never sweeps in unrelated dirty state. A
    // failed commit propagates as an error so the terminal batch's fail-fast
    // stops BranchCleanup from running against an uncommitted entry.
    let git = GitFlow::new(&ctx.project_root);
    git.commit_path(
        "CHANGELOG.md",
        &format!("docs: add changelog entry for {version}"),
    )?;
    info!("ChangelogAppend: wrote and committed entry for {version}");
    Ok(())
}

fn version_bump(ctx: &mut HookContext) -> Result<(), HookError> {
    let version = version::compute_version(&ctx.project_root)?;
    let git = GitFlow::new(&ctx.project_root);

    // D-12/T-26-11: compute the changelog body BEFORE `git.tag(&tag)` below.
    // Once that tag exists, `reachable_semver_baseline` resolves to it and
    // the range this body is computed over collapses to empty — the same
    // desync class WR-04/17-12 already documented for `shipped_version`. A
    // failure to compute the body must not abort the version bump: the
    // fallback line `prepend_changelog` substitutes for an empty body is a
    // correct degraded outcome, and a version bump must not fail on a
    // changelog-content problem.
    match version::reachable_semver_baseline(&ctx.project_root) {
        Ok(baseline) => {
            let range_start = match &baseline {
                Some(tag) => version::release_range_start(&ctx.project_root, &format!("v{tag}")),
                None => Ok(String::new()),
            };
            match range_start.and_then(|range_start| {
                version::changelog_sections(&ctx.project_root, &range_start)
            }) {
                Ok(sections) => {
                    ctx.shipped_changelog_body = Some(version::render_changelog_body(&sections));
                }
                Err(err) => {
                    warn!("VersionBump: could not compute changelog body: {err}");
                }
            }
        }
        Err(err) => {
            warn!("VersionBump: could not resolve changelog baseline: {err}");
        }
    }

    // Write the computed version into the version file when one exists, and
    // commit that write before tagging (17-12: previously left uncommitted,
    // so the tag named a version the tagged commit itself didn't contain,
    // and the working tree stayed dirty through the rest of the terminal
    // batch — the same "write without committing" defect WR-04 named for
    // ChangelogAppend, just not called out there).
    if has_version_file(&ctx.project_root) {
        let path = version::write_version(&ctx.project_root, &version)?;
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            git.commit_path(name, &format!("chore: bump version to {version}"))?;
        }
        info!("VersionBump: wrote {version} to {}", path.display());
    } else {
        warn!("VersionBump: no supported version file; tagging only");
    }
    let tag = format!("v{version}");
    git.tag(&tag)?;
    // Hand the tagged version to ChangelogAppend via batch-scoped context
    // state (GAP-7) — on both branches above, since both tag. Without this,
    // ChangelogAppend re-derives the version from disk and, with no version
    // file, falls back to the `unreleased` literal while the tag names a
    // real version.
    ctx.shipped_version = Some(version.to_string());
    info!("VersionBump: tagged {tag}");
    Ok(())
}

/// Today's date as YYYY-MM-DD (best-effort via the `date` command).
///
/// `pub(crate)` (29-05): the release-cut executor's `prepare_bump_branch`
/// reuses this exact helper for its own changelog entry's date, rather than
/// a second `date`-shelling implementation.
pub(crate) fn today() -> String {
    Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unreleased".to_string())
}

/// Whether a project has a version file, used by callers to decide if a version
/// bump is meaningful.
pub fn has_version_file(project_root: &Path) -> bool {
    version::detect_version_file(project_root).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let ok = crate::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn init_repo(root: &Path) {
        init_repo_with_options(root, true);
    }

    /// Same as [`init_repo`], but lets a test choose whether a version file
    /// gets written. `init_repo` unconditionally wrote `Cargo.toml`, which
    /// made `version_bump`'s no-version-file `else` branch unreachable from
    /// the batch tests (GAP-7). `init_repo` delegates here with `true`, so
    /// its observable effect for every existing test is unchanged byte for
    /// byte.
    fn init_repo_with_options(root: &Path, write_version_file: bool) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        git(root, &["config", "core.hooksPath", "/dev/null"]);
        if write_version_file {
            std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
        } else {
            std::fs::write(root.join("README.md"), "no version file in this repo\n").unwrap();
        }
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "init"]);
        git(root, &["branch", "-M", "main"]);
        git(root, &["checkout", "-q", "-b", "develop"]);
    }

    fn ctx(root: &Path, stage: Stage) -> HookContext {
        HookContext {
            phase: 11,
            project_root: root.to_path_buf(),
            stage,
            git_flow: GitFlowConfig::default(),
            shipped_version: None,
            shipped_changelog_body: None,
        }
    }

    #[test]
    fn transition_map_finalizes_docs_only_before_ship() {
        // WR-04 (17-12): ChangelogAppend no longer fires here — a changelog
        // heading naming a release can't be true before VersionBump (which
        // runs in hooks_after_ship, strictly after this transition) cuts the
        // tag it describes.
        assert_eq!(
            hooks_for_transition(Stage::Validate, Stage::Ship),
            vec![Hook::DocsUpdate]
        );
        assert!(hooks_for_transition(Stage::Define, Stage::Plan).is_empty());
        assert!(hooks_for_transition(Stage::Code, Stage::Validate).is_empty());
    }

    #[test]
    fn validate_to_ship_hooks_do_not_touch_changelog() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let mut context = ctx(dir.path(), Stage::Ship);

        for hook in hooks_for_transition(Stage::Validate, Stage::Ship) {
            hook.run(&mut context).unwrap();
        }

        assert!(!dir.path().join("CHANGELOG.md").exists());
    }

    #[test]
    fn after_ship_runs_version_changelog_then_cleanup() {
        // WR-04 (17-12): ChangelogAppend strictly after VersionBump (so it
        // can read back the version VersionBump just tagged), and before
        // BranchCleanup (so a changelog failure still stops short of
        // deleting the feature branch).
        assert_eq!(
            hooks_after_ship(),
            vec![
                Hook::Merge,
                Hook::VersionBump,
                Hook::ChangelogAppend,
                Hook::BranchCleanup,
            ]
        );
    }

    #[test]
    fn branch_create_makes_feature_branch() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        Hook::BranchCreate
            .run(&mut ctx(dir.path(), Stage::Define))
            .unwrap();
        assert!(GitFlow::new(dir.path()).branch_exists("feature/phase-11"));
    }

    #[test]
    fn changelog_append_writes_entry() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        Hook::ChangelogAppend
            .run(&mut ctx(dir.path(), Stage::Ship))
            .unwrap();
        let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
        assert!(changelog.contains("# Changelog"));
    }

    #[test]
    fn changelog_append_commits_its_own_write() {
        // WR-04 (Round 2, 17-12): changelog_append must not leave its write
        // uncommitted — that's what let the entry get orphaned when
        // BranchCleanup ran before it.
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        Hook::ChangelogAppend
            .run(&mut ctx(dir.path(), Stage::Ship))
            .unwrap();

        let status = git_output(dir.path(), &["status", "--porcelain"]);
        assert!(status.is_empty(), "expected clean tree, got: {status}");

        let committed_files = git_output(dir.path(), &["log", "-1", "--name-only"]);
        assert!(
            committed_files.contains("CHANGELOG.md"),
            "expected CHANGELOG.md in the latest commit, got: {committed_files}"
        );
    }

    #[test]
    fn version_bump_tags_repo() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Hybrid SemVer: major 2 (Cargo.toml), minor 0 (no tags), patch from
        // the commit count since the last tag — one `init` commit → v2.0.1.
        let expected = format!("v{}", version::compute_version(dir.path()).unwrap());
        Hook::VersionBump
            .run(&mut ctx(dir.path(), Stage::Ship))
            .unwrap();
        let tags = crate::test_support::git_command(dir.path())
            .arg("tag")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&tags.stdout).contains(&expected));
    }

    #[test]
    fn terminal_hooks_version_post_merge_develop() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        git(dir.path(), &["checkout", "-q", "-b", "feature/phase-11"]);
        std::fs::write(dir.path().join("feature.txt"), "phase work\n").unwrap();
        git(dir.path(), &["add", "feature.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "phase work"]);

        let feature_tip = git_output(dir.path(), &["rev-parse", "feature/phase-11"]);
        let pre_merge_count = git_output(dir.path(), &["rev-list", "--count", "HEAD"]);

        let mut context = ctx(dir.path(), Stage::Ship);
        for hook in hooks_after_ship() {
            hook.run(&mut context).unwrap();
        }

        git(
            dir.path(),
            &["merge-base", "--is-ancestor", &feature_tip, "develop"],
        );
        let post_merge_count = git_output(dir.path(), &["rev-list", "--count", "develop"]);
        assert_ne!(pre_merge_count, post_merge_count);

        // Exactly one tag was created, and it names the version VersionBump
        // actually wrote to the version file (not a raw rev-list count,
        // which would now also include VersionBump's own commit and
        // ChangelogAppend's — both introduced by 17-12).
        let all_tags = git_output(dir.path(), &["tag"]);
        assert_eq!(all_tags.lines().count(), 1, "expected exactly one tag");
        let tag = all_tags.trim().to_string();
        let version_file_version = version::read_version(dir.path()).unwrap().to_string();
        assert_eq!(tag, format!("v{version_file_version}"));

        // The tag no longer points at develop's tip — ChangelogAppend's
        // commit (17-12) lands after it.
        let develop_tip = git_output(dir.path(), &["rev-parse", "develop"]);
        let tag_commit = git_output(dir.path(), &["rev-parse", &format!("{tag}^{{commit}}")]);
        assert_ne!(develop_tip, tag_commit);
    }

    #[test]
    fn after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean() {
        // Full regression for WR-04 (17-12): drives the whole hooks_after_ship
        // batch and asserts three-way agreement between the changelog
        // heading, the created git tag, and the version file's version —
        // plus the Round 2 WR-04 commit requirement (clean tree, CHANGELOG.md
        // present in a commit). Must fail against pre-17-12 main: the old
        // batch order never ran ChangelogAppend here at all (it fired at
        // Validate→Ship, before any tag existed), so CHANGELOG.md would not
        // exist after running only hooks_after_ship().
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Merge fires events::emit, which creates .devflow/ — gitignored in
        // every real project (WR-11); mirror that here so the clean-tree
        // assertion below checks hook writes, not test-fixture telemetry.
        std::fs::write(dir.path().join(".gitignore"), ".devflow/\n").unwrap();
        git(dir.path(), &["add", ".gitignore"]);
        git(dir.path(), &["commit", "-q", "-m", "add gitignore"]);
        git(dir.path(), &["checkout", "-q", "-b", "feature/phase-11"]);
        std::fs::write(dir.path().join("feature.txt"), "phase work\n").unwrap();
        git(dir.path(), &["add", "feature.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "phase work"]);

        let mut context = ctx(dir.path(), Stage::Ship);
        for hook in hooks_after_ship() {
            hook.run(&mut context).unwrap();
        }

        // Exactly one tag was created by this batch (init_repo creates none).
        let all_tags = git_output(dir.path(), &["tag"]);
        assert_eq!(all_tags.lines().count(), 1, "expected exactly one tag");
        let tag = all_tags.trim().to_string();

        let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
        let changelog_version = changelog
            .lines()
            .find(|l| l.starts_with("## "))
            .and_then(|l| l.trim_start_matches("## ").split(' ').next())
            .unwrap()
            .to_string();

        let version_file_version = version::read_version(dir.path()).unwrap().to_string();

        assert_eq!(
            tag,
            format!("v{changelog_version}"),
            "tag must match the changelog heading version"
        );
        assert_eq!(
            changelog_version, version_file_version,
            "changelog heading must match the version file's version"
        );

        // Round 2 WR-04: the changelog write must be committed, and the
        // working tree must be clean after the full batch.
        let status = git_output(dir.path(), &["status", "--porcelain"]);
        assert!(status.is_empty(), "expected clean tree, got: {status}");
        let committed_files = git_output(dir.path(), &["log", "-1", "--name-only"]);
        assert!(
            committed_files.contains("CHANGELOG.md"),
            "expected CHANGELOG.md in the latest commit, got: {committed_files}"
        );
    }

    #[test]
    fn after_ship_batch_with_no_version_file_keeps_tag_and_changelog_in_sync() {
        // GAP-7: with no version file, version_bump takes the `else` branch
        // (warns, tags only) and still tags v{compute_version()}.
        // changelog_append then calls version::read_version, which errors
        // with no version file present, and falls back to the literal
        // "unreleased" -- desyncing the tag from the changelog heading.
        // init_repo unconditionally writes Cargo.toml, so this branch is
        // unreachable from the other batch tests; init_repo_with_options
        // reaches it without changing init_repo's own behavior.
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_options(dir.path(), false);
        // Mirror the existing batch test's .gitignore / feature-branch setup
        // so the run is comparable.
        std::fs::write(dir.path().join(".gitignore"), ".devflow/\n").unwrap();
        git(dir.path(), &["add", ".gitignore"]);
        git(dir.path(), &["commit", "-q", "-m", "add gitignore"]);
        git(dir.path(), &["checkout", "-q", "-b", "feature/phase-11"]);
        std::fs::write(dir.path().join("feature.txt"), "phase work\n").unwrap();
        git(dir.path(), &["add", "feature.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "phase work"]);

        let mut context = ctx(dir.path(), Stage::Ship);
        for hook in hooks_after_ship() {
            hook.run(&mut context).unwrap();
        }

        let all_tags = git_output(dir.path(), &["tag"]);
        assert_eq!(all_tags.lines().count(), 1, "expected exactly one tag");
        let tag = all_tags.trim().to_string();
        let tag_version = tag
            .strip_prefix('v')
            .expect("tag should be prefixed with v")
            .to_string();

        let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
        let changelog_version = changelog
            .lines()
            .find(|l| l.starts_with("## "))
            .and_then(|l| l.trim_start_matches("## ").split(' ').next())
            .unwrap()
            .to_string();

        assert_ne!(
            changelog_version, "unreleased",
            "changelog heading must name the tagged version, not fall back to the literal"
        );
        assert_eq!(
            changelog_version, tag_version,
            "changelog heading must match the git tag ({tag}) even with no version file"
        );
    }

    #[test]
    fn merge_succeeds_while_feature_branch_is_checked_out_in_linked_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let worktree = dir.path().join("phase-worktree");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/phase-11",
                worktree.to_str().unwrap(),
                "develop",
            ],
        );
        std::fs::write(worktree.join("feature.txt"), "phase work\n").unwrap();
        git(&worktree, &["add", "feature.txt"]);
        git(&worktree, &["commit", "-q", "-m", "phase work"]);

        Hook::Merge.run(&mut ctx(&repo, Stage::Ship)).unwrap();

        git(
            &repo,
            &["merge-base", "--is-ancestor", "feature/phase-11", "develop"],
        );
        assert!(GitFlow::new(&repo).branch_exists("feature/phase-11"));
    }

    #[test]
    fn branch_cleanup_is_fail_soft_when_branch_absent() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // No feature branch exists — cleanup must still succeed.
        Hook::BranchCleanup
            .run(&mut ctx(dir.path(), Stage::Ship))
            .unwrap();
    }

    #[test]
    fn merge_fails_closed_when_branch_absent() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Branch absence cannot prove that phase work reached develop.
        let error = Hook::Merge
            .run(&mut ctx(dir.path(), Stage::Ship))
            .unwrap_err();
        assert!(error.to_string().contains("unproven merge"));
    }

    /// 23-06 Task 2 acceptance: a real merge through the hook, with the new
    /// post-merge ancestry re-check present, still succeeds and still
    /// records a `merge_result` event with `merged: true` — proving the
    /// added assertion is a no-op on the happy path it re-confirms.
    #[test]
    fn merge_through_hook_records_true_merged_result_after_ancestry_reconfirmed() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        git(dir.path(), &["checkout", "-q", "-b", "feature/phase-11"]);
        std::fs::write(dir.path().join("feature.txt"), "phase work\n").unwrap();
        git(dir.path(), &["add", "feature.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "phase work"]);
        git(dir.path(), &["checkout", "-q", "develop"]);

        Hook::Merge.run(&mut ctx(dir.path(), Stage::Ship)).unwrap();

        assert!(GitFlow::new(dir.path()).is_merged_into_develop(11));
        let last = crate::events::last_event_for_phase(dir.path(), 11)
            .expect("merge_result event recorded");
        assert_eq!(last["event"], "merge_result");
        assert_eq!(last["merged"], true);
        assert_eq!(last["branch"], "feature/phase-11");
    }

    /// 23-06 Task 2: the pre-existing missing-branch refusal is unchanged —
    /// it still short-circuits before the merge (and before the new
    /// post-condition) ever runs, so it never even reaches the event log.
    #[test]
    fn merge_fails_closed_when_branch_absent_emits_no_merge_result_event() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());

        let _ = Hook::Merge.run(&mut ctx(dir.path(), Stage::Ship));

        assert!(
            crate::events::last_event_for_phase(dir.path(), 11).is_none(),
            "a missing feature branch must short-circuit before any event is emitted"
        );
    }

    fn git_output(root: &Path, args: &[&str]) -> String {
        let output = crate::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// D-12 end-to-end: `VersionBump` computes and hands forward the
    /// changelog body, `ChangelogAppend` writes it — a real `feat:` commit
    /// produces a `### Added` section naming that commit's subject in the
    /// actual `CHANGELOG.md` file the hook wrote (a file contract, not an
    /// internal return value). Reverting only `version_bump`'s body-capture
    /// hunk makes this fail on the `### Added` assertion below, not on a
    /// compile error or a fixture panic.
    #[test]
    fn changelog_append_writes_the_generated_body_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        git(
            dir.path(),
            &[
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "feat: add the widget endpoint",
            ],
        );

        let mut context = ctx(dir.path(), Stage::Ship);
        Hook::VersionBump.run(&mut context).unwrap();
        Hook::ChangelogAppend.run(&mut context).unwrap();

        let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
        assert!(
            changelog.contains("### Added"),
            "expected a ### Added section, got: {changelog}"
        );
        assert!(
            changelog.contains("add the widget endpoint"),
            "expected the feat commit's subject, got: {changelog}"
        );
    }
}

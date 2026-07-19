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
    pub fn run(&self, ctx: &HookContext) -> Result<(), HookError> {
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

fn changelog_append(ctx: &HookContext) -> Result<(), HookError> {
    // Read back the version VersionBump (which runs immediately before this
    // hook in hooks_after_ship()) actually wrote and tagged. Deliberately
    // NOT version::compute_version — that recomputes MINOR from the live git
    // tag count, which VersionBump's own tag just incremented, yielding a
    // version one higher than the tag actually cut (WR-04, 17-12).
    let version = version::read_version(&ctx.project_root)
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "unreleased".to_string());
    let path = ctx.project_root.join("CHANGELOG.md");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = crate::ship::prepend_changelog(&existing, &version, &today());
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

fn version_bump(ctx: &HookContext) -> Result<(), HookError> {
    let version = version::compute_version(&ctx.project_root)?;
    let git = GitFlow::new(&ctx.project_root);
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
    info!("VersionBump: tagged {tag}");
    Ok(())
}

/// Today's date as YYYY-MM-DD (best-effort via the `date` command).
fn today() -> String {
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
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn init_repo(root: &Path) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        git(root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
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
        let context = ctx(dir.path(), Stage::Ship);

        for hook in hooks_for_transition(Stage::Validate, Stage::Ship) {
            hook.run(&context).unwrap();
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
            .run(&ctx(dir.path(), Stage::Define))
            .unwrap();
        assert!(GitFlow::new(dir.path()).branch_exists("feature/phase-11"));
    }

    #[test]
    fn changelog_append_writes_entry() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        Hook::ChangelogAppend
            .run(&ctx(dir.path(), Stage::Ship))
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
            .run(&ctx(dir.path(), Stage::Ship))
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
            .run(&ctx(dir.path(), Stage::Ship))
            .unwrap();
        let tags = Command::new("git")
            .arg("tag")
            .current_dir(dir.path())
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

        let context = ctx(dir.path(), Stage::Ship);
        for hook in hooks_after_ship() {
            hook.run(&context).unwrap();
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

        let context = ctx(dir.path(), Stage::Ship);
        for hook in hooks_after_ship() {
            hook.run(&context).unwrap();
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

        Hook::Merge.run(&ctx(&repo, Stage::Ship)).unwrap();

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
            .run(&ctx(dir.path(), Stage::Ship))
            .unwrap();
    }

    #[test]
    fn merge_fails_closed_when_branch_absent() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Branch absence cannot prove that phase work reached develop.
        let error = Hook::Merge.run(&ctx(dir.path(), Stage::Ship)).unwrap_err();
        assert!(error.to_string().contains("unproven merge"));
    }

    fn git_output(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}

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
/// - Validate → Ship: docs + changelog are finalized before shipping.
/// - Ship → (done): merge + version bump + branch cleanup.
/// - everything else: none.
pub fn hooks_for_transition(from: Stage, to: Stage) -> Vec<Hook> {
    match (from, to) {
        (Stage::Validate, Stage::Ship) => vec![Hook::DocsUpdate, Hook::ChangelogAppend],
        _ => Vec::new(),
    }
}

/// Hooks that fire after Ship completes (the workflow's terminal transition).
pub fn hooks_after_ship() -> Vec<Hook> {
    vec![Hook::Merge, Hook::VersionBump, Hook::BranchCleanup]
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
    if git.is_merged_into_develop(ctx.phase) {
        info!("Merge: {branch} is already merged or absent; nothing to merge");
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
    let version = version::compute_version(&ctx.project_root)
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "unreleased".to_string());
    let path = ctx.project_root.join("CHANGELOG.md");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = crate::ship::prepend_changelog(&existing, &version, &today());
    std::fs::write(&path, updated)?;
    info!("ChangelogAppend: wrote entry for {version}");
    Ok(())
}

fn version_bump(ctx: &HookContext) -> Result<(), HookError> {
    let version = version::compute_version(&ctx.project_root)?;
    // Write the computed version into the version file when one exists.
    if has_version_file(&ctx.project_root) {
        let path = version::write_version(&ctx.project_root, &version)?;
        info!("VersionBump: wrote {version} to {}", path.display());
    } else {
        warn!("VersionBump: no supported version file; tagging only");
    }
    let git = GitFlow::new(&ctx.project_root);
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
    fn transition_map_finalizes_docs_and_changelog_before_ship() {
        assert_eq!(
            hooks_for_transition(Stage::Validate, Stage::Ship),
            vec![Hook::DocsUpdate, Hook::ChangelogAppend]
        );
        assert!(hooks_for_transition(Stage::Define, Stage::Plan).is_empty());
        assert!(hooks_for_transition(Stage::Code, Stage::Validate).is_empty());
    }

    #[test]
    fn validate_to_ship_hooks_append_changelog() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let context = ctx(dir.path(), Stage::Ship);

        for hook in hooks_for_transition(Stage::Validate, Stage::Ship) {
            hook.run(&context).unwrap();
        }

        let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
        assert!(changelog.contains("## "));
    }

    #[test]
    fn after_ship_runs_version_and_cleanup() {
        assert_eq!(
            hooks_after_ship(),
            vec![Hook::Merge, Hook::VersionBump, Hook::BranchCleanup]
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

        let tag = git_output(dir.path(), &["tag", "--points-at", "develop"]);
        assert_eq!(tag, format!("v2.0.{post_merge_count}"));
        assert_ne!(tag, format!("v2.0.{pre_merge_count}"));
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
    fn merge_is_fail_soft_when_branch_absent() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // No feature branch exists — merge must report a no-op and succeed.
        Hook::Merge.run(&ctx(dir.path(), Stage::Ship)).unwrap();
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

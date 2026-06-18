//! Git worktree operations implemented with plain `git worktree` commands.
//!
//! Worktrees give each coding agent an isolated working directory that shares
//! the main repository's object database. DevFlow places them under
//! `<project_root>/.worktrees/` so they are easy to find and clean up.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Errors produced by worktree operations.
#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    /// Spawning git failed.
    #[error("failed to execute git: {0}")]
    Io(#[from] std::io::Error),
    /// Git returned a non-success status.
    #[error("git worktree command failed: {0}")]
    Command(String),
    /// The target worktree path already exists.
    #[error("worktree path already exists: {0}")]
    Exists(PathBuf),
}

/// One entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree's working directory.
    pub path: PathBuf,
    /// Checked-out branch (short name), or `None` for a detached HEAD.
    pub branch: Option<String>,
    /// HEAD commit SHA.
    pub head: String,
}

/// The `.worktrees` directory for a project root.
pub fn worktrees_dir(project_root: &Path) -> PathBuf {
    project_root.join(".worktrees")
}

/// Worktree path for a phase: `.worktrees/phase-NN`.
pub fn phase_path(project_root: &Path, phase: u32) -> PathBuf {
    worktrees_dir(project_root).join(format!("phase-{phase:02}"))
}

/// Worktree path for a single agent on a phase: `.worktrees/phase-NN-<agent>`.
pub fn phase_agent_path(project_root: &Path, phase: u32, agent: &str) -> PathBuf {
    worktrees_dir(project_root).join(format!("phase-{phase:02}-{agent}"))
}

/// Worktree path for the static reference snapshot: `.worktrees/reference`.
pub fn reference_path(project_root: &Path) -> PathBuf {
    worktrees_dir(project_root).join("reference")
}

/// Add a worktree.
///
/// When `create_branch` is set, runs `git worktree add -b <branch> <path>
/// <start_point>` (creating `branch` off `start_point`). Otherwise runs
/// `git worktree add <path> <branch>` to check out an existing branch.
///
/// Returns [`WorktreeError::Exists`] if `path` already exists — callers decide
/// whether to remove-and-readd (refresh) or surface the error.
pub fn add(
    project_root: &Path,
    path: &Path,
    branch: &str,
    start_point: &str,
    create_branch: bool,
) -> Result<(), WorktreeError> {
    if path.exists() {
        return Err(WorktreeError::Exists(path.to_path_buf()));
    }
    let path_str = path.to_string_lossy();
    if create_branch {
        run(
            project_root,
            &["worktree", "add", "-b", branch, &path_str, start_point],
        )
    } else {
        run(project_root, &["worktree", "add", &path_str, branch])
    }
}

/// Remove a worktree directory via `git worktree remove [--force] <path>`.
pub fn remove(project_root: &Path, path: &Path, force: bool) -> Result<(), WorktreeError> {
    let path_str = path.to_string_lossy();
    if force {
        run(project_root, &["worktree", "remove", "--force", &path_str])
    } else {
        run(project_root, &["worktree", "remove", &path_str])
    }
}

/// Prune stale worktree administrative entries via `git worktree prune`.
pub fn prune(project_root: &Path) -> Result<(), WorktreeError> {
    run(project_root, &["worktree", "prune"])
}

/// List all worktrees for the repository by parsing `--porcelain` output.
pub fn list(project_root: &Path) -> Result<Vec<WorktreeInfo>, WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(WorktreeError::Command(stderr_or_status(&output)));
    }
    Ok(parse_porcelain(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse `git worktree list --porcelain` output.
///
/// Records are separated by blank lines. Each record has a `worktree <path>`
/// line, a `HEAD <sha>` line, and either `branch refs/heads/<name>` or
/// `detached`.
fn parse_porcelain(text: &str) -> Vec<WorktreeInfo> {
    let mut result = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut head = String::new();
    let mut branch: Option<String> = None;

    let mut flush = |path: &mut Option<PathBuf>, head: &mut String, branch: &mut Option<String>| {
        if let Some(p) = path.take() {
            result.push(WorktreeInfo {
                path: p,
                branch: branch.take(),
                head: std::mem::take(head),
            });
        } else {
            *head = String::new();
            *branch = None;
        }
    };

    for line in text.lines() {
        if line.is_empty() {
            flush(&mut path, &mut head, &mut branch);
            continue;
        }
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(p));
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            head = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.trim_start_matches("refs/heads/").to_string());
        }
        // `detached`, `bare`, `locked`, etc. leave `branch` as None.
    }
    // Final record (porcelain output may or may not end with a blank line).
    flush(&mut path, &mut head, &mut branch);
    result
}

fn run(project_root: &Path, args: &[&str]) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorktreeError::Command(stderr_or_status(&output)))
    }
}

fn stderr_or_status(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("exited with {}", output.status)
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Init a repo with `main` and `develop` and one commit.
    fn init_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "base"]);
        git(root, &["branch", "-M", "main"]);
        git(root, &["checkout", "-q", "-b", "develop"]);
        dir
    }

    #[test]
    fn path_helpers_format_phase_numbers() {
        let root = Path::new("/repo");
        assert_eq!(worktrees_dir(root), Path::new("/repo/.worktrees"));
        assert_eq!(phase_path(root, 7), Path::new("/repo/.worktrees/phase-07"));
        assert_eq!(
            phase_agent_path(root, 7, "claude"),
            Path::new("/repo/.worktrees/phase-07-claude")
        );
        assert_eq!(
            reference_path(root),
            Path::new("/repo/.worktrees/reference")
        );
    }

    #[test]
    fn add_creates_worktree_on_new_branch() {
        let repo = init_repo();
        let root = repo.path();
        let wt = phase_path(root, 7);

        add(root, &wt, "feature/phase-07", "develop", true).expect("add");

        assert!(wt.exists());
        assert!(wt.join("README.md").exists());

        let listing = list(root).expect("list");
        let entry = listing
            .iter()
            .find(|w| w.path.ends_with("phase-07") || w.path == wt)
            .expect("phase-07 worktree present");
        assert_eq!(entry.branch.as_deref(), Some("feature/phase-07"));
    }

    #[test]
    fn add_errors_when_path_exists() {
        let repo = init_repo();
        let root = repo.path();
        let wt = phase_path(root, 7);
        add(root, &wt, "feature/phase-07", "develop", true).expect("add");

        let err = add(root, &wt, "feature/phase-07b", "develop", true).unwrap_err();
        assert!(matches!(err, WorktreeError::Exists(_)));
    }

    #[test]
    fn list_includes_main_and_added_worktrees() {
        let repo = init_repo();
        let root = repo.path();
        let before = list(root).expect("list before");
        assert_eq!(before.len(), 1, "only the main worktree initially");

        add(
            root,
            &phase_path(root, 1),
            "feature/phase-01",
            "develop",
            true,
        )
        .expect("add");
        let after = list(root).expect("list after");
        assert_eq!(after.len(), 2);
        assert!(after.iter().any(|w| w.branch.as_deref() == Some("develop")));
        assert!(
            after
                .iter()
                .any(|w| w.branch.as_deref() == Some("feature/phase-01"))
        );
    }

    #[test]
    fn remove_deletes_the_worktree() {
        let repo = init_repo();
        let root = repo.path();
        let wt = phase_path(root, 2);
        add(root, &wt, "feature/phase-02", "develop", true).expect("add");
        assert!(wt.exists());

        remove(root, &wt, false).expect("remove");
        assert!(!wt.exists());
        let listing = list(root).expect("list");
        assert!(!listing.iter().any(|w| w.path == wt));
    }

    #[test]
    fn add_existing_branch_without_creating() {
        let repo = init_repo();
        let root = repo.path();
        // Create a branch in the main checkout, then check it out in a worktree.
        git(root, &["branch", "topic"]);
        let wt = worktrees_dir(root).join("topic-wt");
        add(root, &wt, "topic", "", false).expect("add existing branch");
        let listing = list(root).expect("list");
        assert!(listing.iter().any(|w| w.branch.as_deref() == Some("topic")));
    }

    #[test]
    fn parse_porcelain_handles_detached_and_trailing_record() {
        let text = "worktree /repo\nHEAD abc123\nbranch refs/heads/develop\n\
                    \nworktree /repo/.worktrees/phase-07\nHEAD def456\ndetached\n";
        let parsed = parse_porcelain(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, PathBuf::from("/repo"));
        assert_eq!(parsed[0].branch.as_deref(), Some("develop"));
        assert_eq!(parsed[0].head, "abc123");
        assert_eq!(parsed[1].path, PathBuf::from("/repo/.worktrees/phase-07"));
        assert_eq!(parsed[1].branch, None);
        assert_eq!(parsed[1].head, "def456");
    }

    #[test]
    fn prune_succeeds_on_clean_repo() {
        let repo = init_repo();
        prune(repo.path()).expect("prune");
    }
}

//! Git worktree operations implemented with plain `git worktree` commands.
//!
//! Worktrees give each coding agent an isolated working directory that shares
//! the main repository's object database. DevFlow places them under
//! `<project_root>/.worktrees/` so they are easy to find and clean up.

use crate::git::git_command;
use crate::phase_id::PhaseId;
use std::path::{Path, PathBuf};

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
pub fn phase_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    worktrees_dir(project_root).join(format!("phase-{padded}", padded = phase.padded()))
}

/// Worktree path for a single agent on a phase: `.worktrees/phase-NN-<agent>`.
pub fn phase_agent_path(project_root: &Path, phase: PhaseId, agent: &str) -> PathBuf {
    worktrees_dir(project_root).join(format!("phase-{padded}-{agent}", padded = phase.padded()))
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

/// Add a worktree checked out at `commitish` in **detached HEAD** state.
///
/// Used for the static reference snapshot: a branch already checked out in the
/// main worktree cannot be checked out again, but it can be snapshotted detached
/// at its tip.
pub fn add_detached(
    project_root: &Path,
    path: &Path,
    commitish: &str,
) -> Result<(), WorktreeError> {
    if path.exists() {
        return Err(WorktreeError::Exists(path.to_path_buf()));
    }
    let path_str = path.to_string_lossy();
    run(
        project_root,
        &["worktree", "add", "--detach", &path_str, commitish],
    )
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
    let output = git_command(project_root)
        .args(["worktree", "list", "--porcelain"])
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
    let output = git_command(project_root).args(args).output()?;
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
    use tempfile::TempDir;

    fn git(root: &Path, args: &[&str]) {
        let output = crate::test_support::git_command(root)
            .args(args)
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
        assert_eq!(
            phase_path(root, PhaseId::new(7)),
            Path::new("/repo/.worktrees/phase-07")
        );
        assert_eq!(
            phase_agent_path(root, PhaseId::new(7), "claude"),
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
        let wt = phase_path(root, PhaseId::new(7));

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
        let wt = phase_path(root, PhaseId::new(7));
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
            &phase_path(root, PhaseId::new(1)),
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
        let wt = phase_path(root, PhaseId::new(2));
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

    // -----------------------------------------------------------------
    // 27-02 (D-03/T-27-03): worktree::list is immune to a hostile GIT_DIR
    // -----------------------------------------------------------------

    /// D-03/T-27-03: `list` does NOT route through the `run` chokepoint and
    /// so needed its own, independent migration. Proven the same way
    /// 27-01's `origin_main_ancestor_status_holds_under_a_hostile_git_dir`
    /// proves immunity (no process-global env mutation — Rust 2024
    /// `unsafe`, unsound under threaded tests, Phase 25 D-14), in two
    /// parts: (a) the `Command` `list` builds via the scrubbing constructor
    /// is unconditionally scrubbed — no bypass parameter, no env-var check,
    /// no config lookup (D-01), asserted directly on the built `Command`;
    /// (b) the actual `list(real_root)`
    /// production function, called normally with nothing re-adding
    /// `GIT_DIR` afterward, reaches the correct answer — proven when THIS
    /// test itself runs under this crate's hostile-`GIT_DIR` harness
    /// (`GIT_DIR=<hostile>/.git cargo test ... \
    /// list_resolves_caller_root_under_a_hostile_git_dir`), whose OS-level
    /// env var this test's own process (and so `list`'s spawned child,
    /// unless scrubbed) inherits.
    ///
    /// A literal chained `.env("GIT_DIR", foreign)`-after-the-constructor
    /// reproduction of `list`'s own argv (the technique
    /// `hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir`
    /// uses for `--show-toplevel`) was deliberately NOT added here:
    /// empirically verified (this machine, git 2.55.0) that
    /// `worktree list --porcelain` genuinely IS redirected by an explicit
    /// `GIT_DIR` override chained after the scrub (`cd <real> && GIT_DIR=
    /// <foreign>/.git git worktree list --porcelain` enumerates
    /// `<foreign>`'s own single worktree entry, not `<real>`'s) — unlike
    /// `--show-toplevel`, which falls back to cwd when `GIT_WORK_TREE` is
    /// unset. This is exactly T-27-03's threat: an explicit/inherited
    /// `GIT_DIR` genuinely retargets this command, which is why the fix is
    /// scrubbing it away before it ever reaches the child, not proving the
    /// child resists an override that was never removed (same reasoning as
    /// 27-01's documented deviation for `merge-base --is-ancestor`).
    #[test]
    fn list_resolves_caller_root_under_a_hostile_git_dir() {
        let repo = init_repo();
        let root = repo.path();
        let wt_path = phase_path(root, PhaseId::new(9));
        let wt_str = wt_path.to_string_lossy();
        // Fixture setup goes through the already-scrubbed general
        // constructor directly (not the production `add()`, which itself
        // depends on `run()` — kept independent so this test proves only
        // `list`'s own immunity, not `run`'s).
        assert!(
            crate::git::git_command(root)
                .args([
                    "worktree",
                    "add",
                    "-b",
                    "feature/phase-09",
                    &wt_str,
                    "develop",
                ])
                .output()
                .unwrap()
                .status
                .success(),
            "git worktree add fixture setup failed"
        );

        // (a) unconditionally scrubbed.
        let cmd = crate::git::git_command(root);
        assert!(
            cmd.get_envs()
                .any(|(key, value)| key == "GIT_DIR" && value.is_none()),
            "list's own Command must mark GIT_DIR for removal"
        );

        // (b) the actual, scrubbed mechanism reaches the correct answer.
        let entries = list(root).expect("list must succeed");
        let canonical_root = std::fs::canonicalize(root).expect("canonicalize root");
        assert!(
            entries.len() >= 2,
            "expected at least main + added worktree, got: {entries:?}"
        );
        for entry in &entries {
            let canonical_entry =
                std::fs::canonicalize(&entry.path).expect("canonicalize entry path");
            assert!(
                canonical_entry.starts_with(&canonical_root),
                "worktree entry {canonical_entry:?} must be under real_root {canonical_root:?}"
            );
        }
        assert!(
            entries
                .iter()
                .any(|w| w.branch.as_deref() == Some("feature/phase-09")),
            "list must include the added worktree, got: {entries:?}"
        );
    }
}

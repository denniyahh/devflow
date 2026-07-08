//! Git-flow operations implemented with plain `git` commands.

use crate::config::GitFlowConfig;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

/// Errors produced by git-flow operations.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Spawning git failed.
    #[error("failed to execute git: {0}")]
    Io(#[from] std::io::Error),
    /// Git returned a non-success status.
    #[error("git command failed: {0}")]
    Command(String),
}

/// Repository helper bound to a project root.
#[derive(Debug, Clone)]
pub struct GitFlow {
    root: PathBuf,
    config: GitFlowConfig,
}

/// Summary of a feature branch for the `devflow list` command.
#[derive(Debug, Clone)]
pub struct BranchInfo {
    /// Branch name (e.g. "feature/phase-05").
    pub name: String,
    /// Number of commits this branch has that develop doesn't.
    pub ahead: usize,
    /// Number of commits develop has that this branch doesn't.
    pub behind: usize,
    /// ISO-8601 date of the last commit on this branch.
    pub last_commit: String,
}

impl GitFlow {
    /// Create a git-flow helper for a project root, using the hardcoded
    /// git-flow constants (`main`, `develop`, `feature/`).
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            config: GitFlowConfig::default(),
        }
    }

    /// Create a feature branch from the develop branch.
    ///
    /// Returns an error if the branch already exists (use
    /// [`feature_start_force`] to overwrite).
    pub fn feature_start(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        info!("creating feature branch: {branch}");
        self.git(["checkout", &self.config.develop])?;
        self.git(["checkout", "-b", &branch])?;
        Ok(branch)
    }

    /// Create or reset a feature branch, overwriting it if it already exists.
    pub fn feature_start_force(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        warn!("force-creating feature branch: {branch}");
        self.git(["checkout", &self.config.develop])?;
        self.git(["checkout", "-B", &branch])?;
        Ok(branch)
    }

    /// Merge a feature branch into develop and delete it.
    pub fn feature_finish(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        info!("finishing feature branch: {branch}");
        self.git(["checkout", &self.config.develop])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["branch", "-d", &branch])?;
        Ok(branch)
    }

    /// Create or reset a release branch from the current `HEAD`.
    ///
    /// The release branch is cut from wherever the caller currently is — the
    /// branch being shipped — not from `develop`. `devflow ship` writes the
    /// version bump into the working tree first, so branching from `HEAD`
    /// keeps any commits unique to the shipped branch in the release.
    pub fn release_start(&self, version: &str) -> Result<String, GitError> {
        let branch = format!("release/{version}");
        info!("creating release branch: {branch}");
        self.git(["checkout", "-B", &branch])?;
        Ok(branch)
    }

    /// Merge a release branch into main and develop, tag it, and delete it.
    pub fn release_finish(&self, version: &str) -> Result<String, GitError> {
        let branch = format!("release/{version}");
        info!("finishing release branch: {branch}");
        self.git(["checkout", &self.config.main])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["tag", &format!("v{version}")])?;
        self.git(["checkout", &self.config.develop])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["branch", "-d", &branch])?;
        Ok(branch)
    }

    /// Create an annotated-free lightweight tag at the current `HEAD`.
    pub fn tag(&self, tag: &str) -> Result<(), GitError> {
        info!("tagging {tag}");
        self.git(["tag", tag])
    }

    /// Delete a single local branch.
    ///
    /// With `force`, uses `git branch -D` (deletes even if unmerged); otherwise
    /// `git branch -d` (refuses to delete unmerged work). Protected branches
    /// (`main`, `develop`) are never deleted.
    pub fn delete_branch(&self, branch: &str, force: bool) -> Result<(), GitError> {
        if branch == self.config.main || branch == self.config.develop {
            return Err(GitError::Command(format!(
                "refusing to delete protected branch `{branch}`"
            )));
        }
        let flag = if force { "-D" } else { "-d" };
        if force {
            warn!("force-deleting branch: {branch}");
        } else {
            info!("deleting branch: {branch}");
        }
        self.git(["branch", flag, branch])
    }

    /// Whether a local branch exists.
    pub fn branch_exists(&self, branch: &str) -> bool {
        Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .current_dir(&self.root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// The commit SHA at the tip of `branch`.
    pub fn branch_tip(&self, branch: &str) -> Result<String, GitError> {
        Ok(self.git_output(["rev-parse", branch])?.trim().to_string())
    }

    /// Create `branch` at `start_point` if it does not already exist, without
    /// checking it out (leaves the current checkout untouched).
    pub fn ensure_branch(&self, branch: &str, start_point: &str) -> Result<(), GitError> {
        if self.branch_exists(branch) {
            return Ok(());
        }
        self.git(["branch", branch, start_point])
    }

    /// Fast-forward `target`'s ref to `source` (must be a descendant).
    ///
    /// `target` must not be checked out in any worktree. Errors if the move
    /// would not be a fast-forward.
    pub fn fast_forward_branch(&self, target: &str, source: &str) -> Result<(), GitError> {
        let is_ancestor = Command::new("git")
            .args(["merge-base", "--is-ancestor", target, source])
            .current_dir(&self.root)
            .output()?
            .status
            .success();
        if !is_ancestor {
            return Err(GitError::Command(format!(
                "{target} is not an ancestor of {source}; refusing non-fast-forward update"
            )));
        }
        self.git(["branch", "-f", target, source])
    }

    /// Rebase the branch checked out at `dir` onto `onto`.
    ///
    /// Runs `git rebase` inside the given worktree directory. On conflict the
    /// rebase is aborted and an error is returned so the caller can surface it.
    pub fn rebase_in(&self, dir: &Path, onto: &str) -> Result<(), GitError> {
        debug!("rebasing worktree at {} onto {onto}", dir.display());
        match git_in(dir, &["rebase", onto]) {
            Ok(()) => Ok(()),
            Err(err) => {
                // Leave the worktree clean for the user to retry.
                warn!("rebase conflict in {}; aborting", dir.display());
                let _ = git_in(dir, &["rebase", "--abort"]);
                Err(err)
            }
        }
    }

    /// Check out an existing branch in the main worktree.
    pub fn checkout(&self, branch: &str) -> Result<(), GitError> {
        debug!("checking out branch: {branch}");
        self.git(["checkout", branch])
    }

    /// Delete `branch` on `origin` (best-effort; errors if no remote/branch).
    pub fn delete_remote_branch(&self, branch: &str) -> Result<(), GitError> {
        info!("deleting remote branch: {branch}");
        self.git(["push", "origin", "--delete", branch])
    }

    /// Whether the repository has at least one configured remote.
    pub fn has_remote(&self) -> bool {
        self.git_output(["remote"])
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }

    /// Push `branch` to `origin`, setting upstream.
    pub fn push(&self, branch: &str) -> Result<(), GitError> {
        info!("pushing branch: {branch}");
        self.git(["push", "-u", "origin", branch])
    }

    /// Delete local branches already merged into the current branch.
    pub fn cleanup_merged(&self) -> Result<Vec<String>, GitError> {
        let output = self.git_output(["branch", "--merged"])?;
        let protected = [self.config.main.as_str(), self.config.develop.as_str()];
        let mut deleted = Vec::new();
        for line in output.lines() {
            let branch = line.trim().trim_start_matches('*').trim();
            if branch.is_empty() || protected.contains(&branch) {
                continue;
            }
            info!("cleaning up merged branch: {branch}");
            self.git(["branch", "-d", branch])?;
            deleted.push(branch.to_string());
        }
        Ok(deleted)
    }

    /// Stage all changes and commit with the given message.
    /// Returns Ok(()) whether or not there were changes to commit.
    pub fn commit_all(&self, message: &str) -> Result<(), GitError> {
        debug!("committing all changes: {message}");
        self.git(["add", "."])?;
        // --allow-empty so we don't fail when there are no changes
        match self.git_raw(&["commit", "--allow-empty", "-m", message]) {
            Ok(()) => Ok(()),
            // If the commit produced no changes and we used --allow-empty,
            // this should still succeed. But just in case, ignore "nothing to commit".
            Err(GitError::Command(ref msg)) if msg.contains("nothing to commit") => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Return divergence from develop: (ahead, behind) commit counts.
    ///
    /// If currently on the develop branch, returns (0, 0).
    /// `ahead` = commits on current branch not yet on develop.
    /// `behind` = commits on develop not yet on current branch.
    pub fn divergence_from_develop(&self) -> Result<(usize, usize), GitError> {
        let current = self
            .git_output(["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string();
        if current == self.config.develop {
            return Ok((0, 0));
        }
        let ahead = self
            .rev_count(&format!("{}..{current}", self.config.develop))
            .unwrap_or(0);
        let behind = self
            .rev_count(&format!("{current}..{}", self.config.develop))
            .unwrap_or(0);
        Ok((ahead, behind))
    }

    /// List all feature branches with divergence from develop.
    ///
    /// Returns branches matching `feature/phase-*` with ahead/behind counts
    /// and last commit dates. Protected branches (main, develop) are excluded.
    pub fn list_feature_branches(&self) -> Result<Vec<BranchInfo>, GitError> {
        let prefix = &self.config.feature_prefix;
        let branches = self.git_output(["branch", "--format=%(refname:short)"])?;
        let mut result = Vec::new();
        for name in branches.lines().map(|l| l.trim()) {
            if name.is_empty()
                || name == self.config.main
                || name == self.config.develop
                || !name.starts_with(prefix)
            {
                continue;
            }
            let ahead = self
                .rev_count(&format!("{dev}..{name}", dev = self.config.develop))
                .unwrap_or(0);
            let behind = self
                .rev_count(&format!("{name}..{dev}", dev = self.config.develop))
                .unwrap_or(0);
            let last_commit = self
                .git_output(["log", "-1", "--format=%aI", name])
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            result.push(BranchInfo {
                name: name.to_string(),
                ahead,
                behind,
                last_commit,
            });
        }
        // Sort by phase number so phase-01 comes before phase-10.
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    /// Count revisions in the given range. Returns None if the command fails.
    fn rev_count(&self, range: &str) -> Option<usize> {
        self.git_output(["rev-list", "--count", range])
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    fn git_raw(&self, args: &[&str]) -> Result<(), GitError> {
        debug!("git {}", args.join(" "));
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(GitError::Command(stderr_or_status(&output)))
        }
    }

    fn git<const N: usize>(&self, args: [&str; N]) -> Result<(), GitError> {
        debug!("git {}", args.iter().copied().collect::<Vec<_>>().join(" "));
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(GitError::Command(stderr_or_status(&output)))
        }
    }

    fn git_output<const N: usize>(&self, args: [&str; N]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(GitError::Command(stderr_or_status(&output)))
        }
    }
}

/// Run a git command in an arbitrary directory (e.g. a worktree).
fn git_in(dir: &Path, args: &[&str]) -> Result<(), GitError> {
    debug!("git (in {}) {}", dir.display(), args.join(" "));
    let output = Command::new("git").args(args).current_dir(dir).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(GitError::Command(stderr_or_status(&output)))
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

    /// Run a git command in `root`, asserting success.
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

    fn current_branch(root: &Path) -> String {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(root)
            .output()
            .expect("rev-parse");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_file(root: &Path, name: &str) {
        std::fs::write(root.join(name), name).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", &format!("add {name}")]);
    }

    /// Initialize a repo with `main` and `develop` branches and one commit.
    fn init_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        // Disable any globally-configured hooks (e.g. gitleaks) for isolation.
        git(root, &["config", "core.hooksPath", "/dev/null"]);
        commit_file(root, "README.md");
        git(root, &["branch", "-M", "main"]);
        git(root, &["checkout", "-q", "-b", "develop"]);
        dir
    }

    fn flow(root: &Path) -> GitFlow {
        GitFlow::new(root)
    }

    #[test]
    fn feature_start_branches_from_develop() {
        let repo = init_repo();
        let root = repo.path();
        let branch = flow(root).feature_start(3).expect("feature_start");
        assert_eq!(branch, "feature/phase-03");
        assert_eq!(current_branch(root), "feature/phase-03");
    }

    #[test]
    fn list_feature_branches_reports_ahead_and_behind_semantics() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        gf.feature_start(12).expect("feature_start");
        commit_file(root, "feature-one.txt");
        commit_file(root, "feature-two.txt");
        git(root, &["checkout", "-q", "develop"]);
        commit_file(root, "develop-only.txt");

        let branches = gf.list_feature_branches().unwrap();
        let branch = branches
            .iter()
            .find(|branch| branch.name == "feature/phase-12")
            .unwrap();

        assert_eq!(branch.ahead, 2);
        assert_eq!(branch.behind, 1);
    }

    #[test]
    fn feature_finish_merges_into_develop_and_deletes() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        gf.feature_start(1).expect("start");
        commit_file(root, "feature.txt");

        let branch = gf.feature_finish(1).expect("finish");
        assert_eq!(branch, "feature/phase-01");
        assert_eq!(current_branch(root), "develop");

        // Branch is deleted and its work is now on develop.
        let branches = Command::new("git")
            .args(["branch"])
            .current_dir(root)
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&branches.stdout);
        assert!(!listing.contains("feature/phase-01"));
        assert!(root.join("feature.txt").exists());
    }

    #[test]
    fn release_start_and_finish_tags_main_and_merges_both() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // Add work on develop so the release has content.
        commit_file(root, "work.txt");
        let branch = gf.release_start("1.2.0").expect("release_start");
        assert_eq!(branch, "release/1.2.0");

        gf.release_finish("1.2.0").expect("release_finish");
        assert_eq!(current_branch(root), "develop");

        // Tag exists.
        let tags = Command::new("git")
            .args(["tag"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&tags.stdout).contains("v1.2.0"));

        // Release branch deleted.
        let branches = Command::new("git")
            .args(["branch"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&branches.stdout).contains("release/1.2.0"));
    }

    #[test]
    fn release_start_branches_from_current_head_not_develop() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // Ship from a feature branch carrying a commit that is NOT on develop.
        gf.feature_start(5).expect("feature_start");
        commit_file(root, "feature-only.txt");
        let feature_tip = gf.branch_tip("feature/phase-05").expect("feature tip");

        let branch = gf.release_start("2.0.0").expect("release_start");
        assert_eq!(branch, "release/2.0.0");
        assert_eq!(current_branch(root), "release/2.0.0");

        // The release branch tip must descend from the feature commit — i.e.
        // the feature-only work is present, not dropped to develop's HEAD.
        let release_tip = gf.branch_tip("release/2.0.0").expect("release tip");
        let is_ancestor = Command::new("git")
            .args(["merge-base", "--is-ancestor", &feature_tip, &release_tip])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success();
        assert!(
            is_ancestor,
            "release branch must descend from the shipped feature commit"
        );
        assert!(root.join("feature-only.txt").exists());
    }

    #[test]
    fn cleanup_merged_removes_merged_but_keeps_protected() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // Create and merge a feature branch into develop.
        gf.feature_start(2).expect("start");
        commit_file(root, "f.txt");
        gf.feature_finish(2).expect("finish");

        // Create an already-merged stray branch off develop.
        git(root, &["branch", "stale-merged"]);

        let deleted = gf.cleanup_merged().expect("cleanup");
        assert!(deleted.contains(&"stale-merged".to_string()));
        // Protected branches survive.
        assert!(!deleted.contains(&"develop".to_string()));
        assert!(!deleted.contains(&"main".to_string()));
    }

    #[test]
    fn delete_branch_removes_unmerged_with_force_and_protects_trunk() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // Create a feature branch with an unmerged commit.
        gf.feature_start(8).expect("start");
        commit_file(root, "unmerged.txt");
        // Switch back to develop so the branch isn't checked out.
        git(root, &["checkout", "-q", "develop"]);

        // -d would refuse (unmerged); force deletes it.
        assert!(gf.delete_branch("feature/phase-08", false).is_err());
        gf.delete_branch("feature/phase-08", true)
            .expect("force delete");
        let branches = Command::new("git")
            .args(["branch"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&branches.stdout).contains("feature/phase-08"));

        // Protected branches are never deleted.
        assert!(gf.delete_branch("develop", true).is_err());
        assert!(gf.delete_branch("main", true).is_err());
    }

    #[test]
    fn sequentagent_helpers_integrate_and_rebase_cleanly() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // Base branch off develop, not checked out anywhere.
        gf.ensure_branch("feature/phase-07", "develop")
            .expect("ensure base");
        assert!(gf.branch_exists("feature/phase-07"));
        assert!(!gf.branch_tip("feature/phase-07").unwrap().is_empty());
        // ensure_branch is idempotent.
        gf.ensure_branch("feature/phase-07", "develop")
            .expect("ensure again");

        // Two agent worktrees off the same base tip.
        let wt_a = root.join(".worktrees/a");
        let wt_b = root.join(".worktrees/b");
        crate::worktree::add(root, &wt_a, "feat-a", "feature/phase-07", true).expect("add A");
        crate::worktree::add(root, &wt_b, "feat-b", "feature/phase-07", true).expect("add B");

        // Agent A commits a new file, then we integrate A into the base (ff).
        std::fs::write(wt_a.join("a.txt"), "from-a\n").unwrap();
        git(&wt_a, &["add", "."]);
        git(&wt_a, &["commit", "-q", "-m", "a work"]);
        gf.fast_forward_branch("feature/phase-07", "feat-a")
            .expect("ff base to A");
        assert_eq!(
            gf.branch_tip("feature/phase-07").unwrap(),
            gf.branch_tip("feat-a").unwrap()
        );

        // Agent B (no overlapping changes) rebases onto the updated base cleanly.
        gf.rebase_in(&wt_b, "feature/phase-07")
            .expect("clean rebase");
        // B now contains A's file.
        assert!(wt_b.join("a.txt").exists());
    }

    #[test]
    fn rebase_in_aborts_and_errors_on_conflict() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        gf.ensure_branch("feature/phase-07", "develop")
            .expect("ensure base");

        // Worktree B is created off the ORIGINAL base, then edits a.txt.
        let wt_b = root.join(".worktrees/b");
        crate::worktree::add(root, &wt_b, "feat-b", "feature/phase-07", true).expect("add B");
        std::fs::write(wt_b.join("a.txt"), "from-b\n").unwrap();
        git(&wt_b, &["add", "."]);
        git(&wt_b, &["commit", "-q", "-m", "b edits a"]);

        // Meanwhile the base advances with a conflicting a.txt (via worktree A).
        let wt_a = root.join(".worktrees/a");
        crate::worktree::add(root, &wt_a, "feat-a", "feature/phase-07", true).expect("add A");
        std::fs::write(wt_a.join("a.txt"), "from-base\n").unwrap();
        git(&wt_a, &["add", "."]);
        git(&wt_a, &["commit", "-q", "-m", "base edits a"]);
        gf.fast_forward_branch("feature/phase-07", "feat-a")
            .expect("ff base to A");

        // Rebasing B onto the updated base conflicts on a.txt → error + abort.
        let err = gf.rebase_in(&wt_b, "feature/phase-07").unwrap_err();
        assert!(matches!(err, GitError::Command(_)));
        // The abort left no rebase-in-progress state behind.
        assert!(!root.join(".git/worktrees/b/rebase-merge").exists());
        // B is still usable: its own commit is intact.
        assert_eq!(
            std::fs::read_to_string(wt_b.join("a.txt")).unwrap(),
            "from-b\n"
        );
    }

    #[test]
    fn merge_of_missing_branch_is_an_error() {
        let repo = init_repo();
        let root = repo.path();
        // feature_finish for a phase that was never started: checkout develop
        // succeeds, but merging the nonexistent feature branch fails.
        let err = flow(root).feature_finish(99).unwrap_err();
        assert!(matches!(err, GitError::Command(_)));
    }
}

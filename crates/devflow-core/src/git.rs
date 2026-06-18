//! Git-flow operations implemented with plain `git` commands.

use crate::config::GitFlowConfig;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    /// Create a git-flow helper for a project root.
    pub fn new(root: impl AsRef<Path>, config: GitFlowConfig) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            config,
        }
    }

    /// Create a feature branch from the develop branch.
    ///
    /// Returns an error if the branch already exists (use
    /// [`feature_start_force`] to overwrite).
    pub fn feature_start(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        self.git(["checkout", &self.config.develop])?;
        self.git(["checkout", "-b", &branch])?;
        Ok(branch)
    }

    /// Create or reset a feature branch, overwriting it if it already exists.
    pub fn feature_start_force(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        self.git(["checkout", &self.config.develop])?;
        self.git(["checkout", "-B", &branch])?;
        Ok(branch)
    }

    /// Merge a feature branch into develop and delete it.
    pub fn feature_finish(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        self.git(["checkout", &self.config.develop])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["branch", "-d", &branch])?;
        Ok(branch)
    }

    /// Create a release branch from develop.
    pub fn release_start(&self, version: &str) -> Result<String, GitError> {
        let branch = format!("release/{version}");
        self.git(["checkout", &self.config.develop])?;
        self.git(["checkout", "-B", &branch])?;
        Ok(branch)
    }

    /// Merge a release branch into main and develop, tag it, and delete it.
    pub fn release_finish(&self, version: &str) -> Result<String, GitError> {
        let branch = format!("release/{version}");
        self.git(["checkout", &self.config.main])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["tag", &format!("v{version}")])?;
        self.git(["checkout", &self.config.develop])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["branch", "-d", &branch])?;
        Ok(branch)
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
            self.git(["branch", "-d", branch])?;
            deleted.push(branch.to_string());
        }
        Ok(deleted)
    }

    /// Stage all changes and commit with the given message.
    /// Returns Ok(()) whether or not there were changes to commit.
    pub fn commit_all(&self, message: &str) -> Result<(), GitError> {
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
                .rev_count(&format!("{name}..{dev}", dev = self.config.develop))
                .unwrap_or(0);
            let behind = self
                .rev_count(&format!("{dev}..{name}", dev = self.config.develop))
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
        GitFlow::new(root, GitFlowConfig::default())
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
    fn merge_of_missing_branch_is_an_error() {
        let repo = init_repo();
        let root = repo.path();
        // feature_finish for a phase that was never started: checkout develop
        // succeeds, but merging the nonexistent feature branch fails.
        let err = flow(root).feature_finish(99).unwrap_err();
        assert!(matches!(err, GitError::Command(_)));
    }
}

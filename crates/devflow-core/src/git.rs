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

impl GitFlow {
    /// Create a git-flow helper for a project root.
    pub fn new(root: impl AsRef<Path>, config: GitFlowConfig) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            config,
        }
    }

    /// Create a feature branch from the develop branch.
    pub fn feature_start(&self, phase: u32) -> Result<String, GitError> {
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

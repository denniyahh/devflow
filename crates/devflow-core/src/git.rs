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
        let branch = self.merge_feature_into_develop(phase)?;
        self.git(["branch", "-d", &branch])?;
        Ok(branch)
    }

    /// Merge a feature branch into develop without deleting it.
    ///
    /// Default DevFlow runs keep the feature branch checked out in a linked
    /// worktree, so deletion belongs to the later best-effort cleanup hook.
    pub fn merge_feature_into_develop(&self, phase: u32) -> Result<String, GitError> {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        info!("merging feature branch: {branch}");
        self.git(["checkout", &self.config.develop])?;
        self.git(["merge", "--no-ff", &branch])?;
        Ok(branch)
    }

    /// Whether a phase feature branch has nothing left to merge into develop.
    ///
    /// An absent branch is not proof of a merge. Callers must fail closed
    /// rather than treating a deleted or never-created branch as shipped.
    pub fn is_merged_into_develop(&self, phase: u32) -> bool {
        let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
        if !self.branch_exists(&branch) {
            return false;
        }

        Command::new("git")
            .args(["merge-base", "--is-ancestor", &branch, &self.config.develop])
            .current_dir(&self.root)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
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
        // `-c tag.gpgSign=false` scopes the override to this invocation only
        // (never the user's global/repo config) — without it, a global
        // `tag.gpgsign=true` forces this lightweight tag into an
        // annotated+signed one requiring a message, which blocks on
        // `$EDITOR` in what must be a headless, unattended flow (Phase 13
        // dogfood finding).
        self.git(["-c", "tag.gpgSign=false", "tag", &format!("v{version}")])?;
        self.git(["checkout", &self.config.develop])?;
        self.git(["merge", "--no-ff", &branch])?;
        self.git(["branch", "-d", &branch])?;
        Ok(branch)
    }

    /// Create an annotated-free lightweight tag at the current `HEAD`.
    ///
    /// Passes `-c tag.gpgSign=false` scoped to this invocation only — a
    /// global `tag.gpgsign=true` (common for developers who sign their own
    /// tags) otherwise forces this lightweight tag into an annotated+signed
    /// one requiring a message, which blocks on `$EDITOR` in what must be a
    /// headless, unattended flow (Phase 13 dogfood finding: VersionBump hung
    /// on a live `devflow start --mode auto` run).
    pub fn tag(&self, tag: &str) -> Result<(), GitError> {
        info!("tagging {tag}");
        self.git(["-c", "tag.gpgSign=false", "tag", tag])
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

    /// Delete local branches already merged into `develop`.
    ///
    /// WR-04 (13-REVIEW.md): passes `develop` explicitly rather than relying
    /// on `git branch --merged`'s default of "whatever HEAD currently is" —
    /// if the main checkout is ever left on a branch other than `develop`
    /// when this runs, an implicit baseline would silently prune branches
    /// merged into that other branch instead.
    ///
    /// Deletion uses `-D`, not `-d`: `-d` verifies merged-into-HEAD, which
    /// contradicts the `--merged develop` listing above in exactly the
    /// checkout-not-on-develop scenario WR-04 targets (every genuinely
    /// merged branch would be refused as "not fully merged"). The listing IS
    /// the merge safety check. A branch git still refuses to delete (e.g.
    /// checked out in a worktree) is logged and skipped so one failure
    /// doesn't abort the rest of the sweep.
    pub fn cleanup_merged(&self) -> Result<Vec<String>, GitError> {
        let output = self.git_output(["branch", "--merged", &self.config.develop])?;
        let protected = [self.config.main.as_str(), self.config.develop.as_str()];
        let mut deleted = Vec::new();
        for line in output.lines() {
            // git's porcelain marker is an exact two-char prefix ("* " for
            // the current branch, "+ " for a worktree checkout, "  "
            // otherwise) — strip it positionally rather than trimming
            // marker CHARACTERS, which would mangle a branch legitimately
            // named e.g. "+foo" (WR-03, revised).
            let branch = line
                .strip_prefix("* ")
                .or_else(|| line.strip_prefix("+ "))
                .unwrap_or(line)
                .trim();
            // Skip blanks, protected trunks, and the detached-HEAD line
            // ("(HEAD detached at ...)"), which is not a branch name.
            if branch.is_empty() || branch.starts_with('(') || protected.contains(&branch) {
                continue;
            }
            info!("cleaning up merged branch: {branch}");
            match self.git(["branch", "-D", branch]) {
                Ok(()) => deleted.push(branch.to_string()),
                Err(err) => warn!("could not delete merged branch {branch}: {err}"),
            }
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

    /// Stage a single relative path and commit with the given message.
    /// Mirrors `commit_all`, but scoped to one path, for hooks that must not
    /// sweep in unrelated dirty state left by other hooks or the workflow.
    /// Returns Ok(()) whether or not the path had changes to commit. Unlike
    /// `commit_all`, a path with no changes produces **no commit** — it is a
    /// genuine no-op, not a forced empty commit, so a caller such as
    /// `hooks::version_bump` can never tag a release on a commit containing
    /// nothing (19b/D-16).
    pub fn commit_path(&self, relative_path: &str, message: &str) -> Result<(), GitError> {
        debug!("committing {relative_path}: {message}");
        // `add` first so a brand-new file is known to git — a pathspec-only
        // commit errors on a path git has never seen. The trailing pathspec is
        // what actually scopes the commit: without it, `commit` writes whatever
        // else is already in the index, which is exactly the sweep-in this
        // function exists to prevent.
        self.git(["add", relative_path])?;
        match self.git_raw_combined(&["commit", "-m", message, "--", relative_path]) {
            Ok(()) => Ok(()),
            // No forcing flag above, so this arm is now the live no-op path:
            // a path with nothing staged makes git exit non-zero with
            // "nothing to commit", and we convert that back to Ok(()) rather
            // than let it propagate as an error (19b/D-16, T-19-11).
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
        // Pin the subprocess locale to C (Antigravity review, 19b): commit_path's
        // "nothing to commit" match arm above compares against git's own
        // English-locale output, which a non-English LC_ALL/LANG would
        // localize, silently defeating the match and reopening 19b under a
        // localized environment (T-19-14). Scoped to this one call path only.
        let output = Command::new("git")
            .args(args)
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .current_dir(&self.root)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(GitError::Command(stderr_or_status(&output)))
        }
    }

    /// Like [`git_raw`](Self::git_raw), but the error text combines stdout
    /// with stderr instead of inspecting stderr alone.
    ///
    /// Discovered empirically while implementing 19b: `git commit`'s
    /// "nothing to commit, working tree clean" message is written to
    /// **stdout**, not stderr. `stderr_or_status` only ever inspects
    /// `output.stderr`, so a plain `git_raw` error can never contain that
    /// text — `commit_path`'s `nothing to commit` match arm (immediately
    /// above its call site) would never fire, no matter how the arm itself
    /// is written. This sibling exists solely so `commit_path` can see it;
    /// `commit_all` keeps calling `git_raw` unchanged (D-17 out of scope),
    /// and `git_raw`'s own error-mapping branch is untouched by this
    /// addition.
    fn git_raw_combined(&self, args: &[&str]) -> Result<(), GitError> {
        debug!("git {}", args.join(" "));
        let output = Command::new("git")
            .args(args)
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .current_dir(&self.root)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let combined = match (stderr.is_empty(), stdout.is_empty()) {
                (false, false) => format!("{stderr}\n{stdout}"),
                (false, true) => stderr,
                (true, false) => stdout,
                (true, true) => format!("exited with {}", output.status),
            };
            Err(GitError::Command(combined))
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

/// Result of checking whether `origin/main` is already an ancestor of
/// `HEAD` — i.e. whether `scripts/sync-main-to-develop.sh` would be a no-op
/// — WITHOUT issuing any `git fetch` (20d, review: Codex HIGH — a
/// "read-only" preflight must not depend on the network).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncestorStatus {
    /// `origin/main` is an ancestor of `HEAD` — sync would be a no-op.
    Ancestor,
    /// `origin/main` resolves locally but is NOT an ancestor of `HEAD` —
    /// develop has diverged and `scripts/sync-main-to-develop.sh` should be
    /// run before cutting the next release.
    Diverged,
    /// `origin/main` does not resolve locally at all (never fetched, or no
    /// remote configured). Distinct from [`Diverged`](Self::Diverged) so
    /// the caller can degrade to an actionable "run `git fetch` first"
    /// message instead of reporting a false divergence.
    RefAbsent,
}

/// Check whether `origin/main` is an ancestor of `HEAD`, against
/// ALREADY-FETCHED local refs — issues NO `git fetch`. Mirrors
/// `scripts/sync-main-to-develop.sh`'s own `git merge-base --is-ancestor
/// origin/main HEAD` invocation (`:41`), minus the preceding `git fetch`
/// (`:38`), which mutates `.git/FETCH_HEAD`/tracking refs and would make a
/// "read-only" preflight false (20d, review: Codex HIGH).
pub fn origin_main_ancestor_status(project_root: &Path) -> AncestorStatus {
    let ref_exists = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !ref_exists {
        return AncestorStatus::RefAbsent;
    }
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if is_ancestor {
        AncestorStatus::Ancestor
    } else {
        AncestorStatus::Diverged
    }
}

/// Derive the crates.io publish order for a workspace's local-path members
/// (e.g. `devflow-core` before `devflow`) — sourced from the workspace's own
/// `[workspace] members` list and each member's own `[dependencies]`
/// section (which member depends on which), never a hardcoded prose string
/// (20d). Read-only; returns an empty `Vec` (never panics) if the workspace
/// Cargo.toml or a member manifest cannot be read.
pub fn publish_order(project_root: &Path) -> Vec<String> {
    let Ok(root_contents) = std::fs::read_to_string(project_root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let member_paths = workspace_member_paths(&root_contents);

    let mut members: Vec<(String, String)> = Vec::new();
    for path in &member_paths {
        let manifest = project_root.join(path).join("Cargo.toml");
        let Ok(contents) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let name = package_name(&contents).unwrap_or_else(|| path.clone());
        members.push((name, contents));
    }

    let names: Vec<String> = members.iter().map(|(name, _)| name.clone()).collect();
    let mut edges: Vec<(String, String)> = Vec::new();
    for (name, contents) in &members {
        for other in &names {
            if other != name && member_depends_on(contents, other) {
                edges.push((name.clone(), other.clone()));
            }
        }
    }
    topo_sort(names, edges)
}

/// Extract the `[workspace] members = [...]` array's quoted path entries.
/// Hand-rolled, single-array-only scan (this project deliberately avoids a
/// TOML parser dependency for its version/workspace tooling — see
/// `version.rs`).
fn workspace_member_paths(contents: &str) -> Vec<String> {
    let Some(start) = contents.find("members") else {
        return Vec::new();
    };
    let rest = &contents[start..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find(']') else {
        return Vec::new();
    };
    let inner = &rest[open + 1..open + close];
    inner
        .split(',')
        .filter_map(|fragment| {
            let fragment = fragment.trim();
            let fragment = fragment.strip_prefix('"')?.strip_suffix('"')?;
            (!fragment.is_empty()).then(|| fragment.to_string())
        })
        .collect()
}

/// Extract a member manifest's `[package] name`.
fn package_name(contents: &str) -> Option<String> {
    let mut current = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = inner.trim().to_string();
            continue;
        }
        if current == "package"
            && let Some((key, value)) = trimmed.split_once('=')
            && key.trim() == "name"
        {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// Whether a member manifest's `[dependencies]` section references
/// `dep_name` — either `dep_name.workspace = true` or `dep_name = { ... }`
/// under an inline `[dependencies]` table, OR the equally-valid expanded
/// long-form section `[dependencies.dep_name]` (WR-03, phase 20 review): a
/// manifest may spell a dependency out as its own section (e.g.
/// `[dependencies.devflow-core]\nworkspace = true`), which parses to a
/// section header of `"dependencies.devflow-core"` — never equal to the
/// plain `"dependencies"` the inline-table branch below checks against, so
/// that edge was previously dropped from `publish_order`'s topo-sort
/// entirely.
fn member_depends_on(contents: &str, dep_name: &str) -> bool {
    let mut current = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = inner.trim().to_string();
            if let Some(name) = current.strip_prefix("dependencies.")
                && name == dep_name
            {
                return true;
            }
            continue;
        }
        if current != "dependencies" {
            continue;
        }
        let key = trimmed.split(['.', '=']).next().unwrap_or("").trim();
        if key == dep_name {
            return true;
        }
    }
    false
}

/// Kahn's-algorithm topological sort: `edges` are `(dependent, dependency)`
/// pairs, meaning `dependent` must be published AFTER `dependency`. Falls
/// back to appending whatever remains (rather than looping forever) if a
/// cycle is present — a genuine cyclic Cargo dependency would already fail
/// `cargo build` long before this check runs.
fn topo_sort(names: Vec<String>, edges: Vec<(String, String)>) -> Vec<String> {
    let mut result = Vec::new();
    let mut published: Vec<String> = Vec::new();
    let mut remaining = names;
    while !remaining.is_empty() {
        let ready: Vec<String> = remaining
            .iter()
            .filter(|name| {
                edges
                    .iter()
                    .filter(|(dependent, _)| dependent == *name)
                    .all(|(_, dep)| published.contains(dep))
            })
            .cloned()
            .collect();
        if ready.is_empty() {
            result.extend(remaining);
            break;
        }
        for name in &ready {
            published.push(name.clone());
            result.push(name.clone());
        }
        remaining.retain(|name| !ready.contains(name));
    }
    result
}

// ---------------------------------------------------------------------------
// tag-signing viability (20d, Pattern 4)
// ---------------------------------------------------------------------------

/// Pure classification of `ssh-add -l`'s exit code into an actionable
/// signing-viability status. Isolated from any I/O so it can be
/// unit-tested for all three documented exit codes without a live agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningStatus {
    /// Exit 2 — no ssh-agent reachable (`SSH_AUTH_SOCK` unset or dead).
    NoAgent,
    /// Exit 1 — agent reachable but has no identities loaded.
    AgentEmpty,
    /// Exit 0 — agent has at least one key loaded (caller still must check
    /// whether it's THIS key, via a fingerprint match).
    KeysListed,
    /// Any other exit code — genuinely unexpected; degrade rather than
    /// crash or silently misclassify.
    Unknown(i32),
}

/// Map `ssh-add -l`'s exit code to a [`SigningStatus`] (Pattern 4: exit
/// 2 = no agent, 1 = agent-but-empty, 0 = keys listed).
pub fn classify_ssh_add_status(exit_code: i32) -> SigningStatus {
    match exit_code {
        2 => SigningStatus::NoAgent,
        1 => SigningStatus::AgentEmpty,
        0 => SigningStatus::KeysListed,
        other => SigningStatus::Unknown(other),
    }
}

/// Outcome of the tag-signing viability check. Carries only a boolean-ish
/// status plus an optional PUBLIC key fingerprint — never private key
/// material or a full filesystem path (T-20-04, ASVS V6 / WR-02 — mirrors
/// the existing "no path/username" discipline this project already applies
/// elsewhere, e.g. `PhaseFinding`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningViability {
    /// Signing is viable. `fingerprint` is the matched public key's
    /// `SHA256:...` fingerprint, when one could be extracted.
    Viable { fingerprint: Option<String> },
    /// Not viable, with an actionable (never key-leaking) reason.
    NotViable { reason: String },
    /// Could not be determined — tool absent, format unset with no key,
    /// etc. Fail-soft: never a crash.
    Unknown { reason: String },
}

/// `git config --get <key>`, scoped to `project_root`. `None` if unset or
/// the command fails (missing `git`, not a repo, etc.) — never panics.
fn git_config(project_root: &Path, key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// `ssh-keygen -lf <pub_key_path>`'s fingerprint (`SHA256:...`) — reads only
/// the PUBLIC key file, never a private key, and returns only the hash
/// token, never a filesystem path.
fn public_key_fingerprint(pub_key_path: &Path) -> Option<String> {
    let path_str = pub_key_path.to_str()?;
    let output = Command::new("ssh-keygen")
        .args(["-lf", path_str])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Format: "<bits> SHA256:<hash> <comment> (<type>)"
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// `gpg.format == "ssh"` branch (Pattern 4): `user.signingkey` must be set
/// and the key file must exist, then `ssh-add -l`'s exit code determines
/// viability. On a match, only the PUBLIC key's fingerprint is reported —
/// never the configured key's filesystem path.
fn check_ssh_signing_viability(project_root: &Path) -> SigningViability {
    let Some(signingkey) = git_config(project_root, "user.signingkey") else {
        return SigningViability::NotViable {
            reason: "gpg.format=ssh but user.signingkey is not set".into(),
        };
    };
    let key_path = Path::new(&signingkey);
    if !key_path.exists() {
        return SigningViability::NotViable {
            reason: "user.signingkey is set but the key file does not exist".into(),
        };
    }

    let output = match Command::new("ssh-add").arg("-l").output() {
        Ok(out) => out,
        Err(_) => {
            return SigningViability::Unknown {
                reason: "cannot verify signing viability — ssh-add not found".into(),
            };
        }
    };
    let exit_code = output.status.code().unwrap_or(-1);
    match classify_ssh_add_status(exit_code) {
        SigningStatus::NoAgent => SigningViability::NotViable {
            reason: "no ssh-agent reachable (SSH_AUTH_SOCK unset or dead)".into(),
        },
        SigningStatus::AgentEmpty => SigningViability::NotViable {
            reason: "ssh-agent reachable but has no identities loaded".into(),
        },
        SigningStatus::KeysListed => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match public_key_fingerprint(key_path) {
                Some(fingerprint) if stdout.contains(&fingerprint) => SigningViability::Viable {
                    fingerprint: Some(fingerprint),
                },
                Some(_) => SigningViability::NotViable {
                    reason: "ssh-agent has keys loaded, but not the configured signing key".into(),
                },
                None => SigningViability::Unknown {
                    reason: "cannot verify signing viability — ssh-keygen not found or the key \
                             is unreadable"
                        .into(),
                },
            }
        }
        SigningStatus::Unknown(code) => SigningViability::Unknown {
            reason: format!("ssh-add -l exited with an unexpected code {code}"),
        },
    }
}

/// `gpg.format` unset or `"openpgp"` branch (Pattern 4): verify a secret
/// key exists for `user.signingkey` via `gpg --list-secret-keys`.
fn check_gpg_signing_viability(project_root: &Path) -> SigningViability {
    let Some(signingkey) = git_config(project_root, "user.signingkey") else {
        return SigningViability::Unknown {
            reason: "cannot verify signing viability — user.signingkey is not set".into(),
        };
    };
    let output = match Command::new("gpg")
        .args(["--list-secret-keys", &signingkey])
        .output()
    {
        Ok(out) => out,
        Err(_) => {
            return SigningViability::Unknown {
                reason: "cannot verify signing viability — gpg not found".into(),
            };
        }
    };
    if output.status.success() {
        SigningViability::Viable {
            fingerprint: Some(signingkey),
        }
    } else {
        SigningViability::NotViable {
            reason: "no secret key found for the configured user.signingkey".into(),
        }
    }
}

/// Tag-signing viability check (20d): branches on `git config gpg.format`
/// since the check is a genuinely different code path per format — a
/// GPG-only check would miss the `ssh_askpass` failure this project's own
/// release actually hit (Pattern 4). Fail-soft throughout: an absent tool
/// or unset config degrades to an actionable [`SigningViability::Unknown`],
/// never a crash.
pub fn check_signing_viability(project_root: &Path) -> SigningViability {
    match git_config(project_root, "gpg.format").as_deref() {
        Some("ssh") => check_ssh_signing_viability(project_root),
        _ => check_gpg_signing_viability(project_root),
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

    /// A global/repo `tag.gpgsign=true` must not turn `tag()`'s lightweight
    /// tag into an annotated+signed one — that would require a tag message
    /// and block on `$EDITOR`, silently hanging a headless, unattended run
    /// (Phase 13 dogfood finding: VersionBump hung on a live
    /// `devflow start --mode auto` run because the operator's global
    /// gitconfig sets `tag.gpgsign=true`).
    #[test]
    fn tag_stays_lightweight_when_gpgsign_is_forced_on() {
        let repo = init_repo();
        let root = repo.path();
        // Simulate an operator whose global config signs tags by default —
        // override the test harness's own `tag.gpgsign false` to prove
        // `tag()`'s per-invocation `-c` override wins regardless.
        git(root, &["config", "tag.gpgsign", "true"]);

        flow(root)
            .tag("v9.9.9")
            .expect("tag must not block on $EDITOR");

        let tags = Command::new("git")
            .args(["tag", "-l"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&tags.stdout).contains("v9.9.9"));

        // Confirm it's a lightweight tag (points directly at the commit),
        // not an annotated tag object (which `cat-file -t` would report as
        // "tag" rather than "commit").
        let obj_type = Command::new("git")
            .args(["cat-file", "-t", "v9.9.9"])
            .current_dir(root)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&obj_type.stdout).trim(),
            "commit",
            "tag() must stay lightweight even when tag.gpgsign=true"
        );
    }

    #[test]
    fn commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted() {
        // The property that distinguishes commit_path from commit_all
        // (17-12, Task 2b): a hook using commit_path must never sweep in
        // unrelated dirty state.
        let repo = init_repo();
        let root = repo.path();
        std::fs::write(root.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        std::fs::write(root.join("unrelated.txt"), "not part of this commit\n").unwrap();

        // Stage the unrelated file BEFORE calling commit_path. An untracked
        // file is excluded by any implementation and so proves nothing; an
        // already-staged one is the real failure mode — a bare `git commit`
        // writes the whole index and would sweep it in.
        Command::new("git")
            .args(["add", "unrelated.txt"])
            .current_dir(root)
            .status()
            .unwrap();

        flow(root)
            .commit_path("CHANGELOG.md", "docs: add changelog entry")
            .expect("commit_path");

        let committed = Command::new("git")
            .args(["log", "-1", "--name-only", "--pretty=format:"])
            .current_dir(root)
            .output()
            .unwrap();
        let committed_files = String::from_utf8_lossy(&committed.stdout);
        assert!(committed_files.contains("CHANGELOG.md"));
        assert!(!committed_files.contains("unrelated.txt"));

        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .unwrap();
        let status = String::from_utf8_lossy(&status.stdout);
        assert!(
            status.contains("A  unrelated.txt"),
            "unrelated.txt must remain staged-but-uncommitted, got: {status}"
        );
    }

    /// `git rev-list --count HEAD`, parsed. Shared by the three tests below
    /// so a failure reports both counts instead of a bare assertion.
    fn rev_list_count(root: &Path) -> u32 {
        let output = Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success(), "git rev-list --count HEAD failed");
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .expect("rev-list --count HEAD must print an integer")
    }

    /// 19b/D-16: `hooks::version_bump` (hooks.rs:242) calls `commit_path` and
    /// then tags whatever commit it last produced (hooks.rs:249). If a
    /// terminal-batch retry calls `commit_path` again with byte-identical
    /// content (the file untouched since the first call), a forced commit
    /// here means the release tag can end up naming a commit that contains
    /// nothing new. This pins the exact retry scenario: two calls, unchanged
    /// content, `git rev-list --count HEAD` must not move between them.
    #[test]
    fn commit_path_twice_with_identical_content_creates_only_one_commit() {
        let repo = init_repo();
        let root = repo.path();
        std::fs::write(root.join("CHANGELOG.md"), "# Changelog\n").unwrap();

        flow(root)
            .commit_path("CHANGELOG.md", "docs: add changelog entry")
            .expect("first commit_path call");
        let n1 = rev_list_count(root);

        // The file is not touched again -- this is the retry scenario, not
        // a second genuine change.
        flow(root)
            .commit_path("CHANGELOG.md", "docs: add changelog entry")
            .expect("second commit_path call");
        let n2 = rev_list_count(root);

        assert_eq!(
            n2, n1,
            "a repeat commit_path call on unchanged content must not add a \
             commit: n1={n1}, n2={n2}"
        );
    }

    /// 19b/D-16, T-19-11: separates the "no commit" claim from the "no
    /// error" claim so a future change can't satisfy one by breaking the
    /// other. `hooks.rs` propagates `commit_path`'s `Result` with `?` at both
    /// call sites (changelog_append:225, version_bump:242) -- turning a
    /// genuine no-op into `Err` would stall the terminal hook batch (see
    /// T-19-11 in this plan's threat model), so both properties must hold
    /// simultaneously.
    #[test]
    fn commit_path_with_no_changes_returns_ok_without_committing() {
        let repo = init_repo();
        let root = repo.path();
        std::fs::write(root.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        flow(root)
            .commit_path("CHANGELOG.md", "docs: add changelog entry")
            .expect("initial commit_path");
        let n1 = rev_list_count(root);

        // CHANGELOG.md is already committed and unmodified -- a single call
        // here has nothing to commit.
        let result = flow(root).commit_path("CHANGELOG.md", "docs: add changelog entry");
        let n2 = rev_list_count(root);

        assert!(
            result.is_ok(),
            "no-op call must return Ok(()), got: {result:?}"
        );
        assert_eq!(
            n2, n1,
            "no-op call must not create a commit: n1={n1}, n2={n2}"
        );
    }

    /// Edge case the fix must NOT change: `commit_path` on a path that does
    /// not exist on disk still errors at the staging step (`git add` fails
    /// on an unknown pathspec). Asserted explicitly so the fix for the
    /// no-change case above cannot be over-applied into "commit_path never
    /// fails".
    #[test]
    fn commit_path_on_nonexistent_path_still_errors() {
        let repo = init_repo();
        let root = repo.path();

        let result = flow(root).commit_path("does-not-exist.md", "docs: add changelog entry");

        assert!(
            result.is_err(),
            "commit_path on an unknown pathspec must still error, got: {result:?}"
        );
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

    /// WR-04 (13-REVIEW.md): `cleanup_merged` must compute "merged" relative
    /// to `develop` explicitly, not whatever the main checkout's current
    /// HEAD happens to be. If the main checkout is left on a divergent
    /// branch, an implicit-HEAD baseline would wrongly identify (and
    /// delete) a branch that's merged into that other branch but was never
    /// actually merged into `develop`.
    #[test]
    fn cleanup_merged_is_relative_to_develop_not_current_head() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // `topic` diverges from develop with a unique commit develop never
        // sees, then `premature` branches off `topic`'s tip — so
        // `premature` is merged into `topic` but NOT into `develop`.
        git(root, &["checkout", "-q", "-b", "topic", "develop"]);
        commit_file(root, "topic-only.txt");
        git(root, &["checkout", "-q", "-b", "premature", "topic"]);

        // Leave the main checkout on `topic` — NOT `develop` — before
        // calling cleanup_merged, mirroring an operator who forgot to
        // check out develop first. (`topic` itself is also technically
        // "merged into HEAD" under an implicit baseline since it IS HEAD,
        // which git's own `-d` correctly refuses as the checked-out branch
        // — so the call's overall Ok/Err is not itself decisive here; check
        // the actual side effect on `premature` instead.)
        git(root, &["checkout", "-q", "topic"]);

        let _ = gf.cleanup_merged();
        assert!(
            gf.branch_exists("premature"),
            "premature is merged into topic (current HEAD) but not into \
             develop — it must survive cleanup_merged when the baseline is develop"
        );
    }

    /// WR-03 (13-REVIEW.md), revised: `git branch --merged` prefixes a
    /// branch checked out in a linked worktree with `+ `. The prefix must be
    /// stripped positionally (not by trimming marker characters, which would
    /// mangle a branch legitimately named "+foo"), and a branch git refuses
    /// to delete — a worktree checkout can never be deleted, by design —
    /// must be skipped with a warning rather than aborting the sweep before
    /// the remaining merged branches.
    #[test]
    fn cleanup_merged_skips_worktree_branch_and_continues_sweep() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // Merge a branch into develop WITHOUT deleting it (feature_finish
        // deletes on merge, which would leave nothing to check out).
        git(
            root,
            &["checkout", "-q", "-b", "worktree-merged", "develop"],
        );
        commit_file(root, "g.txt");
        git(root, &["checkout", "-q", "develop"]);
        git(root, &["merge", "-q", "--no-ff", "worktree-merged"]);

        // Check the merged branch out in a linked worktree so
        // `git branch --merged` reports it with a `+ ` prefix.
        let wt_dir = tempfile::tempdir().unwrap();
        git(
            root,
            &[
                "worktree",
                "add",
                wt_dir.path().to_str().unwrap(),
                "worktree-merged",
            ],
        );

        // A second merged branch that sorts after "worktree-merged" would be
        // reached only if the sweep survives the worktree refusal; "zz-" also
        // guards against luck in iteration order via the branch before it.
        git(root, &["branch", "aa-stale"]);
        git(root, &["branch", "zz-stale"]);

        let deleted = gf
            .cleanup_merged()
            .expect("a skipped worktree branch must not abort the sweep");
        assert!(deleted.contains(&"aa-stale".to_string()));
        assert!(deleted.contains(&"zz-stale".to_string()));
        assert!(
            !deleted.contains(&"worktree-merged".to_string()),
            "worktree checkout cannot be deleted"
        );
        assert!(gf.branch_exists("worktree-merged"));
    }

    /// The delete side must agree with the `--merged develop` listing: `-d`
    /// verifies merged-into-HEAD, so with the main checkout parked on a
    /// stale branch every genuinely-merged branch was refused as "not fully
    /// merged" — in exactly the scenario WR-04 exists for.
    #[test]
    fn cleanup_merged_deletes_when_head_is_not_on_develop() {
        let repo = init_repo();
        let root = repo.path();
        let gf = flow(root);

        // `old` is parked before the merge below, so nothing merged later is
        // reachable from HEAD while it's checked out.
        git(root, &["checkout", "-q", "-b", "old", "develop"]);
        git(root, &["checkout", "-q", "develop"]);
        git(root, &["checkout", "-q", "-b", "merged-feature", "develop"]);
        commit_file(root, "h.txt");
        git(root, &["checkout", "-q", "develop"]);
        git(root, &["merge", "-q", "--no-ff", "merged-feature"]);
        git(root, &["checkout", "-q", "old"]);

        let deleted = gf.cleanup_merged().expect("cleanup");
        assert!(
            deleted.contains(&"merged-feature".to_string()),
            "merged-into-develop branch must be deleted even when HEAD is elsewhere: {deleted:?}"
        );
        assert!(!gf.branch_exists("merged-feature"));
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

    // -----------------------------------------------------------------
    // 20d: publish-order helpers (pure, no I/O)
    // -----------------------------------------------------------------

    #[test]
    fn workspace_member_paths_parses_multiline_array() {
        let contents = "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/devflow-core\",\n    \"crates/devflow-cli\",\n]\n";
        assert_eq!(
            workspace_member_paths(contents),
            vec![
                "crates/devflow-core".to_string(),
                "crates/devflow-cli".to_string()
            ]
        );
    }

    #[test]
    fn package_name_reads_the_package_section() {
        let contents = "[package]\nname = \"devflow-core\"\nversion.workspace = true\n";
        assert_eq!(package_name(contents), Some("devflow-core".to_string()));
    }

    #[test]
    fn member_depends_on_matches_dotted_workspace_shorthand() {
        let contents = "[package]\nname = \"devflow\"\n\n[dependencies]\ndevflow-core.workspace = true\nclap.workspace = true\n";
        assert!(member_depends_on(contents, "devflow-core"));
        assert!(!member_depends_on(contents, "serde"));
    }

    /// WR-03 (phase 20 review): the equally-valid expanded long-form TOML
    /// section syntax (`[dependencies.NAME]`) parses to a section header of
    /// `"dependencies.NAME"`, never equal to the plain `"dependencies"` the
    /// inline-table branch checks against — this must still be recognized
    /// as a dependency edge.
    #[test]
    fn member_depends_on_matches_long_form_dependency_section() {
        let contents = "[package]\nname = \"devflow\"\n\n[dependencies.devflow-core]\nworkspace = true\n\n[dependencies.clap]\nversion = \"4\"\n";
        assert!(member_depends_on(contents, "devflow-core"));
        assert!(member_depends_on(contents, "clap"));
        assert!(!member_depends_on(contents, "serde"));
    }

    #[test]
    fn topo_sort_orders_dependency_before_dependent() {
        let names = vec!["devflow".to_string(), "devflow-core".to_string()];
        let edges = vec![("devflow".to_string(), "devflow-core".to_string())];
        assert_eq!(
            topo_sort(names, edges),
            vec!["devflow-core".to_string(), "devflow".to_string()]
        );
    }

    #[test]
    fn topo_sort_falls_back_to_input_order_on_a_cycle() {
        // A genuine cyclic dependency would already fail `cargo build`
        // long before this check runs — this just proves no infinite loop.
        let names = vec!["a".to_string(), "b".to_string()];
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ];
        let result = topo_sort(names, edges);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn publish_order_derives_core_before_cli_from_a_fixture_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\n    \"crates/devflow-core\",\n    \"crates/devflow-cli\",\n]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/devflow-core")).unwrap();
        std::fs::write(
            root.join("crates/devflow-core/Cargo.toml"),
            "[package]\nname = \"devflow-core\"\n\n[dependencies]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/devflow-cli")).unwrap();
        std::fs::write(
            root.join("crates/devflow-cli/Cargo.toml"),
            "[package]\nname = \"devflow\"\n\n[dependencies]\ndevflow-core.workspace = true\n",
        )
        .unwrap();

        assert_eq!(
            publish_order(root),
            vec!["devflow-core".to_string(), "devflow".to_string()]
        );
    }

    /// WR-03 (phase 20 review): a workspace member manifest written with
    /// the long-form `[dependencies.devflow-core]` section (rather than the
    /// inline `[dependencies]\ndevflow-core.workspace = true` form) must
    /// still contribute its dependency edge to `publish_order`'s topo-sort
    /// — the release-safety-critical crates.io publish order this
    /// self-pin regression would otherwise silently get wrong.
    #[test]
    fn publish_order_recognizes_long_form_dependency_section_self_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\n    \"crates/devflow-core\",\n    \"crates/devflow-cli\",\n]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/devflow-core")).unwrap();
        std::fs::write(
            root.join("crates/devflow-core/Cargo.toml"),
            "[package]\nname = \"devflow-core\"\n\n[dependencies]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/devflow-cli")).unwrap();
        std::fs::write(
            root.join("crates/devflow-cli/Cargo.toml"),
            "[package]\nname = \"devflow\"\n\n[dependencies.devflow-core]\nworkspace = true\n",
        )
        .unwrap();

        assert_eq!(
            publish_order(root),
            vec!["devflow-core".to_string(), "devflow".to_string()],
            "the long-form dependency section must still order devflow-core before devflow"
        );
    }

    // -----------------------------------------------------------------
    // 20d: origin/main ancestor check (no fetch)
    // -----------------------------------------------------------------

    #[test]
    fn origin_main_ancestor_status_is_ref_absent_without_a_remote() {
        let repo = init_repo();
        let root = repo.path();
        assert_eq!(origin_main_ancestor_status(root), AncestorStatus::RefAbsent);
    }

    #[test]
    fn origin_main_ancestor_status_is_ancestor_when_head_is_up_to_date() {
        let repo = init_repo();
        let root = repo.path();
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
        git(root, &["update-ref", "refs/remotes/origin/main", &head_sha]);
        assert_eq!(origin_main_ancestor_status(root), AncestorStatus::Ancestor);
    }

    // -----------------------------------------------------------------
    // 20d: signing-viability helpers
    // -----------------------------------------------------------------

    #[test]
    fn classify_ssh_add_status_maps_all_three_documented_exit_codes() {
        assert_eq!(classify_ssh_add_status(2), SigningStatus::NoAgent);
        assert_eq!(classify_ssh_add_status(1), SigningStatus::AgentEmpty);
        assert_eq!(classify_ssh_add_status(0), SigningStatus::KeysListed);
        assert_eq!(classify_ssh_add_status(7), SigningStatus::Unknown(7));
    }

    /// Guards tests that temporarily override the process-global `HOME`
    /// env var (same idiom as `config.rs`'s test-local `ENV_MUTEX`) — this
    /// project's own dev machine sets `gpg.format=ssh` / `user.signingkey`
    /// GLOBALLY (the exact Pattern 4 research finding), so a hermetic test
    /// of the "unset" branch must isolate `$HOME/.gitconfig`, not just the
    /// repo-local config.
    static HOME_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey() {
        // 20d/empty: no gpg.format, no user.signingkey — must degrade to an
        // actionable message, never panic.
        let _lock = HOME_ENV_MUTEX.lock().unwrap();
        let repo = init_repo();
        let root = repo.path();
        let fake_home = tempfile::tempdir().unwrap();
        let original_home = std::env::var_os("HOME");
        // SAFETY: serialized under HOME_ENV_MUTEX; restored below before
        // the guard drops.
        unsafe { std::env::set_var("HOME", fake_home.path()) };

        let result = check_signing_viability(root);

        // SAFETY: still serialized under HOME_ENV_MUTEX.
        match original_home {
            Some(home) => unsafe { std::env::set_var("HOME", home) },
            None => unsafe { std::env::remove_var("HOME") },
        }

        match result {
            SigningViability::Unknown { reason } => {
                assert!(
                    reason.contains("user.signingkey"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected Unknown (fail-soft), got: {other:?}"),
        }
    }
}

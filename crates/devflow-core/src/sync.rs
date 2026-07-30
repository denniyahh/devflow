//! Port of `scripts/sync-main-to-develop.sh` — sync `origin/main` back into
//! `develop` after a release, so the next release PR's merge-base does not
//! go stale (999.52).
//!
//! One implementation, two entry points (D-07): the standalone `devflow
//! sync` CLI subcommand (`crates/devflow-cli/src/commands.rs::sync_cmd`)
//! and 26-06's release executor both call [`sync_main_to_develop`] directly
//! — a second copy of this logic anywhere would reintroduce exactly the
//! drift 999.52 exists to prevent.

use std::path::Path;
use std::process::Command;

use crate::config::DEVELOP;
use crate::git::{AncestorStatus, GitFlow, origin_main_ancestor_status};

/// Errors produced by [`sync_main_to_develop`].
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Spawning git failed.
    #[error("failed to execute git: {0}")]
    Io(#[from] std::io::Error),
    /// A git command exited non-zero. Bounded and control-character-
    /// neutralized via [`crate::version::sanitize_changelog_subject`]
    /// before storage — the same bounded-reason discipline
    /// [`crate::git::release_tag_state`] already applies (T-26-08) — so
    /// untrusted, unbounded git stderr never reaches a log line unbounded.
    #[error("git command failed: {0}")]
    Git(String),
    /// The working tree has uncommitted changes (script check 1). This
    /// refusal precedes every mutation, including the fetch.
    #[error("working tree is not clean — commit, stash, or discard changes first")]
    DirtyWorkingTree,
    /// The current checkout is not on `develop` (script check 2). `current`
    /// is bounded/neutralized the same way [`SyncError::Git`] is.
    #[error("must be run from 'develop' (currently on '{current}')")]
    NotOnDevelop {
        /// The branch actually checked out.
        current: String,
    },
    /// No git remote is configured. A typed refusal instead of letting the
    /// fetch fail with a confusing message — additive to the ported script,
    /// never a removal of one of its checks.
    #[error("no git remote configured")]
    NoRemote,
    /// The `-X ours` merge changed develop's tree — `origin/main` had
    /// content develop genuinely lacked. Refused BEFORE pushing (D-09):
    /// nothing was pushed and the remote `develop` ref is provably
    /// unmoved. The LOCAL merge commit is deliberately left in place for
    /// the operator to inspect — undoing it would require a hard reset,
    /// which D-05 forbids as an automatic compensating action. Do not
    /// "fix" this later into a reset.
    #[error(
        "the merge changed develop's tree (before: {before_tree}, after: {after_tree}) — \
         origin/main had content develop genuinely lacked. Nothing was pushed. Inspect the \
         local merge commit (`git show HEAD --stat`) before deciding what to do next — this \
         refusal does not undo the local commit."
    )]
    TreeChanged {
        /// `HEAD^{tree}` captured before the merge.
        before_tree: String,
        /// `HEAD^{tree}` captured after the merge.
        after_tree: String,
    },
}

/// Outcome of a successful [`sync_main_to_develop`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// `origin/main` was already an ancestor of `HEAD` — a pure no-op
    /// (D-06's live-state idempotency shape). No commit was created, no
    /// fetch-derived mutation occurred beyond the fetch itself.
    AlreadyAncestor,
    /// The content-preserving merge landed and was pushed to
    /// `origin/develop`.
    Merged {
        /// The merge commit's SHA (also `origin/develop`'s new tip).
        merge_commit: String,
    },
}

/// The merge commit message — keeps the proven script's subject line
/// verbatim, self-reference updated from the script to `devflow sync`.
pub const SYNC_MERGE_MESSAGE: &str = "merge: sync main back into develop after release

Standing post-release history-linking step (devflow sync) — keeps main a
real ancestor of develop so the next release PR doesn't conflict against a
stale merge-base. -X ours: develop's content is authoritative; the
resulting tree is verified byte-identical to develop's pre-merge tree
before this merge is pushed.";

/// Sync `origin/main` back into `develop`, exactly porting
/// `scripts/sync-main-to-develop.sh`'s nine checks (see that file for the
/// original), short-circuiting at the first refusal. Every subprocess call
/// is an argv array (`Command::new("git").args([...])`) — never `sh -c`
/// string interpolation.
///
/// Step-by-script-line map:
/// 1. `status --porcelain` (script check 1) — dirty tree refuses before
///    anything else, including the fetch.
/// 2. `rev-parse --abbrev-ref HEAD` (script check 2) — wrong branch refuses
///    before the fetch too.
/// 3. `GitFlow::has_remote()` — additive: a typed [`SyncError::NoRemote`]
///    instead of a confusing fetch failure. Not present in the script; not
///    a removal of one of the script's checks either.
/// 4. `fetch origin main develop --quiet` (script check 3).
/// 5. [`origin_main_ancestor_status`] (script check 4) — the ancestry
///    short-circuit is D-06's idempotency shape applied to this step. No
///    second ancestry check is written here (D-10) — the shared predicate
///    from `git.rs` is reused verbatim.
/// 6. Capture `before_tree` (script check 5).
/// 7. `merge -X ours origin/main --no-edit` (script check 6).
/// 8. Capture `after_tree` (script check 7).
/// 9. Tree-identity check (script check 8, D-09) — the one check that must
///    never be relaxed. A mismatch leaves the local merge commit in place
///    deliberately: the alternative is a hard reset, which D-05 forbids as
///    an automatic compensating action.
/// 10. `GitFlow::push_ref(DEVELOP)` (script step 9, now automated —
///     D-08/D-01). `push_ref` structurally cannot pass a force option, so
///     this push can never overwrite anyone else's work.
pub fn sync_main_to_develop(project_root: &Path) -> Result<SyncOutcome, SyncError> {
    // Step 1 (script check 1): dirty working tree refuses before any mutation.
    let status = git_output(project_root, &["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        return Err(SyncError::DirtyWorkingTree);
    }

    // Step 2 (script check 2): wrong branch also refuses before the fetch.
    let current = git_output(project_root, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if current != DEVELOP {
        return Err(SyncError::NotOnDevelop {
            current: crate::version::sanitize_changelog_subject(&current),
        });
    }

    // Step 3: no-remote typed refusal (additive; still precedes the fetch).
    if !GitFlow::new(project_root).has_remote() {
        return Err(SyncError::NoRemote);
    }

    // Step 4 (script check 3): fetch both branches.
    git_raw(
        project_root,
        &["fetch", "origin", "main", "develop", "--quiet"],
    )?;

    // Step 5 (script check 4): ancestry short-circuit, delegated — never a
    // second ancestry check (D-10).
    // NOTE(RED): intentionally always claims "AlreadyAncestor" so the
    // pushes_on_success TDD test fails for the right reason before the
    // real merge/push logic below is implemented.
    let _ = origin_main_ancestor_status(project_root);
    return Ok(SyncOutcome::AlreadyAncestor);

    #[allow(unreachable_code)]
    {
        match origin_main_ancestor_status(project_root) {
            AncestorStatus::Ancestor => return Ok(SyncOutcome::AlreadyAncestor),
            AncestorStatus::RefAbsent => {
                return Err(SyncError::Git(
                    "origin/main did not resolve after fetch — anomalous state".to_string(),
                ));
            }
            AncestorStatus::Diverged => {}
        }

        // Step 6 (script check 5): capture pre-merge tree.
        let before_tree = git_output(project_root, &["rev-parse", "HEAD^{tree}"])?
            .trim()
            .to_string();

        // Step 7 (script check 6): the content-preserving merge.
        git_raw(
            project_root,
            &[
                "merge",
                "-X",
                "ours",
                "origin/main",
                "--no-edit",
                "-m",
                SYNC_MERGE_MESSAGE,
            ],
        )?;

        // Step 8 (script check 7): capture post-merge tree.
        let after_tree = git_output(project_root, &["rev-parse", "HEAD^{tree}"])?
            .trim()
            .to_string();

        // Step 9 (script check 8, D-09, D-05): terminal refusal, no push,
        // no compensating action.
        if before_tree != after_tree {
            return Err(SyncError::TreeChanged {
                before_tree,
                after_tree,
            });
        }

        // Step 10 (script step 9, D-08/D-01): push — `push_ref` structurally
        // cannot force.
        GitFlow::new(project_root)
            .push_ref(DEVELOP)
            .map_err(|err| SyncError::Git(crate::version::sanitize_changelog_subject(&err.to_string())))?;

        let merge_commit = git_output(project_root, &["rev-parse", "HEAD"])?
            .trim()
            .to_string();
        Ok(SyncOutcome::Merged { merge_commit })
    }
}

fn git_output(project_root: &Path, args: &[&str]) -> Result<String, SyncError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(SyncError::Git(crate::version::sanitize_changelog_subject(
            &stderr,
        )))
    }
}

fn git_raw(project_root: &Path, args: &[&str]) -> Result<(), SyncError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(SyncError::Git(crate::version::sanitize_changelog_subject(
            &stderr,
        )))
    }
}

/// Shared crate-internal test fixtures. `pub(crate)` because this becomes
/// the shared fixture home for every post-26-03 core module (26-06 imports
/// `init_repo`/`init_bare_remote` from here rather than building a third
/// copy): `test_support.rs` cannot host them (`tempfile` is a
/// `[dev-dependencies]` entry of `devflow-core`, and `test_support` also
/// compiles under the `test-support` feature in a non-test build for
/// `devflow-cli`'s dev-dependency, where dev-dependencies are unavailable —
/// a `TempDir`-returning helper structurally cannot live there), and
/// `git.rs`'s equivalent fixtures are private to a module 26-05 owns in
/// this same wave.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    /// Run a git command in `root`, asserting success.
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

    fn commit_file(root: &Path, name: &str, content: &str) {
        std::fs::write(root.join(name), content).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", &format!("add {name}")]);
    }

    /// `git rev-parse <rev>` in `root`, trimmed. Asserts success.
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

    /// Initialize a repo with `main` and `develop` branches and one commit
    /// (mirrors `git.rs`'s equivalent fixture, `git.rs:1193-1207`).
    pub(crate) fn init_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        // Disable any globally-configured hooks for isolation.
        git(root, &["config", "core.hooksPath", "/dev/null"]);
        commit_file(root, "README.md", "README.md");
        git(root, &["branch", "-M", "main"]);
        git(root, &["checkout", "-q", "-b", "develop"]);
        dir
    }

    /// Create a local bare repository and configure `repo_root`'s `origin`
    /// to point at it (mirrors `git.rs:1233-1249`) — the hermetic way to
    /// exercise a real `git push` without a network dependency. Keep the
    /// returned `TempDir` alive for the whole test: dropping it deletes the
    /// bare repository out from under `origin`.
    pub(crate) fn init_bare_remote(repo_root: &Path) -> TempDir {
        let bare_dir = tempfile::tempdir().unwrap();
        let output = crate::test_support::git_command(bare_dir.path())
            .args(["init", "--bare", "-q"])
            .output()
            .expect("spawn git init --bare");
        assert!(
            output.status.success(),
            "git init --bare failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        git(
            repo_root,
            &["remote", "add", "origin", bare_dir.path().to_str().unwrap()],
        );
        bare_dir
    }

    /// B5. Diverged-but-content-identical state: both `main` and `develop`
    /// modify the SAME path, so `-X ours` resolves to develop's content and
    /// the tree is unchanged.
    #[test]
    fn pushes_on_success() {
        let fixture = init_repo();
        let root = fixture.path();
        let bare = init_bare_remote(root);

        // Push both branches so the bare remote has a baseline for main and
        // develop.
        git(root, &["push", "-q", "-u", "origin", "main"]);
        git(root, &["push", "-q", "-u", "origin", "develop"]);

        // On develop: commit a value for f.txt, push develop.
        commit_file(root, "f.txt", "develop-value");
        git(root, &["push", "-q", "origin", "develop"]);

        let before_tree = rev_parse(root, "HEAD^{tree}");

        // Clone the bare remote into a second working directory, commit a
        // DIFFERENT value for the SAME path on main there, and push it —
        // both sides have then modified the same path, so `-X ours`
        // resolves to develop's content without conflict, and the tree is
        // unchanged.
        let clone_dir = tempfile::tempdir().unwrap();
        let clone_root = clone_dir.path();
        git(
            Path::new("."),
            &[
                "clone",
                "-q",
                bare.path().to_str().unwrap(),
                clone_root.to_str().unwrap(),
            ],
        );
        git(clone_root, &["config", "user.email", "test@example.com"]);
        git(clone_root, &["config", "user.name", "Test"]);
        git(clone_root, &["config", "commit.gpgsign", "false"]);
        git(clone_root, &["checkout", "-q", "main"]);
        commit_file(clone_root, "f.txt", "main-value");
        git(clone_root, &["push", "-q", "origin", "main"]);

        let result = sync_main_to_develop(root)
            .expect("sync_main_to_develop must succeed on a content-preserving merge");

        assert_eq!(
            result,
            SyncOutcome::Merged {
                merge_commit: rev_parse(root, "HEAD")
            },
            "expected a successful, tree-preserving merge"
        );

        let remote_develop = rev_parse(bare.path(), "refs/heads/develop");
        let local_head = rev_parse(root, "HEAD");
        assert_eq!(
            remote_develop, local_head,
            "the bare remote's develop must equal the fixture's HEAD after push"
        );

        let after_tree = rev_parse(root, "HEAD^{tree}");
        assert_eq!(
            before_tree, after_tree,
            "the merge must not have changed develop's tree"
        );

        let ancestor_check = crate::test_support::git_command(root)
            .args([
                "merge-base",
                "--is-ancestor",
                "origin/main",
                "develop",
            ])
            .status()
            .expect("spawn merge-base");
        assert!(
            ancestor_check.success(),
            "origin/main must be an ancestor of develop after sync"
        );
    }
}

//! The phase worktree `start` forks its run into (`.worktrees/phase-NN/` on
//! `feature/phase-NN`). Moved out of the removed `parallel` module (52-07, D-23)
//! unchanged in behaviour.

use devflow_core::config::{FEATURE_PREFIX, GitFlowConfig};
use devflow_core::git::GitFlow;
use devflow_core::phase_id::PhaseId;
use devflow_core::worktree;
use std::path::{Path, PathBuf};

use crate::CliError;

/// Create the phase worktree at `.worktrees/phase-NN/` on `feature/phase-NN`,
/// forked from `base`.
///
/// `base` is the project-resolved value from `config::base_branch` (D-01 /
/// AUTO-01), not the built-in trunk: a project whose `.planning/` lives on a
/// planning branch needs the worktree to carry it, or
/// `preflight_unattended_launch_check` refuses every unattended launch.
/// Callers must pass the same value the reachability and currency guards
/// inspected, so the branch checked and the branch forked can never disagree.
pub(crate) fn ensure_phase_worktree(
    project_root: &Path,
    phase: PhaseId,
    force: bool,
    base: &str,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{FEATURE_PREFIX}phase-{padded}", padded = phase.padded());

    if force {
        // Project-resolved: `delete_branch`'s protected-branch list, and the
        // trunk the merged check counts against, must name the configured
        // trunk, which is `base`, not the default one.
        let git = GitFlow::with_config(
            project_root,
            GitFlowConfig {
                develop: base.to_string(),
                ..GitFlowConfig::default()
            },
        );
        // Checked before anything is removed: a refusal leaves the worktree too.
        refuse_force_over_unmerged_branch(
            &git,
            &branch,
            base,
            wt.exists().then_some(wt.as_path()),
        )?;
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        if git.branch_exists(&branch) {
            git.delete_branch(&branch, true)?;
        }
    }

    match worktree::add(project_root, &wt, &branch, base, true) {
        Ok(()) => Ok(wt),
        Err(devflow_core::worktree::WorktreeError::Exists(path)) => {
            Err(CliError::Message(format!(
                "worktree already exists at {} — use --force to recreate it (refused while \
                 its branch has commits not merged into `{base}`)",
                path.display()
            )))
        }
        Err(err) => Err(err.into()),
    }
}

/// "1 commit" or "N commits" not merged into `base`, for the lines that keep or refuse
/// to delete a branch.
pub(crate) fn unmerged_commits(count: usize, base: &str) -> String {
    let noun = if count == 1 { "commit" } else { "commits" };
    format!("{count} {noun} not merged into `{base}`")
}

/// Refuse a `--force` start that would delete or reset `branch` while it holds commits
/// not merged into `git`'s trunk (`base`). Since there is no Ship stage, a finished
/// run's branch holds the only copy of its work until the operator closes it out, and
/// `git branch -D` would lose it. An absent branch, or one whose every commit is merged,
/// passes; a branch whose merge state cannot be read is refused, never assumed merged.
/// `worktree` is the phase worktree that has the branch checked out, when one exists:
/// git refuses to delete a branch a worktree has checked out, so the command the
/// refusal names removes that worktree first.
pub(crate) fn refuse_force_over_unmerged_branch(
    git: &GitFlow,
    branch: &str,
    base: &str,
    worktree: Option<&Path>,
) -> Result<(), CliError> {
    if !git.branch_exists(branch) {
        return Ok(());
    }
    let why = match git.commits_not_in_develop(branch) {
        Ok(0) => return Ok(()),
        Ok(count) => format!("has {}", unmerged_commits(count, base)),
        Err(err) => format!("could not be checked for commits not merged into `{base}` ({err})"),
    };
    let delete = match worktree {
        Some(path) => format!(
            "git worktree remove --force {} && git branch -D {branch}",
            path.display()
        ),
        None => format!("git branch -D {branch}"),
    };
    Err(CliError::Message(format!(
        "refusing --force: branch `{branch}` {why}, and recreating it from `{base}` would \
         delete them. Nothing was removed. Close out or merge the branch into `{base}` first, \
         or delete it yourself with `{delete}` and run this start again"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 45-01 / AUTO-01: the fork point is the value the caller supplies, not
    /// the built-in trunk. BOTH directions are asserted in this one body —
    /// a test that only checked the configured base would pass against a
    /// function that ignored its argument in a repository where `develop`
    /// happened to carry the same file.
    #[test]
    fn ensure_phase_worktree_forks_from_the_supplied_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let out = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q", "-b", "develop"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);

        // A planning branch carrying a file that is absent on `develop` —
        // the `.planning/config.json` shape AUTO-01 is about.
        git(&["checkout", "-q", "-b", "workspace/example"]);
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(root.join(".planning/config.json"), "{}").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "planning"]);
        git(&["checkout", "-q", "develop"]);

        let wt = ensure_phase_worktree(root, PhaseId::new(45), false, "workspace/example")
            .expect("worktree off the configured base");
        assert!(
            wt.join(".planning/config.json").exists(),
            "a worktree forked from the configured base must carry its files"
        );

        // NEGATIVE CONTROL: the same call against `develop` must NOT see it.
        let wt_default =
            ensure_phase_worktree(root, PhaseId::new(46), false, devflow_core::config::DEVELOP)
                .expect("worktree off develop");
        assert!(
            !wt_default.join(".planning/config.json").exists(),
            "a worktree forked from develop must not carry the planning branch's files"
        );
    }

    /// `--force` still recreates the phase worktree, from the base, when every commit on
    /// its branch is merged into the base: the old worktree's files are gone and the
    /// branch is re-forked at the base's tip.
    #[test]
    fn force_recreates_a_worktree_whose_branch_is_merged() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| -> String {
            let out = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        git(&["init", "-q", "-b", "develop"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);

        let phase = PhaseId::new(47);
        let wt = ensure_phase_worktree(root, phase, false, "develop").expect("first worktree");
        std::fs::write(wt.join("phase-work.txt"), "work").unwrap();
        devflow_core::test_support::git_command(&wt)
            .args(["add", "-A"])
            .status()
            .expect("spawn git add");
        let committed = devflow_core::test_support::git_command(&wt)
            .args(["commit", "-q", "-m", "phase work"])
            .status()
            .expect("spawn git commit");
        assert!(committed.success(), "premise: the phase commit lands");
        git(&["merge", "-q", "--ff-only", "feature/phase-47"]);
        std::fs::write(wt.join("scratch.txt"), "left in the old worktree").unwrap();

        let recreated = ensure_phase_worktree(root, phase, true, "develop")
            .expect("--force recreates a worktree whose branch is merged");
        assert_eq!(recreated, wt);
        assert!(
            !recreated.join("scratch.txt").exists(),
            "the old worktree was removed"
        );
        assert_eq!(
            git(&["rev-parse", "feature/phase-47"]),
            git(&["rev-parse", "develop"]),
            "the branch is re-forked at the base's tip"
        );
    }
}

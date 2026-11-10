//! The phase worktree `start` forks its run into (`.worktrees/phase-NN/` on
//! `feature/phase-NN`). Moved out of the removed `parallel` module (52-07, D-23)
//! unchanged in behaviour.

use devflow_core::config::FEATURE_PREFIX;
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
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        // Project-resolved: `delete_branch`'s protected-branch list must name
        // the configured trunk, not the default one.
        let _ = GitFlow::for_project(project_root).delete_branch(&branch, true);
    }

    match worktree::add(project_root, &wt, &branch, base, true) {
        Ok(()) => Ok(wt),
        Err(devflow_core::worktree::WorktreeError::Exists(path)) => {
            Err(CliError::Message(format!(
                "worktree already exists at {} — use --force to recreate it",
                path.display()
            )))
        }
        Err(err) => Err(err.into()),
    }
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
}

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
    base: &str,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{FEATURE_PREFIX}phase-{padded}", padded = phase.padded());
    let git = GitFlow::with_config(
        project_root,
        GitFlowConfig {
            develop: base.to_string(),
            ..GitFlowConfig::default()
        },
    );
    // 52.1 D-09.1, D-10: `start` removes nothing an earlier run left, so a leftover
    // branch is refused here rather than by `git worktree add -b`'s own error.
    if !wt.exists() && git.branch_exists(&branch) {
        return Err(leftover_refusal(phase, &format!("branch `{branch}`"), base));
    }

    match worktree::add(project_root, &wt, &branch, base, true) {
        Ok(()) => Ok(wt),
        Err(devflow_core::worktree::WorktreeError::Exists(path)) => Err(leftover_refusal(
            phase,
            &format!("the phase worktree {}", path.display()),
            base,
        )),
        Err(err) => Err(err.into()),
    }
}

/// "1 commit" or "N commits" not merged into `base`, for the lines that keep or refuse
/// to delete a branch.
pub(crate) fn unmerged_commits(count: usize, base: &str) -> String {
    let noun = if count == 1 { "commit" } else { "commits" };
    format!("{count} {noun} not merged into `{base}`")
}

/// `start`'s refusal over `what` (a phase worktree or branch) that an earlier run of
/// `phase` left. `start` removes nothing (52.1 D-09.1, D-10): `devflow discard` is the
/// verb that removes a phase's worktree, branch and run state, and it lists them first.
pub(crate) fn leftover_refusal(phase: PhaseId, what: &str, base: &str) -> CliError {
    CliError::Message(format!(
        "phase {phase}: {what} is left from an earlier run, and `start` removes nothing. \
         Run `devflow discard --phase {phase}` to remove the phase's worktree, branch and run \
         state (it lists them first, and refuses a branch with commits not merged into \
         `{base}` unless you name it), then run this start again"
    ))
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

        let wt = ensure_phase_worktree(root, PhaseId::new(45), "workspace/example")
            .expect("worktree off the configured base");
        assert!(
            wt.join(".planning/config.json").exists(),
            "a worktree forked from the configured base must carry its files"
        );

        // NEGATIVE CONTROL: the same call against `develop` must NOT see it.
        let wt_default =
            ensure_phase_worktree(root, PhaseId::new(46), devflow_core::config::DEVELOP)
                .expect("worktree off develop");
        assert!(
            !wt_default.join(".planning/config.json").exists(),
            "a worktree forked from develop must not carry the planning branch's files"
        );
    }
}

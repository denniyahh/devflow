//! Hermetic git invocation for test fixtures (999.37).
//!
//! Test fixtures build throwaway repositories in tempdirs and shell out to
//! `git` against them. Pinning the working directory is **not** sufficient to
//! keep those commands inside the fixture: git's repository-local environment
//! variables — `GIT_DIR` above all — outrank a process's working directory
//! when git resolves which repository to act on. `git -C <dir>` does not
//! override `GIT_DIR` either (only the `--git-dir` flag does), and
//! `GIT_CEILING_DIRECTORIES` does not contain it, so clearing the variables is
//! the only reliable containment.
//!
//! This matters because Rust runs a test binary's tests as threads in ONE
//! process: if the suite is launched with those variables set, every fixture
//! inherits them at once. That is exactly what happened when a `git push` from
//! a linked worktree ran the pre-push hook (git exports `GIT_DIR` to hooks
//! when the gitdir is non-default) — fixtures retargeted the real checkout,
//! setting `core.bare=true` on it, rewriting its committer identity, and
//! stacking fixture commits onto its `main` branch.
//!
//! `scripts/hooks/pre-push` clears these before running the suite, and
//! `git_env_hermeticity.rs` fails fast if they are present. Both are process
//! level. This helper is the per-command layer: a fixture built through it is
//! contained even when the environment is dirty and whatever the launch path.

use std::path::Path;
use std::process::Command;

/// Git's own list of repository-local environment variables, as reported by
/// `git rev-parse --local-env-vars` (15 entries on git 2.55).
///
/// Kept as a constant rather than shelled out per call so building a command
/// stays free of process spawns; `local_env_vars_match_git` asserts it still
/// agrees with the installed git, so a version that adds one fails loudly
/// instead of silently reopening the hole.
pub const REPO_LOCAL_GIT_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Variables that are not repository-local — and so absent from
/// `--local-env-vars` — but still redirect where git reads or writes.
pub const ALSO_REDIRECTING_GIT_VARS: &[&str] =
    &["GIT_NAMESPACE", "GIT_DISCOVERY_ACROSS_FILESYSTEM"];

/// A `git` command pinned to `repo` **and** stripped of every inherited
/// variable that could redirect it somewhere else.
///
/// Use this for every fixture git invocation instead of building
/// `Command::new("git")` directly. `GIT_EXEC_PATH` is deliberately left alone:
/// it only locates git's own helper binaries and cannot change which
/// repository git acts on.
///
/// Clearing `GIT_CONFIG_COUNT` is sufficient to neutralize any inherited
/// `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` pair — git only reads those when
/// the count is set — so they need no separate sweep.
pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}

/// As [`git_command`], for a program that is not `git` itself but will shell
/// out to it — `cargo`, whose build scripts invoke `git`, is the motivating
/// case. The redirecting variables are inherited all the way down a process
/// tree, so scrubbing only the direct `git` calls would leave that path open.
pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract callers depend on, asserted on the built command rather
    /// than inferred: every redirecting variable is marked for removal.
    #[test]
    fn git_command_marks_every_redirecting_var_for_removal() {
        let cmd = git_command(Path::new("/tmp"));
        let removed: Vec<&str> = cmd
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .filter_map(|(key, _)| key.to_str())
            .collect();

        for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
            assert!(
                removed.contains(var),
                "{var} is not cleared by git_command — a fixture inheriting it \
                 would operate on that repository instead of its tempdir"
            );
        }
    }

    /// GIT_EXEC_PATH must survive: clearing it can break git's own helper
    /// lookup on installations that rely on it, and it cannot redirect
    /// repository resolution.
    #[test]
    fn git_command_preserves_git_exec_path() {
        let cmd = git_command(Path::new("/tmp"));
        assert!(
            !cmd.get_envs()
                .any(|(key, value)| key == "GIT_EXEC_PATH" && value.is_none()),
            "GIT_EXEC_PATH must not be cleared"
        );
    }

    /// Guards the hard-coded list against a git upgrade that adds a
    /// repository-local variable. If this fails, add the new name to
    /// `REPO_LOCAL_GIT_VARS` — do not delete the assertion.
    #[test]
    fn local_env_vars_match_git() {
        let output = Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .expect("run `git rev-parse --local-env-vars`");
        assert!(
            output.status.success(),
            "`git rev-parse --local-env-vars` failed"
        );

        let mut from_git: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        let mut ours: Vec<String> = REPO_LOCAL_GIT_VARS
            .iter()
            .map(|v| (*v).to_string())
            .collect();
        from_git.sort();
        ours.sort();

        assert_eq!(
            ours, from_git,
            "REPO_LOCAL_GIT_VARS has drifted from `git rev-parse --local-env-vars`"
        );
    }
}

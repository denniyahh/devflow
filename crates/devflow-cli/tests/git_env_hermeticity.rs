//! Guard against the 999.37 test-sandbox escape.
//!
//! Git's repository-local environment variables — `GIT_DIR` above all —
//! outrank a process's working directory when git decides which repository to
//! operate on. Rust runs a test binary's tests as threads in ONE process, so
//! if the suite is launched with those variables set, EVERY fixture that
//! shells out to git in a tempdir silently retargets the real repository,
//! however carefully it pinned `.current_dir()`. Observed consequences:
//! `core.bare=true` on the main repo, a worktree's HEAD moved onto a fixture
//! branch, fixture files staged in the real index, and the committer identity
//! rewritten to the fixture's.
//!
//! Git exports `GIT_DIR` into hook environments when the gitdir is
//! non-default — which is precisely the case when pushing from a linked
//! worktree. `scripts/hooks/pre-push` therefore clears these before running
//! the suite. This test is the backstop for every OTHER way the suite can be
//! launched with a dirty environment: `git rebase --exec`, `git bisect run`,
//! any other hook, or a CI runner that sets them.
//!
//! Detection, not prevention: tests run in parallel, so a fixture may already
//! have run by the time this fails. It exists so the failure is loud and
//! names the cause, instead of presenting as an unrelated flake — the
//! original incident surfaced as "41 staleness failures".

use std::process::Command;

/// Git's own authoritative list of repository-local variables, so this stays
/// correct when a git upgrade adds one (15 on git 2.55).
fn repo_local_git_vars() -> Vec<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--local-env-vars")
        .output()
        .expect("run `git rev-parse --local-env-vars`");
    assert!(
        output.status.success(),
        "`git rev-parse --local-env-vars` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn suite_does_not_inherit_repo_local_git_env() {
    // Not in `--local-env-vars` (they are not repository-local), but both
    // still redirect where git looks.
    const ALSO_DANGEROUS: &[&str] = &["GIT_NAMESPACE", "GIT_DISCOVERY_ACROSS_FILESYSTEM"];

    let mut vars = repo_local_git_vars();
    vars.extend(ALSO_DANGEROUS.iter().map(|v| v.to_string()));

    let leaked: Vec<String> = vars
        .into_iter()
        .filter_map(|name| {
            std::env::var(&name)
                .ok()
                // An empty GIT_PREFIX is what git exports for a push from the
                // repository root; it is inert and always present, so
                // flagging it would make this test fire on every hook run.
                .filter(|value| !value.is_empty())
                .map(|value| format!("  {name}={value}"))
        })
        .collect();

    assert!(
        leaked.is_empty(),
        "This test process inherited git's repository-local environment:\n{}\n\n\
         Every fixture that shells out to git will act on THAT repository \
         instead of its tempdir, regardless of `.current_dir()` — this is the \
         999.37 sandbox escape, which corrupts the real checkout.\n\n\
         If you reached here from a git hook, clear them first:\n    \
         unset $(git rev-parse --local-env-vars)\n\
         See scripts/hooks/pre-push for the worked example.",
        leaked.join("\n")
    );
}

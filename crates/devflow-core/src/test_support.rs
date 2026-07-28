//! This crate's test-only helper surface, reachable cross-crate through the
//! `test-support` feature (`lib.rs:76-79`'s
//! `#[cfg(any(test, feature = "test-support"))]` gate keeps all of it absent
//! from a normal build). Two unrelated hazards live here, each documented at
//! its own definitions below: hermetic git invocation for test fixtures
//! (999.37, the module's original scope) and the 999.47 exec-visibility
//! barrier (25-11).
//!
//! ## Hermetic git invocation (999.37)
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

// ## Exec-visibility barrier (25-11/999.47)
//
// `process_start_time`'s doc comment (`crate::agent`) is this codebase's own
// authoritative statement of the mechanism: between `Command::spawn()`
// returning and the child completing `execve`, the child is a copy of its
// parent, so `/proc/<pid>/cmdline` transiently reports the PARENT's argv,
// not the child's own. A test that spawns a child and immediately reads a
// `/proc`-cmdline census about it (via
// `crate::agent::discover_stray_devflow_processes` or its CLI-side
// equivalent) races that window, load-sensitively — 0 failures across 17
// warm local runs, 2 failures in 2 attempts under the loaded shape
// `scripts/check-in-container.sh all` runs (`25-CI-OBSERVATION.md`).
//
// `wait_for_exec_visibility` is the barrier a test must cross before
// asserting on such a census. `crate::agent::agent_running` is NOT such a
// barrier: `kill(pid, 0)` succeeds for a forked-but-unexec'd child, because
// the pid is allocated at `fork()`, well before `execve` runs — a liveness
// poll closes no window at all here.

/// Bounded default wait for [`wait_for_exec_visibility`]. The window this
/// barrier waits out is sub-millisecond in the normal case and the function
/// returns immediately once the child has exec'd, so a generous ceiling
/// costs nothing in the common path — it exists only so a pathological case
/// (a child that never execs, or a wrong `expected_argv0_basename`) fails
/// loudly within a bounded time instead of hanging a test binary.
pub const EXEC_VISIBILITY_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Poll interval for [`wait_for_exec_visibility`]. Matches the granularity
/// [`crate::agent::TERMINATE_VERIFY_POLL`] uses for the same reason: fine
/// enough that the barrier resolves promptly once the condition is true,
/// coarse enough not to busy-loop.
pub const EXEC_VISIBILITY_POLL: std::time::Duration = std::time::Duration::from_millis(2);

/// Poll `/proc/<pid>/cmdline` until `pid`'s argv is genuinely its OWN — not
/// its parent's, transiently inherited across the `fork()`->`execve()`
/// window — and report whether that happened within `wait`.
///
/// Returns `true` only when BOTH hold, checked on every poll:
///
/// (i) argv[0]'s basename equals `expected_argv0_basename`, via
///     [`crate::agent::argv_basename`] — the exact idiom
///     `classify_stray_layer` uses, reused rather than copied so a second
///     basename idiom cannot drift from the first.
/// (ii) the observed cmdline is not byte-identical to the caller's own
///     `/proc/self/cmdline`, captured once at call time. This is the guard
///     against the degenerate case where the caller's own argv[0] basename
///     happens to equal `expected_argv0_basename` — without it, a test
///     asserting on its OWN pid could pass merely because it started out
///     matching, which would make the barrier's answer
///     probabilistically-correct instead of unambiguous.
///
/// Parses the NUL-separated cmdline the same way
/// [`crate::agent::discover_stray_devflow_processes`] does. An unreadable
/// `/proc/<pid>/cmdline` is not itself a failure — a pid that has not yet
/// appeared, or has already exited, is exactly the kind of transient state
/// this function polls through — but a pid that is verifiably not alive
/// (checked via [`crate::agent::agent_running`] on every iteration) returns
/// `false` immediately rather than waiting out the full `wait` ceiling: a
/// dead pid can never become exec-visible, so there is nothing to wait for.
pub fn wait_for_exec_visibility(
    pid: u32,
    expected_argv0_basename: &str,
    wait: std::time::Duration,
    poll: std::time::Duration,
) -> bool {
    let self_cmdline = std::fs::read(format!("/proc/{}/cmdline", std::process::id())).ok();
    let deadline = std::time::Instant::now() + wait;

    loop {
        if !crate::agent::agent_running(pid) {
            return false;
        }

        if let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) {
            let args: Vec<String> = raw
                .split(|&byte| byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect();

            let basename_matches = args
                .first()
                .and_then(|argv0| crate::agent::argv_basename(argv0))
                .is_some_and(|name| name == expected_argv0_basename);
            let differs_from_caller = self_cmdline.as_deref() != Some(raw.as_slice());

            if basename_matches && differs_from_caller {
                return true;
            }
        }

        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(poll);
    }
}

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

    /// Positive case (25-11/999.47, the whole point of the barrier): a real
    /// child whose argv[0] basename is known and differs from this test
    /// binary's own must be reported exec-visible, and after the function
    /// returns, `/proc/<pid>/cmdline` must genuinely hold the child's own
    /// argv — never the caller's.
    #[test]
    fn wait_for_exec_visibility_detects_a_real_child_and_leaves_it_exec_visible() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep fixture");
        let pid = child.id();

        let visible = wait_for_exec_visibility(
            pid,
            "sleep",
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(2),
        );
        assert!(visible, "a real sleep child must become exec-visible");

        let raw = std::fs::read(format!("/proc/{pid}/cmdline"))
            .expect("must be able to read the child's cmdline after the barrier returns");
        let args: Vec<String> = raw
            .split(|&b| b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        assert_eq!(
            args.first().map(String::as_str),
            Some("sleep"),
            "after the barrier returns, cmdline must be the child's own argv, not the caller's"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Timeout is a value, not a hang (25-11/999.47): calling against this
    /// process's OWN pid with an `expected_argv0_basename` that can never
    /// match must return `false` within roughly `wait`, not indefinitely.
    #[test]
    fn wait_for_exec_visibility_times_out_bounded_when_it_never_matches() {
        let start = std::time::Instant::now();
        let visible = wait_for_exec_visibility(
            std::process::id(),
            "this-basename-can-never-match-anything",
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(2),
        );
        let elapsed = start.elapsed();

        assert!(
            !visible,
            "a basename that can never match must return false"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "must return within roughly `wait`, not hang indefinitely — took {elapsed:?}"
        );
    }

    /// Dead pid (25-11/999.47): a pid that is not alive must return `false`
    /// promptly, never panicking and never blocking for the full `wait`
    /// ceiling — the caller cannot distinguish "will never be alive" from
    /// "still forking" without this short-circuit.
    #[test]
    fn wait_for_exec_visibility_returns_false_promptly_for_a_dead_pid() {
        let start = std::time::Instant::now();
        let visible = wait_for_exec_visibility(
            0x7FFF_FFFE,
            "anything",
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(20),
        );
        let elapsed = start.elapsed();

        assert!(!visible, "a dead pid must never be reported exec-visible");
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "a dead pid must not wait out the full ceiling, took {elapsed:?}"
        );
    }

    /// Self-argv guard (25-11/999.47): the function must not report success
    /// merely because argv[0]'s basename matches while the observed cmdline
    /// is still byte-identical to the caller's own `/proc/self/cmdline`.
    /// Calling it against the caller's own pid with the caller's own argv[0]
    /// basename matches condition (i) by construction — the result must
    /// still be `false`, because condition (ii) never holds for the caller's
    /// own unchanging cmdline.
    #[test]
    fn wait_for_exec_visibility_rejects_a_self_match_on_unchanged_cmdline() {
        let self_pid = std::process::id();
        let raw = std::fs::read(format!("/proc/{self_pid}/cmdline"))
            .expect("must be able to read this process's own cmdline");
        let self_basename = raw
            .split(|&b| b == 0)
            .find(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .and_then(|arg0| {
                std::path::Path::new(&arg0)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            })
            .expect("must be able to derive this process's own argv[0] basename");

        let visible = wait_for_exec_visibility(
            self_pid,
            &self_basename,
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(2),
        );

        assert!(
            !visible,
            "a self-match on unchanged cmdline must never report exec-visible, even though \
             argv[0]'s basename matches by construction"
        );
    }
}

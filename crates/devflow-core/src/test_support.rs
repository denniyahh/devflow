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

// Hermetic git invocation constants and constructors now live in
// `crate::git` — the always-compiled home (this module is gated
// `#[cfg(any(test, feature = "test-support"))]`, absent from a normal
// build, so it cannot be the canonical home for a production-reachable
// constructor — 27-01/D-01). Re-exported here so every existing fixture
// call site (~40 across both crates' test targets) keeps compiling
// unchanged, and so the two lists can never drift apart.
pub use crate::git::{
    ALSO_REDIRECTING_GIT_VARS, REPO_LOCAL_GIT_VARS, git_command, hermetic_command,
};

// ## Why there is no absent-`git` (`NoGitPath`) harness in THIS crate
//
// 35-01 planned one `NoGitPath` guard per crate, so that criterion 6's tests
// could force `git` to be unresolvable and drive `phase_commit_count`'s
// could-not-measure branch from inside `devflow-core`. **That guard was built
// here, measured, and removed.** It is recorded rather than silently omitted,
// because the next author to need a failing `git` will otherwise rebuild it.
//
// A `PATH`-replacing guard mutates process-global state, and `cargo test` runs
// this crate's whole suite as threads in ONE process. `devflow-core` shells out
// to `git` from eight modules (`git`, `version`, `worktree`, `agent_result`,
// `monitor`, `ship_evidence`, `hooks`, and this one), and — decisively — its
// tests reach `git` by calling PRODUCTION code that spawns it, not only through
// fixture helpers. So no fixture-level lock can cover them: serializing this
// module's own `git()` helper still left
// `agent_result::tests::evaluate_layer2_exit_zero_no_commits_is_failed` failing,
// because its `git` call happens inside `evaluate_layer2` itself.
//
// Measured, with a control:
//
//   - guard used by three regression tests ....... 1-5 unrelated failures/run
//   - guard used by its own sanity test only ..... 1 failure in 8 runs
//   - sanity test `#[ignore]`d (control) ......... 0 failures in 10 runs
//
// The asymmetry between the last two lines is what identifies the guard itself
// as the cause rather than a pre-existing flake. Ten clean runs is a weak bound
// on the control arm, not a proof of zero flake rate; it is enough to establish
// the direction, which is all that is being claimed.
//
// **What to use instead.** `hermetic_command` sets `cmd.current_dir(dir)`, so
// passing a path that does not exist makes the spawn itself fail and
// `.output()` return `Err` — the identical arm a missing binary produces,
// reached with no environment mutation and therefore no effect on any other
// test. `agent_result`'s `phase_commit_count_reports_none_when_git_cannot_run`
// and `evaluate_layer3_unmeasurable_count_is_unknown_not_failed` both take that
// route. It is also immune to the latent fragility of a `PATH` guard, which a
// future refactor to an absolute `git` path would disarm silently.
//
// **When that is not enough**, because the code under test must also READ a
// file from `project_root` (`evaluate_layer2` reads its exit file there, so a
// non-existent root would fail for the wrong reason), the test belongs in
// `devflow-cli`'s binary, where every `PATH` mutation goes through one
// `ENV_MUTEX` that its `git`-touching tests already hold. Criterion 6's
// layer-level and cascade-level tests live in
// `devflow-cli/src/pipeline_outcomes.rs` for exactly this reason; they call the
// same `pub` functions, so only the binary differs.

#[cfg(test)]
mod tests {
    use super::*;

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

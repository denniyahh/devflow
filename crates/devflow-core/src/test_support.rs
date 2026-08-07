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

// ## The absent-`git` harness (35-01, criteria 1 and 6)
//
// **The `#[cfg(test)]` gate on this whole region is load-bearing and is the
// reason there are two `NoGitPath` types in this workspace rather than one
// shared one (35-01/F-2).**
//
// The enclosing module is gated `#[cfg(any(test, feature = "test-support"))]`
// (`lib.rs`), so `devflow-cli` CAN reach it — it depends on `devflow-core`
// with `features = ["test-support"]`. Two consequences, either of which alone
// would rule out sharing:
//
//   1. `tempfile` is a DEV-dependency of `devflow-core`. In the
//      `test-support`-feature build `devflow-cli` links, `cfg(test)` is false
//      for this crate and its dev-dependencies are absent, so a
//      `tempfile`-based guard exposed under the feature gate would not
//      compile. Making it compile would mean promoting `tempfile` to an
//      optional real dependency of a published crate.
//   2. A guard reachable from `devflow-cli` would give a single test binary
//      access to two guards backed by two different mutexes, which is exactly
//      the `PATH` race the one-var-one-mutex invariant exists to prevent.
//
// The narrower `#[cfg(test)]` gate closes both: this region exists only in
// `devflow-core`'s OWN test binary. `devflow-cli` cannot reach it in any
// build, so no test binary can ever hold two `PATH` guards under two mutexes
// — the race is prevented structurally, not by discipline. `devflow-cli`
// carries its own identically-shaped `NoGitPath` guarded by its own single
// `ENV_MUTEX`. Criterion 6 gets the same harness SHAPE rather than the same
// symbol, deliberately.

/// This crate's first and only `PATH` mutex.
///
/// `devflow-core` had zero `PATH` mutations before 35-01 (verified: `rg` for
/// `set_var("PATH"` over `crates/devflow-core/src/` returned nothing, against
/// six files in `crates/devflow-cli/src/`). It follows the same
/// one-static-per-test-binary shape as the two other module-scoped env mutexes
/// in this crate (`gates.rs`, `config.rs`), each of which guards a disjoint
/// set of variables — `PATH` is guarded here and nowhere else in this crate.
///
/// A future author adding a second `PATH`-mutating fixture to `devflow-core`'s
/// tests is joining this mutex, not creating another one.
#[cfg(test)]
pub(crate) static PATH_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire [`PATH_MUTEX`], recovering the guard if a previous holder panicked.
///
/// **This is the intended entry point; do not call `PATH_MUTEX.lock().unwrap()`
/// directly.** Mirrors `devflow-cli`'s `test_support::env_lock()`, including
/// its reasoning for tolerating poison: the state this mutex guards is the
/// process environment, and the only thing that mutates it here is
/// [`NoGitPath`], which restores `PATH` in its `Drop` — and `Drop` runs during
/// unwinding. By the time a poisoned guard reaches the next test, the state
/// poisoning would warn about has already been restored.
///
/// That argument is conditional on the restore being an RAII `Drop` and not a
/// trailing statement. Replacing [`NoGitPath`] with a trailing
/// `set_var("PATH", original)` would convert this function from "tolerates
/// poison because cleanup already happened" into "silently hands the next test
/// a `PATH` naming a deleted directory", with no compiler error and no failing
/// test to say so.
#[cfg(test)]
pub(crate) fn path_lock() -> std::sync::MutexGuard<'static, ()> {
    PATH_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII guard that REPLACES `PATH` with a deliberately EMPTY directory, so
/// `git` cannot be resolved at all for the scope it is bound in.
///
/// **Why empty rather than a failing shim (35-01/F-1).** The consumers of
/// [`crate::agent_result::phase_commit_count`] must distinguish "the git child
/// could not be executed" from "git ran and reported zero". Only the first is
/// a measurement failure, and only an UNRESOLVABLE binary produces it:
/// `Command::output()` returns `Err(NotFound)` when the program cannot be
/// spawned, whereas a shim that runs and exits non-zero returns `Ok(status)` —
/// a real observation, which this crate deliberately maps to `Some(0)`. A test
/// built on a failing shim would exercise the already-correct path while
/// appearing to cover the new one.
///
/// Structurally identical to `devflow-cli`'s `test_support::NeutralPath` and
/// `NoGitPath`: the guard owns the `TempDir`, so the directory outlives every
/// use of the `PATH` that names it, and `Drop` restores the captured value
/// before the `TempDir` is dropped (a type's own `Drop::drop` runs before its
/// fields are dropped), so `PATH` never transiently names a deleted directory.
/// The restore runs on EVERY exit path including a panicking one, which a
/// trailing statement would not.
///
/// **The caller must already hold [`PATH_MUTEX`]** (via [`path_lock`]).
/// `set_var` is process-wide and `cargo test` runs in parallel; this guard
/// makes the restore unconditional, it does not make the mutation safe on its
/// own.
///
/// # Read this before using it in a new test
///
/// **Measured, not hypothetical (35-01, F-1b): using this guard in
/// `agent_result`'s tests failed 1-5 UNRELATED sibling tests per run,
/// nondeterministically.** `devflow-core` shells out to `git` from eight
/// modules (`git`, `version`, `worktree`, `agent_result`, `monitor`,
/// `ship_evidence`, `hooks`, and this one), all of which compile into a single
/// test binary that `cargo test` runs in parallel. Holding [`PATH_MUTEX`]
/// serializes only the tests that opt into it; every other test that invokes
/// `git` inside the guarded window sees an empty `PATH` and fails for a reason
/// that does not point anywhere near the offender.
///
/// **Prefer an unspawnable working directory.** `hermetic_command` sets
/// `cmd.current_dir(dir)`, so passing a path that does not exist makes the
/// spawn itself fail and `.output()` return `Err` — the same arm this guard
/// produces, reached with no environment mutation and therefore no effect on
/// any other test. `phase_commit_count_reports_none_when_git_cannot_run` and
/// `evaluate_layer3_unmeasurable_count_is_unknown_not_failed` both take that
/// route, and their doc comments record why.
///
/// This guard is retained because
/// `tests::no_git_path_makes_git_unresolvable_and_restores_it` is what
/// establishes that the `PATH`-replacement mechanism works at all — the
/// control `devflow-cli`'s own `NoGitPath` (which IS used by a regression
/// test, under the single `ENV_MUTEX` its sibling tests already hold) depends
/// on. If you reach for it here, scope it to one call and expect flakes.
#[cfg(test)]
pub(crate) struct NoGitPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl NoGitPath {
    /// Named `install`, not `new`: binding it is not bookkeeping, it mutates
    /// process-global state at the moment of the call.
    pub(crate) fn install() -> Self {
        // Deliberately empty — see the type's doc comment. Nothing is written
        // into this directory, by design.
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::var_os("PATH");
        // SAFETY: the caller holds PATH_MUTEX (documented precondition), so
        // no other test thread is reading or writing PATH concurrently.
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self {
            _dir: dir,
            original,
        }
    }
}

#[cfg(test)]
impl Drop for NoGitPath {
    fn drop(&mut self) {
        // SAFETY: still serialized under the PATH_MUTEX guard the caller holds
        // for at least as long as this guard's own scope.
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NC-1 (35-VALIDATION.md), this crate's copy, and the reason it is
    /// load-bearing rather than ceremony: criteria 1 and 6 both assert on
    /// behaviour that occurs ONLY when `git` cannot be executed. If
    /// [`NoGitPath`] silently failed to take effect — wrong `PATH` ordering, a
    /// guard dropped early, an absolute-path `git` invocation — every
    /// downstream assertion would still run and every one of them would pass
    /// for the wrong reason. It must pass before any criterion-1 or
    /// criterion-6 result is believed.
    ///
    /// The control is inside the test: the pre-guard call must succeed and the
    /// post-drop call must succeed, so a guard that did nothing at all cannot
    /// produce a green result — all three observations would agree and the
    /// middle assertion would fail.
    ///
    /// `git` is invoked through [`git_command`], the same PATH-resolved
    /// constructor production code uses (`git.rs`'s `git_command` ->
    /// `hermetic_command` -> `Command::new("git")`), so this measures the
    /// harness against the real spawn path rather than an approximation of it.
    #[test]
    fn no_git_path_makes_git_unresolvable_and_restores_it() {
        let _guard = path_lock();
        let dir = tempfile::tempdir().unwrap();
        let path_before = std::env::var_os("PATH");

        let before = git_command(dir.path()).arg("--version").output();
        assert!(
            before.is_ok(),
            "control: `git` must be resolvable BEFORE the guard is installed, \
             otherwise the middle assertion below proves nothing"
        );

        let during = {
            let _no_git = NoGitPath::install();
            git_command(dir.path()).arg("--version").output()
        };
        let err = during.expect_err("NoGitPath must make `git` unresolvable");
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::NotFound,
            "the spawn must fail because the binary cannot be found — any other \
             error kind means the guard blocked git for the wrong reason"
        );

        let after = git_command(dir.path()).arg("--version").output();
        assert!(
            after.is_ok(),
            "control: `git` must be resolvable again once the guard has dropped"
        );
        assert_eq!(
            std::env::var_os("PATH"),
            path_before,
            "PATH must be byte-identical to its pre-guard value after the guard drops"
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

//! Shared `#[cfg(test)]` fixtures for `devflow-cli`'s test suite (19-06).
//!
//! Declared as `#[cfg(test)] mod test_support;` on the `mod` item in
//! `main.rs`, not `#![cfg(test)]` inside this file, so the non-test binary
//! build never sees this module at all (Pitfall 5 — a binary-only crate
//! compiles without `#[cfg(test)]`, so a mid-split item used only by test
//! code would otherwise trip a `dead_code` lint under `-D warnings`).
//!
//! Every item here is a mechanical, byte-for-byte relocation out of
//! `main.rs`'s `mod tests` — no body was retyped or edited. `ENV_MUTEX` and
//! these fixtures are used by exactly one shared `mod tests` today and by
//! every future sibling cluster's own test module once the split lands
//! (19c–19f), so they live in a module every future cluster can import
//! rather than inside any one cluster.

use devflow_core::agents;
use devflow_core::state::State;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Serializes tests that mutate process-global env vars (`set_var`/
/// `remove_var` are process-wide and `cargo test` runs in parallel by
/// default) so they don't race each other.
///
/// **D-04 invariant, stated explicitly: every env var is guarded by
/// exactly one mutex, and no var is touched under two.** This invariant currently
/// holds only by accident across three independent statics — this one, and
/// two more in `devflow-core` (`gates.rs:348`, `config.rs:174`). Those two
/// are safe today only because `devflow-core` and `devflow-cli` compile
/// into different test binaries, so their env mutations can never race each
/// other's process. Nothing in the type system or a lint enforces the
/// invariant across that crate boundary — it is true today only because no
/// env var is currently guarded by more than one of the three statics.
///
/// This mutex currently guards five variables: `PATH`,
/// `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`,
/// `DEVFLOW_GATE_NOTIFY_CMD`, `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` (WR-02,
/// phase 20 review). A future author adding a sixth mutated variable to
/// this crate's tests is joining this set — guard it here, not with a new
/// mutex (D-02: per-module mutexes were rejected on measured evidence that
/// `PATH` alone is mutated 36 times across 12 lock regions spanning at
/// least three future target clusters).
///
/// One static suffices for the whole `devflow` binary crate after the
/// `main.rs` split (19c–19f): every D-05 target module stays inside this
/// same binary crate, so `cargo test -p devflow` compiles them all into
/// exactly one test binary regardless of how many modules the split
/// creates — the one-instance-per-process guarantee this mutex depends on
/// is preserved by construction, not by convention.
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Acquire [`ENV_MUTEX`], recovering the guard if a previous holder panicked.
///
/// **This is the intended entry point; do not call `ENV_MUTEX.lock().unwrap()`
/// directly.** [`NeutralPath`]'s doc comment already describes the cascade this
/// exists to stop, in the paragraph beginning "What the trailing-statement
/// shape costs" — one legible failing assertion becomes a `PoisonError` panic
/// in every subsequent env-mutating test in the binary. That paragraph
/// documents the amplification; this function is what prevents it. Measured on
/// this suite: a single induced `assert!(false)` under the lock reported **25
/// failures (24 of them `PoisonError`)** through `.lock().unwrap()`, and
/// **exactly 1** through this accessor.
///
/// **Why recovering a poisoned guard is CORRECT here, not merely convenient.**
/// Poison exists to warn that a panic may have left data behind the lock in a
/// half-mutated state. The data this mutex guards is not the `()` payload — it
/// is the process environment, and every mutation of it in this crate's tests
/// is restored on the unwinding path by an RAII guard rather than by a trailing
/// statement:
///
/// - [`NeutralPath`] restores (or removes) `PATH` in its `Drop`, and holds the
///   neutral `TempDir` so `PATH` never transiently names a deleted directory.
/// - [`ReapMonitorOnDrop`] reaps the detached monitor wrapper in its `Drop`,
///   with a `std::thread::panicking()` interlock so it cannot double-panic into
///   an `abort()` during the very unwind it is cleaning up after.
///
/// `Drop` runs during unwinding, so by the time the poisoned guard is handed to
/// the next test, the state that poisoning would warn about has already been
/// restored. That is the whole argument, and it is conditional:
///
/// **Without those guards this accessor WOULD be unsound.** It is stated
/// plainly because the failure is silent — a future author who replaces a
/// `NeutralPath` binding with a trailing `set_var("PATH", original)`, or drops a
/// `ReapMonitorOnDrop` in favour of a trailing `reap_spawned_monitor` call, will
/// see no compiler error and no failing test. They will instead have converted
/// this function from "tolerates poison because cleanup already happened" into
/// "silently hands the next test a `PATH` naming a deleted directory". If you
/// are removing an unwind-safe guard, you are changing this function's premise;
/// re-read [`NeutralPath`] before you do.
///
/// Directly mutated env vars inside a lock region are still the caller's
/// responsibility to restore — this accessor does not make an unguarded
/// `set_var` safe, any more than [`NeutralPath`] makes one safe on its own.
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Build a real git repo (main + develop, with a Cargo.toml committed) so
/// the terminal-path hooks (`VersionBump`, `BranchCleanup`) exercised below
/// have real git plumbing to operate on rather than an empty directory.
pub(crate) fn init_repo(root: &Path) {
    let git = |args: &[&str]| {
        let ok = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "devflow@example.com"]);
    git(&["config", "user.name", "DevFlow Tests"]);
    git(&["config", "commit.gpgsign", "false"]);
    git(&["config", "tag.gpgsign", "false"]);
    git(&["config", "core.hooksPath", "/dev/null"]);
    std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "init"]);
    git(&["branch", "-M", "main"]);
    git(&["checkout", "-q", "-b", "develop"]);
}

/// Same as [`init_repo`], but without a committed `Cargo.toml`, so
/// `version_bump` takes its no-version-file branch. Mirrors
/// `devflow_core::hooks`' `init_repo_with_options(root, false)`.
pub(crate) fn init_repo_no_version_file(root: &Path) {
    let git = |args: &[&str]| {
        let ok = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "devflow@example.com"]);
    git(&["config", "user.name", "DevFlow Tests"]);
    git(&["config", "commit.gpgsign", "false"]);
    git(&["config", "tag.gpgsign", "false"]);
    git(&["config", "core.hooksPath", "/dev/null"]);
    std::fs::write(root.join("README.md"), "no version file in this repo\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "init"]);
    git(&["branch", "-M", "main"]);
    git(&["checkout", "-q", "-b", "develop"]);
}

/// Land one real commit on `feature/phase-{phase:02}`, creating the branch
/// from the current `HEAD` if it does not already exist. Designed to be
/// called repeatedly within one test, so each call adds exactly one commit
/// to the `develop..branch` range — assumes [`init_repo`] has already run.
///
/// Uses plain `checkout`, never `checkout -B`, when the branch already
/// exists: `-B` resets an existing branch to `HEAD`, which would silently
/// discard the commits an earlier call in the same test already made.
pub(crate) fn commit_on_feature_branch(root: &Path, phase: u32, label: &str) {
    let git = |args: &[&str]| {
        let ok = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    };
    let branch = format!("feature/phase-{phase:02}");
    let branch_exists = devflow_core::test_support::git_command(root)
        .args(["rev-parse", "--verify", &branch])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if branch_exists {
        git(&["checkout", &branch]);
    } else {
        git(&["checkout", "-b", &branch]);
    }
    let file_name = format!("{label}.txt");
    std::fs::write(root.join(&file_name), label).unwrap();
    git(&["add", &file_name]);
    git(&["commit", "-m", label]);
}

/// TEST-ONLY adapter (module-scope so any test can reach it — hoisted
/// from a test-function-local `AlwaysRejectAdapter`, 18f Task 1) whose
/// `preflight` fails unconditionally, with no interior mutability. Two
/// module-scope fixtures that both mean "always fails preflight" would
/// drift, so this is the single one; `run_preflight_adapter_hook_override_fires`
/// (above) and 18f's new wedge-reproduction tests (below) both use it.
///
/// `FailOnceAdapter`, just below, explicitly documents that an
/// unconditionally-failing adapter would recurse into a second gate no
/// pre-18f test seeds a response for (CR-01, 17-08). That is no longer
/// true here: 18f's persisted `preflight_retries` ceiling
/// (`mode::MAX_PREFLIGHT_RETRIES`) bounds the recursion regardless, so
/// an unconditionally-failing preflight now terminates in a logged
/// `abort` instead of blocking forever on a second gate's
/// `poll_response`.
pub(crate) struct AlwaysFailAdapter;

impl agents::AgentAdapter for AlwaysFailAdapter {
    fn name(&self) -> &'static str {
        "test-always-fail"
    }
    fn exec_command(
        &self,
        _phase: u32,
        _prompt: &str,
        _roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        ("true", Vec::new())
    }
    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
    fn preflight(&self, _state: &State) -> Result<(), String> {
        Err("test adapter always rejects".to_string())
    }
}

/// TEST-ONLY adapter whose `preflight` fails on the first call only —
/// modeled on `AlwaysFailAdapter` above, but with a `Cell<bool>` flag
/// so any SECOND call through this specific adapter reference would
/// pass. An adapter that fails unconditionally would make a recursive
/// `launch_stage` retry fail its OWN preflight check too, recursing into
/// a second gate this test never seeds a response for — blocking on
/// `poll_response` instead of asserting.
pub(crate) struct FailOnceAdapter {
    failed_once: std::cell::Cell<bool>,
}

impl FailOnceAdapter {
    pub(crate) fn new() -> Self {
        Self {
            failed_once: std::cell::Cell::new(false),
        }
    }
}

impl agents::AgentAdapter for FailOnceAdapter {
    fn name(&self) -> &'static str {
        "test-fail-once"
    }
    fn exec_command(
        &self,
        _phase: u32,
        _prompt: &str,
        _roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        ("true", Vec::new())
    }
    fn completion_signal_detected(&self, _output: &str) -> bool {
        false
    }
    fn preflight(&self, _state: &State) -> Result<(), String> {
        if self.failed_once.get() {
            Ok(())
        } else {
            self.failed_once.set(true);
            Err("test adapter fails on the first preflight call only".to_string())
        }
    }
}

/// Create a harmless, always-succeeding executable named `name` in a
/// fresh tempdir — used to satisfy `ensure_agent_binary` and let
/// `monitor::spawn_monitor`'s backgrounded `"$@"` exec safely resolve to
/// a no-op instead of a real agent CLI. This host has real
/// `claude`/`codex`/`opencode` binaries on PATH (the identical concern
/// documented on `transition_resets_infra_failures`), so any real
/// `launch_stage` completion here — both the recursive retry inside
/// `run_preflight` and this test's own simulated caller continuation —
/// must never resolve `state.agent`'s adapter program name to a real
/// CLI.
/// A PATH directory containing ONLY a `git` symlink — no agent CLIs.
///
/// For tests that must guarantee `launch_stage` can never find and spawn
/// a real `claude`/`codex`/`opencode` binary, without also making `git`
/// unresolvable process-wide (19i). Unlike `prepend_path`, which layers a
/// stub on top of the real PATH, this REPLACES PATH entirely — the real
/// PATH's entries (which contain the agent CLIs on a dev host) must not
/// be searched at all, only this curated directory.
pub(crate) fn agent_free_git_only_path_dir() -> tempfile::TempDir {
    let real_git = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let candidate = dir.join("git");
                candidate.is_file().then_some(candidate)
            })
        })
        .expect("git must be resolvable on PATH to run this test");
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&real_git, dir.path().join("git")).unwrap();
    dir
}

/// RAII guard that REPLACES `PATH` with an [`agent_free_git_only_path_dir`]
/// for the scope it is bound in, and restores the previous `PATH` on EVERY
/// exit path of that scope — including a path on which a later assertion
/// panics (WR-05).
///
/// Same reasoning as [`ReapMonitorOnDrop`], applied to a different resource:
/// a plain trailing `set_var("PATH", original)` only runs on the success path,
/// since Rust abandons the rest of a function's statements the instant a panic
/// begins unwinding, so it is the language's own `Drop` guarantee — not a call
/// ordering convention — that makes the restore unconditional.
///
/// What the trailing-statement shape costs when a region does panic is
/// specific and compounding: `PATH` stays pointed at the neutral tempdir,
/// whose `TempDir` the same unwind then drops and DELETES, so every other
/// parallel test thread inherits a `PATH` naming a directory that no longer
/// exists; and the panic poisons [`ENV_MUTEX`], turning every subsequent
/// `ENV_MUTEX.lock().unwrap()` into a `PoisonError` panic. One legible
/// failing assertion becomes a cascade across the whole binary.
///
/// The guard owns the `TempDir`, so the neutral directory outlives every use
/// of the `PATH` that names it. `Drop` restores the captured value first and
/// only then drops the `TempDir` (a type's own `Drop::drop` runs before its
/// fields are dropped), so `PATH` never transiently names a deleted directory.
///
/// **The caller must already hold [`ENV_MUTEX`].** `set_var` is process-wide
/// and `cargo test` runs in parallel; this guard makes the restore
/// unconditional, it does not make the mutation safe on its own.
pub(crate) struct NeutralPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl NeutralPath {
    /// Named `install`, not `new`: binding it is not bookkeeping, it mutates
    /// process-global state at the moment of the call.
    pub(crate) fn install() -> Self {
        let dir = agent_free_git_only_path_dir();
        let original = std::env::var_os("PATH");
        // SAFETY: the caller holds ENV_MUTEX (documented precondition), so
        // no other test thread is reading or writing PATH concurrently.
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self {
            _dir: dir,
            original,
        }
    }
}

impl Drop for NeutralPath {
    fn drop(&mut self) {
        // SAFETY: still serialized under the ENV_MUTEX guard the caller holds
        // for at least as long as this guard's own scope.
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}

/// [`agent_free_git_only_path_dir`], extended with a real `sh` symlink
/// (needed by `monitor::spawn_monitor`'s backgrounding script) and a
/// harmless no-op stub for `program` (needed by `ensure_agent_binary`),
/// so a preflight-resolved relaunch through `launch_stage`/
/// `launch_stage_inner` can run to completion under a REPLACED PATH
/// instead of merely failing at `ensure_agent_binary`. 18f's
/// wedge-reproduction tests need the relaunch to actually happen (to
/// prove the fix), not merely to error out before reaching it —
/// `program` is always the STUBBED binary, never the real
/// `claude`/`codex`/`opencode` CLI, since PATH still never includes the
/// real system directories that hold it (19i's replace-not-prepend
/// requirement is preserved).
pub(crate) fn agent_free_dir_with_agent_stub(program: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = agent_free_git_only_path_dir();
    let real_sh = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths).find_map(|d| {
                let candidate = d.join("sh");
                candidate.is_file().then_some(candidate)
            })
        })
        .expect("sh must be resolvable on PATH to run this test");
    std::os::unix::fs::symlink(&real_sh, dir.path().join("sh")).unwrap();
    let path = dir.path().join(program);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    dir
}

pub(crate) fn stub_agent_binary(name: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    dir
}

/// Prefix `PATH` with `stub_dir`, keeping the rest of `original` intact
/// so `sh`/`git` still resolve normally — only the stubbed binary name
/// is shadowed (it is found first).
pub(crate) fn prepend_path(
    stub_dir: &tempfile::TempDir,
    original: &Option<std::ffi::OsString>,
) -> std::ffi::OsString {
    let mut dirs = vec![stub_dir.path().to_path_buf()];
    if let Some(original) = original {
        dirs.extend(std::env::split_paths(original));
    }
    std::env::join_paths(dirs).unwrap()
}

/// Count `stage_launched` events recorded for `phase` across the WHOLE
/// event log — `last_event_for_phase` only sees the most recent line and
/// cannot distinguish one launch from two.
pub(crate) fn stage_launched_count(root: &Path, phase: u32) -> usize {
    std::fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| {
            event.get("phase").and_then(serde_json::Value::as_u64) == Some(u64::from(phase))
                && event.get("event").and_then(serde_json::Value::as_str) == Some("stage_launched")
        })
        .count()
}

/// The single reap mechanism: escalate through `terminate_and_verify`
/// (bounded TERM-then-KILL — 999.44 measured 15 of 15 orphaned monitor
/// wrappers surviving a bare `SIGTERM` against this exact process shape, so
/// an unescalated signal would leave the leak in place while appearing to
/// fix it), then return a VERIFIED liveness answer.
///
/// Asserts nothing and panics on no path — the caller decides what a `false`
/// means. This matters because one of its two callers
/// ([`ReapMonitorOnDrop::drop`]) may run during a stack unwind, where a
/// panic would call `abort()` and kill the whole test binary instead of
/// merely failing the one test in flight.
fn reap_monitor_pid(pid: u32) -> bool {
    devflow_core::agent::terminate_and_verify(
        pid,
        devflow_core::agent::TERMINATE_VERIFY_WAIT,
        devflow_core::agent::TERMINATE_VERIFY_POLL,
    );
    !devflow_core::agent::agent_running(pid)
}

/// Reap the detached monitor wrapper a real `launch_stage_inner` call just
/// spawned, verifying its death rather than assuming it (WR-03, 999.46).
///
/// This plain-call form runs ONLY if every preceding statement in its caller
/// returned normally — a panic anywhere between the launch and this call
/// skips it entirely, since Rust abandons the remaining statements of a
/// function the moment a panic begins unwinding. [`ReapMonitorOnDrop`] is
/// therefore the default for any test with assertions between the launch and
/// teardown; this function is for the narrow case where a guard cannot be
/// bound. Its only caller after G-25-2 (25-17) is
/// `tests::trailing_reap_call_is_skipped_when_a_later_assertion_panics`,
/// which exists precisely to demonstrate this failure mode.
///
/// Any test that drives a real `launch_stage_inner` causes it to spawn a
/// detached monitor wrapper (`monitor::spawn_monitor`) that is DESIGNED to
/// outlive the call that spawned it — that is exactly what lets a real
/// `devflow start` invocation return while the monitor keeps watching the
/// agent. A test that drives the same call path gets the same detached
/// wrapper, and since nothing else in a test's lifetime ever signals it, the
/// TEST is the only thing that ever could — so the test owns reaping it.
///
/// The pid to reap comes from `state.monitor_pid` on the same `&mut State`
/// the caller handed to `launch_stage_inner` — the only on-disk-free handle a
/// test has on what it just caused to exist (`pipeline_launch.rs` writes the
/// spawned pid there immediately after `monitor::spawn_monitor` returns).
/// This function never scans `/proc`, never guesses, and never signals a pid
/// it did not read from that same handle.
///
/// Must be called BEFORE the caller's `TempDir` guard drops — reaping after
/// the project root has already been deleted is 999.44's reproduction shape
/// with extra steps, not a fix for it.
///
/// Tolerates `state.monitor_pid == None` by returning quietly, with no
/// `unwrap`, no `expect` and no panic: an early-failing `launch_stage_inner`
/// clears the field before any fallible step (`pipeline_launch.rs:70`), and a
/// teardown helper that panics on that path would mask the launch's own
/// failure rather than merely fail to clean up after it.
pub(crate) fn reap_spawned_monitor(state: &State) {
    let Some(pid) = state.monitor_pid else {
        return;
    };
    assert!(
        reap_monitor_pid(pid),
        "monitor wrapper pid {pid}, spawned by this test's own launch_stage_inner call, must be \
         verified dead after reaping — not merely assumed dead"
    );
}

/// RAII guard that reaps a spawned monitor wrapper on EVERY exit path of the
/// scope it is bound in — including a path on which a later assertion
/// panics. This is the fix for G-25-2 / WINDOWS.md item 3: a plain trailing
/// call to [`reap_spawned_monitor`] only runs on the success path, since Rust
/// abandons the rest of a function's statements the instant a panic begins
/// unwinding, so it is the language's own `Drop` guarantee — not a call
/// ordering convention — that makes the reap unconditional.
pub(crate) struct ReapMonitorOnDrop {
    pid: Option<u32>,
}

impl ReapMonitorOnDrop {
    /// Capture `state.monitor_pid` BY VALUE, not `&'a State`.
    ///
    /// By value: at every call site this guard is bound strictly AFTER the
    /// test's final `&mut state` use, so a borrow would not conflict with
    /// anything at that point — but a `&'a State` guard would still tie the
    /// guard's lifetime to the `State` binding's own scope, which is exactly
    /// the kind of incidental coupling a teardown guard should not have. A
    /// bare `Option<u32>` keeps the guard independent of `State` entirely.
    ///
    /// Named `after_launch`, not `new`: bound before the launch call
    /// returns, it captures `None` and silently does nothing — the
    /// constructor name carries the ordering requirement to the call site
    /// rather than leaving it to a doc comment alone.
    pub(crate) fn after_launch(state: &State) -> Self {
        Self {
            pid: state.monitor_pid,
        }
    }
}

impl Drop for ReapMonitorOnDrop {
    fn drop(&mut self) {
        let Some(pid) = self.pid else {
            return;
        };
        if !reap_monitor_pid(pid) {
            // The check below is the double-panic interlock — the ONLY
            // thing standing between "an assertion failed" and "the whole
            // test binary aborted". A `Drop` that panics while an unwind is
            // already in flight calls `abort()`, which would turn one
            // legible failing assertion into a crashed test binary that
            // reports nothing about any of the other ~694 tests in the
            // process. On that path we still attempted the reap above —
            // only the complaint is downgraded to stderr, since the panic
            // already in flight is the more informative failure.
            if std::thread::panicking() {
                // NOT `eprintln!`: that macro routes through `std::io::_eprint`,
                // which panics ("failed printing to stderr") if the underlying
                // write fails. On this branch a panic is already unwinding, so
                // that second panic would `abort()` — the very outcome the
                // `panicking()` check exists to avoid. Write directly and
                // discard the result: if we cannot even report the leak, the
                // in-flight panic is still the more informative failure.
                use std::io::Write as _;
                let _ = writeln!(
                    std::io::stderr(),
                    "ReapMonitorOnDrop: monitor wrapper pid {pid} still alive after reap \
                     during an unwind — not re-panicking because a panic is already in flight"
                );
            } else {
                panic!(
                    "monitor wrapper pid {pid}, spawned by this test's own launch call, must be \
                     verified dead after reaping — not merely assumed dead"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;
    use std::panic::AssertUnwindSafe;
    use std::process::Command;

    /// Owns a real, deterministically long-lived child process (`sleep
    /// 300`) so these tests have a subject whose liveness is a deterministic
    /// question. WR-05 established that the stubbed agent wrapper's own
    /// exit is a timing accident (the stub exits in under a millisecond and
    /// its trailing `devflow advance` resolves to the test binary, which
    /// rejects the argument shape and exits immediately) — "is it still
    /// alive?" would not reliably discriminate anything against it. It must
    /// be deterministic here, or neither test below proves what it claims
    /// to.
    ///
    /// `Drop` kills then waits, ignoring both results and panicking on no
    /// path: these tests police process leaks and must not leave a zombie
    /// of their own behind.
    struct ChildGuard(std::process::Child);

    impl ChildGuard {
        fn spawn() -> Self {
            Self(
                Command::new("sleep")
                    .arg("300")
                    .spawn()
                    .expect("sleep must be spawnable to run this test"),
            )
        }

        fn pid(&self) -> u32 {
            self.0.id()
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// A `State` whose only relevant field is `monitor_pid`. No I/O occurs —
    /// the project root is never read, since nothing in the reap path
    /// touches disk.
    fn state_holding(pid: u32) -> State {
        let mut state = State::new(
            0,
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/nonexistent"),
        );
        state.monitor_pid = Some(pid);
        state
    }

    /// Proves the guard reaps during a REAL unwind: bind it inside a closure
    /// whose body then fails an assertion, and confirm the subject is dead
    /// after `catch_unwind` returns `Err`.
    ///
    /// `cargo test` captures the resulting panic output and prints it only
    /// for a FAILING test, so a passing run here is quiet — no panic hook is
    /// installed (installing one would be process-global and would swallow
    /// output from unrelated tests running in parallel).
    #[test]
    fn reap_guard_reaps_the_monitor_when_a_later_assertion_panics() {
        let child = ChildGuard::spawn();
        let pid = child.pid();
        let state = state_holding(pid);

        assert!(
            devflow_core::agent::agent_running(pid),
            "precondition: a test whose subject is already dead proves nothing"
        );

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _reap_guard = ReapMonitorOnDrop::after_launch(&state);
            assert_eq!(
                std::hint::black_box(1_u32),
                2,
                "deliberate failing assertion"
            );
        }));

        assert!(result.is_err(), "the closure must have panicked");
        assert!(
            !devflow_core::agent::agent_running(pid),
            "the guard did not reap the monitor during the unwind"
        );
    }

    /// CONTROL: identical setup, but the closure calls the plain trailing
    /// helper AFTER a failing assertion instead of binding the guard. Proves
    /// the pair discriminates rather than being vacuous — a trailing
    /// statement cannot run during an unwind, so the subject here must
    /// still be ALIVE after `catch_unwind` returns `Err`, unlike the test
    /// above.
    ///
    /// The outer `ReapMonitorOnDrop` is this test's OWN unwind-safe cleanup
    /// (bound before the closure so it drops after it, killing the survivor
    /// the control just proved the trailing call could not reach); declaring
    /// `ChildGuard` before it means drop order is closure-locals, then the
    /// outer guard (kills), then `ChildGuard` (waits, clearing the zombie).
    #[test]
    fn trailing_reap_call_is_skipped_when_a_later_assertion_panics() {
        let child = ChildGuard::spawn();
        let pid = child.pid();
        let state = state_holding(pid);

        assert!(
            devflow_core::agent::agent_running(pid),
            "precondition: a test whose subject is already dead proves nothing"
        );

        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            assert_eq!(
                std::hint::black_box(1_u32),
                2,
                "deliberate failing assertion"
            );
            reap_spawned_monitor(&state);
        }));

        assert!(result.is_err(), "the closure must have panicked");
        assert!(
            devflow_core::agent::agent_running(pid),
            "this proves the trailing call form does NOT run during an unwind — the reason the \
             guard test above is not vacuous"
        );
    }
}

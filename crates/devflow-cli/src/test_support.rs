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
/// This mutex currently guards four variables: `PATH`,
/// `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`,
/// `DEVFLOW_GATE_NOTIFY_CMD`. A future author adding a fifth mutated
/// variable to this crate's tests is joining this set — guard it here, not
/// with a new mutex (D-02: per-module mutexes were rejected on measured
/// evidence that `PATH` alone is mutated 36 times across 12 lock regions
/// spanning at least three future target clusters).
///
/// One static suffices for the whole `devflow` binary crate after the
/// `main.rs` split (19c–19f): every D-05 target module stays inside this
/// same binary crate, so `cargo test -p devflow` compiles them all into
/// exactly one test binary regardless of how many modules the split
/// creates — the one-instance-per-process guarantee this mutex depends on
/// is preserved by construction, not by convention.
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Build a real git repo (main + develop, with a Cargo.toml committed) so
/// the terminal-path hooks (`VersionBump`, `BranchCleanup`) exercised below
/// have real git plumbing to operate on rather than an empty directory.
pub(crate) fn init_repo(root: &Path) {
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
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
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
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

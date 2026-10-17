//! Helpers shared by the `devflow` CLI integration tests (999.141, D-07).
//!
//! Each test target includes this file with `mod common;`. It holds helpers
//! only and no `#[test]`: every target that declares `mod common;` compiles
//! it, so a test here would be added to each of those targets' counts.
//!
//! The rule it exists for: an e2e child that runs the `devflow` binary gets a
//! tempdir `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME` and `DEVFLOW_CACHE_DIR`
//! and has the repo-local `GIT_*` variables removed, so its registry entries
//! (and, from 50-11, its sockets) never land in the operator's real
//! `~/.cache/devflow`. Spawn `devflow` through [`hermetic_devflow`] only;
//! `test_home_isolation.rs` scans every suite that declares `mod common;` for
//! a raw spawn. Isolation is set on each child `Command`, never by mutating
//! the test process's own environment.

#![allow(
    dead_code,
    reason = "each test target uses a different subset of the shared helpers"
)]

use devflow_core::phase_id::PhaseId;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// The compiled `devflow` binary under test. The only place in `tests/` that
/// names Cargo's binary path variable once migration completes.
pub fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// A throwaway `HOME` for one or more `devflow` children, removed on drop.
///
/// Created directly under `/tmp` with a short prefix, never under a possibly
/// long `$TMPDIR`, so a socket path below it stays under the 107-byte
/// `sun_path` limit (R-5).
pub struct HermeticHome {
    dir: tempfile::TempDir,
}

impl HermeticHome {
    /// Create a fresh, empty home under `/tmp`.
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .expect("create hermetic HOME under /tmp");
        Self { dir }
    }

    /// The home directory itself (the child's `HOME`).
    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    /// The child's DevFlow cache dir (`home/.cache/devflow`), where its
    /// registry `roots/` entries land.
    pub fn cache_dir(&self) -> PathBuf {
        self.home().join(".cache").join("devflow")
    }

    /// The child's `XDG_CONFIG_HOME` (`home/.config`).
    pub fn config_dir(&self) -> PathBuf {
        self.home().join(".config")
    }
}

impl Default for HermeticHome {
    fn default() -> Self {
        Self::new()
    }
}

/// A `devflow` command whose `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME` and
/// `DEVFLOW_CACHE_DIR` all point inside `home`, with every repo-local and
/// repo-redirecting `GIT_*` variable removed. `home` must outlive the child
/// and any monitor it launches.
pub fn hermetic_devflow(home: &HermeticHome) -> Command {
    let mut command = Command::new(devflow_bin());
    command
        .env("HOME", home.home())
        .env("XDG_CACHE_HOME", home.home().join(".cache"))
        .env("XDG_CONFIG_HOME", home.config_dir())
        .env("DEVFLOW_CACHE_DIR", home.cache_dir());
    for name in devflow_core::test_support::REPO_LOCAL_GIT_VARS
        .iter()
        .chain(devflow_core::test_support::ALSO_REDIRECTING_GIT_VARS)
    {
        command.env_remove(name);
    }
    command
}

/// A directory of stub programs to put first on a child's `PATH`. Removed on
/// drop.
pub struct FakeBin {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
}

/// A fake `claude` whose body is `agent_body`, plus `git` and `sh` links.
pub fn fake_bin_dir(agent_body: &str) -> FakeBin {
    fake_bin_dir_for("claude", agent_body)
}

/// A fake agent named `program` (for example `claude` or `codex`) whose body
/// is `agent_body`, plus `git` and `sh` links.
pub fn fake_bin_dir_for(program: &str, agent_body: &str) -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let agent = dir.path().join(program);
    fs::write(&agent, format!("#!/bin/sh\n{agent_body}\n")).unwrap();
    let mut permissions = fs::metadata(&agent).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&agent, permissions).unwrap();
    for linked in ["git", "sh"] {
        symlink(format!("/usr/bin/{linked}"), dir.path().join(linked)).unwrap();
    }
    FakeBin {
        path: dir.path().to_path_buf(),
        _dir: dir,
    }
}

/// Poll `predicate` every 25ms until it is true, panicking with `what` once
/// `timeout` elapses.
pub fn wait_for(mut predicate: impl FnMut() -> bool, timeout: Duration, what: &str) {
    let started = Instant::now();
    while !predicate() {
        assert!(
            started.elapsed() < timeout,
            "timed out after {timeout:?} waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Kill `child` if it is still running, then wait on it.
pub fn kill_and_reap(child: &mut Child) {
    if child.try_wait().expect("poll child").is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

/// How long [`LaunchedRunReaper`] waits for a supervisor's final shutdown line.
const REAP_SHUTDOWN_WAIT: Duration = Duration::from_secs(15);

/// Ends, on drop, the run recorded in the phase state, through its
/// supervisor's socket (D-07): a `shutdown` request first; then, if the
/// recorded agent group's leader is still the process the record names
/// (`agent::is_same_process` against its start time), a SIGKILL to that
/// group. Only when the state holds no `supervisor` record does it fall back
/// to the agent named in the agent-pid file. A pid in `spare` is never
/// signalled. Bind it before (or right after) the spawn that launches a run,
/// and keep the run's [`HermeticHome`] alive until this has dropped.
pub struct LaunchedRunReaper {
    root: PathBuf,
    phase: PhaseId,
    spare: Vec<u32>,
}

impl LaunchedRunReaper {
    pub fn new(root: &Path, phase: PhaseId, spare: Vec<u32>) -> Self {
        Self {
            root: root.to_path_buf(),
            phase,
            spare,
        }
    }
}

impl Drop for LaunchedRunReaper {
    fn drop(&mut self) {
        let record = devflow_core::workflow::load_state(&self.root, self.phase)
            .ok()
            .and_then(|state| state.supervisor);
        if let Some(record) = record {
            let _ =
                devflow_core::supervisor::request_shutdown(&record.socket_path, REAP_SHUTDOWN_WAIT);
            let leader = record.agent_pgid;
            if !self.spare.contains(&leader)
                && devflow_core::agent::agent_running(leader)
                && devflow_core::agent::is_same_process(leader, record.agent_start_time)
            {
                signal_group(leader, libc::SIGKILL);
            }
            return;
        }
        let agent = fs::read_to_string(devflow_core::agent_result::agent_pid_path(
            &self.root, self.phase,
        ))
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok());
        if let Some(pid) = agent
            && !self.spare.contains(&pid)
            && devflow_core::agent::agent_running(pid)
        {
            let _ = devflow_core::agent::terminate_and_verify(
                pid,
                devflow_core::agent::TERMINATE_VERIFY_WAIT,
                devflow_core::agent::TERMINATE_VERIFY_POLL,
            );
        }
    }
}

/// Send `signal` to process group `pgid`. Refuses pgid 0 or 1, a value that
/// does not fit `pid_t`, and this process's own group.
pub fn signal_group(pgid: u32, signal: libc::c_int) {
    let Ok(signed) = libc::pid_t::try_from(pgid) else {
        return;
    };
    // SAFETY: getpgrp has no preconditions and cannot fail.
    if signed > 1 && signed != unsafe { libc::getpgrp() } {
        // SAFETY: `signed > 1` and not our own group: one process group
        // whose leader the caller has just identity-checked.
        unsafe { libc::kill(-signed, signal) };
    }
}

/// Every file below `dir`, recursively, sorted. Empty when `dir` is missing.
pub fn entries_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(dir, &mut files);
    files.sort();
    files
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        // `file_type` does not follow symlinks, so a link loop cannot recurse.
        if entry.file_type().expect("read entry type").is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

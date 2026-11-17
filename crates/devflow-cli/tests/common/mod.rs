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
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// The service-manager tools DevFlow runs by name, each faked in
/// [`HermeticHome::service_bin`].
pub const SERVICE_TOOLS: [&str; 3] = ["systemctl", "systemd-run", "loginctl"];

/// What the fake `systemctl --user show -p Version --value` prints.
pub const FAKE_SYSTEMD_VERSION: &str = "256-fake";

/// What every fake service tool prints, followed by its name and
/// `: unhandled: <args>`, on a call it does not handle. Also the marker that
/// tells [`HermeticPath::hermetic_path`] a directory holds the fakes.
pub const FAKE_TOOL_PREFIX: &str = "fake ";

/// The fake `systemctl`. It handles `--user show -p Version --value`,
/// `--user show -p ActiveState --value U` (`U.fail` in its own dir: exit 1
/// with a bus error; `U.deactivating`: deactivating; otherwise inactive) and
/// `--user reset-failed U`.
const FAKE_SYSTEMCTL_CALLS: &str = r#"if [ "$#" -eq 5 ] && [ "$1 $2 $3 $4 $5" = "--user show -p Version --value" ]; then
  echo "@VERSION@"; exit 0
fi
if [ "$#" -eq 6 ] && [ "$1 $2 $3 $4 $5" = "--user show -p ActiveState --value" ]; then
  if [ -e "$dir/$6.fail" ]; then
    echo "Failed to get properties: Transport endpoint is not connected" >&2; exit 1
  fi
  if [ -e "$dir/$6.deactivating" ]; then echo deactivating; else echo inactive; fi
  exit 0
fi
if [ "$#" -eq 3 ] && [ "$1 $2" = "--user reset-failed" ]; then exit 0; fi
"#;

/// Write the fake `tool` into `dir`: `calls` (shell lines that exit on each
/// call they handle, with `$dir` naming `dir`), then exit 1 naming the call.
fn write_fake_service_tool(dir: &Path, tool: &str, calls: &str) {
    let dir_text = dir.to_str().expect("a UTF-8 service-bin path");
    assert!(
        !dir_text.contains('\''),
        "the service-bin path is single-quoted in the fakes: {dir_text}"
    );
    let script = format!(
        "#!/bin/sh\ndir='{dir_text}'\n{calls}echo \"{FAKE_TOOL_PREFIX}{tool}: unhandled: $*\" >&2\nexit 1\n"
    );
    let path = dir.join(tool);
    fs::write(&path, script).expect("write a fake service tool");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("make a fake service tool executable");
}

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
    /// Create a fresh home under `/tmp`, holding only the fake service
    /// manager in [`Self::service_bin`].
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .expect("create hermetic HOME under /tmp");
        let home = Self { dir };
        let service_bin = home.service_bin();
        fs::create_dir(&service_bin).expect("create the fake service-manager dir");
        let systemctl_calls = FAKE_SYSTEMCTL_CALLS.replace("@VERSION@", FAKE_SYSTEMD_VERSION);
        for tool in SERVICE_TOOLS {
            let calls = if tool == "systemctl" {
                systemctl_calls.as_str()
            } else {
                ""
            };
            write_fake_service_tool(&service_bin, tool, calls);
        }
        home
    }

    /// The home directory itself (the child's `HOME`).
    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    /// The dir holding the fake `systemctl`, `systemd-run` and `loginctl`,
    /// first on the `PATH` of every [`hermetic_devflow`] child. A marker file
    /// `<unit>.fail` or `<unit>.deactivating` here sets what the fake
    /// `systemctl` reports for that unit's `ActiveState`.
    pub fn service_bin(&self) -> PathBuf {
        self.home().join("service-bin")
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
/// repo-redirecting `GIT_*` variable removed, and whose `PATH` is
/// [`HermeticHome::service_bin`] followed by this process's `PATH`, so the
/// child never reaches the real user manager (D-07b). Add `PATH` entries with
/// [`HermeticPath::hermetic_path`], never `.env("PATH", ...)`. `home` must
/// outlive the child and any monitor it launches.
pub fn hermetic_devflow(home: &HermeticHome) -> Command {
    let mut command = scrubbed_devflow(home);
    with_fake_service_manager(&mut command, home);
    command
}

/// [`hermetic_devflow`] without the fake service manager: `PATH`,
/// `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` stay this process's, so
/// the child reaches the real user manager. Only `service_e2e.rs` may use it,
/// because that suite runs under `scripts/check.sh service` against a real
/// manager; every other suite runs where there is none.
pub fn host_service_devflow(home: &HermeticHome) -> Command {
    scrubbed_devflow(home)
}

/// Put `home`'s fake service manager first on `command`'s `PATH`, followed by
/// this process's `PATH`. Touches no other variable, so the R-13 plain-Command
/// control keeps its own `HOME` and cache variables.
pub fn with_fake_service_manager(command: &mut Command, home: &HermeticHome) {
    let parent = std::env::var_os("PATH").unwrap_or_default();
    command.env("PATH", path_after(&home.service_bin(), &parent));
}

/// `first`, then `rest` when it is not empty, joined with `:`.
fn path_after(first: &Path, rest: &OsStr) -> OsString {
    let mut path = first.as_os_str().to_os_string();
    if !rest.is_empty() {
        path.push(":");
        path.push(rest);
    }
    path
}

/// Setting a child's `PATH` without dropping the fake service manager.
pub trait HermeticPath {
    /// Set `PATH` to the fake service-manager dir this command already has
    /// first on its `PATH`, followed by `path`. Panics when the command's
    /// `PATH` does not start with that dir, that is, when
    /// [`hermetic_devflow`] (or [`with_fake_service_manager`]) did not build
    /// it.
    fn hermetic_path(&mut self, path: impl AsRef<OsStr>) -> &mut Command;
}

impl HermeticPath for Command {
    fn hermetic_path(&mut self, path: impl AsRef<OsStr>) -> &mut Command {
        let service_bin = fake_service_dir_first_on_path(self).unwrap_or_else(|| {
            panic!(
                "hermetic_path needs a command built by common::hermetic_devflow: its PATH \
                 does not start with the fake service-manager dir"
            )
        });
        self.env("PATH", path_after(&service_bin, path.as_ref()))
    }
}

/// The first entry of the `PATH` that `command` sets, when it is a dir
/// holding a fake `systemctl`.
fn fake_service_dir_first_on_path(command: &Command) -> Option<PathBuf> {
    let (_, value) = command
        .get_envs()
        .find(|(key, _)| *key == OsStr::new("PATH"))?;
    let first = std::env::split_paths(value?).next()?;
    let systemctl = fs::read_to_string(first.join("systemctl")).ok()?;
    systemctl
        .contains(&format!("{FAKE_TOOL_PREFIX}systemctl: unhandled"))
        .then_some(first)
}

/// [`hermetic_devflow`]'s `HOME`, XDG, cache and `GIT_*` isolation, with the
/// caller's `PATH`.
fn scrubbed_devflow(home: &HermeticHome) -> Command {
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

/// The file under a capture agent's `logs` dir that holds its argv, one
/// argument per line.
pub const AGENT_ARGV_LOG: &str = "agent-argv";

/// The file under a capture agent's `logs` dir that holds its stdin.
pub const AGENT_STDIN_LOG: &str = "agent-stdin";

/// The codex `--json` event lines a finished turn prints: the stream DevFlow's
/// codex driver parses, whose final `agent_message` carries a `DEVFLOW_RESULT`
/// success line.
pub const CODEX_SUCCESS_EVENTS: &str = r#"printf '%s\n' '{"type":"thread.started","thread_id":"t-fake"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"work done\nDEVFLOW_RESULT: {\"status\":\"success\",\"commits\":1}"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'"#;

/// A fake capture-shape agent: `codex`, which gets its prompt as argv. It
/// answers `--version`, appends its argv (one argument per line) to
/// `logs/`[`AGENT_ARGV_LOG`] and its stdin to `logs/`[`AGENT_STDIN_LOG`],
/// commits one empty commit as its "work", prints a successful codex turn
/// ([`CODEX_SUCCESS_EVENTS`]) and exits 0.
pub fn capture_agent_bin(logs: &Path) -> FakeBin {
    capture_agent_bin_then(
        logs,
        &format!(
            "git commit -q --allow-empty -m \"fake stage work\"\n{CODEX_SUCCESS_EVENTS}\nexit 0"
        ),
    )
}

/// [`capture_agent_bin`] that, after logging, stays running until it is
/// ended instead of finishing its turn.
pub fn waiting_capture_agent_bin(logs: &Path) -> FakeBin {
    capture_agent_bin_then(logs, "sleep 120")
}

/// The fake `codex` behind [`capture_agent_bin`]: answer `--version`, log
/// argv and stdin under `logs`, then run the shell lines `tail`.
pub fn capture_agent_bin_then(logs: &Path, tail: &str) -> FakeBin {
    let logs = logs.to_str().expect("a UTF-8 logs path");
    assert!(
        !logs.contains('\''),
        "the logs path is single-quoted in the stub: {logs}"
    );
    fake_bin_dir_for(
        "codex",
        &format!(
            "if [ \"$1\" = \"--version\" ]; then echo \"codex-cli 0.0.0-fake\"; exit 0; fi\n\
             for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done >> '{logs}/{AGENT_ARGV_LOG}'\n\
             cat >> '{logs}/{AGENT_STDIN_LOG}'\n\
             {tail}"
        ),
    )
}

/// Plant a `gsd_run` stub at `<home>/.claude/gsd-core/bin/gsd_run`, so a run
/// under `home` reads GSD's verification status from `responses` (one reply
/// per `query verification status` call, in order) and never from a real GSD
/// install. Each call's working directory and argv land in `logs`; read them
/// with [`gsd_run_calls`]. See `devflow_core::test_support::plant_gsd_run_stub`
/// for what the stub answers.
pub fn stub_gsd_run(home: &HermeticHome, responses: &[&str], logs: &Path) {
    devflow_core::test_support::plant_gsd_run_stub(home.home(), responses, logs);
}

/// The calls a [`stub_gsd_run`] stub logged under `logs`, as
/// `(working directory, argv)`, in call order.
pub fn gsd_run_calls(logs: &Path) -> Vec<(PathBuf, String)> {
    devflow_core::test_support::gsd_run_calls(logs)
}

/// Write a CONTEXT.md for `phase` under `root` and return its path relative to
/// `root`, for the caller to commit. `start` refuses to launch codex for a
/// phase with no CONTEXT.md on the base branch.
pub fn write_phase_context(root: &Path, phase: PhaseId) -> String {
    let padded = phase.padded();
    let relative = format!(".planning/phases/{padded}-fixture/{padded}-CONTEXT.md");
    let path = root.join(&relative);
    fs::create_dir_all(path.parent().expect("a phase dir")).unwrap();
    fs::write(&path, "# Context\n").unwrap();
    relative
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

/// How long [`LaunchedRunReaper`] waits for the phase's supervisors to exit on
/// their own before it ends the ones still alive.
const REAP_EXIT_WAIT: Duration = Duration::from_secs(15);

/// Ends, on drop, the run recorded in the phase state, through its
/// supervisor's socket (D-07): a `shutdown` request first; then, if the
/// recorded agent group's leader is still the process the record names
/// (`agent::is_same_process` against its start time), a SIGKILL to that
/// group. Only when the state holds no `supervisor` record does it fall back
/// to the agent named in the agent-pid file. A pid in `spare` is never
/// signalled. Bind it before (or right after) the spawn that launches a run,
/// and keep the run's [`HermeticHome`] alive until this has dropped.
///
/// It then waits, bounded by [`REAP_EXIT_WAIT`], until no `devflow
/// __monitor` whose argv names this root and phase is alive, and ends any
/// still alive after that wait. The state cannot name every one: a stop that
/// lands while an advance is launching the next stage leaves that next
/// supervisor spawned but never recorded, with the stop flagged. It records
/// its own stop under the state write lock a moment later, and taking that lock
/// recreates `<root>/.devflow/` when the root's `TempDir` has already been
/// removed, leaving `/tmp/.tmpXXXXXX/.devflow/.state-NN.lock` behind.
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

    /// Every live `devflow __monitor` whose argv names this root (as given or
    /// canonicalized) after `--project` and this phase after `--phase`, with
    /// its start time: each supervisor this run started, recorded or not.
    fn live_supervisors(&self) -> Vec<(u32, u64)> {
        use std::os::unix::ffi::OsStrExt as _;
        let canonical = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let roots = [
            self.root.as_os_str().as_bytes(),
            canonical.as_os_str().as_bytes(),
        ];
        let Ok(entries) = fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
                let cmdline = fs::read(entry.path().join("cmdline")).ok()?;
                let args: Vec<&[u8]> = cmdline.split(|byte| *byte == 0).collect();
                let value_after = |flag: &str| {
                    args.windows(2)
                        .find(|pair| pair[0] == flag.as_bytes())
                        .map(|pair| pair[1])
                };
                let phase = value_after("--phase")
                    .and_then(|value| std::str::from_utf8(value).ok())
                    .and_then(|value| value.parse::<PhaseId>().ok());
                let ours = args.contains(&b"__monitor".as_slice())
                    && value_after("--project").is_some_and(|value| roots.contains(&value))
                    && phase == Some(self.phase);
                if !ours {
                    return None;
                }
                let start = devflow_core::agent::process_start_time(pid)?;
                devflow_core::agent::agent_running(pid).then_some((pid, start))
            })
            .collect()
    }

    /// Whether every supervisor this run started has exited on its own within
    /// `within`. Ends nothing: a run that is still live reads `false`, so a
    /// caller can count a run's events only once nothing can add to them.
    pub fn supervisors_exited_within(&self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            if self.live_supervisors().is_empty() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait for this phase's supervisors to exit, then end any left, so none
    /// of them writes under the root once this returns.
    fn wait_for_supervisors_to_exit(&self) {
        let deadline = Instant::now() + REAP_EXIT_WAIT;
        while Instant::now() < deadline {
            if self.live_supervisors().is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        for (pid, start) in self.live_supervisors() {
            if !self.spare.contains(&pid) && devflow_core::agent::is_same_process(pid, start) {
                let _ = devflow_core::agent::terminate_and_verify(
                    pid,
                    devflow_core::agent::TERMINATE_VERIFY_WAIT,
                    devflow_core::agent::TERMINATE_VERIFY_POLL,
                );
            }
        }
    }

    /// The shutdown described on [`LaunchedRunReaper`]: the recorded
    /// supervisor, else the agent in the agent-pid file.
    fn end_recorded_run(&self) {
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

impl Drop for LaunchedRunReaper {
    fn drop(&mut self) {
        self.end_recorded_run();
        self.wait_for_supervisors_to_exit();
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

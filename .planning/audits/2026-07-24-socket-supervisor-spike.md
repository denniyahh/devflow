# Spike: Socket-Addressable Supervisor — Results

Date: 2026-07-24
Spike code: `<scratchpad>/sockspike/` (throwaway; ~200 lines, `std` + `libc` only)
Companion to: `2026-07-24-process-lifecycle-problem-definition.md` (F1–F7),
`2026-07-24-process-teardown-solution-research.md`

**Verdict: mechanism proven. All six claims hold. One real constraint
discovered that changes the design — the socket cannot live in the project
tree.**

---

## Why this shape

The prior design treated a **PID** as the durable handle, which failed because
a PID is neither stable (reuse) nor representative of a tree (leader ≠ group).

The insight this spike tests: **DevFlow already has a supervisor.** The `sh -c`
monitor already owns the agent and — verified earlier — has no `exec`, so it
lives for the tree's entire lifetime. The problem was never a missing
supervisor; it was that the supervisor is **unaddressable** except by PID.

So: keep the supervisor, give it an address. The handle becomes a **socket
path** — a string, with no PID anywhere in it.

## Results

| Claim | Result |
|---|---|
| **C1** Socket path is a durable handle (string, no PID) | ✅ |
| **C2** Liveness via `connect()`; STALE distinguishable from ALIVE | ✅ **the key result** |
| **C3** Teardown kills the whole tree, incl. severed-ppid orphans | ✅ |
| **C4** Monitor survives its spawner exiting | ✅ |
| **C5** pgid backstop works when the monitor is SIGKILLed | ✅ |
| **C6** Socket path length limits | ⚠️ **real constraint — see below** |
| Takeover safety: a 2nd monitor must not steal a live socket | ✅ refused, exit 3 |

### C2 — the result that matters

Three states are cleanly distinguishable **with no PID involved**:

| Condition | Probe result |
|---|---|
| No socket file | `GONE` — never started, or cleanly torn down |
| File exists, nobody listening | `STALE` — `ECONNREFUSED` (errno 111) |
| Connected | `ALIVE` |

**Contrast with the abandoned design (F1):** when the monitor died there, the
liveness check (`agent_running(monitor_pid)`) returned false, and teardown was
**skipped entirely** while the agent kept running — `devflow stop` printed
*"no live process found"* about a live process. Here the identical scenario
reports `STALE`, which is *actionable*, and the backstop can then run.

This also removes the F2 failure by construction: there is no "poll the leader
for death" step to get wrong, because liveness is not leader-based.

### C3 — verified against the hard case

The test agent double-forks an orphan whose ppid chain is severed:

```
orphan ppid=1965   (reparented — a ppid-walk would MISS it)
orphan pgid=3623847 (== agent's pgid — group-reachable)
```

After `stop` over the socket: **monitor DEAD, agent DEAD, orphan DEAD**, socket
file removed, subsequent probe returns `GONE`. The mechanism is
socket-for-addressing + `killpg` for the actual tree kill, with the monitor
holding the real `Child` handle so there is no PID-reuse hazard on its own
child.

### C5 — backstop for the SIGKILL case

SIGKILL the monitor (no cleanup possible):

```
socket file left behind? YES-STALE
agent still alive?       YES-ORPHANED
orphan still alive?      YES-ORPHANED
probe:                   STALE (ECONNREFUSED)
```

Recovery: enumerate group members by pgid, then `killpg`:

```
group members by pgid 3625862:
  pid=3625862 pgid=3625862 cmd=sleep
  pid=3625866 pgid=3625862 cmd=sleep
→ kill -TERM -3625862 → agent DEAD, orphan DEAD, 0 members remain
```

This is safe because **POSIX forbids PGID reuse while the group is non-empty** —
and enumerating members *is* the non-emptiness check. So the ordering is:
probe socket → if STALE, enumerate by pgid → if non-empty, `killpg`.

### C6 — the constraint that changes the design ⚠️

`sun_path` is a fixed-size buffer: **108 bytes on Linux, 104 on macOS.**
Measured on this host by bisecting on `bind()`:

```
longest bindable socket path = 107 bytes (first failure at 108)
```

Project-relative socket paths do not have enough headroom:

| Path | Length | macOS headroom |
|---|---|---|
| `<repo>/.devflow/monitor-22.sock` | 59 | 45 |
| `<repo>/.worktrees/phase-22/.devflow/monitor-22.sock` | 79 | **25** |
| `/Users/jonathan/Documents/Projects/work/acme-corp/backend-services/.worktrees/phase-22/.devflow/monitor-22.sock` | **111** | **FAILS** |

That last one is an entirely ordinary macOS project path. **A socket inside the
project tree is not viable.**

**Mitigation (verified):** put sockets in a short runtime dir, keyed by a hash
of project root + phase:

```
/run/user/1000/devflow/ab12cd34-22.sock   len=39, macOS headroom=65
```

`$XDG_RUNTIME_DIR` on Linux (confirmed `/run/user/1000` here), `$TMPDIR` on
macOS. The project-root hash keeps concurrent projects from colliding.

**Consequence for state:** the socket path must be *stored* in `state.json`,
not *derived* from the project path at probe time — otherwise a changed
`$XDG_RUNTIME_DIR`/`$TMPDIR` between invocations orphans the handle.

**⚠️ Open decision — `$XDG_RUNTIME_DIR` is deleted on logout.** systemd removes
`/run/user/$UID` when the last session for that user ends. DevFlow's whole
premise is long unattended runs, so an operator logging out mid-run would
destroy the socket while the monitor kept running — unreachable, with only the
pgid backstop left. `~/.cache/devflow/` is ~45 bytes on macOS (still far under
104) and survives logout. **Recommend `~/.cache/devflow/` over
`$XDG_RUNTIME_DIR`** unless there's a reason to prefer tmpfs.

---

# Part 2 — Replacement parity

Second spike round: can this actually *replace* the production `sh -c` monitor?
Tested every responsibility of the current script, in a realistic layout
(project root + `.worktrees/phase-NN` + runtime-dir socket).

| Req | Production responsibility | Result |
|---|---|---|
| **R-A** | Agent runs in the **worktree** cwd | ✅ `CWD=/tmp/pr/.worktrees/phase-22` |
| **R-B** | stdout/stderr to **separate** files; stderr must never corrupt JSON stdout | ✅ 0 stderr lines in stdout, 1 in stderr; `DEVFLOW_RESULT` parseable on its own line |
| **R-C** | Agent pid recorded to pidfile | ✅ |
| **R-D** | Accurate exit code recorded | ✅ agent `exit 7` → exitfile `7` (signal deaths map to 128+n) |
| **R-E** | `devflow advance` tail runs on natural completion | ✅ |
| **R-F** | Adapter env (Codex `GIT_CONFIG_*`) reaches agent **and its children** | ✅ `GRANDCHILD_SEES=false` — propagated two levels |
| **R-G** | Detached; survives the spawning CLI exiting | ✅ (Part 1, C4) |
| **R-J** | Project-wide sweep with **no state file** | ✅ lists + probes `rt/*.sock` |
| **R-K** | Teardown still records an exit code (no silent outcome) | ✅ `143` (SIGTERM) |
| **R-L** | Socket not world-accessible | ✅ mode `0600` |
| **R-M** | **A stop is not a completion** — `advance` must be suppressed | ✅ advance marker absent after stop |
| — | Concurrency (`devflow parallel`): 3 monitors, selective stop | ✅ stopping phase 8 left 7 and 9 untouched |
| — | Phase 22 incident replay: SIGKILL monitor by hand, no state file | ✅ sweep reports `STALE`; pgid backstop reaped 2 group members |

### The architectural win found in round 2

**The `advance` tail should run *in-process*, not as a forked child.**

The production script ends with `; devflow advance --phase N` — a *separate
forked process*. That process is the thing the original bug orphaned (the trap
only ever tracked `$apid`, so a signal arriving during the tail had nothing to
kill). Because the monitor is now `devflow` itself, it can simply call the
advance logic directly and then exit.

**This removes the original failure mode by construction: there is no tail
process left to orphan.** The stage-N monitor calls advance, advance spawns the
stage-N+1 monitor, stage-N's monitor exits. Cleaner than today's shape.

### R-M is a genuine behavioural improvement, not just parity

Today, killing a monitor mid-run leaves the advance tail's fate ambiguous
(orphaned and still running, in the bug case). The socket design makes the
distinction explicit and enforceable:

- agent exits on its own → record exit code → **run advance** → exit
- operator issues `stop` → kill group → record exit code → **suppress
  advance** → exit

A stopped phase must not advance its own state machine. That was never
expressible in the shell script.

## Resulting design

```jsonc
{
  "supervisor": {
    "socket_path": "/run/user/1000/devflow/ab12cd34-22.sock",  // durable handle
    "agent_pgid":  3625862,   // backstop only, used when socket is STALE
    "agent_start_time": 26234043,  // validates pgid before any killpg
    "boot_id": "…"                 // invalidates the whole handle across reboots
  }
}
```

- **Liveness:** connect to `socket_path`. `GONE` / `STALE` / `ALIVE`.
- **Teardown, happy path:** send `shutdown`; the monitor `killpg`s its own
  group (it holds the real handle), reaps, removes the socket, exits.
- **Teardown, STALE path:** enumerate by `agent_pgid`; if non-empty *and*
  `start_time`/`boot_id` validate, `killpg` with SIGTERM → grace → SIGKILL.
- **Takeover safety:** a monitor that finds an existing socket must probe it
  first and refuse to start if it is live (proven, exit 3). Only a `STALE`
  socket may be reclaimed.

### Coverage against the known failure modes

| | Handled by |
|---|---|
| F1 leader dead / members alive | `STALE` is distinguishable and actionable; backstop enumerates the group |
| F2 escalation never fires | No leader-liveness poll exists; the monitor owns escalation with a real handle |
| F3 PID reuse | Happy path uses no PID at all. Backstop is guarded by POSIX non-reuse + `start_time` + `boot_id` |
| F4 Ctrl-C regression | Moot if `sequentagent` is dropped; otherwise the supervisor is addressable regardless of process group |
| F5 stranded lock | The monitor releases it on clean shutdown; stale-lock reclaim already exists |
| F6 zombies read as alive | Liveness is socket-based, not `kill(pid,0)`; the monitor `try_wait`s its real child |
| F7 no state ⇒ no teardown | Scan the runtime dir for `devflow/*.sock` — a project-wide sweep with no state file needed |

**F7 is worth calling out:** it becomes trivially solvable and *portable*,
where the cgroup design could only solve it on Linux. A sweep is just a
directory listing plus a probe per socket.

## What this buys over the layered cgroup/pgroup design

- **One implementation, all platforms.** Unix sockets behave identically on
  Linux, macOS, and inside containers. No cgroup-vs-pgroup branch, no systemd
  dependency, no delegation probing, no container degradation.
- **No new dependency.** `std::os::unix::net` + the existing `libc`. No
  `sysinfo` needed on the happy path (only for backstop `start_time`
  validation).
- **No daemon.** One supervisor per phase, spawned by the CLI as today, exits
  when the phase ends. No bootstrap, no upgrade skew, no idle timeout, no
  system-wide state.

## Known gaps / not yet proven

Resolved in round 2: capture/exit-code/advance parity, concurrency, socket
permissions. Still open:

- **macOS is unverified.** No macOS host, no macOS CI. The 104-byte `sun_path`
  limit is documented, not measured. Everything used here is POSIX (Unix
  sockets, `killpg`, `process_group`, `current_dir`), so it *should* port
  unchanged — but that is an inference, and macOS support should not be
  claimed until it is run there. **This is now the single largest unknown.**
- **The monitor's own signal handling.** The spike installs no handler, so a
  SIGTERM to the monitor kills it without cleanup → stale socket. That degrades
  correctly (sweep finds it, backstop reaps the group), but production should
  trap SIGTERM/SIGINT and perform the same clean shutdown as the socket
  `shutdown` command.
- **`wait_for_agent_exit` semantics for `sequentagent`.** The exit file is
  still written, so the existing blocking-poll consumer should work unchanged —
  but this was not exercised. Moot if `sequentagent` is dropped.
- **Large / streaming output.** Captures go straight to files via
  `Stdio::from(File)` — the same mechanism as the shell redirect, so no
  buffering regression is expected. Not stress-tested.
- **Rewrite scope.** The spike proves the mechanism, not the migration. Every
  consumer of `spawn_monitor`/`wait_for_agent_pid`/`wait_for_agent_exit` and
  the `.devflow/phase-NN-*` artifacts (10+ files) must keep working.

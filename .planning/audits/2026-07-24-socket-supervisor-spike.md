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

- **macOS is unverified.** No macOS host, no macOS CI. The 104-byte `sun_path`
  limit is documented, not measured here. Everything else *should* be
  identical (Unix sockets, `killpg`, `process_group` are all POSIX) but this
  needs a real run before macOS support is claimed rather than implied.
- **The real monitor is more than the spike.** The spike's supervisor does
  capture-free `spawn → serve → kill`. The production one must also do stdout/
  stderr capture to files, exit-code recording, and the `devflow advance` tail
  call — i.e. everything the current shell script does, rewritten in Rust.
  That is the bulk of the actual work and is not de-risked by this spike.
- **Concurrency under `devflow parallel`** (N monitors, N sockets) is not
  exercised here.
- **Socket permissions.** Not tested; should be user-only (0600) since any
  local process that can connect can issue `shutdown`.

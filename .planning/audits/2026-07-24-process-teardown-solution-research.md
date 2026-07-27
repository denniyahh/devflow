# Process Teardown — Solution Research & Recommended Design

Date: 2026-07-24
Companion to: `2026-07-24-process-lifecycle-problem-definition.md` (requirements
R1–R5, failure modes F1–F7)
Method: two independent research passes (Rust-crate survey; prior-art survey of
established process managers), plus direct empirical verification on this host.

**Headline: no library solves this. But the problem is solved — by `runc`,
doing precisely our problem, with a pattern that is well-attested across
systemd, psutil, and SPIRE. We should copy it.**

---

## 1. The ecosystem gap is real, and it has a precise shape

> Every process-management crate in Rust assumes **the killer is the spawner.**

That single sentence explains all the dead ends. `command-group`,
`process-wrap`, `processkit`, `duct` — all return an in-memory handle
(`GroupChild`, `ChildWrapper`, `ProcessGroup`) from the spawn call. Our
requirement R3 (a *later, unrelated* process kills the tree using only on-disk
state) is outside every one of their designs.

### Corrections to earlier notes in this repo

Two things previously recorded here were wrong; both were only settled by
building/verifying:

1. **`command-group` is superseded by [`process-wrap`](https://crates.io/crates/process-wrap)**
   (same watchexec org, 4.07M downloads/90d vs command-group's 756K; last
   command-group release 2023-11-18). The conclusion is unchanged — both are
   spawn-centric — but `process-wrap` is the crate to cite, and it has a sync
   `std` frontend with tokio behind an optional feature.

2. **A pidfd CAN be persisted to disk on modern Linux.** The earlier note said
   it could not. On kernels with pidfs file-handle support (~6.14+),
   `name_to_handle_at(pidfd, "", …, AT_EMPTY_PATH)` yields an **8-byte handle**
   that is JSON-serializable, and any later unprivileged process can call
   `open_by_handle_at(FD_PIDFS_ROOT, …)` to get a working pidfd back —
   returning `ESTALE` exactly when the process is gone. **Verified empirically
   on this host** (unprivileged, `CapEff: 0`). It is still Linux-only, needs a
   very new kernel, and **cannot kill a tree** — so it is an opportunistic
   liveness upgrade, not a foundation.

### Verdicts on the candidates

| Crate / approach | Verdict | Why |
|---|---|---|
| `process-wrap` / `command-group` | **SPAWN-ONLY** | Handle can't be serialized and reconstituted |
| `processkit` | **ARCHITECTURALLY INCAPABLE** | Source read: only `new()`/`with_options()`, **no path/id accessor**, and the cgroup dir name is *salted with a per-process value* specifically so it is not reconstructible. `adopt()` takes a live tokio `Child`. Also 9,683 total downloads with total == 90-day (i.e. ~8 weeks old), tokio hard dep. Not "immature" — wrong shape. |
| `kill_tree` | **ACTIVELY DANGEROUS** | Walks the **ppid chain** (`/proc/N/status` PPid; macOS `pbi_ppid`); `rg start_time src/` returns nothing. Orphans reparent to pid 1, **severing the chain** — which is exactly our case. Zero PID-reuse protection. Unmaintained since 2024-02. |
| `cgroups-rs` | **VIABLE (Linux) but heavy** | `Cgroup::load(hier, path)` genuinely reconstitutes from a path string — a real durable handle. But carries an **unconditional `zbus` 5.8 dependency** (128 crates incl. `async-io`, `async-executor`) for four file operations in a sync CLI. |
| `sysinfo` | **VIABLE (identity layer)** | Only crate exposing process start time on **both** Linux and macOS behind one sync API. 39M downloads/90d. Caveat: **1-second resolution on both platforms.** |
| `procfs-core` (Linux) / `libproc` (macOS) | **VIABLE (high-res identity)** | Raw `Stat.starttime` (jiffies, ~10ms) / `pbi_start_tvsec`+`pbi_start_tvusec` (µs) if 1s proves too coarse. |
| bare `pidfd_open` fd | **DEAD END** | An fd is per-process kernel-table state; dies with the holder (violates R5). Only the *file-handle* form persists. |
| `kqueue`/`EVFILT_PROC` (macOS) | **DEAD END** | Requires a live watcher process. Violates R3 and R5 by construction. |
| `PR_SET_CHILD_SUBREAPER` | **DEAD END** | Requires a live reaper; Linux-only. |
| `pkill -P` / ppid tree walking | **UNSOUND** | Racy, misses `setsid`'d descendants, each PID reuse-vulnerable. |

---

## 2. What everyone else actually does

**Every daemon-based supervisor requires a long-lived resident supervisor, and
that IS the industry-standard answer.** supervisord, pm2, circus, god, runit,
s6, daemontools — none reconstruct control from disk. They sidestep R3 by never
losing the parent-child relationship.

supervisord is explicit about the limit: *"If a process created by supervisord
creates its own child processes, supervisord cannot kill them"* and
*"supervisord can only kill a process which it creates itself."* Its actual
mechanism is `os.setpgrp()` in the child then `kill(-pid, sig)` — i.e. the same
process-group primitive we already tried.

Dev-workflow runners (foreman, hivemind, overmind, process-compose) are
foreground-only or re-derive "resident daemon + Unix socket." **overmind** is
the most interesting: it uses tmux panes *and* a resident master reached over
`./.overmind.sock` — the socket is the durable handle, tmux is just the
container.

**Implication for DevFlow: the established answer to our problem is "write a
daemon."** That directly violates R1 and merely relocates R5 to "who supervises
the supervisor." Worth naming explicitly as the road not taken.

### The one tool solving our exact problem without a daemon: `runc`

`runc` reconstructs control of a detached container tree from
`/run/runc/<id>/state.json`. Its mechanism, verbatim from
`libcontainer/container.go`:

```go
// InitProcessStartTime is the init process start time in clock cycles since boot time.
InitProcessStartTime uint64 `json:"init_process_start"`
```

```go
func (c *Container) signalInit(s os.Signal) error {
    // To avoid a PID reuse attack, don't kill non-running container.
    if !c.hasInit() { return ErrNotRunning }
...
func (c *Container) hasInit() bool {
    pid := c.initProcess.pid()
    stat, err := system.Stat(pid)
    if err != nil { return false }
    if stat.StartTime != c.initProcessStartTime || stat.State == system.Zombie || stat.State == system.Dead {
        return false
    }
    return true
}
```

Note it treats **Zombie and Dead as not-running** — which independently fixes
our failure mode F6 (`kill(pid,0)` reporting zombies as alive).

The `(pid, start_time)` tuple is also what **systemd** (`PidRef`, with pidfd as
the primary and start-time as fallback), **psutil** (*"The process id and the
creation time uniquely identify a process in a system"*), and **SPIRE** use.
The canonical citation for why a bare PID is unacceptable is
`start-stop-daemon(8)`: *"Using the pidfile matching option alone might cause
unintended processes to be acted on."* That is our current design's bug.

---

## 3. Empirical findings on this host

### cgroup v2 works — and is a complete durable handle

Verified on Fedora 44, kernel 7.0.12, unprivileged uid 1000:

- `/sys/fs/cgroup` is `cgroup2fs`; our session cgroup is delegated and
  **owned by the user** (`drwx------ denniyahh denniyahh`), with
  `cgroup.kill`, `cgroup.procs`, `cgroup.subtree_control` present.
- Created a child cgroup, spawned a tree into it, **let the spawner exit**,
  then from a completely separate invocation with only the path string:
  `echo 1 > $CG/cgroup.kill` → everything died, `cgroup.procs` emptied,
  `rmdir` succeeded.

**The decisive test:** the tree included a **double-forked orphan** whose
parent had been reparented (`parent=1965`, chain severed). It was **still a
cgroup member and was still killed.** A ppid-walking tool would have missed it
entirely — this is exactly why `kill_tree` is unsound for us.

Liveness is `cgroup.procs` non-empty (or `cgroup.events` `populated`). No PID
is involved anywhere, so **PID reuse is structurally impossible**.

### cgroup v2 does NOT work in containers — fallback is mandatory

Verified with rootless podman on this host:

| Container config | `/sys/fs/cgroup` writable? | Child cgroup creatable? |
|---|---|---|
| `--user 1000:1000` | **NO** (read-only) | FAILED |
| default (root in userns) | **NO** (read-only) | FAILED |
| `--cgroupns=private --security-opt unmask=/sys/fs/cgroup` | **NO** | FAILED (`Permission denied`) |

Even the opt-in flags did not produce a writable cgroupfs here. This confirms
the operator's instinct: **humans developing inside containers cannot rely on
cgroups, and there is no reasonable flag we can ask them to pass.**

`runc` itself concedes this case in
`libcontainer/container_linux.go` — when cgroup delegation is unavailable it
logs *"failed to kill all processes, possibly due to lack of cgroup"* with the
comment `// Some processes may leak when cgroup is not delegated`.

### The POSIX guarantee that de-risks the fallback

> *"A process group ID shall not be reused by the system until the process
> group lifetime ends."* — POSIX, with process group lifetime ending *"when the
> last remaining process in the group leaves the group."*

**This matters enormously for us, and a detail of our own monitor makes it
bite.** Verified by reading `monitor.rs`: the script is
`… wait $apid; echo $? > exit_file; <binary> advance …` — there is **no
`exec`**. The `sh` leader forks the advance tail and waits for it, so the
leader is alive for the tree's entire lifetime. As long as the leader lives,
the group is non-empty, and **the PGID cannot be recycled**. That converts
`killpg` from "unsound" to "sound while the leader is verifiably alive."

---

## 4. Recommended design: layered, with cgroup as a fast path

A synthesis of runc's design, adapted to our platform constraints.

### Persisted handle (in `state.json`)

```jsonc
{
  "process_handle": {
    "mechanism": "cgroup" | "pgroup",
    "cgroup_path":       "/sys/fs/cgroup/.../devflow-p07-<uuid>",  // cgroup only
    "pgid":              12345,        // pgroup only
    "leader_pid":        12345,
    "leader_start_time": 26234043,     // /proc field 22 | pbi_start_tvsec+usec
    "boot_id":           "…"           // invalidates across reboots
  }
}
```

`boot_id` (`/proc/sys/kernel/random/boot_id` on Linux, `sysctl kern.boottime`
on macOS) makes a stale post-reboot `state.json` fail validation outright,
which closes the R5-after-reboot hole cheaply.

### Spawn

- Always: `CommandExt::process_group(0)` (stable since Rust 1.64, no `unsafe`,
  works Linux + macOS). **Only for `spawn_monitor`, never for
  `spawn_monitor_no_advance`** — that distinction is failure mode F4.
- On Linux with a writable delegated cgroup: create
  `<own-cgroup>/devflow-p<NN>-<uuid>`, write the leader pid to `cgroup.procs`,
  record `mechanism: "cgroup"`. Otherwise record `mechanism: "pgroup"`.
- Probe, don't assume: (a) is `/sys/fs/cgroup` cgroup2, (b) is our own cgroup
  dir writable, (c) does `cgroup.kill` exist (5.14+). Any failure → `pgroup`.

### Liveness (R1/R2)

- `cgroup`: `cgroup.procs` non-empty. Exact, no PID involved.
- `pgroup`: leader pid alive **AND** `start_time` matches **AND** `boot_id`
  matches **AND** state is not Zombie/Dead. Anything else → treat as gone.
  (This is runc's `hasInit()`, and it fixes F6.)

### Teardown (R2/R4)

- `cgroup`: `echo 1 > cgroup.kill`. Atomic, recursive, reaches severed-ppid
  orphans, zero PID-reuse risk. Then `rmdir`.
- `pgroup`: validate the leader first (above). If valid →
  `killpg(pgid, SIGTERM)` → grace → `killpg(pgid, SIGKILL)`.
  **Poll group emptiness, not leader liveness** — that is failure mode F2.

### The one honest residual

**Leader dead + descendants alive, on the `pgroup` path.** We must *refuse* a
blind `killpg` (the PGID may have been recycled). Narrow it by enumerating
processes and killing only those where `pgid == stored_pgid && start_time >=
leader_start_time`. Readable from `/proc` on Linux and `libproc` on macOS
(`ps -eo pid=,pgid=,lstart=` works on both as a shell fallback).

This residual is exactly what `runc` accepts and documents. We should document
it too rather than pretend it's covered.

### Dependencies

- `sysinfo` for portable start-time — start here; 1s resolution is sufficient
  (a false positive needs PID-space exhaustion *within one second*).
- **Do not adopt `cgroups-rs`.** The cgroup path is ~40 lines of `std::fs`
  (`create_dir`, write `cgroup.procs`, read `cgroup.procs`, write
  `cgroup.kill`, `rmdir`). Pulling `zbus` + `async-io` into a sync CLI to avoid
  40 lines is a bad trade. Borrow its v2-detection logic by reference, not by
  dependency.
- No tokio anywhere. No new external binaries. No daemon.

### Coverage against the known failure modes

| | Handled by |
|---|---|
| F1 leader dead / members alive | Liveness = group/cgroup emptiness, never leader liveness |
| F2 escalation never fires | Poll group emptiness for the grace period |
| F3 PID reuse | cgroup: structurally impossible. pgroup: start_time + boot_id validation |
| F4 Ctrl-C regression | `process_group(0)` on `spawn_monitor` **only** |
| F5 stranded per-phase lock | Release/clean the lock as part of teardown |
| F6 zombies read as alive | Treat Zombie/Dead as not-running (runc's rule) |
| F7 no path when state is missing | cgroup: scan our cgroup tree for `devflow-p*` dirs. pgroup: no clean answer — document it |

---

## 5. Alternatives rejected, with reasons

| Approach | Why not |
|---|---|
| **Resident DevFlow daemon** | The industry-standard answer (supervisord/pm2/circus/s6). Violates R1 and relocates R5 to "who supervises the supervisor." Reject unless we want to own a daemon. |
| **tmux/abduco session as handle** | Genuinely satisfies R1–R5 and is PID-free (overmind's bet). But adds a hard external binary dependency — fatal for the container requirement — and `kill-session` does **not** reliably kill grandchildren (tmuxinator#315). |
| **`systemd-run --user --unit=…`** | Perfect R1–R5 on Linux; unit name is the handle, `KillMode=control-group` gives free tree kill. Rejected as primary: two entirely different implementations (launchd on macOS), dies in systemd-less containers, pollutes a user-global namespace. (Note: `--scope` is the *wrong* mode — it's synchronous.) |
| **pidfs file handle** | Elegant and verified, but Linux-only, ~6.14+ kernel floor, and **cannot kill a tree**. Worth adding later as an opportunistic liveness upgrade. |

---

## 6. Open decisions

1. **Adopt the layered design?** (recommendation: yes)
2. **`sequentagent`: drop it?** It is the sole cause of F4 and has never been
   run (0 occurrences in `events.jsonl`). Dropping removes a failure mode
   before we write any code. Caveat: `retry_after_from_reason` must *move*, not
   be deleted — `pipeline_outcomes.rs:92` uses it for the primary loop's
   rate-limit auto-resume.
3. **Project-wide sweep (F7)?** Clean and foolproof on the cgroup path (scan
   for `devflow-p*` cgroups). No sound equivalent on the `pgroup` path. Ship it
   Linux-only, or not at all?
4. **macOS verification.** Every macOS claim here is documentation-based —
   there is no macOS host and no macOS CI. If macOS support is to be real
   rather than nominal, this needs a CI runner or manual verification.

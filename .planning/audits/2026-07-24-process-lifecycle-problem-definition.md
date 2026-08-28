# Process Lifecycle & Teardown — Problem Definition

Date: 2026-07-24
Purpose: **design input**, not a review recap. Everything here is context for
building a real solution to DevFlow's process-management problem.
Related: `.planning/ROADMAP.md` 999.33 (open problem) / 999.34 (blocked),
GitHub: [#185](https://github.com/denniyahh/devflow/issues/185) (migrated from Linear DEN-58) / DEN-59.

Status of prior work: branch `feature/monitor-process-group-stop` is
**unmerged and being abandoned** as an implementation. Its *findings* are
preserved below; its code is not load-bearing for the new design.

---

## 1. The requirement

DevFlow spawns a **detached process tree that deliberately outlives the CLI
invocation that created it**. A *later, unrelated* CLI invocation must then
be able to:

- **R1 — Determine liveness.** Is that tree still running? (Not "does some
  process with this PID exist" — is *our* tree alive.)
- **R2 — Terminate it, completely.** Kill every member, not just whichever
  one happens to be identifiable, with escalation if it ignores a polite
  signal.
- **R3 — Do both from a different process,** using only what is persisted on
  disk. No in-memory handle exists or can exist.
- **R4 — Never signal something that isn't ours.** PID reuse must not cause
  DevFlow to kill an unrelated process or process group.
- **R5 — Survive DevFlow's own death.** The mechanism must work after a
  crash, `kill -9`, or reboot of the spawning process.

R3 is the constraint that invalidates most off-the-shelf solutions.

## 2. How the process model actually works today

### Spawn (`crates/devflow-core/src/monitor.rs`)

`spawn_monitor_inner` builds a shell script and spawns it detached with
stdin/stdout/stderr all `Stdio::null()`. The tree is:

```
sh -c "<script>"                     ← "the monitor"; its PID is recorded
 ├── <agent>  (claude -p … / codex exec …)   ← backgrounded, PID → pid file
 └── devflow advance --phase N        ← tail call, runs AFTER the agent exits
```

The script, in order: traps TERM/INT → `cd` to the worktree → launches the
agent in the background → records the agent PID → `wait $apid` → records the
exit code → **execs `devflow advance`**.

Two variants share this code path:

| Function | Used by | Advance tail? | Caller behaviour |
|---|---|---|---|
| `spawn_monitor` | `devflow start`, `devflow parallel` (via `start`) | Yes | CLI **exits immediately**; monitor detached |
| `spawn_monitor_no_advance` | `devflow sequentagent` | No | CLI **blocks in foreground** on `wait_for_agent_exit` |

That table is load-bearing — the two variants have genuinely different
lifecycle needs, and conflating them caused one of the defects in §4.

### What is persisted (the only inputs a later process has)

`.devflow/state-NN.json` (`crates/devflow-core/src/state.rs`):
- `phase`, `stage`, `agent`, `worktree_path`
- `monitor_pid: Option<u32>` ← **the only process handle persisted**
- `stopped: bool`, `stop_reason: Option<String>`

`.devflow/` per-phase files (`agent_result.rs`):
- `phase-NN-agent-pid`, `phase-NN-stdout`, `phase-NN-stderr.log`,
  `phase-NN-exit`

`.devflow/lock-NN` (`lock.rs`) — per-phase lock, holds the PID of the holder.
**`advance()` holds this across a gate's multi-day blocking wait**
(`lock.rs:8-10`, `pipeline_launch.rs:287-288`).

Critically: **`sequentagent` persists nothing.** It builds a "synthetic,
never-persisted state" (`parallel.rs:192-195`) and never calls `save_state`,
so its `monitor_pid` lives only in the running CLI's memory.

### Who reads process state

`status`, `doctor` (reconciliation: `check_dead_agent`, `check_dead_monitor`),
`recover`, `cleanup` (refuses to remove a worktree with a live agent/monitor),
and `liveness()` which classifies `Healthy` / `BetweenStages` / `Stuck` /
`Unknown`.

All of them ultimately call `agent::agent_running(pid)` → `libc::kill(pid, 0)`.

## 3. Platform reality (verified 2026-07-24)

This narrows the solution space more than expected:

- **CI is `ubuntu-latest` only** — all three jobs in `.github/workflows/ci.yml`
  plus `devcontainer.yml`. macOS and Windows are never built or tested.
- **Zero `cfg(unix)` / `cfg(target_os)` / `cfg(windows)` gating exists
  anywhere in `crates/`.** The code is unconditionally Unix.
- Production code already depends on `std::os::unix` and `libc`
  (`agent.rs`, `monitor.rs`, `preflight.rs`).
- **Windows is therefore already unsupported** — it would not compile.
- **macOS is *nominally implied*** (`DEPENDENCIES.md` says "Linux/macOS" for
  git; README says "A POSIX shell") **but is never tested and never
  exercised.** macOS has POSIX process groups but no cgroups.

**So the open question is not "do we need macOS?" in the abstract. It is:
do we keep an untested, unverified, implied macOS claim — or drop it
explicitly?** Answering this picks the design (see §6).

## 4. Failure modes discovered — treat these as the acceptance tests

All found while building/reviewing the abandoned branch. Any real solution
must handle every one of these. The first three share a single root cause:
**a process-group *leader's PID* is not a durable handle to the group.**

| # | Failure mode | Evidence |
|---|---|---|
| F1 | **Leader dead, members alive → teardown skipped entirely.** Guarding teardown on the leader's liveness means the classic `Liveness::Stuck` state (dead monitor, live agent) is never cleaned up. | **Demonstrated empirically.** `devflow stop` printed *"no live process found"* while the target agent kept running. |
| F2 | **Leader exits first → escalation never fires.** The monitor's trap always `exit 0`s promptly, so polling the leader for death returns "dead" almost immediately, and a SIGKILL-escalation guard conditioned on it never triggers. A SIGTERM-ignoring agent survives indefinitely. | Code-path analysis; the trap is `cleanup() { kill "$apid"; exit 0; }`. |
| F3 | **PID reuse → signalling an unrelated process group.** `kill(-pid, …)` on a recycled PID hits a foreign group. Strictly worse blast radius than a single-PID kill. | Inherent to PID-as-handle. |
| F4 | **Process-group isolation breaks foreground Ctrl-C.** Applying it to `spawn_monitor_no_advance` put `sequentagent`'s monitor in its own group, so terminal SIGINT no longer reaches it — the CLI dies and the agent keeps running. And because `sequentagent` persists no `monitor_pid`, no recovery command can reach it either. | Code-path analysis + verified pgid change. |
| F5 | **Killing the tree strands the per-phase lock.** `advance` holds `lock-NN` across gate waits; killing it means `LockGuard::Drop` never runs, leaving a lock file with a dead PID. (Mitigated but not solved by `lock.rs:95-105` stale-holder reclaim — leaves a `doctor` finding and a reclaim warning right after a "clean" stop.) | Code read + `lock.rs` module docs. |
| F6 | **`agent_running()` reports zombies as alive.** `kill(pid, 0)` succeeds for a not-yet-reaped zombie. Cost me two false debugging detours; also means `liveness()` can report a zombie monitor as `Healthy`. | Demonstrated — `/proc/<pid>/status` showed `State: Z (zombie)` while `kill(pid,0)` returned 0. |
| F7 | **No teardown path at all when state is missing.** A state-driven stop returns "nothing to stop" while processes linger. This is the shape of the original Phase 22 incident: `doctor` said *"no active phases"* while worktree, branch, and commits survived. | Observed in the Phase 22 incident. |

## 5. What has been ruled out — do not re-litigate

- **`command-group`** (6.7M downloads, watchexec org) — **does not satisfy
  R3.** API confirmed spawn-centric: `GroupChild`/`AsyncGroupChild` handles
  returned from spawning, plus `UnixChildExt` for signalling children *you*
  spawned. **No attach-to-an-existing-pgid API.** A handle cannot be
  serialized into `state.json` and reconstituted by a later process. Would
  not have prevented F1–F4.
- **`duct`** — maintainer explicitly declined process-group handling
  ([duct.rs#41](https://github.com/oconnor663/duct.rs/issues/41)).
- **`daemonize` / `fork`** — solve terminal-detachment daemonization; a
  different problem.
- **`nix` / `rustix`** — typed `killpg`/`getpgid`/`waitpid` wrappers.
  Cosmetic only: identical logic, would have caught none of F1–F7.
- **Leader-PID-as-handle** — the root cause of F1/F2/F3. Any design that
  reintroduces it is wrong.

**What *was* proven to work:** `std::os::unix::process::CommandExt::process_group(0)`
correctly places the whole tree in one process group at spawn time (stable
since Rust 1.64, no new dependency). RED-proven: reverting only that line
reproduces the orphaning; restoring it fixes it. **This is a valid building
block** and should be carried into the new design — the mistake was the
*handle*, not the grouping.

## 6. The shape a real solution needs

**A durable, nameable handle to a process tree that survives its spawner's
exit and can be queried and signalled by an unrelated process.**

Process groups are a *weak* form: named by a recyclable PID, and the leader
can vanish while members live — which is precisely F1/F2/F3.

Strong forms:

| Primitive | Platform | Properties |
|---|---|---|
| **cgroup v2** | Linux | Nameable path, persists independent of any member, `cgroup.procs` enumerates members, `cgroup.kill` kills all atomically. No PID-reuse hazard. |
| **Job Objects** | Windows | Equivalent semantics. Irrelevant — Windows already unsupported. |

Candidate implementations:

- **DIY cgroup v2** — write the cgroup path into `state.json`; liveness = "is
  `cgroup.procs` non-empty"; teardown = write to `cgroup.kill`. Costs:
  Linux-only; unprivileged creation needs systemd user delegation
  (`user@.service` / `systemd-run --user --scope`); needs a documented
  fallback if delegation is unavailable.
- **[`processkit`](https://crates.io/crates/processkit)** — conceptually
  exactly right (kernel-backed cgroup v2 / Job Object containers, whole-tree
  kill-on-drop, supervision, restart/backoff). Blocked on maturity: created
  2026-05-31, ~9K downloads, and requires adopting tokio into a fully
  synchronous codebase.

**Hardening that applies regardless of which primitive wins:**
- Probe *group/container* existence, never leader liveness (kills F1/F2).
- Validate process identity by start-time —
  [`sysinfo`](https://crates.io/crates/sysinfo), 172M+ downloads — so a
  recycled PID can't be mistaken for the original (kills F3, and fixes the
  zombie half of F6 if it distinguishes zombie state).
- Keep `process_group(0)` for `spawn_monitor` only; **not** for
  `spawn_monitor_no_advance` (kills F4).
- Release/clean the per-phase lock as part of teardown (kills F5).
- Provide a state-independent sweep — "find and kill anything belonging to
  this project" — so F7 has a recovery path.

## 7. Decisions needed before designing

1. **macOS: drop the implied claim, or keep it?**
   - *Drop it* (declare Linux-only, matching CI reality) → DIY cgroup v2 with
     no fallback. Simplest, most robust. Requires editing DEPENDENCIES.md /
     README.
   - *Keep it* → `cfg(target_os)` dual implementation: cgroup v2 on Linux,
     hardened process-group + `sysinfo` identity validation on macOS. Roughly
     double the surface, and the macOS path stays untested until CI adds a
     runner.
2. **Is the systemd user-delegation dependency acceptable?** Unprivileged
   cgroup v2 creation typically requires it. If DevFlow must run in
   containers/CI without systemd, a fallback is mandatory regardless of (1).
3. **Does `sequentagent` survive?** Flagged separately as a pruning candidate
   in `2026-07-24-scope-creep-complexity-review.md` §4. If it's cut, F4
   disappears entirely and the design gets simpler. **Answer this before
   designing, not after.**
4. **Scope of teardown:** per-phase only, or a project-wide "kill everything
   DevFlow started here"? F7 argues for the latter as a recovery path.

## 8. Design elements already validated (carry forward)

From the abandoned `devflow stop` work, these survived review and are
independent of the handle problem:

- Emit an authoritative `workflow_stopped` event so `doctor`/`status`/
  `events.jsonl` show an intentional stop rather than a run trailing into
  silence.
- Resolve open gates for the phase so nothing waits on a response no process
  will consume.
- Set `State.stopped` / `stop_reason`, clear `monitor_pid` / `gate_pending`
  (mirrors the existing `--until` halt precedent in `pipeline_gate.rs`).
- Leave worktree/branch/commits untouched; `cleanup --force` stays the
  separate destructive step.
- Idempotent on an already-stopped or already-dead phase — **but preserve a
  pre-existing `stop_reason` instead of overwriting it.**
- Don't swallow gate-cleanup errors while reporting a success count.

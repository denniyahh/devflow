# Phase 23: End-to-End Dogfood - Research

**Researched:** 2026-07-25
**Domain:** Unix process supervision (socket-addressable handle replacing a shell-script monitor), DevFlow's own gate/pipeline state machine, CLI surface reduction
**Confidence:** HIGH (grounded in re-read source, live `rg`/`cargo test` verification, and the project's own spike/audit docs — not training-data guesses about DevFlow internals). Two items are explicitly flagged LOW/MEDIUM (macOS, exact historical reference counts) — see Assumptions Log.

## Summary

Phase 23 has almost no invention left to do at the mechanism level — the hard
design work is already finished and recorded in
`.planning/audits/2026-07-24-socket-supervisor-spike.md` (superseding an
earlier, rejected cgroup/pgroup design in
`2026-07-24-process-teardown-solution-research.md` — see the note in
Architecture Patterns). What remains is: (1) run the probe (23a) to prove or
correct the assumption that the supervisor is the actual blocker; (2)
mechanically migrate ~8 production call sites plus 2 test files from
`sh -c`-monitor primitives to the socket-addressable ones (23b); (3) build
`devflow stop` on top of the new handle (23c, cheap once 23b lands); (4)
delete `sequentagent` (142 references across 11 files — larger than
CONTEXT.md's recorded ~110/11, see Package Legitimacy... no, see the 23d
inventory below); and (5) add `--yes-ship`, which has one exact call site
(`pipeline_outcomes.rs:275-286`, `handle_ship_outcome`) and one exact
mechanism for "auto-answer, not bypass" (self-write a `GateResponse` via the
existing `Gates::respond` API immediately after `Gates::write_gate`, so the
audit trail is indistinguishable in shape from a human response except for
`responded_by`).

No new dependency is needed anywhere in this phase. `libc = "0.2"` is already
a `devflow-core` dependency; `std::os::unix::net::{UnixListener, UnixStream}`
needs nothing new. The spike proves the exact shape to build.

**Primary recommendation:** Plan 23b as a mechanical, file-by-file migration
against the verified call-site inventory below (not a redesign — the design
is already spike-proven), land signal handling (SIGTERM/SIGINT → clean
`shutdown`-equivalent) inside 23b rather than deferring it (cheap given
`std`-only signal handling is available via a self-pipe/`signal-hook`-free
polling pattern — see Common Pitfalls), give the scratch repo for 23a the
minimal `develop`+`main` git-flow scaffold plus a one-phase `.planning/`
skeleton, and thread `--yes-ship` as a `State`-persisted per-run boolean
(`#[serde(default)]`, following the exact precedent of `monitor_pid` /
`stop_until` / `preflight_retries`) rather than a CLI-only value, since the
Ship gate may fire stages/process-invocations after the original `start`
command line is gone.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Process supervision (spawn/liveness/teardown) | Backend/CLI process (devflow-core) | OS (Unix socket + pgid) | The monitor **is** `devflow` itself post-23b; the socket is an OS-level rendezvous point, not a separate service tier |
| `advance` (stage transition logic) | Backend/CLI process (in-process, D-10) | — | Runs inside the same process as the monitor after 23b; no longer a forked CLI invocation |
| Gate approval / `--yes-ship` | Backend/CLI process (devflow-core `gates.rs`) | Persisted state (`state.json`) | Gate protocol is file-based IPC already; auto-answer is a producer of the same file, not a new tier |
| `devflow stop` | Backend/CLI process | OS (socket `shutdown` message + `killpg` backstop) | Mirrors the supervisor's own teardown path exactly — no new tier |
| Scratch-repo probe target (23a) | Filesystem / git (repo tier) | — | Not a code capability — a fixture; belongs to the "project under test" tier, entirely outside devflow's own process model |
| `sequentagent` removal | CLI surface (devflow-cli `main.rs`/`parallel.rs`) | — | Pure subtraction from the command-dispatch tier; no new tier introduced |

## User Constraints

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Dogfood probe and acceptance (23a)**
- D-01: 23a's probe runs in a scratch repo, not this checkout (blast-radius isolation post-999.37). Reversible.
- D-02: The phase's final acceptance run is self-hosted (this repo), only after the scratch probe has proven the supervisor — because `staleness_outcome` (`crates/devflow-cli/src/staleness.rs:276-284`) only hard-blocks `(is_self_dogfood: true, Stale)`; a scratch-only acceptance would never exercise the most frequent observed dogfood killer. Reversible.
- D-03: Probe output is a recorded artifact (events.jsonl excerpts + `.devflow/phase-N-*` captures), not a verbal report. 23a is explicitly permitted to invalidate the rest of this phase's scope. Reversible.

**Unattended semantics and `--yes-ship`**
- D-04: Add a `--yes-ship` pre-authorization flag. Required because `Mode::Auto` does not gate-skip Ship — `crates/devflow-core/src/mode.rs:82-94`/`96-105` documents "Ship always gates (both modes)". Costly to reverse (user-facing CLI flag on the irreversible merge/version/changelog path).
- D-05: `--yes-ship` is a per-run flag only — never config-persistable. Must be typed on each invocation; must not be settable in `devflow.toml`. Costly to reverse.
- D-06: `--yes-ship` auto-answers the gate rather than bypassing it — the gate still fires and still records an explicit pre-authorized approval in `events.jsonl` and the gate ledger. Reversible.
- D-07 (ACCEPTED RISK): `--yes-ship` is NOT refused on the self-dogfood workspace. Combined with D-02, the acceptance run will unattended-merge a real phase into `develop`, bump the version, and append the changelog on this repository. Suggested mitigations for the planner to encode, not re-decide: drive a low-stakes phase for the acceptance run; establish a recovery point (tag or branch) before it starts. One-way.

**Supervisor migration (23b)**
- D-08: Big-bang replacement, no feature flag — delete the `sh -c` path outright. One-way; ~8 files.
- D-09: Already decided, do not re-open — socket lives in `~/.cache/devflow/` (not `$XDG_RUNTIME_DIR`, which systemd deletes on logout); socket path is stored in `state.json`, never derived at probe time; liveness is `connect()` → GONE/STALE/ALIVE with no PID on the happy path; pgid backstop applies only when STALE, guarded by `start_time` + `boot_id`. Costly to reverse.
- D-10: The `advance` tail runs in-process — the monitor becomes `devflow` itself and calls advance directly, removing the Phase 17 failure mode by construction. Reversible within the new design.

**`sequentagent` removal (23d)**
- D-11: Hard delete the `sequentagent` verb (`crates/devflow-cli/src/main.rs:159`, dispatch at `:483`). One-way — removes a published CLI command from a crates.io-released binary.
- D-12: This removal earns the v2.0.0 slot — assume a major version bump, not minor. One-way.
- D-13: The capability intent is preserved (DEN-67/999.42, not discarded) — reimplemented on the supervisor when a second agent is supported, prerequisites DEN-58 + DEN-56. Reversible.

### Claude's Discretion

- **Supervisor signal handling.** DEN-58 notes the spike installs no handler, so SIGTERM to the monitor leaves a stale socket (degrades correctly via sweep + pgid backstop, but production "should" trap SIGTERM/SIGINT and perform the same clean shutdown as the socket `shutdown` command). Land in 23b or defer — must be an explicit, recorded call.
- **Scratch-repo scaffolding for 23a** — minimum `.planning/` + GSD structure the probe target needs to be a valid devflow target.
- **In-flight-phase behaviour across the D-08 upgrade** — whether a phase whose `state.json` predates the `supervisor` field should be refused with guidance, or handled some other way.
- **Whether `hooks_after_ship` gains a `WorktreeRemove` step** and whether per-phase capture files get swept (DEN-59's operator note) — both untested-on-success paths this phase is first to exercise.

### Deferred Ideas (OUT OF SCOPE)

- `--yes-ship` refusal on the self-dogfood workspace — considered and declined this phase (D-07); ready-made mitigation if the accepted risk proves uncomfortable.
- 999.31/DEN-56 Modular Agent Driver — deferred, Codex blocker not Claude.
- 999.25/DEN-50 release-cut executor — crates.io publish stays manual.
- 999.42/DEN-67 agent failover on token exhaustion — preserved intent, blocked on DEN-58 + DEN-56.
- macOS verification — DEN-58 flags it as the single largest unknown; out of scope, do not claim macOS support from this phase.
- 999.38/DEN-65 test-suite PATH race; 999.39/DEN-66 production git calls inherit a redirecting environment; the old Test Suite & CI Hardening theme (999.15/17/18/19/20/22).
</user_constraints>

<phase_requirements>
## Phase Requirements

No REQ-IDs exist for this phase (ROADMAP.md and CONTEXT.md both record "TBD — units 23a-23d, sourced from 999.33/DEN-58 and 999.34/DEN-59 plus the dogfood-probe finding this phase generates first"). The planner should treat the four units below as the requirement set; 23a is explicitly permitted to add/replace requirements based on what it finds.

| ID (unit) | Description | Research Support |
|-----------|-------------|-------------------|
| 23a | Dogfood probe: run `devflow start` on a small real phase in a scratch repo with a ≥v1.8.1 binary; record exactly where it dies | See "Scratch-repo scaffolding" and "Current binary/version state" below |
| 23b | Socket-addressable supervisor replacing `sh -c` monitor (999.33/DEN-58) | See "23b Migration Inventory" and "Architecture Patterns" |
| 23c | `devflow stop` — explicit clean phase abort (999.34/DEN-59), blocked on 23b | See "How --yes-ship threads through" adjacent section and "hooks_after_ship / capture sweep" |
| 23d | Drop `sequentagent` (subtractive) | See "23d Deletion Inventory" |
| (cross-cutting) | `--yes-ship` pre-authorization flag (D-04..D-07) | See "How --yes-ship threads through" |
</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|---------------|
| `libc` | `0.2` (already a `devflow-core` dependency) `[VERIFIED: crates/devflow-core/Cargo.toml:19]` | `kill`, `killpg`, raw signal numbers | Already used by `agent.rs`/`monitor.rs`/`preflight.rs`; no version bump needed for this phase |
| `std::os::unix::net::{UnixListener, UnixStream}` | stable since Rust 1.0 (Unix-only) | Socket-addressable supervisor's durable handle | Proven in the spike (`spikes/socket-supervisor/main.rs`); zero new dependency, matches DEN-58's explicit "no new dependency" design win |
| `std::os::unix::process::CommandExt::process_group(0)` | stable since Rust 1.64 `[CITED: process-lifecycle-problem-definition.md §5]` | Places the whole spawned tree in one killable process group at spawn time | Already the validated building block from the abandoned branch; carries forward unchanged into the socket design |
| `serde` / `serde_json` | `1` (workspace) `[VERIFIED: Cargo.toml:21-22]` | `State.supervisor` block persistence | Existing pattern for every other `state.json` field |
| `thiserror` | `2` (workspace) `[VERIFIED: Cargo.toml:25]` | New supervisor error variants, matching `MonitorError`'s existing shape | House convention (`CONVENTIONS.md` Error Handling) |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | `3` (already a dev-dependency, both crates) `[VERIFIED: Cargo.toml]` | Test fixtures for the new supervisor module | Only in `#[cfg(test)]`/integration tests, per existing pattern |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Unix domain socket handle | cgroup v2 (`cgroup.kill`/`cgroup.procs`) | **Rejected by this project's own prior research** (`2026-07-24-process-teardown-solution-research.md`): does not work in containers even with delegation flags (verified empirically with rootless podman on this host), and the operator's own dev environment includes containerized work. The socket design is container-safe and cross-platform-uniform; do not resurrect the cgroup design. |
| Unix domain socket handle | `command-group` / `process-wrap` (watchexec org) | **Ruled out, do not re-propose** — spawn-centric handle (`GroupChild`), cannot be serialized into `state.json` and reconstituted by a later process (fails requirement R3). |
| Unix domain socket handle | `processkit` | **Ruled out, do not re-propose** — architecturally incapable: cgroup dir name is salted per-process specifically to prevent reconstruction; `adopt()` needs a live tokio `Child`; ~8-week-old crate with a hard tokio dependency in a fully synchronous codebase. |
| Unix domain socket handle | `duct` / `daemonize` / `nix` / `rustix` | **Ruled out, do not re-propose** — `duct`'s maintainer explicitly declined process-group handling; `daemonize`/`fork` solve terminal detachment, a different problem; `nix`/`rustix` are cosmetic typed wrappers over the same `libc` calls already in use, catching none of F1-F7. |
| Persisted `sysinfo`-based PID+start_time identity | `procfs-core`/`libproc` high-res start time | Only relevant for the STALE-path pgid backstop, which this design already uses `start_time` + `boot_id` for without adding `sysinfo` — grounded in the spike's own C5 result (`/proc` fields read directly). Not needed as a new dependency for the happy path. |

**Installation:** No new packages. Verify existing pins:
```bash
cargo tree -p devflow-core -i libc   # confirm libc 0.2.x already resolved
```

**Version verification:** `libc = "0.2"` and `serde`/`serde_json`/`thiserror` are already pinned workspace-wide and used by the exact modules (`monitor.rs`, `agent.rs`, `state.rs`) this phase touches — re-verified live via `Cargo.toml` reads on 2026-07-25, not training-data recall.

## Package Legitimacy Audit

**Not applicable.** This phase installs zero new external packages — the entire design is `std` + the already-present `libc` crate (D-09/DEN-58's explicit "no new dependency" win, re-confirmed against `Cargo.toml` this session). No package-legitimacy check is needed.

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

## Architecture Patterns

### Two prior research passes exist — only the second is current

`.planning/audits/2026-07-24-process-lifecycle-problem-definition.md` and its
companion `2026-07-24-process-teardown-solution-research.md` reach a
**cgroup-v2-primary, pgroup-fallback** design (`process_handle` with
`mechanism: "cgroup" | "pgroup"`). That design was then **superseded** by a
third, later document — `2026-07-24-socket-supervisor-spike.md` — which
spikes and validates a simpler, uniform **socket-addressable supervisor**
design instead (no cgroup at all). CONTEXT.md's D-09 locks the *socket*
design ("carried forward from DEN-58's spike... do not re-open"), and the
ROADMAP/CONTEXT text for 23b describes the socket design exclusively. **The
planner must build the socket design, not the cgroup/pgroup design** — the
two earlier documents remain useful for (a) the failure-mode catalog F1-F7,
which the socket design is graded against, and (b) the list of ruled-out
crates, but their own `process_handle`/cgroup recommendation is not what
ships. This is a real inconsistency across the project's own planning docs;
flagging it explicitly so the plan is not built against the stale
recommendation.

### System Architecture Diagram

```
devflow start --phase N
        |
        v
  launch_stage_inner (pipeline_launch.rs)
        |  (preflight, staleness gate, capture archival)
        v
  monitor::spawn_supervisor(state, program, args, envs)   <- NEW, replaces spawn_monitor
        |
        v
  +---------------------------------------------------------+
  |  supervisor process (was: "sh -c ...", is now: devflow   |
  |  itself, argv0 signals "I am the supervisor")            |
  |                                                            |
  |  binds ~/.cache/devflow/<hash(project_root)>-<phase>.sock  |
  |  spawns agent (Command::process_group(0))                 |
  |  writes agent-pid file; captures stdout/stderr to files    |
  |                                                            |
  |  loop {                                                    |
  |    agent exits naturally -----------> write exit code      |
  |                                        run `advance` IN-    |
  |                                        PROCESS (D-10)       |
  |                                        remove socket, exit  |
  |                                                              |
  |    receives "shutdown" on socket ---> killpg(SIGTERM..KILL)  |
  |                                        write exit code 143   |
  |                                        SUPPRESS advance (R-M)|
  |                                        remove socket, exit   |
  |                                                              |
  |    receives SIGTERM/SIGINT (OS) ----> [Claude's discretion:  |
  |                                        same clean-shutdown    |
  |                                        path as "shutdown"]    |
  |  }                                                            |
  +---------------------------------------------------------+
        ^                                          ^
        | connect() -> ALIVE/STALE/GONE            | "shutdown" message
        |                                          |
  devflow status / doctor                    devflow stop (23c, NEW)
  (liveness re-pointed at socket probe        (writes workflow_stopped event,
   instead of PID-based agent_running)         resolves open gates, suppresses
                                                advance via R-M)
```

### Recommended Project Structure

No new top-level module is strictly required — the existing `crates/devflow-core/src/monitor.rs` is the natural home for the rewritten supervisor (it already owns `spawn_monitor`/`spawn_monitor_no_advance`/`wait_for_agent_pid`/`wait_for_agent_exit`, all of which this phase replaces or removes). Recommend replacing its internals in place rather than adding a parallel `supervisor.rs`, since D-08 is a big-bang replacement (no dual-path period during which two modules would coexist).

```
crates/devflow-core/src/
├── monitor.rs        # rewritten: socket bind/listen, spawn, liveness probe, shutdown, sweep
├── state.rs           # + `supervisor: Option<SupervisorHandle>` field (serde(default))
├── agent.rs            # agent_running() retained ONLY as the pgid-backstop identity check
crates/devflow-cli/src/
├── pipeline_launch.rs  # spawn_supervisor call site (replaces spawn_monitor)
├── commands.rs         # `status`/`doctor` liveness re-pointed at socket probe; NEW `stop` command
├── parallel.rs          # DELETED (sequentagent, 23d) — `parallel` (N-phases-concurrently) stays
├── main.rs              # Sequentagent variant + dispatch arm DELETED (23d); `--yes-ship` flag added to Start
```

### Pattern 1: Socket path as a durable, PID-free handle

**What:** Bind a Unix domain socket at a short, fixed-length path (`~/.cache/devflow/<hash>-<phase>.sock`, keyed by a hash of `project_root` + phase to avoid cross-project collision) and store that path string in `state.json`. Liveness is answered purely by `connect()`: no socket file = GONE; file exists but `ECONNREFUSED` = STALE; connects = ALIVE. No PID is read on the happy path.

**When to use:** Every phase's monitor/supervisor spawn (replaces both `spawn_monitor` and `spawn_monitor_no_advance` — though the latter's caller, `sequentagent`, is deleted in this same phase, so only the `spawn_monitor` shape survives).

**Example (from the verified spike):**
```rust
// Source: .planning/spikes/socket-supervisor/main.rs (re-run this to reproduce)
fn status(sock: &str) {
    if !Path::new(sock).exists() { println!("GONE"); return }
    match UnixStream::connect(sock) {
        Ok(mut s) => {
            let _ = writeln!(s, "ping");
            let mut r = String::new();
            let _ = BufReader::new(s).read_line(&mut r);
            println!("ALIVE ({})", r.trim());
        }
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => println!("STALE"),
        Err(e) => println!("UNKNOWN {e}"),
    }
}
```

**Why `~/.cache/devflow/` and not `$XDG_RUNTIME_DIR` (already locked, D-09):** `$XDG_RUNTIME_DIR` is removed by systemd when the last session for a user ends — fatal for a long unattended run if the operator logs out. `~/.cache/devflow/<hash>.sock` is ~45 bytes on macOS (well under the 104-byte `sun_path` limit) and survives logout.

### Pattern 2: In-process `advance` tail (D-10)

**What:** The supervisor, on natural agent exit, calls the `advance` logic as a Rust function call in the same process — not a forked `devflow advance` subprocess.

**When to use:** Every natural-completion path. This is the single property that removes Phase 17's failure mode by construction (no forkable tail process left to orphan on a signal).

**Example:**
```rust
// Source: .planning/spikes/socket-supervisor/main.rs (adapted)
if let Ok(Some(st)) = child.lock().unwrap().try_wait() {
    let code = st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0));
    std::fs::write(&exit_file, format!("{code}\n"))?;
    // R-E: call devflow's own advance() function directly here — no
    // `Command::new(binary).arg("advance")` subprocess spawn.
    devflow_core::advance_in_process(&project_root, phase)?;   // production name TBD by planner
    std::fs::remove_file(&sock)?;
    std::process::exit(0);
}
```

**Migration note:** today's `advance_tail` in `monitor.rs:138-147` builds a shell string
`"; {binary} advance {project_root} --phase {phase}"`. This entire code path
is deleted; the equivalent logic must be reachable as a plain function call
from within the (now Rust, not shell) supervisor process. The planner should
verify whether `commands::advance` (the CLI's existing `Advance` dispatch
target) is already free of CLI-only concerns (arg parsing, `project_root()`
resolution) or needs a thin extraction so the supervisor can call the core
logic directly without going through `clap`.

### Pattern 3: STALE-path pgid backstop, gated by identity validation

**What:** When the socket is STALE (file exists, `ECONNREFUSED`), do not assume the tree is dead — enumerate processes by the persisted `agent_pgid`, validate `start_time` + `boot_id` before `killpg`, per the spike's C5 result.

**When to use:** `devflow stop`'s STALE branch, and any sweep/recovery path.

**Anti-Patterns to Avoid**
- **Leader-PID-as-handle:** the root cause of the original three failure modes (F1/F2/F3). Never reintroduce a design where liveness is answered by `kill(leader_pid, 0)`.
- **Deriving the socket path at probe time instead of reading it from `state.json`:** a changed `$XDG_RUNTIME_DIR`/`$TMPDIR` between invocations would orphan the handle. Already locked by D-09 — do not regress this in implementation.
- **Treating a stop as a completion:** R-M is the one behavioral property that most needs a dedicated regression test — a `devflow stop` must never allow `advance` to run afterward.
- **Reintroducing `process_group(0)` on the no-advance path:** F4 (Ctrl-C regression) was caused by exactly this. Moot after 23d removes `spawn_monitor_no_advance`'s only caller, but note it explicitly in case any other future caller of that shape appears.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| Process-tree teardown across a later, unrelated process | A ppid-walking kill utility, or a hand-rolled cgroup v2 client | The already-spiked socket+killpg design (this phase) | ppid-walking is unsound (severed-ppid orphans, verified reproduce-able); cgroup v2 fails in containers (verified with rootless podman) |
| Cross-process durable liveness | Polling `kill(pid, 0)` | `connect()` to the socket | `kill(pid, 0)` cannot distinguish a dead leader from a healthy between-stages pause (Finding 1) and reports zombies as alive (F6) |
| Gate auto-approval audit trail | A CLI-only bypass flag that skips gate-writing entirely | `Gates::write_gate` + `Gates::respond` (both already exist) | D-06 requires the decision to be recorded exactly like a human response; the existing API already supports self-produced responses — no new mechanism needed |
| Phase capture-file cleanup on stop | A bespoke deletion routine | `agent_result::archive_phase_files` (already exists, already called on relaunch rollover) | Reuses the exact retention-aware archival the project already trusts, rather than inventing a second deletion path with different retention semantics |

**Key insight:** Every "don't hand-roll" item above already has a load-bearing implementation living in this codebase today. The work in 23b/23c/`--yes-ship` is almost entirely *wiring existing primitives to a new call site*, not inventing new ones.

## 23b Migration Inventory (verified against the live tree, 2026-07-25)

`rg -n "spawn_monitor\b|spawn_monitor_no_advance|wait_for_agent_pid|wait_for_agent_exit" --type rust crates/`
was re-run this session (not assumed from CONTEXT.md's summary). **CONTEXT.md's
"~8 files" figure is confirmed exactly correct** — 8 distinct files outside
`monitor.rs` itself reference these four functions:

| File | Line(s) | Reference type | What it needs to become |
|------|---------|------------------|------------------------------|
| `crates/devflow-cli/src/pipeline_launch.rs` | `:126` (functional call: `monitor::spawn_monitor(state, program, &args, &adapter.extra_env())`); `:68`, `:91`, `:562` (comments) | **Functional call site — the production spawn path** | Replace with the new supervisor-spawn function (e.g. `monitor::spawn_supervisor`). This is the single load-bearing migration point for the entire "start a phase" path. |
| `crates/devflow-cli/src/parallel.rs` | `:201` (functional: `monitor::spawn_monitor_no_advance(&state, program, &args, &adapter.extra_env())`); `:217` (functional: `monitor::wait_for_agent_exit(project_root, phase, monitor_pid)`) | **Functional call sites — both exclusively serve `sequentagent`** | **Deleted, not migrated** — both call sites live inside `sequentagent`'s synchronous handoff loop, which 23d removes in this same phase. Confirms DEN-58's note that dropping `sequentagent` "closes the explicitly-untested `wait_for_agent_exit` gap" — that gap disappears because the function itself disappears. |
| `crates/devflow-core/tests/monitor_e2e.rs` | `:19` (import), `:77` (`spawn_monitor(&state, "sh", &args, &[])`), `:80` (`wait_for_agent_pid(root, phase)`) | **Test file — functional calls against the OLD API** | Rewrite both tests (`monitor_owns_fake_agent_and_records_devflow_result`, `advance_state_loading_fails_cleanly_for_missing_and_corrupt_state`) against the new socket-based spawn/liveness API. This is the natural home to add the new GONE/STALE/ALIVE regression coverage (see Validation Architecture). |
| `crates/devflow-core/tests/devflow_dir_gitignore.rs` | `:56`, `:109` (comments); `:115` (functional: `monitor::spawn_monitor_no_advance(...)`); `:121`, `:130` (comments/assertions) | **Test file — functional call against a function being deleted (23d)** | This test currently exercises `.devflow/` directory-creation coverage via the no-advance variant specifically. Since `spawn_monitor_no_advance` is deleted, this test must be repointed at the surviving spawn function (whatever `spawn_monitor` becomes) to keep its actual coverage goal (all `.devflow`-constructing call sites produce a correctly-`.gitignore`'d directory) intact — do not simply delete the test case, or the "7 independent `create_dir_all` sites" coverage guarantee from Phase 19 silently narrows. |
| `crates/devflow-cli/src/preflight.rs` | `:4`, `:95`, `:174`, `:361` (comments); `:365` (test function name `run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`) | **Doc-comment / test-name references only — no functional call** | Update comment wording to match the new function name(s); the test itself (asserting preflight failure never reaches the spawn step) does not need behavioral changes, only its name/comments to stay accurate. |
| `crates/devflow-cli/src/staleness.rs` | `:288` (comment: "`monitor::spawn_monitor`. A Stale build against DevFlow's OWN workspace...") | **Doc-comment reference only** | Update wording only — `enforce_build_staleness` itself has no functional dependency on the monitor's internals, only calls it "before `monitor::spawn_monitor`" in prose. |
| `crates/devflow-cli/src/test_support.rs` | `:187`, `:218` (comments) | **Doc-comment references only** | Update wording only. |
| `crates/devflow-core/src/monitor.rs` | entire file | **The module being rewritten** | Not a "consumer" — this IS the implementation. Replace `spawn_monitor_inner`'s shell-script body, delete `spawn_monitor_no_advance`/`wait_for_agent_exit` (23d makes them dead code), keep `wait_for_agent_pid` only if the new design still needs a short post-spawn poll (likely superseded by the socket handshake itself). |

**Additional consumer beyond the "~8 files" scope, not double-counted by CONTEXT.md's list but load-bearing for the phase's actual goal (observability):** `crates/devflow-cli/src/commands.rs` — `liveness()` (`:517-526`), `check_dead_agent`/`check_dead_monitor` (`:1770`, `:1793`), and `status`'s PID-based probe (currently `agent_running(agent_pid)` per `main.rs:2917`/`commands.rs`) all key off `state.monitor_pid` today and must be re-pointed at the new socket probe so GONE/STALE/ALIVE actually surface to `devflow status`/`doctor` — this is explicitly called out in CONTEXT.md's "Integration Points" section and is what makes a dead monitor distinguishable from a healthy pause (the phase's core observability goal), even though it wasn't counted in the "~8 files" figure.

## 23d Deletion Inventory (verified against the live tree, 2026-07-25)

`rg -c "sequentagent|Sequentagent|SequentAgent" --type rust crates/` was re-run
this session. **The actual total is 142 references across 11 files — higher
than CONTEXT.md's/ROADMAP.md's recorded "~110 references across 11 files."**
The 11-file count is correct.

**Root cause of the discrepancy, verified directly:** a second, case-sensitive,
lowercase-only pass (`rg -c "sequentagent"`, no alternation) reproduces
CONTEXT.md's exact numbers (`agent_result.rs` 34, `parallel.rs` 28,
`commands.rs` 21, `phase7_cli.rs` 10, `ship.rs` 8, `monitor.rs` 3, total 111 —
matching the recorded "~110" almost exactly). **CONTEXT.md's count was a
lowercase-only search that missed every PascalCase Rust identifier** — the
`Sequentagent` CLI enum variant (`main.rs:159`), the `Command::Sequentagent`
match arm (`main.rs:483`), `SequentagentSlotKind`, and similar. Those
identifiers are real, functional, must-delete references, not noise — so the
higher, case-insensitive count is the correct one to plan against.

| File | CONTEXT.md's count | Verified count (this session) | Notes |
|------|----------------------|----------------------------------|-------|
| `crates/devflow-core/src/agent_result.rs` | 34 | **48** | Largest undercounts here — likely `sequentagent`-slot rendering (`SequentagentSlotKind`, `write_sequentagent_slot`) plus its own test module, both real production+test surface, not comment noise |
| `crates/devflow-cli/src/parallel.rs` | 28 | **40** | This file's own module is largely `sequentagent`'s home; confirm whether the entire file is deleted or only the `sequentagent`-specific functions within it (the `parallel` — N-phases-concurrently — command lives in the same file and must be preserved) |
| `crates/devflow-cli/src/commands.rs` | 21 | **24** | Includes `status`'s `sequentagent_status_renders_*` rendering + tests (`commands.rs:2585` area) |
| `crates/devflow-cli/tests/phase7_cli.rs` | 10 | **10** | Matches exactly |
| `crates/devflow-core/src/ship.rs` | 8 | **8** | Matches exactly |
| `crates/devflow-core/src/monitor.rs` | 3 | **3** | Matches exactly — the doc comment on `spawn_monitor_no_advance` plus its own reference |
| `crates/devflow-cli/src/main.rs` | (implied, "singles") | **4** | The `Sequentagent` CLI variant (`:159`), its dispatch arm (`:483-488`), and the `use parallel::{parallel, sequentagent}` import (`:23`) |
| `crates/devflow-core/tests/devflow_dir_gitignore.rs` | (implied, "singles") | **2** | Comment references to the `spawn_monitor_no_advance` call this test currently exercises |
| `crates/devflow-core/src/git.rs` | (implied, "singles") | **1** | Not yet inspected line-by-line this session — verify at plan time whether this is a functional dependency or a comment |
| `crates/devflow-core/src/agent.rs` | (implied, "singles") | **1** | Not yet inspected line-by-line this session — verify at plan time |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 1 (`retry_after_from_reason` per the process-teardown research doc's open-decision #2) | **1** | **Load-bearing exception, do not delete this one:** `2026-07-24-process-teardown-solution-research.md` §6 explicitly warns `retry_after_from_reason` "must *move*, not be deleted — `pipeline_outcomes.rs:92` uses it for the primary loop's rate-limit auto-resume." Verify at plan time whether this specific reference is that function (in which case it survives, relocated) or an unrelated `sequentagent` mention. |
| **Total** | **~110** | **142** | 32 references higher than recorded; file-count of 11 confirmed accurate |

**Public/documented-contract surface for D-12's v2.0.0 justification — verified this session, full-repo grep beyond `crates/`:**
- `crates/devflow-cli/tests/snapshots/devflow-help.txt:12` — the **committed help snapshot already lists** `sequentagent  Run two agents sequentially on one phase, each in its own worktree`. This is direct, verified evidence the command is real, documented, public CLI surface — regenerating this snapshot after deletion is a required, not optional, part of 23d (and `help_snapshot.rs`'s existing regression test will fail loudly if the snapshot isn't updated, per its design as a CLI-surface guard).
- `README.md` and `CHANGELOG.md` both mention `sequentagent` `[VERIFIED: rg -l "sequentagent" README.md CHANGELOG.md, this session]` — both need a documentation update as part of 23d's scope (a v2.0.0-earning breaking change should not leave stale user-facing docs describing a deleted command). Exact wording changes are a planning-time task, not sized here.

## Runtime State Inventory

> Included because 23b is a rename/refactor-adjacent phase: the on-disk `monitor_pid`-based process handle is being replaced by a `supervisor` socket-path handle, and 23d deletes a published CLI verb.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data (`state.json`) | `monitor_pid: Option<u32>` field (currently the only process handle persisted, `state.rs:66-72`). Must coexist with or be replaced by a new `supervisor: Option<SupervisorHandle>` block (`socket_path`, `agent_pgid`, `agent_start_time`, `boot_id`, per D-09/the spike's "Resulting design"). | **Code edit, not data migration** — every existing field added since 17-01 (`infra_failures`, `preflight_retries`, `monitor_pid` itself, `stop_until`/`stopped`/`stop_reason`) uses `#[serde(default)]` and this phase should follow the identical pattern. See "In-flight-phase behaviour" below for the precise semantics of an old `state.json` missing the new field. |
| Live service config | None found outside this repo's own `.devflow/` directory — DevFlow has no external service (n8n/Datadog/Tailscale-style) configuration to worry about for this phase. | None. |
| OS-registered state | None — no systemd units, launchd plists, pm2, or Task Scheduler entries reference the monitor process. The monitor is a plain child process, not an OS-registered service. | None. |
| Secrets/env vars | None — no secret or env-var *names* reference `monitor`/`sequentagent`; `DEVFLOW_GATE_TIMEOUT_SECS` etc. are untouched by this phase. | None. |
| Build artifacts | The installed `devflow` binary on `PATH` (`/home/linuxbrew/.linuxbrew/bin/devflow`, a symlink to `target/release/devflow`) is a **stale build from 2026-07-23 22:31, reporting `v1.8.0`**, while `Cargo.toml`'s workspace version is already `1.8.1`. | **Rebuild required before 23a's probe** — `cargo build --release` (or equivalent) must run and produce a binary reporting `≥1.8.1` before the probe is meaningful. See "Current binary/version state" below — this is a documented, recurring project pitfall (memory: "Rebuild before re-validating a dogfood fix"). |

**In-flight-phase behaviour across the D-08 upgrade (Claude's-discretion answer, with rationale):**

`state.rs` currently follows one consistent, well-tested pattern for every
field added after the initial design: `#[serde(default)]`, defaulting to a
value that is *distinguishable from a real value* and that downstream
consumers already treat as "unknown, never assume Stuck/Healthy" (see
`state.rs:66-70`'s own doc comment on `monitor_pid: None`, and
`commands.rs:517-526`'s `liveness()` function, whose `None` arm always
resolves to `Liveness::Unknown`, never `Stuck` — this is directly tested by
`liveness_unknown_when_no_monitor_recorded`). **Recommendation: follow this
exact precedent.** Add `supervisor: Option<SupervisorHandle>` with
`#[serde(default)]`. A `state.json` written by a pre-23b binary deserializes
with `supervisor: None`, and every new liveness/stop code path must treat
`None` the same way `monitor_pid: None` is treated today — `Unknown`, never
a hard error, never a false `Stuck`.

The genuinely new risk (not covered by "absent field defaults to None") is
narrower and worth calling out explicitly for the planner: **a phase whose
monitor was spawned by the OLD `sh -c` mechanism (so `state.json` still has
a *populated* `monitor_pid: Some(pid)`) gets its binary upgraded mid-run to
a ≥23b binary that only knows how to probe `supervisor.socket_path`.**
Because D-08 is a big-bang replacement with no dual-path period, the new
binary's `status`/`doctor`/`stop` will find `supervisor: None` and correctly
report `Unknown` — but the *actual* `sh -c` monitor may still be alive,
untracked by anything the new binary can query. This is not a data-corruption
risk (nothing is silently misreported as healthy) but it IS a **silent loss
of control** — an old-style monitor becomes unreachable by any new-binary
command. **Recommendation:** the planner should have `devflow doctor`
explicitly detect this exact shape (`monitor_pid: Some(_)` AND
`supervisor: None`) and surface a distinct, named finding — e.g. "phase N was
started by a pre-supervisor binary; its monitor cannot be queried or stopped
by this binary — locate and signal it manually, or let it complete
naturally" — rather than folding it into the generic `Unknown` bucket. This
is a one-time transition concern (D-08's big-bang has no long-term dual-path
cost) and is cheap to add as one more `doctor` finding alongside the existing
`check_dead_agent`/`check_dead_monitor`.

## Common Pitfalls

### Pitfall 1: Building the superseded cgroup/pgroup design instead of the locked socket design

**What goes wrong:** `.planning/audits/2026-07-24-process-teardown-solution-research.md` is a thorough, well-cited document recommending a *different* design (`process_handle` with `mechanism: "cgroup" | "pgroup"`) than the one CONTEXT.md's D-09 locks in.
**Why it happens:** Both documents are dated 2026-07-24, both are canonical-refs-listed, and a planner skimming rather than reading D-09 closely could reasonably pick up the cgroup design as "the plan."
**How to avoid:** Treat `2026-07-24-socket-supervisor-spike.md` (and D-09's explicit "already decided — do not re-open" language) as authoritative for the design; treat the two lifecycle/teardown-research docs as background on failure modes (F1-F7) and ruled-out crates only.
**Warning signs:** Any plan task that mentions `cgroup.kill`, `cgroup.procs`, or `mechanism: "cgroup" | "pgroup"` in `state.json` is building the wrong design.

### Pitfall 2: Threading `--yes-ship` as a CLI-only value instead of persisted state

**What goes wrong:** The Ship gate (`handle_ship_outcome`, `pipeline_outcomes.rs:275-286`) may fire long after the original `devflow start --phase N --yes-ship` invocation exited — potentially across a monitor restart, a `devflow resume`, or (post-23b) a fresh supervisor process. A CLI-only flag captured only in the original process's memory is gone by the time Ship's gate fires.
**Why it happens:** `--force`, `--dry-run`, and most other `Start` flags ARE CLI-only and consumed immediately, so it is the natural first instinct to treat `--yes-ship` the same way.
**How to avoid:** Persist the authorization on `State` at `State::new()` time (a new `yes_ship: bool` field, `#[serde(default)]` — false for anything that predates it), exactly like `mode`/`stop_until` are persisted per-phase-run values, not global config. This does not violate D-05 ("never config-persistable") — D-05 is about `devflow.toml`/env-var defaults becoming a standing setting, not about a single run's own `state.json` remembering the flag that run was given.
**Warning signs:** A plan task that reads `--yes-ship` only inside the `Command::Start` match arm and never touches `state.rs`.

### Pitfall 3: Auto-answering the wrong gate

**What goes wrong:** There are (at least) two gates that touch Ship: the primary "Ship complete — approve merge?" gate at `handle_ship_outcome` (`pipeline_outcomes.rs:276-280`), and a **separate** finalization-retry gate inside `finish_workflow_with_gate_timeout` (`pipeline_gate.rs`, fired only when the terminal hooks — Merge/VersionBump/ChangelogAppend/BranchCleanup — fail after the first approval). D-06 says "the Ship gate" (singular) auto-answers; it does not say the finalization-retry gate should also be silently pre-approved.
**Why it happens:** Both gates use `Stage::Ship` as their tag, so a naive `if stage == Stage::Ship && state.yes_ship` check in `run_gate_with_timeout` would auto-answer both, including the one that exists specifically because something already went wrong (a git/version error mid-finalization).
**How to avoid:** Scope the auto-answer narrowly to the call site in `handle_ship_outcome`, not to `run_gate`/`run_gate_with_timeout` generically by stage tag. Recommend either a dedicated wrapper (`run_gate_auto_approved`) called only from `handle_ship_outcome`, or an explicit extra parameter distinguishing "the routine pre-merge approval" from "a post-failure finalization retry."
**Warning signs:** A single boolean check keyed only on `stage == Stage::Ship` anywhere inside `pipeline_gate.rs`.

### Pitfall 4: Forgetting the `advance` tail's env/adapter propagation when moving it in-process

**What goes wrong:** Today's shell monitor rides adapter-scoped env vars (e.g. Codex's unsigned-commit override) through the whole chain: `sh → agent → its git children` (`monitor.rs:168-170`, R-F in the spike). Moving `advance` in-process removes a shell hop but the supervisor process itself must still have inherited/threaded the same env for anything `advance` does that shells out to git.
**Why it happens:** It's easy to focus the migration on "the agent still needs the env" and forget that `advance`'s own git operations (checkout hooks, commits) run in the same process now, inheriting whatever the supervisor process's environment happens to be at that point — which may differ from what a fresh `devflow advance` subprocess invocation would have had.
**How to avoid:** Audit what env `advance`'s call chain (`transition` → `run_checkout_hooks` → git operations) currently expects when invoked as a fresh CLI process vs. as a function call inside the long-lived supervisor process that already launched the agent with `adapter.extra_env()`.
**Warning signs:** A test that passes when `advance` runs as `devflow advance` from a clean shell but fails/differs when driven through the new in-process path.

### Pitfall 5: Believing `--until ship` already gives you `--yes-ship`-equivalent behavior

**What goes wrong:** `main.rs`'s existing `--until` handling has a special case: "`--until ship` is a semantic no-op... Ship is already the pipeline's terminal stage." This is unrelated to `--yes-ship` — `--until` controls where the pipeline **stops**, not whether Ship's gate auto-approves. Do not conflate the two flags or assume one subsumes the other.
**Warning signs:** A plan task that tries to reuse `--until`'s Ship handling to implement `--yes-ship`.

### Pitfall 6: Validating a dogfood fix (or running the 23a probe) against a stale binary

**What goes wrong:** The binary on `PATH` at research time (`/home/linuxbrew/.linuxbrew/bin/devflow`, a symlink into `target/release/devflow`) reports `devflow 1.8.0` and was built 2026-07-23 22:31 — **before** the workspace's own `Cargo.toml` version was bumped to `1.8.1`. CONTEXT.md's requirement of "≥v1.8.1 binary" for 23a is not currently satisfied.
**Why it happens:** `cargo install --path` / a release build is a manual step that is easy to forget after a version bump that only touched `Cargo.toml`/`CHANGELOG.md` in a docs-only commit.
**How to avoid:** `cargo build --release --workspace` (or `cargo install --path crates/devflow-cli --force`) immediately before running 23a's probe, then re-verify `devflow --version` reports `≥1.8.1` before proceeding. This is a previously-documented, recurring project pitfall (own project memory: "Rebuild before re-validating a dogfood fix").
**Warning signs:** `devflow --version` printing `1.8.0` (or any version behind `Cargo.toml`'s `[workspace.package].version`) right before 23a is attempted.

## Code Examples

### Verified STALE-vs-ALIVE probe (the mechanism 23b must reproduce in production code)

```rust
// Source: .planning/spikes/socket-supervisor/main.rs:163-175 (verified to build
// and run standalone, rustc 1.97.1, 2026-07-24)
fn status(sock: &str) {
    if !Path::new(sock).exists() { println!("GONE"); return }
    match UnixStream::connect(sock) {
        Ok(mut s) => {
            let _ = writeln!(s, "ping");
            let mut r = String::new();
            let _ = BufReader::new(s).read_line(&mut r);
            println!("ALIVE ({})", r.trim());
        }
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => println!("STALE"),
        Err(e) => println!("UNKNOWN {e}"),
    }
}
```

### `--yes-ship` auto-answer (grounded in existing production API, not the spike)

```rust
// Existing production API this phase reuses verbatim (crates/devflow-core/src/gates.rs):
//   Gates::write_gate(project_root, phase, stage, context) -> writes the request
//   Gates::respond(project_root, phase, stage, &GateResponse) -> writes a response
//   Gates::poll_response(...) -> already polls for and picks up whatever response exists,
//                                 including one written milliseconds ago by ourselves.
//
// Recommended shape for handle_ship_outcome's auto-approve path (pipeline_outcomes.rs:275-286):
pub(crate) fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    if state.yes_ship {
        // D-06: still fires the gate and still records an explicit decision —
        // this is Gates::respond, not a bypass of run_gate's event/notify emission.
        Gates::write_gate(project_root, state.phase, Stage::Ship, "Ship complete — approve merge?")?;
        Gates::respond(project_root, state.phase, Stage::Ship, &GateResponse {
            approved: true,
            note: Some("pre-authorized via --yes-ship".to_string()),
            responded_by: Some("--yes-ship".to_string()),
        })?;
    }
    match run_gate(project_root, state, Stage::Ship, "Ship complete — approve merge?")? {
        GateAction::Advance => finish_workflow(project_root, state),
        GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}
```
The planner should verify the exact ordering (write-then-respond before
`run_gate`'s own `write_gate` call would double-write; more likely
`run_gate_with_timeout` itself needs the auto-respond injected between its
existing `Gates::write_gate` call and its `Gates::poll_response` call, using
an added parameter rather than duplicating `write_gate`). The sketch above is
illustrative of the *event-shape*, not a literal diff — see Pitfall 3 for why
this must not be a blanket `stage == Stage::Ship` check inside the generic
`run_gate_with_timeout`.

### `WorktreeRemove` hook (reusing an existing primitive, for the `hooks_after_ship` question)

```rust
// worktree::remove already exists and is already called this way from
// crates/devflow-cli/src/commands.rs:278 (cleanup) and parallel.rs:39/350:
worktree::remove(project_root, &path, /* force */ true)?;
```
A `Hook::WorktreeRemove` variant added to `hooks_after_ship()` (`hooks.rs:105-111`)
would call this exact function against `state.worktree_path`, matching the
existing `BranchCleanup` hook's tolerance for "already gone" (see
`hooks.rs:127-135`'s handling of an already-deleted branch) rather than
treating a missing worktree as an error.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| `monitor_pid`-based liveness (`kill(pid, 0)`) | Socket-based liveness (`connect()` → GONE/STALE/ALIVE) | This phase (23b) | Distinguishes a dead monitor from a healthy between-stages pause — closes Finding 1/F1/F6 by construction |
| `devflow advance` as a forked tail process | In-process `advance` call from the supervisor | This phase (23b, D-10) | Removes the exact Phase 17 orphaned-tail failure mode |
| `sequentagent` (two-agent sequential handoff) | Deleted; capability intent preserved in DEN-67 for future re-implementation on the supervisor | This phase (23d) | Removes ~142 references, closes DEN-58's untested `wait_for_agent_exit` gap, earns the v2.0.0 slot (D-12) |
| Cgroup-v2-primary / pgroup-fallback teardown design (`2026-07-24-process-teardown-solution-research.md`) | Socket-addressable supervisor (uniform across platforms and containers) | Superseded same-day, 2026-07-24, by the spike | Container-compatible (cgroup v2 fails under rootless podman even with delegation flags, verified on this host); one implementation instead of a Linux/macOS branch |

**Deprecated/outdated:**
- `spawn_monitor_no_advance` / `wait_for_agent_exit` — both existed solely to serve `sequentagent`'s synchronous handoff; both are removed once 23d lands, closing DEN-58's explicitly-untested gap in this exact code path.
- The `sh -c` shell-script monitor body in `monitor.rs:148-160` (the `apid=''; cleanup() {...}; trap cleanup TERM INT; ...` script) — replaced wholesale by 23b, not incrementally patched.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | The exact name/shape of a new `SupervisorHandle` struct field on `State` (`socket_path`, `agent_pgid`, `agent_start_time`, `boot_id`) — the spike's "Resulting design" JSON is illustrative, not a committed Rust type name. `[ASSUMED]` | Architecture Patterns, Runtime State Inventory | Low — the planner will define the actual struct; this is a naming/shape suggestion only, not a locked contract |
| A2 | Whether `commands::advance`'s existing CLI dispatch logic is already separable into a pure "advance this phase" function callable without going through `clap`/`project_root()` resolution, or needs extraction first. Not directly verified by reading every line of `advance()`'s body this session. `[ASSUMED]` | Architecture Patterns Pattern 2 | Medium — if not already separable, 23b's estimate should include a small extraction step before the in-process call can be wired up |
| A3 | macOS `sun_path` limit (104 bytes) and the general macOS-portability claims in the spike are **the spike's own admission**, not independently re-measured this session (no macOS host available). Already correctly marked out-of-scope per CONTEXT.md/ROADMAP.md. `[CITED: socket-supervisor-spike.md, already flagged there as documented-not-measured]` | Standard Stack, Architecture Patterns | None for this phase specifically — explicitly out of scope; risk belongs to a future macOS-verification phase |
| A4 | The precise historical Linear/DEN-58 "Known gaps — read before planning" section content was not independently re-fetched via a Linear MCP tool in this research session (no such tool was available in this agent's toolset). Reliance is on the spike audit document's own "Known gaps / not yet proven" section, which appears to be the same content restated. `[ASSUMED — content equivalence, not independently cross-checked against Linear]` | Package Legitimacy Audit (N/A note), general | Low-Medium — if DEN-58's Linear description carries additional detail beyond the spike doc, the planner should fetch it directly before finalizing the 23b plan |

**If this table is empty:** N/A — see entries above.

## Open Questions (RESOLVED)

*All three were resolved during planning (2026-07-25). Resolutions recorded inline below.*

1. **RESOLVED — Exact ordering of gate write/respond for `--yes-ship`.**
   *Resolution: the wrapper-function shape was adopted — `run_gate_auto_approved`, with exactly one
   call site, plus a named negative test guarding the finalization-retry gate. See `23-09-PLAN.md`.*
   - What we know: `Gates::write_gate` + `Gates::respond` + `Gates::poll_response` together produce the right event shape (gate_fired → gate_resolved with an explicit `responded_by`).
   - What's unclear: whether the cleanest implementation adds a parameter to `run_gate_with_timeout` (auto-respond immediately after `write_gate`, before `poll_response`) or introduces a separate wrapper function called only from `handle_ship_outcome`. Both satisfy D-06; the tradeoff is code reuse vs. avoiding Pitfall 3 (auto-answering the wrong gate).
   - Recommendation: planner picks the wrapper-function shape — it makes "only the primary Ship approval gate is ever auto-answered" true by construction (the finalization-retry gate's call site never invokes the wrapper), rather than true by convention (a boolean check someone could accidentally widen later).

2. **RESOLVED — Whether `commands::advance`'s CLI-facing function already separates cleanly from its core logic** (see A2). Recommendation was: verify at plan time by reading the full body of `advance()` before sizing the 23b in-process-advance task.
   *Resolution: verified at plan time. `advance` is `pub(crate)` in the CLI crate, so the split is by
   crate rather than by extraction — the supervisor loop lives in `devflow-core` (`monitor::serve`)
   and the advance wiring lives in `devflow-cli` (`pipeline_launch::supervise`). Assumption A2 is
   settled. See `23-06-PLAN.md`.*

3. **RESOLVED — Whether 23a's probe should target a genuinely trivial synthetic phase, or a small real backlog item.** CONTEXT.md says "a small real phase" for 23a and reserves the actual acceptance run (D-02) for a "low-stakes phase" in this repo. Recommendation: for 23a (scratch repo), any single-file, single-requirement synthetic phase is sufficient and lower-risk than trying to import a real backlog item into a throwaway repo — the probe is about the supervisor/pipeline mechanism, not the content of the work.
   *Resolution: the synthetic single-task scratch repo was adopted, gated behaviourally on `doctor`
   plus `--dry-run` rather than structurally. See `23-01-PLAN.md`.*

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|-----------|
| Rust toolchain (`cargo`, `rustc`) | Building the ≥v1.8.1 binary for 23a; the whole phase | Yes | rustc 1.97.1 (per spike README) | — |
| `libc` crate | Socket/signal/pgid primitives | Yes, already resolved | `0.2.x` (workspace-pinned `"0.2"`) | — |
| Unix domain sockets (`AF_UNIX`) | The supervisor's durable handle | Yes (Linux host, confirmed by the spike's own successful run on this exact host) | Kernel/OS feature, not a package | — |
| `~/.cache/devflow/` writability | Socket bind location (D-09) | Not explicitly verified this session, but `~/.cache` is a standard user-writable XDG cache dir on this host (Fedora Kinoite) | — | If unwritable, the design has no documented fallback — the planner should add a preflight check with a clear error, not silently fall back to `$TMPDIR`/project-relative paths (both explicitly ruled out by C6) |
| `devflow` binary on `PATH` | 23a's probe ("with a ≥v1.8.1 binary") | **Currently stale** — installed binary reports `1.8.0`, built 2026-07-23, one version behind `Cargo.toml`'s `1.8.1` | 1.8.0 (installed) vs 1.8.1 (workspace source) | Rebuild: `cargo build --release --workspace` or `cargo install --path crates/devflow-cli --force`, then re-verify `devflow --version` |
| Rootless container runtime (podman) | Only relevant if the cgroup design were chosen | N/A — not needed; the socket design is the locked one | — | — |

**Missing dependencies with no fallback:**
- None identified as blocking, once the binary is rebuilt.

**Missing dependencies with fallback:**
- The stale installed `devflow` binary — fallback is a rebuild (see above), not a blocker once done.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust's built-in `cargo test` harness (no separate framework/assertion crate) `[VERIFIED: .planning/codebase/TESTING.md, cross-checked live: 541 tests currently pass across 13 binaries, cargo test --workspace, 2026-07-25]` |
| Config file | none — behavior driven by `.github/workflows/ci.yml`'s three jobs (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`) |
| Quick run command | `cargo test -p devflow <filter>` (CLI) / `cargo test -p devflow-core <filter>` (core) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req (unit) | Behavior | Test Type | Automated Command | File Exists? |
|------------|----------|-----------|---------------------|---------------|
| 23a | Probe records exact failure point in a scratch repo | Manual/behavioral — not automatable (the entire point is running the real unattended pipeline once) | N/A — the probe itself IS the test; assert on `events.jsonl` content + `.devflow/phase-N-*` captures per D-03 | N/A |
| 23b (socket mechanism) | GONE/STALE/ALIVE liveness with no PID; whole-tree teardown incl. severed-ppid orphans; takeover safety | unit + integration | `cargo test -p devflow-core -- monitor::` (new tests replacing the current `spawn_monitor`-based ones in `monitor.rs`'s own `#[cfg(test)] mod tests`) | ❌ Wave 0 — current tests exercise the shell-script monitor and must be rewritten against the new implementation, not merely extended |
| 23b (state round-trip) | `supervisor: Option<SupervisorHandle>` round-trips through serde; absent-field defaults correctly | unit | `cargo test -p devflow-core -- state::tests` | ❌ Wave 0 — new field, new tests, following the exact pattern of `monitor_pid_round_trips_through_serde` / `monitor_pid_absent_from_json_defaults_to_none` (`state.rs:312-345`) |
| 23b (advance in-process, D-10) | Natural agent exit triggers `advance` without a forked subprocess | integration | Extend or succeed `crates/devflow-core/tests/monitor_e2e.rs` (see below) | ⚠️ Partial — existing file covers the OLD mechanism; needs new or replaced test cases |
| 23c (`devflow stop`) | Explicit stop suppresses `advance` (R-M); idempotent on already-stopped/dead phase; preserves pre-existing `stop_reason` | unit + integration | `cargo test -p devflow -- stop` (new) | ❌ Wave 0 |
| 23d (`sequentagent` removal) | CLI no longer accepts `sequentagent`; help snapshot updated; no dangling references | integration (regression guard) | `cargo test -p devflow -- help_snapshot` (existing, `tests/help_snapshot.rs`) + `rg -c sequentagent crates/` returns 0 | ✅ existing guard, needs its committed snapshot regenerated |
| `--yes-ship` | Gate fires, auto-responds, records `responded_by`; only the primary Ship gate is affected, not the finalization-retry gate | unit + integration | `cargo test -p devflow -- pipeline_outcomes::tests` (extend existing Ship-gate tests, e.g. near `pipeline_outcomes.rs:1514-1576`) | ⚠️ Partial — existing Ship-gate test scaffolding exists; needs new `yes_ship` cases |
| `--yes-ship` (D-05, not persistable) | `devflow.toml`/env var cannot set `yes_ship` | unit | New test in `config_parse.rs` or `main.rs` asserting no config/env path sets it | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** targeted `cargo test -p devflow-core -- <module>` / `cargo test -p devflow -- <module>` for the module just touched.
- **Per wave merge:** `cargo test --workspace` (full suite, currently 541 tests / 0 failed at time of research) plus `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`, matching CI exactly.
- **Phase gate:** Full suite green, PLUS the 23a scratch-repo probe run (D-03's recorded artifact), PLUS the D-02 self-hosted acceptance run, before `/gsd-verify-work`.

### What can only be validated by the actual dogfood run

`cargo test` proves the supervisor mechanism (liveness state machine,
teardown, `state.json` round-trip) and the gate-recording mechanism for
`--yes-ship`. It **cannot** prove the phase's actual acceptance criterion —
"one phase driven start-to-finish by devflow, unattended, reaching a
completed Ship stage" — because that requires a real Claude Code invocation
consuming real GSD slash commands over real wall-clock time, which is
exactly what 23a (scratch probe) and the final D-02 self-hosted acceptance
run exist to exercise. `crates/devflow-core/tests/monitor_e2e.rs` is the
right *pattern* to extend (it already fakes the agent binary and asserts on
captured stdout/exit files) but is not a substitute for the real run — it
uses a fake `sh`/echo agent, not Claude. Recommend: `monitor_e2e.rs` gets new
test cases for the rewritten supervisor mechanism (fake-agent, fast,
deterministic); the actual unattended-with-Claude proof stays entirely
outside `cargo test`, captured instead as the D-03 recorded artifact (events
+ captures) from 23a, and again from the final self-hosted acceptance run.

### Distinguishing "healthy between-stages pause" from "silent stall" — the core observability requirement

This is the literal problem statement (Finding 1) the phase exists to solve,
so the validation plan must assert on it directly, not assume it:

- **What must be observed:** after 23b, `devflow status`/`doctor` must be
  re-pointed at the socket probe (GONE/STALE/ALIVE) instead of
  `agent_running(monitor_pid)`. A regression test should assert that a
  **STALE** socket (monitor process killed without cleanup, socket file
  left behind) renders as a distinct, actionable state — not silently
  folded into the same bucket as a monitor that legitimately hasn't been
  spawned yet (`GONE`/`Unknown`).
- **Sampling/observation rate for the actual unattended run:** the 23a
  probe and the D-02 acceptance run should be observed by polling
  `devflow status`/`.devflow/events.jsonl` at an interval short enough to
  catch a stage transition (stages here run minutes-to-tens-of-minutes per
  the codebase's own test-timeout conventions) but long enough not to
  interfere — every 30-60 seconds is a reasonable default, mirroring the
  spike's own liveness-poll cadence (30ms in the spike's internal exit-code
  poll, which is far tighter than needed for human/operator-facing
  observation).
- **Evidence the run must capture to count as validated:** per D-03, this
  is not optional — `events.jsonl` excerpts spanning every stage transition
  (`transition`, `stage_launched`, `gate_fired`, `gate_resolved`,
  `workflow_finished`) plus the `.devflow/phase-N-*` capture files (stdout,
  stderr, exit code, agent-pid) for at least the stage where the run
  succeeds or first fails. A run that "seems to have worked" without this
  evidence trail does not satisfy the phase's own behavioral acceptance
  criterion.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|---------------------|
| V2 Authentication | No | No user-facing authentication surface changes in this phase |
| V3 Session Management | No | N/A — DevFlow has no session concept beyond its own phase state machine |
| V4 Access Control | Yes | The new Unix domain socket must be mode `0600` (already proven in the spike, `main.rs:80`) so only the owning user can connect and issue `shutdown`/`ping` — anyone who can connect can stop the phase (R-L in the spike's own parity table) |
| V5 Input Validation | Yes | The socket protocol is line-based text (`ping`/`shutdown`); the supervisor must reject/ignore unrecognized commands (already proven: spike's `o => writeln!(s, "unknown {o}")` fallback) rather than executing arbitrary input |
| V6 Cryptography | No | No cryptographic material introduced by this phase |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| A local, unprivileged user on a shared multi-user host connects to another user's supervisor socket and issues `shutdown` | Denial of Service | Socket file permissions `0600` (owner-only), already proven in the spike; verify this survives the production port, since `UnixListener::bind` followed by `set_permissions` has a brief TOCTOU window between bind and chmod — recommend binding into a directory that is itself `0700` (the `~/.cache/devflow/` directory itself, not just the socket file) to close that window |
| A recycled PID is mistaken for the original agent/monitor process during the STALE-path pgid backstop | Spoofing / Tampering | `start_time` + `boot_id` validation before any `killpg`, exactly as designed in the spike/D-09 — do not skip this validation as an optimization |
| `--yes-ship` silently becomes a standing default via config/env, defeating its own "explicit per-run" safety property | Elevation of Privilege (of a sort — an unattended process gains merge authority it shouldn't have by default) | D-05's own constraint: no `devflow.toml`/env-var path may set it; must be a CLI flag consumed once per invocation and persisted only in that run's own `state.json` |
| A malformed or adversarial line sent to the socket (neither `ping` nor `shutdown`) is misinterpreted as a command | Tampering | Explicit match with a catch-all `unknown` response (already in the spike) — the planner must carry this exact fallback into production code, not assume only well-formed input arrives |

## Sources

### Primary (HIGH confidence)
- `.planning/audits/2026-07-24-socket-supervisor-spike.md` — read in full this session; authoritative design (C1-C6, R-A..R-M, resulting `state.json` shape)
- `.planning/spikes/socket-supervisor/main.rs` and `README.md` — read in full this session; the actual proof-of-mechanism code
- `.planning/audits/2026-07-24-process-lifecycle-problem-definition.md` — read in full this session; failure-mode catalog F1-F7, ruled-out crates
- `.planning/audits/2026-07-24-process-teardown-solution-research.md` — read in full this session; superseded cgroup/pgroup recommendation, but authoritative for ruled-out crates and empirical container findings
- `.planning/audits/2026-07-24-scope-creep-complexity-review.md` — read in full this session
- `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` — read in full this session (Findings 1-3)
- `.planning/ROADMAP.md` § Phase 23 — read in full this session
- `.planning/phases/23-end-to-end-dogfood/23-CONTEXT.md` — read in full this session
- Live source reads this session: `crates/devflow-core/src/monitor.rs`, `state.rs`, `mode.rs`, `gates.rs`; `crates/devflow-cli/src/pipeline_launch.rs`, `pipeline_gate.rs`, `pipeline_outcomes.rs` (excerpts), `staleness.rs`, `preflight.rs`, `main.rs`, `commands.rs` (excerpts), `hooks.rs` (grep), `agent_result.rs` (grep)
- Live command output this session: `rg` counts for `spawn_monitor`/`wait_for_agent_*` call sites and `sequentagent` references; `cargo test --workspace` (541 passed, 0 failed); `devflow --version` (1.8.0, stale) vs `Cargo.toml` version (1.8.1)

### Secondary (MEDIUM confidence)
- `.planning/codebase/TESTING.md`, `.planning/codebase/CONVENTIONS.md` — read in full this session, house conventions

### Tertiary (LOW confidence)
- macOS-specific claims throughout the spike/audit docs — explicitly self-flagged there as documented, not measured; not independently re-verified this session (no macOS host); correctly out of scope per CONTEXT.md

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies; existing pins verified live against `Cargo.toml`
- Architecture (socket supervisor design): HIGH — spike-proven and re-read in full this session; the one open item (Pitfall 1's document inconsistency) is called out explicitly rather than silently resolved
- `--yes-ship` threading: HIGH — exact call site (`pipeline_outcomes.rs:275-286`) and exact reusable API (`Gates::write_gate`/`Gates::respond`) both verified live this session
- 23b/23d inventories: HIGH — both re-counted live via `rg` this session; 23d's actual count (142/11 files) differs from CONTEXT.md's recorded ~110, documented as a correction, not a guess
- Pitfalls: HIGH — each grounded in either a specific source line read this session or a specific project-memory note, not generic training-data pattern-matching
- macOS/Linear-issue-content equivalence: LOW/MEDIUM — explicitly logged in Assumptions

**Research date:** 2026-07-25
**Valid until:** ~7 days (fast-moving — this is an active, currently-being-planned phase in a solo-maintained repo where the underlying source can change daily; re-verify call-site counts and installed-binary version immediately before planning if more than a few days pass)

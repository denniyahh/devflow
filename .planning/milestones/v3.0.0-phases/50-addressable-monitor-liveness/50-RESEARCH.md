# Phase 50: Addressable Monitor Liveness - Research

**Researched:** 2026-10-14 (resumed session; HEAD `b3c0c11` on `workspace/denniyahh`, crates identical to `origin/develop`)
**Domain:** Unix-domain-socket process supervision in Rust (`std::os::unix::net` + `libc`), Linux process groups, signal handling, DevFlow monitor/advance/stop/doctor wiring
**Confidence:** HIGH for mechanism (probed on this host with negative controls), MEDIUM for wiring (reasoned from source), LOW where tagged `[ASSUMED]`

<user_constraints>
## User Constraints (from CONTEXT.md)

Copied from `50-CONTEXT.md`. Where a D-decision and the R-block disagree, the R-block wins. D-20 retires D-13, D-14,
D-15 and D-19; no 2.x→3.x migration is researched here.

### Locked Decisions

#### Carried forward (Operator, Phase 23 — not re-opened)

- **D-01 (Operator, Phase 23 D-09): Socket location and probe.** The socket lives under `~/.cache/devflow/`, not
  `$XDG_RUNTIME_DIR` (systemd deletes it at logout). Its path is stored in `state.json`, never derived at probe time.
  Liveness is `connect()`: no socket file → GONE; file present but `ECONNREFUSED` → STALE; connected → ALIVE. A path
  that would exceed `sun_path` (108 bytes Linux, 104 macOS) is refused at construction with a named error (ROADMAP
  criterion 3). — **Reversibility:** costly — the path is persisted in `state.json`, so moving it later strands the
  handles of every in-flight phase.
- **D-02 (Operator, Phase 23 D-10): The `advance` tail runs in-process.** `PipeOwning` already does this through
  `run_monitor` (`crates/devflow-cli/src/pipeline_launch.rs:918`). The `Legacy` replacement must do the same, so no
  forked `devflow advance` tail process exists to orphan.
- **D-03 (Operator, Phase 23 D-08): Big-bang replacement, no feature flag.** The `sh -c` script path is deleted, not
  kept behind a toggle. That decision was made before `PipeOwning` existed (Phase 31); it is consistent with
  criterion 2, which calls `Legacy` "a genuine replacement". — **Reversibility:** one-way — reverting restores a
  process model the rest of the phase has been rewritten against.

#### Supervisor scope (Operator, 2026-10-10)

- **D-04 (Operator): The full spike design, not liveness only.** The supervisor's socket answers both a liveness
  ping and a `shutdown` request. Three parts:
  - **Shutdown (ALIVE path).** The supervisor kills its agent's process group, waits a grace period, escalates to
    SIGKILL, records exit code 143, **suppresses `advance`** (a stop is not a completion, spike R-M), removes the
    socket, exits. `devflow stop` uses this whenever the probe answers ALIVE. This closes 999.138 (`stop` signals
    nothing while no per-phase lock is held: `stop_via_lock`, `crates/devflow-cli/src/commands.rs:2039-2043`,
    verified).
  - **STALE backstop.** When the socket is STALE, teardown enumerates the recorded agent process group and `killpg`s
    it, only if the group is non-empty **and** the recorded start time and boot id still validate. POSIX forbids pgid
    reuse while a group is non-empty, and enumeration is the non-emptiness check (spike C5).
  - **Sweep.** `~/.cache/devflow/*.sock` can be listed and probed with no state file (spike R-J).

  The operator chose this over "shutdown + backstop" and "liveness + shutdown" after a pros/cons pass. Recorded
  against it: the backstop is the riskiest code in the design (it kills by a value read from disk); STALE frequency
  in the field is thin evidence (two silent monitor deaths in Phase 17, none in Phase 23's 11 launches, unmeasured in
  Phase 49); whether an agent outlives a dead monitor is untested for either variant.
  — **Reversibility:** costly — `state.json` gains the backstop fields (pgid, start time, boot id), and `stop`,
  `doctor`, `recover` are rewired onto the socket.
- **D-05 (Operator): SIGTERM / SIGINT to the supervisor behaves exactly like a socket `shutdown`.** Kill the agent
  group, record 143, suppress `advance`, remove the socket, exit. Applies to both variants. Closes 999.134 (the
  `PipeOwning` monitor installs no handler and puts the agent in its own group, `crates/devflow-core/src/monitor.rs:1009`
  `.process_group(0)`, verified — so a TERM orphans the agent every time). The `Legacy` script's existing trap
  (`monitor.rs:544`, verified) is the behaviour being preserved, minus the advance it runs today.
- **D-06 (Operator): The sweep reports; only an explicit verb acts.** `doctor` lists ALIVE and STALE sockets that have
  no matching loadable state. An explicit operator verb (`stop` or `recover --clean`, exact verb is Claude's call) tears
  a named one down, with STALE going through the guarded backstop. **No read-only command kills a process or removes a
  socket.**

#### Folded backlog (Operator, 2026-10-10)

- **D-07 (Operator): 999.141 is folded and is a prerequisite.** E2E tests must point `HOME` (and `XDG_CACHE_HOME`)
  at a tempdir per child `Command`, and own and reap any monitor they launch. The sweep reads `~/.cache/devflow`, so
  its tests are unsafe until this lands. Sequence it first. The one-off cleanup of the operator's real cache was done
  on 2026-10-09 (999.141 entry); do not repeat it without asking.
- **D-08 (Operator): 999.128 is folded.** Lock reclaim compares the recorded start time, so a live pid with a
  different start time counts as stale. It must stay serialised through the existing reclaim coordination
  (`stale_reclaim_is_serialized_before_it_removes_the_lock_path`). Uses the same identity primitive as the backstop
  (`agent::process_start_time` / `agent::is_same_process`, `crates/devflow-core/src/agent.rs:214,228`, verified).
- **D-09 (Operator): 999.139 is folded.** When a state file exists but will not load, `start`'s live-run guard
  consults the sweep for this root and phase, and refuses if it finds an ALIVE (or STALE with a live group) socket,
  pointing at the repair. Today `refuse_start_over_a_live_run` reads `monitor_pid` only from a state that loads.
  Needs the failing test the 999.139 entry names (corrupt state plus a live monitor). Note: the sweep matches a socket
  to a root and phase only if that pairing is recoverable from the socket's name or a sidecar, because the state that
  would record it is the thing that will not load. How is Claude's call (see Discretion).
- **D-10 (Claude, remit, stated during discussion; the operator did not object): 999.134 and 999.138 are closed by
  D-05 and D-04.**
- **D-11 (Operator): 999.130 (split-lock state overhaul) is promoted to its own phase, after 50 and before 51.** Its
  revisit trigger fired: moving `Legacy`'s advance in-process reworks the monitor-to-`advance` handoff. Phase 50's
  in-process advance keeps taking the existing per-phase lock exactly as `advance()` does today; it does not
  introduce the `update_state` API or the ownership token.
- **D-12 (Operator): 999.143 stays parked.** Phase 50 removes its monitor-pid half by construction. The agent-pid
  identity and the work-preserving repair stay in the backlog.

#### No 2.x migration (Operator, 2026-10-12)

- **D-20 (Operator, 2026-10-12; retires D-13, D-14, D-15 and D-19).** DevFlow has no outside users, so Phase 50
  builds no 2.x → 3.x migration. 3.x makes no promise about a `state.json` written by 2.x: it may read as GONE or fail
  to load, and either is acceptable. There is no `schema_version` field, no "incompatible" report, no refusal on a 2.x
  state, no upgrade verb and no 2.x compatibility test. ROADMAP criterion 4 is retired with it. Operational rule
  instead of code: install a 3.x binary only while no 2.x run is live. The retired decisions and their alternatives
  remain in `50-DISCUSSION-LOG.md` and in git history.

#### Proof bar (Operator, 2026-10-10)

- **D-16 (Operator): Tests plus discriminating fixtures, plus the full Phase 49 live rig.** The criterion-1 fixtures
  each produce exactly one of GONE / STALE / ALIVE and nothing else, and the same holds for both `MonitorLaunch`
  variants (criterion 2). On top of that comes a full disposable-project run to Ship on the new binary, reusing Phase
  49's rig (`scripts/scratch-dogfood-repo.sh`, `49-…/tools/`): a fresh private throwaway GitHub repo per attempt,
  `--agent claude`, the seeded `blocking-human` checkpoint, and `--yes-ship`. The seeded checkpoint's auto-decide
  resume goes through `relaunch_checkpoint_session`, which hardcodes `MonitorLaunch::Legacy`
  (`crates/devflow-cli/src/pipeline_launch.rs:1236-1281`, verified), so the run crosses both variants. *Verified
  2026-10-11:* Phase 49 reached that relaunch (`checkpoint_auto_decided` at
  `49-evidence/attempt-3/collected/devflow/events.jsonl:21`, also in attempt 2). The live run still exercises
  `Legacy` only through a Claude resume; non-Claude parity is a test obligation (R-11).
- **D-17 (Operator): Plus a separate fault arm on the same binary.** It ends before Ship and needs no GitHub repo.
  Steps: start a run, probe ALIVE mid-stage, `stop` through the socket (agent group gone, advance suppressed, GONE).
  Then start again and `kill -9` the monitor, which must give STALE; the explicit teardown verb reaps the group
  through the backstop, giving GONE.
- **D-18 (Operator): Phase 49's failure handling carries unchanged.**
  - Blocking defects are fixed in-phase and the run is repeated. Non-blocking ones go to the backlog as 999.x entries
    (49 D-10).
  - Full attempts are capped at 3, and only a run that ends without reaching Ship counts (49 D-11). The fault arm
    does not count against the cap.
  - Each attempt is one `checkpoint:human-action`, judged afterwards (49 D-18).
  - Work stays within GSD's confines (49 D-19).
  - Every attempt rebuilds the binary and records its identity (49 D-12).
  - Evidence goes through the existing sanitizer (49 D-26).
  - The throwaway repo is deleted by the operator only after verification (49 D-24).

#### Post-review corrections (Claude, remit, 2026-10-11) — these win over the D-block

- **R-1 (socket ownership across stage transitions; replaces the "Socket keying" discretion).** The in-process
  advance spawns the next stage's monitor while the current monitor is still alive and bound (`monitor.rs:480-482`;
  `pipeline_launch.rs:979-992`, verified). Two requirements follow:
  - **Naming.** Sockets are named per launch, not per phase. The persisted `state.json` path is the only handle.
  - **Handoff order.** Stage N stays ALIVE through its advance and answers `shutdown` throughout. Stage N+1 binds and
    publishes its own path to state under the per-phase lock. Only then does stage N unlink its socket and exit.

  The takeover rule still holds: a launcher never unlinks a socket that answers. Binding must be atomic: bind to a
  temporary name, then `rename`, or use an ownership file opened with `O_EXCL`. Two concurrent launchers for the
  same phase must yield exactly one live monitor, proved by a test.
- **R-2 (stop and signal during in-process advance; refines D-04 and D-05).** `advance_for_stage` takes the per-phase
  lock (`pipeline_launch.rs:1629`) and may wait on a gate (`:1830`) inside the monitor process, so the spike's
  `process::exit` on shutdown is not acceptable.
  - The supervisor has one terminal-state owner across three phases: agent running, advancing, and waiting on a gate.
    Shutdown and signals are delivered to it as events, through a self-pipe or a signal thread, never as work done
    inside a signal handler.
  - **While the agent runs:** kill the group, record 143, do not advance (D-04 as written).
  - **While advancing:** never abandon a state write or hold the lock past exit. Stop at the next safe point, release
    the lock, and mark the state stopped. Do not launch the next stage.
  - **While waiting on a gate:** the existing `stop_via_gate` path answers.
  - Required test: a deterministic interleaving test proving a stop that races natural completion never advances. Its
    negative control proves that natural completion with no stop still advances.
- **R-3 (`Legacy` process group).** Today the `Legacy` `sh` monitor gets no group of its own (`monitor.rs:571-588`,
  `hermetic_command`). Its agent therefore shares a group inherited from the launching CLI, and a recorded pgid
  could name the operator's group. The replacement must spawn the agent with `process_group(0)` in both arms, and
  must assert `pgid == agent_pid` before recording it.
- **R-4 (backstop when the leader is dead; refines D-04).** `process_start_time` returns `None` for a reaped pid
  (`agent.rs:214-220`). So "the recorded start time validates" refuses exactly when the agent (the group leader) is
  dead and its members live: failure mode F1. The validation rule has two cases:
  - **Leader present:** validate the leader's start time.
  - **Leader gone:** a rule that discriminates the original group from a reused pgid. Research must find it. An
    obvious candidate, "every member started at or after the recorded agent start", does **not** discriminate,
    because a reused group's members also start later. If research finds no sound rule, the backstop refuses for a
    leader-gone group. It then reports the members, and the gap is documented as a residual F1 limit, never a silent
    kill.

  Tests cover leader present, leader gone, and pgid reuse after the original group emptied; the reuse case must not
  be signalled.
- **R-5 (`sun_path` boundary; clarifies D-01 / criterion 3).** The usable maximum is **107 bytes on Linux** (108
  including the NUL; the spike measured first failure at 108, audit `:104`) and **103 on macOS**. Linux tests pin
  both sides: 107 binds, 108 is refused at construction with the named error.
- **R-6 (probe totality).** Only `ECONNREFUSED` means STALE. A missing path means GONE, and a successful connect plus
  a ping reply means ALIVE. Every other `connect()` error (`EACCES`, `ENOTSOCK`, `EAGAIN`, an unlink race) is a
  reported probe error, never STALE, and never triggers the backstop. The spike had a fourth `UNKNOWN` outcome
  (`main.rs:173,199`). This does not add a fourth liveness state. Criterion 1's three states remain the only answers,
  and a probe error is an error.
- **R-7 (sidecar and cache root; makes D-06/D-09 implementable).** Each socket has a sidecar beside it, written
  before the socket is published. It holds the project root, phase, pgid, agent start time, boot id and launch id.
  The sweep, D-09's guard, and the STALE backstop when state will not load all read the sidecar. Both files live
  under `registry::cache_dir()` (`registry.rs:53-64`), which honours `DEVFLOW_CACHE_DIR`, so the D-07 test isolation
  covers sockets too.
- **R-8 (reboot).** A STALE socket whose sidecar boot id differs from the current boot id is certainly dead. Remove
  the socket and sidecar, signal nothing, and report the phase resumable. Test with a fabricated boot id: nothing is
  signalled, and the result is GONE.
- **R-9 (semver; supports the major version).** `pub State::monitor_pid` (`state.rs:288`) may be removed, since no
  2.x file needs to be read (D-20); removing it is a planning decision that must be named, not incidental.
  `pub fn spawn_monitor(...) -> Result<u32, _>` (`monitor.rs:308`) changes shape. Plans list every `pub` item added,
  changed or removed in `devflow-core`, and run `cargo semver-checks check-release -p devflow-core`. The major bump
  rests on those named API changes and the changed monitor mechanism.
- **R-10 (fault arm coverage; refines D-17).** Beyond "kill -9 mid-stage", the fixtures and the fault arm also cover:
  a SIGKILL while advancing (lock held); a SIGKILL between the next monitor's publish and the old monitor's unlink
  (R-1); and a stop racing natural completion (R-2). Each live fault-arm record names the launch shape, socket path,
  probe result, group members before and after, and whether advance ran.
- **R-11 (`Legacy` parity across agents).** The live run pins `--agent claude` and reaches `Legacy` only through
  the Claude checkpoint resume. Criterion 2's real cohort, the non-Claude adapters, is therefore covered by a
  table-driven parity test. That test runs the spike's R-A…R-M responsibilities under both arms with a non-Claude
  adapter. Its negative control proves the test can tell the arms apart.
- **R-12 (D-09 scope).** The corrupt-state refusal goes into the shared `refuse_launch_over_a_live_run`
  (`commands.rs:2470`), which already gates both `start` and `resume`.
- **R-13 (acceptance completeness).**
  - Each folded item keeps its own named failing test with a negative control: 999.128 (the repair at a `Recycled`
    holder succeeds), 999.139 (corrupt state plus a live monitor), and 999.141 (a child writing outside its tempdir
    `HOME` fails the test).
- **R-14 (corrections in place).** Fixed: the folded-item count (five, not three), citations (`spawn_agent_and_record`
  at `:1007`, `spawn_monitor` at `:308`), and D-16's "unverified" Phase 49 relaunch claim (verified).

#### Scope changes planning must carry as plan tasks (from CONTEXT `<domain>`)

- Amend ROADMAP Phase 50's entry so its criteria and requirement list name what this context adds: socket teardown,
  STALE backstop, sweep, signal handling, and the folded items 999.128, 999.134, 999.138,
  999.139, 999.141. Mark those backlog entries as folded into Phase 50.
- Update 999.143 to say the monitor-pid half is removed by Phase 50; the agent-pid half and the work-preserving
  repair stay in the backlog (D-12).
- 999.130 is promoted to its own phase between 50 and 51 (D-11). That insertion is an operator-decided roadmap
  action done through `/gsd-phase --insert`, not a Phase 50 plan task. *(Research note: already done — ROADMAP
  `### Phase 50.1: Split-Lock State-Handling Overhaul (INSERTED)` exists at HEAD `b3c0c11`.)*

### Claude's Discretion

- **Socket keying.** *(Replaced by R-1: per-launch naming.)* The spike used a hash of project root plus phase (for
  example `…/devflow/ab12cd34-22.sock`, 39 bytes). Per-phase versus per-launch naming is Claude's call, under two
  constraints: the takeover rule (a new monitor never steals a live socket; only a STALE one may be reclaimed), and
  D-09's need to map a socket back to a root and phase without loading state.
- **Liveness presentation.** How GONE / STALE / ALIVE maps onto today's `Liveness` enum (`Healthy`,
  `BetweenStages`, `Stuck`, `Unknown`: `crates/devflow-cli/src/commands.rs:959-1025`), its consumers (`cleanup` `:907`,
  `status` `:1127`, `doctor` via `build_phase_facts` `:3932` and `check_dead_monitor` `:3818`), and `doctor --json`.
  With advance in-process, `BetweenStages` likely folds into ALIVE; GONE while state says a stage is running is a
  finding, not a normal state.
- **Wire protocol** (ping and shutdown framing), socket mode `0600` (spike R-L), and grace periods.
- **How the agent pid file and exit file coexist** with the socket; `wait_for_agent_pid` and similar consumers.
- **Group enumeration mechanism** for the backstop on Linux (`/proc`).
- **macOS (Claude, remit).** It was never run here (no host, no CI). The 104-byte limit is enforced by a constant
  and tested as such. **No macOS support is claimed**, and docs and the changelog must say "unverified on macOS".
- **Plan count and wave layout**, with D-07 (test `HOME` isolation) sequenced before any sweep test.

### Deferred Ideas (OUT OF SCOPE)

- **999.130 split-lock state overhaul**: promoted to its own phase between 50 and 51 (D-11). Insert it with
  `/gsd-phase --insert 50 …` after this discussion.
- **999.143 agent-pid half and work-preserving repair**: stays in the backlog (D-12).
- **macOS verification**: no host and no CI; stays unverified (see Discretion). Linked to the deferred
  `chore/macos-ci` work.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SUPV-01 | Monitor liveness is answerable from disk state alone without a PID, in three distinguishable states — GONE, STALE, ALIVE. Both `MonitorLaunch` shapes answer identically; socket under `~/.cache/devflow/`; `sun_path` bound enforced. | Probe classification (Finding 5, verified on this host incl. the regular-file trap), per-launch socket naming and atomic publish via `hard_link` (Finding 2), handoff order (Finding 2), unified `__monitor` supervisor for both arms (Architecture), `sun_path` 107/108 boundary (Finding 4), Validation Architecture rows V-01…V-12 |
| (folded) 999.128 | Lock reclaim by pid + start time | Finding 7a: one-line change site `lock.rs:248` |
| (folded) 999.134 / 999.138 | TERM orphans PipeOwning agent; `stop` cannot end a lock-free run | Finding 3 (self-pipe signal design, verified mask/handler semantics), Finding 8 (stop wiring) |
| (folded) 999.139 | Corrupt state hides a live monitor from the launch guard | Finding 8 (`refuse_launch_over_a_live_run` + sidecar sweep) |
| (folded) 999.141 | Test `HOME` isolation | Finding 7c: 71 spawn sites in 16 e2e files plus in-crate `--bin` tests that reach `registry::register` |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- `devflow-core` is published on crates.io: a `pub` change is a semver change; prefer additive; run
  `cargo semver-checks check-release -p devflow-core` before committing one.
- Test commands: `cargo test -p devflow-core --lib`; `cargo test -p devflow --bin devflow` (package is `devflow`,
  binary-only; `--lib` on it verifies nothing). Full: `scripts/check.sh all`; parity: `scripts/check-in-container.sh`.
- Phases run in worktrees under `.worktrees/phase-N` (`scripts/phase-worktree.sh <phase>`); no concurrent git ops on
  the main checkout while an executor holds it.
- Tooling/CI/hook/devcontainer changes must update `.planning/user/DEV-SETUP-CHECKLIST.md` in the same commit. (This
  research recommends **no** new dependency or tool, so this should not trigger.)
- Global rules that bind plans: no process-global `set_var`/`remove_var` in tests; configure child `Command`s; scrub
  `GIT_*` in fixtures; tempdirs for HOME/XDG; `cargo clippy --workspace --all-targets -- -D warnings`; non-trivial
  negative tests; prefer Rust deps and gate any new one on `cargo deny check`; kill only by recorded PID; bounded
  timeouts on every probe; clean scratch.
- Bare `cargo test <name> -- --exact` with an unqualified name matches nothing and exits 0: module-qualify unit-test
  names and assert on `1 passed` (MEMORY: cargo-test-exact false green).

## Summary

The mechanism is sound and needs **no new dependency**: `std::os::unix::net` plus the existing `libc = "0.2"`
(`0.2.186` in `Cargo.lock`) cover sockets, `pidfd_open`/`pidfd_send_signal` (syscall numbers and the
`PIDFD_SIGNAL_PROCESS_GROUP` constant are exported by libc 0.2.186), `sigaction`, `pipe2`, `poll` and `waitid`.
The probes run on this host (kernel 7.2.5, rustc 1.97.1) settled five planning-relevant facts. Each had a negative
control, and **three of them change the design the CONTEXT implies**:

1. **`connect()` returns `ECONNREFUSED` for a regular file and for a directory at the socket path.** So R-6's "only
   ECONNREFUSED ⇒ STALE" is not enough by itself: the probe must `lstat` first and require `S_ISSOCK` (plus our uid),
   or a stray file reads as STALE and could arm the backstop.
2. **`rename(2)` silently replaces a live socket path; `link(2)` refuses with `EEXIST`.** R-1 lists "bind to a temp
   name, then `rename`" as acceptable. It is not acceptable if the target could exist. Publish with
   `bind(temp) → listen → hard_link(temp, final) → unlink(temp)`. A connect between `bind()` and `listen()` returns
   `ECONNREFUSED` (verified), which is the real reason the final name must appear only after `listen`.
3. **`std::process::Command` does not reset the blocked-signal mask in the child.** A supervisor that blocks
   SIGTERM/SIGINT for a `sigwait` thread passes that mask to every agent, `git` child and next-stage monitor, so the
   agents become immune to the group SIGTERM. Use `sigaction` handlers that write to a self-pipe. Caught
   dispositions reset to default on `exec` (verified), and the mask stays empty.

Two more facts answer R-4 and R-5 directly. R-5: Rust's `UnixListener::bind` accepts 107 bytes and refuses 108 with
`InvalidInput` ("path must be shorter than SUN_LEN"), which carries no errno, so DevFlow needs its own named length
check before binding. R-4: when the leader is reaped, its members keep `pgrp` = the old leader pid and keep the
launcher's session id. The session id equals the operator terminal's session, so it cannot discriminate anything.
**For a leader-gone group no sound rule exists that uses only the values the design records** (pgid, start time,
boot id, sid). The backstop must refuse and report. There is one sound strengthening: if a *different* process now
holds pid == pgid (its start time does not match), the original group is certainly gone (Finding 6).

On the wiring side, the R-1 handoff has to be stated more precisely than the CONTEXT wording allows. Stage N+1 cannot
take the per-phase lock itself, because stage N's `advance_with` holds it through `spawn_agent_and_record`
(`pipeline_launch.rs:1629` → `:1062`). So "N+1 publishes under the lock" has to mean this: **the launcher (stage N's
advance, or the `start`/`resume` CLI) holds the lock, waits for N+1 to answer a ping, and then writes N+1's path to
state.** This is a mechanism refinement within Claude's remit, not a decision change. Separately, `cargo
semver-checks` runs here (0.51.0), and **the tree already fails it** against the published 2.12.0 with 2 major lints
from Phase 49 work. The Phase 50 gate therefore cannot be "semver-checks passes". It has to be "the failing-lint set
equals the named list".

**Primary recommendation:** Both arms become one Rust `__monitor` supervisor. The core holds a terminal-owner state
machine, a control thread that `poll`s {self-pipe, listener}, and newline-JSON `ping`/`shutdown`. Publish the socket
with `hard_link` under `registry::cache_dir()/sock/`, and the launcher records it in state under the lock it already
holds. The STALE backstop is sound only when the leader is present (use `pidfd_send_signal` with
`PIDFD_SIGNAL_PROCESS_GROUP`); otherwise it refuses and reports. Sequence the 999.141 isolation work first.

## Architectural Responsibility Map

DevFlow is a CLI plus detached supervisors; the standard web tiers do not apply. Tiers used here:

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Socket path construction, length check, probe (GONE/STALE/ALIVE), sidecar read/write, sweep, backstop | `devflow-core` (new `supervisor` module) | — | Pure OS mechanism; reused by CLI commands and by the supervisor; testable with `*_in(dir)` paths |
| Supervisor runtime (spawn agent in own group, control thread, terminal owner, exit file, shutdown reply) | `devflow-core` (`monitor`/`supervisor`) | `devflow-cli` `run_monitor` | Shared by both arms; returns `Completed(code)` / `Stopped` so core never calls advance |
| In-process advance, safe points, publishing N+1's path under the lock | `devflow-cli` (`pipeline_launch.rs`) | core `lock`, `workflow` | Advance logic lives in the CLI today (`advance_with`) |
| Liveness presentation (`status`, `cleanup`, `doctor`, `doctor --json`) | `devflow-cli` (`commands.rs`) | core probe | Presentation and repair verbs |
| Teardown verb (`stop`), live-run launch guard | `devflow-cli` (`commands.rs`) | core probe + backstop | Operator verbs |
| Persistence of the handle (`socket_path`, backstop fields) | `devflow-core` `State` (`#[non_exhaustive]`, `#[serde(default)]`) | sidecar JSON under cache dir | State is the handle (criterion 3); the sidecar covers state-less reads (R-7) |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::os::unix::net` (`UnixListener`, `UnixStream`) | rustc 1.97.1 (pinned in `rust-toolchain.toml`) | Socket bind/listen/connect; read/write timeouts | Already the spike's mechanism; the listener fd is `CLOEXEC` [VERIFIED: probe G — child fds `0 1 2 3->pipe 50->socket`; listener fd 3 not inherited, the deliberately non-CLOEXEC `F_DUPFD` copy at 50 was] |
| `libc` | `0.2.186` (lockfile; `libc = "0.2"` in `crates/devflow-core/Cargo.toml`) | `sigaction`, `pipe2(O_CLOEXEC|O_NONBLOCK)`, `poll`, `getpgid`, `waitid(WNOWAIT)`, `syscall(SYS_pidfd_open / SYS_pidfd_send_signal)`, `killpg`, `geteuid` | Already a dependency; no new crate needed. [VERIFIED: `libc-0.2.186/src/new/linux_uapi/linux/pidfd.rs:14-17` — `pub const PIDFD_SIGNAL_THREAD: c_uint = 1 << 0;` `pub const PIDFD_SIGNAL_THREAD_GROUP: c_uint = 1 << 1;` `pub const PIDFD_SIGNAL_PROCESS_GROUP: c_uint = 1 << 2;` — and `libc::PIDFD_SIGNAL_PROCESS_GROUP` compiles from a downstream crate, probe build 2026-10-14] |
| `serde` / `serde_json` | workspace | Sidecar and wire-protocol JSON | Already used for state |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | dev-dep `3` | Tempdir `HOME`/`DEVFLOW_CACHE_DIR` per child; socket fixtures | All tests (999.141) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| libc self-pipe (~40 lines) | `signal-hook` crate [ASSUMED: name and maturity from training; not in `Cargo.lock`, not legitimacy-checked] | A standard Rust crate that does exactly this pattern. It adds a dependency, which means `cargo deny` plus a DEV-SETUP-CHECKLIST touch, and the spike's stated "no new dependency" win is lost. Recommend libc; revisit only if the self-pipe grows beyond one handler. |
| `sigprocmask` + `sigwait` thread | — | **Rejected on evidence:** the blocked mask is inherited by every spawned child (probe A). |
| `rename` publish | `hard_link` publish | `rename` steals a live name (probe B). |
| pgid backstop | cgroup v2 per-run scope (`cgroup.kill`) | The only kernel mechanism that soundly contains a leader-gone tree, but it needs a delegated, writable cgroup or `systemd-run --scope`. The audit (`2026-07-24-process-teardown-solution-research.md`) set it aside and D-04 chose pgid. Out of scope; recorded as the route if the residual F1 gap ever matters. |

**Installation:** none. No package is added.

## Package Legitimacy Audit

No external package is installed by this phase. `signal-hook` appears only as an alternative that is not
recommended. It was not legitimacy-checked and must not be added without a `checkpoint:human-verify` plus
`cargo deny check`.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none added) | — | — | — | — | — | — |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Key Findings (answers to the eight research questions)

### Finding 1 — R-4: a discriminating rule for a leader-gone group

**Answer: no sound rule exists from the values the design records. The backstop must refuse and report for a
leader-gone group.** There is one sound sub-case, and one sound partial extension the planner may choose (not
recommended for Phase 50).

Evidence (probe E, this host, with the probe's own session as the control):

```
leader pid=149154 stat(state,pgrp,sid,start)=Some(('R', 149154, 149129, 335961))
leader reaped; /proc/149154/stat readable=false
member pid=149155 stat(state,pgrp,sid,start)=Some(('R', 149154, 149129, 335961))
member pid=149156 stat(state,pgrp,sid,start)=Some(('Z', 149154, 149129, 335961))
our own sid for comparison=Some(149129)
kill(leader_pid,0) after reap = -1 errno=No such process (os error 3)
kill(-pgid,0) group probe = 0
```
[VERIFIED: probe E, 2026-10-14]

Candidate by candidate:

| Candidate | Survives leader death? | Discriminates original vs reused pgid? | Verdict |
|---|---|---|---|
| `/proc/<pid>/stat` field 5 (pgrp) | yes (verified) | no: equality with the recorded pgid is exactly what a reused group also shows | necessary, not sufficient |
| field 6 (session) | yes (verified) | **no**: without `setsid` the agent's sid equals the launcher's session, which is the operator's terminal session (`sid 149129` = the probe's own sid). Any process in that terminal shares it. `setpgid` also lets any same-session process **join** an existing group. [ASSUMED: POSIX `setpgid` same-session rule, training] | rejected |
| member start time ≥ recorded agent start | yes | no (R-4 already states this; joiners via `setpgid` are unbounded too) | rejected |
| supervisor `setsid()` + recorded sid | yes | no: in the STALE case the session leader (the supervisor) is the dead process, so the same regress repeats one level up | rejected |
| boot id | yes | only across reboots (R-8) | necessary, not sufficient |
| `PR_SET_CHILD_SUBREAPER` | **no**: the subreaper is the supervisor, which is dead in the STALE case | n/a | does not help STALE; useful on the ALIVE path for out-of-group descendants (MEMORY 30-04: `killpg` orphaned two descendants that left the group) |
| pidfd | pidfds cannot be opened for a reaped leader; nobody holds one after the supervisor dies | n/a | helps only the leader-present TOCTOU (below) |
| cgroup membership | yes | **yes**, if a per-run cgroup existed | sound, but a different mechanism (see Alternatives); out of scope |
| periodic snapshot of member `(pid, start_time)` into the sidecar | yes | **yes per process**, since `(pid, starttime)` is unique per boot, but members created after the last snapshot are missed | sound but partial; adds a polling writer to the supervisor. Not recommended for Phase 50 |

**The rule to implement (Linux, `/proc` enumeration over own-uid processes):**

1. Sidecar (and the state copy, if it loads) must agree on `launch_id`, `pgid`, `agent_start_time`, `boot_id`; any
   mismatch → refuse and report.
2. `boot_id` ≠ `/proc/sys/kernel/random/boot_id` → certainly dead. Remove the socket and sidecar, signal nothing,
   GONE (R-8).
3. Enumerate `/proc/*/stat` with `uid == geteuid()` and field 5 == pgid, excluding zombies. Empty → remove the socket
   and sidecar, GONE.
4. **Leader present:** `/proc/<pgid>` exists, its start time == the recorded start time, and its field 5 == pgid.
   This is sound: while the original leader holds pid P, no other process can found group P. →
   `pidfd_open(P)`, re-read the start time **after** opening (the pidfd pins the process, so a match proves the fd
   names the original), then `pidfd_send_signal(fd, SIGTERM, NULL, PIDFD_SIGNAL_PROCESS_GROUP)`, grace, then the same
   with SIGKILL. A send that fails `ESRCH` mid-grace means the leader died, so fall to step 6 for the remaining members.
5. **Leader pid present but its start time differs:** pid P was recycled. A pid number is reusable only once it is
   unused as a PID, PGID and SID [ASSUMED: Linux `free_pid` frees the number only when no task links it as
   pid/pgid/sid; POSIX "process group lifetime" — training, consistent with spike C5], so the original group was
   empty at reuse time and is gone for good. → certainly gone. Signal nothing, remove the socket and sidecar, GONE.
   *(This is the sound strengthening; it also covers R-4's required "pgid reuse after the original group emptied"
   test.)*
6. **Leader gone, members present (no process with pid P, group P non-empty):** ambiguous. The members are either
   the original group or a reused group whose founder already died. → **refuse**. Report each member (pid, start
   time, state, sid, cmdline), name the residual F1 limit, and give a manual inspection command. Never signal.

Verified kernel support for step 4 (probe F, with control):

```
flag 0 (control: leader only): rc=0 ...; member 149871 state after=Some('S')
flag PIDFD_SIGNAL_PROCESS_GROUP: rc=0 ...; member 149878 state after=None
```
[VERIFIED: probe F on kernel 7.2.5. The trailing `os error 2` text in the raw output is a stale `last_os_error`;
`rc=0` is success.] `PIDFD_SIGNAL_PROCESS_GROUP` needs a recent kernel [ASSUMED: added in Linux 6.9, training]. If
the call returns `EINVAL`, fall back to "validate, then `killpg`", and document the microsecond TOCTOU window.

### Finding 2 — R-1: per-launch names, atomic publish, handoff order

**Atomic publish, verified (probe B/B2):**

```
connect final after rename: Ok("ok")
connect old temp path after rename: Err(NotFound, 2)
rename second socket over live final: Ok("REPLACED (rename steals)")
connect via hard-linked name: Ok("ok")
hard_link second socket onto existing name: Err(AlreadyExists, 17)
connect after bind, before listen: Err(ConnectionRefused, 111)
connect after listen (control): Ok("ok")
connect after listener fd closed, file left: Err(ConnectionRefused, 111)
```
[VERIFIED: probe B/B2, 2026-10-14]

So: renaming a bound socket keeps it connectable, **but `rename` replaces a live target**. Use
`UnixListener::bind(<dir>/.<stem>.part)` (bind + listen), then `std::fs::hard_link(part, <stem>.sock)`, which fails
`EEXIST` and so is the O_EXCL equivalent, then `remove_file(part)`. Give temp names a non-`.sock` suffix so the sweep
never probes a half-built socket.

**Naming (Claude remit, recommended):** `<cache_dir>/sock/<root_digest:016x>-<phase.padded()>-<launch_id>.sock`,
with the sidecar `…-<launch_id>.json` beside it and `sock/` at mode `0700`. `phase.padded()` yields `"NN"` or
`"NN.m"` [VERIFIED: `crates/devflow-core/src/phase_id.rs:98-103` —
`Some(minor) => format!("{:02}.{minor}", self.major),` / `None => format!("{:02}", self.major),`]. The FNV digest
helper `path_digest` exists but is private (`registry.rs:88`); expose it `pub(crate)`. `launch_id` = 16 hex chars
from launcher pid ⊕ nanos plus a per-process counter; no RNG crate needed. Length budget for
`/var/home/denniyahh/.cache/devflow/sock/` + 16+1+4+1+16+5 ≈ 83 bytes, under 107. The length check runs on the final
path *and* the `.part` path.

**Handoff order — what the code permits.** Stage N's supervisor runs `advance_for_stage` → `advance_with`, which
takes the lock at `pipeline_launch.rs:1629` and holds it through evaluation, `transition`, `launch_stage` and
`spawn_agent_and_record` (`:1007`). `spawn_monitor` runs at `:1062` and `save_state` at `:1068` [VERIFIED: Read
`pipeline_launch.rs:1007-1110`]. N+1 cannot take the lock while N holds it, so the precise order is:

1. N (holding the lock) picks N+1's `launch_id` and path and spawns `__monitor --socket <path> …`.
2. N+1 installs its signal handler, spawns the agent with `process_group(0)`, asserts `getpgid(agent) == agent`,
   writes the sidecar (temp + rename; sidecars are not sockets, so `rename` is fine), binds `.part`, links `.sock`.
3. N polls `ping` on the path (bounded, e.g. 10 s) and gets the reply's `launch_id`, `agent_pgid`,
   `agent_start_time`, `boot_id`.
4. N writes `state.supervisor = {socket_path, launch_id, agent_pgid, agent_start_time, boot_id}` and saves **under
   its lock**. This is the publish.
5. `advance_with` returns, so the lock drops. N then unlinks its own `.sock` and sidecar and exits.

Between 4 and 5, N is still ALIVE (owner phase `handed_off`), and a `shutdown` it receives must reply `handed_off`
and touch nothing: N+1 owns the run. If step 3 times out or step 4 fails, N sends `shutdown` to N+1's path and
records `stopped` with a reason. Do not leave an unpublished agent running.

**WR-04 pre-clear pitfall:** `spawn_agent_and_record` clears `monitor_pid` and saves *before* spawning
(`pipeline_launch.rs:1023-1024`). Copying that for `supervisor` leaves a window in which `status` reads no socket
(GONE) mid-handoff. Instead, keep N's record until step 4 replaces it in one save, and clear it only on the failure
path.

**Concurrent launchers:** `start` and `resume` both run under the per-phase lock and call
`refuse_launch_over_a_live_run` [VERIFIED: Read `pipeline_launch.rs:1418-1427`]. With the guard probing ALIVE, two
racing launchers serialize on the lock and the loser refuses. The R-1 test is therefore an e2e test: two `devflow
start` children launched together → exactly one ALIVE socket in `sock/`, exactly one agent group, one refusal.

### Finding 3 — R-2: one terminal-state owner, signals as events

**Verified facts:**

```
== A  parent (control, must show bits 0x4002): SigBlk: 0000000000004002
       child via Command::spawn:               SigBlk: 0000000000004002
       child via Command + process_group(0):   SigBlk: 0000000000004002
== G  parent (control, TERM bit 0x4000 must be set): SigCgt: 0000000000004440
       child after exec:  SigBlk: 0000000000000000   SigCgt: 0000000000000400
```
[VERIFIED: probes A and G, 2026-10-14] A blocked mask leaks into children, while caught handlers reset at `exec`.
**Use `sigaction` + self-pipe, not `sigwait`.**

**Design (recommended):**

- At `__monitor` entry, before any spawn: `pipe2(O_CLOEXEC|O_NONBLOCK)`. Store `SUPERVISOR_PID` and the write fd in
  atomics. Install `sigaction` for SIGTERM and SIGINT (`SA_RESTART`). The handler does only
  `if getpid() == SUPERVISOR_PID { write(fd, &signo, 1) }`; both calls are async-signal-safe. The pid guard drops a
  write from a forked child that has not yet exec'd (the Rust analogue of the Legacy script's "Window A",
  `monitor.rs:500-504`).
- **One control thread** `poll(2)`s {self-pipe read end, nonblocking listener}. It turns bytes into `Signal(n)` and
  connections into requests (one newline-JSON line, 4 KiB cap, read and write timeouts).
- **One terminal owner:** `Mutex<OwnerPhase>` with `AgentRunning{pgid}` / `Advancing` / `GateWait` / `HandedOff`,
  plus `stop_requested`. Every transition, including the main thread's "agent exited → advance?" decision, happens
  under this mutex, so a stop and a natural exit are totally ordered.
  - `AgentRunning` + stop/signal → set `stop_requested`, `killpg(pgid, SIGTERM)`, grace, `SIGKILL`. Safe because the
    supervisor has **not reaped** the leader yet. An unreaped child, even a zombie, holds pid and pgid (verified: the
    zombie member in probe E kept its pgrp).
  - `Advancing` → set `stop_requested`. The CLI's safe points act on it.
  - `GateWait` → set `stop_requested`. A cancellable gate poll returns Abort (below). A socket `shutdown` here may
    also reply `{"error":"gate_open"}` so `stop` re-runs `stop_via_gate`. Keep both.
  - `HandedOff` → reply `handed_off`, act on nothing.
- **Main thread:** after the agent exits, lock the owner. If `stop_requested`, write 143, persist `stopped`, and
  return `Stopped`. Otherwise set `Advancing` and return `Completed(code)`. `run_monitor` advances only on
  `Completed`.
- **Safe points inside advance (CLI)**, each under the lock `advance_with` already holds:
  1. right after `load_state` in `advance_with`;
  2. right before `monitor::spawn_monitor` in `spawn_agent_and_record`;
  3. after N+1 answers ping and before publishing, which, if a stop has arrived, sends `shutdown` to N+1 and does not
     publish;
  4. inside the gate wait.

  At a safe point with a stop requested: set `state.stopped = true` plus a `stop_reason`, `save_state`, return (the
  lock drops via the guard), reply `stopped`, unlink, exit. Pass the token to these sites through a
  `thread_local!`-scoped RAII guard set by `run_monitor`, not a process-global static: parallel in-crate tests would
  share a static. Threading an explicit parameter through `launch_stage`'s many callers is the alternative. Planner's
  choice; prefer the scoped thread-local.
- **Gate wait:** `Gates::poll_response` is a sleep loop with 1 s→60 s backoff and no cancellation
  (`crates/devflow-core/src/gates.rs:284-310`, read via shell this session). Add an **additive**
  `poll_response_cancellable(.., cancel: impl Fn() -> bool)` that slices its sleeps to ≤1 s, and treat a cancel as
  `GateAction::Abort`, so the existing `abort()` unwind releases the lock.
- **Who writes `stopped`:** the supervisor, always (single owner). `stop`'s `persist_stopped_state` stays, but becomes
  an idempotent append after it sees the final `stopped` reply or GONE.
- **Residual:** `devflow advance` (a hidden subcommand) never reads `state.stopped` (999.138 text). Recommend
  `advance_with` refuse with an `advance_failed {reason:"stopped"}` event when `state.stopped`.
  [ASSUMED: no legitimate caller advances a stopped state — the planner must grep every `advance_with` caller before
  adding the refusal.]

**Required deterministic test (R-2):** make the owner a pure state machine,
`fn decide(phase, events…) -> Outcome`, and enumerate the interleavings: stop before exit; exit then stop before the
decision; stop after `Advancing` but before safe point 2; stop between spawn and publish; stop during `GateWait`.
Each asserts "next stage not launched, state stopped". Negative control: exit alone → `Advance`, next stage launched.

### Finding 4 — R-5: `sun_path`

```
len=106: Ok("bound")   len=107: Ok("bound")
len=108: Err(InvalidInput, raw=None, "path must be shorter than SUN_LEN")
len=109: Err(InvalidInput, raw=None, ...)
```
[VERIFIED: probe C, 2026-10-14] std refuses at 108 with no errno. So the "named error" must be DevFlow's own:
`SocketPathError::TooLong { len, max }`, checked against `const SUN_PATH_MAX_LINUX: usize = 107` and
`const SUN_PATH_MAX_MACOS: usize = 103` (the macOS value is constant-tested only; "unverified on macOS"). Test both
sides on Linux with a short tempdir base, so `<base>/…` reaches exactly 107 and 108.

### Finding 5 — R-6/R-7/R-8: probe classification, sidecar, boot id

```
missing path: Err(NotFound, 2)
regular file: Err(ConnectionRefused, 111)      <-- trap
directory:    Err(ConnectionRefused, 111)      <-- trap
socket mode 000: Err(PermissionDenied, 13)
live server (control): Ok
SIGKILLed server, file left=true : Err(ConnectionRefused, 111)
```
[VERIFIED: probe D, 2026-10-14]

**Total probe (recommended):**
1. `symlink_metadata(path)`: `NotFound` → **GONE**; any other error → `ProbeError`.
2. Not `S_ISSOCK`, or uid ≠ euid, or a symlink → `ProbeError::NotASocket` (never STALE).
3. `connect`: `ECONNREFUSED` → **STALE**; `ENOENT` (unlink race after lstat) → **GONE**; any other error →
   `ProbeError`.
4. Connected: send `{"op":"ping"}` with 2 s read/write timeouts. A valid reply → **ALIVE(reply)**; a timeout or
   garbage reply → `ProbeError::NoReply` (a listening backlog accepts connects even when nobody calls `accept`, so a
   connect alone is not proof).

No PID is read anywhere in steps 1–4 (criterion 1). The reply carries pids for display only.

**Sidecar** (`…-<launch_id>.json`, mode 0600, written atomically before linking `.sock`):
`{"launch_id","project_root","phase","stage","agent":"<kind>","launch":"pipe_owning|legacy","agent_pid","agent_pgid","agent_start_time","boot_id","supervisor_pid","supervisor_start_time","created_at"}`.
The location is `registry::cache_dir()` [VERIFIED: `crates/devflow-core/src/registry.rs:53-62` —
`if let Some(dir) = std::env::var_os("DEVFLOW_CACHE_DIR") { return Some(PathBuf::from(dir)); }`
`if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") { return Some(PathBuf::from(dir).join("devflow")); }`
`let home = std::env::var_os("HOME")?; Some(PathBuf::from(home).join(".cache").join("devflow"))`]. Provide `*_in(cache_dir)`
variants for every function, as `registry::load_roots_in` already does (MEMORY: hermetic registry pattern).
Criterion 3 says `~/.cache/devflow/`; this resolver puts it under `$XDG_CACHE_HOME/devflow` when that is set.
The operator decided D-01 against `$XDG_RUNTIME_DIR`, not against `$XDG_CACHE_HOME`. See Open Question 1.

**Boot id:** read `/proc/sys/kernel/random/boot_id` and trim [VERIFIED: readable on this host, value
`9f5f5883-d637-4faa-b05d-bede4dae8d5f`, probe 2026-10-14]. Containers share the host kernel's boot id [ASSUMED].

**R-8 vs D-06:** R-8 says "remove socket and sidecar"; D-06 says no read-only command removes a socket. Reconcile it
this way: `status`/`doctor` *report* "STALE (previous boot) — repair: `devflow stop --phase N`", and only the
explicit verb removes.

### Finding 6 — R-3: process group for both arms

The `PipeOwning` agent already gets `.process_group(0)` [VERIFIED: Read `monitor.rs:1009`]. The Legacy `sh` monitor
gets none [VERIFIED: Read `monitor.rs:571-588`]. In the replacement both arms spawn the agent from the Rust
supervisor with `process_group(0)`, then assert `libc::getpgid(agent_pid) == agent_pid` before writing the sidecar.
On mismatch, kill the child and fail the launch. Also put the **supervisor itself** in its own process group at
spawn (`process_group(0)` on the `__monitor` command), so the chain of supervisors never sits in the operator's
shell job. That keeps the terminal's Ctrl-C to a lingering `devflow start` from reaching an unattended run, and
makes "never signal our own pgrp" checkable. *This is a Claude-remit recommendation. It narrows when D-05's SIGINT
path fires (only a deliberate signal reaches it), so state it in the plan.*

### Finding 7 — Folded backlog: locations and named tests

**7a. 999.128** — the change site is one predicate:
[VERIFIED: `crates/devflow-core/src/lock.rs:240-253` — `fn acquire_after_existing(` … `if crate::agent::agent_running(record.pid) {` `return Err(LockError::Contended {`]
Replace it with `classify_holder_record(record, agent_running(pid), process_start_time(pid))` and reclaim on
`NoHolder | Recycled`. Keep `Live`/`Unconfirmable` contended. The enum values are
[VERIFIED: `lock.rs:47-55` — `NoHolder,` `Live { pid: u32 },` `Recycled { pid: u32 },` `Unconfirmable { pid: u32 },`].
The reclaim stays inside the coordination flock already held by `acquire_after_existing`, so
`lock::tests::stale_reclaim_is_serialized_before_it_removes_the_lock_path` (`lock.rs:751`) keeps covering
serialization. Tests:
- new `lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time` (fixture: write a lock record
  naming this test's own pid with start time `0`);
- control `lock::tests::acquire_never_reclaims_a_live_holder_whose_start_time_matches`;
- **flip** `recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock`
  (`crates/devflow-core/src/recover.rs:1117`), which today pins the defect, so it asserts that cleanup happens;
- the R-13 repair test: `stop`/`recover --clean` at a `Recycled` holder succeeds (CLI e2e in `recover_clean_e2e.rs`).
- Also fix the stale `recover_cmd` comment "clear it regardless of staleness" (`commands.rs` explicit-phase arm, per
  999.128).

**7b. 999.134 / 999.138** — closed by Finding 3 plus Finding 8. Tests:
- `monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance` for both arms.
  Negative control (from the 999.134 acceptance): TERM sent straight to the agent kills it, which proves the test
  can observe death.
- e2e `stop_e2e::stop_ends_a_lock_free_live_run_through_the_socket`. The run is mid-agent with no lock held. Assert
  the agent group is gone, the socket is GONE, state is `stopped`, and no `stage_launched` follows. Control: the same
  fixture with no `stop` → the stage completes and advances.
- `stop_e2e::stop_never_signals_the_recorded_monitor_pid` (`stop_e2e.rs:177`) is about T-23-51. Rewrite it to
  "stop never signals by a recorded pid; it addresses the socket". Do not delete it silently.

**7c. 999.141** — scope is larger than "e2e helpers":
- 16 test files each define their own `devflow_bin()`, with 71 `Command::new(devflow_bin())` sites (`stop_e2e.rs`
  27, `phase7_cli.rs` 12, `gate_wedge_e2e.rs` 7, …) [VERIFIED: `rg -c` 2026-10-14]. There is no shared
  `tests/common/`. Only `gate_notify_persist_e2e.rs:121-125` and parts of `start_lock_e2e.rs:661-673` set
  `DEVFLOW_CACHE_DIR`.
- **In-crate `--bin` tests leak too.** `spawn_agent_and_record` calls `registry::register(&state.project_root, …)`
  (`pipeline_launch.rs:1093`), and in-crate tests such as `pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload`
  (`:2134`) drive it with the test process's real `HOME`. Once sockets exist they will also bind in the operator's
  real `~/.cache/devflow/sock/`. Env mutation is forbidden, so production code must take the cache dir as an
  injectable value. Recommended: resolve it once at CLI entry and thread it through as a field of a launch context,
  or add a `#[cfg(any(test, feature = "test-support"))]` process-wide `OnceLock` override in `registry` that
  in-crate tests set to one leaked tempdir per test binary. Tests that count sweep results must still use
  `*_in(own_dir)`.
- **Where it lands:** Wave 1, before any socket/sweep code or test. Create `crates/devflow-cli/tests/common/mod.rs`
  with `hermetic_devflow(home: &TempDir) -> Command`, which sets `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME` and
  `DEVFLOW_CACHE_DIR` and removes inherited `GIT_*`, then migrate all 71 sites. Add a monitor reaper that reaps by
  socket `shutdown` once sockets exist.
- **R-13 test** `test_home_isolation::a_child_writing_outside_its_tempdir_home_fails` (new e2e). Make a fake "real
  home" tempdir R and a hermetic home H. Run `devflow start` (fake agent) through the hermetic helper with the parent
  pointing at R. Assert R is untouched and H has `roots/` (and later `sock/`). Negative control: the same run built
  with a plain `Command` whose env names R must populate R, which proves the assertion can fail. This never reads
  the operator's real cache. *Do not repeat the 2026-10-09 cleanup of the real cache without asking (D-07).*

**7d. 999.139** — Finding 8 (guard), test `start_lock_e2e::start_refuses_over_a_live_supervisor_when_the_state_will_not_load`:
a corrupt `state-NN.json` plus a live supervisor (fake agent) whose sidecar names this root/phase → `start` refuses
and names the repair. Controls: the existing
`start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process` (`start_lock_e2e.rs:588`)
stays green, and a corrupt state with no live sidecar keeps today's behaviour.

### Finding 8 — Wire protocol and consumers

**Protocol (recommended, minimal):** one request per connection, one JSON object per line, 4 KiB cap, 2 s
timeouts, server checks `SO_PEERCRED` uid == own euid (via `libc::getsockopt`).

| Request | Reply |
|---|---|
| `{"op":"ping"}` | `{"ok":true,"launch_id":"…","phase":"50","stage":"code","launch":"legacy","owner":"agent_running|advancing|gate_wait|handed_off","agent_pid":N,"agent_pgid":N,"agent_start_time":T,"boot_id":"…"}` |
| `{"op":"shutdown"}` | immediate `{"ok":true,"accepted":true,"owner":"…"}`; then, on the same connection, `{"ok":true,"stopped":true,"exit_code":143,"advance_suppressed":true}` once the lock is released and `stopped` is persisted, followed by unlink and exit. `gate_wait` may answer `{"ok":false,"error":"gate_open"}`; `handed_off` answers `{"ok":false,"error":"handed_off"}` |
| anything else | `{"ok":false,"error":"unknown_op"}` |

No version field (D-20: no cross-version promise).

**Consumers (all in `crates/devflow-cli/src/commands.rs` unless noted):**

| Consumer | Today | Phase 50 |
|---|---|---|
| `Liveness` enum (`:988-1001`) | [VERIFIED: `Healthy,` `BetweenStages,` `Stuck,` `Unknown,`] from `monitor_pid` + `agent_running` | Replace with the core `Liveness {Gone, Stale, Alive(PingReply)}` plus `ProbeError`. `Healthy`/`BetweenStages` → `Alive` (owner says which); `Stuck` → `Stale` or `Gone`-while-active; `Unknown` → `Gone` with no `supervisor` recorded. The `liveness_matrix_covers_all_four_rows` test (`:5471`) is rewritten |
| `status` (`:1062`, `:1117-1128`) | prints `monitor_pid` and liveness | print the socket path, the probe result and the owner; repair hints: STALE → `devflow stop --phase N` then `devflow resume --phase N`; GONE while active and not stopped → `devflow resume --phase N` |
| `cleanup` (`:906-908`) | refuses worktree removal by liveness | ALIVE → keep; STALE → keep unless `--force` (the group may be live); GONE → existing rules |
| `doctor`: `build_phase_facts` (`:3920-3960`), `check_dead_monitor` (`:3818`) | `monitor_alive` | facts gain `supervisor: Result<Liveness, ProbeError>`. `check_dead_monitor` becomes `check_supervisor`: STALE → Problem; GONE-while-active → Problem; ProbeError → Problem naming the error. Add a **sweep finding**: sockets/sidecars in `sock/` whose root+phase has no loadable state, or whose `launch_id` ≠ the state's (superseded, the R-10 publish/unlink crash). Read-only (D-06) |
| `doctor --json` | `monitor_pid` in facts | replace with `supervisor: {socket_path, liveness, owner|error}` |
| `stop` (`:1882`) | `stop_via_gate`, then `stop_via_lock` (SIGTERM to the lock holder) | `stop_via_gate` first (unchanged). Then probe the socket from state, or from the matching sidecar when state will not load: ALIVE → `shutdown` and wait for the final reply (bounded by grace + margin); STALE → the Finding 1 backstop; GONE → keep `stop_via_lock` as the fallback for a lock held by a non-supervisor process. **Decided verb (D-06, Claude remit): `devflow stop --phase N` is the single teardown verb, ALIVE and STALE alike.** `recover --clean` keeps cleaning files and refuses while the probe is ALIVE |
| `refuse_launch_over_a_live_run` (`:2470-2490`) | [VERIFIED: `.and_then(\|state\| state.monitor_pid)` `.filter(\|&pid\| agent::agent_running(pid));`] | state loads → probe its socket: ALIVE → refuse; STALE with a backstop-valid non-empty group → refuse; STALE dead (empty or boot mismatch) → proceed. State exists but will not load → scan sidecars for this root+phase: any ALIVE or STALE-live → refuse, naming `devflow stop --phase N` (R-12). The agent-pid-file half stays as is (D-12) |
| `registry_reachable_pids` (`:4337-4380`) and `agent::discover_stray_devflow_processes` | include `monitor_pid`; census matches `sh -c` wrappers and `devflow advance` children | after Legacy deletion neither shape is produced. The census already never matched `__monitor`. Keep it unchanged (semver-neutral); drop the `monitor_pid` input; the socket sweep is the monitor census now |
| `pipeline_gate.rs:71-74` | clears `monitor_pid` at finish | clear `supervisor` |
| `stage_launched` event (`pipeline_launch.rs:1101`) | `"monitor_pid": pid` | `"socket_path"`, `"launch_id"`, `"launch"`. Phase 49 tools do not read `monitor_pid` [VERIFIED: `rg monitor_pid` over `49-…/tools/` → no hits] |
| `recover` (`main.rs:273`) | file cleanup | unchanged except refusing while ALIVE |

### Finding 9 — R-9: `pub` items in `devflow-core`, and semver-checks

`cargo semver-checks` 0.51.0 runs here. **At HEAD (crates == `origin/develop`) it already fails against the
published 2.12.0** (exit 100, run 2026-10-14 with `CARGO_TARGET_DIR` in scratch):

```
--- failure constructible_struct_adds_field ---   field AgentResult.checkpoint (agent_result.rs:60)
--- failure enum_marked_non_exhaustive ---        enum RecoverError (recover.rs:19), enum WorkflowError (workflow.rs:20)
Summary semver requires new major version: 2 major and 0 minor checks failed
```
[VERIFIED: semver-checks run, this session] A major bump is already owed for Phase 49 work. The plan's gate must
compare the failing-lint list against "pre-existing 2 + the Phase 50 list below", not expect exit 0.

| Item | Change | Breaking? |
|---|---|---|
| `state::State::monitor_pid` (`state.rs:288`) [VERIFIED: `#[serde(default)]` `pub monitor_pid: Option<u32>,`] | **remove** (recommended; D-20 lets old files fail or read GONE; keeping it invites a second liveness source) | yes (named) |
| `state::State::supervisor: Option<SupervisorRecord>` | add | no (`State` is `#[non_exhaustive]`, `state.rs:34`) |
| `monitor::spawn_monitor(...) -> Result<u32, MonitorError>` (`monitor.rs:308`) | return `Result<Published, MonitorError>` (socket path, launch id, pgid, start time, boot id) and take a cache-dir/launch-context argument | yes |
| `monitor::run_pipe_owning_monitor` (`monitor.rs:961`) | replaced by a shared `run_supervisor(cfg) -> Result<SupervisorOutcome, _>` | yes (signature/removal) |
| `monitor::MonitorError` (`monitor.rs:28`, not `#[non_exhaustive]`) | new variants (`SocketPath(..)`, `PgidMismatch`, `NotReady`) and mark it `#[non_exhaustive]` in the same break | yes |
| `monitor::MonitorLaunch` | variants unchanged; doc changes | no |
| `monitor::wait_for_agent_pid` | keep (agent pid file stays) | no |
| new `pub mod supervisor`: `Liveness`, `ProbeError`, `PingReply`, `probe`, `SocketPath`/`SocketPathError::TooLong`, `SUN_PATH_MAX_LINUX`, `SUN_PATH_MAX_MACOS`, `Sidecar`, `sweep_in`, `request_shutdown`, `backstop::{teardown_stale, BackstopOutcome}`, `boot_id` | add | no |
| `gates::Gates::poll_response_cancellable` | add | no |
| `registry::socket_dir_in` | add | no |
| `agent::StrayLayer::AdvanceChild` | no longer produced; **keep** (removal is gratuitous) | no |
| `lock` (999.128) | private fn only | no |

Version: bump to 3.0.0 in **both** `Cargo.toml` places (`workspace.package.version` and the `devflow-core`
dependency's `version`) [VERIFIED: shell read of root `Cargo.toml`, lines 9 and 20 — `version = "2.12.0"` and `devflow-core = { path = "crates/devflow-core", version = "2.12.0" }`]. That belongs to
release mechanics, not necessarily this phase.

### Finding 10 — The Phase 49 rig to reuse (D-16/D-17)

- Runbook: `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md` (sections 1–8,
  "If the run dies without reaching Ship", "Cleanup", "What an attempt does not establish").
- Tools: `…/49-…/tools/` — `launch-attempt.sh` (prepare/auth/start; `systemd-run --user … -p RemainAfterExit=yes -p
  KillMode=process`), `negctl.sh`, `snapshot-hook.sh` (the `DEVFLOW_GATE_NOTIFY_CMD` target), `surv-watch.py`,
  `collect-attempt.sh`, `check-evidence.sh`, `sanitize-evidence.py` (D-26), `EVIDENCE-LAYOUT.md` (the layout
  contract). The scaffold is `scripts/scratch-dogfood-repo.sh --live-payload`.
- Evidence root `$HOME/.cache/devflow-phase49/attempt-<N>` (`--home` overrides); payload phase is always `1`; unit
  names `p49-a<N>-{start,surv,negctl}`. These names are hard-coded at 14 sites in the tools [VERIFIED: `rg -c`].
  No `p49-*` units are left on this host [VERIFIED: `systemctl --user list-units --all 'p49-*'` empty]. Phase 49
  evidence: `…/49-evidence/attempt-{1,2,3}/`.
- Reuse plan: copy `tools/` into `50-…/tools/` with the P49→P50 names (or pass `--home ~/.cache/devflow-phase50`
  and accept the `p49-a<N>` unit names). Extend `collect-attempt.sh` to copy `sock/*.json` sidecars and record probe
  results. The fault arm (D-17/R-10) needs a new script, `tools/fault-arm.sh`: no repo, fake or real agent, records
  launch shape, socket path, probe result, group members before/after (`ps -o pid,pgid,sid,lstart,args -g <pgid>`),
  and whether advance ran (an `advance_evaluated` event).
- None of the rig tools read `monitor_pid` (verified above), so the field removal does not break them.

## Architecture Patterns

### System Architecture Diagram

```
 devflow start/resume (CLI, holds phase lock)          devflow stop / status / doctor (CLI)
        │ spawn __monitor --socket P --launch-id L               │ read state.supervisor.socket_path
        ▼                                                        │ (or sidecar if state won't load)
 ┌───────────────────── __monitor process (own pgrp) ─────────┐  ▼
 │ sigaction(TERM,INT) ─► self-pipe ─┐                        │  lstat ─► !exists ⇒ GONE
 │                                   ▼                        │       ─► !S_ISSOCK ⇒ ProbeError
 │  control thread: poll{pipe, listener} ─► events ──┐        │  connect ─► ECONNREFUSED ⇒ STALE ─► (stop) backstop:
 │                                                   ▼        │                boot_id? group? leader start time?
 │  terminal owner (Mutex<OwnerPhase>, stop_requested)        │                ─► pidfd group-signal | refuse+report
 │      │ AgentRunning: killpg(own unreaped child) ─► 143     │  connect ok ─► ping ─► ALIVE(owner)
 │      ▼                                                     │                └─(stop)─► shutdown ─► "stopped"
 │  agent (process_group(0), pgid==pid asserted)              │
 │      │ stdout→capture (legacy: file; pipe-owning: pipes)   │
 │      ▼ exit                                                │
 │  decide: stop? ─yes─► persist stopped ─► reply ─► unlink ─► exit
 │          │no                                               │
 │          ▼ Advancing                                       │
 │  advance_with (takes phase lock) ─► evaluate ─► [safe pt] ─► gate? (cancellable) ─► transition
 │          ─► [safe pt] spawn __monitor N+1 ─► ping N+1 ─► [safe pt] publish N+1 in state (under lock)
 │          ─► lock drops ─► HandedOff ─► unlink own socket+sidecar ─► exit
 └─────────────────────────────────────────────────────────────┘
          files: <cache>/sock/<digest>-<phase>-<launch>.sock (0600) + .json sidecar; .devflow/ capture, pid, exit
```

### Recommended Project Structure
```
crates/devflow-core/src/
├── supervisor.rs           # new: SocketPath, probe, Liveness, PingReply, wire protocol, sidecar, sweep_in, boot_id
├── supervisor/backstop.rs  # new: /proc group enumeration, Finding-1 rule, pidfd group signal
├── supervisor/runtime.rs   # new: self-pipe, control thread, terminal owner, run_supervisor (both arms)
├── monitor.rs              # spawn_monitor re-execs __monitor for BOTH arms; Legacy sh script deleted (D-03)
├── lock.rs                 # 999.128 one-predicate change
crates/devflow-cli/src/
├── pipeline_launch.rs      # run_monitor for both arms; safe points; publish N+1 under lock
├── commands.rs             # status/cleanup/doctor/stop/launch-guard rewired to probe
crates/devflow-cli/tests/
├── common/mod.rs           # new: hermetic_devflow() (999.141), reaper
└── supervisor_e2e.rs       # new: concurrency, fault arms R-10
```

### Pattern 1: Self-pipe signal delivery (async-signal-safe)
```rust
// Source: POSIX self-pipe pattern [ASSUMED: training]; mask/handler-on-exec behaviour VERIFIED by probes A and G
static WRITE_FD: AtomicI32 = AtomicI32::new(-1);
static SUPERVISOR_PID: AtomicI32 = AtomicI32::new(0);
extern "C" fn on_signal(signo: libc::c_int) {
    // Only getpid() and write() here: both async-signal-safe.
    if unsafe { libc::getpid() } == SUPERVISOR_PID.load(Ordering::Relaxed) {
        let byte = signo as u8;
        unsafe { libc::write(WRITE_FD.load(Ordering::Relaxed), (&byte as *const u8).cast(), 1) };
    }
}
// install: pipe2(fds, O_CLOEXEC | O_NONBLOCK); sigaction(SIGTERM/SIGINT, sa_flags = SA_RESTART)
// NEVER: pthread_sigmask(SIG_BLOCK, …) — the mask is inherited by every spawned child.
```

### Pattern 2: Atomic socket publish
```rust
// Source: VERIFIED probe B — rename replaces a live target, hard_link returns EEXIST
let listener = UnixListener::bind(&part)?;                 // bind + listen
std::fs::set_permissions(&part, Permissions::from_mode(0o600))?;
std::fs::hard_link(&part, &final_sock)?;                   // EEXIST ⇒ never steals
std::fs::remove_file(&part)?;
```

### Anti-Patterns to Avoid
- **`process::exit` on shutdown** (spike `main.rs:156`): abandons the lock and a half-written state (R-2).
- **Spike's `stopping` check before `try_wait` with no recheck** (`main.rs:108-124`): the race R-2 exists for.
- **Killing by pgid after the leader is reaped** on the ALIVE path: use `waitid(P_PID, pid, WEXITED|WNOWAIT)` to
  observe the exit without reaping, `killpg` leftover members while the zombie holds the pgid, then reap.
- **Treating `ECONNREFUSED` alone as STALE** (regular files and directories produce it too).
- **Process-global stop flag** shared by parallel tests.
- **Passing adapter env values on argv** to `__monitor` (they show up in `ps`). Pass key names (`--keep-env NAME`)
  and re-apply the inherited values on the agent `Command` *after* `hermetic_command` (the `GIT_CONFIG_COUNT` scrub
  note, `monitor.rs:386-404`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Process identity | a new pid-identity scheme | `agent::process_start_time` / `is_same_process` [VERIFIED: `agent.rs:214-230` — `rest.split_whitespace().nth(19)?.parse::<u64>().ok()`; `process_start_time(pid) == Some(expected_start)`] | Already the field-22 parser used by `stop` and the lock |
| Lock holder classification | a second classifier | `lock::classify_holder_record` / `HolderStatus` | 999.128 is a one-predicate reuse |
| Group escalation | a second TERM→KILL loop | MEMORY: the Phase 31 shape (group SIGTERM → `terminate_and_verify(child_pid,…)` → group SIGKILL); `terminate_and_verify` rejects non-positive pids | Avoids a `kill(-1)` / `kill(0)` class |
| Atomic file writes | ad-hoc writes | `workflow::create_unique_temp` + rename (used by `lock::publish_lock`) | Phase 48 durability pattern |
| Cache-dir resolution | env reads in new code | `registry::cache_dir()` + `*_in(dir)` | The only env-reading resolver (`registry.rs:46-53`) |
| Signal library | — | libc self-pipe (or `signal-hook` only with `cargo deny` + checkpoint) | see Alternatives |

**Key insight:** each failure this project has paid for (orphaned tails, lost TERM windows, recycled pids) came from
the same move: inferring identity or liveness from a value that can go stale. The socket answers liveness; any
identity check has to anchor on a living process (an unreaped child, or a leader whose start time matches).

## Runtime State Inventory

This phase changes a persisted handle (`monitor_pid` → `supervisor`) and the monitor mechanism, so the inventory
applies.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `.devflow/state-NN.json` in every DevFlow-managed project on this machine carries `monitor_pid`; the new binary ignores or drops it (serde default; unknown fields are tolerated by the rest of serde [ASSUMED: no `deny_unknown_fields` on `State` — the planner must confirm]) | None by D-20 (no migration). Operational rule: install 3.x only while no 2.x run is live |
| Live service config | none — DevFlow has no service config outside the repo | None (verified: no daemon; units are transient) |
| OS-registered state | Phase 49 systemd user units `p49-a<N>-*`: none present now [VERIFIED: `systemctl --user list-units --all 'p49-*'` empty]. Running 2.x monitors (`sh -c` wrappers / `__monitor`) at install time | Before installing 3.x: `devflow status` shows no live run (D-20 rule). No code |
| Secrets/env vars | `DEVFLOW_CACHE_DIR`, `XDG_CACHE_HOME`, `HOME` now also decide the socket location; `DEVFLOW_GATE_NOTIFY_CMD` is still stripped from the chain (`monitor.rs:292-294`) | Code: the socket dir derives from `cache_dir()`; tests set it per child |
| Build artifacts | the installed `devflow` binary (`~/.cargo/bin` or wherever `scripts/install.sh` puts it); a binary rebuilt mid-run is picked up by `current_exe()` (MEMORY 48-CONTEXT) | Rebuild + reinstall after the phase; Phase 49 D-12 binary-identity recording covers the rig |
| Caches | `~/.cache/devflow/roots/` (registry) gains a sibling `sock/`; the operator's real cache was cleaned 2026-10-09 (999.141) | Do not clean again without asking (D-07) |

## Common Pitfalls

### Pitfall 1: The probe calls a stray file STALE
**What goes wrong:** a regular file or directory at the path gives `ECONNREFUSED` (verified) → STALE → the backstop
arms on whatever pgid the sidecar says.
**How to avoid:** `lstat` and require `S_ISSOCK`, our uid, and no symlink, before `connect`.
**Warning signs:** a test with a regular file at the socket path reporting STALE.

### Pitfall 2: The blocked signal mask leaks to agents
**What goes wrong:** with a `sigwait` design, every child starts with TERM/INT blocked (verified), and `killpg TERM`
does nothing.
**How to avoid:** use the self-pipe handler; never block signals in the supervisor.
**Warning signs:** `SigBlk: …4002` in `/proc/<agent>/status`. Add an assertion to the parity test.

### Pitfall 3: `rename` steals a live socket
**How to avoid:** use the `hard_link` publish (verified `EEXIST`).

### Pitfall 4: The bind→listen window reads STALE
**How to avoid:** link the final name only after `UnixListener::bind` returns (it binds and listens).

### Pitfall 5: The handoff deadlocks or leaves a gap
**What goes wrong:** N+1 tries to take the lock N holds, or N clears its record before N+1 is published.
**How to avoid:** use the Finding 2 order: the launcher publishes under its own lock, with one save that replaces
the record.

### Pitfall 6: Test leakage into the operator's cache
**What goes wrong:** in-crate tests call `registry::register` and spawn supervisors with the real `HOME`.
**How to avoid:** Wave 1 builds the injectable cache dir and the `tests/common` helper first, and sweep tests use
`*_in(dir)`.

### Pitfall 7: The semver gate is green-or-red by accident
**What goes wrong:** semver-checks already exits 100 at HEAD.
**How to avoid:** assert the exact failing-lint set.

### Pitfall 8: `cargo test --exact` false green
**How to avoid:** module-qualify every unit-test name and grep for `1 passed`. Integration tests are top-level fns
in their test target (`--test stop_e2e <fn> -- --exact`).

### Pitfall 9: A TERM-ignoring member survives `stop`
**What goes wrong:** the spike breaks out of its escalation loop once the leader exits, so members that ignore TERM
are never SIGKILLed (`main.rs:139-147`: the loop `break`s on `try_wait` success before escalating).
**How to avoid:** use `waitid(WNOWAIT)`: escalate to a group SIGKILL while the leader is an unreaped zombie, then reap.

## Code Examples

### Total probe skeleton
```rust
// Source: classification VERIFIED by probe D (this session)
pub fn probe(path: &Path) -> Result<Liveness, ProbeError> {
    let meta = match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Liveness::Gone),
        Err(e) => return Err(ProbeError::Stat(e)),
        Ok(m) => m,
    };
    if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
        return Err(ProbeError::NotASocket);
    }
    match UnixStream::connect(path) {
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => Ok(Liveness::Stale),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Liveness::Gone), // unlink race
        Err(e) => Err(ProbeError::Connect(e)),
        Ok(stream) => ping(stream).map(Liveness::Alive),                    // NoReply ⇒ Err
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `kill(pid)` + `/proc` start-time guards | `pidfd_open` + `pidfd_send_signal` | Linux 5.3/5.1 [ASSUMED] | race-free signalling of a specific process |
| `killpg` after validation | `pidfd_send_signal(..., PIDFD_SIGNAL_PROCESS_GROUP)` | Linux 6.9 [ASSUMED] | removes the validate→kill window for the leader-present backstop (verified working on 7.2.5) |

**Deprecated/outdated in this repo after Phase 50:** the Legacy `sh -c` script and its stop-marker machinery
(`monitor.rs:472-593`), `MonitorTail`, `advance_tail`, `shell_escape` (if no other user), the `State::monitor_pid`
liveness path, and the `liveness(monitor_pid, …)` predicate.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | A pid number is not reusable while any task uses it as a pgid/sid (Linux `free_pid`; POSIX group lifetime) | Finding 1 steps 4–5 | Step 5's "certainly gone" inference and the leader-present soundness fail; the backstop could signal a reused group. Spike C5 relies on the same fact |
| A2 | `setpgid` lets same-session processes join an existing group | Finding 1 table | Only strengthens the "no sound rule" conclusion; low risk |
| A3 | `PIDFD_SIGNAL_PROCESS_GROUP` needs Linux ≥ 6.9; CI/container kernels may lack it | Finding 1 step 4 | Need the `EINVAL` fallback; the CI kernel was not measured |
| A4 | `std::process::Child::spawn` returns only after exec (std's CLOEXEC error pipe) | Finding 6 (pgid assert timing) | The pgid assert could race the child's `setpgid`; mitigate by reading `/proc/<pid>/stat` field 5 with a short retry |
| A5 | Containers share the host boot id | Finding 5 | R-8 in containers: a false "previous boot" would remove a live-looking stale socket only, signal nothing (fail-safe) |
| A6 | No legitimate caller advances a stopped state | Finding 3 residual | Adding the refusal could block a real path; the planner must grep callers first |
| A7 | `State` has no `deny_unknown_fields` | Runtime inventory | A 2.x file fails to load instead of reading GONE; acceptable under D-20 either way |
| A8 | `signal-hook` name and maturity | Alternatives | Not recommended; no impact unless adopted |
| A9 | self-pipe pattern details | Pattern 1 | standard; the probes verified the parts that matter (mask, exec reset) |

## Open Questions (RESOLVED)

1. **(RESOLVED) `$XDG_CACHE_HOME` vs the literal `~/.cache/devflow/`.**
   - What we know: criterion 3 and D-01 say `~/.cache/devflow/`. `registry::cache_dir()` (which R-7 mandates)
     prefers `DEVFLOW_CACHE_DIR`, then `$XDG_CACHE_HOME/devflow`, then `$HOME/.cache/devflow`.
   - What's unclear: whether a set `XDG_CACHE_HOME` satisfies criterion 3. The operator's rejection was of
     `$XDG_RUNTIME_DIR` (deleted at logout); `XDG_CACHE_HOME` has no such property.
   - Recommendation: use `cache_dir()` as R-7 says, and have the plan amend criterion 3's wording to "under
     `registry::cache_dir()` (default `~/.cache/devflow/`)". *Needs an operator answer only if the operator reads
     criterion 3 literally.*
   - Resolution (Claude remit, planning 2026-10-14): use `registry::cache_dir()` as R-7 mandates. 50-01 Task (b) appends
     to ROADMAP criterion 3 that the directory is resolved by `registry::cache_dir()` (`DEVFLOW_CACHE_DIR`, then
     `$XDG_CACHE_HOME/devflow`, default `~/.cache/devflow/`), which keeps both of criterion 3's exclusions; 50-11 V-07
     tests that the published path follows the cache dir. No operator answer was requested.
2. **(RESOLVED) The CI kernel and `PIDFD_SIGNAL_PROCESS_GROUP`.** Not measured. Implement the `EINVAL` fallback and make a test
   that requires the flag skip loudly (print "skipped: kernel lacks …"), never pass silently.
   - Resolution (planning 2026-10-14): 50-09 implements the documented `killpg` fallback on `EINVAL` only, and
     `supervisor::backstop::tests::pidfd_group_signal_is_used_when_the_kernel_supports_it` prints `skipped: kernel lacks
     PIDFD_SIGNAL_PROCESS_GROUP` instead of passing silently. The CI kernel itself stays unmeasured until that test runs
     in CI.
3. **(RESOLVED) Should a natural agent exit also reap leftover group members** (background processes the agent left)? This is
   not in D-04. It would close part of 999.46 cheaply via `WNOWAIT`. Recommend leaving it out of Phase 50 unless the
   operator asks; record it in the backlog.
   - Resolution (planning 2026-10-14): left out of Phase 50, because D-04 scopes leftover-member reaping to a stop
     (50-07 `wait_agent` SIGKILLs leftovers only on `Stopped`). No plan files it: 50-20 Task 3 files only defects
     found by this phase's runs, and this is a research idea, not a found defect. Filing it as a backlog idea is left
     to the operator.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustc/cargo | build | ✓ | 1.97.1 (pinned) | — |
| Linux kernel with pidfd + `PIDFD_SIGNAL_PROCESS_GROUP` | backstop | ✓ host | 7.2.5 | `killpg` after validation on `EINVAL` |
| `/proc/sys/kernel/random/boot_id` | R-8 | ✓ | — | — |
| cargo-semver-checks | R-9 | ✓ | 0.51.0 | — |
| cargo-deny | dep policy (no new dep planned) | ✓ | 0.20.2 | — |
| cargo-mutants / cargo-llvm-cov | backstop + owner state machine mutation testing | ✓ | installed | — |
| systemd-run (user) | live rig | ✓ | /usr/bin/systemd-run | — |
| gh, jq, python3, shellcheck | rig tools | ✓ | installed | — |
| macOS host/CI | macOS claim | ✗ | — | none: "unverified on macOS" (Discretion) |

**Missing dependencies with no fallback:** macOS (accepted as unclaimed).

## Validation Architecture

Framework: `cargo test` (Rust built-in harness) with `tempfile`, `insta` (snapshots); TDD mode is on in
`.planning/config.json`. Config: workspace `Cargo.toml`; `scripts/check.sh` (fmt/clippy/test, `INSTA_UPDATE=no`).

| Property | Value |
|----------|-------|
| Framework | Rust test harness (rustc 1.97.1) |
| Config file | `scripts/check.sh`, `rust-toolchain.toml` |
| Quick run command | `cargo test -p devflow-core --lib supervisor::` then `cargo test -p devflow --bin devflow commands::` |
| Full suite command | `scripts/check.sh all` (then `scripts/check-in-container.sh` before push) |

**Exact-name rule:** every single-test command below must be run as
`cargo test -p <pkg> <target> <name> -- --exact 2>&1 | tee /dev/stderr | rg -q '1 passed'`. For unit tests `<name>`
is module-qualified; for e2e it is the bare fn name inside `--test <file>`.

### Phase Requirements → Test Map

| ID | Behaviour | Test (positive) | Negative control | Command | Exists? |
|----|-----------|-----------------|------------------|---------|---------|
| V-01 crit1 GONE | no socket file → `Gone` only | `supervisor::tests::probe_reports_gone_only_when_no_socket_file_exists` | same dir with a live listener → `Alive` | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_gone_only_when_no_socket_file_exists -- --exact` | ❌ W0 |
| V-02 crit1 STALE | socket file, no listener (bind then drop; and a child SIGKILLed) → `Stale` only | `supervisor::tests::probe_reports_stale_only_for_a_socket_whose_listener_is_gone` | listener kept → `Alive` | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_stale_only_for_a_socket_whose_listener_is_gone -- --exact` | ❌ W0 |
| V-03 crit1 ALIVE | connect + ping reply → `Alive` only | `supervisor::tests::probe_reports_alive_only_after_a_ping_reply` | listener that never replies → `ProbeError::NoReply` | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_alive_only_after_a_ping_reply -- --exact` | ❌ W0 |
| V-04 crit1 discrimination | 3×3 table: each fixture yields exactly its state | `supervisor::tests::each_fixture_produces_exactly_one_liveness_state` | a mutated predicate (always `Alive`) must fail the table (cargo-mutants on `probe`) | `cargo test -p devflow-core --lib supervisor::tests::each_fixture_produces_exactly_one_liveness_state -- --exact`; `cargo mutants -v --file crates/devflow-core/src/supervisor.rs` | ❌ W0 |
| V-05 R-6 | regular file / directory / mode-000 → `ProbeError`, never `Stale` | `supervisor::tests::a_non_socket_or_unreadable_path_is_a_probe_error_not_stale` | a real stale socket → `Stale` | `cargo test -p devflow-core --lib supervisor::tests::a_non_socket_or_unreadable_path_is_a_probe_error_not_stale -- --exact` | ❌ W0 |
| V-06 crit3 / R-5 | 107 binds, 108 → `SocketPathError::TooLong{len:108,max:107}`; macOS const 103 | `supervisor::tests::socket_path_of_107_bytes_binds_and_108_is_refused_by_name` | 106 binds (boundary control) | `cargo test -p devflow-core --lib supervisor::tests::socket_path_of_107_bytes_binds_and_108_is_refused_by_name -- --exact` | ❌ W0 |
| V-07 crit3 | path persisted in state and under the `cache_dir_in` override, never under the project tree | `pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state` | a state without `supervisor` → status reports `Gone` | `cargo test -p devflow --bin devflow pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state -- --exact` | ❌ W0 |
| V-08 crit2 / R-11 | parity table R-A…R-M under both arms with a non-Claude adapter (e.g. Codex env reaching grandchildren, split stdout/stderr, pid file, exit 7, 128+n, advance on completion, 143 + no advance on stop, 0600 socket, cwd = worktree, empty `SigBlk` in agent) | `monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter` | a deliberately broken arm (stderr into stdout) must fail the R-B row | `cargo test -p devflow-core --lib monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter -- --exact` | ❌ W0 |
| V-09 crit2 | the same liveness for the same condition across arms (GONE/STALE/ALIVE × arm) | `monitor::tests::both_arms_answer_liveness_identically_for_each_condition` | swap the expected arm label → fails | `cargo test -p devflow-core --lib monitor::tests::both_arms_answer_liveness_identically_for_each_condition -- --exact` | ❌ W0 |
| V-10 R-3 | `pgid == agent_pid` for both arms; never the launcher's pgrp | `monitor::tests::both_arms_put_the_agent_in_its_own_group` | an agent spawned without `process_group(0)` fails the assert | `cargo test -p devflow-core --lib monitor::tests::both_arms_put_the_agent_in_its_own_group -- --exact` | ❌ W0 |
| V-11 R-1 atomic publish | final name appears only after listen; `hard_link` refuses an existing name | `supervisor::tests::publish_never_exposes_an_unlistening_socket_or_replaces_a_live_one` | raw bind-before-listen at the final name reads `Stale` (proves the hazard) | `cargo test -p devflow-core --lib supervisor::tests::publish_never_exposes_an_unlistening_socket_or_replaces_a_live_one -- --exact` | ❌ W0 |
| V-12 R-1 concurrency | two concurrent `start`s → exactly one ALIVE socket and one agent group | `two_concurrent_starts_yield_exactly_one_live_supervisor` | with the guard disabled via a test seam → two | `cargo test -p devflow --test supervisor_e2e two_concurrent_starts_yield_exactly_one_live_supervisor -- --exact` | ❌ W0 |
| V-13 R-1 handoff | state never reads GONE across a stage handoff; N unlinks only after N+1 is published | `pipeline_launch::tests::stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked` | pre-clear ordering (WR-04 style) observes GONE | `cargo test -p devflow --bin devflow pipeline_launch::tests::stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked -- --exact` | ❌ W0 |
| V-14 R-2 | a stop racing natural completion never launches the next stage (all interleavings) | `supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances` | `supervisor::runtime::tests::natural_completion_without_a_stop_advances` | `cargo test -p devflow-core --lib supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances -- --exact` (+ control) | ❌ W0 |
| V-15 R-2 gate | TERM during a gate wait → Abort, lock released, `stopped` | `pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock` | no signal → the gate still waits until answered | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock -- --exact` | ❌ W0 |
| V-16 D-05 / 999.134 | TERM/INT to the supervisor (both arms) → group gone, exit 143, no advance, GONE | `monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance` | TERM to the agent only → the agent dies (observability) | `cargo test -p devflow-core --lib monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance -- --exact` | ❌ W0 |
| V-17 D-04 / 999.138 | `stop` on a lock-free live run ends it through the socket | `stop_ends_a_lock_free_live_run_through_the_socket` | no `stop` → the stage advances | `cargo test -p devflow --test stop_e2e stop_ends_a_lock_free_live_run_through_the_socket -- --exact` | ❌ W0 |
| V-18 R-4 leader present | validated group signalled (pidfd group flag) | `supervisor::backstop::tests::backstop_kills_a_validated_group_whose_leader_is_present` | wrong recorded start time → nothing signalled | `cargo test -p devflow-core --lib supervisor::backstop::tests::backstop_kills_a_validated_group_whose_leader_is_present -- --exact` | ❌ W0 |
| V-19 R-4 leader gone | leader exits, members live → refuse, report members, members survive | `supervisor::backstop::tests::backstop_refuses_and_reports_a_group_whose_leader_is_gone` | the same group with the leader alive → killed | `cargo test -p devflow-core --lib supervisor::backstop::tests::backstop_refuses_and_reports_a_group_whose_leader_is_gone -- --exact` | ❌ W0 |
| V-20 R-4 reuse | recorded pgid now names an unrelated live group (start-time mismatch) → not signalled, GONE | `supervisor::backstop::tests::a_reused_pgid_is_never_signalled` | matching start time → signalled | `cargo test -p devflow-core --lib supervisor::backstop::tests::a_reused_pgid_is_never_signalled -- --exact` | ❌ W0 |
| V-21 R-8 | fabricated boot id → nothing signalled, socket+sidecar removed by `stop`, GONE | `supervisor::backstop::tests::a_previous_boot_socket_is_removed_and_nothing_is_signalled` | current boot id + a live leader → killed | `cargo test -p devflow-core --lib supervisor::backstop::tests::a_previous_boot_socket_is_removed_and_nothing_is_signalled -- --exact` | ❌ W0 |
| V-22 R-7 / D-06 | sweep lists ALIVE/STALE sidecar-backed sockets without state; read-only (no file removed, no signal) | `supervisor::tests::sweep_reports_without_removing_or_signalling` | `stop --phase N` on the same fixture does remove | `cargo test -p devflow-core --lib supervisor::tests::sweep_reports_without_removing_or_signalling -- --exact` | ❌ W0 |
| V-23 999.139 / R-12 | corrupt state + live supervisor → `start` and `resume` refuse | `start_refuses_over_a_live_supervisor_when_the_state_will_not_load` | corrupt state, no live sidecar → today's behaviour; `start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process` stays green | `cargo test -p devflow --test start_lock_e2e start_refuses_over_a_live_supervisor_when_the_state_will_not_load -- --exact` | ❌ W0 |
| V-24 999.128 | a live pid with a different start time is reclaimed | `lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time` | `lock::tests::acquire_never_reclaims_a_live_holder_whose_start_time_matches`; `lock::tests::stale_reclaim_is_serialized_before_it_removes_the_lock_path` green | `cargo test -p devflow-core --lib lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time -- --exact` | ❌ W0 |
| V-25 999.128 repair | `recover --clean` at a `Recycled` holder succeeds | `recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock` **rewritten** to assert cleanup | a `Live` holder still refuses | `cargo test -p devflow-core --lib recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock -- --exact` (renamed accordingly) | ✅ (flip) |
| V-26 999.141 | a child writing outside its tempdir HOME fails the test | `a_child_writing_outside_its_tempdir_home_fails` | a plain `Command` with the fake-real HOME populates it | `cargo test -p devflow --test test_home_isolation a_child_writing_outside_its_tempdir_home_fails -- --exact` | ❌ W0 |
| V-27 R-9 | the semver failing-lint set == the pre-existing 2 + the named Phase 50 list | script gate | — | `cargo semver-checks check-release -p devflow-core 2>&1 \| rg '^--- failure' \| sort` compared to the plan's list | n/a |
| V-28 D-16 | live run to Ship crossing both arms | manual: rig attempt (`checkpoint:human-action`) | Phase 49 attempt record as reference | runbook | manual |
| V-29 D-17 / R-10 | fault arm: ALIVE→stop→GONE; kill -9→STALE→stop→GONE; SIGKILL while advancing; SIGKILL between publish and unlink; stop racing completion | manual + `tools/fault-arm.sh` record | each record names launch shape, socket path, probe, group members before/after, advance ran? | script | ❌ W0 |

### Sampling Rate
- **Per task commit:** the task's named tests (exact, `1 passed`) plus `cargo test -p devflow-core --lib` or
  `cargo test -p devflow --bin devflow` for the touched crate.
- **Per wave merge:** `scripts/check.sh all`; `cargo clippy --workspace --all-targets -- -D warnings`; for the
  backstop and owner modules, `cargo mutants -v --file crates/devflow-core/src/supervisor/backstop.rs` and
  `…/runtime.rs` (no surviving mutants in the decision functions).
- **Phase gate:** full suite green, `scripts/check-in-container.sh`, the V-27 semver comparison, then the live rig.

### Wave 0 Gaps
- [ ] `crates/devflow-cli/tests/common/mod.rs` — hermetic `HOME`/`XDG_*`/`DEVFLOW_CACHE_DIR` child builder + reaper (999.141; **first**)
- [ ] injectable cache dir for in-crate `--bin` tests (Finding 7c)
- [ ] `crates/devflow-cli/tests/test_home_isolation.rs` — V-26
- [ ] `crates/devflow-cli/tests/supervisor_e2e.rs` — V-12 and fault-arm fixtures
- [ ] `crates/devflow-core/src/supervisor*.rs` test modules — V-01…V-06, V-11, V-14, V-18…V-22
- [ ] `.planning/phases/50-…/tools/fault-arm.sh` and the P50 copies of the 49 rig tools

## Security Domain

`security_enforcement` is absent from `.planning/config.json`, so it counts as enabled.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | partial | local-only: socket 0600, `sock/` dir 0700 (MEMORY: Phase 23 cache dirs 0700), `SO_PEERCRED` uid == euid on the server |
| V3 Session Management | no | — |
| V4 Access Control | yes | `shutdown` only from the same uid; the backstop signals only own-uid processes, never pgid ≤ 1 or our own `getpgrp()` |
| V5 Input Validation | yes | wire line capped at 4 KiB with read timeouts; sidecar/state parsed with serde into typed structs; reject non-socket paths (`lstat`) |
| V6 Cryptography | no | — (digest is a filename disambiguator, not security) |
| V12 Files/Resources | yes | atomic writes; refuse a symlinked or foreign-owned `sock/` dir; per-launch names prevent overwrite |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Another local user connects and sends `shutdown` | Tampering / DoS | 0600 socket, 0700 dir, `SO_PEERCRED` check |
| Planted file or symlink at the socket path makes the probe read STALE and arms the backstop | Elevation / Tampering | `lstat` + `S_ISSOCK` + uid check; a probe error never triggers the backstop (R-6) |
| Tampered sidecar names a victim pgid | Tampering | the backstop requires boot id + a leader start-time match, own-uid members only, refuses leader-gone; never signal pid/pgid ≤ 1 or our own group |
| pid/pgid reuse between validation and signal | Spoofing | `pidfd_open` + start-time re-check + `PIDFD_SIGNAL_PROCESS_GROUP`; on the ALIVE path signal only while the leader is unreaped |
| Oversized or slow request wedges the control thread | DoS | 4 KiB cap, 2 s timeouts, one request per connection |
| Adapter env values leaked on argv | Information disclosure | pass names, not values, to `__monitor` |

## Sources

### Primary (HIGH confidence)
- This session's probes (scratch crate `probe50`, libc 0.2.186, rustc 1.97.1, kernel 7.2.5), all with negative
  controls: A (sigmask inheritance), B/B2 (rename/link/bind-before-listen), C (`sun_path`), D (connect
  classification), E (pgid after leader death), F (`PIDFD_SIGNAL_PROCESS_GROUP`), G (handler reset on exec, CLOEXEC
  listener).
- Source read this session: `monitor.rs:1-60,260-709,940-1269`; `pipeline_launch.rs:880-1299,1400-1879`;
  `registry.rs:46-65`; `state.rs:282-289`; `agent.rs:212-231`; `lock.rs:46-56,238-253`;
  `commands.rs:988-1001,2039-2043,2470-2489`; `phase_id.rs:98-103`; libc `pidfd.rs:14-17`. Also read via shell:
  `lock.rs:130-400`, `commands.rs:878-930,980-1060,1870-2160,2400-2540,3795-3962,4330-4385`, `agent.rs:296-510`,
  `gates.rs:280-320`, manifests.
- `.planning/audits/2026-07-24-socket-supervisor-spike.md`; `.planning/spikes/socket-supervisor/main.rs`.
- `cargo semver-checks check-release -p devflow-core` run (0.51.0), 2026-10-14.

### Secondary (MEDIUM confidence)
- ROADMAP backlog entries 999.128/134/138/139/141/153; Phase 49 RUNBOOK and EVIDENCE-LAYOUT; MEMORY-RECALL.md.

### Tertiary (LOW confidence)
- Kernel version history for pidfd flags, `free_pid` semantics, POSIX `setpgid` rules (training; see Assumptions).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependency; libc exports verified by compile.
- Architecture: MEDIUM — the handoff and safe points are reasoned from source; nothing is built yet.
- Pitfalls: HIGH — the four mechanism pitfalls are reproduced on this host with controls.
- R-4 conclusion: HIGH that "no sound rule from recorded scalars" holds (the sid result was verified; the joiner and
  reuse arguments are POSIX). MEDIUM on step 5's strengthening (assumption A1).

**Research date:** 2026-10-14
**Valid until:** 2026-11-13 (stable domain; re-check line numbers before citing them in plans, since they drift)

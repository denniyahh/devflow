# Phase 50: Addressable Monitor Liveness - Context

**Gathered:** 2026-10-10
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace the PID-addressed monitor with a socket-addressed supervisor, so "is this run's monitor alive?" resolves to
exactly one of GONE / STALE / ALIVE from `state.json` plus the socket at the recorded path, with no PID read on the
answering path (SUPV-01, ROADMAP Phase 50 criteria 1–3). `MonitorLaunch::PipeOwning` (already a Rust supervisor) gains
an address; `MonitorLaunch::Legacy` (the detached `sh -c` script) is replaced by a Rust supervisor; both answer
identically. No 2.x → 3.x migration is provided (D-20). This is the milestone's breaking change (major version).

**The operator widened the phase beyond the four liveness criteria** (D-01…D-06): the supervisor also owns teardown
through the socket (the full DEN-58 / #185 spike design), and five backlog items are folded in or closed (999.128,
999.134, 999.138, 999.139, 999.141). Liveness remains the spine; everything else hangs off the same socket.

**Scope changes planning must carry as plan tasks:**

- Amend ROADMAP Phase 50's entry so its criteria and requirement list name what this context adds: socket teardown,
  STALE backstop, sweep, signal handling, and the folded items 999.128, 999.134, 999.138,
  999.139, 999.141. Mark those backlog entries as folded into Phase 50.
- Update 999.143 to say the monitor-pid half is removed by Phase 50; the agent-pid half and the work-preserving
  repair stay in the backlog (D-12).
- 999.130 is promoted to its own phase between 50 and 51 (D-11). That insertion is an operator-decided roadmap
  action done through `/gsd-phase --insert`, not a Phase 50 plan task.

**Evidence labels.** *Verified* = checked in source or by a command this session, with file:line. *Reasoned* = derived
from source, not reproduced. *Spike* = proven by the 2026-07-24 spike on Linux only. Line numbers drift; re-check
before citing them in a plan.

**Provenance labels.** *Operator* = chosen by the operator in this discussion. *Operator (Phase 23)* = an earlier
operator decision carried forward unchanged. *Claude (remit)* = decided within Claude's own remit; not an operator
mandate.

</domain>

<decisions>
## Implementation Decisions

### Carried forward (Operator, Phase 23 — not re-opened)

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

### Supervisor scope (Operator, 2026-10-10)

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

### Folded backlog (Operator, 2026-10-10)

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

### No 2.x migration (Operator, 2026-10-12)

- **D-20 (Operator, 2026-10-12; retires D-13, D-14, D-15 and D-19).** DevFlow has no outside users, so Phase 50
  builds no 2.x → 3.x migration. 3.x makes no promise about a `state.json` written by 2.x: it may read as GONE or fail
  to load, and either is acceptable. There is no `schema_version` field, no "incompatible" report, no refusal on a 2.x
  state, no upgrade verb and no 2.x compatibility test. ROADMAP criterion 4 is retired with it. Operational rule
  instead of code: install a 3.x binary only while no 2.x run is live. The retired decisions and their alternatives
  remain in `50-DISCUSSION-LOG.md` and in git history.

### Proof bar (Operator, 2026-10-10)

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

### Claude's Discretion

- **Socket keying.** The spike used a hash of project root plus phase (for example `…/devflow/ab12cd34-22.sock`,
  39 bytes). Per-phase versus per-launch naming is Claude's call, under two constraints: the takeover rule (a new
  monitor never steals a live socket; only a STALE one may be reclaimed), and D-09's need to map a socket back to a
  root and phase without loading state.
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

### Post-review corrections (Claude, remit, 2026-10-11 — adversarial review of `5698426`)

Source: `.planning/reviews/50-adversarial-2026-10-11-context/SUMMARY.md`. Three external lanes (agy, codex, pi)
reviewed the decisions above, and every finding was re-verified against source. These corrections fix mechanism and
evidence; none changes an operator decision. Where a decision above and this block disagree, this block wins. One
finding (O-1) concerned the 2.x way out; its answer, D-19, was retired with the rest of the migration work by D-20.

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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirement and scope
- `.planning/ROADMAP.md` — Phase 50 entry (goal, sequencing note, criteria 1–3, the no-migration note); backlog entries 999.128, 999.130,
  999.134, 999.138, 999.139, 999.141, 999.143
- `.planning/REQUIREMENTS.md` — SUPV-01 (scope correction, `sun_path` binding constraint); SUPV-02 (Phase 51 depends
  on this phase; it must re-test the dependency, not assume it)

### Supervisor design (read first)
- `.planning/audits/2026-07-24-socket-supervisor-spike.md` — spike Part 1 (C1–C6) and Part 2 (R-A…R-M): scenarios
  and expected results
- `.planning/spikes/socket-supervisor/README.md`, `main.rs`, `Cargo.toml` — the spike code, which builds standalone
- GitHub issue #185 (migrated DEN-58 / 999.33) — full design, failure modes F1–F7, known gaps (macOS, signal
  handling, large output)
- `.planning/milestones/v2.0.0-phases/23-end-to-end-dogfood/23-CONTEXT.md` — D-08, D-09, D-10 (carried as D-01…D-03)

### Prior-phase constraints
- `.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-CONTEXT.md` — per-phase lock, advance
  binding to its stage (R-2), D-03 parked items, D-05 limits
- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-CONTEXT.md` — the live-rig decisions D-17…D-28
  and R-1…R-10 that D-16…D-18 reuse
- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-LIVE-RUN.md` — the reference run on the old
  mechanism (the roadmap's sequencing note)
- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md` and `tools/` — the rig being reused

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MonitorLaunch` and `spawn_monitor` / `spawn_monitor_inner` (`crates/devflow-core/src/monitor.rs:267,308,329`): the
  seam being replaced. The `PipeOwning` arm re-execs `current_exe()` as the hidden `__monitor` subcommand
  (`monitor.rs:342,436,603`; `crates/devflow-cli/src/main.rs:137`).
- `run_monitor` (`crates/devflow-cli/src/pipeline_launch.rs:918`): the in-process advance the `Legacy` replacement
  should share.
- `agent::process_start_time` and `agent::is_same_process` (`crates/devflow-core/src/agent.rs:214,228`): the
  pid-plus-start-time identity used by `stop`'s lock check (`commands.rs:2065-2079`), reused by the backstop and 999.128.
- The spike's `status` / `stop` / `sweep` functions: a reference implementation built only on `std::os::unix::net`
  plus `libc`, with no new dependency (a stated design win; keep it).

### Established Patterns
- `State` is `#[non_exhaustive]` with `#[serde(default)]` fields. The atomic temp-plus-rename save and the per-phase
  lock (Phase 48) apply to any new state writes.
- The `Legacy` script's TERM/INT trap and its lost-TERM race handling (`monitor.rs:488-544`) record hard-won behaviour.
  The replacement must not regress it (see `.planning/debug/resolved/monitor-sigterm-orphans-agent.md`).
- Tests must isolate `HOME` and `XDG_*` per child process; no process-global `set_var` (D-07; TEST-01).

### Integration Points
- `launch_stage` → `spawn_monitor` (and `spawn_agent_and_record`, `pipeline_launch.rs:1007`, which saves `monitor_pid`
  today).
- Liveness consumers: `status`, `cleanup`, `doctor` (`build_phase_facts`, `check_dead_monitor`), and the
  `resume`/`start` live-run guards (48-19, 48-20).
- `stop` (`commands.rs:1882`): `stop_via_gate`, then `stop_via_lock`. Socket shutdown is added for the ALIVE case
  (D-04).
- `recover` (`main.rs:273`): the home for STALE teardown, unless planning picks otherwise.

</code_context>

<specifics>
## Specific Ideas

- The operator wants the pros and cons stated before committing to the riskier design. D-04 records the arguments
  against it that were presented; planning should not lose them.
- "Recoverable by design rather than by diagnosis" (the phase goal) is the test for every operator-facing message:
  each GONE and STALE report names the verb that repairs it.

</specifics>

<deferred>
## Deferred Ideas

- **999.130 split-lock state overhaul**: promoted to its own phase between 50 and 51 (D-11). Insert it with
  `/gsd-phase --insert 50 …` after this discussion.
- **999.143 agent-pid half and work-preserving repair**: stays in the backlog (D-12).
- **macOS verification**: no host and no CI; stays unverified (see Discretion). Linked to the deferred
  `chore/macos-ci` work.

</deferred>

---

*Phase: 50-addressable-monitor-liveness*
*Context gathered: 2026-10-10*

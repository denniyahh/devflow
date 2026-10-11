# Phase 50: Addressable Monitor Liveness - Context

**Gathered:** 2026-10-10
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace the PID-addressed monitor with a socket-addressed supervisor, so "is this run's monitor alive?" resolves to
exactly one of GONE / STALE / ALIVE from `state.json` plus the socket at the recorded path, with no PID read on the
answering path (SUPV-01, ROADMAP Phase 50 criteria 1–4). `MonitorLaunch::PipeOwning` (already a Rust supervisor) gains
an address; `MonitorLaunch::Legacy` (the detached `sh -c` script) is replaced by a Rust supervisor; both answer
identically. The 2.x → 3.x state-file break is explicit. This is the milestone's breaking change (major version).

**The operator widened the phase beyond the four liveness criteria** (D-01…D-06): the supervisor also owns teardown
through the socket (the full DEN-58 / #185 spike design), and three backlog items are folded in. Liveness remains the
spine; everything else hangs off the same socket.

**Scope changes planning must carry as plan tasks:**

- Amend ROADMAP Phase 50's entry so its criteria and requirement list name what this context adds: socket teardown,
  STALE backstop, sweep, signal handling, the 2.x upgrade verb, and the folded items 999.128, 999.134, 999.138,
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

### 2.x state break (Operator, 2026-10-10)

- **D-13 (Operator): An explicit `schema_version` field.** Every state a 3.x binary creates carries
  `schema_version: 3`. A missing field means pre-3.0, which is incompatible. Today `State` has no version field and
  every later field is `#[serde(default)]` (`crates/devflow-core/src/state.rs:15-34`, verified), so a 2.x file loads
  cleanly, and detection must not rely on a load failure. A 2.x state must never be reported as GONE (criterion 4).
  — **Reversibility:** one-way — the field is the published state-file contract for every later version.
- **D-14 (Operator): Read-only verbs report; everything else refuses.** `status` and `doctor` show the phase as
  "incompatible: written by DevFlow 2.x" and never as GONE. Every verb that writes state or launches (`start`,
  `resume`, `advance`, `stop`, `ship`, gate answers, …) refuses with a named error that names the way out (D-15).
- **D-15 (Operator): An explicit, operator-invoked upgrade verb.** For example `devflow recover --upgrade-state --phase
  N` (name is Claude's call). It refuses while the 2.x state's recorded monitor or agent pid is alive. This is the
  only place 3.x still reads a pid for liveness, and it reads a 2.x record by necessity. Otherwise it stamps
  `schema_version: 3`, keeps the worktree and branch, and leaves the phase resumable with `resume`. It never runs
  automatically.

### Proof bar (Operator, 2026-10-10)

- **D-16 (Operator): Tests plus discriminating fixtures, plus the full Phase 49 live rig.** The criterion-1 fixtures
  each produce exactly one of GONE / STALE / ALIVE and nothing else, and the same holds for both `MonitorLaunch`
  variants (criterion 2). On top of that comes a full disposable-project run to Ship on the new binary, reusing Phase
  49's rig (`scripts/scratch-dogfood-repo.sh`, `49-…/tools/`): a fresh private throwaway GitHub repo per attempt,
  `--agent claude`, the seeded `blocking-human` checkpoint, and `--yes-ship`. The seeded checkpoint's auto-decide
  resume goes through `relaunch_checkpoint_session`, which hardcodes `MonitorLaunch::Legacy`
  (`crates/devflow-cli/src/pipeline_launch.rs:1236-1281`, verified), so the run crosses both variants. *Unverified:*
  that Phase 49's shipped attempts actually reached that relaunch; research confirms it from `49-evidence/` events.
  If the run cannot be shown to cross the replaced `Legacy` path, the plan must cross it another way (for example a
  non-Claude agent stage).
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
- **The upgrade verb's exact name and flag**, and the exact refusal texts.
- **Plan count and wave layout**, with D-07 (test `HOME` isolation) sequenced before any sweep test.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirement and scope
- `.planning/ROADMAP.md` — Phase 50 entry (goal, sequencing note, criteria 1–4); backlog entries 999.128, 999.130,
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
- `MonitorLaunch` and `spawn_monitor` / `spawn_monitor_inner` (`crates/devflow-core/src/monitor.rs:267,296,306`): the
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
- `launch_stage` → `spawn_monitor` (and `spawn_agent_and_record`, `pipeline_launch.rs:992`, which saves `monitor_pid`
  today).
- Liveness consumers: `status`, `cleanup`, `doctor` (`build_phase_facts`, `check_dead_monitor`), and the
  `resume`/`start` live-run guards (48-19, 48-20).
- `stop` (`commands.rs:1882`): `stop_via_gate`, then `stop_via_lock`. Socket shutdown is added for the ALIVE case
  (D-04).
- `recover` (`main.rs:273`): the home for STALE teardown and the upgrade verb, unless planning picks otherwise.

</code_context>

<specifics>
## Specific Ideas

- The operator wants the pros and cons stated before committing to the riskier design. D-04 records the arguments
  against it that were presented; planning should not lose them.
- "Recoverable by design rather than by diagnosis" (the phase goal) is the test for every operator-facing message:
  each GONE, STALE and incompatible report names the verb that repairs it.

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

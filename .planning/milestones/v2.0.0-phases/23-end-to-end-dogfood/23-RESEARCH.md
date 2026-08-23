# Phase 23: End-to-End Dogfood - Research (RE-AIM after 23-02's probe)

**Researched:** 2026-07-25 (rewrite — supersedes the 2026-07-25 version written
before the probe ran)
**Domain:** DevFlow's own gate/state-machine persistence and process model
(`devflow advance`, `Gates`, per-phase file lock); a minimal `devflow stop`;
the false-green attestation class that actually blocked Ship twice; CLI
surface reduction (`sequentagent`)
**Confidence:** HIGH for everything under "NEW — the re-aimed research"
below (grounded in source re-read this session, line-cited, and in the two
evidence documents this rewrite is required to treat as ground truth).
MEDIUM/LOW items are flagged inline and logged in Assumptions.

---

## Why this document was rewritten, in one paragraph

Phase 23's own probe (`23-02-PLAN.md` Task 1, recorded in
`23-PROBE-FINDINGS.md`) ran the original plan's leading unit and produced
evidence that **contradicts** the premise the previous version of this
document (and 23b/23c's original scope) was built on. The `sh -c` monitor
does not die — it ran correctly for 59 minutes, 11 stage launches, 3
correctly-counted consecutive failures, and was still alive at the end. Two
independent runs on this machine reached **Ship**, and both were stopped by
content/config gates, not process failures. Separately,
`23-ORPHAN-FORENSICS.md` found 27 real orphaned process pairs (54 processes,
168.6 MB) on this operator's machine, and traced their common cause to a
single, precise mechanism verified in this session against live source:
**`devflow advance` blocks in the foreground, holding a per-phase lock, on a
`Gates::poll_response` call whose default timeout is 7 days** — not literally
infinite, but long enough that every abandoned gate leaves a resident process
pair for hours to days. This document replaces the old "replace the monitor"
research with research into **bounding that wait, building a minimal
`devflow stop`, and fixing the false-green attestation class** — the three
things the operator's 2026-07-25 replan decision asked this rewrite to make
executable.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

*Unchanged from the original CONTEXT.md — reproduced verbatim. The probe's
findings do not revoke any of these decisions; they change which units need
which amount of new implementation to satisfy them (see "Re-Aimed Scope"
below for the mapping).*

### Locked Decisions

**Dogfood probe and acceptance (23a)**
- D-01: 23a's probe runs in a scratch repo, not this checkout (blast-radius isolation post-999.37). Reversible.
- D-02: The phase's final acceptance run is self-hosted (this repo), only after the scratch probe has proven the supervisor — because `staleness_outcome` (`crates/devflow-cli/src/staleness.rs:276-284`) only hard-blocks `(is_self_dogfood: true, Stale)`; a scratch-only acceptance would never exercise the most frequent observed dogfood killer. Reversible.
- D-03: Probe output is a recorded artifact (events.jsonl excerpts + `.devflow/phase-N-*` captures), not a verbal report. 23a is explicitly permitted to invalidate the rest of this phase's scope. **This fired — see below.**

**Unattended semantics and `--yes-ship`**
- D-04: Add a `--yes-ship` pre-authorization flag. Required because `Mode::Auto` does not gate-skip Ship — `crates/devflow-core/src/mode.rs:82-94`/`96-105` documents "Ship always gates (both modes)". Costly to reverse (user-facing CLI flag on the irreversible merge/version/changelog path).
- D-05: `--yes-ship` is a per-run flag only — never config-persistable. Must be typed on each invocation; must not be settable in `devflow.toml`. Costly to reverse.
- D-06: `--yes-ship` auto-answers the gate rather than bypassing it — the gate still fires and still records an explicit pre-authorized approval in `events.jsonl` and the gate ledger. Reversible.
- D-07 (ACCEPTED RISK): `--yes-ship` is NOT refused on the self-dogfood workspace. Combined with D-02, the acceptance run will unattended-merge a real phase into `develop`, bump the version, and append the changelog on this repository. Suggested mitigations for the planner to encode, not re-decide: drive a low-stakes phase for the acceptance run; establish a recovery point (tag or branch) before it starts. One-way.

**Supervisor migration (23b)**
- D-08: Big-bang replacement, no feature flag — delete the `sh -c` path outright. One-way; ~8 files. **Re-scoped by this rewrite: this decision applies IF AND WHEN the socket supervisor is built. The probe/forensics evidence shows the phase's acceptance criterion does not require it — see "What survives" below. D-08 is not revoked; it is deferred alongside the rest of the supervisor build.**
- D-09: Already decided, do not re-open — socket lives in `~/.cache/devflow/` (not `$XDG_RUNTIME_DIR`, which systemd deletes on logout); socket path is stored in `state.json`, never derived at probe time; liveness is `connect()` → GONE/STALE/ALIVE with no PID on the happy path; pgid backstop applies only when STALE, guarded by `start_time` + `boot_id`. Costly to reverse. **Same re-scoping as D-08: this remains the correct design IF the supervisor is later built; the `~/.cache/devflow/` location is reused below for the new cross-root gate registry regardless of whether the socket mechanism itself is built.**
- D-10: The `advance` tail runs in-process — the monitor becomes `devflow` itself and calls advance directly, removing the Phase 17 failure mode by construction. Reversible within the new design. **This rewrite finds a NEW, narrower justification for D-10 (see Research Question A/B below) — it is still not required for the MVP.**

**`sequentagent` removal (23d)**
- D-11: Hard delete the `sequentagent` verb (`crates/devflow-cli/src/main.rs:159`, dispatch at `:483`). One-way — removes a published CLI command from a crates.io-released binary.
- D-12: This removal earns the v2.0.0 slot — assume a major version bump, not minor. One-way.
- D-13: The capability intent is preserved (DEN-67/999.42, not discarded) — reimplemented on the supervisor when a second agent is supported, prerequisites DEN-58 + DEN-56. Reversible.

**None of D-11/D-12/D-13 are touched by the probe.** 23d is untouched scope —
see Scope Verdict table in `23-PROBE-FINDINGS.md` and the re-confirmed
inventory below.

### Claude's Discretion

- **Supervisor signal handling.** DEN-58 notes the spike installs no handler, so SIGTERM to the monitor leaves a stale socket (degrades correctly via sweep + pgid backstop, but production "should" trap SIGTERM/SIGINT and perform the same clean shutdown as the socket `shutdown` command). Land in 23b or defer — must be an explicit, recorded call. **This rewrite finds the equivalent gap already exists in the CURRENT (non-socket) monitor script and is load-bearing for `devflow stop`'s design — see Pitfall "SIGTERM to the monitor does not reach the advance tail" below.**
- **Scratch-repo scaffolding for 23a** — minimum `.planning/` + GSD structure the probe target needs to be a valid devflow target. **Resolved and executed by 23-01; see `23-PROBE-SETUP.md`. Not re-litigated here.**
- **In-flight-phase behaviour across the D-08 upgrade** — whether a phase whose `state.json` predates the `supervisor` field should be refused with guidance, or handled some other way. **Moot for the MVP scope below (no new `supervisor` field is introduced); remains relevant only if/when the socket supervisor is eventually built.**
- **Whether `hooks_after_ship` gains a `WorktreeRemove` step** and whether per-phase capture files get swept (DEN-59's operator note) — both untested-on-success paths this phase is first to exercise. **Still open; unaffected by the re-aim, see "Carried forward: hooks_after_ship" below.**

### Deferred Ideas (OUT OF SCOPE)

- `--yes-ship` refusal on the self-dogfood workspace — considered and declined this phase (D-07); ready-made mitigation if the accepted risk proves uncomfortable.
- 999.31/DEN-56 Modular Agent Driver — deferred, Codex blocker not Claude.
- 999.25/DEN-50 release-cut executor — crates.io publish stays manual.
- 999.42/DEN-67 agent failover on token exhaustion — preserved intent, blocked on DEN-58 + DEN-56.
- macOS verification — DEN-58 flags it as the single largest unknown; out of scope, do not claim macOS support from this phase.
- 999.38/DEN-65 test-suite PATH race; 999.39/DEN-66 production git calls inherit a redirecting environment; the old Test Suite & CI Hardening theme (999.15/17/18/19/20/22).
- **NEW this rewrite:** rate-limit gates that are unresolvable by construction (`23-ORPHAN-FORENSICS.md` point 4 — a probe died at `define` with `status: "ratelimited"`, no parseable retry time, no scheduled auto-resume). Real finding, no locked decision exists for it. See Open Questions — the planner should make an explicit call (fix now vs. defer), not silently drop it.
</user_constraints>

<phase_requirements>
## Phase Requirements (re-aimed)

No REQ-IDs exist for this phase. The unit table below replaces the original
one — units keep their letters for continuity with 23-CONTEXT.md/ROADMAP.md,
but three of the four descriptions changed to match what the probe proved is
actually required.

| ID (unit) | Status | Description (re-aimed) | Research Support |
|-----------|--------|--------------------------|-------------------|
| 23a | **COMPLETE** (23-01, 23-02, merged) | Dogfood probe — ran, found the process model was never the blocker. Do not re-plan. | `23-PROBE-FINDINGS.md`, `23-ORPHAN-FORENSICS.md` |
| 23b | **REDEFINED** | Bound gate lifetime: cross-root enumeration of `gate_pending` roots + an aged-gate auto-reject mechanism, reusing `Gates::respond` verbatim. The full socket-addressable supervisor is now optional/deferred — see "What Survives" below. | Research Question A |
| 23c | **REDEFINED, smaller** | `devflow stop` — buildable directly against the existing per-phase lock file and `Gates::respond`, without waiting on a supervisor. | Research Question B |
| 23d | **UNCHANGED** | Drop `sequentagent` (subtractive). Independently valid; the probe never touched this code path. | 23d Deletion Inventory (carried forward, re-confirmed unchanged) |
| (cross-cutting) | **UNCHANGED, but now the phase's actual bottleneck** | `--yes-ship` pre-authorization flag (D-04..D-07) | See "How --yes-ship threads through" (carried forward) |
| (cross-cutting, **NEW**) | **NEW — added by this replan** | A Ship-stage structural evidence check (git merge/tag/remote), independent of the agent's own self-report, so a false-green `VERIFICATION.md` cannot reach an approved Ship gate a second time. | Research Question C |
</phase_requirements>

---

## Summary

Phase 23's mechanism-level design work from the original research (the
socket-supervisor spike, the migration inventories, the `--yes-ship` wiring)
is still technically sound, but the probe proved the phase does not need most
of it to hit its actual acceptance criterion. The real blocker the operator's
machine has been living with is narrower and cheaper to fix than "replace the
monitor": **`devflow advance`, once it reaches a stage that fires a gate,
blocks in the foreground holding a per-phase file lock
(`crates/devflow-core/src/lock.rs:1-11`, confirmed by its own doc comment),
polling `Gates::poll_response` with a default 7-day timeout
(`crates/devflow-cli/src/config_parse.rs:16-27`)**. Every phase that gates and
is then abandoned — including the 24 of 27 orphans that were pointed at empty
scratch directories and gated instantly and correctly — leaves that process
pair running for up to a week. Two runs on this exact machine, in the same
hour, both reached Ship and were stopped by content/config problems (a
false-green `VERIFICATION.md`, and a missing `SECURITY.md` under an enforcing
gate), not by a dead or ambiguous monitor.

**Primary recommendation:** build the three things the probe actually
justifies, in this order, without the supervisor rewrite:

1. **Bound gate lifetime (23b, redefined).** Add a lightweight, project-root
   registry under `~/.cache/devflow/` (same directory D-09 already locked in
   for a different reason) that `spawn_monitor`/`advance` register into and
   deregister from. A new `devflow gate sweep` (or `doctor --fix`) command
   enumerates every registered root's `gate_pending` state via the *already
   existing* `Gates::list_open` (per-root) and, for gates older than an
   operator-configurable threshold, writes a rejection `GateResponse` via the
   *already existing* `Gates::respond` API — the exact same "auto-answer, not
   bypass" mechanism `--yes-ship` uses. The still-alive, still-polling
   `devflow advance` process picks the response up on its own next
   backoff-capped poll (≤60s) and tears itself down through its own existing
   `abort()` path. **No process is ever signalled by this mechanism** — it
   only writes a file the target process already knows how to read.
2. **`devflow stop` (23c, redefined, smaller).** For the case where nobody
   wants to wait even 60 seconds for the sweep's write to be noticed (or the
   process is genuinely stuck earlier than the gate poll), `devflow stop
   --phase N` reads `.devflow/lock-{phase:02}` — which **already contains the
   PID of the exact process to signal**, because `lock::acquire`
   (`crates/devflow-core/src/lock.rs:32-34`) writes `std::process::id()` of
   whichever process is currently running `advance()`. SIGTERM to that PID
   is safe at any point during the gate poll (nothing is mid-write — the gate
   file and `state.gate_pending = true` are persisted *before* the poll loop
   starts, `pipeline_gate.rs:250-283`). This requires **zero new dependency
   and zero supervisor** — it is a ~30-line CLI command against APIs that
   already exist.
3. **Fix the false-green attestation class (new cross-cutting item).** Both
   Ship blocks recorded in the orphan forensics were content/config problems,
   not devflow-core defects — but devflow-core currently has **no structural
   check of its own** that a Ship approval corresponds to a real merge/tag.
   The existing `evaluate_layer0` (`crates/devflow-core/src/agent_result.rs:704-796`)
   already proves the *pattern* — an operator-approved external probe that
   outranks the agent's own self-report — for arbitrary declared commands.
   Recommend a narrow, default-on (not opt-in like Layer 0 today) Ship-stage
   evidence check built the same way.

`sequentagent` deletion (23d) is unaffected and should proceed exactly as
originally scoped — the probe never exercised that code path and the
inventory below is re-confirmed unchanged (no `crates/` files were touched by
23-01/23-02).

**The socket-addressable supervisor (23b's original scope) is deferred, not
discarded.** Its design remains spike-proven and is the right long-term
architecture for portability and true liveness-ambiguity resolution, but
nothing in the two evidence documents shows the phase's stated acceptance
criterion — "no manual `ps`, no manual `devflow advance`, no silent stall" —
requires it. Building it now would cost ~8 files' worth of migration to fix a
problem (monitor death) the probe did not find, while leaving the actual
problem (unbounded gate wait + no enumeration + no stop) exactly as
unaddressed as a supervisor rewrite with no TTL/reaper would leave it (per
`23-ORPHAN-FORENSICS.md`'s own explicit warning: *"a supervisor that owns the
agent is worth building if and only if it also bounds gate waits... otherwise
it reproduces the leak with better logging"*).

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Gate wait bounding / cross-root enumeration | Backend/CLI process (devflow-core `gates.rs` + new registry module) | Filesystem (`~/.cache/devflow/`) | Pure extension of the existing file-based gate protocol; no OS-level rendezvous point needed |
| `devflow stop` | Backend/CLI process (devflow-cli, new `commands.rs` verb) | OS (`kill()` on the lock-file PID) | Targets a single already-known PID via an already-existing lock file; no new process-group or socket primitive required |
| Ship-stage structural evidence check | Backend/CLI process (devflow-core `agent_result.rs`, new default-on Layer-0-shaped check) | Git (repo tier, read-only queries) | Mirrors `evaluate_layer0`'s existing "external evidence outranks self-report" pattern exactly |
| `sequentagent` removal | CLI surface (devflow-cli `main.rs`/`parallel.rs`) | — | Pure subtraction; unaffected by the re-aim |
| Socket-addressable supervisor (DEFERRED) | Backend/CLI process | OS (Unix socket + pgid) | Still the correct tier if/when built; not required for this phase's acceptance criterion |
| Scratch-repo probe target (23a, COMPLETE) | Filesystem / git (repo tier) | — | Fixture, not a capability; already delivered |

---

## NEW — the re-aimed research

### Research Question A — Bounding gate lifetime (the orphan fix)

**What actually happens today, verified line-by-line:**

1. `crates/devflow-cli/src/pipeline_launch.rs:290-298` — `advance()` acquires
   a **per-phase** lock (`lock::acquire`) before doing anything else. The
   lock's own module doc comment states plainly why it is per-phase and not
   per-project: *"`advance()` holds it across a gate's multi-day blocking
   wait"* (`crates/devflow-core/src/lock.rs:8-9`). This is the load-bearing
   fact for the whole question: **the block is already a known, named,
   documented design property of this codebase**, not an accidental omission.
2. If the stage's outcome fires a gate (Ship's routine approval, Validate's
   retry-exhaustion gate, or the "never-silent" catch-all for any stage), the
   call chain reaches `run_gate_with_timeout`
   (`crates/devflow-cli/src/pipeline_gate.rs:243-319`):
   - `state.gate_pending = true` is persisted, and `Gates::write_gate` writes
     the request file, **both before** the poll starts
     (`pipeline_gate.rs:250-252`).
   - `Gates::poll_response` (`crates/devflow-core/src/gates.rs:222-248`) then
     polls with exponential backoff (1s → 2s → 4s … capped at 60s) until
     `timeout_secs` elapses.
   - `timeout_secs` defaults to `gate_timeout_secs()`
     (`crates/devflow-cli/src/config_parse.rs:16-27`), whose doc comment says
     **"falling back to 7 days on"** an unset `DEVFLOW_GATE_TIMEOUT_SECS`. A
     separate, much shorter default (60s,
     `foreground_gate_timeout_secs`, `config_parse.rs:30-55`) exists but is
     used **only** by `ship_override`'s foreground retry-gate wait
     (`pipeline_gate.rs:176-181`) — it is not the value the detached monitor
     uses for a routine gate.
3. **On timeout** (the `None` branch, `pipeline_gate.rs:307-317`): a
   `gate_timeout` event is emitted and an `Err` is returned. That `Err`
   propagates out of `advance()`, `main()` prints it and calls
   `std::process::exit(1)` (`crates/devflow-cli/src/main.rs:384`). The
   process **does** eventually terminate — this refines the orphan-forensics
   framing of "no timeout" to "a timeout that defaults to 7 days," which for
   an operational leak (168.6 MB / 54 processes / up to 30h observed) is
   effectively the same problem. **Critically, `state.gate_pending` is never
   cleared on this path** (only the `Some(response)` branch at line 286
   clears it) — so even after the natural 7-day timeout fires, the phase's
   `state.json` still claims a gate is pending until a human runs `devflow
   resume`/`advance` again. This IS already correctly detectable —
   `state.monitor_pid`'s liveness goes to `false` once the `sh -c` shell (see
   below) also exits, and the existing (monitor dead, agent dead) → `Stuck`
   classification (Finding 1's table, still implemented in `commands.rs`'s
   `liveness()`) already reports this correctly. **The existing
   observability model is not broken for this case — it is just too slow to
   matter operationally at a 7-day default.**
4. **What kills the leak, concretely, without any of this:** because
   `run_gate_with_timeout` writes the gate file and `gate_pending` *before*
   polling, and `Gates::respond` (`gates.rs:179-198`) simply writes a
   response file that the same live process's own poll loop will pick up on
   its very next iteration (≤60s later, by construction of the backoff cap),
   **the cheapest possible "TTL" implementation writes no kill signal at
   all** — it writes a `GateResponse{approved: false, note:
   Some("...abort...".into()), responded_by: Some("devflow-reap")}` via the
   *existing* `Gates::respond`. `GateAction::from_response`
   (`gates.rs:69-79`) turns `approved: false` + a note containing "abort"
   into `GateAction::Abort(note)`, and the already-running `advance()` call
   drives its own clean `abort()` (`pipeline_gate.rs:322-335`): gate files
   cleaned up, `workflow::clear_state` called, `workflow_aborted` emitted.
   The `sh -c` monitor's own script has nothing after the `devflow advance`
   line, so once `advance` exits cleanly the monitor process exits right
   behind it — **no orphan, no kill(1), no supervisor**, using only APIs that
   already exist and are already tested (`Gates::respond` is exercised by the
   existing `--yes-ship`/`ship_override`/`gate_respond` code paths).

**What would break if `devflow advance` stopped blocking (the "detach
instead" option), traced explicitly:** the per-phase lock's own doc comment
(`lock.rs:8-9`) already explains the design intent — a project-wide lock
would starve `devflow parallel`'s sibling phases, so the lock is scoped
per-phase precisely so ONE phase's multi-day gate wait cannot block another
phase's advance. Making `advance` non-blocking would require reintroducing
*some* long-lived process to own the poll loop and eventually call `abort`/
`transition` once a response appears — which is exactly what the supervisor
would be. There is no way to "just not block" without either (a) building a
supervisor (the deferred option), or (b) polling from an external cron/sweep
process instead of the same process the gate was raised from — which is
precisely what the sweep/reap mechanism above already is, minus the
supervisor. **Recommendation: do not detach `advance`. Keep it blocking (it
correctly holds the per-phase lock and correctly represents "this phase is
mid-workflow" for the whole time it's gated); add an external, periodic sweep
that resolves aged gates via the existing response-file protocol.** This is
not a menu item — it is the only option of the four that requires zero new
process-lifecycle code.

**Gate-state persistence, and the minimum change to let a gate expire without
losing never-fail-silently:**

- `.devflow/gates/{phase:02}-{stage}.json` (`GateFile`, `gates.rs`) already
  carries a `timestamp` field (`unix_now()`, `gates.rs:333-337`) — age is
  already computable with **zero new persisted fields**.
- `state-{phase:02}.json`'s `gate_pending: bool` (`state.rs:28`) is already
  the exact signal a sweep needs to find candidates, combined with
  `Gates::list_open` (`gates.rs:140-172`, already returns `phase`, `stage`,
  `context`, `timestamp` per open gate) — **this enumeration primitive
  already exists, per-project-root.** No new persisted field on `State` or on
  the gate file is required for the sweep itself.
- The never-fail-silently guarantee is **preserved by construction**, not
  weakened, because the sweep's rejection is written through the exact same
  `Gates::respond` → `gate_resolved` event path a human's answer takes
  (`pipeline_gate.rs:284-306`) — the audit trail shows a real, timestamped,
  `responded_by: "devflow-reap"` decision, not a silently-vanished gate. This
  is the same "auto-answer, not bypass" shape D-06 already locked in for
  `--yes-ship`; extending it to a second, symmetric auto-*rejecter* is a
  natural, minimal reuse, not a new mechanism.

**Enumeration — what it would take to answer "what is gated on this
machine," verified as genuinely absent:**

- `Gates::list_open` is scoped to one `project_root` — confirmed by its
  signature (`gates.rs:140`) and every call site (`gate_show`, `gate_respond`,
  `doctor`'s `collect_phase_facts` in `commands.rs`). There is **no existing
  cross-root registry anywhere in this codebase** — confirmed by grepping for
  `~/.cache/devflow`, `registry`, and `sweep` across `crates/`: zero
  production hits. `23-ORPHAN-FORENSICS.md`'s own account of how it was
  written ("assembled with `ps` and `find`") is independent, direct
  confirmation from the operator's own experience.
- **Recommendation:** a small, append-only registry file,
  `~/.cache/devflow/roots.json` (reusing D-09's already-locked-in directory
  choice for an unrelated-but-compatible reason — it survives logout and has
  ample `sun_path`-class headroom for a plain JSON file, which has no length
  constraint at all since it is not a socket path). `launch_stage_inner`
  (`pipeline_launch.rs:55-149`, the same function that already writes
  `state.monitor_pid` at line 131) registers `(project_root, phase)` into it
  at spawn time; `advance()`'s terminal paths (`abort`,
  `finish_workflow_with_gate_timeout`'s success path) deregister it. A sweep
  reads this file, then re-uses `Gates::list_open` per listed root — no new
  per-root state needed beyond what already exists. **This is a new artifact
  (does not exist today), but it is the only new persisted state this
  question's recommendation requires.**

---

### Research Question B — `devflow stop` (was unit 23c)

**Can a useful `devflow stop` be built without the full socket supervisor?
Yes — the exact mechanism to signal already exists and was verified this
session by tracing the shell script `spawn_monitor_inner` generates
(`crates/devflow-core/src/monitor.rs:148-160`):**

```
apid=''; cleanup() { [ -n "$apid" ] && kill "$apid" 2>/dev/null; exit 0; }
trap cleanup TERM INT
cd '<workdir>' || exit 1
"$@" > '<stdout>' 2>'<stderr>' &
apid=$!; echo $apid > '<pid_file>'
wait $apid; echo $? > '<exit_file>'; <binary> advance '<project_root>' --phase <N>
```

The `wait $apid; echo $?; advance ...` line is **one compound statement**,
executed sequentially by the same `sh -c` process, not backgrounded. Two
consequences, both verified against the script text and against
`lock.rs`/`advance()`'s own behavior, and both new findings this session
(neither the original research nor the two evidence documents state this
explicitly):

1. **The `trap cleanup TERM INT` only ever kills `$apid`** — the variable
   captured for the *agent* process, set once, before `advance` ever runs.
   By the time `advance` is the sh script's active foreground child, `$apid`
   already refers to a long-exited process. **Sending SIGTERM to the `sh -c`
   monitor PID (`state.monitor_pid`) while it is running the trailing
   `advance` line does not terminate `advance` — it orphans it**, exactly the
   Phase-17-shaped bug the original hypothesis was about, but for the
   *advance tail* specifically rather than the agent. This is a genuinely new
   failure mode discovered by this rewrite, not previously documented
   anywhere in this project's planning history.
2. **The correct target is the `devflow advance` process itself, and it is
   already trivially findable.** `lock::acquire` (`lock.rs:32-34`) writes
   `std::process::id()` — the CURRENT process's own PID, i.e. the `advance`
   invocation itself — into `.devflow/lock-{phase:02}`. That file is created
   the moment `advance()` starts and is held for the entire duration of any
   gate wait. **A minimal `devflow stop --phase N` needs only: read
   `.devflow/lock-{phase:02}`, confirm the PID is alive
   (`agent::agent_running`, already exists and is already hardened per
   Finding 1), and `kill(pid, SIGTERM)` it.** Once that process exits (however
   it exits — cleanly via a caught signal, or simply killed), the `sh -c`
   monitor's `wait`/foreground-child-exited condition resolves and the
   monitor itself falls off the end of the script and exits on its own —
   **no second signal, no process-group tracking, no pgid backstop needed**,
   because there is nothing left in the script after the `advance` line.

**What this MVP is missing relative to the full supervisor design, stated
plainly:** a bare `kill(pid, SIGTERM)` on the `advance` process interrupts it
mid-`Gates::poll_response` with **no cleanup** — `state.gate_pending` stays
`true`, the gate file stays open, and the operator is left exactly where the
7-day-timeout path already leaves them (a `Stuck` classification, recoverable
via `devflow resume`). **Recommendation: prefer the write-a-rejection-
response approach from Research Question A as `devflow stop`'s primary
mechanism too** (it produces a clean `workflow_aborted` event and a properly
cleared `state.json`, using code that already exists) **and use the
lock-file-PID `kill` only as the fallback** for the (rarer) case where
`advance` is not yet blocked on a gate — mid-`evaluate_agent_result`, for
instance — where there is no gate file to respond to. This gives `devflow
stop` two paths from one small function: if `Gates::list_open` shows an open
gate for the phase, write a rejection response and wait briefly (≤60s) for
the process to notice it and exit on its own; otherwise, fall back to a
direct `kill(pid, SIGTERM)` on the lock-file PID. Either path was verified
this session to require **zero new dependency, zero new process-lifecycle
primitive, and zero of the socket-supervisor spike's mechanism.**

**Interaction with `cleanup --force`'s deliberate refusal to touch live
processes** (`crates/devflow-cli/src/commands.rs:372-443`): `cleanup`'s
liveness check keys off `state.monitor_pid` via `agent::agent_running`
(`commands.rs:405-407`) — the `sh -c` process, which (per the trace above) IS
correctly alive for the entire duration a gate is pending, so `cleanup
--force` correctly refuses to touch it today, exactly as designed. `devflow
stop` must be a **new, separate verb**, not a flag on `cleanup`: `cleanup`'s
whole contract is "remove a worktree for a phase that has already finished or
is safely dead," and its refusal on a live phase is a deliberate safety
property (D-06 in an earlier phase's own decisions, "no override flag" —
confirmed by the doc comment at `commands.rs:375`). Overloading `cleanup
--force` to also mean "stop the live thing first" would weaken a safety
property a previous phase specifically hardened. **`devflow stop` should be
the verb that changes a phase's liveness from live to dead; `cleanup --force`
should keep refusing to touch anything still live, and only be usable
afterward** — exactly the sequencing CONTEXT.md's own Integration Points
section already anticipated ("`cleanup --force` — deliberately refuses...
`devflow stop` (23c) becomes the verb that actually stops one").

---

### Research Question C — The false-green attestation class

**What produced a `01-VERIFICATION.md` that scored the Ship stage `VERIFIED`
while Ship had never run, and where does the catch actually happen?**

Traced through devflow-core's own evaluation pipeline
(`crates/devflow-core/src/agent_result.rs`) this session: `VERIFICATION.md`
is **not a devflow-core artifact and devflow-core never reads it.**
`evaluate_agent_result`/`evaluate_agent_result_inner` (`agent_result.rs:837+`)
consume only: (a) commit evidence on the phase branch, (b) the agent's own
`DEVFLOW_RESULT` JSON payload (its self-reported `status`/`verdict`), and (c)
optionally, `evaluate_layer0`'s operator-declared external probe commands
(`agent_result.rs:704-796`, gated by `crate::config::external_verify_enabled`
— **opt-in, off by default**). None of these three inspect the *content* of a
GSD-produced document like `VERIFICATION.md`. The catch that actually
happened — "review: CR-01 (Critical) — `01-VERIFICATION.md` scores...
VERIFIED... but Ship never ran: `git log --merges --all` is empty, 0 tags, 0
remotes" (`23-PROBE-FINDINGS.md`'s `events.jsonl` line at
`ts:1785015595`) — happened **entirely inside the Claude agent's own GSD
Ship-stage workflow** (a review/audit step the agent itself ran as part of
its `/gsd-ship`-family prompt chain), which then reported `status: "failed"`
back to devflow through the normal `DEVFLOW_RESULT` contract. **This is a
GSD-prompt-side self-audit, not a devflow-core structural guard.** It worked
this one time because the agent's own review pass happened to catch it —
that is non-deterministic prompt behavior, not a code-enforced invariant, and
nothing in devflow-core would have caught the same defect if the agent's
review had missed it.

**Is there an existing check that could have caught "Ship scored verified but
no merge/tag/remote exists"?** No — confirmed by reading
`evaluate_layer0`/`evaluate_agent_result_inner` in full this session. The
closest existing pattern is `evaluate_layer0` itself: it already implements
exactly the right shape — "run an external, operator-approved probe; a
failing probe outranks the agent's own self-report" — just scoped to
arbitrary, opt-in, per-phase-declared shell commands, not to Ship
specifically. **Recommendation: add a narrow, default-on (not opt-in)
structural check that runs automatically whenever a stage's outcome is about
to be evaluated as a Ship approval** — e.g. before `handle_ship_outcome`
(`pipeline_outcomes.rs:275-286`, carried forward below) accepts an
`AgentStatus::Success` at `Stage::Ship`, independently confirm at least one
of: a new merge commit exists on the target branch since the phase's
`started_at`, or a new tag was created, or the remote was pushed to. This is
architecturally the same pattern as `evaluate_layer0`, narrowed to one
stage and enabled unconditionally (no `external_verify_enabled` opt-in,
since this is closing a specific, now-proven-real gap rather than adding a
general-purpose feature). **This is new, devflow-core-side work this replan
adds to the phase's scope** — it did not exist in the original 23b/23c/23d
unit breakdown.

**Is the `security_enforcement=true` + missing `SECURITY.md` block correct
behaviour, or a config-scoping defect? Verified against this repo's own
config, `.planning/config.json` (read this session):** the key is **absent**
from this project's `workflow` block, which — per this project's own
established convention throughout its planning history ("absent =
enabled") — means security enforcement **is active by default in this
repo, exactly as it would be in the scratch probe**. `scripts/scratch-dogfood-
repo.sh` was checked for any override (`rg -n security_enforcement`) and
found none — the scratch repo inherits the same default. **This means the
self-hosted D-02 acceptance run is NOT structurally exempt from the same
wall that blocked `devflow-probe-02`** — it is symmetric, not a scratch-only
artifact. Every prior phase in this repo (see `STATE.md`'s "Recently
Shipped" entries — Phase 21, Phase 20, Phase 18 each produced an
`NN-SECURITY.md`) has produced a `SECURITY.md` as a normal part of its own
Ship-stage workflow, so this is very likely a non-issue **in practice** for
a real phase driven by the standard GSD Ship-stage prompt chain — but it is
an assumption, not verified this session (no phase has yet completed an
end-to-end unattended run in this repo to confirm the artifact is always
produced before the preflight check runs). **Recommendation for the planner:
either (a) explicitly verify the chosen low-stakes acceptance phase's own
plan set includes a task that produces a `SECURITY.md` before Ship, or (b)
add a defensive check earlier in the phase's own pipeline (Validate stage) so
a missing `SECURITY.md` surfaces as an ordinary Validate gap rather than a
Ship-time preflight surprise discovered only after the phase has otherwise
completed.**

---

### Research Question D — What survives from the supervisor work

Read `.planning/audits/2026-07-24-socket-supervisor-spike.md` in full again
this session, cross-checked against A/B/C above.

| Supervisor design element | Survives? | Why |
|---|---|---|
| Socket-addressable liveness (GONE/STALE/ALIVE via `connect()`) | **DEFERRED, not required.** | The probe found monitor liveness was never actually ambiguous in the observed runs (`infra_failures: 0`; the monitor was correctly alive the whole time). PID-based liveness (`agent::agent_running`, already hardened) is adequate for the narrow, known-location kill/enumerate operations Question A/B need. Solves a real but different problem (Finding 1's "who watches the watcher" ambiguity) than the one this phase's acceptance criterion requires closed. |
| In-process `advance` tail (D-10) | **DEFERRED, but gains a new, narrower justification.** | Question B's finding — that SIGTERM to the monitor does not reach the `advance` child because the shell trap only tracks `$apid` — is a real argument *for* eventually folding `advance` into the same process as the monitor (one process, one signal handler, no "which PID do I actually target" lookup). But the lock-file-PID trick (Question B) already gives `devflow stop` a working, minimal target without this migration. Recommend: keep D-10 on the backlog as the long-term cleanup, not required now. |
| `~/.cache/devflow/` as a durable, logout-surviving location (D-09) | **PARTIALLY SURVIVES.** | The location decision (not `$XDG_RUNTIME_DIR`) is reused directly for the new cross-root gate registry (Question A) — same rationale (must survive logout), different payload (a JSON root list instead of a socket). The `sun_path`-length constraint (C6) that motivated the *directory choice* for sockets specifically does not apply to a plain file, but the directory itself is still the right, already-decided location. |
| pgid backstop / `killpg` teardown | **DEFERRED, not required.** | Question B's MVP kills exactly one known PID; there is no process tree to tear down because the shell script never backgrounds the `advance` tail (it's a plain foreground command), and the agent's own tree is already correctly torn down today by the existing `sigterm_to_monitor_also_kills_the_agent` regression test (`monitor.rs:340-382`, WR-08) for the *agent* case, which this rewrite did not find any evidence of being broken. |
| Cross-platform/container uniformity argument for sockets over cgroups | **STILL VALID, still not the deciding factor here.** | Correct reasoning, orthogonal to whether this phase needs to ship it now. |
| Takeover safety / a 2nd monitor refusing a live socket | **DEFERRED.** | Not exercised by anything in the MVP scope; no socket exists to take over. |

**`sequentagent` deletion (23d) never depended on the hypothesis — confirmed,
not merely asserted.** Re-ran the exact verification this session:
`git log --oneline` shows 23-01/23-02 touched only
`scripts/scratch-dogfood-repo.sh` and planning docs — `git status --porcelain
crates/` was clean before this session's reads, and no commit since the
original research's `rg` count (2026-07-25) has touched `crates/`. **The 142
references / 11 files inventory below is re-confirmed current, unchanged,
independently valid cleanup**, exactly as `23-PROBE-FINDINGS.md`'s Scope
Verdict table already states ("UNTOUCHED").

---

## Standard Stack

### Core

No new dependency for any of the three re-aimed units (A/B/C). Everything
they need is already a resolved dependency:

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `libc` | `0.2` (already a `devflow-core` dependency) `[VERIFIED: crates/devflow-core/Cargo.toml:19]` | `kill()` for `devflow stop`'s fallback path | Already used by `agent.rs`'s `agent_running` and `lock.rs`'s `pid_is_alive` — `devflow stop` reuses these, not a new call site |
| `serde` / `serde_json` | `1` (workspace) `[VERIFIED: Cargo.toml:21-22]` | The new `~/.cache/devflow/roots.json` registry file's (de)serialization | Existing pattern for every other JSON artifact (`GateFile`, `GateResponse`, `state-NN.json`) |
| `thiserror` | `2` (workspace) `[VERIFIED: Cargo.toml:25]` | Any new error variants for the registry/sweep/stop modules | House convention |
| `std::fs` / `std::process` | stable | Registry file I/O, `Command`/`kill` for `devflow stop` | Nothing beyond what `lock.rs`/`gates.rs`/`monitor.rs` already do |

**No socket, no `std::os::unix::net`, no `process_group`, no `sysinfo`-class
crate is needed for the re-aimed scope.** Those remain accurate
recommendations *if and only if* the deferred supervisor work is later
picked up (see Question D) — left in the "Alternatives Considered" table
below, carried forward, for that future phase's benefit.

### Alternatives Considered (carried forward — still correct if the deferred supervisor work is later picked up)

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Unix domain socket handle | cgroup v2 (`cgroup.kill`/`cgroup.procs`) | **Rejected by this project's own prior research** (`2026-07-24-process-teardown-solution-research.md`): does not work in containers even with delegation flags (verified empirically with rootless podman on this host). Do not resurrect. |
| Unix domain socket handle | `command-group` / `process-wrap` / `processkit` / `duct` / `daemonize` / `nix` / `rustix` | **Each ruled out for a recorded reason** in the original research (unserializable handle, tokio dependency in a sync codebase, terminal-detachment-not-teardown, cosmetic wrappers over the same `libc` calls). Do not re-propose without new evidence. |

**Version verification:** `libc`/`serde`/`serde_json`/`thiserror` re-verified
live against `Cargo.toml` this session (2026-07-25) — no drift since the
original research's check on the same day.

## Package Legitimacy Audit

**Not applicable.** The re-aimed scope (A/B/C) installs zero new external
packages — every primitive it uses (`libc::kill`, `serde_json`,
`std::fs`) is already a resolved workspace dependency, re-verified this
session. No package-legitimacy check is needed.

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

---

## Architecture Patterns

### System Architecture Diagram (re-aimed scope only)

```
devflow start --phase N
        |
        v
  launch_stage_inner (pipeline_launch.rs:55)  ── unchanged
        |  spawns monitor::spawn_monitor (unchanged sh -c body)
        |  NEW: registers (project_root, phase) into
        |       ~/.cache/devflow/roots.json
        v
  sh -c monitor (unchanged) ── owns agent, then runs `devflow advance` as
        |                      its own foreground child (unchanged)
        v
  advance() (pipeline_launch.rs:260) ── acquires per-phase lock (unchanged)
        |
        v
  evaluate_agent_result ── NEW: Ship-stage structural evidence check
        |                       (Research Question C) runs here, before a
        |                       Ship AgentStatus::Success is trusted
        v
  outcome fires a gate? ── run_gate_with_timeout (unchanged: write_gate,
        |                   gate_pending=true, poll_response, 7-day default)
        |
        +--> NEW: devflow gate sweep (external, periodic or on-demand)
        |         reads ~/.cache/devflow/roots.json
        |         → Gates::list_open per root (unchanged, already exists)
        |         → for gates older than threshold: Gates::respond
        |           {approved:false, note:"...abort...", responded_by:
        |           "devflow-reap"}  (unchanged API, new caller)
        |         → the still-polling advance() picks it up on its own
        |           next iteration (≤60s), runs its own abort() (unchanged)
        |
        +--> NEW: devflow stop --phase N (on-demand, operator-invoked)
                  reads .devflow/lock-{phase:02} for the advance PID
                  (unchanged file, new reader)
                  → if a gate is open: write the same rejection response
                    as the sweep above, wait briefly for self-teardown
                  → else: kill(pid, SIGTERM) directly (fallback only)
```

### Recommended Project Structure

```
crates/devflow-core/src/
├── gates.rs           # unchanged API surface; new caller sites only
├── lock.rs             # unchanged; devflow stop reads its existing file format
├── agent_result.rs      # + a narrow, default-on Ship evidence check
                          #   (new function, same shape as evaluate_layer0)
├── registry.rs          # NEW — ~/.cache/devflow/roots.json read/write,
                          #   the only genuinely new module this scope needs
crates/devflow-cli/src/
├── pipeline_launch.rs   # launch_stage_inner registers into the new registry
├── pipeline_gate.rs      # advance()'s abort() path deregisters
├── commands.rs            # NEW: `stop` command; NEW: `gate sweep` command
├── main.rs                 # NEW: Command::Stop, Command::GateSweep dispatch arms;
                             # `--yes-ship` flag added to Start (unchanged from
                             # original research); 23d's Sequentagent removal
                             # (unchanged from original research)
```

### Pattern 1: Auto-reject via the existing gate-response protocol (NEW)

**What:** Write a `GateResponse{approved: false, note: Some("...abort..."),
responded_by: Some("devflow-reap")}` for any gate older than a threshold,
using the unmodified `Gates::respond` API. No process is signalled.

**When to use:** The sweep's primary mechanism (Question A); `devflow stop`'s
primary mechanism when a gate is actually open (Question B).

**Example (illustrating the exact existing API, not new code):**
```rust
// Existing API, unmodified — crates/devflow-core/src/gates.rs
use devflow_core::gates::{Gates, GateResponse};

Gates::respond(project_root, phase, stage, &GateResponse {
    approved: false,
    note: Some("abort: gate exceeded max unattended age with no response".into()),
    responded_by: Some("devflow-reap".into()),
})?;
// The live `devflow advance` process (still polling Gates::poll_response,
// pipeline_gate.rs:284) picks this up on its own next backoff-capped
// iteration (≤60s) and runs its own clean `abort()` — pipeline_gate.rs:322.
```

### Pattern 2: Lock-file PID as `devflow stop`'s fallback target (NEW)

**What:** `.devflow/lock-{phase:02}` already contains the exact PID to
signal — the process currently running `advance()` for that phase.

**When to use:** `devflow stop`'s fallback path, only when
`Gates::list_open` shows no open gate for the phase (so there is nothing to
write a response to) but the phase is still recorded as active.

**Example:**
```rust
// Existing file format, unmodified — crates/devflow-core/src/lock.rs
let lock_path = project_root.join(".devflow").join(format!("lock-{phase:02}"));
if let Ok(pid) = std::fs::read_to_string(&lock_path) {
    if let Ok(pid) = pid.trim().parse::<u32>() {
        if devflow_core::agent::agent_running(pid) {
            unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
        }
    }
}
```

**Anti-pattern to avoid:** sending SIGTERM to `state.monitor_pid` (the `sh
-c` process) expecting it to also terminate the `advance` child. Verified
this session (Question B) that the script's trap only ever tracks `$apid`
(the agent), never the `advance` tail — this WILL orphan `advance` rather
than stopping it.

### Pattern 3: Ship-stage structural evidence check (NEW, Question C)

**What:** Before trusting an `AgentStatus::Success` at `Stage::Ship`,
independently confirm git evidence exists — mirrors `evaluate_layer0`'s
existing "external probe outranks self-report" shape, narrowed to one stage
and made default-on.

**When to use:** Inside `evaluate_agent_result_inner`'s Ship-stage path, or
as a new check `handle_ship_outcome` (`pipeline_outcomes.rs:275-286`, carried
forward below) consults before accepting the gate approval.

```rust
// Illustrative shape only — planner sizes the actual diff.
// Mirrors evaluate_layer0's existing pattern (agent_result.rs:704-796):
// an external, structural signal outranks the agent's own self-report.
fn verify_ship_evidence(project_root: &Path, state: &State) -> bool {
    // e.g.: any new merge commit on the target branch since state.started_at,
    // OR a new tag, OR evidence of a remote push — planner picks the cheapest
    // reliable signal against this project's actual git-flow shape
    // (crates/devflow-core/src/git.rs already has the merge/tag primitives
    // hooks.rs's own `hooks_after_ship` uses).
    todo!()
}
```

### Anti-Patterns to Avoid

- **Rebuilding the full socket supervisor to fix this phase's acceptance
  criterion.** Per Question D: none of A/B/C require it. If a future phase
  needs true liveness-ambiguity resolution (Finding 1's original problem),
  build it then, against fresh evidence that ambiguity is actually occurring
  — this probe found it was not.
- **Treating the 7-day `gate_timeout_secs()` default as "no timeout."** It is
  a real, already-implemented, already-tested timeout — just too long for
  unattended-orphan purposes. Do not reintroduce a duplicate timeout
  mechanism; either lower the effective default for background contexts or
  add the sweep above (both are compatible, complementary, and neither
  requires touching the other).
- **Sending SIGTERM to `state.monitor_pid` as `devflow stop`'s only
  mechanism.** Verified this session to orphan the `advance` tail rather than
  stopping it (Pattern 2's anti-pattern note above).

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| Gate expiry / auto-rejection | A new kill-based reaper, a new response-file format, a new event type | `Gates::respond` + the existing poll loop's own pickup (≤60s) | The exact mechanism already exists and is already tested via `--yes-ship`/`ship_override`/`gate_respond`; a new mechanism would duplicate it with a different, unaudited shape |
| Finding the right process to stop | A pgid/process-group walk, a new PID-tracking file | `.devflow/lock-{phase:02}`, which already names the exact process | The lock file already exists for a different reason (serializing `advance` invocations) and happens to be the perfect stop-target registry with zero new writer code |
| Cross-root discovery | A `find`/`ps`-based ad hoc sweep (what the forensics doc had to resort to) | A small, explicit registry file at `~/.cache/devflow/roots.json`, written by the same code path that already writes `state.monitor_pid` | A registry is O(1) to read and cannot miss a root the way a filesystem/process scan can (renamed binaries, deleted worktrees, non-conventional scratch paths all defeated `ps`/`find` in the forensics account) |
| Ship-approval integrity | A GSD-prompt-only re-check (what caught it this once, non-deterministically) | A narrow, default-on structural check inside devflow-core, modeled on the existing `evaluate_layer0` pattern | `evaluate_layer0` already proves this exact shape works and is testable; a prompt-only catch is not code-enforced and not guaranteed to fire next time |

**Key insight (unchanged from the original research, still true, more true
now than before):** every "don't hand-roll" item above already has a
load-bearing implementation living in this codebase today. The re-aimed
scope is, if anything, *more* purely "wire existing primitives to a new call
site" than the original supervisor-migration scope was.

---

## CARRIED FORWARD — still valid, unaffected by the probe

### 23b Migration Inventory (re-confirmed unchanged this session — relevant ONLY if/when the deferred supervisor work is later picked up)

`rg -n "spawn_monitor\b|spawn_monitor_no_advance|wait_for_agent_pid|wait_for_agent_exit" --type rust crates/`
re-verified this session as unchanged since the original research (no
`crates/` commits between the two research passes — confirmed via `git log`).
8 files outside `monitor.rs` itself reference these four functions:

| File | Line(s) | Reference type | What it needs to become (IF the supervisor is later built) |
|------|---------|------------------|------------------------------|
| `crates/devflow-cli/src/pipeline_launch.rs` | `:126` (functional call: `monitor::spawn_monitor(state, program, &args, &adapter.extra_env())`); `:68`, `:91`, `:562` (comments) | **Functional call site — the production spawn path** | Replace with the new supervisor-spawn function. |
| `crates/devflow-cli/src/parallel.rs` | `:201` (functional: `monitor::spawn_monitor_no_advance(...)`); `:217` (functional: `monitor::wait_for_agent_exit(...)`) | **Functional call sites — both exclusively serve `sequentagent`** | **Deleted, not migrated** — both call sites live inside `sequentagent`'s handoff loop, which 23d removes regardless of the supervisor question. |
| `crates/devflow-core/tests/monitor_e2e.rs` | `:19`, `:77`, `:80` | **Test file — functional calls against the OLD API** | Rewrite against the new socket-based spawn/liveness API, if built. |
| `crates/devflow-core/tests/devflow_dir_gitignore.rs` | `:56`, `:109`, `:115`, `:121`, `:130` | **Test file — functional call against a function being deleted (23d)** | Repoint at the surviving spawn function; do not delete the test's actual coverage goal. |
| `crates/devflow-cli/src/preflight.rs` | `:4`, `:95`, `:174`, `:361`, `:365` | **Doc-comment / test-name references only** | Wording only. |
| `crates/devflow-cli/src/staleness.rs` | `:288` | **Doc-comment reference only** | Wording only. |
| `crates/devflow-cli/src/test_support.rs` | `:187`, `:218` | **Doc-comment references only** | Wording only. |
| `crates/devflow-core/src/monitor.rs` | entire file | **The module being rewritten, if built** | Not a "consumer." |

**Additional consumer beyond the "~8 files" scope (observability
re-pointing, IF the supervisor is built):** `crates/devflow-cli/src/commands.rs`
— `liveness()` (`:517-526`), `check_dead_agent`/`check_dead_monitor`
(`:1770`, `:1793`), `status`'s PID-based probe — all key off
`state.monitor_pid` today; **note that the re-aimed scope (A/B/C) does NOT
require touching any of these** — PID-based liveness stays exactly as-is for
the MVP, since it was never found to be the problem.

### 23d Deletion Inventory (re-confirmed unchanged this session — proceed exactly as scoped, independent of everything else in this document)

`rg -c "sequentagent|Sequentagent|SequentAgent" --type rust crates/`
re-verified this session: still **142 references across 11 files**, matching
the original research's corrected count (CONTEXT.md's "~110" was a
case-sensitive lowercase-only search that missed real PascalCase Rust
identifiers — confirmed again this session, unchanged).

| File | Verified count | Notes |
|------|----------------------|-------|
| `crates/devflow-core/src/agent_result.rs` | 48 | `SequentagentSlotKind`, `write_sequentagent_slot`, plus its own test module — real production+test surface |
| `crates/devflow-cli/src/parallel.rs` | 40 | Confirm at plan time whether the entire file is deleted or only `sequentagent`-specific functions (the `parallel` — N-phases-concurrently — command lives in the same file and must be preserved) |
| `crates/devflow-cli/src/commands.rs` | 24 | Includes `sequentagent_status_renders_*` rendering + tests |
| `crates/devflow-cli/tests/phase7_cli.rs` | 10 | Matches original exactly |
| `crates/devflow-core/src/ship.rs` | 8 | Matches original exactly |
| `crates/devflow-core/src/monitor.rs` | 3 | Doc comment on `spawn_monitor_no_advance` plus its own reference |
| `crates/devflow-cli/src/main.rs` | 4 | The `Sequentagent` CLI variant (`:159`), its dispatch arm (`:483-488`), the `use parallel::{parallel, sequentagent}` import (`:23`) |
| `crates/devflow-core/tests/devflow_dir_gitignore.rs` | 2 | Comment references to the `spawn_monitor_no_advance` call this test exercises |
| `crates/devflow-core/src/git.rs` | 1 | Verify at plan time whether functional or comment |
| `crates/devflow-core/src/agent.rs` | 1 | Verify at plan time |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 1 | **Load-bearing exception, do not delete:** `retry_after_from_reason` "must *move*, not be deleted" per the earlier teardown-research doc — verify at plan time whether this reference is that function (survives, relocated) or an unrelated mention |
| **Total** | **142** | 11-file count confirmed accurate |

**Public/documented-contract surface for D-12's v2.0.0 justification** (still
accurate, unchanged): `crates/devflow-cli/tests/snapshots/devflow-help.txt:12`
already lists `sequentagent  Run two agents sequentially on one phase, each in
its own worktree` — regenerating this snapshot is required, not optional.
`README.md`/`CHANGELOG.md` both mention `sequentagent` and need updates.

### How `--yes-ship` threads through (carried forward, unaffected by the re-aim — and now more load-bearing than before, since it is the mechanism that actually made both probe runs reach Ship at all)

The exact call site (`pipeline_outcomes.rs:275-286`, `handle_ship_outcome`)
and exact reusable API (`Gates::write_gate` + `Gates::respond`) are unchanged
from the original research. `State`-persisted-boolean precedent
(`monitor_pid` / `stop_until` / `preflight_retries`, all `#[serde(default)]`)
is the pattern to follow for a `yes_ship: bool` field — **this precedent is
also exactly the pattern this rewrite's Question A/B new fields (a future
`gate_pending`-registry entry, if the planner chooses to add anything beyond
the external `roots.json` file) should follow, should any new `State` field
turn out to be needed.**

```rust
// Recommended shape for handle_ship_outcome's auto-approve path (pipeline_outcomes.rs:275-286),
// unchanged from the original research:
pub(crate) fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    if state.yes_ship {
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

**A newly-relevant interaction to flag (not present in the original
research):** the sweep mechanism from Question A and `--yes-ship` both write
through `Gates::respond`, which refuses a second write once a response
exists (`GateError::AlreadyResponded`, `gates.rs:189-191`) — this is a safe,
first-writer-wins race resolution with no double-response risk. If `--yes-
ship` writes its approval before the sweep would otherwise consider the gate
"aged," the sweep simply finds no open gate for that phase+stage on its next
pass and does nothing. No coordination code is needed between the two
mechanisms.

### Carried forward: `hooks_after_ship` / `WorktreeRemove` (Claude's Discretion item, still open, unaffected by the re-aim)

```rust
// worktree::remove already exists and is already called this way from
// crates/devflow-cli/src/commands.rs:278 (cleanup) and parallel.rs:39/350:
worktree::remove(project_root, &path, /* force */ true)?;
```
A `Hook::WorktreeRemove` variant added to `hooks_after_ship()`
(`hooks.rs:105-111`) would call this exact function against
`state.worktree_path`, matching the existing `BranchCleanup` hook's
tolerance for "already gone." Still the planner's discretion whether to land
this now.

---

## Runtime State Inventory

> Included because this rewrite proposes exactly one genuinely new persisted
> artifact (the cross-root registry) and touches an existing bool field
> (`state.gate_pending`) only by reading it, never by changing its shape.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data (`.devflow/gates/*.json`, `state-NN.json`) | `GateFile.timestamp` (already present, `gates.rs`) and `State.gate_pending` (already present, `state.rs:28`) are sufficient for the sweep — **no schema change to either.** | None — read-only consumption by the new sweep/stop commands. |
| Stored data (NEW) | `~/.cache/devflow/roots.json` — a new, project-scoped-but-machine-global file, does not exist today. | **New code, new file** — `launch_stage_inner` writes an entry at spawn; `abort()`/`finish_workflow_with_gate_timeout`'s success path removes it. Follow the existing `write_atomic`-style pattern already used by `gates.rs`'s `write_atomic` (`gates.rs:323-330`) to avoid a torn write from a concurrent `devflow parallel` run. |
| Live service config | None found outside this repo's own `.devflow/` directory — unchanged from the original research. | None. |
| OS-registered state | None — unchanged. | None. |
| Secrets/env vars | `DEVFLOW_GATE_TIMEOUT_SECS`/`DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` already exist and are unaffected by this rewrite's recommendations (the sweep is additive, not a replacement for either). | None. |
| Build artifacts | The installed `devflow` binary is now **current** — 23-01 (`bede035`) ran `cargo build --release --workspace` and confirmed `devflow --version` reports `1.8.1`, matching `Cargo.toml`. **The previous research's Pitfall 6 (stale binary) is resolved and does not need to be repeated for this replan's units**, though it remains good practice to re-verify immediately before any further dogfood run. | None currently — re-verify before executing any new plan against this repo. |

---

## Common Pitfalls

### Pitfall 1: Re-reading the old research's "the monitor dies" framing as still current

**What goes wrong:** A planner skimming ROADMAP.md's original Phase 23 framing, or `OPERATOR-OBSERVABILITY-FINDINGS.md`'s Finding 1 (Phase 17, 2026-07-18/19), could reasonably plan against "the monitor is unobservable and dies silently."
**Why it happens:** Finding 1 is a real, correctly-diagnosed incident from a different run, months before this probe. It is not wrong about what happened in Phase 17 — it is simply not what happened in either of this probe's two runs, and the orphan forensics found the population-level cause is the opposite (immortality, not death).
**How to avoid:** Treat `23-PROBE-FINDINGS.md` and `23-ORPHAN-FORENSICS.md` as the authoritative, most-recent, directly-measured account of what actually blocks an unattended run on this machine today. Finding 1 remains true background on a *different*, real (if less frequent) problem — liveness ambiguity — that Question D leaves correctly deferred, not dismissed.
**Warning signs:** Any plan task whose stated justification is "so a dead monitor can be detected" rather than "so an abandoned gate does not leak a process for up to 7 days."

### Pitfall 2: SIGTERM to `state.monitor_pid` as `devflow stop`'s mechanism

**What goes wrong:** Orphans the `devflow advance` process instead of stopping it — verified this session by tracing the shell script's trap, which only ever tracks `$apid` (the agent), never the trailing `advance` invocation.
**Why it happens:** `state.monitor_pid` is the obvious, already-displayed PID (`devflow status` already shows it); it is natural to assume signalling it stops "the phase."
**How to avoid:** Target `.devflow/lock-{phase:02}`'s PID (the `advance` process itself) for the stop, or prefer the gate-response write path (Pattern 1) whenever a gate is actually open.
**Warning signs:** A `stop` implementation that only reads `state.monitor_pid` and never reads the lock file or `Gates::list_open`.

### Pitfall 3: Building a reaper that kills instead of writing a response

**What goes wrong:** A `kill(pid, SIGTERM)`-based reaper leaves `state.gate_pending: true` and the gate file open — the exact same incomplete state the natural 7-day timeout already produces, just faster. It also loses the audit trail a `GateResponse` would have recorded.
**Why it happens:** "Kill the process" is the more obvious mental model for "stop this from running forever" than "write a file the process is already polling for."
**How to avoid:** Prefer `Gates::respond` (Pattern 1) as the sweep's primary mechanism; reserve `kill()` for `devflow stop`'s narrow fallback (no gate currently open).
**Warning signs:** A sweep/reap implementation whose primary code path calls `libc::kill` rather than `Gates::respond`.

### Pitfall 4: Assuming the false-green catch will happen again

**What goes wrong:** Treating the fact that the agent's own GSD review caught the false `VERIFICATION.md` this one time as evidence the problem is already handled.
**Why it happens:** It DID work, and it is tempting to read a single successful catch as a systemic guarantee.
**How to avoid:** Remember this was a non-deterministic prompt-driven review, not a devflow-core-enforced invariant (Question C). Build the structural check regardless of whether the prompt-side catch "usually" works.
**Warning signs:** A plan that treats Ship's attestation integrity as "already fixed" because of this one probe run.

### Pitfall 5: Believing the self-hosted acceptance run is exempt from the SECURITY.md preflight wall

**What goes wrong:** Assuming D-02's self-hosted acceptance run, unlike the scratch probe, won't hit `workflow.security_enforcement=true` + missing `SECURITY.md`.
**Why it happens:** The scratch repo is a throwaway fixture, so it's tempting to assume the block is scratch-specific.
**How to avoid:** `.planning/config.json` in THIS repo (verified this session) has no `security_enforcement` override either — the default applies here too. Verify the chosen low-stakes acceptance phase's plan set actually produces a `SECURITY.md` before Ship (Question C's recommendation).
**Warning signs:** No task in the acceptance-phase's plan set that produces or checks for a `SECURITY.md`.

### Pitfall 6 (carried forward, still relevant): Threading `--yes-ship` as a CLI-only value instead of persisted state

**What goes wrong:** The Ship gate may fire long after the original invocation's process exited. A CLI-only flag is gone by the time it matters.
**How to avoid:** Persist on `State` at `State::new()` time, `#[serde(default)]`, exactly like `mode`/`stop_until`.
**Warning signs:** A plan task that reads `--yes-ship` only inside the `Command::Start` match arm and never touches `state.rs`.

### Pitfall 7 (carried forward, still relevant): Auto-answering the wrong gate

**What goes wrong:** `run_gate_with_timeout`'s finalization-retry gate (`finish_workflow_with_gate_timeout`) also tags `Stage::Ship` — a naive `stage == Stage::Ship` check would auto-approve both the routine Ship gate and the post-failure retry gate.
**How to avoid:** Scope any auto-answer (yes-ship or the sweep's rejection) to the specific call site, not a blanket stage-tag check.
**Warning signs:** A single boolean check keyed only on `stage == Stage::Ship` anywhere inside `pipeline_gate.rs`.

---

## Code Examples

### The exact evidence for the block (verbatim, from live source read this session)

```
// crates/devflow-core/src/lock.rs:8-9
// The lock is scoped per-phase (not per-project): `advance()` holds it
// across a gate's multi-day blocking wait, ...

// crates/devflow-cli/src/config_parse.rs:16-17,25
/// Parse `DEVFLOW_GATE_TIMEOUT_SECS`'s raw value, falling back to 7 days on
/// ...
/// via `DEVFLOW_GATE_TIMEOUT_SECS` (defaults to 7 days).
```

### Auto-reject sweep (new, illustrative — the pattern, not a literal diff)

```rust
// Read the new registry, then reuse Gates::list_open (unchanged) per root.
for root in registry::load_roots()? {
    for gate in devflow_core::gates::Gates::list_open(&root) {
        if age_secs(&gate.timestamp) > sweep_max_age_secs() {
            let _ = devflow_core::gates::Gates::respond(
                &root, gate.phase, gate.stage,
                &devflow_core::gates::GateResponse {
                    approved: false,
                    note: Some("abort: gate exceeded max unattended age with no response".into()),
                    responded_by: Some("devflow-reap".into()),
                },
            );
        }
    }
}
```

### `devflow stop` (new, illustrative)

```rust
pub(crate) fn stop(project_root: &Path, phase: u32) -> Result<(), CliError> {
    let open = devflow_core::gates::Gates::list_open(project_root)
        .into_iter()
        .find(|g| g.phase == phase);
    if let Some(gate) = open {
        devflow_core::gates::Gates::respond(project_root, phase, gate.stage, &GateResponse {
            approved: false,
            note: Some("abort: stopped by operator".into()),
            responded_by: Some("devflow-stop".into()),
        })?;
        // The live advance() process notices within <=60s and tears itself
        // down via its own existing abort() path — no signal needed.
        return Ok(());
    }
    // Fallback: no gate open, but the phase may still be mid-evaluation.
    let lock_path = project_root.join(".devflow").join(format!("lock-{phase:02}"));
    if let Ok(contents) = std::fs::read_to_string(&lock_path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            if devflow_core::agent::agent_running(pid) {
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
            }
        }
    }
    Ok(())
}
```

### `WorktreeRemove` hook (carried forward, unchanged)

```rust
worktree::remove(project_root, &path, /* force */ true)?;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| "The monitor dies, so replace it" (original 23b premise) | "The monitor never stops, so bound the wait" (this rewrite) | 23-02's probe, 2026-07-25 | Redirects the phase's actual engineering effort from an 8-file rewrite to ~2-3 small, additive modules |
| No cross-root gate visibility | `~/.cache/devflow/roots.json` + `devflow gate sweep` (NEW, this rewrite) | This phase (redefined 23b) | Answers "what is gated on this machine" without `ps`/`find` archaeology |
| `devflow stop` blocked on the supervisor (23c originally depended on 23b) | `devflow stop` buildable directly against the existing lock file + `Gates::respond` | This phase (redefined 23c) | Removes a hard dependency; 23c no longer needs 23b to land first |
| Ship approval trusted the agent's self-report + a non-deterministic prompt-side review | A default-on, devflow-core-side structural evidence check (NEW, this rewrite) | This phase (new cross-cutting item) | Closes the actual class of defect that stopped both recorded Ship attempts |

**Deprecated/outdated:** the original 23b/23c framing as "the socket
supervisor is required to close this phase." It is not — see Question D.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | The self-hosted D-02 acceptance run's chosen low-stakes phase will produce a `SECURITY.md` before Ship through its own ordinary GSD workflow (Question C). Not verified this session — no phase in this repo has yet completed an unattended end-to-end run to confirm. `[ASSUMED]` | Research Question C, Pitfall 5 | Medium — if wrong, the acceptance run hits the exact same preflight wall `devflow-probe-02` did, on the real repo, mid-acceptance |
| A2 | A narrow, default-on Ship-evidence check (git merge/tag/remote) is a sufficient structural signal and does not produce false positives against this project's actual git-flow shape (squash-merge to `develop`, then a separate release PR to `main` — per `STATE.md`'s release history). Not exhaustively verified against every historical Ship shape this session. `[ASSUMED]` | Research Question C, Pattern 3 | Medium — an overly strict check could block a legitimate Ship; the planner should verify the exact evidence signal against 2-3 of this repo's own past Ship commits before landing it |
| A3 | The exact shape/location of the new `~/.cache/devflow/roots.json` registry (fields, atomicity strategy) is a recommendation, not a locked contract — the planner may find a different shape preferable as long as it answers "what roots are currently active" in O(1). `[ASSUMED]` | Research Question A, Runtime State Inventory | Low — naming/shape only, no behavioral risk |
| A4 | Rate-limit gates being "unresolvable by construction" (a probe died at `define` with `status: "ratelimited"`, no scheduled auto-resume) is a real, separate finding this rewrite surfaces but does not design a fix for, per the objective's explicit scope (bound gate lifetime / stop / false-green). `[CITED: 23-ORPHAN-FORENSICS.md]` | Open Questions below | Low for this phase's scope, but the planner should make an explicit call (fix now vs. defer) rather than silently drop it — see Open Questions |
| A5 | macOS portability claims (unchanged from the original research) remain out of scope and unverified — no macOS host available this session either. `[CITED: socket-supervisor-spike.md, self-flagged there as documented-not-measured]` | Standard Stack (deferred supervisor section) | None for the re-aimed scope — the deferred supervisor work carries this risk forward unchanged |

---

## Open Questions (ALL RESOLVED — see resolutions inline)

> **Resolution status, added after planning (2026-07-25).** All three questions
> below were resolved during the replan, but the resolutions were originally
> recorded only in downstream plan objectives. They are marked inline here so a
> reader of this document alone does not see them as still-open:
>
> - **Q1 — rate-limit gates:** RESOLVED (deferred, explicitly) in `23-04-PLAN.md`.
>   Out of scope for this phase; the reaper bounds the process leak they cause,
>   but auto-resume is not built here.
> - **Q2 — Assumption A2 (Ship-evidence placement):** RESOLVED in `23-06-PLAN.md`.
>   The flagged risk was real; see that plan for the corrected placement inside
>   `merge_feature`, before `BranchCleanup`.
> - **Q3 — sweep automation:** RESOLVED in `23-04-PLAN.md` — on-demand only, no
>   background scheduler.

1. **Rate-limit gates (NEW, surfaced by the orphan forensics, not designed here per the objective's scope).**
   - What we know: `23-ORPHAN-FORENSICS.md` documents a probe that died at `define` with `status: "ratelimited"` and no parseable retry time — "auto-resume cron not scheduled; resume manually." This is a gate that is, today, unresolvable except by a human waiting out a weekly quota window.
   - What's unclear: whether this phase's scope should include a minimal fix (e.g., surfacing the retry-after time prominently, or scheduling an auto-resume cron) or should explicitly defer it to the backlog.
   - Recommendation: the planner should make this an explicit, recorded decision (fix now, given it's directly adjacent to the gate-lifetime work already in scope, vs. defer as a separate backlog item) rather than silently absorbing or silently dropping it.

2. **Exact Ship-evidence signal for Question C's structural check.**
   - What we know: git merge commits, tags, and remote pushes are all plausible signals; `hooks_after_ship` already has primitives for at least some of these (`hooks.rs`).
   - What's unclear: which single signal (or combination) is cheapest and most reliable against this project's actual git-flow shape, verified at plan time against 2-3 real past Ship commits in this repo's own history.
   - Recommendation: verify against real history before finalizing, per Assumption A2.

3. **Sweep trigger mechanism — periodic daemon/cron vs. on-demand only.**
   - What we know: the sweep (Question A) can be invoked on-demand (`devflow gate sweep`, operator-run) with zero new background process. A fully "self-healing" system would want it to run periodically without operator action.
   - What's unclear: whether this phase's scope should wire the sweep into an existing periodic mechanism (e.g., a cron/systemd-timer the operator already runs for `devflow`, if one exists) or ship it as an on-demand-only command for this phase, deferring automation.
   - Recommendation: ship on-demand only for this phase (matches D-03's "cheapest workload that crosses the seams" instinct); note automation as a natural, small follow-up, not a blocker for this phase's acceptance criterion.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|-----------|
| Rust toolchain (`cargo`, `rustc`) | Building the re-aimed units | Yes | rustc 1.97.1 | — |
| `libc` crate | `kill()` for `devflow stop`'s fallback path | Yes, already resolved | `0.2.x` (workspace-pinned `"0.2"`) | — |
| `~/.cache/devflow/` writability | The new cross-root registry file | Not explicitly re-verified this session, but `~/.cache` is a standard user-writable XDG cache dir on this host (Fedora Kinoite) — same conclusion as the original research reached for the (now-deferred) socket location | — | If unwritable, add a preflight check with a clear error rather than silently degrading |
| `devflow` binary on `PATH` | Any further dogfood run | **Current** — 23-01 rebuilt it; `devflow --version` confirmed `1.8.1`, matching `Cargo.toml` | 1.8.1 | Re-verify immediately before any new run (standard project practice, not a new risk) |

**Missing dependencies with no fallback:** none identified.
**Missing dependencies with fallback:** none currently blocking.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust's built-in `cargo test` harness `[VERIFIED: .planning/codebase/TESTING.md, cross-checked live this session]` |
| Config file | none — `.github/workflows/ci.yml`'s three jobs (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`) |
| Quick run command | `cargo test -p devflow <filter>` / `cargo test -p devflow-core <filter>` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map (re-aimed)

| Unit | Behavior | Test Type | Automated Command | File Exists? |
|------|----------|-----------|---------------------|---------------|
| 23b (registry) | `~/.cache/devflow/roots.json` round-trips; a spawned phase registers, a torn-down phase deregisters | unit | `cargo test -p devflow-core -- registry::` (new) | ❌ Wave 0 |
| 23b (sweep) | Aged open gate gets an auto-reject response; fresh open gate is left untouched; a live `advance()` process picks up the response and tears down cleanly | integration | `cargo test -p devflow -- gate_sweep::` (new; extend `monitor_e2e.rs`-style fake-agent fixture so the "live process notices the response" half is exercised for real, not just the write) | ❌ Wave 0 |
| 23c (`devflow stop`) | Gate-open path writes a rejection response and the target process exits within the poll's backoff window; no-gate-open path falls back to `kill()` on the lock-file PID; idempotent on an already-stopped phase | unit + integration | `cargo test -p devflow -- stop::` (new) | ❌ Wave 0 |
| 23d (`sequentagent` removal) | CLI no longer accepts `sequentagent`; help snapshot updated; no dangling references | integration (regression guard) | `cargo test -p devflow -- help_snapshot` (existing) + `rg -c sequentagent crates/` returns 0 | ✅ existing guard, needs its committed snapshot regenerated |
| `--yes-ship` | Unchanged from the original research | unit + integration | `cargo test -p devflow -- pipeline_outcomes::tests` (extend existing) | ⚠️ Partial — existing scaffolding |
| Ship evidence check (NEW) | An `AgentStatus::Success` at `Stage::Ship` with no corresponding git evidence is downgraded/rejected before the gate fires; a genuine Ship (real merge/tag) is unaffected | unit + integration | `cargo test -p devflow-core -- agent_result::tests` (extend, new cases mirroring `evaluate_layer0`'s existing test shape) | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** targeted `cargo test -p devflow-core -- <module>` / `cargo test -p devflow -- <module>` for the module just touched.
- **Per wave merge:** `cargo test --workspace` (full suite; last known-green 541/0 as of the original research session, re-verify at plan time) plus `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`.
- **Phase gate:** Full suite green, plus a repeat of the D-02 self-hosted acceptance run (unattended, this repo, driving a real low-stakes phase to a completed Ship), before `/gsd-verify-work`.

### What can only be validated by an actual run

`cargo test` proves the sweep/stop/registry mechanisms and the Ship-evidence
check's logic. It **cannot** prove the phase's actual acceptance criterion —
"no manual `ps`, no manual `devflow advance`, no silent stall" — without a
real unattended run. Recommend: re-run the same scratch-probe shape 23-01/
23-02 already built (`scripts/scratch-dogfood-repo.sh`) as the new units'
integration proof, specifically engineering a scenario that reaches a
Validate-retry-exhaustion or Ship gate and then verifying (a) the sweep or
`devflow stop` actually clears it within the expected window, and (b) no
process pair is left behind afterward (`ps` check, read-only, matching the
probe's own evidentiary discipline). Then the D-02 self-hosted acceptance run
remains the final, real proof of the whole phase.

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|---------------------|
| V2 Authentication | No | No user-facing authentication surface changes |
| V3 Session Management | No | N/A |
| V4 Access Control | Yes | `~/.cache/devflow/roots.json` and `.devflow/lock-{phase:02}` are both per-user filesystem artifacts already relying on standard Unix file permissions (no socket, so no TOCTOU-chmod window exists this time — a plain file created with default `umask` under a user-owned `~/.cache/devflow/` directory is adequate; recommend the directory itself be `0700`, matching the original research's socket-directory recommendation, reused here for the same reason) |
| V5 Input Validation | Yes | The registry file and gate-response writes are all `serde_json`-typed, not free-text — malformed entries fail to parse and are skipped (matching `Gates::list_open`'s existing "any unparsable file is skipped — listing must degrade, not die" pattern, `gates.rs:139`) |
| V6 Cryptography | No | No cryptographic material introduced |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| A stale/incorrect entry in `~/.cache/devflow/roots.json` causes the sweep to probe a root that no longer exists or belongs to a different, unrelated project | Tampering (of a sort — self-inflicted, not adversarial) | `Gates::list_open` already degrades gracefully on an unreadable/missing directory (`gates.rs:142-144`); the sweep should treat a registry entry whose `project_root` no longer exists as stale and prune it, not error |
| A local, unprivileged user on a shared multi-user host reads another user's `.devflow/gates/*.json` context (which may contain agent-generated, untrusted text — same caveat `gates.rs:296`'s doc comment already notes for the notify hook) | Information Disclosure | Standard Unix file permissions on `.devflow/` (owned by the project's owning user); this rewrite introduces no new exposure beyond what already exists |
| A malicious or buggy sweep implementation writes an APPROVE response instead of a REJECT for an aged gate, effectively becoming an unauthorized `--yes-ship` | Elevation of Privilege | The sweep must only ever construct `GateResponse{approved: false, ...}` — recommend a dedicated, narrowly-typed helper (e.g., a function that can only produce a rejection) rather than a generic `respond(approved: bool, ...)` call site the sweep shares with `--yes-ship`'s approval path, so a future refactor cannot accidentally wire the sweep to the approving branch |
| `devflow stop`'s fallback `kill()` path targets the wrong PID due to PID reuse (the lock file's recorded PID has since been recycled by an unrelated process) | Spoofing | `agent::agent_running` is already hardened against pid `0`/out-of-range values (Finding 1); recommend also validating the target process's command line (`/proc/<pid>/cmdline` on Linux) still looks like a `devflow` invocation before signalling it, matching the spirit of the deferred supervisor's `start_time`/`boot_id` validation even without adopting that full mechanism |

---

## Sources

### Primary (HIGH confidence)
- `.planning/phases/23-end-to-end-dogfood/23-PROBE-FINDINGS.md` — read in full this session; the authoritative single-run evidence base for this rewrite
- `.planning/phases/23-end-to-end-dogfood/23-ORPHAN-FORENSICS.md` — read in full this session; the authoritative population-level evidence base
- Live source reads this session, all line-cited above: `crates/devflow-core/src/monitor.rs` (full file), `crates/devflow-core/src/gates.rs` (full file), `crates/devflow-core/src/lock.rs` (relevant sections), `crates/devflow-cli/src/pipeline_launch.rs` (lines 1-330), `crates/devflow-cli/src/pipeline_gate.rs` (lines 1-340+), `crates/devflow-cli/src/config_parse.rs` (gate-timeout functions), `crates/devflow-cli/src/commands.rs` (cleanup, gate-related sections), `crates/devflow-cli/src/main.rs` (main/run/dispatch), `crates/devflow-core/src/agent_result.rs` (evaluate_layer0, reconcile_layer0_verdict, evaluate_agent_result)
- Live command output this session: `git log`/`git show --stat` confirming no `crates/` changes since the original research; `rg` re-counts for `spawn_monitor`/`sequentagent`; `cat .planning/config.json` confirming no `security_enforcement` override in this repo
- `.planning/phases/23-end-to-end-dogfood/23-CONTEXT.md` — read in full this session
- `.planning/STATE.md` — read (first ~490 lines) this session for project history and release-shape context
- `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` — read in full this session (Finding 1, the account this rewrite's Question D corrects the framing of, not the facts of)
- `.planning/audits/2026-07-24-socket-supervisor-spike.md` — read in full this session; re-assessed under Question D, not re-derived from scratch

### Secondary (MEDIUM confidence)
- The original `23-RESEARCH.md` (this file's predecessor) — sections explicitly marked "carried forward" above were re-verified, not merely copied

### Tertiary (LOW confidence)
- macOS-specific claims (unchanged, deferred alongside the supervisor work) — explicitly self-flagged as documented, not measured, in the original spike doc; not independently re-verified this session

## Metadata

**Confidence breakdown:**
- Research Question A (gate lifetime): HIGH — every claim traced to a specific, live-read source line this session, including the two new findings (the 7-day-not-infinite refinement, and the SIGTERM-orphans-the-advance-tail bug)
- Research Question B (`devflow stop`): HIGH — the lock-file-PID mechanism was verified by reading `lock.rs`'s own doc comment and `acquire`'s implementation directly
- Research Question C (false-green class): HIGH for "devflow-core does not read VERIFICATION.md" (directly verified by reading the full evaluation pipeline); MEDIUM for "the self-hosted acceptance run will produce a SECURITY.md in practice" (Assumption A1, not directly verified — no completed unattended run in this repo yet)
- Research Question D (what survives): HIGH — a direct, line-by-line re-assessment of the spike's own claims against A/B/C's findings
- 23b/23d inventories (carried forward): HIGH — re-confirmed unchanged via `git log`/`rg` this session, not merely copied from the prior document
- Pitfalls: HIGH — each grounded in a specific source line or a specific evidence-document quote, not generic pattern-matching
- macOS/rate-limit-gate-fix scope: LOW/MEDIUM — explicitly logged as open in Assumptions/Open Questions, not silently resolved

**Research date:** 2026-07-25 (rewrite)
**Valid until:** ~7 days — this is an active, currently-being-replanned phase in a solo-maintained repo; re-verify line numbers and the "no `crates/` changes since" claim immediately before planning if more than a few days pass.

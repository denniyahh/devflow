# Phase 23: End-to-End Dogfood — Context

**Gathered:** 2026-07-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Make `devflow start --phase N` drive one real phase from **Define through a
completed Ship stage**, **unattended**, **with Claude** — no manual `ps`, no
manual `devflow advance`, no silent stall.

Four units:

- **23a** — Dogfood probe in a scratch repo; record exactly where it dies.
- **23b** — Replace the `sh -c` monitor with the socket-addressable supervisor
  (999.33 / DEN-58).
- **23c** — `devflow stop`, explicit clean phase abort (999.34 / DEN-59).
- **23d** — Delete `sequentagent` (subtractive).

Plus the new `--yes-ship` pre-authorization needed to make "unattended"
actually reachable (see D-04).

**Grounding.** `.devflow/events.jsonl` shows no phase has ever completed a
full five-stage devflow-driven run. Phase 17 (Claude) reached Ship, looped
back to Code, then died to two silent monitor deaths (~4h lost). Phase 21
stopped after Define. Phase 22 (Codex) stops dead at a Plan relaunch with no
`advance_evaluated`. Two of the three were blocked at least once by
`self_dogfood_stale_blocked`.

**Out of scope, decided at scoping time:** 999.31/DEN-56 (Modular Agent
Driver — a *Codex* blocker, not a Claude one), 999.25/DEN-50 (release-cut
executor; crates.io publish stays manual), 999.4 and 999.26 (only bind under
concurrent ship or `devflow parallel`), and macOS verification.

</domain>

<decisions>
## Implementation Decisions

### Dogfood probe and acceptance (23a)

- **D-01:** 23a's probe runs in a **scratch repo, not this checkout.** Chosen
  for blast-radius isolation two days after 999.37 corrupted this repository.
  — **Reversibility:** reversible — the probe is a run, not a code change.
- **D-02:** The phase's **final acceptance run is self-hosted** (this repo),
  and happens **only after** the scratch probe has proven the supervisor.
  This is deliberate coverage repair for D-01: `staleness_outcome`
  (`crates/devflow-cli/src/staleness.rs:276-284`) returns `Block` only for
  `(is_self_dogfood: true, Stale)` and merely `Warn` for `(false, Stale)`, so
  **the staleness hard block — the most frequent observed dogfood killer — is
  structurally unreachable in a scratch repo.** Same for build-provenance and
  worktree-aware paths. A scratch-only acceptance would ship this phase
  without ever exercising the case that broke every prior run.
  — **Reversibility:** reversible.
- **D-03:** Probe output is a recorded artifact, not a verbal report: capture
  the failure point with `events.jsonl` excerpts and the `.devflow/phase-N-*`
  captures, so 23b is scoped from evidence rather than from the assumption
  that the supervisor is the only blocker. 23a is explicitly permitted to
  **invalidate the rest of this phase's scope** if something cheaper bites
  first.
  — **Reversibility:** reversible.

### Unattended semantics and `--yes-ship`

- **D-04:** Add a **`--yes-ship` pre-authorization flag.** Required because
  `Mode::Auto` does **not** make a run hands-off: `crates/devflow-core/src/mode.rs:82-94`
  documents "Ship always gates (both modes)" — Auto suppresses Validate gates
  only. Without this flag a Define→Ship run parks at the Ship gate and the
  phase goal is unreachable by construction.
  — **Reversibility:** costly — a new user-facing CLI flag on the irreversible
  merge/version/changelog path; removing it later breaks any operator or
  script that adopted it.
- **D-05:** `--yes-ship` is a **per-run flag only — never config-persistable.**
  It must be typed on each invocation and must not be settable in
  `devflow.toml`, so a standing unattended auto-merge can never become the
  silent default.
  — **Reversibility:** costly — relaxing this later is easy, but tightening it
  after operators depend on a persisted setting is not.
- **D-06:** `--yes-ship` **auto-answers the gate rather than bypassing it.**
  The Ship gate still fires and still records an explicit pre-authorized
  approval in `events.jsonl` and the gate ledger. The audit trail must show a
  decision, never a missing checkpoint.
  — **Reversibility:** reversible.
- **D-07 (ACCEPTED RISK — recorded deliberately, not an oversight):**
  `--yes-ship` is **not** refused on the self-dogfood workspace. Combined with
  D-02 (self-hosted acceptance run), this means the acceptance run will
  **unattended-merge a real phase into `develop`, bump the version, and append
  the changelog on this repository.** The operator considered and declined a
  self-dogfood refusal guard, on the grounds that a genuine hands-off proof is
  the point of the phase and D-06 keeps the decision auditable.
  **Suggested mitigations for the planner to encode, not to re-decide:** drive
  a low-stakes phase for the acceptance run, and establish a recovery point
  (tag or branch) before it starts.
  — **Reversibility:** one-way — the acceptance run performs a real merge to
  `develop`, a real version bump and a real changelog commit on the operator's
  primary repository. Undoing it means rewriting `develop` history.

### Supervisor migration (23b)

- **D-08:** **Big-bang replacement, no feature flag.** Delete the `sh -c` path
  outright rather than running both process models behind a toggle. DEN-58
  states the migration — not the mechanism — is the real cost; a parallel path
  doubles exactly that surface and risks the flag becoming permanent.
  — **Reversibility:** one-way — the `sh -c` monitor and every consumer of
  `spawn_monitor` / `wait_for_agent_pid` / `wait_for_agent_exit` are removed
  together across ~8 files; reverting means restoring a process model the rest
  of the phase has already been rewritten against.
- **D-09:** Carried forward from DEN-58's spike, **already decided — do not
  re-open:** socket lives in `~/.cache/devflow/` (**not** `$XDG_RUNTIME_DIR`,
  which systemd deletes when the last session ends — fatal for long unattended
  runs); the socket path is **stored in `state.json`**, never derived at probe
  time; liveness is `connect()` → GONE / STALE / ALIVE with no PID on the happy
  path; the pgid backstop applies only when the socket is STALE and is guarded
  by `start_time` + `boot_id`.
  — **Reversibility:** costly — the socket path is persisted in `state.json`,
  so changing its location later strands handles for any in-flight phase.
- **D-10:** The `advance` tail **runs in-process.** Today the monitor script
  ends with `; devflow advance --phase N`, a separate forked process — and that
  process is exactly what Phase 17's incident orphaned. Because the monitor
  becomes `devflow` itself, it calls advance directly. This removes the
  original failure mode **by construction**, and is the single property that
  most directly serves this phase's goal.
  — **Reversibility:** reversible within the new design.

### `sequentagent` removal (23d)

- **D-11:** **Hard delete the `sequentagent` verb** (`crates/devflow-cli/src/main.rs:159`,
  dispatch at `:483`). ~110 references across 11 files (`agent_result.rs` 34,
  `parallel.rs` 28, `commands.rs` 21, `phase7_cli.rs` 10, `ship.rs` 8,
  `monitor.rs` 3, plus singles). Shrinks 23b and closes DEN-58's explicitly
  untested `wait_for_agent_exit` gap in the riskiest part of the migration.
  Coherent with Claude-only: token-exhaustion failover has no second agent to
  reach.
  — **Reversibility:** one-way — removes a published CLI command from a
  crates.io-released binary; restoring it means re-adding a public contract
  after operators have been told it is gone.
- **D-12:** This removal is treated as **the breaking change that earns the
  v2.0.0 slot.** The milestone has been held open explicitly "until a
  genuinely breaking change earns the 2.0 slot"; removing a documented,
  published CLI verb is that change. The planner should assume a **major
  version bump**, not a minor one.
  — **Reversibility:** one-way — a published major version cannot be recalled.
- **D-13:** The capability intent is **preserved, not discarded** — DEN-67
  (999.42) remains open and carries the rationale. When a second agent is
  supported, failover is to be reimplemented **on the supervisor**, not by
  restoring the old in-process chaining. Prerequisites recorded there: DEN-58
  (supervisor) *and* DEN-56 (driver architecture).
  — **Reversibility:** reversible.

### Claude's Discretion

The operator did not constrain these; the planner and researcher decide:

- **Supervisor signal handling.** DEN-58 notes the spike installs no handler,
  so SIGTERM to the monitor leaves a stale socket. It degrades correctly (sweep
  + pgid backstop), but production "should" trap SIGTERM/SIGINT and perform the
  same clean shutdown as the socket `shutdown` command. Whether that lands in
  23b or is deferred is the planner's call — but it must be an explicit,
  recorded call, not an omission.
- **Scratch-repo scaffolding for 23a** — what minimum `.planning/` + GSD
  structure the probe target needs to be a valid devflow target.
- **In-flight-phase behaviour across the D-08 upgrade** — whether a phase whose
  `state.json` predates the `supervisor` field should be refused with guidance,
  or handled some other way. DevFlow's self-dogfooding makes a mid-run upgrade
  plausible.
- **Whether `hooks_after_ship` gains a `WorktreeRemove` step** and whether
  per-phase capture files get swept (see DEN-59's operator note) — both are
  untested-on-success paths this phase will be first to exercise.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Supervisor design (23b) — read these first
- `.planning/audits/2026-07-24-socket-supervisor-spike.md` — the authoritative
  spike results. Part 1 = claims C1–C6 (C2 liveness, C6 the `sun_path`
  length constraint that changes the design); Part 2 = replacement parity
  R-A..R-M against every production responsibility. **This is what the spike
  README tells the reader to consult.** Recovered onto the mainline 2026-07-25
  (`1101a8e`) — it was previously unreachable.
- `.planning/spikes/socket-supervisor/main.rs` — the ~200-line proof-of-mechanism
  (`std` + `libc` only, deliberately outside the cargo workspace). Re-run it to
  reproduce the scenarios rather than reconstructing them.
- `.planning/spikes/socket-supervisor/README.md` — how to run it; notes the
  manifest `path` fix needed to build standalone.
- `.planning/audits/2026-07-24-process-lifecycle-problem-definition.md` — the
  problem statement the design answers (failure modes F1–F7).
- `.planning/audits/2026-07-24-process-teardown-solution-research.md` — the
  options evaluated and rejected. **Read before proposing an alternative** —
  `command-group`, `duct`, `daemonize`, `nix`/`rustix` and `processkit` were
  each evaluated and ruled out for recorded reasons.

### Phase scope and history
- `.planning/ROADMAP.md` § "Phase 23: End-to-End Dogfood" — unit breakdown,
  the run-record table, and the explicit out-of-scope list with rationale.
- `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` § Finding 1 — the Phase 17
  monitor-death incident in the operator's own words; the origin of this
  phase's goal.
- `.planning/audits/2026-07-24-scope-creep-complexity-review.md` — the
  surface-reduction review that first proposed dropping `sequentagent`.

### Code the phase rewrites or depends on
- `crates/devflow-core/src/monitor.rs` (495 lines) — the `sh -c` monitor being
  replaced; `spawn_monitor` / `spawn_monitor_no_advance`.
- `crates/devflow-cli/src/parallel.rs` (622 lines) — `sequentagent` lives here.
- `crates/devflow-core/src/mode.rs:82-94` — **`Mode::Auto` vs `Supervise`, and
  the "Ship always gates (both modes)" rule that forces D-04.**
- `crates/devflow-cli/src/staleness.rs:276-284` — `staleness_outcome`; the
  self-dogfood `Block` vs ordinary `Warn` split that forces D-02.
- `crates/devflow-cli/src/main.rs:159` and `:483` — the `Sequentagent` CLI
  variant and its dispatch arm.
- Consumers of the monitor API that must keep working (~8 files):
  `crates/devflow-cli/src/{pipeline_launch,parallel,preflight,staleness,test_support}.rs`,
  `crates/devflow-core/src/monitor.rs`,
  `crates/devflow-core/tests/{monitor_e2e,devflow_dir_gitignore}.rs`.

### Linear (authoritative issue state)
- **DEN-58** (999.33) — supervisor. Carries the full C1–C6 / R-A..R-M tables,
  the resulting `state.json` design, F1–F7 coverage, and a "Known gaps — read
  before planning" section. Now `Todo`, milestone Phase 23.
- **DEN-59** (999.34) — `devflow stop`. **Its top-of-description `BLOCKED`
  banner is superseded** — see the 2026-07-25 comment. Also records the
  `hooks_after_ship` / capture-file-sweep gaps.
- **DEN-67** (999.42) — where the `sequentagent` capability intent is
  preserved per D-13.

### Conventions
- `.planning/codebase/CONVENTIONS.md`, `.planning/codebase/TESTING.md` —
  house style and test placement.
- `CONTRIBUTING.md` — the `core.hooksPath` / pre-push hermeticity setup; the
  test suite scrubs `GIT_*` per 999.37.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`devflow_core::test_support::{git_command, hermetic_command}`** — hermetic
  fixture construction with the full `REPO_LOCAL_GIT_VARS` scrub, behind the
  off-by-default `test-support` feature. Any new supervisor test that shells
  out to git must use this, not a bare `Command::new`.
- **`.devflow/phase-NN-*` artifact convention** — `agent-pid`, `stdout`,
  `stderr.log`, `exit`. The supervisor must preserve these (R-C, R-D) even
  though it no longer writes them from a shell script.
- **`State` / `save_state`** atomic temp+rename persistence — where D-09's
  `supervisor` block (socket_path, agent_pgid, agent_start_time, boot_id) will
  live.
- **`agent::agent_running`** — the existing `kill(pid, 0)` liveness probe,
  already hardened against pid `0` and values above `i32::MAX`. Retained only
  as the STALE-path backstop; it is *not* the primary liveness mechanism any
  more (D-09).

### Established Patterns
- **No `.unwrap()` / `.expect()` outside tests** — house convention; the
  supervisor's error paths must propagate with `?`.
- **Per-`Command` `env_remove` over process-global `set_var`** — established by
  999.37; `std::env::set_var` is `unsafe` in Rust 2024 and unsound in a
  threaded test binary. 999.38/DEN-65 is the open `PATH` instance of the same
  class.
- **Events are append-only and never-silent** — a hard block still emits
  notify + event before returning the error, so an unattended run sees it. The
  supervisor's new states must follow this.
- **`std` + `libc` only** — the spike deliberately adds no dependency, and
  DEN-58 lists "no new dependency" as a design win. Preserve that.

### Integration Points
- `launch_stage` → `monitor::spawn_monitor` — the seam being replaced.
- `devflow advance --phase N` — becomes an in-process call (D-10) rather than a
  forked tail.
- `status` / `doctor` liveness reporting — currently PID-based; must be
  re-pointed at the socket probe so GONE / STALE / ALIVE surface to the
  operator (this is what makes a dead monitor distinguishable from a healthy
  between-stages pause).
- `cleanup --force` — deliberately refuses to touch a live agent/monitor today;
  `devflow stop` (23c) becomes the verb that actually stops one.

</code_context>

<specifics>
## Specific Ideas

- The operator's framing of the goal, verbatim in intent: *"getting devflow
  working end to end with at least Claude — I don't need full functionality,
  just the basic development workflow."* Scope pressure should resolve toward
  proving the loop, not toward completeness.
- **Acceptance is behavioural, not code-shaped.** The phase is done when one
  phase has actually been driven start-to-finish by devflow, unattended,
  reaching a completed Ship stage — not when the supervisor compiles and its
  unit tests pass.
- 23a leading is load-bearing, not ceremonial: it is the only unit permitted to
  invalidate the rest of the scope.

</specifics>

<deferred>
## Deferred Ideas

- **`--yes-ship` refusal on the self-dogfood workspace** — considered and
  explicitly declined this phase (D-07). If the accepted risk proves
  uncomfortable in practice, this is the ready-made mitigation.
- **`999.31` / DEN-56 — Modular Agent Driver.** Highest-priority backlog item
  by label; deferred because it is a Codex blocker, not a Claude one. Returns
  as the prerequisite for onboarding any second agent.
- **`999.25` / DEN-50 — release-cut executor.** The crates.io publish half of
  Ship. Drives irreversible operations and needs its own failure/rollback
  design pass.
- **`999.42` / DEN-67 — agent failover on token exhaustion.** Preserved intent
  per D-13; blocked on both DEN-58 and DEN-56.
- **macOS verification.** DEN-58 flags it as the single largest unknown (no
  host, no CI; the 104-byte `sun_path` limit is documented, not measured).
  `chore/macos-ci` holds the deferred CI work — note it is built on 10 stale
  planning-doc commits at v1.8.0 and will want rebuilding as a single clean
  commit.
- **`999.38` / DEN-65 — test-suite `PATH` race.** Same class as 999.37, `PATH`
  instead of `GIT_*`. Fixing it would let `ENV_MUTEX` shrink or disappear.
- **`999.39` / DEN-66 — production git calls inherit a redirecting
  environment.** ~86 call sites; a clippy `disallowed-methods` enforcement
  recipe is recorded in that issue's comments.
- **The old "Test Suite & CI Hardening" theme** — 999.15, 999.17, 999.18,
  999.19, 999.20, 999.22. Displaced from this phase because it advances the
  end-to-end goal by ~zero. Untouched in the backlog.

</deferred>

---

*Phase: 23-end-to-end-dogfood*
*Context gathered: 2026-07-25*

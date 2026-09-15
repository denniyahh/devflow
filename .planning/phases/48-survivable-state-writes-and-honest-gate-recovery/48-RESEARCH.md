# Phase 48: Survivable State Writes and Honest Gate Recovery - Research

**Researched:** 2026-09-14
**Domain:** Rust CLI process coordination (file locks, atomic file publication, gate-file protocol), plan-file parsing, test-suite environment isolation
**Confidence:** HIGH for in-repo facts (read this session with file:line); MEDIUM for design recommendations (reasoned, not executed)

**Requirement IDs.** This document uses the IDs from 48-CONTEXT.md D-11: **SURV-01**, **SURV-02**, **CHKPT-01** (ROADMAP alias `GATE-01 (999.125)`), **CHKPT-02** (alias `GATE-02 (999.126)`), **TEST-01** (alias `999.38`, with 999.80 folded in). `GATE-01 (#184)` and `GATE-02 (#170)` in REQUIREMENTS.md's Future Requirements table are unrelated and keep their meaning. Applying the rename is itself a plan task (D-11).

<user_constraints>
## User Constraints (from CONTEXT.md)

Copied verbatim from `48-CONTEXT.md` (sections `<decisions>`, `<review_findings>`, `<deferred>`). The review findings R-1..R-8 are binding amendments that override decision text where they conflict.

### Locked Decisions

## Implementation Decisions

### State writes — SURV-01

- **D-01 (Operator): One writer per phase, enforced by the existing per-phase lock — not merge or
  compare-and-swap machinery.**
  - *Verified:* the per-phase lock `.devflow/lock-NN` (`crates/devflow-core/src/lock.rs`) is taken by
    `advance` (`crates/devflow-cli/src/pipeline_launch.rs:1482`), `resume` (`:1293`, held until return) and
    `ship_override` (`crates/devflow-cli/src/pipeline_gate.rs:518`). Of the 23 production `save_state` call
    sites, two write without it: `start` (saves `monitor_pid` after spawning the monitor,
    `pipeline_launch.rs:1047-1053`, reached from `crates/devflow-cli/src/commands.rs:670-677`) and `stop`
    (`persist_stopped_state`, `commands.rs:1930-1947`).
  - *Reasoned, never observed:* (a) a fast-exiting agent's monitor `advance` saves a stage transition that
    `start`'s later `monitor_pid` save overwrites — the same shape as WR-11 (`commands.rs:661-669`), one save
    later; (b) `stop` loads state, the waiter aborts and `clear_state` deletes the file, then `stop`'s rename
    re-creates a state file for an aborted phase.
  - Neither overlap is caused by the shared `.tmp` name the roadmap cites. A unique temp name alone fixes
    neither.
  - **ROADMAP amendment:** criteria 1–2 change from "both writers' updates are observable" to "a second
    writer is excluded or refused loudly — never silently overwrites". The failing-then-passing evidence
    standard and the single-writer negative control stay.

- **D-02 (Operator): The writer rule.**
  1. `start` holds the per-phase lock from its first `save_state` until it returns, exactly as `resume` does.
  2. `advance` waits for the lock with a bounded, generous wait instead of failing immediately (today
     `pipeline_launch.rs:1482-1488` returns "another devflow process (pid N) is already running"). On expiry
     it emits an event (reuse `advance_failed`). *Verified:* the Legacy monitor script runs `advance` exactly
     once with stdout/stderr sent to `/dev/null` (`crates/devflow-core/src/monitor.rs:473-489`, `:520-522`), so
     an un-logged refusal is invisible; `PipeOwning` reaches the same `advance()` through `run_monitor`
     (`pipeline_launch.rs:977`), so one change covers both launch shapes.
  3. `stop` writes state only when it can take the per-phase lock briefly (`lock::acquire`). While a live
     holder exists it writes nothing — the holder's own abort path clears state.
  4. `write_state_atomic` (`crates/devflow-core/src/workflow.rs:185-193`) uses a unique temp name per write
     as a backstop.
  - *Verified side effect relied on by D-05:* every production gate wait (`run_gate_with_timeout`,
    `pipeline_gate.rs:352`) is reached from `advance`, `resume`, `ship_override` or `start` (directly or via
    `parallel`). Once `start` holds the lock, every waiting process holds the per-phase lock, so
    `lock::holder` + `lock::holder_identity` + `agent::is_same_process` identify a live waiter. Limit: the
    call-graph trace was a text search (over-inclusive; closures and function pointers not followed).
  - — **Reversibility:** costly — changes lock timing on `start` and contention behaviour on every stage
    completion; undoing it means re-auditing both launch shapes.

- **D-03 (Operator): Accepted gaps, and the overhaul is parked.** Recorded as known limits, not fixed here:
  - The `advance` wait bound is a chosen number, not a measured one. A launcher that holds the lock past it
    after spawning still stalls — now logged, and recoverable via `doctor` → `resume`.
  - Writers outside devflow's lock bypass it: manual edits of the state file (e.g. the documented
    `yes_ship` edit) and a binary rebuilt mid-run (`monitor.rs:319` spawns `current_exe()`).
  - A live lock holder may be hung; any message can only say it is alive.
  - Waiter identity is Linux-only (`agent::process_start_time` reads `/proc/{pid}/stat`,
    `crates/devflow-core/src/agent.rs:203-204`) and limited to one PID namespace.
  - Lock reclaim tests pid liveness only (`lock.rs:100`, `:192-194`), so a recycled pid blocks
    `resume`/`ship`/`stop` from taking the lock.
  - Correctness depends on "every state writer takes the lock" (see Claude's Discretion for an optional check).
  - The split-lock overhaul goes to Deferred Ideas with explicit revisit triggers.

- **D-04 (Operator): Gate responses — the first answer wins.**
  - *Verified:* `Gates::respond` checks `path.exists()` and then writes through the fixed-name temp
    (`crates/devflow-core/src/gates.rs:196-199`, `write_atomic` at `:356-364`); `gate approve/reject`, `stop`
    and `gate sweep` all reach it. The gate request and ack files each have a single writer — the only
    production `write_gate` and `poll_response` calls are in `run_gate_with_timeout`
    (`pipeline_gate.rs:371`, `:423`).
  - *Reasoned consequences today:* a torn response that the poller silently skips (`gates.rs:268-272`) while
    every later responder is refused as already answered; or an approval silently replacing a rejection.
  - **Fix:** unique temp name plus an exclusive publish (a hard link that fails if the response already
    exists), so a second responder gets `AlreadyResponded` loudly.

### Gate recovery when nothing is waiting — SURV-02

- **D-05 (Operator): Honest, gate-aware recovery.** When `stop`, `gate approve` or `gate reject` finds no
  live waiter (via D-02's lock identity check):
  1. Say plainly that nothing is waiting. Never claim a poller — today `commands.rs:1417-1422` ("once the
     waiting monitor polls it") and `:1821-1826` ("the process waiting on it will pick this up") both do.
  2. Name the command that works for that gate:
     - Ship gate → `devflow ship --phase N` (`ship_override` handles Advance, LoopBack and Abort,
       `pipeline_gate.rs:569-604`; it refuses when an ack file exists).
     - Any other gate → `devflow resume --phase N` (relaunches the stage; its gate re-fires with a live
       waiter if the problem recurs) or `devflow recover --clean --phase N` (ends the phase).
  3. At a non-Ship gate, write **no** answer file.
  4. `recover --clean --phase N` (`crates/devflow-core/src/recover.rs:130-145`) and a fresh `start` of the
     phase delete that phase's leftover gate request/response/ack files. *Verified gap:* neither calls
     `Gates::cleanup` today (its production callers are `prepare_loop_back_to_code`,
     `finish_workflow_with_gate_timeout`, `abort`, `run_preflight`, `handle_stage_failure`).
  5. `doctor` prints the same repair lines through the same message code. `check_gate_pending_without_gate`
     (`commands.rs:3205-3218`) prescribes `resume` for every gate today, and with (3) a no-waiter non-Ship
     gate stays open, so `doctor` also needs a finding for "gate open, nothing waiting".
  6. `gate approve` is covered by the same change (`gate_respond`, `commands.rs:1385-1424`, serves both).
  - *Verified correction to the roadmap's premise:* `resume` never reads a pending answer. It relaunches the
    saved stage (`pipeline_launch.rs:1383` → `launch_stage`, `:1206-1210`). It recovered #200 only because a
    **preflight-refusal** gate re-fires on relaunch and consumes the stale answer. For stage-failure and
    Validate gates it re-runs the agent instead; for the Ship finalization-retry gate the working repair is
    unverified.
  - *Reasoned hazard closed by (3) and (4):* an unread answer persists and silently decides the next firing of
    that gate, including in a later run of the phase — the hazard `pipeline_gate.rs:463-464` names.
  - Rejected: `stop` running `abort()` itself. `recover --clean --phase N` already ends a dead phase, so it
    would save one command rather than provide the only way out.
  - Unconfirmable waiter identity (non-Linux, or a legacy single-line lock): the message says a process holds
    the lock but cannot be confirmed, and claims neither a waiter nor its absence. Commands that take the lock
    already refuse while that pid is alive.
  - **ROADMAP amendment:** criterion 3 reworded to match D-05.

- **#200 reproduction for criterion 4 (Claude, remit).** An end-to-end test with the real binary, following
  `crates/devflow-cli/tests/gate_sweep_e2e.rs` (which already drives a real poller to abort):
  - *Wedge arm:* kill the foreground `start` at its preflight gate; assert the D-05 message, that no answer
    file is written, and that `recover --clean --phase N` clears the phase.
  - *Self-resolving arm (negative control):* write the answer right after the gate opens; the uninterrupted
    `start` exits 0 with state cleared. Record the observed answer-to-pickup interval — do not cite "~40s",
    which measures the poll backoff (`gates.rs:264-279`: 1s doubling to a 60s cap), not the wedge.
  - If both arms wedge, the harness is broken, not the subject.

### Checkpoint predicate and late re-scan — CHKPT-01, CHKPT-02

- **D-06 (Claude, remit): One shared checkpoint parser (CHKPT-01).**
  - A declaration counts only on a line that opens a `<task` element; content inside fenced code blocks is
    ignored. *Verified with a control:* 0 fenced `<task` lines and 0 multi-line `<task` openings across all
    216 `*-PLAN.md` files, so no real declaration is lost and a full XML parser is unnecessary.
  - Preflight's unattended check refuses on both human-only classes (`gate="blocking-human"`,
    `type="checkpoint:human-action"`); the resume/auto-decide route arms on `blocking-human` only. Carried
    forward from Phase 47 D-02, operator-reconfirmed 2026-09-10 — do not re-widen without a new decision.
  - Replace the stale rationale at `crates/devflow-core/src/verify.rs:191-196` ("fails SAFE … routes more
    checkpoints to a human"): at its only production call site (`pipeline_launch.rs:1594-1602`) a match arms
    the agent's auto-decide resume.
  - Flip the pinned known-limit test `human_only_checkpoint_still_matches_a_task_tag_inside_a_fenced_example`
    (`verify.rs:609`).
  - Acceptance test: a marker outside a task-opening line neither blocks preflight nor arms resume. Control: a
    proper task-level `blocking-human` gate produces the human-only result on both paths.

- **D-07 (Operator; refinements Claude, remit): Record what was checked; new or changed checkpoints go to a
  human (CHKPT-02).**
  - DevFlow records the set of human-only checkpoint declarations — each whole task element, so a rewritten
    task body counts as changed — in `State` as a new `#[serde(default)]` field. *Verified:* no such field
    exists today (checkpoint/preflight-related fields are `preflight_retries`, `checkpoint_resumes`, `canary`).
  - At the resume decision point (`pipeline_launch.rs:1594-1602`), any checkpoint new or changed relative to
    the recorded set parks at a human gate instead of arming auto-decide. Checkpoints already approved still
    auto-resolve as today.
  - *Remit refinement — who can change the set:* first recorded at the phase's first Code evaluation of the
    unattended check; afterwards changed only by a human approval — of a preflight-refusal gate
    (`crates/devflow-cli/src/preflight.rs:1367-1376`) or of the new re-scan gate. A later launch (e.g. a
    loop-back to Code) never widens it silently. *Verified:* the unattended check evaluates in every mode at
    Define and Code and refuses only in Auto (`preflight.rs:1164-1198`), so a later evaluation would otherwise
    absorb an agent-added checkpoint.
  - A state file with no recorded set (written by an older binary) is treated as "nothing approved" and gates.
  - The resume route has no mode check (`pipeline_launch.rs:1572-1602`), so this applies in every mode.
  - Tests (remit): a plan that gains a `blocking-human` checkpoint after Code preflight parks at a gate (today
    it is auto-decided); a rewritten body of an approved checkpoint gates; a missing set gates. Control: an
    unchanged plan still auto-decides.
  - — **Reversibility:** costly — adds a persisted `State` field that every later binary must keep reading.

### Test-suite PATH isolation — TEST-01 (999.38 + 999.80)

- **D-08 (Operator): Tests that replace `PATH` run in a child process.**
  - Generalize Phase 46's `run_test_without_git` (`crates/devflow-cli/src/test_support.rs:411-426`): a test
    re-invokes itself (`current_exe()`, `--exact`, `--test-threads=1`) with `PATH` set on the child `Command`
    only. Guard every child with `assert_child_ran_exactly_one_passing_test` (`test_support.rs:449`; four
    assertions against a vacuous child run). devflow-core needs its own helper — it is a separate test binary.
    No production change.
  - Keep `PATH` set to a directory, never removed: when `PATH` is removed, std falls back to an OS default
    (`/bin:/usr/bin`), which would leave `git` resolvable.
  - *Verified scale, two counts agree:* ~52 `PATH`-replacing blocks in devflow-cli tests plus 15
    `PathGuard::set` calls in `crates/devflow-core/src/agents/pi.rs` and `opencode.rs` (156 `PATH`
    set/restore lines ≈ 3 per block).
  - *Why not pass `PATH` into production:* feasible (std searches a `PATH` set via `Command::env` on Unix), but
    it would touch 14 direct production spawns in nine files, the `hermetic_command` constructor (55 call
    sites) and the in-process lookup `agent_binary_available` (`preflight.rs:65-79`) — and it fails open: a
    missed or new spawn resolves through the developer's real `PATH` and can start a real agent with real
    credentials. A child process covers every spawn, present and future.
  - *Why not locks and guards only:* Phase 46 measured that approach closing 47 of at least 78 exposures
    (`.planning/PROJECT.md` Key Decisions, Phase 46 C-01).
  - *Not measured:* the cost of one extra process start per converted test (no test binary was built in this
    worktree). The plan records full-suite time before and after.

- **D-09 (Operator): `PATH` only, enforced by a lint.**
  - Add `clippy.toml` with `disallowed-methods` entries for `std::env::set_var` and `std::env::remove_var`,
    each with a `reason`. *Verified via clippy stable docs:* the lint covers methods and functions, an unknown
    path is an error unless `allow-invalid`, and it is warn-by-default, so the repo's `-D warnings` makes it
    fail.
  - The ~40 remaining non-`PATH` environment mutations in tests (`DEVFLOW_GATE_TIMEOUT_SECS` 12,
    `DEVFLOW_CLAUDE_LEGACY_LAUNCH` 7, the two idle timeouts 8, `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` 3,
    `DEVFLOW_GATE_NOTIFY_CMD` 2, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` 2, `DEVFLOW_BASE_BRANCH` 1, plus env-guard
    helpers) each get `#[expect(clippy::disallowed_methods, reason = "...")]`. The toolchain is pinned at
    1.97.1, which supports `#[expect]` with `reason`; no `#[expect(` exists in the crates yet.
  - *Verified:* production never mutates the process environment (the only hit outside `mod tests` is
    `test_support.rs`, itself `#[cfg(test)]`), so the ban only ever fires in tests.
  - Update `.planning/codebase/TESTING.md` § Environment Mutation Rule in the same plan, and check whether
    adding `clippy.toml` triggers the `.planning/user/DEV-SETUP-CHECKLIST.md` rule in `CLAUDE.md`.

- **D-10 (Operator): 999.80 folded in.**
  - Tests kept from spawning a real agent only by an `"abort: test cleanup"` fixture note run in a child
    process with a neutral `PATH` (the D-08 mechanism).
  - *Measured:* 18 such fixtures in `crates/devflow-cli/src/pipeline_outcomes.rs`; 13 have no `PATH`
    neutralization in their own function body. Upper bound — protection applied by a caller is invisible to
    the scan, and whether each can reach an agent spawn is unmeasured. The planner confirms reachability first.
  - 999.80's recorded line numbers (`:825-864`, `:878-930`, `:2073-2097`) are stale.
  - Record in ROADMAP that 999.38 and 999.80 were worked together.

- **TEST-01 evidence standard (Claude, remit).** Structural, measured two ways with a control:
  (a) a search for `set_var("PATH"` / `remove_var("PATH"` in test code returns 0; (b) the D-09 lint fails
  clippy on a deliberately added `std::env::set_var` (negative control). Plus one full-suite run pinned to two
  CPUs (`taskset -c 0,1`), reported as a sanity check that does **not** prove the flakes cannot recur.

### Requirements bookkeeping

- **D-11 (Operator; exact IDs proposed in the approved option): New requirement IDs.**
  - `CHKPT-01` = 999.125, `CHKPT-02` = 999.126, `TEST-01` = 999.38 including folded 999.80.
  - Add all three to `.planning/REQUIREMENTS.md`'s v3.0.0 requirements and traceability table. Update
    `.planning/ROADMAP.md` Phase 48's requirements line (`:155`), its progress-table row (`:30`) and the
    criterion labels (`:175`, `:178`).
  - `GATE-01` (#184) and `GATE-02` (#170) in REQUIREMENTS.md's Future Requirements table keep their meaning.
  - *Verified:* `CHKPT-xx` and `TEST-xx` appear nowhere in `.planning`; `GATE-01`/`GATE-02` already carry a
    third, archived meaning in `.planning/milestones/v1.0-phases/11-refactor-gsd-native/`.
  - Do not conflate #170 ("gate declaration vs mention") with D-06: #170 concerns the agent's output stream
    (`agent_result::checkpoint_reported_in_capture`), not plan files.
  - Make these planning-doc edits in a plan task, as Phase 47 did its ROADMAP amendment.

### Process constraints for planning

From `.planning/audits/2026-09-13-phase-47-retrospective.md`:

- **P3 trial — contract inventory at plan time.** Research lists every existing contract the change touches,
  with `file:line`, and plan-checker confirms each has a test. Known so far:
  - WR-11 save-before-launch ordering (`commands.rs:661-669`).
  - T-23-51 / T-23-52: `stop` never signals `monitor_pid` and matches lock identity (`commands.rs:1776-1921`).
  - CR-01 stale gate response cleanup (`pipeline_gate.rs:463-464`).
  - Phase 28 T-28-01 ordering — static declaration check before anything agent-controlled
    (`pipeline_launch.rs:1572-1602`).
  - Phase 47 D-02 carve-out (auto-decide for `blocking-human` only).
  - Preflight D-08 (same evaluation in every mode) and D-09 (no override) (`preflight.rs:1094-1133`).
  - T-18-02: `doctor` findings are report-only.
  - 20e `ship_override` guards (lock, stage, request+response present, ack absent).
  - `env_lock` / `NeutralPath` premises (`test_support.rs:40-99`) and TESTING.md's one-mutex-per-variable
    invariant.
- **P4 trial:** one automated fix round per review, then the operator decides.
- Wave order inside the phase: correctness before survivability (ROADMAP sequencing note).

#### Binding amendments (from CONTEXT <review_findings>)

## Adversarial Review Findings — Binding Amendments

**Reviewed:** 2026-09-14 against commit `f063b42` (read-only `git archive` snapshot). **These amendments
override the decision text above wherever they conflict.** Applied by operator decision ("Apply all").

**Lane roster:**
- codex (`gpt-5.6-terra`, high effort) — completed; 4 findings, 20 distinct `file:line` citations.
- DeepSeek via `pi` (`deepseek-v4-pro`) — completed; 7 findings, 36 distinct citations.
- agy (`gemini-3.8-flash-high`) — **DROPPED**: print timeout after 30m with the turn in progress, 0 bytes of
  output. Not a pass.

Every finding below was re-verified against source by Claude. Failure paths are reasoned from code, not
reproduced.

- **R-1 (HIGH, D-05) — never strand a live waiter.** Delete a phase's gate request/response/ack files only
  when no live process holds the per-phase lock. `recover --clean --phase` (`commands.rs:2286` →
  `recover.rs:130-145`) takes no lock and checks only the agent pid; `Gates::cleanup` removes all three files
  unconditionally (`gates.rs:294-305`); a waiter polls only the response path (`gates.rs:255-272`) and
  `respond` refuses once the request is gone (`gates.rs:192-194`). Cleanup under a live waiter would leave it
  unanswerable until its gate timeout. A fresh `start` cleans up only after taking its lock (D-02).
- **R-2 (HIGH, D-02) — bind a waiting `advance` to its stage.** The monitor passes the stage it launched
  (Legacy `advance_tail`, `monitor.rs:473-479`; PipeOwning `run_monitor`, `pipeline_launch.rs:977`). After
  acquiring the lock, `advance` refuses — logging `advance_failed` — when `state.stage` no longer matches.
  Today `advance` receives only the phase and evaluates whatever `state.stage` holds (`pipeline_launch.rs:1452`,
  `:1496-1510`); the comment at `:1491-1495` relies on duplicates being excluded, which a bounded wait breaks.
- **R-3 (HIGH, D-06) — unclosed fences fail closed.** Ignore only content inside properly closed fences. From
  an unclosed fence to end of file, apply the line-anchored match, so a declaration there still counts.
  Negative test: an unterminated fence before a real `blocking-human` task still refuses unattended preflight.
  Today's scan is not fence-aware and cannot fail this way (`verify.rs:207-220`); the corpus has 0 odd-fence
  plans (both lanes reproduced this), so this guards agent-written plans rather than current ones.
- **R-4 (MEDIUM, D-04) — exclusive publish for responses only.** Apply fail-if-exists publishing only in
  `Gates::respond`. `write_gate` and `ack` keep rename-overwrite: `resume` re-fires the preflight gate over a
  leftover request with no cleanup in between (`preflight.rs:1367` → `pipeline_gate.rs:369-371`).
- **R-5 (MEDIUM, D-02) — `stop` locks before it loads.** Order: acquire the lock → load state (re-check that it
  exists) → mutate → save → release. `persist_stopped_state` loads before saving today
  (`commands.rs:1930-1945`); any other order leaves D-01 hazard (b) open.
- **R-6 (MEDIUM, D-05) — every answer writer follows D-05.** The no-waiter rule also covers `stop_via_gate` →
  `Gates::reap` (`commands.rs:1806-1840`) and `gate sweep`, whose only write path is `Gates::reap`
  (`gates.rs:207-211`) and which checks gate age but not for a waiter (`commands.rs:1442`), in addition to
  `gate_respond`.
- **R-7 (LOW, D-04 and D-02) — collectable temp files.** Unique temp names follow a recognizable pattern, and
  gate cleanup and state cleanup remove orphans left by a crash between publish and unlink.
- **R-8 (LOW, D-07) — approval records the set.** Approving the re-scan gate persists the then-current
  checkpoint set, so a run upgraded past Code gates once rather than at every resume decision.

**Planner notes (not amendments):**
- Hard links are unsupported on some filesystems (FAT/exFAT, some network mounts). Record a fallback or a clear
  error for R-4.
- Not adopted: the concern that D-07's whole-task comparison over-triggers on task-body edits. No GSD workflow
  instructs the executor to edit PLAN.md task bodies (filtered search — weak evidence), so pin the comparison
  rule with a real-plan fixture.

### Claude's Discretion

- The `advance` lock-wait bound — generous, with a stated rationale; tests control the lock directly rather
  than relying on timing.
- Whether a cheap "caller holds the per-phase lock" check in `save_state` fits without reworking the test suite
  (179 `save_state` calls including tests).
- Exact wording of D-05's messages, within D-05's content rules.
- How D-07's checkpoint set is normalized and stored, provided a changed task body is detected.
- How the child-process helper generalizes (one helper taking a `PATH`, or variants), provided every child is
  guarded by `assert_child_ran_exactly_one_passing_test`.
- Whether D-08's conversion lands in an early wave so the other plans' tests run under it.
- Plan split and wave assignment.

### Deferred Ideas (OUT OF SCOPE)

## Deferred Ideas

- **Split-lock state-handling overhaul** — a short OS lock (`flock`) around every state write, an
  `update_state(root, phase, |s| …)` API with `State` consumed by value across blocking calls, a
  launcher-to-monitor ownership token handoff, gate-waiter registration, and fsync of file plus directory.
  Parked for Phase 50, which rebuilds the monitor supervisor. **Revisit when:** Phase 49's live run (or any
  run) records a concurrent state write or an `advance_failed` refusal; Phase 50's discussion reworks the
  monitor-to-`advance` handoff; or a new state writer appears that cannot take the per-phase lock.
- **`gate list` vs `status` after a Ship-gate answer with nothing waiting** — `gate list` reports "no open
  gates" while `status` says pending. Cosmetic; D-05's message already names `devflow ship --phase N`.
- **The ~40 non-`PATH` environment mutations in tests** — convert them (child process or passing values
  directly) and delete `ENV_MUTEX`. Until then each carries D-09's explicit `#[expect]`.
- **Lock reclaim by pid plus start time** — `lock.rs:192-194` treats any live pid as the holder; a recycled
  pid blocks takeover. Belongs with Phase 50's liveness work.
- **Ship finalization-retry gate recovery** — whether `devflow ship --phase N` or anything else consumes an
  answer there when nothing is waiting is unverified; D-05 names `ship` and lets its ack guard refuse.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SURV-01 | Two processes writing state for the same phase cannot silently lose an update (REQUIREMENTS.md:65). Criteria 1-2 amended by D-01: a second writer is excluded or refused loudly. | Writer inventory (23 production `save_state` sites, 2 unlocked); lock-wait pattern `acquire_project_blocking` (lock.rs:55-74); unique-temp publication (Pattern 1); stop/SIGTERM gap (Finding F-1); test design (Pattern 6) |
| SURV-02 | A gate whose consumer is gone reports the recovery that exists (REQUIREMENTS.md:73). Criterion 3 amended by D-05. | Every answer writer located (`gate_respond` commands.rs:1381, `stop_via_gate` :1806, `gate_sweep` :1442, `run_gate_with_timeout` auto-response pipeline_gate.rs:414); waiter identity primitives (lock.rs:178-211, agent.rs:203-219); exclusive response publish (Pattern 2); doctor gaps (Finding F-6); #200 e2e shape (Pattern 7) |
| CHKPT-01 | One parsed checkpoint predicate shared by preflight and resume (ROADMAP criterion 5). | Both predicates read (verify.rs:131-137, :207-220); corpus measurements with control (Finding F-3, F-4); fence rule incl. R-3 (Pattern 4) |
| CHKPT-02 | Checkpoints added or changed after Code preflight are re-scanned before the resume decision (criterion 6). | Resume decision point read (pipeline_launch.rs:1567-1624); recording site constraint (Finding F-7); `State` serde pattern (state.rs:33-396); tri-state storage (Pattern 5) |
| TEST-01 | Process-global `PATH` mutation in tests isolated (criterion 7), plus 999.80 fixtures. | Existing child helper (test_support.rs:362-489); counts done two ways (Finding F-9); clippy lint probe with controls (Pattern 8); PATH-removed fallback probe; child overhead measured |
</phase_requirements>

## Summary

The CONTEXT decisions are implementable with the standard library and code already in the tree: no new crates are needed, and `tempfile` must not be pulled into production because it is only a dev-dependency of both crates (`crates/devflow-core/Cargo.toml:32-33`, `crates/devflow-cli/Cargo.toml:24-25`). The writer rule (D-02) reuses `lock::acquire` plus a generalization of `acquire_project_blocking`'s polling loop; exclusive response publication (D-04/R-4) is `std::fs::hard_link` from a fully written unique temp file, which this session verified returns `ErrorKind::AlreadyExists` on both btrfs and tmpfs; the lint (D-09) was verified end to end against clippy 1.97.1 with positive and negative controls.

Research found **several places where CONTEXT's premises or measurements are incomplete**, collected under "Findings the planner must act on" below. The most consequential: no process in either crate installs a signal handler, so a `devflow advance` that `stop` signals dies without running `abort()` or `LockGuard::drop`. D-02.3's "the holder's own abort path clears state" is true for the gate path and false for `stop_via_lock`'s SIGTERM path. Taken literally, R-5 would silently stop `devflow stop` from marking such a phase stopped.

**Primary recommendation:** Land CHKPT-01's shared parser and TEST-01's child-process helper first, then run the writer rule (SURV-01) before honest recovery (SURV-02), in waves ordered by file overlap (see "Recommended Plan and Wave Shape"). Build every refusal test on a lock held by a live process, and pair it with a no-holder control that must still round-trip.

## Findings the Planner Must Act On

Each finding is either new information, or a correction or extension of CONTEXT that research surfaced. All are within existing decisions' remit unless marked **decision needed**.

**F-1 — `stop_via_lock`'s signal path has no abort path; R-5 as written regresses `stop`.** [VERIFIED: rg over `crates/devflow-cli/src` and `crates/devflow-core/src` for `sigaction|signal_hook|ctrlc|libc::signal\(|SIG_IGN|SIGTERM|SIGINT` returned only signal *senders* — `agent.rs:79` `unsafe { libc::kill(pid, libc::SIGTERM) == 0 }`, `monitor.rs:1402` — and no handler. The senders are the positive control: they prove the pattern matches.] `stop_via_lock` sends SIGTERM through `agent::terminate` (commands.rs:1915). The default SIGTERM action ends the process without unwinding, so `LockGuard::drop` and `abort()` never run. The lock file is left naming a dead pid, and the state file is left in place. Today `persist_stopped_state` then marks `stopped: true` (commands.rs:1939-1945). Under R-5 ("acquire the lock → load"), `stop` tries the lock microseconds after the signal, while the holder may still be alive, and writes nothing. The phase is left un-stopped, and `doctor` prescribes `devflow resume`, the opposite of what the operator asked for. **No existing test pins this path:** every `stop_e2e.rs` test that asserts `stopped` runs with no lock holder (:225-256, :455-538, :548-581, :595-652). Recommendation, inside D-02.3's "take the per-phase lock briefly": after a successful signal, retry `lock::acquire` for a short bounded window; `acquire` reclaims a dead holder's lock (lock.rs:100-117). Then persist. If the lock is still contended, print that the holder is still alive and state was not marked. Add a test that drives exactly this path.

**F-2 — 999.80's fixtures exist outside `pipeline_outcomes.rs`.** The note `"abort: test cleanup"` appears as a fixture 18 times in `pipeline_outcomes.rs` (a 19th hit, :2493, is a doc comment), **and** 3 times in `preflight.rs` (:1850, :2022, :2061), 3 in `pipeline_launch.rs` (:2584, :2745, :3587) and 2 in `pipeline_gate.rs` (:1085, :1405). [VERIFIED: rg, each line inspected.] D-10's definition ("tests kept from spawning a real agent only by an `abort: test cleanup` fixture note") covers all 26. Its measurement counted one file. Enclosing tests in `pipeline_outcomes.rs` include `external_verify_disagreement_gates_immediately` (:1612), `consecutive_failures_reaches_ceiling_across_cycles` (:2133), `ship_agent_failed_fires_gate` (:4960), `stage_failure_retry_cleans_stale_response` (:5298) and 14 more. Reachability to an agent spawn is still unmeasured for all 26 (D-10: the planner confirms it first).

**F-3 — CHKPT-01's exposure is real in this repository's own plans.** Across all 216 `*-PLAN.md` files, 8 contain the substring `gate="blocking-human"`, but only 3 declare it on a task-opening line. The other 5 would arm today's resume predicate without declaring anything: `33-02-PLAN.md`, `34-04-PLAN.md`, `43-01-PLAN.md`, `47-03-PLAN.md`, `47-05-PLAN.md`. [VERIFIED: python scan of `.planning/**/*-PLAN.md` this session.] Real declarations: `19-05-PLAN.md:83` and `19-11-PLAN.md:160` (`<task type="checkpoint:human-verify" gate="blocking-human">`), `44-04-PLAN.md:168` (`<task type="checkpoint:decision" gate="blocking-human">`), and `15-05-PLAN.md:73` (`<task type="checkpoint:human-action" gate="blocking">`). These are ready-made real-plan fixtures (review planner note).

**F-4 — "a line that opens a `<task` element" must mean line *start*, and the choice changes element extraction.** Today's anchor is `line.contains(TASK_ELEMENT_OPENING)` (verify.rs:215-220, with `const TASK_ELEMENT_OPENING: &str = "<task";` at verify.rs:162). Outside fences, 506 lines contain `<task` followed by whitespace or `>`; 505 start with it. The exception is prose with inline code, `35.1-03-PLAN.md:109`: "checkpoint task's markers as attributes on its own `` `<task ...>` `` opening tag". Two scans disagreed, and the prose line explains the gap completely. Anchoring anywhere in the line reported 1 file with an unclosed task element and 3 "nested" openings. Anchoring at line start (`^\s*<task[\s>]`) reported 0 of both. [VERIFIED: two python scans this session.] Use the line-start anchor; the same anchor makes D-07's whole-element extraction well defined across the corpus. The other D-06 measurements reproduced exactly: 0 fenced `<task` lines, 0 multi-line openings, 0 odd-fence files. A synthetic fenced task was the control and was detected (count 1).

**F-5 — `start` has no existing-run check, so D-02.1 changes `start`'s contention behaviour.** `start` (commands.rs:306-689) never calls `load_state` or `lock::`. Its only `save_state` is :670 (WR-11). [VERIFIED: Read commands.rs:630-689 plus a line-filtered rg of :306-689.] Once `start` takes the lock, `start --force` of a phase with a live holder refuses the way `resume` does (pipeline_launch.rs:1293-1301). Take the lock right after the dry-run return (commands.rs:355-356) and before worktree and branch side effects (`ensure_phase_worktree` :503, `feature_start` :518-519). Taken just before :670, a refusal would leave a new branch or worktree behind. `parallel` calls `start` sequentially in one process (`for (phase, agent) in pairs { start(...) }`, parallel.rs), so the hold does not change `parallel`.

**F-6 — Doctor has two prescribing checks, not one, and no lock facts.** `check_orphan_gate` (commands.rs:3222-3239) prescribes `format!("devflow gate approve {} --stage {}", ...)` whenever a gate is open and `gate_pending` is false. At a non-Ship gate with no waiter, that is exactly the lying repair D-05 removes. `check_gate_pending_without_gate` (:3205-3218) prescribes `format!("devflow resume --phase {}", facts.phase)`. `PhaseFacts` (:3165-3190, built at :3368-3410) has no lock-holder or waiter field, so the new "gate open, nothing waiting" finding needs one. Test assertions that pin today's repair strings: commands.rs :5812 (`"devflow gate approve 16 --stage ship"`), :5893, :5912, :6062, :6079 (`"devflow gate approve 3 --stage validate"`), :6096, :6154, :6310. [Search hits, not opened — plan-checker confirms.]

**F-7 — D-07's set must be recorded in `run_preflight`, never inside the check.** `generic_preflight_checks` has two production callers: `run_preflight` (preflight.rs:1335, which holds `&mut State`) and `resume`'s agent handoff, which runs it on a **cloned candidate** that may be discarded (pipeline_launch.rs:1307-1328). The check's own signature is fixed by D-09: "The signature is exactly `(&Path, &State) -> Result<(), String>` and must stay that way" (preflight.rs:1130-1133). Record the set in `run_preflight` after the checks run at `Stage::Code`, and in its `GateAction::Advance` arm (preflight.rs:1368-1377); never on the handoff candidate.

**F-8 — R-2's `--stage` must be optional, and it cannot tell two launches of one stage apart.** `advance` is a hidden subcommand (main.rs:118 `#[command(hide = true)]`; args at :119-127: `project: PathBuf`, `phase: Option<PhaseId>`), so adding a flag does not change `help_snapshot.rs`. The Legacy tail is `"; {binary} advance {project_root} --phase {phase}"` (monitor.rs:474-479). Because a script embeds `current_exe()` (monitor.rs:319), a binary rebuilt mid-run is invoked by an old script with no `--stage`. `advance` must accept `None` and skip the check (loudly), not fail. PipeOwning needs `--stage` on `__monitor` too, because `run_monitor` calls `advance(project_root, Some(phase))` (pipeline_launch.rs:977). Residual gap: two concurrent monitors for the *same* stage (for example `resume` while an old agent still runs) both pass the stage check. This fits D-03's accepted gaps; the ownership token is in Deferred Ideas.

**F-9 — TEST-01 counts reproduce two ways.** (a) Prefix-anchored per-variable count: `env::set_var("PATH` 104 plus `env::remove_var("PATH` 52 = 156 PATH lines, and 41 non-PATH lines, total 197. One further call, `pipeline_outcomes.rs:5246` `std::env::set_var(` with its argument on the next line, is invisible to that pattern, giving 198. (b) Comment-stripped bare `(set_var|remove_var)\(` lines: 198. The counts agree, and no `use std::env::{set_var…}` import exists. Every non-PATH helper that takes the variable name as an argument sits inside a `#[cfg(test)]` module: `config.rs:502/517/525` (module starts :489) and `pi.rs:320/328/329` (module starts :185). Per file, `PathGuard::set` has 8 call sites in `pi.rs` and 7 in `opencode.rs`. The authoritative `#[expect]` inventory is clippy's own output once the lint lands, not rg.

**F-10 — The "~40s" is the poll schedule, not the wedge.** `poll_response` reads once immediately, then sleeps with `backoff` starting at `Duration::from_secs(1)`, doubling, capped at `Duration::from_secs(60)` (gates.rs:263-279). The cumulative read times are 0, 1, 3, 7, 15, 31, 63, 123 s … [reasoned from source]. The self-resolving arm's answer-to-pickup interval is set by where the answer lands in that schedule. An answer written right after the gate file appears is picked up by the 1 s or 3 s read. Report the observed interval as a measurement of backoff position, never of wedge timing.

**F-11 — R-7's temp names must dodge two directory scanners.** `list_states` keeps names where `name.starts_with(STATE_FILE_PREFIX)` and `name.ends_with(".json")` (workflow.rs:221), with `const STATE_FILE_PREFIX: &str = "state-";` (workflow.rs:31). `Gates::list_open` parses any `*.json` that is not `.response.json` or `.ack.json` (gates.rs:155-160). A complete temp copy of a *request* that ends in `.json` would list as an extra open gate. Unique temp names must not end in `.json`, and must share a sweepable pattern.

**F-12 — `.claude/skills/ai-change-acceptance/`, cited by TESTING.md:107, does not exist in this worktree or the main checkout.** Neither `.claude/skills` nor `.agents/skills` exists. [VERIFIED: ls this session.] There are no project skills to load.

**F-13 — Adding `clippy.toml` does not fire the dev-setup checklist hook.** The trigger regex is `'^(\.gitconfig|CONTRIBUTING\.md|rust-toolchain\.toml|\.planning/config\.json)$|^scripts/hooks/|^\.github/(workflows/|PULL_REQUEST_TEMPLATE\.md|ISSUE_TEMPLATE/)|^\.devcontainer/|^scripts/(check\.sh|check-in-container\.sh|assert-image-parity\.sh)$'` (scripts/hooks/post-commit:80-82), which does not match `clippy.toml`. CLAUDE.md's rule ("tooling dependencies") may still apply by judgement; D-09 already asks the plan to check.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Per-phase mutual exclusion and waiter identity | devflow-core `lock.rs` + `agent.rs` | `.devflow/lock-NN` file | Lock file is the only cross-process record naming a live holder with its start time (lock.rs:124-147) |
| Atomic state publication | devflow-core `workflow.rs` | filesystem rename | `save_state` is the single state write primitive (workflow.rs:174-193) |
| Exclusive gate-response publication | devflow-core `gates.rs` (`Gates::respond`) | filesystem hard link | Every answer writer funnels through `respond` (gates.rs:186-231) |
| Writer rule (who may save when) | devflow-cli entry points (`start`, `advance`, `resume`, `stop`, `ship_override`) | core lock primitives | Lock acquisition lives at command entry today (pipeline_launch.rs:1293, :1482; pipeline_gate.rs:518) |
| Honest no-waiter messaging and repair lines | devflow-cli `commands.rs` (one shared message function) | `doctor` | D-05.5 requires one message code for CLI verbs and doctor |
| Leftover gate cleanup | devflow-core `recover.rs` / `Gates::cleanup` | devflow-cli `start` | R-1: only with no live lock holder |
| Checkpoint declaration parsing | devflow-core `verify.rs` | — | Both production callers already import from here (preflight.rs:1076, pipeline_launch.rs:1595) |
| Approved-checkpoint record | devflow-core `state.rs` (`State` field) | devflow-cli `preflight.rs` (record), `pipeline_launch.rs` (compare) | Must survive across the separate `advance` process (state.rs:333-339 rationale) |
| Test PATH isolation | test harness (`test_support` in both crates) | clippy lint config | No production change (D-08) |

## Standard Stack

No new dependencies. Everything below is already in the tree or in `std`.

### Core
| Library / API | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Rust toolchain | 1.97.1 (edition 2024) | Build, test, clippy | Pinned; `rustc 1.97.1 (8bab26f4f 2026-07-14)` and `cargo 1.97.1` observed this session; `edition = "2024"` at Cargo.toml:10 |
| `std::fs::hard_link` | std | Exclusive publish of gate responses (R-4) | Fails when the link path exists [CITED: doc.rust-lang.org/std/fs/fn.hard_link.html]; `ErrorKind::AlreadyExists` (os error 17) on btrfs and tmpfs [VERIFIED: local probe, success control included] |
| `std::fs::rename` + `File::create_new` | std | Unique-temp atomic state write (D-02.4) | Already the pattern (workflow.rs:189-191); `create_new` already used by the lock (lock.rs:85) |
| `lock::acquire` / `holder` / `holder_identity` | in-repo | Writer rule and waiter identity | lock.rs:33-35, :178-182, :198-211 |
| `agent::is_same_process` / `process_start_time` | in-repo | Confirm a live waiter | agent.rs:203-219 (Linux `/proc/{pid}/stat`) |
| `libc` | 0.2 (normal dep of devflow-core) | Already used for `kill` | `libc = "0.2"` at crates/devflow-core/Cargo.toml:23 |
| Clippy `disallowed_methods` + `clippy.toml` | clippy 1.97.1 | D-09 lint | Config keys `path`, `reason`, `replacement`, `allow-invalid` (default false) [CITED: Context7 /rust-lang/rust-clippy book/src/lint_configuration.md]; behaviour verified by local probe (Pattern 8) |
| `#[expect(lint, reason = "...")]` | rustc 1.97.1 | Annotate the ~40 kept env mutations | Probe: suppresses, and an unfulfilled expectation fails under `-D warnings` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | 3 (**dev-dependency only**) | Test fixtures | Tests only. `crates/devflow-core/Cargo.toml:32-33` and `crates/devflow-cli/Cargo.toml:24-25` list it under `[dev-dependencies]`, and `cargo tree -p devflow-core -e normal --depth 1` shows only `libc` of the two. Do **not** use `NamedTempFile::persist_noclobber` in production: it would add a runtime dependency. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `hard_link` exclusive publish | `renameat2(RENAME_NOREPLACE)` via `libc` | Linux-only and filesystem-dependent; hard link is POSIX and already probed |
| `hard_link` exclusive publish | `File::create_new` on the response path, then write | A crash mid-write leaves a torn response forever: the poller skips unparsable JSON (gates.rs:268-272) and every later responder gets `AlreadyResponded`. That is the exact D-04 hazard. |
| Blocking wait loop over `lock::acquire` | `flock(2)` | Deferred with the split-lock overhaul (CONTEXT Deferred Ideas) |
| Storing checkpoint element text in `State` | Hash of the element (sha2/blake3) | No hashing crate is a dependency (rg of all Cargo.toml: none). `std`'s `DefaultHasher` is not guaranteed stable across Rust releases [ASSUMED], so it is unfit for a value persisted across binaries. Stored text is small: 4 declarations across 216 plans. |

**Installation:** none.

## Package Legitimacy Audit

This phase installs no external packages: no crates added, no npm/PyPI packages. `gsd-tools query package-legitimacy check` was not run because there is nothing to check.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none) | — | — | — | — | — | — |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 operator shell                         detached processes                      disk (.devflow/)
 ──────────────                         ──────────────────                      ────────────────
 devflow start ──(lock-NN, D-02.1)──► launch_stage ─► run_preflight ──gate──►  gates/NN-S.json
      │                                     │  (Auto refusal: start waits here,      ▲
      │                                     │   holding lock = live waiter)          │ poll 1s→60s cap
      │                                     └─► spawn_monitor ─► agent ─► exit      │
      │                                                            │                │
      │                         monitor tail: advance --phase N --stage S (R-2)     │
      │                                   │                                        │
      │                   bounded wait on lock-NN (D-02.2) ── expiry ─► events.jsonl advance_failed
      │                                   │ acquired
      │                                   ├─ state.stage != S ─► refuse + advance_failed
      │                                   └─ evaluate ─► transition / gate ─► state-NN.json
      │                                                  │
      │                         GateReview: shared parser ─► blocking-human? ─► capture confirms?
      │                                                  │                         │
      │                              new/changed vs State record (D-07) ─► re-scan gate (human)
      │                                                  └ unchanged ─► auto-decide resume
      │
 devflow gate approve/reject ─┐
 devflow stop ────────────────┼─► waiter check: lock holder alive AND start time matches?
 devflow gate sweep ──────────┘         │ yes ─► Gates::respond (hard-link publish, first wins)
                                        │ no  ─► Ship gate: answer + "devflow ship --phase N"
                                        │        other gate: NO answer + "devflow resume --phase N"
                                        │                   or "devflow recover --clean --phase N"
                                        └ unconfirmable (legacy lock / no /proc) ─► say so, claim neither
 devflow stop (state write) ─► lock-NN acquire (brief, bounded retry after a signal — F-1) ─► load ─► save
 devflow recover --clean --phase N ─► no live holder? ─► clear state + Gates::cleanup + temp sweep (R-1, R-7)
 devflow doctor ─► same message function ─► findings (report-only, T-18-02)
```

### Recommended File Touch Map
```
crates/devflow-core/src/
├── workflow.rs        # unique temp name + orphan-temp sweep in clear_state (D-02.4, R-7)
├── gates.rs           # respond: unique temp + hard_link publish; cleanup sweeps temps (D-04, R-4, R-7)
├── lock.rs            # generalize the blocking acquire to per-phase locks (D-02.2)
├── verify.rs          # shared checkpoint parser; both predicates delegate (CHKPT-01)
├── state.rs           # new #[serde(default)] approved-checkpoint field (CHKPT-02)
├── recover.rs         # clean_phase: gate cleanup only with no live holder (D-05.4, R-1)
├── monitor.rs         # pass --stage in Legacy tail and __monitor args (R-2)
├── test_support.rs    # generic child-process test helper (D-08)
└── agents/{pi,opencode}.rs  # PathGuard tests → child process (TEST-01)
crates/devflow-cli/src/
├── commands.rs        # start lock; stop lock-before-load; gate_respond/stop_via_gate/gate_sweep waiter rule; doctor
├── pipeline_launch.rs # advance bounded wait + stage check; resume decision re-scan gate
├── preflight.rs       # record checkpoint set at Code evaluation and on refusal-gate approval
├── main.rs            # --stage on advance and __monitor
├── test_support.rs    # re-export/adapt child helper; delete NeutralPath once unused
└── pipeline_{outcomes,gate}.rs, staleness.rs  # PATH blocks → child process; 999.80 fixtures
crates/devflow-cli/tests/gate_wedge_e2e.rs     # #200 reproduction, both arms (new)
clippy.toml                                    # disallowed-methods (new, workspace root)
```

### Pattern 1: Unique temp name, then rename (state; D-02.4, R-7)
**What:** Each write gets a temp name nobody else can pick, created exclusively. Rename stays the publish step.
**When to use:** `write_state_atomic`, and gate request/ack writes. Per R-4, those keep overwrite semantics.
```rust
// Proposed (names are new, not in-repo). std only.
use std::sync::atomic::{AtomicU64, Ordering};
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// `.{file_name}.{pid}.{seq}.tmp` — leading dot and `.tmp` suffix keep it out of
/// `list_states` (needs "state-" prefix + ".json") and `Gates::list_open` (needs ".json").
fn unique_temp_path(target: &Path) -> PathBuf {
    let name = target.file_name().expect("target has a file name").to_string_lossy();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(".{name}.{}.{seq}.tmp", std::process::id()))
}

fn write_state_atomic(path: &Path, contents: &str) -> Result<(), WorkflowError> {
    if let Some(parent) = path.parent() { ensure_devflow_dir(parent)?; }
    let tmp = unique_temp_path(path);
    let mut file = std::fs::File::create_new(&tmp)?; // never follows or clobbers an existing path
    file.write_all(contents.as_bytes())?;
    drop(file);
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.into());
    }
    Ok(())
}
```
`use std::io::Write;` is already imported in workflow.rs (:11). `WorkflowError::Io(#[from] std::io::Error)` exists (workflow.rs:17-20).

### Pattern 2: Exclusive response publish (D-04, R-4)
**What:** Write the full response to a unique temp, then `hard_link(temp, response_path)`. `AlreadyExists` maps to the existing loud error. Unlink the temp in every case.
```rust
// Inside Gates::respond, replacing `write_atomic(&path, ...)` (gates.rs:199). Proposed.
let tmp = unique_temp_path(&path);
std::fs::write(&tmp, serde_json::to_string_pretty(response)?)?; // tmp is unique; no contention
let published = std::fs::hard_link(&tmp, &path);
let _ = std::fs::remove_file(&tmp);
match published {
    Ok(()) => {}
    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
        return Err(GateError::AlreadyResponded { phase, stage });
    }
    // Filesystems without hard links (FAT/exFAT, some network mounts) land here:
    // fail loudly — falling back to rename-overwrite would silently reintroduce D-04.
    Err(err) => return Err(err.into()),
}
```
In-repo values used: `GateError::AlreadyResponded { phase: PhaseId, stage: Stage }` and `GateError::Io(#[from] std::io::Error)` (gates.rs:84-98). Keep the `path.exists()` pre-check (gates.rs:196-198) as a fast path; the hard link is what makes the answer authoritative. Pin with a two-responder test: the first responder's bytes survive and the second gets `AlreadyResponded`. Control: a single responder still publishes.

### Pattern 3: Bounded per-phase lock wait (D-02.2)
**What:** Generalize `acquire_project_blocking` (lock.rs:55-74: 100 ms backoff doubling to a 2 s cap, returns the last `Contended` at the deadline) to per-phase locks. Every attempt goes through `acquire_path`, which reclaims a dead holder's lock.
```rust
// Proposed: lock.rs
pub fn acquire_blocking(project_root: &Path, phase: PhaseId, timeout: Duration)
    -> Result<LockGuard, LockError> { /* same loop as acquire_project_blocking, calling acquire(project_root, phase) */ }
```
`advance` calls it with a production constant and emits `advance_failed` on `LockError::Contended { pid, path }` (lock.rs:22-23). Tests call an inner function that takes the `Duration`, and hold the lock with an in-process `LockGuard`. Contention does not need a foreign pid: `acquire` fails on file existence, and the holder pid is the live test process (the `second_acquire_is_contended` test, lock.rs:311-322). **Bound (discretion):** recommend 10 minutes. After spawning, holders do seconds of work at most: `spawn_agent_and_record`'s tail (pipeline_launch.rs:1047-1070), `start`'s trailing prints (commands.rs:683-688), `resume` after `launch_stage`. A holder still present after minutes is at a gate wait or hung, and `doctor` → `resume` is the recovery D-03 accepts. The number is a choice, not a measurement (D-03). No environment variable: that would add operator surface and a test-time env mutation D-09 forbids.

### Pattern 4: One checkpoint parser, two predicates (CHKPT-01, D-06, R-3)
**What:** Parse plan text once into declarations; both public predicates become filters over that list.
**Rules, in order:**
1. **Fences.** Track opening fences as ```` ``` ```` or `~~~` after up to 3 spaces of indentation, recording the fence character and its run length. A fence closes only on the same character with a run at least as long [ASSUMED: CommonMark fence rule, not re-read this session]. A naive toggle is wrong when a `~~~` line sits inside a ```` ``` ```` block.
2. **R-3.** Lines inside a *closed* fence are ignored. If a fence is still open at EOF, rescan from that opening line to EOF as if unfenced, so the scan fails closed.
3. **Anchor (F-4).** A declaration opens on a line matching `^\s*<task[\s>]`.
4. **Element (D-07).** A declaration spans from its opening line to the first `</task>` line, the next opening, or EOF, whichever comes first.
5. **Classes.** Use the in-repo marker literals verbatim: `concat!("gate=", "\"", "blocking-human", "\"")` and `concat!("type=", "\"", "checkpoint:human-action", "\"")` (verify.rs:155-158). The closing quote is load-bearing (verify.rs:151-154).
```rust
// Proposed shape (names new).
pub struct CheckpointDeclaration { pub plan_file: String, pub element: String,
                                   pub blocking_human: bool, pub human_action: bool }
pub fn phase_checkpoint_declarations(root: &Path, phase: PhaseId) -> Vec<CheckpointDeclaration>;
pub fn phase_has_human_only_checkpoint(root, phase) -> bool      // any(blocking_human || human_action)
pub fn phase_has_blocking_human_checkpoint(root, phase) -> bool  // any(blocking_human) — Phase 47 D-02
```
Keep both public names and their callers: preflight.rs:1076 and pipeline_launch.rs:1595. The existing `phase_has_blocking_human_checkpoint` tests (8 test functions at verify.rs:345-487) must still pass, apart from any that pin a prose-only match. Flip `human_only_checkpoint_still_matches_a_task_tag_inside_a_fenced_example` (verify.rs:608-626), whose fixture is a fenced `<task type="checkpoint:human-verify" gate="...">`. Replace the doc paragraph at verify.rs:191-196. Add the R-3 negative (an unterminated fence before a real task still refuses) and the acceptance pair: a marker in prose neither blocks preflight nor arms resume, and a task-level `blocking-human` does both.

### Pattern 5: Recorded checkpoint set with a real third state (CHKPT-02, D-07, R-8)
```rust
// state.rs — follows the file's pattern: every added field is #[serde(default)] (e.g. :45-46, :287-288, :372-373).
/// `None` = never recorded (older binary, or no Code evaluation yet).
/// `Some(vec![])` = recorded, and the phase had no human-only checkpoints.
#[serde(default)]
pub approved_human_checkpoints: Option<Vec<ApprovedCheckpoint>>,   // name proposed
```
- **Why `Option`:** at the resume decision point, `None` and `Some(empty)` gate identically, since every present checkpoint counts as new. But D-07's "a later launch never widens it silently" needs "never recorded" kept separate from "recorded empty". A loop-back Code evaluation must not record agent-added checkpoints.
- **Record:** at the first Code evaluation where the field is `None`, and on approval of the preflight refusal gate or the new re-scan gate (R-8). Recording belongs in `run_preflight` (F-7).
- **Compare:** as a multiset of `(plan_file, normalized element)`. Normalize only line endings (CRLF → LF) and trailing whitespace per line, so an editor save is not a "change". Keep everything else byte-exact, and pin the rule with a real-plan fixture, for example `19-05-PLAN.md`'s task.
- **Where in the resume route:** inside `if checkpoint_confirmed { ... }` (pipeline_launch.rs:1597), **before** `relaunch_checkpoint_session` (:1602). That keeps T-28-01's order: the static declaration check stays first (:1572-1580).
- **Gate semantics to decide in the plan:** at a Code-stage re-scan gate, `GateAction::Advance` must mean "approved: record the set and continue into the auto-decide relaunch", **not** "advance to Validate". Handle the action explicitly; do not reuse `handle_stage_failure`'s dispatch.
- **Edge case to pin:** a run upgraded mid-flight has `None` until its next Code evaluation (a loop-back), which records whatever is present, possibly agent-added. Either accept this as a D-03-style gap, or gate a `None` at a loop-back evaluation as well. **Decision needed** (small): the planner states which.
- **Mode:** the resume route has no mode check (pipeline_launch.rs:1567-1602), so this applies in every mode (D-07).

### Pattern 6: SURV-01 failing-then-passing test with a single-writer control
- **Subject (fails today):** a *live foreign* lock holder exists → run `devflow stop --phase N` → assert that the state file bytes are unchanged and that the output names the live holder. Today `persist_stopped_state` writes `stopped: true` with no lock check (commands.rs:1930-1945), so the test is red.
- **Control:** the same fixture with no holder → `stopped: true` round-trips. This is today's `stop_marks_state_stopped_and_records_reason` (stop_e2e.rs:225-256).
- **Second subject (`start`):** a live holder exists → `devflow start --phase N` refuses and writes no state or worktree (F-5). Control: no holder → proceeds.
- **Harness pitfall:** in-process `lock::acquire` records the *test's own* pid, which suffices for contention tests (Pattern 3). An end-to-end test that runs the `devflow` binary as a child needs the holder to be a *different* live process with a matching start time. Use a real `devflow advance` child parked on a gate: `gate_sweep_e2e.rs:292-319` waits for `lock::holder(root, phase).is_some()` and the gate file. Do not hand-write a lock file for an arbitrary pid, which would exercise the reclaim path instead.

### Pattern 7: #200 reproduction, both arms (criterion 4)
Fixture: follow `gate_sweep_e2e.rs:43-59` (`init_repo`) and `stop_e2e.rs:151-156` (child `Command` with `.env("DEVFLOW_GATE_TIMEOUT_SECS", "15")` set per command, never process-global). Run `devflow start --phase N --mode auto <root>` in a repo with **no** `.planning/config.json`: preflight refuses at Define (`unattended_config_condition`, preflight.rs:990-997) and `start` parks at `.devflow/gates/NN-define.json`. Give the child a `PATH` holding a stub `claude` plus `git`/`sh` symlinks, on the `Command` only (`agent_free_dir_with_agent_stub` shape, test_support.rs:503-521).
- **Wedge arm:** wait for the gate file → kill the child (this test *must* signal, unlike `gate_sweep_e2e.rs`, whose file forbids signalling) → `devflow gate reject N` → assert: the no-waiter message, `devflow resume --phase N` or `devflow recover --clean --phase N` named, no `rm -f`, **no** `NN-define.response.json` written → `devflow recover --clean --phase N` → state, gate files and temp orphans gone.
- **Self-resolving arm (negative control):** wait for the gate file → write the rejection via `devflow gate reject` while the child lives → the child exits 0 with state cleared. Record the observed pickup interval (F-10).
- After D-02.1 the killed `start` leaves `.devflow/lock-NN` naming a dead pid. #200 recorded "No `.devflow/lock-07` file is created", which no longer holds. Assert the new fact, and that `recover --clean` removes the file (`remove_stale_locks`, lock.rs:258-285).
- If both arms wedge, the harness is broken (CONTEXT).

### Pattern 8: Child-process PATH isolation and the lint (TEST-01, D-08, D-09)
**Helper:** generalize `run_test_without_git` (test_support.rs:411-426), which re-execs `current_exe()` with `.arg(test_name).arg("--exact").arg("--test-threads=1")`, sets `.env("PATH", empty.path())` on the `Command`, and marks child mode with `CHILD_NO_GIT_ROOT = "DEVFLOW_CHILD_NO_GIT_ROOT"` (:373). The generalization takes a caller-built `PATH` directory, any extra per-`Command` env, and a child-mode marker. Place it once in `devflow_core::test_support`, which is compiled under `#[cfg(any(test, feature = "test-support"))]` (lib.rs:80-82, search hit) and so is reachable from both test binaries; `current_exe()` resolves per binary at runtime. `assert_child_ran_exactly_one_passing_test` (test_support.rs:449-489) moves or is re-exported with it. That is one helper rather than two; the cost is touching the cli file's imports (discretion).
**PATH must be a directory, never removed** [VERIFIED: probe this session]:
```
control A normal PATH       → git spawned, success=true
subject B PATH=<empty dir>  → spawn ERR kind=NotFound
subject C PATH removed      → PATH=None: git spawned, success=true   (default-path fallback; /bin/git and /usr/bin/git exist)
```
**Cost** [VERIFIED: 5 runs each, prebuilt `target/debug/deps/devflow-519cf288c2f5b2ac` from 2026-09-11, warm cache, 4 CPUs]: a no-child baseline test (`config_parse::tests::max_unattended_age_defaults_when_absent`) took 4 ms in all 5 runs; the child test (`test_support::tests::an_empty_path_child_cannot_resolve_git_while_the_parent_still_can`) took 9-11 ms. Every run reported `1 passed`, `362 filtered out`. That is about 5-7 ms per child, including that test's own git spawns. It does not establish cost under `taskset -c 0,1` or in the container, and the binary predates this branch.
**Lint** [VERIFIED: clippy 1.97.1 probe in a scratch crate]:
```toml
# clippy.toml (workspace root) — syntax exercised by the probe
disallowed-methods = [
    { path = "std::env::set_var", reason = "process-global env mutation races concurrent spawns; set env on the child Command instead" },
    { path = "std::env::remove_var", reason = "process-global env mutation races concurrent spawns; use Command::env_remove instead" },
]
```
Probe results under `cargo clippy --all-targets -- -D warnings`:
- **Run 1:** exit non-zero. `error: use of a disallowed method std::env::set_var` fired on both the fully qualified call and the `use std::env::set_var; set_var(..)` form. The call carrying `#[expect(clippy::disallowed_methods, reason = "...")]` did not fire.
- **Run 2:** every call annotated, exit 0.
- **Run 3:** an `#[expect]` with nothing to fulfil printed `error: this lint expectation is unfulfilled` (`-D unfulfilled-lint-expectations implied by -D warnings`). The run's exit code is confounded by a second, unrelated `items_after_test_module` error the probe itself caused. The printed error line is the evidence; the exit status is not.
- **Consequence:** `#[expect]` is self-policing. A conversion that removes a mutation but leaves its `#[expect]` fails clippy. Keep `#[expect]` off items compiled in any target where the call is absent.
- **Negative control for the phase:** add a deliberate `std::env::set_var` in a test and assert that clippy fails. Assert on the `disallowed method` text, not the exit code (the "control must fail for the right reason" rule).

### Anti-Patterns to Avoid
- **Unique temp name alone as the SURV-01 fix.** CONTEXT D-01: neither real overlap is caused by the shared `.tmp` name.
- **Deciding "waiter present" from `state.monitor_pid`.** T-23-51 (commands.rs:1780-1786): the monitor shell's pid is not the `advance` process. Use `lock::holder` + `holder_identity` + `is_same_process`, as `stop_via_lock` does (commands.rs:1852-1914).
- **Writing any answer file at a non-Ship gate with no waiter** (D-05.3). An unread answer silently decides the next firing (pipeline_gate.rs:463-464).
- **`Gates::cleanup` without checking the lock** (R-1). A waiter polls only the response path (gates.rs:261-272), and `respond` refuses once the request is gone (gates.rs:192-194).
- **Recording the checkpoint set inside `unattended_planned_checkpoint_condition` or on `resume`'s handoff candidate** (F-7).
- **Bare-name `cargo test --exact`** — a false green (TESTING.md False-Green Traps 1-3). Module-qualify and require `1 passed`.
- **Removing `NeutralPath` without re-reading `env_lock`'s premise.** test_support.rs:82-90: "Without those guards this accessor WOULD be unsound". If the remaining non-PATH mutations are trailing-statement restores, poison recovery stops being safe.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| "Is anyone waiting?" | pid-only liveness, cmdline sniffing | `lock::holder_identity` + `agent::is_same_process` | 999.47: a mid-`execve` child carries its parent's cmdline, and pids recycle (commands.rs:1871-1880) |
| Polling a lock | new sleep loop | the `acquire_project_blocking` shape (lock.rs:55-74) | Already tested wait/timeout semantics (lock.rs:437-464) |
| Stale lock takeover | manual `remove_file` | `acquire` (reclaim, lock.rs:100-117) / `remove_stale_locks` (lock.rs:258-285) | Never deletes a live holder's lock |
| Child test runs | ad-hoc `Command` + exit code | generalized `run_test_without_git` + `assert_child_ran_exactly_one_passing_test` | Four-way vacuity guard (test_support.rs:428-489) |
| Real-poller e2e | thread-based fake | `devflow advance` child parked on a gate (gate_sweep_e2e.rs:274-362) | Exercises the real lock and gate seam |
| Aborting a phase | `stop` running `abort()` | `recover --clean --phase N` | Rejected in D-05 |
| Env-mutation inventory | rg counts | clippy JSON output after the lint lands | Path-resolved; catches imports and multi-line calls rg misses (F-9) |

## Contract Inventory (P3 trial)

Existing contracts this phase touches. "Test" names a pinning test found this session; **"not located"** means plan-checker must find one or the plan must add one before changing the contract.

| # | Contract | Source (file:line) | Pinning test | Phase 48 effect |
|---|----------|--------------------|--------------|-----------------|
| C-1 | WR-11: state saved before the monitor exists; clear on launch failure | commands.rs:661-682 | not located | `start` now holds the lock across it (D-02.1); ordering unchanged |
| C-2 | T-23-51: `stop` never signals `monitor_pid` | commands.rs:1776-1786, :1846-1851 | not located (unit) | Unchanged; the waiter check reuses the lock identity |
| C-3 | T-23-52: fail-closed identity before signalling | commands.rs:1871-1914 | not located (unit) | Unchanged; F-1 adds a post-signal bounded lock retry |
| C-4 | `stop` gate path sends no signal; target unwinds through `abort()` | commands.rs:1794-1844 | stop_e2e.rs:135-218 | Preserved while a waiter is live |
| C-5 | `stop` marks `stopped` + appends `stop_reason`, never touches `stop_until` | commands.rs:1923-1947 | stop_e2e.rs:225-312, :548-652 | R-5 order; still true with no holder |
| C-6 | `stop` is idempotent; never clobbers an existing response (Ship gate) | commands.rs:1829-1840 | stop_e2e.rs:346-427 | Holds only if the Ship-gate no-waiter branch still writes (D-05.2/3) |
| C-7 | CR-01: abort cleans the stale response/ack | pipeline_gate.rs:460-477 | `stage_failure_retry_cleans_stale_response` (pipeline_outcomes.rs:5298, search hit) | Unchanged; cleanup also sweeps temps (R-7) |
| C-8 | T-28-01: static declaration check before anything agent-controlled | pipeline_launch.rs:1567-1596 | relaunch tests pipeline_launch.rs:2781-2876 (search hits) | Re-scan compare sits after the static check (Pattern 5) |
| C-9 | 999.76: step 2 reads the execution root, step 3 the project root | pipeline_launch.rs:1581-1596 | verify.rs:447-487 | Shared parser keeps a caller-owned root |
| C-10 | Phase 47 D-02: auto-decide for `blocking-human` only | pipeline_launch.rs:1594-1596 | not located | Preserved by the `blocking_human` filter |
| C-11 | Preflight D-08 (same evaluation in every mode) / D-09 (no override, fixed signature) | preflight.rs:1086-1135, :1194-1198 | `unattended_check_is_not_bypassed_by_yes_ship` (named at preflight.rs:1105) | Recording happens outside the check (F-7) |
| C-12 | D-18f: preflight gate `Advance` skips the re-check; `LoopBack` re-checks | preflight.rs:1308-1327, :1367-1385 | not located | Approval here records the set (R-8) |
| C-13 | T-18-02: `doctor` findings are report-only | commands.rs:3202-3205 | doctor tests commands.rs:6026+ (search hits) | New finding stays report-only |
| C-14 | 20e `ship_override` guards: lock → stage == Ship → request+response exist → ack absent | pipeline_gate.rs:490-554 | not located | Is the Ship-gate no-waiter repair; unchanged |
| C-15 | T-23-41: `reap` cannot approve; the sweep's only write path | gates.rs:207-231 | gate_sweep_e2e.rs:192-199 | Waiter rule added in front (R-6) |
| C-16 | First writer wins (`AlreadyResponded`) is benign for sweep, stop and auto-response | gates.rs:195-198; commands.rs:1433-1441, :1829-1840; pipeline_gate.rs:414-422 | stop_e2e.rs:346-427 | Now enforced atomically (Pattern 2) |
| C-17 | `resume` re-fires the preflight gate over a leftover request (rename-overwrite `write_gate`) | pipeline_gate.rs:369-371 | not located | R-4: `write_gate`/`ack` keep overwrite |
| C-18 | CR-03: the lock is per phase, not per project | lock.rs:8-11 | lock.rs:328-334 | Unchanged |
| C-19 | Stale-lock reclaim; `remove_stale_locks` keeps a live holder | lock.rs:90-117, :250-285 | lock.rs:367-412, :466-478 | The blocking wait relies on it |
| C-20 | 14-CR-06: `advance` failures logged to events.jsonl (monitor output → /dev/null) | pipeline_launch.rs:1463-1474; monitor.rs:520-522 | not located | Reused for wait expiry and the stage mismatch |
| C-21 | WR-04 / 18b: clear `monitor_pid` before fallible steps; save the real pid after spawn | pipeline_launch.rs:1000-1009, :1047-1053 | not located | Now runs under the lock in `start` |
| C-22 | 44 finding 2a: a failed resume re-marks stopped | pipeline_launch.rs:1385-1404 | not located | Unchanged (already under the lock) |
| C-23 | F-14: a marker in prose is not a declaration | verify.rs:164-212 | verify.rs:572 test | Kept; also applied to resume |
| C-24 | Known fenced-example limit | verify.rs:185-189 | verify.rs:608-626 | **Flipped** (D-06) |
| C-25 | `env_lock` poison recovery is sound only with RAII restores | test_support.rs:53-99 | test_support.rs `trailing_reap_call_is_skipped_when_a_later_assertion_panics` (named at :593) | Re-verify the premise after the PATH conversion |
| C-26 | TESTING.md one-mutex-per-variable invariant (D-04) | TESTING.md:88-96; test_support.rs:26-51 | reviewer-enforced | Doc updated (D-09) |
| C-27 | VALID-02: `stop` root parsing (flag wins; errors name the path) | stop_e2e.rs:540-810 | stop_e2e.rs:548-810 | Must not regress while `stop` changes |
| C-28 | Doctor repair strings | commands.rs:3202-3283 | commands.rs:5812, :5893, :5912, :6062, :6079, :6096, :6154, :6310 (search hits) | Deliberately updated (D-05.5, F-6) |

## Common Pitfalls

### Pitfall 1: A test that "proves" exclusion with the test's own pid
**What goes wrong:** the holder is the test process, so any "is the holder me?" check passes on every thread, and a refusal test goes green without exercising a foreign writer.
**How to avoid:** use in-process guards only for contention; use a real child (`devflow advance` at a gate) for identity and refusal (Pattern 6).
**Warning signs:** no child `Command`, but the assertion says "another process".

### Pitfall 2: A signalled holder looks alive for a moment
**What goes wrong:** `stop` signals, immediately tries the lock, sees a live pid, and writes nothing (F-1).
**How to avoid:** a bounded retry after a *successful* signal only; the message must distinguish "still alive" from "marked stopped".

### Pitfall 3: Stage binding breaks old monitors
**What goes wrong:** a mandatory `--stage` makes every monitor spawned by the previous binary fail to advance, into `/dev/null`.
**How to avoid:** `Option<Stage>`; `None` skips the check and emits an event naming the legacy invocation (F-8).

### Pitfall 4: Temp files that scanners pick up
**What goes wrong:** a temp ending in `.json` lists as an open gate or state file (F-11).
**How to avoid:** `.{name}.{pid}.{seq}.tmp`; unit test that `list_open` and `list_states` ignore a planted temp. The control is a real request, which must still list.

### Pitfall 5: The fence toggle misreads nested or indented fences
**What goes wrong:** a naive ```` ``` ```` toggle desynchronizes, and every later declaration is ignored — a fail-open result on the resume route.
**How to avoid:** track the fence character and run length, and apply R-3's unclosed-fence rescan. Fixtures: nested ```` ```` ```` around ```` ``` ````; `~~~` inside ```` ``` ````; unterminated fence before a real task.

### Pitfall 6: `#[expect]` on a shared helper compiled into a non-test target
**What goes wrong:** `unfulfilled_lint_expectations` fails the build for the target where the call is absent.
**How to avoid:** annotate the call site inside `#[cfg(test)]` code. All current non-PATH helpers qualify (F-9).

### Pitfall 7: Green tests that measure backoff, not behaviour
**What goes wrong:** a pickup assertion with a tight timeout flakes, or a loose one hides a real wedge.
**How to avoid:** bound the child with `e2e_child_timeout()` (default 90 s, gate_sweep_e2e.rs:79-85) and the child's own `DEVFLOW_GATE_TIMEOUT_SECS`; assert the on-disk outcome, not elapsed time (F-10).

### Pitfall 8: The Ship-gate branch silently stops writing
**What goes wrong:** applying "no answer without a waiter" to every gate breaks C-6 (stop_e2e.rs:346-427) and `ship_override`'s "request+response exist" guard.
**How to avoid:** the rule is scoped to non-Ship gates (D-05.3); keep a Ship-gate test in the same plan.

### Pitfall 9: Planning line numbers drift
**What goes wrong:** plans cite `:1482` after an earlier plan in the phase shifts lines.
**How to avoid:** cite symbols in acceptance gates (`rg -n 'fn advance'`), with line numbers only as hints. The CONTEXT evidence labels already warn about this.

## Recommended Plan and Wave Shape

File overlap is the binding constraint: execute-phase parallelizes within a wave, and wave number must equal DAG depth. Several requirements touch the same files (`pipeline_launch.rs` holds 57 PATH lines, `advance`, and the resume decision).

| Wave | Plan (suggested) | Requirement | Files (primary) | Depends on |
|------|------------------|-------------|-----------------|-----------|
| 1 | Planning bookkeeping: D-11 IDs, ROADMAP criteria 1-3 rewording (D-01, D-05), 999.38 + 999.80 record | all (docs) | .planning/ROADMAP.md, REQUIREMENTS.md | — |
| 1 | Shared checkpoint parser + fence rules + flipped test | CHKPT-01 | core verify.rs | — |
| 1 | Generic child helper in core `test_support`; convert pi.rs/opencode.rs | TEST-01 | core test_support.rs, agents/pi.rs, agents/opencode.rs | — |
| 2 | Convert cli PATH blocks + the 26 fixtures (F-2); clippy.toml; `#[expect]`s; TESTING.md | TEST-01 | cli pipeline_{launch,outcomes,gate}.rs, preflight.rs, staleness.rs, test_support.rs, config/gates tests in core | wave-1 helper |
| 3 | Core write primitives: unique temp (state + gates), exclusive respond, temp sweep, `acquire_blocking` | SURV-01 / D-04 | core workflow.rs, gates.rs, lock.rs | wave 2 (lint in place) |
| 3 | Checkpoint record + re-scan gate | CHKPT-02 | core state.rs; cli preflight.rs, pipeline_launch.rs (resume route only) | CHKPT-01, wave 2 |
| 4 | Writer rule: `start` lock (F-5), `advance` wait + `--stage` (R-2, F-8), `stop` lock-before-load (R-5, F-1) | SURV-01 | cli commands.rs, pipeline_launch.rs (`advance`), main.rs; core monitor.rs | wave 3 both |
| 5 | Honest recovery: waiter rule for respond/stop/sweep, cleanup under R-1, doctor finding and repairs | SURV-02 | cli commands.rs; core recover.rs | wave 4 |
| 5 | #200 e2e, both arms | SURV-02 criterion 4 | cli tests/gate_wedge_e2e.rs (new) | wave 4; its message assertions need the recovery plan, so place it in wave 6 or merge the two |

The ROADMAP sequencing ("correctness before survivability") holds: CHKPT lands before SURV. TDD mode is on (`workflow.tdd_mode: true` in .planning/config.json), so each plan opens with its failing test and its control.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `std::env::set_var` safe to call | `unsafe fn` in edition 2024 | Rust 2024 edition | Workspace is edition 2024 (Cargo.toml:10); every mutation already sits in `unsafe {}` |
| `#[allow(lint)]` | `#[expect(lint, reason = "...")]` | stable in 1.81 [ASSUMED version] | Verified working on 1.97.1; self-policing via `unfulfilled_lint_expectations` |
| Process-global PATH under a mutex (Phase 33 `NeutralPath`) | Child process with PATH set on the `Command` (Phase 46 C-01) | Phase 46 | 47 of at least 78 exposures closed by locks (CONTEXT D-08); the child route covers every spawn |

**Deprecated/outdated inside this repo:** the verify.rs:191-196 rationale ("fails SAFE"), and the fenced-example known limit (verify.rs:185-189).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo / rustc | all | ✓ | 1.97.1 | — |
| clippy | D-09 lint | ✓ (toolchain component) | 1.97.x (help URL names rust-1.97.0) | — |
| git | fixtures, e2e | ✓ | 2.55.0 | — |
| sh | monitor script, stub agents | ✓ | /usr/bin/sh | — |
| taskset | TEST-01 2-CPU sanity run | ✓ | util-linux 2.42.3 | `scripts/check-in-container.sh` pins CPUs via `DEVFLOW_CI_CPUS` (script :96-101) |
| podman | container parity check | ✓ | 5.8.4 | — |
| cargo-nextest, cargo-mutants, cargo-llvm-cov | optional (global rules) | ✓ installed | not probed | Project canonical is `cargo test` / `scripts/check.sh` |
| Linux `/proc` | waiter identity | ✓ (Fedora 7.1.8 kernel) | — | Non-Linux: unconfirmable-identity message (D-05) |

**Missing dependencies with no fallback:** none.
**Build cost warning:** no `target/` exists in this worktree. A cold workspace build and test takes minutes; background long runs (project memory: container push exceeds default timeouts).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust libtest via `cargo test` (toolchain 1.97.1) |
| Config file | none for tests; `clippy.toml` is new in this phase |
| Quick run command | `cargo test -p devflow-core --lib <module>::tests::<name> -- --exact` or `cargo test -p devflow --bin devflow <module>::tests::<name> -- --exact` — require `1 passed` in output |
| Full suite command | `scripts/check.sh all` (fmt, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`) |
| Integration target | `cargo test -p devflow --test <file_stem>` (e.g. `stop_e2e`, `gate_wedge_e2e`) |
| Phase-gate extras | `taskset -c 0,1 scripts/check.sh test` (sanity only, D-TEST-01 standard); `scripts/check-in-container.sh` |

### Phase Requirements → Test Map
Test names are proposals; the module path must be kept exact.

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SURV-01 | `stop` with a live foreign holder writes no state (red today); no-holder control round-trips | e2e | `cargo test -p devflow --test stop_e2e stop_with_a_live_lock_holder_writes_no_state` | ❌ new test in existing file |
| SURV-01 | `start` refuses under a live holder before side effects; control proceeds | e2e | `cargo test -p devflow --test start_reachability_e2e` (or new file) | ❌ |
| SURV-01 | `stop` after signalling a holder still marks stopped once the lock frees (F-1) | unit/e2e | `cargo test -p devflow --bin devflow commands::tests::stop_marks_stopped_after_signalled_holder_exits` | ❌ |
| SURV-01 | `advance` waits for the lock, then proceeds; expiry emits `advance_failed`; stage mismatch refuses (R-2); `None` stage accepted | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::advance_` | ❌ |
| SURV-01 | Unique temp names; `list_states`/`list_open` ignore planted temps (control lists a real file) | unit | `cargo test -p devflow-core --lib workflow::tests::` / `gates::tests::` | ❌ |
| SURV-01 | `acquire_blocking` waits and times out | unit | `cargo test -p devflow-core --lib lock::tests::` | ❌ (mirror lock.rs:437-464) |
| SURV-02 | Two responders: first bytes survive, second `AlreadyResponded`; single responder control | unit | `cargo test -p devflow-core --lib gates::tests::respond_` | ❌ |
| SURV-02 | `gate reject` / `stop` / `gate sweep` with no waiter at a non-Ship gate: no answer file, names resume/recover, no `rm -f`; live-waiter control writes | unit + e2e | `cargo test -p devflow --bin devflow commands::tests::` + `--test stop_e2e` | ❌ |
| SURV-02 | Ship gate with no waiter still writes and names `devflow ship --phase N` (C-6 kept) | e2e | `cargo test -p devflow --test stop_e2e stop_is_idempotent_against_an_already_answered_gate` | ✅ existing, extend |
| SURV-02 | `recover --clean --phase N` removes gate files and temps only with no live holder (R-1) | unit | `cargo test -p devflow-core --lib recover::tests::clean_phase_` | ❌ |
| SURV-02 | Doctor "gate open, nothing waiting" finding; repair strings match the CLI message | unit | `cargo test -p devflow --bin devflow commands::tests::` (doctor block, commands.rs:6026+) | ✅ file, ❌ cases |
| SURV-02 | #200 wedge arm + self-resolving control | e2e | `cargo test -p devflow --test gate_wedge_e2e` | ❌ Wave 0 |
| CHKPT-01 | Prose marker neither blocks nor arms; task-level `blocking-human` does both; fenced example ignored; unterminated fence fails closed (R-3) | unit | `cargo test -p devflow-core --lib verify::tests::` | ✅ file, flip :609 |
| CHKPT-01 | Corpus control: real-plan fixtures (F-3) classify as measured | unit | `cargo test -p devflow-core --lib verify::tests::checkpoint_parser_matches_real_plan_fixtures` | ❌ |
| CHKPT-02 | A checkpoint added after Code preflight parks at a gate; rewritten body gates; `None` gates; unchanged plan auto-decides (control) | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::` | ❌ |
| CHKPT-02 | Old state JSON without the field loads (`None`) | unit | `cargo test -p devflow-core --lib state::tests::` | ❌ |
| TEST-01 | Zero `set_var("PATH"` / `remove_var("PATH"` in test code | structural | `rg -n 'set_var\("PATH"\|remove_var\("PATH"' crates --glob '*.rs' \| rg -v '^\S+:\d+:\s*//'` → empty | — |
| TEST-01 | Lint negative control: a deliberate `std::env::set_var` fails clippy with `disallowed method` text | structural | `cargo clippy --workspace --all-targets -- -D warnings 2>&1 \| rg 'disallowed method'` | — |
| TEST-01 | Each converted child reports `1 passed` and a non-zero `filtered out` | unit | via `assert_child_ran_exactly_one_passing_test` | ✅ helper |

### Sampling Rate
- **Per task commit:** targeted `--exact` test(s) for the touched module, plus `cargo clippy -p <crate> --all-targets -- -D warnings`.
- **Per wave merge:** `scripts/check.sh all`.
- **Phase gate:** `scripts/check.sh all` green; one `taskset -c 0,1 scripts/check.sh test` run reported as a sanity check; `scripts/check-in-container.sh`; full-suite wall time recorded before TEST-01 and after (D-08 "Not measured").

### Wave 0 Gaps
- [ ] `crates/devflow-cli/tests/gate_wedge_e2e.rs` — SURV-02 criterion 4 (new file)
- [ ] Generic child-process helper in `crates/devflow-core/src/test_support.rs` — TEST-01 (all conversions depend on it)
- [ ] Real-plan checkpoint fixtures (copied task elements from F-3 files) — CHKPT-01/02
- [ ] A reusable "live foreign holder" fixture (a `devflow advance` child parked on a gate), extracted from gate_sweep_e2e.rs/stop_e2e.rs duplicates — SURV-01/02
- Framework install: none.

## Security Domain

`security_enforcement` is absent from `.planning/config.json`, so it is treated as enabled.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V1 Architecture (TOCTOU, trust boundaries) | yes | Lock-then-load ordering (R-5); hard-link publish; waiter check paired with honest "at time of check" messaging |
| V2 Authentication | no | Local CLI; gate authority is filesystem permission |
| V3 Session Management | no | — |
| V4 Access Control | yes (narrow) | T-23-41: reap cannot approve (gates.rs:207-231); no new approval path |
| V5 Input Validation | yes | Plan files are agent-writable during Code (verify.rs:118-121). The parser fails closed on malformed fences (R-3); `--stage` is parsed by clap into `Stage` |
| V6 Cryptography | no | No hashing chosen (stored text) |
| V7 Error Handling and Logging | yes | Refusals are loud: `advance_failed` events, `AlreadyResponded`, holder-alive messages; never silent success |
| V11 Business Logic | yes | Race conditions on state and responses (SURV-01, D-04); bounded wait (D-02.2) |
| V12 Files and Resources | yes | Unpredictable temp names created with `create_new` (no clobber, no following a pre-planted path); temp sweep scoped to known patterns |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Second writer overwrites state silently | Tampering | Per-phase lock for every writer (D-02); refuse loudly |
| Approval silently replaces rejection | Tampering / Repudiation | Exclusive hard-link publish; `responded_by` kept |
| Agent adds a `blocking-human` checkpoint to self-route into auto-decide | Elevation of Privilege | CHKPT-02: new or changed → human gate; the set changes only on human approval (D-07) |
| Prose mention arms auto-decide | Elevation of Privilege | CHKPT-01 shared anchored parser |
| Pre-planted symlink at a predictable temp path | Tampering | `File::create_new` on a unique name (today's `std::fs::write(&tmp)` follows and truncates) |
| Cleanup strands a live waiter | Denial of Service | R-1: cleanup only with no live holder |
| Signalling a recycled pid | Spoofing | Unchanged T-23-52 identity match |
| Waiting `advance` wedges forever | Denial of Service | Bounded wait + `advance_failed` + doctor → resume |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | CommonMark fence rule (same char, run ≥ opener, ≤3-space indent) | Pattern 4 | Parser mis-scopes fences; mitigated by R-3 and fixtures |
| A2 | `std::hash::DefaultHasher` output not guaranteed stable across Rust releases | Standard Stack alternatives | Only affects a rejected option |
| A3 | `#[expect]` stabilized in 1.81 | State of the Art | Cosmetic; behaviour verified on 1.97.1 |
| A4 | 10-minute `advance` lock-wait bound is adequate | Pattern 3 | A slow legitimate holder causes a logged stall → doctor/resume (D-03 accepts) |
| A5 | Hard links fail with a non-`AlreadyExists` error on FAT/exFAT/some network mounts | Pattern 2 | If they silently succeed or behave differently, exclusivity is weaker there; unmeasured |
| A6 | A killed foreground `start` is the only realistic #200 wedge shape | Pattern 7 | Other no-waiter gates are covered by the unit tests, not by the e2e |
| A7 | Test locations marked "search hit" / "not located" in the Contract Inventory are accurate | Contract Inventory | Plan-checker verifies each |

## Open Questions (RESOLVED)

1. **Upgraded run with no recorded checkpoint set at a loop-back Code evaluation (Pattern 5).**
   - What we know: `None` must gate at the resume decision; D-07 says a later launch never widens the set silently.
   - What's unclear: whether a first-ever recording at a *loop-back* evaluation counts as a silent widening.
   - Recommendation: gate it (fail closed), or record it as an accepted gap; the planner states which.
   - **RESOLVED (48-07):** gated, fail closed. `Unrecorded` is the serde default for an older state file and is never recorded by a Code evaluation; only `Pending` (state created by this binary) is recorded, so an upgraded run parks at the re-scan gate until a human approves. Pinned by `preflight::tests::code_preflight_does_not_record_an_unrecorded_set`.
2. **F-1 retry window after signalling.**
   - What we know: SIGTERM ends `advance` without unwinding; `acquire` reclaims dead-pid locks.
   - What's unclear: how long a SIGTERM'd devflow takes to disappear from `kill(pid, 0)` (zombie reaping by its parent shell counts; `agent_running` treats zombies as dead, agent.rs:36-46).
   - Recommendation: reuse `agent::TERMINATE_VERIFY_WAIT` (3 s, agent.rs:86) as the window; pin with a test.
   - **RESOLVED (48-12):** the window is `agent::TERMINATE_VERIFY_WAIT` (3 s), applied only after a successful signal. Pinned by `stop_marks_stopped_after_the_signalled_lock_holder_exits` and `stop_writes_no_state_while_the_lock_holder_survives_the_signal`.
3. **Ship finalization-retry gate recovery** remains unverified (CONTEXT Deferred); D-05 names `ship` and lets its ack guard refuse.
   - **RESOLVED (out of scope, 48-CONTEXT.md Deferred Ideas):** not fixed in Phase 48. 48-13's `no_waiter_repair` names `devflow ship --phase N` at the Ship gate and relies on `ship_override`'s ack guard to refuse.

## Sources

### Primary (HIGH confidence — read or executed this session)
- Source read with line ranges: `crates/devflow-core/src/{workflow.rs, lock.rs, gates.rs, verify.rs, recover.rs, state.rs, monitor.rs}`, `crates/devflow-cli/src/{commands.rs, pipeline_launch.rs, pipeline_gate.rs, preflight.rs, test_support.rs}`, `crates/devflow-cli/tests/{gate_sweep_e2e.rs, stop_e2e.rs}`, `scripts/hooks/post-commit`, `.planning/{ROADMAP.md §46-48, §999.118, §999.125-126, §999.38, §999.80; REQUIREMENTS.md; codebase/TESTING.md}`
- GitHub issue #200 body (gh CLI)
- Local probes: hard_link error kind (btrfs + tmpfs, with control); clippy `disallowed-methods` + `#[expect]` (3 runs); PATH removed vs empty-dir spawn (with control); child-process timing (5 + 5 runs); plan-corpus scans (two anchors, fenced control)

### Secondary (MEDIUM)
- Context7 `/rust-lang/rust-clippy` — `book/src/lint_configuration.md` disallowed-methods fields
- doc.rust-lang.org `std::fs::hard_link` (WebFetch; research-store tier LOW for the webfetch provider, upgraded here only by the local probe)

### Tertiary (LOW)
- CommonMark fence details and `#[expect]` stabilization version (training knowledge, see Assumptions)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; std behaviour probed.
- Architecture: MEDIUM — call paths read, but the D-02 waiter-holds-lock trace is CONTEXT's text search (over-inclusive); the recommended shapes are not executed.
- Pitfalls: HIGH for F-1..F-13 (measured or read); MEDIUM for wave shape (depends on plan split).

**Research date:** 2026-09-14
**Valid until:** 2026-10-14 for external facts; in-repo line numbers are valid only until the first Phase 48 commit touches the file.

# Phase 48: Survivable State Writes and Honest Gate Recovery - Context

**Gathered:** 2026-09-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Five defects on the unattended path, plus one folded backlog item:

1. **SURV-01** — two writers of one phase's state can silently overwrite each other.
2. **SURV-02** — `gate approve`/`gate reject`/`stop` claim a waiting process without checking, and the
   repair the roadmap assumes (`devflow resume`) only works for one kind of gate.
3. **CHKPT-01** (roadmap currently says `GATE-01`; backlog 999.125) — preflight and resume use different
   predicates for where a human-only checkpoint exists.
4. **CHKPT-02** (roadmap currently says `GATE-02`; backlog 999.126) — a human-only checkpoint added after
   Code's preflight reaches the agent's auto-decide route without a human ever seeing it.
5. **TEST-01** (backlog 999.38, with 999.80 folded in) — tests replace the process-global `PATH`, racing
   concurrent `git`/`sh` spawns; some tests are kept from spawning a real agent only by fixture content.

**Scope changes made by this discussion — planning must carry them as plan tasks:**

- ROADMAP Phase 48 success criteria 1–2 reworded (D-01) and criterion 3 reworded (D-05).
- Requirement IDs renamed to `CHKPT-01`, `CHKPT-02`, `TEST-01`; REQUIREMENTS.md gains v3.0.0 entries and
  traceability rows for them (D-11).
- 999.80 folded into TEST-01 (D-10).

**Deliberately not in this phase:** the split-lock state-handling overhaul (parked for Phase 50, D-03),
converting the ~40 non-`PATH` environment mutations in tests (D-09), and anything in Deferred Ideas.

**Evidence labels used below.** *Verified* = checked in source or by a command on 2026-09-14 against
`feature/phase-48` at `fbf6e98` (file:line given). *Reasoned* = derived from reading source, not reproduced.
Line numbers drift — re-check before citing them in a plan.

**Provenance labels.** *Operator* = chosen by the operator during discussion. *Claude (remit)* = decided by
Claude inside its own remit, stated to the operator during discussion, not objected to. Do not treat a
Claude (remit) entry as an operator mandate.

</domain>

<decisions>
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
    **Superseded 2026-09-22:** only partly closed. An answer that outlives its waiter still decides the next
    same-stage gate, for example after `resume`. It is accepted as `48-SECURITY.md` AR-48-04 and tracked in
    backlog 999.130.
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

</decisions>

<review_findings>
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

</review_findings>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone, requirements and the defects
- `.planning/ROADMAP.md` § Phase 48 — goal and success criteria 1–7 (criteria 1–3 amended by D-01 and D-05;
  labels renamed by D-11).
- `.planning/ROADMAP.md` § Phase 49 criterion 5 — SURV-01's field question is recorded there, not settled here.
- `.planning/ROADMAP.md` § Phase 999.118 — SURV-01's origin (fixed `.tmp` name).
- `.planning/ROADMAP.md` § Phase 999.125 and § Phase 999.126 — CHKPT-01/CHKPT-02 origin, fix shapes and
  acceptance.
- `.planning/ROADMAP.md` § Phase 999.38 — TEST-01 origin, including the second mechanism (panic mid-region)
  and the devflow-core third site family.
- `.planning/ROADMAP.md` § Phase 999.80 — folded into TEST-01; ordering note against 999.38.
- `.planning/REQUIREMENTS.md` — SURV-01, SURV-02; Future Requirements table (`GATE-01` #184 and `GATE-02` #170
  keep their IDs).
- GitHub issue #200 — the live reproduction and its two corrections (the wedge needs an interrupted `start`;
  `resume` recovers the preflight-gate case).
- `.planning/STATE.md` § Active Phase and § Operator Next Steps — promotion of 999.125, 999.126 and 999.38.

### Prior decisions this phase builds on
- `.planning/phases/47-unattended-decision-policy-consistency/47-CONTEXT.md` — D-02 (auto-decide resume is for
  `blocking-human` gates only) and D-13 (discovery of the predicate mismatch and the time-of-check gap).
- `.planning/audits/2026-09-13-phase-47-retrospective.md` § P3 and § P4 — process trials run in this phase.
- `.planning/PROJECT.md` § Key Decisions — Phase 46 C-01 (child-process `PATH` isolation), 20e (a second
  out-of-process consumer reuses the live terminal function), Phase 27 (`hermetic_command`).

### Test conventions
- `.planning/codebase/TESTING.md` — Environment Mutation Rule (updated by D-09), the one-mutex-per-variable
  invariant, False-Green Traps (`cargo test --exact`, package name `devflow`).
- `crates/devflow-cli/src/test_support.rs` — `env_lock` and `NeutralPath` premises, `run_test_without_git`,
  `assert_child_ran_exactly_one_passing_test`.
- `crates/devflow-cli/tests/gate_sweep_e2e.rs`, `crates/devflow-cli/tests/stop_e2e.rs` — real-binary
  end-to-end patterns (`devflow_bin()`, `e2e_child_timeout()`).

### External documentation checked during discussion
- Rust std `std::process::Command::new`, platform-specific behavior (program lookup through a `PATH` set on the
  `Command`; OS default when `PATH` is removed) — https://doc.rust-lang.org/std/process/struct.Command.html#method.new
- Clippy `disallowed_methods` configuration — https://rust-lang.github.io/rust-clippy/stable/index.html#disallowed_methods

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `lock::acquire`, `lock::holder`, `lock::holder_identity` (`crates/devflow-core/src/lock.rs`) and
  `agent::is_same_process` (`crates/devflow-core/src/agent.rs`) — live-waiter identity for D-02/D-05.
- `lock::acquire_project_blocking` (`lock.rs:55`) — existing polling-with-backoff shape for D-02's bounded wait.
- `Gates::cleanup` (`crates/devflow-core/src/gates.rs:294`) — for D-05's leftover-gate deletion.
- `pipeline_gate::abort` and `pipeline_gate::ship_override` — the terminal paths D-05's messages point at.
- `run_test_without_git`, `assert_child_ran_exactly_one_passing_test`, `agent_free_git_only_path_dir`,
  `stub_agent_binary`, `prepend_path` (`crates/devflow-cli/src/test_support.rs`) — D-08/D-10.
- `verify::phase_plan_files` (`crates/devflow-core/src/verify.rs`) — plan discovery for D-06/D-07.
- `events::emit` and the existing `advance_failed` event (`pipeline_launch.rs:1468-1473`) — D-02 refusal logging.

### Established Patterns
- Temp-write-then-rename persistence (`workflow.rs:185-193`, `gates.rs:356-364`).
- A second out-of-process consumer reuses the live path's terminal function instead of reimplementing it
  (`ship_override` → `finish_workflow_with_gate_timeout`).
- Fail-closed process identity before signalling (`stop_via_lock`, `commands.rs:1846-1921`).
- New `State` fields are `#[serde(default)]` so older state files still load.
- `doctor` findings are report-only (T-18-02).
- Every test carries a case that must produce the opposite result; `cargo test --exact` with a bare name is a
  known false green.

### Integration Points
- `commands::start` (`crates/devflow-cli/src/commands.rs:306`, save-then-launch at `:661-682`);
  `pipeline_launch::resume` (`crates/devflow-cli/src/pipeline_launch.rs:1287`), `advance` (`:1452`),
  `spawn_agent_and_record` (`:992`, `monitor_pid` save at `:1047-1053`), `run_monitor` (`:977`).
- `commands::stop`, `stop_via_gate`, `stop_via_lock`, `persist_stopped_state` (`commands.rs:1787-1947`);
  `gate_respond` (`:1385-1424`); `doctor` checks `check_gate_pending_without_gate` (`:3205`),
  `check_orphan_gate` (`:3222`), `check_dead_monitor` (`:3267`).
- `Gates::respond`, `Gates::reap`, `write_atomic`, `poll_response` (`gates.rs:186-231`, `:252-281`, `:356`).
- `run_gate_with_timeout` (`crates/devflow-cli/src/pipeline_gate.rs:352-430`); production gate sites:
  `preflight.rs:1367`, `pipeline_outcomes.rs:554`, `:729`, `:844`, `:913`, `pipeline_gate.rs:251`.
- `recover::clean_phase` (`crates/devflow-core/src/recover.rs:130-145`).
- `verify::phase_has_blocking_human_checkpoint` (`verify.rs:131-137`) and `phase_has_human_only_checkpoint`
  (`verify.rs:207-220`); production callers `preflight.rs:1076` and `pipeline_launch.rs:1595`.
- `preflight::unattended_planned_checkpoint_condition` (`preflight.rs:1067-1084`) and the preflight refusal
  gate in `run_preflight` (`preflight.rs:1328`, gate at `:1362-1384`).
- `PATH`-mutating test modules: `pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`,
  `preflight.rs`, `staleness.rs`, `test_support.rs` (devflow-cli); `agents/pi.rs`, `agents/opencode.rs`
  (devflow-core).

</code_context>

<specifics>
## Specific Ideas

- Operator priority stated repeatedly: do not over-engineer. Choose the smallest change that makes behaviour
  honest and closes the defect class, and record the accepted gaps explicitly rather than leaving them implicit.
- When a recommendation cannot be fully verified, say which part is reasoned and which is verified — the
  operator relied on that split to make D-01 through D-05.
- The roadmap's own premises were wrong twice in this phase (criterion 1–2's "both updates observable" does
  not match the defect; criterion 3's `resume` is not a general repair). Plans must test the corrected
  behaviour, not the original criterion text.

</specifics>

<deferred>
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

</deferred>

---

*Phase: 48-survivable-state-writes-and-honest-gate-recovery*
*Context gathered: 2026-09-14*

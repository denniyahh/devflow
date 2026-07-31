---
phase: 28-close-the-checkpoint-answer-return-path
plan: 03
subsystem: infra
tags: [rust, devflow-core, devflow-cli, checkpoint, session-resume, tdd]

# Dependency graph
requires:
  - phase: 28-01
    provides: "verify::phase_plan_files, verify::phase_has_blocking_human_checkpoint (D-01 static gate)"
  - phase: 28-02
    provides: "agent_result::session_id_from_capture, agent_result::checkpoint_reported_in_capture, State.session_id, State.checkpoint_resumes"
  - phase: 28-04
    provides: "prompt.rs split — idempotent_stage_prompt(phase) is Plan-only; define_stage_prompt(phase) is separate"
  - phase: 28-05
    provides: "resume()'s stop-marker clear gated on state.stopped (unrelated file, same module, sequenced first in the wave)"
provides:
  - "prompt::checkpoint_auto_decide_prompt(phase) -> String — deterministic synthesized instruction for an unattended checkpoint resume (D-03)"
  - "agents::claude::ClaudeAgent::exec_resume_command(session_id, instruction) -> (program, args) — inherent method, NOT on AgentAdapter (D-05), re-passes both --output-format json and --dangerously-skip-permissions on --resume"
  - "mode::MAX_CHECKPOINT_RESUMES: u32 — bounded resume ceiling, compile-time invariant check"
  - "pipeline_launch::spawn_agent_and_record(...) — extracted shared tail of launch_stage_inner and relaunch_checkpoint_session"
  - "pipeline_launch::relaunch_checkpoint_session(state, session_id) -> Result<(), CliError> — the bounded resume relaunch, emits checkpoint_auto_decided before spawning"
  - "advance()'s Action::GateReview arm now recognizes and resolves a confirmed human-blocking checkpoint via the five-condition D-01/D-03/D-05 guard, before falling through to the unchanged per-stage dispatch"
affects: ["phase 28's own verification/ship step", "any future work touching pipeline_launch.rs's advance() or launch_stage_inner"]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "TDD RED/GREEN per task pair, mirrored across three files at once for Task 1 (agents/claude.rs, prompt.rs, mode.rs), a full extraction + new-function pair for Task 2, and a dispatch-guard insertion for Task 3"
    - "Bounded-ceiling relaunch pattern (mode::MAX_CHECKPOINT_RESUMES), the same shape as MAX_INFRA_FAILURES/MAX_PREFLIGHT_RETRIES: increment with saturating_add, reset on the next ordinary entry point, fall through to the never-silent gate on exhaustion with a reason naming the exhaustion"
    - "Audit-before-spawn: the checkpoint_auto_decided event is emitted BEFORE the fallible relaunch spawn, so a crash mid-relaunch still leaves the decision recorded (D-07)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agents/claude.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "The A1 recognition path (whether a real headless claude -p --dangerously-skip-permissions checkpoint run actually renders the **Gate:** blocking-human literal this plan's confirmation reader keys on) is STILL UNCONFIRMED end-to-end. 28-PROBE.md's Wave-1 probe recorded verdict DIVERGENT — its own claude -p ... --dangerously-skip-permissions invocation was denied at that executing environment's Bash-tool permission classifier before the claude subprocess ever spawned, so no live checkpoint rendering (confirmed or otherwise) was ever observed by this phase. This plan's own attempt to independently re-probe was not made (would have required the same denied invocation shape from within this same classifier-restricted environment) — the unit-level contract built here (the resume argv, the prompt, the ceiling, the dispatch guard, the audit event) is fully implemented and fully tested against the documented/predicted rendering, but the phase's end-to-end claim (\"a real blocking-human checkpoint actually gets auto-decided\") remains unconfirmed against a live run, exactly as 28-PROBE.md and 28-02-SUMMARY.md both already flagged forward."
  - "Task 3's RED phase was verified interactively (a temporarily disabled dispatch guard reproduces pre-fix fall-through behavior, and the positive-resume test genuinely hangs on an unanswered Gates::poll_response — the exact failure mode the guard exists to close) but was NOT committed to git history: leaving that disabled-guard state as a replayable commit carries a real multi-minute hang risk for anyone who checks it out and reruns the test suite. RED evidence is recorded in this SUMMARY instead of a git commit. Task 1 and Task 2's RED phases WERE committed normally (unimplemented!() stubs, no hang risk)."
  - "Session id persistence (state.session_id = session_id_from_capture(...)) was placed immediately after the advance_evaluated event, for EVERY evaluated stage — not gated behind the checkpoint guard — per the plan's explicit instruction, so the value is already durable by the time any later resume needs it, independent of whether this particular stage turns out to be a checkpoint."
  - "The five-condition guard's ordering (agent==Claude, then the static phase_has_blocking_human_checkpoint scan, then the capture confirmation, then session id presence, then the ceiling check) is exactly the order the plan specified and is verified in source via the acceptance criterion's own rg command — the static scan strictly precedes the capture read, which is what keeps a fabricated Gate: line in an agent's own output from routing a phase that never declared a checkpoint into auto-decide (T-28-01)."

requirements-completed: ["999.57", "D-03", "D-04", "D-05", "D-07"]

coverage:
  - id: D1
    description: "A confirmed human-blocking checkpoint in a Claude-driven run is resolved by resuming the exact exited session, unconditionally, with no flag/config toggle (D-03), re-passing both --output-format json and --dangerously-skip-permissions on the --resume relaunch (Pitfall 1, T-28-02)"
    requirement: "D-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/claude.rs#agents::claude::tests::resume_command_includes_permission_bypass"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/claude.rs#agents::claude::tests::resume_command_resume_flag_immediately_precedes_session_id"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every auto-decided checkpoint leaves a durable audit record (stage, session id, instruction, attempt number) emitted BEFORE the relaunch spawns, so a spawn failure still leaves the decision on record (D-07)"
    requirement: "D-07"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::relaunch_checkpoint_session_emits_exactly_one_audit_event"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::relaunch_checkpoint_session_increments_and_persists_counter"
        status: pass
    human_judgment: false
  - id: D3
    description: "A phase whose plans declare no human-blocking checkpoint reaches exactly today's never-silent generic gate on any non-success exit — unchanged (D-01 primary gate)"
    requirement: "D-04"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_without_declared_checkpoint_falls_through_to_generic_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_with_declared_checkpoint_but_unreported_gate_falls_through"
        status: pass
    human_judgment: false
  - id: D4
    description: "A non-Claude agent never takes the resume path (D-05); the AgentAdapter trait itself is untouched"
    requirement: "D-05"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_with_non_claude_agent_never_resumes"
        status: pass
      - kind: other
        ref: "git diff crates/devflow-core/src/agents/mod.rs — confirmed empty"
        status: pass
    human_judgment: false
  - id: D5
    description: "A confirmed checkpoint that cannot auto-resolve (no session id / resume ceiling exhausted) never reads as a generic stage failure — the never-silent gate's context names the exact failed precondition"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_with_confirmed_checkpoint_and_no_session_id_falls_through"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_at_checkpoint_resume_ceiling_falls_through_to_generic_gate"
        status: pass
    human_judgment: false
  - id: D6
    description: "A stuck checkpoint loop is bounded: after MAX_CHECKPOINT_RESUMES, the run falls through to the never-silent gate, never a silent stop, never an unbounded loop. The counter resets on every ordinary fresh stage launch (per-stage budget, not per-phase)"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::launch_stage_inner_resets_checkpoint_resumes_counter"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_at_checkpoint_resume_ceiling_falls_through_to_generic_gate"
        status: pass
    human_judgment: false
  - id: D7
    description: "End-to-end claim — a real gate=\"blocking-human\" checkpoint fired by a live headless DevFlow run is actually recognized and auto-resolved by this code path"
    verification: []
    human_judgment: true
    rationale: "28-PROBE.md's A1 verdict is DIVERGENT, not CONFIRMED — the probing environment's own permission classifier denied the claude -p --dangerously-skip-permissions invocation before a real checkpoint was ever reached, so the exact **Gate:** blocking-human rendering this plan's confirmation reader keys on has never been observed against a live run. Every unit-level contract in this plan (the argv, the prompt, the ceiling, the guard ordering, the audit event) is fully implemented and fully proven against the documented/predicted rendering — but the phase's own headline end-to-end claim needs either a human sign-off accepting the documented-but-unconfirmed default, or a future live probe run from a context DevFlow's own monitor process is not classifier-restricted in (28-PROBE.md's own stated remaining path to CONFIRMED)."

# Metrics
duration: 55min
completed: 2026-07-31
status: complete
---

# Phase 28 Plan 03: Close the Checkpoint Answer Return Path — Resume Primitive, Dispatch Guard, Audit Event Summary

**IMPORTANT — read this first: the end-to-end recognition path (a real headless `claude -p --dangerously-skip-permissions` run actually hitting a `gate="blocking-human"` checkpoint and DevFlow correctly recognizing/resuming it) has NOT been observed working against a live run. `28-PROBE.md`'s Wave-1 probe recorded verdict `DIVERGENT` — its own invocation was denied at the probing environment's Bash-tool permission classifier before the `claude` subprocess ever spawned, so no live checkpoint rendering was ever captured. Everything this plan builds is fully implemented and fully unit-tested against the *documented, predicted* rendering (`**Gate:** blocking-human`), not an empirically confirmed one. Read on for what was actually built and proven at the unit level, but do not read this SUMMARY as proof the phase's headline capability works end-to-end.**

**The resume primitive (`ClaudeAgent::exec_resume_command`, re-passing `--output-format json` + `--dangerously-skip-permissions` per RESEARCH Pitfall 1), the deterministic checkpoint-resolution prompt, the bounded `MAX_CHECKPOINT_RESUMES` ceiling, the shared `spawn_agent_and_record` launch tail, the audited `relaunch_checkpoint_session`, and the five-condition `advance()` dispatch guard (D-01 static-scan-first, D-03 unconditional, D-05 Claude-only) that makes a confirmed human-blocking checkpoint resolve itself unattended.**

## Performance

- **Duration:** ~55 min
- **Started:** 2026-07-31 (worktree base `7333660`)
- **Completed:** 2026-07-31
- **Tasks:** 3 (all `tdd="true"`)
- **Files modified:** 4 (`agents/claude.rs`, `prompt.rs`, `mode.rs`, `pipeline_launch.rs`)

## Accomplishments
- `prompt::checkpoint_auto_decide_prompt(phase)` — a deterministic (no timestamp, no varying content) synthesized instruction sent into a resumed session: states plainly that no operator is available, that the agent must use its own judgment and continue, that it must record its reasoning in its final message, and terminates with the standard `COMPLETION_PROTOCOL` so the resumed session's exit is still parseable by Layer 1.
- `ClaudeAgent::exec_resume_command(session_id, instruction)` — an INHERENT method (not on `AgentAdapter`, D-05) building the exact resume argv: print flag, instruction, `--resume` immediately followed by the session id, `--output-format json`, `--dangerously-skip-permissions`. The permission-bypass flag has its own named regression test (`resume_command_includes_permission_bypass`) because a resumed Claude session restores neither flag from the original launch — omitting either reintroduces the exact silent headless hang this phase exists to close.
- `mode::MAX_CHECKPOINT_RESUMES: u32 = 3` — a bounded ceiling in the same documented-constant style as `MAX_INFRA_FAILURES`/`MAX_PREFLIGHT_RETRIES`, with a compile-time `const _: () = assert!(...)` guarding its own invariant (small, positive, no more lenient than the infra ceiling).
- `pipeline_launch::spawn_agent_and_record` — the mechanically extracted tail of `launch_stage_inner` (WR-04 monitor-pid clear, `ensure_agent_binary`, capture archival, `spawn_monitor`, pid persistence, registry registration, `stage_launched` event, println), now shared verbatim by `relaunch_checkpoint_session` so a checkpoint resume cannot drift from an ordinary launch's bookkeeping over time. Every pre-existing `launch_stage_inner` test (`launch_stage_persists_monitor_pid_for_reload`, `launch_stage_inner_clears_monitor_pid_on_early_failure`) passes unedited — proving the extraction changed no observable behavior.
- `pipeline_launch::relaunch_checkpoint_session` — increments `state.checkpoint_resumes` with `saturating_add`, emits exactly one `checkpoint_auto_decided` event (stage, session id, capped instruction, attempt number, policy tag) **before** the relaunch spawns (D-07), then delegates to `spawn_agent_and_record`. Does not call `run_preflight`, does not change `state.stage`.
- `advance()`'s `Action::GateReview` arm gained the five-condition D-01/D-03/D-05 guard, evaluated in this exact order (verified via `rg -n -B2 -A20`): (1) agent is Claude, (2) `verify::phase_has_blocking_human_checkpoint` (the static, plan-declared, agent-uncontrollable PRIMARY gate — checked BEFORE anything agent-controlled), (3) `agent_result::checkpoint_reported_in_capture` (the confirmation), (4) a session id is on record, (5) the resume ceiling is not exhausted. All five true → resume and return. Any false → fall through to the exact unchanged per-stage dispatch.
- `state.session_id` is now persisted from `agent_result::session_id_from_capture` for EVERY evaluated stage (not only checkpoint ones), immediately after `advance_evaluated` is emitted — so the value is already durable by the time any later relaunch needs it.
- Two unresolved-but-confirmed cases (no session id; ceiling exhausted) augment the never-silent gate's reason string via `augment_unresolved_checkpoint_reason`, so a confirmed checkpoint that couldn't auto-resolve surfaces loudly (T-28-16) rather than reading as a generic stage failure — proven by two named tests asserting the gate's `context` field contains the naming text.

## Task Commits

Each task followed the `tdd="true"` RED → GREEN cycle (per-task commit pairs):

1. **Task 1 RED — add failing tests for resume command + checkpoint prompt (D-03/D-04)** — `e58687d` (test)
2. **Task 1 GREEN — implement resume command, checkpoint prompt, and ceiling (D-03/D-04/D-05)** — `7146319` (feat)
3. **Task 2 RED — add failing tests for checkpoint resume relaunch (D-04/D-07)** — `77deba2` (test)
4. **Task 2 GREEN — implement checkpoint resume relaunch and its audit event (D-04/D-07)** — `3ba91f7` (feat)
5. **Task 3 — wire checkpoint auto-decide into advance() dispatch (D-01/D-03/D-05)** — `b009726` (feat; RED interactively verified but not committed — see Deviations)

**Plan metadata:** (this commit, made after this SUMMARY.md is written)

## Files Created/Modified
- `crates/devflow-core/src/agents/claude.rs` — `ClaudeAgent::exec_resume_command` inherent method + 4 new tests
- `crates/devflow-core/src/prompt.rs` — `checkpoint_auto_decide_prompt` + 4 new tests
- `crates/devflow-core/src/mode.rs` — `MAX_CHECKPOINT_RESUMES` const + compile-time invariant assertion
- `crates/devflow-cli/src/pipeline_launch.rs` — `spawn_agent_and_record` extraction, `relaunch_checkpoint_session`, `augment_unresolved_checkpoint_reason`, `advance()`'s five-condition guard, session-id persistence, 10 new tests

## Decisions Made
See `key-decisions` in frontmatter above — summarized: (1) the phase's A1 end-to-end recognition claim remains genuinely unconfirmed, documented here (not overstated); (2) Task 3's RED phase was verified interactively but deliberately not committed to git history, because the disabled-guard RED state carries a real hang risk (blocks on an unanswered `Gates::poll_response`) unsuitable for a replayable commit; (3) session id persistence runs unconditionally for every evaluated stage per the plan's explicit instruction; (4) the guard's five-condition order matches the plan's specification exactly, verified via the acceptance criterion's own `rg` command.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 2's `spawn_agent_and_record` was temporarily unreachable from production code between Task 2's commit and Task 3's commit, tripping `clippy::dead_code`**
- **Found during:** Task 2's `cargo clippy --workspace --all-targets --features devflow-core/test-support -- -D warnings` run
- **Issue:** `relaunch_checkpoint_session` (the only production caller planned for `spawn_agent_and_record`'s sibling path) has no caller until Task 3 wires it into `advance()`'s dispatch — the lib target (built without `cfg(test)`) reports it as dead code.
- **Fix:** None applied mid-task — this is expected, inherent sequencing from the plan's own two-task split (primitive in Task 2, consumer in Task 3). The plan's `<verification>` block scopes "`scripts/check.sh all` is green" to "before this plan's final commit," not to every individual task boundary. Verified clean immediately after Task 3 landed.
- **Files modified:** none (no code change needed; documented in the Task 2 commit message)
- **Verification:** `scripts/check.sh all` — clean after Task 3's commit (452 devflow-core + 247 devflow-cli unit tests, 0 failed workspace-wide, fmt + clippy both clean)
- **Committed in:** noted in `3ba91f7`'s commit message; resolved by `b009726`

**2. [Rule 1 - Bug/Test-command correction] Plan's literal `<verify>` commands use `-p devflow-cli`, which does not exist**
- **Found during:** Task 3, running the plan's literal verify command
- **Issue:** `cargo test -p devflow-cli ...` fails with "package ID specification `devflow-cli` did not match any packages" — the phase-specific notes and the `ai-change-acceptance` skill both flag this exact pitfall (the binary crate's Cargo package name is `devflow`, not `devflow-cli`).
- **Fix:** Ran every scoped test invocation as `cargo test -p devflow --features devflow-core/test-support <filter>` instead. The underlying assertions (`N passed`) are identical; `scripts/check.sh test` (the project's actual green-gate, unaffected by this naming pitfall) confirms the same result at full-workspace scope.
- **Files modified:** none (test invocation only, no source change)
- **Verification:** `cargo test -p devflow --features devflow-core/test-support pipeline_launch::tests::` — 17 passed, 0 failed
- **Committed in:** n/a (verification-only)

---

**Total deviations:** 2 (1 expected-sequencing clippy note, 1 test-command correction). Neither is scope creep; both are directly caused by the plan's own two-task split and a documented, pre-known project pitfall.
**Impact on plan:** None on deliverables. `scripts/check.sh all` is green at the plan's final commit, as the plan's `<verification>` block requires.

## Issues Encountered

**RED-state hang risk for Task 3 (recorded, not a defect):** while interactively verifying RED for Task 3's dispatch guard (temporarily forcing `checkpoint_confirmed = false`), the positive-resume test (`advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`) genuinely blocked past its 120s harness timeout — because without the guard, that test's fixture (no pre-written gate response, since the fixture expects the resume path to short-circuit before any gate write) falls through to `handle_stage_failure`'s `run_gate`, which polls `Gates::poll_response` with nothing ever written to answer it. This is itself confirmatory RED evidence (the exact "gates hang forever" failure class D-09 documents, now closed by this plan's guard) but was not committed as a git state — see Decisions above. No stray processes were left running after the harness auto-killed the timed-out test; verified via `ps aux` immediately after.

## User Setup Required

None — no external service configuration required.

## Known Stubs

None. All production functions (`checkpoint_auto_decide_prompt`, `exec_resume_command`, `spawn_agent_and_record`, `relaunch_checkpoint_session`, the `advance()` dispatch guard) are fully implemented against real logic, not placeholders.

## Threat Flags

None beyond what this plan's own `<threat_model>` already registered (T-28-01, T-28-02, T-28-03, T-28-15, T-28-16, T-28-05) — no new security-relevant surface (network endpoints, auth paths, file access patterns, schema changes) was introduced beyond what the plan itself declared and dispositioned.

## Next Phase Readiness

- The unit-level contract this plan builds (resume argv, deterministic prompt, bounded ceiling, dispatch guard, audit event, augmented never-silent-gate reasons) is complete, fully tested (17/17 `pipeline_launch::tests::` pass, 452 devflow-core + 247 devflow-cli unit tests pass workspace-wide, 0 failed), and `scripts/check.sh all` is clean (fmt + clippy `--workspace --all-targets -- -D warnings` + `cargo test --workspace`).
- **Carried forward, unresolved by this plan (as `28-PROBE.md` and `28-02-SUMMARY.md` both already anticipated):** the phase's own headline end-to-end claim — that a REAL `gate="blocking-human"` checkpoint fired by a live headless DevFlow run is actually recognized and auto-resolved — has never been observed working. `28-PROBE.md`'s A1 verdict is `DIVERGENT`, not `CONFIRMED`. This is a phase-level verification concern, not something plan 28-03 (or any single plan in this phase) can resolve on its own: it needs either an explicit human sign-off accepting the documented-but-unconfirmed default, or a future live probe run from a context (DevFlow's own actual `monitor` process) that is not subject to the classifier restriction that blocked `28-PROBE.md`'s own attempt.
- `git diff crates/devflow-core/src/agents/mod.rs` remains empty (D-05 — no `AgentAdapter` trait change).

## Self-Check: PASSED

- FOUND: `pub fn checkpoint_auto_decide_prompt` in `crates/devflow-core/src/prompt.rs`
- FOUND: `pub fn exec_resume_command` in `crates/devflow-core/src/agents/claude.rs`
- FOUND: `pub const MAX_CHECKPOINT_RESUMES` in `crates/devflow-core/src/mode.rs`
- FOUND: `fn spawn_agent_and_record`, `pub(crate) fn relaunch_checkpoint_session` in `crates/devflow-cli/src/pipeline_launch.rs`
- FOUND commit `e58687d` (Task 1 RED)
- FOUND commit `7146319` (Task 1 GREEN)
- FOUND commit `77deba2` (Task 2 RED)
- FOUND commit `3ba91f7` (Task 2 GREEN)
- FOUND commit `b009726` (Task 3)

---
*Phase: 28-close-the-checkpoint-answer-return-path*
*Completed: 2026-07-31*

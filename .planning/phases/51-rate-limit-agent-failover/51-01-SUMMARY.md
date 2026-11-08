---
phase: 51-rate-limit-agent-failover
plan: 01
subsystem: pipeline
tags: [rust, supervisor, rate-limit, failover, state]

requires:
  - phase: 50-socket-addressable-supervisor
    provides: "in-supervisor stage relaunch and the 50-13 handoff test harness"
provides:
  - "State.fallback_agent and State.fallback_consumed (serde-default, additive on the non_exhaustive State)"
  - "hand_off_agent: one substitution shared by resume --agent and rate-limit failover"
  - "failover_target: selects the fallback only at Code"
  - "explicit RateLimited arm in advance_with's AutoResume match that relaunches Code under the fallback"
  - "agent_handoff (reason rate_limited) and agent_handoff_refused events"
affects: [51-04, 51-06, 51-12, 51-14]

actuals:
  tokens: 8372
  tasks: 2
  commits: 4
plan_head_before: b8ece0e8d1a7f09300ce4a2b84fdd57fa71eb453
plan_head_after: 587e3d6e7bf6e04fceff07c7ac9da8a62d1816b5

tech-stack:
  added: []
  patterns:
    - "Handoff refusal as a value (HandoffOutcome::Refused) distinct from a state-write CliError"
    - "A second agent stub in the 50-13 handoff child's PATH dir (run_handoff_child_with_stubs)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "Refusal is returned as HandoffOutcome::Refused(HandoffRefusal::{Preflight, Unavailable}); resume --agent maps it to its two unchanged refusal texts, failover maps it to agent_handoff_refused plus the cron arm"
  - "The no-fallback e2e proves 'no successor supervisor' by waiting for N's socket to read Gone, then checking no other socket exists and exactly one stage_launched event, instead of sleeping the full 45 s step wait"
  - "The Plan control asserts the zero-handoff checks before the cron-file check, so a hop at Plan is the failure it reports"

patterns-established:
  - "Failover is reachable only from an explicit AgentStatus::RateLimited arm; the AutoResume wildcard still calls handle_rate_limited_outcome and nothing else"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "A rate-limited Code stage with an unused fallback is handed off by its own supervisor, relaunched under the fallback in the same root on the same branch, and completes; the fallback's commit sits on the primary's; no cron instruction, no resume event"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage"
        status: pass
    human_judgment: false
  - id: D2
    description: "With no fallback, a rate-limited Code stage takes the cron arm (devflow resume --phase N) and nothing hands off"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off"
        status: pass
    human_judgment: false
  - id: D3
    description: "Failover is Code-only: failover_target returns None at Define, Plan, Validate and Ship, and a Plan rate limit with a fallback configured takes the cron arm"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::failover_target_selects_the_fallback_only_at_code"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm"
        status: pass
    human_judgment: false
  - id: D4
    description: "A handoff refused at runtime emits agent_handoff_refused naming the fallback and the reason, then takes the cron arm; the fallback stays unconsumed"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_refused_handoff_takes_the_cron_arm_and_names_the_refusal"
        status: pass
    human_judgment: false
  - id: D5
    description: "The two State fields persist under their own keys and a pre-51 state file loads as no fallback, unused"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::fallback_fields_round_trip_and_tolerate_states_predating_them"
        status: pass
    human_judgment: false
  - id: D6
    description: "resume --agent runs through the shared hand_off_agent and keeps its behaviour and refusal texts"
    verification:
      - kind: integration
        ref: "cargo nextest run -p devflow --bin devflow -E 'test(/^pipeline_launch::tests::resume_with_agent/)' (6 tests) plus resume_re_marks_stopped_when_launch_stage_fails_outright"
        status: pass
    human_judgment: false

duration: 16min
completed: 2026-11-06
status: complete
---

# Phase 51 Plan 01: Rate-Limit Failover Tracer Summary

**A rate-limited Code stage now continues under a configured fallback agent, started by its own supervisor in the same tree on the same branch. Without a fallback, or at any other stage, the existing cron resume still fires. A refused handoff emits an event and then takes that same cron path.**

## Performance

- **Duration:** 16 min
- **Started:** 2026-11-06T15:38:24Z
- **Completed:** 2026-11-06T15:54:51Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `State.fallback_agent: Option<AgentKind>` and `State.fallback_consumed: bool`, both `#[serde(default)]`, added to `State::new`. The doc comment names `--fallback-agent` (51-04), the `devflow.toml` key read by `start` only (51-14, the flag wins), the Code-only scope, and says no environment variable is read.
- `hand_off_agent` runs the substitution for both `resume --agent` and failover. It does the full `generic_preflight_checks` on a candidate, then `ensure_agent_binary`, then driver `health`, all before any mutation. Then ONE `commit` sets the agent and, for a rate-limit hop, `fallback_consumed = true` in the same write. Then it emits `agent_handoff`. `resume_loaded` now calls it and keeps its exact refusal texts and its `println!`.
- `failover_target(&State, Stage)` returns the fallback only at `Stage::Code`, only while it is set, unused and different from `state.agent`.
- In `advance_with`'s `AutoResume` match, a new explicit `AgentStatus::RateLimited` arm sits between `Ambiguous` and the wildcard. It calls `fail_over_on_rate_limit` when `failover_target` selects a fallback, and `handle_rate_limited_outcome` otherwise. The `_ =>` wildcard is unchanged. A successful hop relaunches the same stage with `launch_stage(state, None, Some(stage))`, with no infra bump and no worktree reset.
- A refused hop emits `agent_handoff_refused` (`stage`, `from_agent`, `to_agent`, `reason`) and then calls `handle_rate_limited_outcome` with the original rate-limit reason.
- The 50-13 harness gains `run_handoff_child_with_stubs`, so a child can carry a second agent stub. Existing callers are unchanged: `run_handoff_child` delegates to it with no extra stubs.

## Task Commits

1. **Task 1 RED: failing tests for failover at Code** - `194da83` (test)
2. **Task 1 GREEN: fail a rate-limited Code stage over to the fallback** - `eabb0b5` (feat)
3. **Task 2 RED: cron-arm, refused-handoff and Plan-control tests** - `576a656` (test)
4. **Task 2 GREEN: report a refused failover and take the cron arm** - `587e3d6` (feat)

## Files Created/Modified
- `crates/devflow-core/src/state.rs` - two persisted fields, their `State::new` defaults, the serde test
- `crates/devflow-cli/src/pipeline_launch.rs` - `failover_target`, `HandoffReason`, `HandoffRefusal`, `HandoffOutcome`, `hand_off_agent`, `fail_over_on_rate_limit`, the `RateLimited` arm, the `resume_loaded` call site, the harness extension and five tests

## Verification

| Check | Result |
|-------|--------|
| Task 1 `<automated>` chain (5 named tests + `cargo semver-checks check-release -p devflow-core`) | exit 0; semver-checks: 229 checks pass, "no semver update required" |
| Task 2 `<automated>` chain (5 named tests) | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test -p devflow-core --lib` | 1119 passed, 2 ignored (includes `doc_check`) |
| `cargo test -p devflow --bin devflow` | 646 passed, 2 ignored |
| All six `resume_with_agent_*` tests | 6/6 pass |
| `handle_rate_limited_outcome` vs `ce08a9d` | byte-identical (61 lines each side); `pipeline_outcomes.rs` untouched by this plan |

**What these checks do not establish.** The tests use stub agents and the capture shape. They prove the AutoResume branch. They do not prove that a real provider's rate-limit envelope still matches the classifier. They do not cover the stream path's idle-timeout timing (RESEARCH Pitfall 8). They do not cover the auto-mode Claude → Antigravity pair with its stream relaunch and canary; plan 51-12 owns that. The Plan control covers only Plan end to end. Define, Validate and Ship are covered only by the pure `failover_target` unit test, not by an e2e each.

Each e2e passed on every run it got: three runs for the fallback e2e after GREEN, two for the other two. That is a weak bound and does not size flakiness. The fallback stub is Codex, as planned. Codex's Layer 1 reads a raw `DEVFLOW_RESULT` line through `parse_devflow_result`, which runs ahead of the Codex event parser. No extra fixture file was needed for the Code launch. Under Supervise, the unattended-launch report printed `[COULD NOT BE DETERMINED] no plan declares a human-only checkpoint` and then proceeded, as designed.

## TDD Gate Compliance

RED → GREEN for each task: `194da83` → `eabb0b5`, `576a656` → `587e3d6`. Each GREEN descends from its RED. No REFACTOR commit was needed.

RED evidence came from nextest JUnit reports, passed through `gsd-tools check tdd-red-evidence`. Every record returned `RED_EVIDENCE_OK`. The records are in `target/red-evidence/51-01-*.json` (gitignored).

| Test | Failed on | Semantic assessment |
|------|-----------|---------------------|
| `state::tests::fallback_fields_round_trip_and_tolerate_states_predating_them` | `fallback_agent must be persisted under its own key` (left `None`) | Intended. The RED scaffold declared both fields `#[serde(skip)]` so the test compiled; the key was absent from the JSON |
| `pipeline_launch::tests::failover_target_selects_the_fallback_only_at_code` | `an unused fallback at code` (left `None`, right `Some(Codex)`) | Intended. The stub returned `None` |
| `pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage` | `one handoff` (left 0). The child's event log shows `advance_evaluated` `ratelimited` at Layer 1, then `rate_limit_resume_scheduled` | Intended. The real supervisor classified the envelope and took the cron arm |
| `pipeline_launch::tests::a_refused_handoff_takes_the_cron_arm_and_names_the_refusal` | `one refusal event: []` | Intended. The Task 1 code refused silently |

The no-fallback e2e and the Plan control passed on their first run against Task 1's code, as the plan expected. Task 1 already left both of those paths on the cron arm. The no-fallback e2e's negative control is its own zero-handoff assertion.

**Plan control RED-equivalent.** I removed the `stage == Stage::Code &&` term from `failover_target` and re-ran `a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm`. It failed: `no handoff at Plan: [{"event":"agent_handoff","stage":"plan","from_agent":"claude","to_agent":"codex","reason":"rate_limited"}]`, after `stage plan → launched OpenAI Codex`. I restored the term from a saved copy, confirmed the restore with `cmp`, and checked that no process from that run was still alive.

## Decisions Made
- A refusal is a value (`HandoffOutcome::Refused`) carrying `Preflight` or `Unavailable` and the text. A `commit` error still propagates as `CliError`.
- "No supervisor socket other than stage N's is ever published within the step wait" is checked this way: wait until N's socket probes `Gone` (bounded by `HANDOFF_STEP_WAIT`), then assert no other socket exists and exactly one `stage_launched` event was emitted. Every successor is launched from inside N's advance, so nothing can appear once N is gone. The launch count rules out a successor that appeared and then went. This avoids an unconditional 45 s sleep.
- In the Plan control, the zero-handoff assertions run before the cron-file assertion. The first mutation run failed on the missing cron file instead of naming the hop; after the reorder the second run named the hop.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The worktree-count assertion in the fallback e2e compared moving data**
- **Found during:** Task 1 GREEN
- **Issue:** I compared `git worktree list --porcelain` before and after the run. That output includes each worktree's HEAD sha, which changes when the agents commit, so the test failed on correct behaviour.
- **Fix:** compare only the `worktree <path>` lines.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Verification:** the e2e passes, and every earlier assertion in it already passed in the failing run.
- **Committed in:** `eabb0b5`

---

**Total deviations:** 1 auto-fixed (1 bug in my own test). **Impact:** none on scope.

## Issues Encountered
- The RED scaffold briefly included a `reason()` method on `HandoffRefusal`. Clippy flagged it as dead code in Task 1, so I removed it there and re-added it in Task 2, where the refusal event uses it.

## Known Stubs
None.

## DevFlow Defects Found
None in this run's scope.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plans 51-04 (the `--fallback-agent` flag), 51-06 (canary and session clear in the handoff commit, the event's tree fields, the post-commit relaunch failure) and 51-14 (the `devflow.toml` key) build on `hand_off_agent` and the two State fields.
- `fallback_agent` cannot yet be set by an operator. Only tests set it, until 51-04 lands.

## Self-Check: PASSED
- FOUND: `crates/devflow-core/src/state.rs`, `crates/devflow-cli/src/pipeline_launch.rs`
- FOUND (ancestors of HEAD): `194da83`, `eabb0b5`, `576a656`, `587e3d6`
- `check evaluation-scope --plan 51-01 --commits-only`: status `resolved`, 4 commits

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-06*

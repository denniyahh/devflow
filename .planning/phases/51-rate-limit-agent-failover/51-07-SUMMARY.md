---
phase: 51-rate-limit-agent-failover
plan: 07
subsystem: pipeline
tags: [rust, failover, prompt, state, d-09]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-01: fail_over_on_rate_limit, hand_off_agent and the Code failover path; 51-06: the hardened hop and the waiting-agent test rig"
provides:
  - "FixType derives Serialize/Deserialize with snake_case names (additive)"
  - "State.code_fix: Option<FixType> (serde default, absent on disk when None)"
  - "prepare_loop_back_to_code sets code_fix in the commit that enters Code; transition clears it in both of its commits"
  - "private default_stage_prompt(&State, &dyn AgentDriver), the one renderer of a no-override stage prompt, called by launch_stage_inner and launch_stage"
  - "five tests, one of them a source scan"
affects: [51-09, 51-10, 51-11, 51-12, 51-13]

actuals:
  tokens: 7400
  tasks: 2
  commits: 4
plan_head_before: 6f24992c98c4d02de8ed4cb4f414ee93badaf39d
plan_head_after: edef2871bd42f88ce3feb40fa768f16ce34ee58d

tech-stack:
  added: []
  patterns:
    - "Persist an intent's TYPE, render it at launch with the current agent's driver"
    - "Delivered-prompt tests: a PATH stub writes its argv NUL-separated, then a marker file, and the test reads the argv after the marker"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "State.code_fix holds the FixType, never a rendered prompt; default_stage_prompt renders it with driver_for(state.agent) at launch, so after a handoff the fallback's own renderer is used (D-09)"
  - "code_fix is written only by prepare_loop_back_to_code and cleared by both transition commits (--until stop and stage move); no other write site"

patterns-established:
  - "Every no-override launch renders through default_stage_prompt; the scan every_no_override_launch_renders_through_default_stage_prompt fails a second default render anywhere in non-test pipeline_launch.rs"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "A Code failover during an audit-fix pass delivers the audit-fix workflow, in Codex's rendering, to the spawned fallback; control: a plain Code failover delivers execute-phase"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_code_failover_during_an_audit_fix_pass_replays_audit_fix"
        status: pass
    human_judgment: false
  - id: D2
    description: "One place renders a no-override stage prompt, and both launch_stage_inner and launch_stage call it"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::every_no_override_launch_renders_through_default_stage_prompt"
        status: pass
    human_judgment: false
  - id: D3
    description: "code_fix round-trips as \"audit_fix\", a pre-51 state loads with None, and None writes no key"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::code_fix_round_trips_and_defaults_to_none"
        status: pass
    human_judgment: false
  - id: D4
    description: "Both transition commits clear code_fix: the --until stop after Code and the move to Validate"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::a_transition_clears_the_persisted_code_fix"
        status: pass
    human_judgment: false
  - id: D5
    description: "A devflow resume mid-audit-fix delivers audit-fix to the spawned claude; control: no fix recorded delivers execute-phase"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_resume_mid_audit_fix_replays_audit_fix"
        status: pass
    human_judgment: false

duration: 11min
completed: 2026-11-07
status: complete
---

# Phase 51 Plan 07: Replay the Interrupted Fix Pass Summary

**A Code failover, a `devflow resume` or an `Ambiguous` re-drive in the middle of an audit-fix, gaps-only or full-execute pass now relaunches that pass, not the default execute-phase prompt. `State.code_fix` holds the fix type. The prompt is rendered at launch by the current agent's driver. A Codex fallback therefore gets the workflow-style audit-fix instruction and never Claude's slash command. The tests check the argv that a spawned stub actually received.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-11-07T22:26:41Z
- **Completed:** 2026-11-07T22:37:39Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- `FixType` derives `serde::Serialize, serde::Deserialize` with `#[serde(rename_all = "snake_case")]`. Its doc comment says the type is persisted in `State::code_fix`. `StageIntent` is unchanged since `ce08a9d` (checked with `git diff ce08a9d -- crates/devflow-core/src/prompt.rs`: the only hunk is `FixType`'s).
- `State.code_fix: Option<FixType>` has `#[serde(default, skip_serializing_if = "Option::is_none")]`, a doc comment, and `code_fix: None` in `State::new`.
- `prepare_loop_back_to_code` sets `s.code_fix = Some(fix)` in the same commit that sets `stage = Code`. `transition` sets `s.code_fix = None` in its `--until` early-return commit and in its stage-move commit.
- The private `default_stage_prompt(state, driver)` builds `StageIntent::Code { phase, fix: state.code_fix }` at Code and `for_stage_in_project(..)` at every other stage. It is now the no-override render in `launch_stage_inner`, whose prompt the agent receives, and in `launch_stage`, which uses it to resolve the program. `launch_stage` still passes the caller's `prompt_override` through.

## Task Commits

1. **Task 1 RED** - `d12843e` test(51-07): add failing tests for replaying a fix pass on relaunch
2. **Task 1 GREEN (tracer)** - `0ce1a8b` feat(51-07): replay the persisted fix pass on a same-stage relaunch
3. **Task 2 RED** - `7af7589` test(51-07): add failing test for clearing the fix pass on transition
4. **Task 2 GREEN** - `edef287` feat(51-07): clear the persisted fix pass on every transition

## Files Created/Modified
- `crates/devflow-core/src/prompt.rs` - serde derives and a doc sentence on `FixType`
- `crates/devflow-core/src/state.rs` - `State.code_fix` and its round-trip test
- `crates/devflow-cli/src/pipeline_gate.rs` - the loop-back write, the two transition clears, `waiting_claude_dir` and the transition test
- `crates/devflow-cli/src/pipeline_launch.rs` - `default_stage_prompt` and its two call sites, the argv-recording stub rig, the failover and resume tests, and the single-renderer scan

## Verification

| Check | Command | Result |
|-------|---------|--------|
| Task 1 gate (also re-run as the tracer gate after the GREEN commit) | the plan's 3 chained `cargo nextest run … --no-tests=fail -E 'test(=…)'` commands, then `cargo semver-checks check-release -p devflow-core` | exit 0. semver-checks: `229 checks: 229 pass, 31 skip`, `no semver update required` |
| Task 2 gate | the plan's 3 chained nextest commands, then `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 (the first run exited 101 on clippy, see Deviations) |
| Core suite | `cargo test -p devflow-core --lib` | `1135 passed; 0 failed; 2 ignored`, exit 0. Includes `doc_check` (no env read or `Command` variant added) and the existing prompt snapshot tests |
| CLI suite | `cargo test -p devflow --bin devflow` | `670 passed; 0 failed; 2 ignored`, exit 0 |

**What these checks do not establish.**
- The tests pin the prompt the spawned fallback receives. They do not show that a real agent then performs an audit-fix correctly. No stub test can show that.
- The failover test covers Codex, a workflow-style renderer. The resume test covers Claude, a Claude-style renderer. Pi, OpenCode, Antigravity and Hermes are not exercised. They reach the same `default_stage_prompt` → `driver.render_prompt` path.
- The `Ambiguous` re-drive is covered only by construction: it calls `launch_stage(state, None, …)`, which now renders through `default_stage_prompt`. No test drives an `Ambiguous` outcome mid-fix.
- Each delivered-prompt test passed on every run it got after GREEN: the failover test 4 times and the resume test 4 times (its first run was the expected unexpected-green RED run), counting the gates and the full suite. That bound is weak and does not size flakiness.

## TDD Gate Compliance

RED → GREEN: `d12843e` → `0ce1a8b`, then `7af7589` → `edef287`. Each GREEN commit descends from its RED commit. No REFACTOR commit. RED evidence came from nextest JUnit reports, written with a scratch `red` profile and checked with `gsd-tools check tdd-red-evidence`. The records are in `target/red-evidence/51-07-*.json` (gitignored).

The Task 1 RED commit includes a stub `State.code_fix` marked `#[serde(skip)]`. The stub keeps the tests compiling, so each one fails on an assertion and not on a build error. GREEN replaced it with the persisted field.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `state::tests::code_fix_round_trips_and_defaults_to_none` | RED_EVIDENCE_OK | `code_fix must be persisted under its own key`: left `None`, right `Some("audit_fix")` | Intended: the stub field is never written |
| `pipeline_launch::tests::a_code_failover_during_an_audit_fix_pass_replays_audit_fix` | RED_EVIDENCE_OK | `the Code pass records the fix type it was entered with`: left `None`, right `Some(AuditFix)` | Intended: the loop-back did not persist the fix. The B-1 mutation below shows the argv assertion itself can fail |
| `pipeline_launch::tests::every_no_override_launch_renders_through_default_stage_prompt` | RED_EVIDENCE_OK | the real file reported all four violations. The planted cases before it passed | Intended: two direct renders, and no `default_stage_prompt` yet |
| `pipeline_gate::tests::a_transition_clears_the_persisted_code_fix` | RED_EVIDENCE_OK | case (a): `a run halted after its Code pass is no longer mid-fix`: left `Some(GapsOnly)`, right `None` | Intended. Case (b) did not run in RED, so it was checked by the mutation below |
| `pipeline_launch::tests::a_resume_mid_audit_fix_replays_audit_fix` | INVALID_RED (unexpected_green) | n/a | Expected: Task 1's code already routes `resume` through `default_stage_prompt`. The plan anticipated this. The mutation below shows the test can fail |

**Mutation runs.** Each mutation was applied, run, then restored from a saved copy and confirmed with `cmp`. Each mutation record is RED_EVIDENCE_OK.
- **Review B-1 shape.** I restored the old direct `fix: None` render in `launch_stage_inner` only and kept `launch_stage` on `default_stage_prompt`. The failover test failed on `the fallback receives the audit-fix pass in Codex's own rendering`. The recorded argv held Codex's plain `execute-phase.md for phase 5171 --auto` instruction. The scan reported three violations: not once, outside `default_stage_prompt`, and `launch_stage_inner` bypassing it. So a fix only in `launch_stage` stays red, which is what the plan requires.
- **Resume.** I made `default_stage_prompt` render `fix: None` at Code. The resume test failed on `the resumed agent receives the audit-fix pass`. The argv held `/gsd-execute-phase 5173 --auto`.
- **Transition case (b).** I removed the clear from the stage-move commit only. The test failed on `the transition out of Code clears the fix pass`: left `Some(GapsOnly)`, right `None`.

## Decisions Made
- **Case (b) uses a waiting `claude` stub with `legacy_claude_launch = true`.** A no-op `claude` would exit, and its supervisor would advance Validate and might loop back to Code, which writes `code_fix` again while the test reads it. The legacy launch also skips the delivery canary. The test asserts `supervisor.is_some()`, so "launches Validate" is checked and not assumed.
- **The argv stub writes a marker after the argv file.** The stub has no `mv` on its agent-free PATH. It writes `<program>-argv.bin`, then creates `<program>-argv.done`, and the test reads only after the marker. That avoids a torn read.
- **Scan ownership is by top-level fn.** A line belongs to the non-indented `fn` item above it, up to the next line that starts with `}`. A render inside an `impl` method maps to no fn, so it is reported as outside `default_stage_prompt`. That is the conservative direction.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy `collapsible_if` in the Task 1 scan helper**
- **Found during:** Task 2 verify (`cargo clippy --workspace --all-targets -- -D warnings` exited 101)
- **Issue:** `enclosing_top_level_fns` in the test module had a nested `if`/`if let`. It came from the Task 1 RED commit, and clippy had not been run at Task 1.
- **Fix:** collapsed it into a let-chain. The behaviour is unchanged, and the scan test passes.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Commit:** `edef287`

---

**Total deviations:** 1 auto-fixed (a lint fix in test code). **Impact:** none on scope.

## Issues Encountered
- My first B-1 mutation attempt did not apply. `cargo fmt` had already joined the call onto one line, so my replacement anchor matched nothing, and that run passed on unmutated code. I deleted those two records, reapplied the mutation with the right anchor, and the results above come from that second run.

## Known Stubs
None. The Task 1 RED stub (`#[serde(skip)]` on `code_fix`) was replaced in `0ce1a8b`.

## Threat Flags
None. The plan's threat model covers the new state field: T-51-17 (lifetime, pinned by the transition test) and T-51-18 (renderer mismatch, pinned by the argv tests and the scan). No package was added (T-51-SC).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Later plans that add a no-override launch to `pipeline_launch.rs` must render through `default_stage_prompt`. A second `for_stage_in_project(` there fails the scan.
- A new path that enters Code with a fix must set `code_fix` in its entering commit, as `prepare_loop_back_to_code` does. A new stage-move commit outside `transition` would need to clear it.

## Self-Check: PASSED
- FOUND: all four modified files
- FOUND (ancestors of HEAD): `d12843e`, `0ce1a8b`, `7af7589`, `edef287`
- Commits measured from the ledger: `git rev-list --count 6f24992..edef287` = 4

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-07*

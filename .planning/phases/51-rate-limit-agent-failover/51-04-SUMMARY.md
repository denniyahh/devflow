---
phase: 51-rate-limit-agent-failover
plan: 04
subsystem: cli-launch
status: complete
tags: [fallback-agent, failover, resume, start, preflight, d-01, d-02, d-03, d-05, review-b-3]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-01: State.fallback_agent / State.fallback_consumed, hand_off_agent, failover_target"
provides:
  - "`--fallback-agent <AGENT>` on `devflow start` and `devflow resume`"
  - "preflight::validate_fallback_agent(&State, AgentKind) -> Result<(), CliError> (pub(crate))"
  - "preflight::fallback_launch_shape_refusal(&State, AgentKind) -> Option<String> (pub(crate))"
  - "preflight::print_fallback_notice(fallback, agent) (pub(crate))"
  - "commands::start gains a trailing `fallback_agent: Option<AgentKind>` parameter"
  - "pipeline_launch::resume_with_fallback(project_root, phase, agent, legacy_claude_launch, fallback_agent) (pub(crate)); resume_loaded gains `fallback_agent`"
  - "pipeline_launch::refuse_fallback_for_resume (private), run before commit_notify_env"
  - "test_support::add_agent_stub (test-only)"
affects: [51-14, 51-06, 51-12, resume, start]

actuals:
  tokens: 10605
  tasks: 3
  commits: 5
plan_head_before: f4b789b8912773f2a05ee6c1932d98726eb54d1e
plan_head_after: bdc2b25b2f8dce26bbe8649df565e2e3d83da7dd

tech-stack:
  added: []
  patterns:
    - "Every refusal check on a launch verb runs on a candidate clone before the first commit, including the notify-command commit"
    - "Operator-facing lines that tests read from a child process go through std::io::stdout(), not println! (libtest captures println! on the test thread)"
    - "Refusal tests that drive `start` run in an agent-free hermetic child, so a lost refusal stops at a missing binary under a scratch cache"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/test_support.rs

key-decisions:
  - "The fallback notice is written with writeln!(std::io::stdout()), as the unattended-launch report already is, so the start test can read it from the child's stdout; println! output from the test thread is captured by libtest and never reached the parent"
  - "`resume` is now #[cfg(test)]: main dispatches to resume_with_fallback, so only tests call resume. Its signature is unchanged and its ~33 test callers are untouched; the long doc comment moved to resume_with_fallback"
  - "refuse_fallback_for_resume judges the run as it would launch: the agent after --agent, and the legacy opt-out after --legacy-claude-launch and the environment"
  - "resume --agent onto the saved unused fallback is refused with: `--agent codex is this run's saved unused fallback (codex); a run's agent and its fallback must differ — pass --fallback-agent with another agent to re-arm a different one`"

patterns-established:
  - "A launch-verb flag that is validated and persisted: validate before dry_run / holder / reservation on start, and before commit_notify_env on resume; persist in the commit the verb already makes"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "`devflow start --fallback-agent <AGENT>` parses, and an empty or unknown value is a parse error"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::start_accepts_a_fallback_agent_flag"
        status: pass
    human_judgment: false
  - id: D2
    description: "`start` persists the fallback, unused, before the first supervisor spawns, and prints the fallback and the primary"
    requirement: SUPV-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::start_persists_the_fallback_agent_in_the_run_state"
        status: pass
    human_judgment: false
  - id: D3
    description: "`start` refuses a fallback equal to the agent (D-02), and in auto mode one outside the unattended launch shape (D-03), before any write"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::the_fallback_launch_shape_check_refuses_only_in_auto_mode"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::start_refuses_a_fallback_equal_to_the_agent"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::start_in_auto_mode_refuses_a_fallback_outside_the_unattended_launch_shape"
        status: pass
    human_judgment: false
  - id: D4
    description: "`devflow resume --fallback-agent` parses, and re-arms a spent fallback on a stopped run and on a cron-paused run; a plain resume keeps it spent (D-05)"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::resume_accepts_a_fallback_agent_flag"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_with_a_fallback_agent_rearms_a_consumed_fallback"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_without_a_fallback_flag_keeps_a_consumed_fallback_spent"
        status: pass
    human_judgment: false
  - id: D5
    description: "`resume` refuses a fallback equal to the run's agent and `--agent` onto the saved unused fallback, before commit_notify_env; the saved notify command survives a different DEVFLOW_GATE_NOTIFY_CMD and the reservation is given back (review B-3)"
    requirement: SUPV-02
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_refuses_an_agent_equal_to_the_saved_unused_fallback"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_refuses_a_fallback_equal_to_the_run_agent_before_any_write"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_with_agent_from_a_rate_limited_state_relaunches"
        status: pass
    human_judgment: false

duration: 15 min
completed: 2026-11-07
---

# Phase 51 Plan 04: `--fallback-agent` on start and resume Summary

**An operator can now arm a fallback agent with `devflow start --fallback-agent <AGENT>` and re-arm a spent one with `devflow resume --fallback-agent <AGENT>`. Both verbs refuse a fallback equal to the agent, and in `--mode auto` one outside the Claude/Antigravity launch shape, before they write anything. On `resume` that includes the notify command (review B-3).**

## Performance

- **Duration:** about 15 min
- **Started:** 2026-11-07T21:11:06Z
- **Completed:** 2026-11-07T21:26:11Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- `Command::Start` and `Command::Resume` gain `#[arg(long)] fallback_agent: Option<AgentKind>`. There is no alias, visible alias or name attribute. clap parses the value into the closed `AgentKind` set, so an empty or unknown value is a parse error (T-51-10). The doc comments say failover is Code-only and that the flag overrides the `devflow.toml` key from 51-14. They also say no environment variable selects the fallback and that `resume` does not re-read `devflow.toml`.
- `preflight::validate_fallback_agent` checks four things, in order: the fallback differs from the agent (D-02); in auto mode it holds the Code-stage launch shape (D-03, T-51-09); its binary is installed; its driver reports healthy. It reads no state and writes nothing. `fallback_launch_shape_refusal` evaluates the existing `unattended_launch_shape_condition` on a candidate clone. That condition is not widened.
- `commands::start` validates and sets `state.fallback_agent` after the legacy opt-out block and before `if dry_run`, `claim_foreground_holder` and `reserve_launch`. It then prints `fallback agent: <fallback> takes over the Code stage once if <agent> is rate-limited` (T-51-11). `parallel` passes `None`.
- `pipeline_launch::resume_with_fallback` now holds `resume`'s body, and `main` dispatches `Resume` to it. `resume_loaded` calls `refuse_fallback_for_resume` right after `refuse_unprepared_run` and before `commit_notify_env`. The re-arm (`fallback_agent = Some(f)`, `fallback_consumed = false`) sits in the final commit every resume makes, so it is not conditioned on `stopped`.

## Task Commits

1. **Task 1 RED:** `cd98d8b` test(51-04): add failing tests for start --fallback-agent
2. **Task 1 GREEN (tracer):** `07b25e0` feat(51-04): persist a --fallback-agent given to devflow start
3. **Task 2:** `6915c07` test(51-04): pin start's refusal of an equal or auto-ineligible fallback. Tests only; Task 1's validator already satisfied them.
4. **Task 3 RED:** `baba04a` test(51-04): add failing tests for resume --fallback-agent
5. **Task 3 GREEN:** `bdc2b25` feat(51-04): re-arm a fallback with devflow resume --fallback-agent

## Files Created/Modified

- `crates/devflow-cli/src/main.rs`: the flag on `Start` and `Resume`, the dispatch to `resume_with_fallback`, and two parse tests.
- `crates/devflow-cli/src/preflight.rs`: `fallback_launch_shape_refusal`, `validate_fallback_agent`, `print_fallback_notice`, and one unit test.
- `crates/devflow-cli/src/commands.rs`: `start`'s new parameter, its validation and persistence, the `None` added to four positional test callers, and three tests.
- `crates/devflow-cli/src/parallel.rs`: passes `None`, with a comment.
- `crates/devflow-cli/src/pipeline_launch.rs`: `resume_with_fallback`, `refuse_fallback_for_resume`, the re-arm in the final commit, `resume` as a `#[cfg(test)]` delegate, and five tests.
- `crates/devflow-cli/src/test_support.rs`: `add_agent_stub`, test-only.

## TDD Gate Compliance

Each GREEN descends from its RED: `cd98d8b` → `07b25e0`, and `baba04a` → `bdc2b25`. Task 2 has no GREEN commit because no production change was needed. No REFACTOR commit was needed.

RED evidence came from nextest JUnit reports, generated with a scratch `red` profile and passed through `gsd-tools check tdd-red-evidence`. Every record returned `RED_EVIDENCE_OK`. The records are in `target/red-evidence/51-04-*.json` (gitignored). Each RED commit carried only the scaffolding needed to compile: the flags as `#[arg(skip)]`, and `start` / `resume_with_fallback` accepting the fallback and ignoring it.

| Test | Failed on | Semantic assessment |
|------|-----------|---------------------|
| `tests::start_accepts_a_fallback_agent_flag` | `a known agent parses: … UnknownArgument … --fallback-agent` | Intended: the flag did not exist |
| `commands::tests::start_persists_the_fallback_agent_in_the_run_state` | `left: None, right: Some(Codex)` | Intended: the start launched, but the scaffold dropped the fallback |
| `tests::resume_accepts_a_fallback_agent_flag` | `UnknownArgument … --fallback-agent` | Intended |
| `pipeline_launch::tests::resume_with_a_fallback_agent_rearms_a_consumed_fallback` | `the spent mark is cleared` | Intended: the resume relaunched without re-arming |
| `…::resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run` | `the spent mark is cleared` | Intended. The precondition asserts passed first: cron file present, `stopped == false`, `infra_failures == 1`, leftover record present |
| `…::resume_refuses_an_agent_equal_to_the_saved_unused_fallback` | `expect_err … : ()` | Intended: the scaffold handed off to codex |
| `…::resume_refuses_a_fallback_equal_to_the_run_agent_before_any_write` | `expect_err … : ()` | Intended: the scaffold ignored the fallback and relaunched |

**Tests that passed before their check existed, and their negative controls.** Four tests could not be RED on code written before them, so I proved each one bites by removing the check it guards. Each mutation was restored from a saved copy and confirmed with `cmp`.

- `preflight::tests::the_fallback_launch_shape_check_refuses_only_in_auto_mode`: I removed the `state.mode != Mode::Auto` early return. The test failed: Supervise + Codex returned `Some("the agent is \`codex\`, not claude or antigravity …")`.
- `commands::tests::start_refuses_a_fallback_equal_to_the_agent`: I disabled the equality check. The test failed with `names the fallback: agent binary \`claude\` not found`. The start got past validation and stopped only at the agent-free PATH.
- `commands::tests::start_in_auto_mode_refuses_a_fallback_outside_the_unattended_launch_shape`: I disabled the shape refusal in `validate_fallback_agent`. The test failed with `agent binary \`codex\` not found`.
- `pipeline_launch::tests::resume_without_a_fallback_flag_keeps_a_consumed_fallback_spent`: this guards behaviour that existed before the plan, so it passed on the scaffold. I made the re-arm unconditional (`fallback_consumed = false` on every resume). The test failed with `a plain resume does not re-arm`.

**Ordering control (review B-3).** I moved `refuse_fallback_for_resume` to just after `commit_notify_env`. Both resume refusal tests then failed with `left: Some("new"), right: Some("old")`. The notify assertions therefore discriminate on ordering. They do not pass merely because the run was refused.

## Verification

- Task 1 gate: `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=tests::start_accepts_a_fallback_agent_flag)' && cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=commands::tests::start_persists_the_fallback_agent_in_the_run_state)'`. Both passed, and passed again as the tracer gate before expansion.
- Task 2 gate: the three named tests. All passed.
- Task 3 gate, the plan's full chained command, exit 0: the six new tests, `resume_with_agent_from_a_rate_limited_state_relaunches`, `doc_check::source_devflow_env_vars_and_subcommands_are_documented`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo semver-checks check-release -p devflow-core` (229 checks pass, 31 skip, "no semver update required").
- Plan-level: `cargo nextest run -p devflow --bin devflow --no-fail-fast` gave 658 passed and 2 skipped. 4 were flagged leaky, all tests this plan did not touch: `a_deleted_root_contributes_nothing_to_the_reachable_set`, both `gate_sweep_*_stray` tests, and `reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates`.

**What these checks do not establish.**
- **D-03 is best effort by decision.** Launch can check only stage-independent conditions. The stage-dependent bundle runs again at the handoff (51-01's `hand_off_agent`), and a refusal there still ends the night on the cron arm (D-07).
- **The binary and health checks have no dedicated test here.** The `codex` and `claude` drivers use the default `health`, which always returns `Ok`. The mutation runs showed the missing-binary refusal text, but no test asserts `validate_fallback_agent`'s binary refusal or a failing driver health (Pi, OpenCode) on purpose.
- **The tests use stub agents on the capture shape.** The resume fixtures set `legacy_claude_launch = true`, because a stub `claude` cannot answer the stream launch's background-task canary. A re-armed fallback actually taking over at Code is 51-01's end-to-end, not this plan's.

**Cron-paused fixture finding.** The plan asked for any refusal of the leftover dead `supervisor` record to be logged as a gap. There was none. The leftover record (`gone.sock`, launch id `exited-launch`) did not trip the live-run check. `resume_with_fallback` relaunched and recorded a new launch id. So the scheduled `devflow resume` that reads the same record is not blocked by it.

**Pre-existing ordering, not changed.** The `--agent` handoff refusal (51-01's `hand_off_agent`) still runs after `commit_notify_env`. A refused `resume --agent X` for a reason other than the fallback rules can therefore still commit a changed notify command. This plan does not alter that. Review B-3 covered only the fallback checks this plan adds.

## Decisions Made

- The fallback notice uses `writeln!(std::io::stdout(), …)` rather than `println!`. See deviation 1.
- `resume` is now `#[cfg(test)]`. See deviation 2.
- `refuse_fallback_for_resume` checks the candidate with the `--legacy-claude-launch` / `DEVFLOW_CLAUDE_LEGACY_LAUNCH` opt-out already applied. That opt-out is OR-only and is applied in the same run before relaunch. Without it, `resume --legacy-claude-launch --fallback-agent claude` in auto mode would pass validation and then fail the shape at handoff. `start` already validates after its opt-out block, so the two verbs agree.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The fallback notice is written straight to stdout**
- **Found during:** Task 1 GREEN
- **Issue:** With `println!`, the start test passed in the child, but the parent's stdout assertion failed. libtest captures `println!` from the test thread, so the line never reached the child's stdout.
- **Fix:** `preflight::print_fallback_notice` writes with `writeln!(std::io::stdout())`. This is the same route `preflight_unattended_launch_check` uses for its report, which the child output already carried. The failing run before the fix is the negative control for the stdout assertion.
- **Files modified:** `crates/devflow-cli/src/preflight.rs`, `crates/devflow-cli/src/commands.rs`
- **Commit:** `07b25e0`

**2. [Rule 3 - Blocking] `resume` is now test-only**
- **Found during:** Task 3 RED scaffold
- **Issue:** Once `main` dispatches to `resume_with_fallback`, the non-test build no longer calls `resume`. The plan keeps `resume` as a one-line delegate. That left a `dead_code` warning, which `clippy -D warnings` rejects.
- **Fix:** `#[cfg(test)]` on `resume`, whose signature is unchanged and whose test callers are untouched. Its doc comment moved to `resume_with_fallback`, and two intra-doc links were repointed.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Commit:** `baba04a`

**3. [Rule 2 - Hermeticity] The start refusal tests run in an agent-free hermetic child**
- **Found during:** Task 2, the negative controls
- **Issue:** The tests first ran in-process. With the refusal removed, `start` ran on into `launch_stage` under the host's environment. Only the test-only `launch_context` guard stopped it ("in-crate test launched a stage without a hermetic DEVFLOW_CACHE_DIR"). I found no write to `~/.cache/devflow` after the runs, but relying on that guard is fragile.
- **Fix:** both tests go through `run_in_an_agent_free_child`. In that child a lost refusal stops at a missing agent binary under a scratch cache. I re-recorded both negative controls in this form.
- **Files modified:** `crates/devflow-cli/src/commands.rs`
- **Commit:** `6915c07`

**4. [Rule 3 - Blocking] The resume fixtures use the capture shape**
- **Found during:** Task 3 RED
- **Issue:** The first RED run of the three relaunching resume tests failed with "background-task notification delivery is ABSENT". The stub `claude` was launched on the stream shape and could not answer its canary. The plan's model test avoids this because it relaunches `codex`.
- **Fix:** the fixtures set `legacy_claude_launch = true`, the same capture shape `start_for_test` uses. RED was re-recorded and failed on the planned assertions.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Commit:** `baba04a`

**5. [Rule 3 - Blocking] A test helper outside `files_modified`**
- `test_support::add_agent_stub` puts a second no-op stub on a child PATH, because the fallback tests need both `claude` and `codex`. It is test-only (`crates/devflow-cli/src/test_support.rs`, `cd98d8b`).

**Total deviations:** 5 auto-fixed (4 blocking, 1 hermeticity). **Impact:** none changes the plan's behaviour or interfaces. Deviation 2 narrows `resume` to the test build, so non-test code now calls `resume_with_fallback` instead.

## Issues Encountered

None beyond the deviations above.

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

- 51-14 can now feed its `devflow.toml` `fallback_agent` key into `start`'s new trailing parameter, with the flag winning.
- 51-06 and 51-12 build on `hand_off_agent` and on the fallback fields this plan makes settable.

## Self-Check: PASSED

- Files: all six modified files exist; this SUMMARY exists.
- Commits on this branch: `cd98d8b`, `07b25e0`, `6915c07`, `baba04a`, `bdc2b25`. Each was confirmed with `git merge-base --is-ancestor`.
- Acceptance criteria re-run:
  - All 11 named tests pass.
  - `start` validates before `dry_run` and the holder.
  - `validate_fallback_agent` loads and writes no state.
  - `resume`'s signature is unchanged.
  - Every fallback check precedes `commit_notify_env`.
  - semver-checks reports no major change.

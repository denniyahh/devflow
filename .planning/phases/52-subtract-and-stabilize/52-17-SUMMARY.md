---
phase: 52-subtract-and-stabilize
plan: 17
subsystem: pipeline
tags: [cut-01, cut-04, define-stage, stage-machine, prompt, clap, semver]

requires:
  - phase: 52-16
    provides: every run starts at Plan; `Stage::Define` still compiled and nothing started or entered it
provides:
  - "`Stage` is `Plan`, `Code`, `Validate`; `\"define\"` does not parse and the parse error offers only plan, code, validate"
  - "`StageIntent::Define` and the private `define_stage_prompt` are gone, with every core and CLI match arm for Define"
  - "`devflow start --until define` (and `Define`, `DEFINE`, `ship`, `Ship`, `SHIP`) is a clap invalid value; `PLAN`, `CODE`, `VALIDATE` follow one case rule (all accepted)"
  - "`devflow --help` reads `Plan → Code → Validate`; no help output names Define or Ship; the help snapshot is regenerated"
  - "the handoff fixture's stage N is Plan with `--until code`; the e2e stubs answer no `stage define` turn; the scoped docs show three stages"
affects: [52-18]

actuals:
  tokens: 27450
  tasks: 3
  commits: 3
plan_head_before: aaeae14372878942019d1e5ddf5540f65a89fff6
plan_head_after: 520c9368e5c8d52933a0daa116126b6bb353b756

tech-stack:
  added: []
  patterns:
    - "A stage-machine test enumerates every name a stage has ever had and keeps the ones that parse, so a reintroduced variant fails `plan_is_the_first_stage_and_validate_the_last` without the test naming it"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/stage.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-core/src/agents/codex.rs
    - crates/devflow-core/src/outcome_policy.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-core/src/events.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/help_snapshot.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-cli/tests/stop_e2e.rs
    - crates/devflow-cli/tests/doctor_json_e2e.rs
    - crates/devflow-cli/tests/test_home_isolation.rs
    - crates/devflow-cli/tests/log_format_env.rs
    - README.md
    - ARCHITECTURE.md
    - OPERATIONS.md
    - docs/guides/unattended-mode.md

key-decisions:
  - "The handoff fixture `HandoffRun::launch_with` now launches stage N = Plan with `stop_until = Code`. The stub prints a `DEVFLOW_RESULT` success, which Layer 1 accepts before the Plan/Code zero-commit gate, so the ~49 tests built on it keep their subjects; tests that named N+1 as Plan (the agent-spawn hold `plan:` → `code:`) moved one stage."
  - "`gates::STAGES` is three stages, so `recover --clean` and `Gates::close_phase` no longer look at a 3.x `NN-define` gate file. Accepted under T-52-20 / D-30 (no migration)."
  - "Controls that used Define as 'a stage Codex declares RequiresExistingArtifact but no run starts at' now use Code or Validate. No such stage is left, so `preflight_interactivity_check_flags_auto_plan_without_context_md`'s not-first-stage control and `start_context_probe_asks_the_driver_about_plan`'s control are weaker: the stage there is HeadlessSafe for Codex as well as not first."
  - "failover_e2e's claude stub logs `unexpected turn` for any turn that is not the canary, Plan or Code, rather than dropping the catch-all, so an unexpected Validate turn still shows in the invocation-sequence assertions."
  - "Commit subjects use `feat(52-17)!` and `test(52-17)` (as 52-16 did), not the plan's `feat(core)!`, so GSD's TDD gate finds them."
  - "`cargo semver-checks check-release -p devflow-core` (baseline: published 3.0.0) exits 100 with 9 major lints, the same 9 lint kinds as after 52-15. This plan adds `Stage::Define` and `StageIntent::Define` under `enum_variant_missing`. Intended (D-01); the version bump stays with close-out."

patterns-established:
  - "A single-element stage loop left by a deletion is unrolled (clippy `single_element_loop`)"

requirements-completed: [CUT-01, CUT-04]

coverage:
  - id: D1
    description: "`Stage` is Plan, Code, Validate: `define` and `ship` do not parse, the parse error offers neither, no parsable stage precedes Plan, and the chain from Plan is Plan, Code, Validate"
    requirement: CUT-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/stage.rs#stage::tests::removed_stage_names_do_not_parse"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/stage.rs#stage::tests::plan_is_the_first_stage_and_validate_the_last"
        status: pass
      - kind: unit
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail (1015 passed)"
        status: pass
    human_judgment: false
  - id: D2
    description: "`--until` rejects every case spelling of define and ship as a clap invalid value, and PLAN/CODE/VALIDATE follow one case rule"
    requirement: CUT-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#until_rejects_removed_stages"
        status: pass
    human_judgment: false
  - id: D3
    description: "`devflow --help`, `start --help` and `resume --help` name neither Define nor Ship as a whole word; the committed help snapshot matches"
    requirement: CUT-04
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_names_no_removed_surface"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D4
    description: "No CLI path knows a Define stage; the CLI unit suite, the e2e suites and the doc identifier check pass with runs that start and hand off at Plan"
    requirement: CUT-01
    verification:
      - kind: unit
        ref: "cargo nextest run -p devflow --bin devflow --no-tests=fail (602 passed)"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p devflow --test failover_e2e / gate_wedge_e2e / stop_e2e / doctor_json_e2e, supervisor_e2e capture_supervisor_owns_a_fake_agent_and_records_devflow_result"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false
  - id: D5
    description: "README, ARCHITECTURE, OPERATIONS and the unattended-mode guide describe the pipeline as Plan → Code → Validate"
    requirement: CUT-04
    verification: []
    human_judgment: true
    rationale: "Prose accuracy: doc_check proves the named identifiers exist, not that the surrounding sentences describe the three-stage pipeline correctly"

duration: 24min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 17: Delete the Define Stage Summary

**`Stage` is now Plan → Code → Validate. `Stage::Define`, `StageIntent::Define` and `define_stage_prompt` are gone with every match arm, `--until define` is a clap error in every case spelling, the help reads `Plan → Code → Validate`, and the handoff fixture, e2e stubs and scoped docs all start a run at Plan. `scripts/check.sh all` passes.**

## Performance

- **Duration:** about 24 min
- **Started:** 2026-11-11T03:21:27Z
- **Completed:** 2026-11-11T03:46Z
- **Tasks:** 3
- **Files modified:** 27 (336 insertions, 401 deletions across the three commits)

## Accomplishments

- core: `Stage` has three variants; `next`, `is_agent_stage`, `gsd_command`, `Display`, `FromStr` and the parse error (`expected plan, code, or validate`) have no Define arm. `StageIntent::Define` and `define_stage_prompt` are deleted, with their arms in `render_claude_style`, `render_workflow_style`, the conformance loop, Codex's interactivity table (Codex still declares Plan `RequiresExistingArtifact`), `gates::STAGES` and the mode, outcome-policy and recover tests.
- CLI: `STREAM_JSON_STAGES`, `close_gates_for_resume`'s gate-id list, `set_aside_leftover_gates` and the `Stage::Define => transition(.., Plan)` arm lose Define. The doc on `STREAM_JSON_STAGES` now says the canary runs at the first stage, Plan, and records that the phase-34 Define and Ship captures belong to stages v4.0 removed.
- `main.rs`: `start` is `Begin the workflow for a phase: Plan → Code → Validate.`; `--until plan` "runs Plan then stops before Code"; `resume` does not "reset the workflow to Plan". The help snapshot was regenerated with `cargo run -q -p devflow -- --help`; its only change is the start line.
- e2e and docs: failover_e2e's stub has no `stage define` branch; doctor_json_e2e opens a Plan gate; stop_e2e plants a Validate gate. README, ARCHITECTURE, OPERATIONS and `docs/guides/unattended-mode.md` show three stages; OPERATIONS lists `--until plan|code|validate`, names `--until define` and `--until ship` as rejected, and moves the Codex preflight refusal row to Plan. `docs/guides/quickstart.md` had no stage list or Define mention, so it is unchanged.

## Task Commits

1. **RED for Tasks 1 and 2:** `4ffd6ad` (test)
2. **Tasks 1 and 2, one commit after Task 2's gate passed:** `0efbfbb` (feat!)
3. **Task 3:** `520c936` (feat!)

**Plan metadata:** the docs commit that adds this file.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch nextest config, `--profile red`). Each record went through `gsd_run check tdd-red-evidence`; every verdict was `RED_EVIDENCE_OK` / `target_test_failed` at exit 100.

| Test | Semantic assessment |
|------|---------------------|
| `stage::tests::removed_stage_names_do_not_parse` | Failed at stage.rs:157 on its first new assertion: `"define"` parsed (panic `a removed stage name must not parse: Define`). |
| `stage::tests::plan_is_the_first_stage_and_validate_the_last` | Failed at stage.rs:184: `define` precedes Plan (`Define.next() == Some(Plan)`). |
| `phase7_cli::until_rejects_removed_stages` | `ship`, `Ship` and `SHIP` were rejected first; it then failed on `--until define must be rejected` (exit 0, empty stderr). |
| `help_snapshot::help_names_no_removed_surface` | The planted controls passed (both words in the 3.0.0 start line, `undefined` not reported); it then failed on `devflow --help` still naming `["Define"]`. |

Gate sequence: `test(52-17)` 4ffd6ad → `feat(52-17)!` 0efbfbb → `feat(52-17)!` 520c936. No REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode `end-of-phase`, and Task 1's verify is automated-only. Its chain exited 0 after GREEN, and expansion continued.

## Tests deleted or rewritten

**Deleted (subject was the Define stage):**
- `prompt::tests::define_prompt_never_invokes_discuss_phase`
- the Define section of `agents::tests::workflow_render_preserves_stage_contracts` (the test stays for Validate, Plan and Pi)

**Renamed:**
- `stage::tests::agent_stages_are_define_plan_code` → `agent_stages_are_plan_code`
- `agents::tests::codex_define_and_plan_require_an_existing_artifact` → `codex_plan_requires_an_existing_artifact` (its Claude control asks about Plan)
- `agent_result::tests::layer2_skips_commit_gate_for_define_and_validate` → `layer2_skips_commit_gate_for_validate`

**Core tests rewritten (Define dropped from a stage list, or Define → Plan as a stand-in):** the other `stage::tests`; `prompt::tests::each_stage_prompt_carries_its_gsd_command_and_marker`, `code_stage_prompt_is_unchanged_single_command_template`, `checkpoint_declaration_is_offered_only_to_the_code_stage`; `agents::tests::antigravity_conformance_enrollment` and `hermes_conformance_enrollment` (5 contract results, not 6); `outcome_policy::tests::every_stage` walks from Plan and `idle_timeout_is_never_auto_resumed` expects 3 stages; `mode::tests::non_gate_stages_never_gate`, `consecutive_reset_fires_on_every_other_transition`; `gates::tests::await_answer_ends_within_a_second_of_a_cancel`, `await_answer_returns_the_answer_when_not_cancelled`; `recover::tests::clean_phase_report_preserves_earlier_gate_removal_when_later_gate_removal_fails` (early gate Plan, failing gate Code), `clean_keeps_the_gate_files_of_a_run_that_started_after_the_orphan_listing`; `agent_result::tests::layer0_affirmative_success_on_non_code_stage_with_zero_commits`, `multiple_declared_probes_first_failure_vetoes_regardless_of_order` (stage Plan), `layer2_nonzero_exit_is_failed_all_stages`; `events::tests::last_events_by_phase_tracks_newest_stage_launched_ts_across_the_pass`.

**CLI unit tests deleted:** none.

**CLI unit tests rewritten:**
- commands: `start_context_probe_asks_the_driver_about_plan` (control Define → Validate), `doctor_json_reports_the_open_gates_waiter`, `doctor_json_gives_a_refused_gate_id_as_null`, the doctor reconciliation test that builds phases 27-30, `a_leftover_answer_start_could_not_delete_never_decides_the_new_run` and `start_sets_leftover_answers_aside_and_says_so` (helper `plant_leftover_define_answers` → `plant_leftover_plan_answers`), and the sidecar fixture whose `stage` was `"define"`.
- pipeline_launch: `auto_chain_eligibility_is_code_and_auto_mode_only`, `failover_target_selects_the_fallback_only_at_code`, `recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome` (three stages), `advance_refuses_when_the_saved_stage_moved_on` and `advance_commits_a_changed_notify_command_where_it_sets_it` (mismatch bound to Plan on a Code state), `a_hand_run_advance_refuses_while_a_supervisor_of_the_phase_is_live` (mismatch bound to Validate on a Plan state), `without_a_stop_the_exit_file_keeps_the_agents_code` (`--until plan`), `run_spawn_held_handoff_child` (hold `code:`), the `HandoffRun::launch_with` fixture and the gate-fixture helpers (`launch_parked_at_gate` and its Plan-gate siblings), and with them the ~49 tests built on `HandoffRun::launch`.
- preflight: `preflight_interactivity_check_flags_auto_plan_without_context_md` (not-first-stage control Define → Code), `unattended_check_does_not_apply_outside_plan_and_code` (Define dropped; the loop is now one Validate case).
- Doc-comment-only fixes: `resume_preserves_unfired_until_cap` (the cap is Plan's own stage now, not later than it) and `launch_publishes_the_socket_path_under_the_cache_dir_in_state` (`plan`, not `define`).

**52-16 behaviour migrations:** none was left. No sequence assertion or `--until define` witness remained when this plan started.

## Decisions Made

See `key-decisions`.

## Deviations from Plan

**1. [Rule 3 - Blocking] Core files outside Task 1's list**
- **Found during:** Task 1
- **Issue:** `gates.rs` (`STAGES: [Stage; 4]` and two tests), `mode.rs` (two tests) and `events.rs` (one test payload) used Define and would not build.
- **Fix:** `STAGES` is three stages; the tests use Plan.
- **Committed in:** 0efbfbb

**2. [Rule 3 - Blocking] doctor_json_e2e and stop_e2e changed in the Tasks 1+2 commit**
- **Issue:** both are Task 3 files, but each named `Stage::Define`, so `cargo build --workspace --tests` failed at the Tasks 1+2 commit without them.
- **Fix:** doctor_json_e2e opens a Plan gate; stop_e2e's `recover_clean_refuses_while_the_supervisor_is_alive` plants a Validate gate (the live run sits at Plan, so a Plan gate could collide with its own). Task 3's commit carries the rest.
- **Committed in:** 0efbfbb

**3. [Rule 1 - Bug] Tripwire counts**
- **Issue:** the core suite failed three tests whose counts encode the stage chain: the Antigravity and Hermes conformance enrollment (6 results) and `idle_timeout_is_never_auto_resumed` (4 stages).
- **Fix:** 5 results and 3 stages. These counts are tripwires by design.
- **Committed in:** 0efbfbb

**4. [Convention] Commit subjects** — `feat(52-17)!` / `test(52-17)` instead of the plan's `feat(core)!`, with `BREAKING CHANGE:` footers, as 52-16 did.

**5. [Verification] Gates run under a bounded nextest profile** — every plan gate ran as written plus `--config-file <scratch>/nextest.toml --profile bounded` (`slow-timeout = 60s × 5`), so a test waiting on a default gate timeout could not outlive the run. `scripts/check.sh all` ran unmodified.

---

**Total deviations:** 5 (2 blocking, 1 bug, 1 convention, 1 verification)
**Impact on plan:** deviations 1 and 2 widen the Tasks 1+2 commit by five files; nothing changes behaviour beyond the plan.

## Issues Encountered

- `commands::tests::status_and_doctor_name_a_held_state_write_lock` (999.221) did not fail in this plan's `scripts/check.sh all` run; no rerun was needed.
- The Tasks 1+2 commit `0efbfbb` was built and tested with Task 3's edits (comments, docs and one stub shell string inside a raw string literal) also in the working tree. Those edits change no Rust item, so the commit builds, but the commit's own tree was not built on its own.
- The worktree's `CLAUDE.md` (Dogfooding, Phase 51 conventions) still cites `define_stage_prompt` in `crates/devflow-core/src/prompt.rs`, which no longer exists. It is operator-owned and outside this plan; left as is.
- Historical comments that describe 3.x behaviour (`config.rs` capture-retention rationale, `agent_result.rs` 2026-08-08 observation and prune-history test doc, `preflight.rs` 999.63 rationale, `start_reachability_e2e.rs` header) still name Define, and are left as history.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- The stage machine is Plan → Code → Validate in code, tests, help and the scoped docs. 52-18 can build on it.
- A 3.x state or gate file naming `define` no longer parses (T-52-20, accepted).

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

- All three commits are on `feature/phase-52` (`git merge-base --is-ancestor` for 4ffd6ad, 0efbfbb, 520c936).
- Task 1's chain (build, two named stage tests, core lib suite of 1015), Task 2's chain (two named CLI tests, the help snapshot test, bin suite of 602) and Task 3's chain (failover_e2e 7, gate_wedge_e2e 5, stop_e2e 41, doctor_json_e2e 1, the named supervisor_e2e test, the named doc_check test) each exited 0; `scripts/check.sh all` exited 0 (`check.sh: all OK`), run after the last source edit and before `520c936`.
- `stage.rs` declares exactly `Plan`, `Code`, `Validate`; `rg 'Stage::Define|StageIntent::Define|define_stage_prompt' crates` finds nothing (exit 1).
- No file was created; all 27 modified files exist.

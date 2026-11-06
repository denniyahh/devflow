---
phase: 51
reviewed: 2026-11-05
reviewers: [codex gpt-6.1-sol high, deepseek-flash via pi]
dropped_lanes: [agy — quota exhausted (HTTP 429), operator replaced it with deepseek-flash]
lens: adversarial
target: 51-01..51-12 PLAN.md at 1287539, read against 51-CONTEXT.md operator rulings R-1..R-5
---

# Phase 51 — Plan Review

Every item below is actionable for `/gsd-plan-phase 51 --reviews`. Source-dependent claims were
checked against the code by the orchestrator before being listed; line numbers are approximate
anchors, not contracts. Part A applies the operator rulings recorded in `51-CONTEXT.md` (they
were made after these plans were written). Part B lists review findings.

## Part A — Operator rulings to apply (R-1..R-5, 51-CONTEXT.md)

- **A-1 (R-2) Drop 51-08 entirely.** Plan-stage continuation is out of scope. Remove the plan,
  remove `51-08` from every `depends_on` (51-10, 51-11), remove 51-11's `@`-include of
  `51-08-SUMMARY.md`, and recompute every `wave` as the plan's dependency depth. Update
  51-09's notes that say 51-08 owns `pipeline_launch.rs`.
- **A-2 (R-2) Failover is Code-stage only.** 51-01's eligibility check (fallback present, unused,
  differs from agent) must also require `stage == Code`; every other stage takes today's cron arm
  with the fallback left unused. Add a configured-fallback **Plan** control: cron instructions
  written, no `agent_handoff`, fallback not consumed. Move every handoff fixture that runs at
  Define or Plan to Code: 51-01 (~L113, Define), 51-06 (~L125, Plan), 51-10 (~L102, Define),
  51-12 (~L90, Define). For 51-06, moving to Code under Auto with a Codex fallback then fails the
  unattended preflight (`unattended_launch_shape_condition` admits only claude/antigravity), so
  pick a mode/agent pair that the preflight admits at Code.
- **A-3 (R-3) Add a `devflow.toml` key for the fallback agent,** combined with `--fallback-agent`
  the way `yes_ship` combines with `--yes-ship` (the flag wins when given). No environment
  variable. **Semver:** `DevflowConfig` (`crates/devflow-core/src/config.rs` ~L74) is public,
  has public fields and is not `#[non_exhaustive]`, so adding a field to it breaks downstream
  struct literals. Read the key through a crate-private representation or an additive accessor
  that leaves `DevflowConfig`'s shape unchanged, and remove 51-04's "no core change" semver
  exemption (~L224): run `cargo semver-checks check-release -p devflow-core`. 51-01's doc comment
  saying the fallback is "never read from the environment or `devflow.toml`" (~L122-123) must
  say what R-3 ships.
- **A-4 (R-4) 51-05 keeps only the classifier fix** (Task 1: a torn rate-limit final line is never
  reported as a clean silence). Drop Task 2 (stage-end measurement harness, the `capture_tail`
  event) and Task 3 (`51-999.71-MEASUREMENTS.md`). The production-rate measurement returns to
  backlog 999.71.
- **A-5 (R-2/R-4) 51-11's doc gate demands events whose producers are dropped.** Remove
  `plan_incomplete_on_success` (51-08) and `capture_tail` (51-05 Task 2) from
  `operations_md_documents_the_fallback_agent_and_its_events`'s required fragments (~L26, ~L100)
  and from the instructions to document them (~L118-121). Document the `devflow.toml` key (A-3)
  instead.
- **A-6 (R-2) Backlog 999.184** (filed during planning: an ordinary Plan success over
  uncommitted plan files still advances to Code) cites 51-08's predicate as its fix shape.
  Update its fix-shape line so it does not depend on a dropped plan.

## Part B — Review findings

### High
- **B-1 (51-07) The changed prompt never reaches the agent.** `launch_stage`
  (`pipeline_launch.rs` ~L2890) forwards the caller's `prompt_override` to `launch_stage_inner`,
  which renders its own prompt via `StageIntent::for_stage_in_project` (~L143), producing
  `Code { fix: None }`. Changing a default in `launch_stage` and testing that helper leaves
  audit-fix failover running plain execute-phase with green tests. Change the renderer that
  `launch_stage_inner` actually uses, and assert on the prompt a spawned stub receives. (codex)
- **B-2 (51-10) Failover makes a second liveness read the plan omits.** After saving the
  successor record, publication calls `successor_runs_on` (`pipeline_launch.rs` ~L2304), which
  probes the socket (~L2452); a probe error there returns `true` and keeps the handoff, unlike the
  pre-publication `wait_for_publish`. The plan's "the one read is `wait_for_publish`" (~L24,
  ~L186) is not exhaustive. Trace and intervene on the post-save probe separately, and derive the
  evidence file's conclusion from both outcomes. (codex)

### Medium
- **B-3 (51-04) Refusal is not mutation-free.** Fallback validation is placed after
  `commit_notify_env` (~L183), which commits a changed notify command at once
  (`pipeline_launch.rs` ~L1200). Saved `"old"`, environment `"new"`, resume with a fallback equal
  to the agent: the command becomes `"new"` before the refusal. Validate before any commit, and
  extend the refusal test to assert the saved notify command is unchanged. (codex)
- **B-4 (51-09) The "post-commit" waiter can fire before the save.** `stopped == false` plus a
  pending supervisor is true before `commit_notify_env` runs: the fixture already has
  `stopped == false` (`tests/gate_notify_persist_e2e.rs` ~L368) and `reserve_launch` persists
  `pending_supervisor` (`commands.rs` ~L973) before `resume_loaded`. Wait on a marker that is
  provably later (e.g. a freshly published launch identity). Also: the designated
  "RED-equivalent" control drives `Some("")`, not the WR-13 defect (an unset variable written as
  `None`); either run the `apply_notify_env(None)` mutation in a plan that owns
  `pipeline_launch.rs` or record the gap. (codex, deepseek)
- **B-5 (51-02) The removed-verb scan misses alternative declarations.** Variants are checked
  only for `Sequentagent` and names/aliases only for `"n"`, so `enum Command { N, #[command(name =
  "sequentagent")] Handoff }` passes. Check both forbidden spellings across variants, `name` and
  `alias`, with those two planted controls; and scan every clap subcommand enum (e.g. `GateCmd`),
  not only `Command` in `main.rs`. (codex, deepseek)
- **B-6 (51-02) Self-contradictory acceptance.** Task 1 requires a doc comment recording the
  history ("`Command::Sequentagent` (dbab01a)", "`GitFlow::rebase_in` (0833a6c)") while the
  acceptance forbids any contiguous `rebase_in`/`sequentagent` token outside `concat!` in
  `doc_check.rs`. A `///` comment cannot use `concat!`. Scope the acceptance to stripped code,
  which is what the scan reads. (deepseek)
- **B-7 (51-10) "Exactly one publish wait" is not gated.** The cited existing test asserts
  `waits >= 2` (`monitor.rs` ~L7733), a lower bound. Assert the exact count in this plan's own
  scan test. (deepseek)
- **B-8 (51-06) The named relaunch-failure seam cannot work.** Running the child without
  `DEVFLOW_CACHE_DIR` panics in the `HandoffRun` test helper (`expect("the parent passes
  DEVFLOW_CACHE_DIR")`) before the primary stage launches, and removing it mid-test needs
  `std::env::remove_var`, which the project's test rules forbid. Name a seam that fails the
  relaunch after the commit without process-global env mutation. (deepseek)
- **B-9 (51-03) Use the existing top-level selector.** The plan picks "the last event with
  `type == \"result\"`"; the source's `last_top_level_result` (`agent_result.rs` ~L1476) also
  rejects subagent results (`parent_tool_use_id`). Reuse it, and add competing parent/child and
  earlier/later result fixtures. (codex)

### Low
- **B-10 (51-01) A false truth.** "the failover path never touches `infra_failures`" (~L30)
  contradicts the same plan's refused-handoff assertion `infra_failures == 1` (~L182), which is
  correct (the cron arm bumps). Reword the truth to "a successful handoff does not bump it".
  (deepseek)
- **B-11 (51-VALIDATION.md) Unsubstituted template.** Framework, commands and the per-task map
  are still `{placeholders}` (~L22-51). Fill the per-task verification map for the revised plan
  set. (deepseek)
- **B-12 (51-03) Guard the flattened wrapper's key set.** With `#[serde(flatten)] record:
  IdleTimeoutRecord` beside the new cause keys, a future `IdleTimeoutRecord` field with a
  colliding name makes every record unparseable. Add one test pinning the wrapper's key set.
  (deepseek, suspected)

## Not adopted
- 51-12's stop rule being "subjective" (deepseek S1, speculation): left to the executor; the plan
  already records a gap instead of chasing production defects.

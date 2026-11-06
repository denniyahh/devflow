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

---

# Round 2 — review of the replanned set (2026-11-06)

Reviewers: codex gpt-6.1-sol (high), deepseek-flash via pi. Target: 51-01..51-14 at 5db7fd6. Source claims
marked (verified) were checked by the orchestrator; the rest are reviewer claims the planner must confirm.

## Orchestrator decision (binds)
- **R2-D1 — A failover relaunch that fails after the handoff commit ends at a gate, not the cron arm.** D-04
  allows either. The cron arm is wrong here: when the fallback's canary refused, `canary_gate` persisted the
  outcome (`pipeline_launch.rs` ~L574) and "a recorded `Absent`/`Unverified` refuses on EVERY later launch in
  the run" (~L532, verified), so the scheduled resume can never succeed; and on `shut_down_unrecorded`'s
  unconfirmed-shutdown error the successor "may still be running" (~L2417-2431, verified), so scheduling an
  automatic resume is unsafe. The gate (via the infra gate/abort path) is written while the guarded record still
  owns the state. A refused handoff BEFORE the commit (D-07) still takes the cron arm on the primary agent.
  Correct 51-06's doc-comment text that says a later resume "relaunches"; update 51-11's operator text and
  51-12's `a_handoff_to_antigravity_that_fails_its_canary_...` expectation accordingly.

## Findings
- **R2-1 (High, 51-06)** — covered by R2-D1 (codex 1, deepseek C-1).
- **R2-2 (Medium, 51-06)** — `handle_rate_limited_outcome` commits the counter (owned) and then calls
  `write_cron_instructions`, a plain filesystem write (`pipeline_outcomes.rs` ~L97-106, verified;
  `ship.rs` ~L94-103). A replacement between the two lets a superseded supervisor write the cron file, so 51-06's
  "no cron instructions after replacement" claim is too strong. This window is pre-existing on today's cron arm.
  Either fence the write with the existing pre-side-effect ownership check (`pipeline_launch.rs` ~L1119, "Refuse
  with `NotOwner` a supervisor the state no longer records, before a side effect") and add a control that
  replaces the record between the commit and the write, or narrow the claim and record the window as a
  pre-existing gap for a 999.x entry. Prefer the fence if it is a small change. (codex 2)
- **R2-3 (Medium, 51-05)** — `detect_claude_stream_rate_limit` selects result boundaries and the last
  `rate_limit_event` without `is_top_level` (`agent_result.rs` ~L1735-1760, verified), so a subagent
  `rate_limit_event` with `status: "rejected"` would count as a denial, contradicting 51-05's "only a parsed
  top-level `rate_limit_event`". Make the classifier filter by provenance, with competing parent/subagent event
  controls (both for the event and for the turn boundary). (codex 3)
- **R2-4 (Medium, 51-10 / 51-13)** — resume makes lock-based liveness reads the experiments never withhold:
  `held_without_socket` → `held_liveness_holder` → `liveness::probe` (`commands.rs` ~L4724, ~L4819-4846) and the
  pending-supervisor probe (~L4854-4866). Add unavailable-lock experiments with ordinary-resume controls, or
  record them as unresolved gaps; criterion 3 must not be called settled while they are unexamined. (codex 4)
- **R2-5 (Medium, 51-14)** — key-only selection is tested only under `--dry-run`, which returns before the run
  is persisted (`commands.rs` ~L402-420). Add a non-dry-run key-only test asserting the persisted fallback, with
  "clear the config-supplied fallback after the dry-run return" as its negative control. (codex 5)
- **R2-6 (Low, 51-11 / VALIDATION)** — `scripts/check.sh all` runs fmt, clippy and tests only; dependency
  scanning is the separate `deps` target (`scripts/check.sh` L20-21, L86-92, verified). Add
  `scripts/check.sh deps` to the phase gate and correct the coverage claim in 51-11 and VALIDATION. (codex 6)
- **R2-7 (Medium, 51-02)** — the attribute rule says a `name`/`alias` value "includes" a forbidden spelling; the
  real tree has `#[command(name = "__monitor", hide = true)]` (`main.rs` ~L138), which contains `n`. Specify exact
  element equality (a string value, or any element of a bracketed list, equal to `n` or `sequentagent`) and add
  `name = "__monitor"` as a planted decoy that must not be reported. (deepseek C-2)
- **R2-8 (Medium, 51-13)** — Task 1 withholds the post-save probe as an error, which takes the same branch as the
  healthy baseline (`pipeline_launch.rs` ~L2453-2467: `Err` → warn + keep the handoff), so the outcome cannot
  differ. Withhold it as `Gone` (the branch that flips the run's shape, ~L2456 → `publish_record`
  ~L2305-2309), keep the ordinary-launch control, and record the `Err` arm as established by inspection. (deepseek C-3)
- **R2-9 (Medium, 51-10)** — Task 2's `<behavior>` asserts a shared publish-wait cause in both runs, while its
  `<action>` allows "no durable trace", and its gate checks neither. Make it consistent: assert only what is
  observable and enforce it in the gate, or add the event that carries the cause on the ordinary path. (deepseek C-4)
- **R2-10 (Medium, 51-10 / 51-13)** — the criterion-3 evidence-file gates check headings and bare identifiers
  only, so a stub passes. Replace or back them with a named Rust source-scan test over the evidence file's rows
  (each traced read has a `file:line`, a test name and an outcome; both arm conclusions present), with a planted
  stub file as the negative control. (deepseek C-5)
- **R2-11 (Low, 51-14)** — the `concat!` planted control leaves a literal such as `"FALLBACK"` that the scan
  itself reports, failing the real-tree assertion. Split the name inside the word, or scope the scan's real-tree
  pass to exclude the scan's own test module, and say which. (deepseek C-6)
- **R2-12 (Low, 51-06)** — the guard-site scan uses `non_test_code_lines`, which keeps trailing `//` comments
  (`pipeline_launch.rs` ~L11952-11959, pinned ~L12056-12064), so an inline comment naming
  `FailoverRelaunchGuard::engage(` elsewhere trips it. Match the call outside trailing comments. (deepseek S-1)
- **R2-13 (Low, 51-10)** — Task 1 withholds the publish wait, after which `end_unpublished_child` kills the
  successor `__monitor` but may leave its stage agent group (`monitor.rs` ~L2260, ~L2392-2422; speculation).
  Assert no surviving agent group from that fixture with the existing reap helpers. (deepseek S-2)

Not adopted: deepseek S-3 (51-09 mutant vs 51-12; already exempted by `coupling_justified`, plans run in sequence)
and S-4 (doc-comment drift; R2-D1 rewrites the comment).

---
phase: 28-close-the-checkpoint-answer-return-path
plan: 04
subsystem: infra
tags: [rust, cargo, prompt-builder, headless-agent, tdd]

# Dependency graph
requires:
  - phase: 28-close-the-checkpoint-answer-return-path
    provides: "28-01's phase_has_blocking_human_checkpoint and phase_plan_files verify.rs helpers (dependency only — not consumed by this plan's code, wave ordering)"
provides:
  - "Stage::Define no longer builds a prompt that can invoke an interactive discuss-phase command"
  - "prompt::define_stage_prompt(phase) — the Define stage's dedicated, always-no-op headless prompt"
  - "idempotent_stage_prompt(phase) narrowed to serve only Stage::Plan, with an unchanged rendered output"
  - "A doc note on Stage::gsd_command recording that Define's mapping entry is preview/documentation-only after D-14"
affects: [28-06, "any future phase touching devflow-core/src/prompt.rs or stage.rs"]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Split a shared stage-prompt branch into a per-stage dedicated builder function when one stage's arm becomes unsafe under headless execution — delete the unsafe arm rather than gating it with a flag (D-14 precedent for any future stage-prompt divergence)."

key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/stage.rs

key-decisions:
  - "D-14 (locked upstream in 28-CONTEXT.md): the fix for Define's headless hang is deletion of the branch that invoked the discuss-phase command, not a new flag or config key disambiguating two arms."
  - "idempotent_stage_prompt's signature narrowed from (stage: Stage, phase: u32) to (phase: u32) since only Plan reaches it now; its rendered output for Plan is byte-identical to the pre-change value (same artifact literal 'PLAN.md', same command, same template)."
  - "Stage::Define.gsd_command() was left unchanged (still returns \"/gsd-discuss-phase {N}\") because it remains the canonical value for human-facing preview/documentation output (print_dry_run); only a doc comment was added noting DevFlow's own launch path no longer reads it for Define."

requirements-completed: ["999.59", "D-14"]

coverage:
  - id: D1
    description: "A headless Define stage with no CONTEXT.md completes as a no-op success instead of rendering an interactive discuss-phase command it cannot run"
    requirement: "999.59"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::define_prompt_never_invokes_discuss_phase"
        status: pass
    human_judgment: false
  - id: D2
    description: "The Plan stage's idempotency contract (checks PLAN.md, runs /gsd-plan-phase {N} when missing, no-ops when present) is unchanged after the Define split"
    requirement: "D-14"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::plan_prompt_is_idempotent"
        status: pass
    human_judgment: false
  - id: D3
    description: "Stage::Define.gsd_command()'s returned string is byte-identical to its pre-change value; only a doc comment was added"
    requirement: "D-14"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/stage.rs#stage::tests::gsd_commands_match_stage"
        status: pass
    human_judgment: false

duration: ~12min
completed: 2026-07-31
status: complete
---

# Phase 28 Plan 04: Delete Define's headless-hang branch (D-14) Summary

**Split `Stage::Define`'s prompt off the shared `idempotent_stage_prompt` branch it used to reuse with `Stage::Plan`, deleting (not flag-gating) the arm that rendered `/gsd-discuss-phase {N}` — headless Define now always no-ops with a `DEVFLOW_RESULT` success marker, while Plan's identical-looking idempotency contract is provably untouched.**

## Performance

- **Duration:** ~12 min
- **Completed:** 2026-07-31T02:29:28Z
- **Tasks:** 2 completed
- **Files modified:** 2

## Accomplishments
- `prompt::define_stage_prompt(phase)` — new private prompt builder; the rendered Define prompt never names or invokes an interactive interview command in either the artifact-exists or artifact-missing case, still forbids interactive input, still forbids modifying existing planning artifacts, and still terminates with `COMPLETION_PROTOCOL`.
- `idempotent_stage_prompt` narrowed to `(phase: u32)`, now serving only `Stage::Plan`, with byte-identical rendered output to before the split (same artifact literal, same command, same template text).
- `stage_prompt_with_project`'s dispatch replaced the shared `matches!(stage, Stage::Define | Stage::Plan)` arm with two explicit arms: `Stage::Define => define_stage_prompt(phase)`, `Stage::Plan => idempotent_stage_prompt(phase)`.
- `Stage::gsd_command`'s doc comment now records that Define's mapping entry is preview/documentation-only (`print_dry_run`) after D-14 — the code's returned string is unchanged.
- Existing `define_and_plan_prompts_are_idempotent` test split into `plan_prompt_is_idempotent` (Plan half, assertions unchanged) and `define_prompt_never_invokes_discuss_phase` (new — Define must never render the discuss-phase command). The RED run of the new test was observed to fail against the unmodified implementation before any prompt-code change landed (see below).
- `each_stage_prompt_carries_its_gsd_command_and_marker` updated to exclude `Stage::Define` from its per-stage command-presence loop, since Define no longer carries its GSD command by design; a comment cross-references the new dedicated test.

## Task Commits

Each task was committed atomically (Task 1 followed the RED/GREEN TDD cycle per its `tdd="true"` marker):

1. **Task 1 (RED): split Define/Plan idempotency test for D-14** — `838c746` (test)
2. **Task 1 (GREEN): delete Define's headless-hang branch (D-14)** — `55f47fd` (feat)
3. **Task 2: record that Define's stage-to-command mapping is no longer a launch path** — `eb9d694` (docs)

**Plan metadata:** (this commit, made after this SUMMARY.md is written)

## Files Created/Modified
- `crates/devflow-core/src/prompt.rs` — `define_stage_prompt` added; `idempotent_stage_prompt` narrowed to Plan-only; dispatch split; two tests split from one, one test's case list adjusted
- `crates/devflow-core/src/stage.rs` — doc-comment-only addition on `Stage::gsd_command`

## Decisions Made
- Followed D-14 exactly as locked in `28-CONTEXT.md`: deletion, not a new flag/config key. No `DEVFLOW_*` env var, no `devflow.toml` key, and no runtime branch was introduced to let a headless run "opt into" the interview — the operator's choice is made entirely before `devflow start` is invoked.
- Kept `Stage::Define.gsd_command()`'s returned string unchanged (still `"/gsd-discuss-phase {N}"`) rather than deleting or repointing it, because `print_dry_run` in `pipeline_gate.rs` (owned by plan 28-06 this wave, not touched here) still reads it for human-facing dry-run preview output. Added a doc note instead of a code change, per the plan's explicit instruction.
- `idempotent_stage_prompt`'s `stage` parameter was dropped entirely (not just defaulted) since Plan is now its only caller — narrowing the signature rather than leaving an unused parameter, consistent with CLAUDE.md's "no speculative flexibility" guidance.

## Deviations from Plan

**1. [Rule 1 - Bug/necessary test fix] Updated `each_stage_prompt_carries_its_gsd_command_and_marker` and a stale comment cross-reference**
- **Found during:** Task 1 GREEN phase.
- **Issue:** The plan's `<action>` for Task 1 only named the split of `define_and_plan_prompts_are_idempotent`, but `each_stage_prompt_carries_its_gsd_command_and_marker` (a separate, older test) iterates every stage asserting its prompt contains its own GSD command — including a `(Stage::Define, "/gsd-discuss-phase 11")` case that the D-14 fix necessarily breaks (Define's prompt must never contain that string). Left unfixed, this test would fail post-implementation, violating the plan's own acceptance criterion "no test outside `prompt.rs` needed editing" (which implicitly assumes `prompt.rs`'s existing test suite stays green). A second stale comment in `code_stage_prompt_is_unchanged_single_command_template` referenced the now-deleted `define_and_plan_prompts_are_idempotent` test by name.
- **Fix:** Removed the `Stage::Define` case from `each_stage_prompt_carries_its_gsd_command_and_marker`'s case list with an explanatory comment pointing to the new dedicated test; updated the stale comment's test-name cross-references to the split test names.
- **Files modified:** `crates/devflow-core/src/prompt.rs` (part of the same GREEN commit)
- **Verification:** `cargo test -p devflow-core --features test-support prompt::tests::` — 11 passed, 0 failed.
- **Committed in:** `55f47fd` (Task 1 GREEN commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — necessary to keep the pre-existing test suite green after the intended behavior change; not scope creep, directly required by the task's own `<behavior>` and acceptance criteria).
**Impact on plan:** None beyond the two touched test functions in `prompt.rs`, which the plan's file scope already covers.

## Issues Encountered

**Pre-existing gap confirmed, not fixed (per phase-specific notes and `deferred-items.md` entry #1):** `cargo test -p devflow-core <filter>` fails to compile without `--features test-support` (the two integration test binaries reference `devflow_core::test_support::*`, gated behind a feature only enabled transitively via `devflow-cli`'s dev-dependency when running `cargo test --workspace`). Every scoped test invocation in this plan used `cargo test -p devflow-core --features test-support <filter>` as the working equivalent of the plan's literally-written verify command. `scripts/check.sh all` (which runs `cargo test --workspace`) is unaffected and was confirmed fully green (419 + 2 + 2 = 423 tests passed, 0 failed) before the final commit.

## Backlog Candidate (not fixed here, per plan's explicit instruction)

**`print_dry_run` (`crates/devflow-cli/src/pipeline_gate.rs`, lines ~524-561) still previews the discuss-phase command for the Define row of a `devflow start --dry-run` pipeline listing.** After this plan, that preview over-promises: a real headless run of the Define stage will never execute that command. This is cosmetic (dry-run text only, no runtime behavior affected), it falls outside D-14's locked scope ("the fix is deletion," which says nothing about dry-run rendering), and `pipeline_gate.rs` is owned by plan 28-06 in this same wave — editing it here would create cross-plan file contention. Proposed as a follow-up: either annotate the Define row's dry-run line with a note that the command shown is illustrative/manual-only, or read a small preview-specific string that names the no-op behavior instead of the actual (never-run) slash command.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- `prompt.rs` and `stage.rs` are in a stable, fully-green state (`scripts/check.sh all` clean: fmt, clippy -D warnings, and 423 workspace tests all pass).
- No remaining caller of `gsd_command_for(Stage::Define, _)` on the prompt path — confirmed via `rg -n 'gsd-discuss-phase' crates/devflow-core/src/prompt.rs`, which matches only the one test-assertion line (`prompt.contains("/gsd-discuss-phase")` in the negative assertion).
- Plan 28-06 (`pipeline_gate.rs`, `yes_ship` config) remains free to proceed independently; this plan touched no files it owns.
- The `print_dry_run` cosmetic inconsistency documented above is a candidate for a future small follow-up plan or backlog item — not a blocker for any downstream phase-28 plan.

---
*Phase: 28-close-the-checkpoint-answer-return-path*
*Completed: 2026-07-31*

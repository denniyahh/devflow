---
phase: 28-close-the-checkpoint-answer-return-path
plan: 06
subsystem: infra
tags: [rust, devflow-core, devflow-cli, config, ship-gate, tdd]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood
    provides: "yes_ship's original per-run-only design (D-05) and the auto-answer-not-bypass audit-trail pattern (D-06) this plan reverses/reuses respectively"
provides:
  - "DevflowConfig::yes_ship: bool field + accessor, default false"
  - "config::yes_ship(project_root) -> bool resolver (env > devflow.toml > default, mirrors external_verify_enabled)"
  - "commands::start's OR-combine of --yes-ship and config::yes_ship, plus a never-silent config-sourced notice"
  - "print_dry_run's resolved ship-gate pre-authorization line"
  - "crates/devflow-cli/tests/yes_ship_config.rs — 5 CLI-boundary behavior cases"
  - "DEVFLOW_YES_SHIP documented in OPERATIONS.md"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "New devflow.toml config knob added by copying an existing resolver's exact shape (env var > file > built-in default) rather than inventing a new resolution mechanism — fourth instance of this pattern (capture_retention, review_angles, external_verify_enabled, now yes_ship)"
    - "TDD RED via unimplemented!() stub for a new pub fn, confirmed panicking against real tests, then GREEN implementation — same shape as 28-01's precedent"
    - "CLI-boundary integration test via --dry-run as a cheap, no-worktree observation point for a resolved config value (phase7_cli.rs's start_dry_run_annotates_until_stage precedent)"

key-files:
  created:
    - crates/devflow-cli/tests/yes_ship_config.rs
  modified:
    - crates/devflow-core/src/config.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - OPERATIONS.md

key-decisions:
  - "D-12/D-13 executed exactly as scoped in 28-CONTEXT.md — a deliberate, twice-confirmed reversal of Phase 23's D-05, not softened or hedged."
  - "PATTERNS.md's correction to RESEARCH.md upheld: config_file_with_yes_ship_key_loads_but_never_sets_the_flag's ASSERTION is preserved (State::new alone still never derives yes_ship from devflow.toml), not inverted. Only its name, doc comment, and assertion message change — see 'RESEARCH-vs-PATTERNS correction' below."
  - "Rule 3 auto-fix: doc_check::source_devflow_env_vars_and_subcommands_are_documented failed after Task 1 introduced DEVFLOW_YES_SHIP (every source-read DEVFLOW_* var must appear in scoped operator docs). Added a row to OPERATIONS.md's Environment variables table mirroring DEVFLOW_EXTERNAL_VERIFY_ENABLED's sibling entry."

requirements-completed: ["D-12", "D-13"]

coverage:
  - id: D1
    description: "A project can pre-authorize the Ship gate persistently via devflow.toml, with the project's standard env-over-file-over-default precedence"
    requirement: "D-12"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#config::tests::yes_ship_file_sets_true, yes_ship_file_sets_false, yes_ship_missing_file_returns_false, yes_ship_unrelated_keys_returns_default, yes_ship_defaults_to_false"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#config::tests::env_overrides_file_yes_ship, yes_ship_unparseable_env_falls_back_to_file"
        status: pass
    human_judgment: false
  - id: D2
    description: "The CLI flag still wins over config, and a config-sourced authorization announces devflow.toml as its source on stdout; a flag-sourced one does not"
    requirement: "D-12"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/yes_ship_config.rs#no_config_no_flag_is_not_preauthorized, config_true_no_flag_is_preauthorized_and_announces_source, flag_no_config_is_preauthorized_without_config_claim, flag_overrides_false_config, config_false_no_flag_is_not_preauthorized"
        status: pass
    human_judgment: false
  - id: D3
    description: "Once yes_ship is true, the Ship gate still fires and is still answered through the normal gate protocol with explicit attribution — D-13/Phase 23 D-06 unaffected"
    requirement: "D-13"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution (byte-for-byte untouched by this plan)"
        status: pass
    human_judgment: false
  - id: D4
    description: "The finalization-retry gate still refuses to auto-approve even with yes_ship set — run_gate_with_timeout never derives the auto-response from state.yes_ship"
    requirement: "D-12"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set (byte-for-byte untouched by this plan; note this test lives in pipeline_gate.rs, not pipeline_outcomes.rs — see Deviations)"
        status: pass
    human_judgment: false

# Metrics
duration: 25min
completed: 2026-07-30
status: complete
---

# Phase 28 Plan 06: Ship Approval (`yes_ship`) Config Persistence Summary

**`yes_ship` gains a `devflow.toml` config key resolved env-over-file-over-default and OR-combined with `--yes-ship` at the single existing assignment site, with a never-silent stdout notice when config alone supplied the authorization — D-12's deliberate reversal of Phase 23's D-05, with D-13 (the Ship gate still fires, still records explicit attribution) demonstrably unchanged.**

## Performance

- **Duration:** ~25 min
- **Completed:** 2026-07-30
- **Tasks:** 3 (Task 1 and Task 2 both `tdd="true"`, RED then GREEN commits; Task 3 a rename + doc-comment rewrite)
- **Files modified:** 6 (5 source files, 1 new test file, plus 1 docs fix)

## Accomplishments
- `DevflowConfig::yes_ship: bool` (default `false`, deliberately asymmetric with `external_verify_enabled`'s `true`) + `config::yes_ship(project_root) -> bool` resolver, mirroring `external_verify_enabled`'s exact env-var-then-file-then-default shape with `DEVFLOW_YES_SHIP` as the override key
- `commands::start` resolves `config::yes_ship(project_root)` and OR-combines it with the parsed `--yes-ship` flag at the crate's single `state.yes_ship` assignment site — the CLI flag always wins because the OR can only add authorization, never remove it
- A one-line `println!` notice fires on stdout naming `devflow.toml` as the source, but **only** when config supplied the authorization and the flag did not — the compensating control that keeps D-12's reversal honest (standing, but never silent)
- `print_dry_run` reports the resolved `ship gate: pre-authorized` / `ship gate: not pre-authorized` line, giving both the CLI test suite and a real operator a stable, cheap (`--dry-run`, no worktree) observation point
- `crates/devflow-cli/tests/yes_ship_config.rs` — 5 new CLI-boundary tests, invoking the real built binary via `CARGO_BIN_EXE_devflow`, covering all 5 behavior cases from the plan
- `--yes-ship`'s doc comment in `main.rs` rewritten to describe the new provenance (typed flag OR project config) and drop the sentences asserting the per-run-only guarantee D-12 reverses
- `pipeline_outcomes.rs`'s stale exclusion test renamed `state_new_alone_never_derives_yes_ship_from_config`, its doc comment and assertion message rewritten to reason from what remains true post-D-12 — **the assertion itself is unchanged**, per the RESEARCH-vs-PATTERNS correction below
- `OPERATIONS.md`'s Environment variables table gained a `DEVFLOW_YES_SHIP` row (Rule 3 auto-fix — see Deviations)

## Task Commits

Each task was committed atomically:

1. **Task 1 RED: add failing tests for yes_ship config resolver (D-12)** - `5466f18` (test)
2. **Task 1 GREEN: implement yes_ship config resolver (D-12)** - `51871e7` (feat)
3. **Task 2 RED: add failing CLI-boundary tests for yes_ship config combine (D-12)** - `f7d4445` (test)
4. **Task 2 GREEN: combine yes_ship flag with config, never silently (D-12)** - `57bcd2d` (feat)
5. **Task 3: retarget stale exclusion test's premise, not its assertion (D-12/D-13)** - `7bdeea4` (test)
6. **Deviation fix: document DEVFLOW_YES_SHIP in OPERATIONS.md (D-12)** - `bac176b` (docs)

_Tasks 1 and 2 are `tdd="true"`; RED and GREEN are separate commits per the TDD execution flow. No REFACTOR commit was needed for either. Task 3 is not TDD-tagged (rename + doc rewrite only, verified against the existing green assertion). The docs fix is a Rule 3 auto-fix surfaced by the full-suite gate after Task 1, committed separately since it wasn't anticipated by any single task's declared file set._

## Files Created/Modified
- `crates/devflow-core/src/config.rs` - `DevflowConfig::yes_ship` field/accessor/default, `config::yes_ship(project_root)` resolver, 7 new tests
- `crates/devflow-cli/src/commands.rs` - `start`'s OR-combine of the flag and config, plus the never-silent notice
- `crates/devflow-cli/src/main.rs` - `--yes-ship`'s doc comment rewritten for the new provenance
- `crates/devflow-cli/src/pipeline_gate.rs` - `print_dry_run` gained the resolved ship-gate line
- `crates/devflow-cli/src/pipeline_outcomes.rs` - stale test renamed, doc comment and message corrected; assertion unchanged
- `crates/devflow-cli/tests/yes_ship_config.rs` - new, 5 CLI-boundary behavior tests
- `OPERATIONS.md` - `DEVFLOW_YES_SHIP` row added to the Environment variables table

## Decisions Made
- **D-12/D-13 implemented exactly as scoped**, including the compensating never-silent control the plan required — not softened, not gated behind an extra flag.
- **RESEARCH-vs-PATTERNS correction upheld (see `28-PATTERNS.md` § "config.rs — D-12 yes_ship config option", final paragraph).** RESEARCH.md's "Phase Requirements" and "Validation Architecture" sections both called for flipping `config_file_with_yes_ship_key_loads_but_never_sets_the_flag`'s assertion from `!state.yes_ship` to a positive assertion. That is wrong: this specific test constructs `State::new` directly, never through `commands::start`, and `State::new` takes no project-config input at all — so its assertion (`State::new` alone never derives `yes_ship` from `devflow.toml`) remains true both before and after D-12. Only the doc comment's broader premise ("`DevflowConfig` has no field of that name...") became false once Task 1 added the field. Fix applied: renamed the test to `state_new_alone_never_derives_yes_ship_from_config`, rewrote the doc comment and assertion message to reason from the true premise and cross-reference `yes_ship_config.rs` (where the actual `commands::start`-level positive case lives), and left the `assert!(!state.yes_ship, ...)` line itself unchanged.
- **D-13 evidence recorded, not re-derived.** `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution` (`pipeline_outcomes.rs`) and `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` were left byte-for-byte untouched by every commit in this plan (confirmed via `git diff` scoping — see Deviations for the module-location note on the latter). Both remain green (`cargo test -p devflow pipeline_outcomes::tests::` → 30 passed, 0 failed; `cargo test -p devflow pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` → 1 passed). Their continued, unmodified, green status IS the D-13 evidence: once `yes_ship` is true, the Ship gate still fires exactly once, is still answered through the normal protocol with explicit attribution, and the finalization-retry gate still refuses to inherit the authorization.
- **`pipeline_gate.rs`'s Define/`print_dry_run` cosmetic gap (surfaced by sibling plan 28-04) left untouched.** 28-04 runs in parallel this wave and identified that `print_dry_run` still previews the discuss-phase command for Define after 28-04's own change removes it from the launch path. That's cosmetic, deliberately outside 28-04's locked scope, and outside this plan's declared scope too — noted here as a backlog item, not fixed. (This plan's own edit to `print_dry_run` — the new `ship gate:` line — is unrelated and does not touch the Define-command preview logic.)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `doc_check` test failed after `DEVFLOW_YES_SHIP` was introduced**
- **Found during:** Running `scripts/check.sh all` after Task 3, before the final gate
- **Issue:** `devflow-core::doc_check::source_devflow_env_vars_and_subcommands_are_documented` asserts every source-read `DEVFLOW_*` environment variable appears in scoped operator docs (`README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `OPERATIONS.md`, `docs/guides/*.md`). `DEVFLOW_YES_SHIP` (introduced by Task 1's resolver) was missing.
- **Fix:** Added a row to `OPERATIONS.md`'s Environment variables table, mirroring `DEVFLOW_EXTERNAL_VERIFY_ENABLED`'s existing sibling entry (same table, same "overrides `devflow.toml`" phrasing).
- **Files modified:** `OPERATIONS.md`
- **Verification:** `cargo test -p devflow-core --features test-support --lib doc_check::` → 6 passed, 0 failed. `scripts/check.sh all` green afterward.
- **Committed in:** `bac176b`

### Documented, not auto-fixed (out of scope)

**1. [Scope boundary — logged, not fixed] Plan's `<verify>`/acceptance-criteria commands reference the wrong package name and, for one test, the wrong module**
- **Found during:** Task 3, running the plan's literal verification commands
- **Issue:** Two mismatches between the plan text and live source, both harmless to correctness but worth recording for future readers:
  1. Every `<verify>` block and acceptance criterion in `28-06-PLAN.md` invokes `cargo test -p devflow-cli ...`. The Cargo package name for the CLI binary crate is `devflow`, not `devflow-cli` (documented project-wide false-green trap #2 in `.claude/skills/ai-change-acceptance/rules/change-acceptance.md`) — `-p devflow-cli` does not exist and errors immediately (`did not match any packages`), so this was caught at verification time, not silently skipped.
  2. `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` is referenced throughout the plan (and RESEARCH.md/PATTERNS.md) as `pipeline_outcomes::tests::...`, but the test actually lives in `pipeline_gate.rs` (`pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`). Running the plan's literal filter string (`cargo test -p devflow pipeline_outcomes::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`) matches **zero** tests and exits 0 — exactly the "matches nothing yet exits 0" trap documented in the AI Change Acceptance skill. Caught by reading the reported pass count (`0 passed; 0 failed`) rather than trusting exit status, then locating the test's real module and re-running with the corrected path (`1 passed`).
- **Why not fixed here:** Both are plan/documentation-text inaccuracies, not code defects. No source file needed to change; the test itself is correct, untouched, and green under its real module path.
- **Effect on this plan:** None on the actual gate — `scripts/check.sh all` (the phase's own single "green" definition) is unaffected and confirmed green, and the correct invocations of every named test were run and confirmed passing (see `coverage:` block above and `## Accomplishments`).
- **Recorded in:** This SUMMARY only (not `deferred-items.md`, since there is nothing to fix — it's a corrected reading, not an open item).

---

**Total deviations:** 1 auto-fixed (Rule 3, blocking doc-check gap); 1 out-of-scope discovery logged (plan-text/source mismatch, not a code defect).
**Impact on plan:** The doc fix was necessary for `scripts/check.sh all` to pass — no scope creep, directly caused by this plan's own new env var. The plan-text correction required no code change at all.

## Issues Encountered
None beyond the two items documented above.

## Known Stubs

None. Every deliverable is fully wired: the config field is read by a real resolver, the resolver is consumed by a real CLI combine site, the combine site's output is observable via a real `--dry-run` line and a real conditional notice, and all of it is covered by passing tests at both the unit and CLI-process boundary.

## User Setup Required

None — no external service configuration required. `devflow.toml`'s new `yes_ship` key is entirely operator-optional; its absence preserves today's behavior exactly (Ship gate not pre-authorized).

## Next Phase Readiness
- D-12/D-13 are fully implemented and verified; no other plan in this phase depends on this plan's artifacts (`affects: []` in frontmatter — confirmed against `28-CONTEXT.md`'s unit breakdown, which treats `yes_ship` as an independent unit).
- `scripts/check.sh all` is green at this plan's HEAD (`bac176b`): 425 `devflow-core` lib tests + 4 integration test binaries, all `devflow` (CLI) test binaries including the new `yes_ship_config.rs`'s 5 tests, `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean.
- The one open, deliberately-out-of-scope item from `phase_specific_notes` (the `print_dry_run` Define-command cosmetic preview gap 28-04 identified) remains unaddressed — noted above as a backlog item, not blocking.

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/tests/yes_ship_config.rs`
- FOUND: `pub yes_ship: bool` and `pub fn yes_ship` in `crates/devflow-core/src/config.rs`
- FOUND: `DEVFLOW_YES_SHIP` row in `OPERATIONS.md`
- FOUND commit `5466f18` (Task 1 RED)
- FOUND commit `51871e7` (Task 1 GREEN)
- FOUND commit `f7d4445` (Task 2 RED)
- FOUND commit `57bcd2d` (Task 2 GREEN)
- FOUND commit `7bdeea4` (Task 3)
- FOUND commit `bac176b` (deviation fix)

---
*Phase: 28-close-the-checkpoint-answer-return-path*
*Completed: 2026-07-30*

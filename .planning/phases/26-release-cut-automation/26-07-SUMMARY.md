---
phase: 26-release-cut-automation
plan: 07
subsystem: cli
tags: [git, release-automation, rust, cli, tdd, tracer]

# Dependency graph
requires:
  - phase: 26-release-cut-automation (26-06)
    provides: "devflow_core::release::execute_release, ReleaseReport, ReleaseOutcome, StepReport, StepStatus, ReleaseError — the executor this plan's CLI surface calls"
  - phase: 26-release-cut-automation (26-04)
    provides: "the `devflow sync` CLI surface's command shape (Check-list-then-report render loop, isolated-HOME test harness) this plan's release_execute follows"
provides:
  - "devflow release --execute --yes-release CLI surface: Command::Release { check, execute, yes_release, project }, commands::release_execute"
  - "the per-invocation --yes-release authorization contract (D-03): never settable via devflow.toml, an environment variable, or persisted State"
  - "operator documentation (CONTRIBUTING.md, OPERATIONS.md, README.md) that states truthfully which release steps are automated and which remain human"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-invocation-only authorization flag (D-03), copying --yes-ship's precedent verbatim: read ONLY in main.rs's dispatch arm from the parsed CLI flag, never persisted into State, never a devflow.toml key, never an environment variable — proven both at runtime (withholding it while a devflow.toml key and two env vars are set) and by a source-surface guard over the three files that could otherwise carry it."
    - "Blocking vs. informational pre-gate composition (D-10): self-pin and publish-order are blocking Check failures; divergence is rendered but never blocking (a diverged origin/main is the NORMAL state of an in-flight release between the human's merge and the sync step); check_signing is never called on the executor path — the tag step answers the signing question by running the real command and reading git's own result."
    - "Clean expected halt is exit 0, not a failure: ReleaseOutcome::HaltedAtHumanGate returns Ok(()) after printing the halt reason and next action, exactly like a completed run."

key-files:
  created:
    - crates/devflow-cli/tests/release_execute.rs
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/release_check.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - CONTRIBUTING.md
    - OPERATIONS.md
    - README.md

key-decisions:
  - "Tracer feedback gate (Task 1, type=tracer): auto-mode config resolved false this session (workflow._auto_chain_active false, no workflow.auto_advance key). Followed the same interpretation 26-06-SUMMARY.md (and 26-03-SUMMARY.md before it) recorded for their own tracer tasks: the plan's autonomous:true frontmatter, the complete absence of any checkpoint:* task, and the tracer's <verify> having no UI/URL surface beyond the same cargo test/clippy/fmt output already re-run to green together make a human-verify checkpoint add nothing an automated re-check hadn't already confirmed — and this executor runs as an unattended parallel worktree agent with no interactive user to answer a checkpoint in the first place. Proceeded to Tasks 2-3 without pausing; recorded here explicitly rather than silently applied, per that same precedent."
  - "Verified a genuine RED->GREEN cycle for Task 1 despite writing the implementation before the test file existed on disk: captured the full main.rs/commands.rs diff as a patch, reverted both files to their pre-plan HEAD state, confirmed the new wiring test failed for the correct reason (clap rejects the not-yet-defined --execute flag outright, not a wrong-message assertion failure), then reapplied the patch and confirmed GREEN. Committed as two separate commits (test then feat) to preserve that RED/GREEN evidence in history rather than a single squashed commit."
  - "Task 2's five tests are a single test-only commit, not a second RED/GREEN pair — they pin authorization-contract behavior Task 1's dispatch arm already correctly exhibits (mirrors 26-04-SUMMARY.md's identical treatment of its own Tasks 2/3), confirmed by running all five against the unmodified Task-1 implementation and seeing them pass on the first try."

requirements-completed: ["999.25"]

coverage:
  - id: D1
    description: "devflow release --execute --yes-release reaches the real executor: the CLI's own pre-gate composes self-pin and publish-order as blocking, divergence as informational, never calls check_signing, then calls devflow_core::release::execute_release and renders its StepReport/ReleaseOutcome (closes 26-VERIFICATION.md Truth 7)"
    requirement: "999.25"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_execute.rs#execute_reaches_the_core_executor_and_refuses_off_develop"
        status: pass
    human_judgment: false
  - id: D2
    description: "--yes-release exists, is separate from --yes-ship, and is required on every --execute invocation (closes 26-VERIFICATION.md Truth 10)"
    requirement: "999.25"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_execute.rs#execute_without_yes_release_is_rejected"
        status: pass
    human_judgment: false
  - id: D3
    description: "--yes-release cannot be supplied by devflow.toml, any environment variable, or persisted state (B10, D-03) — a run with a config key and two env vars set, and the flag absent, is still rejected"
    requirement: "999.25"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_execute.rs#yes_release_is_not_settable_via_config_or_env"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/release_execute.rs#yes_release_has_no_config_state_or_env_surface"
        status: pass
    human_judgment: false
  - id: D4
    description: "--check and --execute are mutually exclusive"
    requirement: "999.25"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_execute.rs#check_and_execute_together_are_rejected"
        status: pass
    human_judgment: false
  - id: D5
    description: "The executor's pre-gate runs self-pin/publish-order as blocking, divergence as informational, and check_signing is never called on this path (D-10) — call-count guard holds at exactly crates/devflow-core/src/git.rs:2"
    requirement: "999.25"
    verification:
      - kind: other
        ref: "rg -c 'check_signing\\(' crates/devflow-cli/src/commands.rs -> 2 (definition + release_check's single call); rg -c 'check_ssh_signing_viability' crates/ -g '*.rs' -> crates/devflow-core/src/git.rs:2"
        status: pass
    human_judgment: false
  - id: D6
    description: "The phase adds no pull-request capability: the gh subprocess call-site count across crates/ is unchanged at one file (D-02)"
    requirement: "999.25"
    verification:
      - kind: other
        ref: "rg -n 'Command::new(\"gh\")' crates/ -g '*.rs' -> exactly crates/devflow-cli/src/preflight.rs"
        status: pass
    human_judgment: false
  - id: D7
    description: "CONTRIBUTING.md's 7-step release checklist states which steps --yes-release now performs, which remain human, and which environment preconditions a real run assumes"
    requirement: "999.25"
    verification:
      - kind: other
        ref: "CONTRIBUTING.md § Cutting a Release, all 7 steps plus the crates.io paragraph annotated; doc_check::source_devflow_env_vars_and_subcommands_are_documented and the other 5 doc_check tests pass"
        status: pass
    human_judgment: false
  - id: D8
    description: "backstop truth: an operator running devflow release --execute --yes-release against a real repository with their own devflow.releaseSigningKey observes a tag that passes git tag -v"
    verification: []
    human_judgment: true
    rationale: "Requires the operator's real, non-throwaway signing key and an actual release cut — cannot be exercised in a hermetic unit test. Deferred to the real release run this plan's CLI surface now makes reachable, matching 26-05-SUMMARY.md's and 26-06-SUMMARY.md's identical treatment of the live cargo publish / live signing backstops."
  - id: D9
    description: "backstop truth: an operator observes devflow sync's push landing on origin/develop as a direct push rather than a pull request"
    verification: []
    human_judgment: true
    rationale: "Requires a real origin remote and a real release cut to observe the push land — not exercisable hermetically. Same backstop treatment as D8; 26-04's sync_main_to_develop already proves the direct-push behavior against a local bare remote in its own unit tests."

# Metrics
duration: 55min
completed: 2026-07-30
status: complete
---

# Phase 26 Plan 07: `devflow release --execute --yes-release` — the CLI surface Summary

**Replaced the hard "deferred, not-yet-built executor (DEN-50)" rejection at `main.rs`'s `Command::Release` dispatch arm with a real CLI surface — `--execute --yes-release` now reaches `devflow_core::release::execute_release` directly, with `--yes-release` a separate, per-invocation-only authorization from `--yes-ship` that cannot be set via `devflow.toml`, an environment variable, or persisted `State` — and rewrote CONTRIBUTING.md/OPERATIONS.md/README.md to state truthfully which of the 7 release-checklist steps are now automated.**

## Performance

- **Duration:** ~55 min
- **Started:** 2026-07-29T21:45:00-04:00 (approx., first commit)
- **Completed:** 2026-07-30T02:10:00Z
- **Tasks:** 3/3
- **Files modified:** 8 (1 new, 7 modified)

## Accomplishments

- `Command::Release` gained `execute: bool` and `yes_release: bool` alongside the pre-existing `check`/`project`; the variant's doc comment now describes both modes and states `--yes-release`'s per-invocation-only contract in the operator's own vocabulary (shipping ends at merge to `develop`; releasing ends at merge to `main` plus a full version release).
- `main.rs`'s dispatch arm routes all four flag combinations explicitly: `check && execute` is rejected as mutually exclusive; `check` runs the unchanged `release_check` preflight; `execute && !yes_release` is rejected naming `--yes-release`; `execute && yes_release` calls the new `release_execute`; neither flag names both modes with no "not yet built" or `DEN-50` phrasing anywhere.
- `commands::release_execute`: pre-gates on `check_self_pin` and `check_publish_order` as blocking, renders `check_divergence` as information only (never blocking — a diverged `origin/main` is the normal in-flight-release state), never calls `check_signing` (D-10 — the tag step inside `execute_release` answers that question by running the real command), then calls `devflow_core::release::execute_release` and renders every `StepReport` before branching on `ReleaseOutcome` (`Completed` or the clean, exit-0 `HaltedAtHumanGate`).
- 6 tests in the new `crates/devflow-cli/tests/release_execute.rs`, all driving the real binary: the Task 1 wiring proof (off-`develop` fixture, asserts the executor's own refusal reaches the CLI, no commit/no tag gained), plus Task 2's authorization contract — `execute_without_yes_release_is_rejected`, the B10 runtime proof `yes_release_is_not_settable_via_config_or_env` (withholds the authorization from a `devflow.toml` key AND two plausibly-named env vars, still rejected), `check_and_execute_together_are_rejected`, `bare_release_names_both_modes_and_no_deferred_executor` (asserts the OLD phrasing is ABSENT, not just that new text is present), and the supplementary source-surface guard `yes_release_has_no_config_state_or_env_surface`.
- `release_without_check_is_rejected` (pre-existing, `release_check.rs`) updated to match the new bare-invocation wording — assertion tightened to name all three flags, never loosened to a substring any message would satisfy.
- CONTRIBUTING.md's "Cutting a Release" section rewritten: all 7 numbered steps plus the crates.io paragraph annotated automated-vs-human; the protected-branch preamble now scopes the PR requirement to `main` only, with the `develop` direct-push capability stated purely as an environment precondition (no settings path, no step, no command — D-01 stays the operator's out-of-band responsibility); `Cargo.lock` sync called out as still manual; CHANGELOG generation attributed to Ship-time D-12, not this executor; a preconditions list added (signing key + non-interactive signing environment, non-interactive push credentials, cargo registry credentials).
- OPERATIONS.md's `devflow release --check` row rewritten without the deferred/`DEN-50` wording; a new `devflow release --execute --yes-release` row added. README.md's "where the automation stops" callout updated to reflect that most of the release cut is now automated and the `develop`↔`main` sync no longer needs manual repair when using the executor.
- Full re-verification: `release_execute.rs` 6/6, `release_check.rs` 10/10 (updated test included), `help_snapshot.rs` 1/1, all 6 `doc_check::` invariants pass, full workspace `--lib` 434/0 failed, `cargo test -p devflow` 0 failures across every target (confirmed via `rg -c FAILED` returning zero matches on the full run), `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both clean.

## Task Commits

Each task was committed atomically. Task 1 followed a genuine RED->GREEN cycle (`tdd="true"`, `type="tracer"`), verified live by reverting the implementation and re-running the test before reapplying it:

1. **Task 1: `devflow release --execute --yes-release` reaches the real executor** — `180d2a4` (test, RED) then `14feb15` (feat, GREEN)
   - RED: wrote `execute_reaches_the_core_executor_and_refuses_off_develop` against the pre-plan `main.rs`/`commands.rs` (reverted from the already-drafted implementation via a saved patch, to get a genuine failure rather than writing the test after the fact and trusting it). Ran it: failed correctly — clap rejected the not-yet-defined `--execute` argument outright (`error: unexpected argument '--execute' found`), proving the test exercises real CLI parsing, not a stub.
   - GREEN: reapplied the saved implementation patch (`Command::Release` fields, the four-way dispatch arm, `commands::release_execute`, snapshot regeneration, `release_check.rs`'s updated expectation). Test passed; fixed one clippy `needless_return` finding; full Task 1 `<verify>` block green (wiring test, `release_check` 10/10, `help_snapshot` 1/1, `release --help` lists both new flags, `check_signing` call count still 2, `not yet built`/`DEN-50` both absent, clippy clean).
   - **Tracer feedback gate:** auto-mode config resolved false this session; followed 26-06/26-03's own documented precedent (`autonomous:true` frontmatter, no `checkpoint:*` task, no UI/URL surface beyond the already-green automated `<verify>`, and this run has no interactive user in the first place — a parallel worktree executor) and proceeded to Task 2 without pausing — recorded here explicitly, not silently applied.
2. **Task 2: The `--yes-release` authorization contract (B10, Truths 7 and 10)** — `eacd753` (test)
   - Five tests added to `release_execute.rs`: the four runtime tests plus the supplementary source-surface guard. All five passed on the first run against Task 1's unmodified implementation — no implementation change accompanies this commit, matching 26-04-SUMMARY.md's identical "TDD Gate Compliance" treatment of tests that pin already-correct behavior rather than drive new code.
3. **Task 3: Tell the truth in the docs — the 7-step checklist, now partly automated** — `d877530` (docs)
   - CONTRIBUTING.md/OPERATIONS.md/README.md rewritten per the plan's per-step mapping. One `cargo fmt` line-wrap fixup in `release_check.rs` bundled into this commit (same file Task 1 touched; no assertion changed, formatting only). Full re-verification here: all 6 `doc_check::` tests, full workspace `--lib` 434/0 failed, `cargo test -p devflow` 0 failures, clippy/fmt clean.

_No plan-metadata commit in this worktree — STATE.md/ROADMAP.md are updated centrally by the orchestrator after all wave agents merge._

## Files Created/Modified

- `crates/devflow-cli/tests/release_execute.rs` (new) — 6 tests: the Task 1 wiring proof plus Task 2's authorization-contract suite
- `crates/devflow-cli/src/main.rs` — `Command::Release` gains `execute`/`yes_release`; four-way dispatch arm replaces the old hard rejection; `release_execute` added to the `commands::{...}` import list
- `crates/devflow-cli/src/commands.rs` — `release_execute` (pre-gate + `execute_release` call + report rendering)
- `crates/devflow-cli/tests/release_check.rs` — `release_without_check_is_rejected`'s expectation updated to the new bare-invocation wording (assertion tightened, never loosened)
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated (the `release` row's summary line changed)
- `CONTRIBUTING.md`, `OPERATIONS.md`, `README.md` — release-cut automation documented truthfully

## Decisions Made

- Verified Task 1's RED→GREEN cycle was genuine by reverting the drafted `main.rs`/`commands.rs` implementation to pre-plan `HEAD` via a saved patch, confirming the new test failed for the correct reason (clap parse rejection, not a wrong-message assertion), then reapplying the patch and confirming GREEN — see `key-decisions` in frontmatter for the full reasoning.
- Task 2's five tests committed as a single `test`-only commit with no accompanying implementation change, since they pin authorization-contract behavior Task 1's dispatch arm already correctly exhibits.
- Tracer feedback gate treated as satisfied without pausing, following 26-06's and 26-03's identical precedent under the same auto-mode-config-vs-plan-frontmatter tension, with the additional observation that this execution has no interactive user to answer a checkpoint at all (a parallel worktree agent spawned by the orchestrator).

## Deviations from Plan

None (Rules 1-4) requiring a functional change beyond the plan's own written spec. One mechanical correction caught by `cargo clippy` before the Task 1 commit (not a deviation from the plan's behavioral requirements):

1. **[Rule 1 - lint] `needless_return` in the `check && execute` mutual-exclusion arm.** The first draft of the dispatch arm used an explicit `return Err(...)` inside the initial `if` branch of an if/else-if chain whose final expression is the match arm's return value; clippy correctly flagged the `return` as unneeded since the block is already in tail position. Removed the `return` keyword, no behavior change. Fixed before the Task 1 commit; confirmed by re-running `cargo clippy --workspace --all-targets -- -D warnings` clean.

## Issues Encountered

None beyond the one clippy fixup listed above, resolved before any commit.

## User Setup Required

None — no external service configuration required. Two backstop truths (D8: a real signed tag verifying under `git tag -v` against the operator's own key; D9: `devflow sync`'s push landing as a direct push against a real `origin`) remain operator-pending, requiring an actual release cut against this project's real repository — deferred to that real run, matching 26-05-SUMMARY.md's and 26-06-SUMMARY.md's identical treatment of their own live-operation backstops.

## Next Phase Readiness

- This is the last plan of Phase 26. `devflow release --execute --yes-release` is a real, tested CLI surface reaching `devflow_core::release::execute_release`; 26-VERIFICATION.md Truths 7 and 10 are closed by this plan's `execute_reaches_the_core_executor_and_refuses_off_develop` and `execute_without_yes_release_is_rejected`/`yes_release_is_not_settable_via_config_or_env` tests respectively.
- No forward-declared symbols remain unbuilt: `commands::release_execute` is defined, wired into `main.rs`'s dispatch, and covered by 6 integration tests plus the pre-existing `release_check.rs` suite, all green.
- No blockers for phase-level verification. `crates/devflow-core/src/release.rs`, `crates/devflow-core/src/sync.rs`, and `crates/devflow-core/src/git.rs`'s publish primitives (26-04/26-05/26-06) were called, not modified — this plan's file scope matches its declared `files_modified` exactly.

---
*Phase: 26-release-cut-automation*
*Completed: 2026-07-30*

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/tests/release_execute.rs`
- FOUND: `crates/devflow-cli/src/main.rs`
- FOUND: `crates/devflow-cli/src/commands.rs`
- FOUND: `crates/devflow-cli/tests/release_check.rs`
- FOUND: `crates/devflow-cli/tests/snapshots/devflow-help.txt`
- FOUND: `CONTRIBUTING.md`
- FOUND: `OPERATIONS.md`
- FOUND: `README.md`
- FOUND: `.planning/phases/26-release-cut-automation/26-07-SUMMARY.md`
- FOUND: commit `180d2a4` (test, RED)
- FOUND: commit `14feb15` (feat, GREEN)
- FOUND: commit `eacd753` (Task 2)
- FOUND: commit `d877530` (Task 3)

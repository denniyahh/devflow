---
phase: 52-subtract-and-stabilize
plan: 21
subsystem: cli
tags: [rust, operator-output, staleness, preflight, git-remote]

requires:
  - phase: 52-18
    provides: removed-surface scan (no conflict; no old-value literals were added here)
provides:
  - "No build-provenance warning on a project that is not DevFlow's own checkout"
  - "One line, not two, when a project has no origin remote"
  - "Writer seams enforce_build_staleness_to and ensure_base_ref_current_to so operator output is assertable"
affects: [OPS-03, 999.217, devflow start preflight output]

actuals:
  tokens: 3800
  tasks: 2
  commits: 6
plan_head_before: 86f50e52193e0ec914b115431c5414734bed16a5
plan_head_after: 4755b9f7305ceb22a6f7541906f35e384c42dc34

tech-stack:
  added: []
  patterns:
    - "Operator output goes through a `&mut dyn Write` `_to` variant; the public function passes stdout, tests pass a Vec<u8>"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/staleness.rs
    - crates/devflow-cli/src/preflight.rs

key-decisions:
  - "The provenance Warn arm is silent whenever is_self_dogfood_workspace is false, for Stale, Ahead and Indeterminate alike; DevFlow's own checkout keeps every warning and the hard block"
  - "Missing remote is detected with `git remote` before the fetch; only a definite 'origin not listed' takes the one-line path, a git failure keeps the existing path"
  - "A remote that exists but cannot be fetched keeps both existing lines (different cause)"

patterns-established:
  - "Capture operator output through a writer seam instead of extracting a message function"

requirements-completed: [OPS-03]

coverage:
  - id: D1
    description: "An ordinary project gets no build-provenance warning; DevFlow's own checkout still blocks and still warns"
    requirement: OPS-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::enforce_build_staleness_is_silent_for_an_ordinary_project"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::enforce_build_staleness_blocks_self_dogfood_behind_worktree_head"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::enforce_build_staleness_still_warns_on_devflows_own_checkout"
        status: pass
    human_judgment: false
  - id: D2
    description: "A project with no remote prints exactly one line naming the missing origin remote; a configured remote that is ahead still gets its unchanged refusal"
    requirement: OPS-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::a_project_without_a_remote_prints_one_missing_remote_line"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::a_remote_that_is_behind_still_warns"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 11min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 21: Quieter output on ordinary projects Summary

**`devflow start` on an ordinary project no longer prints DevFlow's build-provenance warning, and a project with no `origin` remote gets one line instead of a fetch failure plus an undeterminable-currency warning.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-11-11T05:30:49Z
- **Completed:** 2026-11-11T05:41:26Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `enforce_build_staleness`: the `Warn` arm prints only when `is_self_dogfood_workspace(project_root)` is true. The `Block` arm (notify, `self_dogfood_stale_blocked` event, error) is untouched. The warn path never emitted an event, so no event was kept or removed.
- `ensure_base_ref_current`: when `git remote` succeeds and does not list `origin`, it prints one line, `warning: no `origin` remote is configured — skipping the check that `<base>` is current with `origin/<base>` (fail-open)`, and returns `Ok(())` without fetching.
- **Which case keeps two lines:** a remote named `origin` that exists but cannot be fetched still prints the fetch-failure line, and also the undeterminable-currency line if no local `origin/<base>` ref exists. That is a different cause (an unreachable remote), so both lines stay as the plan asked. If `git remote` itself fails, the existing path runs unchanged.

## Task Commits

1. **Task 1 (tracer): ordinary project gets no provenance warning**
   - `0b2b648` refactor: send the provenance warning through a writer (no behavior change, 30/30 staleness tests green)
   - `1f87eb3` test (RED)
   - `9b5f1f9` feat (GREEN)
2. **Task 2: missing remote produces one line**
   - `4cdadea` refactor: send the base-currency lines through a writer (no behavior change, 13/13 currency tests green; `base_ref_currency` became `#[cfg(test)]` since production now reaches it through the `_to` variant)
   - `e6c0bf5` test (RED)
   - `4755b9f` feat (GREEN)

## TDD Gate Compliance

- **Task 1 RED:** `cargo nextest run ... -E 'test(=staleness::tests::enforce_build_staleness_is_silent_for_an_ordinary_project)'` exit 100. It failed on the planned assertion (`assert_eq!(printed, "")`, left = the one provenance warning line) after the fixture's own `Staleness::Stale` precondition passed. Semantic assessment: valid RED, because the target ran and failed for the intended reason.
- **Task 2 RED:** `... -E 'test(=preflight::tests::a_project_without_a_remote_prints_one_missing_remote_line)'` exit 100, failing on `lines.len()` (left 2, right 1). The two captured lines were the fetch failure and the undeterminable warning. Semantic assessment: valid RED.
- `gsd_run check tdd-red-evidence` returned `INVALID_RED / invalid_record` for the Task 1 record. The classifier has no adapter for cargo/nextest output (the documented Rust gap #4379), so it could not judge this record either way. RED was judged by the semantic inspection above, not by the classifier.
- The controls passed before GREEN: `enforce_build_staleness_still_warns_on_devflows_own_checkout` and `a_remote_that_is_behind_still_warns`.
- Each `test(52-21)` commit has a `feat(52-21)` commit after it. Each refactor commit comes before its test commit and only adds the writer seam.

## Verification

- Tracer gate (interactive, end-of-phase, automated-only verify): re-ran Task 1's exact `<automated>` chain, exit 0.
- Task 2 `<automated>` chain, including `scripts/check.sh all`: exit 0 ("check.sh: all OK"; the devflow bin ran 606 passed, 0 failed, 4 ignored). The known flake 999.221 did not appear.
- `enforce_build_staleness_warns_for_ordinary_project_with_stale_commit` no longer exists: `rg` over `crates/` exit 1 (no match).

**What this does not establish:** the tests run in tempdir fixtures with GIT_* scrubbed. The production git calls still read the host's global git config, the same as every existing currency test, so a global `remote.origin.url` would change the outcome. No live `devflow start` on a real ordinary project was run. The silence is proven at the function level, not through the installed binary.

## Decisions Made
- The silence covers every Warn verdict on an ordinary project (Stale, Ahead, Indeterminate), not only Stale. The must-have says Warn is silent when `is_self_dogfood_workspace` is false.
- I used a writer seam rather than extracting a message function, so each test asserts the operator's actual output (empty, or exactly one line).

## Deviations from Plan

**1. [Rule 3 - Blocking] Refactor commits added before each RED commit**
- **Found during:** Tasks 1 and 2
- **Issue:** Both outputs were bare `println!` calls, so no test could observe them. A RED test against a missing seam would fail to compile, which counts as INVALID_RED.
- **Fix:** One behavior-preserving `refactor(52-21)` commit per task adds a `_to(&mut dyn Write, …)` variant. The public signatures are unchanged.
- **Committed in:** `0b2b648`, `4cdadea`

**2. [Rule 2 - Missing control] Extra capture control in Task 1**
- Added `enforce_build_staleness_still_warns_on_devflows_own_checkout`. Without it, an empty capture could come from a broken writer rather than from real silence.

**3. Commit subjects** use the phase convention `feat(52-21): …` instead of the plan's suggested `fix(cli): quieter output on ordinary projects`, matching plans 52-15 to 52-20 and the TDD gate's commit pattern.

**Total deviations:** 3 (1 blocking, 1 missing control, 1 naming). **Impact:** no scope creep. Only the two planned files changed.

## Issues Encountered
- My first edit script's doc-comment anchor did not match, and its assertion aborted before writing anything. The first GREEN test run therefore ran unchanged code and failed, as expected. I retried with the correct anchor.

## User Setup Required
None.

## Next Phase Readiness
Wave 18 is complete with 52-19, 52-20 and 52-21. Ready for the next wave.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

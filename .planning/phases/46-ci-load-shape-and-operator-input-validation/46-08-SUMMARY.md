---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 08
subsystem: testing
tags: [rust, libtest, process-isolation, env-lock, path, docker, cpu-affinity, gap-closure]

requires:
  - phase: 46-ci-load-shape-and-operator-input-validation
    provides: "46-06 and 46-07's seven child-process migrations, removal of NoGitPath, and their measured zero remaining empty-PATH windows"
provides:
  - "Removal of the 47 direct-spawner env_lock acquisitions introduced by d525f9a, with 81 pre-existing environment-mutation locks retained"
  - "Three pinned, four-worker contended workspace-suite runs with explicit 3-run/0-failure accounting"
  - "The record correction that d525f9a's direct-spawn scan did not fix the class: 31 helper-mediated git callers remained outside its 47-lock sweep"
affects: [phase-46 verification, CI load shape, future process-global test hazards]

actuals:
  tokens: 4228
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Remove a mutex only after the process-global hazard it serialized has been made impossible by a child-process boundary"
    - "Audit a historical lock sweep from its introducing diff, not from a current-tree count"
    - "Treat contended green-run counts as corroboration; the structural isolation argument carries the conclusion"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/staleness.rs

key-decisions:
  - "Removed exactly the 47 lock acquisitions identified in d525f9a's direct-spawner diff; the two imports added solely for them were also removed because clippy rejects them unused."
  - "Retained every one of the 81 remaining env_lock holders because they predate d525f9a or guard genuine environment mutation outside the eliminated empty-PATH hazard."
  - "Recorded three contended green runs as a weak bound, not a flake-elimination claim; child-process isolation is the load-bearing argument."

patterns-established:
  - "A historical additive sweep can have a different number of semantic acquisitions and changed source lines; verify both before treating a numeric plan gate as authoritative."
  - "When foreground execution truncates a long verification, preserve the same bounded instrument in a managed container and read its terminal exit code."

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "The 47 d525f9a direct-spawner lock acquisitions are absent while 81 other environment locks remain."
    requirement: INFRA-01
    verification:
      - kind: other
        ref: "d525f9a~1..d525f9a contains 47 added let _guard = env_lock() lines; 32ea1e9 working diff removed 47 such lines, left 81 total holders, and passed git diff --check"
        status: pass
      - kind: other
        ref: "comment-stripped crates/ scan: NoGitPath=0, empty-PATH-window=0; neutral_path_dir control=46"
        status: pass
    human_judgment: false
  - id: D2
    description: "The full workspace and both named historical flake cases pass under the same two-CPU pin and four competing workers."
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "three mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm scripts/check.sh test runs under taskset -c 0,1: contended_runs=3, contended_failures=0"
        status: pass
      - kind: unit
        ref: "wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks: 1 passed, 362 filtered out under contention"
        status: pass
      - kind: unit
        ref: "reachability_is_reachable_when_roadmap_entry_and_phase_dir_are_both_on_base: 1 passed, 362 filtered out under contention"
        status: pass
    human_judgment: false

duration: "20 min minimum (start timestamp was not captured; lower bound is first completed task measurement to completion)"
completed: 2026-09-06
status: complete
---

# Phase 46 Plan 08: Remove obsolete direct-spawner locks Summary

**The 47 direct-spawner locks added by d525f9a are gone after all seven empty-PATH tests moved into child processes; 81 genuine environment-mutation locks remain, and three pinned, contended workspace runs completed with 0 failures.**

## Performance

- **Duration:** at least 20 min; the executor start timestamp was not captured, so no exact duration is asserted.
- **Completed:** 2026-09-06T12:42:45Z
- **Tasks:** 2 of 2
- **Files modified:** 7 source files, 49 deleted source lines, 0 additions

## Accomplishments

- Removed all 47 `let _guard = env_lock();` acquisitions that `d525f9a` added by matching its authoritative diff test-for-test.
- Kept 81 pre-existing holders. The reconciliation is `135` original holders minus `2` removed by 46-06, `5` removed by 46-07, and `47` here.
- Re-ran the full workspace suite three times in the pinned CI image on CPUs `0,1` with four competing CPU workers: `contended_runs=3`, `contended_failures=0`.
- Exercised the named original instance and the helper-mediated preflight instance under the same contention. Each ran `1 passed` with `362 filtered out`.

## Task Commits

1. **Task 1: confirm the precondition and remove the direct-spawner locks** — `32ea1e9` (fix)
2. **Task 2: contended re-run and evidence statement** — verification-only; no source diff required a separate task commit.

## Measurements

| Measurement | Result |
| --- | --- |
| Comment-stripped `NoGitPath` references | `0` |
| Comment-stripped empty-PATH-window references | `0` |
| `neutral_path_dir` control references | `46` |
| Direct-spawner lock acquisitions in `d525f9a~1..d525f9a` | `47` |
| Direct-spawner locks deleted by this plan | `47` |
| Source additions / deletions / files in this plan | `0 / 49 / 7` |
| Remaining `env_lock()` holders | `81` |
| Contended workspace runs / failures | `3 / 0` |

The 49 deleted source lines are deliberate: d525f9a added 47 lock acquisitions *and* two imports
used only by those locks. Removing only 47 textual lines would leave those imports unused and make
`cargo clippy -- -D warnings` fail. The semantic lock subtraction is exactly 47; no pre-existing
holder was removed.

## Verification

| Check | Result |
| --- | --- |
| Precondition scan | zero empty-PATH windows; `neutral_path_dir` control remained non-zero |
| `git diff --check` | pass |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Pinned container suite, run 1 | exit 0; `scripts/check.sh: test OK` |
| Pinned container suite, runs 2–3 | `contended_run_2_exit=0`, `contended_run_3_exit=0` |
| Named staleness test under contention | exit 0; `1 passed`, `362 filtered out` |
| Named preflight helper-mediated test under contention | exit 0; `1 passed`, `362 filtered out` |
| `scripts/check.sh` / `.github/workflows/ci.yml` / test runner diff | unchanged by this plan |

The three green contended runs do **not** establish that the historic flake is eliminated. The
unfixed record was roughly one failure in eight; a small run count is consistent with zero observed
failures even if nothing changed. The load-bearing argument is mechanical: the empty `PATH` now
lives only in a child process, so it cannot overlap a sibling test in the parent process. The old
mutex only made that overlap improbable by serialization.

## Correction to the Historical Record

`d525f9a`'s pushed commit message says it was "Fixed as a class, not as this instance." That is
false as written. Its scan found direct git spawns only, so 31 tests reaching git through one of 31
spawning helpers were outside its 47-lock sweep; the reviewed trace was `preflight.rs` through
`reachability_fixture`. The commit is not amended. This summary is the correction of record.

## Phase 46 Review-Disposition Roll-up

| Finding | Disposition / closing plan |
| --- | --- |
| C-01 | Child-process migration in 46-06 and 46-07; obsolete direct-spawner locks removed here in 46-08 |
| C-02 | Closed by 46-04's job-scoped, fixture-driven CI parity guards |
| C-03 | Closed by 46-04's asserted CPU-pin decider and negative cases |
| C-04 | Closed by 46-05's containment-based plan-bashism scanner and hook delegation |
| C-06 | Closed by 46-04's single `DEVFLOW_CI_CPUS=all` decision site |
| C-05 | Deferred with GitHub #207; canonical git start-point work is required |
| C-07 | Deferred with GitHub #207; the silent wrong-fork case remains the same canonical-start-point problem |

`cargo nextest` remains backlog, not a phase-46 change: it measured as a wall-clock wash, the
cross-test `suite_reap_audit` is structurally incompatible with isolated processes, and six other
leaky tests remain untraced.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Plan-verification bug] Reconciled semantic lock count with source-line count**

- **Found during:** Task 1 verification.
- **Issue:** The plan says d525f9a has 47 added source lines, but its per-file counts and direct diff show 47 lock acquisitions plus two imports: 49 source lines. The literal `removed=47` numstat gate therefore fails for the correct, clippy-clean change.
- **Fix:** Verified the semantic subtraction independently (`47` original direct-spawner locks, `47` removed locks), removed the two now-unused imports, and recorded both counts rather than retaining dead imports to force a false metric.
- **Files modified:** the seven planned Rust files only.
- **Verification:** `0 additions / 49 deletions / 7 files`, `81` remaining holders, clippy exit 0.
- **Committed in:** `32ea1e9`.

**2. [Rule 1 - Verification bug] Replaced an invalid precondition probe before acting**

- **Found during:** Task 1 precondition check.
- **Issue:** The first comment filter had invalid `awk` syntax and allowed empty match counts to bypass numeric assertions, so its printed pass line was not evidence.
- **Fix:** Discarded that result and re-ran a fail-closed, comment-stripped `rg` probe with an independent `neutral_path_dir` control before editing.
- **Files modified:** none.
- **Verification:** zero `NoGitPath` and empty-PATH-window references, while the control returned `46`.
- **Committed in:** n/a — measurement correction only.

**3. [Rule 3 - Verification infrastructure] Preserved the container instrument after foreground execution was truncated**

- **Found during:** Task 2.
- **Issue:** The foreground container command stopped before emitting a terminal result; a detached child was terminated by the sandbox, and the host had no user D-Bus session for a bounded user service.
- **Fix:** Ran named, bounded Docker containers with the same image, worktree mounts, `taskset -c 0,1` pin, four PID-scoped workers, and `scripts/check.sh test`; read terminal container exit codes and then removed only those two stopped containers.
- **Files modified:** none.
- **Verification:** all three suite exits were 0; both exact named tests passed under the same load.
- **Committed in:** n/a — verification infrastructure only.

**4. [Rule 1 - State-record bug] Corrected stale phase status after the state advance reported success without persisting it**

- **Found during:** Final close-out verification.
- **Issue:** `state.advance-plan` reported `last_plan` and `ready_for_verification` but left `STATE.md` claiming 46-07 was next and the milestone roadmap's phase table saying 46 was not started.
- **Fix:** Updated the frontmatter activity/status, Current Position, and roadmap phase row to reflect 46-08 and its 8/8 completed-plan state.
- **Files modified:** `.planning/STATE.md`, `.planning/ROADMAP.md`.
- **Verification:** the final state names 46-08's measured locks and contention result; the roadmap reads "Plans complete; verification pending."
- **Committed in:** final metadata correction commit.

**Total deviations:** 4 auto-fixed (3 Rule 1 measurement/record corrections, 1 Rule 3 infrastructure recovery).
**Impact on plan:** No scope expansion or product-code change. The corrections prevent a false precondition pass, a misleading 47-line report, and stale phase-completion records.

## Known Stubs

None. The only source change is deletion, and the committed diff adds zero placeholder, TODO, FIXME, empty-rendered-data, or mock-data lines.

## Issues Encountered

None blocking.

## User Setup Required

None — no external service configuration is required.

## Next Phase Readiness

Phase 46's in-scope C-01, C-02, C-03, C-04, and C-06 dispositions are recorded closed. C-05 and
C-07 remain deferred under GitHub #207; `cargo nextest` remains backlog. The contention evidence
corroborates the structural C-01 fix but is not a reliability-rate proof.

## Self-Check: PASSED

- All seven planned source files and this SUMMARY exist.
- Task commit `32ea1e9` and the initial metadata commit `9559675` resolve in `git log --all`.
- `git diff --check` exited 0, and coverage classification accepted both automated deliverables.

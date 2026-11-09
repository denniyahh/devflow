---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 01
subsystem: infra
tags: [github-actions, ci, taskset, cpu-affinity, devcontainer, regression-guards, bash]

requires:
  - phase: 23-container-parity
    provides: "scripts/check.sh, scripts/check-in-container.sh, the pinned devcontainer image, and the ci_parity_guards.rs guard file this plan extends"
provides:
  - "An advisory `Sequential 2-CPU check` job in ci.yml that runs `scripts/check.sh all` under `taskset` inside the pinned devcontainer image"
  - "scripts/lib/ci-cpus.sh — the single definition site for the CI CPU pin, sourced by both the local pre-push gate and CI"
  - "Three in-tree guards pinning the job's shape, the pin's single-definition-site invariant, and the job's advisory status"
  - "Empirical evidence that the 2-CPU load shape converts a previously-acknowledged intermittent flake (PATH/ENV_MUTEX race) into a deterministic 2/2 failure"
affects: [46-02, 46-03, ci, release, debugging-999.47]

actuals:
  tokens: 5128
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Shared shell fragment sourced (never parsed) by both a local gate and a CI workflow"
    - "Negative-control print step inside a CI job: report the measurement AND the case that must differ"

key-files:
  created:
    - scripts/lib/ci-cpus.sh
  modified:
    - .github/workflows/ci.yml
    - scripts/check-in-container.sh
    - crates/devflow-cli/tests/ci_parity_guards.rs
    - .planning/user/DEV-SETUP-CHECKLIST.md

key-decisions:
  - "ADD-ALONGSIDE, not promote: the new job is a fourth ci.yml job, and devcontainer.yml's required sequential job is untouched (D-01)."
  - "The job is advisory purely by being outside the four required status contexts; continue-on-error was rejected because it reports SUCCESS on a real suite failure (D-05)."
  - "The CPU list is SOURCED from scripts/lib/ci-cpus.sh, never sed-extracted — a `sed -n ...p` exits 0 on no match and would run the suite unpinned while the log claimed success (D-03)."
  - "The D-04 print step emits the unpinned core count as the pinned count's negative control, because a partly-invalid CPU list narrows silently and still exits 0."
  - "The staleness test failure found during plan-level verification was DEFERRED, not fixed: it pre-dates this phase and lives in a different test binary (see deferred-items.md D-46-01-A)."

patterns-established:
  - "Single-definition-site enforcement by test, not by comment: cpu_pin_has_exactly_one_definition_site mirrors what scripts/assert-image-parity.sh does for the image tag."
  - "Comment-stripping before counting occurrences in a guard, with a negative-control demonstration proving the guard measures code and not prose."

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "ci.yml defines a fourth job running scripts/check.sh all under a taskset CPU pin inside the pinned devcontainer image"
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#ci_workflow_runs_the_sequential_check_under_a_cpu_pin"
        status: pass
    human_judgment: false
  - id: D2
    description: "The CPU-list value has exactly one definition site, sourced by both the local gate and CI"
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#cpu_pin_has_exactly_one_definition_site"
        status: pass
      - kind: integration
        ref: "bash -c '. scripts/lib/ci-cpus.sh; echo $CPUS' with and without DEVFLOW_CI_CPUS override"
        status: pass
    human_judgment: false
  - id: D3
    description: "The new job is advisory and carries no success-masking error flag"
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/ci_parity_guards.rs#sequential_job_name_is_not_a_required_status_check"
        status: pass
    human_judgment: false
  - id: D4
    description: "The new job actually ran and reported on this phase's PR at its current HEAD_SHA, read from an UNFILTERED gh pr checks listing (D-06)"
    verification: []
    human_judgment: true
    rationale: "PENDING-AT-PR. Nothing cargo test can assert proves a GitHub job ran. The branch is unpushed and no PR exists yet; `gh pr list --head feature/phase-46` returned `[]`. This must be read at phase close from an unfiltered `gh pr checks <PR>`, never `--required` (which would omit the very job being accepted) and never `gh run list`."

duration: 26min
completed: 2026-09-05
status: complete
---

# Phase 46 Plan 01: CI Load Shape Summary

**An advisory `Sequential 2-CPU check` CI job that runs the whole suite under a `taskset` pin from a single shared definition site — and which, on its first local exercise, deterministically surfaced a latent defect the repo's normal 4-vCPU CI cannot see.**

## Performance

- **Duration:** ~26 min
- **Started:** 2026-09-05T11:13Z (approx.)
- **Completed:** 2026-09-05T11:39Z
- **Tasks:** 3 of 3
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments

- `.github/workflows/ci.yml` now defines **four** jobs. The fourth, `Sequential 2-CPU check`, runs `scripts/check.sh all` wrapped in `taskset` inside the same pinned devcontainer image as the other three — the sequential-under-CPU-pressure load shape the local pre-push gate has always had and CI never did.
- `scripts/lib/ci-cpus.sh` is the one place the CPU list is defined. Both `scripts/check-in-container.sh` and the new CI job **source** it. The gate's own header still prints `==> cpus:   0,1` after the relocation, so the move is behaviour-preserving.
- Three new guards in `crates/devflow-cli/tests/ci_parity_guards.rs` (7 → 10 tests), each with its failing direction observed rather than assumed.
- **The pin works, and it already caught something.** See "Issues Encountered" — this is the most consequential result of the plan and it is not a clean green.

## Task Commits

1. **Task 1 (tracer): one CPU-pin definition site, wired from the local gate through to a new CI job** — `a67fc3f` (feat)
2. **Task 2: guard the CPU pin against a second definition site** — `0da596c` (test)
3. **Task 3: pin the sequential job as advisory, both directions** — `b8edd9a` (test)

## Files Created/Modified

- `scripts/lib/ci-cpus.sh` *(created)* — single definition site for the CPU pin; sourced, not executed, so it deliberately omits `set -euo pipefail`, mode `644`.
- `.github/workflows/ci.yml` — new `sequential` job; header comment corrected to state four required contexts on both trunks and the dual `gh api` verification form.
- `scripts/check-in-container.sh` — dot-sources the fragment; stale "runners are 2-core" reason corrected (this is a public repo, so runners are 4 vCPU — the value was right, the reason was not).
- `crates/devflow-cli/tests/ci_parity_guards.rs` — three guards, two constants, and a hoisted `code_lines` helper.
- `.planning/user/DEV-SETUP-CHECKLIST.md` — §4 now describes four `ci.yml` jobs and records that the fourth *required* context lives in `devcontainer.yml`; moved in the same commit as the workflow and script edits.

## Verification — every claim below was run in-session

### Task 1 — the RED/GREEN pair (verbatim)

RED, before any YAML or script change:

```
test ci_workflow_runs_the_sequential_check_under_a_cpu_pin ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.00s
```

GREEN, after:

```
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
cargo_exit=0
```

The `7 filtered out` / `1 failed` split confirms the filter matched a real test — a name matching nothing would have reported `0 failed` and exited 0.

Fragment behaviour and its negative control:

```
default_CPUS=0,1
override_CPUS=all
source_exit=0
syntax_exit=0
```

Pin narrowing, with its own negative control:

```
unpinned_nproc=4
pinned_nproc=2
pin_exit=0
```

### Task 2 — three demonstrations (verbatim)

1. Literal re-typed into `ci.yml`:
```
Found: ["retyped_pin: \"0,1\""]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.00s
```
2. Assignment re-added to `scripts/check-in-container.sh`:
```
Found: ["CPUS=\"${DEVFLOW_CI_CPUS:-0,1}\""]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.00s
```
3. **Negative control** — the literal in a full-line YAML COMMENT only:
```
comment_line_present=1
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.00s
```
`comment_line_present=1` is the control on the control: it proves the comment actually landed in the file before the test ran, so the `ok.` is not a vacuous pass over an unmodified tree.

Post-commit: `test result: ok. 9 passed; 0 failed; ...`, `cargo_exit=0`, `scoped_dirty_lines=0`.

### Task 3 — two demonstrations (verbatim)

4. Job renamed to a required context:
```
renamed_lines=2
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s
```
`renamed_lines=2` shows the hijack actually materialised — two jobs claiming the `Test` context — before the guard fired.

5. Success-masking error flag added:
```
coe_lines=1
Found: ["continue-on-error: true"]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s
```

Post-commit: `test result: ok. 10 passed; 0 failed; ...`, `cargo_exit=0`, `scoped_dirty_lines=0`.

### Plan-level verification

| # | Check | Result |
|---|---|---|
| 1 | `cargo test -p devflow --test ci_parity_guards` | **PASS** — `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| 2 | `cargo test --workspace --no-fail-fast` (host) | **PASS** — `workspace_exit=0`, zero `test result: FAILED` lines |
| 3 | `scripts/check-in-container.sh all` | **FAIL** — `container_gate_exit=101`. See Issues Encountered. Pre-existing, deferred, NOT caused by this plan. |
| 4 | D-06 — unfiltered `gh pr checks <PR>` on this phase's PR | **PENDING-AT-PR.** Not run, not claimed. Branch unpushed; `gh pr list --head feature/phase-46` → `[]`. |
| 5 | `Test`/`Clippy`/`Format` still present and passing on the PR | **PENDING-AT-PR** (same reason). Their continued *existence* in `ci.yml` is guarded in-tree and verified: `name_Test=1`, `name_Clippy=1`, `name_Format=1`. |

Structural checks, all run: `ci.yml` parses under `yq` (4 jobs, `sequential` name correct, 6 steps) with a negative control confirming `yq` exits 1 on deliberately broken YAML; `continue-on-error` count 0; no `- run: cargo ` lines; only `actions/checkout@v7` as an action; `git ls-files -- scripts/lib/ci-cpus.sh` returns the path; fragment mode `644` with zero `set -` lines; `.github/workflows/devcontainer.yml` and `CLAUDE.md` absent from every commit; zero file deletions in any commit.

## Decisions Made

Followed the plan's D-01…D-06 as specified. Two judgment calls inside the plan's discretion:

- **Hoisted `code_lines` to module level** rather than duplicating the comment-stripping filter chain across two guards, and pointed Task 1's guard at `SEQUENTIAL_JOB_NAME` instead of a second copy of the literal — so a rename cannot be applied to one guard and missed in the other.
- **Ran every verification through `bash -c`.** The agent harness shell is **zsh**, not bash, where `${PIPESTATUS[0]}` silently expands to nothing. Measured directly: `pipestatus_test=UNSET`. The plan's `<verify>` blocks are written for bash and would have reported an empty exit code — i.e. no exit code at all — if run as-is.

## Deviations from Plan

None affecting the plan's own scope — all three tasks executed as written. One out-of-scope discovery was deferred rather than fixed; see below.

## Issues Encountered

### The 2-CPU pin deterministically fails a pre-existing test — deferred, not fixed

Plan-level verification step 3 failed:

```
thread 'staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks' (7415)
panicked at crates/devflow-cli/src/staleness.rs:1436:22:
called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "No such file or directory" }
test result: FAILED. 362 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.08s
```

`staleness.rs:1436` is the `.unwrap()` on `.output()` in the test's `git` closure — a process **spawn** failed with ENOENT. It is not an assertion about staleness.

**Single-variable measurement.** Same host, same image, same commit, same tree; only `DEVFLOW_CI_CPUS` changed:

| `==> cpus:` | `wr01_...` | Gate exit |
|---|---|---|
| `0,1` (`check-in-container.sh all`) | FAILED | `container_gate_exit=101` |
| `0,1` (`check-in-container.sh test`) | FAILED | `container_test_exit=101` |
| `all` | ok | `allcpu_exit=0` |
| `all` | ok | `allcpu2_exit=0` |

Host, unpinned, outside the container: `363 passed; 0 failed` in that binary.

**What this establishes:** the CPU pin is the variable that flips this test — 2/2 fail pinned, 2/2 pass unpinned. It is deterministic, not a flake.

**Root cause — CONFIRMED, and already on record as unfixed.** `crates/devflow-cli/src/test_support.rs:361` documents an RAII guard that "REPLACES `PATH` with a deliberately EMPTY directory, so `git` — and every other binary — cannot be resolved at all", noting that "`Command::output()` returns `Err(NotFound)` when the program cannot be spawned". `NeutralPath::install` (`:336`) mutates `PATH` process-globally for the whole `--bin devflow` test binary. `wr01_...` spawns `git` **without** holding `ENV_MUTEX`, so it can land inside that window.

This exact mechanism was diagnosed on 2026-09-02 in `.planning/milestones/v2.4.0-phases/34-.../deferred-items.md` §2 with a "**Plausible fix, not applied**", and STATE.md's deferred table carries it as "pre-existing `PATH`/`ENV_MUTEX` race, fix identified not applied". The victim test differs — phase 34 saw `embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir` — which is what a process-global race hitting whichever unguarded spawn overlaps the window looks like.

**What 46-01 contributes that was not previously known:** phase 34 classified this as a **flake**. Under the 2-CPU pin it is **deterministic** — 2/2, not intermittent. The pin narrows the scheduler enough that the overlap happens every time. That is an unplanned but direct demonstration of this phase's premise: the load shape converts an acknowledged-and-shelved intermittent defect into one that fails reliably and can therefore be fixed and regression-tested.

**What this still does NOT establish:**
- The mechanism is inferred from the guard's documented behaviour plus an exactly-matching panic signature — not from instrumenting the two threads overlapping. Nobody captured the interleaving.
- n = 4 runs (2 pinned, 2 unpinned). Enough to act on; far short of a reliability characterisation.
- **Nothing here predicts the GitHub runner.** A 2-vCPU hosted runner is a different machine under a different scheduler. That the pin makes this deterministic *here* does not establish it will be deterministic *there*.

**Why it was deferred rather than fixed.** It is not caused by this plan. The `0,1` pin pre-dates phase 46 (it was `check-in-container.sh:89` before 46-01 relocated it) and still resolves to `0,1`; this plan's only Rust change is in `tests/ci_parity_guards.rs`, a *different* test binary from the failing `--bin devflow` one, as the log's own `to rerun pass '-p devflow --bin devflow'` confirms. Fixing it means finding a spawn-time root cause or reworking process-global env mutation in a file outside this plan's `files_modified` — plausibly deviation Rule 4 territory.

Recorded in `.planning/phases/46-ci-load-shape-and-operator-input-validation/deferred-items.md` (D-46-01-A) and in `.planning/WINDOWS.md`.

**Consequence for this phase:** the new `Sequential 2-CPU check` job will very likely be **red on its first CI run**, for this reason. That is an argument *for* D-05's advisory posture, not against the job — but it means D-06's acceptance reading needs an operator decision about what "accepted" means when the job is red for a defect it correctly found.

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

- 46-02 and 46-03 share wave 1 with no `depends_on` on this plan and are unblocked. Neither had committed its deliberate RED test at the time this plan's workspace suite ran, so that suite's green is attributable to this plan alone.
- **Blocker for pushing:** `git push` from this worktree runs `scripts/check-in-container.sh all` via `scripts/hooks/pre-push:220`, which currently exits 101 for D-46-01-A. The phase cannot be pushed without either resolving that defect or a deliberate operator decision.
- **Open, needs an operator answer:** whether to fix D-46-01-A before opening this phase's PR, or open it with the advisory job knowingly red and the reason recorded. Relevant to the choice: this is not a new defect and its fix was already identified in phase 34 ("have the outer half acquire `ENV_MUTEX`"), just never applied — so the work is scoped and known, not exploratory.

---
*Phase: 46-ci-load-shape-and-operator-input-validation*
*Completed: 2026-09-05*

## Self-Check: PASSED

All five plan artifacts plus the SUMMARY and deferred-items.md exist on disk; all three task
commits (`a67fc3f`, `0da596c`, `b8edd9a`) are present in `git log --oneline --all`.

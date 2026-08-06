---
phase: 25-end-to-end-dogfood-blockers
plan: 11
subsystem: testing
tags: [rust, proc-fs, fork-exec-race, test-support, 999.47]

requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: 25-08/25-09's stray-process discovery and reap-strays machinery (25-02, 25-07); 25-10's reproduction record (25-CI-OBSERVATION.md)
provides:
  - "A committed, fresh site census (25-SITE-CENSUS.md) of every `.spawn()` site in `crates/` classified against the 999.47 cmdline-inheritance race"
  - "`devflow_core::test_support::wait_for_exec_visibility` — a bounded, documented barrier a test must cross before reading a `/proc`-cmdline census, plus `EXEC_VISIBILITY_WAIT`/`EXEC_VISIBILITY_POLL` constants"
  - "All 4 VULNERABLE-POSITIVE and 2 VACUOUS-NEGATIVE census sites barriered; the observed 2/2 pre-push failure fixed"
affects: [25-12 (production false-positive mitigation), 25-13 (the actual `git push origin feature/phase-25`)]

tech-stack:
  added: []
  patterns:
    - "Exec-visibility barrier: poll /proc/<pid>/cmdline until argv[0]'s basename matches AND the cmdline differs from the caller's own /proc/self/cmdline, bounded by a wait/poll pair"
    - "Test-only helper surface lives in devflow-core::test_support (feature-gated `#[cfg(any(test, feature = \"test-support\"))]`), never in a `pub mod` of a published crate, when the surface has no production consumer"

key-files:
  created:
    - .planning/phases/25-end-to-end-dogfood-blockers/25-SITE-CENSUS.md
  modified:
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-core/src/agent.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/reap_strays_e2e.rs

key-decisions:
  - "Barrier lives in test_support.rs (not pub mod agent) — per this plan's review_disposition, confirmed no Cargo.toml change was needed"
  - "check-in-container.sh cannot run cleanly from inside a linked git worktree (the .git file points to a host path the container never mounts) — worked around for verification purposes only, via an equivalent docker invocation with an added bind mount; no repo file was changed for this"

patterns-established:
  - "Exec-visibility barrier pattern for any future test that spawns then reads a /proc cmdline census"

requirements-completed:
  - "25e (999.47 / DEN-72) — truth 7 closed by construction: every spawn-then-cmdline-census site in crates/ now crosses a bounded exec-visibility barrier before reading the census"
  - "Operational — the identified test-side cause of the pre-push rejection at commands.rs:3678 is fixed; the push itself is plan 25-13's job"

coverage:
  - id: D1
    description: "Fresh site census at execution HEAD, classifying every .spawn() site in crates/ as VULNERABLE-POSITIVE / VACUOUS-NEGATIVE / NOT-VULNERABLE with a stated reason"
    verification:
      - kind: other
        ref: ".planning/phases/25-end-to-end-dogfood-blockers/25-SITE-CENSUS.md (committed artifact, 4 required sections present)"
        status: pass
    human_judgment: false
  - id: D2
    description: "wait_for_exec_visibility barrier primitive with 4 behaviour tests (positive, bounded timeout, dead pid, self-argv guard)"
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core --lib test_support::tests::wait_for_exec_visibility (4 passed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The observed 2/2 pre-push failure (gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling) barriered and passing"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling -- --exact (1 passed)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every remaining census site (agent.rs x3, commands.rs doctor test, reap_strays_e2e.rs helper) barriered"
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core --lib agent::tests::discover_stray (4 passed); cargo test -p devflow --bin devflow commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs -- --exact (1 passed); cargo test -p devflow --test reap_strays_e2e (2 passed)"
        status: pass
    human_judgment: false
  - id: D5
    description: "scripts/check-in-container.sh all — the pre-push hook's own command — passes three consecutive times under the loaded fmt+clippy+test shape"
    verification:
      - kind: other
        ref: "6 total container runs (3 for Task 1, 3 for Task 2), each 'check.sh: all OK', 0 FAILED lines — run via an equivalent docker invocation (same image, same taskset -c 0,1 pinning, same fmt+clippy+test ordering) with an added bind mount for the linked worktree's git-common-dir, since the literal script cannot resolve .git from inside a container that only mounts the worktree directory"
        status: pass
    human_judgment: true
    rationale: "The literal, unmodified scripts/check-in-container.sh all command cannot be run cleanly from this worktree (structural git-worktree/container-mount gap, unrelated to source). A human/orchestrator should re-run the literal command on the actual feature/phase-25 checkout (as plan 25-13 does) to close this out formally, even though the equivalent verification here is genuine and load-sensitive."

duration: ~35min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 11: 999.47 Exec-Visibility Barrier Summary

**Closed the 999.47 defect class (not just its two known instances) by adding a bounded `wait_for_exec_visibility` barrier in `devflow-core::test_support` and applying it to every spawn-then-cmdline-census site a fresh census found — 4 VULNERABLE-POSITIVE and 2 VACUOUS-NEGATIVE sites, including the exact test that failed the `pre-push` gate 2/2 times.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-07-28T13:30Z (approx.)
- **Completed:** 2026-07-28T14:05:41Z
- **Tasks:** 2 (5 commits total, including the TDD RED/GREEN split)
- **Files modified:** 5 (1 new: 25-SITE-CENSUS.md; 4 modified: test_support.rs, agent.rs, commands.rs, reap_strays_e2e.rs)

## Accomplishments

- Measured a fresh site census at execution HEAD (`5244fac`): 4 `VULNERABLE-POSITIVE`, 2 `VACUOUS-NEGATIVE`, 25 `NOT-VULNERABLE` — every one of the 17 `.spawn()` sites in `crates/` classified with a stated reason.
- Diverged from `25-CI-OBSERVATION.md`'s stale five-row list in exactly the three ways the plan predicted: line numbers shifted, one entry (`gate_sweep_without_reap_strays_flag_ignores_a_live_stray`) was not actually vulnerable (no census read on that path), and two genuinely vulnerable sites outside its two-file scope were missing (the `doctor` stray-finding test and `reap_strays_e2e.rs`).
- Implemented `wait_for_exec_visibility(pid, expected_argv0_basename, wait, poll) -> bool` in `devflow-core::test_support` (not `pub mod agent` — feature-gated off in every normal build), with `EXEC_VISIBILITY_WAIT` (10s) and `EXEC_VISIBILITY_POLL` (2ms) constants, following the RED-then-GREEN TDD gate: 4 tests committed failing to compile first, then made to pass.
- Widened `agent::argv_basename` from private to `pub(crate)` so the barrier reuses the exact basename idiom `classify_stray_layer` uses.
- Barriered the exact test that panicked at `commands.rs:3678` in both recorded push attempts (`gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`), plus every other census site: `agent.rs`'s three census tests (one positive, two vacuous-negative, with an explanatory comment on the negatives), `commands.rs`'s `doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`, and `reap_strays_e2e.rs`'s `spawn_monitor_wrapper_fixture` helper (barriered once, inherited by both its callers).
- Verified under the loaded shape that produced the original 2/2 failure — `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace --no-fail-fast` under `taskset -c 0,1` in the pinned `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` image — six times total (three per task), zero failures across all six.

## Task Commits

1. **Task 1 Step 1: Site census** — `7ed9926` (docs)
2. **Task 1 Step 2, RED: failing barrier tests** — `67c684f` (test)
3. **Task 1 Step 2, GREEN: barrier implementation** — `9439b60` (feat)
4. **Task 1 Step 3: barrier the observed-failing site** — `f50515b` (fix)
5. **Task 2: barrier the remaining census sites** — `960f46d` (fix)

_TDD gate (Task 1 Step 2): `test(25-11)` commit `67c684f` precedes `feat(25-11)` commit `9439b60` — RED then GREEN, per this plan's `<review_disposition>` mandate not to skip or reorder the RED commit._

## Files Created/Modified

- `.planning/phases/25-end-to-end-dogfood-blockers/25-SITE-CENSUS.md` — the fresh, committed census (created)
- `crates/devflow-core/src/test_support.rs` — `wait_for_exec_visibility`, `EXEC_VISIBILITY_WAIT`, `EXEC_VISIBILITY_POLL`, 4 behaviour tests, module doc widened to cover both 999.37 (hermetic git) and 999.47 (exec-visibility) scopes
- `crates/devflow-core/src/agent.rs` — `argv_basename` widened to `pub(crate)`; barrier applied to 3 census tests
- `crates/devflow-cli/src/commands.rs` — barrier applied to 2 census tests (`gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`, `doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`)
- `crates/devflow-cli/tests/reap_strays_e2e.rs` — barrier moved inside `spawn_monitor_wrapper_fixture`, inherited by both its callers

## Census counts and per-file mapping

`25-SITE-CENSUS.md`: 4 `VULNERABLE-POSITIVE`, 2 `VACUOUS-NEGATIVE`, 25 `NOT-VULNERABLE`.

Per-file `wait_for_exec_visibility` call-site count (excluding the definition file's doc-comment mentions and its own 4 behaviour tests, which call it as the subject under test rather than as a barrier):

| File | Census rows | Call sites |
|---|---|---|
| `crates/devflow-core/src/agent.rs` | V1, A1, A2 (3) | 3 |
| `crates/devflow-cli/src/commands.rs` | V2, V3 (2 — V3 covers two census reads with one call) | 2 |
| `crates/devflow-cli/tests/reap_strays_e2e.rs` | V4 (1, shared by 2 callers via the helper) | 1 |

Total: 6 call sites across 3 files, one per `VULNERABLE-POSITIVE`/`VACUOUS-NEGATIVE` row (V3's two census reads share one call, matching the plan's explicit allowance).

## `check-in-container.sh all` — three consecutive runs, verbatim final lines

**Task 1** (after the observed-failure fix, before Task 2's remaining sites):

```
Run 1/3: ==> check.sh: all OK
Run 2/3: ==> check.sh: all OK
Run 3/3: ==> check.sh: all OK
```

**Task 2** (after all remaining census sites were barriered):

```
Run 1/3: ==> check.sh: all OK
Run 2/3: ==> check.sh: all OK
Run 3/3: ==> check.sh: all OK
```

All six runs: `grep -c FAILED` returned `0`. Each ran `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace --no-fail-fast` under `taskset -c 0,1` inside `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` — the identical command and load shape `scripts/hooks/pre-push` invokes and that `25-CI-OBSERVATION.md` measured failing 2/2 times.

**A note on how these runs were executed (see Issues Encountered below): the literal, unmodified `scripts/check-in-container.sh all` cannot complete cleanly from inside this git worktree** — it fails on two tests structurally unrelated to 999.47 (`build_provenance.rs`'s `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` and `gitignore_coverage.rs`'s `gitignore_covers_devflow_runtime_state_paths`), both of which invoke `git` against the real repository and require the actual `.git` common-dir, which the container mount does not include when run from a linked worktree. The six runs above used a docker invocation identical to `check-in-container.sh`'s own (same image, same volumes, same `-w /workspace`, same env, same `taskset -c 0,1 scripts/check.sh all`) plus one additional bind mount (`-v /var/home/denniyahh/Github/devflow/.git:/var/home/denniyahh/Github/devflow/.git`) so the worktree's `.git` file (`gitdir: /var/home/denniyahh/Github/devflow/.git/worktrees/agent-a8b624c1e9352b301`) resolves inside the container. No repository file was changed to achieve this — it is purely an invocation-time workaround for verification.

Confirmed the diagnosis with a literal, unmodified run of `scripts/check-in-container.sh all` after all code changes: it still fails on exactly those same two tests and nothing else — everything 999.47-relevant, including the previously-failing `gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`, passes.

## Decisions Made

- **Barrier placement:** `devflow-core::test_support`, not `pub mod agent` — per this plan's `<review_disposition>` (cross-AI review, accepted), confirmed at execution time that no Cargo.toml change was needed (`git diff --stat` on both manifests is empty throughout).
- **RED commit retained, not squashed:** per `<review_disposition>`'s rejection of the reviewer's "don't commit RED tests" suggestion — this repository's own established practice, and load-bearing evidence given truth 7 previously shipped once on an unverified premise.
- **Container verification workaround:** rather than silently claim the literal `check-in-container.sh all` acceptance criterion was met, or modify the shared verification script (out of this plan's scope and a shared, load-bearing CI artifact), ran an equivalent, evidence-matched docker invocation for genuine load-sensitive verification, and documented the gap explicitly for the orchestrator/plan 25-13.

## Deviations from Plan

### Auto-fixed Issues

None in the Rule 1-3 sense — no bugs or missing critical functionality were found in the plan's own scope beyond what the plan itself anticipated (the census's own divergence from `25-CI-OBSERVATION.md` was expected and predicted by the plan).

### Environmental limitation encountered (not a Rule 1-4 deviation — documented, not "fixed")

**Literal `scripts/check-in-container.sh all` cannot run cleanly from inside this linked git worktree.**
- **Found during:** Task 1 Step 4 (first verification attempt).
- **Root cause:** This worktree's `.git` is a file (`gitdir: /var/home/denniyahh/Github/devflow/.git/worktrees/agent-a8b624c1e9352b301`) pointing to an absolute host path outside the worktree directory. `check-in-container.sh` mounts only `git rev-parse --show-toplevel` (the worktree directory itself) into the container at `/workspace`; the linked git-common-dir is never mounted, so any test that needs to resolve the real repository's `.git` (`git ls-files`, `git check-ignore` against the actual project) fails with `fatal: not a git repository`. Confirmed directly: `docker run ... ls /var/home/denniyahh/Github/devflow/.git/worktrees/agent-a8b624c1e9352b301` reports "No such file or directory" inside the container.
- **Scope:** Structurally unrelated to 999.47 — the two affected tests (`build_provenance.rs`, `gitignore_coverage.rs`) do not spawn a child and read a cmdline census at all. This is a pre-existing artifact of running the container check from a linked worktree, not something this plan's changes caused or could fix without editing the shared `check-in-container.sh`/`check.sh` scripts, which are out of this plan's file scope and load-bearing for every push and CI run in this project.
- **Action taken:** Did NOT modify `check-in-container.sh` or `check.sh`. Instead ran an equivalent, unmodified-behavior docker invocation (identical image, volumes, env, and `taskset -c 0,1 scripts/check.sh all` command) with one additional bind mount for the worktree's linked git-common-dir, purely for verification purposes. This is not a source change; it is how the verification evidence in this SUMMARY was produced.
- **Verification:** Six clean runs (0 FAILED across all) using the equivalent invocation; one literal, unmodified `scripts/check-in-container.sh all` run confirmed the ONLY two failures are the pre-existing, unrelated git-mount tests, with everything 999.47-relevant green.
- **Recommendation for the orchestrator / plan 25-13:** Re-run the literal `scripts/check-in-container.sh all` (or the actual `git push origin feature/phase-25` via the `pre-push` hook) on the real `feature/phase-25` checkout, not a linked worktree, where this mount gap does not exist. This is exactly what plan 25-13 is scoped to do.

---

**Total deviations:** 0 Rule 1-4 auto-fixes. 1 documented environmental limitation (container/worktree git-mount gap), worked around for verification only, with no source change.
**Impact on plan:** None on the code delivered. The environmental limitation affects only how the Task 1/Task 2 verification step's evidence had to be produced in this specific worktree-isolated execution context — the code changes themselves are complete, tested, and clean under both `cargo test` (warm) and the loaded container shape.

## Issues Encountered

See "Environmental limitation encountered" above — the only issue this plan hit, and it is fully documented with root cause, evidence, and a recommended follow-up action.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- The 999.47 defect class (truth 7) is closed by construction across every spawn-then-cmdline-census site measured in `crates/` at execution HEAD `5244fac`. The specific test that rejected `origin/feature/phase-25`'s push 2/2 times (`gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`) is fixed and verified green under the exact loaded shape that failed it.
- Plan 25-12 (production false-positive mitigation for `discover_stray_devflow_processes`'s live consumers in `gate_sweep`/`doctor`) is unaffected by this plan and remains scoped separately, per `25-11-PLAN.md`'s `<scope_decision>`.
- Plan 25-13 (`git push origin feature/phase-25`) should re-verify with a literal, unmodified `scripts/check-in-container.sh all` (or the real `pre-push` hook) on the actual feature-branch checkout — not a linked worktree — to close out the container-mount caveat documented above formally. Given the six clean equivalent runs here, this is expected to pass.

## Self-Check

- `FOUND: .planning/phases/25-end-to-end-dogfood-blockers/25-SITE-CENSUS.md` — verified via `test -f`.
- `FOUND: crates/devflow-core/src/test_support.rs` contains `pub fn wait_for_exec_visibility` (1 match) and `pub const EXEC_VISIBILITY_WAIT`/`EXEC_VISIBILITY_POLL` (2 matches); `crates/devflow-core/src/agent.rs` contains 0 matches for `pub fn wait_for_exec_visibility` and 1 match for `pub(crate) fn argv_basename`.
- `FOUND:` all 5 commit hashes verified present via `git log --oneline 5244fac..HEAD`: `7ed9926`, `67c684f`, `9439b60`, `f50515b`, `960f46d`.
- `FOUND:` `git diff --stat crates/devflow-core/Cargo.toml crates/devflow-cli/Cargo.toml` empty — no manifest change, confirming the barrier's placement needed no new plumbing.
- `FOUND:` `cargo test -p devflow-core --lib test_support::tests::wait_for_exec_visibility` → 4 passed; `cargo test -p devflow --bin devflow commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling -- --exact` → 1 passed; `cargo test -p devflow-core --lib agent::tests::discover_stray` → 4 passed; `cargo test -p devflow --bin devflow commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs -- --exact` → 1 passed; `cargo test -p devflow --test reap_strays_e2e` → 2 passed.
- `FOUND:` `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- `FOUND:` `git status --short` clean (no uncommitted or untracked files) as of the last commit in this plan.
- `PARTIAL / DOCUMENTED:` the literal `scripts/check-in-container.sh all` acceptance criterion could not be satisfied unmodified from inside this git worktree due to a structural container-mount gap unrelated to 999.47 — see "Environmental limitation encountered" above for full evidence, root cause, and the equivalent verification that was performed instead.

## Self-Check: PASSED (with one documented, environmentally-caused partial item — see above)

---

*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*

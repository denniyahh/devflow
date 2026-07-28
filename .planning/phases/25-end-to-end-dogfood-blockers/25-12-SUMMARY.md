---
phase: 25-end-to-end-dogfood-blockers
plan: 12
subsystem: testing
tags: [rust, proc-fs, fork-exec-race, reap-strays, 999.47]

requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: 25-11's exec-visibility barrier (test_support::wait_for_exec_visibility) and its committed site census (25-SITE-CENSUS.md)
provides:
  - "agent::process_age(pid) -> Option<Duration> — /proc/uptime minus process_start_time, converted via a kernel-resolved (sysconf(_SC_CLK_TCK)) tick rate, never a hardcoded divisor"
  - "agent::STRAY_MIN_AGE (2s) — the age floor reap_stray_candidates refuses to signal below; an age floor, not a classifier, refusing both mid-execve false positives AND genuine strays younger than the floor"
  - "commands::StrayReapOutcome::TooYoung and reap_stray_candidates' new min_age parameter — the production reaper (devflow gate sweep --reap-strays) now refuses to SIGKILL a candidate whose age is unresolvable or below the floor"
  - "discover_stray_devflow_processes' doc comment now records a third hard constraint naming reap_stray_candidates as the caller obliged not to act on an unqualified census"
affects: [25-13 (the actual git push origin feature/phase-25)]

tech-stack:
  added: []
  patterns:
    - "Age-floor-at-the-signalling-decision: gate a destructive consumer of a structural /proc census by process age, not by trying to make the census itself exec-aware — the census stays a pure, unchanged read-only survey"
    - "min_age as an injected parameter (not a global constant read internally) so fixture-owned unit tests can disable a timing-sensitive floor deterministically via Duration::ZERO instead of sleeping past it"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "Age floor placed at the signalling decision (reap_stray_candidates), not at the census (discover_stray_devflow_processes) — per this plan's design_decision, gating the destructive consumer is proportionate, gating discovery would also suppress a legitimate doctor finding"
  - "STRAY_MIN_AGE = 2 seconds, hardcoded per the plan's design_decision (not tuned/configurable) — the two populations it separates (sub-millisecond execve window vs. minutes-to-hours orphan) are six orders of magnitude apart, so no value in between is contentious"
  - "min_age taken as a parameter, not read from the constant internally, so the four pre-existing reaper tests could be migrated to Duration::ZERO (deterministic, no sleep) rather than either sleeping past 2s per test or hardcoding the floor into every fixture"
  - "Container verification workaround (inherited from 25-11): ran the identical docker invocation check-in-container.sh uses, plus one added bind mount for the linked worktree's git-common-dir, for genuine load-sensitive verification; also ran the literal unmodified script once to confirm it fails on exactly the same two pre-existing, structurally unrelated tests 25-11 already documented. No repository file changed for this."

patterns-established:
  - "Age-floor-at-signalling pattern for any future /proc-derived structural census with a destructive consumer"

requirements-completed:
  - "25e (999.47 / DEN-72) — the production half of the defect class closed: gate sweep --reap-strays now refuses to signal any candidate younger than STRAY_MIN_AGE or whose age is unresolvable, reported via a new TooYoung outcome that increments skipped (never reaped) and emits no stray_reaped event"
  - "25d (999.44 / DEN-68) — gate sweep --reap-strays no longer signals a process it misidentified through fork-inheritance; the age floor is the guard is_same_process alone cannot provide, since a mid-execve child is genuinely the same process with genuinely the same recorded start time as its parent"

coverage:
  - id: D1
    description: "agent::process_age primitive: resolves the kernel's own USER_HZ tick rate via sysconf(_SC_CLK_TCK) rather than a hardcoded 100, computes age from /proc/uptime minus process_start_time, returns None on any unresolvable input (fail-closed), clamps a rounding-artefact negative delta to zero"
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core --lib agent::tests::process_age (3 passed: process_age_returns_some_for_the_current_process, process_age_returns_none_for_a_dead_pid, process_age_is_below_the_floor_for_a_fresh_child_and_grows_monotonically_for_self)"
        status: pass
    human_judgment: false
  - id: D2
    description: "reap_stray_candidates refuses to signal a candidate whose age is unknown or below agent::STRAY_MIN_AGE (TooYoung), evaluated after identity re-confirmation and before the dry_run early return; a live fixture proves the candidate is still alive after the refusal"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow commands::tests::reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age -- --exact (1 passed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The floor is a floor, not a blanket refusal: the same fixture shape, called with Duration::ZERO, IS reaped and verified dead"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow commands::tests::reap_stray_candidates_reaps_when_the_floor_is_zero -- --exact (1 passed)"
        status: pass
    human_judgment: false
  - id: D4
    description: "gate_sweep's call site passes the real agent::STRAY_MIN_AGE (not a literal Duration); its per-outcome match gains a TooYoung arm incrementing skipped and emitting no stray_reaped event"
    verification:
      - kind: other
        ref: "rg -n 'reap_stray_candidates\\(' crates/devflow-cli/src/commands.rs shows the gate_sweep call site passing agent::STRAY_MIN_AGE; reading the TooYoung match arm confirms skipped increment and no events::emit call"
        status: pass
    human_judgment: false
  - id: D5
    description: "Four pre-existing reap_stray_candidates_* tests migrated to the 3-arg signature with Duration::ZERO and an explanatory comment, no assertion changed; a new fail-closed test documents that a dead pid classifies as IdentityMismatch (not TooYoung) because is_same_process is evaluated first; the genuinely-unreachable TooYoung-via-None-age arm is disclosed as source-reasoned, following this file's own precedent for an untestable-by-black-box-fixture match arm"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow commands::tests::reap_stray_candidates (7 passed)"
        status: pass
    human_judgment: false
  - id: D6
    description: "discover_stray_devflow_processes' doc comment gains a third numbered hard constraint naming reap_stray_candidates as the obligation's owner; process_age's own doc comment cross-references both discover_stray_devflow_processes and reap_stray_candidates"
    verification:
      - kind: other
        ref: "rg -c 'reap_stray_candidates' crates/devflow-core/src/agent.rs returns 4; reading the doc comment confirms the third constraint and the cross-reference"
        status: pass
    human_judgment: false
  - id: D7
    description: "scripts/check-in-container.sh all passes three consecutive times, no overrides — the pre-push hook's own command, under the loaded fmt+clippy+test shape"
    verification:
      - kind: other
        ref: "3 equivalent-container docker invocations (same image/volumes/env/taskset -c 0,1 scripts/check.sh all as check-in-container.sh, plus one added bind mount for the linked worktree's git-common-dir), each ending '==> check.sh: all OK'; one literal, unmodified scripts/check-in-container.sh all run confirms exactly the same two pre-existing, structurally unrelated failures 25-11 already documented (build_provenance.rs, gitignore_coverage.rs — both fail on 'fatal: not a git repository' against the un-mounted git-common-dir) and nothing else"
        status: pass
    human_judgment: true
    rationale: "The literal, unmodified scripts/check-in-container.sh all command cannot run cleanly from inside this linked git worktree (same structural git-worktree/container-mount gap 25-11-SUMMARY.md documented, unrelated to 999.47 or this plan's changes). A human/orchestrator should re-run the literal command on the actual feature/phase-25 checkout (plan 25-13's job) to close this out formally, even though the equivalent verification here is genuine, load-sensitive, and reproduces the exact failure mode this defect class lives in."

duration: ~50min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 12: 999.47 Production Reaper Age Floor Summary

**Closed the production half of the 999.47 defect class: `devflow gate sweep --reap-strays` now refuses to `SIGKILL` any process discovered by `discover_stray_devflow_processes` whose age is below a new 2-second `agent::STRAY_MIN_AGE` floor or unresolvable — an age floor at the signalling decision, not a classifier at the census, that bounds out the `fork()`->`execve()` window `is_same_process` alone cannot catch.**

## Performance

- **Duration:** ~50 min
- **Started:** 2026-07-28T13:50Z (approx.)
- **Completed:** 2026-07-28T14:40Z
- **Tasks:** 2 (5 commits total, including the mandated TDD RED/GREEN split)
- **Files modified:** 2 (`crates/devflow-core/src/agent.rs`, `crates/devflow-cli/src/commands.rs`)

## Accomplishments

- Added `agent::process_age(pid) -> Option<Duration>` and `agent::STRAY_MIN_AGE` (2s): a process's age computed from `/proc/uptime` minus `process_start_time`, converted via a kernel-resolved `sysconf(_SC_CLK_TCK)` tick rate (never a hardcoded `100`), `None` on any unresolvable input, negative rounding artefacts clamped to zero rather than treated as errors.
- `reap_stray_candidates` (the injectable core `gate_sweep`'s `--reap-strays` path calls) gains a `min_age: Duration` parameter, evaluated after identity re-confirmation and before the `dry_run` early return, so a dry-run preview matches what a real run would refuse. A candidate whose age is unknown or below `min_age` is refused as a new `StrayReapOutcome::TooYoung` — never signalled.
- `gate_sweep`'s live call site passes the real `agent::STRAY_MIN_AGE`; its per-outcome `match` gains a `TooYoung` arm that increments `skipped` (never `reaped`), prints the pid/layer/reason, and emits no `stray_reaped` event — so an operator sees the refusal and `gate_sweep`'s existing post-pass re-discovery still reports it as clearable on the next invocation.
- Migrated all four pre-existing `reap_stray_candidates_*` unit tests to the 3-arg signature with `Duration::ZERO` (floor deliberately disabled, one-line comment each) — zero assertions changed.
- Added a new test documenting the actual control-flow interaction the plan flagged as a risk: a dead pid is classified `IdentityMismatch`, not `TooYoung`, because `is_same_process` is evaluated first and fails for the identical reason `process_age` would. The genuinely age-unknown-while-alive arm is disclosed as unreachable by any black-box fixture (would require `/proc/uptime` or `sysconf` to fail while `/proc/<pid>/stat` succeeds for the same live pid) — following this file's own established precedent for an untestable match arm.
- `discover_stray_devflow_processes`'s doc comment gains a third numbered hard constraint naming `reap_stray_candidates` as the caller obliged not to act on an unqualified census result; `process_age`'s own doc comment cross-references both directions.
- Verified under the loaded shape three consecutive times (see below), plus one literal run confirming the residual failure is the exact same pre-existing, unrelated container/worktree-mount gap 25-11 already documented.

## Task Commits

1. **Task 1 Step 1 — RED: failing tests for the age floor** — `9d905da` (test)
2. **Task 1 Step 2 — GREEN: process_age primitive and STRAY_MIN_AGE** — `654b94d` (feat)
3. **Task 1 Step 3 — GREEN (workspace intentionally non-compiling): reaper refusal** — `ea18c92` (fix)
4. **Task 2 Step 1 — migrate the four pre-existing reaper tests** — `4e8063d` (test)
5. **Task 2 Steps 2-3 — fail-closed test + invariant documentation + fmt fix** — `c37435a` (test)

_TDD gate (Task 1): `test(25-12)` commit `9d905da` precedes `feat(25-12)` commit `654b94d` — RED then GREEN, per this plan's `<review_disposition>` mandate (a cross-AI review's suggestion to drop the RED commit was explicitly rejected there) not to skip or reorder it._

## Files Created/Modified

- `crates/devflow-core/src/agent.rs` — `clock_ticks_per_second` (private), `process_age`, `STRAY_MIN_AGE`, 3 new tests, `discover_stray_devflow_processes`'s doc comment's third hard constraint
- `crates/devflow-cli/src/commands.rs` — `StrayReapOutcome::TooYoung`, `reap_stray_candidates`'s new `min_age` parameter, `gate_sweep`'s `TooYoung` match arm, 3 new tests (`reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age`, `reap_stray_candidates_reaps_when_the_floor_is_zero`, `reap_stray_candidates_refuses_a_dead_pid_as_identity_mismatch_before_the_age_check_runs`), 4 migrated pre-existing tests

## Test names created (acceptance criteria reference these by shape)

**`crates/devflow-core/src/agent.rs`** (all begin `process_age_`, per `artifacts_this_phase_produces`):
- `process_age_returns_some_for_the_current_process`
- `process_age_returns_none_for_a_dead_pid`
- `process_age_is_below_the_floor_for_a_fresh_child_and_grows_monotonically_for_self`

**`crates/devflow-cli/src/commands.rs`**:
- `reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age` (Test D — the outcome enum AND the live-process assertion)
- `reap_stray_candidates_reaps_when_the_floor_is_zero` (Test E — the floor is a floor, not a blanket refusal)
- `reap_stray_candidates_refuses_a_dead_pid_as_identity_mismatch_before_the_age_check_runs` (Step 2's fail-closed-path disposition — see below)

## `sysconf(_SC_CLK_TCK)` — host vs. container

Both report **100** (10ms ticks): `getconf CLK_TCK` on the host and inside `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` agree. No divergence to reconcile — `process_age` resolves this dynamically via `libc::sysconf(libc::_SC_CLK_TCK)` rather than assuming it, so a future environment where they differ would not silently mis-scale the floor.

## Step 2 disposition (fail-closed path)

Per the plan's own anticipated risk: constructing a candidate from a dead pid (`9_999_999`, above the default kernel `pid_max`, same shape `stop_is_a_success_no_op_when_the_lock_names_a_dead_pid` uses) does make `agent::process_age` return `None` — but `reap_stray_candidates` evaluates `agent::is_same_process` **first**, and that check ALSO fails for the identical reason (both derive from `agent::process_start_time`). The outcome is therefore `IdentityMismatch`, not `TooYoung` — added as `reap_stray_candidates_refuses_a_dead_pid_as_identity_mismatch_before_the_age_check_runs`, which documents this interaction rather than assuming a different one.

The `TooYoung`-via-`None`-age arm specifically (identity re-confirmed alive, but `process_age` itself returns `None`) requires `/proc/uptime` or `sysconf(_SC_CLK_TCK)` to fail while `/proc/<pid>/stat` succeeds for the **same live pid** — a combination no black-box process fixture in this suite can construct without faking `/proc`. Per the plan's explicit instruction, this is disclosed rather than faked: covered by source reasoning (`reap_stray_candidates`'s age check, `agent::process_age(candidate.pid).is_some_and(|age| age >= min_age)`, treats `None` as `false` by construction of `Option::is_some_and`, so an unresolvable age can never accidentally fall through to `Reaped`), documented inline in the new test's own doc comment, following this file's established precedent for an unreachable-by-black-box-test match arm (`stop_via_lock`'s wildcard arm).

## `check-in-container.sh all` — three consecutive runs, verbatim final lines

All three runs used the equivalent docker invocation `check-in-container.sh` itself issues (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`, the same `devflow-ci-target`/`devflow-ci-registry` named volumes, `CARGO_TARGET_DIR=/ctarget`, `taskset -c 0,1 scripts/check.sh all`), plus one added bind mount for the linked worktree's git-common-dir (`-v /var/home/denniyahh/Github/devflow/.git:/var/home/denniyahh/Github/devflow/.git`) — the same workaround 25-11-SUMMARY.md documented, for the same structural reason. No repository file was changed to achieve this.

```
Run 1/3: ==> check.sh: all OK
Run 2/3: ==> check.sh: all OK
Run 3/3: ==> check.sh: all OK
```

Each run's `test result: ok` lines confirm all 7 `reap_stray_candidates*` tests and all 3 `process_age*` tests pass inside the container, in addition to the full `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace --no-fail-fast` sequence `scripts/check.sh` runs.

**Confirmed the diagnosis with one literal, unmodified `scripts/check-in-container.sh all` run** after all code changes: it fails on exactly the same two pre-existing, structurally unrelated tests 25-11-SUMMARY.md already documented (`build_provenance.rs`'s `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild`, `gitignore_coverage.rs`'s `gitignore_covers_devflow_runtime_state_paths` — both `fatal: not a git repository: (null)`, because this worktree's `.git` file points to a host path (`/var/home/denniyahh/Github/devflow/.git`) the container's default mount (only the worktree directory itself) does not include). Everything 999.47/25-12-relevant, including every `reap_stray_candidates*` and `process_age*` test, passes.

## Decisions Made

See `key-decisions` in the frontmatter. In addition:

- **`Test A` (self-age > 0) required a >1-tick sleep before asserting positivity.** `process_age`'s own documented USER_HZ granularity floor (10ms, matching `process_start_time`'s existing caveat) means a process asked for its own age within one tick of start genuinely reads `Duration::ZERO` — measured directly, reproduced deterministically 5/5 running the test in isolation before the fix, not a flake. Sleeping 20ms first makes the assertion test "age advances," which is what the test's stated intent (`d > 0`) actually requires, rather than depending on incidental scheduling delay.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `process_age`'s self-age test assumption didn't hold at this environment's tick granularity**
- **Found during:** Task 1 Step 2 (taking Tests A-C green)
- **Issue:** The plan's Test A spec ("for this process's own pid it returns `Some(d)`, and `d` is greater than zero") assumed a freshly-started test binary's own age would read nonzero by the time the assertion runs. Measured directly: it reads exactly `Duration::ZERO` 5/5 times running the test in isolation — the same 10ms USER_HZ granularity `process_start_time`'s own doc comment already caveats, applied to a fast-starting process asked about itself within its first tick.
- **Fix:** Added a 20ms sleep before the self-age read, matching the codebase's own "measured, not assumed" ethos already documented for this exact granularity caveat.
- **Files modified:** `crates/devflow-core/src/agent.rs`
- **Verification:** 3/3 stable runs of `agent::tests::process_age` after the fix.
- **Committed in:** `654b94d` (Task 1 Step 2 commit)

### Environmental limitation encountered (not a Rule 1-4 deviation — inherited from 25-11, documented not "fixed")

**Literal `scripts/check-in-container.sh all` still cannot run cleanly from inside this linked git worktree** — identical root cause and identical two failing tests 25-11-SUMMARY.md already documented (this worktree's `.git` file points outside the mount `check-in-container.sh` provides). No repository file was modified to work around this; the docker invocation used for genuine verification evidence is documented above. Same recommendation as 25-11: plan 25-13 should re-verify with the literal, unmodified command on the actual `feature/phase-25` checkout, where this mount gap does not exist.

---

**Total deviations:** 1 Rule 1 auto-fix (test assumption corrected against measured behavior). 1 inherited, documented environmental limitation (container/worktree git-mount gap), worked around for verification only, with no source change.
**Impact on plan:** None on the code delivered. Both items affect only how evidence had to be gathered in this worktree-isolated execution context.

## Issues Encountered

See "Deviations from Plan" above — both items this plan hit are fully documented with root cause, evidence, and (for the environmental one) a recommended follow-up action identical to 25-11's.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- The 999.47 defect class is now closed on both halves: 25-11 closed the test-side window (every spawn-then-cmdline-census site in `crates/` crosses a barrier before reading the census), and this plan (25-12) closes the production-side window (`gate sweep --reap-strays`, the one path where a census miscall becomes a `SIGKILL`, now refuses to act inside the `fork()`->`execve()` window via an age floor).
- 999.44's own reaping machinery (`reap_stray_candidates`, `StrayReapOutcome`) is unchanged in its other three outcomes (`Reaped`, `IdentityMismatch`, `ReapFailed`) — this plan is strictly additive to that surface.
- `discover_stray_devflow_processes` itself is behaviourally unchanged (doc comment only) — its two consumers (`doctor`'s read-only finding, `gate_sweep`'s destructive reap) are unaffected except for the new `TooYoung` refusal on the destructive path, exactly per this plan's `<design_decision>`.
- Plan 25-13 (`git push origin feature/phase-25`) should re-verify with a literal, unmodified `scripts/check-in-container.sh all` (or the real `pre-push` hook) on the actual feature-branch checkout — not a linked worktree — to close out the inherited container-mount caveat formally. Given the three clean equivalent runs here (in addition to 25-11's own six), this is expected to pass.

## Self-Check

- `FOUND:` `crates/devflow-core/src/agent.rs` contains `pub fn process_age` (1 match) and `pub const STRAY_MIN_AGE` (1 match); `rg -c '_SC_CLK_TCK'` returns 2 (definition + doc-comment mention); no literal `100` divisor appears in the age computation (read directly).
- `FOUND:` `crates/devflow-cli/src/commands.rs` contains `StrayReapOutcome::TooYoung` and `reap_stray_candidates`'s 3-arg signature; `rg -c 'Duration::ZERO'` returns 10 (4 migrated call sites + explanatory comments + Test E).
- `FOUND:` all 5 commit hashes verified present via `git log --oneline 39da531..HEAD`: `9d905da`, `654b94d`, `ea18c92`, `4e8063d`, `c37435a`.
- `FOUND:` `cargo test -p devflow-core --lib agent::tests::process_age` → 3 passed; `cargo test -p devflow --bin devflow commands::tests::reap_stray_candidates` → 7 passed (`test result: ok`); `cargo test -p devflow --bin devflow commands::tests::reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age -- --exact` → 1 passed.
- `FOUND:` `cargo build --workspace --tests` succeeds; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean (after the one machine-applied formatting fix, committed).
- `FOUND:` `cargo test --workspace --no-fail-fast` (warm, host) → 0 failed across all targets.
- `FOUND:` `git status --short` clean (no uncommitted or untracked files) as of the last commit in this plan.
- `PARTIAL / DOCUMENTED (inherited from 25-11):` the literal `scripts/check-in-container.sh all` acceptance criterion could not be satisfied unmodified from inside this git worktree, for the same structural container-mount reason 25-11-SUMMARY.md already recorded — see "Environmental limitation encountered" above for full evidence and the equivalent verification performed instead (3 consecutive clean runs).

## Self-Check: PASSED (with one documented, environmentally-inherited partial item — see above)

---

*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*

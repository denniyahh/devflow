---
phase: 25-end-to-end-dogfood-blockers
plan: 14
subsystem: infra
tags: [git, preflight, compare-and-swap, worktree, ref-write, devflow-start]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: "25e's ensure_base_ref_current / base_ref_currency (999.51/D-18a) — the Behind arm's fast-forward this plan makes safe"
provides:
  - "base_is_checked_out_anywhere — repository-wide checked-out predicate parsed from `git worktree list --porcelain`, fail-CLOSED on an unreadable answer"
  - "fast_forward_base_ref — compare-and-swap `git update-ref refs/heads/<base> <new> <old>` write, refusing on a stale expected-old value"
  - "ensure_base_ref_current's Behind arm rewired onto both: single repository-wide checked-out gate, compare-and-swap write conditional on the local SHA resolved immediately before the write"
  - "Doc comment rewritten to state the new design and its residual scan-to-swap window instead of arguing the old design was sufficient"
affects: [phase-25-closure, devflow-start-preflight]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "predicate/enforce pairing (base_is_checked_out_anywhere / ensure_base_ref_current) matching this module's existing phase_reachability_on_base / ensure_phase_reachable_on_base shape"
    - "compare-and-swap ref write via git update-ref's <oldvalue> parameter, resolved with the existing rev-parse --verify --quiet idiom immediately before the write"
    - "deliberate fail-CLOSED polarity (return true / refuse-safe on an unreadable predicate) as the stated exception to this module's fail-open-where-blind default, scoped to write-authorizing predicates only"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/preflight.rs

key-decisions:
  - "Repository-wide `git worktree list --porcelain` scan combined with a compare-and-swap `git update-ref`, not `git branch -f` alone — `branch -f` has no `<oldvalue>` parameter and would close only the checked-out defect, leaving the lost-update defect open (per `<resolved_decision>` in 25-14-PLAN.md, following 25-VERIFICATION.md and 25-REVIEW.md CR-02)"
  - "The scan-to-swap window (a worktree checking out `<base>` between the repository-wide scan and the compare-and-swap) is accepted as a documented residual, not eliminated — bounded to two subprocess invocations, and the compare-and-swap still prevents a lost update inside it"

requirements-completed: ["25a", "CR-02", "D-17"]

coverage:
  - id: D1
    description: "The fast-forward write is a compare-and-swap: a base ref that moved between base_ref_currency's read and the write causes a refusal, not a silent discard"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#fast_forward_base_ref_refuses_a_stale_expected_old_value"
        status: pass
    human_judgment: false
  - id: D2
    description: "The checked-out precondition is repository-wide (git worktree list --porcelain across every worktree), not project_root-local"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#base_is_checked_out_anywhere_sees_a_linked_worktree"
        status: pass
    human_judgment: false
  - id: D3
    description: "ensure_base_ref_current refuses, and leaves refs/heads/<base> unmoved, when <base> is checked out in a second worktree not visible to project_root's own HEAD"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#currency_behind_refuses_when_base_is_checked_out_in_another_worktree"
        status: pass
    human_judgment: false
  - id: D4
    description: "All five pre-existing currency_* tests (Current/Ahead/Undeterminable/Diverged arms, plus the fast-forward-failure fall-through) pass unmodified"
    verification:
      - kind: unit
        ref: "cargo test --package devflow --bin devflow preflight::tests::currency_ (10 passed; 0 failed)"
        status: pass
    human_judgment: false

# Metrics
duration: 13min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 14: Compare-and-swap fast-forward + repository-wide checked-out predicate Summary

**`ensure_base_ref_current`'s `Behind` arm now fast-forwards via a `git update-ref <ref> <new> <old>` compare-and-swap and refuses on a base checked out in ANY worktree of the repository, closing both defects CR-02/999.51 found in the unconditional, single-worktree-scoped repair path.**

## Performance

- **Duration:** 13 min
- **Started:** 2026-07-28T17:25:58Z (worktree base commit `4975a98`)
- **Completed:** 2026-07-28T17:38:16Z
- **Tasks:** 2 completed
- **Files modified:** 1 (`crates/devflow-cli/src/preflight.rs`)

## Accomplishments

- `base_is_checked_out_anywhere(project_root, base) -> bool`: parses `git worktree list --porcelain`, returns `true` when ANY worktree stanza (not just `project_root`'s own `HEAD`) carries a line whole-line-equal to `branch refs/heads/<base>`; returns `true` (fail-CLOSED, refuse-safe) on a spawn error or non-zero exit.
- `fast_forward_base_ref(project_root, base, expected_old, new) -> bool`: a compare-and-swap `git update-ref refs/heads/<base> <new> <expected_old>` — refuses when the ref is not at `expected_old`.
- The `Behind` arm now: (1) gates on the single repository-wide predicate above (the old `project_root`-scoped `symbolic-ref` probe is deleted entirely, not left alongside), then (2) resolves both endpoints (`refs/heads/<base>`, `refs/remotes/origin/<base>`) via the existing `rev-parse --verify --quiet` idiom immediately before the write, and (3) calls the compare-and-swap with the resolved local SHA as `expected_old`. Any resolution failure or a refused swap falls through to the unchanged `stale_base_message` refusal, with the success line printed only after a swap that actually succeeded.
- `ensure_base_ref_current`'s doc comment rewritten: states the compare-and-swap and why (`Behind` establishes losslessness at read time, not write time); states the repository-wide scope of the checked-out predicate and why (`git update-ref` has no checked-out-branch protection of its own); states the scan-to-swap residual window in one sentence naming both what it prevents (a lost update) and what it does not (a live worktree observing a moved HEAD). No longer argues the old design was sufficient.
- Two new regression tests, each proven RED before the corresponding fix landed (see `## RED-before-GREEN evidence` below).

## Task Commits

Each task was committed atomically:

1. **Task 1: A repository-wide checked-out predicate, wired end to end through the Behind arm** - `6a1e467` (feat)
2. **Task 2: Make the fast-forward a compare-and-swap, and correct the doc comment that argued otherwise** - `97f6d78` (feat)

**Plan metadata:** pending (this commit, `docs(25-14): complete plan`)

_Note: both tasks were `tdd="true"`; RED-before-GREEN was proven for each new test by temporarily reverting the relevant fix (Task 1) or by the test failing to compile against the pre-fix source (Task 2), per the plan's own instructions — see evidence below._

## Files Created/Modified
- `crates/devflow-cli/src/preflight.rs` - added `base_is_checked_out_anywhere`, added `fast_forward_base_ref`, rewired `ensure_base_ref_current`'s `Behind` arm onto both, rewrote its doc comment, added 3 tests (`base_is_checked_out_anywhere_sees_a_linked_worktree`, `currency_behind_refuses_when_base_is_checked_out_in_another_worktree`, `fast_forward_base_ref_refuses_a_stale_expected_old_value`)

## Decisions Made

- **Compare-and-swap PLUS repository-wide scan, not `git branch -f` alone** — per the plan's `<resolved_decision>`: `git branch -f` has no `<oldvalue>` parameter and would close only the checked-out defect while leaving the lost-update (unconditional write) defect open. `git update-ref <ref> <new> <old>` is the only ref-write primitive that is a compare-and-swap, so the repository-wide predicate had to be built explicitly rather than delegated to `branch -f`'s own checked-out refusal.
- **Scan-to-swap window accepted as residual, not eliminated** — a worktree could still check out `<base>` between the repository-wide scan and the compare-and-swap. This is bounded to two subprocess invocations, the compare-and-swap still prevents a lost update inside that window, and the only alternative that closes it (`branch -f`) reopens the larger, higher-severity defect. Recorded in the doc comment and in `must_haves.truths` as a stated backstop rather than hidden.
- **Whole-line equality, not `contains`, for the worktree-branch match** — `line.trim() == format!("branch refs/heads/{base}")`, so `develop` cannot be falsely matched against a worktree checked out on `develop-experiment`. No test in the existing fixture set would have caught a `contains`-based implementation, so this was called out explicitly as its own acceptance criterion and verified by reading, not just by test count.

## RED-before-GREEN evidence

**Test 1 (Task 1): `currency_behind_refuses_when_base_is_checked_out_in_another_worktree`**

Pre-fix (with the old single-worktree `symbolic-ref --quiet --short HEAD` probe temporarily restored, new tests otherwise present and compiling):

```
running 1 test
test preflight::tests::currency_behind_refuses_when_base_is_checked_out_in_another_worktree ... FAILED

failures:

---- preflight::tests::currency_behind_refuses_when_base_is_checked_out_in_another_worktree stdout ----
advanced `develop` to `origin/develop` (1 commit(s) fast-forwarded)

thread 'preflight::tests::currency_behind_refuses_when_base_is_checked_out_in_another_worktree' (254299) panicked at crates/devflow-cli/src/preflight.rs:2221:66:
called `Result::unwrap_err()` on an `Ok` value: ()

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 223 filtered out; finished in 0.13s
```

This fails for exactly the defect this task closes: the deleted probe read `other` from `project_root`'s own `HEAD`, judged `develop` not checked out, and let the write proceed — even though `develop` was checked out in the linked worktree. Post-fix (repository-wide `base_is_checked_out_anywhere`): `1 passed`.

**Test 2 (Task 2): `fast_forward_base_ref_refuses_a_stale_expected_old_value`**

Pre-fix (`fast_forward_base_ref` did not yet exist — a legitimate compile-failure RED per the plan's own instruction):

```
error[E0425]: cannot find function `fast_forward_base_ref` in this scope
    --> crates/devflow-cli/src/preflight.rs:2253:14
     |
2253 |             !fast_forward_base_ref(local_root, "develop", &remote_sha, &remote_sha),
     |              ^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error[E0425]: cannot find function `fast_forward_base_ref` in this scope
    --> crates/devflow-cli/src/preflight.rs:2266:13
     |
2266 |             fast_forward_base_ref(local_root, "develop", &before, &remote_sha),
     |             ^^^^^^^^^^^^^^^^^^^^^ not found in this scope

error: could not compile `devflow` (bin "devflow" test) due to 2 previous errors
```

Post-fix: `1 passed`. (A first draft of this test also produced a false pass for the wrong reason — it read `origin/develop` without fetching first, so the remote-tracking ref was stale and accidentally equalled the correct expected-old value. Caught immediately by the assertion failing in the intended direction; fixed by adding an explicit `git fetch -q origin develop` before capturing the SHAs, documented inline in the test as a trap for future readers.)

## Verbatim output requested by `<output>`

**`cargo test --workspace --no-fail-fast` totals:**
- Baseline (stated in plan/project-traps): `688 passed / 0 failed` across 19 test binaries.
- After this plan: `691 passed / 0 failed` across 19 test binaries (`691 = 688 + 3` new tests: `base_is_checked_out_anywhere_sees_a_linked_worktree`, `currency_behind_refuses_when_base_is_checked_out_in_another_worktree`, `fast_forward_base_ref_refuses_a_stale_expected_old_value`). Verified by summing every `test result: ok. N passed; 0 failed` line across the full `cargo test --workspace --no-fail-fast` run — no `FAILED` line anywhere in the output.

**Exact four-element `git update-ref` argument list, as it appears in the final source (`fast_forward_base_ref`):**
```rust
.args([
    "update-ref",
    &format!("refs/heads/{base}"),
    new,
    expected_old,
])
```

**Exact line-matching expression `base_is_checked_out_anywhere` uses:**
```rust
let needle = format!("branch refs/heads/{base}");
String::from_utf8_lossy(&out.stdout)
    .lines()
    .any(|line| line.trim() == needle)
```
Whole-line equality on a trimmed line — not `contains`.

**Sentence recording the scan-to-swap residual window, as added to `ensure_base_ref_current`'s doc comment:**
> RESIDUAL, documented rather than eliminated: a worktree that checks out `base` in the window between the repository-wide scan and the compare-and-swap is not protected by the scan; the compare-and-swap still prevents a lost update in that window, but it cannot prevent that worktree from observing a moved HEAD.

## Deviations from Plan

None affecting scope or design — one self-corrected test-authoring mistake during Task 2 (see RED-before-GREEN evidence above): the first draft of `fast_forward_base_ref_refuses_a_stale_expected_old_value` read `origin/develop` without fetching first, making the "wrong" expected-old value accidentally equal the actual current ref value. This is not a Rule 1-3 deviation against the plan's design — it was caught and fixed before the task's `<verify>` was run, and the fix (an explicit fetch, documented inline) is exactly the kind of fixture correctness the plan's `<read_first>` pointers already establish elsewhere (`currency_fixture`/`advance_remote`'s existing pattern of an explicit remote-vs-local split). Recorded here for transparency, not as a scope change.

## Issues Encountered

None beyond the self-corrected test-authoring mistake above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Both defects CR-02/999.51 identified against `ensure_base_ref_current`'s `Behind` arm are closed: the write is now a compare-and-swap, and the checked-out precondition is repository-wide.
- `ensure_base_ref_current`'s signature and its `commands.rs:154` call site are unchanged, so this plan is fully disjoint from `25-15-PLAN.md` and `25-16-PLAN.md` in the same wave — no coordination needed at merge.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0.
- The phase-level container gate (`scripts/check-in-container.sh all`) is explicitly NOT run by this plan (see `<project_traps>` / `<verification>` item 6 in `25-14-PLAN.md`) — it is a post-merge, once-per-wave orchestrator step, deferred to avoid manufacturing the load-induced flake class 25-11/25-12/25-13 already closed.

## Self-Check

**Files exist:**
```
FOUND: crates/devflow-cli/src/preflight.rs
```

**Functions/tests present in final source:**
```
FOUND: base_is_checked_out_anywhere
FOUND: fast_forward_base_ref
FOUND: base_is_checked_out_anywhere_sees_a_linked_worktree
FOUND: currency_behind_refuses_when_base_is_checked_out_in_another_worktree
FOUND: fast_forward_base_ref_refuses_a_stale_expected_old_value
```

**Commits exist:**
```
FOUND: 6a1e467 (Task 1)
FOUND: 97f6d78 (Task 2)
```

**Test results:**
```
cargo test --package devflow --bin devflow preflight::tests::currency_behind_refuses_when_base_is_checked_out_in_another_worktree -- --exact => 1 passed
cargo test --package devflow --bin devflow preflight::tests::base_is_checked_out_anywhere_sees_a_linked_worktree -- --exact => 1 passed
cargo test --package devflow --bin devflow preflight::tests::fast_forward_base_ref_refuses_a_stale_expected_old_value -- --exact => 1 passed
cargo test --package devflow --bin devflow preflight::tests::currency_ => 10 passed; 0 failed
cargo test --workspace --no-fail-fast => 691 passed; 0 failed (19 binaries)
cargo clippy --workspace --all-targets -- -D warnings => exit 0
cargo fmt --check => exit 0
git diff --stat (both tasks combined) => 1 file changed: crates/devflow-cli/src/preflight.rs
```

## Self-Check: PASSED

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*

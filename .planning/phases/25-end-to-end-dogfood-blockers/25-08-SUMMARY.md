---
phase: 25-end-to-end-dogfood-blockers
plan: 08
subsystem: infra
tags: [preflight, git, worktree, versioning, gate, tdd]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers (25-06)
    provides: the original D-09 major-bump preflight gate (`preflight_major_bump_check`, `generic_preflight_checks`)
provides:
  - preflight_major_bump_check now classifies execution_root (the phase's worktree when state.worktree_path is set), not project_root
  - generic_preflight_checks aggregates all three generic checks instead of ?-short-circuiting, major-bump reason ordered first
  - a real git-worktree regression fixture (major_bump_worktree_fixture) proving CR-02, demonstrably RED before the fix and GREEN after with the test body unedited
  - a composed regression test proving CR-01 (gh-auth failure no longer hides a simultaneous major-bump failure), including a 300-char truncate_reason survival assertion
affects: [25-09, 25-10, any future phase touching preflight.rs's D-09/D-14 block]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "execution_root = state.worktree_path.as_deref().unwrap_or(project_root) — reused verbatim from staleness.rs::enforce_build_staleness for a second git-shelling check, now the established idiom for any check that must evaluate the phase's actual code tree"
    - "Aggregate-not-short-circuit for a Vec<String> of reasons joined with '; ', ordered by consequence so the highest-severity reason survives a downstream truncation cap"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/preflight.rs

key-decisions:
  - "CR-02 fix threads execution_root through all five git-shelling call sites (highest_semver_tag, reachable_semver_baseline, release_range_start, classify_range_bump, breaking_commit_subjects) rather than leaving any on project_root, per the plan's explicit instruction that a narrower fix could let the D-10 refusal and the classification disagree on tree."
  - "CR-01 fix is aggregation (option a), not special-casing GateAction::Advance (option b) — closes the same hole for any future check added to generic_preflight_checks, not just the major-bump check that surfaced it."
  - "Reason ordering is major-bump FIRST, then interactivity, then gh-auth, because truncate_reason's 300-char cap is load-bearing: the major-bump reason is the longest of the three and the only one whose loss would silently reopen the unattended-ship hole."
  - "Task 3 (tdd=\"true\") was executed as a genuine RED-then-GREEN cycle: after initially writing the test and fix together, the implementation change was reverted, the test re-run to confirm it fails for the right reason (short-circuits on gh-auth, err never contains MAJAR), committed as test(25-08), then the fix was reapplied and committed as feat(25-08)."

requirements-completed:
  - "25c (999.49 / DEN-74) — CR-01: generic_preflight_checks short-circuits, major-bump never runs"
  - "25c (999.49 / DEN-74) — CR-02: preflight_major_bump_check evaluates project_root, not the worktree"
  - "25c (999.49 / DEN-74) — test gap: no real-worktree fixture exercises the D-09 gate"

coverage:
  - id: D1
    description: "preflight_major_bump_check classifies execution_root (the worktree when state.worktree_path is set) instead of project_root, closing CR-02"
    requirement: "25c (999.49 / DEN-74) — CR-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs::tests::preflight_major_bump_check_fires_against_the_worktree_head"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs::tests::major_bump_errs_naming_bump_baseline_and_version_for_major_at_ship (pre-existing no-worktree case, unchanged)"
        status: pass
    human_judgment: false
  - id: D2
    description: "generic_preflight_checks aggregates all three generic checks (major-bump, interactivity, gh-auth) instead of ?-short-circuiting, so a gh-auth failure can never hide a simultaneous major-bump failure, closing CR-01"
    requirement: "25c (999.49 / DEN-74) — CR-01"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first"
        status: pass
    human_judgment: false
  - id: D3
    description: "Real git worktree add fixture (major_bump_worktree_fixture) exists and reproduces CR-02 structurally — demonstrated RED (0 passed; 1 failed) before Task 2's fix"
    requirement: "25c (999.49 / DEN-74) — test gap"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs::tests::preflight_major_bump_check_fires_against_the_worktree_head (RED-then-GREEN transition documented below)"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 08: D-09 major-bump gate fixed for the default worktree execution path

**Fixed CR-02 (preflight_major_bump_check evaluated project_root instead of the worktree) and CR-01 (generic_preflight_checks short-circuited on the first failing check, hiding the major-bump reason) — both proven by real `git worktree add` fixtures with a demonstrated RED-before-fix / GREEN-after-fix transition.**

## Performance

- **Duration:** ~6 min (first commit 00:17:33-04:00, last commit 00:23:07-04:00)
- **Started:** 2026-07-28T04:17:33Z
- **Completed:** 2026-07-28T04:23:07Z
- **Tasks:** 3
- **Files modified:** 1 (`crates/devflow-cli/src/preflight.rs`)

## Accomplishments
- A real `git worktree add` fixture (`major_bump_worktree_fixture`) and a regression test proving the D-09 major-bump gate did not fire against the worktree's HEAD in `devflow start`'s default execution mode — demonstrated RED (`0 passed; 1 failed`) before the fix.
- `preflight_major_bump_check` now classifies `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)`, mirroring `staleness.rs::enforce_build_staleness`'s established idiom — all five git-shelling call sites re-pointed, `breaking_commit_subjects`'s parameter renamed to match.
- `generic_preflight_checks` now aggregates all three generic checks (major-bump first, then interactivity, then gh-auth) instead of `?`-short-circuiting, so a `GateAction::Advance` (which never re-runs this function) can no longer discharge a control that was never evaluated. A composed regression test proves the aggregated reason survives `truncate_reason`'s 300-character cap.

## Task Commits

Each task was committed atomically:

1. **Task 1: Real-worktree fixture + RED regression test (CR-02)** - `4821b76` (test)
2. **Task 2: CR-02 — thread execution_root through the major-bump check** - `948991e` (fix)
3. **Task 3: CR-01 — aggregate generic preflight checks (TDD)** - `70ee8e3` (test, RED) + `4ad126d` (feat, GREEN)

**Plan metadata:** pending (this commit)

_Note: Task 3 (`tdd="true"`) has two commits — RED test, then GREEN implementation — per the plan's TDD requirement._

## Files Created/Modified
- `crates/devflow-cli/src/preflight.rs` — `preflight_major_bump_check` (execution_root threading), `breaking_commit_subjects` (renamed param), `generic_preflight_checks` (aggregation), `run_preflight`'s 18f doc paragraph (states its new dependency on aggregation), plus test infrastructure: `major_bump_worktree_fixture`, `git_only_path_dir_with_failing_gh`, and the two new regression tests.

## Decisions Made
- See `key-decisions` in frontmatter. No architectural decisions (Rule 4) were needed — both fixes are exactly the scope the plan specified.

## RED → GREEN Evidence (Task 1 → Task 2)

**RED (before Task 2's fix), `preflight_major_bump_check_fires_against_the_worktree_head`:**
```
running 1 test
test preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head ... FAILED

failures:

---- preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head stdout ----

thread 'preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head' (2322510) panicked at crates/devflow-cli/src/preflight.rs:1117:69:
called `Result::unwrap_err()` on an `Ok` value: ()

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 217 filtered out; finished in 0.05s
```
Failed on the FIRST assertion (the `Err` expectation), exactly as the plan required — `preflight_major_bump_check` classified `project_root`'s `develop` HEAD, where the breaking commit does not exist, and returned `Ok(())`.

**GREEN (after Task 2's fix), same test, unedited:**
```
running 1 test
test preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 217 filtered out; finished in 0.06s
```
`git diff 4821b76 HEAD -- crates/devflow-cli/src/preflight.rs | grep -c 'fn preflight_major_bump_check_fires_against_the_worktree_head'` returns `0` — the test function was never touched between the two states.

## RED → GREEN Evidence (Task 3, TDD)

**RED (before the aggregation fix), `generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first`:**
```
running 1 test
test preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first ... FAILED

---- preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first stdout ----

thread '...' panicked at crates/devflow-cli/src/preflight.rs:1320:9:
gh auth status reports not authenticated

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 218 filtered out; finished in 0.04s
```
The panic message IS the old `?`-chain's `Err` string — the interactivity check passed trivially (Ship stage), the gh-auth check failed and short-circuited via `?`, and `preflight_major_bump_check` never ran at all. `err.contains("MAJOR")` failed exactly as CR-01 predicted.

**GREEN (after the aggregation fix), same test:**
```
running 1 test
test preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 218 filtered out; finished in 0.06s
```

## Full Verification (plan's `<verification>` block, run in order)

1. `cargo fmt --check` — exit 0. ✅
2. `cargo clippy --workspace --all-targets -- -D warnings` — exit 0, no warnings. ✅
3. `cargo test --package devflow --bin devflow preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head -- --exact` — `1 passed`. ✅
4. `cargo test --package devflow --bin devflow preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first -- --exact` — `1 passed`. ✅
5. `cargo test --package devflow --bin devflow preflight::` — `36 passed; 0 failed`. ✅
6. `cargo test --workspace --no-fail-fast` — **676 passed, 0 failed** across every binary/integration-test target in the workspace. No failures to attribute; nothing deferred.

Note on the RED-phase mutex poisoning: when Task 1's RED test (or Task 3's RED test) panics while holding `ENV_MUTEX`, it poisons that mutex for the remainder of the SAME `cargo test` process — any subsequent `ENV_MUTEX`-guarded test in that run also reports a spurious failure (`PoisonError`). This is expected, self-resolving behavior of a deliberately-panicking RED test sharing a process-wide mutex with other tests, not a real regression; it was confirmed by re-running the unaffected tests in isolation (`--skip <red-test-name>`), which passed cleanly. Post-fix, no test panics, so no poisoning occurs in the final (GREEN) state — item 6 above is the proof, run against the fully-fixed HEAD.

## Deviations from Plan

**1. [Process correction, no Rule applies — TDD execution order] Task 3's test and fix were initially written together, then split into proper RED→GREEN commits**
- **Found during:** Task 3 (CR-01 aggregation)
- **Issue:** Task 3 carries `tdd="true"`, which requires a RED commit (failing test against the pre-fix code) before a GREEN commit (the fix). The test and the `generic_preflight_checks` rewrite were initially authored and verified together in one pass, skipping the RED proof.
- **Fix:** Before committing, the `generic_preflight_checks` implementation and the `run_preflight` doc-comment update were reverted back to the pre-fix `?`-chain, the new test was re-run to confirm it fails for the CR-01 reason specifically (gh-auth's `Err` string, never containing `MAJOR`), committed alone as `test(25-08)`, then the implementation was reapplied and committed as `feat(25-08)` after confirming `1 passed`.
- **Files modified:** `crates/devflow-cli/src/preflight.rs` (no additional files — same two commits described in Task Commits above)
- **Verification:** Both RED and GREEN output transcripts are recorded above; `git log` shows `test(25-08): add failing test for CR-01...` (`70ee8e3`) immediately followed by `feat(25-08): aggregate generic preflight checks...` (`4ad126d`), satisfying the TDD Gate Sequence Validation (RED commit exists, GREEN commit exists after it).
- **Committed in:** `70ee8e3` (RED), `4ad126d` (GREEN)

---

**Total deviations:** 1 process correction (no deviation Rule 1-4 applies — this is executor self-correction of task sequencing, not a plan or code defect).
**Impact on plan:** None on the shipped artifact. The final `generic_preflight_checks` implementation and test are identical to what a strict RED-first execution would have produced; only the commit history now correctly reflects the TDD gate sequence.

## TDD Gate Compliance

Task 3 (`tdd="true"`) gate sequence, verified in `git log`:
1. RED gate: `test(25-08): add failing test for CR-01 (generic_preflight_checks short-circuit)` — `70ee8e3`. ✅ present.
2. GREEN gate: `feat(25-08): aggregate generic preflight checks, major-bump reason first` — `4ad126d`, immediately after RED. ✅ present.
3. REFACTOR gate: not needed — no cleanup pass required after GREEN; omitted per the "optionally" clause.

## Issues Encountered
None beyond the process correction documented above.

## User Setup Required
None - no external service configuration required.

## Known Stubs
None. No hardcoded empty values, placeholder text, or unwired data sources were introduced by this plan.

## Threat Flags
None. All five threats named in this plan's `<threat_model>` (T-25-08-01 through T-25-08-05, plus T-25-08-SC) map exactly to the mitigations delivered (Task 2's execution_root threading, Task 3's aggregation and ordering) — no new, unaccounted-for surface was introduced. No dependency was added (T-25-08-SC's `cargo add` prohibition was never triggered).

## Next Phase Readiness
- `25-VERIFICATION.md` GAP 1's three `missing:` items are each closed by a named artifact: aggregation (Task 3), execution-root threading (Task 2), real-worktree regression test (Task 1).
- The full workspace suite (676 passed, 0 failed) confirms no regression outside `preflight::`.
- Plans 25-09 and 25-10 (the other gap-closure plans in this wave) are unaffected — this plan touched only `crates/devflow-cli/src/preflight.rs`.

## Self-Check

Verifying claims made in this SUMMARY against the actual repository state:

```
FOUND: crates/devflow-cli/src/preflight.rs (modified, confirmed via git diff c91ea9e HEAD --stat)
```

Commits:
- `4821b76` — `git log --oneline --all | grep 4821b76` → FOUND
- `948991e` — `git log --oneline --all | grep 948991e` → FOUND
- `70ee8e3` — `git log --oneline --all | grep 70ee8e3` → FOUND
- `4ad126d` — `git log --oneline --all | grep 4ad126d` → FOUND

Test assertions re-run at write-time (not self-reported from memory):
- `preflight::tests::preflight_major_bump_check_fires_against_the_worktree_head -- --exact` → `1 passed; 0 failed` (re-confirmed)
- `preflight::tests::generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first -- --exact` → `1 passed; 0 failed` (re-confirmed)
- `preflight::` full module → `36 passed; 0 failed` (re-confirmed)
- `cargo fmt --check` → exit 0 (re-confirmed)
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0, no warnings (re-confirmed)
- `cargo test --workspace --no-fail-fast` → `676 passed; 0 failed` across all targets (re-confirmed)

## Self-Check: PASSED

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*

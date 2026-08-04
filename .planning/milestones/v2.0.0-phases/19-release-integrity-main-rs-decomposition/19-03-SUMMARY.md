---
phase: 19-release-integrity-main-rs-decomposition
plan: 03
subsystem: infra
tags: [git, release-integrity, tdd, locale]

requires:
  - phase: 19-release-integrity-main-rs-decomposition (19-01, 19-02)
    provides: sequencing only (same file family, no functional dependency)
provides:
  - "Git::commit_path is a genuine no-op on unchanged content (no forced empty commit)"
  - "git_raw's subprocess locale pinned to C (LC_ALL=C, LANG=C)"
  - "New git_raw_combined helper (stdout+stderr) so the 'nothing to commit' match can actually fire"
  - "Written D-17 finding on commit_all's empty-commit behavior"
affects: [19c-19f (main.rs split, same crate), release/ship tooling]

tech-stack:
  added: []
  patterns:
    - "git subprocess helpers pin LC_ALL=C/LANG=C when a caller string-matches git's own output"
    - "sibling low-level helper (git_raw_combined) added instead of touching a shared, other-caller helper (git_raw) when only one call site needs different error-text capture"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/git.rs

key-decisions:
  - "commit_path routed through a new git_raw_combined helper instead of git_raw, after discovering git's 'nothing to commit' message is written to stdout, not stderr — stderr_or_status only inspects stderr, so the plan's literal 'git_raw maps it to GitError::Command' description could never actually fire. git_raw itself (and commit_all, which still calls it) is untouched except the two .env() lines; see Deviations."
  - "D-17 verdict: commit_all's empty-commit behavior is NOT load-bearing, based on direct source search — its only caller (hooks.rs:184, docs_update) already treats a commit failure as non-fatal (warn + continue), and no test in crates/devflow-core or crates/devflow-cli asserts a commit exists after docs_update runs. commit_all left byte-identical per the plan's hard prohibition regardless."

requirements-completed: [19b]

coverage:
  - id: D1
    description: "commit_path called twice with byte-identical content creates exactly one commit (the version_bump retry scenario)"
    requirement: "19b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#commit_path_twice_with_identical_content_creates_only_one_commit"
        status: pass
    human_judgment: false
  - id: D2
    description: "commit_path on a path with no changes returns Ok(()) and leaves the commit count unchanged (no-op, not an error)"
    requirement: "19b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#commit_path_with_no_changes_returns_ok_without_committing"
        status: pass
    human_judgment: false
  - id: D3
    description: "commit_path on a nonexistent path still errors (unchanged edge case, not over-applied)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#commit_path_on_nonexistent_path_still_errors"
        status: pass
    human_judgment: false
  - id: D4
    description: "git subprocess locale pinned to C so the nothing-to-commit match is locale-independent"
    verification:
      - kind: other
        ref: "rg -n 'LC_ALL|LANG' crates/devflow-core/src/git.rs (4 hits: 2 env() pairs in git_raw and git_raw_combined)"
        status: pass
    human_judgment: false
  - id: D5
    description: "commit_all provably untouched (D-17 collision guard with 19a)"
    verification:
      - kind: other
        ref: "git diff crates/devflow-core/src/git.rs shows zero hunks inside commit_all; region-scoped rg count of --allow-empty inside commit_all still 1"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-07-21
status: complete
---

# Phase 19 Plan 03: commit_path idempotence (19b) Summary

**`Git::commit_path` no longer forces an empty commit on unchanged content — the previously-dead `nothing to commit` arm is now live, routed through a new stdout-aware helper after discovering git's own no-op message is on stdout, not stderr.**

## Performance

- **Duration:** ~20 min
- **Completed:** 2026-07-21
- **Tasks:** 2 (RED, GREEN)
- **Files modified:** 1

## Accomplishments

- `commit_path` is a genuine no-op when the scoped path has no changes — `git rev-list --count HEAD` is unchanged by a repeat call with identical content, closing the defect where `hooks::version_bump` could tag a release on a commit containing nothing.
- Two new regression tests pin the exact `version_bump` retry scenario and the `Ok(())`-without-a-commit contract as two separate, independently-checked claims (T-19-11).
- A third test pins the unchanged edge case (`commit_path` on an unknown path still errors) so the fix cannot be over-applied into "never fails".
- `git_raw`'s subprocess locale pinned to `C` (`LC_ALL=C`, `LANG=C`), per Antigravity's review, so the `nothing to commit` string match survives a non-English host locale (T-19-14).
- `commit_all` is provably byte-identical apart from the two locale env lines it also inherits (it calls `git_raw`, unchanged); its retained empty-commit behavior is documented per D-17 below rather than removed.

## Task Commits

1. **Task 1 (RED): failing idempotence test** - `694abcc` (test) — three new tests added to the existing `commit_path` test block; idempotence test failed with `n2 == n1 + 1` against unfixed HEAD, exactly as the defect predicts.
2. **Task 2 (GREEN): stop forcing the commit** - `f5b7e41` (fix) — `commit_path` drops `--allow-empty`; new `git_raw_combined` sibling helper added (see Deviations); doc comment corrected; `git_raw` gains `LC_ALL=C`/`LANG=C`.

**Plan metadata:** (this commit, docs)

## Files Created/Modified

- `crates/devflow-core/src/git.rs` — `commit_path` no longer forces an empty commit; new `git_raw_combined` helper; `git_raw` pins subprocess locale; three new tests; doc comment corrected.

## Verbatim RED Evidence (Task 1)

```
running 4 tests
test git::tests::commit_path_on_nonexistent_path_still_errors ... ok
test git::tests::commit_path_twice_with_identical_content_creates_only_one_commit ... FAILED
test git::tests::commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted ... ok
test git::tests::commit_path_with_no_changes_returns_ok_without_committing ... FAILED

failures:

---- git::tests::commit_path_twice_with_identical_content_creates_only_one_commit stdout ----

thread 'git::tests::commit_path_twice_with_identical_content_creates_only_one_commit' panicked at crates/devflow-core/src/git.rs:742:9:
assertion `left == right` failed: a repeat commit_path call on unchanged content must not add a commit: n1=2, n2=3
  left: 3
 right: 2

---- git::tests::commit_path_with_no_changes_returns_ok_without_committing stdout ----

thread 'git::tests::commit_path_with_no_changes_returns_ok_without_committing' panicked at crates/devflow-core/src/git.rs:772:9:
assertion `left == right` failed: no-op call must not create a commit: n1=2, n2=3
  left: 3
 right: 2

test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 302 filtered out
```

Both failures show `n2 == n1 + 1` — the exact "forced empty commit" defect the plan describes — and the pre-existing scoping test (`commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted`) stayed green throughout, confirming the fixtures weren't disturbed.

## D-17 finding: commit_all

**Verdict: not load-bearing** (based on direct source search; `commit_all` left unchanged regardless, per the plan's hard prohibition).

**Evidence:**
- `commit_all`'s only remaining caller is `crates/devflow-core/src/hooks.rs:184`, inside `docs_update`:
  ```rust
  if let Err(err) = git.commit_all("docs: update generated docs") {
      warn!("DocsUpdate: commit failed: {err}");
  } else {
      info!("DocsUpdate: docs regenerated and committed");
  }
  ```
  A commit failure here is already non-fatal — it's logged and the hook still returns `Ok(())` (line 193). The call site was written to tolerate `commit_all` not producing a commit at all, let alone tolerate an empty one.
- Searched the full test suite (`rg -n "DocsUpdate|docs_update" crates/devflow-core/src/hooks.rs crates/devflow-cli/tests/`) for any assertion that a commit exists after `docs_update` runs. The only test that actually invokes `Hook::DocsUpdate` is `validate_to_ship_hooks_do_not_touch_changelog` (hooks.rs), and it asserts only that `CHANGELOG.md` does not exist afterward — nothing about commit count, `git log`, or working-tree cleanliness. No test in `crates/devflow-cli/tests/` references `DocsUpdate` or `docs_update` at all.
- `crates/devflow-core/src/lock.rs:37` mentions "docs commits" only in a comment about what the checkout lock serializes, not a behavioral dependency on an empty commit specifically.
- Conclusion: nothing downstream inspects, counts, or relies on `commit_all` having produced a commit when the working tree was clean going in. `commit_all` is left exactly as it was at HEAD (verified: `git diff` shows zero hunks inside its body), matching the plan's hard prohibition (T-19-13) independent of this finding.

## Decisions Made

- Routed `commit_path` through a new `git_raw_combined` helper rather than `git_raw` directly (see Deviations below for why).
- D-17 verdict recorded as "not load-bearing" per the evidence above; `commit_all` not modified.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `git commit`'s "nothing to commit" message is on stdout, not stderr — the plan's literal fix could not work as written**
- **Found during:** Task 2 (GREEN), first test run after removing `--allow-empty`
- **Issue:** The plan's action text assumes `git_raw`'s existing error mapping (`stderr_or_status`, which only inspects `output.stderr`) would surface git's "nothing to commit, working tree clean" message so the pre-existing match arm could convert it back to `Ok(())`. Reproducing the exact command by hand (`git commit -m ... -- path` on a clean path) showed git writes that message to **stdout**; `output.stderr` is empty, so `stderr_or_status` always falls back to `"exited with {status}"` and the match arm's `msg.contains("nothing to commit")` check could never be true — with `git_raw` unmodified, the whole fix would be non-functional (confirmed live: both new tests failed with `Err(Command("exited with exit status: 1"))` even after `--allow-empty` was removed).
- **Constraint in tension:** The plan explicitly prohibits changing `git_raw`'s error-mapping logic, `stderr_or_status`, or the `GitError` enum — "the ONE permitted change to `git_raw` is adding `LC_ALL=C`/`LANG=C`... touch no error-mapping branch." That prohibition's stated rationale ("altering it would break the arm this fix relies on") assumed the existing mapping already worked; it does not, so the rationale doesn't cover this case, but the letter of the prohibition still applies to `git_raw` itself.
- **Fix:** Added a new private sibling method, `git_raw_combined`, used **only** by `commit_path`. It runs the same subprocess but builds the `GitError::Command` string from both `stdout` and `stderr` combined, so the "nothing to commit" text (wherever git puts it) is visible to the match arm. `git_raw` itself is untouched except the two `.env()` locale lines (verified: `git diff` shows the only hunk inside `git_raw` is the env addition); `commit_all` keeps calling the original `git_raw` and is provably byte-identical otherwise.
- **Files modified:** `crates/devflow-core/src/git.rs`
- **Verification:** `cargo test -p devflow-core commit_path` — 4/4 pass; region-scoped `rg` assertions confirm `git_raw`'s error-mapping branch is unchanged and `commit_all` has zero hunks; `cargo test --workspace` 306+ tests, 0 failed; clippy/fmt clean.
- **Committed in:** `f5b7e41` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — bug in the plan's own literal fix description, discovered empirically)
**Impact on plan:** Necessary for the feature to function at all; all of the plan's explicit prohibitions on `commit_all`, `stderr_or_status`, and `GitError` are still honored to the letter, and `git_raw`'s only change is the locale env addition the plan itself required. No scope creep — the new helper is ~15 lines, used by exactly one call site.

## Issues Encountered

None beyond the deviation above — resolved inline during Task 2.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 19b closed. `commit_path` and `commit_all` are now independently understood: one is idempotent by design, the other retains a documented (not load-bearing) empty-commit behavior.
- No blockers for 19c-19f (`main.rs` split) — this plan touched only `crates/devflow-core/src/git.rs`, disjoint from the split's `main.rs` files.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-21*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/git.rs
- FOUND: .planning/phases/19-release-integrity-main-rs-decomposition/19-03-SUMMARY.md
- FOUND: 694abcc (RED commit)
- FOUND: f5b7e41 (GREEN commit)

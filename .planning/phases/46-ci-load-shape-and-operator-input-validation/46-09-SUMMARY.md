---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 09
subsystem: infra
tags: [git, pre-commit, bash, awk, staged-index, regression-tests, scanner]

requires:
  - phase: 46-ci-load-shape-and-operator-input-validation
    provides: "46-05's containment-based plan-bashism scanner and named-path fixture suite"
provides:
  - "`lint-plan-bashisms.sh --staged` reads selected plan bytes from Git's index, never from the worktree."
  - "NUL-delimited staged discovery includes added, copied, modified, and renamed plan destinations while deliberately excluding deletions."
  - "Four disposable-repository integration tests prove index/worktree divergence, rename discovery, and deletion exclusion."
affects: [phase-46-verification, pre-commit-plan-safety, INFRA-01]

actuals:
  tokens: 3315
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "For staged Git checks, discover paths and read content from the same index snapshot."
    - "Use `devflow_core::test_support::git_command` for every disposable-repository Git subprocess."
    - "Pair staged-state regressions with inverse controls so a scanner that always accepts or always rejects cannot look correct."

key-files:
  created: []
  modified:
    - scripts/lint-plan-bashisms.sh
    - crates/devflow-cli/tests/plan_bashism_scanner.rs

key-decisions:
  - "Staged mode uses `git show \":$path\"` through a temporary file, preserving blob bytes and terminal newlines while retaining the original path for diagnostics."
  - "The cached-diff filter is `ACMR`: rename destinations are selected, while deletions remain visible zero-file scans because they have no index blob to inspect."
  - "Disposable fixture repositories disable commit signing locally and invoke every Git command through the hermetic test-support constructor."

patterns-established:
  - "A staged check must not discover from the index and then read the working tree; that pairing is bypassable after partial staging."
  - "A tab-bearing rename path is a useful negative control for NUL-delimited Git discovery."

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "The staged scanner refuses a bad index blob even when the worktree copy is clean, and ignores a bad unstaged worktree copy when the index is clean."
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/plan_bashism_scanner.rs#staged_bad_working_tree_clean_is_blocked and #staged_clean_working_tree_bad_is_accepted"
        status: pass
    human_judgment: false
  - id: D2
    description: "A renamed bad plan is selected and refused, while a staged deletion is a successful visible zero-file scan."
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/plan_bashism_scanner.rs#staged_renamed_bad_plan_is_blocked and #staged_deleted_plan_is_skipped"
        status: pass
    human_judgment: false
  - id: D3
    description: "The existing direct-path containment fixtures remain executable in the full scanner binary."
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "cargo test -p devflow --test plan_bashism_scanner -- --nocapture"
        status: pass
    human_judgment: false

duration: 4 min
completed: 2026-09-07
status: complete
---

# Phase 46 Plan 09: Staged index scanner correctness Summary

**The plan-bashism scanner now evaluates only the plan bytes Git will commit, including renamed destinations, with four hermetic disposable-repository regressions that prove both dangerous and inverse staged/worktree directions.**

## Performance

- **Duration:** 4 min, measured from first task commit to final task commit; this excludes pre-commit inspection and post-commit verification.
- **Started:** 2026-09-07T00:53:18Z (first task commit)
- **Completed:** 2026-09-07T00:56:53Z (final task commit)
- **Tasks:** 2 of 2
- **Files modified:** 2

## Accomplishments

- Closed C-08: `--staged` selects plans from the cached diff and feeds each selected `:$path` index blob, not filesystem bytes, to the existing containment parser.
- Closed C-09: discovery is NUL-delimited `ACMR`; a renamed bad destination is inspected, while a deletion remains an intentional zero-file success.
- Closed C-10: disposable repositories exercise actual Git indexes with the required staged-bad/worktree-clean, clean-index/worktree-bad, renamed-bad, and deletion controls.

## Task Commits

1. **Task 1: RED — prove the scanner follows the working tree and overlooks a staged rename** — `3fc383f` (test)
2. **Task 2: GREEN — select renamed index entries and scan the staged blob** — `1898f76` (fix)
3. **Task 2 follow-up: satisfy the workspace lint gate** — `24238be` (fix)

## Evidence

### RED gate

Before changing the scanner, the required filtered run reported:

```text
test staged_bad_working_tree_clean_is_blocked ... FAILED
test staged_clean_working_tree_bad_is_accepted ... ok
test staged_deleted_plan_is_skipped ... ok
test staged_renamed_bad_plan_is_blocked ... FAILED
test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 11 filtered out
cargo_exit=101
```

The failing staged-bad test observed `scanned 1 file(s)` with exit 0 because the old scanner read the clean working-tree bytes. The failing rename test observed `scanned 0 file(s)` with exit 0 because the old `ACM` filter skipped it. The two controls remained green, so RED was not a blanket failure.

### GREEN gate

The final filtered staged run reported `4 passed; 0 failed; 11 filtered out` and `cargo_exit=0`:

| Disposable repository test | Observed scanner outcome asserted by the test |
| --- | --- |
| `staged_bad_working_tree_clean_is_blocked` | Non-zero refusal; `scanned 1 file(s)` and one diagnostic line naming `docs/staged-bad-PLAN.md` with `${PIPESTATUS[0]}`. |
| `staged_clean_working_tree_bad_is_accepted` | Exit 0 and `scanned 0 file(s)`; the only bad bytes were unstaged. |
| `staged_renamed_bad_plan_is_blocked` | Non-zero refusal; `scanned 1 file(s)` and a diagnostic naming the tab-bearing `docs/new<TAB>bad-PLAN.md` destination with `${PIPESTATUS[0]}`. |
| `staged_deleted_plan_is_skipped` | Exit 0 and `scanned 0 file(s)` after `git rm`. |

The full binary was re-run after the final commit: `15 passed; 0 failed`. It includes the direct-path legitimate `bash -c` acceptance control and the known violating multiline fixture. `bash -n`, ShellCheck, `cargo fmt --all --check`, and `cargo clippy --workspace --all-targets -- -D warnings` also passed; Clippy completed with `clippy_exit=0`.

This evidence proves local index-versus-worktree and rename/deletion semantics for the production scanner. It does **not** establish that a hosted Git hook is installed or invoked in another checkout.

## Decisions Made

- Index content is the sole content source in staged mode. A failed `git show ":$path"` is a path-naming exit 2; there is no filesystem fallback.
- The staged blob is written to a `mktemp` input for AWK rather than command-substituted, preserving terminal newlines. The EXIT trap removes only those owned temporary inputs.
- Tests seed commits inside their disposable repositories and set `commit.gpgSign=false` locally, preventing an inherited interactive signing configuration from breaking fixture setup.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Disposable baseline commits inherited an interactive signing policy**
- **Found during:** Task 1 RED execution.
- **Issue:** Fixture commits exited 128 because an inherited signing configuration prompted for an unavailable private-key passphrase, preventing the two baseline-dependent controls from reaching scanner behavior.
- **Fix:** Configured `commit.gpgSign=false` only inside each disposable repository, through `devflow_core::test_support::git_command`.
- **Files modified:** `crates/devflow-cli/tests/plan_bashism_scanner.rs`
- **Verification:** The RED run then produced exactly two failures and two passing controls.
- **Committed in:** `3fc383f`

**2. [Rule 1 - Test correctness] The clean fixture still contains the allowed expression**
- **Found during:** Task 1 RED execution.
- **Issue:** The first precondition treated any `${PIPESTATUS[0]}` occurrence as violating, but the legitimate fixture contains it inside a valid `bash -c` region.
- **Fix:** Compared index and worktree bytes against their complete intended fixture strings, proving the actual bad-versus-clean arrangement instead of using a proxy substring.
- **Files modified:** `crates/devflow-cli/tests/plan_bashism_scanner.rs`
- **Verification:** The discriminating RED result and final four-test GREEN result.
- **Committed in:** `3fc383f`

**3. [Rule 1 - Lint] ShellCheck cannot statically see the EXIT-trap cleanup callback**
- **Found during:** Task 2 verification.
- **Issue:** ShellCheck reported SC2329 even though the callback is invoked by `trap ... EXIT`.
- **Fix:** Added one local SC2329 suppression with the trap rationale.
- **Files modified:** `scripts/lint-plan-bashisms.sh`
- **Verification:** `bash -n` and ShellCheck pass.
- **Committed in:** `1898f76`

**4. [Rule 1 - Lint] The rename assertion triggered `clippy::manual_contains`**
- **Found during:** Plan-level Clippy verification.
- **Issue:** The otherwise passing test suite failed the required workspace lint backstop.
- **Fix:** Replaced the manual iterator comparison with `fields.contains(&new_path.as_bytes())`.
- **Files modified:** `crates/devflow-cli/tests/plan_bashism_scanner.rs`
- **Verification:** Full 15-test binary, formatting, and workspace Clippy all pass.
- **Committed in:** `24238be`

**5. [Rule 3 - Verification command] This host's `rg -E` treats `-E` as an encoding flag**
- **Found during:** Task 2 GREEN verification.
- **Issue:** The plan's regex assertions stopped after the test command despite the four tests being green.
- **Fix:** Re-ran the identical patterns with Ripgrep's portable expression flag, `rg -e`.
- **Files modified:** None.
- **Verification:** The rerun matched the required 4/0 filtered and 15/0 full-test summaries.

---

**Total deviations:** 5 auto-fixed (3 Rule 1, 2 Rule 3).
**Impact on plan:** All fixes were necessary to make the mandated test and lint evidence trustworthy. No dependency, hook, or unrelated Phase 46 artifact changed.

## TDD Gate Compliance

- RED is committed as `test(46-09)` in `3fc383f`.
- GREEN is committed as `fix(46-09)` in `1898f76`, matching the task's explicit requested commit type. There is no `feat(46-09)` commit, so a strict commit-type-only TDD audit may flag the label even though the GREEN implementation and its tests are present.

## Known Stubs

None. The modified files were scanned for TODO/FIXME/placeholder-style markers; no renderable stub or deferred behavior was found.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

C-08, C-09, and C-10 now have executable regression coverage and passing local verification. Phase verification can rely on this plan for staged scanner semantics; hosted-hook installation remains outside the evidence provided here.

---
*Phase: 46-ci-load-shape-and-operator-input-validation*
*Completed: 2026-09-07*

## Self-Check: PASSED

The SUMMARY exists, all three task commits resolve, and none of the task commits deletes a tracked file.

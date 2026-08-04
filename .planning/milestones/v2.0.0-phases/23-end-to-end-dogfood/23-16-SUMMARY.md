---
phase: 23-end-to-end-dogfood
plan: 16
subsystem: staleness-guard
tags: [devflow, staleness, dogfood, gap-closure, git]
status: complete
dependency-graph:
  requires: [23-15]
  provides: [23g-divergent-lineage-content-check]
  affects: [23-17]
tech-stack:
  added: []
  patterns:
    - "content-aware ancestry narrowing extended from the strict-ancestor arm to the divergent-lineage arm of embedded_commit_is_stale, reusing ancestry_range_affects_build verbatim (D-07)"
key-files:
  created:
    - .planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md
  modified:
    - crates/devflow-cli/src/staleness.rs
    - CHANGELOG.md
decisions:
  - "23g: the Ok(Some(1)) reverse-probe arm of embedded_commit_is_stale now calls ancestry_range_affects_build before deciding Stale vs Fresh, mirroring the strict-ancestor arm exactly — no new helper, no forked logic (D-07)"
  - "This plan supersedes the previously-agreed 'retry with a develop-built binary' workaround (23-ACCEPTANCE-RUN-2.md §15) at the operator's explicit direction: fix the staleness check itself rather than special-case which branch the binary is built from"
  - "Scope boundary held: this plan does not relaunch the phase-23 acceptance attempt — that is 23-17"
metrics:
  duration: "~2.5h"
  completed: "2026-07-26"
---

# Phase 23 Plan 16: Content-Check the Divergent-Lineage Staleness Arm Summary

Closed the exact defect that hard-blocked the 23-15 acceptance run: a
genuinely divergent build (neither commit an ancestor of the other) was
classified `Stale` unconditionally, with no content check at all, even when
the only commit that made it diverge touched nothing but a `.planning/` doc.

## What Was Built

`embedded_commit_is_stale`'s `Ok(Some(1))` reverse-probe arm — the genuine-
divergence case (`crates/devflow-cli/src/staleness.rs`) — now calls
`ancestry_range_affects_build(execution_root, embedded_commit)` before
deciding `Stale` vs. `Fresh`, exactly mirroring the strict-ancestor arm
immediately above it (the 21d/999.29 fix). The helper itself needed no
changes — its `git diff --name-only <embedded> HEAD` is a two-dot TREE
comparison, not a history comparison, so it was already correct across
divergence; only its call site was gated behind the wrong condition.

A divergent range whose committed diff touches nothing build-affecting
(`.rs`/`Cargo.toml`/`Cargo.lock`/`build.rs`/`rust-toolchain.toml`) now
classifies `Fresh`. A divergent range that does touch a build-affecting
file still classifies `Stale`, unchanged — regression-locked by a dedicated
test.

Three new tests in `staleness::tests`:

1. **`divergent_lineage_docs_only_range_is_fresh`** — constructs two
   genuinely mutually-non-ancestor commits (confirmed via two
   `merge-base --is-ancestor` exit-code assertions inside the test itself,
   both must exit 1) whose only difference is a `.planning/` doc. Proven
   RED against the unmodified code first (see below), then GREEN after
   the fix.
2. **`divergent_lineage_with_source_change_is_stale`** — the same
   divergent construction, but the range also touches a real `.rs` file.
   Asserts `Stale`, guarding against the fix over-permitting a genuinely
   stale divergent build.
3. **`enforce_build_staleness_does_not_block_self_dogfood_on_divergent_docs_only_lineage`**
   — drives the real `enforce_build_staleness` entry point (not only the
   pure `embedded_commit_is_stale` predicate) against a self-dogfood
   workspace fixture, asserting `Ok(())` — per this project's
   test-signal-rejection standard (pattern 4), a fix must be proven at the
   actual call path `devflow start` uses, not only at a private helper.

Doc-comment corrections: `ancestry_range_affects_build`'s own comment now
states it narrows both the strict-ancestor arm and the divergent-lineage
arm; the `Staleness` enum's top-level doc block now states that a strict-
ancestor OR a divergent range with no build-affecting diff is `Fresh`, not
only an exact HEAD match.

`CHANGELOG.md` gained a `### Fixed` subsection under `## 2.0.0 —
2026-07-26`, naming the mechanism (21d/999.29 extended), the concrete
2026-07-26 incident, and stating explicitly that a divergent range
touching real source still blocks.

**PR #33** (`feature/phase-23 → develop`,
https://github.com/denniyahh/devflow/pull/33) was opened with all 8 CI
checks green, then **merged into `develop` by the operator**
(`9916e2fdaa5d5a16e9875fd90ab93381d0312726`, confirmed via `gh pr view 33`:
`state: MERGED`, `mergedBy.login: denniyahh`, `is_bot: false`) — no
autonomous write ever touched `develop` across any task of this plan. Both
the fix commit (`c2e947a`) and the PR head (`2af0374`) are confirmed
ancestors of `origin/develop`'s new tip via `git merge-base
--is-ancestor`, exit 0 for both.
`.planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md`
records the branch, commit SHAs, PR number/URL, per-check CI conclusions,
before/after test counts, the operator's verbatim merge confirmation, and
the post-merge ancestry proof.

## RED Evidence (Task 1 Step 1, verbatim)

Run against the unmodified pre-fix code:

```
$ cargo test -p devflow --bin devflow staleness::tests::divergent_lineage_docs_only_range_is_fresh -- --nocapture

thread 'staleness::tests::divergent_lineage_docs_only_range_is_fresh' (2783943) panicked at crates/devflow-cli/src/staleness.rs:1598:9:
assertion `left == right` failed: 23g / 2026-07-26 acceptance-run regression shape: a docs-only divergent range must not hard-block
  left: Stale
 right: Fresh
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test staleness::tests::divergent_lineage_docs_only_range_is_fresh ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 185 filtered out; finished in 0.05s
```

Failed exactly on the `Fresh` assertion (`left: Stale, right: Fresh`) — not
a fixture-precondition assertion (both `merge-base --is-ancestor` exit-code
assertions passed, confirming genuine divergence), and not a compile
error. This is a valid RED per requirement 1 and 3 of
`.claude/skills/ai-change-acceptance/rules/change-acceptance.md`.

## Existing Coverage Re-Confirmed, Not Duplicated

`embedded_commit_is_stale_maps_ancestry_exit_codes` (pre-existing, D-20)
already asserts an empty string and an unknown 40-hex-char commit both
classify `Indeterminate`, and this path is untouched by this fix. It ran
unchanged and passed as part of the full `staleness::tests` run below,
satisfying "unknown/absent commit → Indeterminate" without a duplicate
test, per the plan's own instruction.

## Gate Chain Results

**`staleness::tests` module in isolation:**

```
$ cargo test -p devflow --bin devflow staleness::tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 166 filtered out
```

18 pre-existing + 3 new, matching the plan's required count exactly.

**Full workspace suite, direct `&&` status chain:**

```
$ cargo test --workspace --no-fail-fast
```

Per-binary counts summed: 611 passed, 0 failed, 0 ignored (across 17
binaries). Baseline per `23-ACCEPTANCE-RUN-2.md` §11 / `23-VERIFICATION.md`
was **608 passed, 0 failed**. Delta: **exactly +3, 0 failed** — matching
the 3 tests this plan adds. No count disagreed in any other direction.

**Clippy and fmt:**

```
$ cargo clippy --workspace --all-targets -- -D warnings
CLIPPY_EXIT=0
$ cargo fmt --check
FMT_EXIT=0
```

**Source-property acceptance checks (plan-specified `rg` assertions):**

- `rg -v '^\s*//' crates/devflow-cli/src/staleness.rs | rg -c 'ancestry_range_affects_build\(execution_root, embedded_commit\)'` → `2` (strict-ancestor call site + this plan's new divergent-arm call site).
- `rg -n 'Ok\(Some\(1\)\) => Staleness::Stale' crates/devflow-cli/src/staleness.rs` → no match (the bare unconditional-Stale arm is gone, not merely shadowed).
- `git diff crates/devflow-cli/src/staleness.rs` before committing Task 1 showed only additions plus the single narrowed match arm and the two doc-comment corrections — confirmed by inspecting every `-` line in the diff; no pre-existing test assertion was modified.

## Deviations from Plan

### Auto-fixed / Self-corrected Issues

**1. [Executor process error, self-caught and corrected] PR opened before the CHANGELOG commit existed.**
- **Found during:** Task 2, immediately after opening PR #33.
- **Issue:** The plan's action sequence is baseline → CHANGELOG → gate chain
  → open PR → wait for CI → write record. The fix commit (`c2e947a`) was
  pushed and PR #33 opened before the `CHANGELOG.md`/record commit
  (`973d185`) existed, meaning the PR initially did not carry the
  CHANGELOG entry the Task 3 checkpoint's `how-to-verify` explicitly asks
  the operator to review inside the PR diff.
- **Fix:** Pushed `973d185` (and a subsequent record-accuracy correction,
  `2af0374`) to the same branch before finishing Task 2, so PR #33's final
  head (`2af0374`) carries the fix, the CHANGELOG entry, and the complete
  record. Re-waited for CI green after each push.
- **Files affected:** none beyond the already-planned `CHANGELOG.md` and
  `23-STALENESS-FIX-RECORD.md`.
- **Commits:** `973d185`, `2af0374`.

### Findings Recorded, Not Absorbed

**2. Pre-existing, previously-documented CI-intermittent flake
(`agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process`,
`crates/devflow-core/src/agent.rs`) fired on CI on all three of this
plan's pushes** — never locally (three separate full-workspace local runs
all passed cleanly), never in a file this plan touches. The test's own
in-source comment documents it as a known flake ("first seen 2026-07-26,
on commits touching no Rust source... does not reproduce locally —
40/40 under CPU load"), a PID-reuse race in the CI container's `/proc`
reads. Out of this plan's scope boundary (only issues directly caused by
this plan's diff are auto-fixable; this is pre-existing and in an
unrelated file). Each failed job was re-run via `gh run rerun <id>
--failed` with no code change between failure and rerun, and passed
cleanly every time — three reruns total, no code change across any of
them. Full detail, including every affected job ID and rerun, is in
`23-STALENESS-FIX-RECORD.md`'s "Pull request" section.

**3. Redaction acceptance criterion (`rg -c '<operator OS username>'` → `0`)
is not literally satisfiable on this project.** Task 2's acceptance
criteria require `rg -c '<the operator OS username>'
23-STALENESS-FIX-RECORD.md` to return `0`. The operator's OS username and
their GitHub account name are the identical string (`denniyahh`), and the
same task's acceptance criteria separately require this record to carry
the PR URL (`https://github.com/denniyahh/devflow/pull/33`), which
necessarily contains that string — a literal reading of both criteria
together is self-contradictory; satisfying one criterion (deleting the
string) would violate the other (recording the PR URL). This is recorded
as a **criterion deviation with rationale, not a pass**: the narrowed grep
actually run and reported, targeting the real leak class this checklist
exists to catch per 999.10 (filesystem paths —
`/home/denniyahh|/var/home/denniyahh|/tmp/`), is clean, confirmed twice
(once in Task 2, re-confirmed independently by the continuation executor
in Task 3). The public GitHub account name appearing in PR/CI URLs is not
a filesystem-path leak and is retained consistently with every other
committed planning artifact in this repository (e.g.
`23-GUARD-SHIP-RECORD.md`'s identical disposition). Full detail in
`23-STALENESS-FIX-RECORD.md`'s Task 3 section.

## Known Stubs

None. No stub, placeholder, or empty-data-flow pattern was introduced by
this plan.

## Threat Flags

None beyond the plan's own pre-declared `<threat_model>` register (all six
threats T-23g-01…06 dispositioned at plan time — no new security-relevant
surface was introduced beyond what the plan already declared: no new
network endpoint, no new auth path, no new file-access pattern, and the
one new git invocation shape reuses an already-audited helper verbatim at
a second call site).

## Self-Check

- `crates/devflow-cli/src/staleness.rs` — FOUND (modified, committed in
  `c2e947a`).
- `CHANGELOG.md` — FOUND (modified, committed in `973d185`).
- `.planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md` —
  FOUND (created in `973d185`, corrected in `2af0374`).
- Commit `c2e947a53e4781da9ee799beaba9e541d16781db` — FOUND in
  `git log --oneline --all`.
- Commit `973d1859b8ad661c74407e20842c26aaadb61ce9` — FOUND in
  `git log --oneline --all`.
- Commit `2af0374f463600227e22c802fd2b09145b9a21d1` — FOUND in
  `git log --oneline --all` (current `HEAD`).
- PR #33 (`https://github.com/denniyahh/devflow/pull/33`) — FOUND, merged
  by the operator, `state: MERGED`, `mergedBy.login: denniyahh`,
  `is_bot: false`.
- `git log origin/develop --oneline -1` — `0dad20d` at the start of Task 2
  (unchanged through Task 2's entire execution, confirming no autonomous
  write); now `9916e2f` (`Merge pull request #33 from
  denniyahh/feature/phase-23`) after the operator's merge, confirmed via
  independent `git fetch origin` by the Task 3 continuation executor.
- `git merge-base --is-ancestor c2e947a53e4781da9ee799beaba9e541d16781db
  origin/develop` — exit 0 (fix commit).
- `git merge-base --is-ancestor 2af0374f463600227e22c802fd2b09145b9a21d1
  origin/develop` — exit 0 (PR head).

## Self-Check: PASSED

## Task 3: Operator Merge (Complete)

The operator reviewed PR #33 and merged it into `develop` themselves,
reporting the merge commit SHA verbatim: `Merged
9916e2fdaa5d5a16e9875fd90ab93381d0312726`. The continuation executor
independently verified, first-hand:

- `git fetch origin` → exit 0.
- `git log origin/develop -1 --format='%H %an %s'` →
  `9916e2fdaa5d5a16e9875fd90ab93381d0312726 Dennis Kim Merge pull request
  #33 from denniyahh/feature/phase-23` — subject identifies a
  pull-request merge, not a direct push.
- `git merge-base --is-ancestor <SHA> origin/develop` — exit 0 for BOTH
  the fix commit (`c2e947a`) and the PR head (`2af0374`), named
  separately.
- `gh pr view 33 --json state,mergedAt,mergeCommit,mergedBy` →
  `state: MERGED`, `mergeCommit.oid` matching the operator's reported SHA
  exactly, `mergedBy.login: denniyahh` (`is_bot: false`).
- No command run by any executor across this plan's three tasks wrote to
  `develop` — every command touching `origin/develop` in Task 3 was
  read-only (`fetch`, `log`, `merge-base --is-ancestor`, `gh pr view`).

Full detail recorded in `23-STALENESS-FIX-RECORD.md`'s "Task 3: Operator
merge into `develop`" section.

## Next Step

Task 3 is complete — the fix is merged to `origin/develop`. This plan does
not relaunch the phase-23 acceptance attempt against phase 24; that is
23-17, now unblocked by this merge landing.

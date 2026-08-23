---
status: backlog
source: Phase 19 plan 19-11 / 19-VERIFICATION.md accepted override (2026-07-22)
---

# Backlog: Refactor Equivalence Guard in CI

## Goal

Give pure-move refactors an automated equivalence check that runs on CI, not
just in the executing agent's local shell.

Phase 19 split an 8,487-line `main.rs` into nine sibling modules and proved the
move was behavior-preserving with three checks:

1. **Symbol reconciliation** — every top-level function present before is
   present after, none added.
2. **Test name-set identity** — the sorted set of test names is byte-identical
   to a committed baseline (`19-SPLIT-BASELINE-names.txt`).
3. **Per-target pass counts** — each test binary's count matches the baseline.

All three passed. But they were run **locally**, by hand. CI runs only
`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
and `cargo fmt --check`. Phase 19 shipped with an explicit, user-accepted
verification override recording exactly this coverage limitation.

## Why this matters

`cargo test` passing proves the suite is green; it does **not** prove a refactor
preserved behavior. A move that silently drops a test function still shows a
green suite — with a quietly smaller number. The name-set check is what catches
that, and it is the one check CI cannot currently make.

This is the same class of hole 19g's acceptance contract targets: a green
signal standing in for evidence nobody actually gathered.

## Possible shapes (not yet decided)

- A `refactor-equivalence` CI job, opt-in per PR via label or a committed
  baseline file, that diffs the live test-name set against the baseline and
  fails on any removal.
- A committed helper script so the extraction command is reviewed once rather
  than retyped per phase. Note that Phase 19 found the plan's literal
  `rg '::tests::'` extraction was itself buggy — it dropped top-level tests and
  retained Cargo's suffix — and had to be corrected mid-phase
  (see `19-11-SUMMARY.md`). Any committed script needs its own test.
- Narrower alternative: assert only that the total test count never decreases
  without an accompanying baseline update.

## Notes

Scope this to refactor-shaped changes. Running a name-set identity check on
ordinary feature work would fail constantly and get disabled — which is worse
than not having it.

Relates to 999.19 (fast/slow CI lanes) and 999.20 (differential coverage).

Promote with `/gsd-review-backlog` when ready.

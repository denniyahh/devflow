---
phase: 17-pipeline-dogfood-followup
plan: 07
subsystem: infra
tags: [rust, cli, git, build-provenance, gap-closure, dogfood-finding]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "Plan 05's embedded_commit_is_stale/combined_staleness/staleness_outcome/enforce_build_staleness self-dogfood gate, and Plan 06's strict-ancestor (WR-01) tightening of the same function"
provides:
  - "embedded_commit_is_stale distinguishes a strict DESCENDANT embedded commit (Staleness::Ahead) from a genuinely older or divergent one (Staleness::Stale)"
  - "staleness_outcome maps Ahead to Warn for every project, self-dogfood included — an ahead build is never hard-blocked, but never silent either"
  - "DevFlow can drive its own workspace from a binary built on an unmerged feature branch, which is the ordinary self-dogfood posture"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Bidirectional ancestry probing: `merge-base --is-ancestor A B` exit 1 is not a verdict on its own — the reverse probe is required before calling a commit stale"

status: complete
completed: 2026-07-19
---

# 17-07: Descendant build misclassified as stale

## What was built

`embedded_commit_is_stale` treated `git merge-base --is-ancestor <embedded> HEAD`
exit 1 as `Staleness::Stale`. Exit 1 only asserts "not an ancestor", which is
equally true of a commit that is *newer* than HEAD. The gate therefore
hard-blocked builds that were ahead of the checkout — the exact inverse of the
"committed, forgot to rebuild" incident it was written to catch.

- Added `Staleness::Ahead` for a strict descendant.
- The exit-1 arm now runs a reverse probe (`--is-ancestor HEAD <embedded>`):
  exit 0 means descendant (`Ahead`), exit 1 means divergent/older (`Stale`),
  anything else stays `Indeterminate`.
- `staleness_outcome` gained `(_, Ahead) => Warn` — never `Block`, for
  self-dogfood or ordinary projects alike.
- `combined_staleness` needed no change: its early return fires only on
  `Stale`, so an `Ahead` ancestry result still falls through to the mtime arm
  and is correctly upgraded to `Stale` on a dirty tree whose tracked source is
  newer than the build.

## How it was found

Not by review — by the pipeline blocking itself. Phase 17's own Validate stage
was hard-blocked with `self_dogfood_stale_blocked` after the driving binary was
rebuilt from `feature/phase-17` while the primary checkout sat on `develop`.
Confirmed empirically before any code was changed: embedded `8725304` vs HEAD
`a2c314f`, forward probe exit 1, reverse probe exit 0 — descendant, not stale.

Phase 17 verification scored 12/12 without catching this because both existing
ancestry tests exercise only the ancestor direction. This is a coverage gap in
the verification, not a wrong verdict on what it did check.

## Key files

- `crates/devflow-cli/src/main.rs` — `Staleness` enum, `embedded_commit_is_stale`
  reverse probe, `staleness_outcome` Ahead arm, and the regression test
  `ahead_build_from_descendant_commit_warns_instead_of_blocking`.

## Verification

- New test written RED first (failed to compile on the missing variant), then GREEN.
- `cargo test --workspace`: 360 passed, 0 failed. Notably
  `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` still passes,
  so Plan 06's fix is not regressed, and
  `embedded_commit_is_stale_maps_ancestry_exit_codes` still passes, so genuinely
  divergent commits still classify as `Stale`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- Task 2 (self-permit): rebuilt binary resumed phase 17 into Validate with no
  `self_dogfood_stale_blocked` event — the condition that blocked this phase now
  warns and proceeds.

## Known nits (not addressed here)

- The `Warn` message is generic ("did not confirm a fresh build") and does not
  name the ahead-of-checkout case specifically. Accurate, but a more precise
  message would diagnose faster. Left alone to keep this change surgical.
- A one-off 83-minute hang was observed in a `cargo test --workspace` run during
  this work. It did not reproduce: the same test binary subsequently passed
  62/62 in 10.2s single-threaded and 3.0s in parallel, and the full workspace
  suite passed in ~10s. Flagged as an unexplained flake, not diagnosed.

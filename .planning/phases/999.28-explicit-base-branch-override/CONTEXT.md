---
status: backlog
source: Phase 21 dogfood-launch design discussion (2026-07-23)
---

# Backlog: Explicit `--base` Branch Override for `devflow start`

## Goal

Add an explicit `--base <branch>` flag to `devflow start` (and the worktree
launch path), defaulting to `develop`, so an operator can deliberately cut a
phase's `feature/phase-NN` branch onto a base *other* than `develop` — most
importantly, onto an unmerged predecessor phase branch to honor a
`depends_on` chain.

Do **not** switch to implicitly basing on the operator's current branch. The
base must stay explicit and stated, never inferred from shell state.

## The finding

`devflow start` hardcodes the feature-branch base to `develop`:
`crates/devflow-core/src/git.rs:54` derives `feature/phase-{:02}` and the launch
path (`pipeline_launch.rs`, non-test) cuts it from `develop`. The whole
git-flow pipeline downstream — `ship` → Merge into develop → VersionBump →
ChangelogAppend, plus `sync-main-to-develop.sh` and the `release` preflight —
assumes feature→develop→main, and `devflow parallel` integrates concurrent
worktrees off a develop-rooted shared base (`parallel.rs`). So the hardcode is
load-bearing for ship and parallel, not incidental.

The gap it creates: the ROADMAP encodes phase dependencies (e.g. 22→21→20),
and `devflow` tracks them, but because every phase bases on `develop`, you
**cannot build phase 22 on top of an unmerged phase 21**. The dependency is
recognized at the planning layer but unrepresentable at the branch layer.

## Why this matters

Stacked/dependent phases are a first-class concept in the roadmap that the
launch model can't express. Today the only way to give phase 22 phase 21's work
is to merge 21 to develop first — which forces serialization even when the
operator wants to stack deliberately.

An explicit `--base` (default `develop`) closes the gap without regressing the
git-flow guarantees:

- default stays `develop` → ship/parallel/release semantics unchanged;
- `devflow start --phase 22 --base feature/phase-21` enables intentional
  stacking;
- base is explicit, so a phase is never silently rooted on a dirty throwaway
  branch — the footgun of an implicit "current branch" default.

## Scope notes / open questions

- Where does `--base` need to thread through: `start` only, or also `parallel`
  (shared-base derivation) and `resume`/`recover` (they reconstruct base from
  state)?
- Ship/merge target: when a phase is based on `feature/phase-21` rather than
  `develop`, does `ship` merge back to the base or still to `develop`? Likely
  still `develop` after the predecessor lands, but this needs a design pass —
  it interacts with the release flow.
- Validation: reject a `--base` that doesn't exist; warn (not block) if base is
  not an ancestor of `develop`.

Relates to the Phase 21/22 scope (base selection is both operator usability and
concurrency governance). Linear: TBD.

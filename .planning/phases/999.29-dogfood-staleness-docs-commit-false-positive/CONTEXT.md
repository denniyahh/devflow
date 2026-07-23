---
status: backlog
source: Phase 21 dogfood run (2026-07-23) — staleness guard false-positive, observed live
---

# Backlog: Dogfood Staleness Guard False-Positives on Docs-Only Commits

## Goal

Stop `enforce_build_staleness` (17d D-17/D-18, `crates/devflow-cli/src/staleness.rs`)
from **hard-blocking a self-dogfood run when the only commits ahead of the
binary's embedded commit changed nothing the compiler sees** (e.g. `.planning/`
docs). Make the commit-ancestry arm **content-aware**, the same way the
dirty-tree arm already is.

## The finding (observed live, Phase 21)

Repro from this session, exactly:

1. Built the release binary at commit `7163347` (HEAD at the time).
2. Committed two **docs-only** commits — the phase scaffold (`56a1835`) and
   backlog item 999.28 (`3a17381`). Both touched `.planning/` **only**.
3. Ran `devflow start --phase 21`. It created the `feature/phase-21` worktree
   (HEAD `3a17381`) and immediately **hard-blocked**:

   > self-dogfood stale build blocked for stage define: this devflow binary's
   > embedded commit is not an ancestor of …/.worktrees/phase-21's current HEAD
   > (or its tracked source is newer than the build) — rebuild devflow …

4. Rebuilt at `3a17381`, re-ran with `--force` → passed. The rebuild produced a
   **functionally identical binary** — no build input had changed.

### Root cause (verified, not inferred)

`combined_staleness` → `embedded_commit_is_stale` runs
`git merge-base --is-ancestor <embedded> HEAD`. When `<embedded>` is a **strict
ancestor** of HEAD (exit 0, but `HEAD != embedded`) it returns `Staleness::Stale`
**unconditionally** — purely on the commit graph, with **no check of whether the
intervening commits touched build inputs.**

Verified for this incident:
```
git merge-base --is-ancestor 7163347 3a17381   # exit 0 → strict ancestor → Stale
git diff --name-only 7163347 3a17381           # → only .planning/* ; 0 files under crates/
```

So: docs-only commits advance HEAD past the binary's embedded commit, and the
ancestry arm flags `Stale` even though the compiled artifact is current.

### The asymmetry that makes it a bug, not a policy

The **dirty-tree** arm was already narrowed for exactly this reason. 17-10 fixed
`tree_has_modified_build_inputs` to filter `git status --porcelain` paths through
`affects_compiled_binary` (`.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`,
`rust-toolchain.toml`) so a dirty `CHANGELOG.md` no longer hard-blocks Ship. That
same content-awareness was **never applied to the ancestry arm** — which still
treats *any* forward HEAD movement as staleness.

## Why this matters

DevFlow's primary workflow is dogfooding itself, and dogfooding commits
`.planning/` docs constantly (scaffold, backlog, CONTEXT, discussion logs,
SUMMARY). Under the current guard, **every docs commit after a build re-arms the
hard block**, forcing a no-op rebuild before the next stage. It has recurred
across multiple sessions (see `[[feedback-dogfood-rebuild-before-revalidate]]`,
`[[project-gsd-execute-devflow-quirks]]`) and taxes every run. The guard exists
to prevent the Phase 16 false-evidence incident (a stale binary producing false
green) — a real and important protection — but a docs-only delta cannot change
what the binary does, so blocking on it is pure false-positive friction with no
safety value.

## Proposed fix

1. **Make the ancestry arm content-aware.** When `<embedded>` is a strict
   ancestor of HEAD, before returning `Stale`, run
   `git diff --name-only <embedded> HEAD` and filter through the existing
   `affects_compiled_binary`. If **no** build-affecting file changed in that
   range → `Fresh` (the artifact still matches its inputs). Only return `Stale`
   if the range actually touched compiled inputs. Mirrors the 17-10 dirty-tree
   narrowing; reuses `affects_compiled_binary` verbatim.
2. **Fix the misleading message.** The block text says "embedded commit is not
   an ancestor of HEAD" — but the most common dogfood trigger is the case where
   it **is** an ancestor (just behind). Reword to name the actual condition
   ("behind HEAD, and build inputs changed in <range>").
3. **Preserve the safety guarantee.** A commit that *does* change `.rs`/`Cargo.*`
   must still hard-block a self-dogfood run — add a test for the mixed case
   (docs + a `.rs` change in the range → still `Stale`), alongside the
   docs-only-range → `Fresh` case.

## Scope notes / open questions

- **Uncommitted docs edits** already pass (porcelain arm filters them); this item
  is only the committed-range ancestry arm.
- Keep the existing dirty-flag / Indeterminate arms untouched (Pitfall 4:
  Indeterminate must never hard-block).
- Natural home: fits **Phase 21** (operator legibility — the tool should not
  block spuriously on its own workflow) or a fast-track standalone, since it
  taxes every dogfood run including Phase 21's own remaining stages.
- Related but distinct: `[[feedback-dogfood-rebuild-before-revalidate]]` is the
  *workaround*; this item is the *fix* that retires it. Linear: TBD.

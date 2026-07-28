---
phase: 25-end-to-end-dogfood-blockers
reviewed: 2026-07-28T02:03:59Z
depth: standard
files_reviewed: 10
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/tests/reap_strays_e2e.rs
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/version.rs
findings:
  critical: 3
  warning: 3
  info: 0
  total: 6
status: issues_found
---

# Phase 25: Code Review Report

**Reviewed:** 2026-07-28T02:03:59Z
**Depth:** standard
**Files Reviewed:** 10
**Status:** issues_found

## Summary

`agent.rs`'s new `terminate_and_verify`/`discover_stray_devflow_processes`/`is_same_process`
triad is sound: every signal is gated on a re-confirmed `(pid, starttime)` identity, the
SIGTERM→SIGKILL escalation is bounded and re-verified, and the structural stray matchers are
narrow (named argv positions, not a basename-prefix scan). `commands.rs`'s `--reap-strays` path
correctly re-confirms identity immediately before acting and never signals off an unfiltered
census. `staleness.rs`'s worktree-aware `execution_root` plumbing and its dirty/ancestry
decision table both check out against their fixtures.

The version-derivation work (25-01) and the new D-09 major-bump gate (25-06) are where this
review found real defects, and they compound: **the human gate this phase's own headline
feature relies on (`preflight_major_bump_check`) can be silently bypassed by approving an
unrelated, earlier-failing preflight check**, and separately, **it evaluates the wrong git ref
in the default (worktree) launch mode**, so a breaking-change commit can reach `VersionBump`'s
no-rollback batch without ever having been shown to a human. `version.rs`'s new commit-range
anchoring (`release_range_start`) also has an untested topology under which it silently
reverts to the exact naive-range bug it was built to fix. None of these are exercised by the
existing test suite (confirmed by reading the fixtures each area's own tests use).

## Critical Issues

### CR-01: A preflight gate approved for one failing check silently skips every check after it, including the major-bump gate

**File:** `crates/devflow-cli/src/preflight.rs:718-724` (`generic_preflight_checks`) and
`crates/devflow-cli/src/preflight.rs:796-806` (`run_preflight`'s `GateAction::Advance` arm)

**Issue:** `generic_preflight_checks` runs its three checks with `?`-short-circuiting:

```rust
fn generic_preflight_checks(project_root: &Path, state: &State) -> Result<(), String> {
    preflight_interactivity_check(project_root, state)?;
    preflight_gh_auth_check(state)?;
    preflight_major_bump_check(project_root, state)
}
```

If `preflight_gh_auth_check` fails (e.g. an expired `gh` token at Ship — both checks apply
only to `Stage::Ship`, so this is not a contrived combination), `preflight_major_bump_check`
**never runs at all** in that pass — it isn't skipped as "already adjudicated," it simply never
executes. `run_preflight` then writes a gate whose context names only the gh-auth failure and
waits for a human.

When the operator fixes `gh auth login` and answers `GateAction::Advance`, `run_preflight`'s
Advance arm calls `launch_stage_inner` directly (by design, per the D-18f doc comment, to skip
"the just-adjudicated check on the retry"). But `launch_stage_inner` never calls
`generic_preflight_checks` again — so the major-bump check, which was never evaluated in the
first place, is now permanently bypassed for this stage launch. The human approved a gh-auth
gate; they never saw the major-bump reason string (it doesn't exist yet), and the design's own
justification for skipping re-checks — "a gate approval cannot change what the commit range
classifies to" (`preflight.rs:565-570`) — does not apply, because the major-bump check was
never run once, let alone re-run.

`preflight_major_bump_check`'s own doc comment states "**This check cannot be auto-approved**"
and goes to lengths to prevent `state.yes_ship` from auto-approving it directly
(`run_preflight_major_bump_gate_not_auto_approved_by_yes_ship`) — but this composition bug
defeats that guarantee through a different, untested door: approving any *earlier* failing
check in the same chain. `run_preflight_major_bump_gates_and_never_ships_unattended` (the only
integration test exercising this check) scrubs `gh` off PATH specifically so gh-auth cannot
fail simultaneously — which means this exact composition was never tested.

**Fix:** Either (a) make `generic_preflight_checks` run every check and aggregate failures
rather than short-circuiting on the first one, so a human always sees the major-bump reason
whenever it applies, regardless of what else failed; or (b) special-case the `Advance` arm so
it re-runs `generic_preflight_checks` (not just the adapter hook) whenever the check that
originally failed was not `preflight_major_bump_check` itself. Option (a) is simpler and closes
the gap for any future check added to this chain, not just this one:

```rust
fn generic_preflight_checks(project_root: &Path, state: &State) -> Result<(), String> {
    let mut reasons = Vec::new();
    if let Err(r) = preflight_interactivity_check(project_root, state) { reasons.push(r); }
    if let Err(r) = preflight_gh_auth_check(state) { reasons.push(r); }
    if let Err(r) = preflight_major_bump_check(project_root, state) { reasons.push(r); }
    if reasons.is_empty() { Ok(()) } else { Err(reasons.join("; ")) }
}
```

### CR-02: The major-bump gate classifies the wrong git ref in the default worktree launch mode

**File:** `crates/devflow-cli/src/preflight.rs:614-668` (`preflight_major_bump_check`),
called from `crates/devflow-cli/src/pipeline_launch.rs:191-194` with `project_root =
state.project_root` (never the worktree)

**Issue:** `preflight_major_bump_check` calls `version::highest_semver_tag`,
`version::reachable_semver_baseline`, `version::release_range_start`, and
`version::classify_range_bump` all against `project_root` — every one of them shells `git`
with `current_dir(project_root)` and reasons about "HEAD" there.

`state.project_root` is always the **main checkout**, never the phase's worktree
(`commands::start` sets `state.worktree_path` separately, and `staleness.rs`'s own
`enforce_build_staleness` had to be taught this exact distinction in 18c — see its
`execution_root` parameter and doc comment). In the default worktree flow (`worktree = true`
unless `--no-worktree`), the phase's own commits live only on the worktree's feature branch;
the main checkout's `HEAD` stays on `develop` and is untouched by them until
`hooks_after_ship`'s `Merge` step runs `GitFlow::new(&ctx.project_root)` to actually merge the
feature branch in (`devflow-core/src/hooks.rs:143-...`, confirmed: `merge_feature` and the
later `version_bump` both operate on `ctx.project_root`, i.e. the same directory this preflight
check reads).

So at the moment the D-09 gate is supposed to fire — **before** `hooks_after_ship` runs — the
classification range (`baseline..HEAD` at `project_root`) reflects only what was already merged
into `develop` from *previous* ships. It cannot see the current phase's own feature-branch
commits at all, because they are not reachable from `project_root`'s `HEAD` until `Merge`
actually runs, which happens strictly *after* this preflight check. A `feat(scope)!: ...` or
`BREAKING CHANGE:` commit made during this phase's Code stage will therefore never be observed
by `preflight_major_bump_check`, and the very first time it is classified is inside
`VersionBump`, after `Merge` has already committed — exactly the "no rollback once Merge has
committed" scenario D-09 exists to prevent (see the module's own `hooks_after_ship` doc comment
in `devflow-core/src/hooks.rs:97-104`).

Confirmed no test exercises this: every `major_bump_*` test in `preflight.rs` builds its
fixture with `major_bump_fixture()`, a single directory with no worktree, where `state`'s
(implicit, unset) `worktree_path` is `None` and the fixture's own checked-out branch *is* where
the commits land — the exact case (`--no-worktree`) where this bug does not manifest, since
there `state.project_root`'s checkout literally *is* the feature branch.

**Fix:** Thread the execution root the same way `staleness.rs::enforce_build_staleness` already
does — evaluate against `state.worktree_path.as_deref().unwrap_or(project_root)`, not
`project_root` unconditionally:

```rust
fn preflight_major_bump_check(project_root: &Path, state: &State) -> Result<(), String> {
    if !major_bump_check_applies(state.stage) {
        return Ok(());
    }
    let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
    let highest = version::highest_semver_tag(execution_root).map_err(|err| err.to_string())?;
    let baseline = version::reachable_semver_baseline(execution_root).map_err(|err| err.to_string())?;
    // ...and release_range_start / classify_range_bump / breaking_commit_subjects likewise.
```

Add a regression test that builds a real worktree fixture (mirroring
`staleness.rs::worktree_staleness_fixture`) with a `feat!:` commit only on the worktree branch,
and asserts `preflight_major_bump_check(&project_root, &state_with_worktree)` still fires.

### CR-03: `release_range_start`'s anchor heuristic silently reverts to the naive, over-broad range when a commit lands directly on the release branch between the tag and the next sync-back merge

**File:** `crates/devflow-core/src/version.rs:258-313` (`release_range_start`)

**Issue:** The function's own doc comment states the invariant it depends on: the earliest
commit (`C1`) on the ancestry path from the baseline tag to `HEAD` is assumed to be the
sync-merge-back commit itself, so that checking whether the tag is an ancestor of `C1`'s first
parent correctly distinguishes "ordinary non-squashed range" (anchor at the tag) from "squashed
release, entered via a sync merge" (anchor at `C1`).

That assumption breaks when **any commit lands directly on the release branch (trunk) between
the tag and the sync-merge-back** — e.g. a hotfix pushed straight to `main`, or the tag simply
not sitting on the literal last pre-sync commit. In that topology, `--ancestry-path` still
picks up the intervening trunk commit `X` (it is both a descendant of the tag and an ancestor
of `HEAD` via the later merge's second parent), and `X` — being chronologically earlier than
the sync-merge commit — becomes `C1` instead of the merge commit. `X`'s first parent is then
the tag commit itself, and `git merge-base --is-ancestor <tag> <tag>` returns **true** (a
commit is its own ancestor), so `tag_is_ancestor_of_first_parent` is (incorrectly) true. The
function concludes "the tag already sat on this mainline" and returns `baseline_tag` — the
literal `tag..HEAD` range — reintroducing precisely the "677 non-merge commits, 62 `feat`"
pathology the function's own doc comment names as the reason `--ancestry-path`-based anchoring
exists at all (the develop-side pre-squash commits leak back in via `M`'s first-parent chain).

Both existing regression tests (`squash_sync_topology_classifies_only_post_merge_commits`,
`two_squash_sync_cycles_anchor_to_the_second_merge_only`) construct the sync-merge as the
*first* commit after the tag with nothing intervening, so `C1` is always the merge commit in
those fixtures — this topology is not covered.

**Fix:** Don't rely on "is the tag an ancestor of `C1`'s first parent" to distinguish the two
cases — walk the whole `--ancestry-path` list and anchor at the **last** merge commit in it
(the commit with more than one parent, closest to `HEAD`), falling back to `baseline_tag` only
when the ancestry path contains no merge commit at all:

```rust
// Sketch: find the last (closest-to-HEAD) merge commit in the ancestry path,
// not just inspect C1's first parent.
for candidate in ancestry_path.iter().rev() {
    if commit_has_multiple_parents(project_root, candidate) {
        return Ok(candidate.clone());
    }
}
Ok(baseline_tag.to_string())
```
Add a fixture with a direct trunk commit between the tag and the sync-merge-back, asserting the
range still excludes the pre-release develop history.

## Warnings

### WR-01: A merge-base failure (e.g. shallow clone) is classified `Diverged` and hard-refuses, contradicting the module's own fail-open-where-blind contract

**File:** `crates/devflow-cli/src/preflight.rs:355-386` (`base_ref_currency`)

**Issue:** `is_ancestor` maps any spawn error or non-zero/non-{0,1} exit from
`git merge-base --is-ancestor` to `false` (`unwrap_or(false)`). If `merge-base` fails for a
reason other than "genuinely not an ancestor" — e.g. a shallow clone (`git clone --depth=1`,
common in CI) that lacks the commits needed to compute a merge base — **both** directions
report `false`, and the match falls into `(false, false) => BaseRefCurrency::Diverged`, which
`ensure_base_ref_current` treats as an unconditional hard refusal ("neither is an ancestor of
the other"). Every other indeterminate case in this same function (`ref_exists` false, a failed
`git fetch`) is documented and handled as fail-open (`Undeterminable`, proceed with a warning).
A shallow-clone CI runner would instead see `devflow start` hard-refuse with a "diverged" error
that is actually just "couldn't compute a merge-base," directly contradicting the "fail-open
where it cannot see" contract this same function's doc comment promises.

**Fix:** Distinguish "both directions genuinely returned exit 1" (real divergence) from "either
`merge-base` invocation failed to spawn or exited with something other than 0/1" (indeterminate)
and route the latter to `BaseRefCurrency::Undeterminable` instead of `Diverged`.

### WR-02: A reachable baseline tag with prerelease metadata skips the stable release version entirely on the next bump

**File:** `crates/devflow-core/src/version.rs:430-466` (`compute_version`),
`crates/devflow-core/src/version.rs:412-422` (`apply_bump`)

**Issue:** `highest_semver_tag`/`reachable_semver_baseline` accept any tag that parses as
`semver::Version` after stripping `v`, including prerelease tags (`v2.0.0-rc.1`). If such a tag
is the highest reachable baseline, `compute_version` uses its full `major.minor.patch` triple
(ignoring the prerelease field) as `baseline_version`. `apply_bump`'s `Patch`/`None` arm then
produces `major.minor.patch + 1` — i.e. from baseline `2.0.0-rc.1` a `fix:` commit computes
`v2.0.1`, and the stable `v2.0.0` release this prerelease was presumably heading toward is
never produced by this algorithm at all. A `Minor`/`Major` bump has the same effect one level
up. The phase context specifically flags "prerelease and non-semver tag handling" for scrutiny;
this repository may not currently cut prerelease tags, but nothing in `compute_version` refuses
or special-cases one, so the first prerelease tag anyone creates silently skips its own stable
release on the very next ship.

**Fix:** Either strip the prerelease/build metadata from the baseline before computing
`baseline_version` and treat a prerelease tag's underlying `major.minor.patch` as already
"spent" (current behavior, but documented as intentional), or refuse `compute_version` (mirroring
D-10's `UnreachableBaseline` refusal) when the baseline carries a prerelease component, forcing
a human decision the same way an unreachable tag does today. Either is acceptable; the current
silent-skip is not.

### WR-03: `ensure_base_ref_current`'s fast-forward assumes `develop` can only ever be checked out at `project_root`

**File:** `crates/devflow-cli/src/preflight.rs:445-479` (`ensure_base_ref_current`)

**Issue:** Before fast-forwarding `refs/heads/{base}` via `git update-ref`, the function checks
only whether `project_root`'s own `HEAD` (via `git symbolic-ref --short HEAD`) equals `base`.
`git update-ref` (unlike `git branch -f`) does **not** refuse to move a branch ref that is
checked out in some *other* linked worktree — if `develop` were ever checked out in a linked
worktree other than `project_root` (this project's own conventions never do this today: phase
worktrees are feature branches, and `reference()` always uses `add_detached`), this code would
silently advance that worktree's branch ref out from under its checked-out files, without
touching its index or working tree, producing a corrupted-looking worktree the next time
anything runs `git status` there. This is a latent risk contingent on an operator manually
checking out `develop` in a linked worktree, not a currently-reachable path through this
project's own commands — recorded as a WARNING rather than a BLOCKER for that reason.

**Fix:** Check every worktree's checked-out branch (`git worktree list --porcelain`), not just
`project_root`'s own `HEAD`, before calling `update-ref`.

---

_Reviewed: 2026-07-28T02:03:59Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

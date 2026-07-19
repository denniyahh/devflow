---
phase: 17-pipeline-dogfood-followup
reviewed: 2026-07-19T00:00:00Z
round: 3
depth: standard
files_reviewed: 15
files_reviewed_list:
  - crates/devflow-cli/build.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/version.rs
findings:
  critical: 1
  warning: 0
  info: 0
  total: 1
status: issues_found
ship_gate: BLOCKED
---

# Phase 17: Code Review Report — Round 3

**Reviewed:** 2026-07-19 (round 3)
**Depth:** standard (per-file, focused on the plan-17-12 delta since round 2)
**Files Reviewed:** 15
**Status:** issues_found — **1 new Critical** found in code landed since round 2's close, and
resolved in this same run (`39e2e65`); round 2's Warnings remain open
**Ship gate:** BLOCKED — on round 2's still-open Warnings (WR-01, WR-02, WR-06 through WR-11)
only. No open Critical remains.

## Summary

Round 2 closed its own CR-01(R2)/WR-04 (the changelog-outlives-its-tag defect) via plan `17-12`,
which reordered `hooks_after_ship()` to `Merge → VersionBump → ChangelogAppend → BranchCleanup`,
added `version::read_version` (a git-free reader so `ChangelogAppend` can never see a tag
`VersionBump` just cut and recompute a higher version), and added a new `GitFlow::commit_path`
scoped-commit helper so both `changelog_append` and `version_bump` commit their own writes instead
of leaving them dirty for a later hook to lose.

Per this round's brief, `git.rs` and `main.rs` were not in plan `17-12`'s declared
`files_modified` — `GitFlow::commit_path` (`git.rs`) is a genuine executor deviation, and it is
where this round's finding lives.

Confirmed via `git diff a3a1067~1..b81ec7d` (the exact `17-12` commit range) that no other file in
this round's scope changed beyond `git.rs`, `hooks.rs`, `version.rs`, and doc/test-only edits in
`main.rs`. The other 11 files in scope (`agent_result.rs`, `agents/mod.rs`, `lib.rs`, `mode.rs`,
`outcome_policy.rs`, `ship.rs`, `state.rs`, `build.rs`, `build_provenance.rs`, `log_format_env.rs`,
`devflow-help.txt`) are byte-identical to their state at round 2's review commit (`92581fa`) —
verified with `git diff --stat 92581fa..HEAD` over that file list, which returns no output. Round
2's dispositions for those files stand unchanged and are not re-litigated here.

The batch's fail-fast ordering claim was independently re-traced and holds:
`run_checkout_hooks` (`main.rs:1700-1772`) breaks out of the loop on the first hook error only when
`terminal_batch` is true (`main.rs:1767-1769`), and `terminal_batch` is computed by slice-equality
against `hooks::hooks_after_ship()` (`main.rs:1741`) — unchanged by this plan, confirmed still
correct. A failed `Merge` or `VersionBump` genuinely stops `ChangelogAppend` and `BranchCleanup`
from running.

---

## Critical Issues

### CR-01 (R3): `GitFlow::commit_path` does not actually scope the commit to the given path — it commits the entire index

**File:** `crates/devflow-core/src/git.rs:325-334` (new in `17-12`)
**Callers:** `crates/devflow-core/src/hooks.rs:210-214` (`changelog_append`),
`crates/devflow-core/src/hooks.rs:229-232` (`version_bump`)
**Insufficient test:** `crates/devflow-core/src/git.rs:646-678`
(`commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted`)

**Issue.** `commit_path`'s own doc comment states its purpose is to scope a commit to one path "for
hooks that must not sweep in unrelated dirty state left by other hooks or the workflow" — this is
the exact invariant Round 2's WR-04 fix depends on. The implementation:

```rust
pub fn commit_path(&self, relative_path: &str, message: &str) -> Result<(), GitError> {
    debug!("committing {relative_path}: {message}");
    self.git(["add", relative_path])?;
    // --allow-empty so we don't fail when the path had no changes.
    match self.git_raw(&["commit", "--allow-empty", "-m", message]) {
        Ok(()) => Ok(()),
        Err(GitError::Command(ref msg)) if msg.contains("nothing to commit") => Ok(()),
        Err(e) => Err(e),
    }
}
```

`git add relative_path` stages exactly `relative_path` — but the subsequent `git commit` call
carries **no pathspec**, so it commits the *entire index* as it stands at that moment, not just
what this call just staged. If anything else is already staged (but not yet committed) in the
repository when `commit_path` runs, it is silently swept into this commit too.

**Reproduced directly** (not simulated through the Rust code — raw `git`, mirroring exactly what
`commit_path` executes):

```
$ git add unrelated.txt        # simulates pre-existing staged content
$ git add CHANGELOG.md         # commit_path's own `git add relative_path`
$ git commit -m "docs: add changelog entry"   # commit_path's own commit call — no pathspec
$ git log -1 --name-only --pretty=format:
CHANGELOG.md
unrelated.txt                  # ← swept in, contradicting the doc comment's claim
```

**The existing regression test does not catch this.** `commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted`
(`git.rs:646-678`) only exercises an **untracked, never-staged** file (`?? unrelated.txt` in
porcelain output) — a file that was never `git add`ed is trivially excluded from any commit,
staged or not, and proves nothing about the property the function's doc comment actually claims.
It does not cover the case that matters: a file that is *already staged* (`A  unrelated.txt`) when
`commit_path` is called. Confirmed with the fix applied that the correct behavior is achievable and
distinguishable from the current one:

```
$ git add unrelated.txt
$ git add CHANGELOG.md
$ git commit -m "docs: add changelog entry" -- CHANGELOG.md   # pathspec-scoped commit
$ git log -1 --name-only --pretty=format:
CHANGELOG.md                   # only this
$ git status --porcelain
A  unrelated.txt               # correctly left staged, untouched
```

**Why this matters in production, not just in a contrived test.** `commit_path` runs against
`project_root` — the developer's real primary checkout, not a scratch worktree (`hook_context_root`,
`main.rs:1688-1698`, routes the terminal batch there deliberately). Nothing in `run_checkout_hooks`
or the two callers verifies the index is clean before calling `commit_path`. If an operator has any
staged-but-uncommitted edits in that checkout when `devflow ship`'s terminal batch runs — plausible,
since DevFlow is documented to drive a developer's live repository, not an isolated sandbox — either
the `chore: bump version to {version}` commit or the `docs: add changelog entry for {version}`
commit silently absorbs that unrelated content. This is precisely the failure class Round 2's WR-04
existed to close (a release-record commit whose actual contents don't match what its message and
the surrounding tooling claim), reintroduced by the very helper written to prevent it.

**Fix:** add a pathspec to the commit step, not just the `add` step — `git add` is still required
to pick up brand-new/untracked files (a bare pathspec-scoped `git commit -- path` errors with
"pathspec did not match any file(s) known to git" on a file never previously tracked or staged,
confirmed by direct reproduction), but the commit itself must also be scoped:

```rust
pub fn commit_path(&self, relative_path: &str, message: &str) -> Result<(), GitError> {
    debug!("committing {relative_path}: {message}");
    self.git(["add", relative_path])?;
    match self.git_raw(&["commit", "--allow-empty", "-m", message, "--", relative_path]) {
        Ok(()) => Ok(()),
        Err(GitError::Command(ref msg)) if msg.contains("nothing to commit") => Ok(()),
        Err(e) => Err(e),
    }
}
```

Verified this fix directly (raw `git`, not yet applied to the Rust source) round-trips correctly
for both the pre-staged-unrelated-file case above and a brand-new/never-tracked target path. Also
strengthen `commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted` (or add a sibling
test) to stage an unrelated **tracked, modified** file (`git add unrelated.txt` before calling
`commit_path`, not just leave it untracked) and assert it survives in `git status --porcelain` as
still staged (`A  unrelated.txt`) — the current test's untracked-file case cannot regress this bug
and must not be mistaken for coverage of it.

**Disposition: RESOLVED** — `39e2e65`, applied by the `/gsd-execute-phase 17 --gaps-only`
code-review gate in the same run that raised it.

- `git.rs:334` now runs `["commit", "--allow-empty", "-m", message, "--", relative_path]`; the
  preceding `git add` is retained, with a comment recording why (a pathspec-only commit errors on
  a path git has never seen) so a future reader does not "simplify" it back out.
- `commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted` (`git.rs:646`) now
  `git add`s `unrelated.txt` **before** calling `commit_path` and asserts `A  unrelated.txt`
  survives, replacing the untracked-file assertion that could not fail on this bug.
- Confirmed RED→GREEN, not assumed: with the strengthened test in place and only the commit
  invocation reverted to its pre-fix form, the test FAILS (`0 passed; 1 failed`); with the
  pathspec restored it passes (`1 passed`).
- Full gates re-run after the fix: `cargo test --workspace` 376 passed / 0 failed across 10
  targets, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

---

## Verified Unchanged Since Round 2 (no re-review performed)

- `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/agents/mod.rs`,
  `crates/devflow-core/src/lib.rs`, `crates/devflow-core/src/mode.rs`,
  `crates/devflow-core/src/outcome_policy.rs`, `crates/devflow-core/src/ship.rs`,
  `crates/devflow-core/src/state.rs`, `crates/devflow-cli/build.rs`,
  `crates/devflow-cli/tests/build_provenance.rs`, `crates/devflow-cli/tests/log_format_env.rs`,
  `crates/devflow-cli/tests/snapshots/devflow-help.txt` — `git diff --stat 92581fa..HEAD` over this
  list returns no output. Round 2's dispositions for these files (including all open Warnings and
  Infos attributed to them) stand as recorded below and are not re-opened.
- `crates/devflow-core/src/hooks.rs` `hooks_for_transition`/`hooks_after_ship` ordering and the
  terminal-batch fail-fast in `main.rs:1700-1772` were re-traced fresh this round (not merely
  carried forward) and confirmed correct as described in Round 2's WR-04 resolution note.

---

## Ship Gate

**BLOCKED** — but no longer on a Critical. CR-01 (R3) was raised and resolved within this same
run (`39e2e65`); no open Critical remains in round 3. The gate stays BLOCKED solely on round 2's
still-open Warnings (WR-01, WR-02, WR-06 through WR-11, in the Round 2 section below), which are
unchanged and remain an operator call — per this file's own convention, clearing them is not
something an automated gap-closure run decides on its own.

---
---

# Appendix: Round 2 (2026-07-19)

Preserved verbatim (including Round 2's own embedded Round 1 appendix). Round 2 frontmatter
recorded: 1 Critical, 15 Warning, 11 Info, 27 total, `status: issues_found`, `ship_gate: BLOCKED`,
16 files reviewed.

# Phase 17: Code Review Report — Round 2

**Reviewed:** 2026-07-19 (round 2)
**Depth:** deep (5 parallel angles, merged and deduplicated)
**Files Reviewed:** 16
**Status:** issues_found — **0 Critical OPEN** (CR-01's root cause, WR-04, resolved by `17-12` —
see the CR-01 and WR-04 entries below)
**Ship gate:** BLOCKED (unchanged — Warnings WR-01, WR-02, WR-06 through WR-11 remain open;
flipping `ship_gate` for those is an operator call, not this plan's scope)

Round 1 of this review is preserved verbatim in the appendix below. This round re-ran the same
five angles against the current branch state (63 commits ahead of `develop`, merge-base
`a2c314f`) plus the uncommitted working tree.

> **Audit-fix pass 2026-07-19 (`/gsd-audit-fix 17`).** Seven findings resolved: **CR-01**
> (symptom only — see its entry; WR-04's root cause remains open, so recurrence is expected),
> **WR-03**, **WR-05** (`1de9e3c`, both mutation-verified), and **WR-12/13/14/15** (`ebc343f`).
> Eleven Warnings remain open and were classified manual-only: WR-01, WR-02, WR-04, WR-06, WR-07,
> WR-08, WR-09, WR-10, WR-11 — each changes a behavioral contract or edits a point-in-time plan
> record, and needs a design call rather than a mechanical fix. All eleven Infos are below the
> pass's severity threshold and untouched. **`ship_gate` is deliberately left `BLOCKED`**: the
> Critical is closed in the tree, but WR-04 guarantees the next dogfood run re-emits it, and that
> is an operator call to make, not an autonomous one. Frontmatter counters are found-counts and
> are not decremented, per this file's existing convention.

## Summary

Toolchain is green and the phase's headline work is genuine. Independently reproduced:

- `cargo test --workspace` → **367 passed / 0 failed / 0 ignored**
- `cargo clippy --workspace --all-targets -- -D warnings` → **exit 0**
- `cargo fmt --check` → **exit 0**
- GAP-3 closure is real (`build.rs:43` always-rerun sentinel; `DEVFLOW_BUILD_TIMESTAMP` survives
  only in comments).
- GAP-4 closure is real (`build_provenance.rs:23` now asserts SHA shape; the discarded
  `let _ = commit.len();` is gone).
- Round 1's CR-02 regression test does real work — it snapshots the worktree, runs
  `git pack-refs --all`, and asserts the build script's *cached* output flips `false`→`true`.

One Critical is open. It is a **regression of Round 1's CR-01**, and it regenerated precisely
because Round 1's WR-04 — the root cause — was left open.

---

## Critical Issues

### CR-01 (R2): `CHANGELOG.md` asserts a release (1.4.88) that does not exist — recurrence of Round 1's CR-01

**File:** `CHANGELOG.md:3-5` (uncommitted working-tree change)
**Angles:** doc-accuracy, external-state (independently found by both; verified a third time by
the orchestrator)

The working tree adds:

```markdown
## 1.4.88 — 2026-07-19

- Released phase via DevFlow.
```

Verified actual state:

| Claim | Reality | Command |
|---|---|---|
| Release 1.4.88 exists | No such tag; latest is `v1.3.69` | `git tag -l` → `v1.0.1 v1.2.0 v1.3.0 v1.3.69` |
| ... | No such GitHub release | `gh release list` → newest is `v1.3.69 Latest` |
| ... | Workspace still at 1.3.69 | `Cargo.toml:9` → `version = "1.3.69"` |
| ... | Branch never pushed | `origin` has only `develop`/`main`; no `feature/phase-17` |
| ... | No PR opened | `gh pr list --state all` tops out at PR #10 |

**Why this is Critical.** Every prior heading in this file corresponds to a real tag and a real
GitHub release — `1.3.69` included. The file's established convention is *heading == released
version*, so a human reader or an automated release-notes consumer will reasonably treat 1.4.88
as shipped. It is not, by five independent measures. Round 1 classified the byte-identical
defect at version `1.4.26` as Critical and recorded it fixed via `5431f9e`; Round 1's own framing
(`17-REVIEW.md:70`) calls this class "a false release claim that a merge would propagate into the
project changelog."

**Why it came back.** Round 1's **WR-04** documented the root cause and left it open:
`changelog_append()` (`hooks.rs:173-183`) writes the entry without committing, and
`version_bump()` (`hooks.rs:185-198`) — the only hook that creates a tag — never ran. So the
hook re-emits a false heading on every dogfood run. `5431f9e` deleted the symptom, not the cause.
`17-10-SUMMARY.md:104` independently flags the same placeholder body as a content-quality wart.

**Fix (either):**
1. Drop the heading from the working tree before shipping (what `5431f9e` did — symptom only,
   expect a Round 3 recurrence), **or**
2. Close WR-04: make `changelog_append` and `version_bump` atomic, so an entry cannot outlive
   the tag it names. This is the durable fix.

**RESOLVED (symptom only) — `/gsd-audit-fix 17`.** Option 1 applied: the four-line `1.4.88` block
was dropped from the working tree, which now matches `HEAD` exactly (`git status --porcelain`
empty) — no commit was needed, the change was never committed. Re-verified before removal that the
claim was false by all five measures this entry lists (`git tag -l` tops out at `v1.3.69`,
`Cargo.toml:9` is `1.3.69`).

**This is explicitly the symptom fix, and this entry's own prediction still stands.** WR-04 — the
root cause, `changelog_append()` writing an entry that `version_bump()` never backs with a tag —
was classified manual-only by that pass (it changes hook atomicity semantics and needs a design
call). Until WR-04 closes, the next dogfood run re-emits a false heading and a Round 3 will find
this again.

**WR-04 (root cause) RESOLVED — `17-12`.** `changelog_append` is now ordered strictly after
`version_bump` in `hooks_after_ship()` (was fired independently at the Validate→Ship transition,
before any tag existed), and reads the version via a new `version::read_version` — never
`compute_version`, which would see the tag `version_bump` just cut and derive a version one
higher than the tag actually names. `changelog_append` also now commits its own write (Round 2's
WR-04 facet below), scoped to `CHANGELOG.md` via a new `GitFlow::commit_path`, and a failed commit
propagates as an error so the terminal batch's fail-fast stops `BranchCleanup` from running against
an uncommitted entry. `version_bump` had the identical "write without commit" defect on its own
version-file write — surfaced only once the regression test asserting a clean tree after the full
batch made it visible — and is fixed the same way. Regression test
(`hooks.rs::after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean`) drives the
full `hooks_after_ship()` batch and asserts three-way agreement (changelog heading version == tag
== version file version) plus a clean working tree with `CHANGELOG.md` in a commit. `ChangelogAppend`
deliberately moved to the terminal batch and now targets `project_root` via `hook_context_root`
(a release record belongs on the base branch alongside the tag), while `DocsUpdate` stays in the
Validate→Ship batch targeting the worktree (17-10 unchanged) — a future reader should not restore
17-10's worktree targeting to `ChangelogAppend`. The generated entry body is still the placeholder
"Released phase via DevFlow" — a content-quality defect (also flagged `17-10-SUMMARY.md:104`)
deliberately left out of `17-12`'s scope. No open Critical or WR-04 entry remains in this file.

**Round 3 note:** `17-12`'s new `GitFlow::commit_path` helper (the mechanism this resolution
depends on for "changelog_append also now commits its own write") does not actually scope its
commit to the given path — see Round 3's CR-01 above. WR-04's ordering/read-back defects are
genuinely closed; the commit-scoping guarantee is not.

---

## Warnings

### WR-01: `run_preflight` recurses into `launch_stage` with no depth or attempt bound

**File:** `crates/devflow-cli/src/main.rs:796-822`, re-entered at `:1133`
**Angle:** generalist. **Severity note:** Round 1 rated this **Info** (IN-05); this round raises
it to Warning. It is not raised to Critical because every cycle requires an external actor to
re-approve the gate — a human hits an unbreakable prompt loop, not a spontaneous crash.

`run_preflight` → `run_gate` → `GateAction::Advance`/`LoopBack` → `launch_stage(state, None, None)`
→ `run_preflight`, with the *same* `state`. Nothing increments `consecutive_failures` or
`infra_failures`, so neither `MAX_CONSECUTIVE_FAILURES` nor `MAX_INFRA_FAILURES` applies, and
there is no depth counter. Both shipped generic checks are deterministic
(`preflight_interactivity_check`, `preflight_gh_auth_check`), so the retry re-fails identically.

The authors were aware — the `FailOnceAdapter` doc comment states that an unconditionally-failing
adapter "would make a recursive `launch_stage` retry fail its OWN preflight check too, recursing
into a second gate this test never seeds a response for." The hazard was worked around in the
fixture rather than bounded in the code.

**Failure scenario:** phase at `Stage::Ship`, `gh` installed but logged out. Under an unattended
auto-approving responder (the D-15 cron mode this gate exists for), each cycle adds stack frames,
re-fires a desktop notification, and appends to `events.jsonl` while `advance` holds the
per-phase lock — terminating in stack exhaustion.

**Fix:** increment `state.infra_failures` in the failure arm and abort past `MAX_INFRA_FAILURES`.
`transition()` already resets the counter, so a self-clearing preflight does not leak count.

### WR-02: total loss of git provenance silently downgrades the staleness hard-block, undetectably

**File:** `crates/devflow-cli/build.rs:51-62`; guard at `crates/devflow-cli/tests/build_provenance.rs:32-36`
**Angle:** ci-build

`run_git` returns `None` on a missing binary, non-git dir, *or* non-zero exit, and
`commit.unwrap_or_default()` turns that into an empty string. Downstream: empty commit →
`embedded_commit_is_stale` = `Indeterminate` → `combined_staleness:976-986` propagates it →
`staleness_outcome:1045` maps `(_, Indeterminate)` → `Warn`. The D-18 hard block at `:1042` can
never fire.

The GAP-4 guard accepts empty by design (`commit.is_empty() || ...`), so it cannot distinguish
"provenance working" from "provenance entirely dead." This is not the GAP-4 defect — that is
genuinely closed — it is the residual hole the new assertion still leaves.

**Fix:** keep D-20 empty-tolerance for crates.io builds, but add a git-conditional test: when
`git rev-parse HEAD` succeeds from `CARGO_MANIFEST_DIR`, assert `DEVFLOW_BUILD_COMMIT` is
non-empty and equal to it.

### WR-03: `tree_has_modified_build_inputs` misses staged edits

**File:** `crates/devflow-cli/src/main.rs:934-940` · **Angle:** generalist

Gates on `git status --porcelain` being non-empty, then enumerates with `git ls-files -m`, which
reports worktree-vs-index only. Verified empirically: after `git add src/lib.rs`,
`git status --porcelain` prints `M  src/lib.rs` while `git ls-files -m` prints nothing. A staged
but uncommitted source edit yields `Some(false)` → falls through to ancestry `Fresh` → `Ok`. The
stale binary drives its own workspace silently — the exact Phase 16 false-evidence class this
gate exists to catch.

**Fix:** derive the modified path list from `git status --porcelain` itself (handling the
`XY<space>path` shape and ` -> ` rename entries).

**RESOLVED — `/gsd-audit-fix 17` (`1de9e3c`).** Fix applied as prescribed: a new
`porcelain_tracked_path()` helper parses the `XY<space>PATH` shape, takes the destination of
` -> ` rename entries, strips git's quoting, and returns `None` for `??` so untracked files stay
excluded exactly as under `ls-files -m`. The claim was reproduced independently first (`git add`
a source file ⇒ porcelain `M  lib.rs`, `ls-files -m` empty). Coverage added to
`dirty_flag_arm_ignores_non_build_files_but_still_flags_sources`, which now stages the same edit
and asserts it still reads `Some(true)` / `Staleness::Stale`. Mutation-verified: restoring
`ls-files -m` fails it at `left: Some(false) / right: Some(true)`.

### WR-04: `build_dirty` and the live check disagree on what "dirty" means

**File:** `crates/devflow-cli/build.rs:63` vs `crates/devflow-cli/src/main.rs:982` · **Angle:** generalist

`build.rs` computes dirty from `git status --porcelain` (counts **untracked** files); the runtime
arm uses `git ls-files -m` filtered by `affects_compiled_binary` (tracked, build-affecting only).
So the `Some(true) if build_dirty => Indeterminate` arm fires on dirt unrelated to compiled
inputs. An untracked `notes.txt` at the workspace root makes every subsequent genuinely-stale
build downgrade from Block to a printed Warn. The documented "same dirt vs more dirt" tradeoff is
supposed to be rare; this makes it the common case.

**Fix:** have `build.rs` use `--untracked-files=no` and the same path predicate.

### WR-05: `is_self_dogfood_workspace` matches `default-members`

**File:** `crates/devflow-cli/src/main.rs:1002` · **Angle:** generalist

`contents.find("members")` takes the first substring occurrence anywhere in the file.
`"default-members"` contains `"members"`. If the root `Cargo.toml` gains a `default-members` key
above `members`, the scanned array is wrong, `has_member("crates/devflow-core")` is false, and the
self-dogfood hard block silently degrades to `Warn` with no test failure. Existing tests only
cover fixtures where `members = [...]` is the first hit.

**Fix:** anchor on the assignment (reject matches preceded by an identifier character) rather than
a bare substring search.

**RESOLVED — `/gsd-audit-fix 17` (`1de9e3c`).** Fix applied as prescribed: `match_indices("members")`
with the first hit not preceded by an identifier character (alphanumeric, `_`, or `-`). New test
`is_self_dogfood_workspace_anchors_on_members_not_default_members` uses a manifest with
`default-members` above `members`; mutation-verified — restoring `contents.find("members")` fails
it on its named assertion.

### WR-06: `start` prints a success banner and exits 0 after a preflight abort

**File:** `crates/devflow-cli/src/main.rs:629-640` · **Angle:** generalist

`GateAction::Abort` calls `abort(...)` (clearing state) and returns `Ok(false)`; `launch_stage`
returns `Ok(())`, so `start`'s error branch is skipped and it prints
`"started phase N ... monitor will auto-advance"` with exit code 0. Any wrapper keying off exit
status believes the phase is running; `devflow logs -f --phase N` finds nothing.

**Fix:** propagate the did-not-launch signal out of `launch_stage`, or have the `Abort` arm return
`CliError::Message` after `abort()`.

### WR-07: `resume` never deletes the cron-instructions record it was invoked by

**File:** `crates/devflow-cli/src/main.rs:1200-1213` · **Angle:** generalist

Both other consumers delete the record after acting (`:2242`, `:2342`); `resume()` does not.
`cron_instruction_hints` (`:2844-2855`) then prints "Cron instruction pending (phase N)"
indefinitely, and an operator following the hint re-installs a Hermes job targeting a phase whose
state file no longer exists. `recover::clean_stale` only GCs the record once state is already
gone, so the stale hint survives the rest of the run.

### WR-08: an unschedulable cron record is still persisted and advertised

**File:** `crates/devflow-cli/src/main.rs:1424`, `:1435-1442` (same pattern at `:2357-2367`) · **Angle:** generalist

`write_cron_instructions` is called unconditionally, *then* the empty-schedule check diverts to
`gate_or_abort_infra` — leaving a record with `"schedule": ""` on disk. The in-code comment at
`:1428-1434` states an empty cron expression "would degrade into an every-minute resume," yet the
file containing exactly that is persisted and surfaced in `devflow status`. The test
`rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently` asserts the record
loads successfully with an empty schedule — locking in the hazard rather than preventing it.

**Fix:** build the instructions, check the schedule, and only write when non-empty.

### WR-09: a phase gate command matches zero tests and exits 0

**File:** `.planning/phases/17-pipeline-dogfood-followup/17-03-PLAN.md:119,130,156` · **Angle:** ci-build

`cargo test -p devflow-core evaluate_layer0` is used as an `<automated>` gate. Actual output:

```
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 276 filtered out
EXIT=0
```

The tests are named `layer0_affirmative_success_*`, not `evaluate_layer0_*`. This is a recurrence
of the known `cargo test <name>` false-green class. **Mitigating:** the next link in the same `&&`
chain, `cargo test -p devflow-core agent_result::`, matches 71 tests including both `layer0_*`
tests — so the branches *are* exercised. The damage is a false coverage claim, not an untested
branch. All other phase gate filters verified real (`advance` → 7, `ship::` → 16,
`agent_result::` → 71, `evaluate_layer3` → 2, two named tests → 1 each).

**Fix:** correct the filter to `layer0_`, and assert on a non-zero pass count rather than exit
status to kill the class.

### WR-10: the branch tip claims a docs regeneration that changed nothing

**Commit:** `8140bea` "docs: update generated docs" · **Angle:** external-state

`git diff-tree --no-commit-id --name-only -r 8140bea` returns nothing — an empty commit.
`17-10-SUMMARY.md:113-115` already documents this as known product behavior ("a hook that commits
nothing should probably not commit at all"), so it is a recorded wart rather than a fabrication —
but `git log` still reads as if generated docs moved.

### WR-11: `.devflow/` is gitignored pattern-by-pattern, so a future runtime file kind is committable by default

**File:** `.gitignore:23-34`; guard at `crates/devflow-cli/tests/gitignore_coverage.rs:28-43` · **Angle:** security

Twelve specific patterns are enumerated with no `.devflow/` catch-all, and the guard mirrors that
list exactly. It catches *removal* of an existing pattern (its WR-07/CR-01 purpose) but cannot
catch *omission*. A later phase writing `.devflow/transcript-01.jsonl` or `.devflow/agent-session.json`
matches no pattern, is absent from `RUNTIME_PATHS`, keeps the guard green, and gets committed by
`git add -A`. **No leak exists today** — Phase 17 adds no new file kind.

**Fix:** add a `.devflow/` catch-all with `!` re-includes, making the default deny rather than allow.

### WR-12: `17-VERIFICATION.md` describes a test that no longer exists

**File:** `.planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md:129` · **Angle:** doc-accuracy

Claims `build_commit_is_accessible_and_does_not_panic` "still asserts nothing
(`let _ = commit.len();`) ... explicitly left unfixed." That symbol does not exist:
`build_provenance.rs:23` is `build_commit_is_empty_or_a_full_hex_sha` and it does assert. Closed
by `46058a7`, which precedes `1070df0` (when VERIFICATION.md was last written). Understates
completeness and points readers at a nonexistent symbol.

### WR-13: `17-VERIFICATION.md` claims `nyquist_compliant: false`; it is `true`

**File:** `17-VERIFICATION.md:130` vs `17-VALIDATION.md:7` · **Angle:** doc-accuracy

VERIFICATION.md asks for a future pass to "flip `nyquist_compliant` back to `true`" — already done
by `3d6e6a6` and re-confirmed by `41345fc`. An auditor would open a re-validation task that is
complete. Same root cause as WR-12: the anti-pattern table was not refreshed in `1070df0`.

### WR-14: `ROADMAP.md` promises a build timestamp that 17-11 removed

**File:** `.planning/ROADMAP.md:211` · **Angle:** doc-accuracy

Says 17-02 delivers provenance "(commit/dirty/**timestamp**)". `DEVFLOW_BUILD_TIMESTAMP` has zero
emission or consumption — all three remaining hits are comments. Directly contradicts
VERIFICATION.md Truth 13, which asserts full removal. The roadmap is the shipped-state-of-record,
so a Phase 18 planner would assume the timestamp is available.

### WR-15: `PROJECT.md` cites the superseded verification score

**File:** `.planning/PROJECT.md:106` · **Angle:** doc-accuracy

Footer reads "verified 12/12"; `17-VERIFICATION.md:5` is `score: 14/14`, with `:15` explicitly
recording `previous_score: 12/12` as superseded.

---

**WR-12 / WR-13 / WR-14 / WR-15 — all four RESOLVED by `/gsd-audit-fix 17` (`ebc343f`).** Each was
re-verified against the tree before editing, not taken from this report: WR-12's symbol is absent
and `build_provenance.rs:23` is the renamed assertion; WR-13's `17-VALIDATION.md:7` already reads
`true`; WR-14's `DEVFLOW_BUILD_TIMESTAMP` has three hits in `crates/`, all comments; WR-15's
`17-VERIFICATION.md` frontmatter is `score: 14/14`. The two `17-VERIFICATION.md` anti-pattern rows
are struck through and marked superseded rather than deleted, consistent with how this project
treats point-in-time execution records.

---

## Info

- **IN-01** `main.rs:1650-1659` — `hook_context_root` keys on "not the terminal batch" rather than
  "is a content hook"; correct today, but a future non-terminal hook (e.g. `BranchCreate`) would
  silently inherit the worktree root.
- **IN-02** `outcome_policy.rs:38` — `decide_action`'s `_stage` is an unused forward-compat stub;
  `decide_action_is_deterministic` can only assert trivially. Pin the contract with an explicit
  cross-stage equality assertion.
- **IN-03** `main.rs:951-961` — `affects_compiled_binary` omits `include_str!`/`include_bytes!`
  assets, `.cargo/config.toml`, and non-`.rs` compiled sources; also allocates per candidate. The
  doc comment justifies exclusions but not omissions.
- **IN-04** `build.rs:52-54` — a `git status` failure defaults the dirty flag to `false`, the
  less-safe value. Largely mitigated by the live check at `:981`.
- **IN-05** `17-VERIFICATION.md:10`, `17-VALIDATION.md:5` — bare `#6` / `#2117` are review-round
  and upstream-GSD identifiers, but GitHub auto-links `#6` to an unrelated merged PR ("docs: add
  autwicky-powered wiki"). This repo has zero issues. Qualify as `round 6` / `GSD#2117`.
- **IN-06** commit `3d6e6a6` — body says "across 9 targets"; `17-VALIDATION.md:340` says 10. Ten
  `test result:` lines sum to exactly 367; the tenth reports 0 tests. The headline 367 is correct.
- **IN-07** `main.rs:843-845` — `workflow_started` records absolute `worktree` and `exe_path`,
  disclosing the OS username. The file is gitignored, but `OPERATIONS.md:105` advertises
  `events.jsonl` for external tailing. (Round 1 IN-03; the security angle argued Warning.)
- **IN-08** `main.rs:1533-1558` — `truncate_reason` bounds agent output to 300 chars but does not
  redact; a token echoed in a failure reason reaches `.devflow/gates/` and a desktop notification.
- **IN-09** `17-REVIEW.md:178,448`, `17-VALIDATION.md:237` — the operator's home path
  (`/var/home/<user>/...`) is committed into planning docs, permanently in git history.
- **IN-10** `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` Finding 1 — `main.rs:2917` citation has
  drifted ~96 lines (and was off by 2 when authored). The substantive claim verifies.
- **IN-11** `17-VERIFICATION.md:77` — cites lines 139-202 for the CR-02 test; the attribute is at
  `:148`. Test exists and passes.

---

## Verified Clean (no finding)

- **build.rs embeds no machine-identifying data** — only `DEVFLOW_BUILD_COMMIT` (a SHA, not a
  branch name) and `DEVFLOW_BUILD_DIRTY` (a bool). This phase *removed* `DEVFLOW_BUILD_TIMESTAMP`,
  a net reduction in build-machine fingerprinting.
- **CI workflows handle no secrets** — zero `secrets.`, no `pull_request_target`, no
  `github.event.*` interpolation, no artifact upload. No `continue-on-error`, `|| true`, `set +e`,
  `if: always()`, matrix, or `needs` graph. The Phase 17 diff is a strict tightening (clippy
  widened to `--workspace --all-targets`). Bare `cargo test` in `ci.yml` covers the same 367 tests
  as `--workspace` (virtual manifest, no `default-members`) — CI scope is not a gap.
- **`gitignore_coverage.rs` is not vacuous** — invokes `git check-ignore -q` once per path,
  explicitly documenting why batched argv would be unsound; both no-match (exit 1) and
  git-unavailable (exit 128) fail the assertion.
- **`log_format_env.rs` asserts the inverse** — log lines land on stderr and are *absent* from
  stdout, so a regression routing logs into the agent-output stream fails.
- **`shell_quote` (`ship.rs:409-424`) is correct** — conservative allowlist, `'\''` escaping, and
  the interpolated `phase` is a `u32`; the cron command string is not injectable.
- **No secrets committed** — all 18 added files are `.planning/*.md`. A full-diff scan for
  `ghp_`/`github_pat_`/`sk-`/`AKIA`/`Bearer`/PEM headers returned only a doc line describing that
  same scan.
- **The SUMMARY-frontmatter vs diff mismatch is a false alarm** — `lib.rs`, `outcome_policy.rs`,
  and `log_format_env.rs` all exist at the merge-base (`outcome_policy.rs` created in `68a1b00`,
  an ancestor). Plans 17-01/17-02 were already merged to develop; the diff range simply does not
  span the whole phase.
- **No artifact claims Phase 17 was merged, tagged, or pushed** — apart from CR-01.
- **Re-audit #6's scoping claim holds** — `git diff --stat 46058a7..1070df0` touches only
  ROADMAP.md, STATE.md, 17-VALIDATION.md, 17-VERIFICATION.md; nothing under `crates/`.
- **Mutation evidence corroborates GAP-3** — the build cache at `target/debug/build/devflow-*/output`
  reads `DEVFLOW_BUILD_COMMIT=8140bea...`, matching current HEAD exactly.

---

## Ship Gate

**BLOCKED** — 1 Critical open (CR-01 R2). Resolve the false `1.4.88` changelog heading, then
re-run the review.

---
---

# Appendix: Round 1 (2026-07-19T17:10:00Z)

Preserved verbatim. Round 1 frontmatter recorded: 3 Critical (0 open at time of writing), 9
Warning, 6 Info, 18 total, `status: issues_found`, 14 files reviewed.


# Phase 17: Code Review Report

**Reviewed:** 2026-07-19T17:10:00Z
**Depth:** deep (5 parallel angles, merged and deduplicated)
**Files Reviewed:** 14
**Status:** issues_found — **3 Critical found, 0 open** (CR-01 fixed via `5431f9e`, CR-03 fixed
via `f531d08`, CR-02 fixed via `17-11` — see the CR-02 entry and the Audit-Fix Addendum below).
Warnings/Info items remain open per the "Still open" list in the Addendum.

## Summary

This review supersedes the standard-depth pass captured at commit `3caf985`. It was run as
five independent angles — doc-accuracy cross-reference, security/leaked-data, CI/build
correctness, external-state claims, and a generalist deep pass — merged and deduplicated here.
Every Critical was independently re-verified by the orchestrator before being recorded.

**Baseline is genuinely green.** `cargo build --workspace`, `cargo test --workspace`
(364 passed / 0 failed across 10 targets), `cargo clippy --workspace --all-targets -- -D warnings`,
and `cargo fmt --check` all pass at `6995b6f`. Exit codes were captured directly, not through a
pipe. Nothing in this report is a currently-failing test.

**Prior findings, re-verified:**

| ID | Status |
|---|---|
| CR-01 (`run_preflight` double-launch) | **Genuinely fixed** (`c03498d`). `run_preflight` returns `Result<bool>`; the single production call site at `main.rs:1126` short-circuits on `Ok(false)`. Two RED-verified regression tests cover the Advance and LoopBack arms. |
| WR-01 (staleness warning noise) | **Still open** — see WR-01 below. |
| WR-02 (substring self-dogfood match) | **Still open** — `is_self_dogfood_workspace` is byte-identical to the reviewed version. |
| WR-03 (`gate_or_abort_infra` doc comment) | **Still open** — doc at `main.rs:1367` still misdescribes the call graph. |

**Security angle is clean at Critical.** No network egress anywhere in `crates/` (no HTTP
dependency in any `Cargo.toml`), no credentials or session data committed, no shell injection
(all 26 `Command::new` sites use argv arrays), and `gh auth status` output is discarded rather
than logged. All 15 runtime-written paths are correctly gitignored today.

The three Criticals below are: a false release claim that a merge would propagate into the
project changelog, a build-provenance gate that silently certifies stale binaries under CI
conditions, and a rate-limit path that violates the never-silent invariant this very phase exists
to enforce.

---

## Critical Issues

### CR-01: `CHANGELOG.md` asserts a release (1.4.26) that does not exist

**File:** `CHANGELOG.md:3` (introduced by `bde8f73`)

**Issue:** The diff adds a dated release heading for a version that exists nowhere else in the
project:

```
## 1.4.26 — 2026-07-19

- Released phase via DevFlow.
```

Verified against actual repository state:

```
$ git tag -l
v1.0.1  v1.2.0  v1.3.0  v1.3.69          # no v1.4.26, local or on origin

$ rg -n '^version' Cargo.toml
9:version = "1.3.69"                      # workspace still at 1.3.69

$ devflow --version
devflow 1.3.69
```

The string `1.4.26` appears in exactly two places in the tree: this changelog heading and a
tempdir test fixture at `main.rs:5316`. The branch's own `.planning/STATE.md` Completed table
records Phase 17 with Version `—`, directly contradicting the changelog.

**Root cause** is visible in `crates/devflow-core/src/hooks.rs`: `changelog_append()` (`:173`) and
`version_bump()` (`:183`) each independently call `version::compute_version()`, which derives
`minor = count_git_tags()` (now 4) and `patch = commits_since_last_minor_tag()` — yielding 1.4.26.
`changelog_append` ran and wrote the heading; `version_bump`, which writes `Cargo.toml` and
creates the `v{version}` tag, did not. This is not an unsupported-format fallback: `write_version`
handles `[workspace.package]` correctly, proven by the passing test
`write_version_replaces_in_workspace_cargo_toml` (`version.rs:415`).

**Impact:** Merging this puts a dated, false release record at the top of the project's changelog.
A user reading it sees 1.4.26 as current, while `devflow --version` prints 1.3.69 and no such tag
is fetchable. This is the only external-state assertion on the branch that is provably false, and
it is the one a merge propagates. It is also the single user-facing doc claim the phase adds.

Note the `bde8f73` "move stranded changelog entry onto the branch it describes" operation itself
is sound — no entry was dropped or duplicated (`git log --all -S'1.4.26' -- CHANGELOG.md` returns
exactly one commit; `develop` and `main` changelogs are untouched at 1.3.69). The defect is the
entry's *content*, not the move.

**Fix:** Either remove the heading until `version_bump` actually runs and tags, or make
`changelog_append` and `version_bump` share one computed version and run atomically so a changelog
heading can never outlive its tag. See WR-04 for the related ordering defect in the same hook batch.

---

### CR-02: `build.rs` never re-runs on working-tree changes — the staleness gate certifies stale binaries as Fresh

**File:** `crates/devflow-cli/build.rs:36-41` (declared triggers) vs `:48` and `:67` (actual inputs)

**Issue:** Input-versus-trigger audit of the build script:

| Input read | Covered by a `rerun-if-changed`? |
|---|---|
| `git rev-parse HEAD` (`:47`) | Yes — `.git/refs` / `HEAD` / `packed-refs` |
| `git status --porcelain` (`:48`) — reads the **entire working tree** | **No trigger at all** |
| `SystemTime::now()` (`:67`) | **No trigger possible; never refreshed** |

Reproduced in a fresh `git clone` — exactly what `actions/checkout@v4` produces, and which has
`packed-refs` present:

```
$ git clone --no-hardlinks <worktree> /tmp/dfprobe && cd /tmp/dfprobe
$ ls .git/packed-refs
.git/packed-refs                              # CI-like: packed refs exist

=== BUILD 1 ===
DEVFLOW_BUILD_COMMIT=6995b6f0922ccf5fd3f663c6effd33551f10f2e2
DEVFLOW_BUILD_DIRTY=false
DEVFLOW_BUILD_TIMESTAMP=1784493764

# append a line to crates/devflow-cli/src/main.rs, rebuild (crate DOES recompile)
$ git status --porcelain
 M crates/devflow-cli/src/main.rs              # git says dirty

=== BUILD 2 ===
DEVFLOW_BUILD_COMMIT=6995b6f0922ccf5fd3f663c6effd33551f10f2e2
DEVFLOW_BUILD_DIRTY=false                      # still false
DEVFLOW_BUILD_TIMESTAMP=1784493764             # byte-identical, unchanged
```

The binary is rebuilt from modified source but embeds `dirty=false` and the *previous* build's
timestamp. Both values feed `enforce_build_staleness(...)` (`main.rs:1133-1138`) and the
`workflow_started` payload (`main.rs:841-843`).

**Impact:** The build-provenance/staleness gate is the headline deliverable of 17d, and its stated
purpose is catching "you forgot to rebuild." Under CI conditions it reports a binary carrying
uncommitted changes as a clean build of the old commit — the exact failure it exists to prevent.
Commits *are* covered (a commit bumps `.git/refs/heads/...` and correctly re-triggers); uncommitted
edits are not.

**This is currently masked on the developer machine.** `/var/home/denniyahh/Github/devflow/.git/packed-refs`
does not exist, and cargo treats a missing `rerun-if-changed` path as *always rerun* — so locally
the provenance looks correct by accident. Any `git gc`, or any CI checkout, creates `packed-refs`
and the bug appears. That is why no test catches it, and why the phase's own dogfood runs did not
surface it.

**Fix:** `git status --porcelain` cannot be fingerprinted by path. Either drop the caching intent
(emit `cargo:rerun-if-changed=` against a non-existent sentinel, accepting one build-script run per
build) or stop embedding a dirty flag and timestamp that the trigger set cannot honor. Add a test
that builds twice across a working-tree edit and asserts the provenance actually changed.

**RESOLVED — `17-11` (gap closure).** Reclassified manual-only below because both fixes this
entry proposed reverse a recorded design decision (`build.rs:32-35`, D-20 / review consensus #7);
the operator has now made that call. Chosen: **always re-run `build.rs`, and stop embedding
`DEVFLOW_BUILD_TIMESTAMP`**, keeping `DEVFLOW_BUILD_COMMIT` + `DEVFLOW_BUILD_DIRTY`. `build.rs`
now declares a single sentinel `rerun-if-changed` path that can never exist, forcing cargo's
"missing input ⇒ always rerun" rule on every `cargo build`. Retiring the timestamp is what keeps
that from recompiling `devflow-cli` on every build — it was the only embedded value that changed
every run; only the commit/dirty flag change now, so `rustc-env` (and the recompile it triggers)
only fires when they actually do.

Retiring the timestamp also retired the mtime arm — the fixture home of 17-10's CHANGELOG
false-positive fix — replaced with a two-signal decision: the build's own `DEVFLOW_BUILD_DIRTY`
flag plus a live check of whether the working tree currently has modified build-affecting files
(`combined_staleness` / `tree_has_modified_build_inputs`, `main.rs`). `(dirty=false, tree
modified)` ⇒ Stale (this CR-02 case); `(dirty=true, tree modified)` ⇒ Indeterminate (warn, never
block — Pitfall 4, since "same dirt" and "more dirt" can't be told apart without a timestamp). The
ancestry arm (WR-01/17-06) and Ahead classification (17-07) are unchanged. As a byproduct,
`DEVFLOW_BUILD_DIRTY` is now actually read by a staleness code path (WR-09's second bullet) — the
call site at `main.rs`'s `launch_stage` passes `env!("DEVFLOW_BUILD_DIRTY") == "true"` into
`enforce_build_staleness`; WR-09's `Ahead`/`Indeterminate` output-collapse bullet is untouched and
remains open.

Verified via the reviewer's own reproduction, automated as a regression test
(`crates/devflow-cli/tests/build_provenance.rs::build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild`):
a synthetic packed-refs checkout is built, a tracked `.rs` file is edited, it is built again, and
the build script's own cached `output` file is asserted to show `DEVFLOW_BUILD_DIRTY` flip
`false → true`. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
and `cargo fmt --check` all pass after the fix.

---

### CR-03: rate-limit auto-resume stalls the phase silently when the retry hint isn't a timestamp

**File:** `crates/devflow-cli/src/main.rs:1417-1421` (and the `sequentagent` path at `:2344`)

**Issue:**

```rust
if instructions.hermes_cron.schedule.is_empty() {
    println!("no parseable retry time — auto-resume cron not scheduled; resume manually");
} else { ... }
// ... events::emit(...) ...
Ok(())
```

There is **no gate and no `fire_gate_notify`** on the empty-schedule branch — verified by reading
through to the function's `Ok(())` at `main.rs:1443`.

**Traced failure path:** `detect_claude_rate_limit` (`agent_result.rs:178`) falls back to the
literal `Some("usage limit".to_string())` when a 429 payload carries no `retry_after`/`message`/`error`
field. `rate_limited_result` (`agent_result.rs:518`) builds `reason: "rate limited until usage limit"`.
`retry_after_from_reason` (`main.rs:2327`) strips the prefix, yielding `"usage limit"`.
`cron_schedule_from_retry_after("usage limit")` returns `None` (correctly — WR-06 forbids turning
unparseable agent output into an every-minute cron), so the schedule is empty.

**Concrete scenario:** Claude returns `{"subtype":"error_rate_limit"}` with no retry field.
`advance()` dispatches `Action::AutoResume` → `handle_rate_limited_outcome` prints to the
**detached monitor process's stdout**, which nobody reads → emits an event → returns `Ok(())`.
The monitor exits. `state.gate_pending` is untouched, no gate file is written, no notification
fires. The phase is permanently stalled with zero operator signal.

**Impact:** This is precisely the "never-silent" invariant (WR-11 / D-15) that the rest of this
phase enforces everywhere else. A rate limit with no parseable retry time is a routine upstream
condition, not an exotic edge case.

**Fix:** Route the empty-schedule branch through the same gate/notify path the infra ceiling uses,
so an operator is told the phase needs a manual resume rather than left with a silently dead monitor.

---

## Warnings

### WR-01: `enforce_build_staleness` prints a near-universally-firing, misleading warning for every non-self-dogfood project

**File:** `crates/devflow-cli/src/main.rs:1033-1041`, `:1082-1086`, `embedded_commit_is_stale` at `:895`

Carried forward from the prior review and **confirmed still open** — no commit touches it.

`embedded_commit_is_stale` shells `git merge-base --is-ancestor <DevFlow's embedded build commit> HEAD`
inside `project_root`. For the common case (DevFlow driving some *other* project), that commit does
not exist in the target's object store, so git exits `128` and the `_ => Staleness::Indeterminate`
arm catches it. `staleness_outcome(false, Indeterminate)` maps to `Warn` for every project:

```rust
(_, Staleness::Indeterminate) => StalenessOutcome::Warn,
```

So on essentially every stage launch of every ordinary project, DevFlow prints "build provenance
staleness check did not confirm a fresh build." Accurate but meaningless in that context, and it
trains operators to ignore DevFlow warnings generally — including the rare real ones from this same
codepath.

**Fix:** Short-circuit `enforce_build_staleness` for non-self-dogfood projects; the design already
treats that combination as a no-op-equivalent outcome, so no information is lost.

### WR-02: `is_self_dogfood_workspace` uses substring matching, which can false-positive-block an unrelated project

**File:** `crates/devflow-cli/src/main.rs:1002`

Confirmed still open. The check is `members.contains("crates/devflow-core") && members.contains("crates/devflow-cli")`
— `str::contains` is a substring match, not an exact array-element match. A workspace with members
like `crates/devflow-core-extras` / `crates/devflow-cli-plugin` satisfies both without containing
either real member, and would be classified as self-dogfood. Combined with the hard `Block` outcome
for self-dogfood + `Stale`, that hard-blocks an unrelated project's entire pipeline — the one
outcome this feature set exists to never inflict.

**Fix:** Split `members` on `,`, trim quotes and whitespace from each entry, and compare for exact
equality.

### WR-03: `gate_or_abort_infra`'s doc comment misdescribes the call graph

**File:** `crates/devflow-cli/src/main.rs:1367`

Confirmed still open. The comment claims the AutoResume arm "bumps `infra_failures` itself before
calling this." It does not — `handle_rate_limited_outcome`'s ceiling branch (`main.rs:1408`) calls
`handle_infra_outcome`, which performs the increment. Behaviour is correct (bumped exactly once);
the comment describes a call graph that does not exist, and a maintainer trusting it could
reintroduce a double-increment.

### WR-04: `ChangelogAppend` is written after the only committing hook, so the entry is never committed

**File:** `crates/devflow-cli/src/main.rs:1627` (`hook_context_root`, added in `ae744ed`) + `crates/devflow-core/src/hooks.rs:173`

`changelog_append` writes the file and never commits:

```rust
fn changelog_append(ctx: &HookContext) -> Result<(), HookError> {
    ...
    std::fs::write(&path, updated)?;      // writes, never commits
```

The batch order (`hooks.rs:81`) is `vec![Hook::DocsUpdate, Hook::ChangelogAppend]`, and `docs_update`
(`hooks.rs:159`) is the only hook that runs `git.commit_all(...)` — it runs **first**.

**Scenario:** Validate→Ship fires with a worktree configured. `hook_context_root` returns the
worktree; `docs_update` commits what is dirty at that moment; then `changelog_append` writes
`CHANGELOG.md` into the worktree, uncommitted. `Merge` (terminal batch, `project_root`) merges the
feature branch into develop, so the changelog edit reaches neither branch. `BranchCleanup`'s
non-force delete succeeds (the branch *is* merged), leaving an orphaned worktree holding the only
copy. Commit `bde8f73` is the operator hand-fixing this exact outcome — `ae744ed` relocated the
write but left the root cause in place, and moved it off the primary checkout where `git status`
would have surfaced it.

**Secondary:** `changelog_append` now calls `version::compute_version(&ctx.project_root)` against
the *worktree* HEAD. `commits_since_last_minor_tag` (`version.rs:110`) counts `tag..HEAD`, so the
worktree (N commits ahead) yields a different patch number than `VersionBump` later computes on
post-merge develop — the changelog heading and the git tag disagree. This is the same machinery
that produced CR-01.

**RESOLVED — `17-12` (gap closure).** Fixed both the primary defect (this entry) and its ordering
corollary (the Secondary paragraph above) together, since they share one root cause. `ChangelogAppend`
moved from the Validate→Ship batch into `hooks_after_ship()`, strictly after `VersionBump` and
before `BranchCleanup`. It now reads the version via a new `version::read_version` (added this
plan) instead of `compute_version` — read_version reports exactly what a prior `write_version` call
wrote, with no git involved, so it can never see a tag `VersionBump` just cut and derive a
different, larger version (the trap the Secondary paragraph names). `changelog_append` also now
commits its own write via a new `GitFlow::commit_path`, scoped to `CHANGELOG.md` so it cannot sweep
in unrelated dirty state; `version_bump`'s own version-file write had the identical uncommitted-write
defect and is fixed the same way. See the live CR-01 entry above for the full disposition and the
regression test proving three-way agreement (changelog heading version == tag == version file
version) plus a clean working tree.

**Round 3 note:** the "cannot sweep in unrelated dirty state" claim for `GitFlow::commit_path` does
not hold — see Round 3's CR-01. The ordering/read-back half of this fix is sound; the commit-scoping
half is not.

### WR-05: CI's clippy gate does not lint test code — a required check passes on code clippy rejects

**File:** `.github/workflows/ci.yml:30` — `cargo clippy -- -D warnings` (no `--all-targets`)

Proved by planting a `needless_range_loop` in `tests/build_provenance.rs`:

```
=== ci.yml's exact command: cargo clippy -- -D warnings ===
exit=0                                     <- CI is GREEN

=== with --all-targets ===
error: the loop variable `i` is only used to index `v`
error: could not compile `devflow` (test "build_provenance")
exit=101
```

This branch adds ~2200 lines to `main.rs` plus test files; no `tests/` target is linted by CI today.

**Fix:** `cargo clippy --workspace --all-targets -- -D warnings`.

### WR-06: the devcontainer job shares the identical clippy scope gap

**File:** `.github/workflows/devcontainer.yml:26` — `cargo clippy --workspace -- -D warnings`

Has `--workspace` but still no `--all-targets`, so the second "CI-parity" job does not compensate
for WR-05. Both required checks share the same blind spot.

### WR-07: the `.gitignore` regression guard covers 3 of 15 runtime paths, omitting raw agent stdout

**File:** `crates/devflow-cli/tests/gitignore_coverage.rs:27-33`

The test asserts only `.devflow/state.json`, `.devflow/events.jsonl`, and `.devflow/gates/probe.json`.
Its own docstring states its purpose is preventing a repeat of commit `d021e3a`, where "a `.gitignore`
rewrite silently dropped coverage and leaked runtime telemetry into git."

Nothing leaks today — all 15 written paths verified `git check-ignore` IGNORED. But a future
`.gitignore` rewrite could drop `.devflow/phase-*-stdout`, `.devflow/phase-*-stderr.log`,
`.devflow/history/`, or `.devflow/cron-instructions*.json` and the guard stays green — including
raw agent stdout, the highest-value leak surface since it captures whatever an agent printed.
Phase 17 added `cron-instructions-NN.json` to the written set without extending the guard.

**Fix:** Extend the `check-ignore` argument list to all 15 paths. One line.

### WR-08: the cb9359f "bounded poll" is test-only — the production wedge it documents is still unbounded

**File:** `crates/devflow-cli/src/main.rs:1791-1813` (`finish_workflow`), gate default at `:30-33`

`git show cb9359f` touches **only** `mod tests` — it sets `DEVFLOW_GATE_TIMEOUT_SECS=2` inside
`concurrent_ship_advances_finish_both_phases_independently`. No production code changed:

```rust
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
raw.and_then(|s| s.parse().ok()).unwrap_or(SEVEN_DAYS)
```

**Scenario** (from the commit's own RED evidence): two phases reach `version_bump` concurrently and
compute the same version; the loser's `git tag` fails with "reference already exists";
`finish_workflow`'s loop calls `run_gate` → `Gates::poll_response(..., gate_timeout_secs())` →
**7 days**, holding that phase's `lock::acquire` guard throughout.

The timeout path itself is safe — `poll_response` returning `None` produces
`CliError::Message("gate ... timed out")` (`main.rs:1899`), never a success, so there is no false
green. But nothing in production bounds it, and 17-09's summary lists this as a shipped feature.

### WR-09: `Ahead` and `Indeterminate` collapse to the same output, and `DEVFLOW_BUILD_DIRTY` is never read

**File:** `crates/devflow-cli/src/main.rs:1033-1041`

The (dirty-tree, ancestor, descendant, exact, unrelated) × (self-dogfood, not) matrix is coherent
after `f73a968`/`3c2774e`/`ae744ed` — every cell traced, no wrong classification. Two gaps remain:

- `Ahead` and `Indeterminate` both map to `Warn` and print the *same* message, so "your binary is
  newer than your source" is unreportable from the logs.
- `build.rs:57` embeds `DEVFLOW_BUILD_DIRTY` and `build_dirty_is_exactly_true_or_false` asserts on
  it, but **no staleness code path reads it**. Scenario: build from a dirty tree, then `git checkout .`.
  `embedded_commit == HEAD` → `Fresh` → `Ok`; the tree is clean so the mtime arm returns `Some(false)`.
  A binary containing code matching no commit is certified Fresh, and the one signal that would catch
  it is embedded and discarded. (Compounded by CR-02, which makes the flag unreliable anyway.)

---

## Info

### IN-01: `build_commit_is_accessible_and_does_not_panic` asserts nothing

**File:** `crates/devflow-cli/tests/build_provenance.rs:31-37`

```rust
let commit = env!("DEVFLOW_BUILD_COMMIT");
let _ = commit.len();
```

`env!` resolves at compile time to a `&'static str`; `.len()` cannot panic and the result is
discarded. The test passes unconditionally regardless of what `build.rs` emits, inflating the pass
count by one. The sibling tests are fine. Note that none of the three would catch CR-02, since a
*stale* value is still parseable and still `"false"`.

### IN-02: an orphaned doc comment makes `cargo doc` actively misleading

**File:** `crates/devflow-cli/src/main.rs:1498-1512`

The doc comment describing `handle_stage_failure` ("Handle a non-Validate stage failure… WR-11:
this path must never be silent") is attached to `truncate_reason`, whose own doc comment follows at
`:1505`. `handle_stage_failure` at `:1538` is now undocumented.

### IN-03: `events.jsonl` records the binary's absolute path, disclosing the OS username

**File:** `crates/devflow-cli/src/main.rs:835-848` (new in this diff)

`workflow_started_payload` emits `"exe_path": std::env::current_exe()` and `"worktree"`, e.g.
`/var/home/denniyahh/...`. Gitignored and local-only, so not a leak today — but `OPERATIONS.md:105`
documents `events.jsonl` as "tail it from any tool," making it an explicit integration surface.
Worth knowing before anyone pipes it somewhere shared.

### IN-04: `truncate_reason` bounds but does not redact agent output

**File:** `crates/devflow-cli/src/main.rs:1510-1536` (pre-existing, not introduced here)

Sanitizes control characters and caps at 300 chars, then routes into gate JSON and the desktop
notification. If an agent echoed a token, up to 300 characters of it reach the gate context.
Bounded, and `.devflow/gates/` is gitignored.

### IN-05: `run_preflight`'s gate arms recurse into `launch_stage` with no depth bound and drop `archived_stage`

**File:** `crates/devflow-cli/src/main.rs:810-819`

Both arms call `launch_stage(state, None, None)`, whereas `handle_stage_failure` (`:1553`, `:1560`)
passes `Some(stage)`. Consequence: the `capture_archived` event records `to_stage` as the current
stage rather than the archived one. Separately, if the preflight predicate is *permanent* (Codex +
Auto + Define + no `CONTEXT.md` on develop — `preflight_interactivity_check`, `:722`), each
`Advance` response re-enters `launch_stage` → `run_preflight` → re-gates, growing the stack one
frame per approval. Human-gated, so not reachable as a spin, but there is no retry ceiling.

### IN-06: planning artifacts carry stale counts, stale line citations, and miss plan 17-10

The behavioural claims in the artifacts are accurate; the numbers attached to them are not.
Actual suite at HEAD: 66 (bin) + 276 (core lib) + 22 (integration) = **364 passed**.

- `17-VERIFICATION.md:41,52,93` claim "61 devflow-cli unit tests"; `:175` claims "64/64 bin" — the
  document contradicts itself, and both differ from the actual 66.
- `17-VALIDATION.md:105` claims "64/64"; `:335` claims a 361-test total.
- `.planning/ROADMAP.md:194` says `9/9 plans executed` and lists only 17-01…17-09, while 17-10 was
  planned (`504fa38`), executed (`ae744ed`, +206 lines to `main.rs`), and summarized (`6995b6f`).
  `.planning/STATE.md` `stopped_at` likewise reads `Completed 17-09-PLAN.md`, and its status line
  says `14/14` while its own `progress` block says `43/43` — three denominators for one phase.
- Line citations drifted: `main.rs:1640-1641` for the `infra_failures` reset is actually `:1732`;
  `main.rs:870-874` for the `rev-parse HEAD` equality gate is actually `:884`.

A reviewer reading ROADMAP would not know `ae744ed` is part of this phase.

---

## Verified Clean (no finding)

Recorded so a later pass need not redo the work:

- **No network egress or telemetry.** No `reqwest`/`hyper`/`ureq`/`std::net`/`TcpStream` anywhere
  in `crates/`; no HTTP dependency in any `Cargo.toml`.
- **No credentials or session data committed.** Scanned all added lines for `ghp_`/`gho_`/`github_pat_`/`sk-`/`Bearer`/`AKIA`/`xox`/PEM headers — zero matches. All 16 added files are `.planning/*.md`.
- **`.gitignore` coverage is complete today** — all 15 runtime paths verified IGNORED via
  `git check-ignore`. `archive_phase_files` only moves data *into* ignored `.devflow/history/`.
- **`build.rs` leaks no builder identity** — embeds only a commit SHA, a dirty bool, and a Unix
  timestamp. No hostname, username, or env passthrough.
- **`gh auth status` output is never logged** (`main.rs:759-773`) — on failure the output is
  discarded and a fixed literal is returned.
- **Subprocess safety** — all 26 `Command::new` sites use argv arrays with `.current_dir()`. The
  single `sh -c` (`main.rs:3024`) interpolates only hardcoded literals and predates this diff.
- **No CI exit-code masking** — no `|| true`, `continue-on-error`, `set +e`, backgrounded steps, or
  vacuous `if:` guards in either workflow; no pipes at all.
- **The known `--exact` false-green trap is not triggered** — no `--exact` and no bare test-name
  filter anywhere in CI, scripts, or tests. The only `-p` reference names the real `devflow` package.
- **`devcontainer.yml`'s fail-fast guard is strong** — `devcontainer_ci_failfast.rs` asserts `set -e`
  is the first non-blank line and precedes every `cargo` invocation, rather than grepping loosely.
- **Help snapshot is accurate** — `cargo run -- --help` is byte-identical to the snapshot; the one
  absent variant (`Advance`) carries `#[command(hide = true)]`.
- **All 17 tests named in the artifacts exist**, individually verified. The one apparent miss is
  documented as a rename in `17-03-SUMMARY.md:145`, and its replacement exists at `agent_result.rs:1554`.
- **`ae744ed`'s "content hooks at the worktree" claim is real** — these are DevFlow's internal `Hook`
  enum variants, not git hooks. Landed as `hook_context_root()` (`main.rs:1612-1685`) with a covering
  test at `:5366`.
- **Every commit SHA cited in the artifacts exists and is an ancestor of HEAD.**
- **`ship.rs` makes no unverified external-success claim** — its diff is purely additive
  (`build_single_agent_cron_instructions` + one test); it builds a manifest and returns it.
- **No artifact claims Phase 17 is merged, PR'd, or pushed.** ROADMAP's `## Shipped` section was
  correctly left untouched.
- **`log_format_env.rs` is high quality** — `env_remove` sanitizes ambient `RUST_LOG`/`DEVFLOW_LOG_FORMAT`
  so the runner's shell cannot mask behaviour; all three tests assert both directions.

**Context, not a finding:** local `develop` (`a2c314f`, the merge-base) is 18 commits ahead of
`origin/develop` (`c034ad7`). Phase 17-01/17-02 work is committed locally but unpushed. No artifact
claims otherwise, so nothing asserts falsely — worth knowing before a ship decision.

---

## Audit-Fix Addendum 2026-07-19 (`/gsd-audit-fix 17`, `--max 5 --severity medium`)

Baseline at `36a5e14` re-confirmed green before any edit: 364 passed / 0 failed, exit 0.

| ID | Disposition | Commit |
|---|---|---|
| CR-01 | **Fixed** — false 1.4.26 heading removed | `5431f9e` |
| CR-02 | **Fixed** — `17-11` gap closure, operator decision (always-rerun `build.rs`, drop the timestamp) | `3e39cf6` |
| CR-03 | **Fixed** — empty-schedule branch routes through `gate_or_abort_infra`; regression test RED→GREEN | `f531d08` |
| WR-02 | **Fixed** — exact member-path equality; regression test RED→GREEN | `02e17dd` |
| WR-05/06 | **Fixed** — `--all-targets` in both clippy gates | `50a6b16` |
| WR-07 | **Fixed** — guard covers all 14 runtime paths, one `check-ignore` per path | `2a92ebe` |

Suite after all fixes: **366 passed / 0 failed** (+2 regression tests), `cargo clippy --workspace
--all-targets -- -D warnings` exit 0, `cargo fmt --check` exit 0.

**CR-02 was reclassified from auto-fixable to manual-only at this pass.** `build.rs:32-35`
documented the narrow trigger set as a deliberate decision ("Re-run only when git refs actually
move — not on every `cargo build`", review consensus #7 / D-20), and both fixes this addendum
could have applied autonomously would have reversed that recorded decision without operator
sign-off — a design tradeoff for a human, not an autonomous edit. **The operator has since made
that call and CR-02 is fixed — see the CR-02 entry above and the `17-11` disposition row in the
table.**

**Additional weakness found while fixing WR-07, not named in the review:** `git check-ignore` exits
0 when *any* argument is ignored (verified directly: batched call with one ignored + one unignored
path exits 0). The original guard passed all three paths in a single invocation, so it would have
stayed green after losing 2 of its 3 paths. Now one invocation per path.

**Still open after this pass:** WR-01, WR-03 (both auto-fixable, over `--max 5`), WR-04, WR-08,
WR-09 (manual-only), and IN-01…IN-06 (below `--severity medium`). CR-02 is resolved (`17-11`);
`status: issues_found` is left unchanged pending these remaining Warning/Info items.

Note WR-08 interacts with the CR-03 fix: a rate-limited phase with an unparseable hint now blocks
on `Gates::poll_response` — bounded only by the 7-day production default WR-08 flags — instead of
exiting stalled. That is the intended never-silent semantics, but it changes monitor process
lifetime, and WR-08's production default is the thing that bounds it.

---

_Reviewed: 2026-07-19T17:10:00Z_
_Reviewer: Claude (5-angle merged deep review)_
_Depth: deep_

---
---

_Reviewed: 2026-07-19 (round 3)_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

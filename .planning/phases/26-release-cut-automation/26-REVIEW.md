---
phase: 26-release-cut-automation
reviewed: 2026-07-30T15:05:00Z
depth: deep
review_mode: re-review-of-unreviewed-critical-fixes
supersedes: 26-REVIEW.md (2026-07-29T23:59:00Z, deep/five-angle, 7 Critical)
files_reviewed: 18
files_reviewed_list:
  - CONTRIBUTING.md
  - OPERATIONS.md
  - README.md
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/tests/project_root_guard.rs
  - crates/devflow-cli/tests/release_check.rs
  - crates/devflow-cli/tests/release_execute.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/release.rs
  - crates/devflow-core/src/release_ledger.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/sync.rs
  - crates/devflow-core/src/version.rs
findings:
  critical: 5
  warning: 8
  info: 2
  total: 15
status: issues_found
ship_gate: BLOCKED
---

# Phase 26: Code Review Report (re-review of the unreviewed Critical fixes)

**Reviewed:** 2026-07-30
**Depth:** deep — fix-diff audit (`git show` per fix commit) + current-state read +
executable reproduction against the built binary
**Files Reviewed:** 18
**Status:** issues_found — **Ship gate: BLOCKED (5 Critical)**

## Scope and method

This supersedes the deep five-angle review recorded at this path on
2026-07-29 (7 Critical / 24 Warning / 5 Info). That review blocked the ship.
All 7 Criticals were subsequently "fixed" across `e4a3236`, `43a7a96`,
`8f5f2d1`, `7bd9a37`, `0f0e17a`, `5824fdb`+`c5f7ea0` (plan 26-08), and
`4b236f4`+`bb93c57` (plan 26-09) — and **none of those fixes had ever been
reviewed**; the re-review that should have covered them was cut short by an API
usage limit, not by a verdict.

The fixes themselves were the prime targets here, not the original code. For
each fix I asked, in order: does it close the finding or merely move the
hazard; is there still a reachable path to the original bad outcome; **did the
fix introduce a new hazard**; and is the accompanying test real.

**Four of the five new Criticals below were reproduced by execution** against
the built binary in throwaway fixture repos under `/tmp`, not hypothesized. The
one that was not (CR-04) is a documentation defect whose semantics I verified
against `git`'s own `@{u}` behavior.

**Verification baseline (matches the stated expectation exactly).** Full
`cargo test --workspace --no-fail-fast` in an isolated `CARGO_TARGET_DIR`,
asserting on `test result:` counts, not exit codes:

| target | result |
|--------|--------|
| `devflow_core` lib | **446 passed / 0 failed / 0 ignored** |
| `devflow` bin | **234 passed / 0 failed** |
| `tests/release_execute.rs` | 8 / 0 |
| `tests/release_check.rs` | 11 / 0 |
| `tests/project_root_guard.rs` | 4 / 0 |
| all other CLI targets | green, 0 failed |
| `clippy --workspace --all-targets -D warnings` | exit 0, clean |
| `cargo fmt --check` | exit 0, clean |

The tests are green. **The suite being green is not evidence of correctness
here** — every one of the five Criticals below sits on a path the suite does
not fixture.

---

## Status of the 7 previously-Critical findings

| ID | Verdict | Evidence | Reachable path that still fails |
|----|---------|----------|---------------------------------|
| **C-01** stray unreachable tag adopted and pushed | **CLOSED** | `release.rs:415-430` resolves the tag's target and refuses with `StrayBaselineTag` unless it names `origin/main`'s tip. The refusal is at `compute_release_version`, called at `release.rs:544/566` — **before** the ledger write (`:633-634`) and before step 1's first mutation (`:644`). Test `refuses_a_stray_unreachable_tag_instead_of_adopting_its_version` (`release.rs:1625-1687`) is real: it asserts the remote `develop` ref and `Cargo.toml` are byte-identical across the refusal, that no step reported, and that no ledger was written. | None found. Note the guard is now fail-closed in two extra ways: a repo with no `origin/main` errors on `rev-parse` (`:416`), and an `origin/main` that advanced after a legitimate release tag was cut yields a spurious `StrayBaselineTag` (W-07's shape). Both refuse rather than act. |
| **C-02** failed publish unresumable → second release | **PARTIALLY-CLOSED** | Closed for the reproduced case: `resume_after_publish_failure_does_not_start_a_new_release` (`release.rs:1821-1893`) drives a genuine step-5 failure via a dead-registry `.cargo/config.toml`, asserts run 1 failed *at the publish step*, and asserts `origin/develop` is byte-identical across run 2 plus exactly one `chore: bump version to` commit. That is a real test of a real path. | **Still fails via `CompletedWithoutPublish` — see CR-02, reproduced.** Also does not cover a repository with **no** ledger (fresh clone, second machine, pre-ledger binary), which the module doc at `release.rs:35-38` acknowledges behaves exactly as before. And the fix **introduces CR-05** (permanent in-flight deadlock) whose only escape re-arms C-02 in full. |
| **C-03** step ledger discarded on failure | **CLOSED (in-memory); persisted half REGRESSED** | `ReleaseFailure { error, steps }` (`release.rs:255-261`), sequence moved into `run_release` borrowing the ledger (`:505-508`), CLI prints it on both paths (`commands.rs:2351-2364`). `execute_failure_reports_the_steps_that_already_landed` (`release_execute.rs:324-395`) drives the real binary and asserts the ✓ version-bump line and the "NOT rolled back" advisory. | The *persisted* record is destroyed at the start of every resume — `resumed.steps.clear()` (`release.rs:621`) followed by the unconditional write at `:634`, before any work. See W-N2. |
| **C-04** unpublished release reported complete | **PARTIALLY-CLOSED** | The unqualified "release cut complete" string is genuinely gone; `ReleaseOutcome::CompletedWithoutPublish` (`release.rs:116`) forces the caller to say so (`commands.rs:2325-2338`), and `a_release_that_publishes_nothing_is_never_reported_as_complete` (`release_execute.rs:470-533`) drives the real binary through a full tag-and-push with an empty publish order. | The run still **exits 0** (reproduced), `finalize_ledger` marks the cut Complete (`release.rs:922`), and the printed remediation — "fix the workspace `members` list and re-run" — **starts a second release**. See CR-02. |
| **C-05** `default-members` truncates the publish set | **PARTIALLY-CLOSED — still reachable** | `members_key_offset` (`git.rs:801-817`) correctly rejects `default-members` (the preceding `-` fails the `starts_key` test) and the new test asserts both orderings. | The scan is still position-blind in every other respect. A **commented-out** `# members = [...]` line, or a `members` key in any non-`[workspace]` table, that appears earlier in the file latches the scan onto the wrong array. **Reproduced** — see CR-03. |
| **C-06** mutating command retargets to an ancestor repo | **PARTIALLY-CLOSED** | `mutating_project_root` (`main.rs:718-779`) resolves via `git rev-parse --show-toplevel` and refuses on mismatch. The four `project_root_guard.rs` tests are real and well-designed — in particular `release_execute_from_a_worktree_refuses_on_the_worktree_not_the_parent` explicitly rejects "exited non-zero" as sufficient and asserts *which* repository the refusal is about. The read-only carve-out is proven by behavior (`release_check_from_a_subdirectory_still_walks_up`). | The guard compares **paths**, not **repositories**. An inherited `GIT_DIR` makes `--show-toplevel` report the local cwd while every other git call targets a different repository — the guard passes and the executor acts on a repo the operator never named. **Reproduced end-to-end.** See CR-01. |
| **C-07** README manual repair lacks sync's tree check | **PARTIALLY-CLOSED → REGRESSED** | The named defect is genuinely fixed: `README.md:56-64` now fetches, captures `HEAD^{tree}` before and after, aborts on a change, and calls out the inverted `git diff --stat` signal by name so it is not reintroduced. | The new abort path is `git reset --hard "@{u}"` (`README.md:62`) — a data-losing compensating action that `sync.rs:46-58` explicitly documents as forbidden under D-05 ("Do not 'fix' this later into a reset"). See CR-04. |

**Summary: 1 CLOSED, 5 PARTIALLY-CLOSED, 1 REGRESSED.** No fix was purely
cosmetic, and every one of them moved the code in the right direction — but
five of the seven left a reachable path to the original class of outcome, and
three of them introduced a new hazard on an irreversible surface.

### Binding decisions — compliance check

| Decision | Verdict | Evidence |
|----------|---------|----------|
| **D-05** fail-fast, no automatic rollback | **HONORED in code, VIOLATED in docs** | The ledger module has no `clear`/`remove` by deliberate design (`release_ledger.rs:316-318`); nothing in `release.rs` un-pushes, un-tags, un-publishes, deletes, or force-updates. But `README.md:62` prescribes `git reset --hard` as an automatic undo — CR-04. |
| **D-06a** ledger permitted for resume only; live state wins; must distinguish mid-flight from finished | **HONORED, with gaps** | The ledger is never a skip source — every step keeps its live predicate, and `a_ledger_claiming_a_step_completed_does_not_skip_it` (`release.rs:1941-1969`) plants a lying ledger and asserts the tag is really created. The in-flight/complete distinction exists and is corroborated against live `git rev-parse HEAD` (`release.rs:550-568`). Schema is versioned (`LEDGER_VERSION`), version is checked *before* deserialization (`release_ledger.rs:272-294`), and corrupt / forward-version / hand-edited ledgers refuse loudly and leave the file byte-identical (three real tests). Scope limit is respected — no reader outside `release.rs`. **Gaps:** no concurrency protection (W-N6), the halt is persisted as `status: "skipped"` for a step that never happened (W-N3), and the version pin can drift arbitrarily far from the manifest (W-N1). |
| **D-10** no signing-viability prediction, ever | **HONORED — no reintroduction** | `check_signing` is deliberately excluded from the execute pre-gate (`commands.rs:2269-2273`); the pre-gate is `check_self_pin` + `check_publish_order` only (`:2275-2278`). `create_signed_release_tag` runs the real `git tag -s` and returns git's own result (`git.rs:713-731`). `local_tag_is_verifiable` (`release.rs:979-995`) answers with two real git commands, not a guess. No new caller of `check_ssh_signing_viability` anywhere on this path. |
| **D-13** mutating commands refuse a resolved root ≠ invoking dir; read-only keeps the upward walk; the surface is positional `[PROJECT]` | **PARTIALLY HONORED** | Correct for the path topology (worktree, subdirectory, `.` default, trailing slash, symlinks — both sides canonicalized at `main.rs:727/758`), and read-only behavior is genuinely unchanged and proven by behavior. The refusal names both paths and both remedies including the literal `[PROJECT]`. **Not honored for:** the `GIT_DIR` bypass (CR-01), and D-13's own stated scope "anything that pushes, tags, or publishes" — only `Release{execute}` and `Sync` were converted (W-N4). |

---

## New hazards introduced by the fixes

These did not exist before the audit-fix pass. All are in code written under
time pressure to close a blocker, on irreversible surfaces — which is exactly
where they were predicted to be.

1. **CR-02** — `ReleaseOutcome::CompletedWithoutPublish` (the C-04 fix) marks
   the ledger Complete and prints remediation advice that starts a second
   release. The C-04 fix re-arms C-02.
2. **CR-04** — `git reset --hard "@{u}"` (the C-07 fix) destroys un-pushed
   local commits on a shared branch and violates D-05.
3. **CR-05** — the in-flight ledger (the C-02 fix) permanently bricks the
   release path after an ordinary phase Ship, with no tool verb and no
   documentation to resolve it; the only escape re-arms C-02.
4. **W-N1** — the ledger's version pin decouples the release version from the
   manifest for an unbounded time, widening the pre-existing W-03 window from
   "one invocation" to "however long the cut stays in flight".
5. **W-N2** — the resume path destroys the persisted record of what the
   previous run landed, before doing any work, undermining half of C-03.
6. **W-N5** — two new terminal refusals with no operator documentation
   anywhere, one of which offers no remedy at all.

---

## Critical Findings (5) — all block Ship

### CR-01 — `mutating_project_root` is bypassed by an inherited `GIT_DIR`: the guard passes while the executor acts on a repository the operator never named

- **File:** `crates/devflow-cli/src/main.rs:718-779` (the `--show-toplevel`
  call at `:730-741`, the comparison at `:766`)
- **Closes/breaks:** C-06 · **Confidence:** high · **Reproduced by execution**

The guard's own doc comment claims it "makes a silent redirect structurally
impossible rather than merely detected" (`main.rs:711-713`). It does not. It
compares two **paths**; it never establishes that the repository those git
calls will act on is the one at that path. `git rev-parse --show-toplevel`
reports the *work tree*, which defaults to the current directory when `GIT_DIR`
is set — while `HEAD`, refs, objects, index, and every push/tag come from
`GIT_DIR`.

Reproduced directly:

```
$ cd /tmp/.../gitdir-a          # on branch develop
$ git rev-parse --show-toplevel
/tmp/.../gitdir-a
$ GIT_DIR=/tmp/.../gitdir-b/.git git rev-parse --show-toplevel
/tmp/.../gitdir-a               # <-- the guard's input: unchanged, so it passes
$ GIT_DIR=/tmp/.../gitdir-b/.git git rev-parse --abbrev-ref HEAD
main                            # <-- what release.rs actually reads
$ GIT_DIR=/tmp/.../gitdir-b/.git git log --oneline -1
07e672e b                       # <-- repository B's history
```

And end-to-end against the built binary, from a repo A that is provably clean
and on `develop`:

```
$ devflow release --execute --yes-release
  ...
error: no git remote configured                       # guards ran on A

$ GIT_DIR=$S/gd-b/.git devflow release --execute --yes-release
  self-pin (workspace member versions) ✓  1 member pin(s) match 1.0.0
  crates.io publish order          ✓  publish in order: repro-core
error: working tree is not clean — ...                # guards ran on B
```

No "refusing to act on a repository you did not name" appears. The pre-gate
read repo A's `Cargo.toml`; the entry guards read repo B's index and HEAD. That
is C-06's exact failure shape — safety checks validating the wrong repository —
through a different vector.

**Why this is reachable, not exotic.** `git` itself sets `GIT_DIR` in the
environment of every hook process, and it is set by `git rebase --exec`,
`git bisect run`, `git filter-branch`, and `git submodule foreach`. This
repository runs hooks via `core.hooksPath=scripts/hooks`. Neither `release.rs`
nor `sync.rs` scrubs the git environment — production code uses bare
`Command::new("git")` (`release.rs:276`, `:291`, `:311`, `:326`; `sync.rs:209`,
`:224`), never the hermetic `test_support::git_command`. The project already
knows this: `project_root_guard.rs:18-21` states that "an inherited `GIT_DIR`
would retarget the very resolution under test" — and then tests the resolver
without ever setting one.

**Refutation attempted and failed.** I checked whether `GIT_WORK_TREE` alone
would fail closed (it does — `--show-toplevel` then reports the foreign work
tree and the paths mismatch). `GIT_DIR` alone is the failing case, and it is
the one git sets for you.

**Fix:**

```rust
// main.rs, mutating_project_root: scrub the redirecting variables on the
// resolution AND assert the repository, not just the path.
let output = std::process::Command::new("git")
    .args(["rev-parse", "--show-toplevel"])
    .current_dir(&start)
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .env_remove("GIT_COMMON_DIR")
    .env_remove("GIT_INDEX_FILE")
    .output()
```

…and scrub the same variables in `release.rs`/`sync.rs`'s own `Command::new`
sites (or route them through a shared hermetic constructor), since the guard
cannot vouch for calls it does not make. Add a `project_root_guard.rs` test
that sets `GIT_DIR` to a second repository and asserts the refusal names the
*invoking* repository.

---

### CR-02 — `CompletedWithoutPublish` exits 0, marks the ledger Complete, and its own printed remediation starts a second release: the tagged version is never publishable

- **File:** `crates/devflow-core/src/release.rs:900-928` (`finalize_ledger` at
  `:922`); rendering at `crates/devflow-cli/src/commands.rs:2325-2338`
- **Closes/breaks:** C-04 re-arms C-02 · **Confidence:** high ·
  **Reproduced by execution**

The C-04 fix stopped the false *string*. It did not stop the false *outcome*.
An empty publish order still: exits **0**, has already created and **pushed**
the signed release tag, and now additionally marks the ledger `complete` — so
the release is closed out with the registry having received nothing.

The CLI then tells the operator exactly what to do (`commands.rs:2333-2334`):
*"If this project publishes to crates.io, fix the workspace `members` list and
re-run."* Following that instruction is the C-02 defect.

Reproduced, both halves, against the built binary:

```
RUN 1 (members points at an absent path):
  version bump                     ⚠  version file already declared 0.1.0 ...
  signed release tag               ✓  created and pushed the signed release tag v0.1.0
  sync main back into develop      ⚠  origin/main is already an ancestor ...
  crates.io publish                ⚠  no workspace members were resolved — NOTHING was published
release tag cut: 0.1.0 (tag v0.1.0) — but NOTHING was published ...
        exit 0;  remote tags: v0.1.0
        ledger:  "status": "complete", "version": "0.1.0"

RUN 2 (following the CLI's own instruction — fix members, re-run):
  crates.io publish order          ✓  publish in order: cwp-foo
  version bump                     ✓  wrote and committed version 0.1.1, pushed develop to origin
  signed release tag               ⚠  halted at the human gate: origin/main declares 0.1.0,
                                      release version is 0.1.1 ...
        remote tags: v0.1.0        <-- 0.1.0 still tagged, still unpublished, now unreachable
```

A version bump nobody asked for is pushed to the shared `origin/develop`, and
`v0.1.0` — tagged and pushed on `origin` — can never be published by this tool,
because the ledger says that cut finished and the recomputed version has moved
past it. This is the same permanent split-state C-02 described, produced by
following the C-04 fix's own advice.

**Refutation attempted and failed.** I checked whether `LastReleaseCompleted`
catches the re-run: it does not, because fixing the `members` list is a commit,
so `HEAD` moves and the corroboration at `release.rs:554-567` correctly
concludes "new work landed" and falls through to a fresh computation.

**Fix:** treat "tag pushed but registry received nothing" as a terminal state
that is **not** complete. Either (a) leave the ledger `InFlight` on
`CompletedWithoutPublish` so a re-run resumes the *same* cut once the members
list is fixed, or (b) exit non-zero and refuse to finalize. Option (a) is the
one that makes the printed remediation true. Do not leave both the exit code
and the ledger saying "done".

---

### CR-03 — `members_key_offset` still latches onto a commented-out or non-`[workspace]` `members` key: the publish set is silently truncated and the pre-gate reports ✓

- **File:** `crates/devflow-core/src/git.rs:801-817`
  (`workspace_member_paths` at `:772-792`)
- **Closes/breaks:** C-05 · **Confidence:** high · **Reproduced by execution**

The fix ruled out exactly one shape — a key ending in `members` preceded by a
non-whitespace character. It left the scan position-blind in every other
respect: it never checks that the match is inside `[workspace]`, and it accepts
a match preceded by a space, which is what a comment line looks like.

Reproduced with a root manifest carrying a commented-out `members` line above
the real one:

```toml
# members = ["crates/cli"]
[workspace]
members = ["crates/core", "crates/cli"]
```

```
$ devflow release --check .
  crates.io publish order          ✓  publish in order: repro-cli
```

The correct order is `repro-core -> repro-cli`. The pre-gate reports **✓**.
`repro-cli` would be published depending on an unpublished `repro-core` — the
exact C-05 outcome, on the irreversible step, silent in both the pre-gate and
the final report. `[workspace.metadata.*] members = [...]` and
`[package.metadata.*] members = [...]` appearing earlier in the file behave the
same way.

**Refutation attempted and failed.** I confirmed the C-05 test
(`workspace_member_paths_ignores_default_members`, `git.rs:2226-2258`) is a
real test that genuinely fails against the old code — it just tests one input
shape. I also confirmed today's real root `Cargo.toml` is unaffected. The
trigger is a single ordinary editing habit (commenting a line out while
reorganizing a workspace), not an attack.

**Fix:** make the scan section-aware and comment-aware rather than adding a
third special case:

```rust
fn members_key_offset(contents: &str) -> Option<usize> {
    let mut offset = 0usize;
    let mut in_workspace = false;
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_workspace = trimmed.starts_with("[workspace]");
        } else if in_workspace && !trimmed.starts_with('#') {
            if let Some(rest) = trimmed.strip_prefix("members")
                && rest.trim_start().starts_with('=')
            {
                return Some(offset + (line.len() - trimmed.len()));
            }
        }
        offset += line.len() + 1;
    }
    None
}
```

Add cases for a leading comment line and for a `members` key in a
`[workspace.metadata.*]` table.

---

### CR-04 — README's repaired manual procedure prescribes `git reset --hard "@{u}"`, which destroys un-pushed commits on `develop` and is the compensating action D-05 forbids

- **File:** `README.md:56-64` (the reset at `:62`) vs
  `crates/devflow-core/src/sync.rs:46-58`
- **Closes/breaks:** C-07 · **Confidence:** high

The C-07 defect is genuinely fixed — the fetch and the `HEAD^{tree}`
before/after comparison are present, and the inverted `git diff --stat` signal
is called out by name. The abort path is new, and it is worse than what it
replaced:

```bash
if [ "$before" != "$after" ]; then
  echo "ABORT: the merge changed develop's tree — do not push" >&2
  git reset --hard "@{u}"                # discard the merge; investigate by hand
```

Two independent defects:

1. **`@{u}` is `origin/develop`, not the pre-merge local `HEAD`.** The snippet
   ran `git fetch origin main develop` two lines earlier, so `@{u}` is the
   freshly-fetched remote tip. `git reset --hard @{u}` therefore discards
   **every local commit on `develop` not yet pushed** — not just the merge. The
   state in which an operator performs this repair is precisely the state in
   which local-only commits exist (a manual release bump, or work in progress).
   Recovery is reflog-only, and the snippet gives no warning.
2. **It is exactly the action D-05 forbids.** `sync.rs`'s `TreeChanged` doc
   comment (`sync.rs:46-58`) states: *"The LOCAL merge commit is deliberately
   left in place for the operator to inspect — undoing it would require a hard
   reset, which D-05 forbids as an automatic compensating action. Do not 'fix'
   this later into a reset."* The README now claims to document "what
   `devflow sync` actually does" (`README.md:45-46`) while prescribing the one
   thing `sync` refuses to do.

The merge in the snippet has already committed (no `--no-commit`), so
`git merge --abort` will not work either — `ORIG_HEAD` is the only correct
target if a discard is wanted at all.

**Fix:** match `sync.rs`'s actual behavior — stop, do not undo:

```bash
if [ "$before" != "$after" ]; then
  echo "ABORT: the merge changed develop's tree — do not push." >&2
  echo "The local merge commit is left in place on purpose. Inspect it with" >&2
  echo "  git show HEAD --stat" >&2
  echo "and decide by hand. devflow sync deliberately does not undo it (D-05)." >&2
  exit 1
fi
git push origin develop                # never --force
```

If a discard must be offered, it is `git reset --hard ORIG_HEAD`, with an
explicit warning — never `@{u}`.

---

### CR-05 — an in-flight ledger permanently bricks the release path after an ordinary phase Ship, and the only escape re-arms C-02

- **File:** `crates/devflow-core/src/release.rs:575-611` (the
  `LedgerContradicted` guard at `:583-605`), interacting with
  `crates/devflow-core/src/hooks.rs:327-328`
- **New hazard introduced by the C-02 fix** · **Confidence:** high ·
  **Reproduced by execution**

`HaltedAtHumanGate` deliberately leaves the ledger `InFlight` (`release.rs:486-488`)
— and that halt is the **designed outcome of every first release invocation**,
not an error path. The in-flight version pin is then permanent: the ledger has
no `clear`, by explicit design (`release_ledger.rs:316-318`).

Meanwhile `hooks::version_bump` creates a `v{version}` tag on `develop` at the
end of **every** phase's Ship stage (`hooks.rs:327-328`), and that tag is
reachable from `HEAD`. `reachable_semver_baseline` reads exactly
`git tag --merged HEAD` (`version.rs:216-217`). So the first phase that ships
while a release is in flight pushes the reachable baseline above the pinned
version and trips `LedgerContradicted` — forever.

Reproduced against the built binary:

```
RUN 1 (the designed first invocation):
  version bump          ✓  wrote and committed version 0.1.0, pushed develop to origin
  signed release tag    ⚠  halted at the human gate ...
        exit 0;  ledger: "status": "inflight", "version": "0.1.0"

(an ordinary phase Ship happens — hooks::version_bump tags v0.2.0 on develop)

RUN 2:  error: the release ledger at .../devflow-release-ledger.json records an
        in-flight release at version 0.1.0, but the highest semver tag reachable
        from HEAD is v0.2.0, which is already past it — refusing to act on either.
        Inspect the ledger and the repository; devflow deletes, re-points, and
        force-updates nothing

RUN 3:  (identical — permanent)
```

The refusal is evaluated **before** the human gate, so the remedy the operator
would naturally reach for — merge the release PR and re-run, which is what
CONTRIBUTING.md:269 tells them to do — does not clear it either. There is no
CLI verb to resolve it and no documentation anywhere (see W-N5).

The only escape is to hand-delete `.git/devflow-release-ledger.json`. Doing so
returns the repository to the no-ledger state, where `compute_version` computes
a fresh version and `run_release` starts a **second release** — C-02 in full.
The C-02 fix's only failure remedy is the C-02 defect.

**Why Critical.** It does not itself push or tag wrongly, so it sits just
outside the literal bar. It is filed Critical because (a) it makes the phase's
headline capability unusable on this project's own ordinary dogfood loop, (b) it
strands a release half-cut with a bump already pushed to a shared branch, and
(c) its only escape hatch performs a wrong push. Downgrade to Warning only if
the operator is willing to accept manual `.git/` surgery as the documented
recovery.

**Fix (pick one, and document it):**
- Give the in-flight pin an explicit abandon path — e.g.
  `devflow release --abandon` writing a terminal `Abandoned` status (not
  deleting the file, which preserves D-06a's "never remove the record"), and
  name it in `LedgerContradicted`'s message; **or**
- narrow the contradiction test so a tag created by `hooks::version_bump`
  (which the executor knows about — `release.rs:402-406` already reasons about
  it) does not count as "reality moved past this cut". The current test uses a
  predicate that this project's own Ship stage trips on schedule.

At minimum, `LedgerContradicted`'s message must tell the operator what to *do*.
It currently says only what devflow will not do.

---

## Warning Findings (8 new/escalated)

**W-N1 — the ledger's version pin decouples the release version from the manifest for an unbounded time, and `cargo publish` ships the manifest's version, not the pin.**
`release.rs:575-611` (pin), `:641-643` (`write_needed` uses `<`, so a manifest
*ahead* of the pin is never corrected), `git.rs:1035-1039`
(`cargo publish -p <pkg>` takes no version and publishes whatever the manifest
declares), `release.rs:931` (`crate_already_published(pkg, pinned)` asks about
the pin). This is the prior review's W-03, escalated: pre-ledger the divergence
window was a single invocation; now it lasts as long as the cut stays in
flight, during which `hooks::version_bump` may rewrite the manifest version on
every phase Ship. The observable failure is `published pkg@X` reported for a
`cargo publish` that shipped `Y`. Fix: refuse when
`read_version(project_root) != pinned` on the resume path, naming both.

**W-N2 — a resume destroys the persisted record of what the previous run landed, before doing any work.**
`release.rs:618-634`: `resumed.steps.clear()` at `:621`, then an unconditional
`write` at `:634`. If run 1 published `devflow-core` and failed on `devflow`,
and run 2 then fails at step 1 (network, dirty tree, ledger I/O), the persisted
ledger no longer records the publish and `ReleaseFailure::steps` for run 2 is
empty — the operator has no record of the irreversible action. Live state
(crates.io) still knows, which is why this is a Warning and not a Critical, but
it removes half of what C-03 was fixed to provide. Fix: keep an
`append`-shaped history alongside the current run's list, or clear only after
the first successful step of the new run.

**W-N3 — the persisted ledger labels the human-gate halt as a `Tag` step with status `"skipped"`, for a tag that was never created.**
`release.rs:740-753` records `StepStatus::Skipped` with a halt detail;
`step_status_label` (`:451-456`) flattens it to `"skipped"`, whose documented
meaning is "live state already satisfied this step; nothing was done"
(`release.rs:79-82`). Nothing reads `steps` back today, so this is latent — but
it is a persisted record asserting a step was satisfied when git says
otherwise, which is precisely what D-06a forbids the ledger from doing. Fix:
add a distinct persisted label (`"halted"`) without touching the in-memory
`StepStatus` enum, which 26-08 prohibits extending.

**W-N4 — D-13's stated scope is broader than what was implemented.**
D-13 covers "anything that pushes, tags, or publishes". Only
`Command::Release{execute}` (`main.rs:635`) and `Command::Sync` (`:644`) use
`mutating_project_root`. `Ship`/`Advance`/`Start` still resolve through the
upward-walking `project_root` and reach `hooks_after_ship`
(`pipeline_outcomes.rs:570/608/652`), whose `VersionBump` creates a local tag
(`hooks.rs:328`) and whose `BranchCleanup` deletes local branches — in whatever
repository the upward walk landed on. Lower severity than C-06 (no remote
mutation on these paths), but it is the same class and D-13 names it.

**W-N5 — the release ledger has zero operator documentation.**
`rg -i ledger README.md CONTRIBUTING.md OPERATIONS.md` returns nothing. Two new
terminal refusals — `LastReleaseCompleted` (`release.rs:212-225`) and
`LedgerContradicted` (`:231-243`) — can now stop a release, and neither the
file's location (inside `.git/`, invisible to `git status` by design), nor its
lifecycle, nor any recovery procedure appears anywhere an operator would look.
`LedgerContradicted`'s message offers no remedy at all. This is what turns
CR-05 from an inconvenience into a dead end.

**W-N6 — no concurrency protection on the ledger.**
`release_ledger::write` (`:319-358`) is atomic per-write (temp + rename,
correctly done), but there is no lock and no compare-and-swap. Two concurrent
`release --execute` runs both read `Ok(None)` at `release.rs:536`, both compute
a version, and both proceed; the last writer wins the ledger. D-11 dropped the
`devflow parallel` tag race on the grounds that the operator does not run
phases concurrently — that reasoning does not extend to a persisted record
shared across every linked worktree via `--git-common-dir`
(`release_ledger.rs:211-215`).

**W-N7 — a ledger write failure aborts the run after an irreversible step.**
`record_step` (`release.rs:466-481`) propagates `LedgerError` via `?` at
`:479`, and it is called immediately after the tag push (`:794`) and after each
`cargo_publish` (`:948`). A read-only `.git`, a full disk, or a permissions
problem therefore converts a successful publish into a failed run. The
in-memory push happens first (`:477`), so `ReleaseFailure::steps` is still
correct — but the persisted record is not, and the operator sees a failure for
an action that succeeded.

**W-N8 — CONTRIBUTING's release section was updated for D-13 but not for the ledger or the prior review's W-19/W-20/W-21.**
`CONTRIBUTING.md:359-366` correctly documents the new mutating-root rule. But
`:345-358`'s "environment preconditions" still omits the two hard entry guards
(`DirtyWorkingTree`, `NotOnDevelop`) while `:252-254` still instructs the
operator to run `cargo build` — which trips the first one on the required
re-run. `:269` still says "re-run the same command once it merges" with no
mention that a re-run can now terminate in `LastReleaseCompleted` or
`LedgerContradicted`.

---

## Info Findings (2 new)

**IN-N1** — `ReleaseReport::steps`' one-entry-per-step contract
(`release.rs:127`) is now enforced for the resume arm (the C-01 commit folded
the duplicate `VersionBump` into step 1's own report and added an assertion),
but the publish loop still emits one entry per package (`release.rs:930-963`),
so `steps.iter().find(|s| s.step == Publish)` still retrieves only the first
package's verdict. The prior review's W-18 is therefore half-closed.

**IN-N2** — `parse_bare_version` (`release.rs:345-349`) round-trips the ledger's
`version` field through a synthetic TOML document. It is fed a value that a
human may have hand-edited (`release.rs:576`). Failures surface as
`VersionError::Parse`, which is safe, but the error will name a synthetic
`release-tag` path rather than the ledger, which will confuse diagnosis.

---

## Known-open findings carried forward (not re-litigated)

Verified still correctly scoped; escalations noted.

| ID | Scope | Current severity |
|----|-------|------------------|
| **WR-02** | `release_tag_state` check 4 (`git.rs:640-645`) asks "is the released commit an ancestor of the tag", so a tag ahead of `origin/main` reports `Released` and the tag step is skipped. Unchanged. The ledger does not lean on this predicate for anything new. | Warning — **not escalated** |
| **WR-04** | Publish-existence rests on `cargo info` stderr substring matching (`git.rs:945-957`). Unchanged, and still fails safe (`Ambiguous` → `Err`). **Escalated in one respect:** the ledger now supplies the *version* this predicate is asked about (`release.rs:931`), and that version can diverge from the manifest — see W-N1. The predicate itself was not weakened; what it is asked about became less trustworthy. | Warning — **escalated via W-N1** |
| **WR-05** | `classify_validate_outcome` accepts `verdict: Pass` without checking `status == Success`. Untouched by this phase's fixes. | Warning |
| **WR-06**, **WR-07** | Unchanged. | Warning |
| **IN-01** | `hooks_after_ship`'s `VersionBump` and the executor's signed tag share the `v{version}` namespace through two independent code paths. **Escalated:** this is no longer only a namespace collision — `hooks.rs:328`'s tag is now the trigger for CR-05's permanent deadlock, because `reachable_semver_baseline` cannot distinguish it from a release tag. | Info → **contributing cause of a Critical** |
| **W-17** | The live `develop` ruleset `develop-merge-or-squash` is `enforcement: active` with an empty bypass list, so step 1's direct push cannot land against this repository and all three `26-UAT.md` items are gated on it. **Not a code change** — a repository-settings action for the operator. Nothing in this pass changes it. | Open, operator action |

Also still open from the prior review and **not** fixed by the audit-fix pass:
**W-01** (unsanitized subprocess stderr on the `GitError`/`VersionError`
`#[from]` paths — `release.rs:143-146` still carry raw payloads),
**W-02**, **W-03** (see W-N1), **W-04**, **W-05**, **W-06**, **W-07**,
**W-08** (`HaltedAtHumanGate` exits 0 — confirmed by reproduction this pass),
**W-09** through **W-16**, **W-18** (see IN-N1), **W-19**–**W-21** (see W-N8),
**W-22**, **W-23**, **W-24**. The audit-fix pass explicitly scoped these out
(`--max 5` exhausted by the high-severity set); recording them here so the
record is not lost.

---

## Prior review's Audit-Fix Pass table (carried forward verbatim)

Five of the seven Criticals were classified auto-fixable and fixed, each with a
test that failed against the prior code, each committed atomically with its
finding ID. Gate re-run after the last commit at that time: 436 lib tests /
0 failed, 302 CLI tests across 16 targets / 0 failed, clippy clean, fmt clean.

| ID | Status | Commit | Fix |
|----|--------|--------|-----|
| C-05 | fixed | `e4a3236` | `members_key_offset` matches the exact `members` key, so `default-members` can no longer truncate the publish set. New test asserts both key orderings and the only-`default-members` case. |
| C-03 | fixed | `43a7a96` | `execute_release` returns `ReleaseFailure { error, steps }`; the sequence moved into `run_release`, which borrows the ledger, so every failure path reports what already landed. The CLI prints the ledger on both paths and states the steps are not rolled back. |
| C-04 | fixed | `8f5f2d1` | New `ReleaseOutcome::CompletedWithoutPublish`; the CLI can no longer print an unqualified "release cut complete" when the registry received nothing. Deliberately not an error — `publish_order` reads a Cargo workspace's `members`, so single-crate and non-Rust projects legitimately resolve to none. |
| C-01 | fixed | `7bd9a37` | The `UnreachableBaseline` arm resolves the tag's target commit and resumes only when it names `origin/main`'s tip; anything else is `StrayBaselineTag`, refused before any mutation. New test asserts the remote `develop` ref and the version file are byte-identical across the refusal. **Also fixes WR-01/W-18** — the resume note folds into step 1's own report instead of a second `VersionBump` entry. |
| C-07 | fixed | `0f0e17a` | README's manual repair now fetches, captures `HEAD^{tree}` before and after, aborts on a tree change, and pushes only when the merge is content-neutral. The inverted `git diff --stat` check is called out explicitly so it is not reintroduced. |

Escalated at the time, then closed by plans `26-08` (C-02, unblocked by D-06a)
and `26-09` (C-06, unblocked by D-13). Both plans landed; both are audited
above.

---

## Hypotheses raised and refuted (carried forward, plus this pass's)

From the 2026-07-29 pass — still valid, premises unchanged by the new code:

- **Credentials in the remote URL do not leak.** git 2.55 anonymizes userinfo
  in both `unable to access` and `Authentication failed for`, verified against
  a local 401 server. Downgraded to a documentation defect.
- **No shell interpolation anywhere** — all subprocess calls use argv vectors.
  No reachable argument injection. No `git add -A` / `commit_all` in any
  release path. No new network dependencies, no telemetry, no env capture, no
  hardcoded secrets.
- **`cargo info --registry crates-io` does not resolve a same-named local
  workspace member** (exit 101, `could not find`), and `@X.Y.Z` is exact, not a
  caret range.
- **`commit_path`'s "nothing to commit" → `Ok` masking is unreachable** when
  `write_needed` is true.
- **The `.unwrap_or(false)` degradations in `is_ancestor` /
  `local_tag_is_verifiable` all degrade toward "act"/"refuse", never "skip".**
- **CI itself is sound for this angle.** No workflow or script changed;
  `scripts/check.sh test` runs `cargo test --workspace --no-fail-fast` with no
  name filter.
- **The phase's own artifacts are honest.** No SUMMARY claims a merge, tag,
  publish, PR, or deletion that did not happen.

Raised and refuted **this** pass:

- **Does the ledger ever cause a step to be skipped?** No. Every step retains
  its live predicate; the ledger supplies only the version and the in-flight
  flag. `a_ledger_claiming_a_step_completed_does_not_skip_it`
  (`release.rs:1941-1969`) plants a ledger asserting the tag step completed and
  asserts the tag is really created. D-06a's inversion is not present.
- **Did a signing-viability predictor come back (D-10)?** No. The execute
  pre-gate is `check_self_pin` + `check_publish_order` only; `check_signing` is
  excluded with a comment citing D-10.
- **Did the ledger introduce a compensating action (D-05)?** No — in code.
  There is no `clear`/`remove`, and no path un-does anything. (The violation is
  in `README.md` — CR-04.)
- **Is the C-01 refusal actually before the mutation?** Yes.
  `compute_release_version` is called at `release.rs:544/566`, the first ledger
  write is `:634`, the first write to disk is `:645`. The `git fetch` at `:526`
  precedes it, as before.
- **Can a `Complete` ledger block a legitimate release?** No.
  `head_at_completion` is corroborated against live `git rev-parse HEAD`
  (`:551-567`); any new commit on `develop` clears the refusal. Verified by
  reproduction.
- **Does `GIT_WORK_TREE` also bypass the root guard?** No — it fails closed
  (`--show-toplevel` reports the foreign work tree, the paths mismatch, the
  guard refuses). Only `GIT_DIR` bypasses it. Verified experimentally.
- **Does the ledger's path resolution break under linked worktrees?** No.
  `--git-common-dir` is the correct scope and `ledger_path`
  (`release_ledger.rs:219-251`) handles both the relative (`.git`) and absolute
  (linked worktree) forms, refusing rather than falling back to the working
  tree. The `ledger_is_invisible_to_git_status` test asserts git's real answer,
  not the shape of the path.
- **Is `sanitize_changelog_subject` applied on the ledger round trip?**
  Yes — `write` re-sanitizes every operator-visible field
  (`release_ledger.rs:325-345`), so a hand-edited ledger cannot reintroduce
  control characters into terminal output.

---

## Recommendation

**Do not ship. Ship gate: BLOCKED.**

A clean verdict here would have been a surprising result, and it is not one I
reached. Five Criticals, four reproduced by execution.

The fixes are not bad work — C-01 is properly closed, the ledger is a
thoughtfully designed and genuinely well-tested piece of code (versioned,
atomic, refuses corrupt input, provably never a skip source), and the root
guard closes the path topology it was written for. The problem is a consistent
pattern: **each fix closed the instance that was reported and left the class
open**, and three of them added new hazards on irreversible surfaces.

The four that must be fixed before any release is cut with this tool:

- **CR-01** — the root guard does not actually guarantee the repository. One
  `.env_remove` per call site plus a test. Small fix, highest leverage.
- **CR-02** — do not mark a cut Complete when the registry received nothing,
  and do not print remediation advice that starts a second release.
- **CR-03** — make the manifest scan section- and comment-aware instead of
  adding a third special case.
- **CR-04** — delete the `git reset --hard "@{u}"` line. It is one line, it can
  destroy an operator's un-pushed work, and it contradicts the module the
  document claims parity with.

**CR-05** needs an operator decision, not a unilateral fix: either an explicit
abandon path for an in-flight cut, or a narrower contradiction predicate. It
should not be closed by loosening `LedgerContradicted` into a fall-through —
that is the C-02 inversion.

Two structural observations for whoever picks these up:

1. **Three of the five Criticals are the same bug**: a predicate that answers a
   *nearby* question and is trusted to answer the real one. `--show-toplevel`
   answers "which work tree", not "which repository" (CR-01). `find("members")`
   answers "where does this text appear", not "where is this key" (CR-03).
   `reachable_semver_baseline` answers "highest reachable tag", not "has a
   release superseded this one" (CR-05). Worth a targeted sweep of the other
   predicates on this path — WR-02 and WR-04 are the same shape and are still
   open.
2. **The suite cannot see any of this.** All 446 + 234 tests pass, clippy and
   fmt are clean, and every Critical above is reachable on an ordinary operator
   path. The three fixture gaps that hid them — no test sets `GIT_DIR` against
   the guard, no test follows the `CompletedWithoutPublish` remediation, no
   test ships a phase while a release is in flight — are cheap to close and
   should be closed as part of the fixes, not after.

**W-17 remains open and is not a code change.** The live `develop` ruleset is
`enforcement: active` with an empty bypass list, so step 1's direct push cannot
land against this repository and all three `26-UAT.md` items remain gated on a
precondition proven absent. None of the above can be UAT-verified against the
real `origin` until the operator resolves it.

---

_Reviewed: 2026-07-30T15:05:00Z_
_Reviewer: Claude (gsd-code-reviewer), adversarial re-review_
_Depth: deep — fix-diff audit + current-state read + executable reproduction_

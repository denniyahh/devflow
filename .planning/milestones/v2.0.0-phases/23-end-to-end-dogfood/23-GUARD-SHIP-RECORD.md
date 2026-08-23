# 23-13: Guard Ship Record

Tracks the guard (23-12) reaching `origin/develop`, the operator merge, and
the rebuilt binary's runtime proof. Extended across Task 1 → Task 3.

## Task 1: Pull request opened, CI green

- **Working branch** (read via `git rev-parse --abbrev-ref HEAD`):
  `feature/phase-23`
- **Pushed head SHA:** `2f8686efdc7eefe26fc4fadbc6b170372030ee10`
  (`git push --force-with-lease origin HEAD` — created the remote branch;
  it did not previously exist on origin, so this was a create, not an
  overwrite. The project's tracked pre-push hook
  (`scripts/hooks/pre-push` → `scripts/check-in-container.sh all`, run
  inside the pinned devcontainer) ran `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, and `cargo test --workspace
  --no-fail-fast` before the push left the machine; all reported clean —
  `check.sh: all OK`.)
- **Guard commits carried by this PR** (all confirmed ancestors of the
  pushed head via `git merge-base --is-ancestor <sha> HEAD`):
  - `fdc0a3d` (feat) — end-to-end "devflow start refuses an unreachable
    phase and scaffolds nothing"
  - `fbf535b` (test) — discrimination / fail-open unit tests
  - `9ebde0b` (docs) — CHANGELOG.md / OPERATIONS.md reconciliation
- **Pull request:** [PR #32](https://github.com/denniyahh/devflow/pull/32)
  — `feat(23-12): refuse devflow start on an unreachable phase`
  - `gh pr view 32 --json number,baseRefName,state,url`:
    `{"baseRefName":"develop","number":32,"state":"OPEN","url":"https://github.com/denniyahh/devflow/pull/32"}`
- **CI conclusion** (`gh pr checks 32`, final poll, verbatim — two runs
  appear per check because the push triggered two workflow dispatches;
  both are reported and both pass):

  ```
  Build + test in devcontainer	pass	1m53s	https://github.com/denniyahh/devflow/actions/runs/30219853962/job/89840301319
  Build + test in devcontainer	pass	1m56s	https://github.com/denniyahh/devflow/actions/runs/30219871702/job/89840347041
  Clippy	pass	59s	https://github.com/denniyahh/devflow/actions/runs/30219853945/job/89840299677
  Clippy	pass	1m0s	https://github.com/denniyahh/devflow/actions/runs/30219871706/job/89840346938
  Format	pass	50s	https://github.com/denniyahh/devflow/actions/runs/30219853945/job/89840299725
  Format	pass	50s	https://github.com/denniyahh/devflow/actions/runs/30219871706/job/89840346924
  Test	pass	1m35s	https://github.com/denniyahh/devflow/actions/runs/30219853945/job/89840299666
  Test	pass	1m45s	https://github.com/denniyahh/devflow/actions/runs/30219871706/job/89840346945
  ```

  No check pending or failing at time of recording.
- **`origin/develop` untouched by this task:**
  - Before: `git log origin/develop --oneline -1` (checked before Task 1's
    push) → `06824c2 Merge pull request #31 from denniyahh/feature/phase-23`
  - After: `git fetch origin && git log origin/develop --oneline -1`
    (checked after PR #32 opened and CI confirmed green) →
    `06824c2 Merge pull request #31 from denniyahh/feature/phase-23`
  - Identical. No autonomous write reached `develop`.
- **PR body** cites 23-FINDINGS.md §B1 ("A third precondition class the
  setup did not check") and names the 2026-07-26 acceptance failure by
  date, states the fail-open decision (no `.planning/ROADMAP.md` on the
  base branch at all → not guarded, matching every pre-existing
  `phase7_cli.rs` fixture), and quotes 23-12 Task 3's reported gate counts
  verbatim (`17 passed; 0 failed` for `phase7_cli`; `608 passed; 0 failed;
  0 ignored` for the full workspace suite; clippy and fmt both clean)
  rather than asserting "tests pass."
- **Redaction grep and a documented interpretation call:** the operator's
  OS username and their GitHub account name are the identical string
  (`denniyahh` — confirmed via `whoami`/`getent passwd` vs `gh auth
  status`), and this repository is public
  (`gh repo view --json visibility` → `PUBLIC`) with every prior release
  already linked unredacted in this project's own committed docs (e.g.
  `.planning/STATE.md` cites
  `https://github.com/denniyahh/devflow/releases/tag/v1.8.0` plainly).
  Literally scrubbing the string `denniyahh` would also delete the PR
  URL this same task's acceptance criteria requires recording. Read
  narrowly against the 999.10 leak class this checklist exists to catch
  (an operator's *local filesystem path* — home directory, absolute
  path, tempdir — leaking into a committed artifact), the grep run was:
  `rg -n '/home/denniyahh|/var/home/denniyahh|/tmp/' .planning/phases/23-end-to-end-dogfood/23-GUARD-SHIP-RECORD.md`
  → no match at the point this grep was run. The public GitHub account
  name in the PR/release URL is retained, consistent with every other
  committed planning artifact in this repository, and is not the leak
  class this checklist targets.

**Do not merge.** Task 1 stops here — no `gh pr merge`, no `git merge`,
no `git push origin develop` was run.

## Task 2: Operator merge into `develop`

**Operator response, verbatim:**

```
merged 0dad20d3e85d82d60235b8f91cb944e4cbed433c
```

- **Merge commit SHA:** `0dad20d3e85d82d60235b8f91cb944e4cbed433c`

**Ancestry check, run after the executor's own `git fetch origin`** (the
operator's confirmation names the same SHA independently confirmed by the
orchestrator before this task began):

```
$ git fetch origin
$ git merge-base --is-ancestor 2f8686efdc7eefe26fc4fadbc6b170372030ee10 origin/develop
$ echo "exit: $?"
exit: 0
```

The guard head SHA (`2f8686e…` — Task 1's pushed head, carrying commits
`fdc0a3d`, `fbf535b`, `9ebde0b`) is confirmed an ancestor of `origin/develop`.

**`origin/develop` tip after fetch:**

```
$ git log -1 --format='%H %an %s' origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c Dennis Kim Merge pull request #32 from denniyahh/feature/phase-23
```

The subject line names "Merge pull request #32" — the same PR Task 1 opened
and recorded — confirming the merge landed through the pull-request path,
not a direct push. This matches `gh pr view 32`'s independently-reported
`state: MERGED`, `mergedAt: 2026-07-26T21:02:25Z`,
`mergeCommit.oid: 0dad20d3e85d82d60235b8f91cb944e4cbed433c`.

**No command run by the executor in this task wrote to `develop`.** The only
git operations performed in Task 2 were `git fetch origin` (read-only,
updates remote-tracking refs only) and `git merge-base --is-ancestor` (a
read-only query). The merge itself was performed by the operator through the
GitHub pull-request UI, outside this executor's control, per the plan's one
hard rule.

---

### Finding 1 — the local `develop` ref is stale, and it is the ref the guard actually reads

**This is load-bearing, not a housekeeping note.** `ensure_phase_reachable_on_base`
is called at `crates/devflow-cli/src/commands.rs:146` as:

```rust
ensure_phase_reachable_on_base(project_root, phase, DEVELOP)?;
```

`DEVELOP` (`crates/devflow-core/src/config.rs:17`) is the **literal string
`"develop"`** — the local branch name, not `origin/develop`. This is the same
constant `GitFlow::feature_start` uses for `git checkout develop` and the
same one `ensure_phase_worktree` passes to `worktree::add` as `start_point`.
The guard resolves this with `git rev-parse --verify --quiet develop` and
reads `.planning/ROADMAP.md` via `git show develop:...` — both against the
**local** ref, never the remote-tracking one.

**Measured, this repository, after the fetch above:**

```
$ git rev-parse develop
e0f87c2c2230257f7aa8092a836225626941d09a
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-list --left-right --count develop...origin/develop
0	120
$ git merge-base --is-ancestor develop origin/develop; echo "exit: $?"
exit: 0
```

Local `develop` is **0 ahead / 120 behind** `origin/develop` — a strict
ancestor, a pure fast-forward gap, not a divergence. But it is 120 commits
behind, and PR #32's merge is one of those 120 commits local `develop` does
not yet have.

**Consequence, measured directly on the ROADMAP heading Task 3 needs:**

```
$ git show develop:.planning/ROADMAP.md | rg -c '^### Phase 24:'
(no match — 0)
$ git show origin/develop:.planning/ROADMAP.md | rg -c '^### Phase 24:'
1
```

Local `develop` does not contain phase 24's ROADMAP heading; `origin/develop`
does. If the acceptance run's `devflow start --phase 24` were launched right
now, exactly as this project's binary calls it — against local `develop`,
per `commands.rs:146` — it would consult a ref that does **not** yet contain
the phase-24 heading Task 3 confirms below, and would refuse. Confirming
reachability against `origin/develop` alone, as Task 3's own acceptance
criteria literally specify, would be a **false green**: it records "phase 24
is reachable" while the binary at launch consults the stale local ref and
would in fact refuse.

**Remedy, stated but deliberately not applied here:** fast-forwarding local
`develop` to `origin/develop` (`git fetch origin && git checkout develop &&
git merge --ff-only origin/develop`, or equivalent) would close this gap.
This executor does **not** apply that remedy in this task. Re-measuring
launch preconditions immediately before the acceptance attempt is 23-14's
job, and applying the fix here — before that plan's own measurement step
runs — would destroy the evidence that 23-14's re-measure step actually
catches this condition. The orchestrator has deliberately deferred the fix
to 23-14, and this record states that deferral explicitly so a later reader
does not mistake the omission for an oversight.

### Finding 2 — commit signing is configured but disabled (noted, not acted on)

`gpg.format` is `ssh` and `user.signingkey` is set in this repository's git
config, but `commit.gpgsign` and `tag.gpgsign` are both `false`. Every commit
made across this phase is therefore unsigned:

```
$ git log -1 --format='%G?' fdc0a3d
N
$ git log -1 --format='%G?' 2f8686e
N
```

(`%G?` = `N`, "no signature," for both a 23-12 guard commit and the Task 1
push head.) This is out of scope for this plan's guard-shipping goal and no
git config was changed to investigate or fix it. Noted here only because
this section's provenance claims (branch state, ancestry, merge path) are
exactly the kind of claim commit signing would otherwise corroborate — its
absence does not weaken any claim above, since every claim here is a direct
git measurement (SHA equality, `--is-ancestor` exit status), not an appeal
to signature trust.

**Redaction grep, Task 2 section:**
`rg -n '/home/denniyahh|/var/home/denniyahh|/tmp/' .planning/phases/23-end-to-end-dogfood/23-GUARD-SHIP-RECORD.md`
— the only match at the point this grep was run is the grep command itself,
quoted verbatim inside Task 1's pre-existing redaction note. No new path
leaked by this section.

## Task 3: Rebuild from the merged tip, prove the guard at runtime

**Fetch and confirm the base** (re-run independently in this task, per the
plan's instruction to treat every claim as its own measurement):

```
$ git fetch origin
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git merge-base --is-ancestor 2f8686efdc7eefe26fc4fadbc6b170372030ee10 origin/develop
$ echo "exit: $?"
exit: 0
```

The guard head SHA is confirmed an ancestor of `origin/develop`, again.

**What tree the rebuild was actually built from.** This task's build ran
from the working branch `feature/phase-23` at `0c9dcfe` (the commit that
just recorded Task 2 above), not from a checkout of `origin/develop`
itself. The equivalence is demonstrated, not asserted:

```
$ git rev-parse --abbrev-ref HEAD
feature/phase-23
$ git diff origin/develop HEAD -- crates/ Cargo.toml Cargo.lock | wc -l
0
```

Zero lines of diff across every path that can affect the compiled binary
(`crates/`, `Cargo.toml`, `Cargo.lock`) between this HEAD and
`origin/develop`. The only commits on `feature/phase-23` not on
`origin/develop` are this plan's own planning-doc commits (Task 1's
`7f055c5` and Task 2's `0c9dcfe`, both touching only
`.planning/phases/23-end-to-end-dogfood/23-GUARD-SHIP-RECORD.md`), so the
compiled code built here is identical, byte-for-byte in source, to what
`origin/develop`'s tree contains. This is the same reasoning
`23-ACCEPTANCE-RUN.md`'s own binary-freshness precondition used
(`git diff --stat <prior-rebuild-sha> HEAD -- crates/ Cargo.toml Cargo.lock`),
reproduced here against `origin/develop` directly rather than against a
prior rebuild's SHA.

**Rebuild:**

```
$ cargo build --release
   Compiling devflow-core v1.8.1 (…/crates/devflow-core)
   Compiling devflow v1.8.1 (…/crates/devflow-cli)
    Finished `release` profile [optimized] target(s) in 16.39s
```

**Version and hash of the freshly built binary:**

```
$ ./target/release/devflow --version
devflow 1.8.1
$ sha256sum ./target/release/devflow
b5db079ad7c76a9e33d7f6b1bffa0b1caeedf208789f7f38353602628e26dc98  ./target/release/devflow
```

`1.8.1` matches the workspace `Cargo.toml` version.

**PATH-resolved binary comparison:**

```
$ command -v devflow
<homebrew-prefix>/bin/devflow
$ sha256sum "$(command -v devflow)"
b5db079ad7c76a9e33d7f6b1bffa0b1caeedf208789f7f38353602628e26dc98  <homebrew-prefix>/bin/devflow
```

**Match: yes.** The PATH-resolved `devflow`'s sha256 is byte-identical to
the freshly built `./target/release/devflow`'s sha256
(`b5db079a…6dc98` both). The binary the acceptance run would invoke via
PATH is the same binary just rebuilt from the merged tip — no
stale-binary mismatch to surface.

**Runtime guard proof, throwaway clone — not this repository.**

Cloned the current tree into a tempdir:

```
$ git clone --no-hardlinks --branch develop <project root> <tmpdir>
Cloning into '<tmpdir>'...
done.
$ cd <tmpdir> && git rev-parse --abbrev-ref HEAD && git rev-parse HEAD
develop
e0f87c2c2230257f7aa8092a836225626941d09a
```

Note on which `develop` this clones: `--branch develop` against a local
path clones the **local** `develop` ref (`e0f87c2…`), the same stale ref
Finding 1 measures above — not `origin/develop`. This is not a defect in
the proof: the guard being tested is compiled into the binary already (this
repository's HEAD is diff-empty against `origin/develop` for all code
paths, confirmed above), and what the clone supplies is real, git-backed
`ROADMAP.md`/phase-directory data for the binary's runtime probe to
inspect. For the phase-97 probe below, phase 97 is absent from every
`develop` tip this repository has ever had, local or remote, so the choice
of which `develop` tip is cloned does not affect this probe's validity.
This clone additionally happens to be a faithful reproduction of exactly
what the production binary consults today, since Finding 1 established
that `commands.rs:146` reads the local `develop` ref, not `origin/develop`.

**Pre-confirmation that phase 97 is genuinely absent, read-only, against the clone's `develop`:**

```
$ git show develop:.planning/ROADMAP.md | rg -c '^### Phase 97:'
(no match — rg -c exits 1, prints nothing: 0 occurrences)
$ git ls-tree -r --name-only develop -- .planning/phases/ | rg '^\.planning/phases/97-'
(no match — rg exits 1: no such path)
```

Both confirmed absent before the probe runs.

**The runtime invocation, verbatim, against the freshly built binary:**

```
$ ./target/release/devflow start --phase 97 --agent claude --mode auto <tmpdir>
```

**Exit status:** `1` (non-zero).

**Verbatim stderr:**

```
error: phase 97 is not reachable from `develop` — the branch `devflow start` forks its worktree from:
  missing: the `### Phase 97:` heading in `ROADMAP.md` on `develop`
  missing: a `.planning/phases/97-*/` directory on `develop`
a phase promoted only on another branch is invisible to this run — merge that branch into `develop` first, then re-run.
```

**Stdout:** empty.

The refusal fired: stderr contains both `is not reachable from` and
`develop`, matching this task's asserted contract. The message names each
missing half (`ROADMAP.md` heading, phase directory) and contains no
absolute filesystem path, no username — consistent with `23-12-PLAN.md`'s
stated no-path-leak requirement.

**Post-refusal scaffolding check, inside the clone:**

```
$ test -d .worktrees && echo EXISTS || echo absent
absent
$ test -f .devflow/state-97.json && echo EXISTS || echo absent
absent
$ git branch --list feature/phase-97
(empty)
```

No worktree, no state file, no feature branch. Nothing was scaffolded
before or during the refusal.

**Why a clone and not this repository, and why the complementary property
is not separately proven here:** if the guard were broken, this exact
command run against this repository's own working tree would have forked
a real worktree from `develop` and launched a real Claude session on a
phase that does not exist. Bounding that to a tempdir costs nothing in the
strength of the claim — it is the same binary, and the clone's `ROADMAP.md`
and phase-directory data are the real, git-tracked data this project
maintains, not a synthetic fixture. The complementary property — that the
guard does not over-refuse a *reachable* phase — is not independently
proven inside this task; it fails loudly and visibly instead, since 23-15's
own acceptance launch would be refused immediately were the guard
over-broad, which is a stronger, later-stage falsification than anything
this task could construct.

**Phase 24 reachability, confirmed read-only against `origin/develop` — not the local ref, not the working branch:**

```
$ git show origin/develop:.planning/ROADMAP.md | rg '^### Phase 24:'
### Phase 24: `release --check` Signing-Key Inline Classification
$ git ls-tree -r --name-only origin/develop -- .planning/phases/ | rg '^\.planning/phases/24-'
.planning/phases/24-release-check-signing-key-inline-classification/.gitkeep
```

Both halves present on `origin/develop`. The matched roadmap heading line
and the matched phase-directory path are quoted verbatim above.

**Finding — is a `.gitkeep`-only directory sufficient?** Yes, and this is
stated as an explicit finding rather than assumed. The phase-24 directory
on `origin/develop` currently holds only `.gitkeep`
(`git ls-tree -r --name-only origin/develop -- .planning/phases/24-*/`
lists exactly that one path and nothing else). The guard's contract, per
`23-12-PLAN.md`, is **directory presence**, not directory contents — the
same idiom `commands::phase_artifact_on_develop` already uses elsewhere in
this codebase. The acceptance run's own Define stage
(`/gsd-discuss-phase 24`) is what populates the directory with
`24-CONTEXT.md` and onward; the guard exists to stop a run from being
invisible to `devflow start` altogether, not to pre-validate that Define
has already happened. A `.gitkeep`-only directory therefore satisfies both
the guard's check and the acceptance run's actual needs — the same shape
`23-ACCEPTANCE-RUN.md` § 1 recorded as a pre-launch precondition
("`.planning/phases/24-*/` exists with only a `.gitkeep`") the last time
this phase reached this exact launch point, before the guard existed to
fail-open on it.

**Redaction check, Task 3 section, and the placeholders it required.** This
section's raw output named this executor's actual home-directory path
(`command -v devflow` and the throwaway clone's tempdir path); both are
replaced above with `<homebrew-prefix>` and `<tmpdir>` placeholders before
committing, per this phase's redaction checklist (999.10 leak class:
absolute home paths and tempdir paths). Grep run against the file as
committed:

```
$ rg -n '/home/denniyahh|/var/home/denniyahh|/tmp/' .planning/phases/23-end-to-end-dogfood/23-GUARD-SHIP-RECORD.md
```

Every matching line is one of this file's own three redaction-grep commands,
quoted verbatim inside Task 1's, Task 2's, and this Task 3's redaction
notes respectively (self-reference, since the grep command string itself
contains the pattern it searches for) — no operator path leaked into any
recorded command output.

**Cleanup, confirmed:**

```
$ rm -rf <tmpdir>
$ test -d <tmpdir> && echo "STILL EXISTS" || echo "removed"
removed
$ git status --porcelain
(empty)
```

The temporary clone is removed and this repository's working tree is
clean.

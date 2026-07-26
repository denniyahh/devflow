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

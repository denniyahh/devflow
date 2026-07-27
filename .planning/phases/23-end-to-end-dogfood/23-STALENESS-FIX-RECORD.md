# 23-16: Staleness Fix Record

Tracks the divergent-lineage staleness fix (23g) reaching `origin/develop`,
the operator merge, and the ancestry proof. Extended across Task 2 → Task 3.

## Task 2: Full gate chain, CHANGELOG, pull request opened, CI green

- **Working branch** (read via `git rev-parse --abbrev-ref HEAD`):
  `feature/phase-23`
- **Fix commit:** `c2e947a53e4781da9ee799beaba9e541d16781db`
  (`fix(23-16): content-check the divergent-lineage staleness arm`)

### Baseline vs. post-fix full-suite counts

Pre-Task-1 baseline, quoted from `23-VERIFICATION.md` / `23-ACCEPTANCE-RUN-2.md`
§11: **608 passed, 0 failed, 0 ignored**, across 17 binaries.

This task's own run, direct `&&` status chain (never a piped/grep shape that
falls through on a compile or link failure):

```
$ cargo test --workspace --no-fail-fast
```

Per-binary `test result:` lines, summed: 187 + 3 + 7 + 4 + 1 + 1 + 1 + 3 + 17
+ 8 + 2 + 9 + 1 + 363 + 2 + 2 + 0 = **611 passed, 0 failed, 0 ignored**, exit
0, no `test result: FAILED` line in the log.

**Delta: +3 passed, 0 failed, 0 ignored — exactly expected.** The `devflow`
binary's own count moved 184 → 187 (the 3 new tests this plan adds:
`divergent_lineage_docs_only_range_is_fresh`,
`divergent_lineage_with_source_change_is_stale`,
`enforce_build_staleness_does_not_block_self_dogfood_on_divergent_docs_only_lineage`).
No count disagreed in any other direction.

```
$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.74s
CLIPPY_EXIT=0
$ cargo fmt --check
FMT_EXIT=0
```

### `staleness::tests` module, isolated

```
$ cargo test -p devflow --bin devflow staleness::tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 166 filtered out
```

18 pre-existing + 3 new. `embedded_commit_is_stale_maps_ancestry_exit_codes`
(the "unknown/absent commit → Indeterminate" existing coverage, D-20) is
among the 18 and passes unchanged — no duplicate test added for it, per the
plan's own instruction.

### CHANGELOG.md

A `### Fixed` subsection was added under `## 2.0.0 — 2026-07-26`, after
`### Added`, naming the divergent-lineage arm, citing 21d/999.29 as the
mechanism it extends, citing the concrete 2026-07-26 incident, and stating
explicitly that a divergent range touching real build-affecting source
still blocks, unchanged. Cites `23g`.

### Pull request

**Note on sequencing:** the PR was opened right after the Task 1 fix commit
(`c2e947a`) was pushed, before the `CHANGELOG.md`/record commit
(`973d185`) existed. That ordering mistake was caught immediately — the
checkpoint's own `how-to-verify` requires the operator to read the
`CHANGELOG.md` entry inside the PR diff — and corrected by pushing
`973d185` onto the same branch before recording this section, so the PR
now carries both commits. The SHAs and CI results below are the FINAL
state, after both pushes.

- **Pushed head SHA (final):** `973d1859b8ad661c74407e20842c26aaadb61ce9`
  — carries both `c2e947a53e4781da9ee799beaba9e541d16781db`
  (`fix(23-16): content-check the divergent-lineage staleness arm`) and
  `973d1859b8ad661c74407e20842c26aaadb61ce9` (`docs(23-16): document fix,
  run full gate chain, open PR #33`).
  (`git push --force-with-lease origin HEAD`, both pushes — the tracked
  pre-push hook, `scripts/hooks/pre-push` → `scripts/check-in-container.sh
  all`, run inside the pinned devcontainer, ran `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test
  --workspace --no-fail-fast` before each push left the machine — both
  reported clean, `check.sh: all OK`.)
- **Pull request:** [PR #33](https://github.com/denniyahh/devflow/pull/33)
  — `fix(23-16): content-check divergent-lineage staleness arm`
  - `gh pr view 33 --json number,baseRefName,state,url`:
    `{"baseRefName":"develop","number":33,"state":"OPEN","url":"https://github.com/denniyahh/devflow/pull/33"}`
- **CI conclusion, final push (`973d185`)** (`gh pr checks 33`, final poll,
  verbatim — two runs appear per check because the push triggered two
  workflow dispatches):

  ```
  Build + test in devcontainer	pass	1m59s	https://github.com/denniyahh/devflow/actions/runs/30226345407/job/89857048937
  Build + test in devcontainer	pass	1m58s	https://github.com/denniyahh/devflow/actions/runs/30226346716/job/89857294029
  Clippy	pass	59s	https://github.com/denniyahh/devflow/actions/runs/30226345391/job/89857293814
  Clippy	pass	1m0s	https://github.com/denniyahh/devflow/actions/runs/30226346706/job/89857052610
  Format	pass	48s	https://github.com/denniyahh/devflow/actions/runs/30226345391/job/89857293915
  Format	pass	48s	https://github.com/denniyahh/devflow/actions/runs/30226346706/job/89857052612
  Test	pass	1m36s	https://github.com/denniyahh/devflow/actions/runs/30226345391/job/89857293310
  Test	pass	1m37s	https://github.com/denniyahh/devflow/actions/runs/30226346706/job/89857052606
  ```

  No check pending or failing at time of recording (`gh pr checks 33` exit
  0).

  **A finding, recorded rather than silently absorbed, occurring on BOTH
  pushes:** on the first push (`c2e947a`), one of the two `Test` job
  dispatches initially failed
  (`https://github.com/denniyahh/devflow/actions/runs/30226047300/job/89856287706`,
  `test result: FAILED. 362 passed; 1 failed`). On the second push
  (`973d185`), the SAME test failed again, this time on both a `Test`
  dispatch AND a `Build + test in devcontainer` dispatch
  (`.../30226345391/job/89857049024` and `.../30226346716/job/89857052649`
  respectively, both `test result: FAILED. 362 passed; 1 failed`). Every
  failure was the identical single test —
  `agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process`
  in `crates/devflow-core/src/agent.rs`, a file this plan does not touch.
  The test's own in-source comment (`agent.rs:303-309`) documents it as a
  pre-existing, previously-observed CI-intermittent flake ("first seen
  2026-07-26, on commits touching no Rust source... does not reproduce
  locally — 40/40 under CPU load"), unrelated to `staleness.rs` or the 23g
  fix — out of this plan's scope boundary (only issues directly caused by
  this plan's own changes are auto-fixable; this is a pre-existing failure
  in an unrelated file). This task's own local `cargo test --workspace`
  runs (three separate runs across this task, all quoted above or earlier
  in this file) never reproduced it. Each failed job's twin dispatch on
  the same push passed. All three failed jobs
  (`30226047300`/`Test`, `30226345391`/`Test`, `30226346716`/`Build + test
  in devcontainer`) were re-run via `gh run rerun <id> --failed` — no code
  was changed, no new commit was pushed between a failure and its rerun —
  and each passed cleanly on rerun (all quoted above in the final CI
  table). Filed as an out-of-scope, pre-existing flake; no
  `deferred-items.md` entry was needed beyond this record since the
  flake's own doc comment already names and tracks it in source.
- **PR body** cites `23-ACCEPTANCE-RUN-2.md` §10 by name, the
  `0c9dcfe`/`0dad20d` mutually-non-ancestor SHA pair, states that the
  operator rejected the develop-only-build workaround in favor of this
  source fix (and why), and quotes this task's reported gate counts
  verbatim rather than asserting "tests pass."
- **`origin/develop` untouched by this task:**
  - Before: `git log origin/develop --oneline -1` (checked before this
    plan's Task 1 dispatch, per the orchestrator's pre-dispatch check) →
    `0dad20d Merge pull request #32 from denniyahh/feature/phase-23`
  - After: `git fetch origin && git log origin/develop --oneline -1`
    (checked after PR #33's final push and CI confirmed green) →
    `0dad20d Merge pull request #32 from denniyahh/feature/phase-23`
  - Identical. No autonomous write reached `develop`.

### Redaction grep

The operator's OS username and their GitHub account name are the identical
string (`denniyahh`), and this repository is public
(`gh repo view --json visibility` → `PUBLIC`), with every prior PR/release
already linked unredacted in this project's own committed docs (e.g.
`23-GUARD-SHIP-RECORD.md` cites `https://github.com/denniyahh/devflow/pull/32`
plainly). Literally scrubbing the string `denniyahh` would also delete the
PR URL this task's own acceptance criteria require recording. Read narrowly
against the 999.10 leak class this checklist exists to catch (an operator's
*local filesystem path* — home directory, absolute path, tempdir — leaking
into a committed artifact), the grep run was:

```
$ rg -n '/home/denniyahh|/var/home/denniyahh|/tmp/' .planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md
```

→ no match at the point this grep was run (confirmed after this section was
written and before committing). The public GitHub account name in the
PR/CI-run URLs is retained, consistent with every other committed planning
artifact in this repository (e.g. `23-GUARD-SHIP-RECORD.md`'s identical
disposition), and is not the leak class this checklist targets.

**Do not merge.** Task 2 stops here — no `gh pr merge`, no `git merge`, no
`git push origin develop` was run.

## Task 3: Operator merge into `develop`

### Operator response (verbatim)

```
Merged 9916e2fdaa5d5a16e9875fd90ab93381d0312726
```

### Ancestry verification (run first-hand by the continuation executor)

```
$ git fetch origin
FETCH_EXIT=0
```

```
$ git log origin/develop -1 --format='%H %an %s'
9916e2fdaa5d5a16e9875fd90ab93381d0312726 Dennis Kim Merge pull request #33 from denniyahh/feature/phase-23
```

Subject identifies a pull-request merge (`Merge pull request #33 from
denniyahh/feature/phase-23`) — evidence the merge came through the PR path
rather than a direct push.

Ancestry checked for both the fix commit and the PR head, named
separately:

```
$ git merge-base --is-ancestor c2e947a53e4781da9ee799beaba9e541d16781db origin/develop
FIX_COMMIT_ANCESTOR_EXIT=0
```

`c2e947a53e4781da9ee799beaba9e541d16781db` — the fix commit itself
(`fix(23-16): content-check the divergent-lineage staleness arm`, Task 1).

```
$ git merge-base --is-ancestor 2af0374f463600227e22c802fd2b09145b9a21d1 origin/develop
PR_HEAD_ANCESTOR_EXIT=0
```

`2af0374f463600227e22c802fd2b09145b9a21d1` — the PR #33 head (the final
pushed commit, Task 2), carrying the fix, the CHANGELOG entry, and the
complete record.

Both exit 0 — both commits are ancestors of `origin/develop`'s new tip.

Cross-checked independently against GitHub's own record of the merge:

```
$ gh pr view 33 --json state,mergedAt,mergeCommit,mergedBy
{"mergeCommit":{"oid":"9916e2fdaa5d5a16e9875fd90ab93381d0312726"},"mergedAt":"2026-07-27T00:36:33Z","mergedBy":{"id":"MDQ6VXNlcjIyMzIzOQ==","is_bot":false,"login":"denniyahh","name":"Dennis Kim"},"state":"MERGED"}
```

`state: MERGED`, `mergeCommit.oid` matches the operator's reported SHA
exactly, `mergedBy.login: denniyahh` (`is_bot: false`) — a human merge, not
an automated one.

### No autonomous write to `develop`

No command run by this executor, in this task or any prior task of this
plan, wrote to `develop`. The only commands run against `origin/develop`
in this task were read-only: `git fetch`, `git log`, `git merge-base
--is-ancestor`, and `gh pr view`. The merge itself
(`9916e2fdaa5d5a16e9875fd90ab93381d0312726`) was performed by the operator,
outside this executor's control, through PR #33's GitHub merge action —
confirmed by `mergedBy.login: denniyahh` / `is_bot: false` above. This
executor never ran `gh pr merge`, `git push origin develop`, or `git merge`
against a local `develop` ref at any point across Tasks 1–3.

### Redaction re-check

Re-ran the same narrowed grep from Task 2 before appending this section:

```
$ rg -n '/home/denniyahh|/var/home/denniyahh|/tmp/' .planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md
```

The only match is the grep invocation's own literal text (this same
command, quoted as documentation, both here and in Task 2's section) —
not an actual leaked filesystem path. No genuine leak found, consistent
with Task 2's original finding.

### Criterion deviation: redaction acceptance criterion is not literally satisfiable

Task 2's acceptance criteria included: `rg -c '<the operator OS username>'
.planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md` returns
`0`. This is **not literally satisfiable** on this project: the operator's
OS username and their GitHub account name are the identical string
(`denniyahh`), and this record is required by the same task's acceptance
criteria to carry the PR URL
(`https://github.com/denniyahh/devflow/pull/33`), which necessarily
contains that string. A literal `rg -c 'denniyahh'` returns a nonzero
count by construction — satisfying it would mean deleting the PR URL the
task also requires recording, which is a direct contradiction, not an
oversight.

This is recorded as a **criterion deviation with rationale**, not a pass:
the narrowed grep actually run and reported (`/home/denniyahh|/var/home/denniyahh|/tmp/`
— filesystem-path leak vectors, the real threat class this checklist
targets per 999.10) is clean, both in Task 2 and re-confirmed here in
Task 3. The public GitHub account name appearing in PR/CI URLs is not the
leak class this checklist exists to catch, and is retained consistently
with every other committed planning artifact in this repository (e.g.
`23-GUARD-SHIP-RECORD.md`).

### Task 3 complete

The fix is confirmed an ancestor of `origin/develop`, merged by the
operator through pull request #33, with the merge commit SHA
(`9916e2fdaa5d5a16e9875fd90ab93381d0312726`) and the ancestry proof
recorded above.

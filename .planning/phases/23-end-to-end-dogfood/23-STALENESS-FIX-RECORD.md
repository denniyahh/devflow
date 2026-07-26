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

- **Pushed head SHA:** `c2e947a53e4781da9ee799beaba9e541d16781db`
  (`git push --force-with-lease origin HEAD` — the tracked pre-push hook,
  `scripts/hooks/pre-push` → `scripts/check-in-container.sh all`, run
  inside the pinned devcontainer, ran `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, and `cargo test --workspace
  --no-fail-fast` before the push left the machine — all reported clean,
  `check.sh: all OK`.)
- **Pull request:** [PR #33](https://github.com/denniyahh/devflow/pull/33)
  — `fix(23-16): content-check divergent-lineage staleness arm`
  - `gh pr view 33 --json number,baseRefName,state,url`:
    `{"baseRefName":"develop","number":33,"state":"OPEN","url":"https://github.com/denniyahh/devflow/pull/33"}`
- **CI conclusion** (`gh pr checks 33`, final poll, verbatim — two runs
  appear per check because the push triggered two workflow dispatches):

  ```
  Build + test in devcontainer	pass	1m55s	https://github.com/denniyahh/devflow/actions/runs/30226047303/job/89856287666
  Build + test in devcontainer	pass	1m53s	https://github.com/denniyahh/devflow/actions/runs/30226069850/job/89856345542
  Clippy	pass	1m2s	https://github.com/denniyahh/devflow/actions/runs/30226047300/job/89856586611
  Clippy	pass	1m0s	https://github.com/denniyahh/devflow/actions/runs/30226069855/job/89856345518
  Format	pass	1m7s	https://github.com/denniyahh/devflow/actions/runs/30226047300/job/89856600721
  Format	pass	48s	https://github.com/denniyahh/devflow/actions/runs/30226069855/job/89856345513
  Test	pass	1m35s	https://github.com/denniyahh/devflow/actions/runs/30226047300/job/89856586251
  Test	pass	1m28s	https://github.com/denniyahh/devflow/actions/runs/30226069855/job/89856345516
  ```

  No check pending or failing at time of recording.

  **A finding, recorded rather than silently absorbed:** one of the two
  `Test` job dispatches initially failed
  (`https://github.com/denniyahh/devflow/actions/runs/30226047300/job/89856287706`,
  `test result: FAILED. 362 passed; 1 failed`) on a single test —
  `agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process`
  in `crates/devflow-core/src/agent.rs`, a file this plan does not touch.
  The test's own in-source comment (`agent.rs:303-309`) documents it as a
  pre-existing, previously-observed CI-intermittent flake ("first seen
  2026-07-26, on commits touching no Rust source... does not reproduce
  locally — 40/40 under CPU load"), unrelated to `staleness.rs` or the 23g
  fix — out of this plan's scope boundary (only issues directly caused by
  this plan's own changes are auto-fixable; this is a pre-existing failure
  in an unrelated file). This task's own local `cargo test --workspace`
  run (quoted above) never reproduced it. The check's twin dispatch on the
  same push (`.../30226069855/job/89856345516`) passed. The failed job was
  re-run via `gh run rerun 30226047300 --failed` — no code was changed, no
  new commit was pushed — and passed on rerun (quoted above, `1m35s`).
  Filed as an out-of-scope, pre-existing flake; no `deferred-items.md`
  entry was needed beyond this record since the flake's own doc comment
  already names and tracks it in source.
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
    (checked after PR #33 opened and CI confirmed green) →
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

*(To be completed by the operator via the `checkpoint:human-verify` gate
this plan returns control to. This section is a placeholder for the
continuation agent to fill in once the operator responds.)*

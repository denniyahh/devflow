---
phase: 19-release-integrity-main-rs-decomposition
plan: 11
subsystem: verification
tags: [rust, equivalence-proof, ci, env-mutex, downstream-git]

requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: "Plans 19-01 through 19-10: release-integrity fixes, acceptance contract, split baseline, mechanical module extraction, and reconciled documentation"
provides:
  - "Three-part equivalence proof reconciled against baseline f35d6c1 and exact pushed commit aa95873"
  - "Three green GitHub Actions attempts on aa95873"
  - "Explicit ENV_MUTEX disposition and downstream-repository reproduction across all three required edges"
affects: [phase-19-completion, phase-20]

tech-stack:
  added: []
  patterns:
    - "Verification-only plans preserve failures and limitations in the record instead of editing source to make the gate pass"
    - "GitHub Actions rerun attempts provide independent shared-runner samples without changing the commit under test"

key-files:
  created:
    - .planning/phases/19-release-integrity-main-rs-decomposition/19-11-SUMMARY.md
  modified: []

key-decisions:
  - "The plan's literal test-name command is invalid because rg '::tests::' drops top-level tests and leaves ': test'; the corrected comparison selects all ': test' lines, removes module prefixes and the suffix, C-sorts both sides, and returns 438/438 with an empty diff."
  - "The committed 438-test baseline was captured after plans 19-01 through 19-03, so the correct delta against that baseline is zero; the 12 tests added earlier in the phase are enumerated below and already included in both sides."
  - "CI evidence is described literally: the workflow runs cargo test, not cargo test --workspace or --all-targets. Symbol/name-set scripts ran locally at the exact pushed SHA because ci.yml has no steps for them."

requirements-completed: [19a, 19b, 19c, 19d, 19e, 19f, 19g]

duration: continuation across prior executor and checkpoint
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 11: Release Integrity and Split Verification Summary

**The complete Phase 19 tree at `aa9587355d51de51737703be4878a77c4ff747d1` preserves all 100 moved functions and all 438 baseline tests, passed three independent GitHub Actions attempts, retained one CLI `ENV_MUTEX` across all 18 lock sites, and kept `.devflow/` out of ordinary downstream commits while preserving forced-add behavior.**

## Task 1: Three-Part Equivalence Proof

### Check 1: symbol-level reconciliation

Baseline: `f35d6c1ec34fc3fbc5e4c4477d98e16f4355d04f`.

| Plan | Moved functions | Result |
|---|---:|---|
| 19-07, staleness + preflight | 18 | zero unexplained hunks |
| 19-08, pipeline launch/outcomes/gate | 26 | zero unexplained hunks; three signature wraps explained as rustfmt consequences of `pub(crate)` |
| 19-09, parallel/commands/config | 56 | zero unexplained hunks |
| **Total** | **100** | **zero unexplained hunks** |

Independent reconciliation:

```text
baseline top-level functions in main.rs: 103
current top-level functions in main.rs: 3 (main, run, project_root)
independently implied moved functions: 103 - 3 = 100
summary aggregate: 18 + 26 + 56 = 100
unexplained hunks: 0
```

The broader item inventory also reconciles: baseline `main.rs` had 119 top-level production items; seven remain (`Cli`, `Command`, `GateCmd`, `CliError`, `main`, `run`, `project_root`), matching the 112 items now distributed across the eight production modules.

### Check 2: test name-set identity

The plan's literal verify pipeline is not usable: `rg '::tests::'` excludes top-level `tests::...` entries and the `sed` expression leaves Cargo's `: test` suffix. It produced 405 malformed entries. The corrected set comparison was:

```bash
cargo test --workspace -- --list 2>/tmp/19-final-list.stderr \
  | sed -n -E '/: test$/ { s/: test$//; s/.*:://; p; }' \
  | LC_ALL=C sort > /tmp/19-final-names.txt
LC_ALL=C sort \
  .planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE-names.txt \
  > /tmp/19-baseline-names.txt
diff -u /tmp/19-baseline-names.txt /tmp/19-final-names.txt
```

Verbatim result:

```text
LIVE_NAMES=438
BASELINE_NAMES=438
NAMESET-IDENTICAL
```

### Check 3: per-target pass-count identity

Fresh `cargo test --workspace` at `aa95873` returned exit 0 and the same 11 target counts as the committed baseline:

| Target order | Baseline | Final | Failed |
|---:|---:|---:|---:|
| 1 | 106 | 106 | 0 |
| 2 | 3 | 3 | 0 |
| 3 | 4 | 4 | 0 |
| 4 | 1 | 1 | 0 |
| 5 | 1 | 1 | 0 |
| 6 | 3 | 3 | 0 |
| 7 | 10 | 10 | 0 |
| 8 | 306 | 306 | 0 |
| 9 | 2 | 2 | 0 |
| 10 | 2 | 2 | 0 |
| 11 | 0 | 0 | 0 |
| **Workspace total** | **438** | **438** | **0** |

The delta against the committed baseline is exactly zero because plan 19-06 captured that baseline after the release-integrity tests landed. The earlier phase additions already present on both sides are:

- Plan 19-01 unit tests: `ensure_devflow_dir_writes_star_gitignore`, `ensure_devflow_dir_is_idempotent_and_preserves_existing_gitignore`, `ensure_devflow_dir_preserves_foreign_gitignore_content`, `ensure_devflow_dir_on_nested_subpath_marks_the_devflow_ancestor`, `ensure_devflow_dir_on_relative_devflow_leaf_path_marks_it`, `ensure_devflow_dir_without_a_devflow_ancestor_only_creates_dirs`, and `ensure_devflow_dir_concurrent_calls_both_succeed`.
- Plan 19-01 integration tests: `all_seven_devflow_constructors_produce_the_gitignore` and `git_add_all_no_longer_sweeps_devflow_into_a_commit`.
- Plan 19-03 tests: `commit_path_twice_with_identical_content_creates_only_one_commit`, `commit_path_with_no_changes_returns_ok_without_committing`, and `commit_path_on_nonexistent_path_still_errors`.

No unexplained target delta exists.

## CI Evidence

The pushed commit was `aa9587355d51de51737703be4878a77c4ff747d1`. All three attempts of GitHub Actions run 29927890337 completed successfully:

| Attempt | URL | Test | Clippy | Format |
|---:|---|---|---|---|
| 1 | https://github.com/denniyahh/devflow/actions/runs/29927890337/attempts/1 | success, 49s | success, 24s | success, 15s |
| 2 | https://github.com/denniyahh/devflow/actions/runs/29927890337/attempts/2 | success, 51s | success, 27s | success, 15s |
| 3 | https://github.com/denniyahh/devflow/actions/runs/29927890337/attempts/3 | success, 49s | success, 25s | success, 15s |

Every attempt reports the identical head SHA. No red attempt occurred. The only annotations were GitHub's Node.js 20 deprecation warnings for `actions/checkout@v4`; Phase 999.9 already tracks that dependency-update concern.

Literal commands from `.github/workflows/ci.yml`:

```text
test   (line 20): cargo test
clippy (line 31): cargo clippy --workspace --all-targets -- -D warnings
fmt    (line 42): cargo fmt --check
```

**Known CI-coverage limitation for D-11:** CI does not run `cargo test --workspace`, `cargo test --all-targets`, the symbol-level diff script, or the test-name-set script. With no `default-members`, plain `cargo test` currently covers every workspace member, but the command is not equivalent textually and carries neither explicit flag. The local equivalence scripts and stronger `cargo test --workspace` ran against the exact SHA pushed to CI; CI contributes the three independent shared-runner race-window samples. This plan did not modify `ci.yml`, as prohibited.

## ENV_MUTEX Disposition

ENV_MUTEX preserved — no finding

Evidence:

- Plan 19-06: three consecutive `cargo test -p devflow` runs after the mutex/fixture hoist, all stable.
- Plan 19-07: three consecutive runs after staleness/preflight extraction, all stable.
- Plan 19-08: three consecutive runs after pipeline outcomes and another three after pipeline gate, all stable.
- Plan 19-09: three consecutive runs after the final extraction, all stable.
- Final local workspace run and pre-push workspace run: 438 tests, zero failures, identical per-target counts.
- CI attempts 1, 2, and 3 on `aa95873`: every Test, Clippy, and Format job green.

That is 15 explicitly recorded consecutive package-test runs during plans 19-06 through 19-09, followed by final local and shared-runner evidence with no differing count or failure.

Source assertions:

```text
devflow-cli static ENV_MUTEX count: 1
devflow-cli ENV_MUTEX.lock sites: 18
location: crates/devflow-cli/src/test_support.rs
devflow-core separate statics: crates/devflow-core/src/config.rs and crates/devflow-core/src/gates.rs
```

All CLI lock sites import the one `crate::test_support::ENV_MUTEX`. Its D-04 documentation states: every environment variable is guarded by exactly one mutex, and no variable is touched under two. The two core statics were left untouched; core and CLI tests are separate binaries, so they do not share a process-global environment.

## Downstream Repository Reproduction

Scratch repository: `/tmp/devflow-phase19-downstream.dnw9OM` (outside this workspace). Its initial branch was `develop`; the root `.gitignore` contained only:

```text
target/
```

It therefore contained no `.devflow` pattern.

### Binary provenance and real phase start

```text
$ git rev-parse HEAD
aa9587355d51de51737703be4878a77c4ff747d1
$ target/debug/devflow --version
devflow 1.5.0
$ strings target/debug/devflow | rg -F aa9587355d51de51737703be4878a77c4ff747d1
agentversion1.5.0commitaa9587355d51de51737703be4878a77c4ff747d1dirtyfalse
```

The branch binary then ran `devflow start --phase 1 --agent claude --mode auto` in the scratch repository with a controlled sleeping `claude` stub. Relevant verbatim output:

```text
created worktree: /tmp/devflow-phase19-downstream.dnw9OM/.worktrees/phase-01 (branch feature/phase-01)
stage define -> launched Claude Code (monitor pid 178)
started phase 1 in auto mode at 1784729660 -> monitor will auto-advance
  watch live: devflow logs -f --phase 1
```

The generated marker bytes were `2a 0a`, i.e. exactly `*\n`.

### Full `.devflow/` edge

After adding a runtime sentinel and an ordinary README change:

```bash
git add .
git commit -m "user commit"
git log -1 --name-only --format=fuller
```

Verbatim log:

```text
commit 01b7c9001ae06a952413c8e8c7831c360fe39a8c
Author:     Phase 19 Verification <phase19@example.invalid>
AuthorDate: Wed Jul 22 10:15:04 2026 -0400
Commit:     Phase 19 Verification <phase19@example.invalid>
CommitDate: Wed Jul 22 10:15:04 2026 -0400

    user commit

README.md
```

`git ls-files '.devflow/*'` produced no output. The marker content was:

```text
*
```

### Marker-only edge

All generated runtime artifacts were moved out of `.devflow/`, leaving its sole member `.gitignore`; an ordinary README change was then committed with `git add .`.

Verbatim log:

```text
commit 01c0e10e70cf7f49316f4d8318ff7e621b440673
Author:     Phase 19 Verification <phase19@example.invalid>
AuthorDate: Wed Jul 22 10:15:24 2026 -0400
Commit:     Phase 19 Verification <phase19@example.invalid>
CommitDate: Wed Jul 22 10:15:24 2026 -0400

    marker-only user commit

README.md
```

`find .devflow -mindepth 1 -printf '%P\n'` printed only `.gitignore`; `git ls-files '.devflow/*'` again produced no output.

### Forced-add boundary

```text
$ git add -f .devflow/some-file
$ git diff --cached --name-only
.devflow/some-file
```

Verbatim commit log:

```text
commit 73aa95c50d383c037b96eacd02faab6e6ab780fc
Author:     Phase 19 Verification <phase19@example.invalid>
AuthorDate: Wed Jul 22 10:15:35 2026 -0400
Commit:     Phase 19 Verification <phase19@example.invalid>
CommitDate: Wed Jul 22 10:15:35 2026 -0400

    forced devflow boundary

.devflow/some-file
```

Verdict: both ordinary sweep edges commit zero `.devflow/` paths, while explicit `git add -f` remains available. The fix ignores; it does not forbid.

## Requirement Roll-Call

| Requirement | Verdict | Owner | Evidence |
|---|---|---|---|
| 19a | landed | 19-01, 19-02 | All seven constructors self-protect `.devflow`; full and marker-only scratch commits contain zero `.devflow/` paths; forced add works; `exe_path` is a filename only. |
| 19b | landed | 19-03 | Three `commit_path` regression tests are present and green; repeated unchanged content creates no empty commit; `commit_all` remained out of scope as required. |
| 19c | landed | 19-06, 19-07 | Durable 438-test baseline, single shared CLI mutex, visibility foundation, and staleness/preflight extraction; aggregate symbol proof reconciles. |
| 19d | landed | 19-07, 19-08 | Tests moved with their production owners and the pipeline seams preserve direct coupling; name set and pass counts are identical. |
| 19e | landed | 19-08, 19-09 | Pipeline split completed and `main.rs` reduced from 8,487 baseline lines to a 478-line crate root without behavioral deltas. |
| 19f | landed | 19-09, 19-10 | Parallel, commands, and config clusters moved into flat siblings; structure/testing documents and Phase 19 roadmap entry reconciled to the live tree. |
| 19g | landed | 19-04, 19-05 | Acceptance skill and contributor contract committed; dogfood checkpoint approved. The recorded non-blocking citation gap remains: an isolated reviewer applies generic judgment but does not cite the contract unless dispatched to load it. |

## Source Integrity and Checkpoint

`git status --porcelain -- crates/` is empty. Plan 19-11 changed no source file.

The user approved the blocking human checkpoint on 2026-07-22 after reviewing the named `ENV_MUTEX preserved — no finding` disposition, the downstream evidence, the CI limitation, and all seven requirement verdicts.

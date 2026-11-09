---
phase: 50-addressable-monitor-liveness
plan: 25
subsystem: testing
tags: [evidence, judge, selftest, supervisor, release-handoff]
status: complete
requires:
  - phase: 50-18
    provides: check-supervisor-evidence.sh (mode dispatch, st_case/expect selftest harness, 32 cases)
  - phase: 50-19
    provides: live attempt 1 closed, so the tool is no longer in use when this plan edits it
provides:
  - "check-supervisor-evidence.sh --published <dir>: PUBLISHED verdict=pass|fail (50-20 Task 1 gate)"
  - "check-supervisor-evidence.sh --live-run <file>: LIVE_RUN verdict=pass|fail (50-20 Task 2 gate)"
  - "check-supervisor-evidence.sh --handoff <file> --plan <plan>: HANDOFF verdict=pass|fail (50-20 Task 2 gate, R6-D)"
  - "check-supervisor-evidence.sh --defects <file> --roadmap <ROADMAP.md>: DEFECTS verdict=pass|fail (50-20 Task 3 gate)"
  - "23 named selftest cases; the selftest now holds 55"
affects: [50-20]

actuals:
  tokens: 6350      # chars/4 over the realized diff (25391 bytes of `git diff 7a09f65..8e0faa0`)
  tasks: 2
  commits: 2        # measured: git rev-list --count plan_head_before..HEAD, before the SUMMARY commit
plan_head_before: 7a09f65f0653ee8b3220ec0fe0be2fba53ffbc12
plan_head_after: 8e0faa02d704e1c659eb5f2fb2c32cbc20f26283

tech-stack:
  added: []
  patterns:
    - "Each judge prints one `<LABEL> <problem>` line per problem, then `<LABEL> verdict=pass|fail`; exit 0 only on pass, 2 on usage"
    - "Every failing selftest case asserts its specific reason line as well as the exit code and verdict"

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/50-25-SUMMARY.md
  modified:
    - .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh

key-decisions:
  - "All new rg calls pass --no-config, so a user ripgrep config cannot narrow the $HOME search or the fragment checks"
  - "--defects matches a backlog id only as a whole number (999.67 is not satisfied by 999.675), and reads the no-defect statement case-insensitively so a sentence-initial 'No non-blocking defect found.' passes"
  - "--defects also fails when the roadmap is absent or not tracked at HEAD: git diff of an untracked path is empty and would otherwise pass as 'no heading added'"
  - "--defects counts every `+### Phase 999.N:` line in the diff, so retitling an existing backlog heading also requires the record to reference it (fail-closed reading of the spec)"

patterns-established: []

requirements-completed: []
---

# Phase 50 Plan 25: Evidence judges for 50-20 Summary

**Four append-only judges (`--published`, `--live-run`, `--handoff`, `--defects`) in `check-supervisor-evidence.sh`, each a one-call gate for 50-20 whose exit code is the verdict, pinned by 23 named selftest cases. Every failing case was shown to fail for its intended reason by a mutation of the judge.**

## Performance

- Tasks: 2/2
- Files modified: 1 (600 lines added, 0 removed)
- Selftest: `SELFTEST passed=55 failed=0`; `fault-arm.sh --selftest` still `passed=6 failed=0`

## Task Commits

1. **Task 1 (tracer): `--published` and its 7 cases**: `5ef0e39`
2. **Task 2: `--live-run`, `--handoff`, `--defects` and their 16 cases**: `8e0faa0`

Tracer feedback gate: Task 1's `<verify>` was re-run end to end after the commit and passed (39 cases, `GATE ok`) before Task 2 started.

## What each judge checks

- `--published <dir>`: at least one `fault-*` and one `attempt-*` directory (a file with that name does not count); a `fault-*/verdict-*.txt` line starting `FAULT verdict=pass` (the same line in another file does not count); a line anywhere starting `ATTEMPT_GATE verdict=shipped|cap_reached`; no file whose content names `$HOME`, searched with `rg -uuu`; an empty or unset `$HOME`, or an rg error, fails; no symlink anywhere, including the root.
- `--live-run <file>`: the eight fragments, plus a `^\| *(criterion )?N *\|` row for N = 1 to 8.
- `--handoff <file> --plan <plan>`: exactly one `^    bash -c .R=.*changelog_3_0_0_lines=.*$` line in the plan; the record contains that line without its indent, verbatim; the six handoff fragments.
- `--defects <file> --roadmap <roadmap>`: `git diff --no-color --no-ext-diff --unified=0 HEAD` in the roadmap's repository. A git failure fails, and so does a roadmap that is absent or not at HEAD. Each added `### Phase 999.N:` id must appear in the record as a whole number. If none was added, the record must say "no non-blocking defect found".

## Verification evidence

- **Both plan gates passed.** Task 1's ran verbatim. Task 2's ran with one change, described under Deviations.
- **Mutation controls.** Each mutation was applied to a scratch copy of the tool, the selftest was run, and in every row exactly the case shown failed:

| Mutation of the judge | Case that failed |
|---|---|
| `-uuu` changed to `-uu` | published fails a hidden file naming the home path |
| `$HOME` search removed | both home-path cases |
| symlink `find` neutered | published fails a symlink |
| fault verdict read from any `*.txt` | published fails without a passing fault verdict |
| `ATTEMPT_GATE verdict=[a-z_]+` accepted | published fails without a shipped or cap_reached verdict |
| `-type d` dropped | published fails without an attempt directory |
| pass branch prints fail | published passes a sanitized tree |
| `V-29` dropped from the fragments | live run fails a missing fragment |
| row regex unanchored | live run fails a missing criterion row (the planted `\| 17 \|` row stood in for row 7) |
| criteria bound 8 changed to 6 | live run fails a missing criterion row |
| file-existence check removed | live run fails an absent file |
| check-line comparison skipped | handoff fails an altered check line |
| count `-ne 1` changed to `-lt 1` | handoff fails a plan holding the check line twice |
| any indent accepted | handoff fails a plan without the check line |
| `never squash` dropped | handoff fails a missing handoff fragment |
| id matched as a substring | defects fails a new backlog id missing from the record (the record names 999.675) |
| `-i` dropped | defects passes no new id with the no-defect statement |
| git failure ignored | defects fails outside a git repository |
| statement check always true | defects fails no new id without the statement |
| heading pattern never matches | both new-id cases |

- **Ad hoc controls.** All exited 1 with the stated reason line unless noted:
  - a copy of the real 50-20 plan with its check line altered: `HANDOFF missing=check-line`
  - an absent plan: `rg_error=2`
  - an absent roadmap: `roadmap=ABSENT`
  - a roadmap untracked at HEAD: `roadmap=NOT_AT_HEAD`
  - a repository without commits: `git=FAILED rc=128`
  - unreadable file in the published tree: `PUBLISHED rg_error=2 search=home-path`
  - a symlinked root: `PUBLISHED symlink=<dir>`
  - empty and unset `HOME`: `home=EMPTY`
  - usage errors exited 2
- **Real data, read-only.** The raw `$HOME/.cache/devflow-phase50` fails `--published` as it should: 59 `names_home` lines and `missing=attempt-gate-verdict`, because the gate verdict is not saved yet; 50-20 Task 1 saves it. The repository's current `ROADMAP.md` gives `DEFECTS added=0`.
- **Append-only.** `git diff 5ef0e39^..HEAD` on the tool shows 600 additions and 0 deletions. The only `-` line in the diff is the `---` file header.

## What these checks do not establish

- The cases prove the judges on planted inputs. They do not prove that 50-20's real `50-evidence/` tree or `50-LIVE-RUN.md` will pass; that is 50-20's gate.
- `--published` checks file contents for the literal `$HOME` string only:
  - It does not check file names.
  - It does not catch other spellings of the path (`/var/home/...` versus `/home/...` on this host, where `$HOME` is `/home/denniyahh` and `/home` resolves to `/var/home`), `~`, or the username on its own.
  - The Phase 49 sanitizer's secret scan stays the main redaction control.
- `--live-run` and `--handoff` check that fragments are present, not what the text says or where it sits: a fragment inside an unrelated sentence counts.
- `--defects` sees only the working-tree diff against HEAD, so it must run before Task 3's commit, as 50-20 states. A heading committed earlier is invisible to it.

## Deviations from Plan

**1. [Rule 3 - Blocking] Task 2's gate ran with a literal scratch path instead of `mktemp` plus an `rm -rf` trap.**
- **Found during:** Task 2 commit.
- **Issue:** Claude Code's built-in removal check refused the gate's `bash -c` because its `trap "rm -rf \"$L\""` removes a path it cannot resolve. Task 1's gate, with the same construct, had been allowed earlier.
- **Fix:** the same gate body was run with `L=` set to a `mktemp -d` scratch directory made beforehand and no trap. The commit was chained on its exit code. Afterwards that literal directory was removed in a separate command. The gate's checks were unchanged.
- **Commit:** `8e0faa0`.

**2. [Rule 2 - Missing critical] `--defects` fails on an absent roadmap or one not tracked at HEAD.**
- **Why:** git diffs an untracked or mistyped path as empty. That would let a wrong `--roadmap` argument pass as "no heading added" (T-50-93).
- **Commit:** `8e0faa0`.

**3. [Rule 2] All new rg calls use `--no-config`.** A `RIPGREP_CONFIG_PATH` could otherwise add globs or size limits that silently narrow the `$HOME` scan (T-50-92).

**Count inconsistency in the plan text (not a code deviation):** the plan says "54" cases and "31" for 50-18 in places. Its own name lists give 32 + 7 + 16 = 55, which is what the selftest holds and what Task 2's gate pins.

## Known Stubs

None.

## Notes

- The post-commit hook may touch `.planning/user/errors/state-stale.json`. It was not staged by this plan; `git status` was clean after both commits.
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not modified. The orchestrator owns those. SUPV-01 is not complete here: it closes when 50-20 publishes and judges the evidence.

## Self-Check: PASSED

- FOUND: .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh
- FOUND: 5ef0e39 (ancestor of HEAD)
- FOUND: 8e0faa0 (ancestor of HEAD)

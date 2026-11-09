---
phase: 50-addressable-monitor-liveness
plan: 20
subsystem: testing
tags: [live-run, evidence, sanitizer, verdict, release-handoff, macos]
status: complete
requires:
  - phase: 50-19
    provides: live attempt 1 (shipped) and fault-5 on the same binary, raw under $HOME/.cache/devflow-phase50/
  - phase: 50-25
    provides: check-supervisor-evidence.sh --published, --live-run, --handoff, --defects (this plan's gates)
provides:
  - "50-evidence/attempt-1 and 50-evidence/fault-5: sanitized, curated copies with checker verdicts saved before redaction"
  - "50-LIVE-RUN.md: quoted verdicts, criterion 1-8 mapping, limits, defect disposition, macOS compile gate result, 3.0.0 release handoff"
affects: [50-verification, 3.0.0-release]

actuals:
  tokens: 888000    # chars/4 over the realized diff (3552728 bytes); about 883000 of it is the copied evidence, about 5000 the authored 50-LIVE-RUN.md
  tasks: 3
  commits: 3        # measured: git rev-list --count plan_head_before..HEAD, before the SUMMARY commit
plan_head_before: 2afbee85050307fd590018d0397b0aabc607366a
plan_head_after: 05e7eef71593976b50be422fa81b5853cb1a7e56

tech-stack:
  added: []
  patterns: []

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/50-evidence/
    - .planning/phases/50-addressable-monitor-liveness/50-LIVE-RUN.md
  modified: []

key-decisions:
  - "Published fault-5 only: it is the fault directory the attempt gate pairs with attempt 1 by bin_sha256 (D-17); fault-1..4 are older-binary runs, judged and verdicts saved raw, not committed"
  - "Curated attempt-1 as Phase 49 did (D-26): no project/, collected/sessions/, prelaunch/agent-env-names.txt, ledger.lock; also dropped DevFlow's '*' .gitignore, which would have hidden the published event log from git"
  - "No non-blocking defect from 50-18/50-19, so ROADMAP.md is unchanged; the ps -g hint was fixed in-phase (ce6e718)"
  - "SUPV-01 not checked off in REQUIREMENTS.md here: the orchestrator owns tracking files"

patterns-established: []

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "Sanitized attempt-1 and fault-5 evidence committed with checker verdicts derived on the raw tree"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "check-supervisor-evidence.sh --published .planning/phases/50-addressable-monitor-liveness/50-evidence (PUBLISHED verdict=pass)"
        status: pass
      - kind: other
        ref: "sanitize-evidence.py scan .planning/phases/50-addressable-monitor-liveness/50-evidence (exit 0, no findings)"
        status: pass
    human_judgment: false
  - id: D2
    description: "50-LIVE-RUN.md: verdicts quoted, criteria 1-8 mapped, limits, defects, release handoff"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "check-supervisor-evidence.sh --live-run 50-LIVE-RUN.md (LIVE_RUN verdict=pass)"
        status: pass
      - kind: other
        ref: "check-supervisor-evidence.sh --handoff 50-LIVE-RUN.md --plan 50-20-PLAN.md (HANDOFF verdict=pass)"
        status: pass
      - kind: other
        ref: "check-supervisor-evidence.sh --defects 50-LIVE-RUN.md --roadmap ROADMAP.md (DEFECTS added=0 verdict=pass) && gsd-tools roadmap.analyze (exit 0)"
        status: pass
    human_judgment: true
    rationale: "The judges check that fragments are present, not what the text claims. Whether the evidence supports SUPV-01 is the phase verifier's judgment."
  - id: D3
    description: "Phase-end macOS compile gate on the final tree (V-56, MAC-3)"
    verification:
      - kind: other
        ref: "cargo check --target aarch64-apple-darwin -p devflow-core --all-targets && cargo check --target aarch64-apple-darwin -p devflow --all-targets (exit 0)"
        status: pass
    human_judgment: false

duration: 30min
completed: 2026-10-20
---

# Phase 50 Plan 20: Live-run verdicts and published evidence Summary

**Attempt 1 and fault-5 are committed as sanitized, curated copies under `50-evidence/`, each carrying the checker output saved on the raw tree (`ATTEMPT_GATE verdict=shipped attempt=1`, `FAULT verdict=pass`, `ATTEMPT verdict=pass`). `50-LIVE-RUN.md` quotes those lines, maps criteria 1-8 to 26 named tests (all re-run by exact name, all passing) and to the live records, lists what the evidence does not establish, records no non-blocking defect, and carries the 3.0.0 release handoff. The macOS compile gate (V-56) passed.**

## What this does NOT establish (read first)

- **The gates check presence, not meaning.** `--live-run` and `--handoff` pass a record that holds the required fragments; they do not check that its claims are right. The criterion table was checked by hand against the evidence and the test run, not by a tool.
- **One live run, one fault run.** Nothing here bounds a success rate (see 50-LIVE-RUN.md's limits section).
- **The criterion tests were re-run on the final tree, not on the attempt binary.** The crate sources are unchanged since `1deaa4d` (the attempt binary's build commit; `git diff --stat 1deaa4d HEAD` over `crates`, `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` is empty), so they are the same code, but each test is a fixture, not a live record.
- **The 3.0.0 check line was not run in this session.** Claude Code's removal check refuses its `trap "rm -rf ..."`. Its behaviour rests on the plan's 2026-10-15 and 2026-10-16 records. The `--handoff` gate only checks that the line is copied verbatim.
- **macOS: compile only.** Nothing was linked or run on macOS.

## Performance

- **Duration:** about 30 min
- **Started:** 2026-10-20T19:52Z
- **Completed:** 2026-10-20T20:04Z
- **Tasks:** 3/3
- **Files created:** 95 (94 evidence files and 50-LIVE-RUN.md)

## Accomplishments

- **Evidence published (V-28, V-29).** The checkers ran on the raw tree first, and each output was saved as `verdict-<check>.txt` in that raw directory, never overwriting a file. The checkers were `--fault --require-all` for every `fault-*`, `--attempt` for `attempt-1`, and one `--attempt-gate` run copied into each directory. Then `attempt-1` (curated) and `fault-5` went through the Phase 49 sanitizer into scratch and were moved into `50-evidence/`. Both sanitized with no refusal.
- **Record written.** `50-LIVE-RUN.md` has the verdict, the attempt table (`bin_sha256` `9b0ec70c…`, build `1deaa4d`), the fault-arm lines, the six `stage_launched` rows (5 `stream`, 1 `capture` on the resumed Code turn), the raw socket path length (77 bytes under `$HOME/.cache/devflow/sock`), the criterion 1-8 table, nine limits, the defect disposition and the release handoff.
- **Phase-end macOS gate passed.** Both `cargo check --target aarch64-apple-darwin ... --all-targets` calls exited 0, with no warning or error lines.

## Task Commits

1. **Task 1 (tracer): judge, save verdicts, sanitize, publish:** `abbfd6f`. Tracer gate: `--published` re-run after the commit passed before Task 2 began.
2. **Task 2: 50-LIVE-RUN.md:** `97b5def`
3. **Task 3: defect disposition and the macOS compile gate:** `05e7eef`

## Gate results

| Gate | Result |
|------|--------|
| Task 1 `--published 50-evidence` | `PUBLISHED verdict=pass`, rc 0 (before the commit and again after it) |
| Orchestrator extra: `sanitize-evidence.py scan 50-evidence` | exit 0, no output (no file or pattern names printed) |
| Task 2 `--live-run` | `LIVE_RUN verdict=pass`, rc 0 |
| Task 2 `--handoff` | `HANDOFF verdict=pass`, rc 0 |
| Task 3 `--defects` + `roadmap.analyze` | `DEFECTS added=0`, `DEFECTS verdict=pass`; analyze exit 0 (run before the Task 3 commit) |
| Task 3 macOS compile gate (V-56) | rc 0 |
| Criterion tests (26, exact names, one nextest call each) | 26 PASS |

**Negative controls**, all on scratch copies, each failing for its intended reason:

- A planted `/var/home/<user>/…` line. `--published`: `names_home=…planted.txt`, `verdict=fail`. Sanitizer scan: `home_path`, exit 1.
- A hidden file naming the literal `$HOME`. Both checks failed the same way.
- The 50-LIVE-RUN.md check line altered by one character. `--handoff`: `missing=check-line`, rc 1.
- The criterion 6 row removed. `--live-run`: `missing=criterion-row-6`, rc 1.
- The no-defect statement removed. `--defects`: `missing=no non-blocking defect found`, rc 1.
- A misspelled test name. nextest `--no-tests=fail`: FAIL.

**The orchestrator's `/var/home` concern does not hold for this tree.** `--published` also caught the `/var/home/<user>` spelling, because `/var/home/denniyahh` contains `$HOME` (`/home/denniyahh`) as a substring. The sanitizer scan caught it independently.

## Files Created/Modified

- `.planning/phases/50-addressable-monitor-liveness/50-evidence/attempt-1/` (82 files): sanitized curated attempt evidence, with `verdict-attempt.txt` and `verdict-attempt-gate.txt`
- `.planning/phases/50-addressable-monitor-liveness/50-evidence/fault-5/` (12 files): sanitized fault-arm records, with `verdict-fault.txt`, `verdict-attempt-gate.txt` and the operator's `verdict-require-all.txt`
- `.planning/phases/50-addressable-monitor-liveness/50-LIVE-RUN.md`: the verdict record
- Raw tree (not in the repository): `verdict-fault.txt` added to `fault-1`..`fault-5`, `verdict-attempt.txt` to `attempt-1`, and `verdict-attempt-gate.txt` to all six directories

## Decisions Made

- **Only `fault-5` was published.** The plan's action says to process "every fault directory", but its acceptance asks for "a sanitized fault directory". The orchestrator's tie-break and D-17 both point to fault-5, the directory the gate pairs with the attempt. 50-LIVE-RUN.md says why, and records that fault-1..4 also passed (saved raw).
- **No backlog entry.** None of 50-18's or 50-19's findings is an open non-blocking defect. The product `ps -g` hint was fixed by `ce6e718`, the fault-arm member recipe was fixed inside 50-18's tool, and the operator's guided script lives outside the repository and was fixed during the attempt. The "two meanings of unexpected" are both correct under their own definitions, so 50-LIVE-RUN.md records them as a reading note, not as a defect. 999.164 and 999.165 are referenced, not re-filed.
- **Task 3 also records the V-56 result in 50-LIVE-RUN.md.** With no defect, Task 3 would otherwise change no file. The result belongs in the record either way.

## Deviations from Plan

**1. [Rule 3 - Blocking] The attempt was curated before sanitizing, rather than sanitizing whole directories.**
- **Found during:** Task 1
- **Issue:** `attempt-1/project/` holds binary git objects, which the sanitizer refuses (`not_utf8`). `collected/sessions/` holds 9.3 MB of full transcripts, which Phase 49's D-26 keeps out of the repository.
- **Fix:** applied Phase 49's 49-12 curation. Excluded `project/`, `collected/sessions/`, `prelaunch/agent-env-names.txt` and the empty `ledger.lock`. Also excluded `collected/devflow/.gitignore`, which holds `*` and would have made git ignore every published file in that directory. 50-LIVE-RUN.md lists each exclusion.
- **Commit:** `abbfd6f`

**2. [Rule 1 - Bug, my own] Six empty `verdict-attempt-gate.txt` files were briefly written into the raw tree.**
- **Found during:** Task 1
- **Issue:** a `set -o noclobber` stopped the gate's output from being written to its scratch file, and the following `cp` copied that empty scratch file into all six raw directories.
- **Fix:** removed those six zero-byte files, each one created by this run a minute earlier, by their literal paths. Re-ran `--attempt-gate` and copied its 28-line output. Nothing else in the raw tree was touched. A `diff` showed the saved output was identical to the first scratch run.

**3. The plan's 3.0.0 check line was not executed.** Claude Code's built-in removal check refused it (`trap "rm -rf ..."`). The plan does not require running it in this phase, so it was not worked around.

**Total deviations:** 2 auto-fixed (1 blocking, 1 self-inflicted) and 1 note. The scope is unchanged.

## Issues Encountered

- `rg -c '^(warning|error)'` over the macOS check log printed nothing, which is rg's output for zero matches. The gate verdict rests on the exit code, which was 0.

## Known Stubs

None.

## Notes

- STATE.md, ROADMAP.md (any line) and REQUIREMENTS.md were not modified. SUPV-01 is ready to be checked off by the orchestrator, or at verification.
- `git status --short` was clean after each commit. The post-commit hook did not leave `.planning/user/errors/state-stale.json` modified.
- The throwaway repository `denniyahh/p50-live-attempt-1` was not touched, and no GitHub calls were made.

## Self-Check: PASSED

- FOUND: .planning/phases/50-addressable-monitor-liveness/50-LIVE-RUN.md
- FOUND: .planning/phases/50-addressable-monitor-liveness/50-evidence/attempt-1/verdict-attempt-gate.txt
- FOUND: .planning/phases/50-addressable-monitor-liveness/50-evidence/fault-5/verdict-fault.txt
- FOUND: abbfd6f, 97b5def, 05e7eef (ancestors of HEAD)

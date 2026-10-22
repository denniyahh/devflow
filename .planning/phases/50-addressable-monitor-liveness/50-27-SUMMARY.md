---
phase: 50-addressable-monitor-liveness
plan: 27
subsystem: evidence-tooling
tags: [bash, python, evidence-judge, runbook, live-attempt, gap-closure]

requires:
  - phase: 50-addressable-monitor-liveness
    provides: "50-18/50-20's check-supervisor-evidence.sh (--attempt-gate, --src-check, --fault --require-all, selftest harness) and 50-RUNBOOK.md; 50-26's committed WR-03 fix"
provides:
  - "check-supervisor-evidence.sh --attempt-gate <home> --current-source [--repo <checkout>]: refuses up front on uncommitted crate source; the paired fault dir must also pass --src-check; a match prints `ATTEMPT_GATE current_source=yes`"
  - "three selftest cases for --current-source (ships / other source refused / uncommitted refused)"
  - "50-RUNBOOK.md: the operator's 2026-10-21 ruling bullet, a separate planner-adaptations bullet, attempt 2 closed on --current-source for the 50-28 checkpoint, attempt 1's history kept"
affects: [50-28, 50-29, live-attempt-2]

actuals:
  tokens: 4492
  tasks: 2
  commits: 3
plan_head_before: d4ca296b2e074192274a9cbf452e8b0740ba253d
plan_head_after: e6aae4b8c8cca579ba22293d8fbb1c5f4d061faa

tech-stack:
  added: []
  patterns:
    - "Opt-in stricter mode on an existing judge: the flag only adds refusals; without it every existing line and selftest case is unchanged"
    - "Selftest case carries its own controls: the other-source case also asserts the plain gate ships the same home, and the uncommitted case asserts the clean tree ships first"

key-files:
  created: []
  modified:
    - .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh
    - .planning/phases/50-addressable-monitor-liveness/50-RUNBOOK.md

key-decisions:
  - "The uncommitted-crate-source refusal runs after current-attempt is read and validated, so the refusal line names the attempt number (attempt=1), not attempt=ABSENT"
  - "--repo given without --current-source is a usage error (exit 2), and --current-source on --attempt is a usage error: a flag that changes nothing is refused rather than silently ignored"
  - "The crate path list is passed from the bash CRATE_PATHS array into the Python judge, so --src-check, --fault-current and --attempt-gate --current-source share one list"
  - "Section 7's added sentence says the gate refuses an attempt whose paired fault record was not taken from the committed crate tree (what the check actually ties), rather than the plan's shorter 'refuses a binary not built from the committed crate tree', which the plan's own 'does not establish' section says the check cannot prove"

patterns-established: []

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "--attempt-gate --current-source ships only when the fault dir paired with the attempt's binary passes --src-check against the checkout's committed crate tree, and refuses uncommitted crate source"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "check-supervisor-evidence.sh --selftest#attempt gate with current source ships when the paired fault directory records the checkout crate tree"
        status: pass
      - kind: unit
        ref: "check-supervisor-evidence.sh --selftest#attempt gate with current source refuses a paired fault directory from other crate source"
        status: pass
      - kind: unit
        ref: "check-supervisor-evidence.sh --selftest#attempt gate with current source refuses an uncommitted crate change"
        status: pass
    human_judgment: false
  - id: D2
    description: "On the real P50 home (read-only), attempt 1 is refused under --current-source for its crate source, the plain gate still ships it, and the home's listing is unchanged"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "50-27-PLAN.md Task 1 second <automated> gate (real-data control)"
        status: pass
    human_judgment: false
  - id: D3
    description: "50-RUNBOOK.md records the operator's 2026-10-21 delegation apart from the planner adaptations, closes attempt 2 on --current-source for 50-28, and keeps attempt 1's history"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "50-27-PLAN.md Task 2 <automated> gate, run from the plan's exact bytes"
        status: pass
    human_judgment: true
    rationale: "The gate checks phrases and the ruling bullet's word set, not meaning; whether the edited paragraphs read correctly and the ruling bullet holds nothing else of the planner's is a reading judgment (the plan's own 'does not establish')"

duration: 5min
completed: 2026-10-21
status: complete
---

# Phase 50 Plan 27: Attempt gate requires the current crate source, runbook delegation Summary

**`check-supervisor-evidence.sh --attempt-gate --current-source` refuses any attempt whose paired fault record was not taken from the checkout's committed crate tree (attempt 1 is refused on the real home, the plain gate still ships it), and 50-RUNBOOK.md now records the operator's 2026-10-21 delegation of attempt 2, separate from the planner's adaptations, closing attempt 2 on that gate for 50-28.**

## Performance

- **Duration:** about 5 min of execution, measured from the start marker (the reading before it is not counted)
- **Started:** 2026-10-21T20:19:01Z
- **Completed:** 2026-10-21T20:23:54Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- `--attempt-gate` takes `--current-source [--repo <checkout>]`. With the flag it refuses up front (`reason=uncommitted crate source`) when `git status --porcelain` on the crate paths prints anything. A fault directory counts as the attempt's match only if it carries the attempt's `bin_sha256`, passes `--fault --require-all`, and also passes `--src-check` against the checkout. A match prints `ATTEMPT_GATE current_source=yes`. With no match the not-ready reason ends `from the current crate source`. `cap_reached` is unchanged. Without the flag the gate behaves exactly as before.
- Selftest: 55 passed before, 58 passed after (3 new), 0 failed. `bash -n` and shellcheck are clean.
- Real-data control on `~/.cache/devflow-phase50`, read-only, `current-attempt` = 1:
  - plain gate, exit 0: `ATTEMPT_GATE fault=fault-5 bin_sha256=9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6` and `ATTEMPT_GATE verdict=shipped attempt=1`
  - `--current-source`, exit 1: `ATTEMPT_GATE verdict=not_ready attempt=1 reason=no passing fault arm on attempt 1's binary from the current crate source`
  - Attribution check: fault-5 (the only directory on attempt 1's binary) passes `--fault --require-all` (`FAULT complete=yes`, `FAULT verdict=pass`) and fails `--src-check` (`SRC tree_is_head=no recorded_matches=true expected_matches=true`). The refusal comes from the source check alone.
  - The P50 listing (553 entries: paths, sizes, mtimes) was identical before and after.
- 50-RUNBOOK.md was edited at exactly the eight spots the plan lists (nine diff hunks: spots 5 and 6 each touch two places). The Cleanup section is unchanged.

## Task Commits

1. **Task 1: the attempt gate can require the current crate source (tracer, TDD)**
   - RED `5b79ca2` (test): three selftest cases. All three failed because `--current-source` was a usage error (exit 2, confirmed directly). The 55 existing cases still passed.
   - GREEN `2fdb2d9` (feat): `feat(50-27): let the attempt gate require the current crate source`
2. **Task 2: runbook delegation and judge**: `e6aae4b` (docs), `docs(50-27): state the attempt-2 delegation and its judge in the runbook`

## Gate results

| Gate | Result |
|------|--------|
| Task 1 gate 1 (lint + selftest + 8 named cases) | exit 0, `GATE ok`, `SELFTEST passed=58 failed=0` |
| Task 1 gate 2 (real-data control) | exit 0, `GATE ok` |
| Tracer feedback re-run on the committed bytes | `git diff --quiet HEAD` on tools/ held; lint OK; selftest 58/0; the 3 new cases ok. The plan's exact gate-1 and gate-2 commands could not be re-run at this point (see Issues). The pre-commit runs of both were on the same bytes. |
| Task 2 gate, from the plan's exact bytes on the edited runbook | exit 0, `GATE ok` |
| Task 2 gate, exact bytes, negative controls | pre-edit runbook (`HEAD` before e6aae4b): exit 1 at the first `rg`. Edited copy with the superseded "Run every step…" sentence appended: exit 1 with `FAILED: runbook still holds (rg exit 0): …` |

## Files Created/Modified

- `.planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh`: the `--current-source`/`--repo` options for `--attempt-gate`, a usage line and paragraph, and 3 selftest cases with their fixtures
- `.planning/phases/50-addressable-monitor-liveness/50-RUNBOOK.md`: the eight spots

## Decisions Made

See `key-decisions` in the frontmatter. All four fall within the plan's scope. The last one rewords a sentence the plan dictated so that it states only what the check proves.

## Deviations from Plan

- **TDD commit split (protocol, not a rule deviation):** the plan names one commit for Task 1. TDD mode (`workflow.tdd_mode: true`, `tdd="true"`) calls for a separate RED commit, so Task 1 has `test(50-27)` followed by `feat(50-27)`. The feat commit uses the plan's exact message.
- **Rule 2, a small addition:** `--repo` without `--current-source`, and `--current-source` on `--attempt`, now exit 2 (usage error). The plan does not specify this. It stops a flag from being silently ignored and changes no existing invocation. Checked by hand (both exit 2). There is no selftest case for it.

**Total deviations:** 1 small Rule-2 addition and 1 commit split required by the TDD protocol. **Impact:** no scope change.

## TDD Gate Compliance

- RED `5b79ca2`, then GREEN `2fdb2d9`. No refactor commit was needed.
- RED evidence: the selftest's own `  ok   / FAIL` line format is not one of the report formats `gsd-tools check tdd-red-evidence` supports (TAP, JUnit, go test, swift-testing, unittest), so the classifier was not run. Semantic assessment: each of the three target cases failed in its own `st_case` line, and the cause was the planned one, `--current-source` being an unknown option (exit 2, confirmed by a direct invocation). The other 55 cases passed in the same run, so this was not a load or fixture fault. That is the RED the plan specifies ("they must fail, because the flag is a usage error (exit 2) before the code exists"). Machine classification of it is absent, not passed.

## Issues Encountered

- Claude Code's built-in removal safety check blocked one combined re-run of the plan's two Task 1 gate commands after the GREEN commit. Their `trap "rm -f …"` sits inside a `bash -c` script the check cannot inspect, and the permission prompt timed out. That command did not run, and it left nothing behind. I did not retry it in another form. The same gate commands had already passed before the commit on identical bytes (`git diff --quiet HEAD` confirmed this). The post-commit selftest was re-run without any removal. Consequence for later plans: anyone running these plan gates may hit the same prompt.

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

- 50-28 can close only on `--attempt-gate "$HOME/.cache/devflow-phase50" --current-source`. Attempt 1 cannot stand in for attempt 2.
- What this does not establish:
  - That `--current-source` ships attempt 2. That is 50-28's verify, after the attempt.
  - That the binary's bytes came from the committed tree. The check ties the paired fault record's `src_tree` to the committed crate tree. The binary rests on `prepare` building from the checkout just before the fault arm runs.
  - That the runbook reads correctly. The runbook gate checks phrases, not meaning.
- `.planning/user/errors/state-stale.json` remains modified and uncommitted (the post-commit hook writes it), as instructed.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-21*

## Self-Check: PASSED

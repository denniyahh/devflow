---
phase: 50-addressable-monitor-liveness
plan: 01
subsystem: planning-tooling
tags: [cargo-semver-checks, semver, bash, roadmap]

requires: []
provides:
  - "tools/semver-gate.sh: staged named-break gate (`check --through <stage> [--log <file>]`, `compare --through <stage> [--fail-count <n>] <file>`, `--selftest`) for 50-08, 50-10, 50-11, 50-16"
  - "ROADMAP Phase 50 criteria 4-8, `**Folded backlog**:` line, fold markers on 999.128/134/138/139/141, 999.143 monitor-pid note"
affects: [50-04, 50-05, 50-08, 50-10, 50-11, 50-16, 50-24]

actuals:
  tokens: 4802
  tasks: 2
  commits: 3
plan_head_before: f6100b0ff289d476d458ac449f3d6a65b37a507b
plan_head_after: e0750f13b1c43a779918f74c46f16f0145ecddb0

tech-stack:
  added: []
  patterns:
    - "Item-level semver gate: every failing item must match a named (lint, identifier) row, every required row must be hit, and the parsed block count must equal semver-checks' own fail count"

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/tools/semver-gate.sh
  modified:
    - .planning/ROADMAP.md

key-decisions:
  - "Errors (no failure lines, unparseable header or item line, a block with no items, a fail-count disagreement, semver-checks exiting other than 0/100) print `SEMVER verdict=error reason=...` and exit 3, distinct from mismatch (1) and usage (2)"
  - "check mode cross-checks the number of parsed failure blocks against the `N fail` total semver-checks prints on stderr, so a block the parser does not recognise cannot silently drop its items"

patterns-established:
  - "Phase 50 shell tools: selftest prints `  ok   <case>` / `  FAIL <case>` and `SELFTEST passed=N failed=M`, as Phase 49's negctl.sh"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "semver-gate.sh selftest: 15 cases over synthetic logs in the real 0.51.0 layout, including the plan's six (unlisted lint, unexpected item under a listed lint, missing required item, not-yet-happened stage, empty run, baseline match)"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "bash .planning/phases/50-addressable-monitor-liveness/tools/semver-gate.sh --selftest -> SELFTEST passed=15 failed=0"
        status: pass
      - kind: other
        ref: "Task 1 <automated> gate run verbatim at 7484bc2 -> rc 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "Real cargo semver-checks run matches --through baseline; the same saved output mismatches --through 50-08 naming enum_marked_non_exhaustive:MonitorError as missing"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "semver-gate.sh check --through baseline --log <f> -> SEMVER verdict=match; compare --through 50-08 <f> -> exit 1, missing=enum_marked_non_exhaustive:MonitorError"
        status: pass
    human_judgment: false
  - id: D3
    description: "ROADMAP Phase 50 criteria 1-8, Folded backlog line, five fold markers, 999.143 note; parsers still read SUPV-01"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "Task 2 <automated> gate run verbatim at e0750f1 -> rc 0 (three negative controls exit 1)"
        status: pass
    human_judgment: true
    rationale: "The gate proves presence and list equality; whether criteria 4-8 state the 2026-10-10 discussion's intent correctly (and add no D-20 migration wording) is a reading judgment the plan itself marks `verification: judgment`"

duration: 8min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 01: Semver Gate and ROADMAP Scope Summary

**A staged semver gate that compares the real `cargo semver-checks` 0.51.0 run, item by item, with R-9's named breaks for each plan stage, plus ROADMAP Phase 50 criteria 4-8 and the folded-backlog markers.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-10-17T11:16:02Z
- **Completed:** 2026-10-17T11:23:41Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- `tools/semver-gate.sh` (mode 100755): one literal named-break table at the top (8 rows: 3 baseline, 2 at 50-08, 1 each at 50-10, 50-11, 50-16). `check` runs `cargo semver-checks check-release -p devflow-core --color never`, saves stdout with `--log`, and compares. `--through S` includes every row introduced at S or earlier. Verdict `match` (exit 0) only when every parsed item matches an included row by lint id and whole-word identifier (`rg -w -F`), every included `required` row is hit, and at least one block was parsed.
- The real run at this plan's tree printed:
  ```
  SEMVER lint=constructible_struct_adds_field item=field AgentResult.checkpoint
  SEMVER lint=enum_marked_non_exhaustive item=enum RecoverError
  SEMVER lint=enum_marked_non_exhaustive item=enum WorkflowError
  SEMVER verdict=match
  ```
  The same saved output, compared `--through 50-08`, printed `SEMVER verdict=mismatch unexpected=- missing=enum_marked_non_exhaustive:MonitorError` and exited 1.
- ROADMAP Phase 50: `**Folded backlog**: 999.128, 999.134, 999.138, 999.139, 999.141 (...)` below the byte-identical `**Requirements**: SUPV-01`; criterion 3 names `registry::cache_dir()`; criteria 4-8 added; the migration paragraph says the criterion now numbered 4 came from the 2026-10-10 discussion. Fold markers under the five backlog entries, and the monitor-pid note under 999.143. 999.130 and Phase 50.1 are byte-identical to before.

## Task Commits

1. **Task 1 (tracer): staged semver gate.** RED `3e55306` test(50-01), GREEN `7484bc2` feat(50-01)
2. **Task 2: ROADMAP Phase 50 amendments.** `e0750f1` docs(50-01)

Tracer feedback gate: interactive run (`auto_advance=false`, `_auto_chain_active=false`), `human_verify_mode=end-of-phase`, automated-only `<verify>`, so row 3 applied: the Task 1 gate was re-run at `7484bc2` with a clean tree (rc 0, `SELFTEST passed=15 failed=0`, real run `verdict=match`), then Task 2 went ahead.

## TDD Gate Compliance

| Task | RED commit | RED run | RED evidence check | GREEN commit | GREEN run |
|------|-----------|---------|--------------------|--------------|-----------|
| 1 | `3e55306` test(50-01) | exit 1, `SELFTEST passed=1 failed=13`; target line: `  FAIL a stage whose break has not happened yet is missing — exit 99 (want 1), got [SEMVER verdict=unimplemented], want [SEMVER verdict=mismatch unexpected=- missing=enum_marked_non_exhaustive:MonitorError]` | `RED_EVIDENCE_OK` / `target_test_failed` (14 tests, 13 fail) | `7484bc2` feat(50-01) | `SELFTEST passed=15 failed=0` |

Semantic assessment: every failing case ran and failed on its planned assertion (exit code plus verdict line) because `compare_log` was a stub; the synthetic logs were built and the usage-error case (argument parsing, present in RED) passed, so this was not a load or fixture failure. `check tdd-red-evidence` parses TAP, so the RED run's `  ok` / `  FAIL` lines were transcoded one-to-one into TAP before the check (format bridge only, as in 49-03). Task 2 is a docs task with no `tdd` attribute. No REFACTOR commit.

**Mutation checks (not in the plan, run for discrimination):** 10 mutants of the GREEN script, each run through the selftest on a scratch copy; all 10 killed: whole-word match dropped, missing-row collection dropped, stage filter disabled, empty-block check disabled, lint-id equality dropped, fail-count check disabled, path not stripped, `allowed` rows treated as required, no-blocks check disabled, header check disabled. The lint-id mutant first survived the 14-case selftest; the 15th case (an unlisted lint whose item names a listed identifier) was added in the GREEN commit to kill it.

## Gate results (run verbatim from the plan)

- Task 1 `<automated>`: rc 0 at `7484bc2` (selftest `passed=15 failed=0`, real `check --through baseline` `verdict=match`, negative control `compare --through 50-08` exit 1 naming `enum_marked_non_exhaustive:MonitorError` missing, index mode `100755`, `bash -n` and `shellcheck` clean). Failing direction: at `3e55306` the selftest exits 1, which ends the `&&` chain.
- Task 2 `<automated>`: rc 0 at `e0750f1`. Negative controls, each with the gate's `R=` pointed at a modified copy: the pre-plan ROADMAP (`HEAD` at `f6100b0`) exits 1; the current ROADMAP plus a sixth fold marker under 999.130 exits 1; the current ROADMAP with `999.128` appended to the Requirements line exits 1. `init.plan-phase 50` prints `phase_req_ids` = `SUPV-01`; `roadmap.analyze` exits 0.
- Plan `<verification>`: `git diff --stat "$(git log --format=%H --grep='(50-01)' | tail -1)^..HEAD"` lists only `.planning/ROADMAP.md` (37 lines) and `tools/semver-gate.sh` (338 lines).
- Acceptance: 999.130 (47 lines) and Phase 50.1 (27 lines) hash identical before and after; every ROADMAP hunk sits in the Phase 50 section or directly under one of the six named backlog headings. `git log -- tools/semver-gate.sh` subjects both carry `(50-01)` (the 50-16 provenance check).

What these do not establish: the gate is proven on synthetic logs in the 0.51.0 layout and on one real run whose only failures are the three baseline items. The item formats for `function_parameter_count_changed`, `function_missing`, `struct_pub_field_missing` and `enum_variant_added` come from those lints' 0.51.0 `per_result_error_template`s (fetched from the v0.51.0 tag), not from a real run; 50-08/50-10/50-11/50-16 are the first real runs that produce them. A different semver-checks version may change the layout; the parser then errors (exit 3) rather than passing, as far as the selftest's header, item-line, empty-block and fail-count cases cover.

## Files Created/Modified

- `.planning/phases/50-addressable-monitor-liveness/tools/semver-gate.sh` - the staged named-break gate with its selftest
- `.planning/ROADMAP.md` - Phase 50 criteria 4-8, Folded backlog line, criterion-3 clause, migration-paragraph sentence, five fold markers, 999.143 note

## Decisions Made

- Error exit code 3 with a `SEMVER verdict=error reason=...` line; the plan specified exit codes only for match (0), mismatch (1) and usage (2).
- Mismatch lists are `;`-joined, `-` when empty (item text contains spaces).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Parser refuses output it does not recognise, and check mode counts blocks a second way**
- **Found during:** Task 1
- **Issue:** As specified, a failure block whose header did not match the pattern, or whose items were indented differently, would contribute no items, so its breaks would be invisible and could not be reported as unexpected: a false green, the exact threat (T-50-42) the gate exists for.
- **Fix:** A `--- failure` header that does not parse, a non-two-space line inside a `Failed in:` list, and a block with no items are each `verdict=error` (exit 3). `check` reads semver-checks' own `N fail` total from stderr and passes it as `--fail-count` to the comparison, which errors when it differs from the parsed block count. Four selftest cases cover these.
- **Files modified:** `tools/semver-gate.sh`
- **Verification:** selftest cases 11-14 pass; the header-check, empty-block and fail-count mutants are killed; the real run's `2 fail` equals its 2 parsed blocks.
- **Committed in:** `3e55306` (cases), `7484bc2` (implementation)

**2. [Rule 2 - Missing critical] `--color never` on the cargo invocation**
- **Found during:** Task 1
- **Issue:** The parser matches literal text; colour escapes would break it if cargo ever decided the output was a terminal.
- **Fix:** `cargo semver-checks check-release -p devflow-core --color never`.
- **Files modified:** `tools/semver-gate.sh`
- **Verification:** real run parsed and matched `baseline`.
- **Committed in:** `3e55306`

**3. [Rule 1 - Test gap] Cross-lint selftest case added in the GREEN commit**
- **Found during:** Task 1, mutation check
- **Issue:** With the lint-id equality removed, an item of an unlisted lint naming a listed identifier would match that row; the 14-case selftest did not notice.
- **Fix:** Case "an unlisted lint naming a listed identifier is unexpected" (`enum_variant_missing` item `variant WorkflowError::Paused`).
- **Files modified:** `tools/semver-gate.sh`
- **Verification:** the mutant now fails one case; the real script passes all 15.
- **Committed in:** `7484bc2` (a test added after RED, so it never had its own RED run; it was green on arrival)

---

**Total deviations:** 3 auto-fixed (2 missing critical, 1 test gap)
**Impact on plan:** All three tighten the gate against false greens; the interface the later plans call (`check --through <stage>`, exit 0 only on `verdict=match`) is unchanged. No scope creep beyond the tool.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 50-08, 50-10, 50-11 and 50-16 can run `semver-gate.sh check --through <their stage>` before their break commits. 50-16's provenance check (every commit touching the tool carries `(50-01)`) holds today; any later fix to the tool would break it, so a defect found in it later needs a decision, not a quiet edit.
- SUPV-01 is listed in `requirements-completed` because the template requires copying the plan's `requirements`; this plan does not deliver SUPV-01 itself. Marking it complete is left to the orchestrator.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

Files: `tools/semver-gate.sh`, `50-01-SUMMARY.md` exist. Commits `3e55306`, `7484bc2`, `e0750f1` are ancestors of HEAD.

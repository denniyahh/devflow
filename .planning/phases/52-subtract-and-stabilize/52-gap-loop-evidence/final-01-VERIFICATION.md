---
phase: 01-calculator
verified: 2026-11-08T22:30:00Z
status: passed
score: 9/9 must-haves verified
covered_files:
  - ".planning/phases/01-calculator/01-01-PLAN.md"
  - ".planning/phases/01-calculator/01-01-SUMMARY.md"
  - ".planning/phases/01-calculator/01-02-PLAN.md"
  - ".planning/phases/01-calculator/01-02-SUMMARY.md"
  - "calc.py"
  - "test_calc.py"
covered_digest: "v4:sha256:bf0045438d1395af063dcd4963ccea775b1ee5e3e6e76d610078671fb68026b9"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 4/8
  gaps_closed:
    - "calc.sub(5, 3) == 2 from the repo root (CALC-02, ROADMAP SC 2)"
    - "python3 -m unittest -v test_calc lists test_sub_* methods and ends in OK (ROADMAP SC 3)"
    - "sub tests FAIL against a b - a mutant"
    - "add and sub each take exactly parameters a, b"
  gaps_remaining: []
  regressions: []
---

# Phase 1: Calculator Verification Report

**Phase Goal:** `calc.py` provides `add` and `sub`, each with a passing unittest in `test_calc.py`.
**Verified:** 2026-11-08T22:30:00Z
**Status:** passed
**Re-verification:** Yes, after gap closure. The previous report said `gaps_found` because commit 60fcca2 deleted `sub` and `TestSub`. Plan 01-02 (commit 2e4a35b) restored them.

## Goal Achievement

### Observable Truths

All checks below were run in this session from the repo root, without `python3 -I`, with `PYTHONDONTWRITEBYTECODE=1`.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | ROADMAP SC 1 / CALC-01: `python3 -c "import calc; assert calc.add(2, 3) == 5"` succeeds | VERIFIED | exit 0 |
| 2 | ROADMAP SC 2 / CALC-02: `python3 -c "import calc; assert calc.sub(5, 3) == 2"` succeeds | VERIFIED | exit 0. Negative control: the same command with a nonexistent function (`calc.nosuch`) exits 1 with AttributeError, so the check can fail. |
| 3 | ROADMAP SC 3: `python3 -m unittest -v test_calc` passes with a test for each function | VERIFIED | exit 0, `Ran 4 tests`, final line `OK`. Lists `test_add_handles_negative_operand`, `test_add_returns_sum_of_positive_ints`, `test_sub_returns_difference`, `test_sub_subtracts_second_operand_from_first`. |
| 4 | Add tests FAIL against a wrong `add` | VERIFIED (behavioral) | In-process rebinding: `add = a - b` makes both test_add_* FAIL (0 errors); `add = a * b` also makes both FAIL. |
| 5 | Sub tests FAIL (not ERROR) against a wrong `sub` | VERIFIED (behavioral) | `sub = b - a` makes both test_sub_* FAIL, 0 errors; `sub = a + b` also makes both FAIL. Control: a correct `a - b` rebinding runs 4 tests with 0 failures and 0 errors. |
| 6 | `import calc` resolves to the repo-root calc.py | VERIFIED | `Path(calc.__file__).resolve() == Path('calc.py').resolve()` printed True |
| 7 | Only imports are `unittest` and `calc` | VERIFIED | `rg` for import statements: only `test_calc.py:2 import unittest` and `test_calc.py:4 import calc`; calc.py imports nothing |
| 8 | `add` / `sub` each take exactly parameters `a`, `b` | VERIFIED | `inspect.signature`: `(a: float, b: float) -> float` for both |
| 9 | `calc.py` and `test_calc.py` are byte-identical to f19cd3a (last 8/8 state) and CALC-01 did not regress | VERIFIED | `git diff --exit-code f19cd3a2a223005f04782fa3661de82fa0011df7 -- calc.py test_calc.py` exits 0. Negative control: the same diff against 60fcca2 exits 1. `git status --porcelain -- calc.py test_calc.py` is empty. |

**Score:** 9/9 truths verified (0 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `calc.py` | exports `add`, `sub` | VERIFIED | 10 lines; `add` returns `a + b`, `sub` returns `a - b`; real implementations, not stubs; used by tests |
| `test_calc.py` | `TestAdd`, `TestSub` | VERIFIED | `TestAdd` 2 methods, `TestSub` 2 methods; assertions pin results and operand order (`sub(3, 5) == -2`) |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| test_calc.py | calc.py | `calc.add(` in TestAdd | WIRED (both calls present; add mutants break the tests) |
| test_calc.py | calc.py | `calc.sub(` in TestSub | WIRED (both calls present; sub mutants break the tests) |

### Data-Flow Trace (Level 4)

Not applicable. Pure functions with no rendered or persisted data.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| add works | `python3 -c "import calc; assert calc.add(2, 3) == 5"` | exit 0 | PASS |
| sub works | `python3 -c "import calc; assert calc.sub(5, 3) == 2"` | exit 0 | PASS |
| suite green | `python3 -m unittest -v test_calc` | 4 tests, OK | PASS |
| sub tests discriminate | in-process mutants `b - a`, `a + b` | both sub tests FAIL, 0 errors | PASS |
| add tests discriminate | in-process mutants `a - b`, `a * b` | both add tests FAIL, 0 errors | PASS |
| control for mutant harness | correct `a - b` sub in the same harness | 4 run, 0 fail, 0 error | PASS |

The mutants were applied in memory only. No file was written.

### Probe Execution

No probes declared or present. SKIPPED.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| CALC-01 | 01-01 | `calc.add(a, b)` returns `a + b`, unittest in test_calc.py | SATISFIED | truths 1, 3, 4 |
| CALC-02 | 01-01, 01-02 | `calc.sub(a, b)` returns `a - b`, unittest in test_calc.py | SATISFIED | truths 2, 3, 5 |

Both IDs appear in REQUIREMENTS.md and in plan frontmatter (01-01 lists both; 01-02 lists CALC-02). No orphaned requirements: REQUIREMENTS.md maps only CALC-01 and CALC-02 to Phase 1.

### Anti-Patterns Found

None. `rg` for TBD/FIXME/XXX/TODO/HACK/PLACEHOLDER/eval/exec in calc.py and test_calc.py returned no matches (exit 1).

### Human Verification Required

None.

### Deferred Items

None.

### Gaps Summary

No gaps. All four gaps from the previous report are closed, and nothing that passed before has regressed.

**What this verification does not establish:**
- Float, overflow and non-numeric behavior of `add` and `sub` is untested (plan assumptions A-01 and A-02, flagged and unresolved by design). `add(0.1, 0.2)` returns `0.30000000000000004`; the tests only pin small ints.
- The mutant gate covers four hand-picked mutants (two per function). It does not show the tests catch every possible wrong implementation. For example, a `sub` that special-cases `(5, 3)` and `(3, 5)` would pass.
- "Passes" is measured on this working tree at HEAD (2ee11fd), not on a clean checkout elsewhere.

### Documentation Drift (not a code gap)

These planning files are stale relative to the code. I did not edit them.
- `.planning/REQUIREMENTS.md`: CALC-01 is unchecked and traceability says `Gaps Found`. The code satisfies CALC-01 (it never failed).
- `.planning/ROADMAP.md`: the phase table says `Not started` and the progress table says `In Progress`, while plans show 2/2 executed. The orchestrator's phase-completion step normally updates these.

---

_Verified: 2026-11-08T22:30:00Z_
_Verifier: Claude (gsd-verifier)_

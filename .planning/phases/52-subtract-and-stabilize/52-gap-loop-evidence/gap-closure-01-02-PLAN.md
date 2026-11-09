---
phase: 01-calculator
plan: 02
type: execute
wave: 2
depends_on:
  - 01-01
files_modified:
  - calc.py
  - test_calc.py
autonomous: true
gap_closure: true
requirements:
  - CALC-02

estimate:
  tokens: 18000
  raw_tokens: 18000
  tasks: 1
  confidence: low

must_haves:
  truths:
    - 'From the repo root, python3 -c "import calc; assert calc.sub(5, 3) == 2" exits 0 (ROADMAP success criterion 2, CALC-02)'
    - 'python3 -m unittest -v test_calc exits 0, lists at least one test_add_* method and at least one test_sub_* method, reports Ran 4 tests, and its final line is exactly OK (ROADMAP success criterion 3)'
    - 'The unittest suite reports FAIL (not ERROR) on a test_sub_* method when calc.sub is replaced by a mutant that computes b - a, and the same harness with a correct a - b replacement runs 4 tests with no failure, error or skip'
    - 'calc.add and calc.sub each take exactly two parameters named a and b, matching calc.add(a, b) and calc.sub(a, b) in REQUIREMENTS.md'
    - 'calc.py and test_calc.py are byte-identical to their content at commit f19cd3a2a223005f04782fa3661de82fa0011df7, the last state that passed verification 8/8'
    - 'CALC-01 does not regress: python3 -c "import calc; assert calc.add(2, 3) == 5" exits 0, and a subtracting add mutant still makes a test_add_* method FAIL'
    - 'calc.py and test_calc.py together import exactly unittest and calc, and import calc run from the repo root resolves to the repo-root calc.py'
  artifacts:
    - path: calc.py
      provides: 'add(a, b) returning a + b (unchanged) and sub(a, b) returning a - b (restored)'
      exports: [add, sub]
    - path: test_calc.py
      provides: 'unittest.TestCase classes TestAdd (unchanged) and TestSub (restored, 2 methods)'
      exports: [TestAdd, TestSub]
  key_links:
    - from: test_calc.py
      to: calc.py
      via: 'module import (import calc) followed by calc.sub calls inside TestSub'
      pattern: 'calc\.sub\('
  prohibitions: []
---

<objective>
Close the four failed truths in 01-VERIFICATION.md by restoring `calc.sub` and `TestSub`. Commit 60fcca2 deleted both. This satisfies CALC-02 again.

The restore takes both files from commit f19cd3a2a223005f04782fa3661de82fa0011df7. That is the last commit before the deletion, and the only commit after it that touches these files is 60fcca2. The content is not retyped. The test file is restored first so the suite shows a real red state. Then the implementation is restored so the suite goes green. A mutant gate then shows the restored tests detect an operand-swapped `sub`. The gate swaps the function in memory and never writes a mutant to disk.

Purpose: ROADMAP success criteria 2 and 3 pass again, with sub tests that detect a wrong implementation rather than a green suite that has none. The green-but-empty case is exactly what 60fcca2 produced.
Output: `calc.py` and `test_calc.py` modified (restored), one commit. No other files change.

Gaps closed (from 01-VERIFICATION.md `gaps:`):
1. `python3 -c "import calc; assert calc.sub(5, 3) == 2"` exits 0, closing failed truth 2.
2. `python3 -m unittest -v test_calc` lists test_sub_* methods and ends in OK, closing failed truth 3.
3. The sub tests FAIL against a `b - a` mutant, closing failed truth 5.
4. add and sub each take exactly a, b, closing failed truth 8.
</objective>

<execution_context>
@~/.claude/gsd-core/workflows/execute-plan.md
@~/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/REQUIREMENTS.md
@.planning/phases/01-calculator/01-VERIFICATION.md

Repo state at planning time (HEAD fa18d06, branch feature/phase-01): `calc.py` defines only `add`, and `test_calc.py` defines only `TestAdd` (2 methods). `git log f19cd3a..HEAD -- calc.py test_calc.py` lists only 60fcca2, and `git diff f19cd3a2a223005f04782fa3661de82fa0011df7 -- calc.py test_calc.py` shows 12 deleted lines and nothing else. So the restore puts back exactly what 60fcca2 removed, plus the trailing blank line it left at the end of calc.py.

Why these verify commands differ from 01-01's: the 01-01 mutant gates ran in a temp directory and ended with `rm`. The executor's permission layer blocked that, so the 01-01 SUMMARY records a deviation. The same SUMMARY records one false result caused by stale bytecode in a reused scratch directory. The gates below avoid both problems. They swap the function inside a single `python3 -c` process, so they create, copy and delete no files. None of them uses `python3 -I`, which drops the current directory from sys.path and breaks `import calc` (noted in 01-VERIFICATION.md).

Planning-time grounding (Python 3.15.2, git 2.56.0). Every gate below was run verbatim against six source variants:
- a copy of HEAD;
- a copy of f19cd3a;
- `sub` restored without TestSub;
- TestSub restored without `sub` (the red state);
- a file-level `return b - a` mutant;
- TestSub rewritten to use symmetric operands, `sub(2, 2) == 0` and `sub(4, 4) == 0`, which cannot tell `a - b` from `b - a`.

The full procedure was then rehearsed in a scratch clone, with bytecode caching both on and off, and with stale HEAD bytecode present. Results:
- f19cd3a and the rehearsed restore: every gate exits 0. `git diff --exit-code` against f19cd3a still exits 0 after all gates run.
- HEAD: the success-criterion-2 gate, the listing gate, the sub mutant gate, the control gate and the signature gate all exit 1. The verbatim success-criterion-3 command `python3 -m unittest test_calc` exits 0 at HEAD. That is the green-but-empty trap, and it is why the listing gate exists.
- `sub` restored without tests: success criterion 2 passes, but the listing gate (`no test_sub_ line`) and the sub mutant gate (mutant survived) both exit 1.
- Symmetric-operand tests: the listing gate passes, but the sub mutant gate exits 1 with `b - a mutant survived`. This is the negative control showing the mutant gate catches tests that do not discriminate.
- Red state: `python3 -m unittest -v test_calc` exits 1 with `FAILED (errors=2)`, and both test_sub_* methods show `AttributeError: module 'calc' has no attribute 'sub'`.
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1 (tracer, whole plan): restore TestSub (red), then calc.sub (green), and prove the sub tests detect an operand-swapped mutant</name>
  <precondition>From the repo root, `git cat-file -e f19cd3a2a223005f04782fa3661de82fa0011df7:calc.py && git cat-file -e f19cd3a2a223005f04782fa3661de82fa0011df7:test_calc.py && python3 --version` exits 0. Halt if the restore source is not reachable; do not retype the files from memory.</precondition>
  <files>calc.py, test_calc.py</files>
  <read_first>
    - calc.py (currently defines only `add(a: float, b: float) -> float`; add stays byte-identical)
    - test_calc.py (currently defines only `TestAdd`; TestAdd stays byte-identical)
    - .planning/phases/01-calculator/01-VERIFICATION.md (the `gaps:` block: four failed truths and the missing items)
    - .planning/REQUIREMENTS.md (CALC-02 wording: calc.sub(a, b) returns a - b)
    - .planning/ROADMAP.md (Phase 1 success criteria 2 and 3)
    - `git show f19cd3a2a223005f04782fa3661de82fa0011df7:calc.py` and `git show f19cd3a2a223005f04782fa3661de82fa0011df7:test_calc.py` (the restore source)
  </read_first>
  <behavior>
    - calc.sub(5, 3) returns 2
    - calc.sub(3, 5) returns -2 (first operand minus second; operand order matters)
    - With calc.sub rebound to a b - a mutant, at least one test_sub_* method reports FAIL, not ERROR
    - With calc.sub rebound to a correct a - b function, the same harness runs 4 tests with no failure, error or skip (negative control)
    - TestAdd still passes, and a subtracting add mutant is still detected
  </behavior>
  <action>
Work from the repo root. Restore content from git; do not hand-type it. All three steps use the full SHA f19cd3a2a223005f04782fa3661de82fa0011df7 so an abbreviated ref cannot resolve to anything else.

Step 1, restore the test first (red). Run `git restore --source=f19cd3a2a223005f04782fa3661de82fa0011df7 --worktree -- test_calc.py`. This puts back class `TestSub(unittest.TestCase)` directly after `TestAdd` and before the `if __name__ == "__main__":` guard. TestSub has two single-assertion methods:
- `test_sub_returns_difference` asserts `calc.sub(5, 3)` equals 2;
- `test_sub_subtracts_second_operand_from_first` asserts `calc.sub(3, 5)` equals -2. This case pins operand order.

Run `python3 -m unittest -v test_calc`. Confirm the expected red state: exit 1, a last line of `FAILED (errors=2)`, and both test_sub_* methods reported as ERROR with `AttributeError: module 'calc' has no attribute 'sub'`. Record this red-state output in the SUMMARY. If the red state differs (for example a different error count, or an ERROR in a test_add_* method), stop and report it. Do not proceed.

Step 2, restore the implementation (green). Run `git restore --source=f19cd3a2a223005f04782fa3661de82fa0011df7 --worktree -- calc.py`. This puts back `def sub(a: float, b: float) -> float:` directly below `add`, with the docstring "Return a minus b." and the body returning `a - b`. It also drops the trailing blank line 60fcca2 left after `add`. `add` is unchanged. calc.py still has no import statements, no validation, coercion, rounding, I/O or string evaluation. Flagged assumption A-02 records the operand domain.

Run `python3 -m unittest -v test_calc` until it reports `Ran 4 tests` and a bare `OK` last line.

Step 3, prove discrimination and exactness. Run every `<verify>` gate in the order listed. The exact-restore diff gate runs last on purpose, because it also proves the in-memory mutant gates left nothing on disk. The mutant gates rebind `calc.sub` (or `calc.add`) inside one `python3 -c` process before `test_calc` is imported. They do not write, copy or delete files, so there is nothing to clean up. These gates measure only whether the tests discriminate, not whether the implementation is correct. At planning time they also passed with TestSub restored and `sub` still missing, which is why success criterion 2 and the listing gate are separate gates.

Commit by explicit path only: `git add calc.py test_calc.py`. Do not use `git add -A` or `git add .`. `.planning/state.json` is untracked and outside this plan, and `__pycache__/` is ignored. Use a single commit with a conventional message such as `fix(01-02): restore calc.sub and TestSub removed in 60fcca2`.

Do not edit REQUIREMENTS.md, ROADMAP.md or STATE.md as part of this task. The orchestrator owns requirement and roadmap status.
  </action>
  <verify>
    <automated>python3 -c "import calc; assert calc.sub(5, 3) == 2"</automated>
    <fails_when>non-zero exit (1), with stderr ending in "AttributeError: module 'calc' has no attribute 'sub'" (the baseline at HEAD) or an AssertionError traceback</fails_when>
    <automated>python3 -c "import subprocess, sys; p = subprocess.run([sys.executable, '-m', 'unittest', '-v', 'test_calc'], capture_output=True, text=True); out = p.stdout + p.stderr; print(out); lines = out.splitlines(); assert p.returncode == 0, ('unittest exit code', p.returncode); assert any(l.startswith('test_add_') for l in lines), 'no test_add_ line'; assert any(l.startswith('test_sub_') for l in lines), 'no test_sub_ line'; assert any(l.startswith('Ran 4 tests ') for l in lines), 'run count is not 4'; assert lines[-1] == 'OK', ('last line', lines[-1])"</automated>
    <fails_when>non-zero exit (1) with an AssertionError naming the broken check: "no test_sub_ line" (the baseline at HEAD and with sub restored but no tests), "no test_add_ line", "run count is not 4", ('unittest exit code', N), or ('last line', ...) when the summary is not a bare OK</fails_when>
    <automated>python3 -c "import unittest, calc; calc.sub = lambda a, b: b - a; import test_calc; r = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(test_calc)); failed = [t.id().rsplit('.', 1)[1] for t, _ in r.failures]; print('FAIL:', failed, 'ERROR:', [t.id() for t, _ in r.errors]); assert not r.errors, 'mutant run raised errors, not assertion failures'; assert any(n.startswith('test_sub_') for n in failed), 'b - a mutant survived: no test_sub_ method failed'"</automated>
    <fails_when>non-zero exit (1) with "AssertionError: b - a mutant survived: no test_sub_ method failed" (the baseline at HEAD and with symmetric-operand tests), or "mutant run raised errors, not assertion failures", or a ModuleNotFoundError/ImportError when test_calc cannot be imported. On success, stdout shows a FAIL: list that contains test_sub_ names.</fails_when>
    <automated>python3 -c "import unittest, calc; calc.sub = lambda a, b: a - b; import test_calc; r = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(test_calc)); print('ran', r.testsRun, 'failures', len(r.failures), 'errors', len(r.errors), 'skipped', len(r.skipped)); assert r.wasSuccessful() and r.testsRun == 4 and not r.skipped, 'control harness is not green with a correct sub'"</automated>
    <fails_when>non-zero exit (1) with "AssertionError: control harness is not green with a correct sub"; the printed counts show which check broke (baseline at HEAD: "ran 2 failures 0 errors 0 skipped 0")</fails_when>
    <automated>git --no-pager diff --exit-code f19cd3a2a223005f04782fa3661de82fa0011df7 -- calc.py test_calc.py</automated>
    <fails_when>exit 1 with diff hunks printed for calc.py or test_calc.py (the baseline at HEAD: 12 deleted lines across the two files)</fails_when>
  </verify>
  <acceptance_criteria>
    - The red state in Step 1 is recorded in the SUMMARY: `python3 -m unittest -v test_calc` exited 1, the last line was `FAILED (errors=2)`, and the two ERROR entries were test_sub_* methods with `AttributeError: module 'calc' has no attribute 'sub'`.
    - `python3 -m unittest test_calc` (ROADMAP success criterion 3, verbatim) exits 0.
    - `python3 -m unittest -v test_calc` shows these four lines, each ending in `... ok`: `test_add_returns_sum_of_positive_ints`, `test_add_handles_negative_operand`, `test_sub_returns_difference` and `test_sub_subtracts_second_operand_from_first`.
    - `python3 -c "import calc, inspect; assert [list(inspect.signature(f).parameters) for f in (calc.add, calc.sub)] == [['a', 'b'], ['a', 'b']]"` exits 0. It exits 1 at HEAD.
    - CALC-01 non-regression: `python3 -c "import calc; assert calc.add(2, 3) == 5"` exits 0. Also, `python3 -c "import unittest, calc; calc.add = lambda a, b: a - b; import test_calc; r = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(test_calc)); failed = [t.id().rsplit('.', 1)[1] for t, _ in r.failures]; print('FAIL:', failed, 'ERROR:', [t.id() for t, _ in r.errors]); assert not r.errors, 'mutant run raised errors, not assertion failures'; assert any(n.startswith('test_add_') for n in failed), 'subtracting add mutant survived: no test_add_ method failed'"` exits 0 and its printed FAIL: list contains test_add_ names.
    - `python3 -c "import ast, pathlib; trees = [ast.parse(pathlib.Path(f).read_text()) for f in ('calc.py', 'test_calc.py')]; mods = {a.name for t in trees for n in ast.walk(t) if isinstance(n, ast.Import) for a in n.names} | {n.module for t in trees for n in ast.walk(t) if isinstance(n, ast.ImportFrom)}; print(sorted(mods, key=str)); assert mods == {'unittest', 'calc'}, mods"` prints `['calc', 'unittest']` and exits 0.
    - `python3 -c "import calc, pathlib; assert pathlib.Path(calc.__file__).resolve() == pathlib.Path('calc.py').resolve(), calc.__file__"` exits 0.
    - Before the commit, `git status --porcelain -- calc.py test_calc.py` prints exactly ` M calc.py` and ` M test_calc.py`. After the commit, `git show --stat --format= HEAD` lists only `calc.py` and `test_calc.py`.
    - Every `<verify>` gate exits 0, and the exact-restore diff gate runs after the mutant gates.
  </acceptance_criteria>
  <done>calc.sub(a, b) returns a - b again, and TestSub (2 methods) is back. The suite runs 4 tests with a bare OK. An operand-swapped sub mutant makes test_sub_* methods FAIL, and the same harness with a correct sub runs green. calc.py and test_calc.py are byte-identical to f19cd3a. CALC-01 checks still pass. A single commit touching only calc.py and test_calc.py exists.</done>
</task>

</tasks>

## Artifacts this phase produces

This plan restores the following symbols. They existed at f19cd3a, and 60fcca2 deleted them.

- calc.sub(a, b), in calc.py (returns a - b)
- test_calc.TestSub, in test_calc.py
- test_calc.TestSub.test_sub_returns_difference (asserts calc.sub(5, 3) == 2)
- test_calc.TestSub.test_sub_subtracts_second_operand_from_first (asserts calc.sub(3, 5) == -2)

Unchanged and re-checked: calc.add(a, b), test_calc.TestAdd and its two methods. No new files are created.

## Flagged Assumptions (edge-probe fallback rows, status unresolved)

No SPEC exists. The spec-less edge probe returned both phase requirements as `unclassified` / `unresolved`. Per the fallback protocol, they are neither auto-resolved nor turned into truths. They are carried forward from 01-01 unchanged, as flagged assumptions. No-silent-drop count: 2 probe rows = 0 authored truths + 2 flagged assumptions.

**A-01 (CALC-01, `calc.add`): unclassified, unresolved, flagged; carried from 01-01, untouched here.** `calc.add` behaves exactly as Python's `+` for every input. That covers float rounding (`calc.add(0.1, 0.2)` returns `0.30000000000000004`), overflow to `inf`, and `+` semantics for non-numeric operands. None of it is tested. This plan does not change `add`. It only re-checks the two int cases and the add mutant gate for regression.

**A-02 (CALC-02, `calc.sub`): unclassified, unresolved, flagged.** The restored `calc.sub` behaves exactly as Python's `-` for every input. For example, `calc.sub(0.3, 0.1)` returns `0.19999999999999998`, and `calc.sub(1e308, -1e308)` returns `inf`. None of this is tested. Operand order is not part of this assumption: it is the core CALC-02 contract, pinned by `test_sub_subtracts_second_operand_from_first` and proven by the operand-swap mutant gate.

Resolving either row needs the operand domain stated in REQUIREMENTS.md or a SPEC, followed by a re-plan.

## Prohibition Recall Record (spec-less fallback, stage 1 then stage 2)

The adversarial question was asked of CALC-02 in its restore context: "what could this silently become that the author would not want, but nothing forbids?"

| # | Stage-1 candidate | Stage-2 disposition |
|---|---|---|
| 1 | must not mutate its operands | dropped: routine engineering |
| 2 | must not coerce strings to numbers | dropped: routine; operand-domain edge, carried as A-02 |
| 3 | must not round or truncate results | dropped: routine; precision edge, carried as A-02 |
| 4 | must not evaluate operands as code | canon (injection): covered by /gsd-secure-phase; not minted here |
| 5 | must not perform I/O, logging or network calls | dropped: routine purity hygiene; no data with privacy weight |
| 6 | must not depend on third-party packages | not omitted: explicit PROJECT.md constraint, enforced by the import-allowlist acceptance check |
| 7 | tests must not pass whatever the implementation | dropped as a prohibition: routine test quality, enforced as a truth (mutant gate plus control) |
| 8 | the restore must not silently change add or TestAdd | dropped: routine; enforced as a truth (byte-identical to f19cd3a, plus CALC-01 non-regression) |
| 9 | the mutant check must not leave a mutated calc.py on disk or in a commit | dropped: routine; enforced structurally (in-memory rebinding) and by the exact-restore diff gate, which runs last |
| 10 | must not swap the operand order | dropped as a prohibition: it is the CALC-02 contract itself, pinned by a test and the mutant gate |

Stage 2 kept 0 values, safety or ethics prohibitions. That is the expected outcome for a pure two-operand utility with no user-facing or data surface, so `must_haves.prohibitions` is an empty list. No-silent-drop count: 0 kept = 0 authored + 0 flagged.

## Source Audit

| SOURCE | ID | Item | Plan | Status | Notes |
|---|---|---|---|---|---|
| GOAL | — | calc.py provides add and sub, each with a passing unittest in test_calc.py | 01-01 + 01-02 | COVERED | add intact from 01-01; sub restored by 01-02 Task 1 |
| REQ | CALC-01 | calc.add(a, b) returns a + b, with a unittest | 01-01 | COVERED | Satisfied per 01-VERIFICATION.md; 01-02 re-checks it for regression only and does not claim it |
| REQ | CALC-02 | calc.sub(a, b) returns a - b, with a unittest | 01-02 | COVERED | Task 1 |
| GAP | truth 2 | calc.sub(5, 3) == 2 from the repo root | 01-02 | COVERED | verify gate 1 |
| GAP | truth 3 | the -v suite lists test_sub_* and ends in OK | 01-02 | COVERED | verify gate 2 |
| GAP | truth 5 | sub tests FAIL against a b - a mutant | 01-02 | COVERED | verify gates 3 (mutant) and 4 (control) |
| GAP | truth 8 | add and sub take exactly a, b | 01-02 | COVERED | signature acceptance check |
| RESEARCH | — | none (research skipped in --gaps mode) | — | N/A | |
| CONTEXT | — | none (skip_discuss=true; no D-XX decisions) | — | N/A | |

## Reversibility

No decision here rates above `reversible`. The restore is one commit touching two files, and a later revert undoes it. The function name, parameters and semantics come from REQUIREMENTS.md and the f19cd3a content that already passed verification, so the human already settled them. So no `<reversibility>` element and no `checkpoint:decision` are emitted, and the plan stays `autonomous: true`.

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| git object store to working tree | The restore copies file content out of a commit object into calc.py and test_calc.py. The content is only as trustworthy as the ref it is resolved from. |
| in-process caller to calc.sub | Operands come from Python code in the same interpreter with the same privileges. No external input path exists (unchanged from 01-01). |
| test oracle to phase verdict | A green `python3 -m unittest` run is read as evidence that CALC-02 holds. 60fcca2 showed that the suite can be green while the function and its tests are both missing. |

## STRIDE Threat Register

Existing IDs T-01-01 to T-01-03 (01-01) still apply to the restored `sub` and keep their dispositions. The calc.__file__ acceptance check re-runs the T-01-01 mitigation. New rows continue from T-01-04.

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-01-04 | Tampering | restore source ref used by `git restore --source` | low | mitigate | The restore and the exact-restore gate both use the full 40-character SHA f19cd3a2a223005f04782fa3661de82fa0011df7, never an abbreviated ref or a branch or tag name. The precondition halts if either blob is missing. The behavioral gates (success criterion 2, listing, mutant plus control, signature, import allowlist) check the restored content on its own merits, so wrong content fails them rather than riding in on provenance. |
| T-01-05 | Tampering | working tree during the mutant-discrimination check | low | mitigate | The mutant is injected by rebinding `calc.sub` inside one `python3 -c` process. No file is written, copied or deleted, so no mutant can stay on disk or be committed. The exact-restore diff gate runs after the mutant gates and exits 1 if calc.py differs from f19cd3a. Rehearsed at planning time: diff exit 0 after all gates. |
| T-01-06 | Tampering | test oracle: a green suite that claims coverage it lacks | medium | mitigate | The listing gate requires test_sub_ lines, `Ran 4 tests` and a bare OK. The mutant gate requires a FAIL, not an ERROR, on a test_sub_* method, and its control proves the harness is green with a correct sub. Proven at planning time: HEAD and a "sub restored, tests not" variant both pass `python3 -m unittest test_calc` but fail the listing gate and the mutant gate. A symmetric-operand test variant passes the listing gate but fails the mutant gate. |
| T-01-SC | Tampering | package installs (pip) | low | mitigate | There is no package-manager step in this plan. The AST import-allowlist acceptance check fails unless calc.py and test_calc.py import exactly `unittest` and `calc`. |
</threat_model>

<verification>
Run from the repo root after Task 1. Every command must exit 0:

1. `python3 -c "import calc; assert calc.add(2, 3) == 5"` (ROADMAP success criterion 1, verbatim; CALC-01 non-regression)
2. `python3 -c "import calc; assert calc.sub(5, 3) == 2"` (ROADMAP success criterion 2, verbatim)
3. `python3 -m unittest test_calc` (ROADMAP success criterion 3, verbatim), plus the listing gate from Task 1 verify
4. The sub mutant gate and its control, and the add mutant gate (Task 1 verify and acceptance)
5. The signature, import-allowlist and calc.__file__ checks (Task 1 acceptance)
6. `git --no-pager diff --exit-code f19cd3a2a223005f04782fa3661de82fa0011df7 -- calc.py test_calc.py`, run last

What these checks do not establish:
- The mutant gate shows the sub tests catch one specific wrong implementation (operand swap). It does not show they catch every wrong implementation.
- The mutant gate measures the tests, not calc.py. At planning time it passed with TestSub restored and `sub` missing, so it does not replace success criterion 2.
- `python3 -m unittest test_calc` on its own does not close truth 3: it exits 0 at HEAD, where there are no sub tests.
- Float, mixed-type and non-numeric operands remain untested (flagged assumptions A-01 and A-02).
</verification>

<success_criteria>
- `calc.sub(a, b)` returns `a - b` again, and ROADMAP success criteria 1, 2 and 3 all pass verbatim from the repo root (CALC-02 satisfied; CALC-01 not regressed).
- `test_calc.py` has `TestAdd` (2 methods) and `TestSub` (2 methods). The `-v` run lists both test_add_ and test_sub_ lines, `Ran 4 tests`, and a bare `OK`.
- The sub tests FAIL against an operand-swapped mutant, and the identical harness runs green with a correct sub.
- calc.py and test_calc.py are byte-identical to f19cd3a2a223005f04782fa3661de82fa0011df7. The gates left no mutated file behind.
- The 01-VERIFICATION.md gaps for truths 2, 3, 5 and 8 are closed. Flagged assumptions A-01 and A-02 stay recorded as unresolved in the SUMMARY.
</success_criteria>

<output>
Create `.planning/phases/01-calculator/01-02-SUMMARY.md` when done
</output>

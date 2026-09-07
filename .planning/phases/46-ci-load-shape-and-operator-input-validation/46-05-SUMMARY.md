---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 05
subsystem: infra
tags: [git-hooks, pre-commit, awk, shell, zsh, pipestatus, regression-guards, gap-closure]

requires:
  - phase: 46-ci-load-shape-and-operator-input-validation
    provides: "the inline pre-commit bashism guard added by 46-01/02/03, and 46-REVIEWS.md finding C-04 with codex's two proven live-hook bypasses"
provides:
  - "`scripts/lint-plan-bashisms.sh` — a block-parsing plan scanner whose allow rule is CONTAINMENT (a bashism must lie inside the single-quoted region a `bash -c '` opened), not text order"
  - "`crates/devflow-cli/tests/plan_bashism_scanner.rs` — 11 behavioural tests that EXECUTE the scanner; the enforcement layer CI actually runs, since every CI job runs `cargo test` via `scripts/check.sh`"
  - "`crates/devflow-cli/tests/fixtures/plan-bashisms/` — nine fixtures: seven must-fail (including both proven bypasses and both ordering-defeating forms) and two must-pass negative controls"
  - "a `scripts/hooks/pre-commit` that delegates rather than scanning inline, with no substring filter and no disclosed gap"
affects: [46-06, 46-07, 46-08, phase-46 verification, any future plan authored with an <automated> verify block]

actuals:
  tokens: 15410
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "A guard's allow rule is containment in a lexical region, not the presence or order of a substring"
    - "A hook is early feedback; a test suite that executes the same code is the enforcement"
    - "An end-to-end hook probe must ATTRIBUTE the refusal to the guard under test, not infer it from a non-zero `git commit`"
    - "Fail closed on undecidable input and say what to restructure"

key-files:
  created:
    - scripts/lint-plan-bashisms.sh
    - crates/devflow-cli/tests/plan_bashism_scanner.rs
    - crates/devflow-cli/tests/fixtures/plan-bashisms/multiline-bypass-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/comment-bypass-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/sequenced-bypass-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/double-quoted-bypass-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/array-index-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/bash-rematch-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/unterminated-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/legit-PLAN.md
    - crates/devflow-cli/tests/fixtures/plan-bashisms/no-bashism-PLAN.md
  modified:
    - scripts/hooks/pre-commit
    - .planning/user/DEV-SETUP-CHECKLIST.md
    - .planning/phases/46-ci-load-shape-and-operator-input-validation/deferred-items.md

key-decisions:
  - "Containment, not ordering: a bashism is allowed only when its offsets fall strictly inside a single-quoted region opened by `bash -c '`. Single quotes do not nest in POSIX shell, so the region boundary is unambiguous — and this is the only rule that rejects both `bash -c ':'; echo \"${PIPESTATUS[0]}\"` and `bash -c \"echo ${PIPESTATUS[0]}\"`."
  - "No CI corpus scan was added. `origin/develop` tracks ZERO PLAN.md files (measured), so a step scanning tracked plans would inspect nothing and report green on every PR — the exact false-green class C-04 is about. Enforcement is the fixture suite `cargo test` already runs."
  - "The 14 already-flagged blocks in committed plans are grandfathered by the scanner's staged-only scope and were NOT retro-edited; rewriting the executed record to satisfy a guard added afterwards destroys evidence and changes nothing about what ran."
  - "An unterminated opening tag is a hard error, accepting the false positives that follow. Three archived plans mention the tag in prose and would now be refused if re-staged — logged, not fixed. A parser that matches only complete blocks is bypassed by construction."
  - "The scanner is resolved repo-relative with a fallback to the hook's own sibling, because a borrowed `core.hooksPath` (how the plan's own probe works) leaves cwd outside the tree. Neither lookup degrades to a pass."

patterns-established:
  - "Negative controls are fixtures, not assumptions: `legit-PLAN.md` and `no-bashism-PLAN.md` exist because a scanner refusing every input would satisfy all seven must-fail cases"
  - "Assert a guard's message on a SINGLE line carrying both the file name and the offending text — a whole-output `contains` can be satisfied by the guard's own guidance boilerplate"
  - "Prove a fix in the same medium the defect was proven in: C-04 was demonstrated by committing, so it is disproven by committing"

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "Both bypasses codex committed through the live hook are now refused, and the legitimate `bash -c '…'` form still commits"
    requirement: INFRA-01
    verification:
      - kind: e2e
        ref: "scratch-repo probe: NEW hook -> multiline=refused_by_bashism_guard, comment=refused_by_bashism_guard, legit=COMMITTED"
        status: pass
      - kind: e2e
        ref: "negative control, same probe against the hook at HEAD -> multiline=COMMITTED, comment=COMMITTED, sequenced=COMMITTED, doublequoted=COMMITTED, legit=COMMITTED"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/plan_bashism_scanner.rs#multiline_block_is_blocked, #trailing_bash_c_comment_is_blocked, #legitimate_bash_c_wrapped_block_is_accepted"
        status: pass
    human_judgment: false
  - id: D2
    description: "The two ordering-defeating forms the operator named are fixtures and both fail"
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/plan_bashism_scanner.rs#bash_c_that_closed_before_the_bashism_is_blocked, #double_quoted_bash_c_is_blocked"
        status: pass
      - kind: e2e
        ref: "scratch-repo probe: NEW hook -> sequenced=refused_by_bashism_guard, doublequoted=refused_by_bashism_guard"
        status: pass
    human_judgment: false
  - id: D3
    description: "Block parsing, `-z`/`read -r -d ''`, `--diff-filter=ACM`, and the extended `${name[N]}` / `${BASH_REMATCH[N]}` pattern are implemented"
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/plan_bashism_scanner.rs#multiline_block_is_blocked, #generic_array_index_is_blocked, #bash_rematch_is_blocked, #unterminated_block_is_reported_as_malformed"
        status: pass
      - kind: other
        ref: "grep -n 'diff-filter=ACM' and 'read -r -d' scripts/lint-plan-bashisms.sh"
        status: pass
    human_judgment: false
  - id: D4
    description: "A named path that cannot be read is an error, and a zero-file scan prints a visible count of 0"
    requirement: INFRA-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/plan_bashism_scanner.rs#missing_path_is_an_error_naming_the_path, #zero_arguments_exits_zero_and_prints_a_zero_count"
        status: pass
    human_judgment: false
  - id: D5
    description: "The hook delegates, the `grep -v 'bash -c'` substring filter is gone, and the comment disclosing the multi-line gap is deleted"
    requirement: INFRA-01
    verification:
      - kind: other
        ref: "delegation_sites=3; old_substring_filter_sites=0; grep -c 'Scope note' scripts/hooks/pre-commit = 0; hook_syntax_exit=0"
        status: pass
    human_judgment: false
  - id: D6
    description: "DEV-SETUP-CHECKLIST.md moved in the same commit as the hook, recording the containment rule and the probe-attribution lesson"
    requirement: INFRA-01
    verification:
      - kind: other
        ref: "git show --stat 7a7b626 lists both scripts/hooks/pre-commit and .planning/user/DEV-SETUP-CHECKLIST.md"
        status: pass
    human_judgment: false

duration: 20 min
completed: 2026-09-06
status: complete
---

# Phase 46 Plan 05: Close C-04 — the bypassable pre-commit bashism guard Summary

**The plan-bashism guard is now a block-parsing scanner whose allow rule is containment inside a `bash -c '…'` single-quoted region rather than the presence of the string `bash -c`, backed by 11 executing tests and nine fixtures; both bypasses an external reviewer committed through the live hook are refused by a real commit, and the legitimate form still commits.**

## Performance

- **Duration:** 20 min
- **Started:** 2026-09-06T07:05Z (approx; first task commit 07:09:39)
- **Completed:** 2026-09-06T07:25Z
- **Tasks:** 3 of 3
- **Files modified:** 13 changed (629 insertions, 34 deletions)

## Accomplishments

- **C-04 is closed for all four bypass forms, proven by real commits.** The old hook committed
  every one of them; the new one refuses them and still accepts the legitimate wrapper.
- **The allow rule changed shape, not just coverage.** Ordering ("a `bash -c` appears before the
  bashism") is what made two of the four forms undetectable. Containment in the single-quoted
  region a `bash -c '` opened decides all four correctly, and the reasoning for each of the four
  is written into the script.
- **The scan moved out of the hook into a tested script.** `cargo test` runs
  `plan_bashism_scanner.rs`, and every CI job runs `cargo test` through `scripts/check.sh`, so CI
  genuinely executes the scanner — against fixtures. The hook is early feedback.
- **The silent-skip class is closed at both ends.** `-z` with `read -r -d ''` survives a path
  containing non-ASCII or a tab; `--diff-filter=ACM` keeps a deleted plan out; a named path that
  cannot be read is an error rather than a skip.

## Task Commits

1. **Task 1: fixture corpus + failing suite** — `d6198bc` (test)
2. **Task 2: the block-parsing scanner** — `215fea9` (feat)
3. **Task 3: hook delegation + checklist** — `7a7b626` (fix)

## The evidence

### The scratch-repo probe — required verbatim by the plan

Real `git commit` calls in a temp repo whose `core.hooksPath` points at this worktree's
`scripts/hooks`, on branch `feature/scratch`, under `docs/` so neither the protected-branch nor
the personal-artifact arm fires:

```
multiline=refused_by_bashism_guard
comment=refused_by_bashism_guard
legit=COMMITTED
```

with the refusals attributed by the guard's own output:

```
lint-plan-bashisms: refusing a plan whose <automated> verify command uses bash-only
    docs/a-PLAN.md:9: ${PIPESTATUS[0]}
lint-plan-bashisms: refusing a plan whose <automated> verify command uses bash-only
    docs/b-PLAN.md:7: ${PIPESTATUS[0]}
```

**The negative control — the same probe against the hook at `HEAD` (the version C-04 bypassed):**

```
OLD_HOOK:multiline=COMMITTED
OLD_HOOK:comment=COMMITTED
OLD_HOOK:sequenced=COMMITTED
OLD_HOOK:doublequoted=COMMITTED
OLD_HOOK:legit=COMMITTED
```

Without that control the three lines above are also consistent with a probe that cannot detect
anything. The old hook lets all four through; the new one refuses four and accepts the fifth.

**The remaining fixtures, also end-to-end through a real commit under the new hook:**

```
NEW_HOOK:sequenced=refused_by_bashism_guard
NEW_HOOK:doublequoted=refused_by_bashism_guard
NEW_HOOK:arrayindex=refused_by_bashism_guard
NEW_HOOK:rematch=refused_by_bashism_guard
NEW_HOOK:unterminated=refused_by_bashism_guard
NEW_HOOK:nobashism=COMMITTED
```

### Per-fixture verdicts, scanner invoked directly

```
multiline-bypass=blocked_exit_1
comment-bypass=blocked_exit_1
sequenced-bypass=blocked_exit_1
double-quoted-bypass=blocked_exit_1
array-index=blocked_exit_1
bash-rematch=blocked_exit_1
unterminated=blocked_exit_1
legit=accepted_exit_0
no-bashism=accepted_exit_0
```

Both directions, because seven blocks alone is equally consistent with a scanner that refuses
everything, and two accepts alone with one that accepts everything.

### Test suite

RED (Task 1, before the scanner existed):

```
test result: FAILED. 0 passed; 11 failed; 0 ignored; 0 measured; 0 filtered out
```

with all 11 failures naming `scripts/lint-plan-bashisms.sh: No such file or directory` — the red
attributable to the absent implementation and nothing else.

GREEN (Task 2 onward):

```
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo test -p devflow --test plan_bashism_scanner`, non-zero `filtered out` is not applicable
here because the whole binary is the target rather than an `--exact` name filter; the `11 passed`
count matches the 11 `#[test]` functions in the file.

Gates: `bash -n` and `shellcheck` clean on `scripts/lint-plan-bashisms.sh` (`syntax_exit=0`,
`shellcheck_exit=0`); `bash -n` clean on the hook; `cargo fmt --all --check` `fmt_exit=0`;
`cargo clippy --workspace --all-targets -- -D warnings` `clippy_exit=0`.

## The two measured facts that shaped the design

### `origin/develop` tracks ZERO PLAN.md files — so no CI corpus scan was added

```
develop_tracked_plans=0
branch_tracked_plans=223
```

`.planning/` is gitignored on `develop` and `scripts/cut-pr-branch.sh` strips it from every PR
branch. A CI step that scanned tracked plans would therefore inspect **nothing** and report green
on every PR that reaches `develop` — coverage-shaped output over an empty input set, which is the
same false-green class as the bypasses themselves. Enforcement is the fixture suite instead, which
`cargo test` runs and which every CI job reaches through `scripts/check.sh`.

### 14 already-committed blocks would be flagged and were deliberately left alone

Measured this session by running the finished scanner over all 214 tracked `*PLAN.md` files
(223 total minus the 9 new fixtures), in a single invocation:

```
lint-plan-bashisms: scanned 214 file(s)
bashism_hits=14
```

| File | Blocks flagged |
|---|---|
| `.planning/phases/46-…/46-01-PLAN.md` | 3 |
| `.planning/phases/46-…/46-02-PLAN.md` | 4 |
| `.planning/phases/46-…/46-03-PLAN.md` | 5 |
| `.planning/milestones/v2.4.0-phases/34-…/34-06-PLAN.md` | 2 |

12 in this phase's own first three plans, 2 in an archived v2.4.0 plan — exactly the baseline the
plan recorded. They are grandfathered by the scanner's staged-only scope: they trip only if
re-staged. Retro-fixing them would rewrite the executed record of this phase and would change
nothing about what actually ran.

`46-04`, `46-05`, `46-06`, `46-07` and `46-08` are all clean under the new rule.

## Decisions Made

See `key-decisions` in the frontmatter. The load-bearing one: **containment rather than ordering.**
The operator named two forms an ordering rule accepts —

- `bash -c ':'; echo "${PIPESTATUS[0]}"` — a real `bash -c` precedes the bashism, but its region
  closed at the second quote and the bashism runs in the outer shell.
- `bash -c "echo ${PIPESTATUS[0]}"` — double quotes open no single-quoted region at all, so the
  outer shell expands before bash is ever started.

Both are now fixtures and both fail. Single quotes do not nest in POSIX shell, which is what makes
the region boundary computable at all.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The suite's own `assert_blocked` was a proxy measurement**
- **Found during:** Task 2, before running GREEN
- **Issue:** `assert_blocked` checked `text.contains(expected_text)` over the *whole* scanner
  output. The scanner's guidance boilerplate itself prints `${PIPESTATUS[0]}` as the recommended
  form, so four of the seven must-fail fixtures would have passed on the boilerplate alone,
  without the scanner having reported anything about the file under test.
- **Fix:** the assertion now requires a **single line** carrying both the fixture's file name and
  the offending text. The fixture name never appears in the boilerplate, so it discriminates.
  `unterminated-PLAN.md`'s expected text was tightened from `"unterminated"` (which the *filename*
  satisfies) to `"unterminated <automated> block"`.
- **Files modified:** `crates/devflow-cli/tests/plan_bashism_scanner.rs`
- **Verification:** 11/11 still green after the tightening; the two array/rematch cases, which were
  never satisfiable by boilerplate, are unaffected.
- **Committed in:** `215fea9`

**2. [Rule 3 - Blocker] `scripts/lint-plan-bashisms.sh` staged at mode `100644`**
- **Found during:** Task 2, at `git add`
- **Issue:** `core.fileMode` is `false` in this checkout, so git recorded the new script
  non-executable despite the working-tree bit. The Rust suite spawns it directly and the hook
  requires `-x`, so a fresh clone would have failed at both — the exact trap
  `DEV-SETUP-CHECKLIST.md` §3 already warns about.
- **Fix:** `git update-index --chmod=+x scripts/lint-plan-bashisms.sh`; index now reads `100755`.
- **Verification:** `git ls-files -s scripts/lint-plan-bashisms.sh` → `100755 …`. Negative control
  on the mechanism: a mode-644 copy exits `126 Permission denied`, a mode-755 copy exits `0`.
- **Committed in:** `215fea9`

**3. [Rule 3 - Blocker] The plan's scanner-resolution rule made its own probe unsatisfiable**
- **Found during:** Task 3
- **Issue:** the plan says to resolve the scanner repo-relative (cwd is the top of the working
  tree), *and* to prove the fix in a scratch repo whose `core.hooksPath` points at **this** repo's
  hooks. Those are incompatible: in the scratch repo, cwd is the scratch tree, where
  `scripts/lint-plan-bashisms.sh` does not exist, so the hook would have refused all three probe
  commits with "missing scanner" — including the negative control.
- **Fix:** repo-relative lookup first (the shipped configuration, exactly as specified), falling
  back to `$(dirname "$0")/../lint-plan-bashisms.sh`. Both are `-x` checks; neither degrades to a
  pass, and a missing scanner still exits non-zero naming the path.
- **Files modified:** `scripts/hooks/pre-commit`
- **Verification:** the probe now discriminates — new hook refuses two and commits the third; old
  hook commits all five.
- **Committed in:** `7a7b626`

**4. [Rule 2 - Missing critical] The malformed-block refusal had no actionable guidance**
- **Found during:** Task 3, from the corpus scan
- **Issue:** three archived plans mention `<automated>` in prose, producing an "unterminated block"
  refusal whose accompanying guidance talked only about wrapping commands in `bash -c` — advice
  that does not apply and would leave an author stuck.
- **Fix:** the guidance now names the malformed case, says why an unclosed tag is refused
  deliberately, and tells the author to close the block or write the literal indirectly.
- **Files modified:** `scripts/lint-plan-bashisms.sh`
- **Verification:** `shellcheck` and `bash -n` clean; suite still 11/11.
- **Committed in:** `215fea9` (guidance) and exercised in `7a7b626`

---

**Total deviations:** 4 auto-fixed (1× Rule 1, 2× Rule 3, 1× Rule 2).
**Impact on plan:** No scope creep. Deviation 1 is the most consequential — without it the suite
would have reported green while measuring the scanner's own boilerplate for four of its seven
must-fail cases, which is precisely the failure class this plan exists to close.

## Issues Encountered

**The first probe run misattributed its own result, and the misattribution was invisible.**
The initial run printed `legit=WRONGLY_REFUSED`. The cause was the **commit-msg** hook rejecting
the message `t3` for not being a Conventional Commit — nothing to do with the guard under test.
The plan had warned about the protected-branch and personal-artifact arms; `commit-msg` was the
one it did not name.

Worse: `multiline=refused` and `comment=refused` from that same run were **also** unattributed —
they would have read as success while proving only that *some* hook objected to *something*. The
probe was rewritten to use conforming commit messages and to grep the captured output for
`lint-plan-bashisms` specifically before calling a refusal a refusal. This is recorded as a
`[PATTERN]` entry in `DEV-SETUP-CHECKLIST.md`.

**Two out-of-scope discoveries were logged, not fixed** — see
`.planning/phases/46-…/deferred-items.md`:

1. **`scripts/assert-cpu-pin.sh` (added by 46-04) is tracked mode `100644` and will fail in CI.**
   It is invoked directly at `.github/workflows/ci.yml:165` and from
   `ci_parity_guards.rs:874`, whose own doc comment says it is spawned directly "so a lost exec bit
   fails here instead of in CI". Measured with a negative control: mode 644 → exit `126 Permission
   denied`; mode 755 → exit `0`. It passes locally only because `core.fileMode` is `false`. Fix is
   one `git update-index --chmod=+x`, but 46-05's plan explicitly scopes 46-04's files out.
2. **Three archived plans carry a bare `<automated>` tag in prose** and would be refused if
   re-staged. Conservative-by-design; grandfathered by the staged-only scope.

## Known Stubs

None.

## Scope note on the new fixtures

The nine fixtures are named `*-PLAN.md` and therefore match the scanner's own file filter. Seven
of them are refused by the scanner **by construction** — that is what they are for. They committed
cleanly here because the pre-commit hook in force at the time was the old one, and because the
scanner is staged-only. **Editing one of those seven in a future commit will be refused**; unstage
it and commit with `--no-verify`, or accept the refusal as the guard doing its job. A carve-out for
the fixture directory was considered and rejected: "put the plan under tests/fixtures" would be a
new bypass, and the plan's standing instruction is that a conservative false positive costs one
edit while a false negative is the whole finding.

## Caveat on how this worktree's own commits were hooked

`core.hooksPath` in this repository is an **absolute** path to the main checkout
(`/var/home/denniyahh/Github/devflow/scripts/hooks`), which is shared across worktrees. The three
task commits in this worktree therefore ran the **main checkout's** hook, not the one edited here.
That does not weaken the evidence — the scratch-repo probe pointed `core.hooksPath` explicitly at
this worktree's `scripts/hooks`, so it exercised the new hook — but it does mean the new hook has
**not** been exercised by an ordinary commit on this branch. It will be, once this branch's
`scripts/hooks/pre-commit` reaches the main checkout.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

46-05 is complete and self-contained; it touches no file 46-06/46-07 own
(`crates/devflow-cli/src/**`) and none of the four files 46-04 owns. Ready for 46-06.

One item wants a decision before the phase closes: **the `scripts/assert-cpu-pin.sh` exec-bit
defect will fail CI on this branch's PR.** It belongs to 46-04's file set, so it was logged rather
than fixed. Either 46-06 picks it up as a one-line index change, or it needs an explicit
out-of-band fix before the phase's PR is cut.

---
*Phase: 46-ci-load-shape-and-operator-input-validation*
*Completed: 2026-09-06*

## Self-Check: PASSED

All files listed under `key-files.created` exist on disk (9 fixtures + scanner + test suite +
this SUMMARY). All three task commits resolve: `d6198bc`, `215fea9`, `7a7b626`.

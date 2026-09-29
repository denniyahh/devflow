---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 02
subsystem: testing
tags: [bash, git, fixture, scaffold, selftest, jq]

requires: []
provides:
  - "`scripts/scratch-dogfood-repo.sh --live-payload <dest>`: main + integration topology, devflow.toml on integration only, serde-pretty GSD config, integration checked out"
  - "`--check <dest>`: eight named read-only checks, `CHECK passed=<P> failed=<F>` last"
  - "`--selftest`: ten cases with negative controls, `SELFTEST passed=<P> failed=<F>` last"
  - "Destination guard widened to the main checkout (T-49-02-01)"
affects: [49-06, 49-09, 49-10, negctl]

actuals:
  tokens: 6300
  tasks: 2
  commits: 4
plan_head_before: f97ccdfcfcaad22d2dfd7218b21663ea8b068857

tech-stack:
  added: []
  patterns:
    - "Case line `  ok   <name>` / `  FAIL <name> — <why>`, last line `SELFTEST|CHECK passed=<P> failed=<F>`, exit 0 only if F=0 and P>0"
    - "Controls prove their own precondition first; refusals asserted on stderr text"
    - "JSON key checks via `getpath` into an exact array, so an absent key (null) never reads as false"

key-files:
  created: []
  modified:
    - scripts/scratch-dogfood-repo.sh

key-decisions:
  - "The compact-config control also requires the pretty-form line to be the ONLY FAIL, so it isolates the form check"
  - "--help prints the header's Usage block by pattern (`/^# Usage:/,/^#$/`), not by line numbers, so header edits cannot drift it"
  - "Modes are mutually exclusive and --check/--selftest refuse a positional destination"

patterns-established:
  - "Probe paths inside a checkout are removed only after proving they are the exact probe path the case created"

requirements-completed: [VERIFY-01]

coverage:
  - id: D1
    description: "--live-payload builds main + integration, devflow.toml committed on integration only, integration left checked out, plan-exact bytes"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "scripts/scratch-dogfood-repo.sh --selftest#live payload: integration checked out with devflow.toml"
        status: pass
      - kind: other
        ref: "cmp of generated config.json, devflow.toml, ROADMAP.md, .gitignore against the plan's <context> blocks (all equal)"
        status: pass
    human_judgment: false
  - id: D2
    description: "--check verifies the topology read-only and refuses a compact config.json and HEAD on main for the named reason"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "scripts/scratch-dogfood-repo.sh --selftest#live payload: check passes / control: check refuses a compact config.json / control: check refuses HEAD on main"
        status: pass
      - kind: other
        ref: "ad-hoc mutation sweep: each of the eight check lines failed alone on its own mutation"
        status: pass
    human_judgment: false
  - id: D3
    description: "Default mode byte-identical: develop^{tree} = main^{tree} = f6120b33…, develop checked out, only main and develop"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "scripts/scratch-dogfood-repo.sh --selftest#default mode: tree hash unchanged / default mode: develop checked out with only main and develop"
        status: pass
      - kind: other
        ref: "Task 2 plan gate independent measurement: develop_tree=f6120b337cb9b37d658c6111b7ac14694338dc56"
        status: pass
    human_judgment: false
  - id: D4
    description: "Destination guard refuses this worktree, the main checkout, an existing destination; unknown options refused"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "scripts/scratch-dogfood-repo.sh --selftest#control: refuses a destination inside the main checkout (and the three sibling controls)"
        status: pass
    human_judgment: false

duration: 9min
completed: 2026-09-29
status: complete
---

# Phase 49 Plan 02: Live-payload scaffold and checker Summary

**`scratch-dogfood-repo.sh` gains `--live-payload` (main + integration, devflow.toml on integration, serde-pretty GSD config), a read-only eight-line `--check`, a ten-case `--selftest` with negative controls, and a destination guard widened to the main checkout; the default fixture's tree hash is unchanged.**

## Performance

- **Duration:** 9 min
- **Started:** 2026-09-29T19:30:52Z
- **Completed:** 2026-09-29T19:39:33Z
- **Tasks:** 2 (both TDD: RED then GREEN)
- **Files modified:** 1 (`scripts/scratch-dogfood-repo.sh`), plus `.planning/WINDOWS.md` (ledger entry)

## What the evidence does NOT establish (read first)

- **`--check` passing does not mean DevFlow or GSD will accept the run.** It proves layout and config values only; preflight, GSD config resolution and the pipeline are exercised only by a real launch (stated in the script header).
- **The serde round trip was proven by a scratch probe, not by DevFlow's own function.** No CLI verb exposes `set_auto_chain_active`. Out of repo, I ran the same round trip with the workspace's pinned `serde_json 1.0.150` + `preserve_order` (confirmed enabled via `cargo tree`): set flag true, then false, re-render pretty with trailing newline. Result `round_trip_identical=true` on the generated config; negative control (the same config compacted by `jq -c`) gave `round_trip_identical=false`. This mirrors `gsd_config.rs:133-178`; it is not a call into it. The in-repo `--check` uses `jq --indent 2` equality, which is the plan's chosen proxy.
- **Task 1's two controls first failed on precondition, not on check logic.** In the RED run the stubbed `--live-payload` produced nothing, so both controls reported "precondition: no live payload to copy". The GSD RED-evidence checker accepted it (`RED_EVIDENCE_OK`) because the named case failed; it cannot tell a precondition FAIL from an assertion FAIL. The check logic's discrimination is instead shown by the post-GREEN mutation sweep (below).
- **The main-checkout control only discriminates from a linked worktree.** Run from the main checkout, `MAIN_CHECKOUT == REPO_ROOT` and the case degenerates to the this-checkout case. All runs here were from `.worktrees/phase-49`.
- **Selftest hermeticity vs. CI (unverified).** The selftest nulls global git config, then its children run `git rev-parse` against the real checkout. On a root-owned CI runner that needs a global `safe.directory` entry, this would likely fail. The selftest is not wired into CI; not tested there.
- **The regression direction of the main-checkout control writes into the main checkout.** If the guard ever regresses, the case scaffolds a throwaway repository at `<main checkout>/.p49-guard-probe-<pid>`, then removes exactly that path. This happened once, intentionally, in the Task 2 RED run. Afterwards `ls -a` showed no leftover and `git status` in the main checkout showed nothing named `p49`.

## Accomplishments

- `--live-payload <dest>`: `.gitignore`, `.planning/config.json`, `.planning/ROADMAP.md` committed on `main` (`chore: scaffold devflow live-run payload`); `integration` created from it with `devflow.toml` (`chore: configure devflow base branch`) and left checked out; repo-local `commit.gpgsign false` / `tag.gpgsign false`; no tags. All four generated files are byte-equal to the plan's `<context>` blocks (`cmp`).
- `--check <dest>`: `head is integration`, `working tree clean`, `main and integration exist`, `devflow.toml sets base_branch integration`, `devflow.toml sets capture_retention 64`, `config.json is in pretty form`, `config.json keys resolve as required`, `roadmap has the phase 1 heading`. `git --no-optional-locks` keeps it read-only. A nonexistent destination gives `CHECK passed=0 failed=8`, exit 1.
- `--selftest`: 10 cases, `SELFTEST passed=10 failed=0`.
- Guard: refuses destinations equal to or under the worktree root or the main checkout (`/home` symlink spelling also refused, since `realpath -m` resolves it to `/var/home`), and refuses an existing destination.

## Task Commits

1. **Task 1 RED** — `26f0a5e` test(49-02): add failing live-payload and check selftest
2. **Task 1 GREEN** — `7c50843` feat(49-02): scaffold the live-run integration topology with a checker
3. **Task 2 RED** — `b29ab0a` test(49-02): pin the default tree and the widened destination guard
4. **Task 2 GREEN** — `16287c2` fix(49-02): refuse scaffold destinations inside the main checkout

## Gate Results

**Task 1 gate (verbatim from PLAN):** `selftest_exit=0 passed=4 failed=0`; all four `case[...]=1`; `live_scaffold_exit=0 head=refs/heads/integration`; `check_exit=0 CHECK passed=8 failed=0`; `gate_rc=0`.

**Task 2 gate (verbatim from PLAN):** `selftest_exit=0 passed=10 failed=0`; all six `case[...]=1`; `default_exit=0 develop_tree=f6120b337cb9b37d658c6111b7ac14694338dc56`; `gate_rc=0`.

**Tracer feedback gate (Task 1):** run was interactive (`_auto_chain_active`/`auto_advance` false), `human_verify_mode` end-of-phase, `<verify>` automated-only → re-ran the gate, `gate_rc=0`, continued to Task 2.

**Negative controls run in addition to the plan:**
- Mutation sweep on `--check`: dirty file, renamed `main`, `base_branch = "develop"`, `capture_retention = 12`, `auto_mode: true` (pretty preserved), `_auto_chain_active` deleted, and a two-hash roadmap heading. Each gave exit 1 with exactly its own line failing (`CHECK passed=7 failed=1`).
- Mutation on the committed script, then restored with `git checkout --`: one extra byte in the default `config.json` heredoc turned `default mode: tree hash unchanged` FAIL (`eeafd8e6…`). Touching `devflow.toml` on `main` turned the topology case FAIL ("committed on main"). Each run was `passed=9 failed=1`.
- RED-evidence checker fed the GREEN output: `INVALID_RED unexpected_green`. The checker therefore does discriminate.

## TDD Gate Compliance

- Task 1: RED `26f0a5e` precedes GREEN `7c50843`. RED run (`SELFTEST passed=0 failed=4`, exit 1) quoted line:
  `  FAIL control: check refuses a compact config.json — precondition: no live payload at /tmp/tmp.m4ex2qAd3d/lp to copy`
  Target case `live payload: integration checked out with devflow.toml` failed on its assertion (`HEAD is 'unreadable', want refs/heads/integration`).
- Task 2: RED `b29ab0a` precedes GREEN `16287c2`. RED run (`SELFTEST passed=9 failed=1`, exit 1) quoted line:
  `  FAIL control: refuses a destination inside the main checkout — the scaffold was not refused (exit 0) and created /var/home/denniyahh/Github/devflow/.p49-guard-probe-1439030; removed it`
- `gsd-tools check tdd-red-evidence` returned `RED_EVIDENCE_OK` for both RED runs. The checker parses TAP only, so the selftest lines were translated mechanically: `ok` → `ok N`, `FAIL` → `not ok N`, plus a `# tests/pass/fail` footer. No line was added or dropped.
- Task 2's GREEN is typed `fix(49-02)` as the plan specifies, not `feat`. tdd.md's `^feat\(49-02\)` grep matches only Task 1's GREEN. This is plan-directed, not a missing gate.
- REFACTOR: none (no separate cleanup commit).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Mode conflicts and stray positionals refused**
- **Found during:** Task 1
- **Issue:** The plan's arg loop does not say what `--live-payload --check x` or `--selftest extra` should do. Silently ignoring one of them would scaffold or check the wrong thing.
- **Fix:** `set_mode` dies with `conflicting modes: --A and --B`. `--check`/`--selftest` die on a positional destination. `--check` with no path dies with `--check needs a path`.
- **Verification:** all four refusals observed with exit 1.
- **Committed in:** `7c50843` (the loop itself is in `26f0a5e`)

**2. [Rule 2 - Correctness] The compact-config control also asserts the pretty-form line is the only FAIL**
- **Found during:** Task 1
- **Issue:** A FAIL line alone would still pass if another check failed as well. For example, a key check that mis-parsed compact JSON would go unnoticed.
- **Fix:** Added the requirement that `  FAIL ` appears exactly once.
- **Committed in:** `26f0a5e`

**3. [Rule 3 - Blocking] `--gsd-tools` is parsed but unused (shellcheck SC2034)**
- **Found during:** Task 1
- **Issue:** The plan requires the flag now and consumes it in 49-06. shellcheck fails the gate on an unused variable.
- **Fix:** Scoped `# shellcheck disable=SC2034` on both assignments, with the reason. Recorded as WINDOWS ledger entry 17 (kind `stub`, open) so it stays visible until 49-06 uses it.
- **Committed in:** `26f0a5e`; ledger entry in the SUMMARY commit

**4. Commit subject amended once (my own, unpushed)**
- The first GREEN commit (`e395235`) dropped the word "a" from the plan's subject. I amended it to the exact plan wording, and it became `7c50843`. The content did not change (`git diff --staged` was empty before the amend).

---

**Total deviations:** 3 auto-fixed (2 Rule 2, 1 Rule 3) plus 1 commit-message correction.
**Impact on plan:** No scope creep. All are argument hardening or control strength. The default-mode output is unchanged.

## Issues Encountered

- `gsd-tools windows append --description "--gsd-tools …"` treated the description as a flag (`Unknown flag`). I reworded it to not start with `--`.

## Known Stubs

| File | Line | Stub | Reason / resolver |
|------|------|------|-------------------|
| scripts/scratch-dogfood-repo.sh | 79 | `GSD_TOOLS` parsed, never read | Plan-specified forward hook; 49-06 adds the GSD-resolution probes that consume it. WINDOWS #17. |

## User Setup Required

None.

## Next Phase Readiness

- 49-06 can add the payload phase directory, PROJECT/REQUIREMENTS/STATE, the RED test and the GSD-resolution probes on top of `--live-payload`, and extend `--check` and `--selftest`.
- The default mode is pinned for negctl's fixture (D-14/D-25).

---
*Phase: 49-live-unattended-run-the-milestone-s-instrument*
*Completed: 2026-09-29*

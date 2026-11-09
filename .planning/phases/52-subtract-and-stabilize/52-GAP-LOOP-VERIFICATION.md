# Phase 52: Gap-Loop Verification (52-CONTEXT D-19)

**Purpose:** show what GSD actually does before any plan builds D-08..D-11.
**GSD version:** @opengsd/gsd-core 1.20.1 (`gsd-tools.cjs runtime-identity --raw`)
**Status:** check 1 done; checks 2-4 not run (blocked, see below).

## Check 1: `query verification status` per report status (DONE, 2026-11-08)

**Setup:** a throwaway git repo with phase `01-demo`. `01-VERIFICATION.md` carries
`covered_files: ["src/a.txt"]` and a `covered_digest` from `query verification fingerprint <phase_dir> src/a.txt`.
The status is read before and after a commit that changes `src/a.txt`. The script is reproduced verbatim in
https://github.com/open-gsd/gsd-core/issues/6078 and was run unchanged:

```
[passed, covered file unchanged] passed |
[passed, covered file changed]   stale | execute-phase
[gaps_found, covered file unchanged] gaps_found | plan-phase
[gaps_found, covered file changed]   gaps_found | plan-phase
```

**Established:**
- D-08's `passed` → `stale` route holds: a covered-file change flips a `passed` report to `stale`, routed to
  `execute-phase`. The unchanged `passed` row is the control showing the digest check discriminates.
- D-11's premise holds: a `gaps_found` report stays `gaps_found` after the same change (filed upstream as #6078).

**Not established:** anything about what the routed commands then do. Check 1 reads a status; it runs no agent.
Only `covered_files` + `covered_digest` (v4) reports were tested, not legacy reports without a fingerprint (the
mtime path).

## Checks 2-4: the agent-driven loop (NOT RUN)

2. `/gsd-plan-phase N --gaps` writes `gap_closure` plans from a `gaps_found` report.
3. `/gsd-execute-phase N --gaps-only --auto` executes them, and its verifier re-verifies to `passed`.
4. `/gsd-execute-phase N --auto` on a stale `passed` report with every plan summarized re-runs the verifier.

**Planned shape:** a throwaway GSD project (`calc.py`; CALC-01 `add`, CALC-02 `sub`; research, plan-check, code-review
and nyquist off, verifier on), run by headless `claude -p "<DevFlow's stage command>"`:
1. plan + execute (expect `passed`);
2. delete `sub` and commit (expect `stale`, check 1);
3. execute-phase (check 4: expect the verifier to re-run and write `gaps_found`);
4. plan-phase --gaps (check 2);
5. execute-phase --gaps-only (check 3: expect `passed`).

**Blocked:** the session's auto-mode permission classifier denied launching `claude -p ... --dangerously-skip-permissions`
("Create Unsafe Agents"). The operator decides how these checks run.

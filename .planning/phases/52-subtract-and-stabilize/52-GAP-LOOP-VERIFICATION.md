# Phase 52: Gap-Loop Verification (52-CONTEXT D-19)

**Purpose:** show what GSD actually does before any plan builds D-08..D-11.
**GSD version:** @opengsd/gsd-core 1.20.1 (`gsd-tools.cjs runtime-identity --raw`)
**Status:** all four checks done (2026-11-08). None contradicts a decision, so D-08..D-11 may be planned. The limits
below still bind the plans.
**Evidence:** `52-gap-loop-evidence/`: each launch's `.meta` (prompt, rc, git log before and after) and `.result.md`
(the agent's final message, cost, turns), the throwaway repo's final `git log`, the gap-closure plan, and the final
VERIFICATION.md.

## Check 1: `query verification status` per report status (no agent)

**Setup:** a throwaway repo with phase `01-demo` and a v4 `covered_digest` from
`query verification fingerprint <phase_dir> src/a.txt`. The status is read before and after a commit that changes
`src/a.txt`. The script is reproduced verbatim in https://github.com/open-gsd/gsd-core/issues/6078 and was run
unchanged:

```
[passed, covered file unchanged] passed |
[passed, covered file changed]   stale | execute-phase
[gaps_found, covered file unchanged] gaps_found | plan-phase
[gaps_found, covered file changed]   gaps_found | plan-phase
```

This holds D-08's `passed` → `stale` route and D-11's premise (`gaps_found` is never reported stale; #6078). Check 2
below shows the same `stale` row again, on a real verifier-written report.

## Checks 2-4: the agent loop on a throwaway GSD project

**Project:** `calc.py`, with CALC-01 `add` and CALC-02 `sub`, on branch `feature/phase-01`. Research, plan-check,
code-review and nyquist were off; the verifier and `security_enforcement` (the default) were on. Each launch was
`claude -p "<prompt>" --dangerously-skip-permissions --output-format json`, the prompt being DevFlow's stage command:

| # | Launch | Prompt | Result | Cost |
|---|---|---|---|---|
| a | Plan | `/gsd-plan-phase 1` | `01-01-PLAN.md` committed | $5.29 |
| b | Code (1st) | `/gsd-execute-phase 1 --auto` | **halted, nothing built**: GSD refuses commits on the default branch `main` (setup error; DevFlow always runs on `feature/phase-NN`) | $2.09 |
| b2 | Code | same, on `feature/phase-01` | built; verifier **`passed` 8/8**; v4 digest covering `calc.py`, `test_calc.py` | $2.52 |
| — | plant | commit `60fcca2` deletes `sub` and `TestSub` (unittest still passes, 2 tests) | `verification status` → **`stale` → execute-phase**, naming `calc.py`, `test_calc.py` | — |
| c | **check 4** | `/gsd-execute-phase 1 --auto` (every plan summarized) | verifier **re-ran**: report rewritten (sha256 changed), **`gaps_found` 4/8**, `previous_status: passed`; status → `gaps_found` → plan-phase | $1.64 |
| d | **check 2** | `/gsd-plan-phase 1 --gaps` | `01-02-PLAN.md` committed with **`gap_closure: true`**, wave 2, depends on 01-01 | $4.83 |
| e | **check 3** | `/gsd-execute-phase 1 --gaps-only --auto` | gap plan executed (`2e4a35b` restores `sub`); verifier **`passed` 9/9**, `previous_status: gaps_found`; status `passed` | $4.28 |

Total: **$20.66**, about 50 minutes of agent time.

**Established:**
- **D-08 stale route:** `execute-phase` on a stale `passed` report re-runs the verifier even when every plan already
  has a SUMMARY, and the verifier catches a regression that the test suite hides.
- **D-09 stage-level loop:** Plan `--gaps` → Code `--gaps-only` → `passed` works end to end through DevFlow's own
  command strings, and each step commits.
- **D-08 verdict source:** the status GSD reports tracked the true state at every step: `passed` → `stale` →
  `gaps_found` → `passed`.

## What this does not establish

- **One run of each step.** n=1, on a two-function project with a regression that is deliberately easy to verify. It
  shows the route works, not how reliably it works or how it behaves on a large phase.
- **Not DevFlow's full prompts or config.** The prompts were the bare GSD commands, without DevFlow's
  `DEVFLOW_RESULT` contract or the code policy text. The throwaway config had plan-check, code-review and nyquist off;
  DevFlow's own config has them on, so the execute tail here is shorter than on DevFlow.
- **Validate stage not run.** `/gsd-validate-phase` (DevFlow's Validate) never ran. Re-verification happened inside
  execute-phase, which is what D-08/D-09 rely on, but the Validate stage's verdict pairing (agent AND status) is
  untested.
- **Clean reject not run.** D-10's `/gsd-quick` route was not exercised. That a quick fix then makes the report
  `stale` is inferred from check 1 (any change to a covered file does), not observed.
- **Stale-gaps gate not run live.** D-11's case was only checked through the status read (check 1).
- **Claude only.** No codex or other agent ran the loop.

## Side observations (not defects in DevFlow)

- GSD refuses to commit on the default branch unless `git.allow_default_branch_commits` is set (launch b). DevFlow's
  phase branch avoids this; a run on `--no-worktree` from `main` would hit it.
- After launch c, GSD's revert step reset **every** requirement in the phase to "Gaps Found", including the satisfied
  CALC-01. It recovers when the phase passes again.
- `execute-phase --auto` also ran a security-verification step and the phase/milestone transition commits, both
  times the phase passed.
- On this hand-written ROADMAP, `phase.complete` reported `roadmap_updated: true` while `roadmap.analyze` still said
  `roadmap_complete: false` (no phase checkbox). This is probably specific to the hand-written roadmap; not
  investigated.

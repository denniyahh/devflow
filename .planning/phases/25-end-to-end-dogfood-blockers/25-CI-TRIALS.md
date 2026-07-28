---
phase: 25-end-to-end-dogfood-blockers
unit: 25e
backlog: 999.47 / DEN-72
observed: 2026-07-28T14:59:06Z
tested_head_sha: 82328b31eb5cbb8d795bc86f048b2602904dc8f4
evidence_commit_sha: pending
run_id: 30371091367
local_gate_runs: 6
ci_trials: 1
image: mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm
hooks_path: /var/home/denniyahh/Github/devflow/scripts/hooks
status: no_reproduction
---

# 25e / 999.47 — CI trials (closure evidence for truth 7)

**In progress.** Six local push-gate observations complete (five standalone + the `pre-push`
hook's own sixth run), the push crossed the real gate and landed, and trial 1 of 5 CI `Test`-job
trials is recorded below. Trials 2-5 land in Task 2.

## A note on `hooks_path`

`git config --get core.hooksPath` prints the **absolute** path
`/var/home/denniyahh/Github/devflow/scripts/hooks`, not the literal relative string
`scripts/hooks`. This is the same directory and the same hook a relative-string check would
name — `scripts/hooks` resolved from this repository's root — so the gate's substance (the
`pre-push` hook that ran `scripts/check-in-container.sh all` below is genuinely installed and
genuinely ran) is satisfied. It is recorded here as the value actually measured, not adjusted
to match a literal-string comparison, and git config was not edited to make a literal check
pass.

## Threshold and its justification

**Threshold: 11 observations at one settled tree — `tested_head_sha` — 6 of the push-gate
shape and 5 of CI-on-branch. All 11 must be green, consecutive, and on that same tree.**

One reproduction falsifies "closed"; proving closure is proving a negative and can only ever
bound a residual. This is the negative case, so the argument must be stated plainly rather than
asserted.

**Part 1 — six consecutive green `scripts/check-in-container.sh all` runs (five standalone plus
the `pre-push` hook's own run of the identical command).** This is the only shape that has ever
reproduced the defect: 2 failures in 2 attempts (`25-CI-OBSERVATION.md`), versus 0 in 17 across
every warm standalone container run before this plan. From that 2-of-2 observation, the
exact-binomial 95% one-sided lower bound on its per-run reproduction probability is
`p >= 0.05^(1/2) = 0.224`. Six greens bound the residual at `0.776^6 ~= 0.218` against that
pessimistic bound, and at `0.5^6 ~= 0.016` against the ~50% per-run rate `ROADMAP.md` records
for 999.47 ("cost two retries on 2026-07-27 alone"). Neither is zero, and the pessimistic figure
is weak on its own — which is exactly why this part does not stand alone.

**Part 2 — five consecutive COMPLETED green CI `Test`-job trials on that same
`tested_head_sha`.** The count `5` is inherited from 25-10's own trial design: `0.5^5 = 1/32
~= 3.1%` residual against the recorded ~50% rate. Its purpose is different from Part 1's:
19-RESEARCH.md's D-11 ("Verify on a branch with CI. Local-green is explicitly insufficient") is
this project's standing verification standard for this class of race, and a standard is
satisfied by observing CI, not by a probability. Part 2 discharges the standard; Part 1 does
the statistical work, because Part 1 is the sensitive instrument.

**Combined.** Treating the two shapes as exchangeable — generous to the defect, since CI's
`Test` job is the *less* sensitive of the two (see `## Limits of this evidence` once Task 2
finalises it) — 11 greens bound the residual at `0.776^11 ~= 0.061` against the pessimistic
bound and `0.5^11 ~= 0.0005` against the recorded rate.

**What the record is therefore allowed to say.** `no reproduction across 11 observations at
<tested_head_sha>`, with the residual stated. Not more. The word that means closed belongs to
the human in Task 3, and even then the real load is carried by the **structural** argument —
25-11's measured census plus a bounded exec-visibility barrier at every vulnerable site, and
25-12's age floor in the production reaper — with the observations acting as corroboration
that the structure behaves as designed.

## Local push-gate observations

All six rows: command `scripts/check-in-container.sh all`, no `DEVFLOW_CI_CPUS` set, no
`DEVFLOW_SKIP_CONTAINER_CHECK` set, host toolchain not substituted.

| # | command | head sha | result | final line |
|---|---------|----------|--------|-------------|
| 1 | `scripts/check-in-container.sh all` | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | `==> check.sh: all OK` |
| 2 | `scripts/check-in-container.sh all` | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | `==> check.sh: all OK` |
| 3 | `scripts/check-in-container.sh all` | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | `==> check.sh: all OK` |
| 4 | `scripts/check-in-container.sh all` | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | `==> check.sh: all OK` |
| 5 | `scripts/check-in-container.sh all` | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | `==> check.sh: all OK` |
| 6 (pre-push hook) | `scripts/check-in-container.sh all` | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | `==> check.sh: all OK` |

Row 6 is the `pre-push` hook's own invocation, captured verbatim from `git push origin
feature/phase-25`'s output — it is the identical command, run by the hook rather than by the
executor directly, and is the one that gated the actual push.

## CI trials

| trial | run id | attempt | head sha | Test job | conclusion | url |
|-------|--------|---------|----------|----------|------------|-----|
| 1 | 30371091367 | 1 | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | success | https://github.com/denniyahh/devflow/actions/runs/30371091367 |
| 2 | 30371091367 | 2 | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | success | https://github.com/denniyahh/devflow/actions/runs/30371091367 |
| 3 | 30371091367 | 3 | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | success | https://github.com/denniyahh/devflow/actions/runs/30371091367 |
| 4 | 30371091367 | 4 | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | success | https://github.com/denniyahh/devflow/actions/runs/30371091367 |

Trial 5: pending.

## Discarded runs

none so far.

## Census-test execution proof

pending (Task 2 Step 2).

## Triage

none — nothing has failed.

## Limits of this evidence

pending (Task 2 Step 3 finalises this section).

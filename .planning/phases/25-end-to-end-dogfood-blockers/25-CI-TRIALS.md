---
phase: 25-end-to-end-dogfood-blockers
unit: 25e
backlog: 999.47 / DEN-72
observed: 2026-07-28T15:13:33Z
tested_head_sha: 82328b31eb5cbb8d795bc86f048b2602904dc8f4
evidence_commit_sha: pending
run_id: 30371091367
local_gate_runs: 6
ci_trials: 5
image: mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm
hooks_path: /var/home/denniyahh/Github/devflow/scripts/hooks
status: no_reproduction
---

# 25e / 999.47 — CI trials (closure evidence for truth 7)

**Complete: `no_reproduction across 11 observations at tested_head_sha`.** Six local
push-gate observations (five standalone `scripts/check-in-container.sh all` runs plus the
`pre-push` hook's own sixth run) and five serialized, completed CI `Test`-job trials — all
green, all at the one settled tree `82328b31eb5cbb8d795bc86f048b2602904dc8f4`. Every test
`25-SITE-CENSUS.md` names as vulnerable, plus the two 25-10-named tests, is proven by
verbatim log line to have actually executed in trial 1 and trial 5. See `## Limits of this
evidence` below — this is an observation with a stated residual, not a proof of absence, and
the disposition of truth 7 belongs to the human in Task 3, not to this artifact.

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
| 5 | 30371091367 | 5 | 82328b31eb5cbb8d795bc86f048b2602904dc8f4 | success | success | https://github.com/denniyahh/devflow/actions/runs/30371091367 |

All 5 CI trials complete.

## Discarded runs

none. Cross-checked against `gh run list --branch feature/phase-25 --workflow CI --limit 20
--json databaseId,conclusion,headSha,status`: only two runs exist on this branch — run
`30371091367` (this plan's five attempts, all `success`, at `tested_head_sha`) and run
`30315862664` (the pre-existing `success` run at `a5a068f`, predating this plan). No
`cancelled` or failed run at `tested_head_sha` is missing from this section because none
exists.

## Census-test execution proof

Pulled with `gh run view 30371091367 --attempt <n> --log --job <test_job_id>` — trial 1 =
attempt 1, `Test` job id `90314831687`; trial 5 = attempt 5, `Test` job id `90318117166`.
Every test in the required set (`25-SITE-CENSUS.md`'s four `## Vulnerable sites` rows, plus
the observed-failing test and the two 25-10-named tests — several rows overlap) is present,
verbatim, `... ok`, in both logs. Tests living in `reap_strays_e2e.rs` (integration binary)
print without a module prefix, as expected.

**V1 — `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`**
- Trial 1: `test agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper ... ok`
- Trial 5: `test agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper ... ok`

**V2 — `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`**
(also the mandatory observed-2/2-failing test from `25-CI-OBSERVATION.md`)
- Trial 1: `test commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling ... ok`
- Trial 5: `test commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling ... ok`

**V3 — `commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`**
- Trial 1: `test commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs ... ok`
- Trial 5: `test commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs ... ok`

**V4 — `reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see`** (integration
binary `reap_strays_e2e`, no module prefix)
- Trial 1: `test reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see ... ok`
- Trial 5: `test reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see ... ok`

**25-10-named — `agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process`**
(no longer the test at risk per D-13, execution recorded anyway per this plan's requirement)
- Trial 1: `test agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process ... ok`
- Trial 5: `test agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process ... ok`

**25-10-named — `commands::tests::stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check`**
- Trial 1: `test commands::tests::stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check ... ok`
- Trial 5: `test commands::tests::stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check ... ok`

No required line was absent from either log. `25-SITE-CENSUS.md`'s two `VACUOUS-NEGATIVE`
rows (A1, A2) assert NOT-FIND and are not part of this proof set — they prove the barrier
does not over-trigger, a different property than "did this test run," and are exercised by
the same green `Test` job runs (`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`,
`agent::tests::discover_stray_devflow_processes_rejects_devflow_named_argv0_with_wrong_argv1`)
without needing separate log extraction for this proof.

## Triage

none — nothing has failed.

## Limits of this evidence

1. **The headline is `no reproduction across 11 observations at 82328b31eb5cbb8d795bc86f048b2602904dc8f4`** —
   six of the push-gate shape and five CI `Test`-job trials. It is an observation with a
   stated residual, not a proof of absence.
2. **The residuals.** `~6.1%` against the exact-binomial 95% lower bound `p >= 0.224` derived
   from the 2-of-2 reproduction recorded in `25-CI-OBSERVATION.md`, and `~0.05%` against the
   ~50% per-run rate `ROADMAP.md` records for 999.47. Neither is zero.
3. **The shape asymmetry.** CI's `Test` job runs `scripts/check.sh test`, not `all`, and
   applies no `taskset` pin, so the five CI trials do NOT reproduce the fmt->clippy->test
   ordering under a 2-core pin that produced every observed failure. The six push-gate
   observations (identical to the `pre-push` hook's own command) are the sensitive
   instrument; the CI trials discharge 19-RESEARCH.md's D-11 standing CI-on-branch standard,
   which is a different job than bounding the residual.
4. **What actually carries the argument.** 25-11's measured site census
   (`25-SITE-CENSUS.md`: 4 `VULNERABLE-POSITIVE` + 2 `VACUOUS-NEGATIVE` sites, all barriered)
   plus a bounded exec-visibility barrier at every vulnerable site, and 25-12's age floor
   (`agent::STRAY_MIN_AGE`) refusing the production reaper inside the exec-visibility window.
   The observations above corroborate that structure; they do not stand alone. The phase's
   earlier "structurally removed" claim (routed to human verification in `25-VERIFICATION.md`
   as `PRESENT_BEHAVIOR_UNVERIFIED`, then falsified by `25-CI-OBSERVATION.md`'s 2/2
   reproduction) failed precisely because it asserted a structural argument that had never
   been measured — this one has `25-SITE-CENSUS.md` behind it.
5. **The disposition of truth 7 is the human's**, recorded in Task 3, not this artifact's own
   authority. `status: no_reproduction` above is this artifact's honest description of what
   was observed; it is not a closure claim, and the `status` vocabulary
   (`no_reproduction | reproduced | inconclusive`) deliberately contains no value meaning
   closed.
6. **`evidence_commit_sha` will not equal `tested_head_sha`, and it should not.** The commit
   that records `evidence_commit_sha` into this file's own frontmatter is itself a descendant
   of `tested_head_sha` that did not exist when the eleven observations above were taken. A
   file cannot contain the SHA of the commit that introduces it — this is why the standing
   final-state gate is ancestry (`git merge-base --is-ancestor tested_head_sha
   origin/feature/phase-25`), not equality. See `25-13-PLAN.md`'s `<sha_vocabulary>`.
7. **`core.hooksPath` measures as an absolute path, not the literal string `scripts/hooks`.**
   `git config --get core.hooksPath` printed
   `/var/home/denniyahh/Github/devflow/scripts/hooks` throughout this run, not the relative
   string a literal comparison would expect. Same directory, same hook — the substance of
   every push-gate row above is unaffected — but a downstream automated check written as a
   literal string equality will not match this measured value, and this artifact records what
   was actually measured rather than adjusting the value or the config to make a literal
   check pass.

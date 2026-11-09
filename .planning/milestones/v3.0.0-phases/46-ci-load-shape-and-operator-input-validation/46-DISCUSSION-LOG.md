# Phase 46: CI Load Shape and Operator Input Validation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-03
**Phase:** 46-CI Load Shape and Operator Input Validation
**Areas discussed:** CI job authority, CI runner shape, Base rejection breadth, stop's root argument,
plus a second round on removal strategy, advisory expression, backlog filing, and CI acceptance evidence

---

## Area selection

All four offered areas were selected: CI job authority, CI runner shape, Base rejection breadth,
stop's root argument.

---

## CI job authority

**First pass — question not answered, clarification requested instead.**

The operator did not pick an option, and asked: *"Explain to me the difference between these CI
jobs and the local gates that would explain why the CI jobs haven't caught anything. And what's
different about this CI job revision versus what the current CI run does? Essentially what is the
change we're proposing to make to them, and is there an improvement proposition?"*

Claude had asserted a difference without evidence. It then read `scripts/hooks/pre-push` and
`scripts/check-in-container.sh` and reported the measured difference: the local gate runs
`taskset -c 0,1 scripts/check.sh all` inside the pinned image (sequential, one process tree, two
cores); CI runs three separate jobs with a whole runner each and no ordering. The image is already
identical — only the load shape differs.

This produced an improvement over Claude's own first proposal: use `taskset`, the mechanism
`check-in-container.sh` already uses, instead of measuring `nproc` and setting cargo knobs. The
runner's real core count then stops mattering.

**Second pass:**

| Option | Description | Selected |
|--------|-------------|----------|
| Advisory first | Non-required initially; slowest and most timing-sensitive job, so most likely to flake. Watch, then promote | ✓ |
| Required immediately | Blocks merge from day one; risk of wedging every PR in the milestone | |
| Required only on PRs to develop/main | Advisory on branch pushes, blocking on the PR | |

**User's choice:** Advisory first
**Notes:** Later refined — advisory is expressed by omission from branch protection, not by
`continue-on-error`.

---

## CI load shape / runner environment

| Option | Description | Selected |
|--------|-------------|----------|
| Measure, then constrain | Print nproc/cgroup limits AND constrain parallelism explicitly | ✓ |
| Constrain only | Set knobs, don't record what the runner gave | |
| Neither — plain sequential | Accept whatever the runner has | |

**User's choice:** Measure, then constrain
**Notes:** Superseded in mechanism by the `taskset` finding — the constraint is the CPU pin, not
cargo knobs. The "record what you got" half survives as D-04.

| Option | Description | Selected |
|--------|-------------|----------|
| Pinned devcontainer image | Same container + image-parity assertion as the three existing jobs | ✓ |
| Bare ubuntu-24.04 | No container; diverges from the local gate this job mirrors | |

**User's choice:** Pinned devcontainer image

### CPU pin definition site

| Option | Description | Selected |
|--------|-------------|----------|
| Reuse DEVFLOW_CI_CPUS default | One definition site at check-in-container.sh:89; CI inherits future changes | ✓ |
| Hard-code taskset -c 0,1 in ci.yml | Workflow reads standalone; the default then lives in two places | |
| You decide | | |

**User's choice:** Reuse DEVFLOW_CI_CPUS default
**Notes:** Claude flagged a constraint on this immediately: `check-in-container.sh` drives
`docker run` and cannot be invoked from inside a GitHub container job. The intent (single
definition site) is locked; the extraction mechanism is left to planning.

---

## Base rejection breadth

Claude ran a scratch-repo comparison with negative controls in both directions before asking,
rather than reasoning about git semantics. Result: `git show-ref --verify` rejects `develop~1`,
`develop@{0}` and `develop^{}` while still accepting a real branch and still rejecting a
nonexistent one; `check-ref-format` is syntax-only and wrongly accepts `nonexistent-xyz`.

It also corrected the filing's framing: `refs/heads/develop~1` is *already* refused today, because
the current implementation prefixes `refs/heads/`. The live hole is suffix syntax surviving that
prefix. This made the mechanism question moot and left only the error-message question.

| Option | Description | Selected |
|--------|-------------|----------|
| Two messages | Missing-branch keeps today's actionable advice; revision-syntax gets its own message | ✓ |
| One message, generic | Single refusal naming the value, as today | |
| You decide | | |

**User's choice:** Two messages
**Notes:** Today's advice (`git branch X origin/X`) is actively misleading for `develop@{0}` — it
tells the operator to create a branch literally named that.

---

## stop's root argument

**First pass — question not answered, clarification requested instead.**

The operator asked: *"Why would we need to allow someone to provide the root, in case they wanted
to make the call from a different project directory? What's the use case for root?"*

Claude read `project_root` (`main.rs:718`) and reported that it walks **up** to the nearest
`.devflow` ancestor, so the default `.` already covers nearly every case including running from
inside a phase worktree. The genuine use case is thin — invoking from outside the project
entirely. The real content of VALID-02 is therefore consistency and error legibility, not
capability. Claude also found that `evidence` and `sweep` share the `--root` form, so removing it
from `stop` alone would trade one inconsistency for another.

| Option | Description | Selected |
|--------|-------------|----------|
| Add positional, keep --root | Non-breaking; consistent with both families | |
| Positional only — remove --root | Cleanest; breaking, and makes stop the odd one out vs evidence/sweep | |
| Positional here, file the rest | Add positional now; backlog entry to converge evidence/sweep later | ✓ |

**User's choice:** Positional here, file the rest — *later reversed, see below*

### Both-supplied conflict (first pass)

| Option | Description | Selected |
|--------|-------------|----------|
| Hard error naming both | | |
| --root wins silently | | |
| Not applicable | Only if positional-only with --root removed | ✓ |

**User's choice:** Not applicable
**Notes:** Answered under the positional-only assumption; re-opened once `--root` was kept.

---

## Second round — new gray areas

### --root removal strategy

Claude measured the blast radius first: 11 in-repo call sites (`stop_e2e.rs` ×10,
`reap_strays_e2e.rs:163`) plus `OPERATIONS.md:43`. No external consumers found.

| Option | Description | Selected |
|--------|-------------|----------|
| Hard removal now | Delete flag, migrate 11 call sites + docs, BREAKING CHANGE entry; in-band for v3.0.0 | |
| Accept-but-hidden for one release | Warn on use, remove in v3.1 | |

**User's response (free text):** *"I change my mind, keep root and make it override positional
when used"*

**Resolution:** `--root` is kept and takes precedence over the positional when both are supplied.
No error on the conflict; the flag simply wins. This supersedes both the "positional here, file
the rest" choice above and the earlier "not applicable" conflict answer. Net effect: the phase
carries no breaking change and needs no CHANGELOG BREAKING entry.

### Advisory expression

| Option | Description | Selected |
|--------|-------------|----------|
| Just omit from required checks | Nothing in-tree; job reports honestly, simply not in branch protection | ✓ |
| continue-on-error: true | Advisory visible in ci.yml, but reports SUCCESS on real failure | |
| You decide | | |

**User's choice:** Just omit from required checks
**Notes:** `continue-on-error` would poison the `gh pr checks` reading this repo's own rules
require.

### Backlog filing for evidence/sweep convergence

| Option | Description | Selected |
|--------|-------------|----------|
| ROADMAP 999.x only | Matches standing practice for internal hygiene | |
| ROADMAP 999.x + GitHub issue | Mirror out for visibility | |
| GitHub issue only | | |

**User's response (free text):** *"I changed my mind, we can leave these alone"*

**Resolution:** No backlog entry is filed. `evidence` and `sweep` keep `--root` unchanged, and the
convergence idea is recorded in CONTEXT.md's deferred section only so a future reader knows it was
weighed rather than missed.

### CI acceptance evidence

| Option | Description | Selected |
|--------|-------------|----------|
| This phase's own PR | Job must be green in `gh pr checks` against that PR's HEAD_SHA | ✓ |
| A throwaway PR, then close it | Cleaner separation; leaves a discarded PR | |
| Workflow-file review only | Weakest; forbidden by the repo's own rules | |

**User's choice:** This phase's own PR

---

## Claude's Discretion

- Exact `ci.yml` job name, `timeout-minutes`, and step ordering.
- Exact wording of the two `CliError` messages.
- The extraction mechanism for the shared `0,1` default, subject to the stated docker constraint.

## Deferred Ideas

- Converging `evidence`/`sweep` onto a positional project root — considered and explicitly
  dropped, not filed.
- Promoting the new CI job to a required check — after the milestone has watched it.
- Establishing whether the new job actually catches 999.47 — needs field evidence over time.

## Unverified premise recorded during discussion

The roadmap's criterion 1 says the job runs "on a 2-core runner". `denniyahh/devflow` is a public
repository and GitHub's free standard runner for public repos is larger than 2 vCPU; no job has
ever printed `nproc`, so the premise is unverified. The `taskset` pin (D-02) makes it true by
construction rather than by assumption.

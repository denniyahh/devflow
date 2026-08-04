# Phase 31: Claude Adapter Launch Path — Pipe-Owning Monitor (999.64 arc close) - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-03
**Phase:** 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
**Areas discussed:** Idle-timeout policy, What firing the timeout does, Rollout shape, Acceptance run mechanics, CLI-behaviour guard (M2)

---

## Idle-timeout policy

**What the idle timer measures**

| Option | Description | Selected |
|--------|-------------|----------|
| Every stream line | Reset on any JSONL line. Tightest observed gaps (6.02–7.09s), widest margin | ✓ |
| Milestone events only | Reset only on `result` / `background_tasks_changed`. Gaps 7.70–13.73s — where the 12s floor failed | |
| Both, whichever is later | Most conservative against false kills; a chatty-but-stuck agent would never time out | |

**Timeout value**

| Option | Description | Selected |
|--------|-------------|----------|
| 60s | 2x the measured floor | |
| 30s (the floor itself) | The measured constraint-8 minimum | ✓ |
| 120s | Maximum safety; a hang burns two minutes | |

**Notes:** The 30s option was described at ask-time as having "no headroom". That framing was
calibrated to the *milestone* signal (pooled max 13.73s) and was **corrected during the
discussion**: paired with the every-line signal actually chosen, observed max is 7.09s, making 30s
~4.2x margin. The two answers are consistent. Correction carried into CONTEXT.md D-02 so a
downstream reader does not re-derive the wrong conclusion and "fix" the value upward.

**Outer wall-clock bound**

| Option | Description | Selected |
|--------|-------------|----------|
| No outer bound | Idle-only; constraint 5 rejected fixed wall-clock | ✓ |
| Yes, a generous one | Hard ceiling as last-resort backstop; adds a second thing that can fire wrongly | |

**Configurability**

| Option | Description | Selected |
|--------|-------------|----------|
| Configurable with a hard floor | Clamped at 30s, clamp logged; mirrors `DEVFLOW_GATE_TIMEOUT_SECS` | ✓ |
| Fixed constant | Impossible to misconfigure; no escape hatch | |
| Configurable, no floor | Lets someone reintroduce the 12s value measured to kill healthy runs | |

---

## What firing the timeout does

**Sequencing**

| Option | Description | Selected |
|--------|-------------|----------|
| Write result, then terminate | Result exists before anything can race it | ✓ |
| Terminate, then write | Leaves a window with a dead child and no authoritative result | |
| Write, close stdin, wait, then kill | Gentlest; adds a second timer and more states | |

**Status recorded**

| Option | Description | Selected |
|--------|-------------|----------|
| Distinct first-class variant | Separate from `Failed` and `ResourceKilled`; legible to the oracle | ✓ |
| Reuse `Failed` with a reason string | Smaller diff; distinction survives only in prose | |

**Partial work**

| Option | Description | Selected |
|--------|-------------|----------|
| Fail loudly, name the commits | The 999.64 shape made actionable | ✓ |
| Fail, do not enumerate | Reproduces the defect's worst property: work exists, nothing says so | |
| Fail and roll the commits back | Destroys real work on a possible false positive; irreversible | |

**Retry policy**

| Option | Description | Selected |
|--------|-------------|----------|
| Terminal — stop and report | Matches the project's stop-at-the-first-hard-gate rule | ✓ |
| Retryable once | Retry starts from a dirty, partly-done state (999.65/999.66 territory) | |

---

## Rollout shape

| Option | Description | Selected |
|--------|-------------|----------|
| One stage first, then widen | Produces the first real capture before betting the launch path on it | ✓ |
| All stages at once | Single flip; bets everything on a parser that has never seen real output | |

**First stage**

| Option | Description | Selected |
|--------|-------------|----------|
| Code | Where 999.64 was observed; the stage that actually backgrounds | ✓ |
| Plan | Cheaper to re-run; exercises the multi-child drain path less | |
| Define | Cheapest; rarely backgrounds, so a green result proves least — a proxy | |

**Fallback**

| Option | Description | Selected |
|--------|-------------|----------|
| Explicit opt-out flag, off by default | Recovery without a release; use must be logged loudly | ✓ |
| No fallback | Forces fixes over workarounds; a break needs a release | |
| Automatic fallback on parse failure | Silent downgrade — the invisible-degradation class of the bug itself | |

**Single-document regression protection**

| Option | Description | Selected |
|--------|-------------|----------|
| Keep 30b's isolation tests as the gate | Reuses the existing contract; small diff | ✓ |
| Dedicated adapter-matrix test | More thorough; larger diff against the M cap | |

---

## CLI-behaviour guard (M2)

**Notes:** The operator asked for a non-technical explanation of the problem before answering, and
one was given (the "done" message is undocumented CLI behaviour observed on 2.1.220; if it silently
stops arriving, DevFlow loses work exactly as in 999.64). The operator then **proposed a mechanism
not among the offered options**: have the task declare what a valid success message should say, and
have the orchestrator record that and use it for confirmation.

That proposal was adopted. It is stronger than the offered bare canary because it answers "did
*this specific* helper's message arrive" rather than "did *a* message arrive" — the distinction the
CLI destroys when it coalesces completions (constraint 7). Two traps were surfaced while validating
it, both already known from Phase 30: the stream echoes the prompt, so the token must be matched
only inside a top-level `result` event; and the agent can see its own token, so the token proves
delivery, never that work happened.

| Option | Description | Selected |
|--------|-------------|----------|
| Behavioural canary (as offered) | Detect the behaviour, not the version string | superseded |
| Version-string assertion | Tests a label, not the capability | |
| Both — version gates the canary | Best cost profile, most moving parts | |
| **Declared-token canary (operator's proposal)** | Task declares its success token; orchestrator records and confirms it in a top-level `result` | ✓ |

**Token scope**

| Option | Description | Selected |
|--------|-------------|----------|
| Startup canary only | Cheapest, stays inside the M cap | ✓ |
| Start test + ticket every helper | Best signal; defeats coalescing directly; likely exceeds M | |
| Ticket every helper, no start test | No extra cost; failure surfaces after real work is dispatched | |

**On drift**

| Option | Description | Selected |
|--------|-------------|----------|
| Refuse to run, report clearly | Never-silent gate | ✓ |
| Warn loudly and proceed | Warning scrolls past in unattended mode — the target mode | |
| Fall back to sequential dispatch | Silent capability downgrade | |

**Cadence**

| Option | Description | Selected |
|--------|-------------|----------|
| Once per run, result recorded | Every run carries evidence of what was verified | ✓ |
| Once per CLI version, cached | Cache keyed on the version string — reintroduces the proxy | |
| Only before a multi-plan wave | A prediction about which waves background — constraint 1 bans it | |

---

## Acceptance run mechanics

**Workload**

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal purpose-built 2-plan wave | Cheapest workload crossing every seam under test | ✓ |
| Literally re-run Phase 29 wave 2 | Maximum fidelity; Phase 29 was ABORTED, plans need reconstructing | |
| A real backlog phase end to end | Useful work alongside proof; couples result to that phase's difficulties | |

**Isolation**

| Option | Description | Selected |
|--------|-------------|----------|
| Main checkout, orchestrator hands off git | 30-02/30-04 precedent; a worktree changes the shape being measured | ✓ |
| Dedicated worktree | Isolates the tree; breaks the container pre-push gate | |
| Scratch clone outside the repo | Max isolation; furthest from the real launch environment | |

**Pass rule**

| Option | Description | Selected |
|--------|-------------|----------|
| Both plans produce SUMMARY.md and merge | The exact inverse of the 999.64 failure | ✓ |
| Both completions observed in the stream | Constraint 7 makes an observed count the signal that can undercount | |
| Stage reports Success | Already failed once — the oracle scored the orphaned stage Success | |

**On failure**

| Option | Description | Selected |
|--------|-------------|----------|
| Phase does not close; diagnose and re-run | The arc goal is the acceptance criterion | ✓ |
| Close, file the failure as backlog | Declares the arc closed on evidence that did not hold | |
| Close PARTIAL, like Phase 26 | Truthful precedent; leaves the milestone's closing condition unmet | |

---

## Claude's Discretion

- Where the constraint-9 exit-code guard physically lives in the layer cascade — implementation
  approach, deliberately not put to the operator.
- Internal structure of the pipe-owning monitor replacing `spawn_monitor`'s `sh` script.
- Near-simultaneous-completion test design (review M3), to be resolved by the planner against
  constraint 7's coalescing evidence.

## Deferred Ideas

- **Per-child declared tokens** — extend the token mechanism from the startup canary to every
  dispatched child, defeating constraint 7's coalescing undercount by construction rather than
  relying on the drain gate. Deferred on size only; strong candidate for its own phase.

# Phase 23 — Findings Not Captured by Any Plan Summary

Recorded 2026-07-26, after plan 23-11 closed and after a live cleanup of the
orphan population on the operator's machine. These are findings that emerged
from *operating* the phase's output, not from executing any single plan, so
none of the 11 plan SUMMARYs own them.

Two sources: (A) the orphan-process cleanup run against the machine using this
phase's own new tooling, and (B) the acceptance run's failure mode and its
side effects.

---

## A. Orphan cleanup — live exercise of 23-03/23-04/23-05

**What was done.** 23 orphaned `devflow advance` processes were found on the
machine (~128 MB resident, oldest 11h45m) using `devflow gate list --all-roots`
— the enumeration shipped by plan 23-03. Nothing before this phase could see
them. They were then cleared: **22 by `devflow gate sweep --max-age-secs 60`**
(the reaper from plan 23-04), **1 by `kill -TERM`**.

**Validation result — the phase's central claim held.** `23-ORPHAN-FORENSICS.md`
states: *"`devflow stop` is the missing primitive, and its absence is why this
population exists. Nothing could clean these up. They were removed with
`kill(1)`."* On this population, 22 of 23 were removed by DevFlow itself, with
each child unwinding through its own `abort()` path after the gate was answered
with a rejection. Median unwind was well under 60s. That claim is now retired
in practice, on a real population, not only in tests.

### A1. Residual orphan class — a process can outlive its own lock and state

**Severity: the interesting one.** PID 3744133 (`--phase 7`, root
`/tmp/.tmpMVmZBl`) was unreachable by both new primitives:

- `devflow gate sweep` never listed it — its root was absent from the registry,
  so the cross-root enumeration could not see it.
- `devflow stop --phase 7 --root /tmp/.tmpMVmZBl` reported
  `no lock held for phase 7 — nothing is running advance()` and
  `no persisted state for phase 7 — already stopped`, exiting 0.
- `devflow stop --phase 8 --root …` (the root's state file is `state-08.json`)
  reported the same and also exited 0.

Both responses are *correct* against the recorded state — there genuinely was
no lock and no state. The process had outlived both. `kill -TERM` cleared it in
1 second.

So the orphan story from this phase is complete for gate-blocked processes and
**open for state-orphaned ones**. Any process whose lock/state was removed
while it still runs is invisible to enumeration and inert to `stop`. A fix
needs a registry-independent path — e.g. a PID-based reaper that scans for
`devflow advance` children and validates them against the registry, treating
"running but unregistered" as its own reportable class rather than silence.

Note this is precisely the shape of failure the phase exists to prevent: the
tooling returned exit 0 and a reassuring message while a live orphan sat
underneath it. It is not a false green in the attestation sense — the message
is true — but an operator reading only the exit code would conclude the machine
was clean when it was not.

### A2. Registry produces duplicate entries for one root

`devflow gate sweep --dry-run --max-age-secs 60` listed `/tmp/.tmpal43JM` four
times — `phase 7 code` twice and `phase 8 code` twice, with identical ages per
pair. The dry run reported **24 would be reaped**; the real sweep reported
**22 reaped, 0 skipped, 0 left alone**. The two-entry gap is consistent with
the duplicates collapsing on execution.

Deduplication on `(root, phase, stage)` appears to be missing on the write or
read path in `registry.rs`. Low severity — the sweep is idempotent and no
double-reap occurred — but the dry-run count is what an operator reads before
authorizing a destructive sweep, and it over-reports.

### A3. The e2e suites leak monitor pairs — this is the actual source

Every orphan traced to `/tmp/.tmp*` scratch roots created by this phase's own
test suites (`gate_sweep_e2e.rs`, `stop_e2e.rs`) plus older phase-12 fixtures.
23 accumulated in roughly 12 hours of development. None touched a real
repository.

The tests deliberately spawn real, separate `devflow advance` children — that
is what makes them strong tests, and plan 23-04's summary is right to call it
the strongest claim in the suite. But they do not reap those children on the
way out. Until they do, every full `cargo test --workspace` run refills the
population, and the machine accrues process pairs indefinitely.

Fix belongs in the test harness (a `Drop` guard or explicit teardown that stops
the spawned child), not in production code.

---

## B. Acceptance-run findings

### B1. A third precondition class the setup did not check

Plan 23-10 established seven behavioural checks (Task 2) and two content
preconditions (Task 3, A: security artifact / B: no self-attested Ship claim).
None of them asked the question that actually stopped the run:

> **Is the target phase's ROADMAP entry reachable from the branch
> `devflow start` will fork from?**

`devflow start` builds a fresh feature branch from `develop`'s tip. Phase 24's
ROADMAP entry was created on `feature/phase-23` and never merged, so the tree
handed to the run did not contain it. The run was structurally unable to
succeed from the moment of that promotion — before any of the seven checks ran,
and independent of both content preconditions.

Any future acceptance attempt needs this as an explicit precondition, checked
against `develop` (or whatever `git.base-branch` resolves to), not against the
working branch.

**Attribution:** this was an orchestrator sequencing error, not a DevFlow
defect. DevFlow behaved correctly throughout — the agent detected the missing
roadmap entry, refused to fabricate one, reported failure through the
documented protocol, and the never-silent gate fired. Recording it here so the
failure is not later misread as a product bug.

### B2. `devflow cleanup` deletes recovery refs by ancestry

Disclosed in `23-ACCEPTANCE-RUN.md` §6/§7 and repeated here because it is a
product behaviour, not a run artifact. `devflow cleanup`, run for worktree and
branch hygiene, also deleted the local branch
`recovery/pre-23-11-acceptance-e0f87c2` on the grounds that it was "merged" by
ancestry — which it was, being a pointer at `develop`'s tip.

The remote copy on `origin` was untouched and the local branch was restored in
the same session, so nothing was lost. But a recovery point is defined by
pointing at a known-good commit, which makes it *always* an ancestor and
therefore *always* eligible for ancestry-based deletion. Cleanup should either
skip a configurable ref prefix (`recovery/*`) or refuse to delete refs it did
not create.

### B3. Self-dogfood staleness — only the warn branch was exercised

Already recorded as this plan's own coverage gap in `23-ACCEPTANCE-RUN.md` §9;
noted here for completeness. The run correctly detected self-dogfood, but only
the `Ahead`/warn branch fired — the `Stale`/hard-block branch
(`crates/devflow-cli/src/staleness.rs:276-284`), the most frequent observed
killer of prior runs, remains unexercised in a real run. D-02 chose the
self-dogfood target specifically because that branch is structurally
unreachable in a scratch repository, so this is a gap the acceptance run was
supposed to close and did not.

---

## Ordering lesson for the next attempt

Promoting a backlog item to a numbered phase **on a feature branch** makes it
invisible to `devflow start`. Either merge the promoting branch to `develop`
first, or land the promotion on `develop` via its own PR before launching. On
this repository both hops are protected and force PRs, so "merge first" means a
real PR and its CI wait — budget for it rather than discovering it at launch.

# Phase 31 — Live Acceptance Run (999.64 arc gate)

**Run date:** 2026-08-03
**Executed by:** plan 31-05, task 2
**Adjudication:** NOT made here. Task 3 is a blocking human-verify checkpoint (D-19). This file is
evidence, not a verdict.

---

## Pass criterion

**D-18: both plans produce a `SUMMARY.md` AND both merge.** Nothing else counts as a pass.

Two substitutes are explicitly rejected and were not used:

- **NOT "the stage reported Success."** The completion oracle already scored the orphaned Phase 29
  stage as Success. This run's stage did report Success (`advance_evaluated status=success,
  decided_by_layer=1`), and that fact is recorded below as context, not as the criterion.
- **NOT "both completions were observed in the stream."** Constraint 7 makes an observed count the
  very signal that can undercount: the CLI coalesces, so one `result` event can carry two children's
  completions and is superficially indistinguishable from one delivered and one lost.

A third condition makes the run *readable at all*, added by the adversarial review (B1):

- **VOID unless the capture shows a `background_tasks_changed` event with a NON-EMPTY `tasks` array
  followed by a drain to `[]`.** Without it the wave never backgrounded, the pipe-owning monitor's
  delivery premise was never exercised, and two SUMMARY.md files appearing would prove only that two
  plans ran sequentially. `execute-phase.md` has at least four documented paths that would do exactly
  that.

**Result: PASS on all three, on attempt 1.** Evidence below.

---

## Attempt log

| # | Started (UTC) | Route | Outcome | Diagnosis |
|---|---|---|---|---|
| 1 | 2026-08-03 20:32:24Z | A (seed `.devflow/state-97.json`, `devflow resume --phase 97`) | **PASS** | — |

One attempt. No retries, no blind re-runs, no diagnosis needed. A first-try pass is what is being
shown here; a reader weighing this should note that it is the strongest of the attempt shapes but
also the one that exercised the fewest failure paths.

---

## Route and invocation

**Route A** — the only viable route. Route B (`devflow start --phase 97`) was **not attempted**:
`commands.rs:177` calls `ensure_phase_reachable_on_base(project_root, 97, DEVELOP)`, which refuses
when neither a ROADMAP entry nor the phase directory is reachable on `develop`. Phase 97 is on
neither. The obvious repair — adding a phase 97 entry to ROADMAP on `develop` — is forbidden by
T-31-21 and would put a fabricated phase in a shared branch's permanent history.

Seed state (`.devflow/state-97.json`, atomic temp-plus-rename):

```json
{"stage":"code","phase":97,"agent":"claude","mode":"auto","stop_until":"code",
 "stopped":false,"yes_ship":false,"canary":null,"legacy_claude_launch":false,
 "project_root":"/var/home/denniyahh/Github/devflow", ...}
```

Invocation: `devflow resume --phase 97`

**`stop_until: "code"` / `stopped: false` was the mechanism that made D-17 enforceable.** Without the
cap, `transition()` launches the next stage as its last action, so the git-quiet window would have
been Code, a sub-second gap, Validate, a gap, Ship — a boundary that closes again within
milliseconds, making CLAUDE.md's no-git-during-executor rule unenforceable as written. With the cap
there was exactly one executor and one unambiguous end.

It fired exactly as designed. Final persisted state:

```json
"stop_until":"code", "stopped":true,
"stop_reason":"stopped after code completed (--until code)"
```

and `{"event":"workflow_finished","phase":97,"reason":"stopped_at","stage":"code"}`. No Validate
agent, no Ship agent, no push.

**Verified before launch, not assumed:** a negative control confirmed `stop_until` is genuinely
parsed rather than silently ignored — a sibling state file with `"stop_until":"bogus_stage"` was
rejected with `unknown variant 'bogus_stage', expected one of define, plan, code, validate, ship at
line 16 column 29`, which is the exact line of the field.

---

## Baseline (pre-run, real command output)

| Item | Value |
|---|---|
| Pre-run SHA | `626131bfb0fda8ff823154c325d35fe4fbeced04` |
| Branch | `feature/phase-31` |
| `git status --porcelain` | *(empty — no output at all)* |
| `sha256sum .planning/STATE.md` | `a0353fd7d4074c8eece72411b6937ab50dde0bfd6bf98e80842f0e743e2d4427` |
| `sha256sum .planning/ROADMAP.md` | `3fbe8d4346758d1a325c644a23eab91e8b2f41574964ea02890346554016a6b9` |
| `claude --version` | `2.1.220 (Claude Code)` |
| `devflow` on PATH | `/home/linuxbrew/.linuxbrew/bin/devflow` → symlink → `/home/denniyahh/Github/devflow/target/release/devflow` |
| `devflow --version` | `devflow 2.2.0` |

`claude --version` is **2.1.220** — the exact version the arc's delivery premise was witnessed on
(30c-VERDICT). No version drift to discount.

### Binary provenance — measured directly, not by mtime

The plan's original stale-binary guard compared the binary's mtime against
`git log -1 --format=%cI`. **That check is a proxy and it would have failed here for the wrong
reason:** committing the two scratch plans moved the branch's newest commit to `16:31:02` while the
binary was built at `16:21:35`, so the mtime test reports "stale" for a binary that is in fact
current. mtime proves the file was written after a commit, not built from it.

Direct discriminator instead:

```
$ devflow __monitor --help >/dev/null 2>&1; echo $?
0
```

`__monitor` is the pipe-owning monitor's process entry point, introduced by 31-01. The subcommand
does not exist on any pre-31 binary, so exit 0 is positive proof the running binary carries this
phase's launch path.

Note that `workflow_started` — which carries `DEVFLOW_BUILD_COMMIT` / `DEVFLOW_BUILD_DIRTY` from
`build.rs` — is emitted by `start` and **not** by `resume`. On Route A the event ledger therefore
carries no build provenance at all, which is why the `__monitor` check is the one relied on.

### Containment setup (deliberate, and it did not take effect)

`init.execute-phase 97` returns `branching_strategy: "phase"` and `branch_name: "feature/phase-97"`,
and `execute-phase.md`'s `handle_branching` step would fork that branch off `origin/develop` — which
would have removed every Phase 31 planning file from the working tree mid-run and put the run's
commits on a branch forked from `develop`. To avoid that, `feature/phase-97` was pre-created at the
pre-run SHA (`git branch feature/phase-97 HEAD`) so `handle_branching`'s reuse arm would fire
(`git switch`, no fork, no `develop` touch, no tree change).

**The Code-stage orchestrator did not run `handle_branching` at all.** It read the step, stated a
branching deviation with reasons, and stayed on `feature/phase-31`. The consequence is recorded under
*Contamination* below. The containment branch was never used and has been deleted.

---

## The workload (removed after the run — reproduced here)

`.planning/phases/97-acceptance-two-plan-wave/` held exactly two plans. Both were removed at cleanup;
this section is the surviving record.

**Shared frontmatter shape** (both plans identical except for the marker path):

```yaml
phase: 97-acceptance-two-plan-wave
plan: 01            # / 02
type: execute
wave: 1
depends_on: []
files_modified:
  - .planning/phases/97-acceptance-two-plan-wave/scratch/marker-alpha.txt   # 97-01
  - .planning/phases/97-acceptance-two-plan-wave/scratch/marker-beta.txt    # 97-02
autonomous: true
```

**Each plan's single task, in full:** create its own marker file whose entire content is one line —
`ACCEPTANCE-97-01-ALPHA` for 97-01, `ACCEPTANCE-97-02-BETA` for 97-02 — then commit it. Each task's
`<verify><automated>` checked its own file with `rg -qx` for a whole-line match; each `<done>` named
its own `97-0N-SUMMARY.md` explicitly. Each plan's `<action>` forbade reading the codebase, running
any build/test/lint, and touching the sibling's file. Nothing that could fail for a reason unrelated
to delivery.

`verify.plan-structure` reported `valid: true, task_count: 1` for both.

**`files_modified` disjointness, computed rather than eyeballed** — with a negative control, because
an intersection that is empty for the wrong reason looks identical to one that is empty for the right
reason:

```
INTERSECTION (97-01 x 97-02):  count=0
NEGATIVE CONTROL (97-01 x 97-01): count=1
  .planning/phases/97-acceptance-two-plan-wave/scratch/marker-alpha.txt
```

The control is non-empty, so the empty intersection is a real disjointness result and not a broken
comparison.

---

## Canary — first run of D-13's guard against the real CLI

Verbatim from `.devflow/events.jsonl`:

```json
{"cli_version":"2.1.220 (Claude Code)","event":"claude_delivery_canary_confirmed","phase":97,"reason":null,"stage":"code","token_prefix":"DEVFLOW_DELIVERY_CANARY_","ts":1785789163,"v":1}
```

**Outcome: `Confirmed`.** Cost: ~19 s wall clock, synchronously inside `devflow resume`
(20:32:24Z invoke → 20:32:43Z monitor spawn), against a 300 s deadline. Persisted to
`state.canary: "confirmed"`.

This is the first time the declared-token guard has run against the real Claude CLI rather than an
injected launcher. It is **not** a third-outcome case: a canary refusal (`Absent` / `Unverified`)
would have errored out before spawning any monitor, produced no `phase-97-stdout`, and had to be
recorded as *"canary refused — run not performed"* rather than as an acceptance failure. That did not
happen.

---

## VOID guard — did the wave actually background?

**Yes.** Four top-level `background_tasks_changed` events, non-empty then drained to `[]`:

| capture line | `tasks` | meaning |
|---|---|---|
| 438 | `[a88358fe70a3a44e7]` | 97-01 backgrounded |
| 456 | `[a88358fe70a3a44e7, a572f3ed8b7f202e3]` | **both pending simultaneously** |
| 618 | `[a572f3ed8b7f202e3]` | 97-01's completion delivered, drains out |
| 657 | `[]` | **drain to empty** — 97-02's completion delivered |

**A substring grep for `background_tasks_changed` is not sufficient and was not used.** The first
match in the capture was a *file echo*: the Code agent read `31-05-PLAN.md`, whose text contains the
string, and the tool result came back through the stream. That is D-13's trap 1 (prompt echo) landing
on the evidence-gathering instead of on the canary. The counts above match on the top-level event
shape `{"type":"system","subtype":"background_tasks_changed","tasks":[...]}` — the same shape
`monitor.rs:544` parses.

### The close rule in production

Three top-level `result` events, at capture lines 476, 654 and 932:

| line | event | `DEVFLOW_RESULT` present | background tasks outstanding |
|---|---|---|---|
| 476 | `result` (success, 34 turns) | no | **2** |
| 654 | `result` (success, 2 turns) | no | **1** |
| 932 | `result` (success, 24 turns) | **yes** | **0** |

Line 476 is the 999.64 orphan point made visible: the orchestrator's turn ended with two executors
still running. Under the pre-31 detached `sh` monitor — `.stdin(Stdio::null())` — the child would
have exited there and both executors' work would have been stranded on branches nobody merged. Here
the session stayed alive, received both completion notifications, and only then produced a `result`
carrying the marker.

The orchestrator's own account, from the capture at line 475: *"my turn ends here with two pending
background tasks, and Phase 31's pipe-owning monitor must keep the session alive past turn end and
deliver both completion notifications."* And at line 931, after the fact: *"both completion
notifications were delivered afterward — no spot-check fallback was needed."*

No idle timeout fired: `.devflow/phase-97-idle-timeout` was never written, and `stderr.log` is 0
bytes.

---

## D-18 — both halves, per plan

| Plan | SUMMARY.md exists | Merged | Agent branch | Merge commit | Plan's own commits |
|---|---|---|---|---|---|
| **97-01** | **yes** (66 lines) | **yes** | `worktree-agent-a88358fe70a3a44e7` | `1302e93` (2 parents) | `0590537` marker, `c2efa0d` SUMMARY |
| **97-02** | **yes** (66 lines) | **yes** | `worktree-agent-a572f3ed8b7f202e3` | `5caaa4c` (2 parents) | `147c9bd` marker, `aa317a5` SUMMARY |

### The merge half is topological, not a marker grep

A bare `git log --oneline | rg '<marker>'` is satisfied by an orchestrator that created the two files
itself — trivially performable inline for a task whose whole content is "write one string and commit
it". Worse, `execute-phase.md` **instructs** the orchestrator that if a completion signal never
arrives but commits and SUMMARY.md are visible, treat it as successful and continue: a documented
in-workflow path by which delivery fails completely, a filesystem poll rescues it, and a marker grep
still passes. That path was not taken here, and the evidence that it was not taken is structural:

```
911bf50 parents=[06832ab]                docs(phase-97): complete phase execution
06832ab parents=[5caaa4c]                docs(phase-97): update tracking after wave 1
5caaa4c parents=[1302e93 aa317a5]        chore: merge executor worktree (worktree-agent-a572f3ed8b7f202e3)
1302e93 parents=[626131b c2efa0d]        chore: merge executor worktree (worktree-agent-a88358fe70a3a44e7)
aa317a5 parents=[147c9bd]                docs(97-02): complete delivery marker beta plan
c2efa0d parents=[0590537]                docs(97-01): complete delivery-probe plan
147c9bd parents=[626131b]                feat(97-02): add plan 02 delivery marker marker-beta.txt
0590537 parents=[626131b]                feat(97-01): add plan 01 delivery marker
```

Both merges have **two** parents. Both second-parent chains fork **independently from `626131b`**,
the pre-run SHA. Inline work by a single agent produces a linear chain with no second parents; this
shape is unobtainable without two separate branches that existed concurrently.

Ancestry, checked with `git merge-base --is-ancestor <commit> feature/phase-31`, all six ANCESTOR.
A first negative control for that check was **discarded as broken** — `main` is an ancestor of
`feature/phase-31`, so it could not discriminate. Replaced with the reverse direction, which does:
`git merge-base --is-ancestor feature/phase-31 0590537` → NOT ancestor.

### The strongest single piece of evidence

The harness's own background-task ids appear inside git's merge-commit subjects:

```
stream : "task_id":"a88358fe70a3a44e7"  description "Execute plan 97-01"
git    : chore: merge executor worktree (worktree-agent-a88358fe70a3a44e7)

stream : "task_id":"a572f3ed8b7f202e3"  description "Execute plan 97-02"
git    : chore: merge executor worktree (worktree-agent-a572f3ed8b7f202e3)
```

Corroborated independently by each executor's own return block, which recorded
`"expected_base":"626131bfb0fda8ff823154c325d35fe4fbeced04"` and its branch name, and by the live
observation of `.claude/worktrees/agent-a572f3ed8b7f202e3` and
`.claude/worktrees/agent-a88358fe70a3a44e7` on disk during the run. Four independent systems —
the CLI's background-task ledger, the filesystem, each executor's self-report, and git's commit
graph — agree on the same two identifiers.

### Marker file contents, verbatim after merge

```
.../scratch/marker-alpha.txt : ACCEPTANCE-97-01-ALPHA
.../scratch/marker-beta.txt  : ACCEPTANCE-97-02-BETA
```

---

## Timings

| Moment | UTC | Source |
|---|---|---|
| `devflow resume --phase 97` invoked | 20:32:24.281Z | wall clock, recorded before the call |
| Canary confirmed / `stage_launched` | 20:32:43Z | `events.jsonl` ts `1785789163` |
| `resume` returned (monitor pid 594145 detached) | 20:32:43.128Z | wall clock |
| `advance_evaluated` + `workflow_finished` | 20:54:52Z | `events.jsonl` ts `1785790492` |

- **Canary: ~19 s.** Against a 300 s deadline — this number is the canary's real cost, not a timeout.
- **Code stage, end to end: 1329 s (22 m 09 s).**
- **CLI-reported `duration_api_ms`: 1 462 879 ms.** This is cumulative API time across 24 turns, not
  wall clock, and it exceeds the stage's wall clock. Do not read it as a duration.

### The drain→result interval is a bound, and it measures the wrong thing

`.devflow/phase-97-stdout` carries **no per-line arrival timestamps**, so the interval between the
line-657 drain and the line-932 `result` cannot be read off the capture. From live polling it is
bounded: the drain had not occurred at T+803 s and had occurred by T+1239 s; the final `result` landed
at T+1329 s. So the interval lies in **[90 s, 526 s)**.

That interval is **dominated by the post-merge gate** — between the drain and the final result the
orchestrator merged two worktrees, ran the verification agent, and ran `cargo build` plus a 22-suite
`cargo test`. It is a measurement of DevFlow's own gate, not of notification latency. **Delivery
latency — executor completion to notification arrival — is not measurable from this capture at all.**

Two `rate_limit_event` lines appeared (capture lines 10 and 448). Neither was fatal.

---

## Contamination and cleanup

### `git status --porcelain` is blind here and was not used for either check

`.devflow/.gitignore` contains `*`, so no capture file can ever appear in `git status`. And
`execute-phase` **commits** STATE.md/ROADMAP.md per wave and at phase close, so contamination arrives
as commits on the branch — a clean status after a contaminating commit reads as clean. Both checks
were done against the pre-run SHA and by direct `ls`.

### Contamination that arrived

**Eight commits landed on `feature/phase-31`**, not on the pre-created `feature/phase-97`
containment branch, because the orchestrator declined `handle_branching`:

```
911bf50 docs(phase-97): complete phase execution
06832ab docs(phase-97): update tracking after wave 1
5caaa4c chore: merge executor worktree (worktree-agent-a572f3ed8b7f202e3)
1302e93 chore: merge executor worktree (worktree-agent-a88358fe70a3a44e7)
aa317a5 docs(97-02): complete delivery marker beta plan
c2efa0d docs(97-01): complete delivery-probe plan
147c9bd feat(97-02): add plan 02 delivery marker marker-beta.txt
0590537 feat(97-01): add plan 01 delivery marker
```

**These commits were deliberately NOT rewound.** They *are* the D-18 merge evidence, and leaving them
means a later reader can re-derive the topology from the branch instead of trusting this file's
transcription. The cost is that `feature/phase-31`'s history permanently contains a fabricated
phase 97 and two worktree merges. **This needs an operator disposition before the branch merges to
`develop`.**

### File-level restoration

| File | Baseline sha256 | Post-run sha256 | Action |
|---|---|---|---|
| `.planning/ROADMAP.md` | `3fbe8d43…16a6b9` | `3fbe8d43…16a6b9` | **unchanged** |
| `.planning/STATE.md` | `a0353fd7…2d4427` | `b478fe1e…cd3845` | **changed — restored** from `626131b`; hash now matches baseline exactly |

What changed in STATE.md before restoration: `current_phase 31→97`, `status executing→completed`,
`stopped_at` clobbered with the stale string `"Phase 31 context gathered"`, `last_activity_desc`
rewritten, the five `progress:` values rewritten (`total_phases 21→48`, `completed_phases 15→20`,
`total_plans 129→144`, `completed_plans 124→137`, `percent 88→42`), and the hand-maintained
`# STALE AND UNVERIFIED` comment block deleted in full.

### Three tool-honesty defects reproduced, all known classes

1. `state.begin-phase` deleted the STALE-AND-UNVERIFIED comment block and rewrote the five progress
   values without reporting either in its `updated[]`. (UPSTREAM-GSD-ISSUES entries 9 and 11.)
2. `roadmap.update-plan-progress 97` and `phase.complete 97` **both reported success**
   (`updated: true` / `roadmap_updated: true`) while ROADMAP.md stayed **byte-identical** — verified
   independently here by hash and by a zero-line `git diff`. This also answers one of the reviews'
   open questions: `roadmap.update-plan-progress` on a phase absent from ROADMAP does not error, it
   silently no-ops and reports success.
3. `phase.complete` clobbered `stopped_at` with a stale value rather than leaving it or writing a
   current one.

### Removed

- `.planning/phases/97-acceptance-two-plan-wave/` — both plans, both SUMMARYs, `97-VERIFICATION.md`,
  and `scratch/` — removed via `git rm -r`, i.e. a deletion commit, not a checkout.
- `.devflow/state-97.json` and every `.devflow/phase-97-*` capture file — confirmed gone by
  `ls .devflow/phase-97-*` returning no matches (not by `git status`, which is blind to them).
- `feature/phase-97` containment branch — `git branch -D` (was `626131b`, no unique commits).
- Executor worktrees and `worktree-agent-*` branches — already removed by `worktree.cleanup-wave`;
  confirmed by an empty `.claude/worktrees/` and `git worktree list` showing only the main checkout.
- `develop` was **not** touched: still at `956f3c2`, identical to `origin/develop`. Nothing was
  pushed.

---

## D-17: no git between launch and the executor's report

**The rule was not violated.** Between `devflow resume --phase 97` (20:32:24Z) and the executor's
report — `.devflow/phase-97-exit` appearing with `0`, plus the `advance_evaluated` event, both at
20:54:52Z — no `git` command of any kind was run by this plan's executor: no `add`, no `commit`, no
`push`, no `checkout`, no branch or tag operation, and no `git status`. Observation during the run was
read-only and non-git: `tail`/`rg` on `.devflow/events.jsonl`, `.devflow/phase-97-stdout`, `ls` on
`.claude/worktrees/`, and `ls` on the phase directory. `devflow advance` was never run to "check on
it" — it blocks on a pending gate and leaks a process pair.

The `stop_until: "code"` cap is what made that window a single, unambiguous interval rather than
three sub-second-separated ones.

---

## What this run does NOT establish

Read this section before the result, not after it.

1. **It does not establish that "the stage reported Success" means anything.** The stage did report
   Success (`advance_evaluated status=success, decided_by_layer=1`). That is recorded as context. The
   completion oracle already scored the orphaned Phase 29 stage as Success, so this signal has a known
   false-positive and carries no weight in the verdict.

2. **It does not establish a count of observed completions.** Constraint 7 makes an observed count the
   very signal that can undercount: the CLI coalesces, and a coalesced pair is superficially
   indistinguishable from one delivered and one lost. The evidence used instead is the *drain to `[]`*
   plus two independent merge commits — state, not counting.

3. **One passing run is a weak reliability bound.** A single success is enough to *refute* "the
   delivery path is broken" and enough to show the mechanism works at least once end-to-end under
   real conditions. It supports approximately nothing about a failure rate: a mechanism failing 1 in
   5 times passes a single trial 80% of the time. A reliability claim would need on the order of
   10–20 consecutive runs, and ideally varied wave widths — 30c's seven-trial set is the precedent
   for what that costs.

4. **It does not isolate the drain arm of constraint 4's close rule as load-bearing.** At both early
   `result` events (lines 476 and 654) the `DEVFLOW_RESULT` marker was *also* absent, so the marker
   arm alone would have prevented an early close in this run. What is proven is that the session
   survived two turn-ends with tasks pending and received both completions. Whether the drain arm
   would catch a case where a marker arrives while tasks are still outstanding remains covered only
   by 31-01's unit tests, not by this run.

5. **It does not establish that the idle timeout is correctly calibrated.** The timeout never fired
   across 22 minutes including `cargo build` and a 22-suite `cargo test`, which is a real data point
   in the 120 s floor's favour. But only **one** `tool_progress` keepalive appeared in 932 capture
   lines — the session was busy enough that inter-line gaps stayed far below 120 s throughout. This
   run therefore never approached the floor and does not test it.

6. **The dispatch was staggered, even though the execution overlapped.** The orchestrator recorded
   dispatching the two executors *sequentially* "avoiding worktree-add lock races", which is why the
   task array went 1 → 2 rather than 0 → 2. Both were concurrently pending at line 456 and the turn
   ended at line 476 with both outstanding, so the concurrency the criterion needs is real. But this
   run does not exercise a truly simultaneous two-agent dispatch, and it does not exercise the
   near-simultaneous-completion coalescing case constraint 7 describes — the two completions arrived
   as separate drains (lines 618 and 657), not coalesced into one.

7. **The Code-stage orchestrator knew it was being measured.** It read `31-05-PLAN.md` early in the
   run and reasoned explicitly about the acceptance criterion, the branching hazard, and what its own
   report did and did not establish. Its behaviour was more careful than a naive run's would be. The
   *mechanism* under test — pipe ownership, stdin lifetime, notification delivery — is not something
   an agent's care can substitute for, so the delivery evidence stands. Its *choices* (declining
   `handle_branching`, dispatching sequentially) are plausibly influenced and should not be read as
   representative of an unobserved run.

8. **Two plans is the narrowest possible wave.** 999.64 was observed at two. Nothing here speaks to
   three or more, where coalescing is likelier and the drain gate does more work.

9. **The delivery latency was not measured**, and the drain→result figure quoted above is a bound
   dominated by DevFlow's post-merge `cargo test` gate, not by anything about notification delivery.

10. **Route B remains unexercised**, so `devflow start`'s behaviour for this workload is still
    unknown beyond the static reading of `commands.rs:177` / `preflight.rs:280-302`.

---

## What each prior plan contributed, as witnessed here

- **31-01** — the pipe-owning monitor and the always-on `stream-json` transport. Witnessed:
  `devflow __monitor --help` exit 0 on the running binary; the spawn line
  `claude -p --input-format stream-json --output-format stream-json --verbose
  --dangerously-skip-permissions`; a 932-line real stream-json capture; and the session surviving two
  turn-ends with background tasks pending.
- **31-02** — the first-class idle-timeout verdict. Witnessed negatively and weakly: no
  `phase-97-idle-timeout` record across a 22-minute stage. See limit 5.
- **31-03** — the declared-token delivery canary. Witnessed: `claude_delivery_canary_confirmed`
  against the real CLI at 2.1.220, ~19 s, persisted to `state.canary`.
- **31-04** — exit-code arbitration and the legacy opt-out. Witnessed: `phase-97-exit` = 0 agreeing
  with a Layer-1 stream success, so the arbitration had no contradiction to resolve — the load-bearing
  premise the reviews flagged as unmeasured (*"whether the real `claude` binary exits 0 after
  `CloseRule` releases stdin"*) **is now measured, and it exits 0.** No
  `claude_legacy_launch_forced` event and no monitor log, confirming the opt-out stayed off.

---

## Post-run repository health

Run after cleanup, on `feature/phase-31` at `62e3a72`. Each command's own exit code was captured
directly — not a pipeline's, per CLAUDE.md.

| Command | Exit | Result |
|---|---|---|
| `cargo test --workspace` | **0** | 22 suites, **876 passed, 0 failed** |
| `bash scripts/check.sh all` | **0** | `==> check.sh: all OK` |
| `git status --porcelain` | — | *(empty)* |

The pass/fail claim rests on two independent measures that agree: the process exit code, and a
summed parse of all 22 `test result:` lines. A single one of those could look clean while being
wrong; both agreeing is the check.

---

*Evidence file for plan 31-05, task 2. The phase-close decision is task 3's, and it is the
operator's (D-19).*

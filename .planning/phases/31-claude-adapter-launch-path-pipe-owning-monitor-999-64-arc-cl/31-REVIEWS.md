# Phase 31 — Adversarial Plan Reviews (31-04, 31-05)

**Run:** 2026-08-03, between wave 2 and wave 3, on `feature/phase-31` at `e9abb0b`.
**Why:** standing operator rule, set this session — *every phase gets an adversarial review after
the phase definition (CONTEXT.md) and after the plan set (PLAN.md), unprompted.* Phase 31 was past
both anchor points with neither run; waves 1–2 were already merged, so the pass was scoped to the
two **unexecuted** plans, where it could still act before the fact rather than produce rework.

**Method:** two independent reviewers, distinct scopes, each briefed to *refute* rather than
confirm, each required to cite `file:line` read in the same turn and to state what it could not
check. Because waves 1–2 were merged, both could read the **landed code** rather than the plans
that promised it — the highest-value difference from an ordinary pre-execution review.

**Result: 8 BLOCKERs.** None would have been caught by `gsd-plan-checker`, the decision-coverage
gate, the post-planning gap analysis, or `verify.plan-structure` — all of which passed this phase
clean. One finding was subsequently **downgraded on evidence** (see 31-04 B3) and one was
**upgraded into a code fix** (see 31-05 B2).

---

## Plan 31-04 — exit-code arbitration and the D-11 opt-out

### B1 — the downgrade would be a no-op at Validate — FIXED
The plan said: on a non-zero exit return `status: Failed` with *"every other field carried over"*.
That includes `verdict`. `classify_validate_outcome` (`pipeline_outcomes.rs:206`) matches
`(_, Some(Verdict::Pass)) => ValidateOutcome::Passed` **first**, `_` discarding status entirely. So
`{"status":"success","verdict":"pass"}` + exit 1 → status `Failed`, verdict `Pass`, Validate
**Passed**, run advances — the task's own success criterion false at the stage that matters most.

31-02 hit this and dodged it deliberately; `idle_timeout_result` sets `verdict: None` with a
comment naming the reason. The plan instructed the opposite.

**Verified independently before acting** (source read + the 31-02 comment). Fixed: `verdict: None`
on the downgraded result. Underlying defect filed as **999.74 / DEN-95**, deliberately not fixed
here — changing that match arm silently re-routes `Failed`, `Unknown`, `ResourceKilled`.

### B2 — "no code path selects legacy automatically" was already false — FIXED by scoping
`relaunch_checkpoint_session` (`pipeline_launch.rs:496`) hardcodes `MonitorLaunch::Legacy` and is
reached by unconditional checkpoint auto-decide. That route also bypasses `canary_gate` and
`claude_stream_launch_enabled` entirely, so the plan's `parse_failure_does_not_trigger_a_fallback`
would have passed green over a live automatic legacy route it never touches.

**Verified independently.** Fixed: the claim is now scoped to *parse-failure-driven* fallback, with
the checkpoint-resume relaunch recorded as a known un-migrated exception rather than silently
counted as covered. Not migrated here — out of scope, and changing it without the acceptance run's
evidence would be the behaviour prediction constraint 1 forbids.

### B3 — "the opt-out is a D-15 guard bypass" — **DOWNGRADED TO A NOTE, the reviewer was wrong**
The reviewer argued that because `canary_gate` short-circuits when the launch is not stream-mode
(`pipeline_launch.rs:171-173`), `--legacy-claude-launch` silently disables the delivery guard.

**Checked, and the premise does not hold.** The canary launches with `--input-format stream-json`,
prompt on stdin. `MonitorLaunch::Legacy` runs the child with **stdin at `/dev/null`**
(`monitor.rs:211-215`); a task-notification arrives as an input turn on stdin, so on that path
there is no channel for one to arrive on. The mechanism the canary tests **structurally does not
exist** in legacy mode. Skipping it is correct scoping, not a bypass — and the existing code
comment already said so.

Two of the three remedies originally proposed to the operator were therefore actively harmful:
running the canary regardless would burn a real agent invocation on a 300s deadline answering a
question the launch never asks.

**The real issue survives, smaller and elsewhere:** the escape hatch drops the operator back to the
path that orphans delegated work — that *is* 999.64. Not a control-flow problem; a messaging one.
Fixed as a required line in the loud notice.

*Recorded because it is the session's clearest case of a well-argued finding with a false premise,
and of the reviewer's own citations containing the refutation.*

### B4 — both of Task 2's verify commands cannot execute — FIXED
`cargo test -p devflow --lib` → `error: no library targets found in package 'devflow'`, exit
**101** (reproduced). The plan corrected the *package* trap in its acceptance criteria while
repeating the *target* trap two lines above. 31-03 had already found and recorded this exact fix.

Fixed to `--bin devflow` and confirmed running (`23 passed; 232 filtered out`). A sweep found two
further live instances in `31-VALIDATION.md`, also fixed. `31-03-PLAN.md` still carries the broken
form and is **deliberately left alone** — it is executed, and its SUMMARY is the record of what
actually ran.

### Warnings applied
- **W1** — blanket `Failed` flattens 137→`ResourceKilled` and 127→`AgentUnavailable`, which route
  to `GateInfra` not `GateReview`; same exit code would reach two different gates. Also, on the
  `PipeOwning` arm a SIGKILLed child records `-1`, not `137`. Both now in the plan.
- **W3** — `files_modified` listed `monitor.rs` (unchanged by the body) and omitted `commands.rs`
  and `OPERATIONS.md` (both required; the `doc_check` gate forces the latter). Corrected.
- **W4** — env-var parsing unspecified; a naive `is_ok()` makes `=false` *enable* the legacy path.
  Now specified, including that a const-mediated read passes `doc_check` **by blindness**.
- **W5** — resume persistence unspecified. Now specified.
- **NOTE** — every `<read_first>` line reference was stale (authored before the wave 1–2 merges).
  All five refreshed against live source and re-verified this session.
- **NOTE** — `contains: "exit"` as an artifact check is vacuous (59 matching lines in that file).
  Replaced with the new function name.

---

## Plan 31-05 — the live acceptance run

### B1 — the run can pass while the mechanism was never engaged — FIXED
D-18's *wording* never drifted: `must_haves` and the threat model both restate it with the two
rejected proxies named. The drift was entirely in **operationalization**. The plan asserted
concurrency as a property of the two plan *files* (wave 1, no deps, disjoint files) and checked
exactly that. **No criterion anywhere required the run to have backgrounded anything** — and
`execute-phase.md` has at least four documented paths that run those same files sequentially,
including the #683 fork-base degrade, which announces itself with one stderr line.

Under any of them both SUMMARY.md files appear, both merge, D-18 passes, and the pipe-owning
monitor's delivery premise was never exercised.

Fixed: a new `verification: backstop` truth requires the capture to show a
`background_tasks_changed` event with a **non-empty** `tasks` array followed by a drain to `[]`,
and makes its absence **VOID, not pass**. `monitor.rs` already parses that shape, so the evidence
is free.

### B2 — the 30s idle floor is miscalibrated — **UPGRADED TO A CODE FIX**
The reviewer flagged that the 30s floor rested on 30d's *backgrounded* 10s/22s sleeps and might
kill a stage that shells out — and was explicit that this was **a risk it could not observe**.

Measured rather than argued. Five workload-controlled trials, two unrelated workload types, CLI
2.1.220: the CLI emits `tool_progress` keepalives on a **fixed 30.00s interval** (±0.02s). Against
a 30s timeout the margin is ~zero, and on the wrong side. The old floor would have killed healthy
Code stages running any tool call over ~30s — including `cargo test --workspace`, which is inside
DevFlow's own post-merge gate.

**Fixed in source, not in the plan:** floor and default raised 30s → 120s (commit `4d8901a`),
D-02 amended with provenance, its "do not correct this upward" instruction withdrawn. Full trials
and limits in `31-IDLE-GAP-MEASUREMENTS.md`. The `(b)` per-run env override this originally implied
is now unnecessary.

*This is the finding that most justifies the standing rule: no automated gate would have found it,
and the phase's own locked decision actively instructed readers not to look.*

### B3 — "and merge" as a marker grep proves nothing — FIXED
Each plan's sole task is "create one file with a fixed string and commit it" — trivially
performable by the orchestrator inline. And `execute-phase.md` **instructs** the orchestrator that
if a completion signal never arrives but commits and SUMMARY.md are visible, treat it as successful
and continue: a documented in-workflow path by which delivery fails completely, a filesystem poll
rescues it, and D-18 passes. D-13's trap 2 applied to the workload instead of the canary.

Fixed: each plan's commit must be reachable **through a merge commit from a distinct branch**, with
both branch names and merge SHAs recorded. Free evidence when delegation happened, unobtainable
when it did not.

### B4 — Route B does not exist — FIXED
`start --phase 97` refuses at `commands.rs:177` → `ensure_phase_reachable_on_base(.., 97, DEVELOP)`
(`preflight.rs:280-302`); phase 97 is on neither ROADMAP nor `develop`. **Verified independently.**
The plan's stated hazard (branch-from-develop) is real but never reached. `commands.rs:167` also
fetches and fast-forwards `develop` *before any executor exists*.

Fixed: Route B marked non-viable, and the obvious repair explicitly forbidden — adding a phase 97
entry to ROADMAP on `develop` would violate T-31-21 and put a fabricated phase in a shared branch's
permanent history. If Route A is blocked: **halt and report.** That is a finding about `devflow`'s
usability, worth more than a worked-around run.

### Warnings applied
- **D-17's boundary was undetectable** — `transition` launches the next stage as its last action,
  so the git-quiet window is Code, sub-second gap, Validate, gap, Ship. Fixed using a correction the
  reviewer surfaced: `resume` no longer clears `stop_until` unconditionally (gated on
  `state.stopped`, pinned by `resume_preserves_unfired_until_cap`, verified passing). Seeding
  `stop_until: "Code"` / `stopped: false` gives one executor and one unambiguous end. *A stale
  memory of mine asserting the old behaviour has been corrected.*
- **Contamination arrives as commits**, not working-tree edits — `execute-phase` commits STATE.md
  and ROADMAP.md per wave and at phase close, so `git status --porcelain` reads clean after a
  contaminating commit. Also `.devflow/.gitignore` is `*`, so capture files can never appear there.
  Both checks replaced.
- **Stale-binary guard was an mtime proxy.** `devflow __monitor --help` succeeds only on a ≥31-01
  binary — a direct discriminator. Note `workflow_started` (carrying `DEVFLOW_BUILD_COMMIT`) is
  emitted by `start` and **not** `resume`, so Route A records no build provenance.
- **A canary refusal is a third outcome class**, not an acceptance failure, and `State.canary`
  persists — a second attempt against the same state file will not re-run it.

---

## What these reviews did not check

- **31-04:** whether the real `claude` binary exits 0 after `CloseRule` releases stdin — the
  arbitration's load-bearing premise, unmeasured. If it exits non-zero, 31-04 downgrades *every*
  successful Code stage to `Failed`. Only 31-05 will reveal it, and it is sequenced after.
- **31-05:** whether `roadmap.update-plan-progress 97` errors or no-ops for a phase absent from
  ROADMAP; the default of `parallelization` when unset (`config-get` cannot distinguish absent from
  false).
- **Both:** waves 1–2 were reviewed only incidentally. Neither reviewer executed anything live.
- **Neither reviewer read `31-01`/`31-02`/`31-03` plans in full** — only their summaries and the
  landed source.

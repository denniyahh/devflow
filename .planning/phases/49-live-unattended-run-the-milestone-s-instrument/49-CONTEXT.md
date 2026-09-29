# Phase 49: Live Unattended Run — The Milestone's Instrument - Context

**Gathered:** 2026-09-24 · **Updated:** 2026-09-28 (re-scope; post-review corrections R-1…R-10)
**Status:** Ready for planning

<domain>
## Phase Boundary

One real `devflow start --mode auto` run **against a disposable GSD project**, recorded fork point through
Ship merge target, plus a same-binary negative-control run on an unconfigured scratch repo. It answers
VERIFY-01 and records the two carried observations: DECN-03's behavioural arm and SURV-01's field arm.

**The live attempt stays out of GSD's plan graph.** GSD plans build and self-test the tools; one plan holds a
single `checkpoint:human-action` in which the operator runs an attempt from a runbook; an ordinary plan judges
the recorded evidence afterwards. Retries happen inside the human step (D-17…D-19).

**Re-scope, 2026-09-28 (operator).** The first plan set (18 plans, 11 waves; commits `a913495`…`2b12d84`)
tried to drive the live run step by step inside GSD's plan graph and to retry it there. Four retry designs and
four external review rounds (`49-REVIEWS.md`, `.planning/reviews/49-adversarial-2026-09-2[57]/`) showed the
defects lived in the workarounds for GSD's run-once model and in the real-repository side effects, not in the
work. The operator re-scoped the target and the execution model; ROADMAP Phase 49's goal, criterion 1 and an
execution note were amended in `968b713`. The superseded plans are design input only (see Claude's Discretion).

**Scope changes planning must carry as plan tasks:**

- Amend REQUIREMENTS.md VERIFY-01's closing sentence ("**Carries a setup step:** this repository has no
  committed `devflow.toml`", `:116`) to match amended criterion 1. **First plan task.**
- Amend ROADMAP criterion 2's "unattended stage progression" to name the two operator-approved preflight
  gates (Define, Code) the run parks at by design (D-22). Carried from the 2026-09-24 context, adjusted: the
  Ship gate no longer blocks (D-23).
- The superseded PLAN.md files must not remain executable: `execute-phase` runs every plan file without a
  SUMMARY. The replan replaces them; git history keeps them.
- 999.55 is **not** promoted (it stays in the backlog; see Deferred).

**Evidence labels.** *Verified* = checked in source or by a command, with file:line. *Reasoned* = derived from
source, not reproduced. Line numbers drift; re-check before citing them in a plan.

**Provenance labels.** *Operator* = chosen by the operator. *Claude (delegated, 2026-09-28)* = the operator
answered "you decide on all" for this discussion's four areas; Claude decided within the operator's standing
decisions and says so where one is changed. *Claude (remit)* = decided within Claude's own remit. Do not treat
a Claude entry as an operator mandate.

</domain>

<decisions>
## Implementation Decisions

### Re-scope and execution model (Operator, 2026-09-28)

- **D-17 (Operator): The run targets a disposable GSD project, not this repository.** It is still a real
  live run: same binary, `--mode auto`, real agent, real GSD, real gates, real Ship. Only the target is
  disposable, which removes the production side effects (a public PR to `main`, a merge into
  `workspace/denniyahh`, the `v3.0.0` bump and tag) and every guard they required.
- **D-18 (Operator): Each attempt is one human-action checkpoint, judged afterwards.** (`checkpoint:human-action`.) The
  operator starts the attempt and answers the gates DevFlow raises; Phase 49's own execution pauses once for this. An
  interrupted session re-asks the same question, which is harmless. No GSD plan tracks the run gate by gate.
- **D-19 (Operator principle): Operate within GSD's current confines.** No workaround that makes GSD behave
  differently: no `status: halted` used as a failure marker, no wrapper plans re-running summarized plans, no
  `<precondition>` used as a branch, no hand-built re-run idempotence, no SUMMARY written before a plan's last
  task. (Memory: operate within GSD's confines; `project-gsd-halt-has-no-exit`.)

### Run payload (Claude, delegated, 2026-09-28)

- **D-20: A synthetic single-task phase in the disposable project**, as Phase 23a chose for the same purpose:
  "the probe is about the supervisor/pipeline mechanism, not about the content of the work"
  (`scripts/scratch-dogfood-repo.sh` header). The task is a small, deterministic code change with a test the
  Validate stage runs, so Code and Validate do real work and can pass. Build it by extending
  `scripts/scratch-dogfood-repo.sh`, keeping its blast-radius guards (repo created outside any checkout,
  absolute-path-prefix refusal, repo-local git identity only). Use the smallest toolchain already on the host;
  research confirms what the Validate stage runs in such a project. *Corrected: see R-9.*
- **D-03 (Operator, re-confirmed; payload adapted by delegation): Seed one `gate="blocking-human"`
  checkpoint** in the payload plan. It asks for a value with no answer derivable from the code (the analogue
  of the original "CI default budget value"), so a "Followed" result can show the comparison reasoning
  `47-PHASE49-OBSERVATION.md` requires.
  - *Verified:* in `Mode::Auto` a plan declaring `blocking-human` makes the unattended-launch condition
    `DoesNotHold` (`crates/devflow-cli/src/preflight.rs:1076-1080`); the check runs at Define and Code
    (`:971-972`). Auto-decide resume fires only for checkpoints in the set recorded when a human approves the
    Code preflight gate (`:1432-1434`). **No fully unattended run can reach DECN-03's path.**
  - **Rejected (unchanged):** pre-writing gate responses before launch; dropping the checkpoint.
- **D-04 (Operator, re-confirmed): Pre-author the payload plan** and commit it in the disposable project
  before launch, so the seeded checkpoint is guaranteed and planning variance stays out of the measurement.
  *Verified:* Define is a no-op in a DevFlow launch and Plan keeps an existing PLAN.md
  (`crates/devflow-core/src/prompt.rs:374-380`, `:341`).
- **D-21: The payload must not classify as a MAJOR bump.** DevFlow's MAJOR-bump gate opens at Ship preflight
  and **cannot be auto-approved** (`crates/devflow-cli/src/preflight.rs:793-848`: it takes no `yes_ship`
  handling). Research confirms what the classifier reads (the comment says the commit range); the scaffold
  and the payload plan make the bump MINOR or PATCH. *Corrected: see R-8.*

### Gates (Claude, delegated, 2026-09-28)

- **D-22: Exactly two human gates per attempt — Define preflight and Code preflight.** They follow from D-03
  with D-04: at Define, "no plans yet" is deferred to Code (`preflight.rs:1068-1072`), but a pre-authored plan
  is already present, so Define gates too. The Define approval is a formality (it confirms the seeded
  checkpoint is intentional); the Code approval records the auto-decide set DECN-03 needs. *Happy path
  only: see R-6.*
- **D-23: Launch with `--yes-ship`. This changes the operator's D-07 (human Ship gate).** `--yes-ship`
  auto-approves the Ship gate once, with attribution (`crates/devflow-cli/src/pipeline_outcomes.rs:839`),
  and Phase 23 introduced it for exactly this purpose. D-07's gate existed to protect the real repository's
  local merge, bump and tag; a throwaway repository has nothing to protect, and criterion 2's merge target is
  observed from the run's events (`merge_result`). D-07's verified finding stands: the Ship gate fires only
  after `/gsd-ship` has pushed and opened the PR (`pipeline_outcomes.rs:828-850`, `ship.md:190-202,370-374`).
  *Corrected: "once" and the `merge_result` observation are wrong; see R-2 and R-3.*
- **Rejected:** zero gates (reverses D-03's reason, "DECN-03 would stay open past the milestone"); one gate
  through an agent-written plan (reverses D-04's reason).

### Throwaway repository and topology (Claude, delegated, 2026-09-28)

- **D-24: A fresh private GitHub repository per attempt.** `/gsd-ship` needs a real remote: it pushes and runs
  `gh pr create` (`~/.claude/gsd-core/workflows/ship.md:194-199`, `:370`). A fresh repository per attempt
  leaves nothing to archive, abort or refuse between attempts. The operator creates it from the runbook under
  their own account (an outward-facing action stays operator-owned); research confirms the `gh` auth scope
  needed. — **Reversibility:** one-way — deleting a throwaway repository destroys its branch and PR history,
  so all evidence must be captured locally first and the operator performs the deletion after Phase 49
  verification. *Other outward effects: see R-10.*
- **D-06 (Operator, adapted by delegation): Keep the two-base split inside the disposable project.**
  `devflow.toml` sets a non-default `base_branch` (for example `integration`), which satisfies amended
  criterion 1; GSD's ship and PR target stays the repository's `main` (`origin/HEAD`). This mirrors DevFlow's
  production topology, and `validate_base_branch` refuses `main` as a base by design
  (`crates/devflow-core/src/config.rs:351`). *Corrected: GSD's PR base is not read from `origin/HEAD`
  first, and `integration` must exist locally; see R-5.*
- **D-25: The negative-control arm stays a local, unconfigured scratch repository** with no remote: it refuses
  at preflight before anything could be pushed. D-14 applies unchanged.
- **D-26: Evidence committed to this repository is sanitized; the throwaway PR is private.** The evidence
  sanitizer design carries over; public-PR secret scanning and containment are no longer needed.

### Failure handling (Operator D-10/D-11 carried; mechanism Claude, delegated, 2026-09-28)

- **D-10 (Operator, unchanged): Blocking defects are fixed in-phase and the run repeated; non-blocking ones go
  to the backlog as 999.x entries.**
- **D-11 (Operator, unchanged): Cap of 3 full live attempts.** Only a run that ends without reaching Ship
  counts; Validate loop-backs are data, not failures.
- **D-27: A stopped attempt is answered at the checkpoint, and GSD's normal resume does the rest.** The
  operator replies `stopped: <what happened>`; the checkpoint plan fails its gate and writes no SUMMARY; a
  plain `/gsd-execute-phase 49` re-presents the checkpoint for the next attempt (GSD resume,
  `execute-phase.md:1453-1457`). No halt marks, no wrapper plans. **Mechanism corrected by R-1**: GSD has no
  `stopped:` reply; the intent above is unchanged.
- **D-28: The fix vehicle for a blocking DevFlow defect.** `/gsd-quick` for a contained fix; an inserted
  decimal phase (`/gsd-phase --insert 49 …`) when the fix needs a design decision or crosses crates. The
  operator confirms the vehicle at the stop. Fixes go through DevFlow's normal tests and pre-push gate on its
  own branches, and the binary is rebuilt before the next attempt (D-12). A cause inside the payload is fixed
  in the disposable project's pre-authored plan, not in DevFlow. *The phase-worktree guard applies: see R-4.*

### Carried unchanged (Claude, remit — from 2026-09-24)

- **D-12: Every attempt rebuilds, and both arms re-run on the new binary.** The binary's identity (git SHA
  plus checksum) is recorded with each arm.
- **D-14: The negative-control fixture must be built to fail, and asserted on text.**
  `scripts/scratch-dogfood-repo.sh` writes `.planning/config.json` (`:187-191`); the negative arm removes and
  commits the removal, asserts on the refusal text ("no `.planning/config.json` under the launch root"), and
  runs with `DEVFLOW_BASE_BRANCH` scrubbed. The positive arm asserts the opposite (config present, source
  `ConfigFile`).
- **D-15: Pin the agent** with `--agent claude` (DECN-03 closes for claude only,
  `pipeline_launch.rs:1777`); record the claude CLI version with each arm.
- **D-16: Fork-point measurement** — three values, now in the disposable project: the base ref's SHA just
  before `start`; the payload worktree's HEAD just after creation (must equal it); and `git merge-base` of the
  feature branch and the base at Ship.

### Post-review corrections (Claude, delegated, 2026-09-28 — adversarial review of `066251f`)

Source: `.planning/reviews/49-adversarial-2026-09-28-context/SUMMARY.md`. Three external lanes reviewed the
decisions, and every finding was re-verified against source. These correct the mechanism or the evidence
behind delegated decisions. No operator decision changes. Where a decision above and this block disagree,
this block wins. The operator approved applying them before planning.

- **R-1 (D-27 mechanism).** GSD has no `stopped:` reply.
  - A `checkpoint:human-action` resumes on `"done"`. A failing `<verify>` makes GSD inform the operator and
    wait (`execute-plan.md:346-348`).
  - Any other reply spawns a continuation agent, and the loop runs until the plan completes or the user stops
    (`execute-phase.md:1066-1073`). An unresolvable checkpoint offers "Abort phase execution?"
    (`execute-phase.md:1451`).
  - Resume restarts the first plan without a SUMMARY at its Task 1 (`execute-phase.md:327,1455`).
  - A plan that has commits but no SUMMARY is an illegal partial state (`execute-plan.md:16-20`).

  Therefore the live-attempt plan holds exactly one task, the `checkpoint:human-action`, and makes no commit
  before the operator's `done`. Its `<verify>` reads the attempt's evidence record, never the reply text. To
  stop an attempt, the operator records the stop in that evidence and stops execution without answering
  `done`. The plan keeps no SUMMARY and no commits, and the next `/gsd-execute-phase 49` restarts it at
  Task 1, which is the checkpoint.
- **R-2 (D-23 and criterion 2 evidence).** `merge_result` carries only `merged` and `branch`
  (`crates/devflow-core/src/hooks.rs:201-202,213-214,231-232`); the target appears only in a log line. The
  Ship merge target is observed from git in the launch checkout, using the merge on `integration` and
  `git merge-base --is-ancestor`, read together with the persisted `State::base_branch`. `merge_result` only
  shows that a merge happened.
- **R-3 (D-23 scope).** `--yes-ship` is never cleared: non-test code only ever sets `state.yes_ship = true`.
  It therefore auto-approves every routine Ship gate in the run, not just one. It cannot answer the
  MAJOR-bump gate or a terminal-hook retry gate.
- **R-4 (D-28 vehicle).** While `feature/phase-49` has a worktree, the pre-commit guard refuses staged
  `crates/*` on any other branch (`scripts/lint-phase-worktree.sh:93-112`). A `/gsd-quick` fix on its own
  branch needs `DEVFLOW_ALLOW_OFF_WORKTREE=1` (`:23,40`), never `--no-verify`. An inserted decimal phase may
  also move `current_phase`, which the guard reads (unverified). The operator still confirms the vehicle at
  the stop.
- **R-5 (D-06 topology).**
  - GSD's PR base is `gsd_run query git.base-branch` (`ship.md:46,373`). It reads the `git.base_branch`
    config key first and falls back to `origin/HEAD`. The scaffold sets it to `main` explicitly.
  - `integration` must be a local branch at `start` (`crates/devflow-cli/src/commands.rs:405`) and must carry
    the payload phase (`:456`).
  - Pushing `integration` is optional; DevFlow fails open for a local-only base
    (`crates/devflow-cli/src/preflight.rs:556-574`).
- **R-6 (D-22 count).** "Exactly two" is the happy path. A preflight failure, the Validate failure ceiling, a
  terminal-hook failure or a MAJOR classification each add a gate. A Validate→Code loop-back likely adds a
  Code gate too, because the checkpoint condition ignores prior approval (`preflight.rs:1067-1083`; reasoned).
  The judging plan records any extra gate as a deviation, with its cause.
- **R-7 (evidence collection, Claude's Discretion).** The auto-decide resume opens no gate
  (`crates/devflow-cli/src/pipeline_launch.rs:1886-1887`), so no gate-open snapshot sees DECN-03's resumed
  turn.
  - Gate snapshots keep the state around each gate.
  - The resumed turn is collected after the run from `.devflow/history/phase-<N>/` and the Claude transcript.
    Set `capture_retention = 64`; the default is 12 (`crates/devflow-core/src/config.rs:36`).
  - No timed runbook step is needed.
- **R-8 (D-21 citation).** The MAJOR-bump gate is `preflight_major_bump_check`
  (`crates/devflow-cli/src/preflight.rs:793-848`). The classifier is in `crates/devflow-core/src/version.rs`.
  With no tags, it classifies all reachable non-merge history, and only `type!:` or a `BREAKING CHANGE:`
  footer yields MAJOR.
- **R-9 (D-20 Validate).** DevFlow runs no tests at Validate. The stage prompts `/gsd-validate-phase`
  (`crates/devflow-core/src/prompt.rs:283-321`), so "Validate does real work" means GSD's validation of the
  payload's own test.
  - The Validate→Ship `DocsUpdate` hook runs `cargo doc`, which fails soft in a non-Rust project
    (`crates/devflow-core/src/hooks.rs:100-104,237-253`).
  - The payload declares no `external_verify`, because external probes run at every stage and would fail
    Define and Plan.
- **R-10 (D-24 outward effects).** Before any deletion, an attempt:
  - pushes the feature branch;
  - opens a PR;
  - pushes a ship-note commit;
  - may push an empty CI commit (`ship.md:190-202,370-374,487-521`).

  GitHub notifications and Actions usage follow. The runbook records these effects and the `gh` account and
  scopes, and it removes DevFlow's per-project registry entry under `~/.cache/devflow`.

### Superseded by the 2026-09-28 re-scope

Full text in git history (`2b12d84:…/49-CONTEXT.md`).

- D-01 (999.55 as the payload) → replaced by D-20. 999.55 stays in the backlog.
- D-02 (payload numbered Phase 49.1) → moot: the payload phase lives in the disposable project.
- D-05 (payload plan and ROADMAP heading on `workspace/denniyahh` before launch) → moot.
- D-07 (human Ship gate, PR to `main` closed unmerged) → replaced by D-23 and D-24.
- D-08 (observe, then undo the version bump, changelog and tag) → moot: they are observed and discarded
  with the throwaway repository.
- D-09 (criterion 1 satisfied by `050ef7f`) → replaced by amended criterion 1 (`968b713`).
- D-13 (this repository's main checkout clean and restored) → moot: the run never touches it; the
  disposable project's launch checkout is created fresh per attempt.

### Claude's Discretion

- **Evidence layout:** event log, persisted state per stage, session captures, gate ledger, binary identity —
  sufficient for `47-PHASE49-OBSERVATION.md`'s Followed / Not followed / Void classification, recording which
  turn carried which rule (`CODE_STAGE_POLICY` in turn 1, `checkpoint_auto_decide_prompt` in the resumed turn).
  The gate watcher's snapshot at each gate open should capture everything DECN-03 needs; research confirms,
  because a capture that must happen while the run is parked is otherwise a timed runbook step. *Answered:
  a gate snapshot cannot see the resumed turn; see R-7.*
- **SURV-01 detection method.** Recorded as an observation either way; "no interleaving observed" must not be
  written up as confirming anything.
- **Runbook form and supervision.** Owned outside any agent worktree or subagent turn (memory: long probes live
  outside agent worktrees). It names the exact approve and reject commands and how the operator learns a gate
  is waiting (`DEVFLOW_GATE_NOTIFY_CMD` is a no-op unless set, `crates/devflow-core/src/gates.rs:388-394`; the
  production gate timeout defaults to 3 days).
- **Where the disposable project and the negative-control repository live on disk** (outside every checkout).
- **Reuse of the superseded tool designs:** launch helper, gate helper, gate watcher and snapshot, SURV watcher,
  evidence checker, negative-control driver and evidence sanitizer (in the 49-02…49-08 and 49-16 plans at
  `2b12d84`). Not needed: accommodations, the post-Ship undo, PR containment, the landing hop, and archive or
  abort tooling for reused state.
- **Gate logic lives in tested scripts** with selftests and negative controls, not in long one-line
  `bash -c` commands inside PLAN.md; adversarial review is scoped to the runbook and any irreversible step.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and carried questions
- `.planning/ROADMAP.md` § "Phase 49: Live Unattended Run" — goal, criteria 1-5 and the execution note
  (amended `968b713`).
- `.planning/REQUIREMENTS.md` VERIFY-01 (`:107-116`) — the requirement; its closing sentence is amended by
  this phase.
- `.planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md` — DECN-03's evidence
  standard (Followed / Not followed / Void), claude-only scope, position-effect recording.
- `.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-CONTEXT.md` D-01..D-05 — writer rule
  and lock identity for the SURV-01 observation.

### Why the model changed
- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-REVIEWS.md` — two review rounds and
  their verified dispositions, including why record-and-continue failed.
- `.planning/reviews/49-adversarial-2026-09-27/SUMMARY.md` — round-3 findings against the lockstep plans.
- `~/.claude/gsd-core/workflows/execute-phase.md:327`, `:1453-1457`; `workflows/execute-plan.md:228-238`;
  `references/planner-preconditions.md:115-124` — the GSD behaviour D-18/D-19/D-27 rely on.

### Disposable project and dogfood precedent
- `scripts/scratch-dogfood-repo.sh` — scaffold, blast-radius guards, pre-authored-plan pattern, negative-control
  fixture (`:187-191` writes `.planning/config.json`).
- `.planning/milestones/v2.0.0-phases/23-end-to-end-dogfood/23-CONTEXT.md` — the prior end-to-end dogfood (23a scratch-repo
  probe, `--yes-ship` introduced for unattended Ship).
- `scripts/unattended-drill.sh` header — why Plan can never auto-approve; two-arm drill with a negative control.

### DevFlow behaviour the run exercises
- `crates/devflow-cli/src/preflight.rs` — unattended-launch check stages (`:963-972`), plan-condition logic
  (`:1060-1085`), auto-decide set recording (`:1432-1434`), MAJOR-bump gate (`:793-848`).
- `crates/devflow-core/src/version.rs` — the bump classifier the MAJOR-bump gate reads (R-8).
- `scripts/lint-phase-worktree.sh` — the phase-worktree commit guard and its escape hatch (R-4).
- `.planning/reviews/49-adversarial-2026-09-28-context/SUMMARY.md` — the review behind R-1…R-10, with
  every finding re-verified and the rejected ones listed.
- `crates/devflow-cli/src/pipeline_outcomes.rs` — Ship outcome handling (`:828-850`), `--yes-ship`
  auto-approval (`:839`).
- `crates/devflow-core/src/config.rs` — `base_branch` doc, `validate_base_branch` (`:341-351`).
- `crates/devflow-core/src/hooks.rs` — `hooks_after_ship` (`:115`), `merge_feature` (`:180`).
- `crates/devflow-core/src/gates.rs` — gate files (`:115-145`), notification (`:388-394`).
- `~/.claude/gsd-core/workflows/ship.md` — push (`:194-199`) and `gh pr create` (`:370`).
- `crates/devflow-cli/src/main.rs` — `start` flags `--mode`, `--until`, `--yes-ship`, `--agent`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/scratch-dogfood-repo.sh`: builds a legal `devflow start` target with a pre-written plan outside any
  checkout; the base for both the disposable project (extended) and the negative-control arm.
- `scripts/unattended-drill.sh`: the established two-arm live drill pattern with a negative control.
- `devflow_core::events` (`merge_result`, `checkpoint_auto_decided`, `advance_failed`): the run's primary
  evidence stream.
- The superseded plans' tool designs (see Claude's Discretion) — specifications, not code; nothing was built.

### Established Patterns
- **Negative controls must fail for the right reason:** assert on the intended refusal text, not the exit
  code.
- **The event alone is not proof.** `checkpoint_auto_decided` is emitted before the spawn; classify as Void
  unless the resumed session capture shows the resolution (`47-PHASE49-OBSERVATION.md`).
- **A SUMMARY file is GSD's completion marker** — never written early (D-19).

### Integration Points
- **Validate in the disposable project runs the payload's own tests,** not DevFlow's `phase7_cli` suite, so
  999.141's cache and monitor pollution from DevFlow's E2E tests does not arise during the run (reasoned). The
  live run still writes DevFlow's registry under `~/.cache/devflow` for its own root.
- **`gh` authentication** for private repository creation and `gh pr create` runs under the operator's
  account; the runbook records the account and scope used (research confirms the required scope).

</code_context>

<specifics>
## Specific Ideas

- Operator, 2026-09-28: "we must operate within the confines of what it currently supports and not force it to
  behave differently via workarounds as a principle" (D-19).
- Operator, 2026-09-28: a throwaway project lowers the stakes; it is still an actual run (D-17).
- Operator, 2026-09-28, on gates: the original plan asked for a single gate (the 2026-09-24 D-07 Ship gate);
  the two preflight gates are the cost of exercising DECN-03 with a pre-authored plan (D-22).

</specifics>

<deferred>
## Deferred Ideas

- **999.55: configurable test-wait budget for the `phase7_cli.rs` helpers.** No longer this run's payload.
  Stays in the backlog as its own future phase; measuring its effect on the flake rate needs CI history after
  its merge.
- **999.123: de-duplicate CI triggers.** Unchanged; stays in the backlog.
- **Making DevFlow merge phase work into `main`.** Unchanged; its own phase if ever wanted (D-06).
- **GSD support for re-running plans, task-level resume and loops.** Outside DevFlow: the operator is taking a
  proposal to the GSD maintainers (`~/Documents/gsd-rerunnable-plans-proposal.md`). Nothing here depends on it.

</deferred>

---

*Phase: 49-live-unattended-run-the-milestone-s-instrument*
*Context gathered: 2026-09-24 · updated for the re-scope: 2026-09-28*

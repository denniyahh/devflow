# DevFlow Roadmap

> Phase plan source of truth. Each phase drives a `devflow start` agent session.

## 🚧 v2.5.0 milestone (Loop-Termination and Release Hardening, ACTIVE — declared 2026-08-06)

**Declared 2026-08-06, the same day v2.4.0 released.** Closes five confirmed defects/gaps that
surfaced from v2.4.0's own release, plus one investigation into a concurrency blind spot the
safety net v2.4.0 just widened. All six are pre-existing issues found via code review, live
release cuts, and Phase 34's own capture campaign — none are new regressions from v2.4.0. See
`.planning/REQUIREMENTS.md` (HARDEN-01..06) and `.planning/PROJECT.md` § "Current Milestone."

**Why two phases, not one or six.** Phase 35 bundles five items that are confirmed live defects
or already-decided fixes, small-to-medium in size, safe to land together in one phase: **999.77**
(a transient `git` failure grants a free `consecutive_failures` reset, contradicting the doc
comment), **999.78** (the Code↔Validate loop has no progress-independent bound, and the
Supervise-mode gate message reports a resettable streak instead of a cumulative total),
**999.79** (`{N}-VERIFICATION.md` never goes stale, so `--force` inherits a prior run's verdict
and gates unresolvably), **999.84** (the `GateReview` checkpoint call site's root argument is
correct by construction but has no regression test), and **999.86** (`release --check`'s
tag-signing predictor has now false-negatived live twice with the correct key present; replace it
with a real `ssh-keygen -Y sign` probe). Phase 36 carries **999.83** alone — the drain gate has
never observed real sub-agent concurrency, and its fixture's shape doesn't match what production
actually emits. That item is investigation-shaped (design the right experiment first, same family
as backlog 999.71's precedent) rather than a quick patch, and bundling it with Phase 35 would slow
those five confirmed fixes down waiting on a harness.

| Phase | Name | Status | Version |
|---|---|---|---|
| 35 | Loop-Termination and Baseline Correctness | Not started | — |
| 36 | Drain Gate Concurrency Measurement | Not started | — |

### Phase 35: Loop-Termination and Baseline Correctness (999.77 + 999.78 + 999.79 + 999.84 + 999.86)

**Goal**: Operator can trust the Code↔Validate loop's failure-gating mechanics and the release
signing preflight behave as documented and are enforced by regression tests, not by
correctness-by-construction alone — a transient `git` failure can no longer forge a fresh
baseline, the loop has a bound independent of trivial per-cycle commits, a `--force` re-run
doesn't inherit a stale verdict, the worktree-mode checkpoint call site is regression-tested, and
`release --check`'s signing result reflects a real probe rather than a predictor that has already
false-negatived live twice.
**Depends on**: Nothing (first phase of this milestone).
**Requirements**: HARDEN-01, HARDEN-02, HARDEN-03, HARDEN-04, HARDEN-05
**Success Criteria** (what must be TRUE):

  1. A transient `git` failure while measuring `phase_commit_count` no longer overwrites the
     persisted `consecutive_failures` baseline with a false zero — the next successful
     measurement compares against the last real observed count, "could not count" is
     distinguished from "counted zero," and `pipeline_outcomes.rs`'s doc comment no longer
     promises a guarantee the code doesn't have (999.77).

  2. An unattended Code↔Validate loop that commits trivial `.planning/` artifacts every cycle
     without making real progress still reaches a bound distinguishable from ordinary streak
     resets — a never-reset per-phase Validate-failure total exists — and in Supervise mode the
     gate message reports that cumulative total rather than a streak length that can read
     misleadingly low at the 2nd, 5th, and 9th gate alike (999.78).

  3. Running `devflow start --phase N --force` against a phase with a stale `{N}-VERIFICATION.md`
     from a previous run no longer inherits that verdict — staleness is detected (via a recorded
     plan-count comparison or equivalent) so the loop-back treats the phase as mid-arc rather than
     dispatching `--gaps-only` against zero matching plans and gating unresolvably (999.79).

  4. The worktree-mode `GateReview` checkpoint auto-decide call site
     (`pipeline_launch.rs:1070`, passing `execution_root` into
     `phase_has_blocking_human_checkpoint`) is covered by an integration test that fails when the
     argument is reverted to `project_root` — demonstrated by actually performing that revert and
     watching the new test fail, with its negative control recorded alongside it, not asserted
     from reading the fix (999.84).

  5. `release --check`'s tag-signing preflight reports `Viable`/`NotViable` from a real
     `ssh-keygen -Y sign` probe against a throwaway payload, not from an `ssh-add -l`
     fingerprint comparison — closing the predictor that has produced a live false negative with
     the correct key present on two separate release cuts (999.86).
**Plans**: TBD

### Phase 36: Drain Gate Concurrency Measurement (999.83)

**Goal**: Operator has a real, evidence-based answer to whether the drain gate — the safety net
the v2.4.0 stream-json widening depends on — actually observes real sub-agent concurrency, in
place of the untested assumption its synthetic fixture currently encodes. This is
investigation-shaped work: design and run the right experiment, in the same family as backlog
999.71's precedent, rather than apply a predetermined fix.
**Depends on**: Nothing structurally — sequenced after Phase 35 only by phase numbering.
Deliberately isolated so this investigation does not block Phase 35's five confirmed fixes; it may
be planned and executed independently of Phase 35's completion.
**Requirements**: HARDEN-06
**Success Criteria** (what must be TRUE):

  1. The event family (or families) Claude CLI actually emits for each kind of concurrent child
     work — sub-agent dispatch, backgrounded shell, or other — is established from live
     production evidence beyond the single n=1 capture already on record (`34-evidence/`), not
     inferred a second time from source reading alone.

  2. The question "does the drain gate see real sub-agent concurrency" is answered honestly per
     observed condition (CLI version, workload shape) — including recording a negative or
     inconclusive result exactly as measured, rather than assuming the answer resolves in the
     direction of "the gate already works."

  3. One of two outcomes is delivered, not deferred: either `CloseRule::observe` is widened to
     the event families production actually emits, or an in-source explanation records why
     `background_tasks_changed` remains the right and sufficient key — and the SYNTHETIC fixture
     label is corrected to match whatever the measurement found.

  4. The measurement's own scope and strength are stated explicitly in its record — sample size,
     CLI version(s), workload shape(s) covered, and what conclusion that evidence can and cannot
     support — so a future reader cannot mistake a narrow result for a general guarantee about the
     drain gate.
**Plans**: TBD

## Progress

**Scope note:** this table is global (spans every shipped milestone plus the active one), per
`deriveProgressFromRoadmap`'s own design (it reads the whole file, not a milestone-scoped
slice). Phases 26 (closed partial, never shipped) and 29 (aborted, never merged) are excluded —
neither belongs to any shipped milestone (see `PROJECT.md` § "Active" and
`.planning/superseded/26-release-cut-automation/`). `999.x` backlog entries are excluded per
`deriveProgressFromRoadmap`'s own `/^999(?:\.|$)/` filter. `Plans Complete` reads "—" where this
table's author did not have a directly-verified per-phase count on hand at authoring time
(2026-08-04); those cells don't affect the derived phase-completion counts this Progress table
exists to fix, only the (unused-by-HYGIENE-03) plans-total figure.

| Phase | Plans Complete | Status | Completed |
|---|---|---|---|
| 1 | 1/1 | Complete | — |
| 2 | 1/1 | Complete | — |
| 3 | 1/1 | Complete | — |
| 4 | 1/1 | Complete | — |
| 5 | 1/1 | Complete | — |
| 6 | 1/1 | Complete | — |
| 7 | 1/1 | Complete | — |
| 8 | 1/1 | Complete | — |
| 9 | 1/1 | Complete | — |
| 10 | 1/1 | Complete | — |
| 11 | 2/2 | Complete | — |
| 12 | — | Complete | — |
| 13 | — | Complete | — |
| 14 | — | Complete | — |
| 15 | — | Complete | — |
| 16 | — | Complete | — |
| 17 | — | Complete | — |
| 18 | 7/7 | Complete | — |
| 19 | 11/11 | Complete | — |
| 20 | 5/5 | Complete | — |
| 21 | 4/4 | Complete | — |
| 22 | 2/2 | Complete | — |
| 23 | — | Complete | — |
| 24 | — | Complete | — |
| 25 | 18/19 | Complete | — |
| 27 | 6/6 | Complete | — |
| 28 | — | Complete | — |
| 30 | 5/5 | Complete | — |
| 31 | 5/5 | Complete | — |
| 32 | 0/0 | Complete    | 2026-08-04 |
| 33 | 6/6 | Complete    | 2026-08-05 |
| 34 | 6/6 | Complete    | 2026-08-06 |
| 35 | — | Not started | — |
| 36 | — | Not started | — |

## v2.4.0 milestone (CLOSED 2026-08-06 — Resume Unattended Dogfooding)

**Declared 2026-08-04, closed 2026-08-06.** Closed the structural defects blocking unattended,
multi-wave `devflow start` runs: the Code↔Validate loop no longer false-gates on healthy work,
Validate's reported outcome reflects derived status rather than the agent's self-report, and Layer 0
verification is no longer inert in worktree mode. All four items were pre-existing defects found
during the Phase 29 dogfood run and Phase 31 planning. Full detail archived to
`.planning/milestones/v2.4.0-ROADMAP.md`; requirements to
`.planning/milestones/v2.4.0-REQUIREMENTS.md`; phase directories to
`.planning/milestones/v2.4.0-phases/`.

**Closed as `override_closeout`, not `verified_closeout`.** Both phases verified and the pre-close
artifact audit was clear, but DOGFOOD-04's traceability row remains `Pending` — its core guarantee
is closed by Phase 34 criteria 3 and 4, while 999.76's second call site is correct by construction
with no regression guard (999.84 / DEN-106). No `/gsd-audit-milestone` was run. See
`.planning/MILESTONES.md` § Known Gaps.

**Not released.** This close is a planning-state operation. At close the workspace version was still
`2.3.0`, `CHANGELOG.md` had no 2.4.0 section, the work sat unmerged on `feature/phase-34`, and no
`v2.4.0` tag existed.

| Phase | Name | Status | Version |
|---|---|---|---|
| 33 | Loop-Back Correctness for Multi-Wave Validate→Code Cycles (999.65 + 999.66) | Complete | — |
| 34 | Stream-JSON Coverage, the Validate Trust Boundary, and Layer 0 in Worktree Mode (999.73 + 999.74 + 999.76) | Complete | — |

## gsd-hygiene milestone (CLOSED 2026-08-04 — GSD Workflow Hygiene)

**Declared 2026-08-04, closed 2026-08-04. Intentionally unversioned** — no `vX.Y.Z`, nothing
published to crates.io; pure `.planning/` documentation structure. Restructured this file so the
active milestone's own phase headings land inside its own heading-to-next-heading window,
closing backlog `999.72` (`roadmap.analyze` misreporting `phase_count: 0`; `milestone.complete
--dry-run` triggering its pass-all degrade) and `999.72a` (the missing `## Progress` table
above). Full detail archived to `.planning/milestones/gsd-hygiene-ROADMAP.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 32 | ROADMAP Layout Hygiene | Complete | — |

## v2.3.0 milestone (CLOSED 2026-08-04 — the unattended run)

**Declared 2026-08-02, closed 2026-08-04.** Deliberately **bounded**: this milestone closed when
999.64 was delivered — Phase 30 (the parser and the feasibility gate) plus Phase 31 (the launch
path itself). Full detail archived to `.planning/milestones/v2.3.0-ROADMAP.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 30 | Keep the Session Alive Past Turn End | Complete    | — |
| 31 | The Launch Path Itself | Complete    | 2.3.0 |

## v2.0.0 milestone (CLOSED 2026-08-02 — spanned releases 2.0.0, 2.1.0, 2.2.0)

**Closed 2026-08-02.** The label named an open-ended milestone rather than a bounded arc
(see "Milestone stays open" below), so it never had a scheduled endpoint and continued past
the 2.0.0 release through 2.1.0 and 2.2.0. Closed by operator decision once Phase 29 aborted
and Phase 30 was rescoped to the 999.64 arc, which is a distinct unit of work. Full detail
archived to `.planning/milestones/v2.0.0-ROADMAP.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 12 | Bootstrap + Housekeeping | Complete | — |
| 13 | MVP Core Loop | Complete    | — |
| 14 | Parallel Safety + Observability | Complete | — |
| 15 | Dogfood Enablement + OSS Readiness | Complete | — |
| 16 | Pipeline Reliability Hardening | Complete    | — |
| 17 | Pipeline Dogfood Follow-Up | Complete    | — |
| 18 | Dogfood Reliability Hardening | Complete    | 1.5.0 |
| 19 | Release Integrity + `main.rs` Decomposition | Complete    | 1.6.0 |
| 20 | Release Correctness + Operator Control | Complete    | 1.7.0 |
| 21 | Operator Legibility & Observability | Complete    | 1.8.0 |
| 22 | Concurrency & Governance Correctness | Complete    | 1.8.1 |
| 23 | End-to-End Dogfood — One Phase, Define→Ship, Unattended, With Claude | Complete    | 2.0.0 |
| 24 | `release --check` Signing-Key Inline Classification | Complete    | 2.0.0 |
| 25 | End-to-End Dogfood Blockers — Start, Progress, Finish, Recover | Complete    | 2.1.0 |
| 27 | Scrub Redirecting Git Environment From Production Calls | Complete    | 2.2.0 |
| 28 | Close the Checkpoint Answer Return Path | Complete    | 2.2.0 |

**Not in this milestone** — Phase 26 (CLOSED PARTIAL, not shipped) moved to
`.planning/superseded/26-release-cut-automation/`; Phase 29 (ABORTED, never merged,
no directory) stays as prose-only history below.

## v1.0 milestone (CLOSED, retroactively labeled 2026-08-04 — never declared at the time)

**No milestone concept existed for this era.** Phases 1-11 shipped individually as versions
0.1.0 through ~1.2.0 before this project adopted "milestone" as a grouping concept at all
(v2.0.0 was the first, declared 2026-07-23). This label was applied retroactively by operator
decision on 2026-08-04, alongside the v2.0.0 archival, rather than leaving this era permanently
un-archived. Full detail archived to `.planning/milestones/v1.0-ROADMAP.md` (see that file for
why this differs from the pre-existing `v1.0-ASSESSMENT.md`, an unrelated older artifact).

| Phase | Name | Version |
|---|---|---|
| 1–5 | Core workflow, versioning, state machine | 0.1.0–0.6.0 |
| 6 | Agent Completion Protocol | 1.0.0 |
| 7 | Worktrees & PR Integration | 1.0.0 |
| 8 | Docs & Onboarding | 1.0.1 |
| 9 | Open-Source Polish | 1.2.0 |
| 10 | Logging + Planning Step | — |
| 11 | GSD-Native Architecture + Remediation | 1.2.0 |

## Reorganized (June 2026)

- **Conventional commits deprecated for versioning (June 2026), ban lifted 2026-07-27** — this bullet originally recorded a bare prohibition on deriving versions from commit messages, with no rationale, incident, or evidence attached anywhere in this file. The operator explicitly authorised deviating from that policy on 2026-07-27 to fully automate versioning (CONTEXT.md D-06); Phase 25 (25c) implements the superseding scheme — baseline from the highest reachable semver tag plus conventional-commit classification of the commits since that baseline (`crates/devflow-core/src/version.rs`)
- **Phase 10 shipped** — logging + Planning step (Planning known bug, addressed in Phase 11 refactor)
- **Phase 11 recast** — full architecture refactor to GSD-native execution engine
- **Phase 12** — Bootstrap (new-project, map-codebase) + versioning automation + publish `devflow` to crates.io (name confirmed available, 2026-07-08)
- **Phase 13** — OSS readiness (dev container, contributing, CI) + Hermes plugin + Hermes/Antigravity adapters
- **Phase 14** — reliability + observability hardening, scoped from external code review feedback (2026-07-08)

## Reorganized for MVP (2026-07-14)

- **Phase 13 repurposed as MVP Core Loop** — priority is getting Define→Plan→Code→Validate→Ship working end-to-end unattended (Claude + Codex, gates via notify hook) so DevFlow can be dogfooded on real projects again. Claims the previously unclaimed `ship.rs` GSD-native rewrite; absorbs the reliability items from old Phase 14 (verdict-vs-ran, native envelope parsing, WR-11, notify hook, gate timeout, worktree default).
- **Phase 14 rescoped to Observability + Hermes Support** — residual `devflow logs`/`events.jsonl`/`status` work plus the previously unclaimed `capture_agent_output()` sync-path decision (now claimed there). Hermes work (agent adapter, skill-file rewrite, plugin) moved in from Phase 15 (2026-07-14) — the plugin's gate watcher consumes this phase's `events.jsonl`, so they ship together.
- **Phase 15 (was 13)** — OSS readiness (docs, dev container, contributing, Antigravity adapter) plus the actual crates.io publish. Hermes items moved out to Phase 14.

## Phase 17 scoping (2026-07-18)

- **Phase 17 narrowed to four units** after source verification resolved the spike's decision gate: `Unknown` non-advance (17a), typed outcomes + retry policy (17b), preflight readiness gate (17c), build provenance (17d). Scoped as a focused repair phase rather than a Phase 16 remediation — only 17d traces to the proven Phase 16 defect.
- **Phase 18 gains 18d/18e** — `devflow doctor` state/event reconciliation and the WR-03 transient-capture test fix moved out of 17. 18d depends on 17b + 17d. See the 2026-07-18 decision entry in STATE.md.

## Phase 19 scoping (2026-07-21)

- **Milestone label corrected.** `v2.0.0` had been carried as the milestone name since Phase 11 while the project actually shipped 1.2.0 → 1.5.0. No v2.0.0 was ever released. The milestone now runs Phase 11–20 and genuinely closes at v2.0.0.
- **Phase 19 = four promoted backlog items**, in sequence: 999.10 (`.devflow/` artifact hygiene, Urgent/S), 999.11 (`commit_path` empty commits, High/S), 999.8 (split `main.rs`, High/L), 999.16 (AI change acceptance contract, High/M, parallel track). Promoted via `/gsd-review-backlog`; all four source claims re-verified present at HEAD during promotion.
- **Cuts v1.6.0, not v2.0.0.** Nothing in the phase is breaking and — apart from the PII fix — almost nothing is user-visible. Tagging a pure-move refactor as a major release would oversell the changeset and burn the 2.0 slot.
- **999.8 is near-alone by necessity.** It conflicts with every other high-priority candidate: 999.6 (`--until`), 999.7 (manual ship override) and 999.3 (`gate show`) all land in `main.rs`. Every phase run before the split makes the split harder and re-pays the serialization tax — Phase 18 burned 6 near-serial waves on 7 plans for exactly this reason, and the file has grown +35% (6,239 → 8,467 lines) since that was logged.
- **Phase 20 gets the deferred set** — 999.6, 999.7, 999.13, likely 999.3 — and is what the split makes plannable as one phase in ~3 waves rather than two phases at 6.

## Phase 20 scoping (2026-07-22)

- **Phase 20 = five promoted backlog items**, in sequence: 999.24 (`VersionBump` workspace self-pins, High/S), 999.23 (`phase7_cli.rs` git-fixture reliability, High/M), 999.6 (`--until` plan-only mode, High/M), 999.13 (release-cut preflight, High/L), 999.7 (manual ship override, High/L). Promoted via `/gsd-review-backlog`; all five source claims re-verified open at HEAD (`8ecbdf9`) during promotion.
- **999.23 re-sized S → M during promotion.** The ROADMAP entry described one flaky test (`reference_and_cleanup_worktree_cli_flow`, worktree removal race). DEN-48 had since been broadened — a second, unrelated flake in the same file (`start_worktree_mode_ignores_main_checkout_divergence`, git object-store corruption on run `29946629986`) reframes the item as a structural weakness in how `phase7_cli.rs`'s fixtures drive real `git` under CI concurrency. Two distinct root causes, and instance 1 likely has a product-side component.
- **999.3 deliberately left in backlog.** The Phase 19 note reserved it for Phase 20 "likely", but it is the only Low-priority item in that set and it bundles four distinct UX gaps (`gate show`, rate-limit reset surfacing, in-stage `status` progress, recovery-verb discoverability). Split it before promoting rather than carrying the largest lowest-value unit in a phase already holding two L-sized items.
- **Two release defects promoted ahead of the operator features.** 999.24 (S) has shipped broken two for two (v1.5.0 patched by `7ad260c`, v1.6.0 by PR #15) and is a *product* bug — any user with a published Cargo workspace hits it identically. 999.23 (M) sits in the release gate, and a coin-flip test trains the reader to re-run red CI instead of investigating it. Both make this phase's own release cut trustworthy, which is why they lead.
- **999.13 blocks on 999.24.** Its highest-value check is the workspace self-pin invariant; it must assert against 999.24's fix rather than encode today's manual patch as the expected state.
- **v2.0.0 is not yet earned.** The milestone reserves 2.0.0 for this phase, but nothing in the five units is inherently breaking, and Phase 19 already declined to burn the 2.0 slot on a non-breaking changeset. Decide at ship time: either the phase earns a breaking change or the milestone closes at 1.7.0 and the slot stays unspent.

## Milestone stays open (2026-07-23) — SUPERSEDED 2026-08-02

> **Superseded.** The open-ended framing recorded below governed v2.0.0 from 2026-07-23 until
> 2026-08-02, when the milestone was closed and the bounded v2.3.0 milestone was declared above.
> The lesson is recorded rather than the decision reversed silently: an open milestone with no
> closing condition drifted three releases and left `.planning/STATE.md` advertising a merged PR
> as open. v2.3.0 carries an explicit closing condition for that reason. The notes below remain
> accurate as history for v2.0.0.

- **Decided at Phase 20 ship time:** ships as **v1.7.0**, not v2.0.0 — nothing across the five units is breaking, consistent with Phase 19's earlier call not to spend the 2.0 slot on a non-breaking changeset.
- **The v2.0.0 milestone does NOT close at Phase 20 or at any other fixed phase.** Earlier notes above ("the milestone now runs Phase 11–20 and genuinely closes at v2.0.0," "the v2.0.0 milestone closes at Phase 20," "the milestone reserves 2.0.0 for this phase") described a *bounded* Phase 11–20 arc culminating in a 2.0.0 release. That framing is superseded: the milestone continues past Phase 20 with no predetermined phase count or closing version — 2.0.0 remains an eventual aspiration, not a scheduled endpoint. Future phases keep numbering forward (21, 22, …) under the same open milestone until a genuinely breaking change actually earns the 2.0 slot; `/gsd-complete-milestone` is not run at Phase 20.
- Table above renamed from "v2.0.0 (Phase 11–20)" to reflect this — the phase list is historical (what's shipped so far), not a closing boundary.

- **Phase 14 rescoped to Parallel Safety + Observability** — the 2026-07-14 move of Hermes into Phase 14 was a workload-balance call made before the CR-03 parallel-safety flaw was deferred there (2026-07-15), which made 14 the heaviest phase instead of the slimmest. Phase 14 now leads with CR-03 (per-phase state files, phase-threaded monitor advance, coarse lock for main-checkout mutations), keeps the `capture_agent_output()` sync-path decision, and builds observability (`logs`/`events.jsonl`/`status`) on the final per-phase state model — in that order, since the state-file shape dictates what `status`/`logs`/`events.jsonl` enumerate.
- **Phase 16 (new): Hermes Support** — HermesAgent adapter, skill-file rewrite, and Hermes plugin moved out of 14. Depends on Phase 14 (the plugin's gate watcher consumes `events.jsonl` and the Phase 13 notify hook); sits after Phase 15 so public-facing OSS readiness isn't gated on personal-infrastructure work.

## Backlog

Unsequenced items — not part of the active phase sequence. Promote with
`/gsd-review-backlog` when ready; each carries accumulated context in its
own `phases/999.N-*/CONTEXT.md`.

### Phase 999.87: `evaluate_layer2` Reads an Unrunnable `git` as "No Work Done", Misclassifying a Successful Agent as `Failed` (BACKLOG)

**Linear:** [DEN-108](https://linear.app/denniskim/issue/DEN-108/99987-evaluate-layer2-reads-an-unrunnable-git-as-no-work-done)
**Found:** 2026-08-06, while reasoning through Phase 35's D-08 decision (change
`phase_commit_count`'s return type, or add a sibling). **Not** found by any of the four adversarial
review lanes run against Phase 35's CONTEXT.md that day — it surfaced from asking what the *other*
consumer does with the same lossy return value, which no lane thought to ask.

**Priority:** Medium | **Size:** S — one branch plus its test, on top of Phase 35's `Option<u32>`
plumbing.

**Severity: Medium.** Same root cause as 999.77, **worse consequence.** 999.77 weakens a *bound*;
this produces a **wrong classification** — a successful agent run reported as failed and looped
back. Needs a transient fault to trigger, which is why it is not High.

**Not a duplicate — checked before filing.** 999.77/DEN-99 shares the root cause but is scoped to
the `consecutive_failures` baseline write in `handle_validate_outcome`, never mentions
`evaluate_layer2`'s classification, and its proposed fix leaves this call site untouched. 999.81's
IN-03 cites `agent_result.rs:1904`, one line above, but concerns duplicated branch-name derivation.

**The defect.** `phase_commit_count` (`crates/devflow-core/src/agent_result.rs:1841`) returns `0`
indistinguishably for three causes — genuinely no commits, branch absent, or `git` could not run.
`evaluate_layer2` (`agent_result.rs:1905`) then computes
`no_work_done = commit_gated && commits == 0` for `Plan`/`Code` stages and routes
`exit_code != 0 || no_work_done` to `AgentStatus::Failed`. So a momentary `git` failure makes an
agent that **exited 0 having committed real work** read as `Failed`, with a reason string naming a
commit count that was never measured.

**Why the bad combination is reachable:** the exit code is read from `.devflow/phase-NN-exit`, not
from git, so the two signals fail independently — a broken `git` does not prevent `exit_code == 0`
from being true.

**Relationship to Phase 35 — this is a follow-up, not independent work.** D-08 changes
`phase_commit_count` to return `Option<u32>`, which forces this call site to be confronted by the
compiler. Phase 35 deliberately maps `None` to today's zero-treatment **explicitly, with a
comment**, rather than widening its own scope (34/D-04). This entry is exactly "revisit that
mapping", and **should not be attempted before Phase 35 lands** — the `Option` plumbing is the
prerequisite.

**Proposed fix.** Decide what Layer 2 does when the count is `None` (could not measure) as distinct
from `Some(0)` (genuinely no commits); "could not measure" is not evidence of no work and must not
feed `no_work_done`. **The open question the fix should answer rather than assume:** what Layer 2
returns instead. Falling through to Layer 3 is the natural precedent — the same function already
does exactly that for an unreadable exit file (`Err(_) => return Ok(None)`). Phase 35's A-06 fixes
the adjacent split: `.output()` returning `Err` → `None`; `Ok` with non-zero status (branch
genuinely absent) → `Some(0)`, because branch-absent is a real observation.

**Test coverage the fix must add.** No current test exercises a failing `git` in this path. The
discriminating case is `exit_code = 0` + `Stage::Code` + unrunnable `git`, asserting the result is
**not** `Failed`-for-no-work; a test of the ordinary `commits == 0` case passes against both the
buggy and the fixed code. `crates/devflow-cli/src/test_support.rs` already carries `NeutralPath`
plus a `PATH`-guarding mutex — the same mechanism `stub_agent_binary` uses — which is the intended
route for a failing-`git` shim.

**Not established:** how often Layer 2 is the deciding layer in production. The code path was read
and verified; the frequency was not. If Layers 0/1 almost always decide first, real exposure is
smaller than the code suggests — worth measuring before prioritising this above other Medium items.

### Phase 999.86: Replace `release --check`'s Tag-Signing Predictor With a Direct Probe (PROMOTED — Phase 35, revises D-10 for `release --check` only)

**Linear:** [DEN-75](https://linear.app/denniskim/issue/DEN-75/release-check-tag-signing-gate-false-negatives-when-the-key-is-not-in)
(re-promoted; priority raised Medium → High). DEN-79, a duplicate describing the same predictor's
other failure face, closed Canceled the same day rather than left open alongside this entry.
**Found (again):** 2026-08-06, live, blocking `release --check` preflight during the v2.4.0 cut.

**This entry does not reopen 999.50/999.54 as filed.** Those entries (below, both marked REMOVED)
recorded a real operator decision — D-10, `superseded/26-release-cut-automation/26-CONTEXT.md` —
and stay as the historical record of it, unedited. This is a new decision, made after the old one's
premise stopped holding, filed fresh rather than as a silent rewrite of settled history.

**Why D-10's premise stopped holding.** D-10 rejected fixing `check_ssh_signing_viability` on the
reasoning that the whole predictor should be replaced by an executor that runs the real `git tag -s`
and reads git's own result — no viability guess needed, ever. That executor is `devflow release`
with real execution (DEN-50, **still Backlog**, never built). `release --check`'s predictor was
never actually removed or replaced — it is the only thing that exists today, still reading
`ssh-add -l` and comparing fingerprints, unchanged since D-10. It just produced the exact false
negative D-10 was filed about, live, ~~on a machine with the correct key loaded~~ — verified
directly: `ssh-keygen -Y sign` against the configured key succeeded and `git tag -v` confirmed the
right fingerprint, while `release --check` reported `NotViable`.

**Correction to "the correct key loaded" (2026-08-06, Phase 35 discussion).** The key was **not
loaded in the agent**, and that is the actual mechanism rather than an incidental detail. Measured
on the operator's host: `ssh-add -l` exits **0** (the agent holds *other* identities), the
configured signing key's fingerprint is **absent** from that output, and `ssh-keygen -Y sign` with
that key still exits **0** — because the unencrypted private-key sibling is on disk. So the
predictor reaches `SigningStatus::KeysListed`, fails its `stdout.contains(&fingerprint)` test, and
returns `NotViable { "ssh-agent has keys loaded, but not the configured signing key" }` while
signing genuinely works.

This matters beyond wording: it names the class. `ssh-add -l` cannot see on-disk private key
material, so **agent membership is not a necessary condition for `git tag -s` to succeed** — the
predictor is testing a condition the real operation does not require. The original phrasing
suggested a lookup that merely missed a present key; the real defect is that the predictor asks the
wrong question. Any regression test asserting `Viable` must therefore **not** depend on agent
membership.

**The fix is not a second predictor.** DEN-75 already named the right shape, unimplemented until
now: stop inferring viability from `ssh-add -l` and compare a probe *is* the operation the tag step
will perform — sign a throwaway payload with `ssh-keygen -Y sign` and report viability from its own
exit code. This cannot disagree with what `git tag -s` does, because it is not a second
implementation of "will signing work" — it does the actual cryptographic operation on disposable
input. D-10's objection to prediction is honored, not violated: the objection was that a predictor
must independently stay in sync with git's real behavior, and a probe has no independent behavior to
drift out of sync with.

**Scope discipline, explicit:** this fixes `check_ssh_signing_viability` in `release --check` only.
It does not build `devflow release`'s real executor (DEN-50, unaffected, still a separate item), and
it does not reopen the "should DevFlow predict signing viability" question D-10 already closed for
that executor — the executor still must run the real signed `git tag`, not call this probe as a
substitute.

**Priority:** High — this is the second time this predictor has produced a false negative during an
actual release cut with the correct key present (first: DEN-75, v2.0.0; now: v2.4.0), and the false
negative was in the caution direction only by luck of which check fired first — the underlying
mechanism (comparing fingerprints against `ssh-add -l`, which knows nothing about on-disk private
key material a probe would find) is unreliable by construction, not by circumstance.
| **Size:** S — one function rewrite plus a regression test asserting `SigningViability::Viable`
against a probe that actually signs, not a fingerprint match. **Revised 2026-08-06: still S, but it
carries a public-API removal** (see finding 3 below). Version handling decided the same day: the
release stays `v2.5.0` (no external consumers), and the removal is recorded in `CHANGELOG.md` and
the crate docs instead of forcing a major bump.

**Findings from the Phase 35 discussion (2026-08-06).** All measured on the operator's host with
positive and negative controls; n=1, one OpenSSH build, one ed25519 key — they fix the *shape* of
the fix, and are not coverage.

1. **A naive probe can BLOCK, and the entry does not mention it.** With an encrypted key, no agent,
   and a *working* askpass, `ssh-keygen -Y sign` waits on a passphrase prompt — measured, it timed
   out at 6s against a 30s askpass. `release --check` hanging on a dialog is the unattended-stall
   class DOGFOOD-01 exists to eliminate, reached from a new direction. `SSH_ASKPASS_REQUIRE=never`
   turns that into exit 255 in **0s**, and the positive control confirms the working signing path
   still exits 0 under the same variable. Phase 35 decided both that variable **and** a wall-clock
   timeout (35 D-01), since the variable only closes the askpass route — a wedged agent or a stalled
   PKCS11 provider still blocks.

2. **`ssh-keygen -Y sign -f` takes a path, so the inline `key::` form is not directly probeable.**
   It resolves the private key by stripping `.pub` from *that path*, or via the agent. A blob
   materialized to a temp file has neither unless the agent holds it — measured working when the
   agent holds it (exit 0) and failing when it does not (255). Phase 35 declined to probe inline
   values at all (35 D-03), returning `Unknown` fail-soft; that was a surface-cost choice, **not** a
   feasibility finding, so reopening it needs only a temp file and a cleanup path.

3. **The fix orphans public API.** `classify_ssh_add_status` and `SigningStatus` are `pub` in
   `devflow_core::git`, their only production caller is the `ssh-add -l` branch being deleted, and
   `devflow-cli` consumes only `SigningViability`. Phase 35 decided to remove both and treat it as
   the real break it is (35 D-04). `inline_key_fingerprint` (private) is orphaned too, by finding 2.

4. **`-n` namespace: verify, do not assume.** The probe's value is being the operation rather than
   an approximation of it, so the namespace must be checked against a real git-produced signature.
   Left to the planner deliberately, flagged here so it is not quietly guessed.

**Not established:** whether `release --check` itself reports `NotViable` on this host was *traced*
through `git.rs` against the measured inputs, not observed by running the command. The inputs are
measured; the verdict is inferred from them.

### Phase 999.85: Two Protected Comments Now Justify Themselves by a Mechanism Phase 34 Deleted (BACKLOG)

**Linear:** [DEN-107](https://linear.app/denniskim/issue/DEN-107/99985-two-protected-comments-now-justify-themselves-by-a-mechanism)
**Found:** 2026-08-06, Phase 34 security audit (`34-SECURITY.md` findings F-34-01 and F-34-02).
Grouped as one entry per the 999.81 advisory-cleanup precedent — same file, same root cause, same
one-paragraph fix.

**Priority:** Low | **Size:** S — two comment rewrites, no production change.

**The shape of it.** Phase 34's success criterion 5 and threat T-34-01-04 explicitly **forbade**
editing `idle_timeout_result`'s doc comment, and the phase honoured that prohibition exactly. The
prohibition protected the comment's *text*. It could not protect the comment's *claim*, which
34-01 and 34-03 invalidated in the same phase. Both comments' conclusions remain correct; the
reasons they give for those conclusions are now false.

**F-34-01 — `agent_result.rs:1746-1750`, `idle_timeout_result`.** The comment says `verdict` stays
`None` because `classify_validate_outcome` "matches `Some(Verdict::Pass)` FIRST and would classify
the stage as passed on the strength of that field alone, **whatever the status says**." Both halves
are now dead:

1. After 34-03, `pipeline_outcomes.rs:233` reads `(_, AgentStatus::Success, Some(Verdict::Pass))`.
   The status position is no longer a wildcard, so a non-`Success` status cannot reach `Passed`.
2. The remaining route — a timeout's verdict grafted through `reconcile_layer0_verdict`, reachable
   because `evaluate_layer1` returns the idle-timeout side channel as its **first statement**
   (`agent_result.rs:1795`) — is closed by 34-01's own
   `.filter(|layer1| layer1.status == AgentStatus::Success)` at `:2203`. `idle_timeout_result` sets
   `status: AgentStatus::IdleTimeout` (`:1753`), so it is filtered.

`verdict: None` is now defended structurally in two places rather than by this convention.

**F-34-02 — `agent_result.rs:6412-6417`, inside
`stream_success_cannot_stand_against_nonzero_exit_code`.** The same superseded claim: "Carrying
`Some(Verdict::Pass)` over would leave Validate classified Passed." Disclosed as out of scope by
34-01-SUMMARY deviation 3 ("[Observation, no action taken] A residual instance of the superseded
claim") and recorded again by the Phase 34 verifier under criterion 5. It is inside a test rather
than production doc, which is why 34-01 correctly left it.

**Why this is worth a ticket rather than a shrug.** This is the Repudiation class T-34-01-03 and
T-34-03-04 were filed about, one level up. A reader who checks either comment's stated mechanism
against the current classifier finds it false, may conclude the guard is vestigial, and may
"helpfully" populate a verdict there — reopening a route Phase 34 closed. The hazard is indirect
and the severity is low; the fix is a paragraph.

**Proposed fix.** Rewrite both comments to cite the two structural defences that now carry the
invariant (the classifier's enumerated status position; the graft's status filter), keeping the
`verdict: None` instruction itself intact and unweakened. Do **not** treat "the mechanism changed"
as licence to relax the instruction — the instruction is still load-bearing, just doubly defended.

**Note for whoever picks this up:** DEN-95 (999.74), the defect these comments describe, is still
open in Linear despite Phase 34 closing it via criterion 3. Same for DEN-98 (999.76) via criterion
6. Both want a status sweep.

### Phase 999.84: Nothing Guards the Root Argument at the `GateReview` Checkpoint Call Site, So 999.76's Fix Can Regress Silently (PROMOTED — Phase 35)

**Linear:** [DEN-106](https://linear.app/denniskim/issue/DEN-106/99984-nothing-guards-the-root-argument-at-the-gatereview-checkpoint)
**Found:** 2026-08-06, Phase 34 UAT test 1 — the phase's sole human-verification item, self-disclosed
by 34-04's own SUMMARY and independently reproduced by the phase verifier rather than accepted on
the claim.

**Priority:** Medium | **Size:** S — one integration test plus its negative control. No production
change; the production code is already correct.

**The gap.** `pipeline_launch.rs:1070` passes `execution_root` to
`verify::phase_has_blocking_human_checkpoint`, which is 999.76 criterion 6's second call site and
the reason the plan-28-03 checkpoint auto-decide path is not dead in worktree mode. **No test
asserts that argument.** Revert it to `project_root` — reintroducing the exact defect 999.76 was
filed for — and `cargo test -p devflow --bin devflow` still reports **279 passed; 0 failed**. That
is the measurement, run twice independently: once by plan 34-04, once by the verifier.

**What already exists, and why it is not enough.** `verify.rs:351`
(`…reads_the_execution_root_in_worktree_mode`) and `verify.rs:377`
(`…still_reads_the_project_root_without_a_worktree`) pin the *function's* root-sensitivity in both
directions, and each carries an explicit opposite-result assertion so the pair cannot degrade into
measuring "a PLAN exists somewhere." They are well built. They simply never drive the call site, so
the argument at `:1070` is unguarded by construction.

**Why the Phase 34 capture campaign does not cover it either** (checked, not assumed — the campaign
is the obvious place to look):

1. Every capture ran `devflow start --phase 1 --no-worktree …`. `main.rs:533` → `commands.rs:238`'s
   `else` branch never assigns `worktree_path`, so it keeps its `None` default (`state.rs:269`), and
   `state.worktree_path.as_deref().unwrap_or(project_root)` evaluates to `project_root`. The changed
   expression returns the identical value the pre-fix code passed — structurally incapable of
   discriminating.

2. `blocking-human` appears **0 times** in `scripts/scratch-dogfood-repo.sh`. Negative control: the
   string matches in 20 files elsewhere in the repo, so the zero is a real zero and not a broken
   search. Condition (2) of the arm's five-condition guard was false regardless of root.

**Proposed fix.** One integration test driving `advance()` through `Action::GateReview` with all
five preconditions satisfied — `worktree_path = Some(worktree)`, a PLAN declaring
`gate="blocking-human"` written **only** under the worktree, `agent = AgentKind::Claude`, a session
id on record, the checkpoint present in the capture, `checkpoint_resumes` below
`MAX_CHECKPOINT_RESUMES` — asserting the resume path fires rather than falling through to the
per-stage dispatch.

**The test only counts if it ships with its negative control:** revert `:1070` to `project_root` and
the new test must FAIL. Without that step this entry produces a 280th test that passes both ways,
which is the failure mode it exists to close.

**Relationship to 999.76's open question — decide together, but they are not the same item.** 999.76
(this entry's parent, promoted into Phase 34) left an unanswered question: whether that fix should
carry ~~the workspace's first *real linked* `git worktree` integration test, since today's
worktree-mode tests use plain `create_dir_all` directories with no git repository at all~~ a real
linked `git worktree` test. That question is motivated by `phase_commit_count`'s shared-refs
property, not by this call site, and it remains open. **This entry does not depend on it** — the
`:1070` guard needs only a directory standing in for the worktree, exactly as `verify.rs:351`
already does, so it can land cheaply and independently. If the linked-worktree harness is built
first, this test should use it.

**Correction: "the workspace's first" is FALSE (2026-08-06, Phase 35 discussion).** Real
`git worktree add` fixtures already exist in at least three places:

- `crates/devflow-cli/src/staleness.rs` — `worktree_staleness_fixture()`, the fullest one: a
  `develop` branch with a recorded commit, a **sibling** feature-branch worktree via
  `git worktree add -b <branch> <path> <start_point>`, and two further commits made inside it.
- `crates/devflow-cli/src/preflight.rs:1198` — a second real fixture (CR-02, `25-REVIEW.md`).
- `crates/devflow-core/src/worktree.rs` — the worktree module's own fixtures.

The claim is true only of the `verify.rs` tests specifically. **Two consequences.** (a) 999.76's
open question is materially cheaper than this entry implies — it is "should the 999.76-touched tests
use a real worktree", not "should the workspace build its first", and a fixture can be adapted
rather than invented. (b) No entry or plan should cite "the workspace has no such harness" as a
reason for anything; it is not true.

**Correction: "one integration test" is loose.** `advance()` is `pub(crate)`
(`pipeline_launch.rs:936`), so **no test under `crates/devflow-cli/tests/` can call it**. The test
must live in `pipeline_launch.rs`'s own `#[cfg(test)]` module.

**Two pieces of the harness already exist**, checked during the Phase 35 discussion — the test
should extend them, not rebuild:

- `code_unknown_does_not_transition_to_validate` (`pipeline_launch.rs:~1452`) drives a real
  `advance()` on a scoped thread over a real git repo via `init_repo`, polling for gate files.
- `relaunch_checkpoint_session_emits_exactly_one_audit_event` (`pipeline_launch.rs:1626`) shows how
  to satisfy the resume path **without launching an agent**, via a `stub_agent_binary("claude")`
  helper and an `env_lock()` guard. The observable is the `checkpoint_auto_decided` event, which
  `relaunch_checkpoint_session` emits *before* the spawn by design (28-03 D-07).

**Phase 35 strengthened the proposed fixture (35 D-05).** The bare form above leaves `project_root`
with no `.planning/phases/` at all, so it discriminates partly by a condition production never
satisfies — the main checkout always carries `.planning/phases/`, often including a previous run's
copy of this phase. The phase writes a **decoy** PLAN under `project_root` for the same phase
declaring no `blocking-human` gate, so the revert fails because the *wrong root was read* rather
than because the main checkout happened to be empty. Same cost, stronger control.

**Phase 35 also added a re-running control (35 D-06).** The performed revert stays mandatory, but it
is a one-time act nothing repeats; the test additionally asserts
`phase_has_blocking_human_checkpoint(project_root, phase)` is `false`, the same opposite-result shape
`verify.rs:351`/`:377` already carry. Stated precisely: the mechanical half proves the two roots
*disagree*; only the performed revert proves `:1070` passes `execution_root`. Neither substitutes
for the other.

### Phase 999.81: Phase 33 Advisory Cleanup — the Loop-Back Prompt Calls a Normal Continuation a Defect, Plus Three Hygiene Items (BACKLOG)

**Linear:** [DEN-103](https://linear.app/denniskim/issue/DEN-103/99981-phase-33-advisory-cleanup-the-loop-back-prompt-calls-a-normal)
**Found:** 2026-08-05, Phase 33 code review (IN-05, IN-01, IN-03, IN-04), each verified in source
before filing. Grouped as one entry per the 21-REVIEW advisory-cleanup precedent.

**Priority:** Low | **Size:** S

**IN-05 is the one worth doing, and it is a single match arm.** `fix_prompt`
(`crates/devflow-core/src/prompt.rs:297-307`) gives all three `FixType` variants the same preamble:
*"Validation reported issues. Run the fix command for this loop:"*. For `FixType::FullExecute` that
contradicts the enum's own doc comment (`:55-60`) — *"the phase is mid-arc rather than defective"*.
Phase 33 fixed the **routing** so a mid-arc phase gets `/gsd-execute-phase {N}`; the agent receiving
it is still told validation reported issues, so it is primed to hunt defects in work that was simply
never judged. The phase's own semantic, undone one layer down.

**The other three:** IN-01 — `select_loop_back_fix` (`pipeline_outcomes.rs:261-267`) is reachable
only through six full `handle_validate_outcome` drives; a table test over
`(worktree_path, artifact_location) → FixType` would make future cases cheap. IN-03 — the
branch-name derivation is duplicated at four sites (`agent_result.rs:1904`, `ship_evidence.rs:160`,
`test_support.rs:122`) despite `phase_commit_count`'s doc saying the extraction exists *because*
copies diverged; expose `phase_branch_name`. IN-04 — `commit_on_feature_branch`
(`test_support.rs:112-137`) never restores the prior checkout, benign for its two callers but a
landmine for any future Validate→Ship pairing.

### Phase 999.80: Three Test Sites Are Protected From Spawning a Real Agent Only by a Content-Dependent Gate Response, Not Structurally (BACKLOG)

**Linear:** [DEN-102](https://linear.app/denniskim/issue/DEN-102/99980-three-test-sites-are-protected-from-spawning-a-real-agent-only)
**Found:** 2026-08-05, Phase 33 code review (WR-06); **corrected the same day** by the Phase 33
security audit, which mapped it to the registered trust boundary "cargo test process → spawned
agent CLI" (T-33-09 / T-33-17).

**Priority:** Low | **Size:** S — ~6 lines per site

**Severity: Low-Medium.** *This entry's original filing said these tests "can spawn a real agent"
and rated it Medium. That was wrong and is retracted here rather than silently edited away.*

**No agent spawn happens today.** All three sites pre-write a rejected gate response whose note
contains `abort` — e.g. `{"approved":false,"note":"abort: test cleanup","responded_by":"test"}` —
so `GateAction::from_response` resolves to `Abort`, not `LoopBack`, and control never reaches
`loop_back_to_code` → `launch_stage`. Verified in source at all three sites. The original filing
inherited 33-REVIEW.md's WR-06 framing without checking the gate-response path.

**What survives, and is still worth fixing:** that protection is **content-dependent, not
structural** — a magic substring in a JSON note in a test fixture, with no assertion protecting it.
Any future edit rewording the note, changing `from_response`'s parsing, or routing one of these down
the `LoopBack` arm silently reintroduces a real `claude` spawn with the developer's inherited
credentials. 33-04 closed exactly this class for two *other* sites **structurally**, with PATH
neutralization, not by relying on response content.

Sites protected only by response content: `pipeline_outcomes.rs:825-864`, `:878-930` (backs three
tests), `:2073-2097`. Hardened structurally: `pipeline_gate.rs:1111-1170`,
`pipeline_outcomes.rs:1823-1881`. 33-06 added the `NeutralPath` RAII guard (`test_support.rs:279`)
but applied it only to the two regions 33-05 added. Cheaper alternative if the full fix is deferred:
assert at each site that the gate resolved to `Abort`, making the content-dependence a checked
invariant rather than an accident.

**Decide the order against 999.38 deliberately — they touch the same lines with opposite intent.**
999.38 wants to *remove* process-global `PATH` mutation in favour of per-`Command` `env`, which
would let `ENV_MUTEX` shrink or disappear; it explicitly names `pipeline_outcomes.rs:879`, inside
one of the three sites above. Either do 999.38 first and harden these with the new mechanism, or do
this first as the cheap safety fix and let 999.38 sweep them later. Do not work them independently.

### Phase 999.79: `{N}-VERIFICATION.md` Never Goes Stale, So a `--force` Re-Run Inherits the Previous Run's Verdict and Gates Unresolvably (PROMOTED — Phase 35)

**Linear:** [DEN-101](https://linear.app/denniskim/issue/DEN-101/99979-n-verificationmd-never-goes-stale-so-a-force-re-run-inherits-the)
**Found:** 2026-08-05, Phase 33 code review (WR-02, carried from the first pass and given a new
concrete instance by 33-05).

**Priority:** Medium | **Size:** S–M

**Severity: Medium** — a correctness regression path Phase 33's own fix opened, reaching the exact
unresolvable-gate outcome DOGFOOD-01 exists to prevent, from a new direction.

`phase_verification_exists` (`crates/devflow-core/src/agent_result.rs:2588`) is a pure existence
check; nothing deletes, dates or invalidates `{N}-VERIFICATION.md`. 33-05 correctly made the probe
follow `state.worktree_path` — but *unfiltered*. A `--force` re-run of the same phase number
(`ensure_phase_worktree`, `commands.rs:239`) checks out `feature/phase-NN`, which still carries the
**previous** run's committed artifact. That re-run is mid-arc by construction, so its first Validate
failure finds the stale artifact, returns `true`, and dispatches `--gaps-only` — matching zero plans
and gating unresolvably.

**Fix:** minimum, record the limitation in the doc comment. Real fix, invalidate on staleness —
compare a recorded plan count (more robust than mtime, which survives `git checkout` in ways that do
not reflect judgement freshness) against the phase's current plan set. **Prohibition:** do NOT
"fix" this by reverting the probe to `project_root` — that reintroduces the CR-01 defect 33-05
closed and two external peer reviews independently confirmed.

**A cheaper "or equivalent" surfaced 2026-08-06 (Phase 35 discussion).** Established, not assumed:
`start()` calls `State::new(...)` **unconditionally** at `commands.rs:124`, *before* any `--force`
handling — so every `devflow start`, forced or not, begins with fresh `State`. That makes a
**run-scoped** freshness signal available without any plan-count bookkeeping: record a content
fingerprint of `{N}-VERIFICATION.md` in `State` and treat the artifact as fresh only once it has
changed within this run. The entry's rejection of mtime is about mtime as an *age* signal (it
survives `git checkout`); as change-detection against a run-start baseline that objection does not
apply, and a content hash removes it entirely. Phase 35 leans this way — recorded explicitly as a
**departure from this entry's stated fix direction**, so it is overrulable on sight rather than
discovered later.

**Why the plan-count route is weaker, stated so the choice is arguable:** it detects "the plan set
changed", so it false-negatives whenever a replan happens to produce the same count.

**The risk in either mechanism, and the test it demands.** A freshness rule that is too strict never
lets `--gaps-only` fire again, silently regressing what Phase 33 built — trading an unresolvable gate
for a loop that always re-runs every plan. Both directions must be tested: (a) a stale artifact
inherited from a prior run must yield `FullExecute`, and (b) an artifact the Validate agent authored
*this run* must yield `GapsOnly`. A test covering only (a) passes against a rule that marks
everything stale forever.

### Phase 999.78: The Code↔Validate Loop Has No Progress-Independent Bound, and the Gate Message Understates How Long It Has Run (PROMOTED — Phase 35)

**Linear:** [DEN-100](https://linear.app/denniskim/issue/DEN-100/99978-the-codevalidate-loop-has-no-progress-independent-bound-and-the)
**Found:** 2026-08-05, Phase 33 code review (WR-01, WR-04, IN-02). Grouped because WR-04's fix
supplies WR-01's ceiling for free.

**Priority:** Medium | **Size:** M

**WR-01 — the only unconditional bound is gone.** `consecutive_failures_made_progress`
(`crates/devflow-core/src/mode.rs:149-151`) resets the streak whenever the commit *count* rises, and
no other counter bounds this loop. `mode.rs:136-148` defers the remedy to "a follow-up if the
assumption proves wrong" but **no numbered entry existed for it** — this is that number; cite it
from the doc comment. It matters more here than in the abstract: the Code stage's fix command is a
GSD command, and GSD commands routinely commit `.planning/` artifacts even when they change no
source, so "commits something trivial every cycle" is the *ordinary* behaviour of the thing in that
slot, not an adversarial hypothetical.

**WR-04 — the gate message understates duration in Supervise mode.**
`pipeline_outcomes.rs:367-370` interpolates `state.consecutive_failures` into *"Validation failed
{} time(s) — human review needed."* After 33-03 that is a *streak length*, not a total. Supervise
gates on every failure (`mode.rs:173-175`), so a phase committing anything each cycle shows
*"Validation failed 1 time(s)"* at the 2nd, 5th and 9th gate alike — in the one mode where a human
sees every occurrence.

**IN-02 — a resumed pre-999.66 state reads as a fresh streak.** `state.rs:71-100`: `None` means
both "genuine first failure" and "state predates this field", and nothing in `events.jsonl` tells
them apart, so an operator upgrading a binary mid-phase gets no signal the failure budget widened.

**Fix:** a separate never-reset per-phase Validate-failure total closes WR-01 and WR-04 together;
add a distinct `loop_back` reason string for the absent-baseline case. Related but deliberately
separate: 999.77 attacks the same counter by corrupting its baseline rather than removing the bound.

**Decided 2026-08-06 (operator, Phase 35 discussion) — what happens at the ceiling.** The entry
specified the counter but never said what exhausting it *does*, which is a behavioural question, not
an implementation detail: **it fires a human gate and the run stays alive**, the same shape as
`MAX_CONSECUTIVE_FAILURES` today. Rejected: aborting the phase (destructive and irreversible
relative to gating — a phase one cycle from converging gets killed); gating in Supervise but
aborting in Auto (contradicts Auto's existing ceiling, which gates). **Accepted cost, stated:** an
unattended overnight run now parks on a gate instead of looping to completion. That is the intent,
and it is still a behaviour change from today's "looped forever unnoticed."

**Shape settled alongside it (Phase 35, orchestrator's remit).** A new `State` field with
`#[serde(default)]`, following `last_validate_failure_commit_count`'s backward-compat pattern, and
**not** touched by `transition()` — it belongs with `preflight_retries`/`checkpoint_resumes` (per-phase
observations) rather than `consecutive_failures`/`infra_failures` (per-streak counters reset on every
successful transition). The ceiling is a named constant meaningfully above `MAX_CONSECUTIVE_FAILURES
= 3` so it acts as a backstop rather than a competing primary bound. The gate message must lead with
the cumulative total and name it as a per-phase total; a streak may appear as a secondary clause only
if it cannot be mistaken for the headline number, since WR-04's whole complaint is that the current
text reads identically at the 2nd, 5th and 9th gate.

### Phase 999.77: A Single Transient `git` Failure Grants a Free `consecutive_failures` Reset, and the Doc Comment Promises the Opposite (PROMOTED — Phase 35)

**Linear:** [DEN-99](https://linear.app/denniskim/issue/DEN-99/99977-a-single-transient-git-failure-grants-a-free-consecutive)
**Found:** 2026-08-05, Phase 33 code review (WR-03); carried forward as still-open by the DeepSeek
v4 Pro peer review and confirmed in source. **Pre-existing relative to 33-05**, but the
accumulate-vs-reset branch it lives in was built by 33-02/33-03, so it is Phase 33-era code.

**Severity: Medium.** It weakens a safety gate rather than producing a wrong result, needs a
transient fault to trigger, and the gate still fires if `git` failures continue uninterrupted. Not
Low: it defeats the bound precisely when the bound matters, does so silently, and the source
currently documents the opposite guarantee — which is how it survived review.

**The defect.** `phase_commit_count` (`crates/devflow-core/src/agent_result.rs:1841`) returns `0`
indistinguishably for "genuinely no commits", "branch does not exist", and "`git` could not be
run" — its own doc at `:1838-1840` says so, and adds *"Every consumer treats all three the same
way."* That last clause is the part that is not true. The baseline write at
`crates/devflow-cli/src/pipeline_outcomes.rs:422` is **unconditional** ("regardless of which branch
ran above"), so a `0` produced by a broken `git` is persisted as though it were a real measurement.

**Failure sequence.** (1) `git` momentarily fails; count reads `0`; the streak correctly
accumulates — this step is fine — but the baseline is overwritten to `Some(0)`. (2) Next cycle
`git` works and reads the branch's real count, say `40`; the predicate at
`crates/devflow-core/src/mode.rs:150` is `previous.is_none_or(|p| current > p)` → `40 > 0` →
**true** → `consecutive_failures = 1`. No new work happened between the two cycles. One flaky `git`
invocation bought one free reset of the `MAX_CONSECUTIVE_FAILURES` ceiling.

**Impact.** In a genuinely stuck Code↔Validate loop — the exact situation the ceiling exists to
bound — an intermittent `git` failure pushes the human gate further away every time it occurs. The
gate is not disabled, but its guarantee degrades from "bounded" to "bounded unless `git` flakes",
with no limit on how often that can repeat in one run. Silent, environmentally triggered, so it
reaches an operator as "the unattended run never gated" with nothing in the record explaining why —
the same class of unattended stall DOGFOOD-01/02 exist to eliminate, reached from the opposite
direction: failing to gate when it should, rather than gating when it should not.

**The doc comment asserts the opposite, and that is the compounding problem.**
`pipeline_outcomes.rs:283-286` promises *"The failure direction is toward gating: an unrunnable
`git` or a missing branch counts zero every cycle, so once a baseline is recorded the counter
accumulates and the gate stays reachable."* That holds only while `git` stays broken; it is false
for exactly one transient failure, which is the likelier event. **Correct this comment even if the
code fix is deferred** — it documents a safety property the code does not have.

**Proposed fix:** distinguish "counted zero" from "could not count" — ~~add a sibling returning
`Option<u32>`~~, treat `None` as not-progress *without overwriting the baseline* so the next
successful measurement compares against the last real observation, and update
`phase_commit_count`'s "every consumer treats all three the same way" line, which the fix
deliberately falsifies. Full patch sketch in the Linear issue and in `33-REVIEW.md` WR-03.

**DECIDED 2026-08-06 (operator): take the breaking change.** Escalated after adversarial review
found this had been resolved without asking, despite being the same one-way class as 999.86's own
public-API removal. `phase_commit_count`'s return type becomes `Option<u32>`. Rejected: a
`#[deprecated]` delegating wrapper that would have kept it non-breaking — declined because the break
is already bought by 999.86's deletions, and a major bump is a fixed cost rather than a per-item
one. **Version, decided separately the same day: the release stays `v2.5.0`.** Strict semver would
say `3.0.0`, declined because `devflow-core` has no external consumers — so the break is
**documented** (a `CHANGELOG.md` entry naming every changed/removed `pub` item, plus a crate-doc
deprecation note) rather than versioned. The milestone is **not** renamed.

**Defect surfaced during that reasoning — now filed as 999.87 / DEN-108 (34/D-04).** `evaluate_layer2`
(`agent_result.rs:1905`) sets `no_work_done = commit_gated && commits == 0` and routes it to
`AgentStatus::Failed`. A transient `git` failure returns `0`, so an agent that exited 0 having
committed real work reads as `Failed — no work done`. Same root cause as this entry, worse
consequence: a misclassification rather than a weakened bound. The sibling proposal below would have
left it silently intact, which is the concrete harm that decided the option above.

**Revision to the sibling proposal (2026-08-06, Phase 35 discussion).** A *sibling* contradicts
`phase_commit_count`'s own doc comment, which states the single implementation exists because
"[re-deriving] the same two git commands … is what made the two counts able to silently diverge
before this extraction." A sibling reinstates exactly that hazard, and nothing stops a future caller
reaching for the lossy one. Phase 35 resolved instead to **change the single implementation's return
type to `Option<u32>`**, so one implementation survives *and* the compiler enumerates every consumer
once — continuing 34/D-06's structural-over-hand-audited line. `evaluate_layer2` maps `None` to its
existing zero-treatment explicitly at the call site, with a comment, so the behaviour it retains is a
visible choice rather than an inherited accident. **Note this makes it a public-API change** in a
published crate, stacking with 999.86's own removal; both land in the same cut.

**Test coverage the fix must add:** no current test exercises a failing `git`. The regression test
is the two-cycle sequence itself — force a measurement failure, then a success with an unchanged
real count, and assert the streak **accumulated** rather than reset. A single-cycle test passes
against both the buggy and the fixed code, so without that sequence the fix is unverifiable.

**Note for the next reader:** Gemini 3.1 Pro reviewed this logic and rated it clean, analysing
`current = 0` against `previous = Some(0)` (two *consecutive* `git` failures) and correctly finding
the streak accumulates. It never examined failure-then-success, which is the actual defect. Do not
treat that AGREE as clearing this finding.

### Phase 999.76: Layer 0 External Verification Reads Its Declaration From the Main Checkout, So It Is Inert in Worktree Mode (PROMOTED — Phase 34)

**Scheduled as Phase 34** (with 999.73 and 999.74), promoted 2026-08-05. Phase 34's success
criterion 5 is this entry's fix, including the second call site at
`phase_has_blocking_human_checkpoint`. **Why it landed here rather than in its own phase:** it is
inside the code 999.74 rewrites. `classify_validate_outcome`'s `external` predicate requires
`decided_by_layer == Some(0)`, which only `evaluate_layer0` produces — and this defect makes that
unreachable in worktree mode, DevFlow's default shape. Rewriting the match around the two
`external`-gated `Ambiguous` arms without fixing this first would be designing around branches that
never fire. It also sequences *before* the match rewrite inside the phase, for the same reason.

**Linear:** [DEN-98](https://linear.app/denniskim/issue/DEN-98/99976-layer-0-external-verification-reads-its-declaration-from-the)
**Found:** 2026-08-05, Phase 33 code review; confirmed independently by two peer reviews (Claude
code-reviewer, DeepSeek v4 Pro via hermes). **Pre-existing** — `git blame` puts both lines at
2026-07-18 (`c620fb37`, `305e2675`), weeks before Phase 33's merge-base `7b55fce` (2026-08-04).
Not introduced by Phase 33; *surfaced* by it, because 33-05 fixed the same defect class one file
over and its new doc comments now make this one's justification provably false.

**Severity: High.** Worktree mode is DevFlow's default operating shape, and in that shape the
pipeline's highest-trust verification layer never runs. Not Urgent: it fails toward the existing
Layer 1/2 cascade rather than toward a false pass, there is no data loss or security exposure, and
it has been latent since 2026-07-18 with no observed production incident.

**The defect in one sentence:** `evaluate_layer0`
(`crates/devflow-core/src/agent_result.rs:2041-2042`) resolves the correct worktree-aware root and
then does not use it for discovery — `let execution_root = state.worktree_path.as_deref()
.unwrap_or(project_root);` on one line, `external_verify_commands(project_root, state.phase)` on
the next. `execution_root` is used only later, to *run* the probes.

**Why that root is wrong.** `.planning/` is tracked content, so an in-flight phase's `{N}-PLAN.md`
is committed on `feature/phase-{N}` and lives inside that phase's worktree; the main checkout sits
on `develop` and does not have it for the phase's whole duration. No merge-back happens during the
phase, so this is the steady state, not a race. Measured with a discriminating negative control:
`git ls-tree -r develop --name-only -- .planning/phases | grep -c '/33-'` returns **0** while the
same command against `HEAD` returns **17**. (The non-recursive form returns 0 for *every* ref and
proves nothing — use `-r`.) The doc comment at `:2025-2031` asserts the opposite and is false.

**Impact — the silent branch is the dangerous one.** With `DEVFLOW_TRUST_EXTERNAL_VERIFY` unset
(the default), `commands` is empty, `approved_commands` is `None`, and `evaluate_layer0` returns
`None`; the cascade falls through to Layer 1/2 and the stage evaluates normally. Declared external
post-condition probes **never execute, and nothing reports that they were skipped**. Layer 0 exists
precisely so a failed probe outranks every agent-controlled signal, and an all-passing probe set is
affirmative completion evidence on its own — in worktree mode that guarantee is currently vacuous.
With the env var set, the opposite: `commands` empty against `Some(approved)` fires the veto and
every stage hard-fails with `"external verification approval mismatch; PLAN declaration was
removed"` — loud, but wrong, and the message names a cause it cannot know.

**Same root cause, second call site.** `crates/devflow-cli/src/pipeline_launch.rs:957` calls
`verify::phase_has_blocking_human_checkpoint(project_root, phase)`, also routed through
`phase_plan_files`, so the whole plan-28-03 checkpoint auto-decide path is silently dead in
worktree mode. Fix both together or the class recurs a third time.

**Why the suite cannot catch it — and why this is a plan, not a patch.**
`external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree`
(`agent_result.rs:5259`) writes the PLAN under the tempdir standing in for `project_root` while
pointing `state.worktree_path` at an empty sibling directory. It manufactures exactly the layout
the defect assumes, so it is structurally incapable of failing on it, and its doc comment
(`:5255-5257`) codifies the false premise as deliberate intent. A naive one-line root swap breaks
this green test. Found by the DeepSeek peer review; both Claude reviews missed it.

**Proposed fix:** (1) pass `execution_root` at `:2042` using the `worktree_path.as_deref()
.unwrap_or(project_root)` idiom Phase 33 standardised; (2) rewrite the `:5259` test so the PLAN
lives under the worktree, keeping a non-worktree companion for the `None` fallback arm; (3) correct
both false doc comments; (4) fix `pipeline_launch.rs:957` with matching coverage; (5) reword the
"PLAN declaration was removed" message.

**Prohibition the fix must respect:** do NOT retarget `phase_commit_count`. It reads `project_root`
deliberately and correctly — git refs and the object database are shared across a repository's
worktrees, so a worktree commit is already visible from the main checkout. Declaration discovery on
the worktree, commit counting on the main checkout, is the correct end state; Phase 33 established
exactly that asymmetry for the loop-back path.

**Open question the fix should answer rather than assume:** ~~no test in the workspace exercises a
real linked `git worktree` — the worktree-mode tests use plain `create_dir_all` directories with no
git repository at all.~~ The **worktree-mode tests for this defect class** use plain
`create_dir_all` directories with no git repository. Adequate for a filesystem stat, but the
shared-refs property `phase_commit_count` depends on is asserted in comments and exercised by
nothing. Decide whether this fix carries ~~the first~~ a linked-worktree integration test.

**Correction to the struck premise (2026-08-06, Phase 35 discussion).** "No test in the workspace
exercises a real linked `git worktree`" is **false**. At least three real `git worktree add`
fixtures exist: `crates/devflow-cli/src/staleness.rs`'s `worktree_staleness_fixture()` (a `develop`
branch, a sibling feature-branch worktree, and commits made inside it),
`crates/devflow-cli/src/preflight.rs:1198` (CR-02), and `crates/devflow-core/src/worktree.rs`'s own
fixtures. The true statement is narrower: *the tests covering this defect class* use plain
directories.

**The open question survives, at lower cost.** It is genuinely unanswered — `phase_commit_count`'s
shared-refs property is still asserted only in comments — but answering it means **adapting an
existing fixture**, not building the workspace's first. Phase 35 declined to do it there (35 D-05):
999.84's call-site guard resolves a path, and a linked worktree's files are ordinary files, so the
machinery buys nothing for *that* test. This question is motivated by the commit-count property
instead, and stays open on its own merits.

### Phase 999.75: `CloseRule` Treats an Unparseable `tasks` List on the FIRST Announcement as Permission to Close (RESOLVED 2026-08-04)

**Linear:** [DEN-96](https://linear.app/denniskim/issue/DEN-96/99975-closerule-treats-an-unparseable-tasks-list-on-the-first)
**Found:** 2026-08-03 by peer code review during Phase 31 (DeepSeek v4 Pro, HIGH-1), verified
against live source the same day. **Deliberately not fixed in Phase 31** — the naive fix breaks the
common case.

**The defect.** `CloseRule::observe` updates its pending-task count only when a
`background_tasks_changed` event carries a readable `tasks` array. Its comment calls leaving the
previous state standing "the conservative direction" — true only when a previous readable
announcement existed. On the *first* announcement, previous state is `None`, and `should_close()`
treats `None` as permission to close. In exactly the case the comment defends against, the code
does the opposite.

**Failure scenario.** First `background_tasks_changed` arrives with `tasks` present but not an
array. State stays `None`. Marker arrives. stdin is released. A background task completing later
has no channel for its notification turn — the 999.64 shape, reachable through the guard built to
prevent it.

**Why the obvious fix is wrong.** Treating `None` as pending would hang every stage that never
backgrounds anything (the common case) until the 120s idle timeout. The distinction needs three
states where the code has two: never-announced (close is correct), announced-and-drained (correct),
announced-but-unreadable (**new** — must not close). Suggested: replace `Option<usize>` with an
explicit enum so the cases cannot collapse.

**Severity, honestly: contingent on an unobserved CLI behaviour.** Nobody has seen the CLI emit a
non-array `tasks`, and it serializes that JSON itself. This is a robustness gap in a guard whose
job is to be conservative, not a demonstrated live failure.

**Resolved 2026-08-04.** `CloseRule` now uses an explicit `BackgroundTaskState` enum
(`NeverAnnounced` / `Pending(usize)` / `Unreadable`) in place of `Option<usize>`. `should_close()`
permits only `NeverAnnounced` and `Pending(0)`; `Unreadable` blocks closing and falls through to the
idle timeout, same as a genuinely pending task. Mutation-tested: reverting to the prior silent-no-op
behaviour reddens the new regression test and nothing else, with a negative control proving ordinary
non-backgrounding stages still close on their marker alone. 880 passed, 0 failed; clippy
`-D warnings` and `fmt --check` clean. Commit `2c20ab4`, merged via PR #82 (`fcae13d`).

### Phase 999.74: `classify_validate_outcome` Trusts the Agent's Verdict Over Its Own Status (PROMOTED — Phase 34)

**Scheduled as Phase 34** (with 999.73 and 999.76), requirement **DOGFOOD-04**. Phase 34's success
criterion 3 is this entry's fix and criterion 4 is its open question — but **both were rewritten on
2026-08-05 after the open question was answered.** Read the correction below before this entry's
original analysis.

> **SUPERSEDED — read the SECOND CORRECTION below first.** The block immediately following was
> written after the first review pass and its headline conclusion is **wrong**. It is kept because
> its narrow structural claim (the classifier's own inputs are always `Success`) is true and load-
> bearing, and because the reasoning error it embodies — checking a function's inputs and inferring
> a whole-system property — is worth being able to retrace.
>
> **SECOND CORRECTION (2026-08-05) — the inversion IS reachable, by a different route.**
> `reconcile_layer0_verdict` (`crates/devflow-core/src/agent_result.rs:2143-2156`) grafts Layer 1's
> `verdict` onto an affirmative Layer-0 probe success while checking only *Layer 0's* status:
>
> ```rust
> if state.stage != Stage::Validate
>     || result.status != AgentStatus::Success        // Layer 0's status
>     || result.decided_by_layer != Some(0) { return result; }
> let verdict = evaluate_layer1(project_root, state.phase)
>     .and_then(|layer1| layer1.verdict);             // Layer 1's verdict, status unread
> AgentResult { verdict, ..result }
> ```
>
> An agent marker `{"status":"failed","verdict":"pass"}` parses to `(Failed, Some(Pass))`; the graft
> transplants `Pass` onto `Success`, producing `(Success, Some(Pass), Some(0))`. `decide_action`
> advances it; the classifier computes `external == true` and returns `Passed`; `Mode::Auto`
> transitions to Ship. Reached via `evaluate_agent_result_inner:2305`, on the production path.
>
> **So the answer to this entry's open question is YES** — the agent's self-reported verdict,
> attached to its own self-reported failure, converts a Validate that would otherwise have gated
> into a Ship transition. Demonstrated end-to-end against a HEAD-built `advance` binary in
> out-of-repo temp projects, with negative controls: verdict removed or set to `gaps` → gates;
> Layer 0 disabled → `decide_action` intercepts as the block below describes.
>
> **Criterion 3's fix does not close this** — the derived status genuinely is `Success`. The graft
> fix is criterion 4. Preconditions are production-reachable: `external_verify_enabled` defaults to
> `true` (`config.rs:81`), plus a matching `DEVFLOW_TRUST_EXTERNAL_VERIFY`, a PLAN declaring
> `external_verify:`, and passing probes. **Still unestablished:** whether a real agent emits a
> self-contradictory marker in practice. No parser cross-checks `status` against `verdict`.
>
> ---
>
> **FIRST CORRECTION (2026-08-05, superseded above) — the inversion is NOT reachable in
> production.** Six independent
> adversarial lanes plus direct source verification established that `classify_validate_outcome`
> has exactly one production call site (`crates/devflow-cli/src/pipeline_launch.rs:937`), inside
> the `Action::Advance` arm of `outcome_policy::decide_action`. That match is wildcard-free and
> maps **only `AgentStatus::Success`** to `Advance`: `Failed`/`Unknown`/`IdleTimeout` →
> `GateReview`, `ResourceKilled`/`AgentUnavailable` → `GateInfra`, `RateLimited` → `AutoResume`. At
> Validate the `GateReview` arm calls `handle_validate_outcome(.., ValidateOutcome::Failed)`
> verdict-blind at `:990`. The other apparent call site, `pipeline_gate.rs:584`, is inside
> `#[cfg(test)]`. Negative control: `cargo test -p devflow-core --lib outcome_policy::` reports
> 9 passed / 538 filtered out, with a named test pinning every non-`Success` variant away from
> `Advance`.
>
> **So the four `(non-Success, Some(Pass))` pairs this entry names are unreachable *at the
> classifier*** — true, and the second correction above does not disturb it. The inference drawn
> from it ("and therefore the answer to the open question is no") was wrong. What the first pass
> identified remains real:
>
> 1. **A latent structural weakness.** The arm's safety depends entirely on a guard in *another
>    crate* that its own doc comment never mentions — and `decide_action`'s comment marks the
>    `Failed`/`Unknown` collapse "DEFERRED… Revisit if 18d requires divergent routing." A future
>    split makes the wildcard live with nothing local to notice.
> 2. **A documentation defect with measured cost.** At least two in-source comments assert the
>    inversion is live and consequential, and code was bent around it twice — `idle_timeout_result`
>    sets `verdict: None` citing this exact reason, and plan 31-04 nearly shipped a change caught
>    only by adversarial review. Both defensive choices were right; both rationales are wrong about
>    reachability.
>
> **Not established:** whether a real agent ever emits a self-contradictory marker. The parsers
> deserialize `status` and `verdict` independently with no cross-check and neither normalises
> `verdict`, but no archived capture in this repo contains such a line — absence here is weak
> evidence, not a bound.
>
> The fix is still worth doing, as defence-in-depth plus a corrected record — **not** as closing an
> exploitable hole. Criterion 3 additionally now covers all seven `AgentStatus` variants; the
> original four omitted `RateLimited` and `AgentUnavailable`.

**Linear:** [DEN-95](https://linear.app/denniskim/issue/DEN-95/99974-classify-validate-outcome-trusts-the-agents-verdict-over-its-own)
**Found:** 2026-08-03, Phase 31 plan 31-02 execution. Re-confirmed independently at HEAD `e9abb0b`
the same day, and again by the adversarial review of plan 31-04. **Pre-existing** — not introduced
by Phase 31.

**The defect in one sentence:** `classify_validate_outcome`
(`crates/devflow-cli/src/pipeline_outcomes.rs:206`) matches
`(_, Some(Verdict::Pass)) => ValidateOutcome::Passed` **first**, with `_` discarding the status
entirely — so an agent writing `DEVFLOW_RESULT: {"status":"<anything>","verdict":"pass"}` has its
Validate stage classified `Passed` whatever the status says.

**Why it matters.** Same trust-inversion family as 999.67, which let an agent plant its own Layer-0
provenance and is now closed by `a557805`. The layered result model exists so an agent's
self-report is not what decides the gate; here the self-reported `verdict` outranks the status the
cascade derived. Applies identically today to `Failed`, `Unknown` and `ResourceKilled`, and to the
new `AgentStatus::IdleTimeout`. `ValidateOutcome::Passed` flows to `ValidateResult::Passed`
(`pipeline_outcomes.rs:260`). Neither `parse_devflow_result` nor `parse_claude_event_result`
normalises `verdict` — both normalise only `decided_by_layer`.

**How it surfaced — twice, both times as code bending around it.** 31-02's `idle_timeout_result`
sets `verdict: None` with a doc comment naming this exact reason
(`crates/devflow-core/src/agent_result.rs:1746-1750`). And plan 31-04 was about to walk into it:
its exit-code arbitration says return `status: Failed` with "every other field carried over",
which includes `verdict` — making its own success criterion false at Validate. Caught by
adversarial review before execution.

**Why not fixed inside Phase 31.** Changing the arm silently re-routes three existing statuses
whose current behaviour nothing has audited — a behavioural change with its own blast radius, not
a one-line correction. Phase 31 is capped at M with a live acceptance run already gating it, and
`IdleTimeout` neither creates nor widens the hole.

**Open question the fix must answer, not assume:** whether this can manufacture a *pass* on a run
that would otherwise have gated. 999.67's analogous entry could only flip `Failed` → `Ambiguous`,
which still gates. The `_` wildcard here looks stronger — it reaches `Passed` directly — but that
needs establishing by reading the Validate routing end to end.

**Proposed fix:** gate the `Pass` arm on the status the cascade produced, mirroring what
`normalise_stream_marker_provenance` did for `decided_by_layer`; audit all three affected statuses
explicitly; add a mirror test per status so the arm cannot silently regain the wildcard.

### Phase 999.73: Widen `STREAM_JSON_STAGES` Beyond `Stage::Code` (PROMOTED — Phase 34)

**Scheduled as Phase 34** (with 999.74 and 999.76), requirement **DOGFOOD-03**. Phase 34's success
criteria 1 and 2 carry this entry's two halves — **both rewritten on 2026-08-05.** Read the
correction below before this entry's original analysis.

> **CORRECTION (2026-08-05) — the per-stage transport verification is vacuous; the behavioural risk
> is larger than stated.** `ClaudeAgent::exec_command`
> (`crates/devflow-core/src/agents/claude.rs:46`) ignores its `_phase`, `_prompt` and
> `_extra_writable_roots` arguments and returns a fixed argv. **The stream-json launch shape is
> byte-identical for all five stages.** Nothing about the transport, monitor wiring or parser varies
> per stage, so "confirm each added stage against a real capture" is evidence about *agent
> behaviour under that stage's prompt* — criterion 2's question — not about the launch mechanism.
> Criterion 1 as originally worded is close to mechanically vacuous.
>
> **What grew instead.** Widening can make a stage *unusable*. `CloseRule::should_close()` requires
> `marker_seen` AND a drained list; if a stage backgrounds work still pending when its marker lands,
> the rule never fires, the supervise loop reaches `RecvTimeoutError::Timeout`, and
> `fire_idle_timeout` terminates the child and records a terminal `IdleTimeout` → `GateReview`.
> Those stages take the legacy path today and cannot hit this at all. That risk is *created by* the
> widening and is the real content of criteria 1 and 2.
>
> **Ordering trap this entry did not anticipate.** A stage produces a stream-json capture only via
> the pipe-owning path, and `claude_stream_launch_enabled` offers an opt-*out* only — no force-on
> exists. So evidence cannot precede widening through the normal pipeline. Two escapes exist:
> widen in the working tree and let the evidence decide what gets *committed* (making the gate
> commit-time, not build-time), or drive the hidden `devflow __monitor` subcommand
> (`crates/devflow-cli/src/main.rs:133`), which never consults `STREAM_JSON_STAGES` — pointed at a
> scratch phase, since it advances the stage machine on reap.
>
> **Two further traps found in the same review.** `31-ACCEPTANCE.md`'s pass bar is *"VOID unless the
> capture shows a `background_tasks_changed` event with a NON-EMPTY `tasks` array followed by a
> drain to `[]`"* — a non-backgrounding stage cannot satisfy that by construction, so it must not be
> reused verbatim as the per-stage bar. And `DEFAULT_CAPTURE_RETENTION = 5`
> (`crates/devflow-core/src/config.rs:12`) will evict an earlier stage's capture if the phase takes
> any Validate→Code loop-back.

**Linear:** [DEN-94](https://linear.app/denniskim/issue/DEN-94/99973-widen-stream-json-stages-beyond-stagecode-once-the-phase-31)
**Deferred:** 2026-08-03 by operator decision, during Phase 31 planning. Explicitly **not** in
Phase 31; a future phase owns it.

**State at Phase 31 close.** `STREAM_JSON_STAGES: &[Stage] = &[Stage::Code]` in
`crates/devflow-cli/src/pipeline_launch.rs`, gating `claude_stream_launch_enabled`. Only the Code
stage launches the Claude adapter through the pipe-owning monitor with `--input-format stream-json
--output-format stream-json`; Define, Plan, Validate and Ship keep the single-document path. Plan
31-04 requires the summary to state that the constant still lists exactly one stage and why, and
31-05 carries the same as a `must_haves` item — so the narrow list is a recorded, verified end
state, not an oversight.

**Why deferred rather than widened.** D-09 sequences the rollout (one stage first, then widen) —
an explicit sequencing choice, which binding constraint 1 permits, unlike a launch-time prediction
of which stages will background, which it forbids. D-10 makes Code first: it is where 999.64 was
observed (Phase 29 wave 2 dispatched two executors from Code and orphaned both) and it is the only
stage that actually backgrounds, so it is the only one exercising task-notification delivery and
the drain gate at all. The Phase 31 acceptance run witnesses **Code only**; widening now would
extend the adapter to four stages on zero evidence.

**What would evidence it.** A passing Phase 31 acceptance run produces the first real production
capture of the stream path. Until then the parser's production correctness is reasoned, not
witnessed — every gate fixture is labelled SYNTHETIC in-source and no archived capture contains a
prompt echo.

**Proposed work:** widen the constant (a one-line change to a named constant, built that way
deliberately by 31-01); confirm each added stage against a real capture rather than a synthetic
fixture; and re-check the close rule's `AND` arm per stage — constraint 4's drain arm was measured
*defensive, not load-bearing* on Code (n=2 Mode B trials delivered everything without it), and a
non-backgrounding stage has different drain behaviour, so that reasoning does not transfer.

**Blocked on:** Phase 31 closing with a passing acceptance run (D-19: a failing run means 999.64
is not closed whatever the unit tests say).

**Related:** CONTEXT.md D-14 (per-child declared tokens) is the adjacent deferral from the same
planning session — deferred on size, not merit. Strong candidate to pair with this one.

### Phase 999.72: ROADMAP.md Layout Hides Every Phase From `gsd-tools`' Milestone-Scoped Parsers (RESOLVED — 2026-08-04, Phase 32)

**Linear:** [DEN-93](https://linear.app/denniskim/issue/DEN-93/99972-roadmapmd-layout-hides-every-phase-from-gsd-tools-milestone)
**Found:** 2026-08-03, creating Phase 31. Not blocking Phase 31.

**The symptom:** `gsd-tools query roadmap.analyze` reports `phase_count: 0` for this repository
while 30 phase directories sit on disk. Two layout choices cause it, and **both are ours** — the
upstream half (that the tool reports this silently, with no error) is filed separately as GSD
issue 14.

**Cause 1 — a closed milestone heading separates the active milestone from every phase entry.**
`extractCurrentMilestone` ends the active section at the next version-bearing heading of level
≤ 2. Here that is `## v2.0.0 milestone (CLOSED …)` at line 25, while `## v2.3.0 milestone
(ACTIVE …)` is at line 5 and the first `### Phase N:` is at line 113. The active window is
therefore lines 5–24: prose, no phases.

Isolated with a negative control — demoting **only** line 25 to `###`, nothing else changed,
yields `phase_count: 22` and `next_phase: "31"`. Recorded because the obvious control is the
wrong one: a first attempt demoted all *non-milestone* `##` headings, changed nothing, and
briefly read as a refutation. It had excluded the single heading that mattered.

**Cause 2 — `### Phase N:` is reused for historical entries.** The SHELVED Phase 30, the ABORTED
Phase 29, and `Phase 29 (original scope)` all match the phase-heading regex; in the control run
they produced duplicate numbers (30 twice, 29 twice) and `current_phase: null`.

**Why it matters.** `workflows/next.md` Route 0 — the hard invariant that catches a phase left
mid-execution when `current_phase` has moved past it — iterates `.phases[]` from
`roadmap.analyze`. An empty array means the loop never runs and `INCOMPLETE_PHASE` stays empty,
which is indistinguishable from a clean scan. `/gsd:progress --next` then routes as though the
safety check passed. The check is not wrong; it is absent.

**Proposed work:** move the historical/closed-milestone blocks below the active milestone's phase
entries (or the entries under the active heading); retitle historical entries so they no longer
match the regex (`### Archived — Phase 30 …` suffices, since `Phase` must follow an optional
bracket tag directly); then re-verify `roadmap.analyze` for non-zero `phase_count`, no duplicate
numbers, and non-null `current_phase`.

**Status update (2026-08-04):** the reproduction snapshot above (line numbers, "ACTIVE" milestone,
30 phase directories on disk) is historical, but **this defect is still fully live in this repo,
verified by direct re-run today, after the v1.0/v2.0.0/v2.3.0 retroactive milestone archival**:
`roadmap.analyze` still returns `phase_count: 0` against current HEAD, and `milestone.complete
v2.3.0 --dry-run` still triggers the pass-all degrade (now sweeping all 17 backlog directories
instead of the historical phases it swept before). The archival did not remove Cause 1 — it
changed its shape. Every milestone heading in the live document (v2.3.0, v2.0.0, v1.0) now
collapses to a plain markdown *table* with no `### Phase N:` headings inside its own window at
all (they're archived out), so `extractCurrentMilestone`'s scan finds zero recognizable phase
entries in the active milestone's section regardless of what heading follows it — the original
"interrupting closed-milestone heading" framing was only one way to reach `milestonePhaseNums.size
=== 0`; a milestone whose live content is header-free is another. **This is a repo-structure
condition, not a backlog-cleanup residue** — closing this item on the theory that archiving old
phases fixed it would be wrong; it is exactly as reproducible today as when filed. It will keep
reproducing for any future milestone unless that milestone's own `### Phase N:` (or
checkbox-bullet) entries land inside its heading-to-next-heading window before archival — not
guaranteed, and complicated by the separately-documented `phase.add`-inserts-at-the-last-`---`
bug, whose landing point has moved now that this document is ~1000 lines shorter. This same root
cause also has a worse, write-path sibling: see upstream
GSD-core issue ledger entry 16 (`gsd-tools milestone.complete`'s pass-all degrade), found closing
v2.3.0 the same day.

**Resolution (2026-08-04, Phase 32):** `gsd-roadmapper`'s write when creating the `gsd-hygiene`
milestone's own ROADMAP.md section (commit `0b1ad74`) put this milestone's `### Phase 32:`
heading and its `## Progress` table inside the active milestone's heading-to-next-heading window
by construction, resolving both Cause 1 and Cause 2 for this milestone as a side effect —
confirmed by direct re-run in Phase 32's `/gsd-discuss-phase`: `roadmap.analyze` returns
`phase_count: 1` / `next_phase: "32"`, and `milestone.complete gsd-hygiene --dry-run` correctly
reports the one unstarted phase instead of the pass-all degrade. **This resolves the condition for
the current milestone; it is not a structural fix** — the 2026-08-04 status update's warning above
still holds for whatever milestone comes after `gsd-hygiene`, since nothing changed in
`gsd-roadmapper` or `phase.add` themselves. See Phase 32's `32-CONTEXT.md` for the verification
detail and for why durability wasn't tackled in this phase.

**Sub-item 999.72a — add the missing `## Progress` table. Size S, independently landable, do this
one first.** Added 2026-08-03 after it was traced as the cause of a *second*, separate misverdict.
**Resolved 2026-08-04** — the table now exists (`## Progress`, this file, added by the same
`gsd-roadmapper` write). `deriveProgressFromRoadmap`'s roadmap-derived completion path is
confirmed live (`smart-entry`'s `roadmap_total_phases`/`roadmap_completed_phases` flipped from
`null` to real counts per STATE.md's 2026-08-04 session note).

`isComplete()` prefers ROADMAP-derived counts and falls back to a legacy STATE.md comparison
"when the roadmap has no Progress table". We have never had one: the table is absent from **all
239 commits** that have touched ROADMAP.md, because this file was hand-authored at GSD-init
(`4f2b849`, 2026-06-17) rather than generated from `templates/roadmap.md`. The fallback then
compares `current_phase: 30` against STATE.md's `progress.total_phases: 21` — a global phase
number against a milestone-scoped count, which gsd-core's own source calls "the two-scale bug" —
concludes the project is past its last phase, and with a `status:` beginning "complete" returns
`situation: complete` / `recommended: /gsd:new-milestone` while Phase 31 is outstanding.

Measured: from a blockers-cleared base, correcting *either* input alone drops the `complete`
verdict. Neither produces a Phase-31 route, because `smart-entry` has no next-phase situation at
all — that is `/gsd:progress --next` Route 6's job.

**Why this is separable from the restructure above:** `deriveProgressFromRoadmap` scopes to the
`## Progress` heading and does **not** go through `extractCurrentMilestone`, so the table works
regardless of the milestone-window problem. It is a genuinely independent, much cheaper fix.

**Why it cannot self-heal:** the table is written once by `gsd-roadmapper` at
`/gsd:new-project` / `/gsd:new-milestone`, then maintained incrementally by `phase.complete`,
whose edits are gated on `content.match(/^##[ \t]+Progress\b/im)`. With no heading the block is
skipped silently — no warning, no creation. There is no repair verb;
`state.update-progress` writes STATE.md frontmatter, not this table.

Shape required by `deriveProgressFromRoadmap`: a `## Progress` section containing a table with
columns `Phase`, `Plans Complete`, `Status`, `Completed` (any order, extra columns ignored).
Completed is counted as rows whose `Status` is exactly `complete`. Either hand-author it once
(~22 rows) and let `phase.complete` maintain it, or take it at the v2.3.0 → next milestone
boundary, which is the idiomatic creation point.

**Priority:** Medium — a disarmed safety gate, but one that has not yet been observed to cause a
loss. **Size: M overall** — a several-hundred-line reorganisation of a ~3300-line document;
mechanical but easy to corrupt, so verify with a diff that removes zero lines of substance.
Sub-item 999.72a is **S** and can land alone.
**Depends on:** nothing.

### Phase 25 candidates — all open High items, validated 2026-07-27

> **Outcome:** Phase 25 was scoped from this analysis the same day. Five of these
> were taken (999.51, 999.48, 999.49, 999.44, 999.47) and are now marked
> `PROMOTED — Phase 25`; the rest stay in the backlog with the exclusion reasons
> recorded in Phase 25's own entry. This table is retained as the record of what
> was considered and why, not as an open work list.

Every backlog entry marked **High** was re-checked against the codebase on
2026-07-27, not trusted from its own text. Eight are genuinely open; one was
already delivered and is called out below so it is not re-promoted.

| Item | Linear | What it blocks | Validation performed |
|---|---|---|---|
| **999.48** Pin the driving binary | DEN-73 | **The end-to-end dogfood goal itself.** No phase that touches DevFlow's own source can complete unattended until this lands — proven by Phase 23's attempt 3 halting at Validate | Filed 2026-07-27 from a live run |
| **999.49** `compute_version` | DEN-74 | The first DevFlow-driven release that ever succeeds. Armed the moment 999.48 unblocks one | Re-measured at `9916e2f`: computes `~1.11.359` against a real `1.8.1` |
| **999.44** State-orphaned processes | DEN-68 | Reliable cleanup. Escalated 2026-07-27: these orphans are **`SIGTERM`-immune**, so any reaper built on `TERM` reports success and leaves them running | 30 processes cleared by hand; 15/15 survived `TERM`, all needed `KILL` |
| **999.47** `looks_like_devflow_process` flake | DEN-72 | CI throughput — cost two retries on 2026-07-27 alone (~50% failure rate). Production risk is already closed; this is now a test-only defect | `/proc/PID/exe` capture confirmed the fork→exec window as the mechanism |
| **999.31** Modular agent driver | DEN-56 | Onboarding any agent beyond Claude; a confirmed Codex dogfood failure | Confirmed still open: `Stage::gsd_command()` (`stage.rs:52`) still returns literal `/gsd-*` strings from core |
| **999.25** Release-cut executor | DEN-50 | Making releases repeatable instead of a manual checklist — the 2.0.0 cut is being done entirely by hand | Confirmed still open: `devflow release` exposes exactly one option, `--check` |
| **999.15** Hermetic tests for shell entry points | DEN-40 | Confidence in `install.sh` (every new user's first run) and `sync-main-to-develop.sh` (mutates real branch history) | All three scripts still present; no behavioral test references them |
| **999.21** AI change-acceptance wiring | DEN-46 | The contract governing AI review rather than only existing | `.claude/skills/ai-change-acceptance/` present in-repo; wiring surface partly lives in the GSD workflow **outside** this repository, so an in-repo fix may not fully close it |

**Not a candidate — already delivered:** 999.29 (dogfood staleness false-positives,
DEN-54) still carries a `**Priority:** High` line in its body, but its heading reads
`DELIVERED — Phase 21 / 21d` and DEN-54 is Done. A naive grep for High will surface
it; it must not be re-promoted. Its "in source but unshipped" note is also stale —
2.0.0 ships it.

**Sequencing observation, not a decision.** 999.48 and 999.49 are the pair that
gates the end-to-end goal, and they compose: 999.48 makes an unattended run
reachable, at which point 999.49 fires on its first success. Taking either alone
leaves the goal blocked. 999.47 is unrelated but cheap and is currently taxing
every PR.

### Phase 999.1: Hermes Support (BACKLOG)

**Goal:** `HermesAgent` adapter with native-envelope completion parsing, rewrite of the stale `skills/hermes/devflow/SKILL.md`, and the Hermes plugin session mode with an events.jsonl-driven gate watcher. Held Phase 18's slot until 2026-07-20, when pipeline-reliability work took priority — personal-infrastructure work that doesn't gate anything else.
**Priority:** Low | **Size:** L — reviewed 2026-07-21: structurally lowest (gates nothing else), operator confirmed still low priority. Linear: DEN-26.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.2: A Phase Tracks Exactly One Process (DELIVERED — Phase 21 / 21c)

**Goal:** One `phase-N-agent-pid` file per phase leaves the monitor unrecorded and `sequentagent`'s second agent homeless. Frame as two tracked processes per phase. *(was 19b)*
**Priority:** Medium | **Size:** M — the "monitor unrecorded" half shipped in v1.5.0 (18b); the narrowed remainder (`sequentagent`'s orphaned second process) was **delivered by Phase 21 unit 21c** (plan 21-04) — a path-free A/B slot record surfaced in `status`, verified 2026-07-23 (21/21). Linear: DEN-27 (Done, 2026-07-23).
**Delivered:** Phase 21 unit 21c / plan 21-04, 2026-07-23

Plans:

- [x] Delivered by Phase 21 (21c / 21-04) — absorbed as a unit, not separately promoted

### Phase 999.3: CLI Operator Discoverability (DELIVERED — Phase 21 / 21a)

**Goal:** Gate reasons truncate with no `devflow gate show`; rate-limit reset times buried in raw JSON; `status` lacks in-stage progress; recovery verbs undiscoverable from a stuck state. *(was 19c)*
**Priority:** Low | **Size:** L — all four bundled gaps **delivered by Phase 21 unit 21a** (plan 21-02): `devflow gate show <phase>` (untruncated), rate-limit reset surfaced in `status`, in-stage progress line, recovery-verb hints — verified 2026-07-23 (21/21). Linear: DEN-28 (Done, 2026-07-23).
**Delivered:** Phase 21 unit 21a / plan 21-02, 2026-07-23

Plans:

- [x] Delivered by Phase 21 (21a / 21-02) — absorbed as a unit, not separately promoted

### Phase 999.4: Version-Tag Contention on Concurrent Ship (REMOVED — 2026-07-29)

**Was:** Two phases computing the same next version race to create one tag under `devflow parallel` (multiple whole phases concurrently). *(was 19h)*
**Removed during Phase 26 discuss-phase:** the race is specific to `devflow parallel`'s whole-phase concurrency; the operator confirmed they never use, and would never want, a single DevFlow user running multiple phases at once ("that's just asking for trouble"). Since the scenario cannot occur in actual usage, this entry is removed rather than left filed. See `superseded/26-release-cut-automation/26-CONTEXT.md` D-11. Linear: DEN-29 (close as won't-do).
`devflow parallel`'s own future (deprecate whole-phase concurrency vs. repurpose for intra-phase workstreams vs. leave alone) is captured as a deferred idea in that same CONTEXT.md, for its own future phase — not lost, just not a defect record.

### Phase 999.5: ChangelogAppend Placeholder Content (PROMOTED — Phase 26)

**Goal:** Every generated changelog entry reads "Released phase via DevFlow" — deferred twice already (17-10, 17-12). *(was 19j)*
**Priority:** Low | **Size:** M — reviewed 2026-07-21: confirmed still generic (`ship.rs:431`). Cosmetic by its own admission, but sized M not S — needs a real content source designed (plan diffs? SUMMARY.md extraction?) before implementation, which is why it's been deferred 3 times already. Linear: DEN-30.
**Requirements:** TBD — see `superseded/26-release-cut-automation/999.5-BACKLOG-DOSSIER.md`
**Promoted:** Phase 26, 2026-07-29 — added using capacity freed by dropping 999.54/999.50/999.4; content source resolved by reusing Phase 25's conventional-commit classifier (see `26-CONTEXT.md` D-12).
**Plans:** 0 plans

Plans:

- [x] Promoted to Phase 26 — see the Phase 26 entry for the active tracking

### Phase 999.12: Layer 0 Unapproved-Probe Veto Coverage (BACKLOG)

**Goal:** 17-REVIEW.md WR-04 — coverage debt on a *deliberate* trade, not a defect. 17-03 removed `evaluate_layer0`'s `Stage::Code` guard by design (D-05 gap 1), so a forgotten `DEVFLOW_TRUST_EXTERNAL_VERIFY` now vetoes at all five stages instead of one, a 5× blast-radius increase. Two verified gaps at HEAD: (a) of the three veto arms, only "approval mismatch" is tested (`agent_result.rs:1644`) — the "not approved" arm a forgotten env var actually hits has no test at any stage; (b) `docs/guides/configuration.md` states the requirement for "the parent DevFlow process" but never that the **detached monitor subprocess must inherit it**, which is where the failure manifests. Deliberately not folded into Phase 18's 18-05 (same file) — that plan had already passed the checker clean, and adding coverage debt to a verified bug-fix plan is scope creep.
**Priority:** Medium | **Size:** S — reviewed 2026-07-21: confirmed still only "approval mismatch" tested at `agent_result.rs`. Test/doc debt on an already-shipped, intentional decision, not a live bug. Linear: DEN-37.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready — ideally soon after Phase 18 ships, while 18-05 is fresh)

### Phase 999.9: Dependency Update Review (BACKLOG)

**Goal:** Triggered 2026-07-20 by a GitHub Actions annotation on the first all-branch CI run — `actions/checkout@v4` targets deprecated Node.js 20 and is being force-run on Node 24. Warning only, all jobs green, but it appears on 4 job definitions across both workflow files, so the eventual break lands everywhere at once. Broader than a one-line bump: the dependency surface is inconsistently pinned — `dtolnay/rust-toolchain@stable` and `rust-toolchain.toml`'s `channel = "stable"` float entirely (CI can break from upstream with no commit here, a reproducibility gap for a project premised on trustworthy pipelines), `devcontainers/ci@v0.3` is pre-1.0, the devcontainer base image pin was last verified in Phase 15, and neither `cargo audit` nor `cargo deny` runs in CI. Deliberately not folded into Phase 18 — a dependency bump mid-phase would confound that phase's test signal.
**Priority:** Medium | **Size:** M — reviewed 2026-07-21: confirmed `actions/checkout@v4` still current pin. Nothing failing today; most of the scope is policy decisions (pin vs. float) rather than code. Linear: DEN-34.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.14: Doctor Reconciliation for Planning-Doc Staleness (DELIVERED — Phase 21 / 21b)

**Goal:** `devflow doctor`'s 18a reconciliation checks phase state against events/PIDs/gates/branches, but nothing checks whether `ROADMAP.md`/`STATE.md`'s own narrative still matches reality once a phase's outcome is decided by a manual, out-of-band action (merge, tag, publish). Found 2026-07-21: `STATE.md`/`ROADMAP.md` claimed Phase 18 was "not yet merged / released" after v1.5.0 had already shipped — the same class of bug `17-REVIEW.md` WR-06 already named once (19e/19f marked open after `17-13` had already closed them).
**Priority:** Medium | **Size:** M — **delivered by Phase 21 unit 21b** (plan 21-03): detection-only `planning_doc_staleness` finding in `doctor` (human + `--json`) reconciling ROADMAP/STATE version claims against git tags, never rewriting prose — verified 2026-07-23 (21/21; live run produced 4 correct Warn findings, 0 false Problems). Linear: DEN-39 (Done, 2026-07-23).
**Delivered:** Phase 21 unit 21b / plan 21-03, 2026-07-23

Plans:

- [x] Delivered by Phase 21 (21b / 21-03) — absorbed as a unit, not separately promoted

### Phase 999.15: Hermetic Tests for Shell Entry Points (BACKLOG)

**Goal:** `scripts/install.sh`, `scripts/sync-main-to-develop.sh`, and `scripts/deploy.sh` have user-facing, side-effecting behavior (network downloads, git history mutation, docs deployment) with no direct behavioral tests — only source-text inspection. From `TEST-SUITE-QA-REVIEW.md` (Codex, 2026-07-21).
**Priority:** High | **Size:** L — re-scoped 2026-07-21 (Claude review): the source document treated all three scripts as equally P0; `deploy.sh` only touches `gh-pages` (docs), meaningfully lower blast radius than `install.sh` (every new user's first run) or `sync-main-to-develop.sh` (mutates real branch history). Demoted `deploy.sh` within this item rather than splitting it out. Linear: DEN-40.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.17: Mutation Testing (`cargo-mutants`) (BACKLOG)

**Goal:** Introduce `cargo-mutants` as a scheduled/manual gate (not a blocking PR check — too slow at this codebase's size), scoped initially to `verify.rs`, `outcome_policy::decide_action`, `agent_result.rs`'s Layer 0–3 evaluators, and git safety logic (`commit_path`/tag functions). Track surviving mutants rather than treating line coverage as the primary quality score. From `TEST-SUITE-QA-REVIEW.md` (Codex, 2026-07-21).
**Priority:** Medium | **Size:** M — initial scope re-prioritized 2026-07-21 (Claude review): `verify.rs` first, since this session's own QA review found a real fail-open bug there, making it the highest-confidence-return target in the codebase. `main.rs`'s display/dispatch code deliberately excluded from initial scope. Linear: DEN-42.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.18: Property and Fuzz Testing for Protocol Parsers (BACKLOG)

**Goal:** DevFlow parses agent markers, JSON event streams, rate-limit responses, YAML frontmatter, shell commands, and git output with extensive example-based tests but no fuzzing/property testing for malformed or adversarial input. From `TEST-SUITE-QA-REVIEW.md` (Codex, 2026-07-21).
**Priority:** Medium | **Size:** M — re-scoped 2026-07-21 (Claude review): the source document listed six targets needing both `proptest` and `cargo-fuzz` undifferentiated. Most (agent markers, JSON envelopes, frontmatter, event logs, git porcelain) are format-aware business logic better suited to `proptest`; only `shell_quote` is a genuine byte-level adversarial `cargo-fuzz` target (command-injection-adjacent). Fuzzing the full original list would be more investment than the risk justifies. Linear: DEN-43.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.19: Fast and Slow Validation Lanes (BACKLOG)

**Goal:** Keep deterministic unit/integration tests in the fast PR lane; move nested-build provenance tests (`build_provenance.rs`, which dominates suite runtime today), mutation testing (999.17), and fuzz smoke runs (999.18) into explicit slow/scheduled lanes that stay visible and required at an appropriate release boundary. From `TEST-SUITE-QA-REVIEW.md` (Codex, 2026-07-21).
**Priority:** Medium | **Size:** S — mostly mechanical CI-workflow restructuring once 999.17/999.18 exist to route into a slow lane; not much to put there yet beyond `build_provenance.rs`. Linear: DEN-44.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.20: Differential Coverage Enforcement (BACKLOG)

**Goal:** Enforce high coverage on changed lines rather than optimizing for a global percentage (currently 92.81%), requiring a written justification when new branches are intentionally left uncovered. Coverage should support review, not replace behavioral inspection or mutation-testing results. From `TEST-SUITE-QA-REVIEW.md` (Codex, 2026-07-21).
**Priority:** Medium | **Size:** M — real risk if implemented naively: blocking merges on any uncovered line (including legitimately-hard-to-test OS-failure paths) creates friction without catching defects. Keep the written-justification escape hatch. Linear: DEN-45.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.21: AI Change Acceptance Contract — Review Wiring (BACKLOG)

**Goal:** Make the `.claude/skills/ai-change-acceptance/` contract actually govern AI change review rather than only existing in the repo. Phase 19's 19-05 dogfood proved the contract's *wording* discriminates correctly (every non-compliant diff flagged, compliant control untouched) but found its *wiring* incomplete: a context-isolated reviewer independently reached the same verdicts yet never cited the project contract as its authority, and graded the findings `warning`/`info` rather than acceptance-blocking. Today the contract binds only when the dispatcher already knows to load it.
**Priority:** High | **Size:** M — the contract exists precisely because a green suite isn't evidence; if it only applies when explicitly invoked, it doesn't close the unattended-AI-change case it was written for. Note part of the wiring surface lives in the GSD code-review workflow *outside this repo*, so an in-repo fix may not fully close it. Linear: DEN-46.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.22: Refactor Equivalence Guard in CI (BACKLOG)

**Goal:** Give pure-move refactors an automated equivalence check on CI. Phase 19 proved its 8,487-line `main.rs` split behavior-preserving via symbol reconciliation, test name-set identity against a committed baseline, and per-target pass counts — but all three ran locally by hand. CI runs only `cargo test --workspace`, clippy, and fmt, so Phase 19 shipped with an explicit user-accepted verification override recording this gap.
**Priority:** Medium | **Size:** M — a green suite doesn't prove a refactor preserved behavior: a move that silently drops a test still shows green, just with a quietly smaller count. Scope to refactor-shaped changes only; a name-set check on ordinary feature work would fail constantly and get disabled. Phase 19 also found the plan's literal `rg '::tests::'` extraction was itself buggy, so any committed script needs its own test. Relates to 999.19, 999.20. Linear: DEN-47.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.25: Release-Cut Executor (`devflow release` that executes) (CLOSED — WON'T DO, superseded by Phase 30)

**CLOSED 2026-07-31 as won't-do**, after two failed attempts (Phases 26 and 29, ~120 commits, neither shipped). DevFlow is withdrawing from release automation entirely — see **Phase 30**. Code from both attempts is archived on `origin` at `archive/phase-26-release-executor` and `archive/phase-29-release-executor`. Everything below is retained as the record of why, and as the brief any future attempt would have to answer. Linear: DEN-50 — close as won't-do.

**RE-OPENED 2026-07-31, after the SECOND failed attempt.** Phase 29 built all 7 plans, merged
6 waves, and reached 921 passing tests with clean clippy and fmt. An independent cross-AI code
review (Codex `gpt-5.6-sol`, high reasoning effort, read-only sandbox, all 6,136 source lines)
returned **REQUEST CHANGES / BLOCK — 5 Criticals and 1 High.** `feature/phase-29` is unmerged
and unpushed. Do not merge it.

**Do not attempt a third implementation from the same premise.** Operator decision, 2026-07-31:
reduce scope first. The constraints in "What a third attempt must promise" below are the shape
of the next attempt; anything wider has now failed twice.

---

#### The fundamental diagnosis: the code passes *locations* where it must pass *identities*

A release is defined by **a commit**. Every oracle in Phase 29 asks a *name*-shaped question —
"does a tag called `v2.3.0` exist?", "does `Cargo.toml` on `main` say 2.3.0?", "is version
2.3.0 on crates.io?" — and every action runs against **a directory** (`project_root`). Nothing
binds either the question or the action to the specific commit being released.

| # | Finding | Sev | Unit |
|---|---|---|---|
| 1 | `cargo publish` runs in `project_root`, not at the observed release commit. Scratch worktrees release `2.3.0`; publish packages whatever the checkout holds. A checkout holding an unpublished `2.4.0` **irreversibly consumes 2.4.0 while cutting 2.3.0** | Critical | 29b/29c |
| 2 | `signed_tag_on_remote` accepts a signed `v{version}` without comparing its peeled commit to `origin/main` — tag at A, main at B, reports Present, skips tagging | Critical | **29a** |
| 3 | `crates_published` consumes `git::publish_order` directly, bypassing `publish_plan`'s completeness guard — an unreadable member manifest makes the observer report the whole workspace Present | Critical | **29a** |
| 4 | `open_and_arm_pr` creates before arming; a failure between them leaves an open, unarmed PR that every later run reads as InFlight and stops on, forever. Also discards `gh` stderr | Critical | 29b |
| 5 | The Ship hook's `VersionBump` and the executor claim the same `v{version}` namespace incompatibly — this is **IN-01, which this entry already listed as a prerequisite**, recurring | Critical | 29c |
| 6 | "Release PR merged" inferred solely from the version field on `main`; a cherry-pick satisfies it, then an irreversible tag is pushed at an incomplete commit | High | 29a/29b |

**Findings 1, 2 and 6 are one mistake wearing three hats.** `project_root` is only its most
visible face — findings 2 and 6 involve no worktree at all.

**Read-only was not safer, and this is the most important lesson in the entry.** Three of six
findings are in `release_observe.rs`, the pure-observation unit with no irreversible operations,
which the assistant had called "genuinely sound" and proposed landing on its own. **That
recommendation was wrong and is withdrawn.** The difficulty was never in the *acting*; it is in
*specifying which question to ask*. A wrong observer fails quietly instead of destructively —
which is worse, because it is the foundation the acting layer trusts.

#### "Derive state, never record it" was half right, and the half that was wrong cost the phase

The premise is correct for **completion** — is this tag pushed, is this crate published. It is
insufficient for two things the review found:

- **Intermediate intent** (finding 4). A PR created but not armed. *Partially* recoverable after
  all: `gh pr view --json autoMergeRequest` reports auto-merge state directly, and the code
  simply never consulted it — it observed only "is a PR open?". The genuinely unobservable
  residue is narrower than first stated: "is this *my* PR, from *this* release?"

- **Concurrent actors** (finding 5). Observation reports that a tag exists, never who created it
  or why, so the executor cannot choose between deferring, replacing, and refusing.

This entry's own earlier text said "the one thing that cannot be observed is operator
authorization." That was too narrow. It is **any intent that has not yet become an observable
fact**. The seam was identified and then under-scoped.

#### The requirement nobody chose, which is what actually made this hard

Releases are not complicated. **"Resumable from an arbitrary partial failure" is a genuinely hard
distributed-systems property**, and the derive-don't-record premise silently committed the phase
to it: every step must be safe to re-enter from any world state, including states produced by
other actors, mid-flight failures, and races. Nobody decided to take that on. It arrived as a
consequence.

#### What a third attempt must promise — and must not

1. **Thread the release commit explicitly.** Every step takes `(release_commit, version)`.
   Publishing happens from a worktree checked out at that commit, never from `project_root`.
   Oracles compare **commit identity**, not names. Closes 1, 2, 6.

2. **Refuse to start rather than handle every state.** Require a clean tree, the expected branch,
   no pre-existing `v{version}` anywhere, and no open release PR. Converts 4 and 5 from
   "disambiguate this" into "do not begin in it."

3. **Do not automate steps whose intent has no oracle.** The PR arm/merge dance is the clearest
   case — leave the merge click to a human. Ten seconds, one fewer Critical.

4. **Drop arbitrary resumability.** "Runs to completion, or refuses to start and reports exactly
   where the world is" is far cheaper and sufficient for a monthly operation.

#### Process failures, recorded so the third attempt does not repeat them

- **The design premise was never adversarially tested.** `/gsd-review` — cross-AI review of
  *plans*, with `review.default_reviewers` already configured — **exists and was not run.** An
  adversarial pass asks of a premise "what states can this not represent? which actors can
  interfere? does the oracle answer the question we need or a similar-looking one?" Run against
  derive-don't-record, that pass surfaces all six findings **before any code**. Cost: minutes.

- **Six of seven plans merged with no independent review.** Only 29-07 carried a review gate. It
  found a real Critical. There was never a reason to believe the other six were cleaner.

- **A fix closed the instance and left the class open — again.** The 29-07 review found the
  CR-03 truncation in `publish_plan` and fixed it; Codex then found the identical class alive at
  a second site (`crates_published`). This is verbatim the Phase 26 pattern this entry already
  records. **Treat "fixed" as "fixed at one site" until the class is searched.**

- **"921 tests green" was reported as if it were validation.** It is not, and this entry has said
  so since Phase 26. `29-VERIFICATION.md`, `29-REVIEW.md` and a security audit were never
  produced; `29-VALIDATION.md` is a stale wave-1 artifact.

**Priority:** High | **Size:** M for the reduced scope above (was L). Two failed attempts at the
wider scope. Linear: DEN-50.

---

**PROMOTED 2026-07-31 → Phase 29.** Re-attempted as a redesign, not a rebase. The
`feature/phase-26` code is **reference material only** (operator decision, 2026-07-31) — it
is not carried forward, because its five open Criticals are one *lifecycle* defect rather
than five independent bugs. The prerequisite list below is superseded: 999.39 landed in
Phase 27; prerequisites 2–4 dissolve under Phase 29's derived-state model (there is no
ledger to give a terminal state to); and prerequisite 5 (W-17) is retired outright —
required approvals are **0** on both protected branches, so the PR route needs no bypass and
Phase 26's direct-push design was fighting a rule it could simply have followed. See Phase
29's entry for the full reasoning and the live ruleset measurement.

**RE-OPENED 2026-07-30.** Phase 26 built this end to end and it is **not shippable**. Deliberately re-opened under its original number rather than filed as a new item: it is the same work, still not delivered. The code exists on `feature/phase-26` (unmerged, ~75 commits) and should be treated as a **starting point with known Critical defects**, not as a near-complete implementation.

**Verification passed; review did not.** `26-VERIFICATION.md` scored **11/11 must-haves** — the executor genuinely exists and does what the phase goal described. Two independent review passes then found Critical defects in it: **7 Criticals** (2026-07-29), then after a fix round, **5 more** (2026-07-30, `26-REVIEW.md`, `review_mode: re-review-of-unreviewed-critical-fixes`). Status of the first seven fixes: **1 closed, 5 partially-closed, 1 regressed.**

**The pattern is the finding, and it is why this is re-opened rather than fix-looped again.** Each fix closed the *reported instance* and left the *class* open, and three introduced a **new** hazard on an irreversible surface. Two rounds went 7 Criticals → 5 Criticals, and **every Critical in both rounds was invisible to the test suite** — 763 tests passing, 0 failing, clippy and `fmt` clean throughout. A third automated round has no mechanism to do better.

**The five open Criticals (all reproduced by execution, not inferred):**

| ID | Defect |
|---|---|
| CR-01 | `mutating_project_root` bypassed by an inherited `GIT_DIR` — the guard passes while the executor acts on a repository the operator never named. **See 999.39, escalated to High; this gates the whole class.** |
| CR-02 | `CompletedWithoutPublish` exits 0, marks the ledger Complete, and its own printed remediation ("fix the workspace `members` list and re-run") **starts a second release** — after the signed tag is already pushed, leaving that tag permanently unpublishable |
| CR-03 | `members_key_offset` still latches a commented-out or non-`[workspace]` `members` key — publish set silently truncated, pre-gate reports ✓. **The original C-05 was never actually fixed**, only narrowed to exclude a `-` prefix |
| CR-04 | README's repaired manual procedure prescribes `git reset --hard "@{u}"` — resolves to `origin/develop`, not the pre-merge HEAD, destroying un-pushed commits. **A D-05 violation shipped inside the fix for C-07**, in the very document claiming parity with `sync`, whose own source (`sync.rs:46-58`) says "Do not 'fix' this later into a reset." |
| CR-05 | An in-flight ledger **permanently bricks the release path**: `HaltedAtHumanGate` (the *designed* first-invocation outcome under D-02) leaves the ledger `inflight`; every ordinary phase Ship then tags `v{next}` and trips `LedgerContradicted` forever. No clear/abandon verb exists; the only escape is deleting the ledger file, which **re-arms C-02** |

**What is genuinely sound and should be carried forward, not rewritten:**

- **The resume ledger's design (D-06a).** Reviewed as the best-built new code in the phase: a planted lying ledger still creates the real tag (live state provably wins), the schema is versioned and checked *before* deserialization, and corrupt or forward-version ledgers refuse loudly. CR-05 is a lifecycle gap — no terminal state for the non-success outcome — not a design flaw.
- **D-10 held.** No signing-viability predictor was reintroduced anywhere; `check_signing` is deliberately excluded from the execute pre-gate.
- **C-01's fix is settled** — the stray-unreachable-tag refusal genuinely precedes the ledger write and step 1's first mutation, with a test asserting the remote ref and `Cargo.toml` are byte-identical across the refusal.

**Prerequisites before this is re-attempted:**

1. **999.39 (`GIT_DIR` scrubbing) must land first** — see CR-01. No root guard is trustworthy until it does.
2. The ledger needs a terminal state for `HaltedAtHumanGate` and an operator-facing clear/abandon verb (CR-05).
3. **IN-01 escalated from Info to a contributing cause of a Critical**: `hooks_after_ship`'s `VersionBump` and the executor's signed tag share the same `v{version}` namespace via two independent code paths — that collision is precisely CR-05's trigger.
4. **WR-04 escalated**: the ledger now supplies the *version* that the fragile `cargo info` stderr-substring predicate is asked about, and the two can diverge from the manifest.
5. **W-17 — an operator action, not code, but blocking UAT specifically.** The live `develop` ruleset (`develop-merge-or-squash`) is `enforcement: active` with an **empty bypass list** — confirmed live via `gh api repos/.../rulesets`, not just from the review. Every push must go through a PR, including DevFlow's, so this executor's direct-push step cannot land against this repository until the operator adds a bypass. **Deliberately left as-is** — the enforcement is currently the only thing stopping a known-defective executor from reaching `origin`, so configuring the bypass before the five Criticals close would remove a safety net, not add a capability. Do this last, immediately before the re-attempt's UAT pass, never before.

**Design lesson worth keeping.** This phase automated three irreversible operations (`push`, `tag`, `publish`) whose failure modes are invisible to unit tests by construction — every defect was found by reading code, never by a red test. A future attempt should treat adversarial review as the primary gate and the suite as necessary-but-far-from-sufficient, and should consider whether each irreversible step can be made independently re-runnable before composing them into a sequence.

**Priority:** High | **Size:** L — unchanged, but now with a known-defective starting point and a hard prerequisite (999.39). Linear: DEN-50.

---

*Original entry follows, retained for provenance:*

**Goal:** A `devflow release` that *executes* the full release cut — version-bump PR → merge to `main` → signed tag → sync `develop` → publish `devflow-core` then `devflow` to crates.io — not just the read-only preflight. Phase 20's 20d (DEN-38) delivers `--check` only; Phase 20 CONTEXT.md D-03 locked that scope and recorded this executor as the follow-up.
**Priority:** High | **Size:** L — drives irreversible operations (squash-merge to `main`, signed tag, a crates.io publish that can never be un-published or reused), so it needs its own discuss-phase design pass on failure/rollback semantics (tag lands but publish fails; core publishes but cli does not). Blocks on Phase 20's 20a (self-pin) and 20d (`--check`): the executor's preflight step *is* 20d's check and its `VersionBump` step inherits 20a's correctness. Source: Phase 20 D-03 (2026-07-22). Linear: DEN-50 (blocked by DEN-49, DEN-38).
**Requirements:** TBD — see `superseded/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md`
**Promoted:** Phase 26, 2026-07-29 — re-verified open at HEAD `76e49f1` before promotion; bundled with 999.54, 999.50, 999.52 (same release-mechanics area).
**Re-opened:** 2026-07-30 — Phase 26 delivered it PARTIAL and not shippable; see the RE-OPENED block above for the five open Criticals and the 999.39 prerequisite.
**Plans:** 7 plans executed in Phase 26 (26-03..26-09), code unmerged on `feature/phase-26`

Plans:

- [~] Attempted in Phase 26 — built, verified 11/11, then blocked by review (5 open Criticals). Code retained on `feature/phase-26` as a defective starting point; re-attempt gated on 999.39.

### Phase 999.26: `devflow parallel` Git Object-Store Race (BACKLOG)

**Goal:** Confirm-or-refute whether `devflow parallel`'s concurrent per-worktree commits can hit the same git object-store corruption seen in Phase 20's 20b instance 2 (`invalid object` mid-commit-loop, a fsync-ordering flake fixed fixture-side per D-05), and fix it at the product level if the race is real. 20-RESEARCH.md assumption A1 flagged the analog as plausible but unconfirmed — `devflow parallel` has no DevFlow-level lock serializing its concurrent commits.
**Priority:** Medium | **Size:** M — low likelihood but high severity: if the product shares the hole, the next occurrence is a corrupted user repo with an opaque `invalid object` error, not a re-runnable red CI job. Dominated by a deliberate reproduction attempt (a code read can't settle it); the fix if needed is bounded. Relates to 999.4 / DEN-29 (concurrent-ship contention — same concurrency family). Source: Phase 20 D-08 / 20-RESEARCH A1 (2026-07-22). Linear: DEN-51.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.27: `release --check` Signing-Key Inline Classification (PROMOTED — Phase 24)

**Goal:** `check_ssh_signing_viability` (20d, `crates/devflow-core/src/git.rs`) misclassifies an inline (non-path) `user.signingkey` value — a literal key blob configured directly rather than as a file path is treated as a path and reported as not-found. Deterministic edge case; every path-based and no-key branch is already correct and tested. Full detail in `.planning/milestones/v2.0.0-phases/20-release-correctness-operator-control/20-REVIEW.md` (INF-01).
**Priority:** Low | **Size:** S — single classification branch + one test; found by Phase 20 code review (2026-07-23), deferred as Info-severity while CR-01/CR-02 + WR-01/02/03 were fixed inline on the phase-20 branch. Linear: DEN-52.
**Requirements:** TBD — see CONTEXT.md
**Promoted:** Phase 24, 2026-07-26 — selected as the acceptance target for Phase 23 plan 23-11 (D-02)
**Plans:** 0 plans

Plans:

- [x] Promoted to Phase 24 — see the Phase 24 entry for the active tracking

### Phase 999.28: Explicit `--base` Branch Override for `devflow start` (BACKLOG)

**Goal:** Add an explicit `--base <branch>` flag to `devflow start` (default `develop`) so an operator can cut `feature/phase-NN` onto a base other than `develop` — chiefly an unmerged predecessor phase branch, to honor a `depends_on` chain and stack dependent phases. Keep the default `develop`; do **not** implicitly base on the operator's current branch (base must be explicit, never inferred from shell state).
**Priority:** Medium | **Size:** M — base is hardcoded to `develop` (`crates/devflow-core/src/git.rs:54`) and the hardcode is load-bearing for `ship` (Merge→develop→VersionBump) and `parallel` (develop-rooted shared base), so `--base` must thread through launch, and the ship/merge-target semantics for a non-`develop` base need a design pass. The gap: the ROADMAP encodes 22→21→20 but no phase can build on an unmerged predecessor. Source: Phase 21 dogfood-launch design discussion (2026-07-23). **Reassigned to Phase 22** (concurrency/stacking value). Linear: DEN-53.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.29: Dogfood Staleness Guard False-Positives on Docs-Only Commits (DELIVERED — Phase 21 / 21d)

**Goal:** Make `enforce_build_staleness`'s commit-ancestry arm **content-aware** so a self-dogfood run is not hard-blocked when the only commits ahead of the binary's embedded commit changed nothing the compiler sees (`.planning/` docs, etc.). `embedded_commit_is_stale` (`crates/devflow-cli/src/staleness.rs`) returns `Stale` on *any* strict-ancestor HEAD (verified live in Phase 21: binary at `7163347`, worktree HEAD `3a17381`, delta = `.planning/*` only, yet hard-blocked), whereas the dirty-tree arm was already narrowed to `affects_compiled_binary` in 17-10. Apply the same filter to the ancestry arm: `git diff --name-only <embedded> HEAD` → if no build-affecting file changed, `Fresh`. Also fix the block message ("is not an ancestor of HEAD" is wrong for the common case where it *is* an ancestor, just behind).
**Priority:** High | **Size:** S — a false-positive hard-block on DevFlow's own primary workflow (dogfooding commits docs constantly, re-arming the block after every build); the fix is a targeted narrowing with direct precedent (17-10) plus a mixed-range test (docs + a `.rs` change must still block, preserving the Phase 16 false-evidence protection). Retires the `[[feedback-dogfood-rebuild-before-revalidate]]` workaround. Source: Phase 21 dogfood run (2026-07-23), observed live. **Delivered as Phase 21 unit 21d** (plan 21-01): content-aware strict-ancestor arm (docs-only → Fresh, build-input → Stale, fails toward Stale on git error), verified 2026-07-23 (21/21). NOTE: in source on `develop` but **unshipped** — the running binary still false-blocks until a ≥21-01 build runs. Linear: DEN-54 (Done, 2026-07-23; estimate set to 3 to satisfy the team's completion rule).
**Delivered:** Phase 21 unit 21d / plan 21-01, 2026-07-23

Plans:

- [x] Delivered by Phase 21 (21d / 21-01) — folded in as a unit, not separately promoted

### Phase 999.30: Phase 21 Code-Review Cleanup — gate_show/doctor DRY + TOCTOU (DELIVERED — Phase 22 / 22-01, 22-02)

**Goal:** Resolve the four advisory findings from `21-REVIEW.md` (0 critical / 3 warning / 1 info; Phase 21 verified 21/21, no correctness or security defect — these are quality/maintainability only). **WR-01:** `gate_show`'s stage auto-resolution is copy-pasted from `gate_respond` (`crates/devflow-cli/src/commands.rs:810-844`) despite a doc comment claiming the two "can never drift" — extract a shared `resolve_single_open_gate_stage` both call (the same copy-paste-with-"can never drift"-comment smell also sits at `commands.rs:1827`). **WR-02:** `collect_planning_doc_findings` (`commands.rs:2285`) hardcodes `"main"` instead of `devflow_core::config::MAIN` (`config.rs:15`) — matches today so no live bug, but an unlinked second source of truth that would emit false `doctor` Problems if the branch ever becomes configurable. **WR-03:** `gate_show` calls `Gates::list_open` twice (narrow TOCTOU + redundant read) — fetch once, reuse. **IN-01 (info):** `latest_stage_launched_ts` reintroduces the per-phase full-file `events.jsonl` rescan that 14-CR-10 eliminated in the same `status()` function — fold the last `stage_launched` ts into the existing single-pass `last_events_by_phase`.
**Priority:** Low | **Size:** S — WR-01/02/03 are small localized fixes (default `--fix` scope covers all three); IN-01 is a follow-up perf refactor. Source: `21-REVIEW.md` (gsd-code-reviewer, 2026-07-23), deferred to backlog by operator decision. Linear: DEN-55 (Done, 2026-07-24).
**Delivered:** Phase 22 / plans 22-01 (`c442e00`), 22-02 (`2bbfabd`), 2026-07-24. Validated (fmt/clippy/workspace tests green), then integrated on `feature/phase-22` as the staging branch for this release alongside the 999.37 containment fix and the naming reframe, and merged to `develop` from there. Re-verified independently after the sandbox-escape investigation, since the original "tests green" claim was recorded on the same day the repository was corrupted: 537 tests across 13 binaries, clean, with zero coupling to the process-management model being revamped (no changed line touches monitor/pgid/spawn/teardown/liveness).

Plans:

- [x] Delivered by Phase 22 (22-01, 22-02) — absorbed as the phase's narrow trial scope, not separately promoted

### Phase 999.31: Modular Agent Driver Architecture (BACKLOG)

**Goal:** Replace the thin `AgentAdapter` trait with a modular `AgentDriver` contract — capability discovery, driver-owned prompt rendering, command building, completion parsing, and health probes — so agent-specific execution semantics stop being scattered across `prompt.rs`, `agents/*.rs`, `agent_result.rs`, and `preflight.rs`. Root cause of a confirmed dogfood failure: `Stage::gsd_command()` bakes raw `/gsd-*` slash-command strings into core, rendered identically for every adapter (enforced by a test), and Codex received them as literal shell commands during the Phase 22 trial.

**Priority:** High | **Size:** L — fixes a confirmed dogfood-breaking defect and is the prerequisite for onboarding more agents. Linear: DEN-56.

*This entry is deliberately abridged.* It is included because four shipped user-facing docs cite "backlog 999.31" by number — `CONTRIBUTING.md`, `ARCHITECTURE.md`, `docs/guides/adding-agent.md` and `docs/architecture/agent-model.md` — and a reference to an absent entry is worse than a short one. The full version, its `CONTEXT.md`, and the Codex compatibility audit that sourced it sit on a separate planning-docs branch not included in this release.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.37: Test Suite Escaped Its Sandbox and Corrupted the Real Repo (RESOLVED — this release)

**What happened:** running the test suite hijacked a git worktree of this repository, flipped the **main repo to `core.bare=true`**, rewrote its committer identity to a fixture's (`t <t@e.st>`), and stacked 10 fixture commits onto local `main` — the first of which **deleted all 511 tracked files**, leaving `main` pointing at a tree containing only `a.txt`. Discovered 2026-07-24 while adding macOS CI.

**Root cause (confirmed, reproduced deterministically):** `scripts/hooks/pre-push` runs `cargo test --workspace`. Git **exports `GIT_DIR` into hook environments when the gitdir is non-default — exactly the case when pushing from a linked worktree** (verified side by side in one repo: a push from a normal checkout gives the hook no `GIT_DIR`; a push from a worktree gives `GIT_DIR=<main>/.git/worktrees/<name>`). `GIT_DIR` **outranks the process working directory** when git resolves which repository to act on, and Rust runs a test binary's tests as threads in ONE process, so the whole suite inherited it and every fixture retargeted the real repo *despite* correctly pinning `.current_dir()`. Because a worktree's gitdir shares `config` with the main repo, the fixtures' `git init` set `core.bare=true` there and their `git config` rewrote the identity.

**Two earlier hypotheses were wrong and are recorded so they are not re-tried:** it is not a `set_current_dir`/cwd race of the `ENV_MUTEX` class (there is no `set_current_dir` anywhere in `crates/`, and an audit of every `.rs` file found **0** unpinned `Command::new("git")` on a 14-line window), and plain `git init` inside a linked worktree does **not** set `core.bare` (tested directly on git 2.55).

**Also verified independently:** `git -C <dir>` does **not** override `GIT_DIR` — only the `--git-dir` flag does — and `GIT_CEILING_DIRECTORIES` does **not** contain it. Pinning the working directory can therefore never be the containment mechanism; clearing the variables is.

**Fix — three layers, because no single one is sufficient:**

1. `scripts/hooks/pre-push` clears `$(git rev-parse --local-env-vars)` — git's own authoritative 15-var list, so it self-maintains across git upgrades — plus `GIT_NAMESPACE`/`GIT_DISCOVERY_ACROSS_FILESYSTEM`/`GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM`. Clearing `GIT_CONFIG_COUNT` is sufficient to neutralize `GIT_CONFIG_KEY_n`/`VALUE_n` (verified). `githooks(5)` prescribes exactly this for hooks touching a "foreign repository"; a fixture tempdir is one. **Prevents the known trigger.**
2. `devflow_core::test_support::git_command` applies the same scrub **per command**, behind an off-by-default `test-support` feature. 50 test-side call sites migrated across 12 files. **Contains fixtures on every other launch path** (`git rebase --exec`, `git bisect run`, another hook, CI).
3. `crates/devflow-cli/tests/git_env_hermeticity.rs` fails fast and names the cause when the environment is dirty. **Detection, not prevention** — tests run in parallel, so a fixture can run first; it exists so this presents as a diagnosis rather than the unrelated flake it originally looked like ("41 staleness failures").

**Validation — controlled A/B in an isolated clone**, same commit and worktree, only the hook differing: scrubbed, the push is green with 156/156 passing; unscrubbed, the push is **blocked** with 42 failures, `core.bare=true` and HEAD hijacked onto fixture branch `side` — reproducing the incident down to the branch name. Separately, running the suite with `GIT_DIR` set: before layer 2, the clone was corrupted; after, the repository is **completely unharmed**.

**Residual, by design:** under a dirty environment 37 unit tests still fail. They exercise *production* functions in-process, which are pinned but not scrubbed — see 999.39. Loud failure is the intended end state; scrubbing production to make them pass would mask that separate exposure.

**Blast radius was test-only for users:** all 86 production `Command::new("git")` invocations already pass `current_dir()`, so `cargo install devflow` users were unaffected. The victims were developers and contributors — via a normal `git push`, not an unusual action.

Plans:

- [x] Delivered in this release (hook scrub, per-command containment, hermeticity guard, `pre-commit` chaining shim)

### Phase 999.38: Test-Suite PATH Race Between ENV_MUTEX Mutators and Concurrent Git Callers (BACKLOG)

**Goal:** `run_git_stdout` (`crates/devflow-cli/src/staleness.rs:106`) resolves `git` through `PATH`, while several tests replace `PATH` **entirely** with a stub directory containing no `git` — `pipeline_launch.rs:590`, `pipeline_outcomes.rs:879/1132/1246`, `preflight.rs:627/701`. `ENV_MUTEX` serializes those mutators against *each other* but not against the ~155 other tests in the same binary that shell out to git concurrently, so `Command::new("git")` intermittently fails to spawn and `run_git_stdout` returns `None`.

**Evidence:** reproduced once in four full-suite runs — `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking` panicking at `staleness.rs:891` (`rev-parse HEAD`). This is a genuine flake source, distinct from 999.37, and was found while investigating it.

**Second mechanism, observed 2026-08-05 (Phase 33 review WR-05 + the 33-06 execution run).** The same `ENV_MUTEX`+`PATH` regions have a *second* failure mode this entry did not originally name: the restore is a trailing statement, and Rust abandons remaining statements the instant a panic begins unwinding. So a panic inside a region (a) leaves `PATH` pointed at a `neutral_path_dir` the unwind then drops and **deletes**, handing every concurrent test a `PATH` naming a nonexistent directory, and (b) poisons `ENV_MUTEX`, so every later `ENV_MUTEX.lock().unwrap()` panics with `PoisonError` — one legible failure becomes a cascade. Observed live during 33-06: a single `index.lock` failure in `concurrent_ship_advances_finish_both_phases_independently` cascaded into ~15 unrelated `PoisonError` failures. Phase 33-06 added a `NeutralPath` RAII guard (`crates/devflow-cli/src/test_support.rs:279`, `impl Drop` at `:300`) and applied it to **two** regions only; ten pre-existing regions still use the trailing-statement form (`pipeline_outcomes.rs:1280-1304`, `:1355-1383`, `:1434-1458`, `:1489-1508`, `:1545-1560`, `:1779-1798`, `:1850-1865`, `:1915-1930`; `pipeline_gate.rs:1140-1159`, `:1231-1246`). Two cheap partial mitigations short of this entry's full per-`Command` fix: retrofit `NeutralPath` to the remaining ten, and replace `ENV_MUTEX.lock().unwrap()` with `.unwrap_or_else(PoisonError::into_inner)` — the mutex guards a `()`, so no invariant can be protected by refusing a poisoned lock. See also 999.80, which needs three *more* such regions and must be sequenced against this entry deliberately.

**Note:** Rust 2024 makes `std::env::set_var`/`remove_var` `unsafe` precisely because this pattern is unsound in a threaded test binary — the fix direction is per-`Command` `env`/`env_remove` (as 999.37's `test_support` now does for git) rather than process-global mutation. Fixing this would let `ENV_MUTEX` shrink or disappear, which is worth more than the flake itself. Related: 999.15 (hermetic tests for shell entry points).

**Priority:** Medium | **Size:** M

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.39: Production Git Calls Inherit a Redirecting Environment (DONE — Phase 27, 2026-07-30)

**Goal:** DevFlow's production git invocations pin `current_dir()` but do **not** clear git's repository-local environment variables. `GIT_DIR` outranks the working directory, so any `devflow` process launched with one set — from a git hook, `git rebase --exec`, or `git bisect run` — silently operates on *that* repository instead of the project root it was given.

**Evidence:** with `GIT_DIR` set, 37 unit tests fail because production functions under test (e.g. `tag_exists_and_reachable`) resolve to the wrong repository. 999.37 deliberately did **not** paper over this: fixture containment was fixed, production left honest, so the failures name a real exposure rather than hiding it.

**Not the same severity as 999.37:** DevFlow is normally invoked directly, not from a hook, and the observed effect is wrong answers rather than corruption. But DevFlow *runs git hooks* as part of its own workflow, which is exactly the shape that sets these variables.

**Fix direction:** route production git calls through one scrubbing constructor, mirroring `test_support::git_command`. Decide deliberately whether an operator-set `GIT_DIR` should ever be honoured — the safer default is no.

**Priority:** ~~Medium~~ → **HIGH, escalated 2026-07-30** | **Size:** M — ~86 call sites, mechanical, but production behavior so it needs its own review.

**ESCALATED — this is no longer defense-in-depth hygiene; it is now load-bearing for a security guard.** Phase 26's re-review (`26-REVIEW.md`, 2026-07-30, finding **CR-01**) found that `mutating_project_root` — the guard added by 26-09 specifically to stop `release --execute`/`sync` acting on a repository the operator never named (D-13, closing C-06) — **is bypassed by an inherited `GIT_DIR`**. `git rev-parse --show-toplevel` reports the *cwd's work tree* while HEAD, refs and objects come from `GIT_DIR`, so the guard compares two paths, sees a match, and passes — while the executor pushes, tags, and publishes against a different repository. That is C-06's exact shape arriving through a second vector, and it defeats a guard written expressly to prevent it.

**Confirmed during that review:** there is **zero** `GIT_DIR` scrubbing anywhere in production code; the only mention in the entire repository is a doc comment inside a `#[cfg(test)]` block (`crates/devflow-cli/src/main.rs:786`). `crates/devflow-cli/tests/project_root_guard.rs:18-21` names the risk for its own helpers and never tests it against the guard.

**Why this changes the priority.** The original entry reasoned "DevFlow is normally invoked directly, not from a hook, and the observed effect is wrong answers rather than corruption." Both halves are now weaker: git sets `GIT_DIR` for hooks, `rebase --exec`, `bisect run` and `submodule foreach` — and DevFlow *runs git hooks as part of its own workflow* — while the effect is no longer wrong answers but **irreversible operations against the wrong repository** (`git push`, `git tag`, `cargo publish`). This is the same variable behind the 999.37 sandbox-escape incident, on the mutating path this time.

**Sequencing consequence:** **no mutating-command root guard can be trusted until this lands.** Any future work on the release executor (999.25) or `devflow sync` (999.52) that relies on root resolution for safety is building on a foundation with a known hole. Fix this first, or accept that the guard above it is decorative.

**Considered and deliberately excluded from Phase 26 (2026-07-29).** Confirmed still open at HEAD `76e49f1` — every production call site still pins `current_dir()` only, and `test_support::git_command`'s scrubbing exists solely inside `#[cfg(test)]` code. Real defense-in-depth value (the 999.37 incident class), but its ~86 call sites span most of `crates/` including `git.rs`, where Phase 26's 999.25/999.54/999.50/999.52 cluster is already working — folding it in risks the same file-overlap serialization tax Phase 26 is structured to avoid. Deferred to its own phase (27+), not dropped.

**PROMOTED 2026-07-30 → Phase 27** ("Scrub Redirecting Git Environment From Production Calls"), re-verified open at HEAD `b3cab1c`. Operator decision, taken immediately after Phase 26 closed PARTIAL: 999.25's re-attempt names this its prerequisite #1, so it leads rather than waits. Scope, evidence and rationale live here; the Phase 27 entry carries the plan breakdown and does not restate them.

Plans:

- [x] Tracked in Phase 27 — **CLOSED 2026-07-30.** All 6 plans executed and verified 7/7 must-haves; the 41 production `Command::new("git")` sites now route through `devflow_core::git::{hermetic_command, git_command}` and Sweep A returns 0. Residual: four indirect `sh -c` spawn edges (`hooks.rs:222`, `gates.rs:323`, `verify.rs:106`, `commands.rs::cmd_check`) are deliberately out of scope with a proposed backlog entry in `27-SPAWN-CENSUS.md`.

<!--
RENUMBERED 2026-07-26. The four findings below were originally filed as
999.40–999.43 by Phase 23, but 999.40/41/42 were already taken in Linear by
DEN-63 / DEN-64 / DEN-67 respectively — three unrelated items that had never
been mirrored back into this file, so the collision was invisible here. The
three colliding entries moved to 999.44/45/46; 999.43 was free and kept its
number. Next free backlog number is 999.47.
-->

### Phase 999.44: State-Orphaned Processes Are Unreachable by `stop` and `gate sweep` (PROMOTED — Phase 25)

**Goal:** A `devflow advance` process can outlive its own lock **and** its persisted state. When it does, both of Phase 23's new primitives return success while the orphan keeps running: `gate sweep` never lists it (its root is absent from the registry, so cross-root enumeration cannot see it), and `devflow stop --phase N --root PATH` reports `no lock held … nothing is running advance()` and exits 0.

**Evidence:** observed live 2026-07-26 while clearing the machine's orphan population with Phase 23's own tooling. 22 of 23 orphans were reaped by `devflow gate sweep`; PID 3744133 (`--phase 7`, root `/tmp/.tmpMVmZBl`, ~8.6h old) was unreachable. `stop` was tried against both phase 7 and phase 8 (the root's state file is `state-08.json`) — both returned the same no-lock/no-state message and exit 0. `kill -TERM` cleared it in 1s. Full detail: `.planning/milestones/v2.0.0-phases/23-end-to-end-dogfood/23-FINDINGS.md` §A1.

**Why it matters:** the messages are *true* against recorded state, so this is not a false attestation — but an operator reading exit codes concludes the machine is clean when it is not. This is the residue of the orphan class Phase 23 was written to close, and the one case still requiring `kill(1)`.

**ESCALATED 2026-07-27 — these orphans are SIGTERM-IMMUNE, contradicting this entry's own evidence above.** The 2026-07-26 note records "`kill -TERM` cleared it in 1s." That did **not** reproduce. Clearing the machine's population today: 15 orphaned monitor wrappers (all `ppid == 1965`, i.e. reparented to `systemd --user`, all rooted in `/tmp/.tmp*`) were sent `SIGTERM`; **all 15 survived**, re-checked after real elapsed time, not an instant re-poll. Only `SIGKILL` cleared them. Killing the wrappers then **orphaned their `devflow advance` children**, which reparented to `systemd --user` in turn and *also* required `SIGKILL` — 30 processes total, none responding to `SIGTERM`.

**Why that matters more than the enumeration gap.** Each wrapper installs `trap cleanup TERM INT` where `cleanup` kills `$apid` and exits. That handler is evidently not firing — the most likely mechanism is the shell being blocked in `wait $apid` on a child it can never reap. **Any reaping path built on `SIGTERM` will therefore report success and leave the process running**, which is a strictly worse failure than the silence this entry already describes. The two-layer structure (wrapper + child, each independently reparented) also means a reaper must handle *both* layers; killing only the wrapper manufactures a fresh orphan.

**Fix direction, revised:** a registry-independent path — scan for running `devflow advance` children and reconcile against the registry, surfacing "running but unregistered" as its own reportable class rather than silence. Likely belongs in `doctor` as a finding plus a `gate sweep` flag. It **must** escalate `TERM` → `KILL` with a bounded wait and verify death rather than assuming it, and must reap the wrapper/child pair together. Add a regression test asserting a `TERM`-ignoring child is still cleared.

**RECURRENCE CONFIRMED 2026-08-06 (Phase 34 execution).** Census during Phase 34 — five worktree-isolated executors plus one main-checkout executor over ~14h — found **10 real `devflow` processes** (filtering on `comm == devflow`), **3 of 10** reparented to `systemd --user`, oldest three at 22h27m / 12h23m / 11h35m, **87 MB** resident, **every root a `/tmp/.tmp*` scratch dir and none in a real repository**. The wrapper/child pair structure reproduced: 9 `sh -c apid=…` wrappers alongside the binaries. This entry's characterisation is unchanged; no new entry was filed. Full census on DEN-68.

**Two measurement traps for whoever builds the reaper**, both hit while taking that census:

- **`pgrep -f` over-reports.** `pgrep -f 'devflow (advance|__monitor)'` returned **12** where `comm == devflow` returned **10** — the wrapper shells carry the devflow command in their own argv and match the pattern. A census built on it over-reports, and a reaper built on it signals the *wrapper*, which this entry already notes manufactures a fresh orphan.
- **`etime` must be sorted numerically.** String-sorting `ps -o etime=` ranks `27:14` above `22:25:31`. Use `etimes`. The "oldest orphan" figure is the one most likely to be quoted, and the string sort silently understates it.

**`SIGTERM` immunity did NOT reproduce on 2026-08-06.** Sweeping the population above — 10 `devflow` plus 7 wrappers, reaped child-then-wrapper — **all 17 died on `TERM`; zero needed `KILL`.** That contradicts the 2026-07-27 escalation (15/15 survived `TERM`), which is the observation that raised this entry to High. It does not refute it: with 2026-07-26 ("cleared in 1s"), 2026-07-27 (15/15 survived) and today (17/17 died), the behaviour is **variable**, not immune. That *strengthens* the fix direction rather than weakening it — a reaper assuming immunity is as wrong as one assuming compliance, so the existing "escalate `TERM` → `KILL` with a bounded wait and verify death rather than assume it" requirement is exactly right. Priority stays High on the enumeration gap regardless. Untested hypothesis worth checking when this is picked up: whether immunity tracks the child's blocked wait state rather than its age.

**Sweeper self-kill — a trap for the proposed `gate sweep` flag.** The first sweep attempt terminated itself (exit 144): an inline `bash -c` script's own command line contains the search patterns, so `pgrep -f devflow` and `pgrep -f apid=` both matched the sweeper. Any reaper implemented as a shell helper must exclude `$$` explicitly, or run from a file so its argv does not contain its own patterns.

**Agent worktrees — a non-complication, recorded so it is not re-investigated.** Six of the ten processes execute binaries under `.claude/worktrees/agent-*/target/debug/devflow` whose worktrees were since removed, so `/proc/PID/exe` reports `(deleted)`. This does **not** affect the identity model: `agent.rs` identifies processes by recorded `(pid, starttime)` precisely because `/proc` lies ("identity must be recorded, never inferred"). The only implication is narrow — the registry-independent *scan* in the fix direction above must tolerate exe paths carrying `(deleted)` and pointing inside removed worktrees, rather than reading an unresolvable path as "not a devflow process."

**Priority:** High — raised from Medium. `SIGTERM` immunity means the documented recovery path silently fails, and the fix direction as originally written (enumeration only) would not have cleared a single one of today's 30 processes. | **Size:** M — needs a PID-discovery mechanism that is safe on shared machines and does not misidentify unrelated processes. Linear: DEN-68.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.45: Registry Emits Duplicate Entries for a Single Root (BACKLOG)

**Goal:** `registry.rs` records more than one entry for the same `(root, phase, stage)` triple, inflating what `gate sweep --dry-run` reports.

**Evidence:** 2026-07-26 sweep of the machine's orphan population — `/tmp/.tmpal43JM` appeared four times in the dry-run listing (`phase 7 code` ×2, `phase 8 code` ×2, identical ages per pair). Dry run reported `24 would be reaped`; the real sweep reported `22 reaped, 0 skipped, 0 left alone`. Detail: `23-FINDINGS.md` §A2.

**Why it matters:** low functional severity — the sweep is idempotent and no double-reap occurred — but the dry-run count is exactly what an operator reads before authorizing a destructive sweep, and it over-reports. Trust in the preview matters more than the two-entry delta.

**Priority:** Low | **Size:** S — dedup on `(root, phase, stage)` at write or read; add a regression test asserting dry-run count equals executed count. Linear: DEN-69.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.46: E2E Suites Leak Scratch Roots; Process Leak Does Not Reproduce on a Clean Run (BACKLOG)

**Goal:** `gate_sweep_e2e.rs` and `stop_e2e.rs` (Phase 23) and older phase-12 fixtures spawn real, separate `devflow advance` children. Their `/tmp/.tmp*` scratch roots are not removed at teardown and accumulate without bound.

**Evidence:** 23 orphaned processes (~128 MB resident, oldest 11h45m) accumulated across roughly 12 hours of Phase 23 development, all in `/tmp/.tmp*` scratch roots created by these suites; none touched a real repository. Detail: `23-FINDINGS.md` §A3.

**SCOPE CORRECTED 2026-07-26 (post-phase-23 verification).** The original entry claimed "every full `cargo test --workspace` refills the machine's orphan population." **That is not reproducible on the current tree and the title has been corrected accordingly.** Two consecutive full runs (`cargo test --workspace --no-fail-fast`, true cargo exit 0, 592 passed / 0 failed across 16 binaries) left **zero** orphaned `devflow advance` processes — `wait_for_child_exit`'s bounded `wait()` in both suites does reap the direct child. What those two runs *did* leak is **2 scratch directories**. The standing population is **410 dirs / 300 MB spanning 2026-07-21 → 2026-07-26**, i.e. accumulated from interrupted and crashed development runs plus the roots held open by the genuinely orphaned processes, **not** from clean runs. Whoever picks this up should scope it as directory-lifecycle hygiene and treat any process-reaping work as belonging to 999.44 instead.

**RE-OPENED 2026-07-27 — the process leak DID reproduce, and the prior "zero" may be a measurement artifact.** During Phase 24's acceptance run, two orphaned monitor/agent pairs were created at 05:21 and 05:26, squarely inside the Code stage window (05:04:29 → 05:41:20), on a stage that completed normally (exit 0, clean transition to Validate) — not an interrupted or crashed run. Their scratch roots `/tmp/.tmp6087E2` and `/tmp/.tmpczQmqp` still exist and still carry `phase-12-*` fixture paths, matching this entry's named suites.

**Census at 2026-07-27:** **14 orphaned monitor/agent pairs (28 processes, ~123 MB RSS, oldest ~19h)**, and **456 scratch dirs / 302 MB** — up from the 410 dirs / 300 MB recorded above.

**A subreaper caveat, worth knowing but NOT the explanation.** These processes reparent to `systemd --user` (pid 1965 on this host), **not** to pid 1, because `systemd --user` sets itself as a child subreaper. A census testing `ppid == 1` therefore returns zero orphans on any systemd host even while dozens exist (`ppid==1` counted 0 while `ppid==1965` counted 14). Any future census must be subreaper-aware. This was initially offered here as the explanation for the "clean runs leak nothing" result above — **that hypothesis was tested and is wrong; see below.**

**Re-measured the same day with a subreaper-aware census — the scope correction above HOLDS for clean runs.** A full `cargo test --workspace && cargo clippy && cargo fmt --check` chain in the phase-24 worktree (618 passed / 0 failed / 17 binaries, chain exit 0) was bracketed by a `ppid`-agnostic census: **14 orphaned pairs before, 14 after — zero new.** That is a third clean run corroborating the original two, this time immune to the `ppid == 1` flaw. The earlier scope correction was right, and the measurement-artifact hypothesis is retracted.

**SETTLED 2026-07-27 by a controlled experiment from a genuinely clean baseline.** The machine's entire orphan population (30 processes, 456 scratch dirs) was cleared first, so for the first time the measurement had no pre-existing contamination. Then one full `cargo test --workspace` (618 passed, exit 0), bracketed by a census:

| Metric | Before | After | Delta |
|---|---|---|---|
| `devflow advance` processes | 0 | 0 | **0** |
| `/tmp/.tmp*` scratch dirs | 2 | 3 | **+1** |

**Conclusion, now on firm evidence rather than inference: a clean `cargo test --workspace` leaks ZERO processes and exactly ONE scratch directory.** The original scope correction was right on both counts, and this entry is correctly scoped as directory-lifecycle hygiene. The `ppid == 1` measurement-artifact hypothesis floated earlier today is retracted (see above); it was worth testing and it was wrong.

**Where the process leak actually comes from.** Not clean runs. The orphans cleared today were rooted in `/tmp/.tmp*` fixtures from **interrupted** runs — including several using `.worktrees/phase-24/target/debug/devflow`, i.e. created during Phase 24's Code stage, where GSD's execute flow runs tests under `workflow.test_gate_timeout`. A timeout killing a test process mid-run orphans whatever it spawned, which matches every property observed. Process reaping therefore belongs to **999.44**, and that entry has been escalated after today's discovery that these orphans are `SIGTERM`-immune.

**Priority:** Low, confirmed — the clean-run claim is settled and the remaining scope is directory hygiene (~1 dir per full test run).

**Do not weaken the tests to fix this.** Spawning a real child is what makes them strong — plan 23-04's summary correctly calls it the suite's strongest claim, and plan 23-05's `stop_e2e` depends on the same property. The fix belongs in teardown, not in the assertion.

**Fix direction:** a `Drop` guard or explicit teardown that stops each spawned child, ideally exercising `devflow stop` so the cleanup path is itself under test.

**Priority:** Low (downgraded from Medium with the scope correction — no process leak on clean runs) | **Size:** S — test-harness only, no production change. It is a contributing source of the population 999.44 is about, not the whole of it. Linear: DEN-70.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.43: `devflow cleanup` Deletes Recovery Refs by Ancestry (BACKLOG)

**Goal:** `devflow cleanup` deletes local branches it judges "merged" by ancestry, which necessarily includes recovery points. A recovery ref is *defined* as a pointer at a known-good commit, so it is always an ancestor and therefore always eligible for deletion.

**Evidence:** during plan 23-11, `devflow cleanup` (run for worktree/branch hygiene) deleted the local `recovery/pre-23-11-acceptance-e0f87c2`. The copy on `origin` was untouched and the local branch was restored from it in the same session, so nothing was lost — but the deletion was unintended and was disclosed rather than silently corrected. Recorded in `23-ACCEPTANCE-RUN.md` §6/§7 and `23-FINDINGS.md` §B2.

**Why it matters:** the whole point of establishing a recovery point before a one-way operation is that it survives until the operator retires it. A cleanup verb that removes it — even locally, even while the remote persists — undermines the mitigation at exactly the moment it is supposed to be load-bearing.

**Fix direction:** skip a configurable ref prefix (`recovery/*` by default), or have cleanup refuse to delete refs it did not create. Linear: DEN-71.

**Priority:** Low | **Size:** S — one predicate plus a test.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.47: `looks_like_devflow_process` False-Positives Under CI Load (PROMOTED — Phase 25)

**Goal:** `agent::looks_like_devflow_process(pid)` intermittently returns `true` for a plain `sleep` process. It is the last guard before `SIGTERM` in `devflow stop` (`commands.rs:1171`), so a false positive is the **dangerous** direction — it *permits* signalling a process that is not DevFlow's, which is exactly the recycled-pid hazard its own error message names ("the lock may be stale with a recycled pid").

**Evidence — TWO independent tests at two layers catch this, 3 failing CI runs across 2 commits on 2026-07-26**, both commits touching no Rust source at all (`e00a16d`, `8929236` — `.planning/`-only). The ~20 CI runs before these were green. Introduced with the predicate itself in `dec4583` (plan 23-05).

1. `agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process` (`agent.rs:142`) — the unit test. Failed on `8929236` in both the CI and Devcontainer workflows.
2. `commands::tests::stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check` (`commands.rs:3163`) — the CLI integration test. Failed on `e00a16d`.

**(2) is the serious one and proves the full production path, not just a wrong bool.** That test spawns a `sleep`, writes its pid into the phase lock file, and asserts `stop()` returns `Err`. It panicked on `expect_err` — so `stop()` returned `Ok`: the identity guard passed and **`devflow stop` sent `SIGTERM` to an unrelated `sleep` process.** This is the guard's stated purpose failing end-to-end, observed in CI, not a theoretical risk inferred from a predicate.

The same commit passes on one trigger and fails on the other, and the pattern flips between commits, so it is non-deterministic rather than environment-specific.

**FAILURE STATE ESTABLISHED 2026-07-26** — from the instrumented assertion, once CI moved into the pinned container (`db1b4bf`) and the flake became **deterministic** rather than ~33%. Verbatim:

```
branch taken:        spawned sleep
pid under test:      3054
cmdline before stop: /__w/devflow/devflow/target/debug/deps/devflow-c2105e00bf2afc4e
cmdline after stop:  /__w/devflow/devflow/target/debug/deps/devflow-c2105e00bf2afc4e
status before stop:  Name=commands::tests State=R (running) Pid=3054 PPid=2856 Threads=1
child.try_wait():    Ok(None)
direct predicate:    looks_like_devflow_process(3054) = true
test process:        pid 2856
```

**SUPERSEDED 2026-07-26 — see "MECHANISM CONFIRMED" below.** The reading in this paragraph ("never execs, persistently") was wrong: both of its samples happened to land inside a window that is merely *wide*, not permanent.

**The child forks and never `exec`s, and stays that way.** `PPid` is the test process, so it *is* our child; `try_wait()` is `Ok(None)`, so it has not exited; `Name` is `commands::tests` — the *forking Rust test thread's* name, not `sleep`; and the state is unchanged across the whole `stop()` call. It therefore still carries the parent's `cmdline`, which is why the predicate answers `true`.

This **rules out**, with evidence: pid recycling (PPid matches), a transient read window (stable before *and* after), a matching-logic bug in the predicate (the cmdline genuinely is a devflow binary's), and fixture self-sabotage (the `spawned sleep` branch was taken).

**WHY the child never `exec`s is still unknown, and three hypotheses are now disconfirmed. Do not adopt one without evidence:**

- **Local CPU contention** — 40/40 pass with the machine loaded.
- **Transient fork/exec cmdline inheritance** — 4000 spawns × 3 modes, including `pre_exec` to *force* the `fork`+`execvp` path off `posix_spawn`, observed it **0 times**. (An earlier revision called this "disproved", then retracted it as an overstatement; the CI evidence above now disproves the *transient* form outright, since the state is persistent, while leaving "never execs" as the actual finding.)
- **glibc environment-lock deadlock inherited across `fork`** (a `setenv` in another thread holding the lock when the child forks, wedging `execvp`'s PATH lookup) — a 4-thread `setenv` storm across 300 spawns wedged **0** children.

**MECHANISM CONFIRMED 2026-07-26 — transient fork/exec window, observed directly.** From the `agent.rs` unit test in the devcontainer job:

```
child cmdline before: /workspaces/devflow/target/debug/deps/devflow_core-e2538b0c9f19931c
child cmdline after:  <empty>
child /proc/7335/exe: /usr/bin/sleep
```

`/proc/<pid>/exe` resolves to `/usr/bin/sleep`, so the child **did** exec. The cmdline read caught it mid-flight: the parent's argv beforehand, empty afterwards. Between `Command::spawn()` returning a pid and the child completing `execve`, `/proc/<pid>/cmdline` still reports the *parent's* command line — and `looks_like_devflow_process` reads exactly that.

**This is the hypothesis that was twice recorded above as "not reproduced" and then "disproved". Both retractions were wrong, and the fault was the instrument, not the theory.** The local probes (3000 and 4000 spawns, including `pre_exec` to force the `fork`+`execvp` path) ran on a warm local filesystem where `execve` completes in microseconds, so the window was never observable. Container overlayfs makes it wide enough to hit routinely. The earlier "persistent, never execs" reading came from a sample pair that both landed inside that wide window.

**Lesson for the next reader:** a negative result from a probe is evidence about the probe's sensitivity, not proof about the system. Reproduce in the environment that fails.

**The fix does not depend on resolving that.** Whatever wedges the child, the production defect is that `looks_like_devflow_process` trusts `/proc/<pid>/cmdline`, which a forked-not-`exec`'d child inherits wholesale from its devflow parent. `/proc/<pid>/exe` is no better — it is inherited too. **Identity must be a recorded pair, not an inference:** persist `(pid, starttime)` — field 22 of `/proc/<pid>/stat` — in the lock file and require both to match before signalling. That is immune to inheritance *and* closes the check-then-signal TOCTOU below, so it subsumes both defects.

**Both tests are instrumented** (`21449bd`, `a6f479a`; no production change) and CI reproduces deterministically in-container, so any future hypothesis can be tested in one push.

**Not to be confused with the unrelated flake in the same PR.** `reference_and_cleanup_worktree_cli_flow` (`phase7_cli.rs:107`) also failed on `91bff73` in both workflows. That is the pre-existing, already-tracked 20b flake (Linear DEN-48, High, Todo) — a git-fixture problem, unrelated to this predicate. Three distinct tests failed across this PR; do not merge them into one story.

**Known independent weakness, visible by inspection:** the predicate matches **any** argv element whose basename starts with `devflow`, not just `argv[0]`. So `sleep /tmp/devflow-scratch/x` matches, as does any process merely *mentioning* a devflow path in its arguments. Tightening to `argv[0]` and corroborating via `/proc/<pid>/exe` narrows the false-positive surface regardless of the intermittent cause.

**Related TOCTOU, likely 999.44's scope:** even a correct predicate is checked and *then* acted on — the pid can be recycled between `looks_like_devflow_process(pid)` and `terminate(pid)`.

**Priority:** High — raised from Medium once test (2) showed `devflow stop` actually signalling a non-devflow process, rather than a predicate merely returning the wrong value. | **Size:** S–M — S if the diagnostics name a simple cause, M if it needs a non-pid identity handle. Linear: DEN-72.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.48: Driving Binary Is Re-Checked for Staleness Mid-Run, Halting Self-Modifying Phases (PROMOTED — Phase 25)

**Goal:** `enforce_build_staleness` is called inside `launch_stage` (`pipeline_launch.rs:93`), so D-18's staleness check re-runs at **every** stage launch, not once at `devflow start`. When DevFlow drives a phase that modifies DevFlow's own source, the first stage boundary after that code lands re-evaluates the binary against the now-ahead worktree, classifies it `Stale`, and hard-blocks. Such a phase can never complete unattended.

**Evidence — Phase 24, 2026-07-27.** The run reached Define→Plan→Code fully unattended (a project first; no phase had previously passed Define), then halted at the Validate boundary with `self_dogfood_stale_blocked`. The Code stage itself succeeded: exit 0, 15 commits, both plans complete with SUMMARYs, review, and UAT. The build-affecting diff that tripped the guard was `crates/devflow-core/src/git.rs` and `crates/devflow-cli/tests/release_check.rs` — i.e. the phase's own deliverable. Detail: `23-ACCEPTANCE-RUN-3.md`.

**This was structurally guaranteed at scoping time.** Phase 24 was promoted as the acceptance target for being "low-stakes *by consequence*" — a preflight advisory touching no merge/version/ship control flow. That criterion measured blast radius, not self-modification. Any phase editing DevFlow's compiled source hits this regardless of blast radius.

**Fix direction — pin the driver.** Once the initial check passes at `devflow start`, hold that verdict for the remainder of the phase run instead of re-evaluating against the evolving worktree. Record the pinned commit in the event log so the provenance stays auditable.

**Rejected alternatives, with reasons (do not re-propose without addressing these):**

1. **Rebuild the binary mid-run.** Adopts *unvalidated* code into the driver and makes the change under test partly responsible for certifying itself. Rejected by the operator 2026-07-27: *"I don't want unvalidated code to be used to rebuild the binary mid-run. Only validated and pushed code should ever be used. What if the code fails validation?"* Confirmed unnecessary as well as unsafe: `check_ssh_signing_viability` has one caller (`git.rs:831`) and is on no pipeline path — `hooks_after_ship` is Merge→VersionBump→ChangelogAppend→BranchCleanup, and neither Validate nor Ship invokes `release --check`. The rebuild would have carried all of the risk for none of the benefit.
2. **A dogfood bypass flag.** `is_self_dogfood_workspace` (`staleness.rs:240`) already restricts the hard block to a `Cargo.toml` whose `members` is exactly `crates/devflow-core` + `crates/devflow-cli` — this repository and nothing else, with tests asserting lookalikes don't trip it. A "bypass for dogfooding" therefore disables the guard in the only circumstance it ever fires: equivalent to deleting D-18. It also contradicts D-05's precedent that a dangerous authorization must be typed per-invocation and never become a standing default — and a stale binary is worse than `--yes-ship`, which at least produces a visible merge rather than silently wrong evidence.

**Design principle:** separate **driver** from **subject**. The driver's trustworthiness comes from *provenance* — built from a validated, pushed, CI-green commit — not from matching the worktree it happens to drive. The subject is validated by `cargo test` compiled from source, which never consults the installed binary. D-18 currently conflates the two. A stronger variant worth evaluating: drive self-dogfood runs from an installed release rather than `target/release/` of the repo under test.

**Do not weaken D-18 generally.** It is correct for its originating incident (Phase 16 false evidence: committed, forgot to rebuild). The defect is its *scope*, not its existence.

**Size re-assessed 2026-07-27 — S, not S–M.** The distinguishing information already exists: `launch_stage`'s third parameter `archived_stage: Option<Stage>` is `None` for a fresh start (`commands.rs:236`) and `Some(stage)` for a transition (`pipeline_gate.rs:110/122`, `pipeline_outcomes.rs:362/370`). The fix is therefore `if archived_stage.is_none() { enforce_build_staleness(…)?; }` — one guard, no new state, no design pass. `pipeline_launch.rs:165` already documents this shape for the `Advance` arm. The earlier estimate was made before checking whether the parameter existed.

**Keep the module; do not delete it.** `staleness.rs` (1,794 lines, 21 tests) is gated entirely on `is_self_dogfood_workspace` and so is genuine dogfooding-only overhead — a fair deletion candidate on cost grounds. It is retained because the outstanding work is a single line, and discarding a tested, working module to avoid one guard is the worse trade. Note the module's tests are a known flake source (999.38's PATH race is `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking`, `staleness.rs:891`), which argues for fixing 999.38 rather than for deletion.

**Priority:** High | **Size:** S — one guard on an existing parameter. Linear: DEN-73.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.49: `compute_version` Derives a Nonsensical Version from Tag Count and Describe Distance (PROMOTED — Phase 25)

**Goal:** `compute_version` (`version.rs:142-150`) composes major from `Cargo.toml`, minor from `count_git_tags()` — a raw `git tag` line count with **no semver filter at all** — and patch from `commits_since_last_minor_tag()` via `git describe --tags --abbrev=0` distance. None of the three tracks semver intent.

**Measured 2026-07-27 at `origin/develop` `9916e2f`:** major=`1`, minor=`11` (eleven tags, one being the non-version `archive-planning-docs-2026-07-24`), patch=`359` (distance from `v1.4.0`). Computed version ≈ **`1.11.359`**, against a real project version of `1.8.1` and a `CHANGELOG.md` staging `2.0.0`.

**Root cause of the describe anomaly — verified, and this corrects an earlier hypothesis.** A previous analysis attributed it to git's default `--candidates=10` being exceeded at 11 tags; that is wrong, and `--candidates=20`/`50` change nothing. `git describe` selects the tag with the **fewest commits to HEAD**. This repository tags releases on squash-merges landing on `main`, then folds them back with `-X ours` sync merges. `v1.4.0` sits on a develop-side commit (distance 359); `v1.5.0`..`v1.8.1` sit on the main-side chain (distance 656+). `git merge-base --is-ancestor v1.4.0 v1.8.1` exits 1 — genuinely divergent lineages. So an older tag legitimately registers as "nearer" than a newer one.

**Impact.** `hooks_after_ship` runs Merge → VersionBump → ChangelogAppend → BranchCleanup as a **fail-fast batch with no rollback**, and `merge_feature`'s doc comment states the no-rollback policy explicitly. Any DevFlow-driven ship writes `~1.11.x` into `Cargo.toml`, tags it, and appends a changelog entry describing it — onto `develop`, after the merge has already committed.

**Why this is still open.** Surfaced twice (23-14 pre-run analysis; again 2026-07-27 pre-launch) and deferred both times *because no acceptance run ever reached Ship*. Phase 24 shipped by manual PR, which bypasses `hooks_after_ship` entirely. The defect is therefore fully latent and will fire on the **first** DevFlow-driven ship that ever succeeds — including the next acceptance attempt.

**Priority:** High — it is the one known defect guaranteed to corrupt a real release, and it is armed the moment 999.48 unblocks an end-to-end run. | **Size:** S — filter `count_git_tags` to semver tags and anchor the patch count to the highest reachable version tag rather than `git describe`'s nearest. Linear: DEN-74.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.50: `release --check` Tag-Signing Gate False-Negatives When the Key Is Not in `ssh-agent` (REMOVED — 2026-07-29)

**Was:** `check_ssh_signing_viability` (`crates/devflow-core/src/git.rs`) false-negatives release preflight when the agent holds other keys but not the configured one, even though the private key file on disk would let `git tag -s` succeed anyway (measured live during the v2.0.0 cut).
**Removed during Phase 26 discuss-phase, after briefly being promoted there:** the operator determined DevFlow should never predict tag-signing viability at all — a predictor is a second implementation of "will signing work?" that must stay in sync with git's real behavior, which is exactly the bug class this item and 999.54 are about. The executor built in Phase 26 instead runs the real signed `git tag` command directly and reads git's own result — no viability guess needed. Full reasoning: `superseded/26-release-cut-automation/26-CONTEXT.md` D-10. Linear: DEN-75 (close as won't-do).

### Phase 999.51: `devflow start` Resolves Its Base From a Possibly-Stale Local `develop`, and Never Fetches (PROMOTED — Phase 25)

**Goal:** `devflow start` resolves its base branch from the **local** `develop` ref — `DEVELOP` is the literal string `"develop"` (`config.rs:17`), consumed by `ensure_phase_reachable_on_base` (`commands.rs:146`) and by the worktree fork (`commands.rs:285`). Nothing in the start path fetches: `commands.rs:1877` and `:1980` state the design explicitly — checks run "against ALREADY-FETCHED local refs, issuing NO `git fetch`". So when local `develop` is behind `origin/develop`, an unattended run either refuses for a phase that demonstrably exists on the remote, or forks its worktree from a stale base and runs the phase against outdated code.

**Evidence — the 2026-07-27 acceptance run required a human to fix this before launch.** Local `develop` was `0 ahead / 21 behind` `origin/develop`. Phase 24's heading was present on `origin/develop` and absent from local `develop`, so `ensure_phase_reachable_on_base` would have refused a phase that was plainly reachable on the remote. The operator ran `git fetch origin develop:develop` by hand before launching. That manual step is the entire finding: **an unattended run has no operator to perform it.**

This is the *second* time the base ref has broken an acceptance attempt in a different way. `23-ACCEPTANCE-SETUP-2.md` records the same class on 2026-07-26 (local `develop` 120 behind, adjudicated and hand-fixed in plan 23-14). Both attempts needed a human to reconcile the base before `devflow start` could work.

**The dangerous variant is the silent one.** A refusal is loud and recoverable. But when the phase heading *does* exist on a stale local `develop` while the code does not, the guard passes and the run forks from an outdated base — producing a green run against the wrong source. That is the false-evidence shape D-18 exists to prevent, arriving through the base ref instead of the binary.

**Distinct from 999.28 (DEN-51).** That item adds an explicit `--base <branch>` override so a phase can stack on an unmerged predecessor. This is not about *which* branch is chosen but about whether the chosen branch is *current*. Fixing 999.28 alone leaves this open, and vice versa.

**Fix direction:** resolve the base through a ref that cannot be stale, or make staleness impossible to ignore. Options, roughly in increasing cost: (1) fetch the base before resolving it, which is the smallest change but adds a network dependency to `start`; (2) resolve against `origin/develop` rather than `develop`, matching what `ship`'s divergence check already does, and require the local ref only where a working tree is genuinely needed; (3) keep local resolution but compare against the remote-tracking ref and refuse loudly with the exact `git fetch` command when they differ — no network on the happy path, and never a silent stale-base run. Whichever is chosen, the "heading present but code stale" case must be closed, not just the "heading absent" one.

**Priority:** High — it has now forced manual intervention on two of three acceptance attempts, which is disqualifying for a goal whose whole definition is "unattended", and its silent variant can produce a green run against the wrong source. | **Size:** S–M — S for the refuse-loudly variant, M if `start` gains a fetch and its failure modes. Linear: DEN-76.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.52: DevFlow Imposes a Branch Model Whose Required Repair Step It Does Not Ship (PROMOTED — Phase 26)

**Goal:** DevFlow hard-codes a git-flow branch model on every project it drives — `MAIN`, `DEVELOP`, `FEATURE_PREFIX` are constants in `config.rs:15-19`, not configuration. That model has a required maintenance step DevFlow neither performs nor provides: when a user squash-merges `develop` → `main` for a release, the squash commit has **no parent relationship** back to `develop`, so `develop` never learns `main` moved and the *next* release conflicts against a stale merge-base.

**DevFlow diagnoses this and offers no remedy.** `release --check`'s divergence gate correctly reports `origin/main is NOT an ancestor of HEAD — develop has diverged` (verified live 2026-07-27, after this repository's own v2.0.0 sync PR was squashed). But the fix — `scripts/sync-main-to-develop.sh` — exists only in this repository: it is not a subcommand (`devflow --help` has no `sync`), and it lives outside the crate directories so cargo does not package it. A user is told something is wrong and left to derive `-X ours`, the tree-identity verification, and the must-not-be-squashed constraint themselves.

**Not merely theoretical, and not merely a dogfooding artifact.** All three of those details went wrong *for this project, with the script in hand*, during the v2.0.0 release on 2026-07-27: the sync PR was squashed (auto-merge defaults to it), destroying the ancestry link and requiring a second PR. The same repository previously hit the underlying divergence going into v1.5.0, producing conflicts across 11 files including core Rust source.

**Feature → develop is unaffected.** `merge_feature` uses `git merge --no-ff` (`hooks.rs:158`), a real merge, so DevFlow's own ship hook never creates this problem. The gap is strictly the `develop` → `main` hop, which is manual for users because `devflow release` is `--check` only (999.25).

**Fix direction:** ship the capability, not just the diagnosis. A `devflow sync` subcommand carrying the script's logic — `-X ours`, verify the resulting tree is byte-identical before proceeding, refuse if it is not — plus a pointer to it from the `release --check` divergence message, so the tool that reports the problem also names the command that fixes it. Folding it into 999.25's release executor is the alternative; doing neither leaves users with a diagnosis and no cure.

**Priority:** Medium — no data loss and the divergence is detected, but it degrades silently into painful merge conflicts at exactly the moment (a release) when a user least wants them, and DevFlow created the condition by imposing the branch model. | **Size:** S–M — the logic already exists in shell and is proven; the work is porting it, wiring the subcommand, and cross-referencing the check. Linear: DEN-77.
**Promoted:** Phase 26, 2026-07-29 — re-verified open at HEAD `76e49f1` before promotion; bundled with 999.25 as its executor's sync step.

Plans:

- [x] Promoted to Phase 26 — see the Phase 26 entry for the active tracking

### Phase 999.53: CI Cannot Reproduce the Load Shape That Catches Exec-Visibility Races (BACKLOG)

**Goal:** `.github/workflows/ci.yml` runs `fmt`, `clippy` and `test` as **three separate parallel jobs** on `ubuntu-24.04`, each invoking its own `scripts/check.sh <part>`; the `Test` job runs `scripts/check.sh test` alone. Every observed reproduction of 999.47 required the **sequential** `fmt → clippy → test` ordering on a loaded 2-core box — the shape `scripts/check-in-container.sh all` produces under `taskset -c 0,1`, which is what the `pre-push` hook runs. Splitting the checks into parallel jobs destroys exactly the condition that produces the race.

**CI is therefore structurally incapable of catching this defect class.** It rejected 0 of the pushes the local `pre-push` gate rejected 2 of 2. Phase 25 had to use six local push-gate observations as its sensitive instrument, with the five CI trials discharging a different standard (19-RESEARCH.md D-11's CI-on-branch requirement) rather than bounding the residual — recorded as `## Limits of this evidence` point 3 in `25-CI-TRIALS.md`. The gap is permanent, not phase-scoped: nothing in CI today would catch a regression that reintroduces a spawn-then-cmdline-census site without a barrier.

**Fix direction:** GitHub's `ubuntu-24.04` standard runner is **already 2-vCPU** — the same core count `taskset -c 0,1` simulates — so one added job running `scripts/check.sh all` reproduces the sensitive shape natively, with no `taskset` and no container indirection. Keep the existing three jobs for fast parallel feedback. Verify the 2-vCPU claim at implementation time rather than trusting this entry; if GitHub has moved standard runners to 4-vCPU, an explicit `taskset -c 0,1` wrapper is needed to preserve the load shape. Sensitivity is probabilistic — the historical per-run rate was ~50% — so the job comment should say so, or a future reader will over-trust a green run.

*Surfaced by the operator during Phase 25's 25-13 human sign-off (2026-07-28), while questioning whether the 999.47 closure evidence could be taken from GitHub CI instead of the local push gate. Recorded as a follow-up in `25-13-SUMMARY.md` Part A; explicitly out of scope for 25-13, which changes no source file by design.*

**Priority:** Medium — no active defect and 999.47's sites are now barriered, but this is the only standing guard that would catch a reintroduction, and its absence is why Phase 25's closure evidence had to be gathered by hand. | **Size:** S — one job block in `ci.yml` plus a comment stating the probabilistic limit. Linear: DEN-78.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.54: `release --check` Tag-Signing Viability Reads the Wrong Config Key (REMOVED — 2026-07-29)

**Was:** `check_signing_viability` reads `user.signingkey` (the agent's ordinary key) instead of `devflow.releaseSigningKey` (the maintainer's release key), so it false-negatives release preflight on a correctly-configured maintainer machine — measured live during the v2.1.0 cut.
**Removed during Phase 26 discuss-phase, after briefly being promoted there:** same disposition as 999.50 — the operator does not want DevFlow predicting tag-signing viability at all, ever. Full reasoning: `superseded/26-release-cut-automation/26-CONTEXT.md` D-10. Linear: DEN-79 (close as won't-do).

### Phase 999.55: `phase7_cli::wait_for` Fixed 5s Budget Times Out Under Load (BACKLOG)

**Goal:** `crates/devflow-cli/tests/phase7_cli.rs`'s `wait_for` polls for artifacts written by a real spawned monitor on a hard-coded budget — `for _ in 0..200 { … sleep(25ms) }`, five seconds, no backoff, no scaling, no env override. **7 call sites** use it, so the exposure is wider than the one test that has tripped.

**Two failures on 2026-07-28,** both on docs-only changes so neither could have caused it: GitHub Actions' `Build + test in devcontainer` on PR #48 (`timed out waiting for …/gates/08-validate.json`, while the identical job passed on the parallel run of the same commit), and the local `pre-push` container gate rejecting the first push of PR #52 (passed on immediate retry, nothing changed). Load-dependent, not random — both occurred alongside concurrent sibling jobs.

**This is not a suite-quality problem, and the data matters.** 25 CI failures in the last 200 runs (12.5%) were dated 2026-07-26/27 and dominated by `agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process` — 999.47, closed by Phase 25. **Zero CI failures on 2026-07-28** once the barrier landed. `phase7_cli` appears nowhere in those 25. The suite runs 17 tests in 1.86s, so speed is not the issue, and it is the only suite driving the real compiled binary against real git repos and real spawned monitors — the coverage class Phase 25 proved in-process tests miss. Fix the timing assumption; do not cull the suite.

**Measurement caveat:** `gh run rerun --failed` overwrites a run's historical conclusion, so a re-run flake vanishes from `gh run list`. The PR #48 instance is already masked this way; future flake-rate analysis will under-report unless it accounts for that.

**Fix direction:** make the budget configurable with a longer CI default (e.g. `DEVFLOW_TEST_WAIT_SECS`, 5s locally / ~30s under `CI`), and have the panic report the budget it actually used. Test-only; no production change.

**Priority:** Medium — intermittently blocks pushes and CI on unrelated changes, and each occurrence costs a retry plus the diagnosis of whether it is real. | **Size:** S — one helper plus its call sites. Linear: DEN-80.

**Considered and deliberately excluded from Phase 26 (2026-07-29).** Confirmed still open at HEAD `76e49f1` (`phase7_cli.rs:100-124`, still `200 × 25ms`, 7 call sites). Not folded into Phase 26 because it's test-only, touches no file that phase's release-mechanics cluster touches, and is cheap enough to fix standalone via `gsd-quick`/`gsd-fast` rather than waiting on phase planning. Handled outside the phase sequence, separately — not excluded for cause.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.56: macOS CI Coverage Without Breaking Required Checks (BACKLOG)

**Goal:** DevFlow shells out to `git`, reads `/proc`, spawns detached monitors and manipulates worktrees — all platform-sensitive, all currently exercised only on Linux. Add macOS to the test and clippy matrices so a platform break is caught before a user finds it.

*Extracted 2026-07-28 from the abandoned `chore/macos-ci` branch (tip `f651698`, last touched 2026-07-24) before that branch was deleted. The branch's other 10 commits were docs already incorporated elsewhere. **The intent is worth keeping; its diff is not.***

**Rationale worth preserving from the branch:** `fail-fast: false`, so a platform-specific break yields the full failure set from both runners in one run rather than whichever tripped first; clippy on **both** platforms deliberately, because once any `cfg(target_os)` code exists a Linux-only clippy cannot see inside the macOS arm; `fmt` on one runner only, being platform-independent; and a longer timeout (the branch raised test from 20 to 30 minutes) since macOS runners are slower and `phase7_cli.rs` drives real git fixtures.

**Traps — do NOT apply the branch's diff as-is.** It predates the container-parity work and regresses three things. (1) It renames the jobs to `Test (${{ matrix.os }})` / `Clippy (${{ matrix.os }})`; those exact strings are REQUIRED STATUS CHECKS on **both** the `main-squash-only` and `develop-merge-or-squash` rulesets, so renaming them makes the required check never report and wedges every merge to both branches — this has already happened once here (`devcontainer.yml`'s header records it), and classic `branches/*/protection` will not show it because rulesets are a separate mechanism. (2) It strips the pinned container and replaces `scripts/check.sh <part>` with raw `cargo`, destroying the CI/local parity that exists because that divergence made 999.47 cost hours. (3) It drops the `Trust the workspace` safe.directory step and the `assert-image-parity.sh` call, both of which exist for recorded reasons.

**Unsolved design constraint:** the Linux jobs get hermeticity from a pinned Linux container that macOS cannot use, so a naive matrix cannot keep both. Either keep the containerised Linux jobs under their existing names and add separately-named macOS jobs, or restructure while preserving the required-check names exactly — and update both rulesets in the same change if any new required name is introduced.

**Priority:** Low — no known macOS user or reported break; this is preventive. | **Size:** M, not the S the branch's diff suggests — the matrix is easy, preserving required-check names and container parity simultaneously is the actual work. Linear: DEN-81.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.57: An Operator's Checkpoint Answer Can Never Reach the Agent (PROMOTED — Phase 28, parts A+C; part B deferred)

**Goal:** A `gate="blocking-human"` checkpoint is currently a **dead end for any DevFlow-driven run**. The agent stops and asks a question; there is no path by which the operator's answer reaches the agent; every retry spawns a fresh process that asks the identical question again, forever. This is the single largest gap between DevFlow's stated goal (unattended `--yes-ship` / `--yes-release` runs) and what it can actually finish, because a plan that *correctly* gates an irreversible decision becomes a plan that can never complete unattended.

**Evidence — reproduced live during Phase 26's dogfood run, 2026-07-29.** Plan `26-01` consists of exactly two `checkpoint:decision` tasks with `gate="blocking-human"` (authorizing direct pushes to `origin/develop`, and unattended `cargo publish`). The agent stopped correctly. The operator's answers were supplied via `devflow gate approve 26 --stage code --note "<both decisions>"` — the only mechanism the CLI exposes for attaching operator input to a gate. **Two consecutive retries reproduced the identical checkpoint verbatim**, asking Task 1 again from scratch. The run only advanced after a human hand-authored the plan's `26-01-SUMMARY.md` (recording both decisions) and committed it — the "close out manually" fallback `execute-phase.md` documents for a *different* failure shape, applied here because nothing else worked.

**Root cause, verified in source at `76e49f1`:**

1. **The note is written, then never read.** `Gates::respond` persists `note` into `.devflow/gates/{phase}-{stage}.response.json`, but `gates.rs:73-75` only ever inspects it for the substring `"abort"` (`GateAction::Abort`). Every other note is stored and dropped on the floor.
2. **The relaunch prompt has no input channel for it.** `prompt::stage_prompt` / `stage_prompt_for_project` / `stage_prompt_with_project` (`crates/devflow-core/src/prompt.rs:169`, `:178`, `:182`) derive the entire prompt from `(stage, phase, project_root)`. There is no parameter through which a prior gate's answer could be threaded, so the relaunched `claude -p` starts with zero knowledge that a question was ever asked.
3. **The protocol assumes a live turn that DevFlow structurally cannot provide.** `gsd-core/references/checkpoints.md`'s `checkpoint:decision` contract ends in `<resume-signal>Select: option-a, or option-b</resume-signal>` — designed for a human typing inline into a live session. DevFlow launches every stage as one-shot `claude -p … --output-format json --dangerously-skip-permissions`, which exits after writing `DEVFLOW_RESULT`. There is no turn to reply into.

**Agreed fix — three parts, operator-decided 2026-07-29:**

- **(A) Resume the real session for Claude — the primary fix.** Every `claude -p --output-format json` result already carries a `session_id` (observed live: `b54a534e-…`), and Claude Code supports `claude -p --resume <session_id> "<answer>"`. On approving a *checkpoint* gate (a class that must be distinguished from a generic transient-error gate), relaunch as a `--resume` of the exact exited session with the operator's answer as the next message, instead of spawning a fresh stage run. This is the only option that gives the checkpoint protocol the live back-and-forth it was written for, and it is dramatically cheaper — no CONTEXT/RESEARCH re-read, no re-running completed tasks, no fresh exposure to the retry hazards this run hit twice.
- **(B) Keep a structured answer file as the portable fallback — build only if needed.** A `{phase_dir}/{plan}-CHECKPOINT-ANSWERS.json` of `{task_id, selected_option}` pairs that the executor consults *before* re-presenting a checkpoint. Agent-agnostic (no session-resume primitive required), and it is the generalized form of the manual-SUMMARY workaround used to unblock Phase 26. **Deliberately not built up front:** DevFlow also drives Codex and OpenCode, both of which are expected to have equivalent resume primitives; this exists so the design does not *depend* on that assumption. Part of the wiring lives in the GSD skill layer (`execute-phase.md` / `gsd-executor`), outside this repository.
- **(C) Make checkpoint gates legible — do regardless of A/B.** `truncate_reason` (`crates/devflow-cli/src/pipeline_outcomes.rs:318-342`) caps the gate context, so the operator sees `"…must select direct-push or pr-based-develop… [truncated; full output in .devflow/]"` and has to go read `.devflow/phase-NN-stdout` by hand to find out what the options even are. A checkpoint gate is semantically different from an error gate and should render as an actual menu (numbered options, verbatim, untruncated) in `devflow status` / `devflow gate show`. This is pure discoverability and is independent of how the answer is transmitted.

**Do not fix this by auto-approving.** `checkpoints.md` is explicit that `gate="blocking-human"` is never bypassed in any mode, and Phase 26's own run proved why: the executor found and fixed a plan that had mistakenly tagged these two irreversible authorizations `gate="blocking"` (auto-bypassable, auto-selects the *first* option) — that bug would have silently authorized `cargo publish` with no human input. The gate is correct; only the return path is missing.

**Priority:** High — it is the one defect that makes a *correctly written* plan unable to finish unattended, and the workaround requires an operator who knows an undocumented manual-SUMMARY trick. | **Size:** M — (A) is bounded but touches launch/prompt/gate plumbing and needs a new gate classification; (C) is S; (B) is M and may not be needed. Source: Phase 26 dogfood run (2026-07-29), full write-up in `superseded/26-release-cut-automation/26-01-SUMMARY.md` § "Issues Encountered". Linear: DEN-82.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.58: The Pre-Push Container Gate Cannot Run From a Linked Worktree (BACKLOG)

**Goal:** `scripts/check-in-container.sh` — the pinned-image gate wired in as `core.hooksPath=scripts/hooks`'s `pre-push` — **fails 100% of the time when the push originates from a linked worktree**, for reasons entirely unrelated to the code being pushed. Two tests fail, and one of them fails with a message that reads as a serious security regression. This collides directly with this project's standing practice of doing phase work in worktrees, so the trap is on the default path, not an edge case.

**Evidence — hit live while merging 999.5, 2026-07-30 (PR #54).** The branch `feature/999.5-changelog-body` was built in `.worktrees/999.5` and was green on the host: `cargo test --workspace` 706 passed / 0 failed, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --all --check` clean. `git push` from that worktree was **rejected** by the gate with 2 failures:

- `crates/devflow-cli/tests/build_provenance.rs:102` — `git ls-files` exits 128, `fatal: not a git repository: (null)`
- `crates/devflow-cli/tests/gitignore_coverage.rs:63` — every one of the 14 `git check-ignore` probes errors, so the test reports **all 14 `.devflow/` runtime-state paths as uncovered by `.gitignore`**, citing 15-REVIEW.md CR-01 and 17-REVIEW.md WR-07. This is a **false alarm that impersonates a telemetry-leak regression** — the most alarming possible failure text for a completely environmental cause.

Removing the worktree, checking the identical commit out in the main checkout, and pushing from there produced `check.sh: all OK` and both tests `ok`. Same code, same image (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`), opposite result. CI on PR #54 then passed all four required checks.

**Root cause, verified in source and reproduced deterministically:**

1. `scripts/check-in-container.sh:17` sets `REPO_ROOT="$(git rev-parse --show-toplevel)"`. From a linked worktree that returns the **worktree** path, not the main repository.
2. `scripts/check-in-container.sh:69` mounts exactly that one directory: `-v "$REPO_ROOT":/workspace`.
3. A linked worktree's `.git` is a **file**, not a directory, containing an absolute *host* path — `gitdir: /var/home/denniyahh/Github/devflow/.git/worktrees/999.5`. That path is outside the mount and therefore does not exist inside the container, so **every `git` invocation in the container fails**.
4. Reproduced on the host, byte-identical to the container failure: a directory whose `.git` file names a nonexistent gitdir yields exactly `fatal: not a git repository: (null)`, exit 128. The literal `(null)` is the tell.

**This is not 999.39 (`GIT_DIR` scrubbing, DEN-66), despite the resemblance.** `devflow_core::test_support::git_command` (`crates/devflow-core/src/test_support.rs:175-190`) already `env_remove`s every var in `REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS` before spawning, and `build_provenance.rs`'s own failure output confirms it (`GIT_CONFIG_GLOBAL=None`). No redirecting variable is involved; the `.git` **file** is.

**Two candidate fixes — pick one, they are not complementary:**

- **(A) Fail fast with an actionable message — S, recommended.** Detect a linked worktree before the `docker run` (`.git` is a file, or `git rev-parse --git-common-dir` differs from `--git-dir`) and exit non-zero telling the operator to push from the main checkout. Matches the script's existing fail-fast-before-any-cargo-invocation contract, already guarded by `ci_parity_guards.rs` (`check_script_fails_fast_before_any_cargo_invocation`, `devcontainer_runcmd_fails_fast_before_any_check`). Costs the operator a checkout switch but never lies.
- **(B) Make the gate actually work from a worktree — M.** Additionally mount the common git dir at the *same absolute path* it has on the host (so the worktree's `gitdir:` pointer resolves), plus the per-worktree admin directory. Strictly better for the operator, but it widens the container's mount surface to a host-absolute path and needs care that `--show-toplevel`-derived paths inside the container still agree. Note the gate tests whichever working tree it runs in, so (B) must not become an excuse to push branch X from a checkout sitting on branch Y — that silently validates the wrong tree.

**Priority:** Medium — no shipped-code defect and there is a working manual path, but it blocks the gate on this project's normal worktree workflow and its loudest symptom is a fabricated security finding, which costs real investigation time to dismiss. | **Size:** S for (A), M for (B). Source: 999.5 split / PR #54 (2026-07-30). Linear: DEN-83.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.59: A Missing CONTEXT.md Is Ambiguous, So Define Attempts an Interview It Cannot Conduct (PROMOTED — Phase 28, unit 28c)

**Goal:** DevFlow's Define stage runs `/gsd-discuss-phase N` under one-shot `claude -p --dangerously-skip-permissions`. But discuss-phase is an **interactive elicitation** — it asks the operator questions via `AskUserQuestion`. Headless, there is nowhere for those questions to go. Today the only thing preventing a deadlock is the idempotency contract in `prompt::idempotent_stage_prompt` (`crates/devflow-core/src/prompt.rs:142-166`): *"if `CONTEXT.md` EXISTS, the stage's work is already done — do NOT run the GSD command, do NOT ask for input."*

**The defect is the other arm.** The **absence** of `CONTEXT.md` is treated as exactly one thing — "run the interview" — when it actually means one of two mutually exclusive things:

1. *"The operator has not run the interview yet and wants DevFlow to run it."* → cannot work headlessly; this is the deadlock arm.
2. *"The operator deliberately wants no interview for this phase; proceed to Plan."* → perfectly reasonable, and currently inexpressible.

DevFlow cannot distinguish them, so it guesses — and guesses the arm that cannot succeed unattended.

**This is a determinism gap, not a missing capability.** The intended division of labor is sound and should not change: a human runs `/gsd-discuss-phase N` interactively (where `AskUserQuestion` works), then hands off to `devflow start`, and Define no-ops. That handoff is the design. What is missing is a way for the operator to *declare* which mode they are in, rather than DevFlow inferring it from a filesystem side effect.

**Evidence — observed live, 2026-07-30 (Phase 27 dogfood).** `devflow start --phase 27 --agent claude --mode auto --until validate` reached Define and no-op'd in ~13 seconds, correctly, because `27-CONTEXT.md` had been produced by an interactive `/gsd-discuss-phase 27` earlier the same session (three operator decisions: D-01 no-escape-hatch scrub, D-02 build.rs excluded, D-03 acceptance target). The handoff worked exactly as designed. The gap is only visible on the untested path: had that CONTEXT.md not existed, Define would have launched an interactive workflow into a process with no input channel.

**Fix direction — operator-declared, two candidate shapes:**

- **(A) An explicit flag — `devflow start --no-interview` (or `--skip-define`).** Define is declared a no-op success up front. Unambiguous, self-documenting in `--help`, and consistent with how `--yes-ship` already makes a mode explicit rather than inferred. Costs one flag.
- **(B) Invert the default: a missing `CONTEXT.md` means "no interview wanted", proceed to Plan.** No new flag at all. Simpler, and arguably the honest default given arm 1 cannot work headlessly anyway. The cost is that a genuinely-forgotten Define becomes silent rather than loud — which cuts against this project's fail-loud posture.

**A third option exists and should be considered explicitly rather than by omission:** discuss-phase has an `--auto` mode that auto-selects its recommended option for every question without asking. Define could pass that instead of failing or skipping, producing a CONTEXT.md with Claude-picked decisions. **Recommend against it by default** — a context full of unreviewed auto-picks is worse than no context, because downstream `gsd-planner` treats CONTEXT.md decisions as *locked operator intent*. Worth having as an opt-in (`--auto-interview`), never as the fallback.

**Design constraint carried from the operator, 2026-07-30:** DevFlow's purpose is to add determinism to what Claude and GSD are unreliable at repeating — not to grow judgment of its own. Any fix here must make an existing ambiguity explicit; it must not make DevFlow decide *what* the phase should contain.

**Priority:** Medium — no live failure yet (the handoff path works and is what this project actually uses), but it is an unexploded footgun on the first unattended run of a phase whose Define was forgotten, and the failure shape is a headless hang rather than a clean refusal. | **Size:** S — one flag or one inverted branch, plus the prompt-contract test in `prompt.rs`'s `define_and_plan_prompts_are_idempotent`. Source: Phase 27 dogfood + design discussion (2026-07-30). Linear: DEN-84.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.60: `resume` Discards an `--until` Cap That Never Fired, and Cannot Re-Specify One (PROMOTED — Phase 28, unit 28d)

**Goal:** `devflow resume --phase N` clears `stop_until` **unconditionally** (`crates/devflow-cli/src/pipeline_launch.rs:226-228`), including when the cap never fired. An operator who started a run with `--until validate` — explicitly declaring Ship out of bounds — and then resumed it after any interruption gets a run with **no cap at all**, silently. `resume` also exposes no `--until` flag, so the boundary cannot be re-established once lost; the only remaining controls are `yes_ship` and `kill`.

**This is NOT "resume drops `stop_until`" — that part is correct and must not be regressed.** Clearing is deliberate, documented (`pipeline_launch.rs:207-214`) and covered by `resume_clears_stop_marker_and_advances_past_stop_point` (`pipeline_launch.rs:457`), which exists because of a 20c review finding: a phase that *halted at* its cap persists `stopped`/`stop_reason`/`stop_until`, and without clearing them `transition()`'s `stop_until == Some(from)` arm would immediately re-stop it, leaving the phase `stopped` forever despite an explicit resume. **That behavior is right. The defect is that the clear is unconditional rather than conditioned on the cap having actually fired.**

**Evidence — reproduced live, 2026-07-30 (Phase 27 dogfood).** `devflow start --phase 27 --agent claude --mode auto --until validate` was killed mid-**Code** by the operator (usage-limit conservation), never reaching the cap. State was verified immediately before resuming: `{'stage': 'code', 'yes_ship': False, 'stop_until': 'validate'}`, and `.devflow/events.jsonl` records **zero** stop events for phase 27 — the cap provably never fired and `stopped` was false. After `devflow resume --phase 27`, state read `{'stage': 'ship', 'yes_ship': False, 'stop_until': None}`. The event log shows the unguarded advance:

```
{"event":"advance_evaluated","phase":27,"stage":"validate","status":"success","verdict":"pass"}
{"event":"transition","from":"validate","phase":27,"to":"ship"}
{"event":"stage_launched","phase":27,"stage":"ship","monitor_pid":3292822}
```

Ship was killed ~10s after launch. **No external side effects occurred** — branch unpushed, no PR, `develop` untouched, no new tags — but only because `yes_ship: false` forces the Ship gate to block for a human. The control the operator actually named (`--until validate`) contributed nothing; a second, independent guard did all the work.

**Why this matters more than the blast radius suggests.** Ship's terminal hooks are `Merge → VersionBump → ChangelogAppend → BranchCleanup`, fail-fast with **no rollback**. `--until` is the mechanism that makes a dangerous stage structurally unreachable. It holds on `start` and evaporates on `resume` — and `resume` is the documented recovery path after a rate limit or infrastructure pause, i.e. exactly when the operator is *least* likely to be watching. An operator who reasons "I capped this at validate, so Ship cannot happen" is correct on the first run and wrong on every resume, with no warning.

**Fix direction — two parts, (A) is the defect proper:**

- **(A) Condition the clear on the cap having fired — S, recommended.** Clear `stopped`/`stop_reason`/`stop_until` only when `state.stopped` is true. That preserves `resume_clears_stop_marker_and_advances_past_stop_point`'s scenario byte-for-byte (that test sets `stopped = true`), while leaving an unfired cap intact. Add a companion test asserting the mirror case: `stopped == false` + `stop_until == Some(Ship-ward stage)` survives a resume.
- **(B) Give `resume` an `--until` flag — S, complementary.** Even with (A), an operator who resumes a genuinely-halted phase currently cannot re-impose any boundary. Mirrors `start`'s existing flag; no new concept.

**Do not fix by making `--until` sticky across the halt.** That reintroduces exactly the permanent-`stopped` wedge 20c closed. The distinction to preserve is *fired vs. not fired*, not *set vs. unset*.

**Priority:** High — it silently removes an operator-declared safety boundary on the recovery path, and the stage it exposes drives irreversible operations. Mitigated in practice only by `yes_ship`, which is a different control the operator may also have set. | **Size:** S for (A), S for (B). Source: Phase 27 dogfood (2026-07-30). Linear: DEN-85.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.64: One-Shot Launch Kills the Session at Turn End, Orphaning Any Delegated Work (PROMOTED — Phase 30)

**Found:** 2026-07-31, Phase 29 wave 2. Companion to `UPSTREAM-GSD-ISSUES.md` § 7 — **that entry is the GSD half; this is DevFlow's.** GSD can be fixed upstream and this defect still stands, because it is a property of how DevFlow launches agents, not of what they do.

**The defect in one sentence:** DevFlow launches every stage as `claude -p "<prompt>"`, where **the agent's turn ending terminates the process** — so any work the agent delegated that outlives its turn is orphaned, and DevFlow cannot tell that from success.

**Observed:** the Code stage's orchestrator dispatched two executors, said *"I'll pick up when they return"*, and exited with `stop_reason: end_turn`, `subtype: success`. The executors completed 5 commits on `worktree-agent-*` branches, wrote no `SUMMARY.md`, and were never merged. DevFlow advanced to Validate.

**Why DevFlow's own backstop missed it.** `agent_result.rs` judges a stage in three layers: (1) the `DEVFLOW_RESULT` marker, (2) exit code + commit count, (3) process existence + commits. The orchestrator emitted **no marker** — it printed prose — so Layer 1 found nothing and Layer 2 saw `exit 0` plus wave 1's commits and returned **Success**. Layers 2–3 were designed against *"agent died having done nothing"* (zero commits). They have no answer for *"agent did some work, delegated the rest, and reported success."*

Note the mis-scoped commit count (counting commits ahead of `develop` rather than commits made during this attempt) is **not** the hole here: the same Code invocation genuinely committed wave 1's work, so attempt-scoping would still have passed.

---

#### The structural fix — keep the session alive past turn end

**`--input-format stream-json` keeps the process alive until stdin closes. Measured, not assumed:**

```
t+2   system/init        session starts
t+3   assistant          turn completes
t+21  process exits      ← only when stdin was closed, 18s later
```

Under this mode DevFlow writes the prompt and **holds the pipe open**. The orchestrator ending its turn no longer kills anything: backgrounded executors finish, their completion notifications land in a session that still exists, and the orchestrator resumes and merges — the behavior GSD's workflow already assumes. DevFlow closes stdin when it sees `DEVFLOW_RESULT` in the output stream, making the completion protocol it already depends on the stream terminator. Requires a wall-clock timeout so an agent that never declares cannot hold the pipe open forever.

**This addresses two defect classes with one change.** Both of DevFlow's hardest problems have the same root — **turn end equals process death**. That is why a `blocking-human` checkpoint was a dead end (999.57) and why delegated work orphans. Phase 28 solved the checkpoint case with `claude --resume`, reconstructing a session after killing it. Streaming input means never killing it. 999.57's part (B) fallback may become unnecessary.

**Not yet proven, and this is the feasibility gate.** The lifetime property is **necessary but not shown sufficient**: it is proven that the process survives its turn; it is **not** proven that a backgrounded subagent's completion notification is delivered into that surviving session and that the orchestrator wakes to merge. One cheap experiment decides it — a scratch directory, no GSD, no worktrees, a trivial prompt that spawns a background `Agent`, ends its turn, and waits. **Run this before committing DevFlow to the approach.**

#### Options considered and rejected, with reasons

| Option | Verdict |
|---|---|
| **`--resume <session_id>` on detecting orphans** | **Keep as a complement, not the fix.** Phase 28 already built both halves (28-02 captures the id, 28-03 added `--resume`). Reactive, but it is the *only* option that can rescue sessions that have already exited. |
| **`--bg` + `claude agents --json`** | Plausible — the JSON listing is scriptable and TTY-free. Moves session lifetime into a surface DevFlow does not control, and it is unverified whether it changes turn-end semantics at all. |
| **DevFlow drives waves via `--wave N`** | **Does not fix it.** Wave 2 *itself* contained 2 plans, so it hits the background branch regardless of who drives the outer loop. Ruled out explicitly. |
| **`parallelization: false`** | The stopgap actually applied (`0955f97`). Serializes within a wave so the 2+ branch is never reached. Changes nothing structural, and does not address subagents backgrounding **by default** in current Claude Code. |
| **Harden the completion oracle** (missing `DEVFLOW_RESULT` → hard failure) | Worth doing as defence-in-depth, but it detects the symptom after the work is already lost. Not a fix. |

**Priority:** High — it silently discards completed work and reports success, and it is the shared root of 999.57's class. | **Size:** M for the streaming-input launch path; S for the experiment that gates it.

Plans:

- [ ] TBD — run the feasibility experiment first, then promote

### Phase 999.63: The Phase-Reachability Guard Demands Define's Own Output as Define's Precondition (DELIVERED — patch, 2026-07-31)

**Found live 2026-07-31**, launching the Phase 29 dogfood: `devflow start --phase 29` refused with `phase 29 is not reachable from \`develop\`` / `missing: a \`.planning/phases/29-*/\` directory`, on a phase whose ROADMAP heading was present and correctly merged.

**The defect.** `ensure_phase_reachable_on_base` (`preflight.rs`) refused unless **both** the `### Phase N:` ROADMAP heading **and** a `.planning/phases/{NN}-*/` directory were present on `develop`. That directory is **Define's own output** — `/gsd-discuss-phase N` is what creates it. The guard therefore demanded the product of the pipeline's first stage as a precondition for running the first stage, which structurally prevented DevFlow from **ever** driving GSD discussion mode for a newly-promoted phase. Every phase had to have its Define performed by a human, out of band, before DevFlow could be handed the work.

**Origin.** Commit `fdc0a3d`, `feat(23-12): refuse devflow start when phase is unreachable from develop` — gap closure 23f, written after the 2026-07-26 acceptance-run failure in which a phase promoted only on another branch was invisible to its own run and floundered silently through Define (`23-FINDINGS.md` §B1).

**Why the original fix over-reached.** For the failure class it targeted, **the heading check alone is sufficient** — a phase promoted only on another branch has no heading on `develop` either. The directory conjunct added *no detection power* for that class while creating a false-positive class whose only members are legitimate bootstrap states. It has no true positives that the heading check does not already catch.

**Corroborating evidence that this was unintended, not a policy choice.** Thirty lines below this guard's call site (`commands.rs`), the Codex leg refuses a missing `CONTEXT.md` with *"codex cannot run an interactive discussion headless. Run /gsd-discuss-phase N interactively first (any agent), **or use `--agent claude`**."* That message is only coherent if Claude driving Define to *produce* the discussion was intended to work. The reachability guard runs first and silently revoked it for every agent — a regression the Codex leg's own wording contradicts.

**Fix (surgical, enforcement-only).** `phase_reachability_on_base` is left untouched — it remains a pure two-field probe, so 27-05's hostile-`GIT_DIR` regression test keeps discriminating on the directory half. Only `ensure_phase_reachable_on_base` changed: a missing directory alone no longer refuses; a missing heading still does, and still names itself in the refusal. The directory is still reported when the heading is *also* absent, where it is useful diagnostic detail.

**Tests.** `enforcement_does_not_refuse_when_only_the_phase_dir_is_absent` (the named regression, with a precondition assertion so it cannot pass on a fixture that satisfies the guard some other way) and `enforcement_still_refuses_when_the_roadmap_heading_is_absent` (the control, pinning that 23-12's real failure class does not regress open). The `start_reachability_e2e.rs` fixtures omit *both* halves, so they still refuse and were unaffected.

**Priority:** High — blocked the Phase 29 dogfood outright and silently narrowed DevFlow's mandate. | **Size:** S.

Plans:

- [x] Delivered as an out-of-band patch, 2026-07-31 (no phase — emergency unblock for the Phase 29 dogfood)

### Phase 999.83: The Drain Gate Never Saw 8 Concurrent Sub-Agents — Its Fixture's Shape Is Not What Production Emits (BACKLOG)

**Linear:** [DEN-104](https://linear.app/denniskim/issue/DEN-104/99983-the-drain-gate-never-saw-8-concurrent-sub-agents-its-fixtures)
**Found:** 2026-08-06, phase 34 plan 34-05's capture campaign (D-04: file capture-revealed defects,
do not fix them in the capture plan).

**Evidence:** `.planning/phases/34-…/34-evidence/` — all five per-stage captures, and
`34-evidence/DRAIN-ANALYSIS.md` for the full working.

**The defect, in one line:** `CloseRule` keys its drain arm on
`type:"system", subtype:"background_tasks_changed"`, and across 1063 top-level events spanning a
complete five-stage production run that subtype appeared **zero times** — including in the two
stages that dispatched **8 concurrent sub-agents** between them.

**Why it is not simply "the run had no concurrency."** The 8 dispatches were announced as
`subtype:"task_started"` carrying `"task_type":"local_agent"` — *the exact `task_type` value the
drain gate's own synthetic fixture manufactures* (`monitor.rs:1164`). The fixture models a
`local_agent` task as arriving inside a `background_tasks_changed` `tasks` array; production, on
`claude` 2.1.222, announced it through the `task_started` / `task_progress` / `task_notification`
family instead. D-09 recorded that every gate fixture is labelled SYNTHETIC and the parser's
production correctness was *reasoned, not witnessed*. It is now witnessed, and for this path the
reasoning did not survive.

**What it costs.** `should_close()`'s drain arm is satisfied vacuously (`NeverAnnounced`) while
sub-agents are live, so stdin closes on the marker alone. That is the precondition of the 999.64
orphan shape — at `Stage::Code`, the exact stage 999.64 was observed at.

**What this does NOT establish, and why the entry is scoped narrowly.** Every `Bash` call in the run
carried `"run_in_background": false` (8 occurrences, **zero** `true`), so the **backgrounded-shell**
path was never exercised and may work exactly as designed. No child work was actually orphaned in
this run. n=1, one CLI version, one workload shape.

**The work.** Establish which event family the CLI uses for each kind of concurrent child, at more
than n=1; then either widen `CloseRule::observe` to the families production actually emits, or
record in-source why `background_tasks_changed` is the right and sufficient key. Re-label the
fixture to match whatever is found.

**Priority:** High — it weakens the specific guard built for 999.64. **Size:** M.

---

### Phase 999.82: Re-File 31/D-14 — Per-Child Declared Tokens, Deferred on Size for the Second Time (BACKLOG)

**Linear:** [DEN-105](https://linear.app/denniskim/issue/DEN-105/99982-re-file-31d-14-per-child-declared-tokens-deferred-on-size-for)
**Found:** re-filed 2026-08-06 by phase 34 plan 34-05 under D-12. Originally CONTEXT.md D-14 in
phase 31, deferred there; carried into phase 34's discussion and deferred again.

**Why this entry exists separately from the existing note.** `ROADMAP.md`'s 999.73-adjacent deferral
note already mentions per-child declared tokens in passing. D-12 requires the item to be a
**numbered backlog entry in its own right**, not a clause inside another phase's note — an item that
lives only as a cross-reference is one nobody schedules.

**The item:** attribute declared token usage **per child** rather than per run. Each sub-agent's
`result` event carries its own usage and `total_cost_usd`; DevFlow currently reads only the
top-level figure.

**Why it keeps getting deferred, stated plainly:** size, not merit. Both times it was cut to protect
a phase's scope, and both times the reason recorded was that the drain gate would cover the same
ground. Phase 34's captures weaken that argument — see 999.83, where the drain gate observed none of
the 8 sub-agents it would have needed to see.

**What it would defeat.** Constraint 7's coalescing undercount, **directly** — by counting each
child's declared tokens instead of inferring concurrency from a gate that this phase has now shown
can miss it entirely.

**Priority:** Medium. **Size:** M.

---

### Phase 999.71: Measure Whether the Capture Writer Actually Leaves Torn Terminal Lines (BACKLOG)

**Linear:** [DEN-92](https://linear.app/denniskim/issue/DEN-92/99971-measure-whether-the-capture-writer-actually-leaves-torn-terminal)
**Found:** 2026-08-02, phase 30 adversarial pass over the malformed-input defect class. Flagged as
unmeasured by the cross-AI code review (`30-CODE-REVIEW.md`) and again in
`30-H1-CONTEXT-FOR-31.md`.

**The question, in one line:** when DevFlow reads `.devflow/phase-NN-stdout`, can the last line ever
be torn?

**Why it is open.** `claude_stream_events` silently drops unparseable lines, and every consumer
assumes what survived is complete. Phase 30 fixed the gate consumer and swept it at every truncation
offset; the verdict consumer is confirmed broken under truncation (constraint 9 item 1, reproduced
at byte offset 1120 by `truncation_sweep_never_upgrades_verdict_to_success`). What nobody has
established is **how often, if ever, a torn line actually reaches a reader in production.**

**Why source reading cannot answer it.** The live gate read
(`checkpoint_reported_in_capture`, `pipeline_launch.rs:491`) sits in the post-result dispatch path
rather than a polling loop, so it is not continuously racing the writer. But the capture is written
by raw `sh` redirection from the monitor, and this milestone's entire premise is sessions outliving
turn end with background tasks still running — precisely the condition where a process could still
be appending when a reader arrives. Neither "safe" nor "exposed" follows from the code alone.

**The experiment.** Same family as 30c/30d: instrument a live run, read the capture at the moment
DevFlow would, and record whether the final line ever fails to parse — plus, if it does, at what
rate and under which exit paths (clean exit, timeout, kill, still-running background children).

**What it changes.** Priority, not exposure. Nothing in phase 30 is gated on the answer: the gate
path is fixed and swept, the session-id path is fail-closed by construction (reads only
`system`/`init`, so truncation yields `None`, never a forged id), and the verdict path is inert on today's captures — its stream branch
requires a parsed `init` that only `stream-json` emits (the earlier "zero callers"
phrasing was wrong; `evaluate_agent_result` calls it on every evaluation). The answer sets how urgently **Phase 31** must close constraint 9 before wiring the launch
path — a measured "never observed" makes it a hygiene fix, a measured "happens on kill" makes it a
prerequisite.

**Size: S.** One harness in the 30c/30d mould, plus a short archived measurements file.

---

### Phase 999.70: Checkpoint Detection Cannot Tell a Gate DECLARATION From a Gate MENTION (BACKLOG)

**Linear:** [DEN-91](https://linear.app/denniskim/issue/DEN-91/99970-checkpoint-detection-cannot-tell-a-gate-declaration-from-a-gate)
**Found:** 2026-08-02, cross-AI code review of phase 30 (codex/gpt-5.6-sol, high effort), Medium
finding 1. Recorded in `30-CODE-REVIEW.md`.

**The issue:** 30-05 scoped stream gate scanning to the `result` text of top-level `result` events,
which establishes **authorship** — the text came from the orchestrator, not an echoed prompt, a
subagent, or mid-turn narration. It does not establish **intent**. A result that merely *documents*
a gate still trips the matcher:

```
"The plan documents **Gate:** `blocking-human`; no checkpoint was reached."
```

returns `true`. And because detection deliberately scans ALL top-level results rather than only the
last (T-30-27), one documentary mention stays decisive through later quiet task-notification turns.

**Why it was not fixed in 30-05.** That plan's scope fence explicitly forbade modifying
`text_reports_human_gate` or `HUMAN_GATE_VALUE`. The matcher encodes a hard-won live observation —
the Phase 28 run that discovered the value rendered as a markdown code span, which defeated the
original matcher entirely. Widening or narrowing it is its own change with its own regression risk,
not a rider on a scoping fix.

**The shape of the fix:** require declaration *framing* rather than the bare token — the
`## CHECKPOINT REACHED` heading together with the `**Gate:**` field, as the live Phase 28 rendering
actually emits — instead of matching the gate field wherever it appears. Note the opposite-direction
hazard (T-30-24): over-tightening silently drops a real human authorization request, which is worse
than the false positive. Any change here needs positives and negatives in the same commit.

**Severity: medium.** Reachable only once the launch path emits `stream-json` (Phase 31). The
consequence is a checkpoint auto-decide firing, or the resume ceiling being consumed, on a stage
whose output merely discussed a gate — and DevFlow's own planning documents are exactly that kind of
content. Bounded by the fact that a false gate normally falls through to the generic human gate
rather than authorizing anything.

**Size: S.** One matcher, its call sites, and a regression cluster covering both directions.

---

### Phase 999.69: Re-Publish the Three Committed `30a-evidence` Captures Through the Redaction Pipeline (BACKLOG)

**Linear:** [DEN-90](https://linear.app/denniskim/issue/DEN-90/99969-re-publish-the-three-committed-30a-evidence-captures-through-the)
**Found:** 2026-08-02, Phase 30 plan 30-02 Task 1, while proving the new publish pipeline against a
real capture rather than only a synthetic fixture. Logged in the phase's `deferred-items.md`.

**The issue:** all three archived 30a captures were committed before a redaction pipeline existed,
and all three match the same three patterns under 30c's own scanner:

| Capture | Lines | Scan result |
|---|---|---|
| `30a-evidence/raw_output.jsonl` | 12 | `home_path`, `os_username`, `session_identifier` |
| `30a-evidence/raw_output_v2.jsonl` | 25 | `home_path`, `os_username`, `session_identifier` |
| `30a-evidence/raw_output_v3.jsonl` | 54 | `home_path`, `os_username`, `session_identifier` |

Concretely: the `init` events carry an absolute `cwd` under the operator's home directory, every line
carries the same real `session_id`, and `task_notification` events carry absolute `output_file`
paths. This is the live instance the cross-AI review cited when it rejected 30-02's original
single-step evidence write as unsafe.

**Severity: low-but-real.** The operator's GitHub username is already public in this repository's
commit metadata and the session id is a local identifier with no credential value — **nothing
credential-shaped matched**. It is nonetheless the same leak class as backlog 999.10 and Phase 18
review finding WR-02, and this repository publishes to crates.io.

**Why it was not fixed in Phase 30.** 30-02's `files_modified` lists only the three 30c paths and the
plan carries an explicit scope fence. More substantively, rewriting a sibling unit's committed
evidence would invalidate the line-number citations that `30-01-PLAN.md`, `30-02-PLAN.md`,
`30-01-SUMMARY.md` and `30-REVIEWS.md` all make into `raw_output_v3.jsonl` — a change with real blast
radius that should be taken deliberately, not as a drive-by.

**The fix is already written and already proven against these exact files.** 30c's
`publish_jsonl()` (in `30c-monitor-env-harness.py`) is importable. During 30-02 verification all
three captures were re-published through it into a scratch directory and all three returned `CLEAN`
with `unparseable=0` and zero line loss (12/12, 25/25, 54/54). What remains is the companion pass
over the line-number citations in the four documents above.

**Deliberately NOT in `.planning/WINDOWS.md`** — an open ledger entry blocks `/gsd-ship`, and
blocking a phase's ship on a pre-existing artifact it scoped out is a policy call. Operator decided
2026-08-02: file for a future fix, do not block Phase 30.

**Priority:** Low-Medium. **Size:** S — the sanitisation is mechanical; the citation pass is the work.
**Depends on:** nothing. Should NOT run while Phase 30 or 31 is mid-flight, since both cite into
`raw_output_v3.jsonl`.

---

### Phase 999.68: Planning-Artifact Architecture — Requirements, a Constitution, and the Linear Boundary (BACKLOG)

**Linear:** [DEN-89](https://linear.app/denniskim/issue/DEN-89/99968-planning-artifact-architecture-requirements-a-constitution-and)
**Found:** 2026-08-02, operator review of long-horizon tracking during Phase 30. Not a code defect —
a planning-system gap. Filed because the symptom ("GSD doesn't track long-term product requirements")
has a different cause than it appears to.

**The finding in one sentence:** GSD's requirements machinery is not missing, it is dormant because
this project never supplied the data that activates it — and the parts that *are* genuinely missing
are a normative principles document and a stated boundary against Linear.

#### Part 1 — Requirements are dormant, not absent. There is no toggle.

Verified in source, 2026-08-02: `init.cjs:283` matches `REQUIREMENTS_HEADER_RE` against a ROADMAP
phase section's `**Requirements**:` line; `phase_req_ids` becomes `null` when that text is missing
**or literally `TBD`**, and a null value silently skips the coverage gate. There is **no config key**
for this — `config-loader.cjs` has none. The only adjacent key, `workflow.context_coverage_gate`,
governs the *decision* gate.

What is therefore sitting unused: categorized REQ-IDs, a v1/v2 split, an explicit Out-of-Scope table
with reasons, a phase→requirement traceability matrix (`templates/requirements.md`), the blocking
requirements coverage gate (plan-phase §13) and the blocking decision coverage gate (§13a). This
project runs none of them — `PROJECT.md` records the opt-out, and every phase carries
`**Requirements:** TBD`.

Activation is two files, no config: create `.planning/REQUIREMENTS.md`, and replace `TBD` with real
IDs in each phase's `**Requirements**:` line. Infrastructure phases can keep the unit convention;
the value is for product-facing work. Note `/gsd-new-milestone` has a "Generate REQUIREMENTS.md"
step — the hand-declared v2.3.0 milestone (2026-08-02) bypassed it.

#### Part 2 — There is no normative document that outlives a milestone.

`PROJECT.md` is *descriptive* and drifts: on 2026-08-02 it claimed workspace version `1.8.0` while
the actual version was `2.2.0`, and stated the milestone-open decision three different ways. GitHub
Spec Kit's `.specify/memory/constitution.md` is the model worth copying — immutable principles every
plan is checked against, with a "Complexity Tracking" table requiring written justification for any
deviation. GSD has no equivalent. Candidate: `.planning/CONSTITUTION.md`, referenced by PROJECT.md
and read at plan time, carrying rules this project already learned the hard way (e.g. "never predict
a gate or route around one", "a document is not a control").

#### Part 3 — The long-horizon tracker already exists and is underused.

The DevFlow Linear project holds 88+ issues and the 999.x backlog is already mirrored into it
(999.67 → DEN-88). The missing piece is a stated boundary, not a tool. Proposed: **Linear owns
product goals, features, and anything outliving a milestone; GSD owns execution of the current
milestone; ROADMAP.md becomes a view of the backlog rather than its store.** Today ROADMAP.md is
~2,750 lines doing five jobs — active phases, backlog, milestone framing, architecture decisions,
and historical narrative — which is why long-term items feel untracked: they are buried in an
execution document that gets archived at milestone close.

**Considered and rejected: adopting GitHub Spec Kit alongside GSD.** Its specs are per-feature
directories (`specs/[###-feature]/`), and its documented "Flow-Forward" persistence model creates new
directories on requirement change, with the docs naming the tradeoff as "potential duplication and
fragmented context." Spec Kit's own Feb/March 2026 newsletters list spec lifecycle management — "move
beyond feature-branch-specific specs", "address spec drift" — as *future* work, i.e. this exact
problem is unsolved there too. Running two planning systems also costs an authority ambiguity this
project has already been bitten by. Worth re-checking whether the lifecycle work has landed since.

**Priority:** Medium. **Size:** S for Part 1, S for Part 2, M for Part 3 (the ROADMAP split is the
bulk). Parts are independently landable.
**Depends on:** nothing. Best sequenced at a milestone boundary, where REQUIREMENTS.md is normally
generated anyway.

---

### Phase 999.67: `parse_devflow_result` Lets an Agent Plant Its Own Layer-0 Provenance (BACKLOG — shortlisted for Phase 31)

**Linear:** [DEN-88](https://linear.app/denniskim/issue/DEN-88/99967-parse-devflow-result-lets-an-agent-plant-its-own-layer-0)
**Found:** 2026-08-02, Phase 30 plan 30-01 execution (finding F-1). Surfaced *by* the fix for the
same defect class on the new stream path, and deliberately not fixed there: 30-01's scope fence
forbids touching the shipped single-document path.

**The defect in one sentence:** `parse_devflow_result` returns `parse_marker_lines`' output
unnormalised, so an agent that writes `"decided_by_layer": 0` into its own `DEVFLOW_RESULT` marker
reaches `classify_validate_outcome` carrying a Layer-0 external-probe provenance it did not earn.

**Confirmed at runtime, not by reading** (30-01, 2026-08-02):
`parse_marker_lines(r#"DEVFLOW_RESULT: {"status":"success","decided_by_layer":0}"#).unwrap().decided_by_layer`
is `Some(0)`. That assertion is now a permanent non-vacuity guard inside
`claude_stream_overwrites_agent_planted_decided_by_layer` (`agent_result.rs:2174`), so the surface
stays visible in the test suite until it is closed.

**Why it matters.** `pipeline_outcomes.rs:184` computes
`external = result.decided_by_layer == Some(0) && result.status == AgentStatus::Success` — the
predicate for *"an independent Layer-0 probe decided this, not the agent's self-report."* An agent
deciding its own provenance is the precise trust inversion DevFlow's layered result model exists to
prevent, and 30-01 closed exactly this hole on the new stream path (T-30-26).

**Blast radius is bounded — this is why it is a backlog entry and not a `WINDOWS.md` blocker.** It
can flip a Validate `Failed` to `Ambiguous`, which still gates and still requires a human. It
**cannot** manufacture a pass. The 30-01 executor correctly declined to file it in the ledger
unilaterally, since an open entry there blocks `/gsd-ship` and this defect predates Phase 30.

**Fix:** a one-line overwrite in `parse_devflow_result` mirroring what 30-01 added to
`parse_claude_event_result`, plus a mirror test. Folding it into Phase 30's own 30-03 was considered
and rejected — it would breach the same scope fence mid-phase.

**Priority:** Medium — a real trust-boundary hole on the currently shipped path, but non-silent and
incapable of fabricating success. **Size:** XS.
**Depends on:** nothing structurally. Independent of the 999.64 arc, shortlisted for Phase 31
because that phase is already editing this file's neighbourhood.

---

### Phase 999.66: `consecutive_failures` Accumulates on Healthy Multi-Wave Progress, Not Just Repeated Failure (BACKLOG)

**Found:** 2026-07-31, investigating the Phase 29 dogfood gate, before 999.65 was found to have
gated first and masked it. Not yet observed firing in production — recorded because it is real
and would have fired at wave 3 had 999.65 not gated at wave 2.

**The defect:** `transition_resets_consecutive_failures` (`mode.rs:112`) excludes exactly one
transition from resetting the counter: `(Stage::Code, Stage::Validate)`. That was correct when
written — `18-04` fixed a prior bug where the counter was unconditionally zeroed on every
transition, making the 3-strike gate (`MAX_CONSECUTIVE_FAILURES`) *unreachable*. But the
loop-back path (`prepare_loop_back_to_code`, `pipeline_gate.rs:141`) assigns `state.stage =
Stage::Code` directly and never calls `transition()` at all — so the reset predicate is never
even consulted on a loop-back. Inside a multi-wave phase's Code↔Validate loop, the counter is
therefore monotonic: every wave transition that isn't a first-try pass increments it, whether or
not anything failed.

**Consequence:** a phase with ≥3 waves, run in `auto` mode, will force a `[never-silent]` gate
around its third wave transition, reporting *"Validation failed 3 time(s) — human review
needed"* about a phase that never failed — it was working normally, wave by wave. This
structurally caps unattended multi-wave phases at 2 waves before requiring a human, independent
of whether the work is good.

**Fix direction (not yet designed):** the counter must distinguish "Validate found the same
unresolved problem again" from "Validate correctly reported wave N incomplete, wave N+1 is new
work." One candidate: reset on any loop-back where the prior Code stage's commit set differs
from the one that produced the last Validate failure (genuine forward progress occurred). Needs
its own design pass — the naive fix (reset on every loop-back) would restore 18-04's original
unreachable-ceiling bug in a different form.

**Priority:** High — caps every unattended multi-wave phase at 2 waves. | **Size:** S–M. Depends
on nothing; independent of 999.64 and 999.65, but only observable once a phase runs 3+ waves,
which requires 999.64 and 999.65 both fixed first.

Plans:

- [ ] TBD — promote with `/gsd-review-backlog` when ready

### Phase 999.65: The Validate→Code Loop-Back Issues an Impossible Command on a Mid-Arc Phase (BACKLOG)

**Found:** 2026-07-31, Phase 29 dogfood. The gate that actually halted the run that day —
`999.64` was found investigating *why* the run got far enough to hit this one.

**The defect:** every Validate→Code loop-back, unconditionally, issues
`/gsd-execute-phase {N} --gaps-only`. That flag selects only plans with `gap_closure: true` in
their frontmatter — a marker minted by `/gsd-plan-phase {N} --gaps`, which itself reads
`{N}-VERIFICATION.md`. A phase that has never run `/gsd-verify-work` — i.e. any phase still
mid-arc, with waves left to execute — has **zero** such plans. The loop-back therefore matches
nothing, the orchestrator correctly reports *"no gap-closure plans found"* rather than silently
succeeding, and the run halts at a `[never-silent]` gate with neither gate answer able to
proceed: `reject` loops back into the identical failing command, `approve` advances into a
Validate that will fail on the same unbuilt waves.

**Live evidence:** Phase 29 wave 1 completed; Validate correctly reported the phase incomplete
(9 findings, all "this module doesn't exist yet" for waves 2–6, not gap-shaped defects); the
loop-back issued `--gaps-only`, matched zero plans, and gated. The executor's own diagnosis,
independently verified: *"there is no `29-VERIFICATION.md` because `verify-work` never ran — the
phase is mid-arc at wave 1 of 6."*

**Fix direction (not yet designed):** the loop-back needs to distinguish two cases DevFlow
currently cannot tell apart — "Validate found defects in already-built work" (gap closure is the
right tool) vs. "Validate correctly observed the phase isn't finished yet" (the right tool is
plain `/gsd-execute-phase {N}`, no flag, to continue the wave sequence). The distinguishing
signal is likely whether `{N}-VERIFICATION.md` exists and what its finding-type breakdown is —
needs its own design pass, not a guess encoded here.

**Priority:** High — without this fix, no multi-wave phase can complete a Code↔Validate loop
unattended; the loop-back mechanism that exists specifically to enable that is what breaks it.
**Size:** S–M. Depends on nothing structurally, but is unobservable in an autonomous run until
999.64 is fixed (999.64 currently prevents any multi-wave phase from reaching a second
loop-back at all).

Plans:

- [ ] TBD — promote with `/gsd-review-backlog` when ready

### Phase 999.61: Four Indirect Git-Reaching Spawn Edges a Literal Grep Cannot See (BACKLOG)

**Goal:** Phase 27 scrubbed all 41 production `Command::new("git")` sites, but its own workspace census found **five** further spawn edges that reach `git` *indirectly* — through `sh`, through `cargo`, or through an operator-configured command — and are therefore invisible to a literal grep for `Command::new("git")`. One (`monitor.rs:148`, the agent spawn) was closed in-phase by `936b371`. **Four remain:**

| Site | Mechanism | Severity |
|---|---|---|
| `crates/devflow-core/src/hooks.rs:222` | `sh -c "cargo doc …"` → cargo → `build.rs::run_git` | Medium — the *same* indirect-compile mechanism 27-04 already found and fixed once, at a second call site |
| `crates/devflow-core/src/gates.rs:323` | operator-configured gate-notify command | Medium |
| `crates/devflow-core/src/verify.rs:106` | operator-approved verification command | Medium |
| `crates/devflow-cli/src/commands.rs:2086` | `cmd_check("git", "git", …)` — program name reaches `Command::new` through a variable | Low |

**Why this is filed rather than folded into Phase 27.** Phase 27's RESEARCH carried Assumption A2 — that the literal-grep site list was exhaustive — and its census **held that assumption OPEN rather than silently closing it** (`27-SPAWN-CENSUS.md` § Assumption A2 verdict). All five were confirmed production-reachable by direct call-graph tracing, not assumed. Closing the highest-severity one in-phase and filing the rest is the honest split, and it is why this entry's ceiling is Medium rather than High: **the agent-spawn edge is already closed.**

**The `gates.rs` / `verify.rs` pair deserves a deliberate decision, not a reflexive scrub.** Both run commands the *operator* configured. The operator therefore has some control over what executes — but the redirecting variables are still inherited invisibly into a command whose contents nobody audited for env-sensitivity. Decide explicitly whether DevFlow scrubs an operator's own command (safer, but silently changes the environment they may have intended) or documents that it does not.

**`commands.rs:2086` is functionally inert** — `git --version` resolves no refs, so nothing can be redirected. Worth closing anyway for D-01 literal-completeness, and because it is the concrete demonstration that a literal grep is defeated by any indirection through a variable.

**Fix direction:** `hermetic_command(program, dir)` at each site, exactly as 27-04 did for `commands.rs::test_cmd` and `936b371` did for `monitor.rs` — the mechanism already exists and is proven; this is applying it, not designing it. Pair with a regression guard that greps for *spawn constructors reaching git indirectly* rather than for the literal string, so a sixth edge cannot appear unnoticed.

**Priority:** Medium — down from the census's original "high", because the one high-consequence site (the agent spawn) is closed. No remaining edge launches an agent or performs a mutating git operation. | **Size:** S — four one-line changes plus the guard. Source: Phase 27 census (`27-SPAWN-CENSUS.md` § Proposed backlog entry), 2026-07-30. Linear: DEN-86.

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 26: Release-Cut Automation

**STATUS: CLOSED PARTIAL, 2026-07-30 — operator decision.** 9 plans executed,
verification **11/11**, 763 tests passing — and **not shipped**. Two independent
review passes found Critical defects in the release executor (7, then 5 after a
fix round: 1 closed / 5 partially-closed / 1 regressed), none of which any test
ever caught. Rather than a third fix round, the executor (**999.25**) was
**re-opened and deferred to its own future phase**; see that entry for the five
open Criticals, the carried-forward sound pieces, and the hard prerequisite.

**Delivered and sound:**

- **999.5** — CHANGELOG content generation from the conventional-commit
  classifier. Clean across both reviews, zero findings against it, fully
  tested. This retires a backlog item deferred three times for want of a
  content source.

- **The resume-ledger design (D-06a)** — reviewed as the best-built new code
  in the phase (live state provably wins over the ledger; schema versioned and
  checked before deserialization; corrupt/forward-version ledgers refuse
  loudly). Its lifecycle gap (CR-05) is a missing terminal state, not a design
  flaw.

- **D-10 held throughout** — no signing-viability predictor was reintroduced.

**Built but NOT shippable — carried to 999.25:** the release executor,
`--execute`/`--yes-release`, the `cargo publish` primitives, and
`devflow sync` (999.52). **Note `sync` is affected too**: it is a mutating
command sharing the same `mutating_project_root` guard that CR-01 bypasses via
an inherited `GIT_DIR`, so it is not independently shippable ahead of 999.39
either.

**Code is unmerged on `feature/phase-26`** (~75 commits ahead of `develop`).
Nothing was merged, tagged, version-bumped, or pushed at any point across the
run — verified repeatedly. The branch is retained as the starting point for
999.25's re-attempt.

**Also blocked on an operator action (W-17):** the live `develop` ruleset is
`enforcement: active` with an **empty bypass list**, so the direct push this
phase's design depends on cannot land against this repository, and all three
`26-UAT.md` items are gated on that precondition. Repository settings, not code.

**Goal (as scoped):** Make `devflow release` *execute* the release-cut sequence —
version bump → direct push to `develop` → develop→main release PR
(human-merged) → signed tag → sync back to `develop` (direct push) →
publish `devflow-core` then `devflow` — not just the read-only `--check`
preflight Phase 20's 20d delivered. Adds a real `devflow sync` subcommand
(999.52, both standalone and executor-internal) and fixes the changelog's
placeholder content (999.5) by generating it from the conventional-commit
classification Phase 25's version-bump step already computes.

**Promoted 2026-07-29**, then **re-scoped 2026-07-29 during discuss-phase**
— see `26-CONTEXT.md` for the full discussion. Originally promoted from four
backlog items, each re-verified open at HEAD (`76e49f1`) before promotion:
999.25 (executor), 999.54 + 999.50 (signing-viability predictor fixes), and
999.52 (sync). Discussion with the operator changed the shape substantially:

- **Automation ceiling widened, not narrowed.** `develop`-bound merges
  (version bump, sync-back) are now **direct pushes**, not PRs — the
  operator wants to eliminate the PR requirement for `develop` specifically
  (via a GitHub ruleset bypass they'll configure themselves, out of this
  phase's scope) while keeping `main` PR-gated and human-merged. `cargo
  publish` for both crates is in scope, driven by the executor itself
  (previously 100% manual, never run by any DevFlow code). A new
  `--yes-release` flag (separate from `--yes-ship`) authorizes the whole
  bump→tag→sync→publish sequence, matching the existing dangerous-operation
  pattern.

- **999.54 and 999.50 dropped from this phase AND removed from the backlog
  entirely.** The operator determined DevFlow should never predict
  tag-signing viability at all — the executor's tag step just runs the real
  signed `git tag` command (CONTRIBUTING.md's already-documented explicit
  key-selection form) and reads git's own real result, rather than
  maintaining a second "will this work?" implementation that has to stay in
  sync with git's actual behavior (the exact bug class those two items
  were about). See `26-CONTEXT.md` D-10 for the full reasoning.

- **999.4 (concurrent-ship tag race) considered and also removed from the
  backlog entirely.** Its race scenario is specific to `devflow parallel`
  (multiple whole phases concurrently); the operator does not and would
  never use DevFlow that way for a single user. See `26-CONTEXT.md` D-11.
  `devflow parallel`'s own future (deprecate vs. repurpose for intra-phase
  workstreams vs. leave alone) is captured as a deferred idea for its own
  future phase — explicitly not decided here.

- **999.5 (changelog placeholder) added**, using added capacity from
  dropping 999.54/999.50/999.4 — see `26-CONTEXT.md` D-12. Deferred three
  times previously for want of a content source; Phase 25's
  conventional-commit classifier now provides one for free.

- **Two other backlog items from the same original candidate table**
  (999.31 modular agent driver, 999.15 hermetic shell-entrypoint tests,
  999.21 AI-acceptance wiring) were **considered for the added capacity and
  explicitly declined** — none share a domain or files with release
  automation, and bundling them would recreate the multi-domain scope-creep
  this phase's construction was specifically trying to avoid. Candidates
  for their own future phase, not lost.

**Sequencing.** Fixed order across the three retained items: build
`devflow sync` (999.52) first since the executor's sync step calls it
directly; then the changelog-content change (999.5), since it's small and
independent; then the 999.25 executor itself, which composes the sync
subcommand and the changelog generator as two of its steps alongside the
new develop-push, tag, and publish code. 999.25 remains the largest, riskiest
unit — it is the first production code anywhere that pushes `develop`,
creates a release tag, or runs `cargo publish` (all three previously
100%-manual or nonexistent), which is exactly why the discuss-phase gave it
this much design attention before planning starts.

**Explicitly excluded, with reasons — do not re-add without revisiting these:**

- **999.55 / DEN-80** (`phase7_cli::wait_for` fixed 5s timeout, Medium/S) —
  confirmed still open (`phase7_cli.rs:100-124`, hardcoded `200 × 25ms`, 7 call
  sites), but it is test-only, touches no file this phase's cluster touches,
  and is cheap enough to run standalone (`gsd-quick`/`gsd-fast`) rather than
  fold into this phase's scope. Handled outside this phase, separately — not
  excluded for cause.

- **999.39** (production git calls don't scrub `GIT_DIR` etc., Medium/M) —
  confirmed still open: all ~86 production `Command::new("git")` call sites
  pin `current_dir()` but never scrub environment; only the test-only
  `test_support::git_command` does, and only inside `#[cfg(test)]` blocks.
  Real defense-in-depth against a repeat of the 999.37 incident class, but it
  touches most of `crates/` including `git.rs`, where this phase's cluster is
  already working — bundling it risks the same file-overlap serialization tax
  this phase is trying to avoid by construction. Deferred to its own phase
  (27+), not because it's low-value.

**Requirements**: TBD — no REQ-IDs; tracked by backlog identifier (`999.25`,
`999.54`, `999.50`, `999.52`), not invented REQ-IDs. See
`superseded/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md` for the original backlog
context (possible shapes, publish-ordering constraint, prior deferral
reasoning from Phase 20 D-03).
**Depends on:** Phase 25
**Plans:** 0 plans

Plans:

- [ ] TBD (run /gsd-discuss-phase 26, then /gsd-plan-phase 26 to break down)

---

*Historical record below, superseded by the FINAL STATUS above but retained:*

**Status (as of plan 23-11): all 11 plans executed, but the phase's own behavioral
acceptance criterion is NOT met.** Plan 23-11 (2026-07-26) ran the acceptance attempt:
`devflow start --phase 24 --agent claude --mode auto --yes-ship` stopped at
Define — the target phase's own ROADMAP entry (promoted by this phase's own
plan-10/11 orchestration) was unreachable from `develop`, the branch
`devflow start` always forks a fresh worktree from, because the promotion
landed only on `feature/phase-23`, still unmerged. Operator verdict:
`record: valid`, `failed`. Root cause is an orchestrator sequencing gap
across plans 23-10/23-11, not a DevFlow defect — see
`23-11-SUMMARY.md`/`23-ACCEPTANCE-RUN.md` for the full record and what a
retry needs (Phase 23 merged to `develop` first, plus a named third
precondition check).

**Gap-closure decision, operator-confirmed 2026-07-26.** The third precondition
is to be shipped as a **`devflow start` guard**, not merely a documented
operator checklist item. Before forking its worktree, `devflow start --phase N`
must verify that phase N's `ROADMAP.md` entry and `.planning/phases/<N>-*/`
directory are reachable **from the base branch it is about to fork from** — and
refuse with a legible message when they are not. Rationale: the guard converts
a ~90-second silent flounder at Define into an immediate, actionable refusal,
and prevents recurrence structurally rather than by discipline. A checklist
item would only have caught this if the operator remembered to run it, which is
exactly what failed in 23-10/23-11. This is an input to `/gsd-plan-phase 23
--gaps`; the gap plan should ship the guard **and** re-run the acceptance
attempt against Phase 24.

**Deliberately NOT the acceptance target: the test-hygiene work (999.46 /
DEN-70).** It was considered and rejected on 2026-07-26. Three reasons: (1) its
headline symptom does not reproduce on the current tree; (2) **reflexivity** —
the acceptance run *is* a `devflow advance` process tree, and process-reaping
teardown executes inside the `cargo test --workspace` that DevFlow's own
Validate stage runs, so an over-broad reaper can kill the harness observing it,
making "DevFlow failed" indistinguishable from "the test suite shot the
supervisor"; (3) it discards Phase 24's existing vetting. "Test-only" is not
"low-consequence" when the tests manipulate the same process class the harness
depends on. Phase 24 remains the target; 999.46 should be done as ordinary
work, not driven by DevFlow.

**Why this scope, from the run record.** `.devflow/events.jsonl` shows **no
phase has ever completed a full five-stage devflow-driven run**:

| Phase | Agent | Furthest reached | How it ended |
|-------|-------|------------------|--------------|
| 17 (2026-07-18) | Claude | define→plan→code→validate→**ship**→loop_back→code | two silent monitor deaths, ~4h lost, recovered by hand |
| 21 (2026-07-23) | Claude | `self_dogfood_stale_blocked` → define | `workflow_finished` after one stage |
| 22 (2026-07-24) | Codex | define→plan→gate→`stale_blocked`→plan relaunch | stops dead — no `advance_evaluated` |

Phase 17 is the high-water mark and predates a month of changes. Phase 22's
death was Codex-specific (999.31). The staleness guard blocked two of the three
runs; 21d addressed that and shipped in v1.8.0/v1.8.1, so it should not recur —
23a verifies rather than assumes.

**Deliberately out of scope, and why:**

- **999.31 / DEN-56 (Modular Agent Driver, High, L)** — the highest-priority
  backlog item by label, and **not a blocker here**. Its root cause is
  `Stage::gsd_command()` emitting raw `/gsd-*` strings that *Codex* receives as
  literal shell commands; Claude Code consumes slash commands natively. Deferring
  it removes the single largest item from the critical path. It returns as the
  prerequisite for onboarding any second agent.

- **999.25 / DEN-50 (release-cut executor)** — "end to end" here ends when the
  **Ship stage completes** (merge, version bump, changelog) on the branch. The
  crates.io publish stays manual; it drives irreversible operations and needs its
  own failure/rollback design pass.

- **999.4, 999.26** (concurrency/contention) — only bind under concurrent ship or
  `devflow parallel`, neither of which is on the single-phase happy path.

Units (operator-decided 2026-07-25; sequencing is load-bearing):

- **23a** — **Dogfood probe.** Run `devflow start` on a small real phase with a
  ≥v1.8.1 binary and record exactly where it dies. **Sequence first**: it either
  confirms the supervisor is the blocker or surfaces something cheaper that bites
  before it, and it is the only unit that can invalidate the rest of this scope.

- **23b** — **Socket-addressable supervisor** (999.33 / DEN-58). Replace the
  `sh -c` monitor with a socket-addressable supervisor. Two properties carry this
  phase: the `advance` tail stops being a separate forkable process and runs
  in-process — **removing the Phase 17 failure mode by construction** — and
  liveness becomes answerable as GONE/STALE/ALIVE with no PID, so a dead monitor
  is no longer indistinguishable from a healthy between-stages pause. Design is
  spike-proven (C1–C6 + R-A..R-M); the spike is preserved at
  `.planning/spikes/socket-supervisor/`. The migration, not the mechanism, is the
  work: ~8 files consume `spawn_monitor`/`wait_for_agent_pid`/`wait_for_agent_exit`.

- **23c** — **`devflow stop`** (999.34 / DEN-59). Explicit clean phase abort.
  Blocked on 23b only; falls out cheaply once the socket handle exists. Includes
  R-M — a stop must **suppress** advance, since a stopped phase must not advance
  its own state machine.

- **23d** *(subtractive)* — **Drop `sequentagent`.** ~110 references across 11
  files. Shrinks 23b and closes DEN-58's explicitly-untested
  `wait_for_agent_exit` gap in the riskiest part of the migration. Coherent with
  Claude-only: token-exhaustion failover has no second agent to reach. The
  capability itself is preserved as an intent in 999.42 / DEN-67, to be
  reimplemented on the supervisor if and when a second agent is supported.

**Acceptance criterion is behavioural, not code-shaped:** one phase driven
start-to-finish by `devflow` with Claude, unattended, reaching a completed Ship
stage without manual intervention.

**macOS note:** DEN-58 flags macOS as entirely unverified (no host, no CI) and
the 104-byte `sun_path` limit as documented-not-measured. Out of scope here —
the operator platform is Linux; do not claim macOS support from this phase.

**Re-aimed 2026-07-25 (operator decision, after 23a's probe).** 23a ran and
**disproved the phase's central hypothesis.** The `sh -c` monitor does not die:
one 59-minute unattended run carried 11 `stage_launched` events, archived a
capture on every hop, counted failures correctly, fired the threshold gate
correctly, ended with `infra_failures: 0`, and was still alive at the end
(`23-PROBE-FINDINGS.md`). It made the first full Define→Ship traverse on record,
and a second independent run reached Ship within the same hour. Both were
stopped by **content/config gates**, and by two *different* ones — a false-green
`VERIFICATION.md` scoring an unrun Ship stage, and a `/gsd-ship` preflight block
on a missing SECURITY.md (`23-ORPHAN-FORENSICS.md`).

The real defect is the inverse: monitor **over-durability**. Forensics on 27
orphaned pairs (54 processes, 168.6 MB, gates up to 30h old) found `wait $apid`
had already returned in every one; what was still running was the trailing
`devflow advance`, blocked on a gate whose wait is bounded at 7 days
(`config_parse.rs:24-28`) — bounded so loosely it is operationally
indistinguishable from unbounded. No detach, no reaper, no way to enumerate what
is gated, and no command to stop a running phase.

Plans 23-03…23-12 were built on the disproved premise and are archived to
`superseded/`. The replanned set is aimed at the evidence:

- **23b — REDEFINED.** No longer "replace the `sh -c` monitor with a socket
  supervisor." Now **bound gate lifetime**: a cross-root registry plus
  `devflow gate list --all-roots` (23-03), and `devflow gate sweep` auto-rejecting
  aged gates through the existing `Gates::respond` protocol so the still-polling
  `advance` tears itself down via its own `abort()` path — no signal, no
  supervisor, no new dependency (23-04).

- **23c — REDEFINED, smaller.** `devflow stop` (23-05), no longer blocked on 23b.
  Built against the existing per-phase lock file, which already records the exact
  PID to signal. Targets the lock holder, never `state.monitor_pid` — the monitor
  shell's trap only ever tracks the agent, so signalling it orphans `advance`
  rather than stopping it.

- **23e — NEW this replan.** The false-green attestation class: a structural
  Ship-evidence oracle (`devflow evidence`), declarable as a Layer 0 probe, plus
  an enforced merge post-condition (23-06). `devflow-core` never reads
  `VERIFICATION.md`, so the catch that worked was a non-deterministic prompt-side
  review, not an enforced invariant.

- **23d — UNCHANGED**, and no longer front-loaded. Its original "delete before
  the migration" rationale died with the supervisor deferral, so it now follows
  the evidence-priority work (23-07, 23-08).

- **`--yes-ship` — UNCHANGED, and now the binding constraint** on the acceptance
  criterion, since `Mode::should_gate` gates Ship in both modes (23-09).

- **The socket-addressable supervisor is DEFERRED, not discarded** — with it,
  D-08 and D-10. Nothing in the evidence shows this phase's acceptance criterion
  requires it; building it now would fix a problem the probe did not find.
  D-09's `~/.cache/devflow/` location decision is reused for the new registry.

Sequencing: enumeration → reaper → stop → evidence oracle → 23d → `--yes-ship`
→ acceptance prep → acceptance run. Waves are mostly sequential because almost
every unit touches `commands.rs` or `main.rs`, and the same-wave
zero-file-overlap rule forbids parallelism.

**RESEARCH correction carried into the plans:** the deletion inventory is
**142 references across 11 files**, not the ~110 recorded above — the original
count came from a lowercase-only grep that missed the PascalCase Rust
identifiers. The 11-file count is correct. Four operator documents mention the
verb (README, ARCHITECTURE, OPERATIONS, CHANGELOG), not two.

**Cross-AI review revision, 2026-07-26 (`23-REVIEWS.md`, verdict
`changes_requested`).** Three lanes (Codex / OpenCode / Hermes); every HIGH
finding was re-verified against source by the orchestrator before replanning.
Plans 23-03 … 23-11 were revised in place — no renumbering, no new plan, no
change to any `depends_on` edge, so the wave assignment above is unchanged.
Four required findings, all now closed in the plan text:

1. **23-06's shipped predicate was itself a false green.** `workflow_finished`
   is emitted at two sites, not one — real Ship finalization
   (`pipeline_gate.rs:221`, `Null` payload) *and* `transition`'s
   `devflow start --until` clean-stop branch (`pipeline_gate.rs:79`, payload
   `{"reason":"stopped_at"}`), which returns before any hook runs. The oracle
   built to eliminate false greens would have reported **shipped** for a phase
   halted after one stage — the shape the run record already logs for Phase 21.
   Fixed by emitting a distinct terminal-only **`workflow_shipped`** event with
   exactly one emission site and making that the predicate.

2. **23-10's one-way authorization preceded the rehearsal it demanded
   confirmation of.** Split into a reversible target selection (Task 1) and the
   one-way authorization (Task 4), with the rebuild, recovery rehearsal, remote
   restore-path discovery and both content preconditions in between.

3. **Verification chains could exit 0 on a broken build.** The
   `cargo test … | rg -q 'FAILED' && exit 1 || cargo clippy …` shape in four
   plans falls through to the `||` branch when a compile, link, or panic failure
   prints no `test result: FAILED` line. Replaced everywhere with direct
   `&&` status chains; targeted runs now capture to a gitignored log and gate on
   cargo's own exit status before asserting a nonzero pass count.

4. **23-03's registry contradicted itself on concurrency.** Storage reshaped
   from one shared `roots.json` to one file per `(project_root, phase)`, so a
   concurrent registration cannot be lost and "a running phase cannot be missing
   from the registry" is structurally true rather than aspirational.

Plans:

- [x] 23-16-PLAN.md

- [x] 23-01-PLAN.md — Rebuild the binary and scaffold an isolated scratch probe target (23a)
- [x] 23-02-PLAN.md — 23a probe: one unattended run, recorded where it stopped (23a)
- [x] 23-03-PLAN.md — 23b: cross-root gate registry (one file per root/phase) + `devflow gate list --all-roots`
- [x] 23-04-PLAN.md — 23b: `devflow gate sweep` — bound gate lifetime by auto-rejecting aged gates
- [x] 23-05-PLAN.md — 23c: `devflow stop`, targeting the lock holder
- [x] 23-06-PLAN.md — 23e: terminal-only `workflow_shipped` event + Ship-evidence oracle + enforced merge post-condition
- [x] 23-07-PLAN.md — 23d: delete the two-agent verb from the CLI crate + reconcile docs
- [x] 23-08-PLAN.md — 23d: delete the core-side surface, workspace count to zero
- [x] 23-09-PLAN.md — `--yes-ship`: per-run flag, one auto-answered Ship gate
- [x] 23-10-PLAN.md — Acceptance prep: target selection, rehearsed recovery point, preconditions, then one-way authorization
- [x] 23-11-PLAN.md — Acceptance run: one phase Define→Ship, unattended, self-hosted
- [x] 23-12-PLAN.md — 23f: the `devflow start` reachability guard — refuse before scaffolding when the phase is not on the base branch
- [x] 23-13-PLAN.md — 23f: merge the guard to `develop` (operator checkpoint), rebuild, prove at runtime the binary carries it
- [x] 23-14-PLAN.md — Acceptance preconditions re-measured on the post-merge tree, fresh recovery ref, one-way launch decision
- [x] 23-15-PLAN.md — Acceptance retry: one phase Define→completed Ship, unattended, judged only by `workflow_shipped` + `--require-shipped`

*(The original 23-03…23-12 are archived under `superseded/` — see the re-aim
note above. The plan list below renumbers from 23-03; 23-01 and 23-02 are
unchanged and already merged.)*

**Wave 1**

- [x] 23-01 — Rebuild the binary and scaffold an isolated scratch probe target (23a)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 23-02 — 23a probe: drive one unattended run in the scratch repo, record where it stopped (23a, tracer)

**Wave 1 (replanned set)**

- [x] 23-03 — 23b: registry module, registration on the launch path, cross-root gate listing with age (23b)

**Wave 2** *(blocked on 23-03)*

- [x] 23-04 — 23b: `Gates::reap` + `devflow gate sweep`, proven against a real parked `advance` child (23b)

**Wave 3** *(blocked on 23-04)*

- [x] 23-05 — 23c: `devflow stop` — gate-response path, lock-holder signalling fallback, identity check (23c)

**Wave 4** *(blocked on 23-05)*

- [x] 23-06 — 23e: terminal-only `workflow_shipped` event, `devflow evidence` oracle, `--require-shipped`, merge post-condition in the Merge hook (23e)

**Wave 5** *(blocked on 23-06)*

- [x] 23-07 — 23d: delete the verb, preserve single-agent resume, reconcile four docs (23d, D-11/D-12 checkpoint)

**Wave 6** *(blocked on 23-07; the two plans below run in parallel — zero file overlap)*

- [x] 23-08 — 23d: delete the core-side surface, re-point constructor coverage (23d)
- [x] 23-09 — `--yes-ship`: persisted per-run flag, one auto-answered gate, two negative guarantees (yes-ship)

**Wave 7** *(blocked on 23-08 and 23-09)*

- [x] 23-10 — Acceptance prep: target selection, seven behavioural checks, rehearsed recovery point, content preconditions, then the D-07 one-way authorization (all units) — operator authorized PROCEED against backlog 999.27 (-> phase 24), both content preconditions accepted unmitigated. Orchestrator must promote 999.27 to phase 24 in this file before dispatching 23-11.

**Wave 8** *(blocked on 23-10)*

- [x] 23-11 — Acceptance run: one phase Define→Ship, unattended, self-hosted (all units) — **plan executed, record valid; ACCEPTANCE FAILED** (target Phase 24 unreachable from `develop` at launch — orchestrator sequencing gap, not a DevFlow defect). Phase's behavioral acceptance criterion NOT met by this run; see `23-11-SUMMARY.md` "Next Phase Readiness" for what a retry needs.

**Gap-closure set, planned 2026-07-26 (`/gsd-plan-phase 23 --gaps`).** Four
plans, strictly sequential — each wave's output is the next wave's precondition.
Two new requirement tokens: **23f** (the guard) and **23-acceptance** (the
behavioural retry), so the phase-level criterion is traceable separately from
the code unit.

`23-VERIFICATION.md` records exactly one gap (truth 8, the behavioural
acceptance criterion) with two `missing:` items. The first item's merge
precondition is **already satisfied** — Phase 23 reached `develop` via PR #31,
and Phase 24's ROADMAP entry and `.planning/phases/24-*/` directory are both
reachable from `origin/develop` — so no plan re-does it. The second item ships
as code per the operator's gap-closure decision above. Units 23a–23e and
`yes-ship` all VERIFIED and are not replanned.

**Wave 9** *(gap closure; no dependency on any earlier gap plan)*

- [x] 23-12 — 23f: `PhaseReachability` probe + refusal in `preflight.rs`, wired into `commands::start` ahead of both fork paths; e2e regression test proven red against the unwired binary; fails open when the base branch carries no `ROADMAP.md`, so the pre-existing `phase7_cli` suite is unaffected (23f)

**Wave 10** *(blocked on 23-12)*

- [x] 23-13 — 23f: pull request to `develop`, CI green, **blocking operator merge checkpoint** (no autonomous write to `develop`), then rebuild and a runtime refusal proof in a throwaway clone with the binary hash recorded (23f, 23-acceptance)

**Wave 11** *(blocked on 23-13)*

- [x] 23-14 — All seven behavioural checks re-run against the post-merge binary (nothing carried forward), an eighth reachability check, fresh remote-only recovery ref with a rehearsed restore, then the **one-way launch decision checkpoint** for 23-15 (23-acceptance)

**Wave 12** *(blocked on 23-14)*

- [x] 23-15 — Acceptance retry: `devflow start --phase 24 --agent claude --mode auto --yes-ship`, observed read-only, recorded in `23-ACCEPTANCE-RUN-2.md` (the 23-11 record is left byte-identical). Acceptance is claimable **only** on a quoted `workflow_shipped` event plus `devflow evidence --phase 24 --require-shipped` exiting 0 — `workflow_finished` is explicitly not sufficient (23-acceptance)

### Phase 30 (original number, SHELVED 2026-08-01 → spike): Withdraw DevFlow From the Release Business

**SHELVED 2026-08-01, before execution.** Operator decision: the project's stated objective for
the next stretch is *"a fully functional DevFlow — whatever we define its functionality to be —
that the operator can start using and that ends successfully."* Four dogfood failures on
2026-07-31, all in the core Define→Plan→Code→Validate→Ship loop and none release-related, mean
that loop is the binding constraint, not release automation. **This phase's number and slot are
reassigned to 999.64** (below); everything produced here is preserved as a spike, not executed.

**What survives, and where.** All planning work — the scope, the Fable adversarial review, three
plan-checker/Codex review rounds (round 2 found a false-green class the checker missed twice;
round 3 found a wave-3 compile contradiction the fix round introduced), and the final unresolved
question (does DevFlow verify a PR exists via `gh`, and what happens when `gh` is unreachable —
answered mid-discussion: **automation halts at a never-silent gate; it does not fail-soft and
does not guess**) — is preserved verbatim on branch **`spike/release-withdrawal-plans`**
(`f3ccd9a`, pushed to origin, never merged). It is a **spike, not a starting point**: the
`gh`-reachability gate decision was never folded into the plans before shelving, so a future
attempt resumes review, not execution.

**Why shelve rather than finish it small.** The scope survived three review rounds and was
converging — this was not a Phase-26/29-style failure. It was deprioritized because it competes
for the same review and execution attention 999.64 needs immediately, and 999.64 blocks
*everything*, including any future dogfood of a resumed release-withdrawal phase. Sequencing, not
abandonment.

**Original entry follows, retained for provenance:**

**Scoped 2026-07-31**, immediately after Phase 29 was aborted. **Subtractive phase — it deletes
capability rather than adding it.** That is the point.

**Goal:** DevFlow's mandate ends when a **reviewable pull request exists**. It drives
Define → Plan → Code → Validate, pushes the branch, opens the PR, and stops. The operator merges,
versions, tags, and publishes. Every irreversible operation leaves DevFlow's surface.

#### Why, from this project's own evidence

Two full phases (26 and 29, ~120 commits) attempted release automation. Neither shipped. Both
died to the same class of defect — irreversible operations whose failure modes are invisible to
tests by construction. Phase 26: 763 green tests, 11/11 self-verification, 12 Criticals. Phase 29:
921 green tests, clean lint, 5 Criticals + 1 High from an independent cross-AI review, three of
them in the *read-only* unit.

**Meanwhile the actual goal — run a development phase autonomously — has never been met, and the
2026-07-31 dogfood run failed at it four separate times, none release-related:**

| Stage | Outcome |
|---|---|
| Define | no-op'd correctly, 13s |
| Plan | succeeded, produced sound plans |
| **Code** | **failed** — backgrounded executors orphaned, stage reported success (999.64) |
| Validate | worked correctly, found the phase incomplete, looped back |
| **Loop-back** | **failed** — `--gaps-only` matches zero plans mid-arc, gated |
| Ship | never reached |

Plus two latent defects surfaced while investigating (the failure counter that gates healthy
multi-wave progress at wave 3; 49 leaked fixture processes, 999.46).

**The trade is bad and always was.** The executor's remaining value is saving ~15 minutes on an
operation performed roughly monthly, deliberately, by an operator who wants to watch it. It has
cost two phases. Removing it eliminates — not fixes, *eliminates* — the entire defect class, and
frees all capacity for the reliability work that actually serves the project's stated purpose.

#### Adversarial review of this plan (Fable, 2026-08-01) — the gate Phases 26/29 skipped

This scope was adversarially reviewed **before planning** by Claude Fable 5, instructed to verify
every claim against live source and to kill the plan if it deserved it. Verdict:
**APPROVE-WITH-CHANGES — 4 Critical, 4 High, 4 Medium findings against the plan as drafted.** All
are folded into the units below; the four Criticals were: (C1) the `compute_version` fork was
false — the D-09 gate is built on the `semver`/`git-conventional` crates, so "delete the deps"
does not compile, and D-09's own fate had never been ruled; (C2) emptying the hooks silently
inverts what `workflow_shipped` means while `ship_evidence` — mechanically read-only but
*semantically defined by that event* — keeps vouching for the old meaning; (C3) the acceptance
run as drafted would have **performed the old hooks on the real repo** (the driving binary is the
installed one, not the branch under test); (C4) `GitFlow::release_start`/`release_finish`/
`feature_finish` are the same release-acting class, compiled and public, and the draft never
mentioned them — "fixed the instance, left the class open," again. The review also proved the
draft's 30d fixture could not catch the historical defect (the incoherent code never pushes, so a
pre-receive hook never fires) and ratified the premise and the 30a-before-999.64 sequencing.

#### Operator rulings (2026-08-01, all three confirmed)

- **R5 — D-09 survives as awareness, not action.** The major-bump gate is kept, reworded as a
  breaking-change *awareness* gate: commit classification is a deterministic local fact, not a
  D-10-class environment prediction. **DevFlow may still notice release-significant facts; it may
  no longer act on them.** `compute_version`, `apply_bump`, `write_version` are deleted; the
  `semver`/`git-conventional` dependencies stay (D-09 needs them). Most of Phase 25's 25c work
  survives through D-09. The "advisory version row" alternative is rejected — a predictor
  presented to the operator as truth is the class D-10 generalized against.

- **R6 — `workflow_shipped` is re-documented, not renamed.** New meaning: *"phase run completed
  through Ship approval; PR open; the DevFlow binary performed zero git mutation."* The event
  name is kept because the event log is append-only — a rename would split its history into two
  vocabularies. `ship_evidence.rs` doc comments, the `--require-shipped` failure text, and the
  tests asserting hook-success-before-emission are all updated in the same change.

- **R7 — `BranchCleanup` is deleted** (supersedes this entry's earlier "retained" text, which is
  hereby amended): with `Merge` gone, its non-force `git branch -d` refuses every unmerged branch
  and the hook degrades to a permanent warning no-op. GitHub deletes branches on PR merge.

#### Units

- **30a — remove release *acting*, the class and not the instance.** The deletion list, per the
  review's C4/H3/M3 extensions:

  - Hooks: `Merge`, `VersionBump`, `ChangelogAppend`, `BranchCleanup` (R7) — the fns, the `Hook`
    variants (serialization-safe: the enum derives no `Serialize`; events store debug strings),
    and the GAP-7 `shipped_version`/changelog-body threading in `HookContext`.

  - **Library primitives of the same class:** `GitFlow::tag`, `GitFlow::release_start`,
    `GitFlow::release_finish`, `GitFlow::feature_finish` — public, compiled, only test callers,
    and `release_finish` is a full local release cut. An uncalled capability is an affordance for
    the next caller; withdrawing means the library *cannot express* release-acting.

  - **The entire signing-predictor chain** (H3): `check_signing`, `check_signing_viability`,
    `check_ssh_signing_viability`, `SigningViability`, and their tests. No production consumer
    remains; D-10 says this must never exist; it carries two live-measured false negatives.

  - Orphans (M3): `ship::prepend_changelog`, `version::write_version`,
    `version::detect_version_file`/`hooks::has_version_file`, and `Hook::BranchCreate` (zero
    production callers *today* — flagged now rather than discovered mid-execution).

  - **Blast radius includes `pipeline_gate.rs`, the real production caller** (H1, absent from the
    draft): the finalization-retry loop and its reopened never-silent Ship gate, the Ship gate
    text "Ship complete — approve merge?" (there is no merge to approve), "phase N shipped —
    workflow complete", `ship_override`'s "re-running the terminal hooks" contract, and
    `hook_context_root`'s terminal-batch arm. The checkout lock **stays** — it serves every batch
    including the surviving `DocsUpdate` (the draft's "machinery is dead" claim was wrong).

  - `workflow_shipped`/`ship_evidence` re-documentation per R6 — an explicit task, not a side
    effect (C2).

- **30b — `release --check` keeps facts, loses predictions.** Publish-order check retained.
  `check_signing` row removed (H3). Stale text rewritten: `check_self_pin`'s "VersionBump should
  have rewritten this" hint and `workspace_version_pin`'s framing (M2) — the pin test itself
  stays, it guards CONTRIBUTING step 1. The D-14 `gh auth` preflight keeps its check but fixes
  its rationale comment — the hooks never pushed; the check's real justification is
  `/gsd-ship`'s `gh pr create` (M1).

- **30c — documentation.** CONTRIBUTING.md becomes the sole release authority. Verified
  non-regression (review, Info): zero hook-authored changelog commits exist in all history — the
  entries were stranded on the unpushable local `develop` — and CONTRIBUTING step 1 already has
  the operator hand-writing CHANGELOG.md, so removing `ChangelogAppend` regresses nothing real.

- **30d — hermetic remote fixture, divergence-first (H4 rewrite).** A local bare repo as
  `origin`. **The load-bearing assertion is divergence: after driving the real production Ship
  path (`finish_workflow`/`advance`, never a hand-rolled loop), `git rev-list
  origin/develop..develop` is empty** — the incoherent code merged locally and never pushed, so
  a push-rejecting hook alone would never fire against it. All four acting artifacts are
  asserted: no divergence, no local `v*` tag, feature branch intact, no changelog commit. The
  `pre-receive` reject on `refs/heads/develop` is retained as belt-and-suspenders against
  *future* push-based acting, explicitly not as the mechanism. **The test must be demonstrated
  red against pre-30a HEAD once, and that run recorded** — a test that has never been red is a
  hope, not a proof.

#### Acceptance (rewritten per C3 — must be executable today)

1. **30d red-then-green:** the fixture test fails against pre-30a HEAD, passes after.
2. **Foreground terminal-path run:** `devflow ship` (`ship_override`) driven against a real clone
   with a real `origin`, phase state at Ship, pre-written gate response — exercising the exact
   production finalization path with a remote present. End state: PR open, feature branch
   intact, local `develop` not ahead of `origin/develop`, no local tag.

3. **Any live dogfood Ship requires the binary rebuilt from the 30a branch first** — with the
   installed binary, the acceptance run itself would execute the old hooks against the real
   repo (C3). Recorded as a hard prerequisite, same class as the standing rebuild-before-
   revalidate rule.

4. Tests-pass is explicitly **not** acceptance (the hooks' tests were green for their entire
   defective life; the fixture had no remote — the property was inexpressible).

**Explicitly NOT in scope:** rebuilding the observer to executor grade — Phase 29's
`release_observe.rs` carries findings 2/3/6 and is not salvaged. The real-GitHub E2E harness
(30e) — deferred until 999.64 lands (R4); a harness cannot test a successful Ship until DevFlow
can produce one.

**Sequencing (ratified by review, axis 7):** 30a before 999.64 is correct and now urgent-adjacent:
every future 999.64 dogfood that accidentally reaches Ship with the current binary corrupts the
operator's local `develop` and tag namespace. Removing the hazard first makes all subsequent
reliability work safer. 999.64 lives in the launch/session machinery, orthogonal to
`finish_workflow` — no rework coupling.

**Requirements:** TBD — tracked by unit identifier (`30a`–`30d`) plus rulings (`R1`–`R7`),
consistent with Phases 21/22/26/27/28/29.
**Depends on:** nothing.
**Priority:** High | **Size:** M — subtractive in intent, but the blast radius now includes
`pipeline_gate.rs`'s finalization path and the `workflow_shipped` semantic migration.

Plans:

- [ ] TBD — promote with `/gsd-plan-phase 30`

---

#### The reliability queue this unblocks — the project's actual purpose

Ranked by what stands between DevFlow and a single autonomous phase. **Not part of Phase 30**;
recorded here so the sequence is not rediscovered.

1. **999.64 — one-shot launch kills the session at turn end.** THE blocker. GSD delegates to
   subagents by design; `claude -p` cannot survive the orchestrator's turn ending, so any wave with
   2+ plans orphans its work. **Until this is fixed, no phase containing a multi-plan wave can
   complete autonomously.** Has a measured candidate fix (`--input-format stream-json` keeps the
   process alive until stdin closes) and a cheap feasibility experiment that **must run first**.

2. **999.65 — the loop-back issues an impossible command.** Validate → Code always loops with
   `/gsd-execute-phase N --gaps-only`, which selects only `gap_closure: true` plans. A mid-arc phase
   has none, so the fix loop matches zero plans by construction and the Code↔Validate loop cannot
   advance an unfinished phase. Second recorded occurrence. Filed as its own backlog entry
   2026-08-01 (was prose-only here).

3. **999.66 — `consecutive_failures` accumulates on healthy progress.** Monotonic inside the
   Code↔Validate loop — `(Code, Validate)` is excluded from the reset predicate and the loop-back
   path bypasses `transition()` entirely. A six-wave phase trips the 3-strike gate around wave 3
   reporting "Validation failed 3 time(s)" about a phase that never failed. Filed as its own
   backlog entry 2026-08-01 (was prose-only here).

4. **999.46 — leaked fixture processes.** 49 live on 2026-07-31, self-inflicted by the test suite.

**Fixing 1 and 2 makes an autonomous phase possible for the first time.** Neither is large.

**The encouraging finding, recorded because it is easy to lose in a day of failures:** every failure
on 2026-07-31 was in the *harness*, not the capability. Plan produced good plans. The executors
produced working, tested, documented code — the Codex findings are *design* defects in code that
compiled, passed its tests, and did what it claimed. Nothing observed suggests the agents cannot do
this work. It says DevFlow cannot yet reliably drive them. That is a better problem, and it is the
one this project is about.

---

### Phase 29: Release-Cut Executor — Observe, Then Act Within the Repo's Rules (ABORTED — 2026-07-31)

**ABORTED 2026-07-31, after review.** All 7 plans executed, 6 waves merged, 921 tests passing with
clean clippy and fmt. An independent cross-AI review (Codex `gpt-5.6-sol`, high reasoning effort,
read-only sandbox, all 6,136 source lines) returned **REQUEST CHANGES / BLOCK — 5 Criticals + 1
High**, three of them in the read-only observer. `feature/phase-29` was never merged and never
pushed as a feature branch.

**Code preserved, not deleted** — operator decision, 2026-07-31:

| Attempt | Archive branch | Tip |
|---|---|---|
| Phase 26 (first) | `archive/phase-26-release-executor` | `f1b885d` (74 commits) |
| Phase 29 (second) | `archive/phase-29-release-executor` | `ac9d28c` (47 commits) |

Both are pushed to `origin`. They are **archives, not starting points** — the premise is defective
in both. Full diagnosis, the four constraints any third attempt must promise, and the process
failures that let this happen twice are in **999.25**. Superseded by **Phase 30**, which withdraws
DevFlow from release automation entirely.

*Original entry follows, retained for provenance:*

### Phase 29 (original scope): Release-Cut Executor — Observe, Then Act Within the Repo's Rules

**Promoted:** 2026-07-31 — **999.25 / DEN-50**, re-attempted after Phase 26 delivered it
PARTIAL and unshippable. Phase 26's code on `feature/phase-26` (74 commits, unmerged) is
**reference material only** — operator decision, 2026-07-31. It is not rebased and not
carried forward. Rationale: its five open Criticals are not five independent bugs but one
*lifecycle* defect — no terminal state for the non-success outcome, two code paths owning
one `v{version}` namespace, and a printed remediation that re-arms the failure it reports.
Rebasing would carry that lifecycle forward and force the redesign to argue its way out of
it; starting from the state model frees it to ask "what are the states" first.

**Goal:** A `devflow release` that *executes* the release cut instead of only checking it —
version bump → changelog → release PR to `main` → signed tag → sync back to `develop` →
publish `devflow-core` then `devflow`.

---

#### The design rule (operator, 2026-07-31) — governs every unit below

> **DevFlow discovers the repo's rules, advances as far as they permit, and stops at the
> first hard gate with an accurate report of where it stopped and why.** It never predicts
> a gate, never routes around one, and never treats stopping at one as failure.

This is **D-10 generalized**. D-10 banned predicting *tag-signing* viability, because a
predictor is a second implementation of git's behavior that must stay in sync with it —
the exact bug class that got 999.50 and 999.54 deleted rather than fixed. "Will this push
be allowed?" is the same question aimed at a different operation, and answering it by
prediction would mean reimplementing GitHub's ruleset engine, including bypass lists and
org-level rules layered above repo-level ones.

The resolution is **deterministic at one layer, adaptive at another**:

| Layer | Behavior |
|---|---|
| Action set | **Fixed and enumerable.** The executor never invents a route at runtime. |
| Route selection | **Adaptive.** May be informed by discovered facts (allowed merge methods, required checks). |
| Outcome | **Authoritative only from performing the operation** and reading the real result — never from a prediction. |

**Discovery informs the route; the attempt decides the outcome.** Corollary: do not hardcode
"squash on `main`, merge on `develop`" — that is a copy of live config that goes stale, the
same failure mode as CONTRIBUTING.md drifting in 25f. Ask which methods are allowed, apply a
fixed internal policy to choose among them, and refuse loudly if the preferred method is not
in the allowed set rather than silently taking the other one.

---

#### Ruleset finding (measured live 2026-07-31, `gh api repos/denniyahh/devflow/rulesets`)

| | `develop` | `main` |
|---|---|---|
| PR required | yes | yes |
| **Required approvals** | **0** | **0** |
| Allowed merge methods | `merge`, `squash` | `squash` only |
| Required checks | Test, Clippy, Format, devcontainer build | same |
| Branch must be current with base (`strict`) | yes | yes |
| Bypass actors | none | none |

**Required approvals are zero on both branches, so the PR route is fully automatable today** —
open a PR, wait for the four checks, merge it. No exemption, no bypass, no ruleset change.

**This retires W-17 and reclassifies it as self-inflicted.** Phase 26's D-01/D-08 chose
*direct push to `develop`* — the one route this repository forbids — and W-17 then recorded
that the executor "cannot land until the operator adds a bypass." The permitted route
needed no bypass at all. The tool was designed to fight the rules instead of following them.
**Do not add a ruleset bypass for this phase.** It is not required, and W-17's own reasoning
(the enforcement is the only thing stopping a known-defective executor from reaching
`origin`) still holds.

Both rulesets target `branch`, not `tag`, and `cargo publish` touches no refs — so neither
the tag step nor the publish steps are gated by them.

---

#### Units — split by reversibility, not by step count, and each independently shippable

The split's purpose is **not tidiness**. Phase 26 produced 74 commits and delivered nothing
usable, because one indivisible blob means a failure at the last gate makes the whole thing
unshippable. Each unit below can ship on its own; a stall in 29c still leaves 29a and 29b
delivered.

- **29a — the observer (foundation, build first).** A read-only `devflow release status`
  answering, for a given version, six questions by *looking*: version bumped, changelog
  written, release PR merged, signed tag present on the remote, sync merged, both crates
  published. Sources are **remote refs and the crates.io API** — never a local progress
  file. Touches nothing, useful shipped alone, and the only part of this feature that is
  straightforwardly testable, because pure observation is what tests are good at.
  **Ships first because it is architecture, not sequencing:** 29b and 29c act only on what
  29a reports missing.

- **29b — the recoverable actions.** Version bump (two places), changelog, release PR to
  `main`, sync PR back to `develop`. Every one is a commit or a PR: if it goes wrong, fix
  and re-run. Unlocked by the zero-approvals finding above.

- **29c — the commit point.** Signed tag and the two publishes, in order. Two steps,
  permanent consequences, and where every one of Phase 26's worst defects lived. Small
  enough to review line by line — which is the gate that actually works on this code.

**State is derived, never recorded.** Every irreversible step has an authoritative external
oracle: remote ref SHA for merges and tags, the registry API for publishes. Phase 26 built a
*ledger* — a local file recording intent, which then became the source of truth — and that
choice generated its two worst Criticals (CR-05: an in-flight ledger permanently bricks the
release path, escapable only by deleting the file, which re-arms CR-02; CR-02: ledger says
Complete while nothing is published). The review's own praise for the ledger is the tell:
*"a planted lying ledger still creates the real tag — live state provably wins."* The
design's best property is that observed state overrides it. Make observation the only
primitive.

What falls out: CR-05 dissolves (no in-flight state to get stuck in, so no clear/abandon verb
is needed); CR-02 dissolves ("complete" is computed — tag exists **and** both crates at
version N — never asserted); **resume becomes free**, because re-running *is* re-observing,
which is precisely the "independently re-runnable" property Phase 26's lesson asked for,
achieved by subtraction; and IN-01's `v{version}` namespace collision between
`hooks_after_ship` and the executor stops being a corruption path and becomes an ordinary
observation ("the tag exists, so do not create it").

**The honest cost, to be designed for explicitly:** the oracle is remote, so a network
partition is indistinguishable from "not done." Every observe step needs an explicit
**unreachable ≠ absent** arm that refuses rather than proceeds. Index lag is real and was
observed during the v2.2.0 publish (`cargo publish` printed *"waiting for devflow-core 2.2.0
to be available"*). The one thing that genuinely cannot be observed is **operator
authorization**, so a minimal persisted record may survive — for *authorization only*, never
for *progress*. Pinning that boundary is a discuss-phase task.

---

#### Operator decisions (stated by the operator; treat as locked)

- **`feature/phase-26` is reference material only.** Not rebased, not carried forward.
- **The design rule above**, verbatim: discover the rules, advance as far as they permit,
  stop at the first hard gate and report accurately.

- **NO OPERATOR-PRESENCE REQUIREMENT.** The executor **must not** carry a rule requiring a
  human at the keyboard, and **must not** refuse to run unattended. Explicitly ruled by the
  operator 2026-07-31, correcting an earlier draft of this entry that recorded the opposite.
  The reasoning that produced that draft — *"the tag step needs the maintainer's signing key,
  so it cannot complete unattended"* — is **a prediction about the environment, which D-10
  bans**. If the key resolves, `git tag -s` succeeds; if it does not, git fails and the
  executor stops and reports git's own error. Attempt, do not predict. A human-presence
  precondition is also a self-imposed gate that no repo rule imposes, which the design rule
  forbids by construction.

- **Authorization is a mandate, not a presence check.** The operator grants intent once (a
  flag, e.g. the `--yes-ship` precedent); thereafter the executor proceeds as far as the
  repo's rules and the environment permit. Do not convert authorization into a requirement
  for a live human during execution.

- **D-10 carried unchanged.** No signing-viability predictor, ever. The tag step runs the
  real `git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s` and
  reports git's own exit code.

- **D-05 carried and strengthened.** Fail-fast, no automatic rollback — trivially safe once
  every step is independently re-runnable.

- **D-06 superseded** by derived state (it specified ledger-based resume/idempotency).

#### Recommendations carried into discuss-phase (NOT operator decisions — confirm or reject)

*Provenance matters here: an earlier draft of this entry promoted an assistant recommendation
into a locked decision, which the operator caught and reversed. Everything in this subsection
is a proposal awaiting confirmation, not a ruling.*

- **Review as the primary gate, tests as necessary-but-insufficient.** Evidence: Phase 26 had
  763 passing tests and scored 11/11 on its own verification while carrying twelve Criticals
  across two rounds — every one found by a human reading code, zero by a test; fix rounds went
  7 → 5. Proposed consequence: one automated fix round maximum, then reassess the design
  rather than the bug list. **Operator has not ruled on this.**

**Motivating evidence, measured this session.** The v2.2.0 cut was performed by hand on
2026-07-31: crates published core-then-cli, and the signed tag **never created**. Nothing
failed loudly — the release quietly stopped one step short, and it took an ad-hoc query to
notice. The executor's value is **consistency and completeness, not speed**; 29a alone would
have caught this the moment it happened.

**Requirements:** TBD — no REQ-IDs; tracked by unit identifier (`29a`–`29c`) plus
`29-CONTEXT.md` decision IDs, consistent with Phases 21/22/26/27/28.
**Depends on:** Phase 27 (999.39, `GIT_DIR` scrubbing — CR-01's prerequisite, delivered).
**Priority:** High | **Size:** L — but with two points at which it can stop and still have
delivered something. Linear: DEN-50.

Plans:

- [ ] TBD — pending `/gsd-discuss-phase 29` and `/gsd-plan-phase 29`

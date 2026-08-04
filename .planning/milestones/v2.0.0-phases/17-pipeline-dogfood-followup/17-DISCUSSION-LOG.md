# Phase 17: Pipeline Dogfood Follow-Up - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-18
**Phase:** 17-pipeline-dogfood-followup
**Areas discussed:** Unknown-outcome policy (17a), Outcome taxonomy + retry (17b), Preflight design (17c), Provenance strictness (17d), main.rs extraction

---

## Scope decision (pre-discussion)

Phase 17 was spiked with six candidate units (P1–P6). User directed P5
(`devflow doctor` reconciliation) and P6 (WR-03 test fix) to Phase 18 before
discussion began, leaving four units. Recorded as 18d/18e in ROADMAP.md and in
STATE.md's 2026-07-18 decision entry.

Retrospective decision-gate Q4 ("focused Phase 17 repair, or pull only proven
defects into a Phase 16 remediation?") answered: focused Phase 17 repair. Only
17d traces to the proven Phase 16 defect; 17a–17c are capability Phase 16
never claimed, so folding them backward would misrepresent Phase 16's scope.

---

## Unknown-outcome policy (17a)

### Zero-commit Unknown

| Option | Description | Selected |
|--------|-------------|----------|
| Treat as failure | Route to `handle_stage_failure` — gate + notify, counts toward `consecutive_failures` | partial ✓ |
| Gate as ambiguous | One uniform Unknown gate for both Layer-3 cases | |
| You decide | Planner discretion within no-auto-advance constraint | |

**User's choice:** Free-text — refined the question's premise.

**Notes:** "The case that brought this to light was an external check that
wouldn't have generated any code. My concern is that we also address making
external checks a recognized validation step. But in the case that no code was
produced and no external check was made and for all intents and purposes it is
ambiguous what actually was done in a stage, then it should be treated as a
failure and the human notified for review."

This converted a binary decision into the three-way policy captured as D-03,
and made D-05 (Layer 0 extension) load-bearing rather than optional. Verified
against source after the answer: Layer 0 exists but is Code-stage-only and
treats passing probes as non-evidence, so the operator's originating case could
not have succeeded cleanly even with `Unknown` fixed.

### Commits-present Unknown

| Option | Description | Selected |
|--------|-------------|----------|
| Post-condition, else gate | Declared 16a post-condition passes → advance; otherwise gate | ✓ |
| Always gate for human approval | No auto-resolve path at all | |
| Treat as failure | Collapse both Layer-3 branches into one failure path | |

**User's choice:** Post-condition, else gate — matches retrospective AC-3 literally.

### Fix locus

| Option | Description | Selected |
|--------|-------------|----------|
| Split at Layer 3 into typed outcomes | Exhaustive match forces future handling; composes with 17b | ✓ |
| Keep Unknown, branch in `advance()` | Smaller diff, but leaves discrimination as a droppable if-statement | |
| You decide | Planner picks | |

### Stage scope

| Option | Description | Selected |
|--------|-------------|----------|
| Every stage | Define/Plan/Code/Ship; Validate already fail-safe via 13-05 verdict gating | ✓ |
| Code only | Narrowest — targets exactly where observed | |
| You decide | Planner determines per-stage applicability | |

---

## Outcome taxonomy + retry (17b)

### Failure budget

| Option | Description | Selected |
|--------|-------------|----------|
| No — separate budget | Infrastructure outcomes get their own counter and ceiling | ✓ |
| Yes — one shared counter | Any non-success increments | |
| You decide | Planner picks | |

**Notes:** Rationale accepted — spending the abort budget on conditions the
agent never controlled aborts healthy phases, the same false-signal family the
phase exists to fix.

### Retry

| Option | Description | Selected |
|--------|-------------|----------|
| Auto-resume `rate_limited` only | Extends existing `cron-instructions-NN.json`; everything else gates | ✓ |
| Auto-retry `rate_limited` + `resource_killed` | Risks looping on a workload that will always exhaust memory | |
| Gate everything | Maximum signal fidelity; pages the operator for self-resolving rate limits | |

### Evidence record

| Option | Description | Selected |
|--------|-------------|----------|
| Structured evidence record | Layer/outcome/detail as fields; machine-readable for 18d | ✓ |
| Always populate the reason string | Minimal change, but unparseable prose | |
| You decide | Planner designs within schema-v1 | |

### Policy locus

| Option | Description | Selected |
|--------|-------------|----------|
| Hardcoded table, no knobs | Exhaustive match; adding an outcome forces declaring its policy | ✓ |
| `devflow.toml` knobs per D-03 | Operator-tunable, but lets the fail-closed guarantee be configured off | |
| Hybrid — fixed policy, tunable ceilings | Numeric limits from config only | |

---

## Preflight design (17c)

### Check split — resolves retrospective decision-gate Q3

| Option | Description | Selected |
|--------|-------------|----------|
| Generic core + optional adapter hook | `AgentAdapter::preflight()` with empty default, mirroring `extra_env` | ✓ |
| All generic, adapters declare capabilities | One shared checker reasoning over adapter flags | |
| All adapter-specific | Each adapter implements its whole preflight | |

**Notes:** Trait examined before asking — `AgentAdapter` has four methods and
`extra_env` already carries a default impl, so the pattern was established
rather than invented. This is the surface Phase 18's Hermes adapter implements.

### Universal vs adapter (multi-select)

| Option | Description | Selected |
|--------|-------------|----------|
| Plan interactivity vs. execution mode | Mode is a DevFlow concept, not an agent one | ✓ |
| Required security artifact present | Artifact layout is DevFlow/GSD-owned | ✓ |
| External credential validity | Git-flow hardcoded project-wide | ✓ |
| Reviewer receiver set non-empty | Closer to adapter/config territory | (left to adapter hook) |

### Failure semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Named preflight gate + notify | Unattended runs are the design target; a hard exit is invisible to cron | ✓ |
| Hard fail, exit non-zero | Cheaper, nothing to recover; silent when nothing watches stdout | |
| Hard fail at start, gate mid-phase | Split by launch context | |

### Timing

| Option | Description | Selected |
|--------|-------------|----------|
| Before every stage launch, scoped to that stage | Fixes the observed Ship-stage miss; catches mid-phase credential expiry | ✓ |
| At `devflow start` only | One cheap check; can't evaluate Ship requirements hours ahead | |
| At start, plus deeper check before Ship | Two checkpoints, no per-stage overhead | |

---

## Provenance strictness (17d)

### Self-dogfood detection

| Option | Description | Selected |
|--------|-------------|----------|
| Workspace identity match | Target root contains the DevFlow workspace; deterministic, offline | ✓ |
| Git remote match | Breaks on forks, SSH-vs-HTTPS spellings, remote-less clones | |
| Explicit opt-in flag or config key | Predictable, but failure mode is forgetting to set it | |

### Strictness default

| Option | Description | Selected |
|--------|-------------|----------|
| Block self-dogfood, warn elsewhere | Strict where cheap; only affects DevFlow's own repo | ✓ |
| Warn by default, block under strict flag | Never blocks unexpectedly, but warnings go unseen in unattended runs | |
| Warn only, never block | Pure telemetry | |

### Staleness definition

| Option | Description | Selected |
|--------|-------------|----------|
| Commit not an ancestor of HEAD, or source newer than build | Composite; ancestry catches the incident, mtime catches dirty trees | ✓ |
| Embedded commit != HEAD | Fires constantly in normal development | |
| Build timestamp older than newest source | Fragile across checkout/clone/rebase | |

### Build metadata

| Option | Description | Selected |
|--------|-------------|----------|
| Hand-rolled `build.rs` | ~30 lines shelling to git; zero new dependencies | ✓ |
| `vergen` crate | Declarative, but a build dependency and its tree | |
| You decide | Planner picks | |

---

## main.rs extraction

**User's request:** "Give me your honest opinion about how to handle the
main.rs situation, regardless of my don't refactor rule."

| Option | Description | Selected |
|--------|-------------|----------|
| Surgical + pure decision functions where testability demands | 17b's policy table extracted; `advance`/`transition`/`handle_*` stay | ✓ |
| Full extraction in 17 | Accept larger diff and `CliError` redesign now | |
| Split the difference — separate module inside the CLI crate | No error-type change, better organization | |

**Notes:** Recommendation was grounded in direct examination rather than the
`CONCERNS.md` framing, which turned out to be stale. Four findings drove it:
(1) the file is 3,806 lines but tests start at 2643, so production code is
~2,640 and the target cluster is ~540 contiguous lines of small functions;
(2) every function in the cluster returns `Result<(), CliError>` and `CliError`
is a CLI-crate type, making extraction an error-type redesign rather than a
file move; (3) that diff would run through exactly the fail-closed paths
Phase 17 must prove correct, giving any verification failure two candidate
causes; (4) the project's own 13-01 precedent is narrower and testability-driven.

---

## Claude's Discretion

- Typed-outcome variant names and structured-evidence record shape (D-10),
  within the schema-v1 convention.
- Separate-counter ceilings and backoff curve (D-08/D-09).
- Module placement of the D-12 pure policy function.
- Preflight check implementation order and stage-scoped requirement declaration
  (D-16).

## Deferred Ideas

- Full `main.rs` orchestration extraction — own phase after 17 ships, if still
  warranted. Rationale and the `CliError` finding recorded in CONTEXT.md so the
  next attempt starts informed.
- Correcting `CONCERNS.md`'s stale main.rs line count and framing. Note that
  Phase 16's 16c doc-claim checker scans operator-facing docs only and excludes
  `.planning/` by design, so this drift class is unchecked.
- 18d (`devflow doctor` reconciliation) and 18e (WR-03 test fix) — moved to
  Phase 18 before discussion began.

## Process note

STATE.md and ROADMAP.md were edited directly with the Edit tool during the
pre-discussion scoping step, bypassing the `gsd-tools query` handlers required
by universal anti-pattern #15. Content is correct; the safe-update path was
skipped. Flagged to the user at the time.

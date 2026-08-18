# DevFlow

## What This Is

DevFlow is a Rust CLI that automates the mechanical workflow steps an AI
coding agent needs to drive a development phase end-to-end: branch creation,
agent launch, completion detection, gated human checkpoints, versioning,
docs/changelog updates, and cleanup. It runs a 5-stage pipeline
(Define → Plan → Code → Validate → Ship), today against four supported
agents (Claude Code, OpenAI Codex, OpenCode, Pi) through the modular `AgentDriver` contract — opinionated by design,
not a universal agent platform — in either `auto` (unattended) or
`supervise` (gated) mode.

## Core Value

A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never
silently corrupt its own state or lose a human's gate decision, even under a
mid-run crash or kill.

## Current Milestone

*(None declared — v2.7.0 closed 2026-08-18; run `$gsd-new-milestone` to declare the next.)*

<details>
<summary>Previous milestone: v2.7.0 Pi End-to-End + Driver Contract Completion — CLOSED 2026-08-18</summary>

**Delivered.** The `AgentDriver` migration is finished and **Pi** runs end-to-end. Phase 37.1
(research spike) returned a **VIABLE** verdict for `@bacnh85/pi-subagent`; Phase 38 removed
`AgentAdapter` + `DriverShim`, wired `InteractivityMode`, and fixed 999.107; Phase 39 landed the Pi
pipeline (provider-aware health, `Legacy` launch, subagent dispatch with no drain gate).

**Target features:**
- Phase 37.1 — Pi subagent-extension spike (research): verdict **VIABLE** (`@bacnh85/pi-subagent`).
- Phase 38 — driver contract completion (999.106 + 999.107).
- Phase 39 — Pi end-to-end: `devflow start --agent pi` completes the pipeline.
- `999.94` (tentative) — unattended `decision` checkpoint takes the first option blindly (carried).

**Closed 2026-08-18, not yet released** — `Cargo.toml` is still `2.6.0` and no `v2.7.0` tag exists;
release/tag is a separate step. Full phase detail archived to `.planning/milestones/v2.7.0-ROADMAP.md`;
phase dirs to `.planning/milestones/v2.7.0-phases/`.

</details>

<details>
<summary>Previous milestone: v2.6.0 Multi-Agent Adapter Migration — CLOSED 2026-08-16</summary>

**Goal:** Replace the thin `AgentAdapter` trait with a driver architecture that onboards new
agents without agent-specific logic leaking into core — and prove it by onboarding **Pi** as the
first newly-supported agent. The full modular `AgentDriver` refactor (backlog 999.31) is sequenced
*after* the first concrete new driver.

**Target features:**
- Phase 36 — **Pi** support (first new agent driver, alongside Claude/Codex/OpenCode) + three
  small items in the same code this phase touches: 999.67 (agent result parsing lets an agent plant
  its own Layer-0 provenance; XS), 999.96 (`release --check` can't catch a forgotten version bump;
  S), and 999.104 (release-signing key workflow; the SPEC settles the one-line-probe vs. two-key
  question).
- Phase 37 — 999.31 (Modular Agent Driver Architecture: capability discovery, driver-owned prompt
  rendering, command building, completion parsing, health probes, shared conformance suite), with
  Pi from Phase 36 as the second native implementation (999.31 D-02). 999.94 (unattended `decision`
  checkpoints take the first option blindly; HIGH) pencilled here.

**v2.6.0 is a planning label**, next minor after the last shipped v2.5.0 — the actual crate version
is still derived automatically from conventional-commit classification at release time
(`version.rs`), per this project's established versioning policy; it may not land exactly here.

</details>

<details>
<summary>Previous milestone: v2.5.0 Loop-Termination and Release Hardening — CLOSED 2026-08-15</summary>

**Shipped and released** (v2.5.0 tag exists). Closed five confirmed loop-termination / release
defects and one concurrency-blind-spot investigation (999.77, 999.78, 999.79, 999.83, 999.84,
999.86, 999.87). Full phase detail archived to `.planning/milestones/v2.5.0-ROADMAP.md`.

**Delivered.** The Code↔Validate loop's failure-gating mechanics and the release-signing preflight
now behave as documented and are regression-tested; the drain gate's concurrency blind spot was
measured against real production captures (999.83 / Phase 35.3).

</details>

<details>
<summary>Previous milestone: v2.4.0 Resume Unattended Dogfooding — CLOSED 2026-08-06</summary>

**Shipped as a planning milestone; not released.** At close the workspace version was still
`2.3.0`, `CHANGELOG.md` had no 2.4.0 section, the work sat unmerged on `feature/phase-34`, and no
`v2.4.0` tag existed. Closing the milestone archived the planning artifacts; cutting the release is
a separate operation on the project's real release path. Released separately the same day: tagged,
GitHub Release published, `devflow-core` and `devflow` published to crates.io.

**Delivered.** The Code↔Validate loop no longer false-gates on healthy work (999.66), loop-back fix
selection reads the worktree (999.65), Validate's reported outcome reflects derived status rather
than the agent's self-report (999.74), all five stages joined the stream-json launch path on real
captures (999.73), and Layer 0 verification works in worktree mode (999.76).

**Closed as `override_closeout`.** Both phases verified and the pre-close artifact audit was clear,
but DOGFOOD-04's traceability row remains `Pending` and no `/gsd-audit-milestone` was run. See
`.planning/MILESTONES.md` § Known Gaps — chiefly 999.84 / DEN-106, an unguarded call site whose
correctness rests on construction rather than a regression test, now carried into v2.5.0 Phase 35.

<details>
<summary>Original v2.4.0 milestone definition (declared 2026-08-04)</summary>

**Goal:** Close the structural defects blocking unattended, multi-wave `devflow start` runs, so
dogfooding can safely resume. All four items are pre-existing defects found during the Phase 29
dogfood run and Phase 31 planning — none are new regressions from the `gsd-hygiene` milestone.

**Target features:**
- Phase 33 — 999.65 (Validate→Code loop-back issues an impossible `--gaps-only` command on a
  mid-arc phase — confirmed live in the Phase 29 dogfood run, an unresolvable gate) + 999.66
  (`consecutive_failures` never resets across a loop-back, false-gating any 3+ wave phase at
  wave 3 regardless of actual outcome)
- Phase 34 — 999.73 (widen the Phase 30/31 stream-json launch path beyond `Stage::Code` — needs
  real per-stage captures and a re-derivation of the drain-gate reasoning, not just a flag flip)
  + 999.74 (`classify_validate_outcome` trusts the agent's self-reported `verdict` over its own
  derived status, a trust-inversion in the same class as 999.67)

**v2.4.0 is a planning label**, next minor after the last shipped v2.3.0 — the actual crate
version is still derived automatically from conventional-commit classification at release time
(`version.rs`), per this project's established versioning policy; it may not land exactly here.

</details>

</details>

## Requirements

### Validated

- ✓ ROADMAP.md layout restructured so `gsd-tools`' milestone-scoped parsers
  (`roadmap.analyze`, `milestone.complete --dry-run`) correctly find the active milestone's own
  phases instead of misfiring (backlog 999.72), and a `## Progress` table exists so the
  roadmap-derived completion check is used instead of the legacy STATE.md two-scale comparison
  (999.72a) — gsd-hygiene milestone, Phase 32. The fix landed incidentally via
  `gsd-roadmapper`'s own milestone-creation write; Phase 32's own work was confirming it,
  closing the 999.72/999.72a backlog entries, and documenting a durability convention in
  CLAUDE.md for future milestone boundaries.
- ✓ 5-stage GSD-native pipeline (Define→Plan→Code→Validate→Ship), `Mode`
  (auto/supervise) with forced-gate-on-repeated-failure — Phase 11
- ✓ File-based human gate protocol (write/poll/ack, 7-day timeout) — Phase 11
- ✓ Agent-agnostic adapters (Claude Code, Codex, OpenCode) — Phase 11
- ✓ Hybrid git-based SemVer (`version.rs`), hardened against workspace +
  array-of-tables `Cargo.toml` shapes — Phase 11, hardened in Phase 12
- ✓ Crash-safe state persistence (atomic temp+rename `save_state`) — Phase 12
- ✓ Argv-based agent spawn (no shell interpolation of agent-controlled
  data) — Phase 12
- ✓ crates.io publish-readiness (metadata, `--dry-run`, `cargo package`) —
  Phase 12, publish itself intentionally held until Phase 15 (OSS readiness)
- ✓ Reliable terminal finalization, reviewed external post-condition probes,
  retained attempt evidence, deterministic doc/runtime invariants,
  worktree-aware CLI behavior, attempt history, and persistent gates — Phase 16
- ✓ Fail-closed outcome pipeline: typed agent outcomes incl. ResourceKilled/
  AgentUnavailable with exhaustive outcome→action policy, `Unknown`
  non-advance, per-loop infra-failure counter, preflight readiness gate, and
  build provenance + self-dogfood staleness gate — Phase 17 (AC-4 narrowed:
  security-artifact + reviewer-set preflight checks deferred to Phase 18)
- ✓ Dogfood reliability hardening: project-aware `doctor` reconciliation,
  monitor liveness (`State.monitor_pid` + `liveness()` predicate consumed by
  both `status` and `doctor`), Code↔Validate safety-gate reachability
  (`transition_resets_consecutive_failures`), Layer 0/Validate verdict
  reconciliation, worktree-aware build staleness enforcement, preflight-gate
  re-run wedge fix (bounded `preflight_retries`) — Phase 18, v1.5.0
- ✓ Release integrity + `main.rs` decomposition: `.devflow/` artifact hygiene
  (path/username redaction via a single `ensure_devflow_dir()` chokepoint),
  `commit_path` no-longer-allows-empty commits, `main.rs` split 8,467 → 7
  focused modules with zero behavioral change (single shared `ENV_MUTEX`
  preserved across the split), AI change acceptance contract — Phase 19,
  v1.6.0
- ✓ Release correctness + operator control: `VersionBump` rewrites workspace
  member self-pins by construction (20a), `cleanup --force` is fail-closed on
  any live agent/monitor with bounded-backoff retry (20b), `devflow start
  --until <stage>` gives a clean stop point short of Ship (20c), `devflow
  release --check` read-only preflight (self-pin, divergence, publish order,
  signing viability) (20d), `devflow ship --phase N [--force]` manual
  override reusing `finish_workflow` when the monitor is dead (20e) — Phase
  20, v1.7.0
- ✓ End-to-end dogfood blockers: `compute_version` derives from the highest
  *reachable* semver tag plus conventional-commit classification and refuses on
  an unreachable baseline (25c), a major bump opens a human gate and never
  ships unattended, `enforce_build_staleness` is adjudicated once at `start`
  rather than per stage (25b), `ensure_base_ref_current` repairs a stale base
  via a compare-and-swap `git update-ref` behind a repository-wide
  checked-out predicate (25a), `doctor` / `gate sweep --reap-strays` filter the
  structural `/proc` census against a machine-wide registry-reachable pid set
  so a live registered monitor can no longer be reported or SIGKILLed as an
  orphan (25d), the 999.47 cmdline-inheritance CI flake is closed under an
  11-observation streak with human sign-off (25e), and CONTRIBUTING.md's
  release procedure no longer drifts from what 25c implements (25f) — Phase 25,
  shipped v2.1.0
- ✓ The 999.64 arc (a DevFlow-driven phase containing a multi-plan wave
  completing without orphaning delegated work): a Claude `stream-json` (JSONL)
  parser reads Layer-1 verdict, rate-limit classification, `is_error`
  attribution, and `session_id` without regressing the single-document path
  (Phase 30), then a pipe-owning Rust monitor puts that parser on the actual
  production Code-stage launch path — bidirectional `stream-json` argv,
  stdin released only once a `DEVFLOW_RESULT` marker lands in a top-level
  `result` AND the background-task list has drained, a nonce-canary guard
  distinguishing "the behaviour is gone" from "the guard could not run," and
  stream-vs-exit-code arbitration with a `--legacy-claude-launch` opt-out
  (Phase 31). Live acceptance run passed on attempt 1, verified independently
  from git — Phase 31, milestone v2.3.0, shipped v2.3.0. Not yet proven beyond
  the Code stage (999.73, backlog).

- ✓ Loop-back correctness for multi-wave Validate→Code cycles (DOGFOOD-01 +
  DOGFOOD-02, validated in Phase 33): the Validate→Code loop-back distinguishes
  a mid-arc phase (no `{N}-VERIFICATION.md` — issues plain `/gsd-execute-phase
  {N}`) from one with genuine recorded gaps (issues `--gaps-only`), reading that
  signal from the phase's *worktree* rather than the main checkout, since
  `.planning/` is tracked and an in-flight phase's artifacts exist only on its
  own branch; and `consecutive_failures` resets on wave-by-wave forward progress
  measured from a persisted commit-count baseline, so a healthy 3+ wave phase no
  longer false-gates at wave 3 while a genuinely repeated failure still gates.
  Evidence is unit-level (4/4 ROADMAP criteria, 20/20 plan truths). Every test
  drives a tempdir with `PATH` neutralized and builds its "worktree" with
  `create_dir_all` rather than a linked `git worktree` — the end-to-end claim
  ("a 3+ wave unattended `devflow start` phase completes") awaits a real
  dogfood run against this binary.

- ✓ **DOGFOOD-03** — every stream-json stage joined the launch path on real per-stage behavioural
  evidence, with committed PII-scrubbed production captures and per-stage drain analysis, rather
  than on a flag flip — v2.4.0 (Phase 34). The campaign refuted its own premise (zero
  `background_tasks_changed` events across 1063, despite 8 sub-agent dispatches) and that was filed
  as 999.83 rather than absorbed.
- ✓ **DOGFOOD-04 (core claim)** — Validate's reported outcome reflects derived status, not the
  agent's self-reported `verdict` — v2.4.0 (Phase 34), via the status-gated graft and the
  exhaustive classifier, both with live tests and negative controls. **Not fully ticked:** its
  traceability row stayed `Pending` at close because 999.76's second call site has no regression
  guard (999.84 / DEN-106).
- ✓ **HARDEN-01, HARDEN-02, HARDEN-03, HARDEN-04, HARDEN-05, HARDEN-07** — the Code↔Validate loop's
  failure-gating mechanics and the release signing preflight are now enforced by regression tests
  rather than correctness-by-construction: an unmeasurable `git` failure no longer forges a fresh
  baseline or misclassifies a successful agent (999.77/999.87), a never-reset per-phase
  Validate-failure total bounds the loop independent of trivial commits (999.78), a run-scoped
  content fingerprint makes a stale `{N}-VERIFICATION.md` detectable so `--force` no longer inherits
  a stale verdict (999.79), the worktree-mode `GateReview` checkpoint call site is
  revert-and-fail regression-tested (999.84), and `release --check`'s signing result comes from a
  real `ssh-keygen -Y sign` probe, not a predictor that had already false-negatived live twice
  (999.86) — Phase 35, v2.5.0. HARDEN-03's fingerprint keys on artifact bytes rather than run
  identity; that residual gap is filed as its own follow-up, Phase 35.2 (999.89).

### Active

- **HARDEN-06** (999.83) — the drain gate's concurrency guarantee, held back from Phase 35's bundle
  specifically to avoid slowing HARDEN-01..05/07 on a harness — Phase 35.3, planned (not yet executed).
- **999.85 / DEN-107** (two comments justifying themselves by mechanisms v2.4.0 deleted) — low
  severity, not folded into v2.5.0's scope; remains backlog for a future pass.
- Phase 35.1 (999.93, unattended-launch prerequisites) and Phase 35.2 (999.89/HARDEN-03
  provenance) — inserted 2026-08-07, not yet planned.

*(Historical note on how the project reached this point: **The v2.3.0 milestone was CLOSED
2026-08-04**,
bounded from the start (declared 2026-08-02) — it closed when the 999.64 arc
landed (Phase 30 + Phase 31). This is the first milestone in this project
actually archived, hand-corrected — see ROADMAP.md and
`.planning/milestones/v2.3.0-ROADMAP.md` for why `gsd-tools milestone.complete`
itself was bypassed (filed as upstream GSD-core issue ledger entry 16).
Immediately after, the full project history was retroactively archived the
same day: **v2.0.0** (phases 12-25, 27, 28 — the milestone's own existing
table just hadn't been kept current past Phase 20) and a **retroactively
declared v1.0** (phases 1-11, which shipped before this project used the
milestone concept at all). Phase 26 (closed partial, not shipped) moved to
`.planning/superseded/26-release-cut-automation/`; Phase 29 (aborted, no
directory) needed no move. `.planning/phases/` now holds only active backlog
(`999.x`) directories — every numbered phase (1-31) is archived. See
`.planning/MILESTONES.md` for the full index.
Hermes Support, previously slotted as "Phase 18," was rescoped out during
the 2026-07-20 reprioritization to Dogfood Reliability Hardening and now
sits in the backlog as `999.1` — it is NOT automatically next; backlog
items require `/gsd-review-backlog` promotion. **The `gsd-hygiene` milestone
(GSD Workflow Hygiene, Phase 32) declared and closed the same day, 2026-08-04**
— intentionally unversioned, `.planning/`-documentation-only, closing backlog
999.72/999.72a. `milestone.complete` ran cleanly this time (unlike v2.3.0's
close), confirming the fix. See `.planning/milestones/gsd-hygiene-ROADMAP.md`.)*

### Out of Scope

- Bootstrap tooling (`new-project`, `map-codebase`) — deferred to its own
  future phase; no detailed requirements exist yet (Phase 12 CONTEXT.md,
  2026-07-08)

## Context

- Originally built around `tmux` for agent launching; Phase 11's GSD-native
  refactor replaced this entirely with direct process spawning + a monitor
  daemon (`monitor.rs`) that captures stdout/stderr/exit/pid to files and
  invokes `devflow advance` on completion. `tmux` is no longer a runtime
  dependency.
- The CLI surface was substantially cut and rebuilt in Phase 11, then expanded
  through Phase 16. Current operator commands include `start`, `gate`, `logs`,
  `history`, `parallel`, `sequentagent`, `reference`, `cleanup`, `status`,
  `list`, `recover`, `doctor`, and `test`; `advance` remains hidden/internal.
- Workspace version is `2.6.0` (shipped 2026-08-16, tag `v2.6.0`, signed).
  Code/docs historically over-claimed "v2.0.0" as current; Phase 12 corrected
  this. The `v2.0.0` label named an **open-ended** milestone rather than a
  bounded arc — decided 2026-07-23 (ROADMAP.md "Milestone stays open") — and
  on that basis it spanned the 2.0.0, 2.1.0 and 2.2.0 releases, each a minor
  bump because nothing in them was inherently breaking. **That milestone was
  closed 2026-08-02.** The **v2.3.0** milestone, unlike its predecessor, was
  bounded from declaration: it closed 2026-08-04 when the 999.64 arc landed
  (Phase 30 + Phase 31). Phase 31 changed DevFlow's internal launch path and
  agent adapter rather than the CLI surface, so v2.3.0 shipped as a minor
  bump; the `3.0.0` slot stays reserved for a genuinely breaking change,
  whenever that lands. No milestone has been declared yet for whatever comes
  next — see `/gsd-new-milestone`.
- No `.planning/REQUIREMENTS.md` exists in this project; requirements are
  tracked per-phase in each phase's `CONTEXT.md`, not via formal REQ-IDs.

## Constraints

- **Tech stack**: Rust 2024 edition, workspace of `devflow-core` (lib) +
  `devflow-cli` (binary). Dependencies: serde, clap, thiserror, tracing (zero
  network deps).
- **Runtime**: `git` required; no `tmux` dependency since Phase 11.
- **Build**: `cargo build --release` → single static binary (~20MB).
- **Versioning**: git-derived SemVer via `version.rs` — the version derives
  from the highest reachable semver tag (ancestry-checked, semver-ordered,
  never `git describe`) plus the conventional-commit intent of the commits
  added since that baseline was released. The version file (`Cargo.toml`) is
  a derived **output** that `VersionBump` writes, not an input `compute_version`
  reads. A major bump opens a human gate inside preflight and never ships
  unattended. (Phase 25, D-06/D-07/D-09/D-11 — supersedes the June 2026
  commit-message-derivation ban, lifted 2026-07-27.)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Replace tmux-based agent launch with direct process spawn + monitor daemon | tmux launcher had a monitor deadlock bug; direct spawn + file-based capture is simpler and testable | ✓ Good |
| File-based gate protocol instead of a live RPC/socket | Human response can come from any interface (Hermes, manual file drop, future UI) without DevFlow depending on any one of them | ✓ Good |
| Hold `cargo publish` until Phase 15 (OSS readiness) — after MVP loop (13) + observability (14) | Publishing is irreversible — a version can never be reused or unpublished; first public release should be reliability-hardened and documented | — Pending |
| MVP restructure (2026-07-14): Phase 13 → MVP Core Loop, old 13 → 15 | Priority is dogfooding the core loop on real projects again; OSS packaging is worthless until the loop it packages works end-to-end | — Pending |
| Reintroduce a minimal `devflow.toml` | Phase 16 required typed reliability knobs while preserving hardcoded git-flow branch constants; environment variables override project values | ✓ Good |
| Defer bootstrap (`new-project`/`map-codebase`) out of Phase 12 | Genuinely unscoped — no detailed requirements exist yet; inventing them would be speculative | — Pending |
| Hoist `ENV_MUTEX` into one shared mutex during the `main.rs` split (Phase 19) | Three independent `static Mutex<()>` definitions were sound only by accident (each guarded a disjoint variable set); per-module mutexes would have silently broken the serialization 19i's fix depended on | ✓ Good |
| Split `main.rs` as flat sibling modules, not a `commands/` subdirectory (Phase 19) | Mapping Phase 18's plans onto proposed clusters showed pipeline state machine absorbed 3 of 7 plans vs. commands' 2 — a subdirectory buys zero wave reduction | ✓ Good |
| Tighten `cleanup --force`'s liveness guard to fail-closed on ANY live agent pid, not just Healthy/BetweenStages monitor states (Phase 20b, cross-AI review) | `Liveness::Unknown` (no recorded monitor) and `Stuck` (dead monitor) both still mean the agent process could be alive; a monitor-state-only guard left a real deletion-race hole | ✓ Good |
| Reuse `finish_workflow` verbatim for the manual `ship --phase` override rather than reimplementing Ship logic (Phase 20e) | The existing fail-closed terminal-Ship contract (retry-gate-reopen, `workflow_finished` emission) already does exactly what a second out-of-process trigger needs; reimplementing risks drift between the monitor-driven and manual paths | ✓ Good |
| Never honour an operator-set `GIT_DIR` — scrub the repository-local git vars unconditionally at `Command` construction (Phase 27, D-03) | `GIT_DIR` outranks `current_dir()`, so `mutating_project_root` — the guard added expressly to stop `release --execute`/`sync` acting on an unnamed repo — compared two paths, saw a match, and passed while the executor pushed and published against a different repository. Honouring the variable would have preserved that bypass | ✓ Good |
| Scrub at construction, apply `.envs(...)` after (Phase 27, WR-03 fix) | Ordering is load-bearing: it makes the scrub the default while still letting an adapter that *deliberately* sets one of these vars win — which is what keeps Codex's unsigned-commit override working | ✓ Good |
| Acceptance run exercises the Code stage only, not all 5 pipeline stages (Phase 31, D-10) | Code is where 999.64 was observed and the only stage that backgrounds — widening to other stages on zero evidence would extend the adapter to four stages the parser has never actually been exercised against | ✓ Good |
| Pass criterion is "both plans produce a SUMMARY.md AND both merge," not "the stage reported Success" or "both completions observed in the stream" (Phase 31, D-18) | Both rejected substitutes were identified as signals that could pass while the underlying orphaning defect was still present — the completion oracle had already scored the original 999.64 failure as Success | ✓ Good |
| `milestone.complete`'s CLI archival step bypassed for v2.3.0's close; milestone archived by hand instead | The CLI's phase-scoping inherited the same ROADMAP-layout defect as 999.72, but on a write path — it tried to archive all 48 project phases instead of the milestone's 2. Caught pre-commit, reverted, filed upstream (GSD-core issue ledger entry 16) | ✓ Good |
| gsd-hygiene closes unversioned — no `vX.Y.Z` tag, archived under the plain label `gsd-hygiene` instead of `v[X.Y]` (Phase 32 close, 2026-08-04) | The milestone was declared intentionally unversioned (pure `.planning/` docs, no crates code, nothing published); a semver tag would misrepresent it and pollute the tag namespace shared with real crates.io releases | ✓ Good |
| Backfilled `32-01-PLAN.md`/`32-01-SUMMARY.md` after the fact, explicitly labeled as backfilled, to work around `init.manager`'s `planCount > 0` assumption (Phase 32 close, 2026-08-04) | `buildPhaseCompletionProjection` never reads a phase's real VERIFICATION.md when `plan_count` is 0, so a genuinely complete zero-plan phase reports `phase_complete: false` forever; filed upstream as issue 18. Operator chose this over silently overriding the milestone-close readiness gate | ✓ Good |
| `milestone.complete` ran cleanly (no `--dry-run` bypass needed) for gsd-hygiene's close, unlike v2.3.0's | 999.72's fix (this same milestone) is what made the CLI's phase-scoping correct — direct confirmation the fix holds, not just the earlier `--dry-run` checks | ✓ Good |
| Pi is the second native `AgentDriver` (Phase 37, operator decision — supersedes 999.31 D-02's Claude/OpenCode answer) | Priority is preserving Claude (zero regression) and onboarding Pi; Claude/OpenCode stay legacy via the shim until 999.106 removes it | ✓ Good |
| `-a never` (Codex global flag, before `exec`) and `--no-approve` (Pi) are spawn-verified against the installed CLIs (Phase 37) | The review showed `--ask-for-approval never` on `codex exec` is rejected; flag placement/form must be proven by a real spawn, never assumed | ✓ Good |
| `AgentAdapter` removal deferred (D-11 conditional) → 999.106 | Pi runs through the shim, so removal is not required; deferring avoids a risky launch-path refactor in the same phase | ✓ Good (Phase 38 removed it) |
| Pi end-to-end (JSON unwrapper + monitor `CloseRule`) deferred to 37.1/38 | The migration core is the shared prerequisite; the Pi-specific tail is deferred rather than rushed | ✓ Good (Phase 39, re-scoped to Legacy + no CloseRule) |
| Pi health probes `settings.json`'s `defaultProvider`, not "any ready `models.json` provider" (Phase 39) | `build_command` passes no `--provider`, so the run uses the default; probing a catalog false-greens, and refusing on absent `models.json` false-rejects standard installs | ✓ Good |
| Pi pinned to `MonitorLaunch::Legacy`, never `PipeOwning` (Phase 39) | `PipeOwning`'s stdin wire protocol deadlocks Pi (phase-38 review); the regression test asserts the `claude_stream_launch_enabled` precondition | ✓ Good |
| Subagent dispatch via `@bacnh85/pi-subagent` (in-process, synchronous) under `Legacy` (Phase 39) | The 37.1 verdict: a synchronous extension awaits its children, so process-exit supervision suffices — no `CloseRule`/drain gate | ✓ Good |
| Capability detection matches the vetted `@bacnh85/pi-subagent` name, not `*subagent*` (Phase 39) | Unsafe/deferred packages (`@mystilleef` etc.) must not report "available" | ✓ Good |

## Key Files

| File | Purpose |
|---|---|
| `.planning/ROADMAP.md` | Phase plan source of truth (current — not the stale pre-GSD `ROADMAP.md` at repo root, which predates the GSD reorg) |
| `.planning/codebase/` | Codebase map (7 documents, 2026-06-17 — predates Phases 1-12; consider `/gsd-map-codebase` before Phase 13) |
| `.planning/CONCERNS.md` | Top findings from the original pre-Phase-1 codebase audit |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-18 after the v2.7.0 milestone (Pi End-to-End + Driver Contract Completion) — 3/3 phases (37.1 research, 38, 39), 2/2 plans, verified + validated. NOT yet released: `Cargo.toml` still `2.6.0`, no `v2.7.0` tag — release/tag is a separate step. Milestone archived to `.planning/milestones/v2.7.0-*`; phase dirs to `v2.7.0-phases/`.*

*Previous: 2026-08-16 after the v2.6.0 milestone (Multi-Agent Adapter Migration) — 2/2 phases (36, 37), 6/6 plans, verified + validated + secured (threats_open: 0). Released as `v2.6.0`: signed tag on `main`, both crates published (devflow-core → devflow), main→develop synced (merge commit), GitHub Release published, milestone archived to `.planning/milestones/v2.6.0-*`; phase dirs archived to `v2.6.0-phases/`.*

*Previous: 2026-07-28 after Phase 25 (End-to-End Dogfood Blockers)
shipped as v2.1.0 — 18/19 plans (25-10 superseded by 25-13), verified 10/10
across five gap-closure rounds, 129/129 threats closed, broken-windows ledger at
0 open / 1 waived / 4 fixed. PR #47 → develop, #50 squash-merged to main,
signed tag `v2.1.0` (maintainer key, fingerprint verified), main→develop sync
merge-committed (#51), GitHub Release published, and both crates published to
crates.io in order (devflow-core → devflow). The v2.0.0 milestone stayed open at
this point (no fixed closing phase); it was closed 2026-08-02*

*Phase 27 (Scrub Redirecting Git Environment From Production Calls) completed and
verified 2026-07-30 — 6/6 plans, 7/7 must-haves, all 41 production
`Command::new("git")` sites routed through `devflow_core::git::{hermetic_command,
git_command}`, Sweep A at 0, both hostile-`GIT_DIR` acceptance commands green at
HEAD (411/0 core, 188/0 cli). This unblocks 999.25 (release executor) and 999.52
(`devflow sync`), which named it prerequisite #1. Not yet shipped/merged.*

---
*Last updated: 2026-08-04. The v2.3.0 milestone ("the unattended run", Phases
30-31) closed, then the full prior project history was retroactively archived
in the same session: v2.0.0 (phases 12-25, 27, 28) and a retroactively
declared v1.0 (phases 1-11). Phase 26 (not shipped) and Phase 29 (aborted)
were excluded from both archives — see their own dispositions in ROADMAP.md
and `.planning/superseded/`. Individual phase accomplishments are not
reproduced above; see `.planning/MILESTONES.md` for the authoritative
phase-by-phase and milestone-by-milestone record.*

---
*Last updated: 2026-08-04 after the gsd-hygiene milestone (Phase 32, ROADMAP Layout Hygiene)
closed the same day it was declared — intentionally unversioned, no crates code. Closed backlog
999.72/999.72a; `.planning/REQUIREMENTS.md` deleted (fresh for next milestone); no active
milestone declared yet — see `/gsd-new-milestone`.*

---
*Last updated: 2026-08-04, same day, starting milestone v2.4.0 Resume Unattended Dogfooding —
Phase 33 (999.65 + 999.66) and Phase 34 (999.73 + 999.74), scoped through conversation rather
than fresh domain research (these are pre-existing internal defects with fix directions already
sketched in ROADMAP.md, not new user-facing capability).*

---
*Last updated: 2026-08-06 after milestone **v2.4.0 Resume Unattended Dogfooding** closed —
2 phases (33, 34), 12 plans, 25 tasks. Closed as `override_closeout`: both phases verified and the
pre-close artifact audit clear, but DOGFOOD-04's traceability row remained `Pending` and no
`/gsd-audit-milestone` was run. The milestone is a planning close, not a release — workspace version
still 2.3.0, no 2.4.0 changelog section, work unmerged on `feature/phase-34`, no `v2.4.0` tag.
Phase 34's UAT closed on operator attestation rather than demonstration, leaving 999.84 / DEN-106 as
the standing gap.

---
*Last updated: 2026-08-06, same day, v2.4.0 released (signed tag, GitHub Release, `devflow-core`
and `devflow` published to crates.io — separate operation from the planning close above) and
milestone **v2.5.0 Loop-Termination and Release Hardening** started — Phase 35 (999.77 + 999.78 +
999.79 + 999.84 + 999.86) and Phase 35.3 (999.83), scoped through conversation rather than fresh
domain research (five confirmed pre-existing defects with fix directions already decided, plus one
investigation-shaped item, none new user-facing capability). Scope agreed with the operator before
this workflow ran; confirmed rather than re-derived at the milestone-summary checkpoint.*

---
*Last updated: 2026-08-07 after Phase 35 (Loop-Termination and Baseline Correctness) completed and
verified — 6/6 plans, 6/6 ROADMAP success criteria, `cargo test --workspace` clean (576 + 303
passed, 0 failed). Code review's 1 Critical (CR-01) and 7 Warnings (WR-01..07) all independently
re-confirmed resolved against current source, not just commit presence. UAT 17/18 passed, 1 issue
resolved. HARDEN-01, 02, 03, 04, 05, 07 now Complete in REQUIREMENTS.md traceability. Scope grew
mid-phase: Phase 35.1 (999.93, unattended-launch prerequisites) and Phase 35.2 (999.89/HARDEN-03
provenance, a residual gap in the fingerprint-keys-on-bytes-not-run-identity limitation) were
inserted into the roadmap 2026-08-07, not yet planned or executed.*

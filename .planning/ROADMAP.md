# DevFlow Roadmap

> Phase plan source of truth. Each phase drives a `devflow start` agent session.

## v2.0.0 (Phase 11–18)

| Phase | Name | Status |
|---|---|---|
| 12 | Bootstrap + Housekeeping | Complete |
| 13 | MVP Core Loop | Complete    |
| 14 | Parallel Safety + Observability | Complete |
| 15 | Dogfood Enablement + OSS Readiness | Complete |
| 16 | Pipeline Reliability Hardening | Complete    |
| 17 | Pipeline Dogfood Follow-Up | Complete    |
| 18 | Hermes Support | Scoped |

## Shipped

| Phase | Name | Version |
|---|---|---|
| 11 | GSD-Native Architecture + Remediation | 1.2.0 |
| 10 | Logging + Planning Step | — |
| 9 | Open-Source Polish | 1.2.0 |
| 8 | Docs & Onboarding | 1.0.1 |
| 7 | Worktrees & PR Integration | 1.0.0 |
| 6 | Agent Completion Protocol | 1.0.0 |
| 1–5 | Core workflow, versioning, state machine | 0.1.0–0.6.0 |

## Reorganized (June 2026)

- **Conventional commits deprecated** — no commit-message-based versioning
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

## Phase 14 split (2026-07-16)

- **Phase 14 rescoped to Parallel Safety + Observability** — the 2026-07-14 move of Hermes into Phase 14 was a workload-balance call made before the CR-03 parallel-safety flaw was deferred there (2026-07-15), which made 14 the heaviest phase instead of the slimmest. Phase 14 now leads with CR-03 (per-phase state files, phase-threaded monitor advance, coarse lock for main-checkout mutations), keeps the `capture_agent_output()` sync-path decision, and builds observability (`logs`/`events.jsonl`/`status`) on the final per-phase state model — in that order, since the state-file shape dictates what `status`/`logs`/`events.jsonl` enumerate.
- **Phase 16 (new): Hermes Support** — HermesAgent adapter, skill-file rewrite, and Hermes plugin moved out of 14. Depends on Phase 14 (the plugin's gate watcher consumes `events.jsonl` and the Phase 13 notify hook); sits after Phase 15 so public-facing OSS readiness isn't gated on personal-infrastructure work.

### Phase 12: Bootstrap + Housekeeping

**Goal:** Pay down the Phase 11 code-review debt (WR-01…WR-10, IN-02…IN-05), close the untested orchestration-core paths and never-run manual verifications, harden versioning (WR-04 + version-consistency to 1.2.0), and get the crates publish-ready (metadata + dry-run, NO publish). Bootstrap (12a new-project/map-codebase) is DEFERRED to its own future phase — see CONTEXT.md "Planning-Time Decisions".
**Requirements**: WR-01, WR-02, WR-03, WR-04, WR-05, WR-06, WR-07, WR-08, WR-09, WR-10, IN-02, IN-03, IN-04, IN-05, 12b, 12c, 12f, 12g (see CONTEXT.md — no formal REQ-IDs)
**Depends on:** Phase 11
**Plans:** 12/12 plans complete

Plans:
**Wave 1**

- [x] 12-01-PLAN.md — WR-07: atomic `save_state` (temp+rename) so a kill mid-write can't corrupt state.json
- [x] 12-02-PLAN.md — WR-06 runaway-cron guard + IN-04 `cargo fmt --check`
- [x] 12-03-PLAN.md — WR-01: monitor spawns the agent as argv (no shell interpolation)
- [x] 12-04-PLAN.md — WR-02/WR-03 + 12f Validate→Ship hook-firing test
- [x] 12-05-PLAN.md — WR-04 TOML parser robustness + 12f workspace write_version + IN-05 version→1.2.0
- [x] 12-06-PLAN.md — 12c publish-prep: crates.io metadata + dry-run/package (NO publish)
- [x] 12-07-PLAN.md — WR-10 config-decoy test cleanup + WR-09 marker-scan doc/guard
- [x] 12-08-PLAN.md — 12f: gate-timeout fast path + branch ahead/behind + monitor advance-failure

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 12-09-PLAN.md — 12f: advance()/finish + consecutive-failures→gate→abort (main.rs)
- [x] 12-10-PLAN.md — WR-05/WR-08 + 12f parse_rfc3339ish negative-offset (ship.rs)
- [x] 12-12-PLAN.md — 12g manual verifications (Hermes gate, real agent, DocsUpdate; Full-Ship blocked)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 12-11-PLAN.md — IN-02 dead-field removal + IN-03 Agent→AgentKind / trait→AgentAdapter rename

### Phase 13: MVP Core Loop

**Goal:** Get the basic AI development loop (Define→Plan→Code→Validate→Ship) working end-to-end so DevFlow is usable on real projects again — `ship.rs` GSD-native rewrite (13a), completion-protocol correctness: verdict-vs-ran + native Claude/Codex envelope parsing (13b), never-silent failures: WR-11 + gate notify hook + configurable timeout (13c), worktree-by-default (13d), and a real dogfood run as the acceptance test (13e).
**Requirements**: 13a–13e, WR-11 (see CONTEXT.md)
**Depends on:** Phase 12
**Plans:** 6/6 plans complete

Plans:

**Wave 1** *(front-loaded: riskiest failure-handling + parsing)*

- [x] 13-01-PLAN.md — 13a/13c/WR-11: never-silent failure handling — Ship failure branch, handle_stage_failure, notify hook, env gate timeout (main.rs, gates.rs)
- [x] 13-02-PLAN.md — 13a: delete dead v1 ship.rs bookkeeping + headless-safe Ship prompt (code-review before ship) (ship.rs, prompt.rs)
- [x] 13-03-PLAN.md — 13b: native envelope parsing — Claude is_error/num_turns, Codex JSONL, stage-scoped Layer 2 (agent_result.rs)

**Wave 2** *(blocked on Wave 1: shares main.rs)*

- [x] 13-04-PLAN.md — 13d: worktree-by-default with `--no-worktree` opt-out (main.rs, phase7_cli.rs)

**Wave 3** *(blocked on Waves 1–2: shares agent_result.rs/prompt.rs/main.rs)*

- [x] 13-05-PLAN.md — 13b: verdict-vs-ran split — Verdict enum, Validate prompt verdict, advance() verdict gating (agent_result.rs, prompt.rs, main.rs)

**Wave 4** *(final: manual acceptance, blocked on all)*

- [x] 13-06-PLAN.md — 13e: MVP acceptance dogfood run — Claude full-loop + Full-Ship re-verification + Codex leg (manual checkpoints)

### Phase 14: Parallel Safety + Observability

**Goal:** Make concurrent phases safe by construction, then surface loop progress instead of a black box. Leads with the deferred CR-03 design flaw from Phase 13's post-fix review: per-phase locks sit on a project-global `state.json` and unguarded main-checkout git ops, so `devflow parallel` is unsafe by construction — fix shape and acceptance criteria in `phases/13-mvp-core-loop/13-DEFERRED-CR-03.md` (per-phase state files, phase-threaded monitor advance, short coarse lock for main-checkout mutations) (14a). Then the `capture_agent_output()` sync-path decision, taken alongside CR-03's sequentagent re-check (14b), and observability — `devflow logs [--follow]`, append-only phase-aware `events.jsonl`, richer `devflow status` — built on the per-phase state model (14c). Hermes work moved out to Phase 16 (2026-07-16).
**Requirements**: 13-DEFERRED-CR-03 (parallel-safety), 14a–14c (see CONTEXT.md)
**Depends on:** Phase 13
**Plans:** 4/4 plans complete

Plans:

- [x] 14-01-PLAN.md — 14a core: per-phase state files + phase-threaded `advance --phase N` (workflow.rs, monitor.rs, main.rs)
- [x] 14-02-PLAN.md — 14a/14b: coarse checkout lock + sequentagent behind the monitor, sync capture path deleted (lock.rs, monitor.rs, agent.rs, main.rs)
- [x] 14-03-PLAN.md — 14a closeout: multi-phase status/recover + concurrent-advance acceptance test
- [x] 14-04-PLAN.md — 14c: events.jsonl (schema v1) + `devflow logs [--follow]` + richer per-phase status

See `14-SUMMARY.md` for validation + live two-phase e2e acceptance evidence.

### Phase 15: Dogfood Enablement + OSS Readiness

**Goal:** Rescoped 2026-07-16 (dogfood-first — operator priority is a fully functional MVP for dogfooding). **15a Dogfood Enablement:** `devflow gate` subcommand (list/approve/reject — removes the last hand-edited-JSON step in the loop), an accurate `OPERATIONS.md` operator reference, and the doc-accuracy quick hits (`.devflow.yaml` decoy removal, IN-01 lib.rs rustdoc, `--help` snapshot test); exit criterion: a real phase runs end-to-end with gates answered only via `devflow gate` + the notify hook. **15b OSS Packaging** (run *through* DevFlow as the first post-MVP dogfood): README/ARCHITECTURE rewrite against v2 reality, CONTRIBUTING, dev container, crates.io publish. Antigravity adapter (old 15c) deferred out of the phase to unscheduled backlog.
**Requirements**: 15a, 15b (see CONTEXT.md)
**Depends on:** Phase 14
**Plans:** 5/5 plans executed

Plans:
**Wave 1**

- [x] 15a — dogfood enablement (gate subcommand, OPERATIONS.md, accuracy fixes) — complete 2026-07-16; exit criterion verified live (full phase with the gate answered only via `devflow gate approve`)
- [x] 15-01-PLAN.md (wave 1) — README/SECURITY/DEPENDENCIES accuracy pass against the real v2 CLI surface
- [x] 15-02-PLAN.md (wave 1) — ARCHITECTURE.md full rewrite against source + docs/guides accuracy
- [x] 15-03-PLAN.md (wave 1) — CONTRIBUTING refresh (required-checks note) + greenfield .devcontainer + container-parity CI job
- [x] 15-04-PLAN.md (wave 1) — dual-license fix (add LICENSE-APACHE) + publish dry-run verification

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 15-05-PLAN.md (wave 2, non-autonomous) — publish devflow-core then devflow to crates.io (operator-held token gate) — complete 2026-07-17; both crates confirmed live on crates.io

### Phase 16: Pipeline Reliability Hardening

**Goal:** Inserted 2026-07-17, pushing the prior Phase 16 (Hermes Support) to 17. Surfaced entirely by dogfooding Phase 15 through DevFlow itself: two Code-stage false positives on the crates.io publish plan (no repo-diff success signal, once via Layer-2 commit-count heuristic and once via an incorrect agent self-report), and four consecutive Ship-time code-review failures on legitimate but distinct findings (leaked runtime telemetry, an incomplete follow-up fix, a CI job that couldn't fail loud, a doc/behavior mismatch) that a single-pass standard-depth reviewer surfaced one at a time instead of together. Scope: (a) external post-condition verification for plans with no repo-diff success signal, (b) retained per-stage capture history instead of clobbering `.devflow/phase-NN-stdout` on every stage launch, (c) a deterministic doc-claim-vs-source checker, (d) deep-mode + multi-angle parallel review for Ship's gating pass instead of one single-pass standard-depth reviewer, (e) incremental per-plan/per-wave review instead of only at phase end, (f) worktree-aware `devflow status` (currently reports `idle` when run from inside the worktree it created), (g) legacy-state WARN cleanup/hint, (h) cross-attempt Ship/Code history view.
**Requirements**: 16a, 16b, 16c, 16d, 16e, 16f, 16g, 16h, 16i, 16j, 16k (scope items — no formal REQ-IDs; binding decisions D-01…D-09 in 16-CONTEXT.md)
**UI hint**: no
**Depends on:** Phase 15 (surfaced entirely by dogfooding it)
**Plans:** 7/7 plans complete

Plans:

**Wave 1** *(16k first per D-09; config foundation in parallel)*

- [x] 16-01-PLAN.md — 16k: wire the missing Merge hook first into the terminal Ship path (idempotent), fix wrong-checkout VersionBump ordering, truthful merge_result event, clean bogus CHANGELOG entries
- [x] 16-02-PLAN.md — D-03: minimal devflow.toml config foundation (toml dep behind a blocking legitimacy checkpoint) + DevflowConfig with all Phase 16 knobs + env>file>default loader

**Wave 2** *(blocked on Wave 1)*

- [x] 16-03-PLAN.md — 16a/16b: Layer-0 external post-condition verification (verify.rs) + retained per-stage capture history (archive instead of wipe)
- [x] 16-04-PLAN.md — 16d/16e: deep multi-angle capability-conditional Ship review + advisory incremental self-review (prompt.rs)
- [x] 16-05-PLAN.md — 16c/16i: deterministic doc-claim checker (existence + pinned claims + allowlist) and source-derived .gitignore invariant (doc_check.rs, all #[test])

**Wave 3** *(blocked on Wave 2: shares main.rs)*

- [x] 16-06-PLAN.md — 16f/16g: shared project-root walk-up resolver + gate positional-arg footgun fix + legacy-state WARN recover hint

**Wave 4** *(blocked on Wave 3: shares main.rs; correlates 16b history)*

- [x] 16-07-PLAN.md — 16j/16h: persistent escalating pending-gate status banner + cross-attempt Ship/Code history view (history.rs)

### Phase 17: Pipeline Dogfood Follow-Up

**Goal:** Close the pipeline-reliability holes the Phase 16 dogfood exposed —
`Unknown` completion must never auto-advance a stage (17a), typed agent
outcomes with a deterministic retry policy (17b), a preflight readiness gate
that fails before agent time is consumed (17c), and build provenance in
`workflow_started` so a stale self-dogfood binary is detectable (17d). The
terminal-Ship alarm was traced to a stale executable, not a live regression;
state/event reconciliation and the WR-03 test fix were deferred to Phase 18 on
2026-07-18.
**Requirements:** P1–P4 in `17-DOGFOOD-RETROSPECTIVE.md`; acceptance criteria
2, 3, 4 (criterion 1 is already covered by Phase 16's regression test — verify
against final HEAD rather than re-plan). AC-4 is narrowed to the
plan-interactivity and Ship-scoped `gh auth` checks only — the
security-artifact and reviewer-set sub-checks are deferred to Phase 18's
Hermes adapter, an accepted override recorded in
`17-VERIFICATION.md`'s frontmatter (`overrides:`).
**Depends on:** Phase 16
**Blocks:** Phase 18 Hermes Support
**Plans:** 11/11 plans executed

Plans:

- [x] 17-10-PLAN.md
- [x] 17-11-PLAN.md

- [x] 17-09-PLAN.md

- [x] 17-07-PLAN.md
- [x] 17-08-PLAN.md

- [x] 17-06-PLAN.md

**Wave 1** *(devflow-core foundations + build script, no shared files)*

- [x] 17-01-PLAN.md — 17b: typed outcome taxonomy (ResourceKilled/AgentUnavailable), Layer 2 exit-code classification, pure exhaustive outcome→action policy module, separate infra-failure counter
- [x] 17-02-PLAN.md — 17d: first workspace build.rs embedding git provenance (commit/dirty/timestamp) with graceful no-git degradation

**Wave 2** *(blocked on 17-01: shares agent_result.rs)*

- [x] 17-03-PLAN.md — 17a: Layer 0 runs every stage + vouches for a passing approved probe (D-05); Layer 3 zero-commit/no-declaration → fail-closed (D-02/D-03)

**Wave 3** *(blocked on 17-01/17-03: rewrites advance() dispatch)*

- [x] 17-04-PLAN.md — 17a/17b: exhaustive decide_action dispatch (Unknown never advances), primary-loop rate-limit auto-resume, infra-counter gating, structured advance_evaluated evidence

**Wave 4** *(blocked on 17-02/17-04: shares main.rs)*

- [x] 17-05-PLAN.md — 17c/17d: scoped preflight readiness gate (adapter hook + generic checks) and workflow_started build provenance + self-dogfood staleness block

### Phase 18: Hermes Support

**Goal:** First-class Hermes support — `HermesAgent` adapter with native-envelope completion parsing (18a), rewrite of the stale `skills/hermes/devflow/SKILL.md` against current CLI behavior (18b), and the Hermes plugin session mode with an events.jsonl-driven gate watcher (18c). Split out of Phase 14 on 2026-07-16 so personal-infrastructure work doesn't gate parallel-safety correctness or OSS readiness. Renumbered from 17 to 18 on 2026-07-18 to make room for Phase 17's dogfood follow-up.

Also carries two items deferred from Phase 17 on 2026-07-18 (18d, 18e). These are pipeline-reliability work, not Hermes work — they landed here to keep Phase 17 focused on invariant correctness:

- **18d — project-aware `devflow doctor` reconciliation:** `doctor()` currently takes `_project_root` unused (`main.rs:2454`) and only checks external tools/PATH. Extend it to diff state vs. events vs. live PIDs vs. gates vs. branch ancestry and report a repair plan, mutating nothing by default. Consumes Phase 17's typed outcomes (17b) and provenance (17d) — sequence after them. Retrospective acceptance criterion 5.
- **18e — WR-03 test stabilization:** `parallel_creates_two_worktrees_and_spawns_two_monitors` (`crates/devflow-cli/tests/phase7_cli.rs:184-200`) waits for live stdout captures, runs unrelated assertions, then re-asserts the same paths — a fast monitor can archive between the two. Fix by asserting the capture immediately or accepting its retained-history generation. Recorded as non-blocking debt in `16-REVIEW.md`. (Distinct from `13-REVIEW.md`'s WR-03 in `git.rs:286` — the review numbering namespaces collide.)

**Requirements**: TBD (see CONTEXT.md) + 18d, 18e (deferred Phase 17 P5/P6)
**Depends on:** Phase 14 (consumes `events.jsonl` + the Phase 13 notify hook); Phase 17 for 18d
**Plans:** 0 plans

Plans:

- [ ] TBD (run /gsd-plan-phase 18 to break down)

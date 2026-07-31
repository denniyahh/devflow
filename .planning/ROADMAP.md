# DevFlow Roadmap

> Phase plan source of truth. Each phase drives a `devflow start` agent session.

## v2.0.0 milestone (open — no fixed closing phase)

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

## Shipped

| Phase | Name | Version |
|---|---|---|
| 21 | Operator Legibility & Observability | 1.8.0 |
| 20 | Release Correctness + Operator Control | 1.7.0 |
| 18 | Dogfood Reliability Hardening | 1.5.0 |
| 11 | GSD-Native Architecture + Remediation | 1.2.0 |
| 10 | Logging + Planning Step | — |
| 9 | Open-Source Polish | 1.2.0 |
| 8 | Docs & Onboarding | 1.0.1 |
| 7 | Worktrees & PR Integration | 1.0.0 |
| 6 | Agent Completion Protocol | 1.0.0 |
| 1–5 | Core workflow, versioning, state machine | 0.1.0–0.6.0 |

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

## Milestone stays open (2026-07-23)

- **Decided at Phase 20 ship time:** ships as **v1.7.0**, not v2.0.0 — nothing across the five units is breaking, consistent with Phase 19's earlier call not to spend the 2.0 slot on a non-breaking changeset.
- **The v2.0.0 milestone does NOT close at Phase 20 or at any other fixed phase.** Earlier notes above ("the milestone now runs Phase 11–20 and genuinely closes at v2.0.0," "the v2.0.0 milestone closes at Phase 20," "the milestone reserves 2.0.0 for this phase") described a *bounded* Phase 11–20 arc culminating in a 2.0.0 release. That framing is superseded: the milestone continues past Phase 20 with no predetermined phase count or closing version — 2.0.0 remains an eventual aspiration, not a scheduled endpoint. Future phases keep numbering forward (21, 22, …) under the same open milestone until a genuinely breaking change actually earns the 2.0 slot; `/gsd-complete-milestone` is not run at Phase 20.
- Table above renamed from "v2.0.0 (Phase 11–20)" to reflect this — the phase list is historical (what's shipped so far), not a closing boundary.

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
**Plans:** 13/13 plans executed

Plans:

- [x] 17-13-PLAN.md

- [x] 17-12-PLAN.md

- [x] 17-10-PLAN.md
- [x] 17-11-PLAN.md

- [x] 17-09-PLAN.md

- [x] 17-07-PLAN.md
- [x] 17-08-PLAN.md

- [x] 17-06-PLAN.md

**Wave 1** *(devflow-core foundations + build script, no shared files)*

- [x] 17-01-PLAN.md — 17b: typed outcome taxonomy (ResourceKilled/AgentUnavailable), Layer 2 exit-code classification, pure exhaustive outcome→action policy module, separate infra-failure counter
- [x] 17-02-PLAN.md — 17d: first workspace build.rs embedding git provenance (commit/dirty) with graceful no-git degradation *(the build timestamp originally planned here was removed by 17-11 closing CR-02: a per-second value forced a devflow-cli recompile on every build once build.rs always re-runs)*

**Wave 2** *(blocked on 17-01: shares agent_result.rs)*

- [x] 17-03-PLAN.md — 17a: Layer 0 runs every stage + vouches for a passing approved probe (D-05); Layer 3 zero-commit/no-declaration → fail-closed (D-02/D-03)

**Wave 3** *(blocked on 17-01/17-03: rewrites advance() dispatch)*

- [x] 17-04-PLAN.md — 17a/17b: exhaustive decide_action dispatch (Unknown never advances), primary-loop rate-limit auto-resume, infra-counter gating, structured advance_evaluated evidence

**Wave 4** *(blocked on 17-02/17-04: shares main.rs)*

- [x] 17-05-PLAN.md — 17c/17d: scoped preflight readiness gate (adapter hook + generic checks) and workflow_started build provenance + self-dogfood staleness block

### Phase 18: Dogfood Reliability Hardening

**Goal:** Make DevFlow's own supervision layer trustworthy and usable from a plain terminal. Reprioritized 2026-07-20 (operator decision) — dogfooding has repeatedly found legitimate functional bugs that tax every subsequent dogfood run, so this pipeline-reliability work takes Phase 18's slot ahead of Hermes (personal-infrastructure, moved to `## Backlog`). Replaces the fixed "Phase 19" roadmap entry entirely: every item it carried is either absorbed here (18a–18g), confirmed already fixed (19e/19f, 19i), or moved to `## Backlog` (19b, 19c, 19h, 19j). Full detail, evidence, and both recorded operator decisions live in `phases/18-dogfood-reliability-hardening/CONTEXT.md`; reproduction evidence in `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` and `17-REVIEW.md`.

- **18a** — `devflow doctor` project-aware reconciliation *(was 18d)*
- **18b** — monitor liveness observability *(was 19a; extends 18a — sequence after it)*
- **18c** — staleness evaluated against the wrong tree; enforces the standing rebuild-before-revalidate dogfood rule *(was 19d; root cause of Round 4 CR-01)*
- **18d** — Code↔Validate `consecutive_failures` reset makes `MAX_CONSECUTIVE_FAILURES` unreachable *(was 19g)*
- **18e** — Layer 0 short-circuit makes Validate unpassable when `external_verify` is declared *(was 19k; operator decision recorded 2026-07-20)*
- **18f** — approving a preflight gate re-runs the identical check and wedges for 7 days *(was 19l; operator decision recorded 2026-07-20)*
- **18g** — WR-03 test stabilization, `parallel_creates_two_worktrees_and_spawns_two_monitors` *(was 18e)*

**Requirements**: 18a–18g (see CONTEXT.md)
**Depends on:** Phase 17 (typed outcomes, build provenance)
**Plans:** 7/7 plans executed

Plans:

- [x] 18-01-PLAN.md — 18a: `devflow doctor` project-aware reconciliation (wave 1)
- [x] 18-02-PLAN.md — 18g: WR-03 test stabilization, assertion placement (wave 1)
- [x] 18-03-PLAN.md — 18b: persist and probe `monitor_pid`, representable "stuck" state (wave 2)
- [x] 18-04-PLAN.md — 18d: make `MAX_CONSECUTIVE_FAILURES` reachable for the Code↔Validate loop (wave 3)
- [x] 18-05-PLAN.md — 18e: Layer 0/Validate verdict reconciliation + three-way outcome (wave 4)
- [x] 18-06-PLAN.md — 18c: evaluate build staleness against the worktree HEAD (wave 5)
- [x] 18-07-PLAN.md — 18f: preflight gate approval skips the adjudicated check, bounded (wave 6)

**Verified** 2026-07-21 (`18-VERIFICATION.md`, 7/7 must-haves, each traced to source + an independently-executed passing test; both binding operator decisions confirmed). **Code-reviewed** (`18-REVIEW.md`, 0 critical / 4 warning) and **review-fixed** in a `18-fix` batch (6 commits): `doctor --json` single-object output (WR-01), stale-`monitor_pid` false-"Stuck" (WR-04), path-free staleness event (WR-02, third instance — see `999.10`), the `unreachable!()` eliminated by construction (WR-03), and the new 18c worktree test hardened against the 19i PATH-race flake. Final: 426 tests, clippy `--workspace --all-targets` clean, fmt clean. **Merged to main and released as v1.5.0** (2026-07-21, PR #12, signed tag `v1.5.0`, published to crates.io).

### Phase 19: Release Integrity + `main.rs` Decomposition

**Goal:** Close the two release-integrity defects whose blast radius reaches outside this repository (999.10's `.devflow/` PII leak into *users'* git history, 999.11's empty commit under a release tag), then decompose the 8,467-line `crates/devflow-cli/src/main.rs` as a pure-move refactor so later phases stop paying the near-serial wave tax. Adds the AI change acceptance contract (999.16) on a parallel, source-conflict-free track.
**Targets:** v1.6.0 — nothing here is breaking and, apart from the PII fix, almost nothing is user-visible. Phase 20 carries the operator-facing set this split makes plannable as one phase.
**Promoted from backlog** 2026-07-21: 999.10 (DEN-35), 999.11 (DEN-36), 999.8 (DEN-33), 999.16 (DEN-41).
**Requirements:** 19a, 19b, 19c–19f, 19g (see CONTEXT.md — no formal REQ-IDs)
**Depends on:** Phase 18 — 999.8 was deliberately blocked on it; 18a/18b are the instrumentation that makes an `ENV_MUTEX` regression observable, and 18e/18f reshaped the functions that determine the module seams.
**Plans:** 11/11 plans executed

**Sequencing is load-bearing:** 19a and 19b land *before* the split, so they are small diffs against the file everyone knows rather than against seven new modules. 19g has no source overlap and can run in any wave.

**Principal risk — `ENV_MUTEX`:** 18 `.lock()` sites / 63 references in `main.rs`, and a repeat root cause across three expensive-to-diagnose failures (19i, GAP-2, 999.4). If its serialization guarantees cannot survive distribution across module boundaries, that is a finding to surface, not to patch around. Verification must be CI-on-branch; local-green is explicitly insufficient.

Plans:

**Wave 1** *(19a/19b/19g — all pre-split, zero file overlap)*

- [x] 19-01-PLAN.md — 19a-WR01: new `workflow::ensure_devflow_dir` writing a self-ignoring `.devflow/.gitignore`, all 7 constructors converted, coverage + scratch-repo tests
- [x] 19-02-PLAN.md — 19a-WR02: redact `exe_path` in `events.jsonl` to the binary filename only
- [x] 19-03-PLAN.md — 19b: `commit_path` no longer forces an empty commit (RED-first); D-17 `commit_all` finding recorded
- [x] 19-04-PLAN.md — 19g: `.claude/skills/ai-change-acceptance/` + `CONTRIBUTING.md` prose

**Wave 2** *(blocked on wave 1)*

- [x] 19-05-PLAN.md — 19g dogfood checkpoint: run `/gsd-code-review` against a non-compliant diff and a compliant control
- [x] 19-06-PLAN.md — split foundation: committed pre-split baseline, `pub(crate)` pass on cross-cluster types, `ENV_MUTEX` hoist into `test_support.rs`

**Wave 3** *(blocked on 19-06)*

- [x] 19-07-PLAN.md — extract `staleness.rs` + `preflight.rs` (procedure shakedown; preflight↔pipeline coupling documented)

**Wave 4** *(blocked on 19-07)*

- [x] 19-08-PLAN.md — pipeline sub-split at the D-06 seams: `pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`

**Wave 5** *(blocked on 19-08)*

- [x] 19-09-PLAN.md — extract `parallel.rs`, `commands.rs`, `config_parse.rs`; reduce `main.rs` to a thin crate root

**Wave 6** *(blocked on 19-09)*

- [x] 19-10-PLAN.md — regenerate `.planning/codebase/STRUCTURE.md` + `TESTING.md`, reconcile this ROADMAP entry
- [x] 19-11-PLAN.md — phase gate: three-part equivalence proof on CI-on-branch (D-11), `ENV_MUTEX` disposition (D-12), scratch-repo 19a reproduction, requirement roll-call

### Phase 20: Release Correctness + Operator Control

**Goal:** Close the two defects that make DevFlow's own release cut unreliable (999.24's `VersionBump` self-pin, which has shipped broken two for two and hits any user with a published Cargo workspace; 999.23's unreliable `phase7_cli.rs` git fixtures, which have produced two distinct coin-flip failures on release-path PRs in a single day), then add the two operator controls the pipeline has never had — a clean stop point short of Ship (999.6) and a way to drive a phase through Ship when the monitor is dead (999.7) — plus a release-cut preflight (999.13) so the manual checklist stops being the only thing between a green suite and a broken publish.
**Targets:** v1.7.0 — decided 2026-07-23 (see "Milestone stays open" below). Nothing in these five units is inherently breaking.
**Promoted from backlog** 2026-07-22: 999.24 (DEN-49), 999.23 (DEN-48), 999.6 (DEN-31), 999.13 (DEN-38), 999.7 (DEN-32).
**Requirements:** 20a, 20b, 20c, 20d, 20e (see CONTEXT.md — no formal REQ-IDs)
**Depends on:** Phase 19 — the `main.rs` split is what makes 999.6, 999.7 and 999.13 plannable as one phase in ~3 waves; all three previously conflicted in a single 8,467-line file. 999.7 also depends on 18a/18b (shipped v1.5.0), which are what tell an operator *why* the pipeline is stuck.
**Plans:** 5/5 plans executed

**Sequencing is load-bearing:** 20a and 20b land first so this phase's own CI and release cut are trustworthy while the rest is in flight. 20d blocks on 20a — its primary check asserts 20a's invariant and must not encode today's manual patch as the expected state. 20e sequences last: it needs a design pass and it touches the Ship/outcome path 20d reasons about.

Plans:

**Wave 1** *(20a/20b — no file overlap; both gate this phase's own release cut)*

- [x] 20-01-PLAN.md — 20a: `version::write_version` also rewrites `[workspace.dependencies]` local-path self-pins (additive inline-table pass; PR #17 guard becomes no-op-by-construction)
- [x] 20-02-PLAN.md — 20b: `cleanup --force` liveness guard + bounded-backoff retry (product fix for the worktree race) and `phase7_cli.rs` fixture durability (instance 2, fixture-side per D-08)

**Wave 2** *(20c — depends on 20b; first of the serialized 20c→20d→20e CLI-dispatch chain)*

- [x] 20-03-PLAN.md — 20c: `devflow start --until <stage>` halts cleanly (new `State` stop marker, `transition` interception, `check_dead_agent` stop-awareness), `--until ship` rejected

**Wave 3** *(20d — depends on 20a + 20c; serialized after 20c to avoid a shared `main.rs`/`commands.rs` clap-enum merge conflict)*

- [x] 20-04-PLAN.md — 20d: `devflow release --check` read-only preflight — self-pin (asserts 20a), `develop`/`main` divergence, publish order, `gpg.format`-aware signing viability

**Wave 4** *(20e — sequenced last; depends on 20a + 20d; inherits 20a's self-pin fix via VersionBump)*

- [x] 20-05-PLAN.md — 20e: `devflow ship --phase N [--force]` manual override — second consumer of the on-disk Ship response, reuses `finish_workflow` (D-01), `--force` scoped to Ship (D-02)

## Backlog

Unsequenced items — not part of the active phase sequence. Promote with
`/gsd-review-backlog` when ready; each carries accumulated context in its
own `phases/999.N-*/CONTEXT.md`.

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
**Removed during Phase 26 discuss-phase:** the race is specific to `devflow parallel`'s whole-phase concurrency; the operator confirmed they never use, and would never want, a single DevFlow user running multiple phases at once ("that's just asking for trouble"). Since the scenario cannot occur in actual usage, this entry is removed rather than left filed. See `phases/26-release-cut-automation/26-CONTEXT.md` D-11. Linear: DEN-29 (close as won't-do).
`devflow parallel`'s own future (deprecate whole-phase concurrency vs. repurpose for intra-phase workstreams vs. leave alone) is captured as a deferred idea in that same CONTEXT.md, for its own future phase — not lost, just not a defect record.

### Phase 999.5: ChangelogAppend Placeholder Content (PROMOTED — Phase 26)

**Goal:** Every generated changelog entry reads "Released phase via DevFlow" — deferred twice already (17-10, 17-12). *(was 19j)*
**Priority:** Low | **Size:** M — reviewed 2026-07-21: confirmed still generic (`ship.rs:431`). Cosmetic by its own admission, but sized M not S — needs a real content source designed (plan diffs? SUMMARY.md extraction?) before implementation, which is why it's been deferred 3 times already. Linear: DEN-30.
**Requirements:** TBD — see `phases/26-release-cut-automation/999.5-BACKLOG-DOSSIER.md`
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

### Phase 999.25: Release-Cut Executor (`devflow release` that executes) (PROMOTED — Phase 29)

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
**Requirements:** TBD — see `phases/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md`
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

**Goal:** `check_ssh_signing_viability` (20d, `crates/devflow-core/src/git.rs`) misclassifies an inline (non-path) `user.signingkey` value — a literal key blob configured directly rather than as a file path is treated as a path and reported as not-found. Deterministic edge case; every path-based and no-key branch is already correct and tested. Full detail in `.planning/phases/20-release-correctness-operator-control/20-REVIEW.md` (INF-01).
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

**Evidence:** observed live 2026-07-26 while clearing the machine's orphan population with Phase 23's own tooling. 22 of 23 orphans were reaped by `devflow gate sweep`; PID 3744133 (`--phase 7`, root `/tmp/.tmpMVmZBl`, ~8.6h old) was unreachable. `stop` was tried against both phase 7 and phase 8 (the root's state file is `state-08.json`) — both returned the same no-lock/no-state message and exit 0. `kill -TERM` cleared it in 1s. Full detail: `.planning/phases/23-end-to-end-dogfood/23-FINDINGS.md` §A1.

**Why it matters:** the messages are *true* against recorded state, so this is not a false attestation — but an operator reading exit codes concludes the machine is clean when it is not. This is the residue of the orphan class Phase 23 was written to close, and the one case still requiring `kill(1)`.

**ESCALATED 2026-07-27 — these orphans are SIGTERM-IMMUNE, contradicting this entry's own evidence above.** The 2026-07-26 note records "`kill -TERM` cleared it in 1s." That did **not** reproduce. Clearing the machine's population today: 15 orphaned monitor wrappers (all `ppid == 1965`, i.e. reparented to `systemd --user`, all rooted in `/tmp/.tmp*`) were sent `SIGTERM`; **all 15 survived**, re-checked after real elapsed time, not an instant re-poll. Only `SIGKILL` cleared them. Killing the wrappers then **orphaned their `devflow advance` children**, which reparented to `systemd --user` in turn and *also* required `SIGKILL` — 30 processes total, none responding to `SIGTERM`.

**Why that matters more than the enumeration gap.** Each wrapper installs `trap cleanup TERM INT` where `cleanup` kills `$apid` and exits. That handler is evidently not firing — the most likely mechanism is the shell being blocked in `wait $apid` on a child it can never reap. **Any reaping path built on `SIGTERM` will therefore report success and leave the process running**, which is a strictly worse failure than the silence this entry already describes. The two-layer structure (wrapper + child, each independently reparented) also means a reaper must handle *both* layers; killing only the wrapper manufactures a fresh orphan.

**Fix direction, revised:** a registry-independent path — scan for running `devflow advance` children and reconcile against the registry, surfacing "running but unregistered" as its own reportable class rather than silence. Likely belongs in `doctor` as a finding plus a `gate sweep` flag. It **must** escalate `TERM` → `KILL` with a bounded wait and verify death rather than assuming it, and must reap the wrapper/child pair together. Add a regression test asserting a `TERM`-ignoring child is still cleared.

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
**Removed during Phase 26 discuss-phase, after briefly being promoted there:** the operator determined DevFlow should never predict tag-signing viability at all — a predictor is a second implementation of "will signing work?" that must stay in sync with git's real behavior, which is exactly the bug class this item and 999.54 are about. The executor built in Phase 26 instead runs the real signed `git tag` command directly and reads git's own result — no viability guess needed. Full reasoning: `phases/26-release-cut-automation/26-CONTEXT.md` D-10. Linear: DEN-75 (close as won't-do).

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
**Removed during Phase 26 discuss-phase, after briefly being promoted there:** same disposition as 999.50 — the operator does not want DevFlow predicting tag-signing viability at all, ever. Full reasoning: `phases/26-release-cut-automation/26-CONTEXT.md` D-10. Linear: DEN-79 (close as won't-do).

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

**Priority:** High — it is the one defect that makes a *correctly written* plan unable to finish unattended, and the workaround requires an operator who knows an undocumented manual-SUMMARY trick. | **Size:** M — (A) is bounded but touches launch/prompt/gate plumbing and needs a new gate classification; (C) is S; (B) is M and may not be needed. Source: Phase 26 dogfood run (2026-07-29), full write-up in `phases/26-release-cut-automation/26-01-SUMMARY.md` § "Issues Encountered". Linear: DEN-82.

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

### Phase 21: Operator Legibility & Observability

**Shipped as v1.8.0** (2026-07-24) — PR #23 (`develop → main`, squash `cfa9167`), signed tag `v1.8.0`, [GitHub Release](https://github.com/denniyahh/devflow/releases/tag/v1.8.0). `sync-main-to-develop.sh` run via PR #24 (merge `01ad9e4`). Published to crates.io (`devflow-core` then `devflow`, both confirmed live at 1.8.0).

**Goal:** Make DevFlow's operator surface **legible** and its self-reported state **trustworthy** — every unit single-writer, operator-facing, reversible or detection-only, and testable without any irreversible side effect. Scope recut from the original "Operator Usability & Release Execution" (operator decision, 2026-07-23): the release-cut executor (999.25) and `--base` (999.28) were removed (→ own phase / Phase 22 respectively) and the phase backfilled with legibility/observability units. Not `/gsd-review-backlog`-promoted; scope is operator-decided — see `phases/21-*/21-CONTEXT.md`.
**Requirements**: TBD (no REQ-IDs — units 21a–21d map to CONTEXT decisions D-03..D-07)
**Depends on:** Phase 20
**Plans:** 4/4 plans complete

**Sequencing is load-bearing:** 21d (staleness content-awareness) leads in Wave 1
per D-07 — the dogfood staleness guard hard-blocks this phase's own stages after
every `.planning/` commit, so it lands first. 21a/21b/21c then serialize
(Waves 2/3/4) because all three edit `crates/devflow-cli/src/commands.rs` and the
same-wave zero-file-overlap rule forbids parallelizing them (the familiar
`commands.rs`/`main.rs` contention from Phases 18/19). 21e (changelog content)
stays excluded stretch (D-08, blocked on a content-source design decision).

Units (operator-decided; committed unless marked optional):

- **21a** — Operator discoverability (999.3 / DEN-28): `gate show`, rate-limit reset surfacing, in-stage `status` progress, recovery-verb hints. Sequence early.
- **21b** — Doctor planning-doc staleness reconciliation (999.14 / DEN-39): flag stale ROADMAP/STATE narrative vs git tags; detection-only.
- **21c** — sequentagent second-process tracking (999.2 / DEN-27): narrowed — monitor half shipped v1.5.0.
- **21d** — Dogfood staleness guard content-awareness (999.29 / DEN-54): stop docs-only false-blocks. **Sequence first** (unblocks this phase's own stages).
- **21e** *(optional/stretch)* — ChangelogAppend real content (999.5 / DEN-30): blocked on choosing a content source.

Plans:

**Wave 1** *(21d first per D-07 — unblocks this phase's own dogfood stages)*

- [x] 21-01-PLAN.md — 21d: content-aware `embedded_commit_is_stale` strict-ancestor arm (docs-only ranges → Fresh; docs+source → Stale) + fix two now-broken fixtures + block-message wording (staleness.rs)

**Wave 2** *(blocked on 21-01; shares commands.rs with 21b/21c)*

- [x] 21-02-PLAN.md — 21a: additive discoverability — `devflow gate show <phase>` (untruncated), rate-limit reset time in `status`, in-stage progress line, recovery-verb hints from a stuck state (commands.rs, main.rs)

**Wave 3** *(blocked on 21-02; shares commands.rs)*

- [x] 21-03-PLAN.md — 21b: detection-only `doctor` planning-doc staleness check vs git tags — third `--json` key, v1.5.0 legacy-noise cutoff, no prose auto-edit (commands.rs)

**Wave 4** *(blocked on 21-03; shares commands.rs)*

- [x] 21-04-PLAN.md — 21c: sequentagent second-process record (path-free slot A/B + AgentKind, not routed through State) surfaced in `status` (agent_result.rs, parallel.rs, commands.rs)

### Phase 22: Concurrency & Governance Correctness

**Goal:** A light dogfooding trial, not the full concurrency/governance phase: resolve the four advisory findings from Phase 21's code review (`21-REVIEW.md`), promoted as backlog **999.30 / DEN-55**. Share the omitted-stage gate resolution `gate_show` and `gate_respond` currently copy-paste (WR-01); replace `collect_planning_doc_findings`'s hardcoded `"main"` with `devflow_core::config::MAIN` (WR-02); make `gate_show` read open gates once, closing a narrow TOCTOU (WR-03); fold `status`'s per-phase `events.jsonl` rescan into the existing single-pass event summary (IN-01). Runs through Validate and stops before Ship. The broader "Concurrency & Governance Correctness" scope (999.4 version-tag contention, 999.26 object-store races, 999.28 `--base`) remains unplanned and is explicitly out of this trial's boundary — see `phases/22-concurrency-governance-correctness/22-CONTEXT.md`.
**Requirements**: TBD (no REQ-IDs — narrow trial scoped directly from 999.30 / DEN-55, see CONTEXT.md)
**Depends on:** Phase 21
**Plans:** 2/2 plans complete

**Trial history:** First attempted with Codex (2026-07-24) — Define succeeded, Plan failed immediately (`/gsd-plan-phase` reached Codex as a literal, non-existent shell command, the confirmed defect behind `.planning/audits/2026-07-24-codex-compatibility-review.md` and backlog 999.31/DEN-56). Two ad-hoc point-fixes were committed directly against `crates/devflow-core/src/agents/codex.rs` as a stopgap; Codex still couldn't route the skill and the run stalled on `apply_patch` errors mid-ROADMAP-promotion, leaving the workflow state lost (`devflow doctor` showed no active phase) though the worktree/branch survived. Retried 2026-07-24 with Claude directly (operator decision, given 999.31 isn't implemented yet): the two ad-hoc Codex commits were reset out (`git reset --hard` to `4af0991`), the existing `22-RESEARCH.md`/plans were kept and executed as-is.

**Executed 2026-07-24** on `feature/phase-22` — 22-01 (`c442e00`) then 22-02 (`2bbfabd`), plus a docs commit (`c30f617`) promoting this ROADMAP section from `[To be planned]`, closing the plan-checker BLOCKER the stalled Codex run never resolved. **Validated**: `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test --workspace` green.

**Re-verified and shipped 2026-07-24.** The original "tests green" claim was recorded the same day this repository was corrupted by 999.37, so it was re-run independently from a clean checkout rather than trusted: 537 tests across 13 binaries, 0 failed. Also audited for coupling to the process-management model currently being revamped (999.33/999.34) — **zero**: no changed line touches monitor, `monitor_pid`, pgid, spawn, teardown, `Liveness`, `process_group` or supervisor, and only two source files change (`commands.rs`, `events.rs`). Both changes are read-side; the event `emit` path is untouched, so the socket-supervisor redesign leaves this work intact. `commands.rs` nets **−6 lines** by removing the `gate_show`/`gate_respond` copy-paste.

The branch was then reused as the **staging branch for this release**, integrating the 999.37 containment fix and the naming reframe, tested as a unit, and merged to `develop` from there. Its name understates its final contents.

*Note:* the Codex compatibility audit (`.planning/audits/2026-07-24-codex-compatibility-review.md`) and the 999.31 agent-driver-modularization backlog entry (`.planning/phases/999.31-agent-driver-modularization/CONTEXT.md`) referenced above were **not in the v1.8.1 release** — they sat on the unmerged `archive/planning-docs-2026-07-24` branch. **Recovered onto the mainline 2026-07-25**, along with the three process-lifecycle/teardown/supervisor audits and the scope-creep review, so those references now resolve. DEN-56 tracks the same defect in Linear.

Plans:

- [x] 22-01 — Shared gate resolution + `gate_show` one-read + `MAIN` constant (WR-01, WR-02, WR-03)
- [x] 22-02 — Single-pass `stage_launched` timestamp summary + full Validate (IN-01)

### Phase 23: End-to-End Dogfood — One Phase, Define→Ship, Unattended, With Claude

**Goal:** Make `devflow start --phase N` drive one real phase from Define
through Ship **unattended with Claude**, with no manual `ps`, no manual
`devflow advance`, and no silent stall. This is the "basic development
workflow works end to end" milestone, scoped deliberately narrow: one agent
(Claude), one phase, no release cut.

**Scope recut 2026-07-25 (operator decision).** This phase previously read
"Test Suite & CI Hardening" with a `[To be planned]` placeholder goal. That
theme — 999.15, 999.17, 999.18, 999.19, 999.20, 999.22 — was reviewed against
the end-to-end goal and advances it by approximately zero, so it returns to the
backlog undisturbed and this phase is repurposed. Nothing was lost: the prior
entry had no content beyond its title.

**Requirements**: TBD (no REQ-IDs — units 23a–23e, sourced from 999.33/DEN-58
and 999.34/DEN-59 plus the dogfood-probe finding this phase generated first).
Plans carry the unit identifiers `23a`, `23b`, `23c`, `23d`, `23e` and
`yes-ship` as requirement tokens. **`23b` and `23c` were redefined, and `23e`
added, by the 2026-07-25 replan** — see "Re-aimed 2026-07-25" below.
**Depends on:** Phase 22
**Plans:** 16/16 plans complete
gap-closure set** (planned 2026-07-26 from `23-VERIFICATION.md`'s single
recorded gap), unstarted
2026-07-25; the original 23-03…23-12 are archived under
`phases/23-end-to-end-dogfood/superseded/`)

**FINAL STATUS 2026-07-27 — CLOSED, 16/16 plans, by recorded operator decision.**
Attempt 3 (`23-ACCEPTANCE-RUN-3.md`) drove Define → Plan → Code **fully
unattended**, all three stages succeeding with verified on-disk deliverables — the
furthest any DevFlow-driven run has reached in this project's history — then halted
at the Validate boundary on a **correct** D-18 staleness firing, because the target
phase modifies DevFlow's own compiled source. Resuming would have required
rebuilding the driver from unvalidated code, which the operator rejected on
soundness grounds.

The behavioural criterion is therefore **accepted-unmet, not satisfied**, and is now
permanently unmeetable for phase 24: that phase was completed manually and merged
via PR #34, so a re-run would no-op through every stage and `workflow_shipped` can
never be emitted for it. The structural cause is filed as **999.48 / DEN-73**; the
oracle was deliberately NOT re-pointed at a substitute target. Full disposition in
`23-VERIFICATION.md`'s `accepted_exception` frontmatter block.

Positively: the halt was **not a silent stall** — the failure mode this phase exists
to eliminate. The guard emitted a typed event and both monitor and agent exited
cleanly, against Phase 17's two silent monitor deaths at ~4h each.

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
`phases/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md` for the original backlog
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

### Phase 24: `release --check` Signing-Key Inline Classification

**Goal:** `check_ssh_signing_viability` (20d, `crates/devflow-core/src/git.rs`) misclassifies an inline (non-path) `user.signingkey` value — a literal key blob configured directly rather than as a file path is treated as a path and reported as not-found. Deterministic edge case; every path-based and no-key branch is already correct and tested. Full detail in `.planning/phases/20-release-correctness-operator-control/20-REVIEW.md` (INF-01).
**Priority:** Low | **Size:** S — single classification branch + one test; found by Phase 20 code review (2026-07-23). Linear: DEN-52.
**Requirements**: TBD — promoted from backlog 999.27
**Depends on:** Phase 23
**Plans:** 2/2 plans complete

*Promoted from backlog Phase 999.27 on 2026-07-26 as the acceptance target for Phase 23 plan 23-11 (D-02). Selected as low-stakes by consequence: a release-preflight advisory check that touches no merge, version, or ship control flow.*

Plans:

**Wave 1**

- [x] 24-01-PLAN.md — Classify `key::`/raw-`ssh-` `user.signingkey` values as inline keys per git's own prefix precedence, fingerprint them via `ssh-keygen -lf -` over stdin, and prove it with five agent-independent tests in `devflow-core::git`

**Wave 2** *(blocked on 24-01)*

- [x] 24-02-PLAN.md — Operator-boundary proof: `devflow release --check` neither leaks the inline blob nor reports it missing, and degrades to a non-blocking `warn` when the ssh tooling is absent

### Phase 25: End-to-End Dogfood Blockers — Start, Progress, Finish, Recover

**Goal:** Make an unattended `devflow start --phase N --agent claude --mode auto --yes-ship` run reach a completed Ship stage **without a human touching it**, by closing the four things that currently prevent it. Phase 23 proved the goal is not reachable today: its third acceptance attempt drove Define→Plan→Code unattended — the furthest any run has gone — then halted, and two of its three attempts additionally required a human to repair the base ref before `devflow start` would even launch. This phase closes the specific, individually-evidenced blockers rather than re-attempting the run and rediscovering them.
**Priority:** High | **Size:** L (re-sized at plan time, 2026-07-27 — was M) — six units plus 999.38 folded in. 25b, 25e, 25f and 999.38 are genuinely S as filed; 25a is S–M, option chosen at plan review 2026-07-27 (fetch + fast-forward-when-safe, else refuse — see `25-05-PLAN.md` §`<resolved_decision>`, and CONTEXT.md D-17 as amended); **25c is M, not the S this entry states** — it is a full replacement of `compute_version`'s three inputs plus a new preflight gate plus a previously-unflagged consumer at `pipeline_gate.rs:809-840`. No phase split recommended; see `25-01-PLAN.md` § Phase-level notes for the assessment and the seam if one is ever wanted.
**Requirements**: TBD — promoted from backlog 999.51, 999.48, 999.49, 999.44, 999.47; plus 25f (CONTRIBUTING release-procedure drift, no backlog entry — found 2026-07-27). Tracked by unit identifier (`25a`–`25f`, `999.38`), not by REQ-ID — this project has no `.planning/REQUIREMENTS.md`.
**Depends on:** Phase 24
**Plans:** 18/19 plans executed

Gap-closure plans (wave numbering restarts at 1 for this run):

- [x] 25-08-PLAN.md — 25c/999.49: the D-09 major-bump gate fires in the default worktree path (CR-01 aggregation + CR-02 execution-root scope + real-worktree regression test) (wave 1)
- [x] 25-09-PLAN.md — 25c/999.49: `release_range_start` anchors correctly across realistic release topologies (CR-03 + two topology fixtures) (wave 1)
- [~] 25-10-PLAN.md — HALTED at Task 1 Step E; SUPERSEDED by 25-13
- [x] 25-11-PLAN.md — 25e/999.47: fresh site census + bounded exec-visibility barrier at every vulnerable spawn-then-cmdline-census site in `crates/` (wave 1)
- [x] 25-12-PLAN.md — 25e/999.47: production reaper age floor (`agent::STRAY_MIN_AGE`) refusing to `SIGKILL` inside the exec-visibility window (wave 2)
- [x] 25-13-PLAN.md — 25e/999.47 + 25f: push through the real `pre-push` gate, an 11-observation CI-on-branch streak, and dual human sign-off (wave 3)

**25-10 disposition:** 25-10 halted at Task 1 Step E when its push was rejected 2/2 by the `pre-push` container gate on the very defect its trials were meant to observe, and is superseded by 25-13 — a corrected protocol with a falsified-premise fix, a corrected test list, and a second (local push-gate) verification shape — rather than re-run, because re-running it unchanged would produce evidence about tests that are no longer the risk.

Gap-closure plans, round 3 — planned 2026-07-28 against `25-VERIFICATION.md`'s two remaining gaps (wave numbering restarts at 1 for this run):

- [x] 25-14-PLAN.md — 25a/999.51: CR-02 — the base-ref fast-forward becomes a compare-and-swap (`git update-ref` with `<oldvalue>`) behind a repository-wide checked-out predicate (`git worktree list --porcelain`), plus a second-worktree regression test (`preflight.rs`) (wave 1)
- [x] 25-15-PLAN.md — 25d/999.44: CR-01 — a registry-reachability filter interposed between the structural `/proc` census and BOTH operator surfaces, an explicit `--root` ruling, corrected `doctor`/`--reap-strays`/census wording, and a three-fixture same-pass regression test (`commands.rs`, `main.rs`, `agent.rs`) (wave 1)
- [x] 25-16-PLAN.md — WR-03 (folded in, not deferred): the two tests that drive a real `launch_stage_inner` now reap the detached monitor wrapper they spawn, with verified death, via one shared `#[cfg(test)]` helper (`test_support.rs`, `staleness.rs`, `pipeline_launch.rs`) (wave 1)

**Round-3 wave rationale:** all three run in parallel in wave 1. Their file sets were checked against live source at plan time and are disjoint — `preflight.rs` / (`commands.rs`, `main.rs`, `agent.rs`) / (`test_support.rs`, `staleness.rs`, `pipeline_launch.rs`) — so the same-wave zero-file-overlap rule that forced near-serial waves in Phases 18, 19, 21 and in this phase's own rounds 1–2 does not bind here. 25-14 does not touch `commands.rs` because `ensure_base_ref_current`'s signature and its `commands.rs:154` call site are unchanged by construction. **The `scripts/check-in-container.sh all` push-gate run is a phase-level, post-merge step run ONCE after all three merge — not once per worktree**, because several simultaneous `taskset -c 0,1` container runs manufacture the same load-induced flake 25-11/25-12/25-13 closed.

Gap-closure plans, round 4 — planned 2026-07-28 against `25-UAT.md`'s two gaps, both test-only defects in the `devflow` CLI crate (wave numbering restarts at 1 for this run):

- [x] 25-17-PLAN.md — G-25-2/WR-06: the monitor reap becomes an RAII `Drop` guard (`test_support::ReapMonitorOnDrop`) bound before the panicking assertions at both sites 25-16 fixed, with a double-panic interlock and a discriminating test plus control (`test_support.rs`, `pipeline_launch.rs`, `staleness.rs`) (wave 1)
- [x] 25-18-PLAN.md — G-25-1/WR-05: the two `preflight.rs` tests that reach `monitor::spawn_monitor` through `run_preflight`'s `Advance`/`LoopBack` recursion bind the same guard and assert they really spawned; closes `WINDOWS.md` items 1 and 3 (`preflight.rs`, `.planning/WINDOWS.md`) (wave 2)

**Round-4 wave rationale:** the two gaps' file sets are disjoint (`preflight.rs` alone vs `test_support.rs`/`pipeline_launch.rs`/`staleness.rs`), but they are NOT independent. G-25-1's fix must use the guard G-25-2 introduces — wiring a plain trailing `reap_spawned_monitor(&state)` into `preflight.rs` would knowingly reproduce WR-06 in two new places on the same day it was closed in two others. So 25-18 depends on 25-17's API rather than on its files, and runs in wave 2. The guard's exact signature is pinned verbatim in 25-18's `<interfaces>` so its executor needs no cross-plan SUMMARY read, and 25-18 carries a precondition asserting the guard exists before it edits.

**WR-03 disposition (explicit, per the verifier's request):** folded into round 3 as 25-16 rather than deferred. The justification is not tidiness — 25-15's regression test asserts against a live `/proc` census, and the leak replenishes that census's noise on every `cargo test --workspace` run. The verifier's own words: it *"actively degrades confidence in any future CR-01 fix's own test suite until closed."* WR-01 (`version.rs:338-349`) and WR-04 (`commands.rs:3727-3762`) remain Info-severity and are NOT pulled in: `version.rs` is in no round-3 file set, and while 25-15 does edit `commands.rs`, it does not touch WR-04's lines — the reachability filter is interposed before `reap_stray_candidates`, leaving that function and its test untouched.

*Scoped 2026-07-27 against one criterion — "what does an unattended run need in order to finish?" — after validating every open High in the backlog against the codebase. The four requirements below are the decomposition; each unit maps to exactly one.*

**The four requirements, and the unit that closes each:**

| # | Requirement | Unit | Evidence it is unmet today |
|---|---|---|---|
| 1 | A run **starts** on a current base | **25a** — 999.51 / DEN-76 | Blocked 2 of 3 acceptance attempts; a human ran `git fetch` by hand each time |
| 2 | A run **progresses** through all five stages | **25b** — 999.48 / DEN-73 | Attempt 3 halted at the Validate boundary on a correct D-18 firing |

**25b re-sized S (was S–M), 2026-07-27 — the fix is one guard, not a design pass.** `launch_stage`'s third parameter, `archived_stage: Option<Stage>`, *already* distinguishes the two cases at every call site: `None` is a fresh start (`commands.rs:236`, i.e. `devflow start`), `Some(stage)` is a transition (`pipeline_gate.rs:110/122`, `pipeline_outcomes.rs:362/370`). Pinning the verdict is therefore `if archived_stage.is_none() { enforce_build_staleness(…)?; }` — no new state field, no threading, no design pass. `pipeline_launch.rs:165` already documents this exact shape for the `Advance` arm ("skipping a check it just adjudicated for this one relaunch, not granting a standing bypass"). The earlier S–M estimate was made before checking whether the distinguishing information already existed; it did.

**Why 25b is kept rather than deleted, though it is the only dogfood-only unit.** `staleness.rs` is 1,794 lines and 21 tests, all gated on `is_self_dogfood_workspace` — it fires in this repository and nowhere else, so it is genuine dogfooding overhead and a fair candidate for removal. It is retained because the outstanding work is a single line: deleting a tested, working module to avoid one guard is a worse trade than paying it. The default stays **on for a self-dogfood workspace, evaluated once at `start`**, which preserves the Phase 16 false-evidence protection (a stale binary cannot *begin* driving its own repo) while removing the mid-run halt, which has no safety value — the source it reports staleness against is the phase's own in-progress work. A bypass *flag* is deliberately not the mechanism: since the guard fires only here, "skip in dogfood" is deletion with extra steps.
| 3 | A run **finishes** with correct artifacts | **25c** — 999.49 / DEN-74 | `compute_version` yields `~1.11.359` against a real `1.8.1` |
| 4 | A stalled run **recovers** without `kill(1)` | **25d** — 999.44 / DEN-68 | 15/15 orphans survived `SIGTERM`; only `SIGKILL` cleared them |

**Plus one unit that is not a requirement but a stall generator:**

**25e** — 999.47 / DEN-72. `MAX_CONSECUTIVE_FAILURES = 3` (`mode.rs:18`) forces a gate after three consecutive Validate failures. A test that fails roughly half the time under CI load is therefore not merely CI noise inside an unattended loop — it is a mechanism for halting a healthy run. Cheapest unit in the phase; its production risk is already closed, so this is test-only work.

**25f** — CONTRIBUTING.md's release procedure is stale in two ways after the signing work merged in PR #38, and both mislead at the highest-stakes step. **(a)** Step 5 says `git tag -s vX.Y.Z <commit>`. On a machine where the agent works, `user.signingkey` is the *agent's* key, so that command produces an **agent-signed release tag** — exactly what the maintainer-key policy exists to prevent. The `pre-push` hook now refuses to push such a tag, so the outcome is a blocked release rather than a mis-signed one, but the operator discovers it *after* the squash-merge to `main` has already happened. Step 5 must direct the reader to `git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s …`. **(b)** The same step warns that "a repo-local `tag.gpgsign=false` means `-a` alone will not sign" — no longer true, since the tracked `.gitconfig` added in #38 sets `tag.gpgsign=true`. A reader following a stale warning reasons about the wrong failure mode. Size S, docs-only. Found 2026-07-27 while walking the v2.0.0 release; the drift was introduced by #38 itself, so it is this project's own regression to close.

**Sequencing.** 25a → 25b → 25c is the spine and is ordered: a run must start correctly before "does it progress" is even observable, and must progress before "does it finish correctly" can be tested. 25d and 25e are independent of the spine and of each other, so they can run in parallel with it.

**25b and 25c compose and must ship together.** 25b makes an unattended run reachable; 25c fires on the first run that succeeds. Shipping 25b alone converts a phase that cannot finish into one that finishes by writing a garbage version and tagging it on `develop` — strictly worse than the current halt, because `hooks_after_ship` has no rollback after its `Merge` step.

**Optional sixth unit — 999.38 (Medium), operator's call at plan time.** A test-suite `PATH` race, distinct from 999.47 but the same failure shape: a genuine flake ("reproduced once in four full-suite runs") feeding the same 3-strike Validate gate. Folding it in with 25e does the flake work in one pass; leaving it out does not block the goal. Not counted in this phase's size.

**Deliberately excluded, with reasons — do not re-add without revisiting these:**

- **999.31 / DEN-56** (modular agent driver) — a *Codex* blocker. This goal is scoped "with Claude"; Phase 23 deferred it for precisely this reason. Including it roughly doubles the phase and moves the stated goal by zero.
- **999.25 / DEN-50** (release-cut executor) — Phase 23's own scoping states *"'end to end' stops when the Ship stage completes, the crates.io publish stays manual."* Including it re-expands a deliberately narrowed scope.
- **999.15 / DEN-40** (hermetic shell-entry-point tests) and **999.21 / DEN-46** (AI-acceptance wiring) — neither sits on the dogfood execution path; DEN-46's wiring surface is partly outside this repository.
- **999.4 / DEN-29** (concurrent-ship version race) — requires two simultaneous ships; this goal is one phase.
- **999.5 / DEN-30** (changelog placeholder content) — on the Ship path but cosmetic, M-sized, and deferred three times for want of a content source.
- **999.39** (production git calls inherit `GIT_DIR`) — a real exposure of the 999.37 class, but `devflow` is not invoked from a git hook on the pipeline's critical path. Considered and excluded, not overlooked.

**Acceptance (revised 2026-07-27 per CONTEXT.md D-15/D-16 — standing policy change, operator-confirmed).** The end-to-end acceptance run this paragraph previously required is now **unofficial and continuous**: it runs when the operator chooses and **gates no phase's completion, until further notice** — not this phase, not later ones. Phase 25 is therefore **complete when 25a–25f are each implemented and verified on their own unit-level merits** — each unit needs its own verifiable acceptance (a test, a closed reproduction) rather than relying on the end-to-end run to backstop it. Anything a future unofficial run surfaces is filed to the backlog the usual way, exactly as Phase 23's runs were. This suspension is deliberately reversible — the single-run closure criterion can be reinstated later if the operator chooses.

**Planned 2026-07-27 — 7 plans across 3 waves.** Ordered by file ownership rather than by the
spine above: `commands.rs` is touched by four of the six units and `preflight.rs` by two, so the
same-wave zero-file-overlap rule forces the sequencing (the same constraint Phases 18, 19 and 21
each hit). D-15's decoupling of the acceptance run dissolves the spine's observational rationale,
so the code dependency graph is what orders the work. **25b (plan 25-03) and 25c (plans 25-01 and
25-06) all land in this phase**, honouring the ship-together constraint below.

Plans:

- [x] 25-19-PLAN.md

- [ ] 25-10-PLAN.md

- [~] 25-10-PLAN.md — SUPERSEDED by 25-13 (see the gap-closure list above; this line previously read `[ ]`, contradicting that disposition)

- [x] 25-11-PLAN.md
- [x] 25-12-PLAN.md
- [x] 25-13-PLAN.md
- [x] 25-14-PLAN.md — 25a/CR-02: compare-and-swap fast-forward + repository-wide checked-out predicate (`preflight.rs`) (round-3 wave 1)
- [x] 25-15-PLAN.md — 25d/CR-01: registry-reachability filter before both stray surfaces, `--root` ruling, corrected wording (`commands.rs`, `main.rs`, `agent.rs`) (round-3 wave 1)
- [x] 25-16-PLAN.md — WR-03: launch-driving tests reap the monitor they spawn (`test_support.rs`, `staleness.rs`, `pipeline_launch.rs`) (round-3 wave 1)
- [x] 25-17-PLAN.md — G-25-2/WR-06: RAII `ReapMonitorOnDrop` guard bound before the panicking assertions, with a double-panic interlock and a discriminating test plus control (`test_support.rs`, `pipeline_launch.rs`, `staleness.rs`) (round-4 wave 1)
- [x] 25-18-PLAN.md — G-25-1/WR-05: the two `preflight.rs` recursion tests bind the same guard and assert they really spawned; closes `WINDOWS.md` items 1 and 3 (`preflight.rs`, `.planning/WINDOWS.md`) (round-4 wave 2)

**Wave 1** *(four plans in parallel — disjoint file sets)*

- [x] 25-01-PLAN.md — 25c derivation: reachable-tag baseline, anchored commit range, conventional-commit classification, rewritten `compute_version` (`version.rs`, `+semver`, `+git-conventional`)
- [x] 25-02-PLAN.md — 25d/25e core primitives: `terminate_and_verify`, registry-independent stray discovery, `#[deprecated]` on the unsound predicate (`agent.rs`)
- [x] 25-03-PLAN.md — 25b + 999.38: hoist the staleness adjudication into `start`, prove it is not re-adjudicated mid-run, de-race the descendant-commit test (`pipeline_launch.rs`, `commands.rs`, `staleness.rs`)
- [x] 25-04-PLAN.md — 25f + D-06 + D-16: CONTRIBUTING release step 5, the versioning constraint in two places, the Acceptance paragraph below (`CONTRIBUTING.md`, `ROADMAP.md`, `PROJECT.md`)

**Wave 2** *(blocked on 25-03 — shares `commands.rs`)*

- [x] 25-05-PLAN.md — 25a: base-ref currency probe wired ahead of the reachability guard; fetch, then fast-forward the local base when safe and refuse loudly otherwise (`preflight.rs`, `commands.rs`)

**Wave 3** *(two plans in parallel — blocked on 25-01/25-02 and on 25-05's file ownership)*

- [x] 25-06-PLAN.md — 25c gate: `preflight_major_bump_check` (D-09) plus the `pipeline_gate.rs` fixture that re-derives the replaced algorithm (`preflight.rs`, `pipeline_gate.rs`)
- [x] 25-07-PLAN.md — 25d/25e surface: `doctor` stray finding, opt-in `gate sweep` reaping, deleted-root e2e test, retargeted `stop` identity test (`commands.rs`, `main.rs`, new `tests/reap_strays_e2e.rs`)

### Phase 27: Scrub Redirecting Git Environment From Production Calls

**STATUS: COMPLETE, 2026-07-30** — merged to `develop` via PR #60 (merge commit `c6192a1`), 47 commits. **Not released**; no tag or version bump was cut, and Ship's terminal hooks never ran (see "How this phase closed" below).

**Delivered:** all **41** production `Command::new("git")` sites across 7 files routed through `devflow_core::git::{git_command, hermetic_command}`, plus two indirect `sh → cargo → git` spawn edges — including **`monitor.rs:148`, the spawn that launches the coding agent itself**, which was the single highest-consequence site in the codebase and was *not* in the original literal-grep scope. Verified on `develop` after merge: **zero** bare `Command::new("git")` remain in production code (every surviving textual match is a comment). `test_support` now re-exports `git.rs`'s constants rather than keeping a second copy, so the two lists cannot drift.

**Acceptance signal met** — the whole point of the phase. 999.37 deliberately left 37 unit tests failing under a dirty environment so the exposure would stay visible. Under a hostile `GIT_DIR` the suite is now `411 passed / 0 failed` (devflow-core) and `188 passed / 0 failed` (devflow-cli); ordinary `cargo test --workspace` is **719 passed / 0 failed**, clippy `-D warnings` and `cargo fmt --check` clean. `27-VERIFICATION.md`: **status `passed`, 7/7 truths**, with the verifier independently re-running the falsification (revert a call site → test fails → restore → passes) rather than trusting the claim.

**Review:** `27-REVIEW.md` — 0 critical, 3 warnings, 3 info. All three warnings closed in `936b371`. WR-01 is worth remembering: two "hostile `GIT_DIR`" tests set no hostile environment at all and passed identically with or without the scrub, and **the first attempt to fix them was itself vacuous** (passed `--exact` against a bare test name, so zero tests ran and the binary still exited 0). Both now assert the child reported `1 passed`, not merely that it exited 0.

**Residual, by design:** four lower-consequence indirect spawn edges remain open — `hooks.rs:222`, `gates.rs:323`, `verify.rs:106`, `commands.rs::cmd_check`. None launches an agent. Found by this phase's own census (`27-SPAWN-CENSUS.md`, RESEARCH Assumption A2 held **OPEN, not silently closed**) and filed as **999.61**. D-02 also held throughout: `crates/devflow-cli/build.rs` is byte-identical to its base state, and the indirect chain behind it was closed at DevFlow's own spawn edge instead.

**How this phase closed — Ship was deliberately not run.** Dogfooded through DevFlow itself (`devflow start --phase 27 --mode auto --until validate`) Define→Plan→Code→Validate. Ship was excluded because its terminal hooks (`Merge → VersionBump → ChangelogAppend → BranchCleanup`) are fail-fast with no rollback and would have merged and tagged directly, and because `develop` is PR-gated (W-17 bypass deliberately unconfigured). The branch was pushed and merged by human review instead. **The run surfaced 999.60/DEN-85 live:** `devflow resume` discarded the never-fired `--until validate` cap and advanced into Ship unguarded; Ship was killed ~10s in with no side effects, held only by `yes_ship: false`.

**Unblocks:** **999.25** (release executor, DEN-50) — this was its prerequisite #1, since `26-REVIEW.md` CR-01 showed `mutating_project_root` is bypassed by an inherited `GIT_DIR` — and **999.52** (`devflow sync`), which shares that guard.

**Promoted:** 2026-07-30 — backlog **999.39 / DEN-66**, re-verified open at HEAD `b3cab1c` before promotion. Promoted to its own phase exactly as Phase 26 anticipated ("Deferred to its own phase (27+), not because it's low-value").

**Goal:** Route every production git invocation through a single scrubbing constructor — mirroring what `test_support::git_command` already does for tests — so that `GIT_DIR`, `GIT_WORK_TREE` and the other repository-local variables cannot silently redirect DevFlow onto a repository the operator never named. At promotion time all production `Command::new("git")` sites pinned `current_dir()` and nothing else (estimated ~86; the grep-confirmed count was **41**, plus indirect spawn edges the literal grep could not see); `GIT_DIR` outranks the working directory, so any `devflow` process launched with one set operates on *that* repository instead.

**Why this is High, not hygiene.** Phase 26's re-review (`26-REVIEW.md`, finding **CR-01**) found that `mutating_project_root` — the guard written expressly to stop `release --execute`/`sync` acting on an unnamed repository (D-13, closing C-06) — **is bypassed by an inherited `GIT_DIR`**. `git rev-parse --show-toplevel` reports the *cwd's* work tree while HEAD, refs and objects come from `GIT_DIR`, so the guard compares two paths, sees a match, and passes while the executor pushes, tags, and publishes against a different repository. The effect is no longer wrong answers but **irreversible operations against the wrong repository**. Git sets these variables for hooks, `rebase --exec`, `bisect run` and `submodule foreach` — and DevFlow runs git hooks as part of its own workflow.

**Sequencing consequence — this is why the phase exists now.** No mutating-command root guard can be trusted until this lands. It is prerequisite #1 for **999.25** (release executor, DEN-50) and blocks **999.52** (`devflow sync`) from shipping independently, since sync shares the same bypassed guard. Both are held on `feature/phase-26` awaiting it.

**Design decision this phase must settle:** whether an operator-set `GIT_DIR` should ever be honoured. The safer default is **no** — decide deliberately in discuss-phase rather than by omission, and record it.

**Acceptance signal already exists.** With `GIT_DIR` set, 37 unit tests currently fail because production functions under test (e.g. `tag_exists_and_reachable`) resolve to the wrong repository. 999.37 deliberately left these failing rather than papering over them, so they are a ready-made red-to-green target — see the 999.39 backlog entry's "Residual, by design" note. Add a regression test asserting the guard itself holds under a hostile `GIT_DIR`; `crates/devflow-cli/tests/project_root_guard.rs:18-21` names the risk for its own helpers today but never tests it against the guard.

**Requirements**: TBD — no REQ-IDs; tracked by backlog identifier (`999.39`), consistent with Phases 21/22/26. Full evidence, escalation rationale and prior-deferral reasoning remain in the **999.39** backlog entry above; this entry does not restate them.
**Depends on:** Phase 26 (CLOSED PARTIAL — no code dependency, but 999.25's re-attempt depends on this)
**Plans:** 6/6 plans complete

**Scope caution carried from Phase 26's exclusion note:** ~86 call sites spanning most of `crates/` including `git.rs`. Mechanical, but it is production behavior and touches the same files as the unmerged 999.25 cluster — expect file-overlap serialization and plan waves accordingly.

**Planning corrections (2026-07-30, measured — supersede the two figures above).** The `~86` call sites is a double count (an `rg` glob matched each file twice); the grep-confirmed figure is **41** production sites across 7 files, verified independently three times at HEAD. The `37 unit tests` acceptance signal was measured during 999.37 and predates several phases; re-measured live under a hostile `GIT_DIR` it is **54** (devflow-core) **+ 44** (devflow-cli, `pipeline_gate`/`pipeline_outcomes` excluded) **= 98**. A bare `cargo test --workspace` does **not** terminate under a hostile `GIT_DIR`, so the acceptance commands are the two scoped ones. Full detail: `27-RESEARCH.md` § Summary and `27-01-PLAN.md` § "Phase-level recorded decisions and corrections".

Plans:
**Wave 1**

- [x] 27-01-PLAN.md — Tracer: the scrubbing constructor in `devflow-core::git`, proven end-to-end through one core caller and one cli caller under a hostile `GIT_DIR` (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 27-02-PLAN.md — Migrate `git.rs`'s remaining 7 sites (4 of them wrapper chokepoints) and both `worktree.rs` sites (wave 2)
- [x] 27-03-PLAN.md — Migrate `version.rs` (10 sites) and `agent_result.rs` (3 sites) (wave 2)
- [x] 27-04-PLAN.md — Migrate `staleness.rs` (2 sites) and `commands.rs` (3 git sites + the indirect `sh -c` spawn, RESEARCH Open Question #1 decided in scope) (wave 2)
- [x] 27-05-PLAN.md — Migrate `preflight.rs` (11 sites, 2 inside closures, 1 write operation) (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 27-06-PLAN.md — Workspace spawn-edge census and the phase acceptance run under a hostile `GIT_DIR` (wave 3)

### Phase 28: Close the Checkpoint Answer Return Path

**Promoted:** 2026-07-30 — bundles **999.57 / DEN-82** (primary), **999.59 / DEN-84**, and **999.60 / DEN-85**. All three re-verified open at HEAD `8072ab6` before promotion (see "Verified at promotion" below). Chosen over 999.25 (release executor), whose own entry lists five prerequisites of which only one — 999.39, delivered by Phase 27 — is now met.

**Goal:** Make an operator's answer reach the agent. A `gate="blocking-human"` checkpoint is currently a **dead end for any DevFlow-driven run**: the agent stops and asks a question, there is no path by which the answer reaches it, and every retry spawns a fresh process that asks the identical question again. This is the single largest gap between DevFlow's stated goal (unattended `--yes-ship` / `--yes-release` runs) and what it can actually finish, because a plan that *correctly* gates an irreversible decision becomes a plan that can never complete unattended. Two adjacent defects in the same gate → prompt → relaunch path are folded in.

**Why this and not 999.25.** 999.25's re-attempt needs prerequisites 2–4 closed (ledger terminal state, the `v{version}` namespace collision, the `cargo info` predicate), and those are findings *inside* code that exists only on the unmerged `feature/phase-26` branch — not schedulable work items. Prerequisite 5 (W-17) is explicitly "do this last, immediately before UAT, never before". And Phase 26's own recorded lesson asks whether each irreversible step can be made independently re-runnable *before* composing them — design work nobody has done. Meanwhile 999.57 blocks 999.25 in practice regardless: plan 26-01 was two `blocking-human` checkpoints authorizing exactly the irreversible operations the executor performs, so any re-attempt hits this immediately.

**Units:**

- **28a — 999.57 (A): session resume, the primary fix.** On approving a *checkpoint* gate (a class that must be distinguished from a generic transient-error gate), relaunch as `claude -p --resume <session_id> "<answer>"` against the exact exited session, instead of spawning a fresh stage run. Gives the checkpoint protocol the live back-and-forth it was written for, and is dramatically cheaper — no CONTEXT/RESEARCH re-read, no re-running completed tasks.
- **28b — 999.57 (C): make checkpoint gates legible.** `truncate_reason` (`pipeline_outcomes.rs:318`, → `render_gate_context(reason, 300)`) caps gate context, so the operator sees `"…must select direct-push or pr-based-develop… [truncated]"` and must read `.devflow/phase-NN-stdout` by hand to find the options. A checkpoint gate is semantically different from an error gate and should render as a real menu — numbered options, verbatim, untruncated — in `devflow status` / `devflow gate show`. Independent of how the answer is transmitted; **do regardless of 28a's outcome.**
- **28c — 999.59 / DEN-84: the interview flag.** A missing `CONTEXT.md` is ambiguous — it means either "run the interview" (impossible headlessly) or "no interview wanted" (inexpressible), and Define guesses the arm that cannot succeed. Lands in `prompt.rs`'s `idempotent_stage_prompt`, the sibling of the `stage_prompt*` family 28a must give a new parameter.
- **28d — 999.60 / DEN-85: preserve an unfired `--until` cap.** `resume` clears `stopped`/`stop_reason`/`stop_until` unconditionally (`pipeline_launch.rs:226-228`), so a cap that never fired is silently discarded. Gate the clear on `state.stopped`. Same resume/relaunch plumbing 28a rewrites.

**Verified at promotion (2026-07-30, HEAD `8072ab6`) — and one entry corrected:**

| Claim | Status |
|---|---|
| `gates.rs` reads the note only for `"abort"` | ✅ confirmed, `gates.rs:74` |
| `stage_prompt*` has no answer channel | ✅ confirmed — all three signatures are `(stage, phase, project_root)` |
| `truncate_reason` caps gate context | ✅ confirmed, 300 chars |
| **`session_id` is available for 28a** | ⚠️ **999.57's entry overstates this.** It says the id was "observed live", implying availability. In the codebase `session_id` appears **only inside a test fixture string** (`agent_result.rs:1362`) — never parsed, never stored, never persisted. The Claude adapter has **no `--resume` support**. |

**Scope consequence of that correction:** 28a is not "thread an existing value through". It needs (i) capture `session_id` from the Claude result envelope, (ii) persist it on per-phase state, (iii) add `--resume` to the Claude adapter, (iv) add the checkpoint-vs-error gate classification. Size **M**, and the planner should not inherit the entry's more optimistic framing.

**999.57 part (B) is deliberately NOT in scope.** The structured `{plan}-CHECKPOINT-ANSWERS.json` fallback exists so the design does not *depend* on every agent having a resume primitive — but whether Codex/OpenCode need it is unknown until (A) proves the shape, and part of its wiring lives in the GSD skill layer outside this repository. Build only if (A) demonstrates the need.

**Do not fix 28a by auto-approving.** `checkpoints.md` is explicit that `gate="blocking-human"` is never bypassed in any mode, and Phase 26's run proved why: a plan had mistakenly tagged two irreversible authorizations `gate="blocking"` (auto-bypassable, auto-selects the *first* option), which would have silently authorized `cargo publish` with no human input. The gate is correct; only the return path is missing.

**Explicitly excluded, with reasons — do not re-add without revisiting:**

- **999.31 / Modular Agent Driver (High, Size L).** Genuinely adjacent — it reworks the adapter layer, exactly where 28a adds `--resume`. Excluded because folding an **L** into an M+S+S cluster recreates the multi-domain scope creep Phase 26 was built to avoid, and Phase 26's lesson was that oversized phases hide defects. Better sequenced *after*, inheriting a working `--resume` as a known-good case.
- **999.28 / `--base` override (Medium, M).** Touches `start` but not the gate/prompt/relaunch path. Adjacent by file, unrelated by concern.
- **999.61 / DEN-86** (four residual spawn edges) — unrelated mechanism; its own small phase.

**Scope narrowed during discussion (2026-07-30) — `28-CONTEXT.md` is authoritative over this entry's original framing.** The goal that survived is *"recognize a checkpoint correctly, and let DevFlow resolve it unattended"*, not *"get an operator's answer back to the agent"*: no usable notification/response channel exists to build a human-answer path on (D-08/D-09), and building one is explicitly deferred (D-11). Consequently **28b was repurposed** from "render checkpoint gates as a legible menu" to "keep a durable audit record of what was auto-decided" (D-07), and **28c's framing changed** from "add an interview flag" to "delete the branch that runs the interview headlessly" (D-14). A fifth item, a persistent `yes_ship` config option (D-12/D-13, a deliberate reversal of Phase 23's D-05), was added during the discussion.

**Requirements**: TBD — no REQ-IDs; tracked by backlog identifier (`999.57`, `999.59`, `999.60`) plus `28-CONTEXT.md` decision IDs (`D-01`…`D-15`), consistent with Phases 21/22/26/27.
**Depends on:** Phase 27 (no code dependency; sequencing only)
**Plans:** 6/6 plans executed

**Capacity note, recorded rather than assumed.** Three items in one phase is what Phase 26 attempted, and Phase 26 did not ship. The mitigating difference: all four units share one narrow mechanism (gate → prompt → relaunch), none drives an irreversible operation, and 28b/28c/28d are each **S**. If scope pressure appears during planning, drop **28c** first (it is the most independent), then **28d**; 28a+28b are the phase's reason to exist.

Plans:
**Wave 1**

- [x] 28-01-PLAN.md — wave 1 · tracer: probe the checkpoint-recognition seam against a live headless run, then encode D-01's static PLAN.md scan

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 28-02-PLAN.md — wave 2 · capture the Claude session id and confirm a reported checkpoint from stdout; persist both new `State` fields (D-01/D-04)
- [x] 28-04-PLAN.md — wave 2 · Define must never invoke the interview headlessly; delete the branch, keep Plan's intact (999.59 / D-14)
- [x] 28-05-PLAN.md — wave 2 · `resume` must not clear an unfired `--until` cap (999.60 / D-15)
- [x] 28-06-PLAN.md — wave 2 · persistent `yes_ship` config option with a never-silent notice; CLI flag still wins (D-12/D-13)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 28-03-PLAN.md — wave 3 · the resume primitive, the bounded relaunch path, the `checkpoint_auto_decided` audit record, and the dispatch guard (D-03/D-04/D-05/D-07)

### Phase 29: Release-Cut Executor — Observe, Then Act Within the Repo's Rules

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

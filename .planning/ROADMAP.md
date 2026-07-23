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

### Phase 999.1: Hermes Support (BACKLOG)

**Goal:** `HermesAgent` adapter with native-envelope completion parsing, rewrite of the stale `skills/hermes/devflow/SKILL.md`, and the Hermes plugin session mode with an events.jsonl-driven gate watcher. Held Phase 18's slot until 2026-07-20, when pipeline-reliability work took priority — personal-infrastructure work that doesn't gate anything else.
**Priority:** Low | **Size:** L — reviewed 2026-07-21: structurally lowest (gates nothing else), operator confirmed still low priority. Linear: DEN-26.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.2: A Phase Tracks Exactly One Process (BACKLOG)

**Goal:** One `phase-N-agent-pid` file per phase leaves the monitor unrecorded and `sequentagent`'s second agent homeless. Frame as two tracked processes per phase. *(was 19b)*
**Priority:** Medium | **Size:** M — reviewed 2026-07-21: the "monitor unrecorded" half is now fixed by Phase 18's 18b (`State.monitor_pid` shipped in v1.5.0); remaining scope is narrower — `sequentagent`'s orphaned second process only. Re-scope before promoting. Linear: DEN-27.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.3: CLI Operator Discoverability (BACKLOG)

**Goal:** Gate reasons truncate with no `devflow gate show`; rate-limit reset times buried in raw JSON; `status` lacks in-stage progress; recovery verbs undiscoverable from a stuck state. *(was 19c)*
**Priority:** Low | **Size:** L — reviewed 2026-07-21: confirmed still true at HEAD (no `gate show` command exists). Self-scoped as UX, safe behind correctness work; bundles 4 distinct gaps — split into smaller issues when promoted. Linear: DEN-28.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.4: Version-Tag Contention on Concurrent Ship (BACKLOG)

**Goal:** Two phases computing the same next version race to create one tag. 17-09 bounded the test-level symptom (2s gate-timeout poll under `ENV_MUTEX`); the product-level race is proven (instrumentation caught both phases ~1.8ms apart) but still open. *(was 19h)*
**Priority:** Medium | **Size:** M — reviewed 2026-07-21: confirmed still open (no new checkout-lock serialization since 17-09). Real but low-frequency (needs two concurrent ships landing on the identical computed version); sizing skews up on verification difficulty, not code volume. Linear: DEN-29.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.5: ChangelogAppend Placeholder Content (BACKLOG)

**Goal:** Every generated changelog entry reads "Released phase via DevFlow" — deferred twice already (17-10, 17-12). *(was 19j)*
**Priority:** Low | **Size:** M — reviewed 2026-07-21: confirmed still generic (`ship.rs:431`). Cosmetic by its own admission, but sized M not S — needs a real content source designed (plan diffs? SUMMARY.md extraction?) before implementation, which is why it's been deferred 3 times already. Linear: DEN-30.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

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

### Phase 999.14: Doctor Reconciliation for Planning-Doc Staleness (BACKLOG)

**Goal:** `devflow doctor`'s 18a reconciliation checks phase state against events/PIDs/gates/branches, but nothing checks whether `ROADMAP.md`/`STATE.md`'s own narrative still matches reality once a phase's outcome is decided by a manual, out-of-band action (merge, tag, publish). Found 2026-07-21: `STATE.md`/`ROADMAP.md` claimed Phase 18 was "not yet merged / released" after v1.5.0 had already shipped — the same class of bug `17-REVIEW.md` WR-06 already named once (19e/19f marked open after `17-13` had already closed them).
**Priority:** Medium | **Size:** M — added 2026-07-21. Detection-only scope (flag stale version claims against git tags), deliberately not auto-correcting prose. Linear: DEN-39.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

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

### Phase 999.25: Release-Cut Executor (`devflow release` that executes) (BACKLOG)

**Goal:** A `devflow release` that *executes* the full release cut — version-bump PR → merge to `main` → signed tag → sync `develop` → publish `devflow-core` then `devflow` to crates.io — not just the read-only preflight. Phase 20's 20d (DEN-38) delivers `--check` only; Phase 20 CONTEXT.md D-03 locked that scope and recorded this executor as the follow-up.
**Priority:** High | **Size:** L — drives irreversible operations (squash-merge to `main`, signed tag, a crates.io publish that can never be un-published or reused), so it needs its own discuss-phase design pass on failure/rollback semantics (tag lands but publish fails; core publishes but cli does not). Blocks on Phase 20's 20a (self-pin) and 20d (`--check`): the executor's preflight step *is* 20d's check and its `VersionBump` step inherits 20a's correctness. Source: Phase 20 D-03 (2026-07-22). Linear: DEN-50 (blocked by DEN-49, DEN-38).
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.26: `devflow parallel` Git Object-Store Race (BACKLOG)

**Goal:** Confirm-or-refute whether `devflow parallel`'s concurrent per-worktree commits can hit the same git object-store corruption seen in Phase 20's 20b instance 2 (`invalid object` mid-commit-loop, a fsync-ordering flake fixed fixture-side per D-05), and fix it at the product level if the race is real. 20-RESEARCH.md assumption A1 flagged the analog as plausible but unconfirmed — `devflow parallel` has no DevFlow-level lock serializing its concurrent commits.
**Priority:** Medium | **Size:** M — low likelihood but high severity: if the product shares the hole, the next occurrence is a corrupted user repo with an opaque `invalid object` error, not a re-runnable red CI job. Dominated by a deliberate reproduction attempt (a code read can't settle it); the fix if needed is bounded. Relates to 999.4 / DEN-29 (concurrent-ship contention — same concurrency family). Source: Phase 20 D-08 / 20-RESEARCH A1 (2026-07-22). Linear: DEN-51.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.27: `release --check` Signing-Key Inline Classification (BACKLOG)

**Goal:** `check_ssh_signing_viability` (20d, `crates/devflow-core/src/git.rs`) misclassifies an inline (non-path) `user.signingkey` value — a literal key blob configured directly rather than as a file path is treated as a path and reported as not-found. Deterministic edge case; every path-based and no-key branch is already correct and tested. Full detail in `.planning/phases/20-release-correctness-operator-control/20-REVIEW.md` (INF-01).
**Priority:** Low | **Size:** S — single classification branch + one test; found by Phase 20 code review (2026-07-23), deferred as Info-severity while CR-01/CR-02 + WR-01/02/03 were fixed inline on the phase-20 branch. Linear: DEN-52.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.28: Explicit `--base` Branch Override for `devflow start` (BACKLOG)

**Goal:** Add an explicit `--base <branch>` flag to `devflow start` (default `develop`) so an operator can cut `feature/phase-NN` onto a base other than `develop` — chiefly an unmerged predecessor phase branch, to honor a `depends_on` chain and stack dependent phases. Keep the default `develop`; do **not** implicitly base on the operator's current branch (base must be explicit, never inferred from shell state).
**Priority:** Medium | **Size:** M — base is hardcoded to `develop` (`crates/devflow-core/src/git.rs:54`) and the hardcode is load-bearing for `ship` (Merge→develop→VersionBump) and `parallel` (develop-rooted shared base), so `--base` must thread through launch, and the ship/merge-target semantics for a non-`develop` base need a design pass. The gap: the ROADMAP encodes 22→21→20 but no phase can build on an unmerged predecessor. Source: Phase 21 dogfood-launch design discussion (2026-07-23). **Reassigned to Phase 22** (concurrency/stacking value). Linear: DEN-53.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.29: Dogfood Staleness Guard False-Positives on Docs-Only Commits (BACKLOG)

**Goal:** Make `enforce_build_staleness`'s commit-ancestry arm **content-aware** so a self-dogfood run is not hard-blocked when the only commits ahead of the binary's embedded commit changed nothing the compiler sees (`.planning/` docs, etc.). `embedded_commit_is_stale` (`crates/devflow-cli/src/staleness.rs`) returns `Stale` on *any* strict-ancestor HEAD (verified live in Phase 21: binary at `7163347`, worktree HEAD `3a17381`, delta = `.planning/*` only, yet hard-blocked), whereas the dirty-tree arm was already narrowed to `affects_compiled_binary` in 17-10. Apply the same filter to the ancestry arm: `git diff --name-only <embedded> HEAD` → if no build-affecting file changed, `Fresh`. Also fix the block message ("is not an ancestor of HEAD" is wrong for the common case where it *is* an ancestor, just behind).
**Priority:** High | **Size:** S — a false-positive hard-block on DevFlow's own primary workflow (dogfooding commits docs constantly, re-arming the block after every build); the fix is a targeted narrowing with direct precedent (17-10) plus a mixed-range test (docs + a `.rs` change must still block, preserving the Phase 16 false-evidence protection). Retires the `[[feedback-dogfood-rebuild-before-revalidate]]` workaround. Source: Phase 21 dogfood run (2026-07-23), observed live. **Folded into Phase 21 as unit 21d.** Linear: DEN-54.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 21: Operator Legibility & Observability

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

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 21
**Plans:** 0 plans

Plans:

- [ ] TBD (run /gsd-plan-phase 22 to break down)

### Phase 23: Test Suite & CI Hardening

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 22
**Plans:** 0 plans

Plans:

- [ ] TBD (run /gsd-plan-phase 23 to break down)

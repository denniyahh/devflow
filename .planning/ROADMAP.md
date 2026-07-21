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
| 18 | Dogfood Reliability Hardening | In Progress|

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
**Plans:** 3/7 plans executed

Plans:

- [x] 18-01-PLAN.md — 18a: `devflow doctor` project-aware reconciliation (wave 1)
- [x] 18-02-PLAN.md — 18g: WR-03 test stabilization, assertion placement (wave 1)
- [x] 18-03-PLAN.md — 18b: persist and probe `monitor_pid`, representable "stuck" state (wave 2)
- [ ] 18-04-PLAN.md — 18d: make `MAX_CONSECUTIVE_FAILURES` reachable for the Code↔Validate loop (wave 3)
- [ ] 18-05-PLAN.md — 18e: Layer 0/Validate verdict reconciliation + three-way outcome (wave 4)
- [ ] 18-06-PLAN.md — 18c: evaluate build staleness against the worktree HEAD (wave 5)
- [ ] 18-07-PLAN.md — 18f: preflight gate approval skips the adjudicated check, bounded (wave 6)

## Backlog

Unsequenced items — not part of the active phase sequence. Promote with
`/gsd-review-backlog` when ready; each carries accumulated context in its
own `phases/999.N-*/CONTEXT.md`.

### Phase 999.1: Hermes Support (BACKLOG)

**Goal:** `HermesAgent` adapter with native-envelope completion parsing, rewrite of the stale `skills/hermes/devflow/SKILL.md`, and the Hermes plugin session mode with an events.jsonl-driven gate watcher. Held Phase 18's slot until 2026-07-20, when pipeline-reliability work took priority — personal-infrastructure work that doesn't gate anything else.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.2: A Phase Tracks Exactly One Process (BACKLOG)

**Goal:** One `phase-N-agent-pid` file per phase leaves the monitor unrecorded and `sequentagent`'s second agent homeless. Frame as two tracked processes per phase. *(was 19b)*
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.3: CLI Operator Discoverability (BACKLOG)

**Goal:** Gate reasons truncate with no `devflow gate show`; rate-limit reset times buried in raw JSON; `status` lacks in-stage progress; recovery verbs undiscoverable from a stuck state. *(was 19c)*
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.4: Version-Tag Contention on Concurrent Ship (BACKLOG)

**Goal:** Two phases computing the same next version race to create one tag. 17-09 bounded the test-level symptom (2s gate-timeout poll under `ENV_MUTEX`); the product-level race is proven (instrumentation caught both phases ~1.8ms apart) but still open. *(was 19h)*
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.5: ChangelogAppend Placeholder Content (BACKLOG)

**Goal:** Every generated changelog entry reads "Released phase via DevFlow" — deferred twice already (17-10, 17-12). *(was 19j)*
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.6: Plan-Only Pipeline Mode (BACKLOG)

**Goal:** `devflow start --until <stage>` to halt cleanly after a named stage. Today `start` always runs Define→Plan→Code→Validate→Ship and `--mode supervise` only moves the gates, so "just do the planning" is inexpressible — the only stop is killing the monitor, which strands state and orphans a worktree. Blocks cheap, frequent dogfood runs. Found 2026-07-20 attempting to plan Phase 18 through devflow itself.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.7: Manual Ship Override (BACKLOG)

**Goal:** Let an operator drive a phase through Ship by hand when the pipeline is unhealthy. `devflow gate approve` does not cover this: it refuses when no gate is open (`gates.rs:186`), and when one is open it only writes a response file that a *live monitor* must consume — so a dead monitor (invisible today, see 18b) leaves the approval unconsumed forever. Must not bypass the fail-closed terminal Ship invariant. Operator request 2026-07-20.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.8: Split `main.rs` (BACKLOG — blocked on Phase 18)

**Goal:** `crates/devflow-cli/src/main.rs` is 6,239 lines (3,307 production + a 2,931-line test module with 71 tests) — 2.6x the next largest file, and now the binding constraint on execution parallelism, since the same-wave zero-file-overlap rule keys on file path. Phase 18 was forced into 6 near-serial waves for 7 plans purely because 6 of them touch `main.rs`. The production half already decomposes cleanly into 7 clusters (preflight / staleness / pipeline state machine / commands / parallel / dispatch / config — measured boundaries in CONTEXT.md); splitting would take those 6 waves to 3.

**Deliberately sequenced AFTER Phase 18, not before.** The primary risk is `ENV_MUTEX` (22 sites in `main.rs`) — redistributing 71 tests across module boundaries while preserving process-global serialization is exactly the failure class with the worst track record here (19i hit 2/2 in CI while passing locally; GAP-2 at 33–40%; 999.4 caught only by instrumentation). Phase 18's 18a/18b are what make that class observable, and 18e/18f reshape the very functions that determine the seams. Must be a pure-move refactor with zero behavioral change, verified on a branch with CI — local-green is explicitly insufficient.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready — after Phase 18 ships)

### Phase 999.10: `.devflow/` Artifact Hygiene (BACKLOG — highest of the WR batch)

**Goal:** Two composing 17-REVIEW.md findings, both re-verified present at HEAD 2026-07-20. **WR-01:** `docs_update` (`hooks.rs:184`) is the only remaining `commit_all` caller and runs `git add .` at the *user's* project root, so a target project whose `.gitignore` lacks `.devflow/` gets raw unredacted agent stdout swept into a commit that `Merge` then pushes — the assumption that `.devflow/` is gitignored is asserted in test fixtures but enforced nowhere, and both existing guards only cover DevFlow's own repo. **WR-02:** `main.rs:843` emits the full `current_exe()` path into `events.jsonl` on every start, i.e. the developer's absolute home directory and OS username, in a file `OPERATIONS.md` tells people to tail and paste. Together they publish PII into someone else's git history. Phase 18 does **not** fix either — its plans cite WR-02 only as a prevention constraint. Preferred WR-01 fix (`lock::ensure_devflow_dir` writes a `.devflow/.gitignore` containing `*`) closes it for every constructor at once.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready — before any wider release push)

### Phase 999.11: `commit_path` Empty Commits (BACKLOG)

**Goal:** 17-REVIEW.md WR-03, re-verified at HEAD 2026-07-20. `commit_path`'s `--allow-empty` does not *skip* when a path is unchanged — it **commits**, contradicting the function's own doc comment ("Ok(()) whether or not the path had changes") and rendering its `nothing to commit` guard arm dead code that reads like the skip path. If `version_bump` re-runs after a fail-fast terminal-batch retry and `write_version` produces byte-identical content, an empty `chore: bump version to X` commit lands on develop and **the release tag is placed on a commit containing nothing**. Reachable, since Phase 16 made terminal-batch retry a designed path. Fix: drop `--allow-empty` and let the existing arm become the genuine no-op.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

### Phase 999.12: Layer 0 Unapproved-Probe Veto Coverage (BACKLOG)

**Goal:** 17-REVIEW.md WR-04 — coverage debt on a *deliberate* trade, not a defect. 17-03 removed `evaluate_layer0`'s `Stage::Code` guard by design (D-05 gap 1), so a forgotten `DEVFLOW_TRUST_EXTERNAL_VERIFY` now vetoes at all five stages instead of one, a 5× blast-radius increase. Two verified gaps at HEAD: (a) of the three veto arms, only "approval mismatch" is tested (`agent_result.rs:1644`) — the "not approved" arm a forgotten env var actually hits has no test at any stage; (b) `docs/guides/configuration.md` states the requirement for "the parent DevFlow process" but never that the **detached monitor subprocess must inherit it**, which is where the failure manifests. Deliberately not folded into Phase 18's 18-05 (same file) — that plan had already passed the checker clean, and adding coverage debt to a verified bug-fix plan is scope creep.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready — ideally soon after Phase 18 ships, while 18-05 is fresh)

### Phase 999.9: Dependency Update Review (BACKLOG)

**Goal:** Triggered 2026-07-20 by a GitHub Actions annotation on the first all-branch CI run — `actions/checkout@v4` targets deprecated Node.js 20 and is being force-run on Node 24. Warning only, all jobs green, but it appears on 4 job definitions across both workflow files, so the eventual break lands everywhere at once. Broader than a one-line bump: the dependency surface is inconsistently pinned — `dtolnay/rust-toolchain@stable` and `rust-toolchain.toml`'s `channel = "stable"` float entirely (CI can break from upstream with no commit here, a reproducibility gap for a project premised on trustworthy pipelines), `devcontainers/ci@v0.3` is pre-1.0, the devcontainer base image pin was last verified in Phase 15, and neither `cargo audit` nor `cargo deny` runs in CI. Deliberately not folded into Phase 18 — a dependency bump mid-phase would confound that phase's test signal.
**Requirements:** TBD — see CONTEXT.md
**Plans:** 0 plans

Plans:

- [ ] TBD (promote with /gsd-review-backlog when ready)

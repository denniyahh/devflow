---
gsd_state_version: 1.0
milestone: v2.0.0
milestone_name: milestone
status: In progress
stopped_at: "Phase 15 shipped — PR #7 (feature/phase-15 → develop)"
last_updated: "2026-07-17T21:18:30.502Z"
progress:
  total_phases: 5
  completed_phases: 2
  total_plans: 27
  completed_plans: 22
  percent: 40
---

# DevFlow — Project State

> Last updated: 2026-07-16

## Active

- **Phase 15 (15a Complete / 15b Scoped):** Dogfood Enablement + OSS Readiness — rescoped 2026-07-16 dogfood-first. 15a COMPLETE: `devflow gate list/approve/reject`, OPERATIONS.md, `.devflow.yaml` decoy removed, lib.rs examples fixed, `--help` snapshot guard; exit criterion verified live (full phase, gate answered only via CLI). 15b: OSS packaging (README/ARCHITECTURE/CONTRIBUTING/devcontainer/crates.io publish), to be run through DevFlow as the first post-MVP dogfood — `devflow start --phase 15 --agent claude --mode auto`. Antigravity adapter deferred to unscheduled backlog
- **Phase 16 (Scoped):** Hermes Support — split out of Phase 14 (2026-07-16); HermesAgent adapter, skill-file rewrite, Hermes plugin. Depends on Phase 14's events.jsonl + Phase 13's notify hook

## Completed

| Phase | Description | Version | Date |
|---|---|---|---|
| 0 | Codebase map + Assessment | — | 2026-06-17 |
| 1 | CI Foundation + Test Coverage | — | 2026-06-17 |
| 2 | Version Bumper Expansion | — | 2026-06-17 |
| 3 | Verify & Docs Execution | — | 2026-06-17 |
| 4 | Hermes Skill | — | 2026-06-17 |
| 5 | Agent Trait Refactor | — | 2026-06-17 |
| 6 | Agent Completion + Ship Readiness | v0.5.1 | 2026-06-17 |
| 7 | Git Worktrees + PR Integration | v1.0.0 | 2026-06-18 |
| 8 | Docs + OSS Onboarding | v1.0.1 | 2026-06-18 |
| 9 | OSS Polish | v1.2.0 | 2026-06-18 |
| 10 | Logging + Planning Step | — | 2026-06-19 |
| 11 | GSD-Native Architecture + Remediation | v1.2.0 | 2026-06-20 |
| 12 | Bootstrap + Housekeeping | — | 2026-07-10 |
| 13 | MVP Core Loop | — | 2026-07-15 |
| 14 | Parallel Safety + Observability | — | 2026-07-16 |

*Phases 8 and 10 shipped without a SUMMARY.md at the time; both were retroactively documented 2026-07-08 (see `8-SUMMARY.md`, `10-SUMMARY.md`) after reconstruction from git history. Phase 11 was reviewed and found already adequately closed out via `11-VALIDATION.md`/`11r-VALIDATION.md` (Nyquist-compliant, sign-off dated 2026-06-20) — no retroactive SUMMARY.md was needed.*

## Blockers

None.

## Decisions

| Date | Decision |
|---|---|
| 2026-07-16 | **Phase 15 rescoped dogfood-first:** operator priority is a fully functional MVP for dogfooding. The MVP engine is done (13 + 14); the remaining friction is operational: gate responses required hand-writing `.devflow/gates/NN-stage.response.json`, and no accurate operator reference exists. Phase 15 now leads with 15a Dogfood Enablement (`devflow gate` list/approve/reject, OPERATIONS.md, plus pulled-forward accuracy items: `.devflow.yaml` decoy removal, IN-01 lib.rs rustdoc, `--help` snapshot test); 15b OSS packaging follows and is to be executed through DevFlow itself as the first post-MVP dogfood run. Antigravity adapter (old 15c) deferred to unscheduled backlog — serves neither priority. Phase 14 was merged to develop (431c743) before this rescope. |
| 2026-07-16 | **Phase 14 post-ship code review + fixes:** independent high-effort review (8 finder angles, 1-vote verification) found 10 issues — 2 critical (recover --clean wiped live sibling phases; checkout-lock timeout ran hooks unserialized), 7 warning, 1 info — all documented in `14-REVIEW.md` and resolved in `14-REVIEW-FIX.md` (7 fixed, 2 mitigated, 1 accepted-by-design). Notable policy calls: `recover --clean` now sweeps stale phases only with `--phase N` as the explicit escape hatch; a checkout-lock timeout skips the hook batch rather than ever mutating the checkout unserialized (`DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` tunable); agent binaries are preflighted before any monitor spawns. |
| 2026-07-16 | **Phase 14 complete — CR-03 closed:** per-phase `state-{NN}.json` + `advance --phase N` threaded from the monitor at spawn time (no shared singleton, pre-lock read deleted), two-level locking (per-phase advance lock + seconds-scale `lock-project` around all primary-checkout git mutation), per-phase `cron-instructions-{NN}.json`, sequentagent behind a no-advance monitor holding its phase lock (sync `launch_agent`/`capture_agent_output` deleted), `events.jsonl` schema v1, `devflow logs [--follow]`, multi-phase `status`/`recover`. Legacy `state.json`/`cron-instructions.json` migrate/read-compat on first touch. Checkout-lock acquisition in the hook path is fail-soft (warn + proceed unserialized after 120s) — a wedged sibling must not abort an advance; integrate paths fail hard instead. Validated: 252 tests, clippy/fmt clean, live two-phase e2e with both Ship gates open concurrently and both version-bump tags landing. |
| 2026-07-16 | **Phase 14 split — Hermes work (14c–e) moved to new Phase 16:** the 2026-07-14 move of Hermes into 14 was a workload-balance call made before CR-03 was deferred there (2026-07-15), which made 14 the heaviest phase instead of the slimmest. Phase 14 is now Parallel Safety + Observability, ordered 14a (CR-03) → 14b (capture_agent_output sync-path) → 14c (observability) because per-phase state files dictate what `status`/`logs`/`events.jsonl` enumerate — building observability first would mean rebuilding it. Phase 16 (Hermes Support) sits after Phase 15 so personal-infrastructure work doesn't gate OSS readiness; it depends on 14's `events.jsonl` and 13's notify hook. Dir renamed: `14-observability-hermes` → `14-parallel-safety-observability`; new `16-hermes-support` (neither 14 nor 16 had plans yet). |
| 2026-07-15 | **CR-03 follow-up deferred to Phase 14:** per-phase locks are correct, but `state.json` and main-checkout git ops stayed project-global, so `devflow parallel` remains unsafe. Fix shape (per-phase state files, phase-threaded monitor advance, coarse lock for checkout mutations) + acceptance criteria in `phases/13-mvp-core-loop/13-DEFERRED-CR-03.md`. |
| 2026-06-19 | **v2.0.0 architecture:** DevFlow is a GSD-native execution engine with gate file protocol. Two modes (full auto, supervise). State machine: Define→Plan→Code→Validate→Ship. All skip logic removed. Conventional commits permanently deprecated. |
| 2026-06-19 | **Versioning:** Hybrid Git-Based SemVer. MAJOR from project version file. MINOR = git tag count. PATCH = commit count since last minor tag. Zero human decisions per release. |
| 2026-06-19 | **Config eliminated:** No `.devflow.yaml` needed. Hardcode git-flow (main/develop/feature/), auto-detect version file, CLI flag for mode. |
| 2026-06-19 | **Hermes Plugin (Phase 13):** First-class DevFlow session mode — prevents prompt confusion, handles gate responses, surfaces state. |
| 2026-06-19 | **Phase reorganization:** Phase 10 shipped. Phase 11 refactors architecture. Bootstrap pushed to Phase 12. OSS + Hermes plugin to Phase 13. |
| 2026-06-19 | Phase 11: Git-flow — `devflow finish` (feature→develop), `devflow release` (release→main+tag), guard rails (`git_flow.enforce`). Merged into new Ship stage. |
| 2026-06-17 | Use GSD for project management going forward |
| 2026-07-08 | External code review (verified against codebase before scoping): confirmed README/ARCHITECTURE describe the pre-Phase-11 product, agent prompts are hardcoded to GSD slash commands, completion protocol conflates "stage ran" with "stage verdict," and defaults (7-day silent gate timeout, worktree opt-in with permission bypass always on) favor a personal setup over general use. Routed to Phase 13 (docs accuracy) and new Phase 14 (reliability/observability). |
| 2026-07-08 | **Reconsidering "Config eliminated" (2026-06-19):** open to reintroducing a `devflow.toml` (agent-agnostic stage/command templates, branch model) per review feedback, but deliberately **shelved** — not part of Phase 13 or 14. Revisit as its own phase when picked up. |
| 2026-07-14 | **Hermes support moved 15 → 14:** all Hermes work (HermesAgent adapter, skill-file rewrite, Hermes plugin) moved from Phase 15 to Phase 14 (retitled "Observability + Hermes Support"). Rationale: workload balance (14 was the slimmest phase) and synergy — the plugin's gate watcher and status display consume 14's `events.jsonl` and 13's notify hook, so building them in the same phase removes a cross-phase integration seam. Phase 15 retitled "OSS Readiness"; keeps Antigravity adapter, docs, dev container, contributing, crates.io publish. Dirs renamed: `14-reliability-observability-hardening` → `14-observability-hermes`, `15-oss-hermes-plugin` → `15-oss-readiness` (neither had plans yet). |
| 2026-07-14 | **MVP restructure:** priority shifted to getting the core loop (Define→Plan→Code→Validate→Ship) working end-to-end so DevFlow can be dogfooded on real projects again. Operator-confirmed scope: agents = Claude + Codex (Hermes/Antigravity deferred); gates answered via pluggable notify hook (ntfy/desktop), not terminal babysitting or Hermes plugin; MVP includes the automated Ship stage. Phase 13 repurposed as **MVP Core Loop** — claims the previously unclaimed `ship.rs` GSD-native rewrite (11h-1…4) and absorbs old-14's verdict-vs-ran split, native envelope parsing, WR-11 silent-halt fix, notify hook + configurable gate timeout, and worktree-by-default; exit criterion is a real dogfood run incl. the Full-Ship verification left BLOCKED in 12-12. Old Phase 13 (OSS + Hermes plugin) renumbered to **Phase 15** unchanged (+ actual crates.io publish). Phase 14 rescoped to pure observability (`logs`/`events.jsonl`/`status`) and now claims the previously unclaimed `capture_agent_output()` sync-path decision — both flagged-unclaimed items from the 2026-07-08 audit are now assigned. |
| 2026-07-08 | **Phase 11 closeout audit:** reviewed `11-REVIEW.md`/`11-VALIDATION.md`/`11r-VALIDATION.md`. All 5 CRITICAL findings confirmed fixed and verified. All 11 WARNING + 5 INFO findings confirmed still open in current code (spot-checked directly, none touched since Phase 11 shipped) — these were explicitly deferred to Phase 12 by `11r-CONTEXT.md` and are now scoped there (12d/12e), plus 9 untested orchestration paths (12f) and 4 never-executed manual verifications (12g) from `11-VALIDATION.md`. Two items routed to their overlapping phase instead of 12: WR-11 → Phase 14, IN-01 → Phase 13. Two items (`ship.rs` GSD-native rewrite, `capture_agent_output()` sync-path decision) remain **unclaimed by any phase** — flagged in Phase 12 CONTEXT.md, not assigned. |

- [Phase 12]: 12-09: added advance()/Ship-finish and Validate-threshold/abort terminal-path tests to close the last two 12f unit-test gaps
- [Phase 12-10]: widened shell_quote's safe set additively, documented parse_rfc3339ish's timezone-safe second-restoration, and closed the negative-UTC-offset test gap (WR-05, WR-08, 12f)
- [Phase 12]: 12-11: renamed Agent enum -> AgentKind (deleting the AgentKind=Agent alias) and adapter trait Agent -> AgentAdapter workspace-wide; removed dead State.agent_result/agent_stdout_path fields (IN-02, IN-03 closed)
- [Phase 12]: 12-12: manual-verified live Hermes gate round-trip, real Claude CLI launch+capture, and DocsUpdate fail-soft WARN visibility against the real compiled devflow binary; Full-Ship workflow recorded BLOCKED on out-of-scope ship.rs rewrite. Phase 12 (12/12 plans) complete.
- [Phase 13]: 13-01: split loop_back_to_code into prepare_loop_back_to_code (pure state mutation) + launch_stage so ReviewFailed dispatch is unit-testable without spawning the real configured agent CLI
- [Phase 13]: 13-01: non_validate_failure_fires_gate_and_hook asserts notify-hook-fired + a pure should_gate() check rather than the exact env value, since DEVFLOW_GATE_NOTIFY_CMD is process-global and races other concurrently-running gate tests
- [Phase 13]: 13-02: Made no-ship-on-Critical MANDATORY in the Ship prompt (not just review-first sequencing) so a headless run never reaches /gsd-ship's interactive optional_review step
- [Phase 13]: 13-02: Adopted the review: reason-string prefix convention (trim + case-fold) for ReviewFailed instead of a new AgentStatus enum variant, to avoid a serde-format break
- [Phase 13]: 13-03: is_error checked before DEVFLOW_RESULT marker in evaluate_layer1, so a Claude envelope's is_error: true always overrides a stale success marker
- [Phase 13]: 13-03: Codex turn.completed returns None (defers), never Success -- a marker-less turn cannot silently advance a stage
- [Phase 13]: 13-03: Layer 2 commit gate uses explicit matches!(stage, Stage::Plan | Stage::Code), not is_agent_stage(), since is_agent_stage() also includes Define
- [Phase 13]: 13-04: Retained --worktree as a hidden deprecated no-op alias for one release instead of removing it, per cross-AI review consensus (#6)
- [Phase 13]: 13-04: Computed effective worktree flag as !no_worktree in the Start match arm, leaving start()'s internal signature and parallel()/sequentagent() call sites unchanged
- [Phase 13]: 13-05: Verdict deserializer uses exact-case matching (not case-folding) per the plan's explicit fail-safe test contract.
- [Phase 13]: 13-05: Excluded Stage::Validate from the generic single-command-template prompt test (renamed it) since Validate now has its own dedicated verdict-requiring prompt, mirroring Ship's existing special-case exclusion.
- [Phase 15]: 15-01: SECURITY.md Supported Versions (v1.0.0+) already covers Cargo.toml 1.2.0 — left unchanged; DEPENDENCIES.md's "Required for Shipping" header also dropped the phantom `devflow ship` command (alongside the plan-flagged `devflow confirm`) in favor of the real gate-driven Ship flow (`devflow gate approve <phase> --stage ship`)
- [Phase 15]: 15-02: ARCHITECTURE.md full rewrite also corrected the Agent model (trait renamed `Agent`->`AgentAdapter` in 12-11; prompts are per-stage via `prompt.rs::stage_prompt`, not one shared template) and Completion evaluation's Layer 2 commit gate (scoped to Plan/Code only, not every stage) — both classified "already accurate" by 15-PATTERNS.md but found stale on direct source verification; CONTRIBUTING.md's "Adding a New Agent" section left untouched (out of files_modified scope) despite already duplicating the checklist inline with the stale trait name — flagged for a future cleanup
- [Phase 15]: 15-03: Verified devcontainer base image tag live against registry (2.0.13-1-bookworm, not stale illustrative 1-1-bookworm) and pinned devcontainers/ci action to @v0.3; CODE_OF_CONDUCT.md spot-checked and left unmodified (contact wording current)
- [Phase 15]: 15-04: Sourced canonical Apache-2.0 body from an already-vendored copy in the local Cargo registry cache (byte-diffed) after an initial from-memory reconstruction was self-caught with garbled Section 8/9 text; kept dual license per plan's locked resolution

## Roadmap Evolution

- Phase 14 split (2026-07-16): Hermes work (adapter, skill rewrite, plugin) moved out of 14 to new Phase 16 (`16-hermes-support`); 14 retitled Parallel Safety + Observability (dir `14-parallel-safety-observability`), leading with the deferred CR-03 flaw. See 2026-07-16 decision entry.
- MVP restructure (2026-07-14): Phase 13 repurposed as MVP Core Loop (dir `13-mvp-core-loop`); old Phase 13 OSS/Hermes content moved to new Phase 15; Phase 14 rescoped to observability. Later same day: Hermes work moved 15 → 14 (now `14-observability-hermes`), 15 slimmed to OSS Readiness (`15-oss-readiness`). See 2026-07-14 decision entries.
- Phase 14 added: Reliability & Observability Hardening — verdict-vs-ran split in completion protocol, native per-agent JSON envelope parsing, worktree-isolation-by-default for `start`, observability (`devflow logs`, `events.jsonl`, gate notify hook, configurable gate timeout). Scoped from external code review (2026-07-08). Extended 2026-07-08 with WR-11 (silent halt on non-Validate stage failure, from Phase 11 code review).
- Phase 13 scope extended: ARCHITECTURE.md full rewrite, `.devflow.yaml` decoy removal, `--help` snapshot CI test, Hermes skill file rewrite — added to existing 13b alongside the already-scoped README rewrite. Extended 2026-07-08 with IN-01 (stale lib.rs rustdoc, from Phase 11 code review).
- Phase 12 scope extended: publish `devflow` to crates.io (name confirmed available 2026-07-08). Fully scoped 2026-07-08 (CONTEXT.md written): bootstrap/versioning/crates.io plus Phase 11's deferred code-review debt (WR-01–10, IN-02–05), test coverage gaps, and never-executed manual verifications.
- Phase 12 and 13 given full `### Phase N:` sections in ROADMAP.md (2026-07-08) — previously only table rows, which meant `gsd-tools roadmap.analyze` could not see them as active phases (a real forensic gap found during `/gsd-progress --forensic`).

## Performance Metrics

| Phase | Plan | Duration | Notes |
|-------|------|----------|-------|
| Phase 12-bootstrap-housekeeping P09 | 15min | 2 tasks | 1 files |
| Phase 12-bootstrap-housekeeping P10 | 15min | 3 tasks | 1 files |
| Phase 12-bootstrap-housekeeping P11 | 15min | 2 tasks | 13 files |
| Phase 12-bootstrap-housekeeping P12 | n/a | 3 tasks | 0 files |
| Phase 13-mvp-core-loop P01 | 17min | 3 tasks | 3 files |
| Phase 13-mvp-core-loop P02 | 10min | 2 tasks | 2 files |
| Phase 13-mvp-core-loop P03 | 12min | 3 tasks | 1 files |
| Phase 13-mvp-core-loop P04 | 7min | 2 tasks | 2 files |
| Phase 13-mvp-core-loop P05 | 15min | 2 tasks | 3 files |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 15-oss-readiness P01 | 15min | 3 tasks | 3 files |
| Phase 15-oss-readiness P02 | 40min | 3 tasks | 3 files |
| Phase 15-oss-readiness P03 | 10min | 3 tasks | 3 files |
| Phase 15-oss-readiness P04 | 12min | 2 tasks | 1 files |

## Session

**Last session:** 2026-07-17T21:30:00.000Z
**Stopped at:** Phase 15 shipped — PR #7 (feature/phase-15 → develop)
**Resume file:** None

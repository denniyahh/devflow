---
gsd_state_version: 1.0
milestone: v2.0.0
milestone_name: milestone
status: In progress
stopped_at: Completed 13-03-PLAN.md
last_updated: "2026-07-14T20:44:36.620Z"
progress:
  total_phases: 4
  completed_phases: 1
  total_plans: 18
  completed_plans: 15
  percent: 83
---

# DevFlow — Project State

> Last updated: 2026-07-14

## Active

- **Phase 13 (Scoped):** MVP Core Loop — repurposed 2026-07-14; ship.rs rewrite + completion-protocol correctness + never-silent gates + dogfood run. Next up: `/gsd-plan-phase 13`
- **Phase 14 (Scoped):** Observability + Hermes Support — rescoped 2026-07-14 (reliability items moved to 13; claims capture_agent_output() decision; Hermes adapter/skill/plugin moved in from 15)
- **Phase 15 (Scoped):** OSS Readiness — renumbered from 13 (2026-07-14); docs/devcontainer/contributing + Antigravity adapter + crates.io publish (Hermes items moved to 14)

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

*Phases 8 and 10 shipped without a SUMMARY.md at the time; both were retroactively documented 2026-07-08 (see `8-SUMMARY.md`, `10-SUMMARY.md`) after reconstruction from git history. Phase 11 was reviewed and found already adequately closed out via `11-VALIDATION.md`/`11r-VALIDATION.md` (Nyquist-compliant, sign-off dated 2026-06-20) — no retroactive SUMMARY.md was needed.*

## Blockers

None.

## Decisions

| Date | Decision |
|---|---|
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

## Roadmap Evolution

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

## Session

**Last session:** 2026-07-14T20:43:39.070Z
**Stopped at:** Completed 13-03-PLAN.md
**Resume file:** None

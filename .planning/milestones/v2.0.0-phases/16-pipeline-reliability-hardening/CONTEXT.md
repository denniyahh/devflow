# Phase 16: Pipeline Reliability Hardening

**Status:** Scoped | **Priority:** HIGH | **Target:** TBD

> Inserted 2026-07-17, pushing the prior Phase 16 (Hermes Support) to 17.
> Surfaced entirely by dogfooding Phase 15 through DevFlow itself — not
> hypothetical, every item below traces to a specific, observed, verified
> incident during that run.

## Goal

Make the pipeline's own completion/review signals trustworthy, and make
Ship's gating review catch what it currently misses on a single pass.

## Evidence (Phase 15 dogfood run, 2026-07-17)

1. **Two Code-stage false positives**, both on plan 15-05 (crates.io
   publish, a plan with zero repo-diff as its success signal):
   - First: Layer-2 commit-count fallback claimed success off pre-existing
     commits unrelated to the actual task.
   - Second: the agent's own `DEVFLOW_RESULT: success` self-report, with no
     evidence it even attempted the work (no partial output, no summary, no
     new commits). Root cause undiagnosed — `.devflow/phase-15-stdout`/`-exit`
     were already wiped by the next stage's launch before they could be
     inspected.
   Both were only caught because an operator independently queried
   crates.io directly rather than trusting `devflow status`.

2. **Four consecutive Ship-review failures**, each legitimate and distinct
   (not repeats, not false positives):
   - Leaked runtime telemetry: `.gitignore` never updated after the 14a
     per-phase rename, so `.devflow/phase-07-stdout` (a real session blob
     with `session_id`/cost data) sat committed in git history.
   - The fix for the above was itself incomplete — missed legacy
     `.devflow/state.json`, `events.jsonl`, `.devflow/gates/`, and the same
     diff added doc claims (ARCHITECTURE.md/README.md/SECURITY.md) that
     contradicted the actual `.gitignore` state.
   - `.devcontainer`'s CI-parity `runCmd` had no `set -e`/`&&` chaining, so
     a failing `cargo build`/`test` could still report the job green.
   - Docs claimed `RUST_LOG` defaults to `info`; `tracing-subscriber`'s
     `EnvFilter::from_default_env()` actually defaults to `ERROR`.
   Each was caught by a single-pass, standard-depth `/gsd-code-review`
   invocation that runs once, at Ship, against the whole phase's
   accumulated diff — and evidently doesn't have the recall to catch
   everything in one pass.

## Scope

- **16a — External post-condition verification.** Let a PLAN.md declare a
  command DevFlow itself runs post-hoc to verify an external-action-only
  plan's real success condition (e.g. a registry-resolution probe),
  independent of agent self-report or commit-count heuristics.
- **16b — Retained per-stage capture history.** Stop clobbering
  `.devflow/phase-NN-stdout`/`-exit`/`-stderr.log` on every stage launch;
  archive the last N per phase so a false-positive self-report can actually
  be diagnosed after the fact.
- **16c — Deterministic doc-claim checker.** A grep-and-cross-reference
  tool (not an LLM judgment call) that checks every code
  identifier/flag/env-var/filename referenced in README/CONTRIBUTING/
  ARCHITECTURE/docs against actual source, runnable cheaply and early
  (even as part of Code's own self-check).
- **16d — Ship review: deep mode + multi-angle parallel review.** Replace
  the single generalist single-pass reviewer at Ship with deep-depth
  analysis plus multiple parallel finder angles (doc-accuracy cross-
  reference, security/leaked-data, CI/build correctness, external-state
  claims) merged into one REVIEW.md — mirroring the multi-angle approach
  that caught real issues in this project's own Phase 13 post-fix review.
- **16e — Incremental per-plan/per-wave review.** Run a lighter review as
  each plan/wave lands, not only once at the very end of the whole phase,
  so drift doesn't compound across waves before anything notices.
- **16f — Worktree-aware `devflow status`.** Currently reports `stage:
  idle` when run from inside the worktree DevFlow itself created for the
  active phase, because `project_root` resolves to the worktree instead of
  the main checkout where state actually lives.
- **16g — Legacy-state WARN cleanup.** Every `devflow status` call prints
  an unconditional "legacy state.json unparsable" warning with no
  self-service hint toward `devflow recover --clean`.
- **16h — Cross-attempt Ship/Code history view.** Reconstructing "what's
  been tried and fixed so far" currently requires manually diffing
  `events.jsonl` and successive `REVIEW.md` snapshots by hand.
- **16i — `.gitignore`/runtime-file CI invariant.** A deterministic test
  (not LLM review) that enumerates every `.devflow/`-writing path in source
  (`agent_result.rs`, `lock.rs`, `ship.rs`, `events.rs`, `gates.rs`, etc.)
  and asserts `.gitignore` covers it — this exact gap is what let real
  session telemetry leak into git history undetected for the length of this
  entire dogfood run, and would catch the next rename-without-gitignore-
  update at commit/CI time instead of two Ship-review cycles later.

- **16j — Verifiable operator notification.** Promoted from candidate
  2026-07-17: three gates fired during the Phase 15 ship (78-minute security
  gate, merge-approval gate, and the CLI-footgun retry) and the operator
  received no actual notice of any of them — `notify_fired` logged
  `unexpected: false` each time while delivering nothing a human saw. A
  notify channel that verifiably reaches the operator (and/or a loud
  persistent pending-gate indicator in `devflow status`), not just an event
  log entry claiming success.
- **16k — Ship terminal false positive (gate-approval advance path).**
  Promoted from candidate 2026-07-17, severity: worst signal failure of the
  dogfood run. After merge-gate approval DevFlow emitted `VersionBump
  ok=true`, `BranchCleanup ok=true`, and `workflow_finished` while: no merge
  happened (PR #7 left open, branch not an ancestor of develop); VersionBump
  mutated the PRIMARY checkout — bumping 1.2.0→1.2.183 uncommitted and
  tagging an unrelated docs commit; BranchCleanup deleted nothing; per-phase
  state was then deleted, hiding everything from `devflow status`. Phase had
  to be shipped by hand (merge 01d511c, v1.3.0). Forensic targets: the
  gate-approval advance path in ship.rs/main.rs (missing/silently-failing
  merge step), VersionBump's commit-count version against the wrong
  checkout, unconditional hook success reporting, and the related bogus
  CHANGELOG auto-entries (1.2.175/176/179).

## Explicitly Out of Scope (this phase)

- Hermes support — Phase 17.
- Antigravity adapter — unscheduled backlog.

**Depends on:** Phase 15 — SATISFIED 2026-07-17: v1.3.0 shipped (PR #7
merged as 01d511c, tag pushed). Note the ship itself required manual
completion (see 16k), which finalized this phase's scope.

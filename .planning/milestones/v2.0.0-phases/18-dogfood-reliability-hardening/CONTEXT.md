# Phase 18: Dogfood Reliability Hardening

**Status:** Scoped | **Priority:** HIGH | **Target:** TBD

> Reprioritized 2026-07-20 (operator decision): dogfooding has repeatedly
> surfaced legitimate functional bugs, and those bugs tax every subsequent
> dogfood run — so pipeline-self-correctness work displaces Hermes
> (personal-infrastructure, non-blocking) as Phase 18's content. Hermes
> moves to the backlog (`999.1-hermes-support`). Phase 19 as a fixed
> roadmap phase is eliminated; every item it carried is either absorbed
> here, confirmed already fixed, or moved to the backlog (see
> `## Backlog` in ROADMAP.md). All items below were found by dogfooding
> Phase 17 end-to-end on 2026-07-18/19 (reproduction evidence in
> `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` and `17-REVIEW.md`)
> except 18f/18g, carried over unchanged from the original Phase 18 scope.

## Goal

Make DevFlow's own supervision layer trustworthy and usable from a plain
terminal, and close the specific reliability gaps that cost real dogfood
hours or produced a proven false-green.

**Depends on:** Phase 17 (typed outcomes, build provenance).

---

## 18a — `devflow doctor` project-aware reconciliation *(was 18d)*

`doctor()` currently takes `_project_root` unused (`main.rs:2454`) and only
checks external tools/PATH. Extend it to diff state vs. events vs. live PIDs
vs. gates vs. branch ancestry and report a repair plan, mutating nothing by
default. Consumes Phase 17's typed outcomes (17b) and provenance (17d).
Sequence **before** 18b — 18b extends this reconciliation rather than
duplicating it.

## 18b — monitor liveness is unobservable ("who watches the watcher") *(was 19a)*

`monitor_pid` is emitted to `events.jsonl` but never persisted to `State`
and never liveness-checked. `devflow status` probes only `agent_pid`, so a
dead monitor renders identically to a healthy between-stages moment. Two
silent monitor deaths in the Phase 17 run cost ~4h; both were found only
via `ps`. Persist `monitor_pid`, probe it, and make "stuck — needs
`devflow resume`" a representable state.

## 18c — staleness is evaluated against the wrong tree *(was 19d)*

`enforce_build_staleness` compares the binary's embedded commit to
`project_root`'s HEAD. For a worktree-based phase the code under test lives
on the worktree branch, so a binary two hours behind that branch classifies
as `Ahead` (warn) because it is a descendant of `develop`. This is the root
cause of Round 4 CR-01: a stale binary silently ran the pre-17-12 hook batch
and re-emitted a false changelog heading. Evaluate against the worktree HEAD
when the phase has one, and **block** (not warn) a self-dogfood binary that
is behind it.

This is the enforcement mechanism for a standing dogfood rule: when a live
bug is fixed mid-run and the pipeline is sent back to re-validate, the
binary MUST be rebuilt and reinstalled at `<project_root>/target/debug/devflow`
before resuming. The monitor resolves its binary via `current_exe()` at
spawn and its wrapper hardcodes the primary path, so running a worktree
binary directly does not propagate to spawned monitors, and `cargo run`
from the primary checkout silently reinstalls a pre-fix build. Until 18c
ships this rule is manual; after 18c the gate enforces it.

## 18d — the Code↔Validate loop can never reach its own safety gate *(was 19g)*

`transition()` unconditionally resets `consecutive_failures`, so the
counter oscillates 0↔1 and `MAX_CONSECUTIVE_FAILURES = 3` is unreachable
for the exact loop it bounds. Observed live across three cycles. Under
`--mode auto` this loops indefinitely while `status` shows a healthy
alternating pipeline. Note 17-06 added the `infra_failures` reset to the
same function, which likely inherits the weakness.

## 18e — Layer 0 short-circuit makes Validate unpassable when `external_verify` is declared *(was 19k)*

17-03 removed `evaluate_layer0`'s `stage != Code` guard and added an
affirmative-success arm returning `verdict: None` (`agent_result.rs:784-796`).
`evaluate_agent_result_inner` returns immediately, so Layer 1 — the only
carrier of a verdict — is never read, and `advance()` computes
`passed = false` at Validate (`main.rs:1354-1361`). Reproduced: an agent's
explicit `verdict: pass` is discarded. Auto mode then loops Code↔Validate
**unbounded**, because 18d's counter reset defeats
`MAX_CONSECUTIVE_FAILURES`. Masked today only because no PLAN in this repo
declares `external_verify`. **Regression introduced by this phase's own
17-03.**

**Decision (2026-07-20, operator):** gate only when ambiguous, not on every
declared `external_verify`. Advance automatically when the probe passes AND
the agent's `DEVFLOW_RESULT` carries `verdict: pass` — two independent
signals agreeing. Gate for a human when they disagree, or when the probe
passes but no verdict arrived at all. Rejected: always gating on any
declared `external_verify` (correct but removes unattended operation for
the exact PLANs that declared a probe *in order to* run unattended).

## 18f — approving a preflight gate re-runs the identical failing check and wedges for 7 days *(was 19l)*

In `run_preflight` (`main.rs:796-828`), both `GateAction::Advance` and
`LoopBack` call `launch_stage` again, which re-runs `run_preflight`. Both
production checks are deterministic, idempotent predicates a gate approval
cannot change (`preflight_interactivity_check`, `preflight_gh_auth_check`).
`Gates::cleanup` deletes the operator's response first, so the second
identical failure writes a new gate and polls `gate_timeout_secs()` — 7
days — then returns an error leaving `gate_pending: true` persisted.
Distinct from the double-launch bug 17-08 fixed.

**Decision (2026-07-20, operator):** treat `GateAction::Advance` on a
preflight gate as an explicit override that SKIPS `run_preflight` entirely
— the check has already been adjudicated by the human. `GateAction::LoopBack`
keeps re-running the check, since that path means "I will fix it, then
retry" and the state may genuinely have changed. Bound the recursion
regardless as a backstop.

## 18g — WR-03 test stabilization *(was 18e)*

`parallel_creates_two_worktrees_and_spawns_two_monitors`
(`crates/devflow-cli/tests/phase7_cli.rs:184-200`) waits for live stdout
captures, runs unrelated assertions, then re-asserts the same paths — a
fast monitor can archive between the two. Fix by asserting the capture
immediately or accepting its retained-history generation. Recorded as
non-blocking debt in `16-REVIEW.md`.

## Already Resolved — Not Carried Forward

- **19i (PATH race):** resolved `96411eb`/`40dade3` before this restructure.
- **19e (`write_version` drops trailing comma) and 19f (changelog/tag
  desync with no version file):** both were already closed by 17-13
  (`12b5b98`, `e421ebd`) with RED-proven regression tests
  (`write_version_preserves_trailing_comma_in_package_json`,
  `after_ship_batch_with_no_version_file_keeps_tag_and_changelog_in_sync`).
  `17-REVIEW.md` WR-06 flagged that the old ROADMAP text still described
  them as open defects — that staleness is corrected by this restructure.

## Explicitly Out of Scope (moved to Backlog)

- Hermes support (`HermesAgent` adapter, skill rewrite, plugin) — `999.1`.
- 19b (two-process-per-phase tracking model) — `999.2`.
- 19c (CLI operator discoverability: `gate show`, in-stage progress,
  discoverable recovery verbs) — `999.3`.
- 19h (version-tag contention on concurrent ship — real race, proven via
  instrumentation, test-level symptom already bounded by 17-09) — `999.4`.
- 19j (`ChangelogAppend` placeholder content, deferred twice already) —
  `999.5`.

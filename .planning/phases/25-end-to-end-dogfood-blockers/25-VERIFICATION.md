---
phase: 25-end-to-end-dogfood-blockers
verified: 2026-07-28T18:30:00Z
status: human_needed
score: 10/10 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 8/10
  gaps_closed:
    - "25a — CR-02 (ensure_base_ref_current's Behind arm): the fast-forward write is now a compare-and-swap (git update-ref refs/heads/<base> <new> <expected_old>), and the checked-out precondition is repository-wide (base_is_checked_out_anywhere, parsed from git worktree list --porcelain, fail-CLOSED on an unreadable answer) instead of project_root-scoped. Both closed by 25-14, independently re-confirmed here by direct source read (preflight.rs:405-560) and by running both new regression tests (fast_forward_base_ref_refuses_a_stale_expected_old_value, currency_behind_refuses_when_base_is_checked_out_in_another_worktree — both `1 passed`), plus all five pre-existing currency_* arms unmodified (`10 passed; 0 failed`)."
    - "25d (surface) — CR-01 (doctor / gate sweep --reap-strays): both operator surfaces now route through one composition, unreachable_stray_candidates -> retain_unreachable_strays against registry_reachable_pids(stray_safety_roots(...)), which reads workflow::list_states' monitor_pid and lock::holder_identity's holder pid (never lock::holder, never registry::prune_missing) across registry::load_roots() unioned with the caller's own root, never narrowed by --root. Closed by 25-15, independently re-confirmed here by direct source read (commands.rs:3050-3200) and by running the two new regression tests (reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates, a_deleted_root_contributes_nothing_to_the_reachable_set — both `1 passed`), plus stray_finding_detail_states_only_what_was_checked (`1 passed`) and both operator-facing string greps (STATE-ORPHANED in main.rs: 0 matches; the old unverified-orphan clause in commands.rs: 0 matches)."
  gaps_remaining: []
  regressions: []
  note: >
    Both truths this round targeted (1 and 7) are now VERIFIED, independently, not merely on the
    executors' or the code reviewer's word — every claim in `25-14-SUMMARY.md`/`25-15-SUMMARY.md`
    and every "CLOSED" verdict in the current `25-REVIEW.md` was checked against live source and a
    live test run by this verifier. Score is a clean 10/10 on the phase's ORIGINAL 10 must-have
    truths. However, this round's third plan (25-16, WR-03 fold-in) surfaced — and this verifier
    independently confirmed — two NEW, unresolved defects (WR-05, WR-06) in that plan's own scope.
    Neither WR-05 nor WR-06 attaches to any of the phase's 10 original truths or to any of the 6
    units (25a-25f + 999.38) the ROADMAP's own acceptance paragraph names as the phase's completion
    criterion — WR-03/05/06 is ancillary test-hygiene work folded into this round voluntarily, to
    protect confidence in 25-15's own live-census regression evidence, not one of the phase's
    deliverables. Both are already recorded as OPEN items in `.planning/WINDOWS.md` (ids 1 and 2),
    which blocks `/gsd-ship` until a human fixes or waives them. That existing project mechanism,
    plus this verifier's own independent confirmation that both findings are real (not merely
    plausible), is why overall status is `human_needed` rather than a clean `passed` — a human must
    decide whether to spend a fourth gap-closure round on WR-05/WR-06 or waive them before shipping.
gaps: []
human_verification:
  - test: >
      WR-05 (confirmed real, not merely cited from 25-REVIEW.md): decide whether to fix
      `preflight.rs::run_preflight_advance_gate_launches_agent_exactly_once` and
      `run_preflight_loopback_gate_launches_agent_exactly_once` (neither calls
      `reap_spawned_monitor`, confirmed by `grep -n reap_spawned_monitor crates/devflow-cli/src/preflight.rs`
      returning zero hits) in a follow-up plan, or waive WINDOWS.md item 1 with a reason.
    expected: >
      Either a follow-up plan wires `reap_spawned_monitor(&state)` into both tests (both reach a
      real `monitor::spawn_monitor` via `run_preflight`'s internal `GateAction::Advance`
      (`preflight.rs:941`, direct `launch_stage_inner` call) and `GateAction::LoopBack`
      (`preflight.rs:946`, `launch_stage` call re-resolving the real Claude adapter) arms —
      confirmed by direct source read, not assumed from the review), or WINDOWS.md item 1 is
      explicitly waived with a stated reason, per the project's own "`/gsd-ship` blocks while
      `open_count > 0`" policy.
    why_human: >
      This is a scope/priority decision (spend a fourth gap-closure round on test-hygiene work
      outside the phase's 6 named units, vs. accept the residual and waive it), not a fact this
      verifier can resolve — the underlying fact (the leak's existence) is already independently
      confirmed by static source reading, so no further verification step would change the finding.
  - test: >
      WR-06 (confirmed real, not merely cited from 25-REVIEW.md): decide whether to harden
      `reap_spawned_monitor`'s two existing call sites (`staleness.rs:802`,
      `pipeline_launch.rs:440`) into an unwind-safe `Drop` guard, or accept the current
      plain-trailing-statement form as sufficient for now.
    expected: >
      Either a follow-up plan replaces the trailing `reap_spawned_monitor(&state)` calls with an
      RAII guard (bound immediately after the launch, before any of the panicking assertions that
      currently precede the reap call — `result.unwrap()` at `pipeline_launch.rs:423`,
      `assert!(state.monitor_pid.is_some(), ...)` at `:425-428`, `workflow::load_state(...).unwrap()`
      at `:429`, `assert_eq!(reloaded.monitor_pid, ...)` at `:430-434` in one test; `result.expect(...)`
      at `staleness.rs:777` and `assert_eq!(blocked_count, 1, ...)` at `:792-796` in the other — all
      confirmed present and all preceding the reap call by direct source read), so a future
      regression that trips one of those assertions does not also leak the process the same test
      spawned; or the residual is explicitly accepted as low-probability (these assertions are the
      tests' own success-path invariants, not expected to flap) and left as-is.
    why_human: >
      Same category as WR-05 — a scope/priority call on defensive test-infrastructure hardening,
      not a fact in dispute. The defect is real (confirmed: `reap_spawned_monitor` is a plain
      trailing statement at both sites, with multiple panicking assertions ahead of it in both
      functions) but its consequence is bounded to "a future regression's own CI run also leaks a
      process," not to the phase's shipped behavior.
deferred: []
---

# Phase 25: End-to-End Dogfood Blockers Verification Report

**Phase Goal:** Make an unattended `devflow start --phase N --agent claude --mode auto --yes-ship` run reach a completed Ship stage without a human touching it, by closing the four things that currently prevent it (a run starts on a current base — 25a; progresses through all stages — 25b; finishes with correct artifacts — 25c; a stalled run recovers without `kill(1)` — 25d), plus 25e (CI-throughput flake), 25f (docs drift), and 999.38 (PATH race).
**Verified:** 2026-07-28T18:30:00Z
**Status:** human_needed
**Re-verification:** Yes — after gap-closure round 3 (25-14/25-15/25-16), against the prior report's two remaining gaps.

## Goal Achievement

### Observable Truths

All 10 of the phase's original must-have truths (from the prior `25-VERIFICATION.md`). Truths 2-6, 8, 9, 10 received a quick regression check (source unchanged since the previous verification, confirmed by `git diff` scoping and by re-running the specific tests still applicable); truths 1 and 7 — this round's targets — received full independent re-derivation.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | 25a — a run starts on a current base ref, **safely** | ✓ VERIFIED | **CR-02 CLOSED, independently re-confirmed.** `ensure_base_ref_current`'s `Behind` arm (`preflight.rs:503-560`) now: (a) gates on `base_is_checked_out_anywhere` (`:411-433`), a repository-wide `git worktree list --porcelain` scan, exact-line-matched, fail-CLOSED (`true`) on spawn error/non-zero exit; (b) resolves both endpoints via `rev-parse --verify --quiet` and calls `fast_forward_base_ref` (`:435-449`) — a genuine 4-argument `git update-ref refs/heads/<base> <new> <expected_old>` compare-and-swap, not the prior unconditional 3-argument write. Ran both new regression tests myself: `preflight::tests::fast_forward_base_ref_refuses_a_stale_expected_old_value` → `1 passed`; `preflight::tests::currency_behind_refuses_when_base_is_checked_out_in_another_worktree` → `1 passed`. Ran `preflight::tests::currency_` as a group → `10 passed; 0 failed` (all five pre-existing `currency_*` arms unmodified). Doc comment (`:481-502`) states the compare-and-swap, the repository-wide scope, and the scan-to-swap residual window verbatim. |
| 2 | 25b — `enforce_build_staleness` is adjudicated once per run (at `start`), never re-invoked mid-run | ✓ VERIFIED | Unchanged since prior verification (no wave-1 plan this round touched `commands.rs`'s staleness call site). `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` still passes (`1 passed`, confirmed as part of this round's own regression baseline). |
| 3 | 25c (derivation) — `compute_version` derives from (reachable semver tag, conventional-commit classification), refuses on unreachable baseline, floors correctly | ✓ VERIFIED | Unchanged; no round-3 plan touches `version.rs`. |
| 4 | 25c (gate) — a major version bump opens a gate and never ships unattended, in the default (worktree) execution path | ✓ VERIFIED | Unchanged; no round-3 plan touches this code. |
| 5 | 25c (anchor) — `release_range_start`'s commit-range anchor excludes pre-release history across realistic release topologies | ✓ VERIFIED | Unchanged; no round-3 plan touches `version.rs`. |
| 6 | 25d (primitives) — bounded TERM->KILL escalation with verified death; registry-independent discovery; identity re-confirmation before signalling | ✓ VERIFIED | Unchanged in behavior — `agent.rs`'s `discover_stray_devflow_processes` body, `STRAY_MIN_AGE`, `process_age`, `terminate_and_verify`, `is_same_process` are untouched by 25-15 (confirmed: 25-15's diff to `agent.rs` is comment-only, per its own acceptance criteria and my own `git diff` read of the doc-comment-only hunk). |
| 7 | 25d (surface) — an operator using `devflow doctor` / `gate sweep --reap-strays` never has a live, registered process misreported as an orphan or destroyed | ✓ VERIFIED | **CR-01 CLOSED, independently re-confirmed.** `registry_reachable_pids` (`commands.rs:3050-3068`) reads `workflow::list_states(root).monitor_pid` and `lock::holder_identity(root, phase)` — grepped and confirmed `lock::holder` and `registry::prune_missing` appear nowhere on this path. `stray_safety_roots` (`:3100-3117`) unions `registry::load_roots()` with the caller's root, never narrows. `unreachable_stray_candidates` (`:3119-3122`) is the ONE composition both `collect_stray_process_findings` (`doctor`, `:3181-3183`) and `gate_sweep`'s stray pass (`:1174`) call — confirmed `agent::discover_stray_devflow_processes()` has no other direct production caller in `commands.rs` besides the one inside this composition and the pre-existing post-pass re-discovery note. Ran all three new regression tests myself: `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` → `1 passed`; `commands::tests::a_deleted_root_contributes_nothing_to_the_reachable_set` → `1 passed`; `commands::tests::stray_finding_detail_states_only_what_was_checked` → `1 passed`. Confirmed both corrected operator-facing strings by grep: `rg -c 'STATE-ORPHANED' main.rs` → 0 matches; `rg -c 'reachable through no registry entry, lock file, or state file' commands.rs` → 0 matches. |
| 8 | 25e — the 999.47 CI flake (cmdline-inheritance race) — human-verified against 11 observations, residual explicitly stated | ✓ VERIFIED (human-verified, carried forward) | Unchanged since prior verification; no round-3 plan touches this evidence or its supporting code. |
| 9 | 999.38 — the test-suite PATH race is de-raced | ✓ VERIFIED | Unchanged; no round-3 plan touches `staleness.rs`'s `ENV_MUTEX`/hermetic-git-read guards (25-16 only adds a trailing reap call to an existing `ENV_MUTEX`-guarded test, does not touch the guard itself — confirmed by reading `staleness.rs:759-802`, the `PATH` mutate/restore block is byte-identical). |
| 10 | 25f — CONTRIBUTING.md's release procedure and the ROADMAP/PROJECT.md versioning-policy prose no longer drift from what 25c implements | ✓ VERIFIED (human sign-off recorded, carried forward) | Unchanged; no round-3 plan touches these docs. |

**Score:** 10/10 truths verified (0 present-but-behavior-unverified, 0 failed).

### New Finding This Round (not one of the 10 truths above): WR-03's fold-in is only partially closed

25-16-PLAN.md folded WR-03 (a test-suite process leak) into this round voluntarily — not because it is one of the phase's 25a-25f/999.38 deliverables, but to protect the reliability of 25-15's own live-`/proc`-census regression evidence. Its own must-have truth claimed "Two such tests exist today... and both are covered." I independently re-verified this claim and it is **false as stated**: the plan's own Step-1 enumeration (recorded honestly in `25-16-SUMMARY.md`, not smoothed over) found **four** launch-driving test sites, not two, and two of them remain unfixed. I additionally independently confirmed a second, distinct defect in the two sites that *were* fixed. See Anti-Patterns table and Human Verification below — both are recorded in `.planning/WINDOWS.md` (ids 1, 2) as open deviations, which the project's own tooling blocks `/gsd-ship` on.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/preflight.rs::base_is_checked_out_anywhere` | 25a repository-wide checked-out predicate | ✓ VERIFIED | Present (`:411-433`), fail-CLOSED, whole-line exact match confirmed by reading (`line.trim() == needle`, not `contains`) |
| `crates/devflow-cli/src/preflight.rs::fast_forward_base_ref` | 25a compare-and-swap write | ✓ VERIFIED | Present (`:435-449`), 4-argument `update-ref` confirmed by reading |
| `crates/devflow-cli/src/preflight.rs::ensure_base_ref_current` | 25a currency probe + safe fast-forward, wired | ✓ VERIFIED | Rewired `Behind` arm confirmed; call site `commands.rs:154` unchanged (confirmed no diff to `commands.rs` from 25-14) |
| `crates/devflow-cli/src/commands.rs::registry_reachable_pids`/`retain_unreachable_strays`/`stray_safety_roots`/`unreachable_stray_candidates` | 25d CR-01 filter | ✓ VERIFIED | All four present (`:3050-3122`), composition confirmed by reading and by grep for remaining `discover_stray_devflow_processes` call sites |
| `crates/devflow-cli/src/commands.rs::collect_stray_process_findings`/`gate_sweep` stray pass | 25d operator surfaces, filtered | ✓ VERIFIED | Both route through `unreachable_stray_candidates` (`:3182`, `:1174`) |
| `crates/devflow-cli/src/test_support.rs::reap_spawned_monitor` | WR-03 shared reap helper | ⚠️ WIRED, incompletely applied | Present and correct in isolation (escalating `terminate_and_verify`, verified death, tolerates `None`) — but wired into only 2 of the 4 launch-driving test sites the plan's own enumeration found (see WR-05) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `preflight.rs::ensure_base_ref_current`'s `Behind` arm | `preflight.rs::base_is_checked_out_anywhere` | sole checked-out gate | ✓ WIRED, correctly | Confirmed: no residual `project_root`-scoped `symbolic-ref` probe alongside it |
| `preflight.rs::ensure_base_ref_current`'s `Behind` arm | `preflight.rs::fast_forward_base_ref` | resolved local SHA as `expected_old` | ✓ WIRED, correctly | Confirmed by reading `:435-449` |
| `commands.rs::doctor`/`collect_stray_process_findings` | `commands.rs::unreachable_stray_candidates` | direct call, `project_root` unioned in | ✓ WIRED, correctly | Confirmed at `:3182` |
| `commands.rs::gate_sweep`'s stray pass | `commands.rs::unreachable_stray_candidates` | direct call, explicit `--root` unioned in, never substituted | ✓ WIRED, correctly | Confirmed at `:1174`; `--root`-narrowing refused per `<resolved_decision>`, machine-wide warning line present |
| `commands.rs::registry_reachable_pids` | `lock::holder_identity` (never `lock::holder`) | pure read | ✓ WIRED, correctly | Confirmed by reading and by grep — `lock::holder`/`registry::prune_missing` absent from `doctor`'s path |
| `staleness.rs`/`pipeline_launch.rs` launch-driving tests | `test_support::reap_spawned_monitor` | trailing call, AFTER prior assertions | ⚠️ WIRED, unwind-unsafe | Confirmed present at both named sites, but confirmed (WR-06) to be a plain trailing statement preceded by 2-4 panicking assertions in both functions |
| `preflight.rs::run_preflight`'s `Advance`/`LoopBack` arms | `test_support::reap_spawned_monitor` | — | ✗ NOT WIRED | Confirmed absent (WR-05): `grep -n reap_spawned_monitor crates/devflow-cli/src/preflight.rs` returns zero hits, while both tests reach a real `monitor::spawn_monitor` via `run_preflight`'s internal recursive relaunch (`preflight.rs:941`, `:946`) |

### Data-Flow Trace (Level 4)

Not applicable — this phase produces CLI/library logic, not UI components rendering dynamic data.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 25a CR-02 fix: CAS refuses a stale expected-old value | `cargo test --package devflow --bin devflow preflight::tests::fast_forward_base_ref_refuses_a_stale_expected_old_value -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| 25a CR-02 fix: refuses when base is checked out in a second (linked) worktree | `cargo test --package devflow --bin devflow preflight::tests::currency_behind_refuses_when_base_is_checked_out_in_another_worktree -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| 25a: all five pre-existing `currency_*` arms unmodified | `cargo test --package devflow --bin devflow preflight::tests::currency_` | `10 passed; 0 failed` | ✓ PASS (run by this verifier) |
| 25d CR-01 fix: same-pass discrimination (live state-named pid + live lock-held pid + genuine orphan) | `cargo test --package devflow --bin devflow commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| 25d CR-01 fix: 999.44's originating case (deleted root) still surfaces | `cargo test --package devflow --bin devflow commands::tests::a_deleted_root_contributes_nothing_to_the_reachable_set -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| 25d CR-01 fix: `doctor`'s finding string states only checked facts | `cargo test --package devflow --bin devflow commands::tests::stray_finding_detail_states_only_what_was_checked -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| 25d CR-01 wording: old unverified-orphan claim removed | `rg -c 'reachable through no registry entry, lock file, or state file' crates/devflow-cli/src/commands.rs` | 0 matches | ✓ PASS (run by this verifier) |
| 25d CR-01 wording: `STATE-ORPHANED` claim removed from help text | `rg -c 'STATE-ORPHANED' crates/devflow-cli/src/main.rs` | 0 matches | ✓ PASS (run by this verifier) |
| WR-05 falsification check: no reap call anywhere in `preflight.rs` | `grep -n reap_spawned_monitor crates/devflow-cli/src/preflight.rs` | no output (zero hits) | ✓ CONFIRMS WR-05 (run by this verifier) |
| Full workspace regression (established by orchestrator on the merged tree; not independently re-run in full by this verifier, but every targeted test above was independently re-run and all passed) | `cargo test --workspace --no-fail-fast` | `694 passed / 0 failed` | ✓ PASS (orchestrator-reported, spot-checks above corroborate) |
| `cargo fmt --check` | `cargo fmt --check` | exit 0 | ✓ PASS (run by this verifier) |
| Debt markers in phase-touched files this round | `rg -n 'TBD|FIXME|XXX'` across `preflight.rs`, `commands.rs`, `main.rs`, `agent.rs`, `test_support.rs`, `staleness.rs`, `pipeline_launch.rs` | no matches | ✓ PASS |

### Probe Execution

Not applicable — this phase has no `scripts/*/tests/probe-*.sh` files, and none are declared in any PLAN/SUMMARY. `SKIPPED (no runnable probe artifacts)`.

### Requirements Coverage

*This project has no `.planning/REQUIREMENTS.md`; tracked by unit identifier per the phase's own convention (`25a`-`25f`, `999.38`). Not reported as a gap.*

| Unit | Backlog ID | Description | Status | Evidence |
|------|-----------|--------------|--------|----------|
| 25a | 999.51/DEN-76 | Base-ref currency, safely repaired | ✓ SATISFIED | Truth 1 — CR-02 closed |
| 25b | 999.48/DEN-73 | Staleness hoist | ✓ SATISFIED | Truth 2 |
| 25c | 999.49/DEN-74 | Version derivation + major-bump gate + anchor | ✓ SATISFIED | Truths 3/4/5 |
| 25d | 999.44/DEN-68 | Orphan process reaping | ✓ SATISFIED | Truths 6/7 — CR-01 closed |
| 25e | 999.47/DEN-72 | Flaky test dead predicate | ✓ SATISFIED (human-verified) | Truth 8 |
| 25f | (no backlog ID) | CONTRIBUTING drift | ✓ SATISFIED (human sign-off) | Truth 10 |
| 999.38 | folded in | PATH race | ✓ SATISFIED | Truth 9 |
| — | WINDOWS.md #1/#2 | WR-03 test-suite leak fold-in (not a unit; ancillary) | ⚠️ PARTIALLY SATISFIED | See Anti-Patterns and Human Verification |

**Note on `25-10-SUMMARY.md`'s absence:** confirmed intentional, not a gap. `25-10-PLAN.md`'s own frontmatter carries `status: superseded` / `superseded_by: "25-13"` and an explicit `<superseded_notice>` stating "No `25-10-SUMMARY.md` will ever be written," corroborated by `ROADMAP.md:1448/1453` ("HALTED at Task 1 Step E; SUPERSEDED by 25-13"). 15 SUMMARY files for 16 plans is the correct, by-design count.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/src/preflight.rs` | 1543-1595, 1604-1657 | **WR-05 (independently confirmed, not merely cited from `25-REVIEW.md`):** `run_preflight_advance_gate_launches_agent_exactly_once` and `run_preflight_loopback_gate_launches_agent_exactly_once` both drive a real `monitor::spawn_monitor` via `run_preflight`'s internal `GateAction::Advance` (`:941`, direct `launch_stage_inner` call) / `GateAction::LoopBack` (`:946`, `launch_stage` call re-resolving the real Claude adapter) arms, and neither calls `reap_spawned_monitor` — confirmed by `grep -n reap_spawned_monitor crates/devflow-cli/src/preflight.rs` returning zero hits. Recorded as WINDOWS.md item 1 (open). | ⚠️ Warning | Test-suite process leak, load-sensitive (masked on a fast, unloaded machine by the stubbed agent's near-instant exit and the wrapper's trailing `devflow advance` resolving to the test binary and erroring out immediately — a timing accident this verifier's own re-derivation independently confirms, not a structural guarantee). Does not affect `devflow start`'s own shipped behavior; affects future test-suite hygiene and any future live-`/proc`-census assertion's evidence quality. |
| `crates/devflow-cli/src/pipeline_launch.rs` (`:414-441`), `crates/devflow-cli/src/staleness.rs` (`:689-803`) | reap call at `pipeline_launch.rs:440`, `staleness.rs:802` | **WR-06 (independently confirmed):** `reap_spawned_monitor(&state)` is a plain trailing statement at both sites, preceded in `pipeline_launch.rs` by `result.unwrap()` (`:423`), `assert!(state.monitor_pid.is_some(), ...)` (`:425-428`), `workflow::load_state(...).unwrap()` (`:429`), `assert_eq!(reloaded.monitor_pid, ...)` (`:430-434`); preceded in `staleness.rs` by `result.expect(...)` (`:777`) and `assert_eq!(blocked_count, 1, ...)` (`:792-796`) — any one of which, on failure, unwinds the test and drops its `TempDir` guard without ever reaching the reap call. Confirmed by direct reading of both functions in full. | ⚠️ Warning | If a future regression trips one of these pre-existing assertions, the same test that spawned the monitor wrapper would ALSO fail to reap it — the CI run reports a genuine regression and manufactures a fresh orphan from the very test meant to guard against orphans. Bounded to the failure path of these two specific tests' own success-path invariants. |
| (carried forward, unresolved, out of round-3 scope) `crates/devflow-core/src/version.rs:338-349` | WR-01 | `merge-base --is-ancestor` spawn/exit-128 errors collapse into the same `false` as a genuine negative | ℹ️ Info | Unchanged from prior review; not touched by this round |
| (carried forward) `crates/devflow-core/src/test_support.rs:101,120` | WR-02 | `wait_for_exec_visibility`'s guard (ii) compares against the caller, not the actual parent | ℹ️ Info | Unchanged; no current call site is a non-parent caller |
| (carried forward) `crates/devflow-cli/src/commands.rs:3861-3895` | WR-04 | `reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age` flaky by construction | ℹ️ Info | Unchanged; not touched by 25-15 (filter interposed before this function) |

## Human Verification Required

Two items, both stemming from this round's voluntary WR-03 fold-in (25-16), both already independently confirmed as real by this verifier and both already recorded as OPEN items in `.planning/WINDOWS.md` (ids 1 and 2), which the project's own `/gsd-ship` gate blocks on until fixed or waived:

### 1. WR-05 — two `preflight.rs` tests still leak a real monitor wrapper

**Test:** Decide: fix `run_preflight_advance_gate_launches_agent_exactly_once` and `run_preflight_loopback_gate_launches_agent_exactly_once` in a follow-up plan (wire `reap_spawned_monitor(&state)` in after their existing `assert_eq!(launches, 1, ...)`), or waive `WINDOWS.md` item 1 with a stated reason.
**Expected:** Either both tests reap what they spawn, or the residual is explicitly accepted.
**Why human:** Priority/scope call (spend a fourth gap-closure round on ancillary test-hygiene work vs. accept and waive) — the underlying fact is already independently confirmed, not in dispute.

### 2. WR-06 — the two "fixed" reap call sites are not unwind-safe

**Test:** Decide: harden `pipeline_launch.rs`'s and `staleness.rs`'s reap calls into an RAII `Drop` guard bound before the panicking assertions that currently precede them (per `25-REVIEW.md`'s own sketch), or accept the current plain-trailing-statement form.
**Expected:** Either the reap survives a future assertion panic, or the residual (bounded to these two tests' own success-path invariants) is explicitly accepted.
**Why human:** Same category — a defensive-hardening priority call, not a fact this verifier can resolve differently by looking harder.

## Gaps Summary

None against the phase's 10 original must-have truths — both truths this round targeted (25a's CR-02, 25d's CR-01) are now genuinely closed, independently re-derived and re-tested by this verifier, not accepted on the executors' or the reviewer's word. All 10/10 truths verified; all 6 named units (25a-25f, plus 999.38) satisfied on their own unit-level merits, matching the ROADMAP's own stated completion criterion for this phase.

The residual is entirely inside this round's own voluntary, ancillary WR-03 fold-in (25-16): two of four launch-driving test sites remain unfixed (WR-05), and the two that were fixed are not unwind-safe (WR-06). Both are Warning-severity (test-suite hygiene, not production behavior), both are already self-disclosed by the executor (25-16-SUMMARY.md's own "delta discrepancy" and "Findings" sections, not smoothed over), both are independently re-confirmed here by direct source reading rather than trusted from either the executor's SUMMARY or the reviewer's REVIEW.md, and both are already tracked as open items in `.planning/WINDOWS.md`, which the project's own tooling blocks `/gsd-ship` on until a human resolves them one way or the other.

---

*Verified: 2026-07-28T18:30:00Z*
*Verifier: Claude (gsd-verifier)*

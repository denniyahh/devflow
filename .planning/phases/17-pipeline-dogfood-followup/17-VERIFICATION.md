---
phase: 17-pipeline-dogfood-followup
verified: 2026-07-19T01:15:04Z
status: gaps_found
score: 9/12 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "Infrastructure-outcome ceiling (state.infra_failures) bounds a STUCK LOOP, not the phase's lifetime (17b, D-08)"
    status: failed
    reason: >
      Code review CR-01 (17-REVIEW.md) is confirmed by direct source reading: `state.infra_failures` is
      incremented (saturating) in `handle_infra_outcome` (main.rs:1291) and `handle_rate_limited_outcome`
      (main.rs:1337-1341), but is NEVER reset anywhere. `transition()` (main.rs:1613-1624) resets
      `state.consecutive_failures = 0` on every successful stage transition but has no matching
      `state.infra_failures = 0` line — confirmed with `rg -n "consecutive_failures = 0"
      crates/devflow-cli/src/main.rs`, which returns exactly one hit (line 1622), and `rg -n
      "infra_failures = 0"` outside `State::new`/tests returns none. Both `state.rs:34-41`'s doc comment
      ("Consecutive infrastructure-class faults... Gates at MAX_INFRA_FAILURES") and `mode.rs:20-30`'s doc
      comment ("bounding a stuck loop to at most 5 unobserved cycles") describe a per-loop counter, but the
      implementation is a lifetime-of-the-phase counter. Consequence: a phase that hits five separate,
      well-spaced, successfully-auto-resumed rate limits or OOM kills across its Define→Plan→Code→Validate→
      Ship lifecycle (or across several Code↔Validate loop-backs) will hard-abort at the fifth occurrence
      even though every occurrence was resolved cleanly and real forward progress was made between them —
      directly contradicting D-08's stated purpose ("a higher ceiling tolerates transient cloud
      outages/OOM blips"). The unit tests that exist (`infra_ceiling_aborts_instead_of_gating`,
      `rate_limited_at_infra_ceiling_stops_resuming_and_aborts`) only exercise the literal ceiling
      comparison in isolation and never exercise a `transition()` call in between two infra faults, so they
      pass without exposing the missing reset.
    artifacts:
      - path: "crates/devflow-cli/src/main.rs"
        issue: "transition() (line 1613-1624) resets consecutive_failures but not infra_failures"
      - path: "crates/devflow-core/src/state.rs"
        issue: "doc comment on infra_failures (line 34-41) describes 'consecutive' semantics the implementation does not provide"
      - path: "crates/devflow-core/src/mode.rs"
        issue: "MAX_INFRA_FAILURES doc comment (line 20-30) describes 'bounding a stuck loop' semantics the implementation does not provide"
    missing:
      - "Reset state.infra_failures = 0 in transition() alongside state.consecutive_failures = 0 (the fix the code review itself proposes), OR explicitly redocument the field as an intentional lifetime budget and update both doc comments plus mode.rs's rationale to match — either resolution needs a decision, not silence"
  - truth: "A stale self-dogfood binary is detected and blocked before stage launch (17d, D-18/D-19, retrospective AC-2)"
    status: failed
    reason: >
      Code review WR-01 was reproduced directly (not merely re-read): a two-commit clean-tree git fixture
      was built where `embedded_commit` = the first commit and a second commit was added afterward with no
      dirty files (`git status --porcelain` empty). `git merge-base --is-ancestor <embedded_commit> HEAD`
      exits 0 in this fixture (confirmed live) because the embedded commit legitimately IS an ancestor of
      the new HEAD — this is true of ANY commit that predates HEAD on a linear history, not only the exact
      build commit. `embedded_commit_is_stale` (main.rs:855-868) therefore returns `Fresh`, and because the
      tree is clean, `tracked_source_newer_than_build` (main.rs:895-908) short-circuits to `Some(false)`
      without inspecting any file — so `combined_staleness` (main.rs:914-927) reports `Fresh` even though
      the running binary is several commits behind checked-out source. This is exactly the "committed new
      code on DevFlow's own repo, forgot to rebuild, re-ran the old binary" scenario — the single most
      common real staleness case, and the literal Phase 16 incident class (`17-DOGFOOD-RETROSPECTIVE.md`'s
      "Confirmed Finding") that 17d exists to catch. The gap is structural to the D-19 ancestry-arm design
      (ancestor-of-HEAD is not equal-to-HEAD) and is not fixed by the mtime arm, which only runs on a dirty
      tree. No prominent doc comment at the `enforce_build_staleness` call site (main.rs:979-1027)
      discloses this residual limitation, despite the code review's minimum-fix recommendation to add one.
    artifacts:
      - path: "crates/devflow-cli/src/main.rs"
        issue: "embedded_commit_is_stale (855-868) + tracked_source_newer_than_build (895-908) + combined_staleness (910-927): the ancestor-of-HEAD-but-behind, clean-tree case is Fresh, not Stale"
    missing:
      - "At minimum, a prominent doc comment on enforce_build_staleness disclosing that a clean tree whose embedded commit is a strict (non-HEAD) ancestor is NOT detected as stale"
      - "Stronger fix (per code review): compare embedded_commit == HEAD (not just ancestry) when the tree is clean, and warn (not hard-block, to avoid alarm fatigue) when they differ but the embedded commit is still an ancestor"
  - truth: "A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before the stage launch (retrospective AC-4, D-14)"
    status: partial
    reason: >
      Two of AC-4's four sub-conditions are implemented and verified: plan interactivity (scoped to
      AgentKind::Codex, via preflight_interactivity_check) and Ship-scoped `gh auth status` credential
      validity (preflight_gh_auth_check). The other two are NOT implemented as functioning checks in this
      phase: (1) "missing required security artifact" has no concrete check at all — the plan's own review
      dispositions record this as DEFERRED because "D-14 (b) names no concrete artifact path/key for Phase
      17"; (2) "unavailable reviewer" ships only as an AgentAdapter::preflight trait default (Ok(())) plus a
      TEST-ONLY adapter demonstrating the empty/non-empty boundary — no built-in adapter (Claude/Codex/
      OpenCode) populates or checks a reviewer set, so no real DevFlow run is protected by this check today.
      This is a disclosed, reasoned scope narrowing (Plan 05's "Review dispositions", consensus #6), not a
      hidden defect, but ROADMAP.md's own Requirements line ties Phase 17 to "acceptance criteria 2, 3, 4"
      without noting AC-4 was narrowed, and no later-phase ROADMAP goal/success-criteria text explicitly
      commits to the security-artifact sub-check (Phase 18's goal text covers Hermes adapter/SKILL.md/session
      mode + 18d/18e only).
    artifacts:
      - path: "crates/devflow-cli/src/main.rs"
        issue: "run_preflight/generic_preflight_checks implement interactivity + gh-auth only; no security-artifact check exists"
      - path: "crates/devflow-core/src/agents/mod.rs"
        issue: "AgentAdapter::preflight is a trait hook only; no built-in adapter enforces a non-empty reviewer set"
    missing:
      - "Either implement a minimal security-artifact preflight check against a concrete artifact path/key, or record an explicit accepted-scope decision (override) narrowing AC-4 and update ROADMAP.md's Requirements line to match"
      - "A committed destination (Phase 18 or later) for real reviewer-set enforcement, if it is to remain deferred"
---

# Phase 17: Pipeline Dogfood Follow-Up Verification Report

**Phase Goal:** Close the pipeline-reliability holes the Phase 16 dogfood exposed — `Unknown` completion
must never auto-advance a stage (17a), typed agent outcomes with a deterministic retry policy (17b), a
preflight readiness gate that fails before agent time is consumed (17c), and build provenance in
`workflow_started` so a stale self-dogfood binary is detectable (17d).

**Verified:** 2026-07-19T01:15:04Z
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `Unknown` completion never auto-advances any stage (17a, D-01/D-06) | ✓ VERIFIED | `outcome_policy::decide_action` maps `Unknown -> GateReview` for every stage, exhaustive match with no wildcard arm (`outcome_policy.rs:38-56`); `advance()` dispatches on it (`main.rs:1237-1272`); test `code_unknown_does_not_transition_to_validate` passes |
| 2 | A legitimately external-only stage with zero commits and a passing approved probe completes cleanly, without weakening `TRUST_EXTERNAL_VERIFY_ENV` (17a, D-02/D-03/D-05) | ✓ VERIFIED | `evaluate_layer3`'s zero-commit branch reclassified to `Failed` (`agent_result.rs:658-693`); `evaluate_layer0`'s stage restriction removed, all-pass branch returns `Success` (`agent_result.rs:720-793`); PLAN discovery reads `project_root`, probe execution reads `execution_root`; approval-mismatch veto branches read unchanged; `cargo test -p devflow-core agent_result::` green |
| 3 | Typed outcomes `ResourceKilled`/`AgentUnavailable` exist with word-boundary-preserving wire names and route through a fail-closed exhaustive policy table (17b, D-07/D-11/D-12) | ✓ VERIFIED | `AgentStatus::as_wire_str()` pinned to serde form for all 6 variants (`agent_result.rs:73-83`); `evaluate_layer2` classifies exit 137/127; `decide_action` exhaustive match, no wildcard arm |
| 4 | Infrastructure-outcome ceiling (`state.infra_failures`) bounds a **stuck loop**, not the phase's lifetime (17b, D-08) | ✗ **FAILED** | Confirmed by source read: `infra_failures` is incremented in two places (`main.rs:1291`, `main.rs:1337-1341`) but reset nowhere — `transition()` resets `consecutive_failures` (`main.rs:1622`) with no matching `infra_failures = 0`. Contradicts `state.rs:34-41`/`mode.rs:20-30`'s own "consecutive"/"stuck loop" doc comments (CR-01, unaddressed). See gaps. |
| 5 | `RateLimited` outcomes auto-resume via a safe, per-phase `devflow resume` path preserving saved agent/mode/stage (17b, D-09) | ✓ VERIFIED | `Command::Resume` + `resume()` (`main.rs:1122-1134`) calls only `workflow::load_state` + `launch_stage`, never `State::new`/`feature_start`/`ensure_phase_worktree`; `ship::build_single_agent_cron_instructions` embeds `devflow resume --phase N`; per-phase cron file naming (`ship.rs:66-70`) makes it safe under `devflow parallel` by construction |
| 6 | Every terminal advance decision emits structured, machine-readable evidence replacing `reason:null` (17b, D-10) | ✓ VERIFIED | `advance_evaluated` emits `status` via `result.status.as_wire_str()`, `decided_by_layer`, truncated `reason` (`main.rs:1222-1230`); test `advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed` passes |
| 7 | A preflight readiness gate runs before every stage launch, never a hard exit, and the first failing check's reason reaches the operator (17c, D-13/D-15/D-16) | ✓ VERIFIED | `run_preflight` called from `launch_stage` before `monitor::spawn_monitor` (`main.rs:1057`); `generic_preflight_checks(...).and_then(|()| adapter.preflight(state))` short-circuits on first failure (`main.rs:777-795`) and routes through `run_gate`, never `process::exit`/panic |
| 8 | A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before stage launch (retrospective AC-4) | ⚠️ **PARTIAL** | Interactivity (Codex-scoped) and Ship-scoped `gh auth status` implemented and tested; "missing security artifact" has no check at all (explicitly deferred, no concrete artifact defined); reviewer-set enforcement is a trait hook only, no built-in adapter uses it. Disclosed scope narrowing, not hidden, but AC-4 as literally written is only half-satisfied. See gaps. |
| 9 | `workflow_started` records executable and build provenance (17d, D-21) | ✓ VERIFIED | `workflow_started_payload` includes `version`/`commit`/`dirty`/`build_timestamp`/`exe_path` (`main.rs:826-839`), wired into the actual emit (`main.rs:627`); test `workflow_started_payload_carries_build_provenance` passes |
| 10 | A stale self-dogfood binary is detected and blocked before stage launch; ordinary projects only warn (17d, D-17/D-18/D-19, retrospective AC-2 — the phase's originating incident) | ✗ **FAILED** | Live reproduction: a clean-tree, linear two-commit fixture (embedded commit = ancestor, not equal to new HEAD, no dirty files) yields `git merge-base --is-ancestor` exit 0 → `Fresh`, and the mtime arm never runs on a clean tree — so `combined_staleness` reports Fresh for a binary that is genuinely stale. This is the exact "committed, forgot to rebuild" incident class 17d exists to catch (WR-01, unaddressed). `is_self_dogfood_workspace` itself (member-path scan) is correctly implemented and tested. |
| 11 | AC-1 (Phase 16's failed-Merge terminal-hook regression) still holds against final HEAD — verify only, not re-planned | ✓ VERIFIED | `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished` and `terminal_hook_failure_stops_before_branch_cleanup` both pass: `cargo test -p devflow --bin devflow` → 59 passed, 0 failed |
| 12 | Full workspace test suite, clippy, and fmt are green | ✓ VERIFIED | `cargo test --workspace` → all suites green (276 devflow-core unit tests, 2 monitor e2e, devflow-cli 59 unit tests + integration suites); `cargo clippy --workspace -- -D warnings` clean; `cargo fmt --check` clean |

**Score:** 9/12 truths verified (2 failed, 1 partial)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/outcome_policy.rs` | Pure exhaustive outcome→action policy | ✓ VERIFIED | Exists, exhaustive match, unit-tested |
| `crates/devflow-core/src/agent_result.rs` | Typed outcomes, Layer 0/3 rework | ✓ VERIFIED | `ResourceKilled`/`AgentUnavailable`, `as_wire_str`, `decided_by_layer`, Layer 0 stage-scope lift, Layer 3 split all present |
| `crates/devflow-core/src/state.rs` | `infra_failures` counter | ⚠️ ORPHANED SEMANTICS | Field exists, round-trips, defaults to 0 — but never reset, contradicting its own doc comment (see Gap 1) |
| `crates/devflow-core/src/mode.rs` | `MAX_INFRA_FAILURES` | ✓ VERIFIED (constant) / ⚠️ (rationale doc doesn't match behavior) | Constant = 5, doc comment describes "stuck loop" semantics the code doesn't provide |
| `crates/devflow-cli/build.rs` | Build provenance emission | ✓ VERIFIED | Emits `DEVFLOW_BUILD_COMMIT`/`DIRTY`/`TIMESTAMP` via `cargo:rustc-env`, absolute `rerun-if-changed`, no `[build-dependencies]` |
| `crates/devflow-cli/tests/build_provenance.rs` | Provenance env-var test | ✓ VERIFIED | Exists, asserts all three vars resolve |
| `crates/devflow-cli/src/main.rs` | `advance()` dispatch, `resume`, preflight, staleness | ✓ VERIFIED (dispatch/resume/preflight-wiring) / ✗ (staleness detection incomplete) | See Truths 4, 8, 10 |
| `crates/devflow-core/src/agents/mod.rs` | `AgentAdapter::preflight` default | ✓ VERIFIED | Default `Ok(())`, adjacency-boundary test via TEST-ONLY adapter |
| `crates/devflow-core/src/ship.rs` | `build_single_agent_cron_instructions` | ✓ VERIFIED | Emits `devflow resume --phase N`, unit-tested |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `advance()` | `outcome_policy::decide_action` | exhaustive match dispatch | ✓ WIRED | Replaces `matches!(Failed \| RateLimited)` boolean |
| `launch_stage` | `run_preflight` | called before `monitor::spawn_monitor` | ✓ WIRED | `main.rs:1057`, confirmed order in source |
| `launch_stage` | `enforce_build_staleness` | called before `monitor::spawn_monitor` | ✓ WIRED | `main.rs:1060-1067`, confirmed order in source |
| `advance()` AutoResume arm | `ship::build_single_agent_cron_instructions` | writes cron file, no blocking gate | ✓ WIRED | Confirmed via `handle_rate_limited_outcome` |
| `advance_evaluated` emit | `AgentStatus::as_wire_str()` / `decided_by_layer` | payload fields | ✓ WIRED | `main.rs:1222-1230` |
| `workflow_started` emit | `workflow_started_payload` | provenance fields | ✓ WIRED | `main.rs:627` |
| `Command::Resume` | `workflow::load_state` + `launch_stage` | no `State::new` | ✓ WIRED | `main.rs:1122-1134` |
| `transition()` | `state.infra_failures = 0` | reset alongside `consecutive_failures` | ✗ NOT WIRED | No such call exists anywhere in `main.rs` (Gap 1) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| AC-1 regression still passes on final HEAD | `cargo test -p devflow --bin devflow` (filtered to the two named tests, full binary run since Rust test filtering runs the whole binary) | 59 passed, 0 failed | ✓ PASS |
| Full workspace suite green | `cargo test --workspace` | all green (276 + 2 + 59 + integration suites) | ✓ PASS |
| Lint/format clean | `cargo clippy --workspace -- -D warnings`, `cargo fmt --check` | clean, no diff | ✓ PASS |
| Staleness gate detects the common "committed, forgot to rebuild" case | Manual two-commit clean-tree git fixture reproduction (see Truth 10) | `merge-base --is-ancestor` exits 0 → reported Fresh, is actually stale | ✗ FAIL (confirms WR-01) |
| `infra_failures` resets on successful transition | `rg -n "infra_failures = 0" crates/devflow-cli/src/main.rs` (outside `State::new`/tests) | no match | ✗ FAIL (confirms CR-01) |

### Requirements Coverage

Per-phase requirements are tracked in `17-DOGFOOD-RETROSPECTIVE.md` (P1–P4, mapped 1:1 to scope units 17a–17d per `STATE.md`'s 2026-07-18 entry) and `17-CONTEXT.md` (D-01…D-21). Retrospective acceptance criterion 1 is out of scope for re-planning (already covered by a Phase 16 regression test) but was re-verified against final HEAD per ROADMAP.md's explicit instruction.

| Requirement | Source | Description | Status | Evidence |
|-------------|--------|-------------|--------|----------|
| P1 / 17a (D-01–D-06) | Retrospective + CONTEXT | `Unknown` non-advance + Layer 0/3 rework | ✓ SATISFIED | Truths 1, 2 |
| P2 / 17b (D-07–D-12) | Retrospective + CONTEXT | Typed outcomes + deterministic retry policy | ⚠️ PARTIALLY SATISFIED | Truths 3, 5, 6 verified; Truth 4 (infra ceiling reliability) FAILED |
| P3 / 17c (D-13–D-16) | Retrospective + CONTEXT | Preflight readiness gate | ⚠️ PARTIALLY SATISFIED | Truth 7 verified; Truth 8 (AC-4 literal text) PARTIAL — 2 of 4 sub-checks implemented, 2 disclosed-deferred |
| P4 / 17d (D-17–D-21) | Retrospective + CONTEXT | Build provenance + stale-binary detection | ⚠️ PARTIALLY SATISFIED | Truth 9 (provenance recording) verified; Truth 10 (stale-binary detection, the phase's originating incident) FAILED |
| AC-1 (criterion 1) | Retrospective | Failed-Merge terminal contract, verify only | ✓ SATISFIED | Truth 11 |
| D-01…D-21 | 17-CONTEXT.md | All 21 numbered decisions | See individual truths above and the 17-REVIEW.md cross-reference below | Every decision has at least one corresponding must-have; no orphaned decision found |

No requirement IDs from PLAN frontmatter (`17a`/`17b`/`17c`/`17d`) are orphaned — all four appear in at least one plan's `requirements:` field and are covered above.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/src/main.rs` | 1613-1624 (`transition`) | Missing `infra_failures` reset alongside `consecutive_failures` reset | 🛑 Blocker | CR-01: premature phase aborts for infra faults spread across a phase's lifetime (see Gap 1) |
| `crates/devflow-cli/src/main.rs` | 855-927 (staleness helpers) | Ancestry-only detection on clean trees; documented D-19 design has an unacknowledged false-negative | 🛑 Blocker | WR-01: the phase's originating incident class is not reliably caught (see Gap 2) |
| `crates/devflow-cli/src/main.rs` | 826-839 (`workflow_started_payload`) | `build_timestamp`/`dirty` emitted as JSON strings, not number/bool | ℹ️ Info | IN-03 in 17-REVIEW.md; downstream (Phase 18 18d) consumers must parse; not a functional bug |
| `crates/devflow-core/src/agent_result.rs` | 704-796 (`evaluate_layer0`) | Every stage now gates on any phase-wide `external_verify` declaration, including the Plan stage that authored it | ℹ️ Info | IN-02 in 17-REVIEW.md; very likely intentional (security property) but untested end-to-end via `advance()` |
| `crates/devflow-cli/src/main.rs` | 939-956 (`is_self_dogfood_workspace`) | String-scan of `Cargo.toml` `members` array is fragile against a future glob (`crates/*`) | ℹ️ Info | IN-01 in 17-REVIEW.md; not exploitable on the current manifest |

No `TBD`/`FIXME`/`XXX` debt markers were found in any file this phase modified.

### Human Verification Required

None. Both blocking gaps (infra-failure reset, staleness ancestry gap) and the AC-4 partial gap are fully diagnosed via direct source reading and live reproduction — no runtime/visual/external-service ambiguity remains that would need a human to observe behavior the codebase can't already show.

### Gaps Summary

Three gaps block full goal achievement, two of them tracing to unaddressed findings in this phase's own `17-REVIEW.md` (both confirmed independently by direct source reading and, for the staleness gap, live reproduction — not accepted on the review's word alone):

1. **`infra_failures` never resets (CR-01, critical).** The counter that is supposed to bound a *stuck* infra-fault loop is actually a phase-lifetime counter, because `transition()` resets `consecutive_failures` but not `infra_failures`. This will cause premature hard-aborts on long-running phases that hit several well-spaced, successfully-resolved infra faults — the exact "false failure signal" class Phase 17 exists to eliminate, just relocated to a new counter. Both `state.rs` and `mode.rs`'s own doc comments describe "consecutive"/"stuck loop" semantics that the code does not implement, so this is also a documentation/implementation mismatch, not just a missing edge case.

2. **Self-dogfood staleness detection misses the most common real case (WR-01, confirmed by reproduction).** A clean-tree binary built from a commit that is an ancestor of (but not equal to) the current HEAD is reported `Fresh`. This is precisely the Phase 16 incident's class ("committed new terminal-hook fixes, kept running the old Homebrew-linked binary") and the retrospective's headline "Confirmed Finding" that 17d was scoped to prevent. The composite design (ancestry OR dirty-tree-mtime) structurally cannot see this case because the mtime arm only runs when the tree is dirty.

3. **Retrospective AC-4 half-implemented (partial, disclosed).** "Missing required security artifact" has no check; reviewer-set enforcement is a trait hook with no built-in adapter behind it. This was a reasoned, transparently-documented scope narrowing during cross-AI plan review (consensus #6), not a silent omission — but ROADMAP.md's Requirements line still claims AC-4 without qualification, and no later-phase roadmap text explicitly commits to closing the security-artifact half. This looks like an intentional, defensible deviation; see the override suggestion below.

**This looks intentional (Gap 3 only).** To accept the AC-4 scope narrowing as-is, add to VERIFICATION.md frontmatter:

```yaml
overrides:
  - must_have: "A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before the stage launch (retrospective AC-4)"
    reason: "D-14(b) named no concrete security-artifact path/key at planning time; cross-AI review consensus #6 deferred it and reviewer-set enforcement to Phase 18's Hermes adapter, which is the first adapter with real reviewer storage"
    accepted_by: "<human reviewer>"
    accepted_at: "<ISO timestamp>"
```

Gaps 1 and 2 are functional defects confirmed by direct code reading and reproduction, not scope decisions — they need a closure plan (`/gsd-plan-phase --gaps`), not an override.

---

_Verified: 2026-07-19T01:15:04Z_
_Verifier: Claude (gsd-verifier)_

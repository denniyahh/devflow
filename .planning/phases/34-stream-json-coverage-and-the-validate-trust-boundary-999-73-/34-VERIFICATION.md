---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
verified: 2026-08-06T10:30:00Z
status: human_needed
score: 7/8 must-haves verified
behavior_unverified: 1
overrides_applied: 0
gaps: []
behavior_unverified_items:
  - truth: "999.76's second Layer-0-in-worktree-mode call site (`verify::phase_has_blocking_human_checkpoint` in `pipeline_launch.rs`'s `Action::GateReview` arm) reads the execution root, so the plan-28-03 checkpoint auto-decide path is no longer silently dead in worktree mode."
    test: "Drive `advance()` through `Action::GateReview` in worktree mode against a phase whose PLAN declares `gate=\"blocking-human\"` and lives only in the worktree (not the main checkout), with a session id and a capture present, and assert the checkpoint is auto-decided rather than falling through to the generic failure dispatch."
    expected: "The auto-decide path fires because `phase_has_blocking_human_checkpoint` is called with the execution root, not `project_root`."
    why_human: "Confirmed by direct re-execution: reverting the call site's argument from `execution_root` back to `project_root` and re-running the full `cargo test -p devflow --bin devflow` suite (279 tests) still reports `0 failed`. No test in the repository — including the two new `verify.rs` tests, which pin the function's own root-sensitivity but never drive this call site — would catch a regression here. The code is present and correctly wired (confirmed by direct source read), but the runtime claim is unexercised by any test."
human_verification:
  - test: "Drive `advance()` through `Action::GateReview` in worktree mode with a blocking-human-checkpoint PLAN placed only inside the worktree, a session id set, and the checkpoint reported in the capture; confirm the run auto-decides rather than falling through."
    expected: "Checkpoint is auto-decided (the plan-28-03 path fires), matching the code's stated behavior."
    why_human: "No automated test exercises this call site end-to-end; a live/manual run or a new integration test is needed to convert this from an inference (root-sensitivity + correct call-site wiring, verified separately) into a demonstration."
---

# Phase 34: Stream-JSON Coverage, the Validate Trust Boundary, and Layer 0 in Worktree Mode Verification Report

**Phase Goal:** Stages join the stream-json launch path on per-stage *behavioural* evidence — what
each stage's drain-and-close interaction actually does — rather than on per-stage transport
verification, which the stage-blind argv makes vacuous. The Validate classifier carries its own
status guard instead of depending on an upstream routing decision in another crate, with the
in-source record corrected where it asserts a live defect that is not reachable. And Layer 0
external verification actually runs in worktree mode, so the arms that classifier is built around
are not inert in DevFlow's default operating shape.

**Verified:** 2026-08-06
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (mapped to ROADMAP's 7 success criteria)

| # | Truth (ROADMAP criterion) | Status | Evidence |
|---|---|---|---|
| 1 | Every widened stage carries a real production capture; every un-widened stage carries a recorded reason; delivery floor satisfied (≥1 stage newly widened or explicit escalation); stage-blind argv recorded in source; `Stage::Code` explicitly resolved (new capture vs. transcription) | ✓ VERIFIED | `STREAM_JSON_STAGES` doc comment (`pipeline_launch.rs:439-539`) names all 5 `Stage` variants by name with per-stage evidence entries; `34-evidence/{define,plan,code,validate,ship}/raw_output.jsonl` + `run.log` exist, committed, non-empty; all 5 widened (delivery floor exceeded — 5, not just 1); Code's entry explicitly states "does NOT supersede Phase 31's transcription" |
| 2 | Each widened stage names its observed `BackgroundTaskState` and the cost of a recurrence | ✓ VERIFIED | `34-evidence/DRAIN-ANALYSIS.md` has one section per stage (`Define`, `Plan`, `Code`, `Validate`, `Ship`), each naming `NeverAnnounced` and a stated recurrence cost; the file also honestly frames the central finding as a refutation (zero `background_tasks_changed` events across 1063 events despite 8 sub-agent dispatches), filed as backlog 999.83 rather than silently absorbed |
| 3 | `classify_validate_outcome`'s `Passed` arm gated on derived status structurally; all 7 `AgentStatus` variants named, no wildcard | ✓ VERIFIED | Source read confirms 0 `_ =>` arms and all 7 variants (`Success`, `Failed`, `Unknown`, `RateLimited`, `ResourceKilled`, `AgentUnavailable`, `IdleTimeout`) named by identifier in the match's status position (`pipeline_outcomes.rs:228-269`); Rust's own exhaustiveness check makes an 8th variant a compile error by construction |
| 4 | `reconcile_layer0_verdict` consults Layer 1's status before transplanting its verdict; demonstrated end-to-end with negative controls | ✓ VERIFIED | Source confirms `.filter(\|layer1\| layer1.status == AgentStatus::Success)` gate (`agent_result.rs:2202-2205`); re-ran the 4 named tests live — `layer0_verdict_graft_declines_when_layer1_status_is_not_success`, `layer0_verdict_graft_still_transplants_a_passing_layer1_verdict`, `layer0_disabled_routes_a_self_reported_failure_to_gate_review`, plus the extended fixture — all pass (`7 passed; 0 failed` on the `layer0_` selector) |
| 5 | Trust inversion recorded as reachable via the graft, not the classifier wildcard; `idle_timeout_result`'s live-guard comment untouched | ✓ VERIFIED | `reconcile_layer0_verdict`'s doc comment states the corrected finding in full, citing D-15/criteria 4/5; `idle_timeout_result` comment confirmed present, unedited (own 34-01-SUMMARY diffed it byte-for-byte against `develop`). One residual overstated comment remains at `agent_result.rs:6415` (inside a test, not production doc), explicitly disclosed as out-of-scope by 34-01's own SUMMARY — cosmetic, not a functional gap |
| 6a | Layer 0 discovers declared `external_verify` commands from the execution root (worktree-aware); a test distinguishes worktree from main-checkout discovery | ✓ VERIFIED | `agent_result.rs:2056-2057` confirmed passing `execution_root`; `external_probe_discovers_from_the_worktree_when_the_main_checkout_lacks_the_plan` and its main-checkout mirror `external_probe_discovers_from_project_root_across_every_stage_without_a_worktree` both re-ran live and pass (`2 passed; 0 failed`) |
| 6b | Second call site (`phase_has_blocking_human_checkpoint` in the `Action::GateReview` arm) reads the execution root, so the checkpoint auto-decide path is no longer silently dead in worktree mode | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Code confirmed present and correctly wired (`pipeline_launch.rs:1068-1070`); two new `verify.rs` tests confirm the *function* is root-sensitive in both directions, but neither drives the *call site*. Independently re-verified: reverted the call site's argument to `project_root` and re-ran the full 279-test binary suite — still `0 failed`. No test would catch a regression here. See Human Verification. |
| 7 | Canary test discriminator survives full widening; canary relocation (Code→Define) recorded as deliberate; capture retention cannot evict an unread capture | ✓ VERIFIED | `canary_gate_only_applies_to_the_stream_launch_path` re-ran live and discriminates on the legacy opt-out, not stage membership (`1 passed`); `DEFAULT_CAPTURE_RETENTION: usize = 12` confirmed with correct arithmetic in its doc comment; `prune_history_retains_a_full_five_stage_run_with_loop_backs` re-ran live (`1 passed`); canary relocation recorded in `STREAM_JSON_STAGES`'s doc comment and in `DRAIN-ANALYSIS.md` |

**Score:** 7/8 truths verified (1 present + wired, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/agent_result.rs` — `reconcile_layer0_verdict` | Status-gated graft | ✓ VERIFIED | Confirmed at HEAD, tests pass |
| `crates/devflow-cli/src/pipeline_outcomes.rs` — `classify_validate_outcome` | Exhaustive `(layer0, status, verdict)` match | ✓ VERIFIED | 0 wildcards, 7 variants confirmed |
| `crates/devflow-core/src/agent_result.rs` — `evaluate_layer0` discovery | Reads `execution_root` | ✓ VERIFIED | Confirmed, tested |
| `crates/devflow-core/src/verify.rs` — `phase_has_blocking_human_checkpoint` | Root-parameterized, caller passes execution root | ✓ VERIFIED (function) / ⚠️ (call-site behavior) | Function tested in isolation; call site untested end-to-end |
| `crates/devflow-cli/src/pipeline_launch.rs` — `STREAM_JSON_STAGES` | Widened on evidence, all 5 stages accounted by name | ✓ VERIFIED | Doc comment confirmed complete |
| `.planning/phases/34-…/34-evidence/{stage}/` | Per-stage capture + run.log, PII-scrubbed | ✓ VERIFIED | All 5 present, committed, PII-clean (re-scanned independently — see below) |
| `.planning/phases/34-…/34-evidence/DRAIN-ANALYSIS.md` | Per-stage `BackgroundTaskState` + cost of recurrence | ✓ VERIFIED | 5 sections, present |
| `.planning/phases/34-…/34-evidence/BINARY-PROMOTION.md` | Binary-provenance answer, placeholder-scrubbed | ✓ VERIFIED | Present, `<home>`/`<user>` placeholders confirmed, no raw `$USER`/`$HOME` |
| `crates/devflow-cli/src/test_support.rs` — `env_lock()` | Poison-tolerant `ENV_MUTEX` accessor | ✓ VERIFIED | Confirmed present, wired at all call sites (`ENV_MUTEX.lock().unwrap()` count: 0 code sites remaining) |

### Independent Re-Verification (not taken from SUMMARY claims)

| Check | Result |
|---|---|
| `cargo test -p devflow-core --lib` | **554 passed; 0 failed** |
| `cargo test -p devflow --bin devflow` | **279 passed; 0 failed** |
| `cargo test -p devflow --test phase7_cli` | **17 passed; 0 failed** |
| `scripts/check.sh all` | **exit 0** (captured directly) |
| PII re-scan: `rg "$USER\|$HOME"` over whole `34-evidence/` tree | **0 matches** |
| PII re-scan: operator's actual email (`<operator-email>`) | **0 matches**, with a working negative control (`linuxbrew` string, 17 matches) confirming the scan itself functions |
| PII re-scan: truncated username fragment (`denniyahh`), `/home/den` fragment | **0 matches** — confirms the fix recorded in 34-05-SUMMARY deviation #3 actually landed |
| Mutation control on criterion 6b: revert `execution_root`→`project_root` at the `GateReview` call site, re-run full 279-test suite | **279 passed; 0 failed** — confirms the coverage gap is real, not a SUMMARY overstatement |
| `AgentStatus` variant/wildcard count in `classify_validate_outcome` | 7 variants, 0 wildcards — confirmed by direct grep of source, not summary claim |
| Debt-marker scan (`TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER`) across all phase-modified files | **0 matches** |

### Requirements Coverage

| Requirement | Source Plan(s) | Description | Status | Evidence |
|---|---|---|---|---|
| DOGFOOD-03 | 34-02, 34-05, 34-06 | Every stream-json stage joined on real per-stage behavioural evidence; un-evidenced stages visibly deliberate; phase moved rollout forward | ✓ SATISFIED | Delivery floor exceeded (5/5 widened on real captures); all 5 `Stage` variants accounted for by name; n=1 limitations explicitly stated |
| DOGFOOD-04 | 34-01, 34-03, 34-04 | Validate's reported outcome reflects derived status, not self-report | ✓ SATISFIED (core claim) — with the 6b caveat | Criteria 3 (classifier) + 4 (graft) both solidly verified with live tests and negative controls; the criterion-6b gap concerns a *different* live route (worktree checkpoint auto-decide) that is architecturally adjacent to DOGFOOD-04 but does not undermine the core self-report-vs-derived-status guarantee, which criteria 3+4 close directly |

No orphaned requirements — every plan's `requirements:` field maps to a declared v1 requirement, and both DOGFOOD-03 and DOGFOOD-04 are covered by at least one plan each.

### Anti-Patterns Found

None. Debt-marker scan (`TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER`) across all 10 phase-modified source/test files returned zero matches. No stub returns, no empty handlers, no hardcoded-empty data flowing to production behavior.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Graft fix closes the exploit | `cargo test -p devflow-core --lib agent_result::tests::layer0_verdict_graft_declines_when_layer1_status_is_not_success -- --exact` | `1 passed; 553 filtered out` | ✓ PASS |
| Classifier exhaustiveness sweep | `cargo test -p devflow --bin devflow pipeline_outcomes::tests::classify_validate_outcome_sweeps_all_forty_two_cells -- --exact` | `1 passed; 278 filtered out` | ✓ PASS |
| Worktree discovery + main-checkout mirror | `cargo test -p devflow-core --lib agent_result::tests::external_probe_` | `2 passed; 0 failed` | ✓ PASS |
| Second call-site coverage (criterion 6b) | Full 279-test suite with call site reverted to `project_root` | `279 passed; 0 failed` | ✗ Confirms gap — no test detects the regression |
| Canary discriminator under widening | `cargo test -p devflow --bin devflow pipeline_launch::tests::canary_gate_only_applies_to_the_stream_launch_path -- --exact` | `1 passed` | ✓ PASS |
| Retention regression pin | `cargo test -p devflow-core --lib agent_result::tests::prune_history_retains_a_full_five_stage_run_with_loop_backs -- --exact` | `1 passed` | ✓ PASS |
| Full workspace gate | `scripts/check.sh all` | exit 0 | ✓ PASS |

### Human Verification Required

#### 1. Criterion 6's second call site — end-to-end `Action::GateReview` worktree behavior

**Test:** Drive `advance()` through `Action::GateReview` in worktree mode, against a phase whose
PLAN declares `gate="blocking-human"` and whose PLAN file lives only in the worktree (not the main
checkout), with a valid session id and the checkpoint reported in the capture.

**Expected:** The checkpoint auto-decides (the plan-28-03 path fires) because
`phase_has_blocking_human_checkpoint` is called with the execution root rather than `project_root`.

**Why human:** No automated test in the repository exercises this call site end-to-end. This
verifier independently confirmed the gap by reverting the call site's argument and re-running the
full 279-test binary suite: it still reports `0 failed`. The production code is correct (confirmed
by direct source read — `execution_root` is passed, matching the sibling `evaluate_layer0` fix), but
the claim that "the checkpoint auto-decide path is no longer silently dead in worktree mode" rests
on inference (root-sensitivity of the function, proven separately, plus correct wiring, confirmed by
grep) rather than a behavioral demonstration. This was self-disclosed by 34-04-SUMMARY.md and is not
a new finding — this verification independently reproduced it rather than trusting the claim.

### Gaps Summary

No FAILED must-haves. The phase goal is substantively achieved: all 7 ROADMAP success criteria have
working, tested code at HEAD, re-verified independently (not from SUMMARY claims) — including a full
live re-run of `cargo test -p devflow-core --lib` (554/0), `cargo test -p devflow --bin devflow`
(279/0), `cargo test -p devflow --test phase7_cli` (17/0), and `scripts/check.sh all` (exit 0).

One item is genuinely unverified at the behavioral level rather than failed: criterion 6's second
call site (the `Action::GateReview` checkpoint-auto-decide path) is wired correctly in source but has
no test that would catch a regression. This routes to human verification rather than blocking the
phase, because:

1. The code is demonstrably correct by direct read (same pattern as the sibling, tested call site).
2. The gap was self-disclosed by the executing plan's own SUMMARY, not discovered by omission.
3. Closing it requires either a live worktree run or a new integration test — genuinely follow-up
   work, not a defect in what was delivered.

**Two things worth the operator's explicit awareness, beyond the human-verification item above**
(not gaps, but part of an honest account of confidence):

- **The capture campaign refuted its own premise.** Zero `background_tasks_changed` events appeared
  across 1063 events despite 8 concurrent sub-agent dispatches — filed as backlog 999.83, correctly
  not fixed in this phase. This does not block the phase's own delivery (the widening decision table
  for `NeverAnnounced` was a pre-existing, reviewed decision the phase followed rather than invented
  favorably), but it means the drain gate — the safety mechanism the widened stages' unattended
  behavior depends on — is currently proven *not* to see sub-agent concurrency on CLI 2.1.222. That
  is a live gap in the safety net underneath this phase's own widening decision, tracked separately.
- **Define and Plan are thin evidence.** 1 turn/2.3s and 2 turns/11.8s, because the capture run's
  scaffold pre-writes the plan so both stages had almost nothing to do. This is honestly disclosed
  in-source and in the SUMMARY, and the delivery floor does not require strong evidence, only real
  evidence — but it means those two stages' `NeverAnnounced` readings carry less weight than Code's.

A minor documentation staleness, not a gap: `deferred-items.md` item #1 (the `phase7_cli` widening
failures) still reads "Status: open" even though plan 34-06b closed it before 34-05 ran. The
underlying issue is resolved (confirmed live: `phase7_cli` 17/17 passing); only the tracking doc is
stale.

---

_Verified: 2026-08-06_
_Verifier: Claude (gsd-verifier)_

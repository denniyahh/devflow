---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
verified: 2026-08-05T04:00:00Z
status: gaps_found
score: 18/20 must-haves verified (2 ROADMAP success criteria FAILED — CR-01)
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: "4/4 ROADMAP success criteria (proxy measurement — see below)"
  gaps_closed:
    - "The pipeline_gate.rs/pipeline_outcomes.rs test-suite flake (abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response silently attempting a real agent launch) is genuinely closed by 33-04. Both repaired tests pass by exact name (1 passed; 268 filtered out each), confirmed against a negative control, 5/5 repeat runs green at ~1.00-1.01s (vs the 23.06s pre-fix real-launch signature), and scripts/check.sh all exits 0 on a direct, non-piped exit-code capture."
  gaps_remaining: []
  regressions:
    - "The prior VERIFICATION.md marked ROADMAP success criterion 1 ✓ VERIFIED on evidence that only proves select_loop_back_fix is WIRED to phase_verification_exists and that a non-worktree test passes — it never asked whether project_root is the correct root to check in DevFlow's actual worktree operating shape. It is not: in worktree mode project_root is the main checkout, the Validate agent writes {N}-VERIFICATION.md into the phase's worktree, and phase_verification_exists(project_root, phase) is therefore always false, making FixType::GapsOnly unreachable on the Validate path regardless of whether the phase genuinely has gaps. This is CR-01 in 33-REVIEW.md, independently re-confirmed in this verification pass (see below)."
gaps:
  - truth: "ROADMAP success criterion 1 — mid-arc Validate failure (no {N}-VERIFICATION.md) issues plain /gsd-execute-phase {N}, not --gaps-only"
    status: failed
    reason: >
      Confirmed live in source (not re-litigated from the review, independently re-derived):
      `select_loop_back_fix(project_root: &Path, phase: u32) -> FixType` at
      `crates/devflow-cli/src/pipeline_outcomes.rs:243` calls
      `agent_result::phase_verification_exists(project_root, phase)` and is invoked at all three
      in-scope call sites (`pipeline_outcomes.rs:291`, `:343`, `:354`) with the bare `project_root`
      parameter — no worktree-aware fallback anywhere in the function or its call sites (`rg -n
      "worktree_path" pipeline_outcomes.rs` shows zero matches inside `select_loop_back_fix` or
      `handle_validate_outcome`; the only matches in the file are inside the unrelated
      `hook_context_root` and its own tests, lines 522-545 and ~2011-2032).
      In worktree mode — `state.worktree_path`'s own doc comment (state.rs:106-110), the
      Validate-agent's actual working directory (`monitor.rs:313-320`, quoted verbatim in
      33-REVIEW.md: "The agent runs in its worktree when worktree mode is active"), and the sole
      production assignment site (`commands.rs:244`) all agree on this — `.planning/` is a tracked
      directory, so a `{N}-VERIFICATION.md` the Validate agent commits lands on
      `feature/phase-{N}` inside the worktree tree, and is simply absent from the main checkout
      that `project_root` points at while the phase is still in flight. No merge-back exists
      (`worktree.rs` exposes add/remove/list/prune only), so this is not a race — it is the state
      for the phase's entire remaining duration.
      Net effect: `phase_verification_exists(project_root, phase)` reads `false` for every in-flight
      worktree-mode phase regardless of whether Validate actually wrote the artifact, so
      `select_loop_back_fix` always returns `FixType::FullExecute` on the Validate path in worktree
      mode. This criterion's OWN case (mid-arc, no verification artifact anywhere) is satisfied by
      accident — the predicate is unconditionally wrong in this mode, and "wrong in a way that
      happens to match the desired output for the no-artifact case" is not the same as "correctly
      detects mid-arc." The wrongness becomes directly observable — and harmful — one criterion
      over, in criterion 2.
    artifacts:
      - path: "crates/devflow-cli/src/pipeline_outcomes.rs"
        issue: "select_loop_back_fix (line 243) and all three of its call sites (lines 291, 343, 354) pass project_root — the main checkout — as the root to check for {N}-VERIFICATION.md, instead of the phase's worktree where the Validate agent actually writes it."
      - path: "crates/devflow-core/src/agent_result.rs"
        issue: "phase_verification_exists itself (line 2578) is correct and needs no change — it faithfully answers the question it is asked; the bug is entirely in which root select_loop_back_fix asks it about."
    missing:
      - "Rename select_loop_back_fix's first parameter from project_root to evidence_root, and at each of the three call sites inside handle_validate_outcome, compute it as state.worktree_path.as_deref().unwrap_or(project_root) before passing it — the same fallback idiom this codebase already uses twice: hook_context_root (pipeline_outcomes.rs:522-535) and staleness.rs:330 (let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);)."
      - "Do NOT change phase_commit_count's root (project_root, used at pipeline_outcomes.rs:301-302) — it must stay on project_root. Git refs and the object database are shared across a repository's worktrees, so a commit made in the worktree is already visible from the main checkout; retargeting this call would not fix anything and would risk breaking the (currently correct) 999.66 wiring. The plan-level prohibition already on record for 33-03 ('the {N}-VERIFICATION.md existence signal must not be reused as the forward-progress signal') is the mirror image of this point and should not be relaxed."
      - "Add a regression test that sets state.worktree_path = Some(wt), writes {N}-VERIFICATION.md ONLY under wt (never under the bare root), drives handle_validate_outcome with ValidateOutcome::Failed, and asserts the loop_back event's fix field is GapsOnly — this is the test class that is completely absent today. Confirmed by direct search: `rg -n \"worktree_path\" crates/devflow-cli/src/pipeline_outcomes.rs` returns only line 531 (inside hook_context_root's own body) and its two unrelated tests at ~2011/2027/2031 (hook_context_root's own test module) — zero hits anywhere near handle_validate_outcome, select_loop_back_fix, or any of Phase 33's eight loop-back tests. Every existing Phase 33 test that exercises select_loop_back_fix runs with state.worktree_path left at its State::new() default of None, so project_root and 'the root the agent actually wrote to' are identical by construction in every test — the exact condition that makes CR-01 invisible to the suite that is otherwise green."
      - "Add the mirrored negative control alongside it: same worktree setup, but with no {N}-VERIFICATION.md anywhere (neither root nor worktree), asserting FullExecute — so the new test suite actually discriminates the worktree-aware fix from a no-op that always returns GapsOnly once a worktree is configured."
  - truth: "ROADMAP success criterion 2 — genuine-gaps Validate failure ({N}-VERIFICATION.md exists) still issues --gaps-only"
    status: failed
    reason: >
      Direct consequence of the same defect, and the one where CR-01 actually produces a wrong,
      harmful dispatch rather than an accidentally-correct one. In worktree mode, a phase that
      Validate has genuinely reviewed and found real gaps in — the exact case this criterion
      describes, with a real {N}-VERIFICATION.md committed on the worktree's feature branch —
      still reads as phase_verification_exists(project_root, ...) == false, because that artifact
      is on the worktree's branch tip, not the main checkout's working tree. select_loop_back_fix
      therefore dispatches FixType::FullExecute (plain /gsd-execute-phase {N}) instead of the
      correct --gaps-only. Per 33-REVIEW.md's CR-01 (independently reproduced with negative
      control, cited in the mandatory finding this verification is required to record): re-running
      an already-complete phase's plans with a plain execute is a no-op that commits nothing, which
      evaluate_layer2's no_work_done gate then classifies as a fresh Code-stage Failed, routing to
      handle_stage_failure's never-silent gate — i.e. this is not a silent inefficiency, it
      re-introduces a human-gate stall of the same CLASS that DOGFOOD-01 was written to eliminate,
      arrived at from the opposite direction.
      The shipped negative-control test for this exact criterion,
      genuine_gaps_loop_back_still_issues_gaps_only (pipeline_outcomes.rs:1483-1526), passes today
      — but only because it writes the artifact under the bare root and never sets
      state.worktree_path (confirmed by reading the test body directly: no worktree_path
      assignment anywhere in it, and workflow::save_state / handle_validate_outcome are both called
      with the same root the artifact was written under). It is a green test over an inverted
      decision in the operating mode the decision actually needs to be correct in — the precise
      failure class RULE ZERO's "negative controls" mechanism exists to catch, and the 2026-08-05
      re-verification catches it only because CR-01's own review supplied the missing negative
      control (a worktree where the artifact and the checked root are deliberately different),
      which no test in this codebase currently does.
    artifacts:
      - path: "crates/devflow-cli/src/pipeline_outcomes.rs"
        issue: "Same root cause and same fix as criterion 1's gap entry above — one fix (evidence_root threading) closes both."
    missing:
      - "Same fix as criterion 1's gap entry. Do not file or plan these as two separate fixes — they share one root cause and one correct fix (thread evidence_root through select_loop_back_fix and its three call sites)."
      - "The worktree-mode regression test specified under criterion 1's gap entry (GapsOnly when the artifact exists only in the worktree) is this criterion's own direct regression coverage — it is the test genuine_gaps_loop_back_still_issues_gaps_only should have been, and should stay alongside it as a companion (worktree case) rather than a replacement (non-worktree case remains valid coverage for --no-worktree runs)."
deferred:
  - truth: "WR-03 (33-REVIEW.md): a transient git failure records Some(0) as the baseline and hands the next successful measurement a free counter reset, contradicting handle_validate_outcome's doc comment claim that the failure direction is 'toward gating.'"
    addressed_in: "Not addressed in this milestone's roadmap (Phase 34 covers 999.73/999.74, unrelated). Recorded here as a real, reviewer-confirmed gap in the safety-gate's documented guarantee, but explicitly below this phase's own ASVS block-on threshold (disposition: mitigate, not accept-and-ignore) and not one of the four ROADMAP success criteria — surfaced for operator awareness per this verification's instructions, not filed as a blocking gap."
    evidence: "33-REVIEW.md WR-03, independently legible in current source: agent_result::phase_commit_count (agent_result.rs:1841-1861) returns a bare u32 that collapses 'counted zero commits', 'branch missing', and 'git could not be run' into the same 0, and handle_validate_outcome's doc comment (pipeline_outcomes.rs:266-268) asserts a safety direction the code does not structurally guarantee once a transient git failure is followed by a successful measurement."
  - truth: "WR-06 (33-REVIEW.md): 3 of the 4 tests that seed consecutive_failures directly and depend on the None-baseline-resets-to-1 invariant were hardened with only the baseline seed, not the PATH-neutralization layer that makes a future drift structurally safe rather than merely currently-avoided by their Abort-shaped gate response."
    addressed_in: "Not addressed in this milestone's roadmap. Recorded for awareness: validate_failure_threshold_forces_gate_then_aborts, drive_validate_advance_and_read_gate_context, and consecutive_failures_increment_saturates (pipeline_outcomes.rs:778-819, :831-888, :1835-1860) currently avoid a real agent launch because their pre-written gate response resolves to Abort rather than LoopBack, and the baseline seed keeps should_gate true so that response is actually read — but none of the three neutralizes PATH the way abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response now does, so a future change to the reset-vs-accumulate predicate could again make one of them silently fall through to loop_back_to_code -> launch_stage with no structural backstop."
    evidence: "Read directly in this verification pass: none of the three cited tests contains an ENV_MUTEX acquisition or a PATH mutation (confirmed by reading each function body in full); this matches 33-REVIEW.md's WR-06 finding exactly."
behavior_unverified_items: []
---

# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles Verification Report

**Phase Goal:** A 3+ wave unattended `devflow start` phase can complete its Code↔Validate loop
without gating on an impossible `--gaps-only` command or a false "3 consecutive failures" ceiling —
the two defects that have blocked every unattended multi-wave phase since the Phase 29 dogfood run.

**Verified:** 2026-08-05
**Status:** gaps_found
**Re-verification:** Yes — after gap closure (33-04 closed the prior VERIFICATION.md's one gap, the
test-suite flake; this pass independently re-derives every truth from source rather than trusting
either the prior VERIFICATION.md or 33-01/33-03's SUMMARY claims, per this task's explicit
instruction, and finds a new BLOCKER that the prior pass's evidence could not have detected)

## Why this replaces the prior VERIFICATION.md's "4/4 verified" result

The 2026-08-05T01:35:07Z VERIFICATION.md marked ROADMAP success criterion 1 **✓ VERIFIED** on this
evidence: *"`select_loop_back_fix` reads `agent_result::phase_verification_exists`;
`mid_arc_loop_back_issues_plain_execute_command` — 1 passed"*. That evidence is real — the two
symbols genuinely are wired together, and the named test genuinely does pass — but it is a proxy
measurement: it confirms wiring and confirms one test passes, and stops there. It never asks
whether `project_root`, the value threaded into the wired call, is the correct filesystem root to
check in the shape DevFlow actually runs in. It is not. Every test that exercises
`select_loop_back_fix` (all 8 of the phase's named loop-back tests, confirmed by direct `rg
-n "worktree_path"` search across `pipeline_outcomes.rs`) leaves `state.worktree_path` at its
default `None`, which makes `project_root` and "the root the agent actually wrote
`{N}-VERIFICATION.md` to" identical by construction — the one condition under which this bug is
invisible. In DevFlow's normal worktree-mode operating shape those two roots diverge, and the
decision this whole phase exists to fix reads the wrong one. See `CR-01` in `33-REVIEW.md`, and
this verification's own independent re-derivation of the mechanism above and below.

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | ✗ FAILED | `select_loop_back_fix` (pipeline_outcomes.rs:243) checks `project_root`, which is the main checkout in worktree mode, not the worktree where the Validate agent writes the artifact. The predicate is unconditionally `false` in worktree mode, so this criterion's own no-artifact case happens to get the right output for the wrong reason — the mechanism is broken, not verified. See `gaps` entry 1. |
| 2 | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists) still issues `--gaps-only` | ✗ FAILED | Same root cause, opposite consequence: a real, committed `{N}-VERIFICATION.md` on the worktree's branch is invisible from `project_root`, so this case dispatches the WRONG command (`FullExecute` instead of `GapsOnly`), silently re-running a complete phase. `genuine_gaps_loop_back_still_issues_gaps_only` passes only because it never configures a worktree. See `gaps` entry 2. |
| 3 | 3+ wave phase making genuine forward progress (real commits between failures) does not false-gate at wave 3 | ✓ VERIFIED | `handle_validate_outcome` (pipeline_outcomes.rs:300-324) calls `phase_commit_count(project_root, ...)` — confirmed deliberately correct to use `project_root` in worktree mode, because git refs/object DB are shared across worktrees (33-REVIEW.md's own negative control: reading the commit count from the main checkout returned the worktree's real commit, `1`, confirming this specific read is right). `healthy_multi_wave_progress_does_not_reach_the_ceiling` — `1 passed`, re-run live in this pass. **What this does not establish:** the test itself, like all Phase 33 tests, runs without a worktree configured; the claim of worktree-mode correctness rests on the shared-refs property of `git` plus the reviewer's independent probe, not on a test that actually drives this code path through a worktree. |
| 4 | `consecutive_failures` still gates when Validate finds the same unresolved problem repeatedly (no new commits) | ✓ VERIFIED | `repeated_failure_without_new_commits_still_reaches_the_ceiling` and the byte-unchanged `consecutive_failures_reaches_ceiling_across_cycles` — both re-run live in this pass, `1 passed` each. Same worktree caveat as #3 applies and is not repeated per-row. |

**Score:** 2/4 ROADMAP success criteria verified. Criteria 1 and 2 — the phase's namesake defect,
999.65/DOGFOOD-01 — are FAILED in the operating mode 33-REVIEW.md documents as DevFlow's normal
one. Criteria 3 and 4 — 999.66/DOGFOOD-02 — hold, including under the worktree lens, because the
underlying git query they depend on is architecturally required to (and does) read from
`project_root` regardless of worktree mode; this is a case where the same lens that breaks
criteria 1-2 independently confirms criteria 3-4 rather than casting doubt on them too.

### Plan-Level Must-Haves (all four plans, re-derived from source)

Every `must_haves.truths` entry across the four plans, checked directly against current source —
not against SUMMARY.md's claims. Truths that restate a ROADMAP criterion are marked accordingly;
their status is inherited from the criteria table above, not re-derived separately.

| # | Truth (abbreviated) | Plan | Status | Note |
|---|---|---|---|---|
| 1 | Mid-arc failure → plain execute (D-01) | 33-01 | ✗ FAILED | = ROADMAP criterion 1 |
| 2 | Genuine-gaps failure → still gaps-only (D-01) | 33-01 | ✗ FAILED | = ROADMAP criterion 2 |
| 3 | All three loop-back arms route through one shared helper | 33-01 | ✓ VERIFIED | This is a wiring claim, not a correctness claim — genuinely true regardless of CR-01: `rg -c "select_loop_back_fix" pipeline_outcomes.rs` shows exactly one definition + three call sites, all inside `handle_validate_outcome`. **What this does not establish:** that the shared helper computes the right answer — only that all three arms ask the same (currently wrong) question the same way, which is precisely why one fix at the helper closes both roadmap gaps at once. |
| 4 | Ship loop-back unaffected (D-02 out-of-scope) | 33-01 | ✓ VERIFIED | `handle_ship_outcome` (pipeline_outcomes.rs:384) still constructs the bare `FixType::GapsOnly` literal directly, confirmed by reading it in this pass; `ship_loop_back_still_issues_gaps_only_when_verification_absent` re-run live, `1 passed`. Untouched by CR-01 — Ship never calls `select_loop_back_fix`. |
| 5 | Operator can see chosen fix in `.devflow/events.jsonl` | 33-01 | ✓ VERIFIED | `"fix"` key present on the `loop_back` event payload (pipeline_gate.rs:151, `format!("{fix:?}")`). **What this does not establish:** that the value shown is correct — in worktree mode it will faithfully report `FullExecute` even on a genuine-gaps phase, correctly reflecting what the (currently wrong) decision was, which is a visibility guarantee, not a correctness one. |
| 6 | Persisted baseline survives across `devflow advance` invocations | 33-02 | ✓ VERIFIED | `State::last_validate_failure_commit_count: Option<u32>`, `#[serde(default)]` (state.rs:100); both serde tests re-run live, `1 passed` each. Unaffected by CR-01 — pure persistence, no root/path logic involved. |
| 7 | Pre-field state deserializes safely; absent baseline reads as "no prior failure" | 33-02 | ✓ VERIFIED | Same evidence as #6; `None`-means-no-prior-record semantics confirmed by reading `mode::consecutive_failures_made_progress`'s body (`previous.is_none_or(|p| current > p)`) and doc comment. |
| 8 | Pure predicate, no `Path`/git/filesystem argument | 33-02 | ✓ VERIFIED | `pub fn consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool` — signature confirmed by direct read, no I/O type present. `transition_resets_consecutive_failures`'s own signature confirmed byte-unchanged (`(from: Stage, to: Stage) -> bool`). |
| 9 | Predicate reports no progress when observed count unchanged (keeps ceiling reachable) | 33-02 | ✓ VERIFIED | `made_progress_requires_a_strictly_higher_count` re-run live, `1 passed`; body logic (`current > p`, strict) confirmed by direct read. |
| 10 | 3+ wave genuine progress does not false-gate | 33-03 | ✓ VERIFIED | = ROADMAP criterion 3 |
| 11 | No-progress repeated failure still gates | 33-03 | ✓ VERIFIED | = ROADMAP criterion 4 |
| 12 | Forward-progress count is git-derived, not agent-self-reported | 33-03 | ✓ VERIFIED | `phase_commit_count` (agent_result.rs:1841) shells out to `git rev-parse --verify` / `git rev-list --count`; no agent-output field is read anywhere in `handle_validate_outcome`'s failure block. |
| 13 | One commit-counting implementation serves both `evaluate_layer2` and the reset decision | 33-03 | ✓ VERIFIED | `evaluate_layer2` calls `phase_commit_count` (agent_result.rs:1905) instead of an inline block; `rg -n "rev-list --count"` shows the pattern appears only inside `phase_commit_count` itself. |
| 14 | Git-unavailable / missing-branch degrades toward gating (count zero every cycle) | 33-03 | ✓ VERIFIED | `phase_commit_count_reports_zero_without_a_branch` re-run live, `1 passed`; fallback-to-0 behavior confirmed by direct read of `phase_commit_count`'s body. |
| 15 | Abort test takes its asserted forced-gate/Abort path | 33-04 | ✓ VERIFIED | `abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response` re-run live in this pass: `1 passed; 268 filtered out; finished in 1.00s` — both branch-pinning assertions (counter reached ceiling, stage stayed `Validate`) present ahead of the gate-file assertions, confirmed by reading the diff region. |
| 16 | No unit test in `devflow-cli` can spawn a real agent (the two identified PATH-unprotected tests) | 33-04 | ✓ VERIFIED (narrow scope) | Both of the two specifically-identified tests (`abort_cleans_up_...`, `failure_gate_loop_back_...`) now neutralize `PATH` under `ENV_MUTEX`, confirmed by direct read. **What this does not establish (WR-06, carried as `deferred`, not a blocker):** three OTHER pre-existing tests that also seed `consecutive_failures` directly and depend on the same `None`-resets-to-1 invariant (`validate_failure_threshold_forces_gate_then_aborts`, `drive_validate_advance_and_read_gate_context`, `consecutive_failures_increment_saturates`) still have no `ENV_MUTEX`/PATH protection — confirmed by reading all three bodies directly in this pass, none contains an `ENV_MUTEX` acquisition or a `PATH` mutation. They currently avoid a real launch only because their pre-written gate response resolves to `Abort`, not `LoopBack` — a second, independent reason, not a structural guarantee. A future predicate change could still make one of them spawn an agent with no backstop. |
| 17 | Gated-arm test exercises the arm it claims to, not the ungated tail arm | 33-04 | ✓ VERIFIED | `failure_gate_loop_back_respects_the_mid_arc_check` re-run live: `1 passed; 268 filtered out`. Branch discriminator (`last["consecutive_failures"] >= MAX_CONSECUTIVE_FAILURES`) present ahead of the `last["fix"]` assertion, confirmed by reading the diff region. |
| 18 | Branch-pinning assertion, not just side effects | 33-04 | ✓ VERIFIED | Both repaired tests' discriminators confirmed present and transcribed verbatim (`rg -c 'a reset means the gate never fired'` = 1, `rg -c 'consecutive-failure-GATED loop-back arm'` = 1). |
| 19 | Every `consecutive_failures` seed site classified, two independent counts | 33-04 | ✓ VERIFIED | 33-04-SUMMARY's reconciled table (Count A=12, Count B=18, real assignment sites=14) matches 33-REVIEW.md's own independent enumeration ("14 total (2 production, 12 test)") — two independently-derived counts agreeing is itself the intended evidence shape here. |
| 20 | `scripts/check.sh all` completes green across repeated runs, strength stated honestly | 33-04 | ✓ VERIFIED | Independently confirmed current in this session's already-established measurements (not re-run again here, per this task's instructions): `scripts/check.sh all` exits 0 (direct exit-code capture); 547 devflow-core tests + 269 devflow bin tests, 0 failures. |

**Score:** 18/20 plan-level truths verified. The 2 FAILED entries (#1, #2) are the same underlying
defect as ROADMAP criteria 1-2 — one fix closes all four.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `agent_result.rs::phase_verification_exists` | `{N}-VERIFICATION.md` existence probe | ✓ VERIFIED | Correct as written — answers the question asked of it faithfully (line 2578); the defect is entirely in what root it is asked about. |
| `prompt.rs::FixType::FullExecute` + `#[non_exhaustive]` | plain execute command variant | ✓ VERIFIED | Unaffected by CR-01. |
| `pipeline_outcomes.rs::select_loop_back_fix` | single D-01 decision point | ⚠️ WIRED, NOT CORRECT | Exists, is the sole call point for all three in-scope arms (structurally sound per truth #3 above) — but its decision is wrong in worktree mode because it is handed the wrong root at all three call sites. This is the artifact CR-01's fix must change. |
| `pipeline_gate.rs` `loop_back` event `"fix"` field | operator-visible fix selection | ✓ VERIFIED | Present and accurate to what the code actually decided (which, in worktree mode, is currently the wrong decision — the event is a faithful reporter of a buggy upstream value, not itself buggy). |
| `state.rs::last_validate_failure_commit_count` | persisted forward-progress baseline | ✓ VERIFIED | Unaffected by CR-01. |
| `mode.rs::consecutive_failures_made_progress` | pure reset-vs-accumulate predicate | ✓ VERIFIED | Unaffected by CR-01. |
| `agent_result.rs::phase_commit_count` | git-derived commit count | ✓ VERIFIED | Correctly and deliberately still reads `project_root` (not affected by, and must not be "fixed" alongside, CR-01 — see the gap entry's explicit prohibition). |
| `test_support.rs::commit_on_feature_branch` | test-only commit helper | ✓ VERIFIED | Unaffected by CR-01. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `pipeline_outcomes.rs` | `agent_result.rs` | `select_loop_back_fix` calls `phase_verification_exists` | ✓ WIRED, ⚠️ WRONG ROOT | The call happens; the argument passed is `project_root`, which is wrong in worktree mode. Wiring ≠ correctness — this is the row the prior verification's proxy measurement stopped at. |
| `pipeline_outcomes.rs` | `prompt.rs` | returns `FixType::FullExecute`, rendered by `fix_prompt` | ✓ WIRED | Correct and unaffected — the rendering step is sound; only the upstream decision of *which* variant to render is wrong in worktree mode. |
| `pipeline_outcomes.rs` | `agent_result.rs` | `handle_validate_outcome` calls `phase_commit_count` | ✓ WIRED, ✓ CORRECT ROOT | Confirmed correct to use `project_root` — the opposite conclusion from the row above, and the reason CR-01's fix must change exactly one of these two adjacent reads, not both. |
| `pipeline_outcomes.rs` | `mode.rs` | passes baseline + fresh count to `consecutive_failures_made_progress` | ✓ WIRED | Unaffected. |
| `agent_result.rs` (`evaluate_layer2`) | `agent_result.rs` (`phase_commit_count`) | single implementation, no duplicated git block | ✓ WIRED | Unaffected. |

### Prohibitions (must-NOT checks)

| Prohibition | Status | Evidence |
|---|---|---|
| `handle_ship_outcome`'s loop-back must not be rewired to consult D-01 | ✓ HOLDS | Confirmed live, line 384 still constructs the bare literal. |
| Ship's out-of-scope call site unaffected | ✓ HOLDS | `ship_loop_back_still_issues_gaps_only_when_verification_absent` re-run live, `1 passed`. |
| `transition_resets_consecutive_failures` signature not widened | ✓ HOLDS | Confirmed by direct read. |
| `consecutive_failures` must not reset unconditionally on every loop-back | ✓ HOLDS | Reset only fires inside the progress branch, confirmed by direct read. |
| `{N}-VERIFICATION.md` signal not reused as forward-progress signal | ✓ HOLDS | Two independent primitives, confirmed by direct read. |
| `prepare_loop_back_to_code` must not gain counter logic | ✓ HOLDS | `sed ... \| grep -c` = 1, re-run live. |
| Neither 33-04-repaired test may be weakened; no retry/serialization added to `check.sh` | ✓ HOLDS | Confirmed by reading both tests and `scripts/check.sh` directly — no `#[ignore]`, no deleted assertion, no retry loop. |
| 33-04's spawning negative control must not have been re-run | ✓ HOLDS | 33-04-SUMMARY states plainly it was not; no evidence in this pass contradicts that. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| `select_loop_back_fix`'s worktree-vs-main-checkout root split | Direct source read: `rg -n "worktree_path" pipeline_outcomes.rs` | Matches only inside the unrelated `hook_context_root` and its own tests; zero matches near `select_loop_back_fix` or `handle_validate_outcome` | Confirms CR-01's mechanism and the missing test coverage independently |
| `phase_verification_exists_finds_the_artifact_by_prefix` | `cargo test -p devflow-core --lib ... -- --exact` | `1 passed; 546 filtered out` | ✓ PASS |
| `genuine_gaps_loop_back_still_issues_gaps_only` | `cargo test -p devflow --bin devflow ... -- --exact` | `1 passed; 268 filtered out` | ✓ PASS (but see gap entry 2 — passes for the wrong reason, no worktree configured) |
| `mid_arc_loop_back_issues_plain_execute_command`, `healthy_multi_wave_progress_does_not_reach_the_ceiling`, `repeated_failure_without_new_commits_still_reaches_the_ceiling`, `consecutive_failures_reaches_ceiling_across_cycles`, `abort_cleans_up_gate_files_...`, `failure_gate_loop_back_respects_the_mid_arc_check` | `cargo test` `--exact` per name | each `1 passed` | ✓ PASS |
| `scripts/check.sh all`, full-suite repeat runs, timing runs | (already established this session, not re-run — see note below) | exit 0; 547 devflow-core + 269 devflow bin passed, 0 failed | ✓ PASS |

Per this task's explicit instruction, the already-established measurements (HEAD `bf1cf01`,
`scripts/check.sh all` exit 0, 547+269 tests passing, the flaky-test closure with its negative
control and 5/5 repeat-run confirmation) were not re-derived in this pass — they were verified as
current by confirming HEAD matches `bf1cf01` and the working tree is clean, and are carried forward
as already-verified rather than re-measured.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DOGFOOD-01 | 33-01 | Mid-arc vs genuine-gaps loop-back dispatch | ✗ BLOCKED | ROADMAP criteria 1 and 2 (the exact text of DOGFOOD-01) both FAILED under CR-01 in worktree mode. `.planning/REQUIREMENTS.md` currently marks this `[x]` / "Complete" (line 19, line 48) — that mark is premature; it should read incomplete pending CR-01's fix. This is a real requirement-traceability discrepancy this verification is required to surface, distinct from the doc-hygiene "Info" finding the prior verification recorded (that finding is now moot — REQUIREMENTS.md's checkboxes ARE marked, they are just marked wrong). |
| DOGFOOD-02 | 33-02, 33-03 | Forward-progress-aware consecutive-failures reset | ✓ SATISFIED | ROADMAP criteria 3 and 4 hold, including under the worktree-mode lens (the underlying git query is architecturally required to use `project_root`, confirmed correct). `.planning/REQUIREMENTS.md`'s `[x]` / "Complete" mark for DOGFOOD-02 (line 22, line 49) is accurate. |

No orphaned requirements: REQUIREMENTS.md's Phase 33 mapping (DOGFOOD-01, DOGFOOD-02) matches
exactly what 33-01/33-02/33-03's PLAN frontmatter `requirements:` fields declare.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 243-354 | CR-01: D-01's entire decision signal is read from the wrong working tree in worktree mode | 🛑 Blocker | See `gaps` entries 1-2 — this is the phase's headline defect, unresolved in the operating mode DevFlow actually runs in |
| `crates/devflow-cli/src/pipeline_outcomes.rs` / `mode.rs` | doc comments | WR-01 (33-REVIEW.md, carried, still open): the forward-progress reset removes the pipeline's only unconditional bound on the Code↔Validate loop | ⚠️ Warning | Accepted design tradeoff (T-33-06: accept, below `high` threshold), but the reviewer's new evidence (GSD commands routinely commit `.planning/` artifacts even with no source change) sharpens it from hypothetical to routine. No numbered backlog entry tracks the deferral yet. |
| `crates/devflow-core/src/agent_result.rs` | 2564-2596 | WR-02 (33-REVIEW.md, carried, still open, now partly superseded by CR-01): `phase_verification_exists` has no staleness invalidation | ⚠️ Warning | A natural companion fix once CR-01 is being touched anyway; not itself a phase-goal failure. |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 255-268, 300-323 | WR-03 (33-REVIEW.md, new): a transient `git` failure grants a free counter reset on the next cycle, contradicting the doc comment's stated safety direction | ⚠️ Warning | Carried to `deferred` above — real, reviewer-confirmed, below this phase's blocking threshold, not one of the four ROADMAP criteria. |
| `crates/devflow-cli/src/pipeline_gate.rs`, `pipeline_outcomes.rs` | 778-819, 831-888, 1835-1860 | WR-06 (33-REVIEW.md, new): 3 of 4 tests needing the 999.66 baseline invariant were hardened with the seed only, not PATH-neutralization | ⚠️ Warning | Carried to `deferred` above; currently harmless (their gate response resolves to Abort, not LoopBack) but structurally unguarded against a future predicate change. |
| `.planning/REQUIREMENTS.md` | 19, 48 | DOGFOOD-01 marked `[x]` / "Complete" while ROADMAP criteria 1-2 FAIL | ℹ️ Info, escalated from the prior pass's doc-hygiene note | The prior verification's Info-level note ("checkboxes still unchecked") is now stale — REQUIREMENTS.md has since been marked complete, and that mark is now the thing that's wrong, not merely undone. Should be reverted to `[ ]` / "Pending" alongside the CR-01 fix, not left as-is. |

No `TBD`/`FIXME`/`XXX` debt markers found in any of the files this phase modified.

### Human Verification Required

None. CR-01's mechanism is fully established by direct source reading, an independent negative
control (33-REVIEW.md's scratch-repo-plus-linked-worktree probe, whose result this verification
took as given per its instructions rather than re-running), and the observable absence of any test
in the workspace that configures `state.worktree_path` on a `handle_validate_outcome` drive. This is
not a judgment call requiring human observation of runtime/visual/UX behavior — it is a mechanical,
well-scoped defect with a precedent fix idiom already twice-present in this same codebase
(`hook_context_root`, `staleness.rs:330`).

### Gaps Summary

**The phase's second defect (999.66/DOGFOOD-02) is genuinely fixed** — ROADMAP criteria 3 and 4
hold, including under the worktree-mode lens this verification pass specifically applied to every
truth, because the underlying git commit-count query is architecturally required to (and does) read
from the main checkout regardless of worktree mode. The 33-04 gap-closure plan's own goal — closing
the test-suite flake so `scripts/check.sh all` is a gate that means something — is also genuinely
achieved, confirmed against a negative control and repeat runs.

**The phase's first and namesake defect (999.65/DOGFOOD-01) is NOT fixed in DevFlow's normal
worktree-mode operating shape.** `select_loop_back_fix` reads `.planning/phases/{N}-*/
{N}-VERIFICATION.md` from `project_root` (the main checkout), but the Validate agent that produces
that artifact writes it inside the phase's worktree, which is a different, uncommitted-to-the-main-
checkout location for the phase's entire in-flight duration. The predicate is therefore
unconditionally `false` in worktree mode: ROADMAP criterion 1 (mid-arc → plain execute) happens to
land on the right output by accident, and ROADMAP criterion 2 (genuine gaps → still `--gaps-only`)
lands on the wrong one — silently re-running an already-complete phase and, per `evaluate_layer2`'s
own no-work-done gate, re-introducing a human-gate stall of the exact class this phase exists to
eliminate.

This gap was invisible to every automated check this phase's own plans specified, and to the prior
verification pass, for the same reason in every case: no test anywhere in the workspace sets
`state.worktree_path` on a drive of `handle_validate_outcome`, so `project_root` and "the root the
Validate agent actually wrote to" are identical by construction in all eight of Phase 33's named
loop-back tests. That is the test gap the fix plan must close, not only the production code.

The fix is small, precedented, and localized: thread `state.worktree_path.as_deref().unwrap_or
(project_root)` through `select_loop_back_fix` and its three call sites, exactly as
`hook_context_root` and `staleness.rs:330` already do elsewhere in this codebase — while leaving
`phase_commit_count`'s use of `project_root` untouched, since that read is independently confirmed
correct. Two deferred, non-blocking items (WR-03, WR-06) are recorded above for operator awareness
per this verification's instructions but do not gate the phase.

---

_Verified: 2026-08-05_
_Verifier: Claude (gsd-verifier)_

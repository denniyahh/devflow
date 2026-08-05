---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
verified: 2026-08-05T05:00:00Z
status: passed
score: 4/4 ROADMAP success criteria verified; 20/20 plan-level truths verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: "2/4 ROADMAP success criteria (CR-01 evidence-root defect: select_loop_back_fix read project_root instead of the worktree)"
  gaps_closed:
    - "ROADMAP success criterion 1 (mid-arc Validate failure -> plain /gsd-execute-phase {N}) — select_loop_back_fix now takes evidence_root and all three handle_validate_outcome call sites resolve it via state.worktree_path.as_deref().unwrap_or(project_root), confirmed by direct source read (pipeline_outcomes.rs:261-267, :321-324, :377-380, :392-395) and by worktree_mode_mid_arc_loop_back_issues_plain_execute (both scenario A and scenario B) passing by exact name."
    - "ROADMAP success criterion 2 (genuine-gaps Validate failure -> still --gaps-only) — same fix; confirmed by worktree_mode_genuine_gaps_loop_back_issues_gaps_only passing by exact name, with the artifact placed only under the worktree path (never the bare root), which is structurally impossible to satisfy under the pre-fix code."
  gaps_remaining: []
  regressions: []
gaps: []
deferred:
  - truth: "WR-03 (33-REVIEW.md, carried, not closed by this phase): a transient git failure records Some(0) as the baseline and hands the next successful measurement a free counter reset, contradicting handle_validate_outcome's doc comment claim that the failure direction is 'toward gating.'"
    addressed_in: "Not addressed in this milestone's roadmap (Phase 34 covers 999.73/999.74, unrelated). Below this phase's own blocking threshold (disposition: mitigate, not accept-and-ignore) and not one of the four ROADMAP success criteria."
    evidence: "phase_commit_count (agent_result.rs:1841-1861) returns a bare u32 collapsing 'counted zero', 'branch missing', and 'git unrunnable' into the same 0; handle_validate_outcome's doc comment (pipeline_outcomes.rs:284-286) asserts a safety direction the code does not structurally guarantee once a transient failure is followed by a successful measurement."
  - truth: "WR-06 (33-REVIEW.md, carried, not closed by this phase): 3 of 4 tests that seed consecutive_failures directly still lack PATH-neutralization, relying only on their pre-written gate response resolving to Abort rather than LoopBack to avoid a real agent spawn."
    addressed_in: "Not addressed in this milestone's roadmap."
    evidence: "validate_failure_threshold_forces_gate_then_aborts, drive_validate_advance_and_read_gate_context, consecutive_failures_increment_saturates (pipeline_outcomes.rs:~819-861, :873-930, :2057-2081) confirmed by direct read to contain no ENV_MUTEX/PATH neutralization."
  - truth: "New CR-01 (33-REVIEW.md, third pass): evaluate_layer0 reads external_verify_commands from project_root instead of execution_root (agent_result.rs:2041-2042), the same defect class one call site over."
    addressed_in: "Pre-existing code, confirmed via git blame to predate the phase-33 merge-base (commits c620fb37 and 305e2675, both dated 2026-07-18, versus merge-base 7b55fce dated 2026-08-04). Outside this phase's diff scope (agent_result.rs is touched by 33-01/33-03, but not at these lines). Not filed as a phase-33 gap per this verification's explicit instructions; recorded here for operator awareness as a same-class candidate for a numbered backlog entry."
    evidence: "git blame -L 2038,2044 crates/devflow-core/src/agent_result.rs, independently re-run in this pass, confirms both lines predate merge-base 7b55fcefb8d047bd00db7db6a1365664ffb25acc."
behavior_unverified_items: []
---

# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles Verification Report

**Phase Goal:** A 3+ wave unattended `devflow start` phase can complete its Code↔Validate loop
without gating on an impossible `--gaps-only` command or a false "3 consecutive failures" ceiling —
the two defects that have blocked every unattended multi-wave phase since the Phase 29 dogfood run.

**Verified:** 2026-08-05
**Status:** passed
**Re-verification:** Yes — third pass. The prior VERIFICATION.md (2026-08-05T04:00:00Z) found CR-01:
`select_loop_back_fix` read its `{N}-VERIFICATION.md` existence signal from `project_root` (the main
checkout) instead of the phase's worktree, where the Validate agent actually authors the artifact,
making ROADMAP criteria 1 and 2 fail in DevFlow's normal worktree-mode operating shape. Plan 33-05
(commits `12f12e6`, `e9a5eb2`, `7dc53ee`) fixed this. This pass independently re-derives every truth
from current source rather than trusting 33-05-SUMMARY's claims, and confirms the fix.

## Independent confirmation that CR-01 is genuinely closed, not papered over

Read directly, not inherited from 33-REVIEW.md's third pass or 33-05-SUMMARY.md:

- `select_loop_back_fix`'s first parameter is renamed `evidence_root` (`pipeline_outcomes.rs:261`).
- All three `handle_validate_outcome` call sites (the Ambiguous gate arm at `:321-324`, the
  consecutive-failure gate arm at `:377-380`, and the plain-Failed tail arm at `:392-395`) resolve
  it identically: `state.worktree_path.as_deref().unwrap_or(project_root)`. A grep for
  `select_loop_back_fix(` confirms exactly one definition and three call sites — no fourth arm was
  missed.
- The adjacent `phase_commit_count` read (`:336`) still passes the bare `project_root`, and the
  source now carries two independent statements of why that asymmetry is correct: the caller
  comment at `:288-298` and the callee's own contract at `agent_result.rs:1833-1836` ("Must be called
  with the main `project_root`, never a worktree path — git worktrees share refs and the object
  database").
- `phase_verification_exists` itself (`agent_result.rs:2578-2596`) is a pure filesystem check
  (`std::fs::read_dir` + `Path::exists()`) — it never shells out to `git`. This matters for what the
  new tests can and cannot prove (see the limitation noted under criteria 1-2 below).

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | ✓ VERIFIED | `worktree_mode_mid_arc_loop_back_issues_plain_execute` re-run live by exact full path in this pass (`pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute`): `1 passed; 270 filtered out`. Exercises both scenario A (no artifact anywhere, worktree configured) and scenario B (artifact visible from the main checkout only, never the worktree — the "OR both roots" misreading this test specifically discriminates). Non-worktree companion `mid_arc_loop_back_issues_plain_execute_command` also re-run live: `1 passed`. **Negative control performed in this pass:** the identical command against a fabricated test name (`this_test_does_not_exist_xyz`) returns `0 passed; 271 filtered out`, confirming the positive runs above matched a real test rather than silently no-opping (this repo's documented `cargo test --exact` false-green risk). |
| 2 | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists) still issues `--gaps-only` | ✓ VERIFIED | `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` re-run live by exact full path: `1 passed; 270 filtered out`. The artifact is written only under `worktree.join(".planning/phases/...")`, never under the bare tempdir root — structurally impossible to satisfy under the pre-fix `project_root`-only predicate. Non-worktree companion `genuine_gaps_loop_back_still_issues_gaps_only` re-run live: `1 passed`, preserved byte-unchanged per the plan's explicit prohibition. |
| 3 | 3+ wave phase making genuine forward progress (real commits between failures) does not false-gate at wave 3 | ✓ VERIFIED | `healthy_multi_wave_progress_does_not_reach_the_ceiling` re-run live: `1 passed`. `phase_commit_count` (the signal this depends on) is architecturally required to read `project_root` regardless of worktree mode — confirmed by reading its implementation directly: it shells `git rev-parse`/`git rev-list` against `project_root`, and git refs/object database are shared across linked worktrees by design, unrelated to the `.planning/`-is-tracked mechanism that caused CR-01. Unaffected by the CR-01 fix or its scope. |
| 4 | `consecutive_failures` still gates correctly when Validate finds the *same* unresolved problem again across a loop-back | ✓ VERIFIED | `repeated_failure_without_new_commits_still_reaches_the_ceiling` and `consecutive_failures_reaches_ceiling_across_cycles` both re-run live in this pass: `1 passed` each. Same "commit count is git-derived, not tracked-file-derived" reasoning as #3 applies. |

**Score:** 4/4 ROADMAP success criteria verified. **What this does NOT establish** (stated plainly,
not as a trailing caveat): every test exercising these four criteria drives a tempdir with `PATH`
neutralized under `ENV_MUTEX`, and the two worktree-mode tests construct their "worktree" with
`std::fs::create_dir_all` — a plain directory, not a linked `git worktree`. This is a legitimate,
sufficient proof of the specific mechanism this phase's fix touches, because `phase_verification_exists`
is filesystem-only and never calls `git` — the plain-directory stand-in faithfully exercises the exact
code path under test. It does **not** prove that `state.worktree_path` itself gets populated with a
real worktree's real path during a real `devflow start` run (that population is pre-existing code at
`commands.rs:244`, unchanged by this phase and outside its diff, documented at `state.rs:104-110`),
and it does not prove an actual multi-wave unattended run completes end-to-end against a real spawned
agent — 33-05-SUMMARY's own coverage table says this explicitly (`D3`, `human_judgment: true`,
"the end-to-end claim belongs to a dogfood run"). The tracked-file mechanism the fix depends on
(`.planning/` files being absent from the main checkout while a phase is in-flight on its feature
branch) was independently confirmed on this repository's own real branches by 33-REVIEW.md's negative
control (`git ls-tree -r --name-only develop -- .planning/phases | grep -c '33-'` → `0`, vs the same
command against `HEAD` listing every phase-33 file) — that is real-repo evidence for the premise the
fix relies on, distinct from and stronger than the tempdir unit tests. On balance: the decision-routing
logic this phase exists to fix is proven correct at the mechanism level with a real negative control;
full end-to-end confirmation is inherently the job of the next phase that actually dogfoods a 3+ wave
unattended run against this fixed binary, not something phase 33 can self-certify.

### Plan-Level Must-Haves (all five plans, re-derived from source)

| # | Truth (abbreviated) | Plan | Status | Note |
|---|---|---|---|---|
| 1 | Mid-arc failure → plain execute (D-01) | 33-01 | ✓ VERIFIED | = ROADMAP criterion 1 |
| 2 | Genuine-gaps failure → still gaps-only (D-01) | 33-01 | ✓ VERIFIED | = ROADMAP criterion 2 |
| 3 | All three loop-back arms route through one shared helper | 33-01 | ✓ VERIFIED | `rg -c "select_loop_back_fix" pipeline_outcomes.rs` shows exactly one definition + three call sites, all inside `handle_validate_outcome`, confirmed by direct read. |
| 4 | Ship loop-back unaffected (D-02 out-of-scope) | 33-01 | ✓ VERIFIED | `handle_ship_outcome` (pipeline_outcomes.rs:411) still constructs `GateResponse`/gate flow directly with no call to `select_loop_back_fix`, confirmed by reading it in this pass. |
| 5 | Operator can see chosen fix in `.devflow/events.jsonl` | 33-01 | ✓ VERIFIED | `"fix"` key present on the `loop_back` event payload, confirmed by both new worktree-mode tests asserting on `last["fix"]`. |
| 6 | Persisted baseline survives across `devflow advance` invocations | 33-02 | ✓ VERIFIED | `State::last_validate_failure_commit_count: Option<u32>`, `#[serde(default)]`. Unaffected by CR-01/33-05. |
| 7 | Pre-field state deserializes safely; absent baseline reads as "no prior failure" | 33-02 | ✓ VERIFIED | Unaffected. |
| 8 | Pure predicate, no `Path`/git/filesystem argument | 33-02 | ✓ VERIFIED | `consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool` confirmed by direct read, no I/O type present. |
| 9 | Predicate reports no progress when observed count unchanged (keeps ceiling reachable) | 33-02 | ✓ VERIFIED | Unaffected. |
| 10 | 3+ wave genuine progress does not false-gate | 33-03 | ✓ VERIFIED | = ROADMAP criterion 3 |
| 11 | No-progress repeated failure still gates | 33-03 | ✓ VERIFIED | = ROADMAP criterion 4 |
| 12 | Forward-progress count is git-derived, not agent-self-reported | 33-03 | ✓ VERIFIED | `phase_commit_count` shells `git rev-parse`/`git rev-list`; no agent-output field read anywhere in `handle_validate_outcome`'s failure block. |
| 13 | One commit-counting implementation serves both `evaluate_layer2` and the reset decision | 33-03 | ✓ VERIFIED | `evaluate_layer2` calls `phase_commit_count` (agent_result.rs:1905); `rg -n "rev-list --count"` shows the pattern appears only inside `phase_commit_count` itself. |
| 14 | Git-unavailable / missing-branch degrades toward gating (count zero every cycle) | 33-03 | ✓ VERIFIED | Confirmed by direct read of `phase_commit_count`'s body (returns `0` on `!branch_exists` or a failed `rev-list`). |
| 15 | Abort test takes its asserted forced-gate/Abort path | 33-04 | ✓ VERIFIED | Unaffected by 33-05; not re-run in this pass, carried from the already-established session measurement (HEAD `bf1cf01`→`1144c35`, no source change to this file's Abort-path region). |
| 16 | No unit test in `devflow-cli` can spawn a real agent (narrow scope: the two 33-04-identified tests) | 33-04 | ✓ VERIFIED (narrow scope) | Both `abort_cleans_up_gate_files_...` and `failure_gate_loop_back_...` neutralize PATH under `ENV_MUTEX`. See `deferred` (WR-06) — three OTHER pre-existing tests remain unhardened, carried forward unresolved, not a phase-33 regression. |
| 17 | Gated-arm test exercises the arm it claims to | 33-04 | ✓ VERIFIED | Unaffected by 33-05. |
| 18 | Branch-pinning assertion, not just side effects | 33-04 | ✓ VERIFIED | Unaffected by 33-05. |
| 19 | Every `consecutive_failures` seed site classified, two independent counts | 33-04 | ✓ VERIFIED | Unaffected by 33-05. |
| 20 | `scripts/check.sh all` completes green; `select_loop_back_fix` reads the evidence root, not the main checkout | 33-05 | ✓ VERIFIED | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` both re-run live in this pass: exit 0 each. Full-suite count (271 devflow bin + 547 devflow-core, 0 failures) carried from the already-established session measurement at HEAD `fbbb041`; independently confirmed current in this pass by `git diff --stat fbbb041 HEAD`, which shows only `.planning/ROADMAP.md`, `.planning/STATE.md`, and `33-REVIEW.md` changed since — no source file touched. |

**Score:** 20/20 plan-level truths verified.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `agent_result.rs::phase_verification_exists` | `{N}-VERIFICATION.md` existence probe | ✓ VERIFIED | Correct and unchanged by 33-05 — filesystem-only, faithfully answers the question asked of it. |
| `prompt.rs::FixType::FullExecute` + `#[non_exhaustive]` | plain execute command variant | ✓ VERIFIED | Unaffected. |
| `pipeline_outcomes.rs::select_loop_back_fix` | single D-01 decision point, taking `evidence_root` | ✓ VERIFIED | Renamed parameter (`:261`), correct at all three call sites (`:321-324`, `:377-380`, `:392-395`), confirmed by direct read and by the two worktree-mode tests. |
| `pipeline_gate.rs` `loop_back` event `"fix"` field | operator-visible fix selection | ✓ VERIFIED | Present and now accurate in worktree mode, not merely a faithful reporter of a buggy upstream value as in the prior pass. |
| `state.rs::last_validate_failure_commit_count` | persisted forward-progress baseline | ✓ VERIFIED | Unaffected by CR-01/33-05. |
| `mode.rs::consecutive_failures_made_progress` | pure reset-vs-accumulate predicate | ✓ VERIFIED | Unaffected. |
| `agent_result.rs::phase_commit_count` | git-derived commit count | ✓ VERIFIED | Correctly and deliberately still reads `project_root` — confirmed by direct read of the function body and its updated doc comment (`:1833-1836`). |
| `test_support.rs::commit_on_feature_branch` | test-only commit helper | ✓ VERIFIED | Unaffected. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `pipeline_outcomes.rs` | `agent_result.rs` | `select_loop_back_fix` calls `phase_verification_exists(evidence_root, ...)` | ✓ WIRED, ✓ CORRECT ROOT | The call happens with the resolved evidence root, not the bare main checkout. Confirmed by direct read and by both worktree-mode tests exercising the corrected value. |
| `pipeline_outcomes.rs` | `prompt.rs` | returns `FixType::FullExecute`, rendered by `fix_prompt` | ✓ WIRED | Unaffected. |
| `pipeline_outcomes.rs` | `agent_result.rs` | `handle_validate_outcome` calls `phase_commit_count(project_root, ...)` | ✓ WIRED, ✓ CORRECT ROOT | Deliberately unchanged — confirmed correct, and now documented in two places (caller comment + callee contract). |
| `pipeline_outcomes.rs` | `mode.rs` | passes baseline + fresh count to `consecutive_failures_made_progress` | ✓ WIRED | Unaffected. |
| `agent_result.rs` (`evaluate_layer2`) | `agent_result.rs` (`phase_commit_count`) | single implementation, no duplicated git block | ✓ WIRED | Unaffected. |

### Prohibitions (must-NOT checks)

| Prohibition | Status | Evidence |
|---|---|---|
| `phase_commit_count`'s root must NOT be changed to the worktree | ✓ HOLDS | Confirmed live at `:336` — still `project_root`. |
| `handle_ship_outcome`'s loop-back must not be rewired to consult D-01 | ✓ HOLDS | Confirmed live, no call to `select_loop_back_fix` from `handle_ship_outcome`. |
| `genuine_gaps_loop_back_still_issues_gaps_only` / `mid_arc_loop_back_issues_plain_execute_command` must not be weakened, deleted, or renamed | ✓ HOLDS | Both re-run live by exact name in this pass, `1 passed` each, bodies unchanged per 33-05's explicit prohibition. |
| No new helper function introduced to deduplicate the three call sites (borrow-checker-forced repetition) | ✓ HOLDS | Confirmed by direct read — three inline call sites, no new symbol beyond the renamed parameter. Noted as WR-08 (hoisting to an owned `PathBuf` binding) in 33-REVIEW.md — a style improvement, not a correctness defect, correctly left open as a non-blocking warning. |
| The evidence-root resolution must use the plain fallback, not `hook_context_root`'s `.exists()`-filtered variant | ✓ HOLDS | Confirmed by direct read — all three sites use the plain `unwrap_or` form. |
| No test may be weakened, ignored, or excluded to reach green | ✓ HOLDS | No `#[ignore]`, no deleted assertion found in the diff. |
| Neither new worktree-mode test may spawn a real agent process | ✓ HOLDS | Both wrap the drive in `ENV_MUTEX` + `agent_free_git_only_path_dir()` PATH neutralization, confirmed by direct read. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| `worktree_mode_mid_arc_loop_back_issues_plain_execute` | `cargo test -p devflow --bin devflow "pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute" -- --exact` | `1 passed; 270 filtered out` | ✓ PASS |
| `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` | same pattern, exact name | `1 passed; 270 filtered out` | ✓ PASS |
| `mid_arc_loop_back_issues_plain_execute_command` | same pattern, exact name | `1 passed; 270 filtered out` | ✓ PASS |
| `genuine_gaps_loop_back_still_issues_gaps_only` | same pattern, exact name | `1 passed; 270 filtered out` | ✓ PASS |
| `healthy_multi_wave_progress_does_not_reach_the_ceiling` | same pattern, exact name | `1 passed; 270 filtered out` | ✓ PASS |
| `repeated_failure_without_new_commits_still_reaches_the_ceiling` | same pattern, exact name | `1 passed; 270 filtered out` | ✓ PASS |
| `consecutive_failures_reaches_ceiling_across_cycles` | same pattern, exact name | `1 passed; 270 filtered out` | ✓ PASS |
| **Negative control** — fabricated test name | `cargo test -p devflow --bin devflow "pipeline_outcomes::tests::this_test_does_not_exist_xyz" -- --exact` | `0 passed; 271 filtered out` | ✓ Confirms the 7 positive runs above matched real tests, not a silent no-op (this repo's documented `cargo test --exact` false-green risk) |
| `cargo clippy --workspace --all-targets -- -D warnings` | direct exit-code capture | exit 0 | ✓ PASS |
| `cargo fmt --all -- --check` | direct exit-code capture | exit 0 | ✓ PASS |
| `TBD`/`FIXME`/`XXX` scan across all 7 phase-33-touched files | `rg -n "TBD\|FIXME\|XXX"` per file | no matches | ✓ PASS (no debt markers) |
| Full-workspace test suite (271 devflow bin + 547 devflow-core) | already established this session at HEAD `fbbb041`, confirmed still current via `git diff --stat fbbb041 HEAD` (docs-only since) | exit 0, 0 failures | ✓ PASS (carried, not re-run, per this task's instructions and the "run the full suite at most once" constraint) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DOGFOOD-01 | 33-01, 33-05 | Mid-arc vs genuine-gaps loop-back dispatch | ✓ SATISFIED | ROADMAP criteria 1 and 2 both VERIFIED. `.planning/REQUIREMENTS.md` currently still reads `[ ]` / "Gaps Found" (lines 19, 51) — that is now stale in the other direction from the prior pass's finding: the requirement is satisfied but not yet marked. This is a doc-hygiene item, not a phase-goal gap; recommend updating the checkbox and traceability status as part of closing out this phase. |
| DOGFOOD-02 | 33-02, 33-03 | Forward-progress-aware consecutive-failures reset | ✓ SATISFIED | ROADMAP criteria 3 and 4 hold. `.planning/REQUIREMENTS.md`'s `[ ]` / "Gaps Found" mark (lines 23, 52) is likewise stale-in-the-satisfied-direction now that DOGFOOD-01 is also closed — both were left unmarked together per 33-05-SUMMARY's explicit decision not to touch REQUIREMENTS.md in a gap-closure plan. |

No orphaned requirements: REQUIREMENTS.md's Phase 33 mapping (DOGFOOD-01, DOGFOOD-02) matches
exactly what 33-01/33-02/33-03/33-04/33-05's PLAN frontmatter `requirements:` fields declare.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| `crates/devflow-core/src/agent_result.rs` | 2041-2042 | New CR-01 (33-REVIEW.md third pass): `evaluate_layer0` reads `external_verify_commands` from `project_root` instead of the `execution_root` computed one line above it — same defect class as the original CR-01, one call site over | 🛑 Blocker-class defect, but PRE-EXISTING and OUT OF SCOPE for phase 33 | Confirmed via `git blame` in this pass: both lines predate the phase-33 merge-base (`c620fb37`/`305e2675`, dated 2026-07-18, vs merge-base `7b55fce` dated 2026-08-04). Not filed as a phase-33 gap per this verification's explicit instructions — carried to `deferred` for operator awareness as a same-class backlog candidate. Not touched by any of this phase's 5 plans. |
| `crates/devflow-cli/src/pipeline_outcomes.rs` / `mode.rs` | doc comments | WR-01 (33-REVIEW.md, carried, still open): the forward-progress reset removes the only unconditional bound on the Code↔Validate loop | ⚠️ Warning | Accepted design tradeoff, recorded in `mode.rs`'s own doc comment and `33-RESEARCH.md`'s D-03. No numbered backlog entry tracks the deferral yet. |
| `crates/devflow-core/src/agent_result.rs` | 2567-2596 | WR-02 (33-REVIEW.md, carried, still open): `phase_verification_exists` has no staleness invalidation | ⚠️ Warning | Not a phase-goal failure; a natural companion fix if CR-01's neighborhood is touched again. |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 280-298, 334-358 | WR-03 (carried, still open, listed in `deferred`) | ⚠️ Warning | See `deferred` frontmatter entry. |
| `crates/devflow-cli/src/pipeline_gate.rs`, `pipeline_outcomes.rs` | multiple | WR-05 (carried, worse — two new PATH-restore-by-trailing-statement regions added by 33-05) / WR-06 (carried, listed in `deferred`) | ⚠️ Warning | Test-hygiene, not a phase-goal failure — no test in this suite currently panics inside one of these regions. |
| `crates/devflow-core/src/agent_result.rs` | 2567-2578, 2549 | WR-07 (new): `phase_verification_exists` and `phase_review_path` still name their parameter `project_root` even though 33-05's callers now correctly pass an `evidence_root` — the exact mislabeling that produced the original CR-01, preserved in the callee | ⚠️ Warning | A future caller reading the callee's signature could be misled back into the original bug. Not itself a phase-33 correctness failure — the value passed is correct at every current call site. |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 322, 378, 393 | WR-08 (new): evidence-root resolution triplicated rather than bound once — forced by the borrow checker per the plan's own documented rationale | ⚠️ Warning | Style/maintainability, not correctness — confirmed the repetition is intentional and explained in-line, not an oversight. |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 1591-1747 (new tests) | IN-06 (new): the mid-arc negative control packs two independent scenarios into one `#[test]`, so a scenario-A failure would hide the scenario-B assertion / IN-07 (new): new tests set `state.stage = Stage::Code` before driving a Validate outcome, copying a pre-existing sibling pattern | ℹ️ Info | Test-hygiene only; does not affect what the tests currently prove — both scenarios did in fact run and both did in fact assert in this pass's re-run. |
| `.planning/REQUIREMENTS.md` | 19, 23, 51-52 | DOGFOOD-01 and DOGFOOD-02 both still read `[ ]` / "Gaps Found" although both are now satisfied | ℹ️ Info | Recommend updating as part of closing this phase — not touched by 33-05 by deliberate decision (see 33-05-SUMMARY's key-decisions). |

No `TBD`/`FIXME`/`XXX` debt markers found in any of the 7 files this phase modified (re-scanned
directly in this pass, all files, no matches).

### Human Verification Required

None required to certify this phase's own ROADMAP success criteria. All four are proven by a
combination of: direct source reading of the corrected call sites, behavioral unit tests with real
negative controls (re-run live in this pass, not trusted from any SUMMARY), and a real-repository
negative control (33-REVIEW.md's `git ls-tree` comparison on this repo's own `develop` vs `HEAD`)
confirming the tracked-file mechanism the fix depends on.

One item is worth the operator's awareness rather than a blocking gate: 33-05-SUMMARY's own coverage
table (`D3`) explicitly flags that no test in this repository drives a real linked `git worktree` or
a real spawned agent end-to-end — the strongest available confirmation of the phase's literal goal
prose ("a 3+ wave unattended `devflow start` phase can complete") will come from actually running such
a phase against this fixed binary, which is inherently the next phase's job rather than something
phase 33 can self-certify in its own verification pass.

### Gaps Summary

**Both of phase 33's namesake defects are now genuinely fixed and independently re-confirmed in this
pass**, not merely inherited from 33-05-SUMMARY's or 33-REVIEW.md's claims:

- **999.65/DOGFOOD-01** (ROADMAP criteria 1-2): `select_loop_back_fix` now resolves its evidence root
  from `state.worktree_path`, falling back to `project_root` only when no worktree is configured —
  confirmed at all three call sites by direct source read and by two worktree-mode tests re-run live
  in this pass, including a scenario that specifically discriminates against the "probe both roots and
  OR them" plausible-wrong-fix.
- **999.66/DOGFOOD-02** (ROADMAP criteria 3-4): unaffected by the CR-01 fix or its scope; the
  underlying git commit-count query is architecturally required to (and does) read `project_root`
  regardless of worktree mode, confirmed by direct read of `phase_commit_count`'s implementation.

Three items are carried forward as non-blocking `deferred` findings for operator awareness, none of
which is one of the four ROADMAP success criteria and none of which this phase's plans were scoped to
fix: WR-03 (a transient `git` failure can grant a free consecutive-failures reset), WR-06 (3 of 4
tests seeding `consecutive_failures` lack PATH-neutralization, currently harmless), and a new same-class
evidence-root defect at `evaluate_layer0` (`agent_result.rs:2041-2042`) that predates this phase's merge
base and was not touched by any of its 5 plans.

---

_Verified: 2026-08-05_
_Verifier: Claude (gsd-verifier)_

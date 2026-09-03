---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
verified: 2026-09-02T18:08:14Z
status: passed
score: 3/3 roadmap success criteria verified (20/20 plan must-have truths, 3 by override)
behavior_unverified: 0
overrides_applied: 3
overrides:

  - must_have: "The Code stage prompt delivered to Codex and Pi carries the byte-identical policy, so the two agent families do not have different unattended semantics (45-03 must_have truth 2; ROADMAP criterion 3 / DECN-01)."
    reason: >-
      CR-03 confirmed in shipped source (prompt.rs:396-399 -> prompt.rs:567-580):
      fix_prompt carries CODE_STAGE_POLICY in no arm, including the live
      FullExecute dispatch, so a Claude/OpenCode loop-back re-run receives no
      unattended-decision policy. Deferred by recorded operator decision
      (45-REVIEW.md Disposition, 2026-09-02) to backlog 999.115 rather than
      reopening 45-03, alongside prioritizing the CR-01/CR-02 trunk-resolution
      blockers. Accepted 2026-09-02 as this phase's terminal state for DECN-01.
    accepted_by: "Dennis Kim"
    accepted_at: "2026-09-02T00:00:00Z"
  - must_have: "The policy explicitly excludes blocking-human gates from self-resolution — and that exclusion holds for the agent that receives it (45-03 must_have truth 4 / prohibition 1)."
    reason: >-
      CR-04 confirmed in shipped source: CODE_STAGE_POLICY (prompt.rs:86-89)
      withholds authority over blocking-human gates; checkpoint_auto_decide_prompt
      (prompt.rs:537-549) grants exactly that authority into the same resumed
      conversation via relaunch_checkpoint_session. Deferred by recorded
      operator decision to backlog 999.116. Accepted 2026-09-02 as this
      phase's terminal state.
    accepted_by: "Dennis Kim"
    accepted_at: "2026-09-02T00:00:00Z"
  - must_have: "Worktree creation forks from the branch tracking `.planning/` so `preflight_unattended_launch_check` passes out of the box (ROADMAP criterion 1 / AUTO-01) — live end-to-end behavior."
    reason: >-
      AUTO-01 is verified at unit/integration level with a real negative
      control on the fork point (parallel::tests::ensure_phase_worktree_forks_from_the_supplied_base),
      plus the four run-scoped git_flow_for_run consumers and the
      State::base_branch round-trip. No automated test drives `devflow start`
      end to end, and this repo has no committed `base_branch`, so the "out of
      the box" live run is not reachable here without a dedicated setup step.
      Operator decision 2026-09-02: defer the live `devflow start --mode auto`
      end-to-end verification to a later phase and track it as backlog 999.119
      (ROADMAP.md), rather than block the v2.8.0 milestone close on an
      environment limitation that is not a code defect. This reclassifies the
      former `behavior_unverified_items` / `human_needed` entry to a tracked
      deferred override; the live run was NOT performed.
    accepted_by: "Dennis Kim"
    accepted_at: "2026-09-02T00:00:00Z"
re_verification: null
gaps: []
deferred: []
behavior_unverified_items: []
human_verification: []
---

# Phase 45: Unattended Auto-Mode Hardening — Verification Report

**Phase Goal:** Make `--mode auto` launchable and safe out of the box by fixing worktree base detection for `.planning/`, scoping staleness detection to workspace crates, and enforcing merit-based decision checkpoint resolution.
**Verified:** 2026-09-02T18:08:14Z
**Status:** passed (initial pass: gaps_found; then human_needed; then passed by operator override — see Post-Verification Disposition)
**Re-verification:** No — initial verification, updated by hand post-verification per the documented override workflow (`verification-overrides.md`)

## Headline

Two of three roadmap success criteria are fully achieved with strong, negative-controlled
behavioral evidence. The third (DECN-01) is **partially delivered**, and the two holes are
real and confirmed in shipped source — not merely asserted by the review documents. The
deferral of those holes to backlog is a recorded operator decision and is **not** counted
against the phase as a defect; the *incompleteness of the criterion* is what makes this
`gaps_found` rather than `passed`.

Nothing in this report rests on a SUMMARY.md claim. Every finding below was read out of the
compiled source tree at `HEAD` (`4453225`).

## Goal Achievement

### Observable Truths — ROADMAP Success Criteria

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Worktree forks from the branch tracking `.planning/`, so `.planning/config.json` is present and unattended preflight passes (AUTO-01) | ✓ VERIFIED | `config::base_branch` resolver (`config.rs:384-415`, env > file > `develop`); `commands::start` resolves ONCE at `commands.rs:325`, validates via `ensure_base_is_a_local_branch` (`commands.rs:147-170`), persists to `State::base_branch` (`commands.rs:341`), and passes the same value to `ensure_phase_worktree` (`commands.rs:453`) → `worktree::add(.., start_point)` (`worktree.rs:78`). Behavioral test `parallel::tests::ensure_phase_worktree_forks_from_the_supplied_base` **1 passed, 362 filtered out** — asserts the worktree forked from `workspace/example` CARRIES `.planning/config.json` and, as a negative control in the same body, that a `develop` fork does NOT. |
| 2 | `affects_compiled_binary` inspects only `crates/*` members plus root build files, ignoring `.planning/spikes/` (AUTO-02) | ✓ VERIFIED | `staleness.rs:242-299`: parent-segment `..` rejection first, four root build files by exact equality, `.cargo/config.toml` root-anchored, then a `crates/` path-SEGMENT prefix. Scope flag computed once at `staleness.rs:448` and threaded through `combined_staleness` → `ancestry_range_affects_build` / `tree_has_modified_build_inputs` → the predicate. `staleness::tests::spikes_only_dirty_tree_does_not_block_self_dogfood` **1 passed, 362 filtered out**, carrying an in-body negative control (a modified `crates/devflow-cli/src/main.rs` must still `Err`). Three `affects_compiled_binary*` tests **3 passed, 360 filtered out**, including an explicit zero-regression control that the UNSCOPED rule is unchanged. |
| 3 | Unattended `decision` checkpoint no longer blindly takes the first option; Code-stage policy instructs merit evaluation and recorded reasoning (DECN-01) | ✗ FAILED (partial) | The policy EXISTS and is correct in content: `CODE_STAGE_POLICY` (`prompt.rs:62-89`) forbids positional choice, treats "recommended" as evidence not verdict, demands the *comparison* that produced the choice, and carves out `blocking-human` / package-verification. It reaches `code_stage_prompt` (`prompt.rs:379`) and `workflow_code_prompt`'s `FullExecute \| None` arm (`prompt.rs:469`). **But** it does NOT reach `fix_prompt` (`prompt.rs:567-580`) — the Claude/OpenCode loop-back — for any arm, while the Codex/Pi sibling arm does carry it. See gaps 1 and 2. |

**Score:** 2/3 roadmap criteria verified.

### Plan Must-Have Truths

| Plan | Truths | Verified | Failed |
|------|--------|----------|--------|
| 45-01 (AUTO-01) | 12 | 12 | 0 |
| 45-02 (AUTO-02) | 6 | 6 | 0 |
| 45-03 (DECN-01) | 7 | 5 | 2 |

**Aggregate:** 18/20 plan must-have truths verified.

45-01 spot-checks beyond the criterion: `ensure_base_ref_current` fail-open contract
(`preflight.rs:517-518` prints the literal `fail-open`); `main` refusal with no absolute path
or username (`config.rs:341-358`, pinned by `config.rs:969-977`); `HookContext.git_flow` now
consumed rather than re-defaulted at all six `hooks.rs` sites (125, 132, 181, 246, 285, 296)
from the single production construction at `pipeline_outcomes.rs:1067-1079`; `--no-worktree`
forks from the same resolved value (`commands.rs:467`); `State::base_branch` round-trips and
tolerates pre-field state (`state.rs:1025-1058`); `OPERATIONS.md:117` documents
`DEVFLOW_BASE_BRANCH` as a string literal so `doc_check::source_read_env_vars` can see it.

### 45-03's Two Failed Truths

- **Truth 2 — "byte-identical policy across both agent families."** Falsified. `fix_prompt`
  has no policy in any arm; `workflow_code_prompt`'s `FullExecute` arm has it. The asymmetry
  is invisible to the test suite because `code_policy_is_identical_across_both_renderers`
  (`prompt.rs:776-790`) exercises `fix: None` only, and the absence test
  (`prompt.rs:792-819`) exercises only the *workflow-style* GapsOnly/AuditFix. The
  Claude-style `FullExecute` prompt is asserted by neither — it renders the same
  `/gsd-execute-phase {phase}` command, so by the phase's own stated contract ("only
  full-execute Code prompts may carry the shared policy") it should carry it and does not.
- **Truth 4 / prohibition 1 — "excludes `blocking-human` from self-resolution."** The
  constant complies; the delivered system does not. `checkpoint_auto_decide_prompt` grants
  precisely the authority `CODE_STAGE_POLICY` withholds, into the same resumed conversation.

Both are dispositioned **Deferred** in `45-REVIEW.md` § Disposition to backlog **999.115**
(ROADMAP.md:422) and **999.116** (ROADMAP.md:402), which do exist. The deferral is a
legitimate recorded decision; it does not make criterion 3 true.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/config.rs` | base_branch resolver, validator, run-scoped git-flow | ✓ VERIFIED | +560 lines; fail-hard on explicit values, fail-soft only on the Default arm; unparseable TOML refused rather than defaulted |
| `crates/devflow-core/src/git.rs` | `GitFlow::with_config` / `for_project`; trunk protection | ✓ VERIFIED | `for_project` (147-151) reads the resolved config; `cleanup_merged` (346-372) keeps `MAIN`/`DEVELOP` protected regardless of resolved trunk (CR-01 fix) |
| `crates/devflow-cli/src/parallel.rs` | worktree forks from supplied base | ✓ VERIFIED | `ensure_phase_worktree(.., base)` → `worktree::add(.., base, ..)` at :42 |
| `crates/devflow-cli/src/commands.rs` | single resolution, both fork paths, guards | ✓ VERIFIED | resolve→validate→persist→announce→guards→fork, all on one value |
| `crates/devflow-cli/src/preflight.rs` | probes the run's persisted base | ✓ VERIFIED | `state.base_branch` preferred at :657, resolver only as fallback (CR-45-01 fix) |
| `crates/devflow-core/src/hooks.rs` | `ctx.git_flow` consumed | ✓ VERIFIED | six `GitFlow::with_config(&ctx.project_root, ctx.git_flow.clone())` sites |
| `crates/devflow-core/src/monitor.rs` | commit evidence on the run's base | ✓ VERIFIED | `git_flow_for_run(project_root, persisted_base)` at :1276-1277 (CR-45-06 fix) |
| `crates/devflow-core/src/ship_evidence.rs` | merge evidence on the run's base | ✓ VERIFIED | :154, :166 (CR-45-07 — the instance neither external lane found) |
| `crates/devflow-cli/src/staleness.rs` | scoped predicate | ✓ VERIFIED | +652 lines with the predicate's first direct unit tests |
| `crates/devflow-core/src/prompt.rs` | shared policy constant on both renderers | ⚠️ PARTIAL | constant is shared and correct; loop-back renderer omits it |
| `OPERATIONS.md` | env var documented | ✓ VERIFIED | :117 |
| `docs/guides/unattended-mode.md` | operator guidance | ✓ VERIFIED | :115-118 |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `config::base_branch` | `git_flow_for_project` / `GitFlow::for_project` | one resolver feeds fork point and merge target | ✓ WIRED |
| `commands::start` | `worktree::add` start_point | resolve → `ensure_base_ref_current` → `ensure_phase_reachable_on_base` → `ensure_phase_worktree` | ✓ WIRED |
| `State::base_branch` | preflight / monitor / ship_evidence / hooks | `git_flow_for_run(root, persisted)` | ✓ WIRED (4/4 sites) |
| `is_self_dogfood_workspace` | `affects_compiled_binary` | `workspace_scoped` flag computed once at :448 and threaded | ✓ WIRED |
| `ensure_phase_worktree` → `state.worktree_path` | `unattended_config_condition` | `launch_root = state.worktree_path.unwrap_or(project_root)` (`preflight.rs:1162`) | ✓ WIRED |
| `CODE_STAGE_POLICY` | `code_stage_prompt` → `render_claude_style` | first Code pass | ✓ WIRED |
| `CODE_STAGE_POLICY` | `workflow_code_prompt` → `render_workflow_style` | Codex/Pi, incl. FullExecute | ✓ WIRED |
| `CODE_STAGE_POLICY` | `fix_prompt` → `render_claude_style` | Claude/OpenCode loop-back | ✗ NOT WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Full workspace suite | `cargo test --workspace` (run ONCE; cargo's own exit captured, not a pipeline's) | `CARGO_EXIT=0`; 1235 passed, 0 failed across 29 `test result: ok` lines | ✓ PASS |
| Lint gate | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | ✓ PASS |
| Format gate | `cargo fmt --check` | exit 0 | ✓ PASS |
| AUTO-01 fork point | `cargo test -p devflow --bin devflow ensure_phase_worktree_forks_from_the_supplied_base` | `1 passed; 362 filtered out` | ✓ PASS |
| AUTO-02 predicate | `cargo test -p devflow --bin devflow affects_compiled_binary` | `3 passed; 360 filtered out` | ✓ PASS |
| AUTO-02 end-to-end | `cargo test -p devflow --bin devflow spikes_only_dirty_tree_does_not_block_self_dogfood` | `1 passed; 362 filtered out` | ✓ PASS |

**What these do not establish.** The first `cargo test --workspace` attempt in this
verification was piped through `tail`, so its exit code was `tail`'s and carried no
information; it was discarded and the suite re-run once with cargo's own status captured.
Counts were derived two independent ways (summing `N passed` and summing `N failed`) because
a single count can never look wrong. The `filtered out` counts above are non-zero, so these
are real named-test matches, not the `--exact`-matches-nothing false green. One green suite
is a weak reliability bound — and known flake **999.114** (CLI test binary blanks
process-global `PATH`) did not manifest this run, which is luck rather than evidence of
absence.

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|---|---|---|---|
| all 11 phase source files | `TODO`/`FIXME`/`XXX`/`TBD`/`HACK`/`unimplemented!`/`todo!` | — | **None found.** Clean. |
| repo root | untracked `run_test` (4.3 MB compiled binary), `run_test.rs`, `test_parse.rs` | ⚠️ WARNING | Phase-45 review scratch debris, not gitignored (`git check-ignore` exit 1; all three show `??`). A `git add -A` would sweep a 4.3 MB binary into a commit — the exact failure CLAUDE.md records from 2026-08-02. Delete or ignore before shipping. |
| `crates/devflow-core/src/workflow.rs:189` | fixed `.tmp` filename in `write_state_atomic` | ⚠️ WARNING | Pre-existing, out of phase scope, correctly deferred — but the deferral was never filed (see gap 4). |

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| AUTO-01 | 45-01 | ✓ SATISFIED (register stale) | Resolver + both fork paths + 4 run-scoped consumers + behavioral test with negative control. REQUIREMENTS.md still says Pending — see gap 3. |
| AUTO-02 | 45-02 | ✓ SATISFIED | Scoped predicate, end-to-end fixture, controls in both directions. Register agrees (Complete). |
| DECN-01 | 45-03 | ⚠️ PARTIAL | Policy delivered on the first Code pass for both families; absent from the Claude/OpenCode loop-back; contradicted by the resume prompt in-session. Register says Pending (honest); `45-03-SUMMARY.md` claims complete (overclaim). |

No orphaned requirements: REQUIREMENTS.md maps exactly AUTO-01, AUTO-02, DECN-01 to Phase 45,
and all three are claimed by a plan.

### Gaps Summary

The engineering on AUTO-01 and AUTO-02 is genuinely strong — one resolver feeding both the
fork point and the merge target, the persisted-base class chased to four call sites (one of
which neither external lane found), a dangerous `cleanup_merged` trunk-deletion regression
that 45-01 itself introduced caught and fixed, and negative controls in the tests that would
actually fail if the code were gutted. Those two criteria are achieved.

Criterion 3 is not. The policy text is well written and correctly carved out, but it is
delivered to three of the four Code-prompt paths, and the one it misses — the Claude/OpenCode
`FullExecute` loop-back — is the path an unattended run reaches *after Validate fails*, which
is exactly when decision checkpoints accumulate. Claude is this project's primary driver, so
the missing path is the common case, not the exotic one. Layered on that, the resume prompt
tells the agent to do the one thing the standing policy forbids, in the same conversation.
An unattended-mode contract that resolves by undocumented recency heuristic is not a contract.

Both are deferred to real backlog entries by operator decision, and that deferral is
**not** treated here as a defect. But a deferral changes who owns the work, not whether the
criterion is true. Criterion 3's own wording — "no longer blindly takes the first option" —
does not hold on the loop-back path for the primary agent, so the phase goal is achieved for
two of its three components.

Two bookkeeping gaps ride along: AUTO-01 is done but still reads Pending in the requirement
register, and the review's own `write_state_atomic` deferral was never filed as a backlog
entry, so it is currently deferred to nowhere.

---

## Post-Verification Disposition (2026-09-02, by operator instruction)

The three gaps above resting on something other than the live-run behavior are resolved. The
frontmatter's `overrides:` array, `gaps: []`, and `status:` reflect the state below, not the
original `gaps_found` pass — this section states the delta explicitly rather than silently
rewriting the narrative above it.

| Item | Original verdict | Disposition |
|---|---|---|
| Criterion 3 / 45-03 truth 2 (CR-03) | ✗ FAILED (partial) | **PASSED (override)** — accepted by Dennis Kim, citing backlog 999.115. See frontmatter `overrides:`. |
| Criterion 3 / 45-03 truth 4 (CR-04) | ✗ FAILED (partial) | **PASSED (override)** — accepted by Dennis Kim, citing backlog 999.116. See frontmatter `overrides:`. |
| REQUIREMENTS.md traceability (gap 3) | ✗ FAILED | **✓ VERIFIED** — AUTO-01 marked Complete in `REQUIREMENTS.md` (checkbox + traceability table); `45-03-SUMMARY.md`'s `requirements-completed: [DECN-01]` overclaim corrected to `requirements-partial`, naming both backlog items. |
| Undischarged review deferral (gap 4) | ✗ FAILED | **✓ VERIFIED** — `write_state_atomic`'s fixed-`.tmp`-filename TOCTOU filed as backlog **999.118** (`ROADMAP.md`). `45-EXTERNAL-REVIEW.md`'s own disposition table updated to point at it. |
| Criterion 1 / AUTO-01 live end-to-end run (`behavior_unverified_items`) | ⚠ human_needed | **PASSED (override)** — deferred to a later phase by operator decision 2026-09-02 (`/gsd-verify-work 45`), tracked as backlog **999.119**. Live run NOT performed; AUTO-01 stays unit/integration-verified. |

**Deferred to a later phase (2026-09-02, second disposition pass, by operator instruction):**
the sole `behavior_unverified_items` entry — a live `devflow start --mode auto` end-to-end run
exercising the configured-base fork/preflight/merge chain — was NOT performed. The operator
decided in `/gsd-verify-work 45` (2026-09-02) that this live run "is most likely not possible
yet" in this environment and deferred it to a later phase, filed as backlog **999.119**
(`ROADMAP.md`). It is accepted here as a third `PASSED (override)` (see frontmatter `overrides:`),
which reclassifies the former `human_needed` item to a tracked deferred override. This is a
deliberate, operator-authorized reclassification recorded in full — not a silent suppression.
`status:` is now `passed`; `gsd_run query phase.complete` will accept the phase. The AUTO-01
code is verified at unit/integration level (with a real fork-point negative control); what
remains unverified is only the live end-to-end behavior, and 999.119 owns closing that gap.

Also untouched by this disposition: the anti-pattern entry above naming `run_test`,
`run_test.rs`, `test_parse.rs`, and the stray `.opencode/opencode.json`/`.gsd/`/
`.planning/milestone.lock`/`.planning/state.json` working-tree state — those are working-tree
hygiene, not phase correctness, and are handled separately (see the session's own commit
history around this timestamp).

---

_Verified: 2026-09-02T18:08:14Z_
_Verifier: Claude (gsd-verifier)_
_Disposition: Claude, 2026-09-02, by direct operator instruction (two passes: gap resolution, then the AUTO-01 live-run deferral via /gsd-verify-work 45)_

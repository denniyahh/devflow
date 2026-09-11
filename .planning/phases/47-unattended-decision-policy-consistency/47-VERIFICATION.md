---
phase: 47-unattended-decision-policy-consistency
verified: 2026-09-11T21:27:32Z
status: gaps_found
score: 19/23 must-haves verified
covered_files:
  - Cargo.toml
  - Cargo.lock
  - .gitignore
  - scripts/check.sh
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap
  - crates/devflow-cli/tests/ci_parity_guards.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - docs/guides/unattended-mode.md
  - .planning/user/DEV-SETUP-CHECKLIST.md
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-01-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-01-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-02-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-02-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-03-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-03-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-04-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-04-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-05-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-05-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-CONTEXT.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-RESEARCH.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-REVIEW.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md
covered_digest: "v1:sha256:02b123887d69fb90f1c44efabae0f916d4b9d2cb4365383a067a72a0335f9901"
behavior_unverified: 3
overrides_applied: 0
gaps:
  - truth: "A resumed Claude session receives one consistent, usable instruction that both permits the narrow blocking-human resolution and records the required decision reasoning."
    status: failed
    reason: "The delivered policy and resume instruction require substantive reasoning in the final message, but COMPLETION_PROTOCOL requires that final message to be exactly one DEVFLOW_RESULT line with nothing after it. The required audit record and the completion parser contract cannot both be satisfied."
    artifacts:
      - path: "crates/devflow-core/src/prompt.rs"
        issue: "COMPLETION_PROTOCOL at lines 46-55 conflicts with CODE_STAGE_POLICY at 117-120 and GATE_RESOLUTION_RULE/checkpoint_auto_decide_prompt at 67-69 and 605-615."
    missing:
      - "Define a parser-compatible decision-audit envelope (for example a DEVFLOW_DECISION record before an unchanged terminal DEVFLOW_RESULT) and update both prompt instructions."
      - "Add a test at the result-parser boundary proving the decision record and terminal result coexist."
behavior_unverified_items:
  - truth: "A wording change to a policy-carrying prompt fails scripts/check.sh test."
    test: "Deliberately drift a committed snapshot, run scripts/check.sh test, then restore the exact baseline."
    expected: "The command exits non-zero without rewriting the baseline."
    why_human: "The current source guard proves the invocation contains the right environment controls, but this verification did not mutate a tracked baseline; the supplied workspace cargo-test run did not exercise this transition."
  - truth: "INSTA_FORCE_UPDATE=1 cannot re-bless a drifted snapshot green."
    test: "Deliberately drift a committed snapshot, run INSTA_FORCE_UPDATE=1 scripts/check.sh test, then restore it."
    expected: "The command exits non-zero and the drift remains; no baseline is re-blessed."
    why_human: "This is the load-bearing environment/side-effect transition. The named source guard passes, but it does not run insta against a drifted artifact."
  - truth: "A clean tree passes scripts/check.sh test under its hardened environment."
    test: "Run scripts/check.sh test on the clean worktree."
    expected: "Exit 0 with no .snap.new files."
    why_human: "The supplied bounded cargo-test regression was green, but it did not execute scripts/check.sh's env -u / INSTA_UPDATE wrapper; it is not proof of this script-level path."
---

# Phase 47: Unattended Decision Policy Consistency Verification Report

**Phase Goal:** An unattended agent receives the same merit-based decision instruction everywhere it is asked to make a Code-stage decision, and is never handed contradictory instructions about resolving a `blocking-human` gate.

**Verified:** 2026-09-11T21:27:32Z  
**Status:** gaps_found  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Snapshot wording drift fails the required test path. | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `check.sh` has the hardened invocation; the named source guard passes. No current drifted-baseline execution was performed. |
| 2 | `INSTA_FORCE_UPDATE=1` cannot re-bless a drifted baseline. | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no` is the sole test invocation and its source guard passes; the side-effect path was not re-executed. |
| 3 | A clean tree passes `scripts/check.sh test`. | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | The supplied full bounded `cargo test` was green, but it did not establish the wrapper path. |
| 4 | `.snap.new` is ignored while `.snap` is tracked. | ✓ VERIFIED | Direct `git check-ignore --no-index`: `.snap.new` exit 0; `.snap` exit 1. Named guard passed. |
| 5 | FullExecute loop-backs deliver policy to claude, opencode, hermes, and antigravity. | ✓ VERIFIED | `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter` passed (1 passed; 769 filtered). |
| 6 | Codex and Pi remain passing controls, and GapsOnly/AuditFix omit policy. | ✓ VERIFIED | Six-adapter test has explicit render-style controls; claude-style omission test passed (1 passed; 769 filtered). |
| 7 | `workflow_code_prompt` behavior remains unchanged. | ✓ VERIFIED | It shares the exhaustive arm helper; the preserved workflow omission control is unchanged from the phase fork and the current helper has only FullExecute/None true. |
| 8 | The loop-back presence test was red before the feature and green after it. | ✓ VERIFIED | Exact history contains `test(47-02)` `59e4334` before `feat(47-02)` `062ebf7`; the live named presence test passes. This establishes the TDD ordering and current behavior, not a new execution of the historical revision. |
| 9 | Resumed Claude turns receive one consistent, usable gate-and-audit instruction. | ✗ FAILED | The gate condition is textually consistent, but the required reasoning cannot coexist with the exact-only final `DEVFLOW_RESULT` completion contract. |
| 10 | The exception is limited to `blocking-human`; package verification remains unconditional. | ✓ VERIFIED | `package_verification_prohibition_is_unconditional` passed (1 passed; 769 filtered) and the pinned text is in the rendered snapshots. |
| 11 | `checkpoint_auto_decide_prompt` explicitly names `blocking-human`. | ✓ VERIFIED | Live prompt code and `the_gate_rule_has_one_definition_site` test passed. |
| 12 | Both layered contradiction tests had a real red predecessor and are now green. | ✓ VERIFIED | Exact `test(47-03)` `94ef699` precedes `feat(47-03)` `cab8d54`; current core and CLI named tests pass. The provided nonexistent `47-99` control produced no commits. |
| 13 | Turn one and turn two come from production constructors and are never joined. | ✓ VERIFIED | Delivery test passed (1 passed; 363 filtered); it captures `PipeOwning` turn one and `argv[1]` turn two, asserts distinction/non-containment, and snapshots separately. |
| 14 | Every changed policy prompt has a committed reviewed baseline. | ✓ VERIFIED | Seven tracked core snapshots all carry the policy heading; two named CLI snapshots are tracked. |
| 15 | The two delivered turns are separate, not a joined baseline. | ✓ VERIFIED | Exactly one named file for each turn; `joined=0`; the delivery test passed. |
| 16 | Snapshot scope excludes non-policy arms. | ✓ VERIFIED | Core snapshot set is seven policy-carrying FullExecute artifacts; GapsOnly/AuditFix are covered by omission tests instead. |
| 17 | The unattended-mode guide describes the actual parked-gate override. | ✓ VERIFIED | Guide states `GateAction::Advance` directly launches while skipping the adjudicated preflight check. |
| 18 | The never-auto-approved statement is qualified, not inverted. | ✓ VERIFIED | Guide retains the rule and names the resumed-to-resolve exception after human approval. |
| 19 | ROADMAP and REQUIREMENTS record the four/two adapter split. | ✓ VERIFIED | Both name hermes and antigravity; codex/pi are identified as workflow-style controls. |
| 20 | Phase 49 has exactly one evidence-standard pointer without changed criteria count. | ✓ VERIFIED | Phase 49 has one pointer and five numbered criteria. |
| 21 | DECN-03's behavioral evidence distinguishes Followed, Not followed, and Void. | ✓ VERIFIED | `47-PHASE49-OBSERVATION.md` defines all three and explains that the event precedes spawn. |
| 22 | The two D-13 defects are placed on the backlog. | ✓ VERIFIED | ROADMAP has separately actionable 999.125 and 999.126 entries with fix shape and acceptance. |
| 23 | Requirements DECN-02 and DECN-03 are accounted for. | ✓ VERIFIED / ✗ BLOCKED | DECN-02 is satisfied by the rendered-path tests. DECN-03's textual gate-condition consistency is present, but its mandated final-message audit trail is impossible under the current completion contract. |

**Score:** 19/23 truths verified (3 present but behavior-unverified; 1 failed).

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/prompt.rs` | Shared arm and gate policy | ✗ BLOCKER | Substantive and wired, but has the final-message contract conflict above. |
| `crates/devflow-cli/src/pipeline_launch.rs` | Production resume shape and delivery test | ✓ VERIFIED | `relaunch_checkpoint_session` emits before spawning, then calls `resume_launch_shape`; named delivery test passes. |
| `scripts/check.sh` + CI | Snapshot guard in required Test job | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | CI calls `scripts/check.sh test`; source guard pins its sole hardened invocation. Runtime drift transition was not rerun. |
| Core and CLI snapshot directories | Policy baselines and separate turn baselines | ✓ VERIFIED | 7 core + 2 CLI tracked snapshots; no `.snap.new` on disk. |
| `docs/guides/unattended-mode.md` | Accurate parked-gate guide | ✓ VERIFIED | Matches the direct preflight/Advance path. |
| ROADMAP, REQUIREMENTS, Phase 49 observation | Scope, requirement, and evidence handoff | ✓ VERIFIED | Four/two split, claude-only limit, pointer, and Followed/Not followed/Void standard exist. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `fix_prompt` | `CODE_STAGE_POLICY` | FullExecute helper branch | ✓ WIRED | `code_policy_applies_to_fix_arm(Some(fix_type))` selects only FullExecute. |
| `render_claude_style` | `fix_prompt` | `StageIntent::Code { fix: Some(_) }` | ✓ WIRED | Live renderer dispatches to `fix_prompt`; six-adapter test exercises it. |
| `CODE_STAGE_POLICY` / resume prompt | delivered turns | `resolve_launch_shape` and `resume_launch_shape` | ✓ WIRED | Named delivery test exercises both production constructors. |
| `scripts/check.sh run_test` | CI Test job | `.github/workflows/ci.yml` | ✓ WIRED | CI invokes `scripts/check.sh test`; named parity guard passes. |
| Phase 49 criterion 4 | evidence standard | `47-PHASE49-OBSERVATION.md` | ✓ WIRED | One direct pointer in the unchanged five-criterion section. |

### Data-Flow Trace (Level 4)

| Artifact | Data variable | Source | Produces real data | Status |
|---|---|---|---|---|
| `fix_prompt` | rendered loop-back prompt | `StageIntent::Code` through each driver | Yes — rendered production string | ✓ FLOWING |
| `resume_launch_shape` | resume `argv[1]` | `checkpoint_auto_decide_prompt(phase)` | Yes — production command shape | ✓ FLOWING |
| snapshots | prompt contents | above rendered strings | Yes — committed golden output | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Six-adapter FullExecute policy | named core test | 1 passed; 769 filtered | ✓ PASS |
| Claude-style omitted arms | named core test | 1 passed; 769 filtered | ✓ PASS |
| Gate-rule text | named core test | 1 passed; 769 filtered | ✓ PASS |
| Package-only carve-out | named core test | 1 passed; 769 filtered | ✓ PASS |
| Separate production turns | named CLI test | 1 passed; 363 filtered | ✓ PASS |
| Snapshot-env source guard | named CLI test | 1 passed; 25 filtered | ✓ PASS |
| Snapshot ignore split | named CLI test | 1 passed; 1 filtered | ✓ PASS |

The supplied bounded full `cargo test` regression was green. It supports the tests it ran, not the unexecuted snapshot-drift side effect, a live resumed transcript, a successful agent spawn, or model instruction-following.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DECN-02 | 47-01, 47-02, 47-04, 47-05 | FullExecute policy reaches the four claude-style loop-back adapters; other arms remain excluded. | ✓ SATISFIED | Live six-adapter and omission tests, committed snapshots, direct adapter split. |
| DECN-03 | 47-01, 47-03, 47-04, 47-05 | Resumed session has one consistent instruction about resolving a blocking-human gate. | ✗ BLOCKED | Gate condition is consistent in both turns, but mandatory reasoning-in-final-message conflicts with the terminal result protocol. |

No orphaned Phase 47 requirements were found: both DECN IDs are declared by its plans and mapped in REQUIREMENTS.md.

### Anti-Patterns and Review Findings

| File | Line | Pattern | Severity | Impact |
|---|---:|---|---|---|
| `crates/devflow-core/src/prompt.rs` | 46-55, 117-120, 605-615 | Mutually exclusive final-message obligations | 🛑 BLOCKER | The decision record Phase 47 requires cannot be emitted without violating the result protocol. |
| `scripts/test-phase-worktree-guard.sh` | 49-55 | Fixture inherits global Git hooks | ⚠️ WARNING | Review's test-isolation finding is real but not a Phase 47 must-have: this file was not modified by Phase 47 and no phase truth relies on its execution. It remains a separate reliability issue. |

No phase-modified source file has an unreferenced `TBD`, `FIXME`, or `XXX` marker. The only scan hits were the workspace's `todo = "warn"` lint configuration and a pre-existing `{N}` placeholder, neither a completion stub.

## Phase 49 Boundary

Source and constructor-level tests do **not** establish actual model behavior in a live unattended run. Phase 49 criterion 4 and `47-PHASE49-OBSERVATION.md` correctly carry that obligation: they require a live Claude run to classify the outcome as Followed, Not followed, or Void and reject the pre-spawn event as proof of a decision.

That handoff does not defer this blocker. Phase 49 observes model behavior; it does not make Phase 47's incompatible completion/audit instructions satisfiable.

## Gaps Summary

Phase 47 materially delivers DECN-02 and most of DECN-03's source-level wiring, including real adapter and excluded-arm controls. It does not achieve its full decision-policy goal because its instructions require an audit record in a final message that the same prompt restricts to one machine-readable result line. This is a code-level, reproducible contradiction, not a question for model observation or a documentation issue.

_Verified: 2026-09-11T21:27:32Z_  
_Verifier: the agent (gsd-verifier)_

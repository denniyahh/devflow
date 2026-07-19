---
phase: 17-pipeline-dogfood-followup
reviewed: 2026-07-18T00:00:00Z
depth: standard
files_reviewed: 12
files_reviewed_list:
  - crates/devflow-cli/build.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
findings:
  critical: 1
  warning: 2
  info: 3
  total: 6
status: issues_found
---

# Phase 17: Code Review Report

**Reviewed:** 2026-07-18T00:00:00Z
**Depth:** standard
**Files Reviewed:** 12
**Status:** issues_found

## Summary

This phase adds build-provenance embedding (`build.rs`), an outcome-policy
dispatch table (`outcome_policy.rs`), two new `AgentStatus` variants
(`ResourceKilled`/`AgentUnavailable`), an infra-failure counter distinct from
`consecutive_failures`, a preflight readiness gate, a self-dogfood
build-staleness hard-block, and a `devflow resume` command. The diff is large
(~2,000 lines across 8 files) but heavily test-covered, and the workspace
compiles cleanly (`cargo check --workspace`).

The implementation is generally careful — argv-array `Command` invocation
everywhere (no shell injection surface), `truncate_reason` applied
consistently before agent-controlled text reaches gates/events, exhaustive
`match` with no wildcard arm on both `decide_action` and `AgentStatus`
dispatch, and extensive unit tests exercising the new branches. However, one
genuine logic defect was found in the new `infra_failures` counter (never
reset, unlike its `consecutive_failures` sibling), and the self-dogfood
staleness gate has a real, if partially-acknowledged, coverage gap. See
findings below.

## Critical Issues

### CR-01: `infra_failures` is never reset, unlike `consecutive_failures` — will cause premature phase aborts

**File:** `crates/devflow-core/src/state.rs:41` (field), `crates/devflow-core/src/mode.rs:20-30` (ceiling), `crates/devflow-cli/src/main.rs:1285-1294` (`handle_infra_outcome`), `crates/devflow-cli/src/main.rs:1613-1635` (`transition`)

**Issue:** `state.rs:41`'s doc comment explicitly calls this field "**Consecutive** infrastructure-class faults ... Gates at `MAX_INFRA_FAILURES`," and `mode.rs:20-30` documents `MAX_INFRA_FAILURES` as "bounding a **stuck loop** to at most 5 unobserved cycles before a terminal abort" — both descriptions imply the counter should reflect a run of infra faults with no progress in between, mirroring `consecutive_failures`'s behavior.

But `consecutive_failures` is explicitly reset to `0` on every successful stage transition (`main.rs:1622`, inside `transition()`), while `infra_failures` is **only ever set at `State::new()`** (`state.rs:105`) and incremented via `saturating_add` in `handle_infra_outcome` (`main.rs:1291`) and `handle_rate_limited_outcome` (`main.rs:1337-1341`). No code path anywhere in `main.rs` or `devflow-core` ever decrements or resets `infra_failures` after a successful transition, `finish_workflow`, or otherwise. I verified this with `grep -rn infra_failures` across `devflow-core/src` and `devflow-cli/src/main.rs`: it appears only in the ceiling check, the two increment sites, serde plumbing, and tests — never in `transition()`, `loop_back_to_code()`, or any success path.

Consequence: `infra_failures` is a **lifetime-of-the-phase** counter, not a "stuck loop" counter. A phase that hits five separate, well-spaced, successfully-auto-resumed rate limits or OOM kills across its Define → Plan → Code → Validate → Ship lifecycle (or across several Code↔Validate loop-backs, each of which re-launches via `launch_stage` and can independently hit a transient rate limit) will hard-abort at the fifth occurrence via `gate_or_abort_infra` (`main.rs:1306-1317`), even though every single occurrence up to that point was resolved cleanly and the phase made real forward progress between them. This directly contradicts the stated purpose of D-08 ("infra faults are not the agent's fault... a higher ceiling tolerates transient cloud outages/OOM blips") — the ceiling is bypassing exactly the scenario it claims to tolerate once faults are spread across stage boundaries rather than clustered in one stuck loop.

**Fix:** Reset `infra_failures` to `0` alongside `consecutive_failures` wherever forward progress is confirmed — at minimum in `transition()` (`main.rs:1622`, next to `state.consecutive_failures = 0;`):

```rust
fn transition(project_root: &Path, state: &mut State, to: Stage) -> Result<(), CliError> {
    let from = state.stage;
    let _ = run_checkout_hooks(project_root, state, &hooks::hooks_for_transition(from, to), to);
    state.stage = to;
    state.consecutive_failures = 0;
    state.infra_failures = 0; // reset alongside consecutive_failures: infra_failures is
                               // documented as "consecutive" (state.rs:41) and the ceiling
                               // is meant to bound a stuck loop, not the phase's lifetime.
    state.gate_pending = false;
    workflow::save_state(state)?;
    ...
}
```
If the intent is genuinely a lifetime budget rather than a consecutive counter, the doc comments on `state.rs:41` and `mode.rs:20-30` should be corrected instead to say so explicitly, and the field renamed away from the "consecutive" framing to avoid the next reader making the same assumption the tests/docs currently encode.

## Warnings

### WR-01: Self-dogfood staleness ancestry check does not detect the most common "committed, forgot to rebuild" case

**File:** `crates/devflow-cli/src/main.rs:855-927` (`embedded_commit_is_stale`, `tracked_source_newer_than_build`, `combined_staleness`)

**Issue:** `embedded_commit_is_stale` treats `embedded_commit` as Fresh whenever `git merge-base --is-ancestor <embedded_commit> HEAD` exits 0 — i.e., whenever the embedded commit is *any* ancestor of current HEAD, however many commits behind. `tracked_source_newer_than_build` (the composite's other arm) is only evaluated when the working tree is dirty (`status --porcelain` non-empty); a clean tree short-circuits straight to the ancestry result (`main.rs:896-899`).

Combined effect: the most common real staleness scenario — a developer commits new code on a linear/fast-forward history (no rebase, no squash-merge divergence), never rebuilds, and re-runs the old binary — is **not detected**. The embedded commit remains an ancestor of the new HEAD (ancestry says Fresh) and the tree is clean (mtime arm is skipped entirely), so `combined_staleness` reports Fresh even though the running binary is now several commits behind the checked-out source. This is exactly the "Phase 16 false-evidence incident" class this gate exists to prevent (per `17-05-PLAN.md`'s own framing), yet it slips through whenever the staleness arises from ordinary linear commits rather than a rebase/squash-merge divergence or an uncommitted dirty edit.

This gap is partially self-acknowledged in the research doc (`17-RESEARCH.md:502-511`, "D-19 Open Question 2": "an ancestor-of-HEAD clean tree is already covered by the ancestry arm") but the underlying assumption — that an ancestor-of-HEAD clean tree can't be stale — is exactly the case that's unguarded.

**Fix:** At minimum, document this residual gap prominently at the call site (`enforce_build_staleness`, `main.rs:987-1027`) so operators don't assume the hard-block is a complete guarantee. A stronger fix would compare `embedded_commit == HEAD` (not just ancestry) when the tree is clean, warning (not hard-blocking, to avoid the "alarms you learn to ignore" trap this design explicitly wants to avoid) whenever the two differ, so a merely-behind-by-N-commits binary is at least visible rather than silently marked Fresh.

### WR-02: `infra_failures` ceiling interacts with unbounded Code↔Validate loop-backs without any visibility into which stage burned the budget

**File:** `crates/devflow-cli/src/main.rs:1298-1373` (`gate_or_abort_infra`, `handle_rate_limited_outcome`)

**Issue:** Related to CR-01 but distinct: because `infra_failures` is shared across every stage and every loop-back iteration, and the abort message (`main.rs:1310-1315`) only reports the aggregate count and ceiling ("infrastructure failures reached the ceiling (N of N)"), an operator debugging a hard-aborted phase has no way to tell from the abort message alone whether the five faults were clustered (genuinely stuck) or spread across the phase's entire history (CR-01's scenario). `events.jsonl` does retain per-event `stage`/`reason` fields, so the information exists, but the terminal abort message itself doesn't point there.

**Fix:** Once CR-01 is fixed (reset on transition), this becomes moot for the common case; if the counter is intentionally kept as a lifetime budget instead, the abort message should say so explicitly and point at `devflow history <phase>` for the breakdown.

## Info

### IN-01: `is_self_dogfood_workspace` string-scan is fragile against non-literal `members` arrays

**File:** `crates/devflow-cli/src/main.rs:939-956`

**Issue:** The scan locates the first `[` after the first literal occurrence of the substring `"members"` anywhere in `Cargo.toml`, then substring-matches `"crates/devflow-core"` / `"crates/devflow-cli"` inside that bracketed region. This is explicitly a documented "sanctioned middle ground" (no TOML parser dependency), but it will silently fail to detect the workspace (never block, only warn) if the real `members` array ever uses a glob (`"crates/*"`) or if `Cargo.toml` grows a comment containing the word `members` before the actual array. Neither is true of the current `Cargo.toml` (verified: explicit paths), so this is not exploitable today, but it's a latent false-negative if the workspace manifest is ever refactored.

**Fix:** No action required now; consider adding a regression test that fails if `Cargo.toml`'s `members` array style changes away from explicit paths (e.g., a `cargo metadata` cross-check in CI), since this function silently degrades from "hard block" to "never fires" rather than erroring.

### IN-02: `evaluate_layer0`'s stage-scope expansion means a Plan stage that authors an `external_verify` declaration gates on itself

**File:** `crates/devflow-core/src/agent_result.rs:704-796`

**Issue:** Per D-05/D-06, Layer 0 now evaluates on every stage rather than only Code. Combined with `external_verify_commands` (`verify.rs:29`) scanning *all* of a phase's `PLAN.md` files regardless of which stage is currently advancing, the very first stage whose plan introduces an `external_verify:` declaration will itself be blocked (`"external verification is not approved"`) until `DEVFLOW_TRUST_EXTERNAL_VERIFY` is set — including the Plan stage's own advance immediately after writing that declaration. This is very likely intentional (a human must approve any agent-declared shell command before it's ever trusted, per the security rationale in `17-CONTEXT.md`), but it's a meaningful behavior change from Phase 16 (Code-stage-only) and isn't covered by an end-to-end test that drives `advance()` through a real Plan-stage completion with a freshly-authored declaration — the existing tests all call `evaluate_agent_result_inner` directly with a manually-set `state.stage`. Worth confirming this blast radius (every stage gates on any phase-wide declaration) matches operator expectations.

**Fix:** No code change proposed; recommend an integration test that drives `advance()` end-to-end for a Plan-stage completion that authors a fresh `external_verify` declaration, to pin the "gates on itself" behavior as intentional rather than accidental.

### IN-03: `build_timestamp` is round-tripped as a JSON string, not a number

**File:** `crates/devflow-cli/src/main.rs:826-839` (`workflow_started_payload`)

**Issue:** `"build_timestamp": env!("DEVFLOW_BUILD_TIMESTAMP")` embeds the Unix-seconds value as a `&str` (via `env!`), so the `workflow_started` event's `build_timestamp` field is a JSON string (e.g. `"1771000000"`) rather than a JSON number, while `"dirty"` is similarly a string `"true"`/`"false"` rather than a JSON boolean. Any downstream consumer of `events.jsonl` doing numeric/boolean comparisons on these fields (e.g. Phase 18's reconciliation, per the `decided_by_layer` precedent set elsewhere in this same diff) will need to parse them first. Not a bug — `env!` can only produce `&'static str` — but worth flagging since the sibling field `decided_by_layer` in the same diff *is* emitted as a real JSON number (`Option<u8>`), so the payload is inconsistent in how it represents typed data.

**Fix:** Consider parsing `build_timestamp`/`dirty` into `serde_json::Value::Number`/`Value::Bool` before embedding in the payload, for consistency with `decided_by_layer` and to spare downstream consumers a string-to-number/bool parse.

---

_Reviewed: 2026-07-18T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_

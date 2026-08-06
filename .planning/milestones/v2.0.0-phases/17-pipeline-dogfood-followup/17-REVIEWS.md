---
phase: 17
reviewers: [codex, opencode, qwen, cursor, antigravity]
reviewed_at: 2026-07-18T18:51:03Z
plans_reviewed: [17-01-PLAN.md, 17-02-PLAN.md, 17-03-PLAN.md, 17-04-PLAN.md, 17-05-PLAN.md]
failed_reviewers:
  qwen: "no auth type configured — lane dropped in 4s, no review produced"
  antigravity: "pre-session stall (#2073 mode 3) — 600s wall-clock cap expired during app boot; no conversation, no transcript"
---

# Cross-AI Plan Review — Phase 17

> Claude lane skipped (this session runs inside Claude Code — self-review excluded for independence).
> Gemini, CodeRabbit, and local model servers not installed/running.

## Codex Review

**Summary**
Overall recommendation: **REQUEST CHANGES**. The plans are directionally strong and target the right defects, but execution as written has several blockers: a cross-crate compile break from `AgentResult.decided_by_layer`, an unsafe `devflow start` resume command for rate-limit recovery, under-specified preflight inputs, and build-script rerun paths that do not match the workspace layout. Independent lanes returned `code-reviewer: REQUEST CHANGES` and `architect: BLOCK`.

**17-01 Plan**
Summary: Solid foundation for typed outcomes and policy extraction, but incomplete migration scope.

Strengths:
- Correctly targets the existing status enum, which currently only has `Success`, `Failed`, `RateLimited`, `Unknown` in [agent_result.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:41).
- Correctly avoids the multi-word serde trap by requiring explicit `resource_killed` / `agent_unavailable` names.
- Pure `decide_action` in core matches the existing library/thin-CLI direction.

Concerns:
- **HIGH:** Adding `AgentResult.decided_by_layer` only updates literals in `agent_result.rs`, but the CLI also constructs `AgentResult` in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:1551). Plan 01’s file list excludes `crates/devflow-cli/src/main.rs` in [17-01-PLAN.md](/var/home/denniyahh/Github/devflow/.planning/phases/17-pipeline-dogfood-followup/17-01-PLAN.md:7), so the workspace can fail to compile after Wave 1.
- **MEDIUM:** Marker-derived Layer 1 results will default to `None` unless explicitly stamped; `parse_marker_lines()` returns the deserialized result directly in [agent_result.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:448).

Suggestions:
- Include `crates/devflow-cli/src/main.rs` in Plan 01 or make `decided_by_layer` non-exhaustive through a constructor/helper migration.
- Stamp `decided_by_layer = Some(1)` at the `evaluate_layer1()` boundary, not only in individual constructors.

Risk Assessment: **MEDIUM**. Conceptually right, but incomplete cross-crate migration creates a real compile risk.

**17-02 Plan**
Summary: Good no-dependency approach for build provenance, but the build-script change detection is wrong for a nested workspace crate.

Strengths:
- Fits the repo’s dependency posture: `crates/devflow-cli/Cargo.toml` has normal dependencies and no `[build-dependencies]` today in [Cargo.toml](/var/home/denniyahh/Github/devflow/crates/devflow-cli/Cargo.toml:12).
- Graceful no-git degradation is appropriate for crates.io installs.

Concerns:
- **HIGH:** `build.rs` lives under `crates/devflow-cli`, so `cargo:rerun-if-changed=.git/HEAD` in [17-02-PLAN.md](/var/home/denniyahh/Github/devflow/.planning/phases/17-pipeline-dogfood-followup/17-02-PLAN.md:92) refers to `crates/devflow-cli/.git/HEAD`, not repo `.git/HEAD`. The workspace root is one level above members in [Cargo.toml](/var/home/denniyahh/Github/devflow/Cargo.toml:3).
- **MEDIUM:** The plan does not mention `packed-refs`, so tag/branch movement can be missed in packed-ref repos.

Suggestions:
- Resolve the git common dir from `CARGO_MANIFEST_DIR` using `git rev-parse --git-common-dir`, then emit absolute `rerun-if-changed` paths for `HEAD`, refs, and `packed-refs`.
- Keep the no-git test tolerant, but add a normal-repo test or documented manual check for rerun behavior.

Risk Assessment: **MEDIUM**. Runtime provenance shape is fine; stale build detection can be unreliable if Cargo does not rerun the script.

**17-03 Plan**
Summary: The strongest plan in the set. It directly fixes the Layer 0/Layer 3 semantics without overreaching into Layer 2.

Strengths:
- Correctly identifies Layer 3’s current blanket `Unknown` behavior in [agent_result.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:610).
- Correctly preserves Layer 2’s documented non-Plan/Code zero-commit success behavior in [agent_result.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:507).
- Extending Layer 0 is necessary: it is currently Code-only in [agent_result.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:638), and successful probes currently only defer because the function returns `None` after no failing command in [agent_result.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:679).

Concerns:
- **LOW:** The plan says preserve mismatch branches “exactly”; adding `decided_by_layer` necessarily touches those literals. The intent is clear, but wording could confuse implementers.

Suggestions:
- Rephrase “byte-for-byte” preservation to “preserve behavior and command comparison semantics.”
- Add one full-cascade test proving Layer 0 Success outranks Layer 2 zero-commit failure for a Plan/Code-like commit-gated stage.

Risk Assessment: **LOW-MEDIUM**. Good scope control; main risk is test fixture complexity around approved external verification.

**17-04 Plan**
Summary: Correct target, but the rate-limit resume and infra routing details need repair before execution.

Strengths:
- Replaces the exact bug: `Unknown` currently falls through the success transition path in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:871).
- Correctly preserves Validate’s verdict gate, which currently only advances on explicit pass in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:882).
- Correctly identifies that existing cron support is sequentagent-only in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:1670).

Concerns:
- **HIGH:** `devflow start --phase N --agent X` is not a safe resume command. `start` requires `--mode` in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:75) and recreates branch/worktree before saving state in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:568). A cron resume should relaunch the saved state, not start a new workflow.
- **HIGH:** The plan says infra outcomes must not increment `consecutive_failures`, but also suggests Validate/Ship keep their handlers; `handle_validate_outcome(false)` increments `consecutive_failures` in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:897).
- **HIGH:** `advance_evaluated.status` must stop using Debug lowercasing; current code would produce `resourcekilled` in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:848).
- **MEDIUM:** Rate-limit ceiling behavior conflicts: no blocking gate in [17-04-PLAN.md](/var/home/denniyahh/Github/devflow/.planning/phases/17-pipeline-dogfood-followup/17-04-PLAN.md:15), but gate/ceiling behavior later in [17-04-PLAN.md](/var/home/denniyahh/Github/devflow/.planning/phases/17-pipeline-dogfood-followup/17-04-PLAN.md:126).

Suggestions:
- Add an internal `resume` command or cron command that loads existing `state-{phase}.json` and calls `launch_stage`, rather than `start`.
- Add a dedicated `handle_infra_outcome` path that never calls `handle_validate_outcome(false)`.
- Add `AgentStatus::as_str()` or serde-based status emission and use it everywhere events need status strings.

Risk Assessment: **HIGH**. This is the main phase risk because a wrong resume path can corrupt/restart live workflow state.

**17-05 Plan**
Summary: Goal is right, but preflight is under-specified and mixes gate semantics with launch-time side effects.

Strengths:
- Correct insertion target: `launch_stage()` currently reaches `monitor::spawn_monitor` at [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:739).
- Default trait-method extension matches `AgentAdapter::extra_env()` in [agents/mod.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agents/mod.rs:39).
- `workflow_started` is the right event to extend; current payload has only agent/mode/worktree in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:605).

Concerns:
- **HIGH:** Empty reviewer-set check has no data source. `State` has no reviewer/receiver fields in [state.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/state.rs:17), and config only has `review_angles` in [config.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/config.rs:55).
- **HIGH:** Existing Codex interactivity preflight hard-errors before state exists in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:530), bypassing the planned named gate + notify model.
- **MEDIUM:** If preflight is inserted only before `spawn_monitor`, capture archival still runs first in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:713), so a failed preflight is not side-effect-free.
- **MEDIUM:** `gh auth status` is not clearly tied to current terminal hooks. Ship hooks are local merge/tag/delete in [hooks.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/hooks.rs:87); `git push` exists for sequentagent integration in [git.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/git.rs:257), not the normal terminal batch.

Suggestions:
- Define reviewer receiver storage first, or remove that acceptance criterion from Phase 17.
- Move existing Codex interactivity checks into the new preflight path after state persistence, or explicitly keep them as hard pre-start checks outside AC-4.
- Run preflight before capture archival if “before agent time is consumed” also means “before launch side effects.”
- Prefer checking actual git remote/push prerequisites over unconditional GitHub CLI assumptions.

Risk Assessment: **HIGH**. The preflight API cannot currently implement all claimed checks, and the stale-binary portion can block valid runs if the git/provenance edge cases are not carefully scoped.
---

## OpenCode Review

Now let me check a few more critical spots — the hooks, the Phase 16 regression test for AC-1, and the `handle_ship_failure` function:
I now have comprehensive evidence from the actual source code. Here is my detailed review.

---

# Phase 17 Plan Review

## Plan 17-01: Outcome Taxonomy + Policy Table

### Summary
This plan correctly identifies all four AgentStatus variants at `agent_result.rs:38-50`, adds the `decided_by_layer` field (absent at `agent_result.rs:16-36`), and creates `outcome_policy.rs` (not in `lib.rs:54-76`). The `#[serde(rename = "...")]` approach for multi-word variants is correct — verified that `#[serde(rename_all = "lowercase")]` at `agent_result.rs:40` would collapse `ResourceKilled` to `"resourcekilled"` (Pitfall 1 confirmed). Layer 2 exit-code classification targets the right code path (`agent_result.rs:559-564`), and the `decided_by_layer` layer constants (0=Layer0, 1=Layer1, etc.) are well-coordinated with Plans 03 and 04.

### Strengths
- **Serde rename annotation explicitly called out.** Plan mandates `#[serde(rename = "resource_killed")]` per variant, not trusting the enum-level `rename_all = "lowercase"` — verified at `agent_result.rs:40` that `rename_all` indeed does not insert underscores. This is the right fix per RESEARCH.md Pitfall 1.
- **Exhaustive match on `decide_action`.** The plan requires no wildcard arm (`_`), which is the entire point of D-11: adding a new `AgentStatus` variant becomes a compile error until its policy is declared. The `stage` parameter is retained in the signature even though the mapping is currently stage-independent, correctly anticipating Plan 04's dispatch.
- **Separate infra counter with serde default.** `State.infra_failures` uses `#[serde(default)]` (`state.rs:32-33` pattern for `consecutive_failures`), so old state.json files without the field deserialize cleanly.
- **All `AgentResult` struct literals updated.** Plan explicitly lists which layer gets which `decided_by_layer` value and instructs updating every literal in the file, preventing a compile error from the new field.

### Concerns

| Severity | Finding | Evidence |
|----------|---------|----------|
| **MEDIUM** | `decide_action` maps `Failed` and `Unknown` identically to `GateReview`. This is correct for the current truth but loses information — a "regular" failure and "process vanished unobserved" are different signals. Plan 04's `handle_stage_failure` won't distinguish them. The plan works (both are non-advance), but Phase 18's reconciliation (18d) might want to differentiate. | Plan 17-01 Task 2 truth: `Failed\|Unknown -> GateReview` |
| **LOW** | `MAX_INFRA_FAILURES = 5` is Claude's Discretion with documented rationale ("more lenient than 3-fault gate ceiling"). The value is reasonable but has no empirical basis — 5 is chosen because it's "more than 3." A stuck OOM loop hitting this ceiling 5 cycles later still burns 5x agent time unobserved. | `mode.rs:18` — `MAX_CONSECUTIVE_FAILURES = 3`; Plan 01 Task 2 proposes `MAX_INFRA_FAILURES = 5` |
| **LOW** | The `decided_by_layer` field type is `Option<u8>`. A Magic number layer value like `Some(1)` is fine for now, but `u8` implies 256 layers — a future Phase that adds a "Layer 4" might accidentally collide if the numbering scheme isn't documented. Mitigated by the fact that layer 0-3 are well-known and no layer 4 is on any roadmap. | Plan 01 Task 1 action |

### Suggestions
- Document the `decided_by_layer` scheme (0=external-probe, 1=marker, 2=exit-code+commits, 3=process-gone) as a single-source comment on the field, so Phase 18's reconciliation code doesn't need to reverse-engineer it.
- Consider an `Action::GateReview` variant that carries the underlying `AgentStatus` so `handle_stage_failure` can tailor context text (e.g., "unknown" vs "failed" might want different gate wording). Can be deferred to Plan 04 if the planner chooses.

---

## Plan 17-02: Build Provenance (build.rs)

### Summary
A correct, minimal build-script plan. The workspace has no `build.rs` today (confirmed: `devflow-cli/Cargo.toml` lines 1-21, no `[build-dependencies]`). The `rerun-if-changed=.git/HEAD` + `rerun-if-changed=.git/refs` pattern avoids spurious rebuilds. The graceful-degradation requirement (no-git → empty string sentinel) satisfies D-20.

### Strengths
- **Zero new dependencies.** Explicit rejection of `vergen`, `chrono`, `git2` — all substantiated by D-20 and the codebase's existing git-shelling precedent (`agent_result.rs:536-549`).
- **Graceful degradation is tested.** The integration test at `build_provenance.rs` permits empty commit to keep crates.io/no-git builds passing.
- **`cargo:rerun-if-changed` pinning.** Only `.git/HEAD` and `.git/refs` trigger rebuilds — no rebuild on every source change.

### Concerns

| Severity | Finding | Evidence |
|----------|---------|----------|
| **LOW** | `DEVFLOW_BUILD_TIMESTAMP` is a Unix-seconds integer from `SystemTime::now()`. On a deterministic/reproducible build, this embeds the build machine's wall-clock, not the commit timestamp. For the stated purpose (comparing against file mtimes for staleness detection), this is fine. For "forensic provenance" (18d reconciliation), the commit timestamp (`git log -1 --format=%ct`) would be more reproducible. Not a blocker — just a note that the type (build-clock vs commit-time) has subtly different properties. | Plan 02 Task 1 action: `SystemTime::now().duration_since(UNIX_EPOCH)` |
| **LOW** | `cargo:rerun-if-changed=.git/HEAD` catches branch switches but NOT `git fetch` pulling new remote refs (which updates `.git/FETCH_HEAD` and packed-refs, not `.git/HEAD` or `.git/refs/heads/`). A build that's re-run after `git fetch` won't regenerate. Mitigated: `rerun-if-changed=.git/refs` catches ref updates on the same branch; new commits that change HEAD would hit `.git/HEAD`. | `build.rs` rerun-if-changed lines |

### Suggestions
- Consider `cargo:rerun-if-changed=.git/packed-refs` to catch fetch-incremental scenarios (minor).
- Document in a code comment that `DEVFLOW_BUILD_TIMESTAMP` is the *build machine's* clock, not the commit timestamp.

---

## Plan 17-03: Layer 0/3 Cascade Rework

### Summary
The highest-stakes plan. It touches the four-layer cascade at `agent_result.rs:633-690` (Layer 0) and `agent_result.rs:588-625` (Layer 3). Both targeted changes are traced correctly to the code: Layer 0's `state.stage != Stage::Code` gate at line 638, Layer 3's `Unknown` blanket at line 611. The TRUST_EXTERNAL_VERIFY_ENV security property is explicitly preserved. This plan is well-designed.

### Strengths
- **Layer 3 split is surgically precise.** Changes ONLY the `commits == 0` branch from `Unknown` to `Failed`, leaving `commits > 0` as `Unknown`. Verified against `agent_result.rs:610-624` — the code currently returns `Unknown` unconditionally. The D-03 three-way distinction (zero-commit-probe-passed → advance, zero-commit-probe-failed → fail, zero-commit-no-declaration → fail) is properly realized as Layer 0 (handles probe cases) + Layer 3 fallthrough (handles no-declaration case).
- **Layer 0 stage-scope lift is minimal.** Removing `state.stage != Stage::Code` from the guard at line 638 is a one-clause change. The approval-mismatch branches (lines 644-678) are left byte-for-byte identical.
- **Pitfall 2 (Don't touch Layer 2) is explicitly prohibited.** Plan correctly notes that `agent_result.rs:507-513`'s "Define/Validate/Ship zero-commit → Success" is normal operation and must not change.
- **Layer 0 affirmative Success is correctly placed.** At lines 679-689, the `.find()` iterator returns `None` when all probes pass — currently that returns `None` (defer to cascade). The plan adds a `Some(AgentResult { status: Success, ... })` for this case, which is the correct insertion point.

### Concerns

| Severity | Finding | Evidence |
|----------|---------|----------|
| **MEDIUM** | `evaluate_layer0` uses `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)` for discovering PLAN files (line 643). In a worktree phase, the worktree checkout may not have `.planning/phases/` — it only carries the working tree contents. `external_verify_commands` reads from `execution_root/.planning/` (`verify.rs:30`), which would return empty commands. With approval set, this hits the "PLAN removed" mismatch branch (line 644-654). This means external probe verification may silently fail for worktree-based phases even when everything is properly configured. **This is a pre-existing Phase 16 issue** (noted at 16-01-SUMMARY.md:132), but lifting the stage restriction in Phase 17 amplifies its blast radius across all stages. Recommended: use `project_root` (not `execution_root`) for PLAN discovery, and keep `execution_root` only for running probes. | `agent_result.rs:643` + `verify.rs:30` |
| **LOW** | The prohibition "MUST NOT add a new AgentStatus variant for the Layer 3 split" means the zero-commit case reuses `Failed`. But the full four-layer cascade at `evaluate_agent_result_inner:702-725` returns at Layer 0 if it fires. Since Layer 3's zero-commit Failed is the "process gone, nothing accounted for" case, and Layer 0 only fires with actual external probes, the Layer 3 Failed won't be confused with Layer 2's executable-says-failed. However, any code that pattern-matches on `AgentStatus::Failed` will see both. This is safe as long as no code takes "Failed" to mean "the agent clearly reported failure" when it could mean "the agent vanished." The `decided_by_layer` field mitigates this — code that cares can check. | `agent_result.rs:559-564` (Layer 2 Failed) vs Plan 03 Task 1 (Layer 3 Failed) |

### Suggestions
- For the worktree PLAN-discovery issue: change `evaluate_layer0` to pass `project_root` (not `execution_root`) to `external_verify_commands`, while keeping `execution_root` for `run_external_verification`. This is a two-argument split, not a design change.
- Consider adding a reason string on `AgentResult` that distinguishes Layer-2-Failed ("agent exited with code 1") from Layer-3-Failed ("process gone, no work accounted for") — the plan already does this for Layer 3. Good.

---

## Plan 17-04: advance() Dispatch + Rate-Limit Auto-Resume + Evidence

### Summary
The convergence point for 17a and 17b. Replaces the `matches!(Failed|RateLimited)` boolean at `main.rs:854-857` with an exhaustive `decide_action` match. Correctly identifies Pitfall 3: the rate-limit auto-resume machinery (`write_rate_limit_cron` at `main.rs:1735-1762`) is ONLY wired into `sequentagent()` (called at line 1672), never the primary `advance()` path. The `build_single_agent_cron_instructions` addition in `ship.rs` is the right shape.

### Strengths
- **Exact defect identification.** Line numbers are correct: `main.rs:854` (`matches!(Failed|RateLimited)`), `main.rs:871` (comment "Success (or Unknown — advance..."), `main.rs:1670-1685` (sequentagent-only cron wiring), `main.rs:842-852` (`advance_evaluated` emit).
- **In-place change, no extraction.** The prohibition against extracting `advance()/transition/handle_*_outcome` into a module follows the Deferred rationale — minimizes diff surface on fail-closed paths. Correct.
- **Single-agent cron builder.** The plan creates a separate `build_single_agent_cron_instructions` rather than twisting `build_cron_instructions` (which hardcodes `sequentagent` at `ship.rs:162-167`). This is the clean approach — `build_cron_instructions` keeps its existing contract.
- **`truncate_reason` discipline on evidence.** The plan explicitly routes agent-derived text through `truncate_reason` (sanitize + cap at 300 chars, `main.rs:953-955`) before hitting `events.jsonl` or a gate context.

### Concerns

| Severity | Finding | Evidence |
|----------|---------|----------|
| **HIGH** | `advance_evaluated` emit at `main.rs:842-852` uses `format!("{:?}", result.status).to_ascii_lowercase()` — Debug formatting, NOT serde serialization. Plan 01 adds `#[serde(rename = "resource_killed")]` on the enum, but the `advance_evaluated` emit doesn't use serde — it uses the Debug derive. `AgentStatus::ResourceKilled` will emit as `"resourcekilled"` (Debug output, lowercased), NOT `"resource_killed"` (serde output). This means Phase 18's `events.jsonl` reconciliation will see `"resourcekilled"` while serde-based readers will see `"resource_killed"`. **The prohibition in Plan 01 says "MUST NOT reuse Debug-derived lowercasing"** but Plan 04's action says "keep the existing status." These conflict. The plan MUST either (a) change `main.rs:848` to use `serde_json::to_string(&result.status)?` or a dedicated Display impl, or (b) explicitly document that the `advance_evaluated` `status` field's wire format is considered frozen. | `main.rs:848`: `format!("{:?}", result.status).to_ascii_lowercase()` |
| **MEDIUM** | `build_single_agent_cron_instructions` takes `agent` (presumably `AgentKind` or `&str`), but the `hermes_cron.command` field in `build_cron_instructions` (`ship.rs:181-184`) embeds a `cd {} && devflow sequentagent...` command string. The new single-agent builder must generate `cd {} && devflow start --phase {phase} --agent {agent} --mode {mode}`. The `mode` is NOT passed — the cron instructions would start in whatever mode the cron environment provides. Auto vs supervise matters: if the original run was in "supervise" mode, the resume should preserve that. Plan explicitly says the resume command is `devflow start --phase N --agent X` without `--mode`. The mode will default to... what? The state file records `mode`, but the cron-instructions record doesn't. The sequentagent path doesn't need mode (it's a handoff), but the primary start path does. | `ship.rs:181-184` vs Plan 04 Task 2 |
| **LOW** | The plan says RateLimited increments `infra_failures` (saturating): "increment infra_failures (saturating, per D-08 to bound infinite rate-limit loops)." This means rate limits share the infra counter with OOM kills. If the project is rate-limited 4 times, then hits an OOM kill, the OOM kill triggers abort (reached 5). Is that desired? D-08 says "infrastructure outcomes... do NOT increment consecutive_failures. They get their own counter with its own ceiling." The plan follows D-08 correctly — the counter bounds all infra outages. One counter for all infra failures is the design. Not a bug, just worth noting. | Plan 04 Task 2 action |

### Suggestions
- **Fix the `advance_evaluated` status field** to use serde serialization (e.g., `serde_json::to_string(&result.status)?.trim_matches('"')`) instead of Debug formatting, so the word-boundary-correct names reach `events.jsonl`. This is the most actionable gap in the entire phase.
- Add `--mode` to `build_single_agent_cron_instructions` — read `state.mode` from the current state and pass it as an additional parameter to the builder, embedding it in the resume command.
- The `truncate_reason` gate in Plan 04's evidence emission should be applied to the `reason` field specifically, preserving the `decided_by_layer` as a plain integer, not truncating it.

---

## Plan 17-05: Preflight + Build Provenance (Runtime)

### Summary
Adds the two pre-launch guards: a scoped preflight gate (generic + adapter hook) and a self-dogfood staleness check. Both are correctly placed in `launch_stage` before `monitor::spawn_monitor` (`main.rs:739`). The trait extension follows the `extra_env` default-impl precedent at `agents/mod.rs:39-41`. The D-17 workspace identity check is reasonable.

### Strengths
- **Trait design mirrors existing precedent.** `AgentAdapter::preflight() -> Result<(), String>` with empty default body exactly follows `extra_env() -> Vec<(String, String)> { Vec::new() }` at `agents/mod.rs:39-41`. Phase 18's Hermes adapter overrides it; other adapters keep the no-op.
- **Staleness composite correctly implemented.** The three-way `Staleness` enum (Fresh/Stale/Indeterminate) maps `git merge-base --is-ancestor` exit codes correctly per Pitfall 4: exit 0→Fresh, exit 1→Stale, other→Indeterminate. Empty provenance → Indeterminate (D-20).
- **Per-stage scoped preflight.** The plan explicitly gates `gh auth status` to remote-pushing stages only (D-16), not unconditionally. Ship's hooks (`hooks_after_ship()` at `hooks.rs:87-89` = Merge, VersionBump, BranchCleanup) are the ones that push to remote.
- **Fail-soft on `gh` absent.** The preflight check design handles `gh` binary missing with a WARN, not a preflight failure — matching the project's fail-soft idiom.

### Concerns

| Severity | Finding | Evidence |
|----------|---------|----------|
| **MEDIUM** | `is_self_dogfood_workspace` detection reads `project_root/Cargo.toml` and checks for `devflow-cli`/`devflow-core` as workspace members. The workspace has no `toml` crate dependency. A reliable detection needs TOML parsing. String-scanning for `"devflow-cli"` and `"devflow-core"` in the raw file could false-positive on: a non-DevFlow project that has both as dependencies, or a fork that renamed the package but kept the old member paths. The plan says "deterministic, offline, no config, no false positives on unrelated Rust projects" — string matching can't guarantee this without TOML-aware parsing. | `Cargo.toml` workspace: `members = ["crates/devflow-core", "crates/devflow-cli"]` |
| **MEDIUM** | D-14 says `gh auth status` is a UNIVERSAL check but D-16 says checks are "scoped to that stage's requirements." The plan resolves this by gating it to stages with remote-pushing hooks. This is the right compromise per Open Question 1 in RESEARCH.md, but the plan needs to be explicit about WHICH stages trigger it. Currently only Ship's `hooks_after_ship()` pushes to remote (Merge hook at `hooks.rs:120-149`). The plan should either hardcode Ship or query `hooks::hooks_after_ship()` to determine if any hook in the stage's batch is a remote-pushing one. | `hooks.rs:87-89`: only Ship has hooks that push |
| **LOW** | `DEVFLOW_BUILD_TIMESTAMP` from `build.rs` is a plain integer, but in `launch_stage`, the mtime comparison needs to compare it against file metadata. `std::fs::metadata(path)?.modified()?` returns `SystemTime`. The plan says to compare source mtime > build timestamp. Unix-seconds → SystemTime comparison requires `UNIX_EPOCH + Duration::from_secs(ts)`. This is straightforward but should be explicit in the implementation. | Plan 05 Task 2, no explicit conversion mentioned |
| **LOW** | `run_preflight` returns `Result<(), CliError>`, but the gate flow calls `run_gate` which returns `Result<GateAction, CliError>`. The preflight failure path needs to match `GateAction::Advance` → retry `launch_stage`, `GateAction::LoopBack` → retry, `GateAction::Abort` → abort. The Code Example in RESEARCH.md shows this correctly, but the plan's action paragraph simplifies it. Not a gap — just make sure the implementation matches. | Plan 05 Task 1 action |

### Suggestions
- For `is_self_dogfood_workspace`: add a lightweight TOML parse. Since `serde` is available, you can do `serde_json::from_str` on a JSON-converted version (not applicable), or manually scan for `members` array lines. A practical middle-ground: scan for the EXACT string `"crates/devflow-core"` AND `"crates/devflow-cli"` within the `[[workspace.members]]` or `members = [...]` section. This is fragile but good enough for the purpose since no non-DevFlow project is likely to have both those EXACT member paths. Add a comment documenting the assumption.
- Scope the `gh auth status` check explicitly to `state.stage == Stage::Ship` rather than probing hooks dynamically at preflight time. Simpler, deterministic, and matches what actually happened (Ship's merge is the only remote-pushing hook).

---

## Overall Phase Assessment

### Risk Assessment: **MEDIUM**

**Justification:** The plans correctly trace all source code locations and decisions to the actual codebase. The dependency ordering (01→03→04, 02→05) is sound. All four scope units (17a–17d) are covered. However, one HIGH-severity gap exists (the `advance_evaluated` Debug-vs-serde formatting conflict) and two MEDIUM-severity gaps (worktree PLAN discovery, workspace identity detection without TOML parser). None of these block implementation, but Plan 04's `advance_evaluated` status field needs an explicit fix before the phase can claim D-10 compliance for event reconciliation.

### Cross-Plan Dependency Check

| Dependency | Declared | Verified | Status |
|-----------|----------|----------|---------|
| 17-03 → 17-01 | `AgentStatus` new variants, `decided_by_layer` field | Plan 01 adds both; Plan 03 sets Layer 0/3 values | OK |
| 17-04 → 17-01 | `decide_action`, `Action` enum, `State.infra_failures` | Plan 01 creates all three | OK |
| 17-04 → 17-03 | Layer 3 zero-commit → Failed, Layer 0 affirmative Success | Plan 03 creates both outcomes | OK |
| 17-05 → 17-02 | `DEVFLOW_BUILD_COMMIT/DIRTY/TIMESTAMP` env vars | Plan 02 emits them | OK |
| 17-05 → 17-04 | `advance()` dispatch (shared `main.rs`) | Wave 4 after Wave 3; same file but different sections | OK — Plan 04 edits `advance()` (line ~788-887); Plan 05 edits `launch_stage` (line ~692-757) and `start()` (line ~605-614). Non-overlapping. |

### Scope Creep Check
All plans stay within the four declared scope units (17a–17d). The Deferred items (full `main.rs` extraction, `CONCERNS.md` line count fix, 18d/18e) are explicitly prohibited. Plan 04's prohibition correctly rejects the extraction.

### Verification Surface
The Wave 0 test gaps listed in RESEARCH.md are correctly distributed across plans. Each plan specifies `cargo test` commands scoped to the modified modules. The AC-1 Phase 16 regression test exists at `main.rs:3141` (`terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`) — verified against source. The plan correctly notes this as "verify, don't re-plan."

---

## Qwen Review

Qwen review failed or returned empty output. stderr:
No auth type is selected. Please configure an auth type (e.g. via settings or `--auth-type`) before running in non-interactive mode.

---

## Cursor Review

# Phase 17 Cross-AI Plan Review

## Overall Summary

The five plans correctly target the live defects: `Unknown` advances via the success arm at `crates/devflow-cli/src/main.rs:854-875`, Layer 3 always emits `Unknown` at `crates/devflow-core/src/agent_result.rs:610-620`, Layer 0 is Code-only and never returns affirmative Success at `agent_result.rs:638-689`, rate-limit cron is sequentagent-only at `main.rs:1670-1672` / `ship.rs:154-187`, and `workflow_started` lacks provenance at `main.rs:605-614`. Wave ordering and the pure `decide_action` extraction are sound. The plans do not yet close several execution traps already known in this repo (CLI package name `devflow` vs `devflow-cli`, Debug-based event status strings, underspecified preflight artifacts, and GateInfra vs Validate counter routing).

---

## Plan 17-01 — Typed outcomes + policy module

### Summary
Solid foundation: serde word-boundary renames, Layer 2 exit-code branches before the `!= 0` catch-all, pure exhaustive `decide_action`, and `State.infra_failures` match D-07/D-08/D-11/D-12 and the current cascade. One compile-break gap and one event-wire gap remain.

### Strengths
- Correctly identifies that `#[serde(rename_all = "lowercase")]` on `AgentStatus` (`agent_result.rs:39-50`) will not produce `resource_killed` / `agent_unavailable`.
- Layer 2 change site is right: plain `i32` from the exit file at `agent_result.rs:528-529`, matching monitor shell `$?` semantics (`monitor.rs:148-154`) — no `ExitStatusExt`.
- Explicitly preserves Layer 2’s Define/Validate/Ship zero-commit Success path (`agent_result.rs:556-564`), consistent with Pitfall 2.
- Pure `outcome_policy` module + exhaustive match is the right compiler-enforced fix for the D-01 regression class.

### Concerns
- **HIGH — Incomplete `AgentResult` literal update:** Plan 01 only updates literals in `agent_result.rs`, but `main.rs:1551-1560` also constructs `AgentResult` without `decided_by_layer`. Adding the field will fail compile until that site (and any others) are updated.
- **MEDIUM — Event emission left on Debug lowercasing:** Research correctly flags `main.rs:848` (`format!("{:?}", result.status).to_ascii_lowercase()`), which already collapses `RateLimited` → `ratelimited`. Plan 01 fixes serde but does not require a Display/serde helper for events; Plan 04 “keeps existing status,” so wire evidence stays wrong for multi-word variants.
- **LOW — `stage` unused in `decide_action`:** Documented as intentional for Plan 04; fine if clippy `unused` is addressed (`allow` or `_stage`).

### Suggestions
- Expand Task 1 acceptance to: every `AgentResult { ... }` in the workspace (at least `rg 'AgentResult \{'`).
- Add `AgentStatus::as_wire_str()` (or serde_json serialize) in Plan 01 and require Plan 04 to use it for `advance_evaluated.status`.

### Risk Assessment
**MEDIUM** — Core design is right; the missed `main.rs` literal is a predictable Wave-1 compile failure.

---

## Plan 17-02 — `build.rs` provenance

### Summary
Narrow, well-scoped compile-time half of 17d. Package name (`-p devflow`), no `[build-dependencies]`, argv git, and graceful empty commit match D-20 and prior Phase 15 lessons.

### Strengths
- Uses correct Cargo package name `devflow` (`crates/devflow-cli/Cargo.toml:2`), unlike Plans 04/05.
- `Command::new("git").args([...])` + `None` on failure matches codebase idiom (`agent_result.rs:536-550`).
- Clear split: emit env vars here; consume in Plan 05.

### Concerns
- **MEDIUM — `cargo:rerun-if-changed=.git/refs`:** Fragile for worktrees / unusual git layouts; dirty-tree-only edits may not rebuild provenance until HEAD moves. Acceptable if documented; mtime staleness in Plan 05 partially compensates.
- **LOW — Timestamp `> 0` vs clock-error `0`:** Task behavior wants `> 0` in a normal build; acceptance only requires parseable `u64`. Align tests with D-20 degraded `0`.

### Suggestions
- Prefer `cargo:rerun-if-changed=.git/HEAD` plus `cargo:rerun-if-env-changed=` only if needed; document worktree limitation.
- Assert commit is either empty or 40-hex when git is present (still allow empty for crates.io).

### Risk Assessment
**LOW** — Mechanical and well-constrained.

---

## Plan 17-03 — Layer 0 / Layer 3 cascade

### Summary
Load-bearing half of 17a. Splitting Layer 3 zero-commit → `Failed` and lifting Layer 0 to all stages with affirmative Success correctly implements D-02/D-03/D-05 against current code.

### Strengths
- Layer 3 defect is real: both branches set `AgentStatus::Unknown` today (`agent_result.rs:610-620`), so zero-commit vanished agents advance via `main.rs:871-875`.
- Layer 0 Code gate at `agent_result.rs:638` and “success defers to Layer 1/2/3” docstring (`627-631`) match D-05 gaps.
- Keeps TRUST_EXTERNAL_VERIFY_ENV mismatch branches (`645-677`) unchanged — correct security posture.
- Correctly does not invent a new status variant (reuse `Failed` / `Unknown`).

### Concerns
- **MEDIUM — Affirmative Success outranks agent Failed:** After Plan 03, all-pass Layer 0 returns early from `evaluate_agent_result_inner` (`702-710`) before Layer 1. An agent `DEVFLOW_RESULT` failure plus a passing external probe becomes Success. Intentional per 16a “external outranks,” but worth an explicit test so it is not treated as accidental.
- **MEDIUM — “no declaration” is cascade-level, not Layer 3-local:** `evaluate_layer3` never sees declarations; fail-closed depends on Layer 0 returning `None`. Tests should drive the full cascade (`evaluate_agent_result_inner`), not only `evaluate_layer3` in isolation.
- **LOW — Validate stage + Layer 0 Success:** Affirmative Success with `verdict: None` will hit Plan 04’s Advance → Validate verdict gate and fail-safe. Confirm that is desired for external-only Validate.

### Suggestions
- Add cascade tests: approved all-pass probe on Define with zero commits → Success; no declaration + process gone + zero commits → Failed; commits present + process gone → Unknown.
- Document that Layer 0 Success intentionally short-circuits Layer 1 failure markers.

### Risk Assessment
**MEDIUM** — Right changes; cascade composition needs stronger tests than the plan’s layer-isolated list.

---

## Plan 17-04 — `advance()` dispatch + rate-limit resume

### Summary
Correct convergence point for 17a/17b: replace `matches!(Failed | RateLimited)` (`main.rs:854-857`) and the explicit “Unknown — advance” comment (`871`). Primary-loop cron via a new `build_single_agent_cron_instructions` is required (Pitfall 3 verified). Several dispatch and packaging issues are HIGH.

### Strengths
- Defect locus and success-arm fallthrough are accurately cited.
- Separating single-agent resume (`devflow start --phase N`) from sequentagent (`ship.rs:162-168`) is necessary; reusing sequentagent builder would be wrong.
- Preserving Validate `Verdict::Pass` gating (`main.rs:876-883`) and Ship handlers is essential.
- AC-1 test names exist as unit tests (`main.rs:2888`, `main.rs:3141`).

### Concerns
- **HIGH — Wrong Cargo package in verify commands:** Multiple steps use `cargo test -p devflow-cli` / `cargo clippy -p devflow-cli`. Package name is `devflow` (`crates/devflow-cli/Cargo.toml:2`); Phase 15 already burned on this. Commands will fail before any logic is tested.
- **HIGH — GateInfra vs Validate/Ship handlers:** Task 1 says GateInfra uses `handle_stage_failure` but also “Validate/Ship keep their own handlers.” `handle_validate_outcome(false)` increments `consecutive_failures` (`main.rs:896-898`). Routing `ResourceKilled`/`AgentUnavailable` through it violates D-08. GateInfra must never call `handle_validate_outcome`.
- **HIGH — `advance_evaluated.status` still Debug-lowercased:** Plan keeps `format!("{:?}", ...).to_ascii_lowercase()` (`main.rs:848`) while adding `decided_by_layer`. New variants become `resourcekilled` / `agentunavailable`, contradicting Plan 01 prohibitions and D-10 / future 18d reconciliation.
- **MEDIUM — AutoResume + `infra_failures` ceiling:** Incrementing infra on RateLimited then falling through to a gate at `MAX_INFRA_FAILURES` is reasonable, but unspecified whether the gate is `handle_stage_failure` vs abort; clarify so unbounded resume cannot soft-loop via human Advance retry.
- **MEDIUM — `devflow start` resume may re-enter mid-stage incorrectly:** Resume command shape must preserve mode/agent/worktree; plan only sketches `--phase` / `--agent`. Check `start` CLI flags so cron resume does not reset stage.

### Suggestions
- Normalize all verify commands to `-p devflow`.
- Spell GateInfra as: saturating infra bump → persist → abort at ceiling → else `handle_stage_failure` for **all** stages (including Validate/Ship); never `handle_validate_outcome`.
- Replace status/verdict event fields with serde wire names (or shared helper from Plan 01).
- Specify exact `devflow start ...` argv for cron (mode, worktree, agent).

### Risk Assessment
**HIGH** — Highest-impact plan; package-name and GateInfra/counter bugs can ship a false “fixed” advance path.

---

## Plan 17-05 — Preflight + provenance consumption

### Summary
Insertion point is correct (`launch_stage` before `spawn_monitor` at `main.rs:739`; trait default mirroring `extra_env` at `agents/mod.rs:39-41`). 17d consumption of Plan 02 env vars and Pitfall 4 merge-base exit handling are well specified. 17c universal checks and D-17 identity matching are under-specified relative to AC-4 and known package naming.

### Strengths
- Preflight-as-gate (not hard exit) matches WR-11 / `handle_stage_failure` (`main.rs:981-1008`) and D-15.
- `gh auth` scoped to remote-pushing stages (`hooks_after_ship` = Merge/VersionBump/BranchCleanup at `hooks.rs:87-89`) matches D-16 and Research Open Question 1.
- Staleness: exit 0/1/other + empty commit → Indeterminate matches git’s merge-base contract and D-20.
- `current_exe()` precedent at `monitor.rs:79` is real.

### Concerns
- **HIGH — D-17 identity string `devflow-cli` is ambiguous/wrong as package name:** CLI `[package] name` is `devflow`, not `devflow-cli`. Looking for `name = "devflow-cli"` never matches this repo (Phase 15 lesson). Root workspace members *paths* do include `crates/devflow-cli` and `crates/devflow-core`. Plan must specify detection via workspace `members` paths and/or packages `devflow` + `devflow-core`, or self-dogfood blocking never arms on the incident workspace.
- **HIGH — Universal preflight checks lack concrete predicates:** “Plan interactivity,” “required security artifact,” and “reviewer receiver set” appear in the retrospective (`17-DOGFOOD-RETROSPECTIVE.md:65-69`) but have **no symbols/paths in the codebase**. Plan 05 leaves implementers to invent them; AC-4 cannot be verified as written.
- **MEDIUM — Reviewer check default is empty for all built-in adapters:** Task leaves `preflight` default `Ok(())` and only tests an override. Empty reviewer will not fail for Claude/Codex/OpenCode in Phase 17; real coverage waits for Hermes. AC-4 “unavailable reviewer” is then only a trait/test stub, not operator-visible behavior.
- **MEDIUM — Wrong `-p devflow-cli` in verify/acceptance** (same as Plan 04).
- **MEDIUM — Stale block returns `CliError` vs named gate:** Task 2 blocks self-dogfood with `CliError`, while preflight failures use `run_gate`. Unattended cron may miss a hard error the same way D-15 rejects hard exits for preflight. Align staleness block with gate+notify or justify the asymmetry.
- **LOW — Mtime arm on every `launch_stage`:** Scoped `git ls-files -m` is fine; still add a cheap dirty short-circuit as Research recommends.

### Suggestions
- Rewrite D-17 detection acceptance: e.g. workspace `Cargo.toml` members contain both `crates/devflow-core` and `crates/devflow-cli`, or packages `devflow-core` + `devflow` are present.
- Name concrete preflight artifacts (files/config keys) and stage applicability before execute; if unknown, spike or shrink AC-4 to `gh auth` + trait hook only.
- Fix cargo package filters to `-p devflow`.
- Consider gate+notify for self-dogfood stale block for parity with D-15.

### Risk Assessment
**HIGH** — Provenance path is good; preflight/AC-4 and self-dogfood identity are the most likely silent misses.

---

## Cross-Plan Assessment

| Focus | Verdict |
|--------|---------|
| Phase goals 17a–17d | Achievable if HIGH items above are fixed before execute |
| Dependency waves | Sound: 01∥02 → 03 → 04 → 05; shared-file conflicts respected |
| Scope creep | Mostly avoided; Plan 05 inventing undefined artifact/reviewer checks is the creep risk |
| Security | Layer 0 approval preserved; gh stdout redaction called out; build.rs argv OK |
| Performance | Negligible; preflight/staleness git probes per launch are acceptable if scoped |
| AC-1 | Correctly verify-only; tests live at `main.rs:2888` and `main.rs:3141` |
| Deferred 18d/18e | Correctly excluded |

### Top fixes before execution
1. Replace every `-p devflow-cli` with `-p devflow`; define D-17 against real package/member names (`devflow` / `devflow-core` / member paths).
2. GateInfra must not touch `consecutive_failures` / `handle_validate_outcome`.
3. Stop using Debug-lowercasing for `advance_evaluated.status` (and ideally `verdict`).
4. Update all `AgentResult` literals workspace-wide (including `main.rs:1551`).
5. Specify concrete preflight checks for interactivity / security artifact / reviewer, or narrow AC-4 to what Phase 17 can actually implement.

### Overall phase risk
**HIGH** — Architecture and defect targeting are strong; packaging identity mistakes and underspecified preflight predicates can leave the dogfood holes only partially closed while tests appear green.

---

## Antigravity Review

Antigravity review failed: pre-session stall (#2073 mode 3). agy spent the full 600s external
wall-clock cap booting its app stack (Electron + language server) and never created a
conversation — no conv-id in ~/.gemini/antigravity-cli/cache/last_conversations.json for this
workspace and no transcript was written, so the transcript fallback had nothing to recover.
Lane dropped; no review produced.

---

## Consensus Summary

Three grounded reviews completed (Codex, OpenCode, Cursor — all opened repo files and cited `file:line` evidence). Qwen and Antigravity lanes failed (auth / pre-session stall) and carry no verdict weight. Verdicts: Codex REQUEST CHANGES (overall HIGH), Cursor HIGH, OpenCode MEDIUM. The architecture, defect loci, and wave ordering are unanimously endorsed; the shared concerns are execution-level traps that should be fixed via `/gsd-plan-phase 17 --reviews` before execute.

### Agreed Strengths

- **Defect loci verified accurate by all three reviewers**: `main.rs:854/871` (`Unknown` success-arm fallthrough), `agent_result.rs:610-620` (Layer 3 blanket `Unknown`), `agent_result.rs:638` (Layer 0 Code-only gate), `main.rs:1670` (sequentagent-only cron wiring), `main.rs:605-614` (provenance-less `workflow_started`).
- **Serde word-boundary fix is correct** (explicit `#[serde(rename)]` per multi-word variant) — all three confirmed `rename_all = "lowercase"` would collapse `ResourceKilled` → `resourcekilled`.
- **Pure exhaustive `decide_action` in devflow-core** endorsed as the right compiler-enforced fix for the D-01 regression class (all three).
- **Layer 2's Define/Validate/Ship zero-commit Success branch correctly fenced off** (Pitfall 2) and **`TRUST_EXTERNAL_VERIFY_ENV` approval mechanism preserved** (all three).
- **Wave ordering sound** (01∥02 → 03 → 04 → 05); 17-05/17-04 same-file sections verified non-overlapping (OpenCode traced line ranges).
- **Separate `build_single_agent_cron_instructions`** rather than twisting the sequentagent builder (Cursor, OpenCode).

### Agreed Concerns

1. **[HIGH — 3/3 reviewers] `advance_evaluated.status` uses Debug-lowercasing** (`main.rs:848`): Plan 01 fixes serde but Plan 04 keeps the existing emit, so new variants hit `events.jsonl` as `resourcekilled`/`agentunavailable` — contradicting Plan 01's own prohibition and breaking D-10/18d reconciliation. Fix: a shared wire-name helper (serde-based `as_wire_str()`) required in Plan 01 and consumed in Plan 04.
2. **[HIGH — 2/3 + independently verified] Cargo package name is `devflow`, not `devflow-cli`** (`crates/devflow-cli/Cargo.toml:2`): 11 occurrences of `-p devflow-cli` across the plans' verify/acceptance commands would all fail before testing anything (Phase 15 already hit this trap). Also invalidates Plan 05's D-17 identity check if it matches on package name `devflow-cli` — detection must use workspace member paths (`crates/devflow-core` + `crates/devflow-cli`) or packages `devflow` + `devflow-core`.
3. **[HIGH — 2/3] Plan 01 compile break**: `AgentResult` is also constructed in `main.rs:1551`; adding `decided_by_layer` while Plan 01's `files_modified` excludes `main.rs` breaks the workspace build at Wave 1. Fix: workspace-wide literal update (`rg 'AgentResult \{'`) or a constructor/`#[serde(default)]`-style migration.
4. **[HIGH — 2/3] Infra outcomes must never route through `handle_validate_outcome(false)`** (`main.rs:896-898` increments `consecutive_failures`): Plan 04's "Validate/Ship keep their handlers" wording conflicts with D-08. GateInfra needs its own path: saturating infra bump → persist → abort at ceiling → else `handle_stage_failure`, for ALL stages.
5. **[HIGH/MEDIUM — 3/3] Cron resume command underspecified and unsafe as sketched**: `devflow start --phase N --agent X` omits `--mode` (OpenCode), requires `--mode` and recreates branch/worktree before reading state (Codex, `main.rs:75`, `main.rs:568`) — a resume must relaunch saved state, not start a new workflow. Specify the exact argv (mode/worktree preservation) or add a dedicated resume entry point.
6. **[HIGH — 2/3] Preflight universal checks lack concrete predicates/data sources**: "reviewer receiver set" has no storage anywhere (`state.rs:17`, `config.rs:55` have no such field), and "required security artifact" names no path/key. AC-4 is unverifiable as written — define the artifacts or narrow AC-4 to what Phase 17 actually implements (`gh auth` + trait hook).
7. **[HIGH/MEDIUM — 3/3] `build.rs` rerun-if-changed paths don't match the workspace layout**: relative `.git/HEAD` resolves against `crates/devflow-cli/`, not the repo root (Codex); packed-refs and worktree layouts are missed (all three touched this). Fix: resolve via `git rev-parse --git-common-dir` from `CARGO_MANIFEST_DIR`, emit absolute paths, include `packed-refs`.

### Divergent Views

- **Overall risk**: Codex and Cursor rate the phase HIGH (blockers before execute); OpenCode rates MEDIUM (correctly traced, fixable gaps). The difference is weighting of the same findings, not disagreement on facts.
- **OpenCode only**: `evaluate_layer0` worktree PLAN-discovery gap (`agent_result.rs:643` + `verify.rs:30`) — external probes may silently fail in worktree phases; pre-existing Phase 16 issue whose blast radius Phase 17's stage-lift amplifies. Suggested split: `project_root` for PLAN discovery, `execution_root` for probe execution. Well-evidenced; worth adopting.
- **Codex only**: existing Codex-interactivity preflight hard-errors before state exists (`main.rs:530`), bypassing the planned gate+notify model; and Plan 05's staleness block via `CliError` is a hard exit inconsistent with D-15's gate+notify semantics (Cursor raised the same asymmetry).
- **Codex vs others on Layer 0 short-circuit**: Codex/Cursor both want an explicit test that affirmative Layer 0 Success intentionally outranks a Layer 1 agent-reported failure — flagged as intentional-but-untested rather than wrong.

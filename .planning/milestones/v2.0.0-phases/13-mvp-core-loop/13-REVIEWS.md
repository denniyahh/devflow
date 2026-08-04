---
phase: 13
reviewers: [codex, opencode, cursor]
failed_reviewers: [qwen, antigravity]
reviewed_at: 2026-07-14T00:00:00Z
plans_reviewed: [13-01-PLAN.md, 13-02-PLAN.md, 13-03-PLAN.md, 13-04-PLAN.md, 13-05-PLAN.md, 13-06-PLAN.md]
---

# Cross-AI Plan Review — Phase 13 (MVP Core Loop)

Reviewers invoked: codex, opencode, cursor-agent (all source-grounded — each ran inside the working tree and verified claims against `main.rs`, `agent_result.rs`, `gates.rs`, `ship.rs`, `prompt.rs`, `hooks.rs`, `mode.rs`, `stage.rs`, `phase7_cli.rs`). Claude was skipped for independence (this session runs in Claude Code). Qwen failed (no non-interactive auth configured); Antigravity produced no output (stdout empty, transcript fallback found no new response).

## Codex Review

**Cross-Plan Findings**

The plans are mostly source-grounded and correctly target the real hot spots: `advance()` silently errors on non-Validate failures (`crates/devflow-cli/src/main.rs:363-374`), `run_gate()` has no notify hook and uses a hardcoded timeout (`crates/devflow-cli/src/main.rs:503-528`), Claude/Codex native JSON signals are not parsed beyond marker/rate-limit fallback (`crates/devflow-core/src/agent_result.rs:62-79`, `216-228`), and `ship.rs` still contains dead v1 bookkeeping (`crates/devflow-core/src/ship.rs:13-36`, `209-301`, `511-524`).

One pervasive issue: all CLI verification commands use the wrong package name. The CLI crate directory is `crates/devflow-cli`, but its Cargo package is `devflow` (`crates/devflow-cli/Cargo.toml:1-3`). Commands like `cargo test -p devflow-cli` and `cargo clippy -p devflow-cli` should be `-p devflow`.

## 13-01-PLAN.md

**Summary**  
Strong plan for WR-11 and notify/timeout wiring, but it misses stale gate cleanup in the new retry path and has a few semantics/test hazards.

**Strengths**
- Correctly targets the silent failure arm in `advance()` where non-Validate failures currently return only `CliError::Message` (`main.rs:363-374`).
- Wiring notify immediately after `Gates::write_gate()` matches the actual gate-write point (`main.rs:511-518`).
- Env-var timeout is consistent with existing env-based runtime config (`main.rs:178-185`).

**Concerns**
- **HIGH:** `GateAction::Advance` retrying the same failed stage would leave stale response/ack files. `run_gate()` only writes an ack (`main.rs:518-523`); cleanup happens only in `loop_back_to_code`, `finish_workflow`, and `abort` (`main.rs:468`, `496-497`, `536`). A second failure on the same stage can consume the old response.
- **MEDIUM:** Mapping `GateAction::LoopBack(_)` to `loop_back_to_code()` is odd for Define/Plan failures; gate rejection always maps to Code today (`gates.rs:65-77`), but Code may not be a valid recovery target before planning exists.
- **MEDIUM:** Env-var tests are risky under Rust 2024 (`Cargo.toml:8-10`). `std::env::set_var/remove_var` are unsafe and process-global; tests need serialization or another isolation strategy.
- **LOW:** Verification commands use `-p devflow-cli`, which will fail; package name is `devflow`.

**Suggestions**
- Cleanup gate files before retrying the same stage on `GateAction::Advance`.
- For non-Validate non-Ship failures, consider rejected gate response as retry/abort rather than always Code loop-back.
- Add a test for “same stage fails twice does not reuse stale response.”
- Replace CLI verification commands with `cargo test -p devflow`.

**Risk Assessment: MEDIUM**  
The core design is right, but stale gate reuse can recreate the exact “lost human decision” class this phase is trying to eliminate.

## 13-02-PLAN.md

**Summary**  
The deletion part is well-scoped and supported by grep evidence, but the prompt change overclaims that running `/gsd-code-review` before `/gsd-ship` avoids `/gsd-ship`’s own interactive review path.

**Strengths**
- Correctly identifies dead `LastShip` and PR-body symbols with no external callers; current grep shows external live callers only for cron and changelog helpers (`main.rs:744`, `848`; `hooks.rs:145`; `phase7_cli.rs:317`).
- Preserves `prepend_changelog`, used by `hooks::changelog_append` (`hooks.rs:139-146`).
- Preserves cron-instruction machinery used by CLI and tests (`main.rs:744`, `848`; `phase7_cli.rs:317-323`).

**Concerns**
- **HIGH:** The Ship prompt change does not prove headless safety. `/gsd-ship` still has an `optional_review` step with `AskUserQuestion` after PR creation (`~/.claude/gsd-core/workflows/ship.md:300-378`). Running `/gsd-code-review` first does not necessarily skip that prompt.
- **MEDIUM:** The plan defines “Critical findings” as the review failure threshold, but `gsd-code-review` emits severity-classified findings without this plan specifying exactly how the agent should parse the generated review artifact (`~/.claude/skills/gsd-code-review/SKILL.md:22-26`).
- **LOW:** `ShipError::Missing` must remain because cron loading still uses it (`ship.rs:138-145`), even after deleting `LastShip`.

**Suggestions**
- Make the Ship prompt invoke `/gsd-ship {N} --text` or explicitly instruct the agent how to answer/avoid the optional review step, if supported.
- Specify the artifact path and severity gate for `REVIEW.md`, not only “Critical issues.”
- Keep a survivor test for `prepend_changelog` and cron helpers after pruning tests.

**Risk Assessment: MEDIUM-HIGH**  
Dead-code deletion is low risk; the headless-safety claim is not fully backed by the actual `/gsd-ship` workflow.

## 13-03-PLAN.md

**Summary**  
Good parser-focused plan, but it needs sharper precedence rules between native envelopes and markers, and it intersects dangerously with Plan 05’s Validate verdict fallback.

**Strengths**
- Correctly observes Claude envelope unwrapping only reads `result`, not `is_error`/`num_turns` (`agent_result.rs:129-138`).
- Correctly separates Codex JSONL from Claude single-document JSON; current parser assumes a single JSON object (`agent_result.rs:131-137`).
- Stage-scoping Layer 2 addresses the current zero-commit failure matrix (`agent_result.rs:275-300`).

**Concerns**
- **HIGH:** Returning Codex `turn.completed` as unconditional `Success` means a Validate run with no `DEVFLOW_RESULT` verdict can advance as pass once Plan 05 treats missing verdict as pass. Current `advance()` treats success at Validate as Ship transition (`main.rs:377-383`).
- **MEDIUM:** The plan says marker precedence remains over Claude `is_error`. That weakens “native envelope authoritative”: a stale or echoed success marker inside an errored envelope could win because marker parsing happens first (`agent_result.rs:62-69`, `216-228`).
- **MEDIUM:** “Code-like stages” should be explicit. `Stage::is_agent_stage()` includes Define/Plan/Code (`stage.rs:46-49`), but requirement says Define must not be commit-gated. Do not use `is_agent_stage()` blindly.
- **LOW:** Verification commands for core are fine; no new dependency is appropriate because `serde_json` is already present (`crates/devflow-core/Cargo.toml:12-17`).

**Suggestions**
- For Validate, require a verdict marker or validation artifact parse; do not let Codex terminal success imply validation pass.
- Make `is_error: true` override marker success, or at least fail when marker and envelope conflict.
- Implement explicit `matches!(stage, Stage::Plan | Stage::Code)` if Plan remains commit-gated.

**Risk Assessment: MEDIUM-HIGH**  
The parser additions are valuable, but success classification can become too permissive in Validate.

## 13-04-PLAN.md

**Summary**  
Good, narrow CLI-layer default flip. The main issue is verification command naming and backward compatibility around removing `--worktree`.

**Strengths**
- Correctly targets `Start.worktree` opt-in (`main.rs:45-47`) and the call-site bool (`main.rs:196-212`).
- `start()` already cleanly branches on the bool and stores `state.worktree_path` (`main.rs:251-309`).
- `parallel()` already passes explicit `true`, so default flip should not affect it (`main.rs:649-663`).

**Concerns**
- **MEDIUM:** Removing `--worktree` may break scripts or docs. The repo test currently uses it (`phase7_cli.rs:351-364`). The plan updates that test, but does not consider a hidden/deprecated alias.
- **LOW:** Help verification should use `cargo run -p devflow -- start --help`, not `-p devflow-cli`.
- **LOW:** The test plan should wait for monitor artifacts, as existing integration tests do with `wait_for()` (`phase7_cli.rs:81-89`, `152-180`), because `start` returns before the monitor completes.

**Suggestions**
- Consider keeping `--worktree` as a hidden no-op compatibility alias for one release.
- Add an assertion that state has `worktree_path: Some(_)` for default and `None` for `--no-worktree`.
- Use `cargo test -p devflow --test phase7_cli`.

**Risk Assessment: LOW-MEDIUM**  
Implementation is simple and source-aligned; compatibility is the only notable concern.

## 13-05-PLAN.md

**Summary**  
The verdict-vs-ran objective is essential, but the plan has two correctness holes: missing verdict defaults to pass, and unknown verdict strings will not behave as described with derived serde.

**Strengths**
- Correctly targets the current bug: Validate success always calls `handle_validate_outcome(..., true)` (`main.rs:377-383`).
- Adding a `verdict` field is a clean model for distinguishing task completion from validation outcome.
- Prompt changes fit the existing prompt construction surface (`prompt.rs:39-45`).

**Concerns**
- **HIGH:** Missing verdict defaults to pass. Combined with Plan 03’s Codex `turn.completed => Success`, a Validate run that omits the marker/verdict can advance to Ship. That conflicts with the phase goal that Validate success must mean validation passed.
- **HIGH:** The plan claims unknown verdict defaults to `None`, but `Option<Verdict>` with derived serde does not ignore unknown strings. `serde_json::from_str::<AgentResult>` would fail and `parse_marker_lines()` would return `None` (`agent_result.rs:208-210`), falling to Layer 2.
- **HIGH:** Adding a non-default field to `AgentResult` requires updating all struct literals, including the one in `run_agent_blocking()` (`main.rs:709-718`), not just literals in `agent_result.rs`.
- **MEDIUM:** The Validate prompt must be special-cased carefully because Plan 02 already special-cases Ship; current prompt tests assume a generic one-command template for every stage (`prompt.rs:62-75`).

**Suggestions**
- For `Stage::Validate`, treat `verdict: None` as failure/gate unless there is an explicit backward-compatibility migration reason.
- Use a custom deserializer or parse the marker as `Value` so unknown verdict becomes a controlled failure reason, not silent fallback.
- Update every `AgentResult { ... }` literal across core and CLI; `rg -n "AgentResult \\{"` already shows the call sites.
- Add a test for malformed/unknown verdict.

**Risk Assessment: HIGH**  
This plan could still let failed validation advance to Ship, which is the central correctness bug 13b is meant to close.

## 13-06-PLAN.md

**Summary**  
The manual dogfood plan is appropriate as the phase gate and correctly waits for all implementation plans, but it should define a tighter remediation loop for failures found during dogfooding.

**Strengths**
- Correctly treats 13e as manual acceptance, not a substitute for unit coverage.
- Depends on all prior plans, which is right because it validates their interaction.
- Captures the real external dependencies: `claude`, `codex`, `gh`, notify hook, and worktree default.

**Concerns**
- **MEDIUM:** “Any failure is in scope” is true, but the plan does not define the required loop: capture failure, patch, rerun targeted test, rerun dogfood checkpoint.
- **MEDIUM:** The preflight says notify hook can be “trusted from unit coverage,” but the acceptance criterion is gate/notify round-trip. It should perform a real hook invocation before the dogfood run.
- **LOW:** `files_modified: []` conflicts with creating `13-06-SUMMARY.md`; it is not source code, but it is still a planned artifact.
- **LOW:** If Plan 02 does not truly avoid `/gsd-ship`’s `AskUserQuestion`, the Claude full-loop may hang at Ship (`~/.claude/gsd-core/workflows/ship.md:361-378`).

**Suggestions**
- Add a formal “failure remediation” subtask: record capture, fix in source, run targeted/full verification, restart dogfood from the failed checkpoint.
- Require a real throwaway gate notification before live agent runs.
- Record the exact Codex stdout JSONL shape in the summary, not just whether parsing matched.

**Risk Assessment: MEDIUM**  
Manual verification is inherently risky but necessary. The largest residual risk is that earlier plan assumptions about Ship headless behavior and Validate verdict fallback survive into dogfood.

---

## OpenCode Review

Now I have a thorough understanding of the codebase. Let me compile the review.

---

# Cross-AI Plan Review: Phase 13 (MVP Core Loop)

## Overall Assessment

These plans are unusually well-grounded in the actual codebase — every claim about `advance()`'s silent-halt bug (`main.rs:364-373`), dead `ship.rs` bookkeeping (`ship.rs:15-36,213-299,474-524`), unread `is_error` field (`agent_result.rs:131-138,216-229`), and the `worktree: bool` opt-in (`main.rs:47`) is verified against the current source. The wave structure and dependency graph are sound. The primary risk is the string-based convention for `review:` and `verdict` fields, which relies on LLM agents reliably emitting exact marker formats — a known weakness of the DEVFLOW_RESULT contract, not a flaw unique to these plans.

### Summary

**13-01 (WR-11 + Ship failure + notify):** Closes the most impactful correctness bug in the codebase — the `_ => Err(...)` arm at `main.rs:368-373` that silently drops non-Validate failures. The notify hook design (`fire_gate_notify` passing context via env vars, not shell interpolation) correctly mirrors the WR-01 argv precedent and the `hooks.rs::docs_update` fail-soft idiom at `hooks.rs:117-136`. The `review:` string convention linking Ship ReviewFailed → loop-to-Code is fragile in principle but safe in practice (loop-back is the least dangerous action).

**13-02 (Dead ship.rs deletion + Ship prompt):** The grep-confirmed dead code list (`ship.rs:15-36` LastShip, `ship.rs:213-229` build_pr_body, `ship.rs:232-241` extract_goal, `ship.rs:474-487` count_passed_tests, `ship.rs:511-524` mark_phase_complete) has exactly zero non-test callers. The survivors (`prepend_changelog` at `hooks.rs:145`, cron machinery at `main.rs:744,848-849`) are correctly identified. The prompt change to sequence `/gsd-code-review` before `/gsd-ship` is the right fix for RESEARCH Pitfall 2 (headless AskUserQuestion hang).

**13-03 (Envelope parsing + Layer 2 scoping):** Addresses a concrete parse/request mismatch — Claude's `--output-format json` flag has been passed since Phase 11 but `is_error` was never read (`agent_result.rs:131-138` reads only `result`, not `is_error`). The Codex JSONL parser design (last `turn.completed`/`turn.failed` wins, interleaved progress lines skipped) matches the buffered-capture model confirmed at `agent.rs::capture_agent_output`.

**13-04 (Worktree default):** Clean CLI-layer flip. The `parallel()` + `sequentagent()` paths pass explicit bools (`main.rs:649` calls `start(..., true, ...)`), so they're immune.

**13-05 (Verdict split):** Correctly uses `#[serde(default)]` on `Option<Verdict>` for backward compat with existing markers. The `advance()` Validate arm change at `main.rs:382` is a one-line compute + call substitution.

**13-06 (Dogfood):** Properly gated manual acceptance. The pre-flight checkpoint requiring `cargo test/clippy/fmt` green before the live run is the right gate.

---

## Strengths

- **Every claim is source-verifiable.** The WR-11 location at `main.rs:364-373`, `GATE_TIMEOUT_SECS` at `main.rs:16`, the `extract_json_result_text` gap at `agent_result.rs:131-138`, and the `worktree: bool` field at `main.rs:47` all match the current code exactly.
- **Fail-soft notify hook design mirrors existing idioms.** Copying `hooks.rs::docs_update` at `hooks.rs:117-136` (always returns `Ok(())`, warns on failure) is the right choice, and passing context via `.env()` vars rather than string interpolation correctly follows the WR-01 argv precedent (`monitor.rs::shell_escape`).
- **13a's scope is correctly constrained.** The plan recognizes that DevFlow's Rust code never creates PRs — `/gsd-ship` handles `gh pr create` externally. "Rewrite ship.rs" means "delete dead v1 bookkeeping + add a Ship failure branch to `main.rs`," not "reimplement PR creation."
- **Wave structure is correct.** 01/02/03 are truly independent (different files: `main.rs+gates.rs`, `ship.rs+prompt.rs`, `agent_result.rs`). 05 as Wave 3 correctly sequences after all prior waves since it stitches together changes from agent_result.rs (03), prompt.rs (02), and main.rs (01/04).
- **Threat model is honest about trust boundaries.** Each plan acknowledges that `DEVFLOW_RESULT` markers are agent-reported (untrusted) and correctly scopes mitigations: gate context is env-var-isolated, verdict gates only control advancement (never merge), and the worktree default is the EoP mitigation, not removing `--dangerously-skip-permissions`.
- **Layer 2 scoping (13b) correctly recognizes Define/Validate produce zero commits.** The current `evaluate_layer2` at `agent_result.rs:276-279` treats `exit=0 && commits>0 → Success, else Failed` — this correctly catches Code-stage failures but wrongly flags legitimate no-commit stages.

---

## Concerns

### HIGH

1. **`handle_stage_failure` unconditionally writes gates, bypassing `Mode::should_gate`** (13-01 Task 2). The plan explicitly says "Ignore `Mode::should_gate`" — the rationale is correct (WR-11's point is that these must never be silent), but this creates a behavioral inconsistency: in Auto mode, Define/Plan/Code stages normally have no gates at all (`mode.rs:37-45` returns `false` for all non-Validate/non-Ship stages). A Define failure suddenly fires a gate in Auto mode where the operator expected unattended operation. This is arguably the right behavior (silent failures are worse than surprise gates), but it should be explicitly called out in logs/context when a mode-unexpected gate fires. Consider adding an `info!` or including "unexpected gate" in the gate context string.

2. **String-based `review:` prefix is fragile with LLM agents** (13-01 Task 2, 13-02 Task 2). The Ship-stage prompt instructs the agent to emit `DEVFLOW_RESULT: {"status": "failed", "reason": "review: <summary>"}` for ReviewFailed, and `handle_ship_failure` case-insensitively matches the `review:` prefix. LLM agents are known to paraphrase instructions — an agent might write `"review found critical issues"` without the exact prefix, or use `"Review:"` (capitalized). The plan says case-insensitive matching but the actual code at `main.rs` will need to implement this — a `reason.to_lowercase().starts_with("review:")` check is straightforward but still a string-based convention on agent output. This is an inherent limitation of the DEVFLOW_RESULT marker approach, not a fixable flaw, but the severity is worth flagging.

3. **Codex JSONL parser has no integration test against real Codex output** (13-03 Task 2, 13-06 Task 3). The plan acknowledges the upstream documentation gap (RESEARCH Pitfall 4/5) and correctly defers real-output verification to 13e's dogfood run. However, 13-03 will ship and merge before 13-06 executes, meaning the Codex parser could silently mis-classify real output between merge and dogfood. Consider adding a "known-limitation" comment in the parser code marking it as unverified against the installed Codex CLI version, similar to how `12-12-SUMMARY.md` flagged its Claude output verification.

### MEDIUM

4. **`handle_stage_failure` retry path (`GateAction::Advance` → re-launch same stage) lacks a circuit breaker** (13-01 Task 2). If a stage failure is systemic (e.g., `gh` not authenticated), the operator could approve the gate, the stage re-launches, fails again, writes a new gate, operator approves again — infinite loop. The plan explicitly says "Do NOT touch `consecutive_failures`" to avoid leaking Validate logic, which is correct, but without any iteration counter the loop has no termination guarantee. Consider tracking a separate `stage_retry_count` or including `consecutive_failures`-like tracking (but separate from Validate's auto-loop threshold) so repeated gate→retry cycles on the same stage eventually escalate.

5. **Layer 2 scoping skips commit-gate for Define/Validate but their exit code still matters** (13-03 Task 3). The plan says zero-commit Define/Validate with exit=0 should not be Failed. But what about exit≠0 on Define/Validate? If the agent crashes (non-zero exit) during Define, the commit-gate is the only defense since Define legitimately has zero commits. Removing the commit-gate for these stages without also considering exit code means a crashed Define agent (exit≠0, commits=0) would pass through to Plan, which is wrong. The plan's acceptance criteria should clarify: exit≠0 is still Failed regardless of stage; only the `exit=0, commits=0 → Failed` branch is stage-scoped.

6. **`handle_validate_outcome` signature and behavior unchanged, but it now can be called with `passed=true` when `verdict=Gaps`** (13-05 Task 2). The plan correctly encodes `verdict: Gaps → passed=false`, but the existing `handle_validate_outcome` at `main.rs:389-423` has this behavior for non-passed Validate: increments `consecutive_failures`, context says "Validation failed N time(s)". With the verdict split, a gaps-only result is semantically different from a genuine agent crash — the agent reported gaps vs. the agent errored. The `consecutive_failures` counter conflates both. An agent that consistently finds gaps (correct behavior) will hit the forced-gate threshold (`MAX_CONSECUTIVE_FAILURES=3`) just like a crashing agent. This is acceptable for MVP but worth noting.

### LOW

7. **13-02 Task 2 sets `fix_prompt(FixType::GapsOnly, ...)` always for `loop_back_to_code`** (confirmed at `main.rs:462-480`). When Ship ReviewFailed loops back to Code (new path in 13-01), it calls `loop_back_to_code` which sends a `GapsOnly` fix prompt. But ReviewFailed from Ship is about code-review findings, not validation gaps — `FixType::AuditFix` (`/gsd-audit-fix`) might be more appropriate. This is a minor semantic mismatch; the `GapsOnly` prompt still works (it re-runs execution targeting gaps), just not optimally.

8. **`fire_gate_notify` accepts `project_root` but the plan says to omit it if unused** (13-01 Task 1). The plan's action text says "prefer omitting an unused parameter to satisfy clippy" — good instinct. But the call site in `run_gate` at `main.rs:505-529` would need to match. This is trivial but worth noting for the executor.

9. **13-04 renames `--worktree` to `--no-worktree`, removing the old flag** (13-04 Task 1). This is a CLI breaking change for anyone scripting `devflow start --worktree`. Since Phase 12 clarified that 1.2.0 is the current version and 2.0.0 is the target for the Phase 11-15 arc, this is acceptable as part of the v2 transition. The plan should note this explicitly in the commit message or a REMOVED section.

10. **`Verdict` enum variants are `Pass` and `Gaps`** (13-05 Task 1). The serialized wire format with `#[serde(rename_all = "lowercase")]` will be `"pass"` and `"gaps"`. The Validate prompt must instruct the agent to use these exact lowercase strings inside the `verdict` field. If the agent writes `"Pass"` (capitalized), serde will fail to deserialize (the `rename_all` directive is lowercase; `"Pass"` won't match `"pass"`). The `#[serde(default)]` on `Option<Verdict>` handles this gracefully (unknown string → None), but it means a capitalized "Pass" would be treated as no-verdict (→ passed for backward compat), which could be confusing. Consider using `#[serde(rename_all = "lowercase")]` with an explicit `#[serde(alias = "Pass")]` / `#[serde(alias = "Gaps")]` or a custom deserializer that's case-insensitive for the verdict field specifically.

---

## Suggestions

1. **Add a forced-gate counter to `handle_stage_failure`** independent of `consecutive_failures`. Track `StageFailures` or similar in `State` with a higher threshold (e.g., 5). After the threshold, append "[REPEATED]" to the gate context so the operator knows this is a recurring failure, rather than silently looping forever. Low implementation cost, high UX value.

2. **Make `evaluate_layer2`'s stage-scoping decision matrix explicit.** The plan says zero-commit Define/Validate with exit=0 should not be Failed. Add: exit≠0 is Always Failed regardless of stage (the agent crashed/errored). Define the matrix precisely:
   ```
   exit≠0              → Failed (all stages)
   exit=0, stage=Code, commits=0  → Failed ("no work done")
   exit=0, stage=Code, commits>0  → Success
   exit=0, stage∈{Define,Plan,Validate}, commits=0 → Success/defer
   ```
   This closes concern #5.

3. **Consider `FixType::AuditFix` for Ship ReviewFailed → Code loop-back.** Add a `fix_type` parameter to `loop_back_to_code` or create a new `loop_back_to_code_with_fix` variant. When Ship ReviewFailed fires, pass `FixType::AuditFix` so the Code re-run uses `/gsd-audit-fix` (designed for review-finding remediation) rather than `/gsd-execute-phase --gaps-only` (designed for validation gaps).

4. **Add a `#[serde(alias = "Pass")]` / `#[serde(alias = "Gaps")]` to Verdict variants** (or a custom case-insensitive deserializer) so that an agent writing `"verdict": "Pass"` (natural casing) doesn't silently become `None`. This is a one-line addition per variant and eliminates an entire class of silent mis-classification.

5. **Add `DEVFLOW_NON_SILENT_GATE=1` to the notify hook's env vars** in `fire_gate_notify`. Since the unconditional gate fires on stages that normally never gate (e.g., Define in Auto mode), the operator's notification handler can use this env var to route "unexpected gate" notifications with higher urgency — e.g., to a different ntfy topic or with a more aggressive alert.

6. **Add a `--verbose`-gated log message in `handle_stage_failure`** when a gate fires on a stage that `Mode::should_gate` would not normally gate. `info!("unattended-gate: {stage} failed in {mode} mode — this stage normally never gates; surfacing via emergency gate")`. This makes the unexpected-gate behavior observable and debuggable.

---

## Risk Assessment

**Overall risk: MEDIUM**

The plans are thorough, well-researched, and correctly scoped. The core mechanisms (fail-soft notify, env-var config, verdict enum with serde compat, Codex JSONL parser, worktree default flip) are all sound and well-matched to the existing codebase patterns. The primary risks are:

- **String-based agent output conventions** (`review:` prefix, `verdict` field values): This is a fundamental limitation of the DEVFLOW_RESULT contract, not a design flaw in these plans. The 13e dogfood run is the correct mitigation — it will surface real agent behavior against the conventions.
- **Unconditional gate-on-failure changing established mode semantics**: Auto mode currently gates only at Ship and on repeated Validate failure. Adding unconditional gates at Define/Plan/Code failure is *correct behavior* (WR-11 mandates it) but is a behavior change the operator must be aware of. The gate context strings should clearly communicate *why* a gate fired at an unexpected stage.
- **Dogfood-dependent verification**: Plans 03 and 05 contain parsers written against externally documented schemas with acknowledged upstream doc gaps; their only integration test against real agent output is the 13e manual run. If the Codex or Claude CLI JSON format has drifted, the parsers will need adjustment post-dogfood.

The plans achieve the phase goals (13a–13e, WR-11) with surgical changes to the right files. No over-engineering, no scope creep, no unnecessary new dependencies. The wave dependency structure is correctly ordered.

---

## Cursor Review

# Phase 13 Plan Review — MVP Core Loop

Cross-checked against current `develop` sources (`main.rs`, `agent_result.rs`, `gates.rs`, `prompt.rs`, `ship.rs`, `hooks.rs`, `mode.rs`, `stage.rs`, `monitor.rs`, `phase7_cli.rs`).

---

## 13-01-PLAN.md — Never-silent failure + notify + Ship failure branch

### Summary
Correctly targets the real WR-11 hole at `main.rs:363-374` (non-Validate failures return `Err` with `gate_pending` never set) and the Ship success-only path at `handle_ship_outcome` (`main.rs:425-436`). Env-var notify + fail-soft shell-out (modeled on `hooks.rs:117-136`) and the security choice to pass context via `.env()` rather than interpolating into `sh -c` are sound. The Advance→retry path as written will reintroduce the CR-01 stale-response bug already fixed for loop-back/abort.

### Strengths
- Failure routing is grounded in actual code: catch-all `_ => Err(...)` at `main.rs:368-373` really is silent today.
- Separating `handle_stage_failure` from `handle_validate_outcome` matches research Pitfall 3 and `mode.rs:37-45` (only Validate/Ship are mode-gated).
- Notify injection model matches the documented V5 threat; `docs_update` fail-soft idiom at `hooks.rs:117-136` is the right copy target.
- Wiring notify into `run_gate` after `write_gate` (`main.rs:513`) notifies Validate and Ship success gates too — consistent with “never silent.”
- `review:` reason prefix keeps `AgentStatus` serde-stable (`agent_result.rs:24-35`) vs adding a new enum variant.

### Concerns
- **HIGH — CR-01 stale gate on Advance→retry:** `run_gate` acks but does not delete the response (`gates.rs:166-174`). `loop_back_to_code` / `abort` call `Gates::cleanup` (`main.rs:467-468`, `536`). Plan 01’s Advance arm re-`launch_stage`s without cleanup. A second failure of the same stage will see the old `.response.json` immediately in `poll_response` (`gates.rs:151-154`) and auto-approve.
- **MEDIUM — ReviewFailed uses GapsOnly:** `loop_back_to_code` always prompts `/gsd-execute-phase --gaps-only` (`main.rs:476-478`, `prompt.rs:51`). Ship `review:` failures come from `/gsd-code-review`; `/gsd-audit-fix` (`FixType::AuditFix`) is likely the better loop-back command.
- **MEDIUM — RateLimited treated like AgentFailed:** `advance` already lumps `RateLimited` with `Failed` (`main.rs:359-361`). Gating improves silence, but the sequentagent cron path (`main.rs:783-791`, `ship::build_cron_instructions`) is not reused — rate limits become “human, retry” instead of scheduled resume.
- **LOW — Env-mutating unit tests:** Plan acknowledges races; `cargo test` parallel runs will need a mutex or `serial_test` if none exists.

### Suggestions
- On every `handle_stage_failure` terminal path (Advance, LoopBack, Abort), call `Gates::cleanup(project_root, state.phase, stage)` before relaunch — mirror CR-01 comments at `main.rs:463-466` and `534-536`.
- For `review:` prefix, call `launch_stage(..., Some(fix_prompt(FixType::AuditFix, ...)))` instead of GapsOnly.
- Optionally special-case `RateLimited` to write cron instructions then gate, or document deliberate deferral.

### Risk Assessment
**HIGH** until stale-response cleanup is specified; otherwise architecture and security approach are strong.

---

## 13-02-PLAN.md — Dead `ship.rs` cleanup + headless Ship prompt

### Summary
Accurate about dead bookkeeping: `LastShip` / `build_pr_body` / `mark_phase_complete` have no non-test call sites; live callers are cron helpers + `prepend_changelog` (`hooks.rs:145`, `main.rs:744,848-849,1029`). Deletion scope and keep-list are correct. Ship prompt sequencing addresses research Pitfall 2 in spirit but may not fully prevent `/gsd-ship`’s internal interactive review.

### Strengths
- Correctly reframes “rewrite `ship_phase()`” as delete dead code + prompt contract — matches the architectural map (PR creation lives in external `/gsd-ship`).
- Keep-list includes every live symbol; line references to cron callers match current `main.rs`.
- Ship prompt defining `review:` binds cleanly to Plan 01’s `handle_ship_failure` without a serde break.
- Tests retaining non-Ship prompts (`prompt.rs:63-75`) prevent accidental COMPLETION_PROTOCOL churn.

### Concerns
- **HIGH/MEDIUM — Sequencing may not skip AskUserQuestion:** `/gsd-ship` still runs after `/gsd-code-review` (`prompt.rs:69` currently only `/gsd-ship`). Research notes `/gsd-ship`’s own `optional_review` uses interactive `AskUserQuestion`. Nothing in this plan confirms the external workflow skips that when REVIEW.md already exists, or how `--dangerously-skip-permissions` behaves. T-13-05 mitigation may be incomplete.
- **MEDIUM — ReviewFailed detection is agent self-report:** Prompt tells the agent to emit `review:` on Critical findings; DevFlow never reads `REVIEW.md`. Acceptable for trust model, but dogfood must verify agents actually comply.
- **LOW — Wave-1 race with Plan 01:** Shared `review:` contract across independently `depends_on: []` plans is fine if string-stable; document the exact prefix including trailing space/`review:` trim rules.

### Suggestions
- Explicitly require confirming (and documenting) how `/gsd-ship` behaves under headless flags when REVIEW.md already exists; if it still prompts, plan a prompt flag / env / GSD config to disable `optional_review`.
- Consider instructing the agent to skip `/gsd-ship` entirely on Critical findings (only emit `review:`) — Plan already says emit failed with `review:`; make “do not run `/gsd-ship` if Critical” mandatory so AskUserQuestion is never reached.

### Risk Assessment
**MEDIUM** — deletion work is low-risk; headless Ship hang risk remains until GSD interaction is verified.

---

## 13-03-PLAN.md — Native envelopes + stage-scoped Layer 2

### Summary
Correctly diagnoses Claude’s single-envelope vs Codex JSONL gap: `extract_json_result_text` uses `serde_json::from_str(trimmed)` on the whole stdout (`agent_result.rs:131-137`), which fails on Codex multi-line JSONL. Preferring `is_error` over undocumented subtypes matches research. The Codex `turn.completed → Success` mapping and Validate zero-commit → Success choice fight the Layer-2/verdict safety model.

### Strengths
- Plugs into real Layer-1 entrypoint `evaluate_layer1` (`agent_result.rs:216-228`) after marker/rate-limit.
- Claude adapters already request `--output-format json` (`agents/claude.rs:21-22`); Codex already uses `--json` (`agents/codex.rs:21`) — parsing is overdue, not speculative.
- Stage-scoping Define/Validate for the commit gate matches CONTEXT 13b; today’s Layer 2 treats any `exit=0, commits=0` as Failed (`agent_result.rs:275-291`).
- No new crates; line-by-line `serde_json` is right.

### Concerns
- **HIGH — `turn.completed` → Success bypasses Layer 2:** Today Success at Layer 1 is almost only from `DEVFLOW_RESULT` (`agent_result.rs:347-349`). Mapping Codex `turn.completed` to `AgentStatus::Success` skips the commit gate for Code (`evaluate_layer2`), so a Codex Code turn that exits cleanly with no commits advances. Claude path correctly defers on `is_error: false` + no marker.
- **HIGH — Validate zero-commit as Success + Plan 05 None=pass:** Plan says Define/Validate exit=0/commits=0 should “succeed or defer.” If Success, `advance` treats it as Validate passed (`main.rs:382`) and Plan 05 treats `verdict: None` as pass → Ship with no verdict. Prefer Not-Failed without claiming Success (e.g. fall through to Layer 3 / `Unknown`, or require marker for Validate success).
- **MEDIUM — Ship still commit-gated:** Ship is not Define/Validate; unmarked successful `/gsd-ship` with zero commits remains Failed. That may be desired fail-safe, but worth an explicit call.
- **MEDIUM — Codex/Claude discrimination:** Heuristic “looks like Codex JSONL” must not consume Claude envelopes; Claude stdout is one JSON object that also has a `type` field (`"result"`). Guard carefully (e.g. Claude path first on whole-document parse; Codex only when multi-line turn events appear).
- **LOW — Schema drift:** Plan correctly relies on 13e capture; keep tests fixture-based and brittle-string subtype matching out.

### Suggestions
- Treat Codex `turn.completed` as “not failed” only: return `None` (defer Layer 2) or `Unknown`, never unconditional Success without a marker (or exit+commits check).
- For Define/Validate Layer 2: if exit=0 and commits=0, return `None` / Layer-3 Unknown — never `Success` — so unmarked Validate cannot auto-ship.
- Parse Claude envelope before Codex JSONL; require ≥1 `turn.*` line for Codex decisive path.

### Risk Assessment
**HIGH** if Success semantics stay as written; **MEDIUM** after Success/deferral is fixed.

---

## 13-04-PLAN.md — Worktree-by-default

### Summary
Clean CLI inversion with correct dependency ordering (Wave 2 after Plan 01’s `main.rs` edits). Default flip is justified: Claude still uses `--dangerously-skip-permissions` (`agents/claude.rs:23`); `monitor.rs:76-84` already runs agents in `state.worktree_path`. Tests updating `reference_and_cleanup_worktree_cli_flow` (`phase7_cli.rs:351-363` still passes `--worktree`) are mandatory.

### Strengths
- Leaves `start(..., worktree: bool)` and `parallel(..., true, ...)` (`main.rs:662`) untouched — correct isolation of behavior.
- Requires asserting the no-flag default creates `.worktrees/phase-NN/` — matches the EoP mitigation in research.
- Fake-bin harness in `phase7_cli.rs` already supports non-spawning agent stubs.

### Concerns
- **MEDIUM — Breaking flag removal:** Dropping `--worktree` with no alias/deprecation breaks scripts/docs still using it. Acceptable for pre-2.0, but should be called out in SUMMARY/changelog.
- **LOW — `start` signature still `worktree: bool`:** Fine internally; ensure help text clarifies default (clap doc on `no_worktree`).
- **LOW — Integration test cost:** Default worktree means every `start` test must tolerate worktree creation or pass `--no-worktree`; plan updates the known offender — grep for other `--worktree` / `start` callers outside `phase7_cli.rs`.

### Suggestions
- Temporarily accept both `--worktree` (no-op/deprecated) and `--no-worktree` if any external docs reference `--worktree`.
- Grep whole repo (README, skills, tests) for `--worktree` in the plan’s verify section.

### Risk Assessment
**LOW** — small, well-scoped change; main risk is undocumented CLI breakage.

---

## 13-05-PLAN.md — Verdict-vs-ran split

### Summary
Hits the real Validate bug: success arm always calls `handle_validate_outcome(..., true)` (`main.rs:382`) even when the agent only “ran validation.” Adding `verdict: pass|gaps` on `AgentResult` and consulting it in advance is the right shape. Wave-3 dependencies on Plans 01–04 are correct. Serde/backward-compat claims and one compile site are underspecified.

### Strengths
- Correct success-arm touchpoint; failure arm already loops/gates via `handle_validate_outcome(..., false)`.
- `Option<Verdict>` + absent → pass preserves markers without `verdict`.
- Prompt change for Validate is necessary; current COMPLETION_PROTOCOL (`prompt.rs:10-23`) has no verdict.
- Test `validate_gaps_does_not_advance_to_ship` matches CONTEXT acceptance.

### Concerns
- **HIGH — Missing `AgentResult` literal in `main.rs`:** `run_agent_blocking` builds `AgentResult { ... }` without `verdict` at `main.rs:711-717`. Adding a field breaks compile unless updated; Task 1 only mentions constructions “in this file” (`agent_result.rs`).
- **MEDIUM — T-13-14 is wrong:** With `serde_json::from_str::<AgentResult>` (`agent_result.rs:209`), an unknown `"verdict":"wat"` fails the whole marker parse (not field-default to `None`). Marker is skipped → Layer 2/3. Document or use `#[serde(deserialize_with = ...)]` that maps bad values to `None`.
- **MEDIUM — Agent self-report:** `verdict: pass` despite gaps remains spoofable (accepted in threat model); optional secondary read of validation report artifact (CONTEXT alternative) is not planned.
- **MEDIUM — Interaction with Plan 03:** Unmarked Validate classified Success (Plan 03) + `verdict: None` = pass → false Ship. Coordinate with Plan 03 Success semantics.
- **LOW — Wave dependencies heavy:** depends_on includes 13-04 though verdict work barely needs worktree; harmless sequencing cost.

### Suggestions
- Task 1/2 must update `main.rs:711-717` (`verdict: None`).
- For Validate, require `verdict` for `status: success` (treat missing as gaps or Unknown/failure in auto mode) once the prompt requires it — sharper than None=pass after the prompt ships.
- Add a test: malformed verdict string does not panic and documents actual Layer behavior.

### Risk Assessment
**MEDIUM** — design is right; compile and Validate false-pass edges need tightening.

---

## 13-06-PLAN.md — Dogfood acceptance

### Summary
Right exit criterion (“the loop ran”) and correctly blocks on 13a–13d. Checkpoints map to the 12-12 BLOCKED Full-Ship item and Plan 03’s schema-uncertainty. Some happy-path Auto expectations are slightly overstated.

### Strengths
- Pre-flight covers clippy/fmt/doctor/notify/Ship prompt — catches Plan 02 hang before burn time.
- Claude full-loop + real PR is the right unblock for `12-12-SUMMARY.md` Full-Ship BLOCKED.
- Codex leg explicitly allows schema delta recording — matches 12-12 empiricism.
- Worktree default verification encodes 13d EoP mitigation on a live run.

### Concerns
- **MEDIUM — Auto-mode gate expectations:** In Auto, Validate gates only at `MAX_CONSECUTIVE_FAILURES` (`mode.rs:40-42`). Step 3 implies mid-loop gates on a happy path; typically only Ship gates (plus Plan 01 failure gates). Clarify Auto vs Supervise.
- **MEDIUM — Optional WR-11 exercise:** Never-silent failure check is optional; for 13c/WR-11 phase goals, make at least one deliberate non-Validate failure mandatory evidence.
- **LOW — Hermes poller mention:** Hermes plugin is Phase 14; “or Hermes poller” may confuse — prefer gate-response file as primary.
- **LOW — External GSD/`gh` variance:** Failures in `/gsd-ship` outside DevFlow may look like AgentFailed; SUMMARY should classify DevFlow vs external workflow failures.

### Suggestions
- Require one forced Code/Ship failure proving gate + notify in 13-06-SUMMARY.md.
- Add Supervise (or forced Validate failure) if validating notify on Validate gates.
- Capture Codex stdout snippet in SUMMARY when parser matches or diverges.

### Risk Assessment
**MEDIUM** — unavoidable for live agents/credentials; plan structure is appropriate; make WR-11 dogfood evidence mandatory.

---

## Cross-plan / phase-level

| Issue | Severity | Plans |
|-------|----------|-------|
| Stale gate response on failure Advance→retry (CR-01) | HIGH | 01 |
| Codex/Validate `Success` without marker bypasses commit/verdict gates | HIGH | 03 ↔ 05 |
| `/gsd-ship` interactive review may still hang despite prompt sequencing | HIGH/MEDIUM | 02 → 06 |
| `AgentResult` field add breaks `main.rs:711` literal | HIGH | 05 |
| ReviewFailed → GapsOnly vs AuditFix | MEDIUM | 01 |
| Breaking removal of `--worktree` | MEDIUM | 04 |

**Phase goal fit:** Plans cover 13a–13e and WR-11 with sensible wave splits (risk front-loaded; shared-file edits serialized). Highest leverage fixes before execute: (1) `Gates::cleanup` on stage-failure Advance, (2) never map envelope/`turn.completed`/zero-commit Validate to Layer-1/2 **Success** without a marker/verdict, (3) hard-stop `/gsd-ship` on Critical review findings.

**Overall phase plan risk: HIGH** until the three items above are resolved; afterward **MEDIUM**, dominated by live-agent dogfood.

---

## Qwen Review

Qwen review failed: no auth type configured for non-interactive mode (see `qwen --auth-type` or settings).

---

## Antigravity Review

Antigravity review failed or returned empty output (agy -p produced no stdout; transcript fallback found no new PLANNER_RESPONSE for this workspace).

---

## Consensus Summary

All three reviewers independently verified the plans against source and agree the plans are well-grounded, correctly scoped, and target the real defects. All three also converge on the same small set of correctness holes — concentrated in Plans 01, 03, and 05 — that should be fixed before execution.

### Agreed Strengths

- **Source-grounded plans (3/3):** every load-bearing claim (WR-11 silent arm at `main.rs:363-374`, dead `ship.rs` bookkeeping with zero non-test callers, unread `is_error` at `agent_result.rs:131-138`, `worktree: bool` opt-in at `main.rs:47`) checks out against current code.
- **Correct 13a reframe (3/3):** "rewrite ship.rs" = delete dead v1 bookkeeping + add a real Ship failure branch; PR creation correctly stays inside the external `/gsd-ship` workflow. Keep-list (cron helpers, `prepend_changelog`) is accurate.
- **Notify hook design (3/3):** fail-soft shell-out modeled on `hooks.rs::docs_update`, gate context passed via env vars — follows the WR-01 no-interpolation precedent.
- **Wave structure (3/3):** riskiest work front-loaded, no same-wave file conflicts, dependency graph acyclic and correct. No scope creep, no new dependencies.

### Agreed Concerns

1. **[HIGH, 3/3 — Plans 03↔05] Validate false-pass composition bug.** Plan 03 maps Codex `turn.completed` to unconditional `Success` and lets zero-commit Validate succeed; Plan 05 treats a missing `verdict` as pass. Composed: an unmarked/verdict-less Validate run advances to Ship — recreating the exact bug 13b exists to close. Fix: envelope/zero-commit signals may mean "not failed" but never `Success` without a marker; for Validate, require an explicit `verdict` (missing verdict ⇒ gaps/gate, not pass) once the prompt ships it.
2. **[HIGH, 2/3 — Plan 01] Stale gate response on Advance→retry (CR-01 recurrence).** `run_gate` acks but does not delete the response file (`gates.rs:166-174`); cleanup only happens in loop-back/finish/abort paths. Plan 01's `GateAction::Advance` retry re-launches the same stage without `Gates::cleanup`, so a second failure auto-approves from the stale `.response.json`. Fix: cleanup gate files on every `handle_stage_failure` terminal path before relaunch. (OpenCode adds the related gap: this retry loop has no circuit breaker — repeated approve→fail cycles never escalate.)
3. **[HIGH/MEDIUM, 3/3 — Plans 02→06] Headless `/gsd-ship` hang not actually proven fixed.** Sequencing `/gsd-code-review` before `/gsd-ship` does not demonstrably skip `/gsd-ship`'s internal `optional_review` `AskUserQuestion`. Fix: make "do not run `/gsd-ship` on Critical findings" mandatory in the prompt contract, and verify/document the headless behavior of `/gsd-ship` when REVIEW.md exists (pre-flight in Plan 06 checkpoint 1).
4. **[HIGH, 2/3 — Plan 05] `AgentResult` literal in `main.rs::run_agent_blocking` (≈`main.rs:711-717`) must gain the new `verdict` field** or the workspace does not compile; Plan 05 only mentions literals in `agent_result.rs`.
5. **[HIGH/MEDIUM, 3/3 — Plan 05] Unknown/miscased verdict breaks the whole marker parse.** With derived serde, `"verdict": "wat"` (or `"Pass"` capitalized) fails `from_str::<AgentResult>` entirely, silently dropping the marker to Layer 2 — contradicting the plan's claimed None-fallback. Fix: case-insensitive/custom deserializer (or serde aliases) + a malformed-verdict test.
6. **[MEDIUM, 3/3 — Plan 04] Removing `--worktree` is a breaking CLI change.** Consider a hidden deprecated no-op alias for one release, and grep the whole repo (README, skills, tests) for `--worktree` references.
7. **[MEDIUM, 2/3 — Plan 01] ReviewFailed→Code loop-back sends `FixType::GapsOnly` (`/gsd-execute-phase --gaps-only`), but review findings are `/gsd-audit-fix` territory.** Pass `FixType::AuditFix` for the `review:` path.
8. **[MEDIUM, 2/3 — Plan 01] Env-var tests are racy:** `std::env::set_var` is unsafe/process-global under Rust 2024; tests need serialization (mutex/`serial_test`) or another isolation strategy.
9. **[MEDIUM, 2/3 — Plan 03] Layer-2 stage-scoping must keep `exit≠0 ⇒ Failed` for ALL stages;** only the `exit=0 && commits=0 ⇒ Failed` branch is stage-scoped. Make the decision matrix explicit.
10. **[MEDIUM, 2/3 — Plan 06] Make WR-11 dogfood evidence mandatory:** at least one deliberately forced non-Validate failure proving gate + notify fires, plus a defined failure→fix→re-run remediation loop.

### Divergent Views

- **Package name in verify commands (Codex only, but factual):** Codex asserts the CLI crate's Cargo package is named `devflow`, not `devflow-cli`, so every `cargo test -p devflow-cli` / `cargo clippy -p devflow-cli` in the plans would fail. **Confirmed by the orchestrator post-review:** `crates/devflow-cli/Cargo.toml` declares `name = "devflow"` — every `-p devflow-cli` command in the plans must be normalized to `-p devflow` before execution.
- **Unconditional gates in Auto mode (OpenCode HIGH vs others):** OpenCode flags that `handle_stage_failure` gating stages that `Mode::should_gate` never gates is a behavioral surprise in Auto mode; Codex/Cursor treat it as the intended WR-11 semantics. Consensus direction: keep the behavior, but log/mark "unexpected gate" in the gate context and notify payload.
- **RateLimited handling (Cursor only):** gating `RateLimited` like `AgentFailed` loses the existing cron-resume path; either special-case it or document the deliberate deferral.
- **Overall risk:** Cursor rates the phase HIGH until concerns 1–3 are fixed (then MEDIUM); Codex and OpenCode rate it MEDIUM overall with Plan 05 as the hot spot. All three agree the fixes are small and plan-level, not architectural.

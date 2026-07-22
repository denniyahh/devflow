---
phase: 20
reviewers: [codex, antigravity, hermes]
reviewed_at: 2026-07-22T23:44:59Z
plans_reviewed: [20-01-PLAN.md, 20-02-PLAN.md, 20-03-PLAN.md, 20-04-PLAN.md, 20-05-PLAN.md]
dropped_reviewers:
  cursor: hit account usage limit (ActionRequiredError) — no review produced
notes: >
  Hermes replaces OpenCode for this run per operator request (update-safe: run
  alongside GSD's built-in dispatch, no global gsd-core edits). Its underlying
  model resolved to deepseek/deepseek-v4-pro. Codex ran its own internal
  code-reviewer + architect lanes. All three completing reviewers were
  source-grounded (file:line evidence verified against HEAD).
---

# Cross-AI Plan Review — Phase 20

> **Panel:** Codex (Request Changes), Antigravity (grounded), Hermes/deepseek-v4-pro (grounded).
> Cursor dropped (usage limit). Three independent source-grounded voices; convergent findings are weighted highest.

## Synthesis — Convergent & High-Value Findings (orchestrator, not a reviewer)

**Actionable before execution (convergence in parens):**

1. **20b — cleanup guard must fail-closed on a live agent pid, not only Healthy/BetweenStages.** `liveness()` (`commands.rs:371`) returns `Unknown` when `monitor_pid: None` (pre-18b state, or agent live with no recorded monitor) and `Stuck` for dead-monitor+live-agent — a guard that only refuses Healthy/BetweenStages can still delete a worktree out from under a live agent. **(Codex HIGH + Hermes MEDIUM.)** Fix: refuse whenever the recorded agent pid is alive regardless of monitor liveness; make the `Unknown` case explicit.

2. **20c — the stop boundary may be off-by-one.** `transition()` sets `state.stage = to` then calls `launch_stage`; a check on `to == stop_until` halts *before* the target stage runs, so `--until plan` would stop before Plan executes, not after PLAN.md is produced. **(Codex HIGH — verify against `pipeline_gate.rs:51-80`, `stage.rs:31`.)** Fix: stop when `from == stop_until` (target already completed).

3. **20c — the doctor gap is bigger than `check_dead_agent`.** `reconcile_phase` also runs `check_dead_monitor` (`commands.rs:1329/:1270`); a stopped phase with a stale `monitor_pid` can still report `Problem`. **(Codex HIGH.)** Also: define whether `stopped`/`stop_reason` clear on `devflow resume`, or the phase stays "stopped" forever. **(Codex MEDIUM + Antigravity LOW + Hermes.)**

4. **20d — a "read-only" preflight must not depend on the network.** The divergence check shells `git fetch origin main develop` (`sync-main-to-develop.sh:38`), which mutates `FETCH_HEAD`/tracking refs and fails offline. **(Codex HIGH + Hermes MEDIUM + Antigravity LOW.)** Fix: check `git merge-base --is-ancestor origin/main HEAD` against already-fetched refs, degrade with an actionable "origin/main not fetched" message; also reject bare `devflow release` (require `--check`, point at the deferred executor). **(Codex MEDIUM.)**

5. **20e — the manual ship can race / re-run a partially-completed live monitor.** Loading state + parsing the response without a per-phase lock can race a still-live monitor's `poll_response` **(Codex HIGH)**; and a response consumed by a monitor that died mid-`finish_workflow` (ack written, terminal hooks partially run) could re-execute Merge/VersionBump. **(Hermes MEDIUM — ack-race.)** Fix: acquire the same per-phase lock; require a Ship gate request/response pair AND check for an existing ack; direct to `devflow doctor` on an inconsistent state.

6. **20a — inline-table parser assumes single-line entries.** A multi-line `[workspace.dependencies]` self-pin would break the line-by-line scan. **(Antigravity MEDIUM + Hermes MEDIUM.)** Fix: track inline-table open/close across lines, or assert/verify single-line entries. Minor: the "don't add a parser dependency" rationale is inaccurate (`toml` is already in `Cargo.lock`; the real reason is comment/format preservation, GAP-6). **(Hermes LOW.)**

**Noted but intentional / already mitigated:**

- **Dependency over-serialization of Wave 2 (20-03→20-04→20-05 chain).** Hermes/Antigravity flag `20-03 depends_on [20-02]` and the `20-04→20-03` edge as over-serializing units CONTEXT.md calls independent. **This is deliberate:** 20-03/20-04/20-05 share `main.rs`/`commands.rs` (new `Command` variants in the same clap enum region) + the help snapshot; the `20-03→20-04` edge was added post-plan-check specifically to prevent a parallel merge conflict. The cost is sequential Wave 2, accepted over a race. Reviewers reviewed `20-03 depends_on [20-02]` as-committed; that edge reflects the shared `commands.rs` region and is also intentional. If worktree-isolated execution is used, these could be relaxed — otherwise keep.
- **Scope discipline:** all three reviewers independently confirmed no scope creep; the release-cut executor (DEN-50) and `devflow parallel` race (DEN-51) are correctly deferred. Antigravity explicitly recommended filing the DEN-51 ticket — already done.

---

## Codex Review

**Overall Verdict: Request Changes**

The plans are mostly source-grounded, and 20a is close to execution-ready. The blockers are in 20b/20c/20d/20e where the plan text slightly misaligns with existing state-machine and read-only contracts. Independent `code-reviewer` and `architect` lanes both returned `REQUEST CHANGES` / `BLOCK`.

**20-01: Version Self-Pin**

**Summary:** Strong plan. It correctly targets `devflow-core::version`, where `write_version` currently rewrites one field and skips inline-table values.

**Strengths**
- Correct source target: `write_version` only calls `replace_version_in_contents` once, then writes the file: [version.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/version.rs:197).
- Correct root cause: inline table values are skipped today: [version.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/version.rs:244).
- Existing release guard remains useful: [workspace_version_pin.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/tests/workspace_version_pin.rs:76).

**Concerns**
- **MEDIUM:** The plan’s `path` rule is currently framed around `crates/`; the workspace member list is the real source of truth: [Cargo.toml](/var/home/denniyahh/Github/devflow/Cargo.toml:3). This is fine for DevFlow today, but weaker as a product rule.

**Suggestions**
- Match local paths against `[workspace].members` where practical, or explicitly document the `crates/` limitation in tests.
- Keep `hooks.rs` untouched; `version_bump` already commits the same `Cargo.toml`: [hooks.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/hooks.rs:239).

**Risk Assessment:** LOW. Mechanical, well-tested, isolated.

**20-02: Cleanup + Flaky Fixtures**

**Summary:** Good direction, but the liveness policy needs tightening. `cleanup` really is currently unsafe, but allowing `Stuck`/`Unknown` removal can still remove a worktree with a live agent.

**Strengths**
- Correctly identifies unconditional removal: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:292), [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:308).
- Reuses existing liveness predicate instead of inventing another one: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:371).
- Correctly avoids `git worktree prune` as primary deletion recovery; `remove` and `prune` are separate operations: [worktree.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/worktree.rs:104), [worktree.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/worktree.rs:114).

**Concerns**
- **HIGH:** `liveness(None, _, live_agent)` returns `Unknown`, and `(dead_monitor, live_agent)` returns `Stuck`: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:371). If cleanup proceeds on `Unknown`/`Stuck`, it can still remove a live agent’s worktree.
- **MEDIUM:** The plan must define the worktree-to-phase join. `cleanup` iterates `git worktree list` entries, while state is keyed by phase files: [workflow.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/workflow.rs:203).

**Suggestions**
- Refuse removal whenever the recorded agent pid is alive, regardless of monitor liveness.
- Add a test for a live agent pid with `monitor_pid = None` and with dead monitor/live agent.
- Match worktrees to `State.worktree_path` first, with branch/path fallback only if needed.

**Risk Assessment:** MEDIUM. The product fix is right, but the guard policy needs a stricter live-agent fail-closed rule.

**20-03: `start --until`**

**Summary:** This is the main blocker. The plan says “stop after Plan completes,” but the proposed `transition()` check on `to == stop_until` stops before the target stage launches.

**Strengths**
- Correctly identifies `transition()` as the central advance path: [pipeline_gate.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:51).
- Correctly keeps `loop_back_to_code` out of the stop path: [pipeline_gate.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:84).
- Correctly recognizes the doctor gap around dead agents: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1247).

**Concerns**
- **HIGH:** `Stage::Define.next()` is `Plan`: [stage.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/stage.rs:31). Since `transition()` sets `state.stage = to` then calls `launch_stage`: [pipeline_gate.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:63), [pipeline_gate.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:79), checking `to == Plan` halts before Plan runs.
- **HIGH:** The plan only suppresses `check_dead_agent`, but `reconcile_phase` also runs `check_dead_monitor`: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1329). A stopped phase with stale `monitor_pid` can still be `Problem`: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1270).
- **MEDIUM:** `resume()` simply reloads state and relaunches the persisted stage: [pipeline_launch.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs:204). The plan needs semantics for clearing `stopped`/`stop_reason` on resume.

**Suggestions**
- Stop when `from == stop_until`, before advancing to `to`, so the requested stage has already completed.
- On stop, clear `monitor_pid`, `gate_pending`, and stale process evidence.
- Teach both `check_dead_agent` and `check_dead_monitor` about stopped phases, or ensure stopped state cannot classify as stuck.
- Add resume tests proving `stopped` is cleared or intentionally preserved with defined behavior.

**Risk Assessment:** HIGH. As written, it likely implements the wrong stop boundary.

**20-04: `release --check`**

**Summary:** Useful feature, but the read-only contract conflicts with the planned `git fetch`, and the CLI shape needs an explicit `--check` gate.

**Strengths**
- Self-pin check correctly depends on 20a and should compare dynamically against workspace version: [Cargo.toml](/var/home/denniyahh/Github/devflow/Cargo.toml:8), [Cargo.toml](/var/home/denniyahh/Github/devflow/Cargo.toml:20).
- Ancestor check uses the existing release-sync logic: [sync-main-to-develop.sh](/var/home/denniyahh/Github/devflow/scripts/sync-main-to-develop.sh:41).
- Publish order is real and documented: [CONTRIBUTING.md](/var/home/denniyahh/Github/devflow/CONTRIBUTING.md:192).

**Concerns**
- **HIGH:** The plan calls `release --check` “strictly read-only,” but `git fetch origin main develop --quiet` mutates refs/FETCH_HEAD: [sync-main-to-develop.sh](/var/home/denniyahh/Github/devflow/scripts/sync-main-to-develop.sh:38).
- **MEDIUM:** `Command::Release { check: bool }` needs to reject bare `devflow release`; current CLI has no release command, so the new surface must avoid silently treating omitted `--check` as a valid check run: [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:43).
- **MEDIUM:** Signing output redaction should match the existing “no path/username” discipline: [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1195).

**Suggestions**
- Either remove fetch and check existing refs, or rename the contract to “working-tree read-only, may update remote refs.”
- Require `--check`; bare `devflow release` should error and say executor is deferred.
- Unit-test signing status classification without requiring a live agent.

**Risk Assessment:** MEDIUM-HIGH. Valuable, but the read-only claim must be made true or revised.

**20-05: Manual Ship Override**

**Summary:** The core reuse design is correct, but the command needs an ownership/lock guard so it cannot race the live monitor consuming the same response.

**Strengths**
- Correctly reuses the existing gate response model: [gates.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/gates.rs:37), [gates.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/gates.rs:69).
- Correctly routes approved Ship through `finish_workflow`: [pipeline_outcomes.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_outcomes.rs:275), [pipeline_gate.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:130).
- Correctly requires `state.stage == Ship`; Ship is terminal and not a Validate bypass.

**Concerns**
- **HIGH:** The live `advance` path owns the phase while running; `ship_override` as planned starts from `workflow::load_state` and response parsing without a lock. That can race a still-live monitor also reading the response via `poll_response`: [gates.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/gates.rs:222).
- **MEDIUM:** If no gate request exists but a response file exists, the plan should define whether that is valid. `Gates::respond` normally refuses when the gate file is absent: [gates.rs](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/gates.rs:179).

**Suggestions**
- Acquire the same per-phase lock before loading state/response; fail if contended.
- Require both `state.stage == Ship` and an existing Ship gate request/response pair.
- `--force` must not bypass stage, gate-existence, or lock ownership checks.

**Risk Assessment:** MEDIUM-HIGH. The reuse path is good, but unsafe without a phase ownership guard.
---

## Antigravity Review

# Cross-AI Plan Review: Phase 20 (Release Correctness + Operator Control)

Overall, these five plans represent a rigorous, well-researched, and carefully sequenced approach to closing the final reliability gaps in the DevFlow phase pipeline. The plans correctly identify and leverage existing architectural patterns (reusing liveness predicates, TOML parsers, and gate logic) rather than inventing parallel mechanisms. The sequencing logic correctly identifies dependency chains and constraints.

Here is the structured feedback for each of the implementation plans.

## 20-01-PLAN.md (20a): Workspace Version Pin

**1. Summary**
This plan corrects the version bumping logic to update both `[workspace.package] version` and `[workspace.dependencies]` local path versions in one pass, utilizing the existing hand-rolled TOML modifier to ensure comment and format preservation.

**2. Strengths**
*   **Correct Source Context:** Correctly identifies that `version.rs` lives in `devflow-core`, fixing the stale documentation.
*   **Zero-Dependency Approach:** Extending the hand-rolled TOML parser respects the strict comment/quote preservation guarantees (GAP-6) that a generic TOML serializer would break.
*   **Atomicity:** Correctly identifies that `version_bump`'s single `commit_path` (in `crates/devflow-core/src/hooks.rs:241`) will capture both file modifications for free.

**3. Concerns**
*   **MEDIUM - Parser Complexity:** The plan correctly notes that `replace_version_in_contents` (in `crates/devflow-core/src/version.rs`) explicitly skips inline tables today (`if !value.trim().starts_with('{')`). Safely parsing inline TOML tables (`name = { path = "...", version = "..." }`) using a hand-rolled line-by-line scanner is notoriously fragile if a user adds unexpected spacing or reorders the keys.

**4. Suggestions**
*   Ensure that the new TOML parsing logic for inline dependencies is strictly anchored to the `path = "..."` and `version = "..."` tokens. Add a unit test specifically asserting that `version` is replaced correctly regardless of whether it appears before or after `path` in the inline table.

**5. Risk Assessment**
**MEDIUM.** Hand-rolling TOML modifications is inherently brittle. The strong suite of existing regression tests mitigates this, but the string manipulation logic must be written with extreme care.

---

## 20-02-PLAN.md (20b): Fixture Reliability & Worktree Race

**1. Summary**
Fixes the underlying product defect behind the `Directory not empty` CI flake by gating worktree cleanup behind the existing 18b liveness check, and improves fixture stability for the git object store flake.

**2. Strengths**
*   **Root Cause Analysis:** Excellent transition from a test-only flake assumption to identifying a genuine product race condition in `cleanup` (in `crates/devflow-cli/src/commands.rs:292`).
*   **Mechanism Reuse:** Perfectly leverages the existing `Liveness` enum and `liveness()` predicate (`crates/devflow-cli/src/commands.rs:371`).
*   **Git Nuance:** Correctly identifies that `git worktree prune` is purely an administrative metadata command and cannot be used as the primary recovery mechanism for leftover files.

**3. Concerns**
*   **LOW - Branch Deletion Ordering:** `cleanup` deletes feature branches immediately after removing the worktree (`commands.rs:312`). If `worktree::remove` enters a bounded retry backoff, ensure the loop doesn't hold up other cleanup operations unnecessarily, and that the retry failure surfaces cleanly.

**4. Suggestions**
*   If the bounded retry loop for `worktree::remove` ultimately fails, print a descriptive warning that the worktree directory must be manually cleared by the user rather than failing silently.
*   Create a distinct Linear/backlog ticket for the unconfirmed `devflow parallel` object-store race (D-08) prior to closing this phase out.

**5. Risk Assessment**
**LOW.** The product fix is highly targeted and appropriately isolated to dead/stuck phases. The test fixture hardening is a standard git durability pattern.

---

## 20-03-PLAN.md (20c): Plan-Only Pipeline Mode

**1. Summary**
Introduces `--until <stage>` to gracefully halt pipeline execution, intercepting control flow at `pipeline_gate::transition` while safely marking the stopped state so that the `doctor` command does not report it as a dead agent.

**2. Strengths**
*   **Choke-Point Interception:** Correctly identifies `transition()` (`crates/devflow-cli/src/pipeline_gate.rs:51`) as the single appropriate bottleneck to cleanly arrest execution without leaving orphaned monitors.
*   **Reconciliation Integration:** Directly addresses the newly discovered `check_dead_agent` false positive in `devflow doctor`.
*   **Backward Compatibility:** Heavily utilizes `#[serde(default)]` to prevent older `State` JSON files from breaking deserialization.

**3. Concerns**
*   **MEDIUM - File Conflict Potential:** This plan (Wave 2) modifies `main.rs` to add `--until` to `Command::Start`. Plan 20-04 (also Wave 2) adds `Command::Release` to `main.rs`. Because both run in Wave 2, they may produce git conflicts depending on the execution order of the autonomous agents.
*   **LOW - State Cleanup:** If a user stops at `Plan`, and later runs `devflow resume`, the phase will pick up at `Plan`. The plan doesn't specify if `state.stopped` should be cleared upon a resume, which may leave the phase forever marked as "stopped" even while actively running.

**4. Suggestions**
*   Ensure that `launch_stage` or the resume path clears `state.stopped` to `false`.
*   Serialize the execution of 20-03 and 20-04 or designate one to rebase over the other to avoid `main.rs` clap parser conflict.

**5. Risk Assessment**
**MEDIUM.** Touches the central transition logic and `doctor` validation. Ensuring the `stopped` flag clears correctly on subsequent runs is vital to avoid permanently silencing `check_dead_agent`.

---

## 20-04-PLAN.md (20d): Release-Cut Preflight

**1. Summary**
Creates a read-only `devflow release --check` preflight command to validate version pins, branch ancestry, publish ordering, and git tag signing viability.

**2. Strengths**
*   **Strict Read-Only Enforcement:** Explicitly refrains from implementing the full release executor, honoring the constraints (D-03).
*   **SSH Key Viability:** Excellent real-world verification of `gpg.format=ssh` and `ssh-add -l` exit codes (`0`, `1`, `2`), matching exactly how the actual release is signed.
*   **Tooling Reuse:** Reuses the exact shell invocation from `scripts/sync-main-to-develop.sh`.

**3. Concerns**
*   **LOW - Side Effects in Read-Only Mode:** The divergence check runs `git fetch origin main develop --quiet` before the ancestor check. While technically not modifying the project's tracked state, it does mutate the `.git/FETCH_HEAD` and local tracking branches. This is acceptable but is a slight deviation from strict "read-only".
*   **LOW - Offline Execution:** The `git fetch` may fail if the user is offline or lacking network credentials, which could crash the read-only check entirely.

**4. Suggestions**
*   Trap `git fetch` failures and degrade gracefully (e.g., emit a warning that the divergence check used stale local refs, rather than crashing the whole preflight).

**5. Risk Assessment**
**LOW.** A purely diagnostic command. Even if it fails or produces a false positive, it cannot corrupt the repository or pipeline state.

---

## 20-05-PLAN.md (20e): Manual Ship Override

**1. Summary**
Adds a `devflow ship --phase N [--force]` command that allows operators to bypass a dead monitor by directly consuming a written Ship gate response and calling `finish_workflow`.

**2. Strengths**
*   **Component Reuse:** Safely relies on `Gates::response_path` and `finish_workflow()` (`crates/devflow-cli/src/pipeline_gate.rs:130`) rather than attempting to duplicate the complex multi-step terminal hook logic.
*   **Scope Enforcement:** The `--force` flag is rigorously bounded to only apply when `state.stage == Stage::Ship`, preventing privilege escalation/scope creep (T-20-05).
*   **Route Symmetry:** Explicitly requires `LoopBack/Abort` actions to route through the exact same functions (`loop_back_to_code` and `abort`) as the live loop.

**3. Concerns**
*   **LOW - Process Spawning:** If the gate response is `LoopBack`, the `loop_back_to_code` path ultimately calls `launch_stage`, which forks the agent process. This means `devflow ship` will transition from a one-shot override command into a long-running monitor daemon process. While this matches the `devflow resume` architecture, it may be unexpected to operators who expect `devflow ship` to just exit.

**4. Suggestions**
*   If `ship` routes to `LoopBack` and spawns a new monitor, ensure the CLI output clearly indicates that a new agent daemon has been launched and detached to the background, so the operator isn't left wondering.

**5. Risk Assessment**
**MEDIUM.** Bypassing the normal asynchronous monitor poll requires precision, but the explicit reuse of the heavily-hardened `finish_workflow` logic significantly reduces the chance of state corruption.

---

## Hermes Review (deepseek-v4-pro)

> Run in place of OpenCode for this pass (operator request). Model resolved to `deepseek/deepseek-v4-pro`. Leading CLI stderr noise (toolset warning, model-name normalization) stripped below.

# Cross-AI Plan Review: Phase 20 — Release Correctness + Operator Control

**Review date:** 2026-07-22  
**Repo:** devflow (Rust workspace: `devflow-core` + `devflow-cli`)  
**Plans under review:** 20-01 through 20-05  
**Reviewer:** Hermes (source-grounded; every claim verified against actual files at HEAD)

---

## Overall Assessment

All five plans are source-grounded, well-scoped, and correctly identify the real code paths they must touch. The research backing is thorough — key claims about `version.rs:244`'s inline-table skip guard, `commands.rs:292-335`'s unconditional worktree removal, `pipeline_gate::transition`'s single-funnel property, `handle_ship_outcome`'s direct `finish_workflow` call bypassing `transition`, and `check_dead_agent`'s `Severity::Problem` false-positive risk all check out against source. Three of five plans have one or more medium-severity concerns worth addressing before execution.

**Overall risk:** MEDIUM — the plans are executable and structurally sound, but the issues identified below (20a inline-table edge handling, 20c dependency ordering, 20d offline resilience, 20e ack-race) could each cause a non-trivial rework if discovered during implementation rather than planning.

---

## PLAN 20-01 — 20a: VersionBump Self-Pin Rewrite

**Summary:** Extends `write_version` (version.rs:197) with an additive inline-table pass over `[workspace.dependencies]` entries carrying a local `path` key. Three tasks: RED-slice test, production fix, edge-coverage tests. Clean TDD flow. Correctly identifies that existing `starts_with('{')` guards in `find_version_in_contents` (:244) and `replace_version_in_contents` (:271) must stay untouched — the new pass is additive.

**Source-grounding verified:**
- `version.rs` confirmed at `crates/devflow-core/src/version.rs` (NOT `devflow-cli` — Pitfall 1 correctly caught). `write_version` at :197-206 rewrites exactly one `field_for()`-resolved field. `find_version_in_contents` at :244 skips inline tables via `value.starts_with('{')`. `field_for` at :59-72 returns `"workspace.package.version"` for workspace `Cargo.toml`. All confirmed.
- `workspace_version_pin.rs` (PR #17 guard) confirmed present at `crates/devflow-cli/tests/workspace_version_pin.rs` — correctly described as a RED-proven guard, not the fix.

**Strengths:**
- TDD pipeline is airtight: Task 1 RED-proves the defect before Task 2 fixes it, Task 3 locks edge coverage.
- Correctly identifies that `hooks::version_bump` calls `write_version` once and commits the same `Cargo.toml` — both fields land in one commit for free.
- Threat model correctly flags tampering risk (T-20-01a: rewriting a third-party dep's version) and mitigates with byte-identical assertion test.
- Edge-probe reconciliation explicitly maps 20a/adjacency and 20a/empty to Task 3 tests.

**Concerns:**
- **[MEDIUM] Inline-table parsing assumes single-line entries.** The plan's behavior section says each self-pin is on one line (`<crate> = { path = "...", version = "..." }`). Cargo.toml inline tables can span multiple lines. `version.rs`'s existing section-scoped iteration reads line-by-line; a multi-line inline table would break that assumption. The plan should either (a) explicitly note this assumption and verify the repo's own `Cargo.toml` entries are single-line, or (b) add a multi-line handling strategy.
  - **Evidence:** `crates/devflow-core/src/version.rs:226-316` — `find_version_in_contents` and `replace_version_in_contents` both iterate `contents.lines()`. The `starts_with('{')` guard at :244 is a line-level check. A multi-line inline table like:
    ```toml
    devflow-core = {
        path = "crates/devflow-core",
        version = "1.6.0"
    }
    ```
    would have `path` and `version` on different lines. The line containing `version = "1.6.0"` does NOT itself start with `{`, so the existing guard would NOT skip it — but the new pass needs to know it's inside an inline table that also has a `path` key.
  - **Recommendation:** Add a sub-section scanner that tracks when a line opens an inline table (`{`) and accumulates keys until `}`. Or verify the repo only uses single-line entries (a quick `rg` confirms `Cargo.toml` entries are single-line, but user workspaces may differ).
- **[LOW] Plan mentions "do not pull in a parser dependency" but rationale is incomplete.** The `toml` crate IS already in `Cargo.lock` (used by `config.rs` and `doc_check.rs`). The real reason to hand-roll is GAP-6: `toml`'s `to_string`/serialize does not preserve comments/quotes/ordering. RESEARCH.md notes this correctly, but the plan's text in `<action>` for Task 2 repeats "hand-rolled TOML handling rather than pulling in a parser dependency" — which is factually inaccurate since `toml` is already a dependency. This won't cause implementation errors but may confuse the executor about whether `toml_edit` is needed.
- **[LOW] Threat model says ASVS V4 non-applicable ("no privilege boundary in a file rewrite").** `write_version` rewrites `Cargo.toml` — a file `cargo publish` trusts for dependency resolution. If a future inline-table rewrite bug swapped a third-party dep's version, the blast radius is a broken publish (DoS) or, if the rewritten version exists on the registry, supply-chain substitution. This is more "Tampering" than "non-applicable."

**Risk:** LOW — the core approach is sound; the inline-table edge case is the only item that could force rework.

---

## PLAN 20-02 — 20b: Fixture Reliability + Cleanup Liveness Guard

**Summary:** Confirms instance 1 (worktree removal race) is product-reachable via `cleanup`'s missing liveness check, then fixes it with a hard-refuse guard using the existing `liveness()` predicate + bounded-backoff retry. Instance 2 (object-store corruption) is fixed fixture-side only with `core.fsyncObjectFiles`/`core.fsync` and a shrunk commit loop.

**Source-grounding verified:**
- `cleanup` at `commands.rs:292-335` confirmed: iterates worktrees, calls `worktree::remove` at :308 unconditionally — zero liveness check.
- `liveness()` at `commands.rs:371` confirmed as private pure function with `Liveness` enum at :341. `Healthy`/`BetweenStages`/`Stuck`/`Unknown` variants exist.
- `phase7_cli.rs:236` (`start_worktree_mode_ignores_main_checkout_divergence`) confirmed with 60-commit loop at :246. `:534` (`reference_and_cleanup_worktree_cli_flow`) confirmed.
- `worktree::remove` at `worktree.rs:105-112` confirmed as simple `git worktree remove [--force]` wrapper.
- `check_dead_agent` at `commands.rs:1247-1261` confirmed — but NOT relevant to this plan (correctly, only 20c touches it).

**Strengths:**
- D-06 is correctly encoded: hard-refuse (not warn), no new override flag (don't overload `--force`'s existing "remove reference worktree" meaning).
- Bounded-backoff retry for dead phases correctly rejects `git worktree prune` as primary recovery (Pitfall 3).
- Task 1 uses `std::process::id()` for a deterministically-alive pid — avoids timing-dependent flakiness in the refusal test.
- Scope discipline: instance 2 explicitly stays fixture-side per D-08; plan output explicitly records whether the deferred `999.N` was filed.

**Concerns:**
- **[MEDIUM] `liveness()` return values need explicit mapping for the refuse decision.** The plan says "Healthy/BetweenStages ⇒ refuse, Stuck/Unknown ⇒ proceed." But `Unknown` means `monitor_pid` is `None` — either the state predates 18b or no monitor was ever spawned. Is removing a worktree for an `Unknown`-liveness phase always safe? An `Unknown` phase could have a live agent with no recorded monitor (pre-18b state that survived an upgrade). Removing its worktree mid-run would cause the same race. The refuse decision should likely be "refuse on Healthy/BetweenStages, warn on Unknown, proceed on Stuck." The plan should make the `Unknown` case explicit.
  - **Evidence:** `commands.rs:371-380` — `liveness()` returns `Unknown` for `monitor_pid: None`. The doc comment at :368 says "matched `None` first so a state written by a pre-18b binary... can never be misclassified as `Stuck`" — this is a safety decision to avoid false `Stuck` diagnoses. But removing a worktree from an `Unknown` phase could be equally dangerous as removing from a `Healthy` one.
- **[LOW] `depends_on: []` is correct but 20-03 claims `depends_on: [20-02]`.** 20b and 20c share `commands.rs` and `phase7_cli.rs` as modified files. The review prompt's CONTEXT.md puts them in different waves (Wave 1 vs Wave 2) with no dependency. 20-03's declared dependency on 20-02 creates unnecessary sequencing where it could run in parallel. See 20-03 concern below.
- **[LOW] CI-on-branch sign-off requirement is correct but the plan's verification section buries it.** The "20b sign-off is CI-on-branch, not local-green" clause is correctly noted but could be missed by an executor scanning only the per-task verify blocks. Consider adding a top-level warning or making it an acceptance criterion on Task 3.

**Risk:** LOW — the core fix is well-understood; the `Unknown` liveness ambiguity is the only material gap.

---

## PLAN 20-03 — 20c: `--until <stage>` Pipeline Stop

**Summary:** Adds `--until` flag to `Start`, three new `#[serde(default)]` State fields (`stop_until`, `stopped`, `stop_reason`), interceptions in `pipeline_gate::transition`, and closes the `check_dead_agent` false-positive gap. Correctly identifies that `transition` is the single funnel and `loop_back_to_code` must stay untouched.

**Source-grounding verified:**
- `pipeline_gate::transition` at :51-80 confirmed: sets `state.stage = to;`, saves state, emits event, calls `launch_stage`. Single funnel for all meaningful advances.
- `handle_validate_outcome` at :213-272 confirmed: calls `transition(project_root, state, Stage::Ship)` from THREE branches (:231 ambiguous-gate, :262 mode-gated, :269 ungated-pass) — all funnel through `transition`.
- `handle_ship_outcome` at :275-286 confirmed: calls `finish_workflow` directly — no `transition` call. `--until ship` is indeed a semantic no-op.
- `loop_back_to_code` at :84-92 confirmed: calls `launch_stage` directly, bypassing `transition`. Correctly identified as off-limits for interception.
- `check_dead_agent` at :1247-1261 confirmed: returns `Severity::Problem` for any `is_agent_stage()` phase with dead agent pid. No `stopped` guard exists. Pitfall 2 is REAL.
- `PhaseFacts` at :1175-1193 confirmed: no `stopped` field.
- `State` struct confirmed: all optional fields use `#[serde(default)]` pattern (`monitor_pid` at :72 is the canonical example).

**Strengths:**
- The doctor gap (D-09) has its own task (Task 2) with an explicit acceptance criterion — not left as a footnote. This is the right treatment for a finding that would otherwise defeat the "clean stop point" goal.
- D-07 correctly encoded: `--until ship` rejected at the Start dispatch site before any stage runs.
- Serde backward-compat handled via explicit round-trip + absent-default tests (Task 3), mirroring existing `infra_failures` pattern.
- Help snapshot regeneration is an explicit task (Pitfall 5), not an afterthought.

**Concerns:**
- **[MEDIUM] `depends_on: [20-02]` is unjustified.** The review prompt's own CONTEXT.md says 20c is "independent" (Wave 2 alongside 20d, not gated on 20a-20b). 20-02 modifies `commands.rs:292-335` (cleanup) and `phase7_cli.rs` (tests). 20-03 modifies `commands.rs:1175+` (PhaseFacts + check_dead_agent) and `phase7_cli.rs` (new test). These touch DIFFERENT regions of the same files — no logical dependency exists. Declaring `depends_on: [20-02]` means 20c can't start until 20b is fully merged, unnecessarily serializing two units the phase context explicitly says can run in parallel. The file overlap is addressable via merge conflict resolution, not a hard dependency.
  - **Evidence:** Review prompt CONTEXT.md: "Wave 2 — 20c + 20d... 20c is independent." Plan header: `depends_on: [20-02]`. These contradict.
  - **Recommendation:** Change to `depends_on: []` for 20-03, mirroring 20-02. Merge conflicts on `commands.rs`/`phase7_cli.rs` can be resolved at integration.
- **[LOW] The `transition` stop path calls `workflow::save_state(state)?` then emits `workflow_finished`.** The plan's `<action>` for Task 1 says "emit `workflow_finished` with a stopped-at reason payload" — but the current `transition` function at :51-80 already emits a `"transition"` event. The stop path should REPLACE that emit (or emit an additional event). The plan should clarify: does the stop path emit BOTH a `transition` event AND a `workflow_finished` event, or only the `workflow_finished`? Currently the sequence in the `<action>` is: "set `state.stopped = true`... `workflow::save_state(state)?`, emit `workflow_finished`... return Ok WITHOUT calling `launch_stage`." This implies it replaces the normal transition emit, which is correct — but the transition emit happens at :70-78 BEFORE the stop check would fire. The plan's pseudocode places the stop check AFTER `state.stage = to;` (already set at :63) but the plan should specify that the emit at :70-78 must be bypassed when stopping.
  - **Recommendation:** Clarify the order of operations in `transition`: stage assignment → stop check → if stopping: save state, emit `workflow_finished`, return; else: save state, emit `transition`, `launch_stage`.
- **[LOW] The plan creates `PhaseFacts.stopped` but `collect_phase_facts` already does I/O.** Task 2 correctly adds `stopped: bool` to `PhaseFacts` and populates from `state.stopped`. This is a read from already-loaded state (zero new I/O), consistent with the `PhaseFacts` doc comment at :1172 ("read-only facts... collected by `collect_phase_facts` (all I/O); consumed with zero I/O by `reconcile_phase`"). Good.

**Risk:** LOW — the dependency ordering concern needs resolution but doesn't affect implementation correctness.

---

## PLAN 20-04 — 20d: `devflow release --check` Preflight

**Summary:** Read-only preflight command with four checks: self-pin, develop/main divergence, publish order, signing viability. Correctly limited to read-only per D-03. Blocks on 20-01 (20a's self-pin fix). Signing check is `gpg.format`-aware from the start.

**Source-grounding verified:**
- `scripts/sync-main-to-develop.sh:41` confirmed: uses `git merge-base --is-ancestor origin/main HEAD` after `git fetch origin main develop --quiet`. Plan correctly reuses this as a shell-out.
- `main.rs:220-227` (Doctor command shape) confirmed as read-only pattern to mirror.
- `commands.rs:975-1026` (doctor, Check struct) confirmed as read-only Check-list-then-report pattern.
- `workspace_version_pin.rs` confirmed as fixture style to mirror for `release_check.rs`.
- RESEARCH.md Pattern 4 (`gpg.format=ssh` live verification) confirmed — this repo uses `gpg.format=ssh`.

**Strengths:**
- Four independent checks, each testable in isolation. Task decomposition (self-pin first, divergence+publish order second, signing third) is logical.
- `gpg.format`-aware signing check is properly isolated with a pure `classify_ssh_add_status` function testable without a live agent.
- Security: signing output leaks no private key material (T-20-04, ASVS V6), with a regression test explicitly asserting absence of private key bytes and filesystem paths.
- The ancestor check correctly reuses the exact command `scripts/sync-main-to-develop.sh` already uses — no reimplementation (Don't Hand-Roll table).

**Concerns:**
- **[MEDIUM] The ancestor check shells out to `git fetch origin main develop --quiet`.** If the operator is offline (no network to `origin`), this hangs or fails. A release preflight should not require network access to run — the self-pin check and publish-order check are entirely local. The plan should either (a) make the fetch optional (`--offline` flag or graceful degradation) or (b) check `origin/main` against the locally-fetched ref without issuing a new fetch. The current design couples a read-only diagnostic to network availability, which is the UX problem 20d is supposed to reduce.
  - **Evidence:** `scripts/sync-main-to-develop.sh:39` — `git fetch origin main develop --quiet`. This is the correct invocation for the SYNC script (which is about TO sync), but `devflow release --check` is a READ-ONLY preflight. It should work offline.
  - **Recommendation:** Check `git merge-base --is-ancestor origin/main HEAD` WITHOUT a preceding fetch. If `origin/main` doesn't exist locally, the check should report "cannot verify — origin/main not fetched" (actionable, not a crash) rather than silently fetching.
- **[LOW] `depends_on: [20-01, 20-03]` — dependency on 20-03 is soft.** 20d and 20c both touch `main.rs` and `commands.rs`. If 20-03's `depends_on: [20-02]` is corrected to `[]`, 20-04's dependency on 20-03 becomes the only thing serializing Wave 2. Consider whether 20-04 truly needs 20-03 to land first, or whether they can merge independently (different regions of main.rs: Start variant vs. new Release variant; different regions of commands.rs: `start` signature vs. new `release_check`). If they're independent regions, change to `depends_on: [20-01]` only and let Wave 2 run fully parallel.
- **[LOW] The self-pin check's verify command runs `cargo test -p devflow --test release_check` but this test file doesn't exist yet (it's created by the plan).** Task 1's verify step needs the binary built first — `cargo test -p devflow --test release_check` will compile the whole crate including the new `Command::Release` variant. This is fine for Rust (test compilation includes the binary crate), but worth noting the implicit dependency: the test file lives in `crates/devflow-cli/tests/release_check.rs` and depends on `main.rs` having `Command::Release`.

**Risk:** LOW — the offline fetch concern is the only design-level issue; implementation risk is low.

---

## PLAN 20-05 — 20e: Manual Ship Override

**Summary:** New `devflow ship --phase N [--force]` command; second out-of-process consumer of the on-disk Ship gate-response record. Reuses `finish_workflow` verbatim (D-01). Scope-locked to `state.stage == Stage::Ship` (D-02). Sequenced last; depends on 20a (inherits VersionBump fix) and 20d (shares main.rs).

**Source-grounding verified:**
- `finish_workflow` at `pipeline_gate.rs:130-164` confirmed: fail-closed retry-gate-reopen loop, `hooks_after_ship`, `workflow::clear_state`, `workflow_finished` emit. Reusable verbatim.
- `Gates::respond` at `gates.rs:179-198` confirmed: writes response atomically; `NoOpenGate` at :186, `AlreadyResponded` at :190.
- `Gates::response_path` at `gates.rs:127` confirmed.
- `GateAction::from_response` at `gates.rs:69-79` confirmed: Advance/LoopBack/Abort routing.
- `abort` at `pipeline_gate.rs:246-259` confirmed: takes `&State`, clears state, emits `workflow_aborted`.
- `handle_ship_outcome` at `pipeline_outcomes.rs:275-286` confirmed: `GateAction::Advance` → `finish_workflow`; `LoopBack`/`Abort` route to shared helpers.
- `main.rs:238-259` (GateCmd::Approve shape) confirmed as pattern to mirror for the new Ship subcommand.

**Strengths:**
- D-01 correctly encoded: one on-disk record, second consumer, same `finish_workflow` effect. The `<assumption_delta_decision>` block correctly frames this as `add-alongside` with no identity-model change.
- D-02 scope enforcement is explicit and regression-tested: `--force` errors on every non-Ship stage with both `true` and `false` (EoP guard).
- Task 1's reversibility annotation is correct: coupling a second command to `finish_workflow` (a path Phase 16/17 hardened) is "costly" to reverse.
- LoopBack/Abort symmetry is explicitly called out in Task 3 — these reuse the existing `loop_back_to_code`/`abort` helpers rather than special-casing.

**Concerns:**
- **[MEDIUM] Ack-race: a response file that exists but has been ACKed.** The plan's 5-step contract checks `response_path.exists()` at step 3, then parses `GateResponse` and routes on `GateAction`. But `respond()` at `gates.rs:179-198` only prevents writing when `response_path` already exists — it does NOT check for an ack file. A live monitor that consumed the response and DIED before clearing state would have: (a) written an ack file, (b) partially executed `finish_workflow`, (c) left `state.stage == Stage::Ship` with the response file still on disk (or consumed). The plan handles "no response file" (fail-closed error) but not "response file exists but was already consumed by a monitor that then died mid-`finish_workflow`." In that case, re-running `finish_workflow` could re-execute terminal hooks (VersionBump, Merge) that already partially ran. This is a narrow window (monitor death between ack+first hook and state clearance) but worth acknowledging.
  - **Evidence:** `gates.rs` — `respond()` writes response atomically via `write_atomic`. `GateCmd::Approve` at `main.rs:239` calls `respond()`. `run_gate`'s poll loop (`pipeline_gate.rs:208`) reads response, acks, then calls `finish_workflow`. An ack file is `NN-{stage}.ack.json` per `gates.rs:132`. The plan doesn't check for an ack file before reading the response.
  - **Recommendation:** Add an ack check to `ship_override`'s step 3: if `response_path` exists but `ack_path` also exists, the response was already consumed by a (now-dead) monitor — the phase is in an inconsistent state (partially-executed `finish_workflow`). This warrants a distinct error message directing the operator to `devflow doctor` rather than silently re-running terminal hooks.
- **[LOW] `ship_override`'s `--force` semantics are `SkipVerifyAfterShip` not `SkipStage`.** The plan's D-02 encoding is correct (can't skip Validate), but the plan doesn't clarify what `--force` actually DOES — it says "skip re-verifying the Ship gate's own preconditions." When `--force` is true, what preconditions are skipped? This should be explicit in the plan body (not just RESEARCH.md), since the executor needs to implement the distinction between "Ship stage check" (always enforced) and "Ship gate re-verification" (skipped with `--force`).
- **[LOW] `depends_on: [20-01, 20-04]` — 20-04 dependency.** 20e depends on 20a (inherits VersionBump fix when `finish_workflow` runs `hooks_after_ship` → `version_bump` → `write_version`). 20e's dependency on 20-04 is for main.rs co-modification order (both add new Command variants). If 20-04's dependency on 20-03 is relaxed (see 20-04 concern), 20-05's dependency on 20-04 should be checked — 20e and 20d add DIFFERENT Command variants (Ship vs Release), which are additive and non-conflicting in the clap enum. This dependency could be relaxed to `depends_on: [20-01]` only.

**Risk:** LOW — the ack-race is a narrow edge case; the core mechanism is correct.

---

## Cross-Cutting Concerns

### Dependency Ordering

The declared dependencies create unnecessary serialization:

| Plan | Declared | Justified | Recommendation |
|------|----------|-----------|----------------|
| 20-01 | `[]` | ✓ Correct — Wave 1, no deps | Keep |
| 20-02 | `[]` | ✓ Correct — Wave 1, no deps | Keep |
| 20-03 | `[20-02]` | ✗ Should be `[]` — 20c is independent per CONTEXT.md | Change to `[]` |
| 20-04 | `[20-01, 20-03]` | 20-01 ✓ (self-pin check needs 20a). 20-03 ~ (soft, share main.rs) | Consider `[20-01]` only |
| 20-05 | `[20-01, 20-04]` | 20-01 ✓ (inherits VersionBump). 20-04 ~ (soft, share main.rs) | Consider `[20-01]` only |

The review prompt's CONTEXT.md correctly identifies three waves with genuine hard deps only between 20d→20a and 20e→20a. The plans add extra soft deps through file-overlap declarations that the workflow executor may not be able to distinguish from hard deps — this could serialize what should be Wave 2's parallel 20c+20d.

### Phase Goal Achievement

All five plans together close the four defects and add the two operator controls the phase requires. Each plan's `<success_criteria>` maps directly to the phase's stated goals. The deferred items (release-cut executor, `devflow parallel` object-store concern) are explicitly called out as out-of-scope in each plan's output instructions — no scope creep detected.

### Security Posture

- 20e's EoP guard (T-20-05) is the strongest security measure: `--force` scoped to Ship-only with regression test for every non-Ship stage.
- 20d's signing check properly isolates key material (boolean + fingerprint only, no private key bytes or paths).
- 20b's liveness guard closes a real tampering vector (force-removing another operator's in-progress work in a shared checkout).
- 20a's threat model slightly undersells the blast radius of a TOML rewrite bug (see concern above).

---

## Cursor Review

**Dropped — account usage limit.** `cursor-agent` returned `ActionRequiredError: You've hit your usage limit`. No review produced (same outcome as Phase 19). Not a plan defect.

---
phase: 19
reviewers: [codex, cursor, opencode, antigravity]
reviewed_at: "2026-07-21T23:46:13Z"
plans_reviewed:
  - 19-01-PLAN.md
  - 19-02-PLAN.md
  - 19-03-PLAN.md
  - 19-04-PLAN.md
  - 19-05-PLAN.md
  - 19-06-PLAN.md
  - 19-07-PLAN.md
  - 19-08-PLAN.md
  - 19-09-PLAN.md
  - 19-10-PLAN.md
  - 19-11-PLAN.md
---

# Cross-AI Plan Review — Phase 19

> Reviewer panel for this run: Codex, Cursor, OpenCode (`deepseek/deepseek-v4-pro`), Antigravity
> (via `agycli`/`antigravity-cli`, not the broken `agy`→GUI wrapper — see orchestrator note below).
> Claude and Qwen are excluded from `review.default_reviewers` by operator decision (self-review and
> no working credentials, respectively) and were not invoked.

## Codex Review

## Summary

Overall, the plans are unusually thorough and mostly well aligned with the actual code. The core premises check out: `.devflow/` is created independently in seven production sites, `workflow_started_payload` currently persists a full executable path, `commit_path` still forces empty commits, `main.rs` is the large monolith the split targets, and the preflight/pipeline coupling is real. The main weaknesses are not in the implementation ideas, but in orchestration and verification: several `depends_on` fields do not encode the sequencing the plan text says is load-bearing, multiple later plans rely on `/tmp/19-before-names.txt` surviving across waves, and Plan 11 claims CI verifies `cargo test --workspace` even though `.github/workflows/ci.yml` currently runs plain `cargo test`.

## Strengths

- Plan 01 correctly targets all seven production `.devflow/` constructors verified in source: `workflow.rs:95`, `gates.rs:325`, `monitor.rs:98`, `agent_result.rs:964`, `events.rs:58`, `ship.rs:85`, and `lock.rs:82`.
- Plan 02 is tightly scoped and matches the source defect: `workflow_started_payload` emits `current_exe().display().to_string()` at `crates/devflow-cli/src/main.rs:902-904`, and the existing test only checks string-or-null at `main.rs:6879`.
- Plan 03 is a strong TDD plan. `commit_path` uses `--allow-empty` at `crates/devflow-core/src/git.rs:333-340`, while the "nothing to commit" arm at `git.rs:343` is effectively dead. The existing scoped-path test at `git.rs:657-700` is the right fixture to extend.
- The split plans correctly identify real coupling. `run_preflight` calls `launch_stage_inner` at `main.rs:861`, and `launch_stage` calls `run_preflight` at `main.rs:1389`; preserving direct calls is the right pure-move choice.
- Plan 06's `ENV_MUTEX` focus is justified. There is one CLI mutex at `main.rs:4034`, with many lock sites, including PATH and timeout env vars, while separate core mutexes exist in `gates.rs` and `config.rs`.
- Plan 09's decision to extract `config_parse.rs` is defensible: env readers/parsers are currently at `main.rs:27-55`, and their tests belong away from a thin crate root.
- Plan 10 correctly updates stale docs after the split, and its "verify every cited path/test before writing" rule directly addresses the rot problem.
- Plan 11's three-part equivalence proof is the right shape for a pure-move refactor: function diff, test name-set identity, and per-target pass-count identity.

## Concerns

- **HIGH: Plan 06 dependency metadata violates D-20.** Plan 06 depends only on `19-02`, but its own must-have says it must start strictly after 19a and 19b. Source confirms 19a spans Plan 01 and Plan 02, and 19b is Plan 03. `depends_on` should include `19-01`, `19-02`, and `19-03`, otherwise the split foundation can start before the `.devflow/` constructor fix or `commit_path` fix lands.

- **HIGH: Plan 11 dependency metadata is incomplete for the phase gate.** Plan 11 depends on `19-01`, `19-03`, and `19-09`, but it verifies all requirements, including 19a-WR02 from Plan 02 and 19g dogfood from Plan 05. It should depend on at least `19-02`, `19-05`, and likely `19-10` if the final requirement roll-call includes documentation reconciliation.

- **HIGH: Plan 11 overstates current CI coverage.** `.github/workflows/ci.yml:20` runs `cargo test`, not `cargo test --workspace`; clippy is workspace-scoped at `ci.yml:31`, and fmt at `ci.yml:42`. If Plan 11 requires CI evidence for `cargo test --workspace`, either the workflow must be updated in an earlier plan or Plan 11 must record this as a CI coverage gap instead of treating it as already satisfied.

- **MEDIUM: Verification commands rely on `/tmp/19-before-names.txt` across waves.** Plan 06 says the baseline lists are written into `19-SPLIT-BASELINE.md`, but Plans 07, 08, 09, and 11 verify by diffing against `/tmp/19-before-names.txt`. `/tmp` is not durable across sessions, reboots, or different executors. The commands should reconstruct the expected list from the committed baseline artifact.

- **MEDIUM: Plan 01's integration coverage may be expensive and brittle around `monitor::spawn_monitor_no_advance`.** The function is public at `monitor.rs:58-64` and writes the capture directory at `monitor.rs:91-99`, so the target is valid. But it spawns a detached shell process at `monitor.rs:162-174`; the test plan should explicitly wait for or clean up the monitor process to avoid leaking background work.

- **MEDIUM: Plan 05 is not realistically executable through this API as written.** It requires running `/gsd-code-review` and a human adjudicating LLM review output. That may be valid in the GSD environment, but as a plan artifact it should name the exact invocation surface and expected reviewer log location. Otherwise the "verbatim outputs" requirement is under-specified.

- **LOW: Plan 01's `ensure_devflow_dir` name/signature could be confusing.** The plan calls `ensure_devflow_dir(dir: &Path)` and relies on scanning for a `.devflow` ancestor. That works, but the name sounds like it takes a project root. The doc comment must be very explicit because `workflow::devflow_dir(project_root)` at `workflow.rs:33-35` already has the project-root contract.

- **LOW: Plan 04's `.claude/skills` claim is plausible but not fully source-proven in-repo.** This repo currently has `.claude/settings.local.json` and `.claude/scheduled_tasks.lock`, but no `.claude/skills`. The plan relies on external GSD project-skill discovery files, so the dogfood Plan 05 is important and should remain blocking.

## Suggestions

- Update dependencies:
  - Plan 06: `depends_on: ["19-01", "19-02", "19-03"]`
  - Plan 11: `depends_on: ["19-01", "19-02", "19-03", "19-05", "19-09", "19-10"]` if it is the true phase gate.
- Replace all `/tmp/19-before-names.txt` verification references with a command that extracts the expected list from `19-SPLIT-BASELINE.md`, or commit machine-readable baseline files under the phase directory.
- Fix Plan 11's CI wording: either add an earlier CI plan to change `.github/workflows/ci.yml:20` to `cargo test --workspace`, or require Plan 11 to record that CI runs `cargo test` and local verification runs `cargo test --workspace`.
- In Plan 01 Task 3, add explicit cleanup/waiting for `spawn_monitor_no_advance` and assert no lingering monitor process remains.
- In Plan 05, specify the concrete command or tool surface used to run `/gsd-code-review`, and where the raw review output is captured.
- In Plan 06 and later split plans, make the name-set comparison collision-aware in the actual verify commands, not only in prose.

## Risk Assessment

**Overall risk: MEDIUM.** The implementation design is strong and source-aligned, especially for 19a, 19b, and the pure-move split. The remaining risk is execution integrity: if dependency metadata, baseline durability, or CI evidence are wrong, the phase can appear verified without actually satisfying its own load-bearing sequencing and D-11 requirements. Fixing those plan mechanics should bring the risk down substantially.

---

## Cursor Review

**Failed — Cursor account usage limit, not a plan defect.**

```
ActionRequiredError: You've hit your usage limit Get Cursor Pro for more Agent usage, unlimited Tab, and more.
```

Cursor was authenticated and detected correctly at setup time; this is an account-quota exhaustion at the moment of this specific run, not a CLI/auth misconfiguration. Re-run `/gsd-review --phase 19 --cursor` after the quota resets (or upgrade) to get this leg's coverage; no action needed on the plans themselves.

---

## OpenCode Review

**Model:** `deepseek/deepseek-v4-pro` (persisted default — no `review.models.opencode` pinned)

# Cross-AI Plan Review: Phase 19 — Release Integrity + `main.rs` Decomposition

**Review date:** 2026-07-21
**Source verification:** All claims cross-checked against live HEAD of `develop` at `crates/` and `.planning/`.
**Scope:** 11 plans (19-01 through 19-11) across 6 waves.

---

## 1. Summary

These plans are grounded in meticulous source-code verification — every `create_dir_all` site, `ENV_MUTEX` definition, call site, and test name has been independently confirmed. The phase correctly identifies that `lock::ensure_devflow_dir` does not exist (Pitfall 3) and pivots to a genuinely new constructor, which is the right call. The split sequencing (19a/19b pre-split, wave-gated pure moves) is sound. The D-12 partial-completion escape hatch is both honest and load-bearing. The primary risk — `ENV_MUTEX` serialization across module boundaries — is aggressively tested with three consecutive runs per plan and multi-run CI gating. **However**, one claim underpinning the 19g workstream cannot be verified from source, and the equivalence-proof procedure has an implicit assumption about line ranges that deserves explicit acknowledgment.

---

## 2. Strengths

- **Source-verified research.** Every claim about line numbers, function names, `create_dir_all` sites, `ENV_MUTEX` definitions, and call-graph edges has been independently confirmed:
  - 7 `create_dir_all` sites: `workflow.rs:95`, `gates.rs:325`, `monitor.rs:98`, `agent_result.rs:964`, `events.rs:58`, `ship.rs:85`, `lock.rs:82` — all present exactly as claimed
  - `devflow_dir()` duplicated at `workflow.rs:33` (public) and `agent_result.rs:872` (private) — confirmed
  - `exe_path` writes full absolute path at `main.rs:902` with `.display().to_string()` — PII leak confirmed
  - Both `commit_all` (`git.rs:312`) and `commit_path` (`git.rs:336`) use `--allow-empty` — confirmed, with `nothing to commit` match arms present at `git.rs:316` and `git.rs:343`
  - Three independent `ENV_MUTEX` statics at `main.rs:4034`, `gates.rs:348`, `config.rs:174` — confirmed
  - 106 `#[test]` functions in `main.rs` — confirmed
  - `run_agent_blocking` (`main.rs:2417`) calls `archive_phase_files` but never `save_state` — confirmed via `rg -n 'save_state'` through the function body

- **Defensive planning.** D-12 ("a partial split is acceptable") is stated upfront as a success condition, not buried as a contingency. This is exactly the right posture for a refactor whose principal risk is a known race class with a documented history (19i, GAP-2, 999.4).

- **Pipeline sub-split honesty.** 19-08 explicitly documents in `pipeline_gate.rs`'s module doc comment that the three files are mutually cyclic and that "a future change to pipeline logic is likely to touch two or three of these files together." Requiring this text as an acceptance criterion (`rg -c 'two or three of these files'`) is excellent — it prevents a future planner from over-claiming wave parallelism.

- **TDD discipline in 19-03.** The plan demands RED-first evidence (captured failure output) before any production change, and the `type: tdd` header enforces this at the workflow level. The separate `commit_path_with_no_changes_returns_ok_without_committing` test splits "no commit" and "no error" into distinct assertions so a future change cannot satisfy one by breaking the other.

- **Equivalence proof is three-dimensional.** Name-set identity, pass-count identity, and per-function symbol diff — each catches a different failure mode (lost test, gained test, behavioral drift). The `uniq -d` collision check in the baseline (19-06 Task 1) is a smart catch: bare trailing-name comparison can mask a loss without it.

---

## 3. Concerns

### HIGH — 19g wiring mechanism cannot be independently verified

> **[Orchestrator note — verified false]** This claim does not hold. `$HOME/.claude/gsd-core/references/project-skills-discovery.md` exists on disk (confirmed via `ls` from the orchestrating session). OpenCode's sandbox most likely could not read outside the repo working directory (`$HOME/.claude/...` is external to `$_AGY_WS`-equivalent scoping for `opencode run`), and it reported "file not found" instead of "outside my read access." Treat this specific HIGH as **refuted** — do not action it. The underlying question it was trying to ask (does 19g's contract actually wire into the review mechanism the plan claims) is legitimate and is exactly what Plan 05's dogfood checkpoint verifies; this false negative does not change that Plan 05 remains the correct verification gate.

**Evidence:** Both `19-RESEARCH.md:122-123` and `19-04-SKILL.md` reference `$HOME/.claude/gsd-core/references/project-skills-discovery.md` as the authoritative source for the claim that GSD's reviewer agent auto-discovers `.claude/skills/`. **This file does not exist at the claimed path** (`cat` returns "FILE NOT FOUND").

**Why it matters:** The entirety of 19g's enforcement path depends on this claim. If the reviewer agent does not actually auto-discover project skills, the contract files created by 19-04 are dead prose — they exist on disk but are never applied by the review gate. This downgrades 19g from "enforced contract" to "aspirational documentation."

**Mitigation:** The 19-05 dogfood checkpoint partially addresses this — if Diff A is flagged citing the contract by name, that is strong evidence the wiring works. But if the checkpoint fails or produces only generic feedback, there is no fallback mechanism documented anywhere in the phase. The research document rates the wiring mechanism as "MEDIUM confidence" and that rating is justified.

**Recommendation:** Either verify the discovery mechanism exists (at an alternative path, or by examining the actual reviewer agent definition at `~/.claude/agents/gsd-code-reviewer.md`) and update the reference, or explicitly downgrade 19g's enforcement claim in the plan output.

### HIGH — Anchor-creator vs. constructor-based strategy tension in 19-01

**Evidence:** 19-01 Task 1 designs `ensure_devflow_dir` with a `create_new(true)` approach for the `.gitignore`. The doc comment states: "skipped when the file already exists and a lost race against a concurrent creator surfaces as `ErrorKind::AlreadyExists`, which this function maps to `Ok(())`."

**Mechanism:** Using `create_new(true)` means if the `.gitignore` is deleted after creation (by a user, a cleanup script, or a git operation), `ensure_devflow_dir` will NEVER recreate it on subsequent calls — because the function maps `AlreadyExists` → `Ok(())`, and there's no check for "file doesn't exist" after the `create_dir_all` succeeds.

**Why it matters:** The original 19a threat is that a routine `git add . && git commit` sweeps `.devflow/` into history. If a user deletes `.devflow/.gitignore` (perhaps because `*` is too aggressive and blocks legitimate files they want committed), the gap reopens silently. The function's idempotence contract (Task 1 behavior: "Calling it a second time on the same directory succeeds") is satisfied — but it's satisfied by doing nothing, not by confirming the protection still exists.

**Severity assessment:** This is the correct design tradeoff for v1.6.0 because:
- Re-creating a deleted `.gitignore` would violate the "MUST NOT overwrite an existing `.devflow/.gitignore` whose content differs" prohibition
- The function cannot distinguish "user deleted the gitignore" from "gitignore was never created"
- The behavior is explicitly documented

But it IS a gap that should be acknowledged, not absorbed. The plan's doc comment mentions preserving existing files but doesn't discuss the deleted-file re-creation gap.

### MEDIUM — Baseline-relative line ranges are implicitly relied on but never explicitly validated

**Evidence:** 19-06 Task 1 captures a baseline SHA with line ranges for every cluster. 19-07 through 19-09 mandate "re-derive every line range at the current HEAD before extracting." This is correct in principle. However, 19-02 modifies `main.rs` before the baseline is captured — adding lines to `workflow_started_payload` (the test's additional assertions). This means the baseline SHA's line ranges for clusters below `workflow_started_payload` are shifted by how many lines 19-02 adds.

**Why it matters:** 19-07's extraction procedure says to use `sed -n 'START,ENDp'` with re-derived ranges. The procedure is sound. But if an executor accidentally reuses a stale range from `19-SPLIT-BASELINE.md` instead of re-deriving, the extraction would silently splice wrong lines. The plans explicitly warn against this, so the mitigation is present, but the warning only appears in the `<extraction_procedure>` block — not in the task-level acceptance criteria that an executor would read first.

### MEDIUM — 19-01's `ensure_devflow_dir` return type choice has downstream typing implications

**Evidence:** The plan specifies returning `std::io::Result<()>` rather than `Result<(), WorkflowError>` for compatibility with six different error enums. All six error enums do carry `Io(#[from] std::io::Error)` variants — verified at `workflow.rs:17-18`, `gates.rs` (GateError), etc. The `?` operator will convert correctly.

**However:** The plan also says `events.rs` uses a "fail-soft let-chain" that logs with `warn!` and early-returns on error. This means `events.rs:58` currently has `let Err(err) = std::fs::create_dir_all(parent)` — a `let ... else` or `if let` chain that does NOT use `?`. The plan says to "preserve the existing let-chain shape exactly, only changing the called function." This means `events.rs` will call `ensure_devflow_dir` but WON'T use `?` on the result — it'll match against `Err(...)` as before. This works, but a future author looking at `events.rs` will see `ensure_devflow_dir` being called differently from every other site, which is a subtle inconsistency.

### LOW — 19-05 scratch diffs don't exercise distinct rejection patterns

**Evidence:** The dogfood checkpoint describes Diff A as "asserts a constant against itself (pattern 1 and pattern 3)." Patterns 1 ("asserts constants") and 3 ("compares a function call with itself") are arguably the same class of untestable test looked at from two angles.

**Why it matters:** A single violation shape only proves the contract catches ONE pattern. The QA pass (`TEST-SUITE-QA-REVIEW.md`) identified at least three distinct shapes (the `ReviewerSetTestAdapter` case being pattern 2 — "reproduces the production algorithm"). Testing only the weakest/most obvious shape leaves three patterns unverified.

### LOW — Ambiguity in 19-11's CI verdict threshold

**Evidence:** 19-11 Task 1 says "Run CI at least three times against the same commit" and "Record every run's result, including any red." The acceptance criteria say "All three green, or every red run's failing test named."

**Why it matters:** The criteria don't state whether 2/3 green + 1/3 red with a named failure constitutes "passed" or "D-12 finding." Given the phase's own history (19i hit 2/2 in CI), an intermittent failure should arguably be a finding regardless of count. The plan's intent ("a single green is weaker evidence than three") is clear, but the adjudication threshold for mixed results is not.

---

## 4. Suggestions

1. **Verify the 19g discovery mechanism before executing 19-04.** *(Orchestrator: already verified — the file exists. This suggestion is superseded; proceed with 19-04/19-05 as planned.)*
2. **Acknowledge the deleted-`.gitignore` gap in `ensure_devflow_dir`'s doc comment.** Add one sentence: "If `.devflow/.gitignore` is deleted after creation, subsequent calls to this function will not recreate it — the protection is established once per directory lifetime." The acceptance criteria already preserve this behavior by design; documenting it prevents a future bug report.
3. **Add the re-derivation warning to each task-level acceptance criterion in 19-07/19-08/19-09**, not only in the `<extraction_procedure>` block. Example: "Before extracting, the current production range was re-derived via `rg -n '^(pub\(crate\) )?fn ' crates/devflow-cli/src/main.rs` and is recorded here as X–Y."
4. **For 19-05, test at least two distinct rejection patterns.** Keep the constant-self-comparison diff but add a second diff that reproduces the production algorithm (pattern 2, à la `ReviewerSetTestAdapter`). This yields stronger evidence that the contract's recognition criteria are comprehensive.
5. **For 19-11, clarify the mixed-CI-result adjudication.** Change the acceptance criterion to: "Any CI run whose result differs from any other CI run against the same commit is a D-12 finding recorded with the failing test name, env var, and lock site — regardless of majority."
6. **Document the `config_parse` resolution in the 19-09 SUMMARY with a comparison.** The discretion call (separate file vs. folding into `main.rs`) is justified by the env-mutation argument, but a brief note comparing the two options against measured line counts would strengthen the rationale. Currently it's stated as "they get a module, for one concrete reason" — recording the rejected alternative's cost (a 35-line inlined section in `main.rs` vs. a 60-line file) makes the decision reviewable.
7. **Add a `cargo test --workspace -- --list` to 19-02's acceptance criteria**, even though it adds no new tests. This establishes that the `exe_path` test is still present (name unchanged) and isn't accidentally lost during the edit — a cheap sanity check consistent with the phase's obsessive verification posture.

---

## 5. Risk Assessment

**Overall: MEDIUM**

**Justification:**

The phase has four high-severity threats in its threat model (T-19-05, T-19-20, T-19-24, T-19-25, T-19-32, T-19-33, T-19-34, T-19-35, T-19-36) and all are mitigated with concrete, verifiable controls — source assertions, stability requirements, diff-scope constraints, and independent count reconciliation. This is strong.

The single HIGH concern — the unverifiable 19g wiring claim — affects only one of four independent workstreams (19a, 19b, 19c–19f, 19g) and the 19-05 dogfood checkpoint provides a path to verification even without the reference file existing. The workstream is also explicitly parallel (D-21), so it cannot block or corrupt the split.

The MEDIUM concerns — the `.gitignore` deletion gap and baseline-relative line range risk — are both well-mitigated in-process. The former is explicitly documented as intended behavior; the latter has explicit warnings.

The phase's D-12 escape hatch is not merely decorative — it is wired into every wave 3–5 plan as a mandatory branch in the `ENV_MUTEX` stability checks, and every plan boundary leaves a green, committable tree. This means even a worst-case serialization finding produces an honest partial outcome rather than a silently broken refactor.

---

## Antigravity Review

# Phase 19 Plan Review: Release Integrity + `main.rs` Decomposition

## 1. Summary

The 11 implementation plans across 6 waves for Phase 19 provide an exceptionally thorough, risk-mitigated strategy for resolving release-integrity defects (999.10 PII leak & 999.11 empty commits) and decomposing the 8,467-line `crates/devflow-cli/src/main.rs` into flat sibling modules without behavioral regression. The sequencing is load-bearing: landing the low-blast fixes (19a, 19b, 19g) in Wave 1 before the module split prevents diff pollution across newly created files. Hoisting process-global test environment locks (`ENV_MUTEX`) into a shared `crates/devflow-cli/src/test_support.rs` in Wave 2 directly addresses the primary concurrency risk (19i / GAP-2) while preserving the equivalence proof across all 106 CLI tests.

## 2. Strengths

- **Surgical Pre-Split Fixing (Wave 1):** Plans 19-01, 19-02, and 19-03 resolve 19a and 19b directly against existing single-file source locations before the split, keeping diffs readable and auditable.
- **Accurate Grounding in Repository Reality:** Plan 19-01 correctly rejected earlier assumptions about a non-existent `lock::ensure_devflow_dir` and identified all 7 production directory creation sites:
  - `crates/devflow-core/src/workflow.rs:95` (`write_state_atomic`)
  - `crates/devflow-core/src/gates.rs:325` (`write_atomic`)
  - `crates/devflow-core/src/monitor.rs:98` (`spawn_monitor`)
  - `crates/devflow-core/src/agent_result.rs:964` (`archive_phase_files`)
  - `crates/devflow-core/src/events.rs:58` (`emit`)
  - `crates/devflow-core/src/ship.rs:85` (`write_cron_instructions`)
  - `crates/devflow-core/src/lock.rs:82` (`acquire_path`)
- **Restoration of Dead Code Contract (19b):** Plan 19-03 removes `--allow-empty` from `commit_path` at `crates/devflow-core/src/git.rs:335`, reviving the previously unreachable `"nothing to commit"` error handling match arm at `crates/devflow-core/src/git.rs:343` as the clean `Ok(())` no-op path.
- **Uncompromised Test Concurrency Architecture:** Plan 19-06 hoists `ENV_MUTEX` into `test_support.rs` within the `devflow-cli` binary crate, preserving the single `static` process lock across all newly created modules without introducing unsafe per-module mutexes or sweeping behavioral refactors to process environment handling.
- **Pragmatic Cycle Handling:** Plans 19-07 and 19-08 acknowledge Rust's inherent support for cyclic module imports (e.g., `preflight.rs` ↔ `pipeline_launch.rs` and `pipeline_launch` → `pipeline_outcomes` → `pipeline_gate` → `pipeline_launch`). They avoid over-engineering artificial trait or callback indirections during a zero-behavior pure move.
- **Strict Equivalence Proof & Baseline Pinning:** Plan 19-06 captures a committed baseline (`.planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE.md`), enabling mechanical verification of test name set identity, per-target test pass counts, and per-function diffs across all stages.

## 3. Concerns

- **[MEDIUM] `ensure_devflow_dir` Path Traversal Ancestor Search Edge Case:**
  In Plan 19-01, `ensure_devflow_dir` scans `dir` and its ancestors for a component named `.devflow` to write `.gitignore`. If a caller passes a relative path like `Path::new(".devflow/captures")` or a custom path where `.devflow` is the leaf directory (`dir.ancestors()` returns `.devflow/captures`, `.devflow`, `""`), shallowest-first ancestor searching must properly handle relative vs. canonicalized absolute paths to guarantee `.gitignore` is created at `.devflow/.gitignore` rather than failing or placing it in an unexpected root.
- **[MEDIUM] Intermediate Visibility Churn in Pipeline Sub-Split (Wave 4):**
  In Plan 19-08, extracting `pipeline_launch.rs` (Task 1) before `pipeline_outcomes.rs` (Task 2) and `pipeline_gate.rs` (Task 3) requires marking functions in `main.rs` as `pub(crate)` and adding temporary `use crate::...` imports in `pipeline_launch.rs`, which are then immediately repointed in Tasks 2 and 3. While necessary to keep each commit compilation-green, it increases diff noise across Wave 4 commits.
- **[LOW] Unredacted Absolute Worktree Path in `workflow_started_payload`:**
  Plan 19-02 redacts `exe_path` to its binary filename in `crates/devflow-cli/src/main.rs:902`, but leaves `"worktree": state.worktree_path.as_ref().map(|p| p.display().to_string())` emitting an absolute path in `events.jsonl`. Threat T-19-09 correctly accepts this as out-of-scope for D-15, but it remains a potential minor PII leak if a user's home directory path is embedded in `worktree_path`.
- **[LOW] Git Command Output Localization Dependency:**
  In Plan 19-03, `commit_path` relies on matching `msg.contains("nothing to commit")` in `crates/devflow-core/src/git.rs:343`. If git is executed in an environment with non-English locale settings (e.g. `LC_ALL` set to a localized environment), git error strings will differ. Note that existing code in `git.rs` already makes this assumption, so this maintains existing behavior.

## 4. Suggestions

1. **Path Normalization in `ensure_devflow_dir`:** In `crates/devflow-core/src/workflow.rs`, implement `ensure_devflow_dir` by converting `dir` to an absolute/canonicalized path or checking components explicitly (`path.components().any(|c| c.as_os_str() == ".devflow")`) to reliably locate the `.devflow` ancestor regardless of relative path inputs.
2. **Set `LC_ALL=C` or `LANG=C` in `git_raw` Invocation:** To harden `commit_path` (Plan 19-03) and `commit_all` against environment-dependent git translations during string matching (`"nothing to commit"`), ensure `Git::git_raw` sets `LC_ALL=C` on process execution.
3. **Atomic Pipeline Cluster Refactor if Wave 4 Contention Occurs:** If intermediate import repointing in Plan 19-08 causes friction during execution, consider staging the file creation of `pipeline_launch.rs`, `pipeline_outcomes.rs`, and `pipeline_gate.rs` within a single unified wave task so inter-module `pub(crate)` dependencies are established in one clean pass.

## 5. Risk Assessment

**Overall Risk Level: LOW**

**Justification:**
The overall risk is **LOW**. The plan features exceptional defensive engineering:
- The phase enforces strict TDD and TDA (test-driven verification) rules.
- Baseline equivalence tracking prevents silent test loss or function logic mutation.
- Hard safety rules (e.g., prohibition of multi-commit splits that trigger binary crate `dead_code` lints) ensure build stability.
- Mandatory 3x CI execution on branch (Plan 19-11) verifies thread-safety and environment lock behavior on shared runners prior to merging.

---

## Consensus Summary

Three of four reviewers produced full, source-grounded reviews (Codex, OpenCode, Antigravity — all independently re-verified the core factual claims in `19-CONTEXT.md`/`19-RESEARCH.md` against live source and found them accurate). Cursor failed on an account usage limit before producing any output — its absence is a coverage gap, not a signal about plan quality.

**Orchestrator calibration on severity:** Codex rated the `depends_on` metadata gaps (Plan 06, Plan 11) as HIGH on the grounds that they violate D-20's sequencing requirement. Checked against `gsd-core/workflows/execute-phase.md`: wave execution is gated purely by **wave number** ("Wave safety check" — incomplete lower-wave plans block a wave filter regardless of `depends_on` content), not by the `depends_on` field. `depends_on` is descriptive metadata (consumed by the plan-checker's graph-validity pass and by `roadmap.annotate-dependencies`'s prose generation), not an execution gate. **This means the actual sequencing D-20 requires is already enforced** — 19-06 (wave 2) cannot execute before 19-01/19-02/19-03 (wave 1) complete, regardless of what its `depends_on` array says. The metadata is still wrong and should be fixed (it's exactly the kind of claim/reality drift 19g exists to catch), but it is a **documentation-accuracy defect, not a functional safety hole**. Recalibrated: MEDIUM, not HIGH.

### Agreed Strengths (2+ reviewers)

- **The D-14 correction is real and correctly implemented.** All three full reviews independently re-derived and confirmed the same 7 `create_dir_all` sites, the same `devflow_dir()` duplication (`workflow.rs:33` public / `agent_result.rs:872` private), and that `lock::ensure_devflow_dir` genuinely does not exist — converging on treating Plan 01's pivot as correct engineering, not just plan-writing.
- **19b's fix is source-accurate.** Codex and Antigravity both independently confirmed `--allow-empty` at `git.rs:333-343`/`:335` and the dead `"nothing to commit"` arm it revives.
- **`ENV_MUTEX` hoist is the right shape.** Codex, OpenCode, and Antigravity all confirmed the single-mutex design (`main.rs:4034`) and endorsed hoisting into a shared `test_support.rs` as correctly averting the per-module-mutex hazard the phase was scoped around.
- **The D-12 partial-completion escape hatch is genuinely wired in, not decorative.** OpenCode and Antigravity both call this out explicitly as load-bearing rather than a caveat.

### Agreed Concerns (2+ reviewers)

- **[MEDIUM] Baseline artifact durability/staleness, from two angles.** Codex flags that Plans 07/08/09/11 verify against `/tmp/19-before-names.txt`, which is not durable across sessions/executors — the committed `19-SPLIT-BASELINE.md` exists in name but the actual verify commands don't read from it. OpenCode independently flags that 19-02 edits `main.rs` *before* the 19-06 baseline is captured, so any line range in the baseline for code below `workflow_started_payload` is offset by however many lines 19-02 adds — and the "re-derive, don't reuse" warning lives only in `<extraction_procedure>` prose, not in task-level acceptance criteria an executor reads first. **Recommendation:** before executing Wave 2, either (a) have 19-06 write the baseline to a path under the phase directory that persists in git (not `/tmp`) and have 19-07/08/09/11 diff against that committed file, and (b) hoist the re-derivation instruction into each task's `<acceptance_criteria>` in 19-07/08/09.
- **[MEDIUM, recalibrated from Codex's HIGH] `depends_on` metadata doesn't match the stated sequencing.** See orchestrator note above — real, worth fixing before execution for traceability, but not an execution-order hazard given wave-gating.

### Divergent Views

- **19g wiring verifiability.** OpenCode rated this HIGH and unverifiable (claimed `project-skills-discovery.md` doesn't exist). **This is refuted** — the orchestrating session confirmed the file exists at `$HOME/.claude/gsd-core/references/project-skills-discovery.md`. Neither Codex nor Antigravity raised this concern at all. Most likely explanation: OpenCode's sandbox couldn't read outside the repo working directory and reported "not found" instead of "inaccessible to me." No action needed beyond what Plan 05's dogfood checkpoint already verifies.
- **Overall risk rating spread.** Codex: MEDIUM (weighted toward the depends_on/CI-wording/tmp-durability cluster). OpenCode: MEDIUM (weighted toward the now-refuted 19g claim plus the gitignore-deletion gap). Antigravity: LOW (did not weight either of the above as materially risk-affecting). Net: the *design* is consistently rated strong by all three; the spread is entirely about how much weight to put on plan-mechanics gaps (baseline durability, dependency metadata) that are fixable without replanning core decisions.
- **CLI-specific findings, no overlap.** Antigravity alone flagged the `ensure_devflow_dir` ancestor-search edge case on relative paths, the locale-dependent `"nothing to commit"` string match, and the unredacted `worktree` path in `workflow_started_payload` (T-19-09, already an accepted threat per CONTEXT.md — noted, not new). None of these were independently found by Codex or OpenCode; each is real and worth a look but none is disqualifying.

### Net Assessment

No reviewer — including the two that rated overall risk MEDIUM — flagged anything that should block execution or trigger a full replan. The concerns cluster into two buckets: (1) plan-mechanics hygiene fixable with small edits before Wave 2 begins (baseline durability, `depends_on` accuracy, CI-wording precision), and (2) genuinely low-severity edge cases worth a one-line doc acknowledgment (deleted-`.gitignore` gap, locale-dependent string match, ancestor-search on relative paths). The one HIGH-severity claim that would have been disqualifying (19g's wiring mechanism doesn't exist) is refuted by direct filesystem check.

**Recommendation:** `/gsd-plan-phase 19 --reviews` to fold in the baseline-durability fix and the `depends_on` metadata corrections before executing Wave 2 onward. Waves already committed to disk do not need to be re-planned from scratch — Codex's own suggestions are targeted edits, not a redesign.

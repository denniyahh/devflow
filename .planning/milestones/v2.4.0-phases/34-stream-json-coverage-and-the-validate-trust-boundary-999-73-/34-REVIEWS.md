---
phase: 34
reviewers: [opencode, antigravity, hermes, internal-fable]
dropped_reviewers:
  codex: usage_limit          # locked out until 2026-08-11; stub stderr confirmed the limit message
  cursor: usage_limit         # ActionRequiredError usage limit (recurring — known lane behaviour)
  qwen: no_auth_configured    # "No auth type is selected" in non-interactive mode
  claude: self_skip           # orchestrator runs inside Claude Code; skipped for independence
reviewed_at: 2026-08-05T20:46:13.223Z
plans_reviewed: [34-01-PLAN.md, 34-02-PLAN.md, 34-03-PLAN.md, 34-04-PLAN.md, 34-05-PLAN.md]
notes: |
  Antigravity lane stubbed via the packaged agy binary (pre-session stall); the review below was
  produced by a manual agycli retry with a file-reference prompt against the identical review
  request. The internal-fable voice is a full-tooling in-repo adversarial pass (Fable 5) run as
  part of the same panel with the same shared instructions plus source-verification obligations.
---

# Cross-AI Plan Review — Phase 34

## Codex Review

codex review failed or returned empty output. stderr (decisive lines; full stub truncated — it echoes the 334 KB prompt):
> ERROR: You have hit your usage limit. … try again at Aug 11th, 2026 8:01 AM.

---

## OpenCode Review

Now let me check one more thing — the `phase_has_blocking_human_checkpoint` test family, and how `prune_history` works:
# Cross-AI Plan Review: Phase 34 — Five Plans

## 1. Plan 34-01 (Wave 1): `reconcile_layer0_verdict` Graft Fix

### Summary

The plan correctly identifies and fixes the real 999.74 defect site: the graft at `agent_result.rs:2154` which transplants Layer 1's `verdict` onto an affirmative Layer 0 probe success without checking Layer 1's own status. The fix is a one-expression change (`Option::filter` on Layer 1's `status`), the test extension is surgically minimal, the three negative controls establish that the exploit needs both `status: failed` and `verdict: pass` together, and criterion 5's written-finding tasks correctly preserve the live guard at `idle_timeout_result`. The plan explicitly disclaims closing criterion 3, which is correct.

### Strengths

- **Surgical scope**: one expression in `reconcile_layer0_verdict` (`agent_result.rs:2154`), unchanged early-return guards, no signature change
- **Extends existing test** (`layer0_affirmative_success_consults_layer1_verdict_at_validate`, `agent_result.rs:5484`) rather than building a new fixture — reuses `dir`, `state`, `approval` bindings already in scope
- **The fourth marker case `{"status":"failed","verdict":"pass"}` → `verdict: None` is exactly the exploit**, and the plan's instruction to "observe the red pre-fix state and record it" is correct discipline
- **NC-5's positive half** (`layer0_verdict_graft_still_transplants_a_passing_layer1_verdict`) prevents an over-broad fix that discards every verdict
- **NC-6** — Layer 0 disabled → `decide_action` routes to `GateReview` — correctly proves the graft is the mechanism, confirmed by the pre-existing 9-test baseline (`outcome_policy.rs` tests unchanged)
- **Criterion 5 task 3**: correctly distinguishes between `idle_timeout_result`'s doc comment (live guard, MUST NOT edit, verified by `git show develop` vs `HEAD` grep) and `reconcile_stream_success_against_exit_code`'s doc comment (overstated, must correct)

### Concerns

- **LOW**: `reconcile_layer0_verdict` calls `evaluate_layer1(project_root, state.phase)` with `project_root` (`agent_result.rs:2154`). After plan 34-04 lands, Layer 0 discovery moves to `execution_root`, but this call stays on `project_root`. This is correct — Layer 1 reads `.devflow/` captures which live in the project root — but the plan doesn't acknowledge this deliberate asymmetry on the line it's editing. Not a defect, just a gap in the reasoning that a reader might reconstruct incorrectly.

### Suggestions

- Add a one-line comment above `evaluate_layer1(project_root, ...)` on the post-fix line stating that Layer 1's capture lives in the project root (`.devflow/`), so `project_root` here is correct even after Layer 0 discovery moves to the execution root in plan 34-04
- The SUMMARY should record the exact `left`/`right` assertion failure from the pre-fix red run, per the acceptance criterion

### Risk Assessment: **LOW**

The fix is a one-expression change to a pure function, the test harness already exists, all three negative controls are in the same crate's test module, and the worst outcome (the fix doesn't take) is caught by the named regression test's assertion on `Some(Pass)` → `None`.

---

## 2. Plan 34-02 (Wave 2): Criterion-7 Collateral and Capture Infrastructure

### Summary

Three independent infrastructure items landed before anything widens `STREAM_JSON_STAGES`: a canary test rebuilt on the legacy opt-out (so it survives full widening), a retention constant raised from 5 to 12 (so loop-backs can't evict unread captures), and a Phase-30-shaped evidence directory layout stubbed with PII instructions. The plan correctly defers the actual widening to 34-05.

### Strengths

- **Canary test rebuild is the correct discriminator**: `legacy_claude_launch = true` always returns `false` from `claude_stream_launch_enabled` (`pipeline_launch.rs:479`: `!legacy_opt_out && ...`), regardless of `STREAM_JSON_STAGES`'s contents. The acceptance criterion requiring a temporary full-wide control (`Stage::Code, Plan, Define, Validate, Ship`) confirms the rebuild actually survives widening
- **Retention arithmetic is recorded in source**: `DEFAULT_CAPTURE_RETENTION = 12` with a doc comment showing 4 + (2 × 4) = 12 transitions, matching `archive_phase_files_with_stamp`'s per-launch-once archive cycle at `agent_result.rs:2453-2547`
- **Evidence directory layout is pre-stubbed** so the capture-copy step has a landing spot. README.md records the `.devflow/.gitignore` trap and the `home_path`/`os_username`/`session_identifier` PII redaction fields from Phase 30's table
- **`depends_on: ["34-01"]`** correctly captures the shared `agent_result.rs` file — same-wave same-file conflict avoidance

### Concerns

- **LOW**: The retention test (`prune_history_retains_a_full_five_stage_run_with_loop_backs`) must construct file names that `prune_history`'s `rsplit_once('-')` at `agent_result.rs:2624-2625` can parse. The stamp format is `{nanos}-{seq}` (two `-` separated components). If the test writes files like `1-1-stdout`, `rsplit_once('-')` splits to `("1-1", "stdout")` — correct. But the plan's instruction to "write 12 files whose names follow `archive_stamp`'s `{nanos}-{seq}` shape" is slightly imprecise: `archive_stamp` (`agent_result.rs:2417`) uses `SystemTime::now()` nanos plus a local counter. A test constructing fixed stamps like `1000000000-1-stdout`, `1000000000-2-stdout`, ... will work correctly against `prune_history`'s sort key (`agent_result.rs:2628-2639`). Good.
- **LOW**: The evidence directory's `.gitkeep` files. Some git workflows treat `.gitkeep` as a convention, but it has no special meaning to git. Committing a directory requires a file. The plan should consider using a 0-byte `.gitkeep` rather than relying on the README alone — the subdirectory creation in task 3 uses `mkdir` but doesn't specify a `.gitkeep` for git tracking. Actually, wait — the plan says "Add a `.gitkeep` in each stage subdirectory so the empty layout is committable." This is correct.

### Suggestions

- None substantive — the plan is mechanically sound

### Risk Assessment: **LOW**

All changes are test-only or documentation-only, except a single `const` raise with immediate-verification arithmetic. No production behavior changes.

---

## 3. Plan 34-03 (Wave 2): Exhaustive `classify_validate_outcome` Match Rewrite

### Summary

The classifier's `match (external, result.verdict)` — where `external` at `pipeline_outcomes.rs:204` is a composite boolean folding `decided_by_layer == Some(0) && status == Success` — is replaced with `match (layer0, result.status, result.verdict)` where `layer0` is purely `decided_by_layer == Some(0)`. The `AgentStatus` position enumerates all seven variants by name with no wildcard toward `Passed` or `Failed`. A 42-cell sweep plus four named controls (NC-1 through NC-4) pin both the intended routing and the exhaustiveness guarantee. The plan explicitly disclaims closing criterion 4.

### Strengths

- **Named trap avoidance**: the composite-`external` normaliser conflates "Layer 1, Success" with "Layer 0, Failed" — the plan correctly rejects reusing it and introduces `layer0` as the layer-only boolean
- **42-cell sweep is correctly dimensioned**: `layer0 ∈ {true, false} × 7 AgentStatus × 3 Option<Verdict>` = 42, with a visited-counter pinned to the literal `42` so a truncated iterator doesn't pass silently
- **`classifier_fixture` uses `Some(0)`/`Some(1)`, never `None`**: `decided_by_layer`'s doc comment (`agent_result.rs:38-42`) reserves `None` for test-only fixture literals — using `None` would silently collapse the `layer0 = false` half of the sweep to indistinguishable from the `layer0 = true` half, and task 2's second mutation control deliberately reintroduces this omission to prove the tests detect it
- **NC-2/NC-3 paired mirrors**: `(true, Success, Some(Gaps)) → Ambiguous` paired with `(false, Success, Some(Gaps)) → Failed` inside the same test function — a regression deleting both `Ambiguous` arms makes the test red
- **The three pre-existing `external_verify_*` tests** at `pipeline_outcomes.rs:1009-1130` are correctly predicted to pass unmodified: all three test Layer-0-provenance scenarios (`decided_by_layer: Some(0)`, `status: Success`) whose destinations the rewritten match preserves
- **NC-4 run as a recorded compile experiment**: correct choice — Rust cannot assert `E0004` from inside the same crate without a `trybuild` dependency, and the plan correctly declines to add one

### Concerns

- **MEDIUM**: The plan's task 1 says "delete the claim that `Some(Verdict::Pass)` is matched FIRST and wins regardless of which layer decided" — this claim in `pipeline_outcomes.rs:179-182`. After the rewrite, the match is no longer ordered by `verdict` first, but the in-source record must still explain *why* the old shape existed and that the new shape is the successor to it. The plan's rewrite instruction covers this in the doc-comment paragraph.

- **MEDIUM**: The `RateLimited` and `AgentUnavailable` cells are routed to `Failed` with an inline comment naming the contradiction with `decide_action`'s `RateLimited => AutoResume` (`outcome_policy.rs:41`). The cells are unreachable at the classifier (R-01), but a future routing change that makes them reachable would create a silent behavioural divergence. The inline comment mitigates this but does not prevent it. The plan acknowledges this as threat T-34-03-05 and accepts it. Given that the current `_` wildcard also maps these to `Failed`, the delta is zero.

- **LOW**: The plan's task 2 says `classifier_fixture` uses `Some(1)` for `layer0 = false`. In production, `decided_by_layer` can be `Some(1)` (Layer 1), `Some(2)` (Layer 2), or `Some(3)` (Layer 3). All three produce the same `layer0 = false` normalisation. The choice of `Some(1)` as the representative is correct and sufficient — what matters is that it's not `Some(0)`.

### Suggestions

- Add a one-line assertion in `classify_validate_outcome_sweeps_all_forty_two_cells` that the visited-cell counter is exactly 42 **before** the last cell is processed, catching an early return
- Consider adding to NC-4's summary an explicit count of how many `AgentStatus` arms exist in the rewritten match, as a secondary structure check independent of the E0004 controls

### Risk Assessment: **LOW**

The rewrite is a pure-function change, the sweep is exhaustive, the three `external_verify_*` tests pass unmodified confirming no behavioral regression, and NC-4's two mutation controls (delete arm → E0004, add variant → E0004) establish compile-time exhaustiveness. The unreachable-`RateLimited` concern is documented, not surprising, and has zero runtime delta.

---

## 4. Plan 34-04 (Wave 3): Layer 0 Discovery from the Execution Root (999.76)

### Summary

Changes `evaluate_layer0`'s discovery call at `agent_result.rs:2042` from `project_root` to `execution_root`, fixes the `phase_has_blocking_human_checkpoint` call at `pipeline_launch.rs:957` the same way, adds a companion fixture that inverts the existing test's layout, and preserves the correct asymmetries (`phase_commit_count` stays on `project_root`, `checkpoint_reported_in_capture` stays on `project_root`). The plan has an explicit `<precondition>` asserting plan 34-01's graft fix is at HEAD.

### Strengths

- **Binding-constraint precondition**: `cargo test -p devflow-core --lib agent_result::tests::layer0_verdict_graft_declines_when_layer1_status_is_not_success -- --exact` must report `1 passed` before any task runs. This is an executable guard, not a prose promise
- **Both 999.76 call sites fixed together**: `evaluate_layer0` (`agent_result.rs:2042`) and `phase_has_blocking_human_checkpoint` at `pipeline_launch.rs:957` — the plan correctly identifies that leaving the second call site unfixed makes the fix incomplete
- **No fallback to `project_root`**: the plan explicitly rejects a fallback, correctly noting that a fallback makes the test unable to discriminate and reintroduces the ambiguity the fix removes
- **Preserved asymmetries**: `phase_commit_count` is explicitly "NOT retargeted" because worktrees share the object database (`agent_result.rs:1832` area), and `checkpoint_reported_in_capture` at `pipeline_launch.rs:958` stays on `project_root` because captures live in `.devflow/`
- **Pre-existing test conversion is explicit**: the plan says to re-run `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` against the change, observe the failure, and convert it by clearing `worktree_path` — naming the exact failure that will occur (the worktree-standing-in directory is empty, discovery finds nothing, "PLAN removed" veto fires) rather than assuming it
- **NC-7 as a recorded provenance measurement**: correctly noted as non-durable (it goes stale when the phase ships), with the durable regression coverage in task 1's tempdir fixtures

### Concerns

- **MEDIUM**: The `evaluate_layer0` doc comment at `agent_result.rs:2025-2031` currently asserts the opposite of the fix: *"`project_root` is used to DISCOVER the PLAN's declared commands (`.planning/phases/` lives there, not in a worktree checkout)"*. The plan says to rewrite it. But the comment also serves as the archival record of the prior decision. The rewrite must keep enough of the prior reasoning that a future reader understands the reversal, not just the new state. The plan's action addresses this: "State explicitly that this fix knowingly overturns a recorded prior peer-review decision."

- **LOW**: After the fix, `evaluate_layer0` discovers from `execution_root` (`agent_result.rs:2042`). Line 2041 computes `execution_root` from `state.worktree_path.as_deref().unwrap_or(project_root)`. The `unwrap_or(project_root)` fallback means: when no worktree is set, the two roots coincide and discovery reads `project_root` — preserving the main-checkout behavior. This is correct.

- **LOW**: The `reversibility` rating of `costly` is justified: "it knowingly overturns a recorded prior peer-review decision, so a revert must restore that decision's reasoning too." The plan doesn't need to change this — it's accurate.

### Suggestions

- Task 1's action on the pre-existing test should specify: after conversion, rename it to something like `external_probe_discovers_from_project_root_across_every_stage_without_a_worktree` so the name matches the post-fix behavior
- The acceptance criterion for the overturned prior-decision record should quote the exact replacement paragraph in the SUMMARY, so the verifier doesn't need to re-derive what "knowingly overturns" means

### Risk Assessment: **MEDIUM**

The mechanical change is a one-argument edit, but it knowingly overturns a recorded prior peer-review decision about where `.planning/` lives. An execution-root discovery that fails to find the PLAN (legitimate missing phase docs, not the 999.76 defect) fires the "PLAN declaration was removed" veto, which is an authoritative `Failed` with `decided_by_layer: Some(0)`. In a run where the worktree genuinely lacks `.planning/phases/`, this is the correct behavior, not a regression. But the phase won't have real-world worktree-mode dogfood evidence until after this fix lands, which is inherently circular.

---

## 5. Plan 34-05 (Wave 4): Capture Campaign and Final Widening

### Summary

The only plan that produces an actual agent run. Widens `STREAM_JSON_STAGES` to all five stages (provisionally), takes a live `--no-worktree` capture campaign against a scratch repo, then lets the evidence decide which stages stay widened. Two blocking human-verify checkpoints gate the run and the evidence commit. The `autonomous: false` flag is correct — this plan requires operator presence.

### Strengths

- **Binary promotion investigation (task 1)**: RESEARCH.md's Open Question 1 is resolved by inspection, not assumption. Records every `devflow` on PATH, notes the shadowing hazard (stale static copy vs fresh symlink), and writes `BINARY-PROMOTION.md` so a later capture run doesn't re-derive it
- **Two independent binary proofs (task 2)**: the build mtime comparison **and** a behavioural proof that the promoted binary resolves a widened stage to the stream path. The plan correctly states that a source-tree proof is not a binary proof and must be labeled honestly if that's the fallback
- **Canary relocation is recorded as a deliberate behavior change BEFORE the run**: task 2 records that the D-15 canary now fires at Define rather than Code, so an `Absent`/`Unverified` outcome refuses at the first stage — and the plan explicitly says NOT to route around this
- **Copy-at-landing discipline**: each capture is copied out of `.devflow/` before the next archive event — plan 34-02 raised `DEFAULT_CAPTURE_RETENTION` to 12, but copy-at-landing is the operational belt-and-braces
- **Commit-time evidence gate (task 5)**: the decision table is explicit — `NeverAnnounced` → widen (vacuously drained), `Pending(n)` that drains → widen (defensive), `Pending(n>0)` never draining → narrow unless a stated basis exists, `Unreadable` → D-14 governs (narrow, file as defect)
- **PII scrubbing**: three named fields (`home_path`, `os_username`, `session_identifier`), an automated `rg` scan, and a blocking human read of every staged file before commit — the one control the plan correctly labels irreversible after push
- **Delivery floor escalation**: if zero stages are newly widened, the plan must name what was attempted, what prevented each stage's capture, and what would unblock it — explicitly addressed to the operator

### Concerns

- **MEDIUM**: The plan's task 5 decision table for `Pending(n>0)` never draining says "if you cannot state it from this single capture, leave the stage narrow and record that the basis could not be established." But D-11 says a non-draining capture *can* widen the stage with a stated pathological basis. Under D-10's n=1, the "pathological rather than routine" distinction is genuinely unanswerable. The plan should default to narrowing for this case and only widen with an explicit, operator-acknowledged basis at the second checkpoint — the current wording leaves ambiguity about the default behavior.

- **MEDIUM**: The plan says `Stage::Code` "must either be re-captured here, or its transcription-only evidence recorded as transcription-only." But re-capturing Code under a scratch repo means the capture is of a *different* phase than what Phase 31 captured. The plan should note that this capture's context differs (scratch repo vs real dogfood), and that the Phase 31 transcription is still the only evidence about Code's behavior during a real phase.

- **MEDIUM**: Task 3's checkpoint asks the operator to "Choose the capture target." The plan recommends `scripts/scratch-dogfood-repo.sh`. The script exists (`glob` confirmed), but the plan doesn't state what the script scaffolds — the operator may need to inspect it at checkpoint time. The recommendation should note that the script's destination-safety guard refuses any path inside this repo, which is correct for the CLAUDE.md git rule.

- **LOW**: The plan depends on `scripts/scratch-dogfood-repo.sh` but doesn't verify the script's `run_preflight` check passes before execution. If `claude` isn't on PATH (RESEARCH.md says it is — `2.1.222`, verified), the preflight silently fails and the run uses the wrong agent. The checkpoint question 1 asks about the `devflow` binary, but doesn't also ask about the `claude` binary.

- **LOW**: The plan says the three-field scrub is "the canon privacy/compliance item deliberately carried here rather than minted as a prohibition." This is correct per PATTERNS.md's correction about Phase 30's raw/scrubbed/operator split being three separate runs. But the plan's scrub procedure should also mention removing or replacing `session_id` values that appear in the stream-json event structure (the `"session_id"` field in `system` events), which is separate from `session_identifier` in the redaction table.

### Suggestions

- Task 5: make the `Pending(n>0)`-never-draining default explicitly "narrow unless a stated basis exists" rather than leaving it ambiguous
- Task 3's checkpoint: add a question about verifying the `claude` binary availability for the scratch run
- Task 6's checkpoint: add a step to visually verify that `session_id` fields in `system` events are also placeholder-scrubbed, not just the three named PII fields
- The SUMMARY should separately note that any Code-stage capture from a scratch repo is contextually different from the Phase 31 real-phase capture, and that Phase 31's transcription is the only production evidence for Code at its actual stage

### Risk Assessment: **MEDIUM**

This is the phase's riskiest plan: it involves a live agent run in `--no-worktree` mode (CLAUDE.md's git rule applies), two blocking checkpoints (999.57 / DEN-82 means they can't be auto-approved), binary promotion with a shadowing hazard, PII exposure to a public repo, and evidence-dependent widening. The plan's safeguards are thorough (two checkpoints, automated PII scan, copy-at-landing, recorded canary relocation), but the inherent risk of committing agent captures to a public repository cannot be eliminated — only mitigated by the scrub procedure and the blocking human read.

---

## Cross-Plan Assessment

### Dependency Ordering

The DAG is correct: 34-01 (wave 1) → 34-02 + 34-03 (wave 2, depends on 34-01 for shared `agent_result.rs`) → 34-04 (wave 3, depends on 34-01 and 34-02) → 34-05 (wave 4, depends on 34-02 and 34-04). The binding constraint (999.76 must not land without the graft fix) is enforced by 34-04's `<precondition>` plus the `depends_on: ["34-01"]` edge.

### Phase Goal Coverage

| Criterion | Plan | Status |
|-----------|------|--------|
| 1 | 34-02 (infra) + 34-05 (widening) | Covered |
| 2 | 34-05 (drain analysis) | Covered |
| 3 | 34-03 (exhaustive match) | Covered |
| 4 (core) | 34-01 (graft fix) | Covered |
| 4 (downstream) | 34-03 (routing demo) | Covered |
| 5 | 34-01 (written finding) | Covered |
| 6 | 34-04 (discovery fix) | Covered |
| 7 (canary) | 34-02 (rebuild) | Covered |
| 7 (retention) | 34-02 (constant) + 34-05 (copy-at-landing) | Covered |
| 7 (canary relocation) | 34-05 (recorded behavior change) | Covered |

All seven success criteria are covered. No criterion is claimed twice without the separation being explicit (criterion 4's two halves are in different plans with cross-crate partner naming).

### Collision-Free Wave Topology

- Wave 1: `agent_result.rs` (34-01)
- Wave 2: `pipeline_launch.rs` + `config.rs` + `agent_result.rs` (34-02), `pipeline_outcomes.rs` (34-03)
- Wave 3: `agent_result.rs` + `verify.rs` + `pipeline_launch.rs` (34-04)
- Wave 4: `pipeline_launch.rs` + evidence dir + `ROADMAP.md` (34-05)

Same-wave same-file collisions: 34-02 and 34-03 have none (different crates/files). 34-02 and 34-01 share `agent_result.rs` — correctly serialized via `depends_on`. 34-04 shares `agent_result.rs` with 34-01/34-02 and `pipeline_launch.rs` with 34-02 — correctly serialized via `depends_on`. 34-05 shares `pipeline_launch.rs` with 34-02/34-04 — correctly serialized via `depends_on`. All waves are topologically consistent.

---

## Qwen Review

qwen review failed or returned empty output. stderr:
> No auth type is selected. Please configure an auth type (e.g. via settings or `--auth-type`) before running in non-interactive mode.

---

## Cursor Review

cursor review failed or returned empty output. stderr:
> ActionRequiredError: You have hit your usage limit …

---

## Antigravity Review

> Lane note: the standard `agy` lane stubbed (pre-session stall). This review is from a manual
> `agycli --print` retry with a file-reference prompt — same request, same instructions.

# Cross-AI Plan Review: Phase 34 (Stream-JSON Coverage, Validate Trust Boundary, and Layer 0 Worktree Discovery)

## Summary

The 5-plan execution arc for Phase 34 (`34-01-PLAN.md` through `34-05-PLAN.md`) is a rigorously structured, TDD-driven remediation plan addressing backlog items `999.73`, `999.74`, and `999.76`. The plan set demonstrates exceptional attention to root-cause mechanics, concrete `file:line` verification, and negative controls. By prioritizing the `reconcile_layer0_verdict` graft fix in Wave 1 (`34-01-PLAN.md`) prior to enabling Layer 0 worktree discovery in Wave 3 (`34-04-PLAN.md`), the plans strictly respect the binding architectural constraint (preventing the widening of the graft exploit precondition). The plan set avoids speculative refactoring and establishes an empirical, commit-time evidence gate for stage widening backed by live `.jsonl` captures.

---

## Strengths

- **Surgical Root-Cause Remediation of the Validate Trust Boundary:** `34-01-PLAN.md` targets the true laundering route at [`crates/devflow-core/src/agent_result.rs:2143-2156`](file:///home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs#L2143-L2156) (`reconcile_layer0_verdict`), where Layer 1's `verdict` was grafted onto a Layer 0 probe success without verifying `layer1.status == AgentStatus::Success`. Requiring status verification before transplanting the verdict closes the live hole where `{"status":"failed","verdict":"pass"}` resulted in `Action::Advance` to `Stage::Ship`.
- **Exhaustive Structural Type Safety Over Hand Audits:** `34-03-PLAN.md` replaces the wildcard match in [`crates/devflow-cli/src/pipeline_outcomes.rs:203-215`](file:///home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_outcomes.rs#L203-L215) with a 42-cell matrix `(layer0, status, verdict)` spanning all seven `AgentStatus` variants. Banning positional wildcards in the status column ensures future variant additions produce `E0004` compile errors rather than silent status laundering.
- **Strict Dependency & Precondition Ordering:** `34-04-PLAN.md` enforces `depends_on: ["34-01"]` and includes an explicit `<precondition>` block asserting that the graft fix is present at `HEAD` before executing 999.76 worktree discovery fixes. This prevents making `decided_by_layer == Some(0)` common in worktree mode while the graft exploit remains open.
- **Preservation of Intentional Architectural Asymmetries:** `34-04-PLAN.md` correctly updates discovery in [`crates/devflow-core/src/agent_result.rs:2042`](file:///home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs#L2042) and [`crates/devflow-cli/src/pipeline_launch.rs:957`](file:///home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs#L957) to use `execution_root`, while explicitly preserving `project_root` for `phase_commit_count` and `checkpoint_reported_in_capture` (which depend on shared git object databases and `.devflow/` project-root paths).
- **Provisional Commit-Time Evidence Discipline:** `34-05-PLAN.md` enforces D-02 Amendment 1: stages are provisionally widened in the working tree for capture taking, but only committed into `STREAM_JSON_STAGES` ([`crates/devflow-cli/src/pipeline_launch.rs:446`](file:///home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs#L446)) if real per-stage captures show clean execution or vacuous draining (`NeverAnnounced`). Any un-evidenced stage is retained on the legacy path with an explicit doc comment reason.
- **Robust Negative Control Coverage:** Every plan pairs positive assertions with paired negative controls (NC-5 positive/negative halves, NC-6 Layer-0-disabled routing, NC-7 two-ref `git ls-tree -r` provenance, and canary gate opt-out discrimination in [`crates/devflow-cli/src/pipeline_launch.rs:1754`](file:///home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs#L1754)).

---

## Concerns

- **Binary Shadowing Hazard during Capture Execution [MEDIUM]:**
  - **Location:** `34-05-PLAN.md` (Task 1 & Task 2)
  - **Issue:** `STREAM_JSON_STAGES` is a compile-time Rust constant. If the operator's shell resolves a pre-existing static binary (such as `~/.local/bin/devflow` at v1.8.0) rather than the newly compiled `target/release/devflow`, `devflow start` will execute on the legacy path without throwing an error.
  - **Verification Limit:** Passing `cargo test -p devflow` verifies source-tree logic but does *not* prove that `PATH` resolution in the capture shell invokes the freshly built binary.
  - **Mitigation:** Task 1's `BINARY-PROMOTION.md` investigation and Task 2's explicit mtime check (`ls -la target/release/devflow` vs `command -v devflow`) mitigate this risk, but the plan must strictly enforce this check before Task 4's live run.

- **Non-Draining Capture Interpretation vs Idle Timeout Gate [MEDIUM]:**
  - **Location:** `34-05-PLAN.md` (Task 5) & [`crates/devflow-core/src/monitor.rs:533-593`](file:///home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs#L533-L593)
  - **Issue:** If a capture exhibits `BackgroundTaskState::Pending(n>0)` that never drains before the marker, the monitor loop terminates via `RecvTimeoutError::Timeout` and triggers `fire_idle_timeout`. This terminates the child process and gates the stage.
  - **Verification Limit:** A single capture ($n=1$) cannot establish whether a non-draining task list is pathological (a one-off anomaly) or routine for that stage.
  - **Mitigation:** Task 5 correctly defaults to keeping the stage on the legacy path unless a specific pathological rationale can be defended.

- **Canary Refusal Relocation to Stage::Define [LOW]:**
  - **Location:** `34-05-PLAN.md` (Task 2) & [`crates/devflow-cli/src/pipeline_launch.rs:333-354`](file:///home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_launch.rs#L333-L354)
  - **Issue:** Widening `STREAM_JSON_STAGES` to include `Stage::Define` moves the `canary_gate` evaluation from `Stage::Code` to `Stage::Define`. An `Absent` or `Unverified` canary outcome will cause the run to refuse at the very first stage rather than completing `Define` and `Plan` on the legacy path.
  - **Verification Limit:** This alters unattended operational behavior. The plan correctly prohibits clearing `state.canary` or forcing `DEVFLOW_CLAUDE_LEGACY_LAUNCH` to bypass a refusal.

---

## Suggestions

1. **Enforce Absolute Path Checks in `BINARY-PROMOTION.md` Verification:** In `34-05-PLAN.md` Task 2, explicitly verify that `$(realpath $(which devflow))` resolves to `/home/denniyahh/Github/devflow/target/release/devflow` (or the intended symlink target) prior to launching Task 4.
2. **Explicit PII Redaction Verification Script:** In `34-05-PLAN.md` Task 5, execute an automated check `rg -i "$USER|$HOME|session-[a-z0-9-]{10,}" .planning/phases/34-*/34-evidence/` to guarantee no unscrubbed session IDs or host path strings are committed to version control.
3. **Preserve `Stage::Code` Raw Capture Provenance:** In `34-05-PLAN.md` Task 5, ensure `Stage::Code`'s doc comment entry explicitly notes whether its evidence stems from a fresh Phase 34 capture or Phase 31 transcription history, preserving artifact auditability.

---

## Risk Assessment

- **Overall Risk Level:** **LOW**
- **Justification:**
  - The plan set is exceptionally thorough, TDD-grounded, and tightly scoped to internal Rust logic in `devflow-core` and `devflow-cli`.
  - All critical architectural decisions (D-01 through D-15) and binding sequencing constraints are enforced.
  - No external crate dependencies are added, eliminating supply-chain or version-drift risks.
  - Pre-fix and post-fix behaviors are empirically bounded by comprehensive unit tests and mandatory negative controls.

---

## Hermes Review

**Summary**  
The five plans for Phase 34 collectively implement the seven success criteria outlined in the ROADMAP for the stream‑JSON coverage, validate trust boundary, and Layer 0 worktree‑mode work. Each plan is test‑driven, includes explicit negative controls, and documents assumptions and limitations. The changes touch the core pipeline launch, outcome classification, agent result handling, and evidence‑capture infrastructure, with clear separation between the graft fix (plan 34‑01), the classifier rewrite (plan 34‑03), and the worktree‑discovery fix (plan 34‑04). Plan 34‑02 prepares the test harness and capture infrastructure, while plan 34‑05 executes the live capture campaign and widens `STREAM_JSON_STAGES` based on evidence.

**Strengths**  
- **Test‑driven with negative controls**: Every plan includes automated tests that verify both the positive case and deliberate negative controls (e.g., NC‑5/NC‑6 in plan 34‑01, the 42‑cell sweep in plan 34‑03, worktree vs. main‑checkout fixtures in plan 34‑04). This aligns with the project’s emphasis on structural guards over hand‑audits.  
- **Clear separation of concerns**: The graft fix (consulting Layer 1 status before verdict transplant) and the classifier rewrite (exhaustive match on `layer0, status, verdict`) are kept in separate plans, satisfying the requirement that neither alone closes the trust boundary.  
- **Evidence‑based widening**: Plan 34‑05 mandates per‑stage production captures, records reasons for any stage left narrow, and enforces a delivery floor (at least one newly widened stage or an explicit escalation). The plan also records the stage‑blind argv fact to ensure captures reflect agent behavior, not transport.  
- **Documentation of assumptions and limitations**: Each plan explicitly states what its evidence does not establish (e.g., n=1 does not guarantee steady‑state behavior) and flags unresolved probe‑fallback rows, preventing over‑claiming.  
- **Adherence to project verification habits**: Plans respect the repo’s conventions (e.g., using `cargo test --exact` with non‑zero filtered‑out counts, avoiding pipelines that mask exit codes, and preserving existing test baselines).

**Concerns**  
- **MEDIUM: Overlapping file modifications**  
  - Plans 34‑01, 34‑02, and 34‑04 all modify `crates/devflow-core/src/agent_result.rs` (adding tests, adjusting `evaluate_layer0`, and adding retention‑related tests). While the changes are non‑conflicting in intent, concurrent edits could create merge conflicts if the plans are applied sequentially without rebasing.  
  - *Suggestion*: Sequence the plans so that the tree is rebased between each, or consolidate overlapping edits into a single preparatory commit.  
- **MEDIUM: Reliance on manual, live agent runs**  
  - Plan 34‑05 requires an operator‑supervised `--no-worktree` capture run and a human‑verified checkpoint. This introduces variability (agent behavior, environment) and potential delays if the operator is unavailable.  
  - *Suggestion*: Automate as much of the capture pipeline as feasible (e.g., script the agent invocation and capture copy‑out) while keeping the human gate only for the final go/no‑go decision.  
- **LOW: Potential test fragility in plan 34‑03**  
  - The 42‑cell sweep depends on explicitly setting `decided_by_layer` in test fixtures. If the fixture helper is later changed to omit this field (relying on `serde(default)`), the sweep would silently miss half the matrix. The plan mitigates this with a mutation control, but the risk remains if future refactors overlook the comment.  
  - *Suggestion*: Add a comment to the fixture helper asserting that `decided_by_layer` must be set explicitly for test correctness.  

**Suggestions**  
1. **Coordinate overlapping file changes** – Before starting plan 34‑02, rebase onto the tip of plan 34‑01; similarly, rebase before plan 34‑04. This ensures a linear history and reduces merge conflict risk.  
2. **Automate capture prerequisites** – Extend the binary‑promotion script (task 1 of plan 34‑05) to also verify that the rebuilt binary contains the widened `STREAM_JSON_STAGES` via a quick inspection (e.g., `strings target/release/devflow | grep -c StreamJsonStage`) before proceeding to the live run.  
3. **Enrich test documentation** – In `agent_result.rs` test module, add a doc comment to the fixture helper used in plan 34‑03 explaining why `decided_by_layer` is set explicitly, to prevent future silent regressions.  
4. **Clarify rollback steps** – In each plan’s reversibility section, note the exact git command to revert the plan’s changes (e.g., `git revert <commit‑range>`), to aid operators who need to back out a failed attempt.  

**Risk Assessment**  
**Overall risk: MEDIUM**.  
- The technical changes are well‑tested and follow the project’s proven patterns, reducing the likelihood of regressions.  
- The primary risks stem from coordination overhead (multiple touching the same files) and the manual nature of the evidence‑gathering step, which could delay phase completion if the operator cannot schedule the live run. Mitigations (rebasing, automation of preparatory steps, explicit preconditions) are present and should be enacted.  
- No security‑critical surfaces are altered; the changes preserve existing threat mitigations (e.g., the graft fix still requires Layer 0 success, and the classifier retains its `Ambiguous` immediate‑gate routing).  

If the overlapping file edits are sequenced carefully and the capture run is adequately prepared, the plans should achieve the phase goals with high confidence.  
**End of review**.

---

## Internal Review (Fable 5, full repo tooling)

# Internal Adversarial Review — Phase 34 Plan Set (34-01 … 34-05)

**Reviewer:** internal (Claude, unrestricted repo tooling)
**Method:** every load-bearing citation, grep-based acceptance criterion, and mechanism claim was
re-derived against HEAD (`20d0c28`, working tree == `develop`'s `agent_result.rs` blob, verified by
`git hash-object`). Commands were run, not read. One of my own early tool runs produced shifted line
numbers; a `sed`/`git show` negative control caught it before it became a finding — ground truth
below is sed/rg-confirmed.

---

## 1. Summary

This is an unusually well-grounded plan set: every line citation I spot-checked is exact at HEAD
(including three-digit test-module line numbers like `MARKER_SUCCESS_CLAIMING_PASS` at
`agent_result.rs:5867/:5882/:5928`), the 42-cell arithmetic is correct from source, the graft defect
is real and the specified fix consults a value genuinely in scope, the binding sequencing constraint
(999.76 must not land without the graft fix) is unbreakable under the wave/depends_on topology, and
none of the five plans depends on any of the six premises recorded REFUTED in 34-REVIEW.md. Against
that strong baseline I found two HIGH defects, both self-contradictions rather than design errors:
34-04's frontmatter asserts a truth its own task body is designed to falsify (the pre-existing
worktree fixture "still passes unmodified" while task 1 predicts its failure and orders a
conversion-and-rename), and 34-05's PII gate is guaranteed to collide with the binary-promotion
record it also mandates — verified against this machine, where every `devflow` path contains the OS
username. Three MEDIUM findings concern acceptance criteria that cannot produce the opposite result
(the exact failure class the plans themselves prohibit) and an undeclared same-wave file touch.

## 2. Strengths

All items below were verified by running the stated check; each note also says what the check does
not establish.

- **Line citations are exact at HEAD.** `reconcile_layer0_verdict` at `agent_result.rs:2143-2156`
  (guard :2148, graft :2154); the composite `let external` at `pipeline_outcomes.rs:204`;
  `classify_validate_outcome` at `:203` with first arm `(_, Some(Verdict::Pass))`; the canary test at
  `pipeline_launch.rs:1754` with premise `Stage::Plan` and negative control at `:1777-1780`; the
  checkpoint call at `pipeline_launch.rs:957`; `state_in` at `agent_result.rs:2662`; `prune_history`
  at `:2615`; `DEFAULT_CAPTURE_RETENTION: usize = 5` at `config.rs:12`; the overturnable "Two roots
  are intentionally kept distinct (review Plan 03 MEDIUM," paragraph at `agent_result.rs:2025`. This
  establishes citation accuracy, not that the specified edits behave as predicted.
- **The 42-cell claim is arithmetic over verified source.** `AgentStatus` has exactly 7 variants
  (`agent_result.rs:47-81`: Success, Failed, RateLimited, Unknown, ResourceKilled, AgentUnavailable,
  IdleTimeout — `as_wire_str`'s wildcard-free match confirms the census independently); `Verdict` has
  2 (`:107-113`), so `Option<Verdict>` has 3 states; 2 × 7 × 3 = 42. The two `Ambiguous` arms are
  conditioned on the composite `external` today (`pipeline_outcomes.rs:204-212`), exactly as D-06's
  trap description says.
- **The graft fix consults a value that is really in scope.** Today's body is
  `evaluate_layer1(project_root, state.phase).and_then(|layer1| layer1.verdict)`
  (`agent_result.rs:2154`) — Layer 1's `status` is bound and discarded. `evaluate_layer1` returns
  `Option<AgentResult>` carrying `.status` (`:1789`), so 34-01's "filter on the bound result's
  status" is a one-expression change with no new plumbing. The RED prediction is right by code
  trace: a `{"status":"failed","verdict":"pass"}` marker parses to `(Failed, Some(Pass))` (lenient
  verdict deserializer keeps `"pass"`, `agent_result.rs:36`), and the graft transplants `Some(Pass)`.
  Not established: I did not execute the pre-fix test; this is a code-trace, not a run.
- **The binding constraint cannot be evaded by any frontmatter-permitted order.** Waves are exactly
  the depends_on DAG's topological levels (34-01→1; 34-02,34-03→2; 34-04→3; 34-05→4), so 34-04
  cannot execute before 34-01 under wave-ordered execution, and its `<precondition>` block
  additionally asserts the graft test passes at HEAD and halts — covering out-of-band single-plan
  invocation. I could not construct a counterexample.
- **Pinned baselines reproduce exactly.** `cargo test -p devflow-core --lib outcome_policy::` →
  `9 passed; 0 failed; 538 filtered out`, byte-matching the number 34-01 t2 pins. NC-7's control
  discriminates today: `git ls-tree -r develop --name-only -- .planning/phases | grep -c '/34-'` = 0,
  same against HEAD = 11. The two-ref timeout-doc probe is well-formed: `rg -c 'A timeout has no
  verdict to offer'` = 1 on both HEAD and develop now. `rg -c 'D-15' agent_result.rs` = 0 today, so
  34-01's ≥2 criterion discriminates.
- **34-05's binary-shadowing "known trap" is live, not hypothetical.** `command -v devflow` →
  `/home/linuxbrew/.linuxbrew/bin/devflow`, a symlink to
  `/home/denniyahh/Github/devflow/target/release/devflow` (v2.3.0, auto-tracks release builds),
  while `~/.local/bin/devflow` is a static copy at **v1.8.0**. The investigation task 1 mandates is
  aimed at a real hazard on this exact machine.
- **No plan depends on a refuted premise.** Checked all six: 34-01 correctly frames the inversion as
  graft-reachable and refuses the unwritable D-07 pin test; 34-03 explicitly does not claim
  criterion 4 and bans the composite normaliser; 34-04 implements the reversed sequencing;
  `idle_timeout_result`'s live-guard comment (`:1749`) is protected by prohibition plus a two-ref
  equality check; the 21-cell fixture trap (`decided_by_layer` `#[serde(default)]`, `:41`) is
  countered by `classifier_fixture`'s explicit `Some(0)`/`Some(1)`; `__monitor` (which calls
  `run_pipe_owning_monitor` directly, `pipeline_launch.rs:513`) is banned as criterion-1 evidence.
- **must_haves hygiene conforms to the gsd-core contract** (`plan-phase.md:823-824`): the single
  backstop item (34-02) is the required flat-scalar flow mapping `{ statement, verification:
  backstop }`; all prohibitions are descriptor-less strings under `must_haves.prohibitions:`; no
  check_* descriptors anywhere; the 4-row spec-less-probe accounting (3 authored + 1 flagged)
  balances across the plans exactly as 34-01's table claims.
- **Supporting artifacts all exist as cited:** `scripts/scratch-dogfood-repo.sh` with its
  destination-safety refusal and the `run_preflight`-only-needs-`claude`-on-PATH note;
  `scripts/install.sh`; Phase 30's `30c-evidence/run.log`; `31-ACCEPTANCE.md:25-26`'s VOID bar;
  `.devflow/.gitignore` is literally `*`; the evidence directory path is not swallowed by any ignore
  rule (`git check-ignore` exits 1); `devflow.toml`'s key is `external_verify_enabled`
  (`config.rs:58`, default `true` at `:81`); the fixture being extended has the `dir`/`state`/
  `approval` bindings 34-01 reuses (`agent_result.rs:5489-5545`).

## 3. Concerns

### HIGH

- **[34-04] must_haves truth #4 is falsified by the plan's own task 1 — a verification-contract
  contradiction.** The truth reads: "The pre-existing
  `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` fixture
  still passes unmodified." I verified it cannot: the fixture (`agent_result.rs:5269-5335`) writes
  the PLAN under `project_root`, sets `state.worktree_path` to an empty sibling, and asserts the
  reason contains `"external verification failed"`. After the fix, discovery reads the execution
  root (the empty worktree), `external_verify_commands` returns zero commands, and the
  approval-mismatch veto fires with reason `"external verification approval mismatch; PLAN
  declaration was removed"` (`agent_result.rs:2043-2050`) — the assertion fails, and the final
  `Success` assertion fails too. Task 1's own action knows this ("If it fails … convert it into the
  mirror by clearing `state.worktree_path` … and rename it"), so the frontmatter truth asserts
  precisely what the body plans to change. A gsd-verifier grading the truths against the
  post-execution tree finds the named fixture gone (renamed) and modified — a guaranteed
  goal-backward verification failure wedged into an otherwise correct plan.

- **[34-05] The PII gate and the binary-promotion record are mutually unsatisfiable as written —
  verified against this machine's real paths.** Task 1 requires `BINARY-PROMOTION.md` (inside
  `34-evidence/`) to record the resolved `devflow` path, `ls -la` symlink targets, and every other
  binary found — on this machine those are `/home/linuxbrew/.linuxbrew/bin/devflow →
  /home/denniyahh/Github/devflow/target/release/devflow` and `/var/home/denniyahh/.local/bin/devflow`,
  every one containing the OS username (`$USER` = denniyahh; the second also matching `$HOME`).
  Task 5's acceptance criterion requires the `rg -c "$USER|$HOME"` scan to report **no match
  anywhere under `34-evidence/`**, and checkpoint task 6 repeats "must return nothing" as the
  blocking pre-commit control. Task 5's scrub instruction covers only `raw_output.jsonl` and
  `run.log`, not `BINARY-PROMOTION.md`. So either the scan fails (wedging task 5), or the executor
  silently weakens the one control the plan calls "the one control on this plan that cannot be
  undone after a push," or scrubs BINARY-PROMOTION.md in an unplanned way. Given the boundary is
  irreversible public history, this needs an explicit resolution in the plan, not executor
  improvisation.

### MEDIUM

- **[34-05 t5] The D-12 filing criterion cannot fail — it is pre-satisfied at HEAD.**
  `rg -c 'per-child declared tokens' .planning/ROADMAP.md` already returns 1 today
  (`ROADMAP.md:892`, the existing 999.73-adjacent deferral note). The criterion "returns at least 1
  — D-12's re-filing landed" is therefore true before the task runs and detects nothing. This is
  exactly the check-that-cannot-produce-the-opposite-result shape the plans' own prohibitions ban.
  Same class, smaller stakes: 34-02 t3's "`rg -c 'exec_command' pipeline_launch.rs` returns at
  least 1" is pre-satisfied (count is 9 at HEAD — `resolve_launch_shape` calls it at `:168/:177`);
  only its companion `-B 20` clause does real work.

- **[34-03 t3] NC-4b temporarily mutates `crates/devflow-core/src/agent_result.rs`, which is
  neither in 34-03's `files_modified` nor compatible with the same-wave zero-overlap invariant.**
  34-02 (same wave 2) declares and edits `agent_result.rs`; 34-02's own objective states "Same-wave
  plans must have zero `files_modified` overlap." Under worktree-isolated execution this is
  harmless, but in a shared-tree parallel or degraded run the temporary eighth `AgentStatus` variant
  plus `cargo check -p devflow` races 34-02's concurrent edits, and 34-03's acceptance criterion
  "`git diff --stat crates/devflow-core/src/agent_result.rs` … is empty" false-fails against 34-02's
  uncommitted work. The mutation should be declared, or the criterion scoped to the mutation's own
  revert (e.g. `git diff` against the pre-mutation stash), or 34-03 moved off wave 2.

- **[34-03] The `fn classify_validate_outcome` grep windows are unanchored and will match the sweep
  test after task 2.** `rg -n 'fn classify_validate_outcome' -A 40/-A 45` substring-matches
  `fn classify_validate_outcome_sweeps_all_forty_two_cells` too. Consequences: the "`_ =>` count
  returns 0" criterion can false-fail on legitimate wildcard arms inside the sweep's
  expected-outcome helper (the positional ban governs the production match only), and the
  seven-variant presence criterion can false-pass off test content rather than the production match.
  Anchor the pattern (`fn classify_validate_outcome\(`) or scope with `-m 1`. Related window risk:
  34-05 t5's five-stage doc-comment scan uses `-B 40` from the `const`, and after 34-02's paragraph
  plus 34-05's five per-stage entries plus the canary-relocation record the doc comment may exceed
  40 lines, false-failing the per-stage presence check.

### LOW

- **[34-02 t1]** `canary::CanaryOutcome::Present` does not exist — the variants are `Confirmed`,
  `Absent`, `Unverified(String)` (`canary.rs:107-117`). The plan's own hedge ("or whichever variant
  the harness accepts — read … before choosing") makes this self-correcting; the right choice is
  `Confirmed`.
- **[34-02 t2]** "Write 12 files whose names follow `archive_stamp`'s `{nanos}-{seq}` shape" is
  under-specified against `prune_history`'s actual grouping: it derives stamps by `rsplit_once('-')`
  and deletes `{stamp}-stdout|-exit|-REVIEW.md` (`agent_result.rs:2615-2652`). Files named bare
  `{nanos}-{seq}` mis-parse (stamp = nanos) and the deletions target nonexistent names — the
  evict-one half can never pass, while the retain-all half false-passes (early return at
  `stamps.len() <= retain`). Files must be named `{nanos}-{seq}-stdout`. Discoverable red, and the
  read_first points at the body, but the instruction as written builds the half-green trap.
- **[34-04 t2]** "The five existing `phase_has_blocking_human_checkpoint_*` tests" — there are six
  (`verify.rs:249, 260, 274, 287, 294, 314`). The relative "+2 over pre-task count" criterion
  absorbs this, so it is cosmetic.
- **[34-01 t3, 34-04 t3]** `git fetch -q origin develop` does not move the local `develop` ref that
  the subsequent `git show develop:` / `git ls-tree develop` read — the fetch contributes nothing to
  the comparison. Harmless today (local `develop` = `91a1b58` exists), but the command implies a
  freshness it does not provide.
- **[34-02 t2]** The retention arithmetic's "12 therefore survives four loop-backs with headroom" —
  4 + 4×2 = 12 exactly; headroom is zero at four loop-backs. The must_have truth's "up to three
  loop-backs" bound (10 ≤ 12) is the one with headroom. Fix the doc-comment wording before it is
  committed as an "inspectable source artifact."
- **[34-02]** `files_modified` omits the `34-evidence/` tree task 3 creates (34-05 declares its
  evidence path; 34-02 does not). Moot for wave ordering, but any tooling keyed on `files_modified`
  is blind to it.
- **[34-02 t3]** `rg -c` counts matching *lines*, not matches: the
  `'home_path|os_username|session_identifier'` == 3 criterion silently requires the three fields on
  three separate README lines; a single-line phrasing yields 1 and false-fails.

## 4. Suggestions

1. **34-04:** Rewrite must_haves truth #4 to describe the post-conversion state — e.g. "the
   pre-existing fixture, converted to its main-checkout mirror (`worktree_path` cleared, renamed
   …`_without_a_worktree`), keeps every original assertion" — and add the renamed test to the
   artifacts list. The current wording guarantees a verifier failure.
2. **34-05:** Decide the BINARY-PROMOTION.md/PII collision explicitly: either extend task 5's scrub
   to placeholder the home-path/username components of recorded binary paths (`<home>/.local/bin/…`),
   with task 1 noting the paths will be scrubbed at commit time, or exclude BINARY-PROMOTION.md from
   the scan with a stated reason. Do not leave the resolution to the executor at a blocking
   checkpoint over irreversible history.
3. **34-05 t5:** Make the D-12 criterion falsifiable — require a *new numbered `999.x` heading*
   whose body contains the phrase (e.g. `rg -c '### Phase 999\.[0-9]+.*per-child' or a count strictly
   greater than the recorded pre-task count of 1), mirroring the before/after-count discipline the
   same plan already applies to `DEVFLOW_CLAUDE_LEGACY_LAUNCH`.
4. **34-03:** Anchor the classifier greps (`fn classify_validate_outcome\(`) in all four criteria
   that window off the function name; state that the `_ =>` ban is scoped to that window only.
   Declare the NC-4b temporary touch of `agent_result.rs` in the plan (a `files_touched_transiently`
   note or a files_modified entry with a "revert-verified" caveat), or gate it on worktree isolation.
5. **34-02 t2:** Specify the test's file names as `{nanos}-{seq}-stdout` explicitly, and consider
   asserting the eviction by stamp-group rather than file count so the test pins `prune_history`'s
   actual grouping semantics.
6. **34-02 t1:** Name `CanaryOutcome::Confirmed` directly and drop the nonexistent `Present`.

## 5. Risk Assessment

**MEDIUM.** The engineering core — the graft fix, the exhaustive-match rewrite, the 999.76 root
relocation, the sequencing topology, the negative-control discipline — survived every attack I could
mount with full repo tooling; the mechanisms are real, the citations exact, the pinned baselines
reproduce byte-for-byte, and the refuted-premise contamination the panel was asked to hunt is
absent. The residual risk is concentrated in the plans' *verification contracts*, not their designs:
one must_haves truth is false by the plan's own construction (34-04), one blocking privacy gate is
guaranteed to collide with a mandated artifact on this exact machine (34-05), and a handful of
acceptance criteria cannot produce the opposite result. All are cheap to fix before execution and
expensive after — the 34-05 collision sits at the phase's only irreversible boundary (public
history), and the 34-04 contradiction wedges the goal-backward verifier on the phase's riskiest
plan. Not established by this review: runtime behaviour of any specified edit (nothing was
executed beyond baseline selectors), the live capture campaign's outcomes, and whether a real agent
ever emits a self-contradictory marker — the plans correctly leave that last one open.

---

## Consensus Summary

Four real reviews (OpenCode, Antigravity, Hermes, Internal-Fable); three lanes dropped for
account-side reasons and carry no verdict weight. All four surviving voices had repo access and
cited file:line evidence; none carries the reviewed-without-repo-access marker.

### Agreed Strengths

- **Negative-control discipline** (all four): every plan pairs positive assertions with named
  opposite-result controls (NC-1…NC-8), and the 42-cell sweep exercises the layer dimension in
  both states.
- **The binding sequencing constraint is mechanically enforced** (all four): 34-04 cannot execute
  before 34-01 under the wave topology, and its precondition block covers out-of-band invocation.
  Internal-Fable explicitly failed to construct a counterexample.
- **Evidence-before-widening is real, not narrated** (OpenCode, Antigravity, Hermes): commit-time
  capture gate, per-stage recorded reasons, delivery-floor escalation.
- **Citation accuracy** (Internal-Fable, Antigravity, OpenCode): spot-checked greps and line
  numbers are exact at HEAD, including the planner-corrected counts.

### Agreed Concerns

1. **The rebuilt-binary resolution before the live run** (Antigravity MEDIUM, Internal-Fable
   verified live, Hermes suggestion 2): a shell resolving the stale v1.8.0 `~/.local/bin/devflow`
   or a pre-widening build silently produces a LEGACY capture. Task 2 mtime check exists; enforce
   it as a hard gate before task 4, and consider Hermes proposal of grepping the binary for the
   widened constant.
2. **The D-11 non-draining default under n=1 is ambiguous in 34-05 task 5** (OpenCode MEDIUM,
   Antigravity MEDIUM): both ask that the default be stated as leave-narrow, with widening on a
   pathological basis requiring explicit operator acknowledgement at the checkpoint.
3. **`agent_result.rs` is touched by three plans, one of them undeclared** (Hermes MEDIUM,
   Internal-Fable MEDIUM): 34-03 NC-4b temporary eighth-variant mutation touches a file it does
   not declare, colliding with the same-wave zero-overlap invariant if execution is not
   worktree-isolated.

### Divergent Views

- **Overall risk:** Antigravity LOW; OpenCode LOW (34-01/02/03) + MEDIUM (34-04/05); Hermes
  MEDIUM (coordination overhead); Internal-Fable MEDIUM (verification contracts, not designs).
- **The two HIGH findings are unique to the internal pass** — no external voice caught either,
  plausibly because both need machine-local verification (real binary paths containing $USER)
  or must_haves-versus-task-body cross-parsing:
  - **34-04 must_haves truth #4 self-contradiction** (fixture "still passes unmodified" vs task 1
    ordering its conversion-and-rename) — wedges goal-backward verification post-execution.
  - **34-05 PII gate vs BINARY-PROMOTION.md collision** (the mandated artifact must record paths
    the blocking $USER/$HOME scan then forbids) — unsatisfiable as written at the phase only
    irreversible boundary.
- OpenCode alone flags that a re-captured Code stage under a scratch repo is evidence about a
  different context than Phase 31 transcription (real dogfood phase) — a nuance worth recording
  in the capture summary rather than a defect.

### Actionable for --reviews replan (priority order)

1. Fix 34-04 must_haves truth #4 to describe the post-conversion state (Internal HIGH).
2. Resolve the 34-05 PII/BINARY-PROMOTION.md collision explicitly — scrub rule or scoped
   exclusion with stated reason (Internal HIGH).
3. Make the pre-satisfied acceptance criteria falsifiable: D-12 filing (count > recorded pre-task
   count), 34-02 t3 exec_command presence (Internal MEDIUM).
4. Declare or isolate 34-03 NC-4b transient mutation of agent_result.rs (Hermes + Internal MEDIUM).
5. Anchor the classifier greps (`fn classify_validate_outcome\(`) so they cannot match the new
   sweep test (Internal MEDIUM).
6. State the D-11 leave-narrow default in 34-05 task 5 (OpenCode + Antigravity MEDIUM).
7. Harden the pre-capture binary gate; add the claude-binary preflight to checkpoint question 1
   (Antigravity + Hermes + OpenCode).
8. LOW sweep: CanaryOutcome::Confirmed not Present; {nanos}-{seq}-stdout fixture names; six not
   five checkpoint tests; drop no-op git fetch; fix the 12-with-headroom wording; add 34-evidence/
   to 34-02 files_modified; rg -c counts lines not matches.

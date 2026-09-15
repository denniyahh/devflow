---
phase: 48
reviewers: [antigravity, codex, pi]
reviewed_at: 2026-09-15T10:50:46-04:00
plans_reviewed: [48-01-PLAN.md, 48-02-PLAN.md, 48-03-PLAN.md, 48-04-PLAN.md, 48-05-PLAN.md, 48-06-PLAN.md, 48-07-PLAN.md, 48-08-PLAN.md, 48-09-PLAN.md, 48-10-PLAN.md, 48-11-PLAN.md, 48-12-PLAN.md, 48-13-PLAN.md, 48-14-PLAN.md]
models:
  antigravity: "gemini-3.8-flash-high"
  codex: "gpt-5.6-terra (reasoning=high)"
  pi: "deepseek-v4-pro"
model_sources:
  antigravity: "command-line flag"
  codex: "command-line flag"
  pi: "command-line flag"
---

# Cross-AI Plan Review — Phase 48

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

**How this review was run**
- Run through the operator's `external-review` skill, not `/gsd-review`. That command's default reviewer set is mostly unavailable on this host.
- The reviewers read a `git archive` snapshot of commit `9915c56`, read-only, stored outside the repo.
- Raw reviewer output is in `~/.cache/devflow-review/phase48-plans-9915c56/review-codex.md` and `review-pi.md`.
- **Reviewer outcomes:** codex completed, with 19 `file:line` citations. pi completed, also with 19. antigravity was **dropped**: `print timeout after 45m0s with turn in progress` and 0 bytes of output. A dropped reviewer is not a pass, so this counts as a two-reviewer review.
- **Checking:** the orchestrator checked every finding below against the snapshot or the worktree source.
- **This file differs from the template:** each reviewer section below records the checked findings rather than the raw reviewer text.

**Verdicts used below**
- **CONFIRMED:** holds as stated.
- **CONFIRMED-DOWNGRADED:** holds, but at a lower severity than the reviewer gave.
- **PARTLY:** part of the finding holds.
- **OUT-OF-SCOPE:** already deferred.
- **UNVERIFIED:** needs a clippy run to settle.

Severity in each heading is the orchestrator's; the reviewer's own rating follows it where they differ.

## Antigravity Review

**Dropped.** No output: the print timeout fired at 45 minutes while the turn was still in progress. No findings recorded, and this is not a pass.

---

## Codex Review

### CX-1 [HIGH, reviewer: BLOCKER] `recover --clean` checks the lock, then deletes without holding it — CONFIRMED-DOWNGRADED
- **Plan:** 48-14 Task 1 (`48-14-PLAN.md:84`)
- **Checked:** the action computes `lock::holder_status`, then calls `Gates::cleanup` for every stage. It never takes the per-phase lock, and `crates/devflow-core/src/recover.rs:130-145` takes none today.
- **What goes wrong:** a `start` can take the lock and write a gate request between that check and the cleanup. Its live gate is then deleted, stranding the waiter. That is the outcome amendment R-1 (`48-CONTEXT.md:305-310`) exists to prevent.
- **Fix direction:** hold the per-phase lock across both the status decision and the cleanup, and treat lock contention as `Live`.

### CX-2 [MEDIUM, reviewer: HIGH] The fence closer accepts lines that are not closing fences — CONFIRMED-DOWNGRADED
- **Plan:** 48-02 Task 2 (`48-02-PLAN.md:127`)
- **Checked:** a fence "closes only on a line of the same character with a run at least as long". Nothing requires the rest of that line to be blank.
- **What goes wrong:** a line with the same character run plus trailing text, such as an info string, closes the fence early. A later task tag inside the example is then read as a real declaration. That false positive blocks preflight or arms resume.
- **Fix direction:** a closing fence is at most 3 spaces of indentation, the run, then only whitespace. Add a test for the opposite case.

### CX-3 [HIGH] A fenced `</task>` line cuts short the recorded checkpoint element — CONFIRMED
- **Plans:** 48-02 Task 3 (`48-02-PLAN.md:157`), used by 48-07 (`48-07-PLAN.md:112-116`)
- **Checked:** `element` ends at "the first line whose trimmed content starts with the closing task tag", with no fence tracking.
- **What goes wrong:** when a task contains a fenced example starting with `</task>`, its recorded element is cut short. Later edits below that line fall outside what 48-07 compares, so a changed checkpoint passes the re-scan without a human. That bypasses CHKPT-02.
- **Fix direction:** extract elements with the same fence tracking used for declarations, and add a regression test with a fenced `</task>`.

### CX-4 [MEDIUM, reviewer: HIGH] Temp file names can collide after pid reuse — PARTLY
- **Plan:** 48-10 Tasks 1-2 (`48-10-PLAN.md:84`, `:110`)
- **Checked:** names follow `.{name}.{pid}.{seq}.tmp`, where `seq` is a per-process `AtomicU64` starting at 0, and files are opened with `File::create_new`. No retry is specified.
- **What goes wrong:** a crashed writer can leave an orphan with pid P and seq 0. A later process reusing pid P then fails its first write with `AlreadyExists`, until a sweep (`clear_state` or `Gates::cleanup`) removes the orphan. The failure is visible rather than a silent loss, and it needs both a crash and pid reuse.
- **Fix direction:** on `AlreadyExists`, retry with the next sequence number or add a random nonce. Test with a planted orphan carrying the same pid.

### CX-5 [LOW, reviewer: HIGH] An answer can be published after the waiter exits — PARTLY
- **Plan:** 48-13 Task 1 (`48-13-PLAN.md:107-109`)
- **Checked:** the status check and the publication are separate operations, so a narrow window remains. The stale-answer consequence is already covered: a fresh `start` deletes leftover gate files after taking its lock, and a test plants a leftover abort rejection (`48-12-PLAN.md:24`, `:103`).
- **What is left:** a message that can be briefly wrong.
- **Fix direction:** optional. Either accept and document the window, or re-check the lock after publishing.

### CX-6 [LOW, reviewer: MEDIUM] A future test can sidestep the PATH lint — CONFIRMED-DOWNGRADED
- **Plan:** 48-09 Task 1 (`48-09-PLAN.md:102-108`)
- **Checked:** `#[expect(clippy::disallowed_methods)]` is allowed at mutation sites for variables other than PATH, and the PATH gate searches only for the literal text `set_var("PATH"` and `remove_var("PATH"`.
- **What goes wrong:** a later expected mutation whose key resolves to `PATH` without being the literal passes both checks. This is a gap against future regressions, not a defect in executing the plan.

### CX-S1 Durability across power loss (fsync) — OUT-OF-SCOPE
- Already listed as a deferred idea (`48-CONTEXT.md:451`, "fsync of file plus directory").

---

## Pi Review

### PI-1 [MEDIUM] `doctor` would write to disk through `lock::holder` — CONFIRMED (citation corrected)
- **Plans:** 48-13 Task 1 (`48-13-PLAN.md:107`) and 48-14 Task 2 (`48-14-PLAN.md:111`; must-have at `:26`)
- **Checked:** `holder_status` is built on `lock::holder`, and `holder` deletes an empty lock file (`crates/devflow-core/src/lock.rs:198-211`, `remove_file` at `:207`). The reviewer cited `:228-236`, which is `LockGuard`'s `Drop`.
- **What goes wrong:** 48-14 calls `holder_status` from doctor's `build_phase_facts`, while its must-have states "doctor performs no write" (C-13). Doctor would delete a stale empty lock file.
- **Fix direction:** give `holder_status` a read-only path (`holder_identity` plus `agent_running` and `process_start_time`), or add a variant of `holder` that does not delete, for doctor to use.

### PI-2 [LOW] 48-02 Task 3 points the executor at example plans that do not exist — CONFIRMED
- **Plan:** 48-02 Task 3 (`48-02-PLAN.md:148`)
- **Checked:** `.planning/phases/19-*/19-05-PLAN.md`, `44-*/44-04-PLAN.md` and `15-*/15-05-PLAN.md` are missing. The files are under `.planning/milestones/v2.0.0-phases/` and `v2.8.0-phases/`.
- **Mitigation:** the same block includes a fallback `rg` over `.planning`, so an executor could still find them.

### PI-3 [LOW] 48-11 changes `advance`'s signature without saying what its test callers should pass — CONFIRMED
- **Plan:** 48-11 Tasks 1-2
- **Checked:** `crates/devflow-cli/src/pipeline_launch.rs` has 9 two-argument `advance(root, Some(…))` calls. 48-11 names none of them and does not say which stage each should pass.
- **What goes wrong:** passing `None` would exercise the unbound path instead of the one those tests cover today.

### PI-S1 Clippy coverage of aliased imports — UNVERIFIED
- **Plan:** 48-09 Task 1
- **Status:** research probed only the fully qualified form and the `use std::env::set_var` form. The `use std::env; env::set_var` form and `as` aliases are untested, and settling this needs a clippy probe. It overlaps CX-6.

### PI-S2 [LOW-MEDIUM] `recover --clean` still clears state under a live lock holder — CONFIRMED
- **Plan:** 48-14 Task 1 (`48-14-PLAN.md:84`: "Keep the existing … state clear …")
- **Checked:** only gate files are guarded. `workflow::clear_state` still runs while a live process holds the lock.
- **Undecided:** the plans don't say whether this is the intended explicit escape hatch (`recover.rs` doc: "warns (but proceeds)") or a gap against 48-12's "every production state writer holds the per-phase lock".

---

## Consensus Summary

Two source-grounded reviewers completed: codex and pi, each with 19 `file:line` citations. antigravity was dropped at its 45-minute timeout.

### Agreed Strengths
- Not established. Only codex's CHECKED AND CLEAN section was read during synthesis, so no strength can be attributed to both reviewers.

### Agreed Concerns
- **Highest priority: `recover --clean` under a live lock holder (48-14 Task 1).** CX-1 (gate cleanup races a new `start`) and PI-S2 (state is cleared regardless of the holder) hit the same function from two sides. Resolve them together by deciding what `recover --clean` may delete while a process holds, or could take, the per-phase lock.
- **The PATH lint is weaker than 48-09 claims.** CX-6 and PI-S1.

### Divergent Views
- **Findings from one reviewer only** (each checked above):
  - codex: CX-2, CX-3, CX-4, CX-5. CX-3 is the most severe of these, because it bypasses CHKPT-02.
  - pi: PI-1, PI-2, PI-3.
- **Severity:** codex rated CX-1 a blocker and CX-2, CX-4 and CX-5 high. After checking, CX-2 and CX-4 are medium, and CX-5 is low because 48-12 already covers most of it.

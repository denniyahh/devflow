# Phase 22: Light Dogfooding Trial - Context

**Gathered:** 2026-07-24
**Status:** Ready for a narrow Codex-run dogfooding session

<domain>
## Trial Boundary

This is a controlled dogfooding run, not the full "Concurrency & Governance
Correctness" phase. It addresses only backlog **999.30 / DEN-55**, the four
advisory findings from Phase 21's post-implementation code review.

The session runs with Codex through **Validate** and stops before Ship. Fix
defects encountered during the run when they are within this boundary; record
and defer anything outside it.

**In scope:**

1. Share the omitted-stage gate resolution used by `gate_show` and
   `gate_respond`, so their behavior cannot drift.
2. Replace `collect_planning_doc_findings`' hardcoded `"main"` with
   `devflow_core::config::MAIN`.
3. Make `gate_show` read open gates once, eliminating its redundant read and
   narrow time-of-check/time-of-use window.
4. Remove `status`'s per-phase `events.jsonl` rescan by extending the existing
   single-pass event summary with the most recent `stage_launched` timestamp.

**Explicitly out of scope:** 999.28 (`--base`), 999.4 (concurrent version-tag
contention), 999.26 (parallel object-store race), and all unrelated backlog
items. Do not expand this trial into a full Phase 22 concurrency design.
</domain>

<decisions>
## Constraints

- Preserve existing CLI output and gate semantics except for the intended
  consistency and TOCTOU fixes.
- Reuse existing helpers and types; add no dependencies.
- Add focused regression coverage for each changed behavior.
- Validate with formatting, clippy, relevant tests, and the workspace test
  suite before stopping.
- Stop after Validate. Shipping, release operations, and remote pushes are not
  part of this trial.
</decisions>

<canonical_refs>
## Required References

- `.planning/phases/21-operator-usability-release-execution/21-REVIEW.md` -
  authoritative descriptions and proposed fixes for WR-01, WR-02, WR-03, and
  IN-01.
- `.planning/ROADMAP.md` - backlog 999.30 / DEN-55 scope and priority.
- `crates/devflow-cli/src/commands.rs` - the affected command and status
  paths.
- `crates/devflow-cli/src/events.rs` - the existing single-pass event-summary
  implementation to extend rather than duplicate.
</canonical_refs>

# Phase 48 targeted adversarial re-review

Review the implementation at `b69a9ab` against its immediate predecessor `9a7cc61`.

This is a focused recovery review after two independent external reviewers found the following defects in the earlier implementation:

1. A Supervise-mode checkpoint re-scan with an `advance` response relaunched Claude without another human gate.
2. `devflow stop` could write an abort response at a non-Ship gate with no confirmably live holder, then tell the operator to resume.
3. A stale gate could receive a response while a live holder was running a different stage.
4. The emitted event policy still claimed the old D-03 behavior.
5. The gate-sweep dry-run test had no positive control proving it could identify a reappable live holder.
6. The security register was stale. Documentation reconciliation is deliberately outside this source-diff review.

Changed code and tests are principally:

- `crates/devflow-cli/src/pipeline_launch.rs`
- `crates/devflow-cli/src/commands.rs`
- `crates/devflow-cli/tests/stop_e2e.rs`
- `crates/devflow-cli/tests/gate_wedge_e2e.rs`

Attack the patch, not its intent. In particular, trace the complete stop/gate/lock state machine and the checkpoint re-scan path for mode-specific behavior, stale state, process-identity races, and operator-facing claims that exceed actual evidence. Check each listed prior defect yourself. Report only reproducible issues as `CONFIRMED`, with exact `file:line` evidence and a concrete reproduction path. Put design concerns that cannot be demonstrated from the current code under `SUSPECTED`. If a prior defect is actually repaired, say so briefly and cite the exact code/test that proves only that bounded claim.

Do not modify files. The review root is this Phase 48 worktree.

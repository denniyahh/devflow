# Phase 49 Adversarial Review Summary

**Review root:** `/var/home/denniyahh/Github/devflow/.worktrees/phase-49`

**Reviewed commit:** `0cf90204a6cc16d4112dd6f9ef15e8cb25af85c2`

**Verdict at reviewed commit:** BLOCK EXECUTION — resolve the confirmed critical/high findings before a live run.
The subsequent working-tree revision has not received an independent checker or adversarial re-review.

## Lanes

- **Codex:** completed at high effort; raw report: `codex.md`.
- **Antigravity:** its report body arrived after this summary was first written; `antigravity.md` now contains five findings.
- **Claude:** not run. The external-runtime request was rejected because sending internal planning documents to Claude was a new destination not covered by the approved Codex/Antigravity review scope.

This is two completed adversarial lanes, not a convergence verdict: the plan revision that followed them has not had
an independent second pass.

## Actionable Findings

1. **Critical — Plan 49-12 requires `scripts/check-in-container.sh all` from linked worktree `phase-49.1`, while backlog 999.58 records that this command fails deterministically from linked worktrees.** Resolve the execution location or the backlog issue before retaining the direct container gate.
2. **High — the retry path does not clear the live `state-49.1.json` after archiving, so the next attempt's precondition prevents relaunch.**
3. **High — archive ref creation precedes the copy/manifest work and rejects retries when partial archival leaves that ref behind.**
4. **High — `KillMode=process` preserves descendants, but the cleanup plan only stops units and does not prove descendant reaping.**
5. **High — the final `workspace/denniyahh == base_sha` assertion is missing immediately before Ship-preflight approval.**
6. **High — the public PR secret scan omits `ANTHROPIC_TOKEN`, despite the research register naming it.**
7. **High — `extensions.worktreeConfig` is a repository-wide Git mutation, contradicting the plan's worktree-only claim.**
8. **High — `record-secure` may accept and later revert unrelated `.planning/` commits.**
9. **High — five-second polling can miss a gate that opens and closes between polls.**
10. **Medium — gate watcher and operator tool concurrently append to `ledger.jsonl` without a serialization contract.**
11. **Medium — C5/SURV-01 may be reported despite an overflow or unclean watcher stop.**
12. **Medium — evidence sanitization is manual and does not check UUID/session redaction, regular-file provenance, or per-verdict evidence.**
13. **Medium — the pinned binary is not explicitly asserted to match the recorded fork SHA.**
14. **Medium — mutable phase-worktree tools judge evidence while watcher tools are immutable copies under the attempt directory.**

See `codex.md` for evidence and non-binding remediation hints.

Antigravity also flagged retry cleanup, narrow secret patterns, the Ship pre-push container-hook accommodation,
evidence/verdict consistency, and backlog-number collisions. The subsequent plan revision assigns these to
49-08, 49-16, the explicit 49-15 limits, the 49-15 verdict gate, and the 49-15 numbering rule respectively. These
are plan changes, not evidence that the live commands work; see `antigravity.md` for the original findings.

## Limits

The completed review is static. It does not establish live Claude behavior, resume behavior, successful Ship/PR flow, inotify behavior under load, container behavior in this exact run, or post-merge CI reliability.

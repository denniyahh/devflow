---
status: partial
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
source: [45-01-SUMMARY.md, 45-02-SUMMARY.md, 45-03-SUMMARY.md]
started: 2026-09-02T22:58:11Z
updated: 2026-09-02T23:02:00Z
---

## Current Test

[testing paused — 1 item deferred to a later phase]

## Tests

### 1. Live unattended launch — configured-base fork + preflight
expected: |
  base_branch = "workspace/denniyahh" configured (devflow.toml or DEVFLOW_BASE_BRANCH).
  `devflow start --phase N --mode auto` on this repo:
  - worktree .worktrees/phase-NN forked from workspace/denniyahh, NOT develop
  - worktree carries .planning/config.json
  - preflight unattended_config_condition reports "Holds"
  - launch proceeds unattended, no operator prompt
  - (if run reaches Ship) merge target is workspace/denniyahh
result: skipped
reason: "Deferred follow-up: this is most likely not possible yet, will defer this to a later phase"

### 2. Spikes-only dirty tree does not block self-dogfood (AUTO-02)
expected: A tracked change confined to .planning/spikes/ no longer hard-blocks DevFlow's self-dogfood staleness check, while a change to a crates/* member source still does.
result: pass
source: automated
coverage_id: D1

### 3. Scoped/unscoped path classification preserved (AUTO-02)
expected: Workspace-scoped and unscoped path classification both preserve root build files, member paths, prefix boundaries, and downstream-project behavior.
result: pass
source: automated
coverage_id: D2

### 4. Full-execute Code renderers deliver shared unattended policy (DECN-01)
expected: Both full-execute Code prompt renderers carry the byte-identical CODE_STAGE_POLICY; prompts that must not carry it (GapsOnly/AuditFix) do not. (Claude/OpenCode loop-back gap deferred by operator decision to backlog 999.115/999.116.)
result: pass
source: automated
coverage_id: D1

### 5. Delivered policy content is correct (DECN-01)
expected: The delivered policy forbids positional option selection, requires the decision reasoning to be recorded, and excludes blocking-human and package-verification checkpoints from self-resolution.
result: pass
source: automated
coverage_id: D2

## Summary

total: 5
passed: 4
issues: 0
pending: 0
skipped: 1
blocked: 0

## Gaps

[none yet]

## Deferred Follow-Ups

- test: 1
  idea: "this is most likely not possible yet, will defer this to a later phase"
  deferred_at: 2026-09-02

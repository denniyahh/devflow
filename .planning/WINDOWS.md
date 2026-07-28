---
schema_version: 1
open_count: 2
waived_count: 0
fixed_count: 0
total_count: 2
last_updated: 2026-07-28T17:44:55.619Z
---

# Broken Windows Ledger

> Cross-phase defect register. `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 25 | deviation | crates/devflow-cli/src/preflight.rs |  | WR-03 reap fix not wired into two additional launch-driving tests found by 25-16's enumeration (run_preflight_advance_gate_launches_agent_exactly_once, run_preflight_loopback_gate_launches_agent_exactly_once); out of scope for 25-16 (owned by 25-15 in same wave) | open |  | 2026-07-28T17:44:48.000Z |  |
| 2 | 25 | unmet-truth | .planning/phases/25-end-to-end-dogfood-blockers/25-16-SUMMARY.md |  | 25-16 acceptance criterion 'before-delta must be at least 1' not met: every leak measurement (isolated test x6, whole-workspace x1, unfixed and fixed tree) showed delta 0 on this machine/run; fix implemented regardless per review's defensive-teardown reasoning | open |  | 2026-07-28T17:44:55.619Z |  |

````json
[
  {
    "id": 1,
    "kind": "deviation",
    "phase": "25",
    "file": "crates/devflow-cli/src/preflight.rs",
    "line": null,
    "description": "WR-03 reap fix not wired into two additional launch-driving tests found by 25-16's enumeration (run_preflight_advance_gate_launches_agent_exactly_once, run_preflight_loopback_gate_launches_agent_exactly_once); out of scope for 25-16 (owned by 25-15 in same wave)",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-07-28T17:44:48.000Z",
    "resolved_at": null
  },
  {
    "id": 2,
    "kind": "unmet-truth",
    "phase": "25",
    "file": ".planning/phases/25-end-to-end-dogfood-blockers/25-16-SUMMARY.md",
    "line": null,
    "description": "25-16 acceptance criterion 'before-delta must be at least 1' not met: every leak measurement (isolated test x6, whole-workspace x1, unfixed and fixed tree) showed delta 0 on this machine/run; fix implemented regardless per review's defensive-teardown reasoning",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-07-28T17:44:55.619Z",
    "resolved_at": null
  }
]
````

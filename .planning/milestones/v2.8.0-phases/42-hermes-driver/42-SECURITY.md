---
phase: 42
slug: hermes-driver
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-21
---

# Phase 42 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Register authored at plan time (both PLAN files carry a `<threat_model>` block).

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| DevFlow process monitor → Hermes CLI | Spawning `hermes -z` with `--yolo` and `--accept-hooks` executes commands non-interactively | prompt text via argv array (`-z <prompt>`) |
| Hermes process stdout → `parse_marker_lines` | Stdout parsing must strictly validate DEVFLOW_RESULT JSON format before advancing commit-gated stages | stdout text / JSON payload |
| Preflight C2 Gate → Headless Launch | Unlocking `--mode auto` allows unattended runs for Antigravity; requires proof of reliable event stream handling | state configuration (`State`) |
| Operator → Validate Gate | Supervise mode ensures human verification before committing phase completion | gate decision (Advance / LoopBack) |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-42-01 | Elevation of Privilege | CLI spawn argv | high | mitigate | Pass `--accept-hooks` and `HERMES_ACCEPT_HOOKS=1` to prevent interactive prompt hang; prompt is passed via argv, not shell interpolation | closed |
| T-42-02 | Tampering | Completion detection | high | mitigate | Process-exit parser requires explicit DEVFLOW_RESULT marker on commit-gated stages; marker-less runs fail closed | closed |
| T-42-03 | Denial of Service | Hung process handling | medium | mitigate | Monitor tracks agent PID and liveness; integration test verifies gate hold and clean cleanup upon kill | closed |
| T-42-04 | Repudiation | Conformance suite | low | mitigate | Enroll HermesDriver in conformance suite to verify 7 trait contract checks | closed |
| T-42-05 | Information Disclosure | Doctor presence check | low | mitigate | Doctor probe checks version string and existence without executing user prompts | closed |
| T-42-06 | Denial of Service | Subagent probe failure | low | mitigate | `hermes_subagent_dispatch_available` fails closed to `false` on any probe failure without halting launch | closed |
| T-42-07 | Denial of Service | Idle timeout misconfiguration | medium | mitigate | Quiet-gap distribution observed during supervised dogfood; `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS=300` configured to accommodate compilation passes | closed |
| T-42-08 | Elevation of Privilege | Premature auto-mode unlock | high | mitigate | Gated unattended mode behind successful completion and validation of supervised dogfood run (ANTG-04) | closed |

*Status: open · closed*  
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on (high) count toward threats_open*  
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

No accepted risks.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-21 | 8 | 8 | 0 | gsd-security-auditor (inline) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-21

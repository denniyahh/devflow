---
phase: 37
slug: modular-agent-driver-architecture-pi-driver-999-31-pi
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-16
---

# Phase 37 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Register authored at plan time (all four PLAN files carry a `<threat_model>` block).

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| `StageIntent` → per-driver `render_prompt` | Intent data (phase, fix kind, review angles) → agent instruction | prompt text |
| Driver `build_command` → agent subprocess | Program + argv built from the prompt | prompt + sandbox config |
| `PiDriver::health` → `pi auth check` child | Credential-readiness verdict | no secret (stdout discarded from the error string) |
| `CodexDriver` → `codex -a never exec` child | Non-interactive approval policy + writable-roots TOML | sandbox config |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-37-01 | Repudiation | `prompt.rs` `render_workflow_style` | high | mitigate | negative-control test over the seven slash-command names; `workflow_render_preserves_stage_contracts` | closed |
| T-37-02 | Tampering | Claude/OpenCode prompt drift | high | mitigate | byte-equality snapshot (`claude_and_opencode_stay_identical_but_codex_renders_native`) | closed |
| T-37-03 | Repudiation | shared-prompt invariant | medium | mitigate | invariant deleted (grep-clean), replaced by the equivalence test | closed |
| T-37-04 | Tampering | Claude argv/prompt drift | high | mitigate | `drivers_reproduce_legacy_adapter_behavior` (stream-json argv + render byte-equality) | closed |
| T-37-05 | Tampering | Claude default `PipeOwning` routing / `--legacy-claude-launch` opt-out | high | mitigate | routing preserved via the shim; launch-shape tests still pass | closed |
| T-37-06 | Tampering | `DriverCapabilities` made exhaustive | medium | mitigate | `#[non_exhaustive]` + `Default` derive | closed |
| T-37-07 | Spoofing | Codex parsing relocation changes marker evaluation | high | mitigate | `CodexDriver::parse_completion` delegates to the unchanged `parse_codex_event_result`; existing evaluation tests green | closed |
| T-37-08 | Elevation of Privilege | non-interactive approval flag mis-wired | medium | mitigate | `-a never` BEFORE `exec` (spawn-tested against the installed CLI) | closed |
| T-37-09 | Information disclosure | Pi health regresses to env-var sniffing | medium | mitigate | `classify_auth_check` + `pi auth check` stub tests; `--no-refresh` added | closed |
| T-37-10 | Spoofing | driver reports `HeadlessSafe` for an interactive stage | high | mitigate | `interactivity_mode` declared (Codex Define/Plan → `RequiresExistingArtifact`) + `codex_define_and_plan_require_an_existing_artifact`; consumption deferred → 999.106 | closed |
| T-37-11 | Repudiation | `AgentAdapter` removed while still referenced | high | mitigate | removal deferred with call sites enumerated → 999.106 (grep-gated) | closed |
| T-37-12 | Repudiation | docs still claim "same prompt for all agents" | medium | mitigate | four docs grep-clean | closed |

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
| 2026-08-16 | 12 | 12 | 0 | gsd-security-auditor (inline) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-16

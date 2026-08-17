---
phase: 36
slug: pi-agent-support
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-15
---

# Phase 36 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Register built **retroactive-STRIDE** — the two PLAN files carried no `<threat_model>`
> block, so the register below was constructed from the implementation files.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| `PiAgent::exec_command` → `pi` child | The DevFlow-generated prompt crosses into a `pi -p` subprocess | prompt text (may embed CLI/user-derived content) |
| `PiAgent::preflight` → `pi auth check` child | The health check reads Pi's own credential-verb output | credential-readiness verdict (no secret) |
| `scripts/cut-release.sh step_tag` → `git tag -s` | `devflow.releaseSigningKey` config value becomes the signing identity | maintainer key path |
| `scripts/hooks/pre-push` → pushed tag | A pushed tag's fingerprint is compared to the configured maintainer key | tag signature fingerprint |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-36-01 | Elevation of Privilege | `crates/devflow-core/src/agents/pi.rs` (`exec_command`) | high | mitigate | `--no-approve` is unconditional — `--approve` would trust project-local extensions/skills that execute **unsandboxed** on a fresh per-phase worktree | closed |
| T-36-02 | Spoofing (false-green credential check) | `crates/devflow-core/src/agents/pi.rs` (`preflight`) | medium | mitigate | health check shells out to `pi auth check` (Pi's authoritative verb), not env-var sniffing — `DEVFLOW_PI_PROVIDER` is a provider *name*, not a credential | closed |
| T-36-03 | Tampering (wrong-identity release signature) | `scripts/cut-release.sh` (`step_tag`) | high | mitigate | fail-loudly guard on unset/unreadable `devflow.releaseSigningKey` **before** `tag -s`; deterministic `git -c user.signingkey=` override | closed |
| T-36-04 | Repudiation (capability-only signing probe) | `crates/devflow-core/src/git.rs` + `crates/devflow-cli/src/commands.rs` | medium | mitigate | removed the `check_signing_viability` cluster — it answered "can this key sign", never "is this the maintainer's key"; clippy-clean | closed |
| T-36-05 | Spoofing (agent key vs maintainer key) | `scripts/hooks/pre-push` | high | mitigate | **retained** the fingerprint comparison — the only check distinguishing the agent key from the maintainer key (both share `user.email`) on the hand-cut release path | closed |
| T-36-06 | Injection (leading-dash prompt parsed as a flag) | `crates/devflow-core/src/agents/pi.rs` (`exec_command`) | low | accept | Pi rejects `--` end-of-options (`Error: Unknown option: --`), so the positional prompt cannot be guarded with `--`; see Accepted Risks Log | closed |

*Status: open · closed*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on (high) count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| R-36-01 | T-36-06 | Pi has no `--` end-of-options convention (`pi -p … -- "prompt"` → `Unknown option: --`), so the leading-dash prompt hazard cannot be guarded with `--`. Out of scope for Phase 36 — the SPEC boundary excludes an end-to-end `devflow start --agent pi` run, and DevFlow's own stage prompts never begin with `-`. Phase 37 (prompt de-Claude-ification + the `AgentDriver` trust decision) owns the durable fix. Severity low, below the `high` block threshold. | gsd-security-auditor | 2026-08-15 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-15 | 6 | 6 | 0 | gsd-security-auditor (retroactive-STRIDE) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-15

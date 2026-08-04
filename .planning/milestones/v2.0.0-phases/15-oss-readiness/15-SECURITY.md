---
phase: 15
slug: oss-readiness
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-07-17
---

# Phase 15 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| doc reader / incident responder → repo docs | Responder trusts SECURITY.md to name the real sensitive artifacts during an incident | Artifact paths (state-NN.json, events.jsonl) |
| contributor / adapter author → ARCHITECTURE.md | Contributors trust the design doc as the authoritative map of the state machine and extension model | Architectural claims |
| upstream container registry → devcontainer build | Base image fetched from mcr.microsoft.com; an unpinned tag lets contents change under the project | Container image contents |
| external GitHub Action → CI runner | devcontainers/ci action executes in CI with repo checkout | Repo contents, CI credentials |
| downstream user / crates.io page → declared license | Users rely on the SPDX license string being backed by actual license texts | License claims |
| local packaging tooling → Cargo metadata | `cargo package` reads workspace metadata to assemble the publishable artifact | Crate contents |
| operator credential → publish tooling | crates.io API token authorizes publishing; must never enter the repo or logs | Registry API token |
| local build artifact → public crates.io registry | Published contents become permanently public and immutable per version | Published crate source |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-15-01 | Information Disclosure | SECURITY.md Best-Practices bullet | medium | mitigate | SECURITY.md names real `state-NN.json` + `events.jsonl` (verified: SECURITY.md:37); no phantom audit-log pointer | closed |
| T-15-01b | Tampering | README/DEPENDENCIES command claims | low | mitigate | Command/flag/version claims traced to source; `help_snapshot.rs` + snapshots guard CLI-vs-docs drift | closed |
| T-15-02 | Tampering | ARCHITECTURE.md / guides factual claims | low | mitigate | Claims re-derived from named source files; grep verified zero `.devflow.yaml`/`confirm`/`rejectpr` refs remain | closed |
| T-15-02b | Information Disclosure | configuration.md env-var docs | low | accept | Documents env-var names only (expected operator guidance); no credential values in docs | closed |
| T-15-03-SC | Tampering | .devcontainer base image (supply chain, ASVS V14) | high | mitigate | Image pinned to `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` (explicit version, not `:latest`) | closed |
| T-15-03b | Tampering | devcontainers CI action reference | medium | mitigate | `devcontainers/ci@v0.3`, `actions/checkout@v4` — explicit version pins, no floating refs | closed |
| T-15-03c | Elevation of Privilege | postCreateCommand contents | low | accept | postCreate is `rustup component add clippy rustfmt && cargo build --workspace` only — no privileged or network-fetching custom scripts | closed |
| T-15-04 | Repudiation | Cargo.toml license claim vs on-disk files (ASVS V14) | medium | mitigate | LICENSE and LICENSE-APACHE both on disk; LICENSE-APACHE verified canonical Apache-2.0 text | closed |
| T-15-04b | Tampering | packaged crate contents | low | mitigate | `cargo package --workspace` re-assembled and built from packaged form (proven-green baseline, 15-04-SUMMARY) | closed |
| T-15-05-SC | Information Disclosure | crates.io API token handling (ASVS V14) | high | mitigate | Zero `CARGO_REGISTRY_TOKEN`/`credentials.toml` references outside docs (grep-verified); operator authenticated via `cargo login` locally; token never committed, echoed, or logged | closed |
| T-15-05 | Tampering | publish order (core before CLI) | medium | mitigate | Leaf-first order verified: devflow-core published 17:39:23 UTC, devflow 17:40:31 UTC; both resolve from clean registry queries (15-05-SUMMARY) | closed |
| T-15-05b | Repudiation | irreversible version publish | medium | accept | Operator personally executed both `cargo publish` invocations after two automated false-positive attempts — explicit human confirmation exceeded the planned gate | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-15-01 | T-15-02b | Documenting real env-var names (no secret values) is required operator guidance; withholding them harms operability more than naming them risks disclosure | Plan 15-02 threat model (operator-approved plan) | 2026-07-17 |
| AR-15-02 | T-15-03c | postCreateCommand limited to official rustup components + workspace build; no third-party scripts, no privilege escalation surface | Plan 15-03 threat model (operator-approved plan) | 2026-07-17 |
| AR-15-03 | T-15-05b | crates.io versions are immutable by design; irreversibility is inherent to publishing. Operator performed the publish manually, making the human confirmation stronger than the planned automated gate | Operator (manual publish, 2026-07-17) | 2026-07-17 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-17 | 12 | 12 | 0 | gsd-secure-phase (State B, L1 grep-depth, short-circuit — plan-time register, no open threats) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-17

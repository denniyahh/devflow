---
phase: 21
slug: operator-usability-release-execution
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-07-23
---

# Phase 21 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Register authored at plan time (all 4 PLANs carried `<threat_model>` blocks); verified at ASVS L1 (grep-depth), `block_on: high`. Highest severity in this phase is `medium`, so no threat is at or above the blocking threshold. All units are read-only / detection-only / additive presentation with no irreversible side effects.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| git subprocess → devflow | `git diff --name-only` / `merge-base` / `rev-parse` output parsed into staleness verdicts and tag reachability | commit SHAs, file paths (git-provenance, not free-form) |
| agent-controlled text → terminal | gate `context` and `retry_after` originate from agent output and are printed to the operator's terminal | untrusted agent text |
| on-disk state / gates / cron files → status display | `status` / `gate show` read state, gate, and cron-instruction files | read-only reads |
| ROADMAP.md / STATE.md text → doctor | operator-authored markdown tables parsed into version claims | version strings |
| doctor finding → operator trust | a finding must not silently mutate the docs it reports on | detection-only output |
| sequentagent → `.devflow/phase-NN-sequentagent` | per-phase slot record persisted to the shared `.devflow/` dir (advertised safe to tail) | slot letter + agent kind (path/username-free) |
| pid → OS | `agent_running(pid)` probes an OS process for liveness | pid |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-21d-01 | Tampering | `ancestry_range_affects_build` git shelling | low | mitigate | `Command::new("git").args([...])` argv array (mirrors `run_git_stdout`); no `sh -c`. Verified: staleness.rs uses argv, no shell string. | closed |
| T-21d-02 | DoS (local) | git error mid-check | low | mitigate | Fails toward `Stale` on any git error (safe), never false-`Fresh`; `Indeterminate`-never-blocks fallbacks intact. Verified by test `git_error_range_fails_toward_stale`. | closed |
| T-21d-03 | Information Disclosure | staleness block event payload | low | mitigate | No new production `events::emit` added in staleness path (git-diff confirmed); block message keeps path in `Err` only, event payload stays `{stage, reason, worktree}`. | closed |
| T-21d-SC | Tampering (supply chain) | npm/pip/cargo installs | low | accept | No dependency changes — `git diff` confirms no `Cargo.toml`/`Cargo.lock` change in phase 21. | closed |
| T-21a-01 | Tampering (terminal injection) | `gate_show` printing `OpenGate.context` | medium | mitigate | Full context routed through `render_gate_context(&gate.context, usize::MAX)` (sanitize control chars, never truncate). Verified at `commands.rs:868`. | closed |
| T-21a-02 | Tampering (terminal injection) | `retry_after` in cron hint | low | mitigate | `retry_after` sanitized through `render_gate_context` before printing. | closed |
| T-21a-03 | Information Disclosure | new `status` lines | low | mitigate | Progress/recovery lines emit only phase numbers, stage names, ages, static verbs; no new production `events.jsonl` writes (only new `events::emit` is test fixture setup). | closed |
| T-21a-04 | Elevation / behavior drift | additive-only guarantee | low | mitigate | Display paths are read-only; no `save_state`/`transition`/`Gates::respond` introduced (grep-asserted, verifier confirmed 21/21). | closed |
| T-21a-SC | Tampering (supply chain) | npm/pip/cargo installs | low | accept | No dependency changes (Cargo.toml/lock unchanged). | closed |
| T-21b-01 | Tampering (of trust) | planning-doc check | medium | mitigate | Detection-only by construction: no write path to ROADMAP.md/STATE.md in the new functions; new `fs::write` calls are all test fixtures (a.txt/side.txt/pid), never the planning docs. Verifier grep-confirmed. | closed |
| T-21b-02 | Tampering | git-tag shelling | low | mitigate | `tag_exists_and_reachable` uses `Command::new("git").args([...])` argv; tag strings from validated `^v?\d+\.\d+\.\d+$` cells; no `sh -c`. | closed |
| T-21b-03 | DoS (local) | markdown row parsing | low | mitigate | Rows not matching the expected shape are skipped, never a panic (degrade-don't-die). | closed |
| T-21b-04 | Information Disclosure | finding `detail` strings | low | mitigate | `detail` carries only phase labels, version strings, tag names — no paths/usernames; no new events writes. | closed |
| T-21b-05 | Repudiation (alert fatigue) | legacy-row noise | medium | mitigate | Scoped to `^v?\d+\.\d+\.\d+$`; pre-v1.5.0 mismatches downgraded to Warn. Live run produced 4 Warn / 0 false Problem. | closed |
| T-21b-SC | Tampering (supply chain) | npm/pip/cargo installs | low | accept | No dependency changes (Cargo.toml/lock unchanged). | closed |
| T-21c-01 | Information Disclosure | slot record contents | medium | mitigate | Record stores only slot letter + agent kind (path/username-free); a test asserts no path/home string present. | closed |
| T-21c-02 | Spoofing (stale pid) | status liveness display | low | mitigate | Liveness derived live via `agent_running(pid)`; a dead pid renders "not running", never a false live agent. | closed |
| T-21c-03 | Tampering (state corruption) | avoiding State/save_state | medium | mitigate | Standalone sibling record, NOT routed through `State`/`save_state` (only `save_state` match is a doc comment asserting this); single-state-per-phase preserved. | closed |
| T-21c-04 | DoS (local) | record parsing | low | mitigate | `read_sequentagent_slot` returns `None` on malformed content — status degrades, never panics. | closed |
| T-21c-SC | Tampering (supply chain) | npm/pip/cargo installs | low | accept | No dependency changes (Cargo.toml/lock unchanged). | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (high) count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-21-SC | T-21a-SC / T-21b-SC / T-21c-SC / T-21d-SC | No dependency changes in any Phase 21 plan — zero supply-chain surface; `git diff` confirms `Cargo.toml`/`Cargo.lock` unchanged. Standing accept for the no-dependency-change case. | operator (dennisk.im) | 2026-07-23 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-23 | 20 | 20 | 0 | gsd-secure-phase (L1 grep verification; short-circuit — plan-time register, ASVS L1) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-23

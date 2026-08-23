---
phase: 33
slug: loop-back-correctness-for-multi-wave-validate-code-cycles-99
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-05
---

# Phase 33 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

Register origin: `register_authored_at_plan_time: true` — five of six PLANs (33-01 … 33-05) carry a
`<threat_model>` block; 33-06 correctly has none, being a refactor/test-hygiene plan. 33-04's
SUMMARY carries a `## Threat Flags` section reading "None". The audit verified mitigations against
the registered set rather than scanning for new threats.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Agent self-report → DevFlow's derived signals | The phase's central subject: which signal decides a Validate→Code loop-back. DevFlow must not let an agent's own report outrank a signal DevFlow derived | `DEVFLOW_RESULT` status/verdict, `{N}-VERIFICATION.md` existence, git commit counts |
| Main checkout ↔ phase worktree | `.planning/` is tracked, so an in-flight phase's artifacts exist only on `feature/phase-NN` inside the worktree. Git refs and the object database are shared; tracked working-tree content is not | `{N}-VERIFICATION.md`, `{N}-PLAN.md`, commit counts |
| `cargo test` process → spawned agent CLI | A test reaching `loop_back_to_code` → `launch_stage` launches a real `claude` process with the developer's inherited credentials | Process spawn, `PATH`, ambient auth, quota |
| Operator → gate response file | Gate responses are read from disk and parsed into a `GateAction` that decides whether the pipeline aborts or loops | `approved`, `note` fields |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-33-01 | Spoofing | loop-back signal | low | accept | Accepted — see AR-01 | closed |
| T-33-02 | Tampering | `prompt.rs` fix command | low | mitigate | `prompt.rs:302` — `u32` interpolation, no shell path | closed |
| T-33-03 | Info disclosure | gate event payload | low | mitigate | `pipeline_gate.rs:151` — `Debug` of a 3-variant enum, no path leakage | closed |
| T-33-04 | Tampering | state deserialization | low | accept | Accepted — see AR-02 | closed |
| T-33-05 | Tampering | `state.rs` new field | low | mitigate | `state.rs:99-100` `#[serde(default)]`; absent-defaults test at `:436` | closed |
| T-33-06 | Elevation of privilege | forward-progress gate | medium | accept | Accepted — see AR-03. Legibility mitigation present (`mode.rs:136-143`); the risk itself is accepted, not controlled | closed |
| T-33-07 | Tampering | commit-count read | low | mitigate | `agent_result.rs:6129` `phase_commit_count_reports_zero_without_a_branch` | closed |
| T-33-08 | Tampering | baseline persistence | low | accept | Accepted — see AR-04 | closed |
| T-33-09 | Tampering | test → agent spawn | medium | mitigate | `pipeline_gate.rs:1140-1170` — `ENV_MUTEX` + PATH neutralization + two branch-pinning assertions at `:1161-1170` | closed |
| T-33-10 | Tampering | test-suite integrity | medium | mitigate | Full-phase diff confirmed additive-only; no `assert_eq!` text changed anywhere | closed |
| T-33-11 | Tampering | progress predicate | medium | mitigate | `mode.rs:149-151` — `previous.is_none_or(\|p\| current > p)`, unchanged | closed |
| T-33-12 | Repudiation | loop-back event record | low | accept | Accepted — see AR-05 | closed |
| T-33-13 | Repudiation | branch attribution | medium | mitigate | `pipeline_gate.rs:1161-1170`, `pipeline_outcomes.rs:1885-1892` — both branch discriminators present | closed |
| T-33-14 | Tampering | evidence-root read | **high** | mitigate | `pipeline_outcomes.rs:314-316` hoisted `evidence_root` + `:335`, `:387`, `:400` all three arms + `agent_result.rs:2588` renamed callee | closed |
| T-33-15 | Tampering | commit-count root | **high** | mitigate | `pipeline_outcomes.rs:347` — `phase_commit_count(project_root, …)`, exactly 1 hit, unchanged from pre-fix baseline | closed |
| T-33-16 | Tampering | fallback-arm coverage | medium | mitigate | Both preserved `--no-worktree` tests present and un-diffed since introduction | closed |
| T-33-17 | Tampering | PATH restore on panic | medium | mitigate | `test_support.rs:279`/`:287`/`:300` (`NeutralPath` + `Drop`), used at `pipeline_outcomes.rs:1632`, `:1688`, `:1752` | closed |
| T-33-18 | Repudiation | fix-selection audit trail | medium | mitigate | Same discriminator as T-33-14, plus recorded RED text in 33-05-SUMMARY.md | closed |
| T-33-19 | Denial of service | vanished-worktree fallback | low | accept | Accepted — see AR-06. Plain-fallback design confirmed unchanged (`pipeline_outcomes.rs:316`, no `.exists()` filter) | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (high) count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

Both **high**-severity threats (T-33-14, T-33-15) are mitigated and independently confirmed — by the
phase verifier, by the Claude code review, and by two external peer reviews (DeepSeek v4 Pro,
Gemini 3.1 Pro).

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-33-01 | The loop-back signal is derived from artifacts the agent itself authors. Narrowing this is the standing subject of 999.67/DEN-88 (agent plants its own Layer-0 provenance), not of this phase | Operator (recorded 2026-08-05) | 2026-08-05 |
| AR-02 | T-33-04 | State deserialization trusts a file only DevFlow writes, inside `.devflow/`. Hardening it is out of scope for a loop-back correctness phase | Operator (recorded 2026-08-05) | 2026-08-05 |
| AR-03 | T-33-06 | A trivial commit each cycle defeats the forward-progress gate. Documented honestly at `mode.rs:136-148` and **now filed as 999.78/DEN-100**, which also supplies the ROADMAP number that doc comment previously promised and lacked | Operator (recorded 2026-08-05) | 2026-08-05 |
| AR-04 | T-33-08 | The persisted baseline is only as trustworthy as the git read that produced it. The transient-failure corruption path is filed as 999.77/DEN-99 | Operator (recorded 2026-08-05) | 2026-08-05 |
| AR-05 | T-33-12 | `loop_back` events do not distinguish an absent baseline from a genuine first failure. Filed as part of 999.78/DEN-100 (IN-02) | Operator (recorded 2026-08-05) | 2026-08-05 |
| AR-06 | T-33-19 | A vanished worktree makes the evidence root unreadable rather than silently falling back to the main checkout — deliberate, per 33-05's own prohibition, since falling back would resurrect a stale or other-branch artifact as if it were this phase's | Operator (recorded 2026-08-05) | 2026-08-05 |
| AR-07 | WR-06 → T-33-09 / T-33-17 boundary | Three test sites reach `handle_validate_outcome`'s Failed branch without PATH neutralization. **No spawn occurs today** — each pre-writes an `abort`-worded gate response resolving to `GateAction::Abort`, verified in source. The protection is content-dependent rather than structural, so it is accepted here and routed to 999.80/DEN-102 rather than fixed in-phase | Operator (recorded 2026-08-05) | 2026-08-05 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-05 | 19 | 19 | 0 | gsd-security-auditor (ASVS L1, block_on: high) |

**What this audit does NOT establish.** It verifies static presence of each mitigation at ASVS
Level 1 — the pattern exists in the cited file — not runtime behaviour under load. Specifically it
does not establish that the three WR-06 sites' "always resolves to `Abort`" property is durable
against a future edit; nothing asserts it, which is precisely the finding routed to 999.80. It also
does not re-litigate the non-STRIDE-registered review findings (WR-01/02/03/04, IN-01–IN-05), which
are carried to 999.77–999.81. And no test in this phase drives a real linked `git worktree` or a
real spawned agent end-to-end, so the trust boundaries above are verified structurally rather than
behaviourally.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-05

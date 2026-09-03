---
phase: 43
slug: opencode-driver-completion
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-24
---

# Phase 43 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

Register authored at plan time (both 43-01-PLAN.md and 43-02-PLAN.md carry a `<threat_model>`
block) — `register_authored_at_plan_time: true`. Per `/gsd-secure-phase`'s short-circuit rule
(`threats_open: 0 AND register_authored_at_plan_time: true AND asvs_level == 1`), this audit
verified each threat's cited mitigation directly against the current implementation
(`feature/phase-43`, commit `35e357c` and its ancestors) at L1 grep-depth — sufficient for
ASVS level 1 — rather than spawning the `gsd-security-auditor` subagent for deeper L2/L3
verification.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| DevFlow monitor → `opencode` CLI subprocess | `--auto` auto-approves every permission not explicitly denied; the spawned agent executes tool calls unattended. | Prompt text out; process control |
| `opencode` stdout (JSONL) → `parse_opencode_event_result` | Every byte is model/provider-controlled; decides whether a commit-gated stage advances. | Untrusted event stream |
| Model-generated `part.text` → `parse_marker_lines` → `AgentResult` | The DEVFLOW_RESULT marker is self-reported by the model, crossing into DevFlow's verdict system. | Self-reported completion status |
| Operator's authenticated session → committed test fixtures | Real captures from an authenticated run, committed to git. | Session/credential-adjacent artifacts |
| `devflow-cli::preflight` → `OpenCodeDriver::health` | Sole gate that can refuse an OpenCode launch; a false-green silently defeats OPCD-03. | Credential-readiness verdict |
| `opencode providers list` / `opencode agent list` stdout → parsers | ANSI-escaped, box-drawn, human-formatted subprocess output crossing into security/capability decisions. | Untrusted subprocess text |
| `health` error string → persisted logs and operator output | Anything placed here reaches `events.jsonl` and the terminal. | Error diagnostics |
| PATH mutation in tests → the whole test process | `set_var("PATH", ...)` is process-global while `cargo test` runs in parallel. | Test-process environment |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-43-01 | Elevation of Privilege | `build_command` `--auto` flag | high | mitigate | `--auto` appears only in launch argv, never in a health/capability probe (verified: `opencode.rs:57`, single occurrence). Same scope as the shipped Pi `--no-approve` / Codex `-a never` precedent. | closed |
| T-43-02 | Spoofing | `parse_opencode_event_result` marker path | high | mitigate | Every marker-derived `AgentResult` passes through `normalise_stream_marker_provenance`, force-setting `decided_by_layer = Some(1)` (verified: `agent_result.rs:843,972`). Proven by `opencode_marker_cannot_forge_layer0_provenance` — passing. | closed |
| T-43-03 | Tampering | Torn / truncated capture | high | mitigate | `torn_json_after_last_matching(\|_\| true)` runs before verdict resolution (verified: `agent_result.rs:918`, precedes the error/marker scans at 928/961). | closed |
| T-43-04 | Denial of Service | JSONL field access | medium | mitigate | All event field access is `Option`-chained, no `unwrap`/`expect`/indexing. Proven by `opencode_malformed_events_do_not_panic` — passing. | closed |
| T-43-05 | Information Disclosure | Vendored evidence fixtures | medium | mitigate | Fixtures leak-scanned for home paths/usernames/key material at commit time; carry only opaque `ses_`/`prt_`/`msg_`/`call_` identifiers. No new scan performed in this audit (documentary control, not re-verifiable from source alone). | closed |
| T-43-06 | Repudiation | Marker-less run | high | mitigate | A marker-less stream returns `None`, deferring to Layer 2 rather than resolving Success. Proven by `opencode_real_success_capture_is_recognised_and_marker_less` and `opencode_real_tool_use_capture_defers_to_layer2` — both passing. | closed |
| T-43-07 | Tampering | Cross-adapter parser collision | low | mitigate | `is_opencode_event_stream` requires an OpenCode-unique `step_start`/`step_finish` sighting, not the generic `error` shape (verified: `agent_result.rs:875-887`). | closed |
| T-43-08 | Information Disclosure | `reason` string content | low | accept | Provider's `error.data.message` lands verbatim in a persisted result — matches existing Codex/Claude handling exactly; changing it would diverge one adapter from four. Generic log-injection hardening is out of this phase's scope. | accepted |
| T-43-09 | Spoofing | `health` readiness signal | high | mitigate | Readiness requires BOTH `output.status.success()` AND a positive anchored credential count (fixed post-review, commit `35e357c`) — never exit status alone, never the model catalog. Proven by `preflight_rejects_constructed_zero_credential_output` and `preflight_rejects_nonzero_exit_with_credential_bearing_stdout` — both passing. | closed |
| T-43-10 | Denial of Service | `health` / `capabilities` probe | medium | mitigate | Spawn failure / non-zero exit / unparseable stdout all resolve to the safe default (`Err` for health, `false` for capabilities), no panic path. Proven by `subagent_probe_fails_closed_on_spawn_error`, `_on_nonzero_exit`, `_on_empty_output`, and `capabilities_never_refuses_a_launch` — all passing. | closed |
| T-43-11 | Information Disclosure | `health` error string | high | mitigate | Fixed error string never interpolates probe stdout, provider names, `auth.json` paths, or env-var names. Proven by `health_error_leaks_no_provider_detail` — passing (asserts `auth.json`/`GOOGLE_API_KEY`/`Google`/`expired` all absent from the error). | closed |
| T-43-12 | Elevation of Privilege | Probe argv | medium | mitigate | Health probe argv is exactly `["providers", "list"]`; capability probe is exactly `["agent", "list"]` — neither carries `--auto`, a prompt, or a model selection. Proven by `health_probe_argv_is_providers_list` — passing (asserts argv literally). | closed |
| T-43-13 | Tampering | Test PATH mutation | medium | mitigate | Every PATH-swapping test takes `ENV_MUTEX` first and restores via `PathGuard`'s `Drop` (verified: `opencode.rs:238,287`, used at 379/394/412 etc.). Copied verbatim from `pi.rs`'s established pattern. | closed |
| T-43-14 | Repudiation | Unverified negative path | high | mitigate | The zero-credential shape was never observed live (A1) — explicitly flagged, not hidden, at three locations in the doc comments (`opencode.rs:149,323,388`). Carried forward as a Manual-Only verification in `43-VALIDATION.md`, not silently closed. | closed |
| T-43-15 | Tampering | Doctor install hint | medium | mitigate | Hint corrected to `npm i -g opencode-ai` (verified: `commands.rs:2317`), the actual registry package matching the installed binary. The prior `cargo install opencode` hint (a different, unrelated crate) is gone — confirmed absent via direct grep. | closed |

*Status: open · closed · open — below `high` (`workflow.security_block_on`) threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `high` count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-43-01 | T-43-08 | Provider error message content flows verbatim into a persisted result; this is the existing, previously-accepted pattern for Codex's `turn.failed` and Claude's stream-envelope-failure handling — diverging OpenCode alone would be inconsistent, and the operator needs the real message to act on failures. Generic log-injection hardening is tracked separately, not per-adapter. | Plan author (43-01-PLAN.md, low severity) | 2026-08-23 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-24 | 15 | 14 (+1 accepted) | 0 | Claude Code (`/gsd-secure-phase 43`, L1 grep-depth per ASVS level 1 short-circuit) |

All 15 threats' cited mitigations were re-verified directly against source in the
`feature/phase-43` worktree at the current HEAD (post-review-fix commit `35e357c`), not merely
copied from the plan's own claims. All 20 cited unit tests were re-run and confirmed passing
(`cargo test -p devflow-core --lib opencode` → 39 passed, 0 failed, includes the relevant subset).
`workflow.security_asvs_level: 1` and `workflow.security_block_on: high` were read from the live
project config at audit time.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-24

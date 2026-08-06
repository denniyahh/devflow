---
phase: 30
slug: keep-the-session-alive-past-turn-end
status: verified
threats_open: 0
asvs_level: 1
created: 2026-08-03
---

# Phase 30 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

State: **B** (no `SECURITY.md` existed; run from artifacts). All 5 plans carried a
`<threat_model>` STRIDE block authored at plan time (`register_authored_at_plan_time: true`).
`asvs_level: 1` + `threats_open: 0` after L1 verification → per the secure-phase workflow's own
short-circuit rule, this audit did not spawn `gsd-security-auditor`. All 33 register entries
below were individually re-verified against **current source at HEAD `5a2b004`** — not against
the plans' original claims — because this exact file (`agent_result.rs`) was rewritten three
times after these threat models were authored (see "Retroactive Findings" below). Every mitigate
disposition has a direct grep/read citation; every accept disposition is recorded as such.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| coding agent → DevFlow | The agent writes arbitrary, attacker-influenceable text to `.devflow/phase-NN-stdout`; DevFlow parses it to decide whether a stage succeeded. | Stage verdict (success/failure), provenance |
| CLI process → DevFlow | The `claude`/`codex` CLI emits stream envelope fields (`type`, `origin`, `session_id`, `is_error`, `rate_limit_event`) DevFlow trusts as authoritative. | Session identity, error/rate-limit state |
| operator prompt → DevFlow control flow | Under stream-json the prompt is echoed back into the same capture DevFlow scans for control signals. | Checkpoint gate declarations |
| subagent → orchestrator relay | Subagent narration is forwarded into the top-level stream tagged with `parent_tool_use_id`. | Checkpoint gate declarations |
| harness → committed repository | 30c/30d harness output is written into `30c-evidence/` / `30d-evidence/`, committed and published with this repo. | Process env, session ids, CLI output |
| harness → live `claude` CLI | The harnesses launch real CLI processes with a scrubbed but otherwise inherited environment, and (30d) deliberately close stdin mid-work. | Environment variables, process lifecycle |

---

## Threat Register

Threat IDs collide across plans (T-30-26 four times, T-30-27 three times) — each plan allocated
IDs independently. Disambiguated below by originating plan; canonical ID unchanged from the PLAN.

| Threat ID | Plan | Category | Component | Severity | Disposition | Status | Evidence |
|---|---|---|---|---|---|---|---|
| T-30-01 | 30-01 | Spoofing | `last_top_level_result` | high | mitigate | closed | No `json_scan`/`json_find_key` in scope — confirmed absent by grep |
| T-30-02 | 30-01 | Tampering | `is_claude_event_stream` | high | mitigate | closed | Gates only on `type:"system"` + `subtype:"init"` — `agent_result.rs:823-828` |
| T-30-03 | 30-01 | Tampering | `evaluate_layer1` cascade order | medium | mitigate | closed | Envelope-failure → stream → `parse_devflow_result` order confirmed at `agent_result.rs:1521-1525` |
| T-30-04 | 30-01 | Denial of Service | `claude_stream_events`/`ParsedCapture::parse` | low | **accept** | closed | Same exposure `parse_codex_event_result` carries since Phase 13; recorded in Accepted Risks |
| T-30-05 | 30-01 | Elevation of Privilege | marker deserialization | low | **accept** | closed | `parse_marker_lines` field surface unchanged; recorded in Accepted Risks |
| T-30-26 | 30-01 | Elevation of Privilege | `decided_by_layer` provenance (stream path) | high | mitigate | closed | `normalise_stream_marker_provenance` at `agent_result.rs:1392`. **Its own noted residual — the same surface on `parse_devflow_result` — is ALSO now closed** (`agent_result.rs:159,161`), by the 6th-pass fix; not just the original scope. |
| T-30-06 | 30-02 | Information Disclosure | `30c-evidence/` published artifacts | high | mitigate | closed | Validate→structurally-redact→secret-scan→atomic-replace pipeline confirmed in `30c-monitor-env-harness.py` |
| T-30-07 | 30-02 | Information Disclosure | child process environment | medium | mitigate | closed | Only variable names logged (`removed_variables:`), never values — `30c-monitor-env-harness.py:844` |
| T-30-26 | 30-02 | Information Disclosure | credential-shaped tokens in CLI output | high | mitigate | closed | Explicit secret scan before publish, redaction-aware regex — `30c-monitor-env-harness.py:592-649` |
| T-30-27 | 30-02 | Repudiation | unproven environment equivalence | medium | mitigate | closed | `## Residual environment` section present in both `30c-VERDICT.md:136` and `30c-VERDICT-scrubbed.md:181` |
| T-30-08 | 30-02 | Tampering | env-scrub list transcription | medium | mitigate | closed | Parsed from `git.rs` at runtime, not transcribed — `30c-monitor-env-harness.py:73,121` |
| T-30-09 | 30-02 | Repudiation | verdict provenance | medium | mitigate | closed | `verify-verdict-frontmatter.py` re-derives from raw JSONL, present in phase dir |
| T-30-10 | 30-02 | Denial of Service | live CLI usage | low | **accept** | closed | Bounded, deliberate; recorded in Accepted Risks |
| T-30-11 | 30-03 | Spoofing | `claude_stream_session_id` | high | mitigate | closed | Direct `.get()` on top-level `init`, no `json_scan` — confirmed absent by grep |
| T-30-12 | 30-03 | Spoofing | `detect_claude_stream_rate_limit` | medium | mitigate | closed | Direct `.get()` on top-level `rate_limit_event` — `agent_result.rs:1232` |
| T-30-13 | 30-03 | Tampering | rate-limit vs failure precedence | high | mitigate | closed | Rate-limit check precedes marker/envelope-failure inside `parse_claude_event_result` — `agent_result.rs:1382` before `1386`/`1401` |
| T-30-26 | 30-03 | Denial of Service | false-positive rate-limit classification | high | mitigate | closed | Positional + semantic dual guard, unchanged since plan time |
| T-30-14 | 30-03 | Elevation of Privilege | `AgentResult` field surface | high | mitigate | closed | No `session_id` field on `AgentResult` — confirmed absent by grep |
| T-30-15 | 30-03 | Tampering | envelope-over-marker precedence | medium | mitigate | closed | `claude_stream_envelope_failure` called after marker, overrides success — `agent_result.rs:1401` |
| T-30-16 | 30-04 | Information Disclosure | `30d-evidence/` archived artifacts | high | mitigate | closed | Imports 30-02's redaction pipeline, not a second redactor — `30d-exit-timing-harness.py:32-34` |
| T-30-17 | 30-04 | Denial of Service | orphaned child processes | medium | mitigate | closed | Process-group kill + verified reap, records survivors even on failure — `30d-exit-timing-harness.py:166,305,332-344` |
| T-30-27 | 30-04 | Repudiation | measurement representativeness | medium | mitigate | closed | Aborts (never silently falls back) if 30c's module can't load — `30d-exit-timing-harness.py:174-194` |
| T-30-28 | 30-04 | Repudiation | premature observation window | medium | mitigate | closed | Window floor = 22s deadline + buffer = 52.0s, refuses shorter — `30d-exit-timing-harness.py:121,1263` |
| T-30-18 | 30-04 | Repudiation | measurement provenance | medium | mitigate | closed | Per-trial raw JSONL archived individually (structural, verified present in `30d-evidence/`) |
| T-30-19 | 30-04 | Tampering | measurement validity | medium | mitigate | closed | Monotonic clock throughout — `30d-exit-timing-harness.py:368-369,435` |
| T-30-20 | 30-04 | Denial of Service | live CLI usage | low | **accept** | closed | Bounded, serialized after 30c; recorded in Accepted Risks |
| T-30-21 | 30-05 | Tampering | `blocking_human_checkpoint_reported` under stream capture | high | mitigate | closed | Type filter keeps only `result` events — `agent_result.rs:1155-1162` |
| T-30-26 | 30-05 | Tampering | intermediate assistant narration | medium | mitigate | closed | `assistant` not present in gate-scan filter — confirmed absent by grep |
| T-30-27 | 30-05 | Denial of Service | over-narrowing to last-result semantics | medium | mitigate | closed | `.any()` over the full filtered iterator, no `.last()`/truncation — confirmed absent by grep |
| T-30-22 | 30-05 | Spoofing | subagent-forwarded events | high | mitigate | closed | Shared `is_top_level` predicate (null-or-absent `parent_tool_use_id`) — `agent_result.rs:1159`, now also enforced on the verdict path (constraint 9 item 2, post-plan hardening) |
| T-30-23 | 30-05 | Tampering | text extraction path | medium | mitigate | closed | Direct `.get()` chain — `agent_result.rs:1160` |
| T-30-24 | 30-05 | Denial of Service | over-correction suppressing real gates | medium | mitigate | closed | Positive + co-occurrence regression tests present |
| T-30-25 | 30-05 | Tampering | single-document regression | high | mitigate | closed | 16 `blocking_human_checkpoint_reported_*` tests present (≥ the original 10, none removed) |

*Status: closed — all 33. Severity: critical > high > medium > low — only open threats at or above
`workflow.security_block_on` (`high`) count toward `threats_open`. Disposition: mitigate
(implementation verified) · accept (documented risk, below).*

---

## Retroactive Findings — not in the original STRIDE register

The original 5 plans' threat models (above) were authored **before** three post-execution
adversarial-review rounds rewrote `agent_result.rs` twice more. Six independent adversarial
passes (5 via codex/gpt-5.6-sol, 1 via a different model family, gemini-3.1-pro-preview) found
**8 additional real defects** the upfront STRIDE modeling did not anticipate — practice finding
what modeling missed, not a failure of this register. All 8 are closed as of `5a2b004`:

| # | Finding | Class | Fixed in |
|---|---|---|---|
| 1 | Gate scoping fell back to raw-stdout scan on a torn `init` line (fail-open, reinstating the prompt-echo false positive) | Tampering | `06675da` |
| 2 | One stray JSONL-shaped line diverted a plain-text capture onto the stream branch, suppressing a real gate (fail-closed) | Denial of Service | `f34756c` |
| 3 | Invalid UTF-8 byte outside the JSON envelope converted an authoritative failure into a Layer-2 success | Tampering | `4867207` |
| 4 | Torn gate-bearing `user` event reopened raw-stdout scanning | Tampering | `4867207` |
| 5 | Torn later `init` resurrected a stale session id | Spoofing | `4867207` |
| 6 | Drop-based UTF-8 decode joined tokens across corrupt bytes, fabricating a valid success marker | Tampering | `a557805` (reverted to lossy-replace) |
| 7 | One corrupt byte before a superseding failure marker let an earlier success decide (Codex path) / rate-limit precedence inversion under edge corruption | Tampering | `8fa9849` |
| 8 | Marker-tail scanner: edge corruption hid markers, a long marker line could be bisected by the fixed byte budget, and the "case-insensitive" prefix match was case-sensitive; codex prose rate-limit heuristic false-positived on bare digit substrings | Tampering / Spoofing | `099bac6` |

**One item remains open by design, not oversight** — ROADMAP constraint 9's surviving obligation:
a capture truncated at an exact line boundary is content-indistinguishable from a healthy shorter
run, so no parser-level assertion can close it. The defense is assigned to Phase 31's launch-path
wiring (a stream-derived Success must not short-circuit a contradicting exit code) and is
explicitly out of this phase's scope — Phase 31 re-runs `/gsd-secure-phase` against its own
threat model, which will need to carry this item forward.

Full detail: `30-CODE-REVIEW.md`, `30-VERIFICATION.md`, `30-H1-CONTEXT-FOR-31.md`, ROADMAP
constraint 9.

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-30-01 | T-30-04 (30-01) | Collecting a capture into `Vec<Value>` is bounded by on-disk capture size; identical exposure `parse_codex_event_result` has carried since Phase 13. No new attack surface. | Plan 30-01 (plan-time) | 2026-08-02 |
| AR-30-02 | T-30-05 (30-01) | `parse_marker_lines` reused unchanged; its existing agent-settable-field surface is neither widened nor narrowed — this plan strictly reduces what reaches it. | Plan 30-01 (plan-time) | 2026-08-02 |
| AR-30-03 | T-30-10 (30-02) | One experiment run consumes CLI quota. Bounded, deliberate, serialized against other live experiments. | Plan 30-02 (plan-time) | 2026-08-02 |
| AR-30-04 | T-30-20 (30-04) | Seven-plus trials consume CLI quota. Bounded, deliberate, serialized after 30c. | Plan 30-04 (plan-time) | 2026-08-02 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-03 | 33 (register) + 8 (retroactive) = 41 | 41 | 0 | Claude (orchestrator, L1 grep+read verification against HEAD `5a2b004`; auditor not spawned per short-circuit rule) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-03

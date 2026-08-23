# Research Summary: DevFlow — Remaining Harness Support + Pi Dogfood

**Domain:** AI coding-agent harness integration (Rust CLI, `AgentDriver` contract)
**Researched:** 2026-08-18
**Overall confidence:** HIGH (installed-CLI probes + source read; no web search required)

## Executive Summary

DevFlow already ships four harness drivers (Claude, Codex, OpenCode, Pi) behind the modular
`AgentDriver` contract. Milestone v2.8.0 completes that coverage: **Antigravity CLI** and **Hermes**
are genuinely new (no `AgentKind` variant exists), **OpenCode** is a 755-byte stub that implements
only three of the trait's methods, and **Codex** is a full driver that has never been dogfooded
end-to-end. The final piece is proving the newest driver — **Pi** — holds up under a real phase run.

The good news: **no architectural change is needed.** All three integrations map onto one of two
existing launch families. Antigravity CLI is Claude-Code-compatible (`-p`, `--output-format
stream-json`, `--input-format stream-json` in the vetted 1.1.14 binary, `--dangerously-skip-permissions`
for headless approval), so its driver mirrors `ClaudeDriver`. Hermes exposes a clean oneshot mode
(`hermes -z "<prompt>"` prints only the final response text; `--yolo` bypasses approvals), so its
driver mirrors `PiDriver` (positional argv + process-exit + prompt-embedded `DEVFLOW_RESULT`).
OpenCode already has `opencode run "<prompt>"`; completing it means adding `--auto` + `--format
json` and a completion parser modeled on Codex's `parse_codex_event_result`.

The risk surface is *flag grammar* (approval flags are positional and binary-specific), *completion
honesty* (never let a marker-less run advance), and *capability detection* (fail-closed, exact-match).
Every one of these has already bitten this project and has a recorded fix pattern.

## Key Findings

**Stack:** No new Rust deps. Three CLIs already installed: Antigravity CLI (1.1.14 via `agycli`),
Hermes (0.20.4, Python), OpenCode (1.18.18).
**Architecture:** New harnesses plug into the existing `AgentDriver` trait; two launch families
already exist (stream-json pipe-owning vs positional single-document).
**Critical pitfall:** flag placement + completion honesty — both have produced real defects in
Phases 13/17/37/39 and must be spawn-tested, not assumed.

## Implications for Roadmap

Suggested phase structure (drivers first, then the dogfood proof, then the opportunistic backlog):

1. **Antigravity driver** — new `AgentKind::Antigravity` + `AntigravityDriver` (Claude-style
   stream-json). Highest reuse, lowest novelty.
   - Addresses: new harness onboarding (table stakes)
   - Avoids: binary-name ambiguity (target `antigravity-cli` 1.1.14, spawn-test argv)

2. **Hermes driver** — new `AgentKind::Hermes` + `HermesDriver` (Pi-style `-z --yolo`).
   - Addresses: backlog 999.1 (Hermes Support)
   - Avoids: parsing oneshot stdout as JSON (it's bare final-text)

3. **OpenCode driver completion** — `--auto` + `--format json` + `parse_completion` + `health`.
   - Addresses: finishing the stub into a real driver
   - Avoids: changing transport and parser in separate plans (atomic)

4. **Phase 40 — Pi dogfood** — run a real Define→Ship phase through `--agent pi`; close the deferred
   isolated-context Pi dispatch if it surfaces.
   - Addresses: proving the newest driver under real use
   - Avoids: repeating the dead-monitor/gate-wedge failures of earlier dogfood runs

5. **Codex end-to-end verification/hardening** — dogfood `--agent codex`, close surfaced gaps.
   - Addresses: the user's "fully support Codex" (already native, needs proof)

6. **999.94 + 999.85** (capacity-permitting) — unattended `decision` checkpoint blind-first-option
   (HIGH) + two stale comments (low). Independent of all of the above.

**Phase ordering rationale:** new drivers (1–3) are independent of each other but all *enable* the
dogfood proofs (4–5), which need a shipped binary. OpenCode (3) is the only *existing* stub — lowest
risk, can run in any wave. 999.94 is a policy/gate change, unrelated to drivers; it rides last so it
never blocks harness coverage.

**Research flags for phases:**
- Phases 1–3: spawn-test argv against the installed CLIs before trusting each driver (flag grammar).
- Phase 4 (Pi dogfood): no new research; reuse the v2.5.0/v2.7.0 dogfood playbook.
- Phase 6 (999.94): needs explicit acceptance — it changes unattended-run policy.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | installed-CLI `--help` is authoritative; versions probed 2026-08-18 |
| Features | HIGH | `AgentDriver` trait read from source |
| Architecture | HIGH | registration points enumerated from source |
| Pitfalls | HIGH | all from recorded phase lessons + direct probing |

## Gaps to Address

- **Antigravity stream-json turn protocol** — the exact NDJSON stdin shape (`--input-format
  stream-json`) matches Claude's, but has not been end-to-end exercised; Phase 1 must verify against
  a real spawn (not assumed).
- **OpenCode `--format json` event schema** — the JSONL fields must be read from a real capture
  before the parser is written; do not assume Codex's schema is identical.
- **Hermes credential/provider surface** — `health()` needs a concrete probe (`hermes doctor`?); the
  exact headless-readiness verb is deferred to Phase 2 planning.
- **Codex gap list** — the actual hardening items are unknown until the dogfood run surfaces them.

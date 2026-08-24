# Phase 43: Adversarial Multi-Model Code Review Summary

**Date**: 2026-08-23
**Phase**: 43 (OpenCode Driver Completion)
**Branch**: `feature/phase-43` (8 commits, `develop..HEAD`)
**Reviewers**:
1. **OpenAI Codex** (`codex exec -c model_reasoning_effort=high`)
2. **Antigravity** (`agy --model gemini-3.1-pro-high --effort high`)
3. **Claude Code** (verification pass — every finding below checked against actual source before being recorded)

All findings were independently verified against `crates/devflow-core/src/agent_result.rs` and
`crates/devflow-core/src/agents/opencode.rs` in the `feature/phase-43` worktree before being
recorded here — neither lane's prose is taken on faith.

---

## 1. Executive Summary

- **Security & Injection**: **CLEAN**. Both lanes independently confirmed `Command::new("opencode").args([...])` argv-array usage throughout; no shell interpolation anywhere in the new code.
- **Credential leakage**: **CLEAN**. `health()`'s error string is a hardcoded constant (`"no OpenCode provider credential configured"`); no captured stdout, provider name, or path is ever interpolated into it. Verified directly.
- **Fail-closed health check (OPCD-03)**: **NOT ACTUALLY FAIL-CLOSED — 3 independent, verified gaps**, all in the same ~70-line region (`opencode.rs:84-156`). This is the most serious finding and the reason for the recommendation below.
- **Fail-closed capability probe (OPCD-03)**: **Confirmed by both lanes independently** — the subagent-dispatch probe can be false-positived by ordinary description text, not just a real header line.
- **Completion-parsing precedence**: **Disputed between lanes** — recorded as such, not resolved by fiat.
- **Blocker status**: 1 High-confidence, high-severity, doubly-confirmed defect (health check). Recommend **fix before Ship**, not an unconditional approve.
- **Tests**: `cargo test -p devflow-core --lib` — 718 passed, 0 failed (verified directly, not just cited). `cargo test -p devflow --bin devflow --no-run` compiles cleanly (codex claimed a pre-existing integration-test compile failure; **this did not reproduce** — noted as an unverified/non-reproducing claim, not included as a finding).

---

## 2. Findings Matrix

| # | Severity | Component | Location | Defect | Verified how |
|---|---|---|---|---|---|
| **1** | **High** | Health check | [`opencode.rs:84-94`](../../../crates/devflow-core/src/agents/opencode.rs#L84-L94) | `health()` never checks `output.status.success()`. A failing `opencode providers list` invocation (non-zero exit) that still happens to print a well-formed `N credentials` line is reported healthy — the exit code is silently ignored. | Codex finding; read the function body directly — confirmed no `status.success()` check anywhere in `health()`. |
| **2** | **High** | Health check | [`opencode.rs:107-128`](../../../crates/devflow-core/src/agents/opencode.rs#L107-L128) (`strip_ansi_escapes`) | Assumes every CSI escape sequence ends in `m` (true only for SGR/color codes). A non-SGR sequence — cursor movement (`\x1b[1A`), erase-line (`\x1b[2K`) — causes the strip loop to keep consuming characters looking for *any* subsequent `m`, which can land inside unrelated words (e.g. the `m` in "environ**m**ent"), silently truncating/corrupting the parsed output. Not observed in the one live capture taken during discuss-phase (which only contained `\x1b[90m` SGR codes), so real-world trigger likelihood on this specific CLI is unconfirmed — but the implementation is unsound as a general ANSI stripper. | Antigravity finding; read `strip_ansi_escapes` line-by-line, confirmed the `if next == 'm' { break }` loop has no bound on sequence type. |
| **3** | **Medium** | Health check | [`opencode.rs:145-156`](../../../crates/devflow-core/src/agents/opencode.rs#L145-L156) (`opencode_configured_provider_count`) | Per-line matching requires only that a line, after stripping box-drawing/bullet characters, starts with a number followed by `credential`/`environment variable` — it does not anchor to the actual footer position or require the surrounding section structure. Compounds #1: a malformed or partial capture that happens to contain a matching substring anywhere still counts. | Codex finding; read the function body, confirmed `rest.starts_with(...)` has no positional/structural anchor. |
| **4** | **Medium** | Capability probe | [`opencode.rs:194-201`](../../../crates/devflow-core/src/agents/opencode.rs#L194-L201) (`parse_opencode_agent_list_for_subagent`) | `stdout.lines().any(\|line\| line.contains("(subagent)") \|\| line.contains("(all)"))` scans **every** line of `opencode agent list` output for a raw substring, not just agent-name header lines. Any line containing that text anywhere (e.g. a custom agent description) flips `subagent_dispatch: true`, contradicting the function's own doc comment claiming it matches "header lines." The author's own comment already discloses this as an honest limitation (A4: "no live capture of a real configured subagent exists on this machine") — a known, not hidden, gap. | **Confirmed independently by both lanes**, same file/lines. Read the function directly. |
| **5** | Disputed | Completion parsing | [`agent_result.rs:928-954`](../../../crates/devflow-core/src/agent_result.rs#L928-L954) | "Error anywhere in the stream" is a hard-fail signal regardless of position. Antigravity (High): a transient error the agent recovers from, followed by a genuine later success, would still be wrongly reported Failed. Codex, reviewing the *same* question: found no concrete case in OpenCode's actual emitter behavior where a recoverable error is followed by continued success, called the design "conservative," and flagged it only as a hypothetical future risk if OpenCode's emitter changes. **Not resolved here** — the two lanes disagree, and neither of us has a live capture of a recovered-transient-error stream to settle it empirically. | Both lanes reviewed this independently and reached different conclusions; recorded as-is rather than picking a side. |
| **6** | Info | Completion parsing | [`agent_result.rs:961-967`](../../../crates/devflow-core/src/agent_result.rs#L961-L967) | Codex: the marker scan (`.rev().find_map(...)`) finds the *last text event that has a parseable marker*, not literally the final event — so an earlier success marker followed by later marker-less prose would still resolve to the earlier marker. **Verified this is byte-for-byte the same pattern as the already-shipped `parse_codex_event_result`** (`agent_result.rs:792-798`) — not a defect novel to this diff. If it's a real concern, it's systemic across every stream-marker parser in this file, not specific to OpenCode. Not counted as a phase-43 blocker for that reason. | Read `parse_codex_event_result`'s identical `.rev().find_map(...)` marker scan side-by-side; confirmed same construct. |
| **7** | Info (unverified claim) | N/A | N/A | Codex claimed `cargo test -p devflow` "integration tests fail to compile with the pre-existing test_support feature-gating issue." **Did not reproduce**: `cargo test -p devflow --no-run` compiled all 19 integration test binaries cleanly in this exact worktree. Recorded so this specific claim isn't silently repeated as fact elsewhere. | Ran the command directly. |
| **8** | Info (unverified, external) | Health check design | [`opencode.rs:84-94`](../../../crates/devflow-core/src/agents/opencode.rs#L84-L94) | Codex: a credential/environment-variable count of zero may wrongly refuse legitimate credential-less configurations (local models, custom providers), citing OpenCode's own provider docs. Plausible architectural gap, but based on external URLs/source paths I have not independently fetched or confirmed — not verified firsthand. | Not independently verified — recorded as a lead, not a confirmed finding. |

---

## 3. Recommendation

**Do not approve the pending Code-stage gate as-is.** Findings #1–#3 are three independently
verified, real gaps in the *same* health-check function that OPCD-03 specifically required to be
"fail-closed" — and they compound each other (a non-zero exit code is ignored, AND the line-match
has no positional anchor, AND the ANSI stripper can corrupt the very lines being matched). This is
a correctness gap against an explicit phase requirement, not a style nitpick, and it was missed by
the plan's own tests (which only exercise exit-0 fixtures the same author wrote).

**Suggested path**: reject with a note citing findings #1, #2, #3, #4 (the four confirmed,
non-disputed defects), have the Code stage re-run to fix them (the fixes are small and localized —
add an `output.status.success()` check, anchor the credential-count regex to the actual footer
shape or use a proper CSI-sequence terminator set instead of `m`-only, and match the capability
probe against a `^<name>\s+\((subagent|all)\)` header shape instead of a bare substring), then
re-review before Ship.

---

## 4. Test Evidence

```
cargo test -p devflow-core --lib          → 718 passed; 0 failed
cargo test -p devflow-core --lib opencode → 35 passed; 0 failed (OpenCode-specific)
cargo test -p devflow-core --lib every_driver_passes_the_conformance_suite → 1 passed
cargo test -p devflow --bin devflow doctor → 21 passed; 0 failed
cargo test -p devflow --no-run             → all 19 integration test binaries compile cleanly
```

None of these cover the adversarial inputs in findings #1–#4 — they only prove the happy path the
implementing agent itself wrote fixtures for.

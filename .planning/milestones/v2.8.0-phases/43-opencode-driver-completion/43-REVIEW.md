---
phase: 43-opencode-driver-completion
reviewed: 2026-08-24T00:00:00Z
depth: deep
files_reviewed: 8
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/agents/opencode.rs
  - crates/devflow-core/tests/fixtures/opencode/opencode_error.jsonl
  - crates/devflow-core/tests/fixtures/opencode/opencode_success.jsonl
  - crates/devflow-core/tests/fixtures/opencode/opencode_success_with_marker.jsonl
  - crates/devflow-core/tests/fixtures/opencode/opencode_tool_use.jsonl
findings:
  critical: 0
  warning: 6
  info: 3
  total: 9
status: issues_found
---

# Phase 43: Code Review Report

**Reviewed:** 2026-08-24
**Depth:** deep
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Ship-stage review of the OpenCode driver completion work, run as five parallel angle-scoped
passes (doc-accuracy cross-reference, security/leaked-data, CI/build correctness, external-state
claims, and a generalist deep pass) and merged here. This supersedes the prior standard-depth
review at this path (2026-08-23), whose four Warning findings (`health()` not checking exit
status, ANSI stripper mis-terminating on non-SGR sequences, unanchored footer-line matching, and
the agent-list marker scan flipping on body text) were fixed in commit `35e357c` — the CI/build
pass in this cycle independently re-verified all four fixes with negative-control mutation testing
(reverting each fix one at a time and confirming the corresponding regression test fails), so they
are not carried forward as open findings.

**No Critical-severity findings in any of the five angles.** No hardcoded secrets, no injection or
unsafe-deserialization vectors (subprocess args are a `Vec<String>` passed to `Command::args`, no
shell interpolation; JSONL parsing is `Option`-chained with no `unwrap()` on attacker-controlled
fields), no false-success path through `parse_opencode_event_result`, and no false or unverifiable
external-state claims (commit SHAs, merge commits, and fix counts cited in the phase's SUMMARY.md
files all check out against `git log`/`git show`). All four fixture files match their claimed
scenario and are leak-scanned clean of credential material.

The remaining findings are robustness/consistency gaps and documentation-accuracy nits, not
incorrect-behavior bugs — see Warnings and Info below.

_Methodology note (external-state-claims pass): the `diff_base` SHA supplied for this review
(`6ae22acf...`) is not actually a git ancestor of HEAD — true merge-base is `0e1a94d`. `git log
<sha>..HEAD` still returned a coherent 17-commit list since HEAD is ahead regardless, so this did
not affect scope or findings, but is noted for whoever wires the diff-base computation next time._

## Warnings

### WR-01: `OpenCodeDriver::capabilities()` (subagent-dispatch probe) is fully implemented but never called in production

**File:** `crates/devflow-core/src/agents/opencode.rs:106-110` (and the ~90-line probe/classifier
it wires up: `opencode_subagent_dispatch_available`, `opencode_subagent_dispatch_available_with`,
`parse_opencode_agent_list_for_subagent`)
**Issue:** The only production caller of `.capabilities()` anywhere in the workspace is
`crates/devflow-cli/src/commands.rs:2467-2469`, scoped to `AgentKind::Pi` only
(`pi_subagent_dispatch_check`, registered in `doctor_checks()`). Nothing calls
`agents::driver_for(AgentKind::OpenCode).capabilities()` outside `opencode.rs`'s own
`#[cfg(test)]` module — the OpenCode subagent-dispatch probe this phase built (OPCD-03, D-10) is
dead in the shipped binary, unlike the parallel Pi probe it explicitly mirrors in its own doc
comments.
**Fix:** Either wire an `opencode_subagent_dispatch_check()` into `doctor_checks()` the same way
`pi_subagent_dispatch_check()` is (mirroring `commands.rs:2462-2494`), or, if intentionally staged
for a later phase task, say so in the module doc comment.

### WR-02: `health()`/`capabilities()` spawn `opencode` with no timeout, risking an indefinite hang

**File:** `crates/devflow-core/src/agents/opencode.rs:88-100` (`health`), `:177-183`
(`opencode_subagent_dispatch_available`)
**Issue:** Both probes call `std::process::Command::new("opencode").args([...]).output()`
synchronously with no timeout. A blocked `opencode providers list`/`opencode agent list` (network
credential check, interactive re-auth prompt, hung subprocess) stalls `health()` forever. For
`capabilities()` this directly contradicts the module's own documented invariant — the doc comment
and the `capabilities_never_refuses_a_launch` test both assert "this probe can never refuse a
launch"; an unbounded hang is a de facto refusal that never surfaces as an `Err`. This mirrors an
existing gap in `pi.rs`'s equivalent probes (not introduced fresh here), but this phase adds two
more unbounded spawns to the same class of risk, and this project has a documented history of
gate/monitor hangs from unbounded external-process waits.
**Fix:** Wrap both `Command::output()` calls with a bounded wait (spawn + `wait_timeout`, or a
thread+channel with a timeout) and treat a timeout as the same fail-closed outcome the current
code already produces for a spawn error.

### WR-03: OpenCode error-event scan uses first-match, not last-match, unlike every other decisive scan in this module

**File:** `crates/devflow-core/src/agent_result.rs` — `parse_opencode_event_result`'s error
lookup uses `events.iter().find(...)` while every other decisive lookup in the same file (Codex's
`terminal` scan, both marker scans, `last_top_level_result` for Claude) uses
`.rev().find(...)` to take the LAST match, on the stated rationale that final state should win.
**Issue:** On a hypothetical stream with two `type:"error"` events carrying different messages,
the reported `reason` would be the FIRST (possibly stale/superseded) error text. No live-fixture
reproduction exists today (real captures show a single terminal error), but it's an unexplained
deviation from the convention used everywhere else in the file.
**Fix:** Use `.rev().find(...)` for consistency, or add a comment explaining why first-match is
intentionally correct here if that is the actual design intent.

### WR-04: An `error` event anywhere in the stream unconditionally beats a marker anywhere, including a chronologically later marker — an admittedly unproven assumption

**File:** `crates/devflow-core/src/agent_result.rs` — `parse_opencode_event_result`'s error-scan
block and its doc comment (D-05 / RESEARCH A3)
**Issue:** The code scans for ANY `type:"error"` occurrence and returns `Failed` regardless of
whether a `DEVFLOW_RESULT` success marker appears later in the same stream. The author's own
comment concedes this is unproven ("RESEARCH assumption A3 deliberately does not assume trailing
placement"); the three real captures used as evidence all show `error` as the sole and final
event. If OpenCode ever emits a transient/recoverable `type:"error"` mid-run and continues to a
genuine later success, this parser would report `Failed` on an actually-successful stage.
**Fix:** Before relying on this beyond the current best-effort captures, get a live capture of a
recoverable mid-run error followed by success, and confirm whether OpenCode's process model
guarantees `error` is always terminal (as the module doc comment for `is_opencode_event_stream`
asserts). If that guarantee doesn't hold in some invocation mode, switch to last-event-wins
ordering like the rest of the module.

### WR-05: Real opencode session data (session IDs, per-call costs) committed verbatim as test fixtures

**File:** `crates/devflow-core/tests/fixtures/opencode/opencode_error.jsonl`,
`opencode_success.jsonl`, `opencode_success_with_marker.jsonl`, `opencode_tool_use.jsonl`
**Issue:** Per the developer's own code comments in `agent_result.rs` ("REAL, verbatim `opencode
run --auto --format json` captures"), all four fixtures are live captures from the operator's real
opencode sessions — each line carries a real `sessionID` (e.g.
`ses_fd1dda10effe6Q5bigMsfdoFqC`) and real per-call `cost` values. No API keys or credential
values are present, and opencode session IDs are local-only handles (not remote auth tokens), so
this is not a credential leak — but it is real operational metadata committed verbatim rather than
synthetic/redacted data.
**Fix:** If the IDs/costs have no test value, replace with placeholder values (e.g.
`ses_test000...`, round cost numbers). Low urgency — not a secret, but does reveal real session
activity.

### WR-06: `43-02-SUMMARY.md` overstates what the conformance-suite re-run actually covers

**File:** `.planning/phases/43-opencode-driver-completion/43-02-SUMMARY.md` (Accomplishments
bullet 5, coverage item D6)
**Issue:** The summary claims re-running `every_driver_passes_the_conformance_suite` now
"exercises `OpenCodeDriver`'s real `health`/`capabilities`/`parse_completion` bodies (previously
trivial trait defaults)." In fact that test (`crates/devflow-core/src/agents/mod.rs:286`) calls
`driver.test_contract()` → `contract_checks()` (`mod.rs:146-171`), which only calls `.name()`,
`.render_prompt()`, and `.build_command()` — never `.health()`, `.capabilities()`, or
`.parse_completion()`. The test does pass, but not for the reason credited. Real coverage of
health/capabilities/parse_completion exists — just in `opencode.rs`'s own `#[cfg(test)]` module
(16 tests) and `agent_result.rs`'s marker/error/torn-tail tests, not in the six-driver conformance
suite.
**Fix:** Correct the summary line to say the conformance suite still passes unchanged
(name/render_prompt/build_command contract only) and cite the actual location of
health/capabilities/parse_completion coverage.

## Info

### IN-01: `strip_ansi_escapes` silently drops the remainder of the buffer on a malformed/unterminated CSI sequence

**File:** `crates/devflow-core/src/agents/opencode.rs:122-138`
**Issue:** When an ESC `[` is found but no byte in the `0x40..=0x7E` final-byte range ever appears
before input ends (a truncated capture), the inner consuming loop runs to exhaustion with no
`break`, silently discarding every remaining character — including any terminal footer line that
would otherwise appear after the truncation point. The failure direction is safe (`health` then
reports "no credentials"), so this isn't incorrect per se, but it's silent full-buffer data loss
with no signal, on adversarial/truncated input the function is explicitly designed to be resilient
to.
**Fix:** Cap the inner consuming loop so a truncated capture degrades to "escape code left in
output" rather than "rest of buffer vanished." Low priority given the safe failure direction.

### IN-02: A regression test doesn't exercise the bracket-prefix guard it's named for

**File:** `crates/devflow-core/src/agents/opencode.rs` —
`agent_list_ignores_marker_text_inside_json_dump_line` (~line 503-507)
**Issue:** The function under test excludes lines starting with `[`/`{` before checking for a
trailing `(subagent)`/`(all)` marker. The test's crafted line (`"    \"description\": \"acts like
a (subagent) proxy\","`) doesn't start with `[`/`{` after trimming, so the bracket guard evaluates
to "don't exclude" regardless — the test passes purely because the line doesn't literally *end*
with `"(subagent)"`. If the bracket-prefix guard were deleted entirely, this test would still
pass.
**Fix:** Add a case where the line does NOT start with `[`/`{` but DOES end with the literal
marker text (e.g. `"    a fallback agent (subagent)"`) and assert it still doesn't flip the
result only once a real anchor (not the bracket guard) prevents it — this would catch a real
regression if the ends-with anchor is ever loosened.

### IN-03: Provider/environment-variable names captured live in a test constant

**File:** `crates/devflow-core/src/agents/opencode.rs`, `LIVE_PROVIDER_LIST_OUTPUT` constant
(test module, ~line 320)
**Issue:** Commented as "the real, live-verified `opencode providers list` output captured this
session," this constant lists the operator's actually-configured providers (Google/OpenAI/
DeepSeek/OpenRouter) and env var names (`DEEPSEEK_API_KEY`, `GOOGLE_API_KEY`,
`OPENROUTER_API_KEY`). No key values are present — only which providers/env-var names are
configured on the developer's machine (metadata, not a secret).
**Fix:** None required for security; could swap to fictional provider names if the operator wants
to avoid revealing their real provider mix.

---

_Reviewed: 2026-08-24_
_Reviewer: Claude (5 parallel angle-scoped passes: doc-accuracy, security/leaked-data, CI/build
correctness, external-state claims, generalist deep)_
_Depth: deep_

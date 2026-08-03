---
phase: 30-keep-the-session-alive-past-turn-end
plan: 05
subsystem: agent-result
tags: ["rust", "parser", "jsonl", "checkpoint", "security", "stream-json"]
status: complete

# Dependency graph
requires:
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30-01: claude_stream_events / is_claude_event_stream / last_top_level_result — the stream parser this plan branches on"
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30-02: 30c-VERDICT.md delivery: confirmed — the conditional gate this plan opens on"
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30-03: V3_INIT_EVENT / V3_RESULT_TURN1-3 / v3_result_event / stream_capture_of / v3_stream_capture — the fixture infrastructure this plan reuses rather than duplicates"
  - phase: 28-checkpoint-relay
    provides: "HUMAN_GATE_VALUE and text_reports_human_gate — the pure matcher, explicitly NOT modified here"
provides:
  - "claude_stream_reports_human_gate — top-level `result`-events-only gate scan, short-circuiting, returns bool"
  # DEPRECATED 2026-08-03: "gated on is_claude_event_stream" was superseded by 06675da, which
  # moved the stream branch onto the separate claude_stream_gate_shape predicate (closing the
  # gate-scoping fail-open on a torn init). This line was never amended when that landed.
  # Authoritative: agent_result.rs and 30-CODE-REVIEW.md. Kept, struck through, rather than
  # rewritten — a SUMMARY records what the plan delivered at the time it was written.
  - "blocking_human_checkpoint_reported stream branch — [SUPERSEDED: now gated on claude_stream_gate_shape, not is_claude_event_stream] never consults raw stdout on the stream path"
  - "The prompt-echo regression cluster: 7 tests pinning both the false-positive closure and the two opposite-direction harms"
affects: ["31-adapter-flip", "checkpoint-resume", "checkpoint_auto_decided"]

actuals:
  tokens: 5197
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Two independent filters (event type + provenance) kept even when one is currently redundant, so a later widening of either cannot silently inherit the excluded surface"
    - "Every negative test asserts a NEGATIVE CONTROL first — the raw capture must still match the pure matcher — so a fixture that loses its gate line fails loudly instead of passing vacuously"
    - "Detection semantics deliberately diverge from verdict semantics (all-results vs last-result), with the contrast written into both doc comments so the two are not 'harmonised' later"

key-files:
  created: []
  modified:
    - "crates/devflow-core/src/agent_result.rs"

key-decisions:
  - "The stream branch returns claude_stream_reports_human_gate's answer ALONE and never falls through to the raw-stdout scan — scanning both would reinstate the exact false positive being closed, since the echo lives in raw stdout by definition"
  - "assistant events are excluded entirely (cross-AI review's MEDIUM finding), not merely provenance-filtered — re-confirmed at execution time by re-parsing all 54 lines of raw_output_v3.jsonl"
  - "ALL top-level result events are scanned, not only the last — a checkpoint report asks 'did a gate fire anywhere in this stage', unlike a verdict, which is one final answer"
  - "RED was demonstrated before the fix by applying the test cluster to the unmodified source and observing 4 specific failures, then reverting the file and implementing — rather than committing a red intermediate state"
  - "The user-event fixture's payload STANDS IN for an echoed prompt rather than reproducing one, because no archived capture contains a prompt echo; this is recorded in the fixture's own doc comment, not just here"

requirements-completed: ["30b", "constraint-3", "constraint-6"]
---

# Phase 30 Plan 05: Scope the Stream Gate Scan to Agent-Authored Events — Summary

`blocking_human_checkpoint_reported` now takes a separate branch for a Claude
`stream-json` capture, answering from the `result` text of top-level `result`
events alone, so an operator prompt echoed back into the same stdout can no
longer read as a live checkpoint declaration.

## Conditional gate

Re-verified independently before any work, as the plan requires.
`30c-VERDICT.md` frontmatter line 3 reads `delivery: confirmed`, so the gate is
**OPEN** and the plan was executed as written. No operator answer was needed —
that branch applies only to `delivery: refuted`.

## What changed

One file, `crates/devflow-core/src/agent_result.rs`, purely additive (the diff
against the pre-plan base `6f343a6` contains **zero `-` lines**).

**`claude_stream_reports_human_gate(&[Value]) -> bool`** — two independent
filters, then the existing pure matcher:

1. **Type** — keep ONLY `result` events. `user` events are the echoed prompt or
   a re-injected `task_notification` summary; `system` events are the inert
   `init` inventory; `assistant` events are excluded outright per the cross-AI
   review's MEDIUM finding.
2. **Provenance** — keep only events whose `parent_tool_use_id` is JSON null
   **or absent**. The absent case is load-bearing: `result` events carry no such
   key at all.

Text is read with a direct `.get()` chain, never `json_scan`/`json_find_key`,
which would descend back into the content both filters just excluded. The
function short-circuits via `.any()` — no `String` accumulation.

**`blocking_human_checkpoint_reported`** gained a branch gated on
`is_claude_event_stream`. Its signature is unchanged and it remains `pub`. The
`else` path is the pre-existing two-target logic, byte-identical.

> **Superseded 2026-08-02, after this plan closed.** The branch is no longer gated
> on `is_claude_event_stream` — that predicate required a successfully parsed
> `system`/`init`, so one torn line reverted the whole capture to the raw-stdout
> scan and reinstated the prompt-echo false positive this plan exists to close
> (code review High 2, fixed in `06675da`). It is now gated on
> `claude_stream_gate_shape`, corrected again in `f34756c` after verification
> found it could be tripped by a single stray JSON line (V-01). See
> `30-CODE-REVIEW.md` and `30-VERIFICATION.md`. `is_claude_event_stream` still
> gates the verdict cascade, deliberately unchanged (T-30-02).

## Verification

### Conditional-gate and scope-fence checks

| Check | Result |
|---|---|
| `30c-VERDICT.md` frontmatter `delivery:` | `confirmed` (line 3) — gate open |
| `git diff --name-only HEAD~2` | `crates/devflow-core/src/agent_result.rs` only |
| `text_reports_human_gate` body vs pre-plan | **byte-identical** (sha256 `3ff02c9bf2fe` both sides) |
| `HUMAN_GATE_VALUE` const line vs pre-plan | **byte-identical** |
| `blocking_human_checkpoint_reported` signature | unchanged, still `pub` (the search string matched in both revisions) |
| `claude_stream_reports_human_gate` body | no `push_str` / `.join(` / `collect::<Vec` / `format!` / `to_string()` |
| Ten pre-existing checkpoint tests | **all ten byte-identical** to `6f343a6`, and all ten pass |

The symbol comparison carried a negative control: the same comparator reported
`blocking_human_checkpoint_reported` as **changed** (it was), and reported two
distinct symbol bodies as unequal — so "identical" is a real measurement, not a
comparator that returns True for everything. The accumulation scan likewise
fired on a deliberately-accumulating variant of the same body.

### Test counts

**Before this plan:** `test result: ok. 109 passed; 0 failed; 0 ignored; 0 measured; 366 filtered out`
(matches the count 30-03-SUMMARY.md recorded — nothing drifted between plans.)

**After this plan:** `test result: ok. 116 passed; 0 failed; 0 ignored; 0 measured; 366 filtered out`

**Net: +7 tests, 0 failures.** Workspace lib total: `test result: ok. 482 passed; 0 failed` (was 475).

### RED before GREEN

The cluster was applied to the **unmodified** source first. Four tests failed —
the four that assert the false positive is closed:

```
test result: FAILED. 112 passed; 4 failed; 0 ignored; 0 measured; 366 filtered out
```

```
blocking_human_checkpoint_reported_false_for_gate_text_in_user_event
blocking_human_checkpoint_reported_false_for_subagent_forwarded_gate_text
blocking_human_checkpoint_reported_false_for_top_level_assistant_narration
checkpoint_reported_in_capture_scopes_stream_gate_text_to_result_events
```

Every negative control held during that run (no test failed on its
"negative control" assertion), confirming the fixtures genuinely contain
matchable gate text and the failures came from scoping, not from absent input.
The three positives passed pre-fix, as expected — they guard against
overcorrection, not against the original defect.

The file was then reverted with `git checkout --` and the implementation
committed on its own, so no red commit exists in history.

### Per-test `--exact` invocations

Each new test, run individually. Quoted verbatim:

| Test | Result |
|---|---|
| `blocking_human_checkpoint_reported_false_for_gate_text_in_user_event` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |
| `blocking_human_checkpoint_reported_false_for_subagent_forwarded_gate_text` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |
| `blocking_human_checkpoint_reported_false_for_top_level_assistant_narration` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |
| `blocking_human_checkpoint_reported_true_for_top_level_result_declaration` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |
| `blocking_human_checkpoint_reported_true_when_only_first_result_declares_gate` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |
| `blocking_human_checkpoint_reported_true_when_echo_co_occurs_with_declaration` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |
| `checkpoint_reported_in_capture_scopes_stream_gate_text_to_result_events` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out` |

The ten pre-existing checkpoint tests were each run the same way; all ten
produced `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 481 filtered out`.

**Negative control on the `--exact` harness itself.** This repository has
already been burned by `cargo test --exact <name>` exiting 0 when the name
matches nothing. A deliberately bogus name was run through the identical
harness:

```
this_test_does_not_exist_negative_control
  cargo exit=0    test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 482 filtered out
```

The exit-0 trap is **confirmed live** on this toolchain, and the check
correctly classified it as NOT-A-PASS. Every `1 passed` above is therefore a
real pass, not an exit code.

### Full suite

```
==> cargo fmt --check
==> cargo clippy --workspace --all-targets -- -D warnings
==> cargo test --workspace --no-fail-fast
==> check.sh: all OK
```

`scripts/check.sh all` → **exit 0**.

## What this verification does NOT establish

- **The production path is not exercised.** The shipped adapter still emits
  `--output-format json` (`crates/devflow-core/src/agents/claude.rs`,
  `exec_command`), so nothing in production produces a stream capture yet. Every
  assertion here is a unit assertion against a constructed capture. The branch
  becomes reachable only when Phase 31 flips the launch path; until then this is
  a surface hardened in advance, not a defect observed and fixed in the field.
- **No fixture is a pure real capture.** No archived capture contains checkpoint
  gate text at all — the 30a harness prompt was about background tasks and never
  mentioned gates. Every gate payload here is **synthetic**, carried inside a
  **real** envelope copied from `raw_output_v3.jsonl` (lines 6, 10, 11 for the
  message events; 19/37/54 for the results, via 30-03's existing constants).
  Each fixture's doc comment states its source line and that the payload is
  synthetic.
- **The user-event fixture stands in for a prompt echo; it does not reproduce
  one.** Every `user` event in the archived capture is a `tool_result` relay,
  because the 30a harness ran one prompt with no re-injection. The substitution
  is sound for what is under test — the filter keys on the event's `type`, which
  is `user` either way — but the specific rendering an echoed prompt takes under
  `stream-json` remains **unobserved**. If it were ever emitted as something
  other than a `user` event, these tests would not catch it.
- **The gate VALUE rendering is observed; the surrounding declaration is not.**
  The markdown code span around `blocking-human` is transcribed from the live
  2026-07-31 A1 run. The `## CHECKPOINT REACHED` scaffolding around it in these
  fixtures is written to match the emitting template, not transcribed from a
  capture.
- **`scripts/check.sh` ran on the host, not in the pinned container.** The
  containerized parity wrapper is a separate script,
  `scripts/check-in-container.sh`, which was **not run** — the change is
  pure-Rust with no environment-sensitive surface, and pushing (which would fire
  the container gate via the pre-push hook) was explicitly out of scope. Host
  green is not container green.
- **`cargo clippy -p devflow-core --all-targets` fails on this tree** with
  `could not find test_support in devflow_core`. This is a pre-existing
  feature-unification artifact of the narrower invocation, not a defect
  introduced here — the workspace-level invocation `check.sh` actually uses is
  clean. Worth knowing before anyone reaches for the `-p` form.

## Deviations from Plan

### Task ordering (process, not scope)

The plan orders Task 1 (implementation) before Task 2 (tests), and both are
`tdd="true"`. Strict RED-first and that ordering cannot both hold in a
single-file plan without committing a red intermediate state. Resolved by
demonstrating RED on the working tree, reverting with
`git checkout -- <file>`, then committing implementation and tests in the
plan's stated order. Both the RED evidence and the plan's commit structure are
preserved; nothing was skipped, and the evidence is quoted above.

### Stale line references in the plan (no action needed)

The plan's `<read_first>` cites `lines 1613-1730` for the eight-test cluster and
`1826-1833` for the tempfile convention. At the pre-plan base `6f343a6` the
cluster actually starts at 1986 and runs to 2101, with the tempfile convention
at 2085-2101. Located by symbol name instead. Recording it so the next reader
does not chase the numbers.

### Fixture infrastructure reused, not rebuilt

The plan describes building envelopes from `raw_output_v3.jsonl` lines 19, 37
and 54 for the three-result positive. 30-03 had already committed exactly those
as `V3_RESULT_TURN1/2/3` with a `__MARKER__` sentinel and a `v3_stream_capture`
helper. Reused them rather than adding parallel copies. Three new constants were
added for the message-event envelopes (lines 6, 10, 11), which had no
pre-existing equivalents.

### STATE.md / ROADMAP.md not updated

Left to the orchestrator, matching this phase's established convention: the
per-plan completion commits (`650df5b` for 30-04) touch only the SUMMARY, while
STATE.md and ROADMAP.md are updated by separate wave-closing `docs(30):`
commits. The generic executor protocol would have updated them; this repo's
observed practice does not, and STATE.md's "Current Position" is presently on an
older phase, which is not mine to guess at.

## Auto-fixed Issues

None. No deviation rule fired — no bug, missing critical functionality, or
blocking issue was encountered.

## Known Stubs

None.

## Threat Flags

None. No new network endpoint, auth path, file access pattern, or schema change
at a trust boundary. The change narrows an existing trust boundary rather than
opening one.

## Commits

| Commit | Message |
|---|---|
| `1fee271` | `feat(30-05): scope stream gate scanning to top-level result events` |
| `5e08b22` | `test(30-05): add the prompt-echo regression cluster` |

## Self-Check: PASSED

- `crates/devflow-core/src/agent_result.rs` — FOUND, contains
  `fn claude_stream_reports_human_gate(events: &[serde_json::Value]) -> bool`
- Commit `1fee271` — FOUND in `git log`
- Commit `5e08b22` — FOUND in `git log`
- Working tree clean after both commits; `git diff --name-only HEAD~2` lists
  only the one permitted file

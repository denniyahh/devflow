---
phase: 30
type: handoff
for_phase: 31
subject: "ROADMAP constraint 9, item 1 — a torn terminal line resurrects an earlier turn's success"
source: "30-CODE-REVIEW.md, codex/gpt-5.6-sol high-effort code review, High finding 1"
status: "NOT FIXED — deliberately deferred, on the operator's instruction 2026-08-02"
line_refs_valid_at: 4763fb5
---

# H1 handoff — verdict resurrection on a torn terminal line

**Deliberately not fixed in Phase 30.** Deferred because the defect is latent: the
only entry point that reaches it, `evaluate_layer1`, currently has **zero callers
anywhere in the workspace**. Phase 31 is the phase that gives it one, so Phase 31
is where this becomes live. Fix it *before* wiring the launch path, not after.

Everything below was verified by reading current source, independently of the
review that found it. Line numbers are valid at commit `4763fb5`; they have
already shifted once this phase, so locate by symbol name.

## The defect in one paragraph

`claude_stream_events` silently discards any line that fails to parse. If the
capture's **final** `result` line is torn, it vanishes, and
`last_top_level_result` returns an **earlier** turn's result instead. If that
earlier turn reported success, Layer 1 reports success — authoritatively — for a
session whose real terminal turn failed or raised a gate. The stage advances.

## The mechanism, step by step

| # | Symbol | Line | What it contributes |
|---|---|---|---|
| 1 | `claude_stream_events` | `agent_result.rs:696` | `.filter_map(… .ok())` at **line 700** drops unparseable lines with no record that anything was dropped |
| 2 | `last_top_level_result` | `agent_result.rs:785` | `.iter().rev().find(type == "result")` — takes the last *surviving* result, which is not necessarily the last *emitted* one |
| 3 | `parse_claude_event_result` | `agent_result.rs:1062` | reads the marker from that result; a `Success` marker does **not** early-return, and `claude_stream_envelope_failure` is then evaluated against the same (successful, `is_error:false`) event, so nothing contradicts it |
| 4 | `evaluate_layer1` | `agent_result.rs:1167` | an `.or_else()` cascade — the first `Some` wins, so a stale `Some(Success)` **short-circuits the exit-code fallback that would otherwise correct it** |

Step 4 is what makes this consequential rather than cosmetic. Layer 1 does not
merely guess wrong; it prevents the layer below from disagreeing.

## Reproduction

A three-line capture. The final line is truncated mid-JSON — no closing brace:

```jsonl
{"type":"system","subtype":"init","session_id":"s"}
{"type":"result","result":"DEVFLOW_RESULT: {\"status\":\"success\"}","is_error":false}
{"type":"result","result":"DEVFLOW_RESULT: {\"status\":\"failed\"}","is_error":true
```

Observed by the reviewer through the public API:

```text
truncated_final_resurrects_success = Some(Success)
```

Control — the identical capture with the final line closed properly:

```text
valid_final_failure_control = Some(Failed)
```

### Why this is not a contrived input

The capture is written to `.devflow/phase-NN-stdout` by **raw `sh` redirection**,
and Layer 1 reads it with `std::fs::read` while the agent may still be appending
(`evaluate_layer1`, `agent_result.rs:1167` — see its own comment about lossy
UTF-8 reads for the same reason). Reading a file that is concurrently being
appended to is *precisely* the condition that yields a torn last line. The
reviewer explicitly flagged that it did **not** establish how often the writer
leaves a torn terminal line — that frequency is still unmeasured, and measuring
it is a reasonable first task for Phase 31.

## A ready-to-drop-in failing test

Not added to the tree (Phase 30 is not fixing this). Written against the existing
fixture helpers in the `tests` module so it should compile as-is:

```rust
/// ROADMAP constraint 9, item 1: a torn terminal result must not let an
/// earlier turn's success become the stage verdict.
#[test]
fn torn_terminal_result_does_not_resurrect_earlier_success() {
    let capture = format!(
        "{}\n{}\n{}",                       // note: final line deliberately unterminated
        V3_INIT_EVENT,
        v3_result_event(V3_RESULT_TURN1, MARKER_SUCCESS),
        &v3_result_event(V3_RESULT_TURN2, MARKER_FAILED)[..80],
    );
    let got = parse_claude_event_result(&capture);
    assert_ne!(
        got.map(|r| r.status),
        Some(AgentStatus::Success),
        "a truncated terminal result must not leave an earlier success authoritative"
    );
}
```

Pair it with the positive control (same capture, final line intact → `Failed`),
or the test passes for the wrong reason.

## What a fix has to satisfy

1. **Malformed input after the last valid terminal result must make the stream
   indeterminate or failed.** It must never let an earlier result become
   authoritative. Returning `None` from `parse_claude_event_result` is *not*
   sufficient on its own — check what the rest of the `or_else` cascade then does
   with the same stdout, because `parse_devflow_result` scans it too.
2. **`claude_stream_events` must stop discarding silently.** It currently cannot
   express "I dropped something"; every caller is therefore structurally unable
   to distinguish a clean parse from a lossy one. This is the root cause shared
   with constraint 9's item 2 and with the gate fail-open already fixed in
   `06675da` — that fix worked around the silence rather than removing it.
3. **Do not fix this by widening `is_claude_event_stream`.** T-30-02: it gates
   the verdict cascade, and admitting more shapes there displaces
   `parse_devflow_result` and changes shipped Layer-1 behaviour. Phase 30 added
   a separate gate-path predicate (`claude_stream_gate_shape`,
   `agent_result.rs:759`) precisely to avoid touching it — follow that pattern.

## Related, same handoff

**Constraint 9 item 2** — `last_top_level_result` (`agent_result.rs:785`) selects
solely on `type == "result"` and never consults `parent_tool_use_id`, despite its
name and its doc comment's T-30-01 claim. Gate scanning enforces provenance;
verdict selection does not. A subagent-origin `result` would be admitted and
would decide the stage. **Fix both together** — they are the same function chain,
and the review's recommendation was explicitly to share ONE top-level predicate
between gate and verdict selection rather than maintain two divergent notions.

## Evidence limits carried forward

- No archived capture contains a subagent-origin `result`, so item 2's
  likelihood is **unverified**; the wrong behaviour is deterministic if that
  shape occurs.
- Independently re-counted, the three archived captures hold 12/25/54 events and
  **0**/2/3 `result` events. Plan 30-05's claim that absent `parent_tool_use_id`
  was "confirmed across all three captures" is therefore **partly vacuous** — the
  first capture evidences nothing. Treating absent as top-level remains
  *necessary* for today's results and *unproven safe*.
- Every probe payload above is synthetic. No archived capture contains gate text
  or a prompt echo.

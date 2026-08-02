---
phase: 30
type: code-review
reviewers: [codex]
reviewer_note: "Ad-hoc single-reviewer pass — codex invoked directly as `codex exec -m gpt-5.6-sol -c model_reasoning_effort=high`, confirmed in the run log as `model: gpt-5.6-sol` / `reasoning effort: high`. Not run through the standard /gsd-review reviewer-instance mechanism. This is a CODE review; 30-REVIEWS.md is the earlier PLAN review."
reviewed_at: "2026-08-02T19:19:00Z"
reviewed_range: "develop...feature/phase-30, code surface crates/devflow-core/src/agent_result.rs"
head_at_review: 7ede2f4
verdict: REQUEST CHANGES
findings: {critical: 0, high: 2, medium: 2, low: 0}
---

# Cross-AI Code Review — Phase 30

> Single-reviewer pass (codex / gpt-5.6-sol / high effort). No cross-model
> consensus is available — treat each finding as one independent, source-grounded
> verdict. Every finding below was **re-verified independently** against the
> source before disposition; those verifications are recorded per finding.

## Review conditions worth recording

`HEAD` moved three times during the review (`7ede2f4` → `5ea2404` → `cfecf49`)
because the orchestrator committed the post-commit hook and a STATE.md update
while the review ran. The reviewer detected this, confirmed `agent_result.rs` was
byte-identical across those commits, and confirmed it modified no tracked files.
The findings stand, but the branch moved under the review — avoid this next time.

The reviewer also re-counted the archived captures independently (`jq`/`rg`):
**12, 25 and 54 valid events; 0, 2 and 3 `result` events; none carrying
`parent_tool_use_id`.** That surfaced a correction to plan 30-05's own wording:
its claim that absent-`parent_tool_use_id` was "confirmed across all three
archived captures" is **partly vacuous** — the first capture contains no `result`
events at all.

## Dispositions

| # | Severity | Finding | Verified? | Disposition |
|---|---|---|---|---|
| H2 | High | Malformed/missing `init` re-enables raw prompt scanning | **Confirmed** — mechanism read directly | **FIXED** in `06675da` |
| H1 | High | Malformed terminal JSON resurrects an earlier success verdict | **Confirmed**, but **unreachable** | Deferred to Phase 31 |
| M2 | Medium | `last_top_level_result` does not enforce top-level provenance | **Confirmed** by direct read | Deferred to Phase 31 |
| M1 | Medium | Any documentary gate mention in any result is a declaration | Mechanism real; out of 30-05's scope fence | Backlog |

### H2 — fail-open on a torn `init` — FIXED

`claude_stream_events` drops unparseable lines (`filter_map(…ok())`), and gate
recognition required a *parsed* `system`/`init`. One torn line made the capture
unrecognised, so `blocking_human_checkpoint_reported` fell back to scanning raw
stdout — which under a stream capture contains the echoed prompt. The
constraint-3 scoping failed **open**, into the exact false positive it exists to
close. Its consumer is live (`pipeline_launch.rs:491`).

Fixed by `claude_stream_gate_shape`, a gate-path-only predicate keyed on
`system`/`user`/`assistant`. `is_claude_event_stream` was deliberately **not**
widened — it also gates the verdict cascade, where admitting more shapes would
displace `parse_devflow_result` (T-30-02). Both regressions were verified RED
against the old predicate before the fix, and three over-correction controls
(plain text, single-document envelope, Codex stream) assert those inputs still
take the raw path and still detect a real gate.

### H1 — verdict resurrection on a torn terminal line — DEFERRED, and downgraded

Confirmed at mechanism level: a truncated terminal `result` is dropped,
`last_top_level_result` returns an *earlier* turn's result, and because
`evaluate_layer1` is an `.or_else()` cascade a stale `Some(Success)`
short-circuits the exit-code fallback. A stage would advance after the real
terminal turn failed.

**However — `evaluate_layer1` has zero callers anywhere in the workspace**
(verified by `grep` with an identical-shape negative control against
`checkpoint_reported_in_capture`, which has one). The reviewer rated this High
without checking reachability. It is a `pub` function with no consumer today, so
this is a **latent trap for whoever wires it up**, not a live defect. It belongs
to Phase 31, which owns the launch-path flip.

### M2 — `last_top_level_result` name/behaviour mismatch — DEFERRED

Confirmed by direct read (line 742): it selects solely on `type == "result"` and
never consults `parent_tool_use_id`, despite the name and the doc comment's
top-level claim. So **gate scanning enforces provenance while verdict selection
does not** — a subagent-origin `result` would be admitted. Same reachability
caveat as H1. No archived capture contains a subagent-origin `result`, so
likelihood is unverified; the wrong behaviour is deterministic if that shape
occurs.

### M1 — documentary mention vs declaration — BACKLOG

Event filtering establishes *authorship*, not *intent*. A result that merely
documents a gate ("the plan documents **Gate:** `blocking-human`; no checkpoint
was reached") still trips the matcher, and because all results are scanned, one
documentary mention stays decisive after later quiet turns.

Real, but outside 30-05's scope fence, which explicitly forbade modifying
`text_reports_human_gate` or `HUMAN_GATE_VALUE` — that matcher encodes a
hard-won live observation. Closing this means requiring declaration *framing*
(the checkpoint heading plus the gate field), not just the token. Backlog.

## What this review does not establish

Stated by the reviewer, and worth preserving because it bounds every finding
above:

- **No probe used a real capture.** Every fixture payload is synthetic, because
  no archived capture contains checkpoint gate text — and none contains a prompt
  echo at all. The findings prove the mechanisms are deterministic; they do not
  establish that Claude emits these shapes.
- Support for array-valued / `content`-only / missing `result` fields is
  unevidenced — all archived results are strings.
- Parsing allocates a `Vec<Value>` linear in capture size; not load-tested, no
  exploitable threshold established.
- Not run: a new live capture containing a real prompt echo or checkpoint,
  fuzz/property tests, concurrent torn-read testing, large-capture benchmarks.
- The green suite before this review did **not** cover malformed terminal
  results, malformed-init fallback, documentary result text, or subagent
  `result` events.

## Checks the reviewer ran

Read `agent_result.rs`, its caller in `pipeline_launch.rs`, the launcher,
harnesses, and all three archived JSONL captures; independently parsed the
captures with `jq` and counted results with `rg`;
`cargo test -p devflow-core --lib blocking_human_checkpoint_reported` (14
passed); `cargo test -p devflow-core --features test-support` (482 unit + 4
integration passed); `cargo check -p devflow-core`; `git diff --check
develop...HEAD`. Public-API adversarial probes reproduced every reported
behaviour with opposite-result controls.

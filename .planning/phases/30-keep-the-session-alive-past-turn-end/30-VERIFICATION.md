---
phase: 30-keep-the-session-alive-past-turn-end
verified: 2026-08-02T21:30:00Z
status: human_needed
score: 8/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
verdict: PARTIAL
verdict_note: >-
  PASS on Phase 30's own stated goal — all 8 derived truths verified against source
  and re-executed commands, not against SUMMARY claims. PARTIAL overall because one
  previously-unrecorded fail-closed exposure was found on a LIVE path during this
  verification, and its disposition is an operator decision, not the verifier's.
yardstick: "Phase 30's own goal (ROADMAP.md §Phase 30 'Goal:'), NOT the 999.64 arc goal — justified in §Yardstick below"
human_verification:
  - test: >-
      Decide the disposition of V-01: a single JSONL-shaped `system`/`user`/`assistant`
      line anywhere in a NON-stream capture suppresses gate detection for the whole
      capture. Reproduced by this verifier with both controls (see §V-01). Options:
      (a) file as a Phase 31 input alongside constraint 9, (b) file as a numbered
      backlog entry like 999.70, (c) fix now in Phase 30, (d) accept and document
      as a residual in `claude_stream_gate_shape`'s doc comment.
    expected: >-
      An explicit recorded choice. Recommendation: (a) — Phase 31 owns the predicate's
      future (once stream-json is always-on the raw path is dead), and the trigger is
      currently implausible under the shipped `--output-format json` adapter.
    why_human: >-
      A scope decision on a phase already complete, on a fix that was itself an
      out-of-plan code-review remediation. Not the verifier's call.
deferred:
  - truth: "The prompt-echo false positive is closed as WITNESSED rather than reasoned"
    addressed_in: "Phase 31"
    evidence: >-
      No archived capture contains a prompt echo or gate text, so no fixture can be a
      real capture. A witnessing capture only exists once Phase 31 flips the launch
      path to `--output-format stream-json` (ROADMAP: 'the always-on adapter switch').
  - truth: "Layer-1 verdict selection survives a torn terminal result line"
    addressed_in: "Phase 31"
    evidence: >-
      ROADMAP binding constraint 9 item 1, added 2026-08-02: 'Phase 31 must not wire up
      `evaluate_layer1` until two confirmed defects behind it are closed.'
  - truth: "`last_top_level_result` enforces top-level provenance via `parent_tool_use_id`"
    addressed_in: "Phase 31"
    evidence: "ROADMAP binding constraint 9 item 2, same paragraph."
---

# Phase 30: Keep the Session Alive Past Turn End — Verification Report

**Phase Goal (verbatim, ROADMAP.md):** *the parser and the feasibility gate — Layer 1 can
read a Claude `stream-json` (JSONL) capture (verdict, rate limit, envelope failure,
`session_id`, checkpoint detection) without regressing the shipped single-document path by a
single test, and the two premises Phase 31 would rest on each have archived evidence and a
recorded verdict: (a) whether `task-notification` delivery survives DevFlow's real launch
environment (`30c-VERDICT.md`), and (b) what the CLI does on stdin close, both drained and
with pending background tasks (`30d-MEASUREMENTS.md`).*

**Verified:** 2026-08-02 · **HEAD:** `cbe8ec3` · **Branch:** `feature/phase-30`
**Verdict:** PARTIAL — goal achieved; one new finding needs an operator decision.

---

## Yardstick — why Phase 30's own goal, not the 999.64 arc goal

Judged independently, not accepted on assertion. **Phase 30's own goal is the correct
yardstick.** Three reasons, in descending weight:

1. **The arc goal is unsatisfiable by construction under this phase's own scope fence.** All
   five plans carry a `<scope_fence>` forbidding any change to `monitor.rs`,
   `agents/claude.rs` or `pipeline_launch.rs`, and I confirmed the fence held (§Scope Fence).
   The arc goal — "a DevFlow-driven phase containing a multi-plan wave completes that wave
   without orphaning delegated work" — requires the pipe-owning monitor those files hold.
   Measuring Phase 30 against it would fail the phase for obeying its own instructions.
2. **The split moved no work.** ROADMAP's "Goal reconciliation" paragraph states the scope of
   neither phase changed, only which goal each is measured against. I checked this against the
   plan list: 30-01/03/05 (parser), 30-02 (30c), 30-04 (30d) are the same five units the entry
   named before the split. Nothing was descoped into Phase 31 to make Phase 30 passable.
3. **The arc goal is not verifiable here even in principle.** Its acceptance criterion is a
   live Phase 29 wave-2 re-run, which the ROADMAP explicitly says is *"not substitutable by
   integration tests."*

**What accepting this yardstick costs, stated plainly:** a PASS on Phase 30 is **not** a claim
that 999.64 is fixed. Nothing in this phase makes an unattended run work today. The v2.3.0
milestone closes only when Phase 31 lands. A reader who reads "Phase 30 verified" as "the
orphaning bug is gone" has read it wrong.

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Layer 1 reads a stream capture's **verdict** | ✓ VERIFIED | `parse_claude_event_result` (`agent_result.rs:1062`) wired into `evaluate_layer1`'s cascade at `:1179`, between `detect_claude_envelope_failure` and `parse_devflow_result`. `evaluate_layer1_parses_claude_stream_capture -- --exact` → `1 passed; 0 failed; 487 filtered out`. Last-result semantics confirmed by direct read (`.iter().rev().find(...)`, `:785`). |
| 2 | Layer 1 reads a stream **rate limit** | ✓ VERIFIED | `detect_claude_stream_rate_limit` (`:928`), ordered ahead of marker and envelope in the documented 5-step precedence. Cluster `agent_result::tests::claude_stream` → `16 passed; 0 failed; 472 filtered out`. |
| 3 | Layer 1 reads a stream **envelope failure** | ✓ VERIFIED | `claude_stream_envelope_failure` (`:981`); `is_error:true` overrides a *held* success marker (`:1087`), matching the single-document rule. Same 16-test cluster. |
| 4 | Layer 1 reads a stream **`session_id`** | ✓ VERIFIED | `claude_stream_session_id` (`:301`) wired stream-first into `session_id_from_capture` (`:335`). Cluster `claude_stream_session_id_` → `4 passed; 0 failed; 484 filtered out`. **This path is LIVE** — one caller, `pipeline_launch.rs:443`. |
| 5 | Layer 1 does **checkpoint detection**, hardened against prompt echo | ✓ VERIFIED | `claude_stream_reports_human_gate` (`:853`), `result`-events-only; branch gated on `claude_stream_gate_shape` (`:759`). Cluster `blocking_human` → `16 passed; 0 failed; 472 filtered out`, incl. `blocking_human_checkpoint_reported_false_when_init_is_torn` (`1 passed`), which carries a fixture precondition, a negative control and a positive control. **LIVE** — one caller, `pipeline_launch.rs:491`. See V-01. |
| 6 | **No regression** of the shipped single-document path, by a single test | ✓ VERIFIED | Strongest evidence in the phase. `git diff develop...HEAD -- agent_result.rs` deletes **6 lines total**: 5 doc-comment lines + 1 dispatch line. **Zero** test functions removed (`comm` over sorted fn-name sets, 93 → 135, empty removal set). `scripts/check.sh all` → **exit 0**, `487 passed; 0 failed; 1 ignored`. Four named isolation tests each `1 passed`: `claude_stream_wiring_leaves_single_document_capture_unchanged`, `single_doc_envelope_not_consumed_by_claude_stream_parser`, `plain_text_not_consumed_by_claude_stream_parser`, `codex_stream_not_consumed_by_claude_stream_parser`. |
| 7 | Premise (a): `task-notification` delivery in the real launch env has **archived evidence + recorded verdict** | ✓ VERIFIED | `30c-VERDICT.md` frontmatter `delivery: confirmed`. **Independently recounted from raw JSONL by this verifier**: 55 events / 0 unparseable / 3 `result` / 2 `origin.kind=="task-notification"` — exact match to the frontmatter, with a negative control (a bogus `origin.kind` returned 0). Scrubbed trial recounted at 2 results / 1 notification, corroborating constraint 7's coalescing claim. Blocking operator sign-off recorded verbatim (`30-02-SUMMARY.md:123-128`, "approved"), plus an 8th trial run by the operator from a plain fish shell — ancestry-independent, which retires the nested-session confound rather than scrubbing around it. |
| 8 | Premise (b): stdin close, **both drained and pending**, has archived evidence + recorded verdict | ✓ VERIFIED | `30d-MEASUREMENTS.md`. Mode A (drained) n=5, per-trial archived: 169.5–279.7 ms, median 242.0. Mode B (pending) n=2, eleven independent per-trial fields each, both `process_exited: true`, `results_after_close: 2`, `final_result_truncated: false`, both child signal files present. Evidence dirs present on disk with `timings.json` per trial. The measurement **refutes** the previously-cited 0.38 s figure rather than confirming it — a result that argues against the phase's own prior claim, which is a good sign about the instrument. |

**Score: 8/8 truths verified** (0 present-but-behavior-unverified, 0 overrides).

### Phase-binding review constraints (2, 3, 6)

ROADMAP assigns constraints 2, 3 and 6 to this phase; 1, 4, 5 and 9 to Phase 31.

| Constraint | Status | Evidence |
|---|---|---|
| 2 — parse layer lands in `agent_result.rs`, monitor/adapter untouched | ✓ VERIFIED | Scope fence held — see §Scope Fence. |
| 3 — parser owns verdict/rate-limit/envelope/`session_id`/checkpoint under JSONL; last-result never first; 4000-char window handled | ✓ VERIFIED | Truths 1–5. The 4000-char tail issue is resolved by scoping `parse_marker_lines` to the *decoded* `result` field (`:1074-1077`), documented at `:1055-1061`. |
| 6 — evidence gaps M1 and M4 closed in-phase | ✓ VERIFIED | M4 = 30c run through a `spawn_monitor` process replica (truth 7). M1 = 30d exit timing re-measured and archived, incl. the previously-undefined pending-tasks case (truth 8). M2 (CLI pinning) and M3 (near-simultaneous completions) are ROADMAP-assigned to Phase 31. |

### Scope Fence

`git diff --stat develop...HEAD` — **no file under `crates/devflow-core/src/monitor.rs`,
`crates/devflow-core/src/agents/`, or `crates/devflow-cli/src/` is modified.** The only Rust
file touched anywhere in the branch is `crates/devflow-core/src/agent_result.rs`. Fence held.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Phase test module | `cargo test -p devflow-core --lib agent_result::` | `121 passed; 0 failed; 1 ignored; 366 filtered out` | ✓ PASS |
| Repository definition of green | `scripts/check.sh all` | `487 passed; 0 failed; 1 ignored` · **exit 0** | ✓ PASS |
| The ignored test genuinely documents an open defect | `... truncation_sweep_never_upgrades_verdict_to_success -- --ignored --exact` | **FAILED** — `truncating to 1120 bytes resurrected an earlier turn's SUCCESS` | ✓ PASS (failure is the intended, documented outcome) |
| `evaluate_layer1` really has zero callers | `rg` over `crates/`, excluding its own file | **0 hits**, against identical-shape controls `checkpoint_reported_in_capture` (1 hit, `pipeline_launch.rs:491`) and `session_id_from_capture` (1 hit, `:443`) | ✓ PASS |
| `--exact` trap control | bogus test name `-- --exact` | `0 passed; 0 failed; 488 filtered out`, **exit 0** | ✓ PASS (trap reproduced; every count above therefore read, not inferred) |
| Evidence recount vs frontmatter | `json.loads` per line over 3 captures + a bogus-key control | matches frontmatter exactly; control = 0 | ✓ PASS |
| Debt markers in changed source | `rg TODO\|FIXME\|TBD\|XXX\|HACK crates/` | 0, with a regex control (`\bfn\b` → 185 hits in the same file) | ✓ PASS |
| Scrubbing of phase-30 evidence | `rg denniyahh` over `30c-evidence*` and `30d-evidence` | 0 hits, against a control that DOES hit `30a-evidence/*.jsonl` | ✓ PASS |

The ignored test is a well-built fail-first artifact: it asserts an intact-capture
precondition first, then sweeps **every** char-boundary truncation offset. It fails for the
right reason and cannot pass vacuously.

---

## V-01 — NEW FINDING: a single JSONL-shaped line suppresses gate detection on a live path

**Not recorded anywhere in the phase's artifacts.** Found by adversarial probe during this
verification, reproduced through the public API with both controls, then removed (tree clean).

**Mechanism.** The H2 remediation (`06675da`) replaced the gate path's precondition with
`claude_stream_gate_shape` (`agent_result.rs:759`), which returns `true` if **any** line in
the capture parses as JSON with `type` ∈ {`system`, `user`, `assistant`}. When it fires,
`blocking_human_checkpoint_reported` returns the stream scan's answer and **never consults raw
stdout** (`:528-539`). So one stray JSONL-shaped line in an otherwise non-stream capture
diverts the whole capture onto a scan that finds nothing.

**Reproduction (verifier's own probe, controls in both directions):**

| Input | Result |
|---|---|
| Plain-text stdout containing ``**Gate:** `blocking-human` `` | `true` — **positive control passes** |
| Same stdout **+ one line** `{"type":"assistant","message":{"content":[]}}` | **`false`** — real gate suppressed |
| Shipped single-document envelope carrying the same gate text | `true` — **unaffected-shape control passes** |

**Direction of the trade.** Before `06675da` this input took the raw path and the gate WAS
detected. The fix converted a fail-**open** (echoed prompt reads as a live gate) into a
fail-**closed** (a stray line hides a real gate). Fail-closed is the better direction, and the
fix's three over-correction controls (plain text, single-doc envelope, Codex stream) are real —
but none covers a **mixed** capture, which is precisely the class the widened predicate created.

**Why this is NOT a blocker, stated as carefully as the finding itself:**

- The call site is agent-gated: `state.agent == AgentKind::Claude` (`pipeline_launch.rs:490`),
  so Codex plain-text captures — the most plausible carrier — never reach it.
- Under today's shipped `--output-format json` adapter the capture is one JSON document whose
  `type` is `result`; agent text lives *inside* the JSON string and cannot become its own
  physical line. I could construct no realistic trigger on the shipped adapter.
- It additionally requires `phase_has_blocking_human_checkpoint` to be true, and the
  consequence is a fall-through to per-stage dispatch — a stall, i.e. pre-Phase-28 behaviour.
- Under Phase 31 the exposure inverts and disappears by design: stream-json always-on makes
  `gate_shape` always true and the raw path dead.

**Why it is still worth an explicit decision:** it is on a live path, it was introduced by
this phase, and it is undocumented — `claude_stream_gate_shape`'s doc comment records a
*different* residual (`:756-758`). This phase's own history is a green suite over a live
fail-open in exactly this control; leaving a second undocumented residual there unremarked
would repeat the pattern. See `human_verification` in frontmatter for the four options.

---

## Warnings — recorded claims that do not match the codebase

| # | Warning | Evidence |
|---|---|---|
| W-01 | **"The stream parser is unreachable in production" is too broad.** True of the *verdict* cascade (`evaluate_layer1`, 0 callers). **False** of two of the five capabilities: checkpoint detection (`pipeline_launch.rs:491`) and `session_id` (`:443`) are wired and live today. The code review itself depended on the distinction — H2 was fixed *because* "its consumer is live". A Phase 31 reader taking the blanket phrasing at face value would mis-scope regression risk. Appears in `STATE.md` and `30-VALIDATION.md` §"What this validation does NOT establish". | `rg` caller counts, above |
| W-02 | **`30-05-SUMMARY.md` is stale on its own headline claim.** Its `provides` says the stream branch is "gated on `is_claude_event_stream`". Source gates it on `claude_stream_gate_shape` since `06675da`. The SUMMARY was never amended; the correct account lives only in `30-CODE-REVIEW.md` and the source doc comment. | `agent_result.rs:530` vs `30-05-SUMMARY.md` frontmatter |
| W-03 | **`STATE.md` reports `119 passed / 0 failed`** for `agent_result::`. Actual at HEAD is `121 passed / 1 ignored`, after `e422081` (truncation sweep) and `cbe8ec3`. Two commits stale. | measured above |
| W-04 | **Out-of-plan changes rode the branch.** `scripts/hooks/post-commit` (new, 82 lines, `5ea2404`), `CLAUDE.md` (new), `CONTRIBUTING.md`, `.gitignore`, `.planning/config.json`, and untracking `UPSTREAM-GSD-ISSUES.md`. None violates the source scope fence and none is attributable to any of the five plans. `5ea2404` landed *during* the code review and is what moved HEAD under the reviewer — the review records this and asks that it not recur. Flagged for ship-time review scope. | `git diff --stat develop...HEAD` |
| W-05 | **`30a-evidence/*.jsonl` carry home paths, OS usernames and session identifiers** — matching the phase's cross-cutting redaction constraint's prohibition. Confirmed present. **Not a phase-30 gap:** those files are on `develop`, predate this branch, and the leak is filed as backlog 999.69 / DEN-90 with a proven fix. Phase 30's *own* archived evidence (30c/30d) is clean. Recorded so it is not mistaken for compliance. | `git ls-tree develop`, ROADMAP:1470 |

None of W-01..W-05 is a goal failure. W-01 and W-02 are the two most worth fixing before
Phase 31 plans, because Phase 31 will read exactly those two documents.

---

## Deferred — open items owned by Phase 31, correctly gated

These are **not** Phase 30 gaps. Each is a confirmed defect or unclosed evidence gap that
Phase 30 deliberately did not close, with a binding mechanism preventing it from going live.

| # | Item | Gate | Verified by this verifier |
|---|---|---|---|
| 1 | Torn terminal result resurrects an earlier success (constraint 9 item 1) | ROADMAP constraint 9 forbids Phase 31 wiring `evaluate_layer1` until closed; `30-H1-CONTEXT-FOR-31.md` carries the handoff and a drop-in failing test | Reproduced: the ignored test FAILS under `--ignored` at truncation offset 1120. Unreachability re-confirmed with an identical-shape control. |
| 2 | `last_top_level_result` ignores `parent_tool_use_id` (constraint 9 item 2) | Same constraint | Confirmed by direct read (`:785-790`) — selects solely on `type == "result"`, contradicting its own doc comment at `:782-784`. |
| 3 | Gate mention vs gate declaration | Backlog 999.70 / DEN-91 | Entry exists and is substantive |
| 4 | Is the capture writer actually capable of tearing a terminal line? | Backlog 999.71 / DEN-92 | Entry exists; this is the unmeasured frequency the code review flagged |
| 5 | Prompt-echo closure is reasoned, not witnessed | Phase 31 produces the first stream captures | Confirmed: every gate fixture is labelled SYNTHETIC in-source (12 explicit sites); no archived capture contains gate text or a prompt echo |

---

## What this verification does NOT establish

Stated at the same level of care as the findings, because a `passed`/8-of-8 line is easy to
over-read — and this phase has already been burned once by a green suite over a live defect.

1. **That the stream parser is correct in production.** It has never processed a real
   production capture. Its verdict path has zero callers; nothing has ever exercised it outside
   tests and fixtures. Correctness is established against *fixtures*, not against Claude.
2. **That Claude emits the shapes the tests assert.** Every gate and rate-limit payload is
   synthetic. Real event *envelopes*, synthetic *payloads*. No archived capture contains gate
   text, and none contains a prompt echo at all. The constraint-3 false positive is closed as
   reasoned; it has never been witnessed.
3. **That 999.64 is fixed, or closer to fixed in any observable way.** Phase 30 delivered a
   parser nothing calls and two experiments. An unattended run fails today exactly as it did
   before this phase.
4. **That the 30c delivery premise is reliable, only that it reproduced.** 8 trials across 3
   environments is a weak reliability bound — it establishes the benign path exists, not that
   it is the only path. The behaviour is undocumented, unpinned upstream CLI behaviour on
   `claude_code_version 2.1.220`; a CLI update can invalidate it silently. Phase 31 owns the
   version smoke-detection (M2) that would catch that.
5. **That close-with-pending-tasks is benign.** n=2. 30d's own text says so. The measurement
   defines the behaviour observed twice; it does not bound the distribution. The
   partial-drain case (one child drained, one not) is explicitly untested.
6. **That my V-01 probe found the only such residual.** I probed one hypothesis about one
   predicate, chosen by reading the diff. I ran no fuzzing, no property tests, no concurrent
   torn-read testing, and no load testing — the same four gaps the code review listed and did
   not close. A second adversarial pass with a different hypothesis could well find more; the
   base rate on this file is now two findings from two independent adversarial passes.
7. **That `scripts/check.sh all` green means CI green.** It was run on this host only. Pinned-
   container parity is claimed in `30-VALIDATION.md` (`485 passed`) but I did **not** re-run
   `scripts/check-in-container.sh`; that claim is carried forward unverified by me.
8. **That the exit-latency numbers generalize.** Mode A's 169.5–279.7 ms is one machine, one
   CLI version, one two-child workload, n=5.

---

_Verified: 2026-08-02 · HEAD `cbe8ec3`_
_Verifier: Claude (gsd-verifier) — every count above was read from a re-executed command, never
from a SUMMARY, and every scan carries a negative control._

---

## Addendum — 2026-08-02, after the root-cause refactor (`a557805`)

Two rows above are corrected; the rest of this report stands.

1. **"`evaluate_layer1` really has zero callers — ✓ PASS" was a false pass.** The scan excluded
   `agent_result.rs` itself, and `evaluate_agent_result` — in that file — runs `evaluate_layer1`
   on every result evaluation, called live from `pipeline_launch.rs:416`. The negative control was
   sound for the claim "no references outside the file"; the row's conclusion claimed more than
   the control covered. What actually kept H1/M2 latent was the capture format (init-gated stream
   branch vs shipped single-document `json`), not a missing caller. This report's own W-01 warning
   ("unreachable in production is too broad") was the same error surviving one layer down.

2. **V-01's disposition changed after this report.** It was fixed (`f34756c`), that fix was found
   defective by the fourth adversarial pass, and both were subsumed by the refactor: classification
   is now a single `classify()` with the majority-json-shaped rule. The refactor also CLOSED
   constraint 9 items 1 and 2 (deferred at verification time) and named the surviving residual:
   boundary-clean truncation is undetectable from capture content, so Phase 31 must not let a
   stream-derived Success short-circuit a contradicting exit code.

The base-rate observation in the original report — "two findings from two passes argues a third
pass would find a third" — was borne out: pass 3 found three (one live-High), pass 4 found five
(one High), and the refactor closed the class rather than the instances.

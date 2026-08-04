---
phase: 30-keep-the-session-alive-past-turn-end
plan: 03
subsystem: agent-result
tags: ["rust", "parser", "jsonl", "rate-limit", "session-id", "stream-json", "layer1"]

# Dependency graph
requires:
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30-01: claude_stream_events / is_claude_event_stream / last_top_level_result / parse_claude_event_result — the parser this plan extends"
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30-02: 30c-VERDICT.md delivery: confirmed — the conditional gate this plan opens on"
  - phase: 28-checkpoint-relay
    provides: "D-04/T-28-04 top-level-only session_id read — the discipline the stream reader reproduces"
provides:
  - "detect_claude_stream_rate_limit — final-turn explicit quota denial only; unknown statuses defer"
  - "CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES — denial vocabulary with per-entry provenance"
  - "claude_stream_envelope_failure — is_error on a stream's last result event, single-document reason shape"
  - "claude_stream_session_id (pub) — CLI-emitted session_id from the last system/init event"
  - "session_id_from_capture stream-first fallback chain — Phase 28 checkpoint-resume works under stream-json"
  - "parse_claude_event_result 5-step documented precedence: gate -> rate limit -> marker -> envelope -> held marker"
affects: ["30-05", "31-adapter-flip", "pipeline_outcomes", "checkpoint-resume"]

actuals:
  tokens: 9665
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Two independent guards for a costly classification (positional + semantic), each pinned by its own test, neither a substitute for the other"
    - "An unknown enum value DEFERS rather than classifying, with the asymmetry of the two error directions written into the doc comment"
    - "Mutation-check every guard test that passes on arrival — apply the exact regression it claims to prevent, confirm the failure, revert"

key-files:
  created: []
  modified:
    - "crates/devflow-core/src/agent_result.rs"

key-decisions:
  - "Denial vocabulary seeded with exactly one token (`rejected`), doc-commented as NOT yet observed as a `status` value — speculative additions are how the false positive comes back"
  - "Retry-hint chain is two rungs (`resetsAt` then the matched status), not the plan's three: `detect_claude_rate_limit`'s final `usage limit` default is unreachable here because a matched status is by construction a non-empty enumerated string; writing it would have been dead code"
  - "claude_stream_envelope_failure duplicates detect_claude_envelope_failure's reason shape rather than sharing a helper — the scope fence forbids modifying the shipped single-document parsers"
  - "Task 2's literal <verify> command was replaced: `agent_result::tests::claude_stream_session_id -- --exact` matches ZERO tests (proven by running it), because a test of that exact name would shadow the glob-imported function under test"
  - "Extended the existing claude_stream_last_result_without_marker_defers test with an explicit is_error:false assertion instead of adding a near-duplicate test"

patterns-established:
  - "A negative regression built from a REAL capture is worth more than a synthetic positive: the archived `status: allowed` event is what killed the presence-based design"
  - "A forgery-guard fixture should defeat every plausible wrong implementation at once (nested traversal AND last-event-with-the-key), not just the one named in the threat register"

requirements-completed: ["30b", "constraint-3", "constraint-6"]

coverage:
  - id: D1
    description: "A healthy stream carrying the real archived rate_limit_event (status: allowed) does NOT classify as RateLimited"
    requirement: "constraint-6"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_real_allowed_rate_limit_event_is_not_rate_limited"
        status: pass
    human_judgment: false
  - id: D2
    description: "An explicit quota denial inside the final turn classifies as AgentStatus::RateLimited with a retry hint, keeping the rate-limit resume path reachable under stream-json"
    requirement: "30b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_final_turn_denial_rate_limit_event_is_rate_limited"
        status: pass
    human_judgment: false
  - id: D3
    description: "A denial that predates the final turn cannot outrank the final turn's own outcome"
    requirement: "constraint-6"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_denial_before_final_turn_does_not_outrank_final_result"
        status: pass
    human_judgment: false
  - id: D4
    description: "An unrecognised rate_limit_info.status defers rather than manufacturing a RateLimited verdict and a fabricated retry time"
    requirement: "constraint-6"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_unrecognised_rate_limit_status_defers"
        status: pass
    human_judgment: false
  - id: D5
    description: "Rate limit outranks the marker path when it fires (T-30-13)"
    requirement: "30b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_final_turn_denial_outranks_failed_marker"
        status: pass
    human_judgment: false
  - id: D6
    description: "A stream whose last result event carries is_error:true classifies as Failed instead of deferring to Layer 2, and overrides a success marker (T-30-15)"
    requirement: "constraint-3"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_last_result_is_error_without_marker_is_failed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_is_error_overrides_success_marker"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_last_result_without_marker_defers"
        status: pass
    human_judgment: false
  - id: D7
    description: "session_id survives a stream capture: session_id_from_capture returns the CLI-emitted id for a JSONL capture, unchanged for a single-document one"
    requirement: "constraint-3"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_session_id_reads_cli_emitted_init_value"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_session_id_from_capture_reads_jsonl"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_wiring_leaves_single_document_capture_unchanged"
        status: pass
    human_judgment: false
  - id: D8
    description: "A session_id planted by the agent inside its own marker text is never returned (T-30-11), and the stream reader declines non-stream shapes"
    requirement: "constraint-3"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_session_id_ignores_agent_planted_value"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_stream_session_id_declines_non_stream_shapes"
        status: pass
    human_judgment: false

duration: 23min
completed: 2026-08-02
status: complete
---

# Phase 30 Plan 03: Stream Rate Limit, Envelope Failure and Session ID Summary

**Rate-limit classification, `is_error` failure attribution and `session_id` now survive a Claude `stream-json` capture with the single-document path's precedence and read discipline intact — and the classifier fires only on a final-turn explicit quota denial, so the real archived `status: "allowed"` telemetry event that would have marked every healthy stream `RateLimited` is a passing negative regression instead of the design.**

## Performance

- **Duration:** ~23 min (16:24 → 16:47 local)
- **Tasks:** 2
- **Files modified:** 1 (`crates/devflow-core/src/agent_result.rs`)
- **Net diff:** 691 insertions, 22 deletions

## Conditional Gate

Satisfied before any work. `30c-VERDICT.md` frontmatter reads `delivery: confirmed`
(`claude_code_version: 2.1.220`, 3 result events, 2 children completed, task set
drained, 0 unparseable lines), and the orchestrator recorded the operator's
approval on 2026-08-02 after 8 trials across 3 environments. The plan's
`delivery: refuted` branches were not exercised and no operator answer needed
recording.

## Accomplishments

- **The false positive the cross-AI review caught is now a test, not a bug.** The archived `rate_limit_event` (`raw_output_v3.jsonl:15`) says `rate_limit_info.status: "allowed"` and sits in a stream that then completed three turns. A presence-based classifier would have marked every healthy Claude stream stage `RateLimited`, and `crates/devflow-core/src/outcome_policy.rs:41` maps that to `Action::AutoResume` — the whole pipeline would have auto-resumed instead of advancing. That exact event, copied verbatim including its nested `overageStatus:"rejected"` trap, is the mandatory negative regression.
- **Two independent guards, each separately pinned.** Positional: only events after the second-to-last `result` are eligible, so history cannot outrank the final turn. Semantic: only an enumerated denial `status` classifies. The negative regression asserts BOTH — the real event at its real position (before the first result), and the same real event moved into the final turn, where only the status check can decline it. Dropping either guard fails a test.
- **Unknown statuses defer, and the asymmetry is written down.** Under-classifying costs the operator automatic resume but still gates (never silent). Over-classifying auto-resumes a healthy run against a retry time the parser invented. That reasoning is in the function's doc comment, along with the instruction to correct the denial list the first time a real blocked capture is archived.
- **`is_error` is no longer invisible under stream-json.** A last `result` event with `is_error: true` now yields `Failed` with the same reason shape `detect_claude_envelope_failure` produces (`result` text → `subtype` → default, plus the `(num_turns: N)` suffix), and it overrides a held success marker.
- **The Phase 28 checkpoint-resume path is unblocked.** `session_id_from_capture` returned `None` for every JSONL capture before this plan — `claude --resume` had no id to resume into. It now reads the last `init` event's top-level `session_id` and falls back to the shipped reader, so single-document captures resolve bit-for-bit as before.
- **Every guard test was mutation-checked, not assumed.** Three of the eight new Task-1 tests and two of the five new Task-2 tests pass on arrival, so passing proves nothing on its own — see "Mutation Checks" below for the regressions applied and the exact failures observed.

## Test Results

**Before this plan:** `cargo test -p devflow-core --lib agent_result::` → `test result: ok. 97 passed; 0 failed; 0 ignored; 0 measured; 366 filtered out`

**After this plan:** `test result: ok. 109 passed; 0 failed; 0 ignored; 0 measured; 366 filtered out`

**Net: +12 tests, 0 failures.** Workspace lib totals: `test result: ok. 475 passed; 0 failed` (was 463).

Each new test individually with `-- --exact`. The non-zero `filtered out` count is the guard against this project's standing false-green trap (a bare name that matches nothing exits 0 with `0 passed`):

| Test | Result line |
|---|---|
| `claude_stream_real_allowed_rate_limit_event_is_not_rate_limited` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out` |
| `claude_stream_final_turn_denial_rate_limit_event_is_rate_limited` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_denial_before_final_turn_does_not_outrank_final_result` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_unrecognised_rate_limit_status_defers` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_final_turn_denial_outranks_failed_marker` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_last_result_is_error_without_marker_is_failed` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_is_error_overrides_success_marker` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_last_result_without_marker_defers` (extended) | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 469 filtered out` |
| `claude_stream_session_id_reads_cli_emitted_init_value` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out` |
| `claude_stream_session_id_ignores_agent_planted_value` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out` |
| `claude_stream_session_id_declines_non_stream_shapes` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out` |
| `claude_stream_session_id_from_capture_reads_jsonl` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out` |
| `claude_stream_wiring_leaves_single_document_capture_unchanged` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out` |

**Plan's Task 1 `<verify>`, run verbatim:**
`cargo test -p devflow-core --lib agent_result::tests::claude_stream` → `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 459 filtered out`, then
`cargo test -p devflow-core --lib agent_result::` → `test result: ok. 104 passed; 0 failed; 0 ignored; 0 measured; 366 filtered out` (state at Task 1; 109 after Task 2).

**Pre-existing rate-limit tests, unedited** (confirmed by grepping the full plan diff for their names — no hits):

- `evaluate_layer1_reports_rate_limited_without_marker` → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out`
- `evaluate_layer1_rate_limit_envelope_with_is_error_is_rate_limited` → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 474 filtered out`
- Whole `evaluate_layer1` cluster: `test result: ok. 5 passed; 0 failed`
- Pre-existing `session_id` cluster: `test result: ok. 7 passed; 0 failed`

**Repository gate:** `scripts/check.sh all` → `==> check.sh: all OK`, **exit code 0**, captured explicitly on the final committed state (`echo $?`, not inferred from the output text).

## TDD Gate Compliance

Both tasks ran RED → GREEN. RED was produced with `None`-returning stubs so the new
tests failed on their assertions rather than on a compile error — acceptance
requirement 3 rejects a compile failure as evidence of RED.

**Task 1 RED** (`d483599`), 4 failures, each for the intended reason:

```
claude_stream_final_turn_denial_rate_limit_event_is_rate_limited
  panicked: a final-turn quota denial must produce a Layer-1 verdict
claude_stream_final_turn_denial_outranks_failed_marker
  left: Some(Failed)   right: Some(RateLimited)
claude_stream_is_error_overrides_success_marker
  left: Success        right: Failed
claude_stream_last_result_is_error_without_marker_is_failed
  panicked: is_error on the last result must not defer to Layer 2
```
`test result: FAILED. 7 passed; 4 failed`

**Task 2 RED** (`0b7c041`), 3 assertion failures (`left == right`, `None` vs the
expected id) in `claude_stream_session_id_reads_cli_emitted_init_value`,
`claude_stream_session_id_ignores_agent_planted_value` and
`claude_stream_session_id_from_capture_reads_jsonl`.
`test result: FAILED. 1 passed; 3 failed`

No REFACTOR commit for either task — neither GREEN implementation needed cleanup.

## Mutation Checks

Five of the new tests pass the moment the stub is in place, so there is no honest
RED for them. Applying the exact regression each claims to prevent is the
substitute evidence. All mutations were applied AFTER the GREEN commit and
reverted with `git checkout -- <file>` on a clean tree; `git status --short` and a
`rg -c MUTATION` both came back empty afterwards, and the suite returned to
`109 passed; 0 failed`.

**Mutation A — presence-based classification** (find any `rate_limit_event` in the
whole stream; skip the status check). Exactly the three rate-limit guard tests
failed, and nothing else:

```
agent_result::tests::claude_stream_denial_before_final_turn_does_not_outrank_final_result
agent_result::tests::claude_stream_real_allowed_rate_limit_event_is_not_rate_limited
agent_result::tests::claude_stream_unrecognised_rate_limit_status_defers
test result: FAILED. 8 passed; 3 failed
```

This is the review's HIGH finding reproduced on demand: under it, the real archived
capture classifies as `RateLimited`.

**Mutation B — "last event carrying a `session_id`"** (the plausible wrong session
reader, gate retained): `claude_stream_session_id_reads_cli_emitted_init_value` and
`claude_stream_session_id_ignores_agent_planted_value` failed —
`test result: FAILED. 2 passed; 2 failed`.

**Mutation C — B plus dropping the `is_claude_event_stream` gate:** all three
session-id guards failed, including `claude_stream_session_id_declines_non_stream_shapes`
— `test result: FAILED. 13 passed; 3 failed`.

**Honest limit of Mutation C's finding:** `claude_stream_session_id_declines_non_stream_shapes`
does NOT fail under gate-removal alone, because a single-document envelope is not a
`system`/`init` event and the `init` search declines it anyway. Its teeth are against an
implementation that both widens the search and drops the gate. The gate itself is
load-bearing for `parse_claude_event_result` (pinned by 30-01's
`single_doc_envelope_not_consumed_by_claude_stream_parser`), not for this reader.

## Must-Haves Verified

| Truth | Status | Evidence |
|---|---|---|
| Real `status: allowed` event does NOT classify as RateLimited | met | `claude_stream_real_allowed_rate_limit_event_is_not_rate_limited` + Mutation A |
| Final-turn explicit denial classifies as RateLimited | met | `claude_stream_final_turn_denial_rate_limit_event_is_rate_limited` |
| A pre-final-turn rate event cannot outrank the final turn's outcome | met | `claude_stream_denial_before_final_turn_does_not_outrank_final_result` |
| Unrecognised status defers rather than classifying | met | `claude_stream_unrecognised_rate_limit_status_defers` |
| Last result with `is_error: true` yields Failed, not a silent Layer-2 deferral | met | `claude_stream_last_result_is_error_without_marker_is_failed`, `claude_stream_is_error_overrides_success_marker` |
| `session_id_from_capture` returns the CLI-emitted id for a JSONL capture | met | `claude_stream_session_id_from_capture_reads_jsonl` |
| A marker-planted `session_id` is never returned | met | `claude_stream_session_id_ignores_agent_planted_value` + Mutation B |
| Single-document behavior unchanged for all three signals | met | `claude_stream_wiring_leaves_single_document_capture_unchanged`, `claude_stream_session_id_declines_non_stream_shapes`, 5 unedited `evaluate_layer1_*` and 7 unedited `session_id_*` tests |

## Task Commits

1. **Task 1 RED** — `d483599` `test(30-03)`: real `status: allowed` fixture, synthetic-denial builder, 7 new tests + stub
2. **Task 1 GREEN** — `85735df` `feat(30-03)`: `detect_claude_stream_rate_limit`, `claude_stream_envelope_failure`, 5-step precedence restructure
3. **Task 2 RED** — `0b7c041` `test(30-03)`: 5 session-id tests + stub
4. **Task 2 GREEN** — `e5bc4b9` `feat(30-03)`: `claude_stream_session_id` + `session_id_from_capture` stream-first chain
5. **Citation fix** — `b945fa8` `docs(30-03)`: `outcome_policy.rs` is in `devflow-core`, not `devflow-cli`

## Scope Fence

Held. `git diff --name-only 233cbea..HEAD` returns exactly
`crates/devflow-core/src/agent_result.rs`. No launch-path file (`monitor.rs`,
`agents/claude.rs`, `pipeline_launch.rs`) was touched, and none of the four
single-document parsers (`detect_claude_rate_limit`,
`detect_claude_envelope_failure`, `parse_devflow_result`, `claude_session_id`)
was modified — `session_id_from_capture` is a wrapper, not one of the four, and
its change is the wiring the plan required.

## Deviations from Plan

### 1. [Rule 3 — Blocking] Task 2's `<verify>` command matches zero tests; replaced with per-test `--exact` runs

- **Found during:** Task 2, before writing the tests
- **Issue:** The plan's verify is `cargo test -p devflow-core --lib agent_result::tests::claude_stream_session_id -- --exact`. A test function named exactly `claude_stream_session_id` inside `mod tests` would shadow the glob-imported (`use super::*`) function under test, so every call site in the module would resolve to the test itself. The name is therefore unusable, and with `--exact` no other name matches.
- **Proof, not inference:** ran the plan's command verbatim against the finished implementation — `running 0 tests` / `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 475 filtered out`, **exit 0**. This is the project's documented false-green trap. (The plan's `rg 'test result: ok\. 1 passed'` would have caught it as a failure rather than a false pass — the review's `;`→`&&` fix did its job — but the command still could not have succeeded.)
- **Fix:** ran `-- --exact` against each real test name instead; all five Task-2 tests and all eight Task-1 tests are quoted above with a real `1 passed` and a non-zero `filtered out`.
- **Commit:** n/a (verification method, no source change)

### 2. [Rule 1 — Bug] `outcome_policy.rs` crate path in a doc comment

- **Found during:** post-implementation citation check
- **Issue:** I wrote `crates/devflow-cli/src/outcome_policy.rs:41` in the negative-regression test's doc comment. The file is `crates/devflow-core/src/outcome_policy.rs`; line 41 and the `AgentStatus::RateLimited => Action::AutoResume` mapping are both correct as cited. Same class as 30-01's finding F-3 (`pipeline_outcomes.rs`), and the plan/threat-register cite the file without a crate.
- **Fix:** corrected in the doc comment, with the wrong crate named explicitly so the next reader does not repeat the search.
- **Commit:** `b945fa8`

### 3. [Deliberate] Retry-hint chain is two rungs, not the plan's three

- **Found during:** Task 1 GREEN
- **Issue:** The plan prescribes `resetsAt` → matched `status` → literal `usage limit`. The third rung is unreachable: control only arrives there after `status` matched a non-empty entry of the denial const, so `unwrap_or_else` on the second rung can never fall through.
- **Decision:** implemented two rungs and documented in the code why the single-document chain's `usage limit` default has no counterpart here. Writing provably dead code to match a plan's letter is worse than recording the divergence.
- **Commit:** `85735df`

### 4. [Deliberate] Reason-shape duplication instead of a shared helper

- **Found during:** Task 1 GREEN
- **Issue:** `claude_stream_envelope_failure` reproduces `detect_claude_envelope_failure`'s `result` → `subtype` → default chain and `(num_turns: N)` suffix. Factoring out the common body would be the DRY move.
- **Decision:** the scope fence explicitly keeps the four shipped single-document parsers unmodified, so the refactor is out of bounds. Duplicated with a doc comment naming the constraint and stating the two must be kept in step by hand. Same call 30-01 made for `parse_codex_event_result`.
- **Commit:** `85735df`

### 5. [Rule 2 — Missing Critical] Extended an existing test rather than adding a near-duplicate

- **Found during:** Task 1
- **Issue:** The acceptance criteria require a test that a last `result` with `is_error: false` and no marker returns `None`. `claude_stream_last_result_without_marker_defers` (30-01) already covers exactly that case, but its name and body did not make the `is_error` dimension legible, and the new envelope branch could regress it silently.
- **Fix:** added an explicit assertion that the fixture really carries `"is_error":false` plus a doc addendum, instead of adding a second test asserting the same thing.
- **Commit:** `d483599`

### 6. [Citation] The plan's "line 1821" single-document literal

- **Found during:** Task 2
- **Issue:** The plan says `claude_stream_session_id` must return `None` for "the single-document envelope literal used at line 1821". After 30-01's insertions that line no longer holds an envelope literal.
- **Fix:** used the literal from `session_id_reads_top_level_string` instead. It is strictly stronger for this assertion: it carries a real `session_id`, so a wrongly-widened reader returns `Some(...)` and the test fails, whereas the `"session_id":"abc"` variant would also have worked but reads less like the shipped case. Paired with an assertion that `claude_session_id` still decides that same document.
- **Commit:** `0b7c041`

---

**Total deviations:** 6 (1 blocking, 1 bug, 2 deliberate, 1 missing-critical, 1 citation)
**Impact:** No scope creep — all six stayed inside the one permitted file. Deviation 1 changed only the verification method, and strengthened it: 13 individually-`--exact` runs with quoted counts instead of one command that could not have matched a test.

## Findings

### F-1 — `cargo clippy -p devflow-core --all-targets` fails on its own; the workspace form is clean (pre-existing, not this plan)

The narrow invocation fails with four `E0433: cannot find test_support in devflow_core` errors from `tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs`. Cause: `test_support` is behind the off-by-default `test-support` feature (`crates/devflow-core/Cargo.toml:16`, `lib.rs:79`), which is enabled only through workspace feature unification. The canonical gate — `cargo clippy --workspace --all-targets -- -D warnings`, which is what `scripts/check.sh clippy` and CI run — is clean and exits 0.

Not caused by this plan (only `agent_result.rs` was touched) and not fixed here (out of scope). Recorded because a reviewer reaching for the per-package form will hit it and may mistake it for a regression.

### F-2 — 30-01's finding F-1 is still open and was NOT fixed here

`parse_devflow_result` still returns `parse_marker_lines`' output unnormalised, so an agent writing `"decided_by_layer": 0` into a marker inside a `--output-format json` envelope still reaches `classify_validate_outcome` with unearned Layer-0 provenance. 30-01 named this "a natural candidate for plan 30-03". It is not in this plan's tasks, `<scope_fence>` or `<threat_model>`, and `parse_devflow_result` is one of the four parsers the fence names as untouchable — fixing it here would have been scope creep, not diligence. The permanent non-vacuity guard 30-01 left inside `claude_stream_overwrites_agent_planted_decided_by_layer` still documents the surface in the test suite. Carry to 30-05 or a numbered backlog entry.

### F-3 — The denial vocabulary is a guess by construction, and the code says so

`rejected` is drawn from the observed schema (it is the `overageStatus` value in the archived event), but it has never been observed as a `status` value — no archived capture is of a blocked stream. The const's doc comment records exactly that per entry, and the unknown-status deferral test makes the failure mode when the guess is wrong a `Failed` verdict (which gates) rather than a fabricated `RateLimited` (which auto-resumes). The first real blocked capture should be archived and this list corrected against it; until then the positive rate-limit path is proven only against a synthetic fixture. This is a known, bounded gap, not a silent one.

## Known Stubs

None. The two `None`-returning stubs used to produce honest TDD RED were both replaced in the following GREEN commit; `rg 'TDD RED stub'` over the final tree returns nothing.

## Issues Encountered

None beyond the deviations above. Both mutation checks were run against a
committed tree, so reverting was a single `git checkout -- <file>` with no risk
to uncommitted work — the hazard 30-01's SUMMARY flagged after having to reverse
its mutation by hand.

## User Setup Required

None. No dependency, `Cargo.toml`, or external service change.

## Next Phase Readiness

**Ready for 30-05.** All three signals the review's constraint 3 named — rate limit,
envelope failure, session id — now survive a JSONL capture, and the precedence
inside `parse_claude_event_result` is documented in the code as a 5-step order with
the reason for each position.

Phase 31 remains blocked on its own work: the shipped adapter still launches with
`--output-format json` (`crates/devflow-core/src/agents/claude.rs`), so nothing in
production emits the stream shape these parsers read. Everything added here is
correct-but-dormant until that flip.

**Carry forward:** F-2 (30-01's `parse_devflow_result` normalisation, still open) and
F-3 (archive a real blocked capture and correct the denial vocabulary).

## Self-Check: PASSED

Verified on disk in this session, not recalled:

- `crates/devflow-core/src/agent_result.rs` — present; contains `fn detect_claude_stream_rate_limit(`, `const CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES`, `fn claude_stream_envelope_failure(` and `pub fn claude_stream_session_id(`.
- Commits `d483599`, `85735df`, `0b7c041`, `e5bc4b9`, `b945fa8` — all present in `git log`.
- `git diff --name-only 233cbea..HEAD` — exactly one file; scope fence intact.
- Working tree clean (`git status --short` empty) with no mutation residue (`rg -c MUTATION` → no match).
- `scripts/check.sh all` exit code captured explicitly as `0` on the final committed state.
- Every test count and `1 passed` line quoted above came from a run made in this session.
- `outcome_policy.rs:41` was re-read before citing it (`AgentStatus::RateLimited => Action::AutoResume`).

---
*Phase: 30-keep-the-session-alive-past-turn-end*
*Completed: 2026-08-02*

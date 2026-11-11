---
phase: 52-subtract-and-stabilize
plan: 30
subsystem: gate-context
tags: [rust, devflow-cli, devflow-core, gates, notify, operator-surface, tdd]

requires:
  - phase: 52-33
    provides: "way_on_sentence / lead_with_way_on (the way on first in every gate context); GATE_CONTEXT_DISPLAY_CHARS raised to 450"
  - phase: 52-29
    provides: "approve_route / reject_route, the routers the way-on sentence is rendered from"
provides:
  - "GATE_CONTEXT_STORED_MAX_CHARS (4096 characters): truncate_reason stores a failure reason whole up to it, so `devflow gate show` prints what a gate fired on"
  - "devflow_core::gates::render_gate_context (pub, moved from the CLI) and devflow_core::gates::GATE_CONTEXT_DISPLAY_CHARS (pub, 450): one cut and one display bound for the banner, `gate list` and the notify hook"
  - "the notify hook's DEVFLOW_GATE_CONTEXT is cut to the display bound (450 characters)"
  - "lead_with_way_on with an empty cause yields the way-on sentence alone"
affects: [52-31, 52-32]

actuals:
  tokens: 8377
  tasks: 2
  commits: 4
plan_head_before: 20445dbd84530f802a44ecac4397b8415f23aa19
plan_head_after: 92673ebba08b4fd4e76b1f05b6aebc33acbb0c4a

tech-stack:
  added: []
  patterns:
    - "Store agent text whole up to a generous bound; cut it only where it is displayed or sent"
    - "One cut function and one display bound in devflow-core, re-exported by the CLI, so the notify hook and the CLI renderings cannot drift apart"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-30-SUMMARY.md
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/agent_result.rs
    - OPERATIONS.md

key-decisions:
  - "52-30: the notify hook's DEVFLOW_GATE_CONTEXT is cut to the display bound, 450 characters, not the plan's 300. The plan wrote 300 as 'the display cap … too' before 52-33 raised the display bound to 450; at 300 a verification gate's 287-character way-on sentence would leave almost no cause in a notification"
  - "52-30: the notify bound and the display bound are one constant, devflow_core::gates::GATE_CONTEXT_DISPLAY_CHARS, and one cut, devflow_core::gates::render_gate_context (both new pub items, additive); the CLI re-exports both"
  - "52-30: the stored bound (4096 characters) applies to each reason truncate_reason bounds, not to the whole gate context; a context is at most 4096 characters plus its fixed lead (marker, way-on sentence, failure words)"

patterns-established:
  - "Stored-whole, displayed-short: a gate file holds the reason up to GATE_CONTEXT_STORED_MAX_CHARS; every rendering except `gate show` cuts the whole context to GATE_CONTEXT_DISPLAY_CHARS"

requirements-completed: [GATE-04]

coverage:
  - id: D1
    description: "A failure reason longer than the display bound reaches `devflow gate show` whole, end to end; `gate list` stays bounded and leads with the way-on sentence"
    requirement: "GATE-04"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_long_failure_reason_reaches_gate_show_whole"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_show_renders_full_untruncated_sanitized_context"
        status: pass
    human_judgment: false
  - id: D2
    description: "A 1000-character cause is stored whole in the failure gate context"
    requirement: "GATE-04"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_long_failure_reason_is_stored_whole"
        status: pass
    human_judgment: false
  - id: D3
    description: "The stored bound's edges in characters: exactly 4096 (multi-byte) unchanged, 4097 cut at a character boundary to exactly 4096 with the truncation suffix"
    requirement: "GATE-04"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_context_at_the_stored_cap_is_unchanged_and_one_over_is_cut_at_a_char_boundary"
        status: pass
    human_judgment: false
  - id: D4
    description: "The failure context leads with the routers' way-on sentence; an empty cause yields the sentence alone"
    requirement: "GATE-04"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_failure_context_leads_with_the_way_on"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run"
        status: pass
    human_judgment: false
  - id: D5
    description: "The notify hook's DEVFLOW_GATE_CONTEXT is cut to the display bound (450 characters); a context of exactly the bound arrives unchanged, one over is cut to the bound"
    requirement: "GATE-04"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::notify_context_is_capped_at_the_display_bound"
        status: pass
    human_judgment: false

duration: 23min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 30: Stored-whole gate reasons Summary

**A failure gate now stores its reason whole (up to 4096 characters), so `devflow gate show` prints all of it. The banner, `gate list` and the notify hook still show only the first 450 characters, and that start always begins with the way-on sentence.**

## Performance

- **Duration:** 23 min
- **Started:** 2026-11-11T14:08:28Z
- **Completed:** 2026-11-11T14:31:24Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- `truncate_reason` bounds a reason at `GATE_CONTEXT_STORED_MAX_CHARS` (4096 characters) instead of 300. Every write-time caller goes through it: the stage-failure, Validate-read, preflight, `workflow_aborted` and launch paths. No write-time caller cuts at 300 any more.
- The notify hook receives the context cut to the display bound. Before this plan it received the whole stored context. Once reasons are stored whole, that would have meant up to about 4.4 K characters (Pitfall 6).
- `render_gate_context` and `GATE_CONTEXT_DISPLAY_CHARS` now live in `devflow-core`. The notify hook and the three CLI renderings (banner, `gate list`, the all-roots row) share one cut and one bound.
- With an empty cause, `lead_with_way_on` returns the sentence alone. Before, it left a trailing space.
- OPERATIONS.md, in the Notify hook section, states the notify context's bound and that `gate show` prints the whole reason.

## Task Commits

1. **Task 1 (tracer): a long failure reason reaches `gate show` whole.** RED `14f3872` (test), GREEN `8d1429d` (feat).
2. **Task 2: the cut boundaries, the empty cause and the notify cap are pinned.** RED `db531b9` (test), GREEN `92673eb` (feat).

**Plan metadata:** the docs commit that carries this file.

The tracer gate ran interactive, in `end-of-phase` mode, with a verify that has only automated checks. After the GREEN commit both of Task 1's verify commands exited 0, so expansion went ahead.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|------|---------|---------------------|
| `gate_wedge_e2e::a_long_failure_reason_reaches_gate_show_whole` | `RED_EVIDENCE_OK` | `gate show` printed the context with the reason cut at 300 characters (`… [truncated; full output in .devflow/]`) and no marker. That is the planned failure. |
| `pipeline_outcomes::tests::the_failure_context_leads_with_the_way_on` | `RED_EVIDENCE_OK` | The context-with-a-cause check passed. The empty-cause check failed on a trailing space after the sentence. |
| `gates::tests::notify_context_is_capped_at_the_display_bound` | `RED_EVIDENCE_OK` | The hook recorded 4000 characters where 450 were expected. |
| `pipeline_outcomes::tests::a_long_failure_reason_is_stored_whole` | `INVALID_RED` (`unexpected_green`) | Not RED. It was written after Task 1's GREEN, which it pins. That behaviour's RED is the e2e above. |
| `pipeline_outcomes::tests::a_context_at_the_stored_cap_is_unchanged_and_one_over_is_cut_at_a_char_boundary` | `INVALID_RED` (`unexpected_green`) | Not RED, for the same reason: Task 1 set the 4096 bound, and `render_gate_context` already counted characters. It has its own control: the 4096-character text is longer than 4096 bytes, so a cut that counted bytes would fail it. |

Commit order: `test(52-30)` `14f3872` → `feat(52-30)` `8d1429d` → `test(52-30)` `db531b9` → `feat(52-30)` `92673eb`. Both GREEN commits descend from a RED commit.

## Files Created/Modified

- `crates/devflow-cli/src/pipeline_outcomes.rs`: `GATE_CONTEXT_STORED_MAX_CHARS`, the new bound in `truncate_reason`, `lead_with_way_on` with an empty cause, `render_gate_context` re-exported from core, four tests added or reworked.
- `crates/devflow-core/src/gates.rs`: `GATE_CONTEXT_DISPLAY_CHARS` and `render_gate_context` (both pub, moved in from the CLI), the notify context cut, and the notify test.
- `crates/devflow-cli/src/commands.rs`: re-exports `GATE_CONTEXT_DISPLAY_CHARS` from core.
- `crates/devflow-cli/src/preflight.rs`: the comments about reason ordering now refer to the display cut, not the old 300-character cap. One test now checks the reason at the display bound.
- `crates/devflow-core/src/agent_result.rs`: the derivation comment for `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` is re-derived for the stored bound (4096 characters, 4057 kept). Comment only.
- `crates/devflow-cli/tests/gate_wedge_e2e.rs`: the end-to-end tracer test.
- `OPERATIONS.md`: the notify context's bound.

## How the plan's numbers were reconciled with 52-33

The plan was written before 52-33. It uses 300 in two senses:

- **The old write-time cut** (`render_gate_context(reason, 300)`). This plan removes it, as written.
- **The display budget.** The plan says "the banner and `gate list` keep their 300-character display cap (`GATE_CONTEXT_DISPLAY_CHARS`), and the notify hook's `DEVFLOW_GATE_CONTEXT` is capped at 300 characters too", and asks for "the same character-boundary cut and ellipsis as the display cap". 52-33 raised that budget to 450, so this plan uses 450 everywhere the plan means the display bound:
  - The e2e test checks each `gate list` context line against 450, not 300.
  - The notify cap is 450. It is the same constant as the display bound, now in `devflow-core`, so the two cannot drift apart again.
  - The notify test is named `gates::tests::notify_context_is_capped_at_the_display_bound`, not `notify_context_is_capped_at_300_chars`. A test that asserts 450 under a name that says 300 would be wrong, and a name with no number survives the next change to the bound.
  - The display-bound edge cases (exactly 450 unchanged, 451 cut to 450) live in that notify test, since the notify hook and the display share one cut.

52-33's 450 and its re-derived `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` comment were not reverted. The comment was re-derived once more, for the stored bound.

## Decisions Made

See `key-decisions`. One limit, stated plainly: the 4096-character bound applies to each reason, not to the whole gate context. The fixed lead (marker, way-on sentence of up to about 290 characters, failure words) comes on top. Threat T-52-42 asks for a bound, and there still is one: about 4.4 K characters, at most about 17.6 KiB of UTF-8.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The plan's 300-character display and notify numbers predate 52-33's 450**
- **Found during:** Task 1 (the e2e's `gate list` bound) and Task 2 (the notify cap)
- **Issue:** The plan's display-bound figures were written before 52-33 raised the bound.
- **Fix:** Used 450 throughout and gave the notify test a name with no number, as described in the reconciliation section above.
- **Effect on the plan's own gate:** Task 2's verify line names `notify_context_is_capped_at_300_chars`. Run literally, it now exits 4 (no test matched). The test that exists, `gates::tests::notify_context_is_capped_at_the_display_bound`, exits 0. So does the `must_haves` artifact check if it looks for the new name. The verifier will see this mismatch.
- **Commits:** `14f3872`, `db531b9`, `92673eb`

**2. [Rule 3 - Blocking] The idle-timeout cause test depended on the old 300-character cut**
- **Found during:** Task 1
- **Issue:** `an_idle_timeout_gate_context_names_the_last_result_cause` checked that the cause survived `truncate_reason` at 300 characters. Its control (a clause moved to the end gets cut) cannot fail once reasons are stored at 4096.
- **Fix:** The test now checks what an operator sees, the failure gate context cut to the display bound, and keeps the placement control. Its at-bound case now also checks that the reason is stored whole.
- **Commit:** `8d1429d`

**3. [Rule 2 - Missing] The notify cut needed the display's cut function in `devflow-core`**
- **Found during:** Task 2
- **Issue:** The plan asks for "the same character-boundary cut and ellipsis as the display cap" at the notify call, which is in `devflow-core`. `render_gate_context` lived in the CLI, which core cannot call.
- **Fix:** Moved `render_gate_context` and `GATE_CONTEXT_DISPLAY_CHARS` into `devflow-core` as new pub items (additive) and re-exported both in the CLI. Behaviour is unchanged.
- **Semver check:** `cargo semver-checks check-release -p devflow-core` exits 100 because of 15 earlier breaking changes in phase 52 (v4.0 is a major release). None of the failures it lists names an item this plan added or changed.
- **Commit:** `92673eb`

**4. [Process] Commit subject**
- The plan prescribes `fix(cli): store a gate's full reason`. The GREEN commits use `feat(52-30): …` so the TDD gate's RED→GREEN check can find them.

**Other files:** `pipeline_gate.rs` and `pipeline_launch.rs` are listed in Task 1's files. They needed no change, because they call `truncate_reason`.

---

**Total deviations:** 3 auto-fixed (1 bug, 1 blocking, 1 missing piece), plus 1 process note.
**Impact on plan:** The plan's goal is met. Its literal display and notify numbers and one test name changed, for the reasons above.

## Issues Encountered

None. The full `scripts/check.sh all` run passed on its first try (exit 0), with no flake reruns.

## Verification

- `cargo nextest run -p devflow --test gate_wedge_e2e --no-tests=fail -E 'test(=a_long_failure_reason_reaches_gate_show_whole)'`: exit 0
- `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=commands::tests::gate_show_renders_full_untruncated_sanitized_context)'`: exit 0
- `pipeline_outcomes::tests::{a_long_failure_reason_is_stored_whole, a_context_at_the_stored_cap_is_unchanged_and_one_over_is_cut_at_a_char_boundary, the_failure_context_leads_with_the_way_on, every_gate_names_only_the_routes_its_answers_run}`, one named nextest each: exit 0 each
- `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=gates::tests::notify_context_is_capped_at_the_display_bound)'`: exit 0
- `cargo nextest run -p devflow --no-fail-fast` after Task 1: 926 passed, exit 0
- `scripts/check.sh all` after the last code change: exit 0 (fmt, clippy with `-D warnings`, `cargo test --workspace`)
- Acceptance check that no write-time caller cuts at 300: `rg 'render_gate_context\([^)]*300' crates/` finds nothing. The negative control, the same pattern run on the pre-plan `pipeline_outcomes.rs`, matches line 2151.

What these checks do not cover:
- The e2e test exercises one failure path, a Claude stream agent's `is_error` result at Code. The other paths (preflight, Validate read, launch) are covered only because they share `truncate_reason`.
- No live notify hook (ntfy or a desktop notifier) was run. The cap is tested with a stub hook that records the variable.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-31 and 52-32. Whoever adds a gate writer: bound reasons with `truncate_reason` (the stored bound) and lead with `lead_with_way_on`. Do not cut at the display bound when writing; displays and the notify hook already do that.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

All key files exist on disk. Commits 14f3872, 8d1429d, db531b9, 92673eb and 3357ae8 are ancestors of HEAD.

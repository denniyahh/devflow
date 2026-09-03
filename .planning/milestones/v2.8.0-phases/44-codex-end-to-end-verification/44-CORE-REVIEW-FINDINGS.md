---
phase: 44-codex-end-to-end-verification
written: 2026-08-27
purpose: >
  Adversarial review of the SHIPPED (post-execution) 44-00..44-03 driver code —
  the actual CODE-01 deliverable — requested after the operator noticed the
  earlier review round only covered this session's hook/pr-branch.md tangent.
  Findings below were fixed in a follow-up session (2026-08-27, this handoff's
  fresh context) — see "Fix status" appended under each finding. Originally
  written to hand off to a fresh context rather than fix inline, per operator
  instruction (fixing costs more context than documenting; document first, fix
  in a fresh session).
reviewers:
  - codex (gpt-5.6-terra, high reasoning effort) — raw output: /tmp scratchpad
    review2-codex.md (this session only; not preserved past session end)
  - antigravity (gemini-3.7-flash-high, no 3.7-pro tier exists) — raw output:
    /tmp scratchpad review2-agy.md (same caveat)
diff_reviewed: "git diff 0e1a94dc3d7e07de108e077a96909abaaef6fa3b..8074d1d -- \
  crates/devflow-core/src/ship.rs crates/devflow-cli/src/main.rs \
  crates/devflow-cli/src/pipeline_launch.rs crates/devflow-cli/src/preflight.rs \
  crates/devflow-cli/src/pipeline_gate.rs crates/devflow-cli/src/commands.rs \
  crates/devflow-core/src/recover.rs"
---

# Phase 44 core deliverable — adversarial review findings (unfixed)

The raw reviewer output files no longer exist once this session ends (they were
in `/tmp` scratchpad, not committed). This document is the durable record —
findings are restated here with enough detail to act on without the raw files.

## 1. CRITICAL — Hermes cron hint has a shell-quoting bug that breaks or is exploitable

**Confirmed independently by both reviewers, and independently verified in-session
with a direct Python reproduction (not just accepted from the reviewers).**

**Location:** `crates/devflow-core/src/ship.rs:242-246` (`build_single_agent_cron_instructions`,
the `command` field) and `crates/devflow-cli/src/commands.rs:1993-1999` (`cron_hint_line`'s
format string).

**Mechanism:** `ship.rs` builds `hermes_cron.command` as
`format!("cd {} && devflow resume --phase {phase}", shell_quote(&project))` — this correctly
single-quotes and escapes the project path via `shell_quote()`. But `cron_hint_line` in
`commands.rs` then wraps that **already-quoted** command string in *another* raw, unescaped
single-quote pair: `'{}'` where `{}` is `instructions.hermes_cron.command`. Single quotes do
not nest in POSIX shell — the first `'` character already present inside `command` (from
`shell_quote`'s own escaping) prematurely closes the outer quote.

**Verified reproduction** (Python, replicating both functions exactly): for a project path
containing a space (e.g. `/home/user/My Project`), the final rendered hint line is:

```
hermes cron create "SCHEDULE" 'cd '/home/user/My Project' && devflow resume --phase 7' --repeat 1 --name x
```

A shell parses this as multiple broken, unquoted word fragments (`/home/user/My` and `Project
&& devflow resume --phase 7` split apart), not a coherent command — the hint is simply not
runnable, defeating 44-03's entire stated purpose ("make the hint runnable", D-10). For a path
containing a literal `'` (e.g. `/home/o'connor/repo`), both reviewers independently assessed
this as a genuine shell-injection vector, not just a syntax break — the outer quote closes at
the embedded `'`, and shell metacharacters after that point in the path would be interpreted
by whatever shell the operator pastes the hint into.

**Fix direction (not yet implemented):** `cron_hint_line` must not re-wrap an already-quoted
command in another quote layer. Either stop calling `shell_quote()` inside `ship.rs` and let
`cron_hint_line` be the single place that quotes the whole command, or have `cron_hint_line`
interpolate `command` unquoted (since it's already self-quoted) instead of wrapping it in `'{}'`.
Whichever direction is chosen, add a regression test asserting the rendered hint is valid,
executable shell for a project path containing a space and one containing an apostrophe —
neither case is covered by the existing test suite (confirmed: codex ran the existing hint
tests and reported they "do not exercise ... the quote-nesting ... case").

**Fix status: FIXED (2026-08-27).** Took the first direction: `ship.rs`'s `command` field is
now built from the raw, unquoted project path — `shell_quote()` is called exactly once, in
`cron_hint_line`, against the whole composite command string. `shell_quote` was made
`pub(crate)` so `commands.rs` could call it. Regression test
`cron_hint_line_command_quoting_roundtrips_through_shell_for_space_and_apostrophe_paths`
(`commands.rs`) covers both the space and apostrophe cases named above, asserting via a real
`sh -c` round-trip that the quoted command reparses to the original byte-for-byte — not just a
string-shape assertion. Verified failing against the pre-fix code first (reproduced the exact
broken output quoted above), then passing after the fix, before treating it as done.

## 2. HIGH — handoff-related state can be persisted before the operation it depends on can still fail

Two reviewers found overlapping but distinctly-framed versions of this; both are worth
carrying forward since they point at different code paths.

**2a (agy's framing) — `resume()` unstops before `launch_stage` can still fail.**
`pipeline_launch.rs:1341-1342`: `state.stopped = false; state.stop_reason = None;
state.stop_until = None;` are saved via `workflow::save_state(&state)?` at line 1341,
*unconditionally*, before `launch_stage(&mut state, None, None)` is called as the function's
tail expression at line 1342. If `launch_stage` fails (preflight failure, canary gate refusal,
monitor spawn failure on a missing worktree), `resume()` returns `Err`, but the state file is
already saved with `stopped: false` and no monitor running — a "zombie" state that isn't
flagged as stopped or stuck by `check_dead_agent`/`check_dead_monitor` (`commands.rs:3053,
3076`, per agy — line numbers not independently re-verified by me).

**2b (codex's framing) — the early handoff preflight check is narrower than the later one
inside `launch_stage`.** `pipeline_launch.rs:1282` runs `preflight_interactivity_check`
against a cloned candidate state *before* `state.agent = requested; save_state(&state)` at
line 1294. But `launch_stage` (line 1342) runs its own, later, stricter `run_preflight`
(`preflight.rs:1288`) — codex's concrete example: an Auto/Code handoff to Codex passes the
early interactivity check (Code is headless-safe per `codex.rs:91`), but a later
unattended-launch check at `preflight.rs:981` can still reject non-Claude/Antigravity Code
launches. If that later check fails, `preflight_retries` gets incremented and saved
(`preflight.rs:1288`), `gate_pending` gets written (`pipeline_gate.rs:369`), but
`state.agent` was already committed to the new driver at line 1294-1306 (including the
`agent_handoff` audit event) — the phase is left handed off to a driver that never actually
launched.

**I independently verified the control-flow shape of 2a by reading `resume()` directly**
(confirmed: line 1341's save has no guard on `launch_stage`'s subsequent success). **I did
not independently verify 2b's specific claim about `preflight.rs:981`'s unattended-launch
check being stricter than the early interactivity check** — that needs a read of
`preflight.rs` around both line 607 (early check) and line 981 (the one codex cites) before
trusting the concrete Codex/Auto/Code example.

**Fix direction (not yet implemented):** the general shape is "don't persist a state change
whose correctness depends on a later operation succeeding, until that operation actually
succeeds." Likely needs `launch_stage`'s full preflight to run (or a check to be added) before
the `agent`/`stopped` mutations are saved, not after — this may require restructuring
`resume()`'s ordering rather than a small patch. Needs care: `44-01-SUMMARY.md`'s own Limits
section already documents a related ordering constraint discovered during original
implementation ("a first preservation probe at Validate failed because normal validation
dispatch stamps validation-window fields"), so this is evidently an area the original executor
already found subtle — a fix here should re-read that SUMMARY before changing the ordering.

**Fix status: FIXED (2026-08-27), both sub-findings, without reordering `resume()`.**

- **2a:** Did not restructure the save ordering (the D-15/999.60 constraint the SUMMARY
  documents is real and still needed). Instead, `resume()` now wraps the
  `launch_stage(&mut state, None, None)` tail call: on `Err`, it re-marks `state.stopped = true`
  with a `stop_reason` naming the failure and best-effort re-saves before propagating the
  original error. This closes the zombie window (state falsely claiming `stopped: false` with
  nothing running) without touching the mid-relaunch-reload guarantee the early save exists for.
  Regression test `resume_re_marks_stopped_when_launch_stage_fails_outright`
  (`pipeline_launch.rs`) forces `launch_stage` to fail (missing agent binary) and asserts the
  reloaded state is `stopped`. Verified failing without the fix (reproduced the exact zombie:
  `stopped: false`, no monitor) before confirming it passes with the fix.
- **2b:** Verified codex's specific claim first (previously not independently checked) —
  confirmed `unattended_launch_shape_condition` (`preflight.rs:981`) does refuse Auto-mode,
  Code-stage handoffs to any non-Claude/Antigravity agent, while the early
  `preflight_interactivity_check` at the handoff site does not evaluate this at all (Codex
  declares Code `HeadlessSafe`). Fix: `resume()`'s handoff branch now calls the full
  `generic_preflight_checks` (major-bump, unattended-launch-shape, interactivity, gh-auth —
  the same bundle `launch_stage` runs later via `run_preflight`) against the candidate state,
  instead of `preflight_interactivity_check` alone; made `generic_preflight_checks` `pub(crate)`
  for this. Regression test
  `resume_with_agent_refuses_auto_mode_handoff_that_would_fail_the_later_unattended_launch_check`
  covers the exact Auto/Code/Codex scenario. Negative control: reverting to the old
  interactivity-only check made the test hang on a real gate wait (killed after ~2 min) rather
  than failing fast, consistent with the finding's description of a handoff that commits and
  then blocks on a live gate.

## 3. MEDIUM — cron-instruction consumption is not atomic / can race or mask consumption

**Location:** `crates/devflow-core/src/ship.rs:174-212` (`consume_cron_instructions`),
called from `pipeline_gate.rs:298` and `pipeline_launch.rs:1052`.

Two related claims, both from reviewer analysis, **neither independently verified by me**:

- **TOCTOU (agy):** `has_per_phase` is checked via `.exists()` at line 176, then
  `std::fs::remove_file(&per_phase)?` runs later at line 199. A concurrent delete between the
  check and the removal would make `remove_file` fail with `NotFound`, and the function
  returns `Err` — callers (`pipeline_gate.rs:298`, `pipeline_launch.rs:1052`) only `warn!`/
  `info!` and emit no audit event for what actually happened, so a subsequent run could
  misreport `CronInstructionPathKind::Legacy` and mask that per-phase consumption already
  occurred.
- **Cross-phase race (codex):** the legacy cron record is project-global (not per-phase
  locked — `lock.rs:8` locks are per-phase, `ship.rs:87`'s legacy path is shared), so two
  phases resuming/shipping concurrently could have phase A delete phase B's legacy record and
  emit an inaccurate `Legacy`/`Both` classification for the wrong phase.

**Why this is lower-confidence than findings 1-2:** this describes a race window that requires
genuine concurrent phase execution to trigger (`devflow parallel`, or two operators/sessions
touching the same repo at once) — real, but a narrower blast radius than the shell-injection
or state-persistence findings, and I have not attempted to reproduce either race scenario.

**Fix status: FIXED (2026-08-27) for the TOCTOU; the cross-phase framing does not hold as
stated, verified against current source.**

- **TOCTOU (agy):** confirmed and fixed. `remove_file` now goes through
  `remove_file_if_still_present`, which tolerates `NotFound` (someone else already removed it)
  instead of propagating `Err`. Fixing only the error path first exposed a second, more
  concrete bug the finding's TOCTOU framing implies but doesn't spell out: two racing consumers
  both computing `has_per_phase = true` from their own `.exists()` snapshot would BOTH report
  `Some(CronInstructionPathKind::PerPhase)` and both fire a `cron_instructions_consumed` event
  for the one record that existed — caught by my own regression test on the first attempt (an
  `assert_eq!(reported, 1, ...)` failure, `left: 2, right: 1`), not anticipated up front. Fixed
  properly: `remove_file_if_still_present` returns whether THIS call did the removing, and
  `consume_cron_instructions` reports a kind only for candidates it actually removed, not for
  what it saw at `.exists()` time. Regression test
  `consume_cron_instructions_tolerates_a_racing_concurrent_consumer` (`ship.rs`) races two real
  threads on a barrier and asserts: neither errors, exactly one reports a kind, and the file is
  gone. Passed consistently across 5 repeated runs.
- **Cross-phase race (codex):** checked against current source and does **not** hold as stated.
  `has_matching_legacy` already gates on `instructions.phase == phase` (verified by the
  pre-existing `consume_cron_instructions_preserves_foreign_legacy_record` test, which was
  green before this session's changes too) — a differently-numbered phase's `consume` call
  cannot touch another phase's legacy record just by racing, because the phase field has to
  match first. The legacy path is also "never written" by current code per its own doc comment
  (`legacy_cron_instructions_path`), so a live collision needs a pre-14a leftover file, which
  narrows this further. The TOCTOU fix above applies uniformly to the legacy `remove_file` call
  too, so the one genuine race this claim gestures at (two callers racing to consume the SAME
  legacy record naming the SAME phase) is covered by the same fix — no separate change was
  needed for the cross-phase framing itself, which does not describe a real path in the current
  code.

## 4. Handled — reviewer-confirmed, no action needed

- **Refused handoff (the early `preflight_interactivity_check` at line 1282 specifically)
  leaves state byte-identical.** Both reviewers confirmed this directly by tracing the code:
  the early check runs on a *cloned* candidate state, before any mutation of the real `state`
  variable, and a failure short-circuits via `?` before line 1294's mutation is ever reached.
  This is genuinely fixed/correct — do not re-flag it. (Finding 2b above is a *different*
  claim — about the *later*, stricter preflight inside `launch_stage` — not a contradiction of
  this one.)
- Codex ran the existing targeted test suite for this code (21 ship tests, 5 handoff tests, 3
  hint tests, 2 ship-cleanup tests) and confirmed they pass, but explicitly noted they do not
  exercise the late-preflight (2b), quote-nesting (1), or concurrent-legacy-record (3) cases —
  i.e. the test suite passing is not evidence against any of the findings above.

## What this document does not establish

Originally: a synthesis of two AI reviewers' claims, with finding 1 and the control-flow shape
of 2a independently verified by direct reproduction/code-reading, and findings 2b and 3 **not**
independently verified — reviewer claims carried forward at face value because context ran out
before they could be checked.

**Follow-up session (2026-08-27) update:** all four findings (1, 2a, 2b, 3) are now
independently verified against current source AND fixed, each with a negative control run
before treating the fix as done (test fails against the pre-fix code, passes after) — see the
"Fix status" note under each finding above for specifics. The one correction to the original
synthesis: finding 3's "cross-phase race" framing (codex) does not hold as literally stated
against current source — the existing `instructions.phase == phase` guard already prevents it —
though the TOCTOU it's adjacent to is real and is fixed by the same change as agy's TOCTOU
claim. Full verification: `cargo build --workspace --all-targets`, `cargo clippy --workspace
--all-targets -- -D warnings`, and `cargo fmt --check` (clean except one pre-existing,
untouched-by-this-session diff in `pre_commit_branch_guard.rs`) all pass; `cargo test -p
devflow-core --lib` (736 passed) and `cargo test -p devflow --bin devflow` (350 passed) both
green, plus the specific integration suites most likely to interact with this code
(`phase7_cli`, `doctor_antigravity`, `auto_chain_flag_e2e`, `auto_chain_leak_repair_e2e`,
`gate_sweep_e2e`, `stop_e2e` — 48 tests, all green).

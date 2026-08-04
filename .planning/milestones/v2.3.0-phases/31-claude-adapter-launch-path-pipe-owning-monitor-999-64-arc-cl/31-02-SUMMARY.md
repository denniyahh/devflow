---
phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
plan: 02
subsystem: infra
tags: [process-supervision, idle-timeout, completion-oracle, signals, process-group, serde, git]

requires:
  - phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
    plan: "01"
    provides: "the pipe-owning supervisor loop with its stubbed RecvTimeoutError::Timeout arm, the --idle-timeout-secs plumbing end to end, monitor_log_path, and .process_group(0) group leadership"
provides:
  - "AgentStatus::IdleTimeout — a first-class status distinct from Failed and ResourceKilled, wire/serde form `idle_timeout`"
  - "decide_action maps IdleTimeout to a human review gate, never AutoResume and never Advance, at every stage"
  - "idle_timeout_path + IdleTimeoutRecord/IdleTimeoutCommit — the monitor's authoritative on-disk verdict"
  - "parse_idle_timeout_side_channel, read as the FIRST statement of evaluate_layer1 so no stale stream `result` can shadow it"
  - "parse_idle_timeout_secs / idle_timeout_setting / IdleTimeoutSetting — the workspace's first clamp-and-log config reader"
  - "IDLE_TIMEOUT_FLOOR_SECS = 30 and DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS, documented in OPERATIONS.md"
  - "The idle firing sequence: enumerate commits -> write+fsync verdict -> terminate process group -> log loudly"
affects: [31-03 delivery canary, 31-04 D-11 opt-out, 31-05 acceptance run]

actuals:
  tokens: 17011
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Side-channel verdict file read AHEAD of the capture parser, so a monitor-produced fact cannot be shadowed by an agent-produced one"
    - "Presence-is-the-signal, contents-are-enrichment: an unreadable verdict file still yields the verdict"
    - "Clamp-and-log config reading: pure parse_* returning a resolution VALUE, resolved in the parent process where the operator can see the notice"
    - "Live ordering assertion via a watcher thread sampling liveness at the instant a file appears, rather than post-hoc inspection"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/outcome_policy.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - OPERATIONS.md

key-decisions:
  - "IdleTimeout maps to GateReview, not GateInfra — nothing infrastructural failed; DevFlow chose to stop waiting and the operator has real commits to review"
  - "The side channel's PRESENCE is authoritative; a corrupt record still returns IdleTimeout rather than falling back into the cascade, because falling back is what lets a stale success win"
  - "IdleTimeoutSetting carries a four-variant resolution enum, not the plan's bool — the loud notice must name the configured value, and an unparseable override deserves the same loudness as a clamp"
  - "The env var is read through a string LITERAL, not the IDLE_TIMEOUT_ENV const, because doc_check only scrapes the literal form and the const would have silently exempted it from the operator-doc gate"
  - "Termination signals the child's process GROUP using the in-memory Child handle's pid; holding the unwaited handle is what makes the negative-pid signal safe from pid reuse"

patterns-established:
  - "Every new behaviour carries a mutation check that must produce the opposite result; three mutations were run and reverted, and each reproduced the exact defect its test exists to catch"
  - "Measure the compiler's real coverage instead of trusting a plan's claim about it — the two-site exhaustive-match cost was measured, not assumed"

requirements-completed: [Constraint-5, Constraint-8, D-01, D-02, D-03, D-04, D-05, D-06, D-07, D-08]

duration: 24min
completed: 2026-08-03
status: complete
---

# Phase 31 Plan 02: Idle Timeout — a First-Class Failure Path for a Quiet Claude Child Summary

**A Claude child that stops emitting for longer than a clamped, floor-30-second idle window now
produces an authoritative on-disk `idle_timeout` verdict — naming every commit it made and rolling
back none of them — written and fsynced *before* the child's process group is signalled, and read
ahead of the capture parser so a stale `result` event already in the same stream cannot shadow it.**

## Performance

- **Duration:** 24 min (started 2026-08-03T16:49:59Z, finished 2026-08-03T17:14:15Z)
- **Tasks:** 3 of 3
- **Commits:** 3
- **Files modified:** 5

## Test numbers actually observed

Reported as printed. `cargo test --workspace` and `scripts/check.sh all` were each run **without a
pipe**, so the exit code the tool reported is that command's own, not a `tail`'s (CLAUDE.md).

| Command | Result |
|---|---|
| `cargo test --workspace` (final) | exit 0; **22** `test result: ok` lines, **zero** lines with a non-zero failure count |
| `devflow-core` lib target | `525 passed; 0 failed; 0 ignored; 0 filtered out` |
| `agent_result::tests::as_wire_str_matches_serde_form_for_every_variant -- --exact` | `1 passed; 0 failed; 513 filtered out` |
| `outcome_policy::tests::idle_timeout_gates_review -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `outcome_policy::tests::idle_timeout_is_never_auto_resumed -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `agent_result::tests::idle_timeout_side_channel_wins_over_stale_stream_result -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `agent_result::tests::idle_timeout_side_channel_is_read_even_when_the_capture_is_missing -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `agent_result::tests::idle_timeout_result_carries_the_commits_it_enumerated -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `agent_result::tests::absent_side_channel_leaves_the_cascade_unchanged -- --exact` | `1 passed; 0 failed; 518 filtered out` |
| `agent_result::tests::an_unreadable_idle_timeout_record_still_produces_the_verdict -- --exact` | `1 passed; 0 failed; 518 filtered out` |
| `monitor::tests::idle_timeout_secs_clamps_below_floor_and_logs -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `monitor::tests::idle_timeout_secs_accepts_values_above_floor -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `monitor::tests::idle_timeout_secs_defaults_to_the_floor -- --exact` | `1 passed; 0 failed; 524 filtered out` |
| `monitor::tests::idle_timer_resets_on_every_stream_line -- --exact` | `1 passed; 0 failed; 524 filtered out`, **1.22s** |
| `monitor::tests::idle_timeout_writes_side_channel_before_terminating_child -- --exact` | `1 passed; 0 failed; 524 filtered out`, **3.32s** |
| `monitor::tests::idle_timeout_does_not_roll_back_commits -- --exact` | `1 passed; 0 failed; 524 filtered out`, **0.35s** |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --check` | exit 0 |
| `scripts/check.sh all` | exit 0; no `FAILED`, no `error[`, every `passed;` line carries `0 failed` |

**False-green control for every `1 passed` above.**
`cargo test -p devflow-core --lib agent_result::tests::this_test_does_not_exist_xyz -- --exact`
prints `0 passed; 0 failed; 525 filtered out` and **still reports `ok`** — exactly the trap
CLAUDE.md warns about. The readings above therefore carry a real non-zero `filtered out` and are
not vacuous. Package is `devflow-core`/`devflow`, never `devflow-cli`.

**Baseline:** the devflow-core lib target had **511** tests at the end of plan 31-01 and has **525**
now: +14, matching 3 (task 1) + 5 (task 2) + 6 (task 3) exactly. No test was deleted or replaced.

## Mutation checks — the tests have teeth

Every claim below was produced by temporarily breaking the implementation and observing the
failure, then reverting. None of this code is in any commit (`git status` clean before each commit;
final tree clean).

| Mutation | Result | What it proves |
|---|---|---|
| `as_wire_str` arm returns `"idletimeout"` | `as_wire_str_matches_serde_form_for_every_variant` FAILS: `left: "idletimeout" right: "idle_timeout"` | The wildcard-free match catches a MISSING arm but not a WRONG one; enumerating the variant in the test is what pins it |
| `parse_idle_timeout_side_channel` demoted from first statement into the `.or_else` chain | All 3 side-channel tests FAIL; the key one with `left: Success right: IdleTimeout` | The ordering is load-bearing and the test detects the exact shadowing bug, not a proxy for it |
| `terminate_child_group` moved BEFORE the verdict write | `idle_timeout_writes_side_channel_before_terminating_child` FAILS with `Some(false)` | The D-05 ordering is genuinely observed live, not inferred after the fact |
| `OPERATIONS.md` row for the new env var removed | `doc_check::source_devflow_env_vars_and_subcommands_are_documented` FAILS naming `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` | The doc gate actually SEES the variable (see deviation 1 — it did not, at first) |

## What these tests do NOT establish

The most important section here.

1. **Nothing has run the real `claude` binary.** Every end-to-end assertion drives an `sh` stub. The
   idle timeout is proven against a stub that stops printing; it is **not** proven against a real
   Claude session that goes quiet, whose silence may have different causes and shapes. 31-05's
   acceptance run remains the gate, exactly as 31-01 said.
2. **The 30-second production floor is not validated by any test here.** `idle_timer_resets_on_every_stream_line`
   injects a **400ms** window and runs **1.22s** — 3× the window. It establishes that *the reset
   mechanism works at that scale*. It says nothing about whether 30s is the right production value;
   that rests entirely on Phase 30d's measured gap distributions (every-line max 7.09s), not on
   anything measured in this plan.
3. **`idle_timeout_writes_side_channel_before_terminating_child`'s 3.32s duration is measuring
   `agent::TERMINATE_VERIFY_WAIT`, not the timeout.** The stub installs `trap '' TERM`, so the run
   is dominated by the fixed 3-second SIGTERM→SIGKILL escalation. The 250ms idle window is a
   rounding error against it. Do not read that number as "the timeout takes 3 seconds".
4. **The ordering test proves one ordering, once.** A single observation is a weak reliability
   bound. It shows that on this run the verdict existed while the child was alive. It does not
   establish the absence of an interleaving under load, on a different scheduler, or with a slow
   `sync_all` — only that the sequential structure is correct and that reversing it is detected.
5. **The commit enumeration is tested against a two-commit local repo on `develop`.** It is not
   tested against a worktree checkout, a missing `develop`, a detached HEAD, or a phase branch that
   does not exist. Those paths degrade to an empty list plus a note **by construction** (they cannot
   abort the firing sequence), but that degradation is reasoned, not exercised.
6. **`terminate_child_group`'s group sweep is not directly asserted.** No test spawns a grandchild
   and checks it dies. The tests assert the leader is terminated and the verdict is correct; the
   group semantics rest on `.process_group(0)` plus the negative-pid signal, which are reasoned from
   source, not measured.

## Task Commits

1. **Task 1: A first-class idle-timeout status** — `3641cdb` (feat)
2. **Task 2: A side-channel verdict a stale stream `result` cannot shadow** — `149c11f` (feat)
3. **Task 3: The idle timer — every line resets it, 30s is the floor, verdict lands before the kill** — `de7fae0` (feat)

## Acceptance evidence

- `rg -c 'idle_timeout' crates/devflow-core/src/agent_result.rs` → **3** (≥2 required)
- `rg -c 'AgentStatus::IdleTimeout' crates/devflow-core/src/outcome_policy.rs` → **3** (≥2 required)
- `rg -c 'idle_timeout_path' crates/devflow-core/src/agent_result.rs` → **6** (≥2 required)
- `rg -c 'IDLE_TIMEOUT_FLOOR_SECS' crates/devflow-core/src/monitor.rs` → **5** (≥2 required); value is `30`
- `rg -c 'DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS' crates/devflow-core/src/monitor.rs` → **1** (≥1 required)
- `rg -c 'terminate_and_verify' crates/devflow-core/src/monitor.rs` → **2** (≥1 required) — escalation reused, not reimplemented

**Ordering evidence, as line numbers (the plan asked for both, recorded):** inside `evaluate_layer1`,
`parse_idle_timeout_side_channel` is called at **`agent_result.rs:1795`** and `read_capture` at
**`agent_result.rs:1799`**. 1795 < 1799, so the ordering is real and not merely intended. (The
`read_capture` at line 411 belongs to `session_id_from_capture`, an unrelated function.)

## D-06's real exhaustive-match cost — measured, not assumed

The plan claimed exactly two workspace matches force a compile error. I did not take that on trust:
I added the variant **alone**, with no arms, and ran `cargo check --workspace --all-targets`.

Exactly two distinct source sites broke — `agent_result.rs:92` (`as_wire_str`) and
`outcome_policy.rs:39` (`decide_action`) — each reported twice, once per compilation unit (lib and
lib-test). `pipeline_launch.rs:453` did **not** break, confirming it needs no new arm because no new
`Action` variant was introduced. That file belongs to plan 31-03 and was not touched.

## The three non-exhaustive equality sites (required finding, one per site)

These compile untouched against a new variant, so the wildcard-free-match mechanism never reaches
them. All three were audited by hand. **All three are correct as-is; none was changed.** Each now
carries the finding as a code comment at its own site.

**1. `agent_result.rs` — `parse_claude_event_result`'s `result.status != AgentStatus::Success`
guard. CORRECT AS-IS, unchanged.**
The only route by which `IdleTimeout` arrives here is an agent writing
`DEVFLOW_RESULT: {"status":"idle_timeout"}` into its own output — forging a verdict only the monitor
should produce. The predicate handles that in the fail-safe direction: not `Success`, so it returns
immediately as decisive non-success and `decide_action` gates it. A forged idle timeout can only
make a run gate, never advance. The real monitor verdict never travels this path at all — it is read
from the side channel before this parser runs.

**2. `agent_result.rs` — `reconcile_layer0_verdict`'s `result.status != AgentStatus::Success` guard.
CORRECT AS-IS, unchanged.**
An idle-timeout result is rejected by **both** independent guards, not just one: its status is not
`Success`, and its `decided_by_layer` is `Some(1)`, never `Some(0)`. It returns unchanged, which is
right — that function exists only to graft Layer 1's `verdict` onto an affirmative Layer 0 probe
success, and a timeout is neither.

**3. `pipeline_outcomes.rs` — `classify_validate_outcome`'s `result.status == AgentStatus::Success`.
CORRECT AS-IS, unchanged — with an adjacent pre-existing hazard flagged rather than fixed.**
A monitor-produced timeout has `decided_by_layer: Some(1)` and `verdict: None`, so `external` is
`false`, the match falls to `_`, and Validate classifies as `Failed` — loop back or gate, never
advance. That is the intended routing.

**The adjacent hazard, stated plainly because it argues against tidiness here:** the
`(_, Some(Verdict::Pass))` arm wins regardless of `status`, so an agent writing
`DEVFLOW_RESULT: {"status":"<anything>","verdict":"pass"}` classifies as `Passed`. This is a
*documented deliberate choice* (the function's own doc comment argues for it) and it applies
identically to `Failed`, `Unknown`, and `ResourceKilled` today. `IdleTimeout` neither creates nor
widens it, and changing the arm "for symmetry" would silently re-route those three other statuses.
Per the plan's explicit instruction — change only if the existing predicate is wrong — I left it
alone and recorded it. **It is nonetheless a real pre-existing weakness and a reasonable candidate
for its own backlog entry.** I deliberately guarded against contributing to it: the
monitor-produced `AgentResult` sets `verdict: None` explicitly, and the reason is documented on
`idle_timeout_result`.

## Deviations from Plan

**1. [Rule 2 — missing critical functionality] The new env var was invisible to the repo's own
doc-parity gate. Caught by a negative control, not by a green test.**

- **Found during:** Task 3, after `cargo test --workspace` passed green.
- **Issue:** I first wrote `std::env::var(IDLE_TIMEOUT_ENV)`, reading the variable through a const.
  `doc_check::source_read_env_vars` filters candidate tokens to those appearing as
  `std::env::var("<LITERAL>")` — so the const form compiles, works, and is **completely invisible to
  the gate**. `doc_check` passed green because it could not see the variable, not because the
  variable was documented. This is precisely a proxy measurement passing for the real one.
- **Fix:** read the variable through a string literal (with a comment saying exactly why the const
  form is wrong here), and add a `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` row to `OPERATIONS.md`.
- **Verification:** removed the `OPERATIONS.md` row and confirmed `doc_check` FAILS with
  `source-read environment variable 'DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS' is missing from scoped
  operator docs`, then restored it. Without that negative control I would have reported a green
  doc gate over an undocumented variable.
- **Committed in:** `de7fae0`

**2. [Rule 2 — fail-safe correctness] An unreadable verdict file still yields the verdict.**

- **Found during:** Task 2, while writing `parse_idle_timeout_side_channel`.
- **Issue:** The natural shape returns `None` when the record will not deserialize. That drops
  control back into the `.or_else` cascade — where the stale success sitting in the same capture
  wins. A corrupt file would have become a silent wrong advance: the exact failure the whole task
  exists to prevent, reintroduced through the error path.
- **Fix:** the file's PRESENCE is the authoritative signal and its contents are enrichment. An
  unparseable record returns `IdleTimeout` with a reason saying the detail was lost and nothing was
  rolled back. Covered by `an_unreadable_idle_timeout_record_still_produces_the_verdict`, which
  carries its own negative control (the same fixture decides `Success` without the file).
- **Committed in:** `149c11f`

**3. [Judgment call, not auto-fixed] `IdleTimeoutSetting` carries a four-variant resolution enum,
not the plan's `bool`.**

- The plan specified "the resolved `Duration` and a boolean recording whether the clamp engaged". A
  bool cannot carry the configured value, and D-04 requires the notice to name the configured value,
  the floor, and the value in force. A bool also cannot distinguish an unparseable override from an
  absent one — and `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS=60O` (letter O) silently resolving to 30s would
  reintroduce the silent-misconfiguration class D-04 exists to close, in the one direction that
  actually harms an operator (they asked to RAISE the timeout and got the floor).
- `IdleTimeoutResolution` is `Default | Configured | Clamped { configured } | Unparseable { raw }`,
  with `clamped()` and `notice()` helpers. `ValidateOutcome` in `pipeline_outcomes.rs` makes the
  same "more than two distinguishable outcomes" argument for the same reason, so this follows repo
  precedent rather than inventing a style.

**4. [Judgment call, recorded] `terminate_and_verify` cannot take a process group, so the group
signal is sent alongside it rather than through it.**

- The plan says "terminate the child's process group ... via `crate::agent::terminate_and_verify`
  (negative pid to `libc::kill`)". Those cannot both hold: `terminate_and_verify` explicitly rejects
  any non-positive pid (`signed <= 0 → return false`), which is a deliberate guard against the
  `kill(-1, sig)` and `kill(0, sig)` hazards `agent.rs` documents at length. Passing it a negative
  pid is a no-op, not a group kill.
- **Resolution:** group `SIGTERM` → `terminate_and_verify(child_pid, …)` for the leader's escalation
  and its *verified* liveness fact → group `SIGKILL` sweep. No new escalation or liveness logic was
  written; the existing function is reused for exactly the part it owns. The `signed > 1` guard
  ensures the negation can never produce `-1` or `0`.
- **Residual, stated:** see "What these tests do NOT establish" item 6 — the group semantics are
  reasoned from source, not measured by a test that spawns a grandchild.

**5. [TDD discipline — process deviation, recorded rather than smoothed over]**

- **Task 1's RED was observed and is real:** `cargo test -p devflow-core --lib outcome_policy::`
  exited **101** with three `E0599` errors before the variant existed. But for a type-driven change
  the RED is a *compile failure*, so committing it separately would put a non-building tree in
  history and redden every `git bisect` step that lands on it. 31-01 already paid that cost
  knowingly and recorded it. I therefore committed one commit per task rather than RED/GREEN pairs.
- **Tasks 2 and 3 did not observe a separate RED at all** — I wrote implementation and tests
  together. That is a genuine lapse against `tdd="true"`, not a considered choice, and I am not
  going to dress it up. I compensated with the four mutation checks tabulated above, which are
  strictly stronger evidence than a RED would have been (a RED shows a test fails when the code is
  absent; a mutation shows it fails when the code is *subtly wrong*, which is the case that
  actually matters here). The lapse is recorded because the compensation does not erase it.

**Total deviations:** 2 auto-fixed under Rule 2, 3 recorded judgment/process calls. No scope creep;
no file outside `files_modified` was touched except `OPERATIONS.md`, which deviation 1 made
mandatory.

## Boundary compliance (parallel wave)

31-03 owns `canary.rs`, `lib.rs`, `state.rs`, `pipeline_launch.rs`. **None was edited.** This was
possible without a signature change because 31-01 already threaded `idle_timeout_secs` from
`run_monitor` into `run_pipe_owning_monitor`, so the whole of task 3 landed inside `monitor.rs`.
The commit enumeration reads `GitFlowConfig::default()` locally rather than taking a new parameter,
matching how `pipeline_launch.rs:528` already resolves git flow at the same boundary — chosen
specifically to avoid a signature change that would have rippled into 31-03's file.

`STATE.md` and `ROADMAP.md` were deliberately not touched; the orchestrator owns those after the
wave merges.

## Known limitations (carried forward, not defects)

1. **A grandchild that ignores both group signals can survive.** The sweep is `SIGTERM` to the group,
   then leader escalation, then `SIGKILL` to the group. A process that has left the group entirely
   (its own `setsid`) is unreachable by construction. Not observed; not tested.
2. **`idle_secs` truncates to whole seconds.** A sub-second injected window records `idle_secs: 0`,
   which reads oddly in the verdict text (`silent for 0s`). Harmless in production, where the floor
   is 30, and the ordering test asserts the truncation explicitly so it cannot drift unnoticed.
3. **The `(_, Some(Verdict::Pass))` hazard in `classify_validate_outcome`** — see site 3 above.
   Pre-existing, deliberately not fixed here, worth its own backlog entry.
4. **`cargo check -p devflow-core --all-targets` fails on `test_support` being feature-gated out.**
   Pre-existing and unrelated to this plan (`tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs`
   reference `devflow_core::test_support::git_command`, which needs the `test-support` feature that
   only `cargo test` enables). It does not affect `cargo test --workspace`, `cargo clippy
   --workspace --all-targets`, or `scripts/check.sh all`, all of which are green. Noted so the next
   person does not mistake it for damage from this plan.

## Note on `actuals.tokens` — two scales, and they disagree by 7×

Recorded as **17,011** = chars/4 over the realized diff (68,043 chars), which is the scale my
execution instructions specified. Against `estimate.tokens: 78000` that reads as a **4.6×
overestimate**.

The alternative reading — chars/4 over the full text of the five changed files (481,485 chars) —
gives **120,371**, which against the same estimate reads as a **1.5× underestimate**. Plan 31-01
recorded its `actuals.tokens` on that *second* scale (110,580), so **31-01 and 31-02 are not
directly comparable and must not be averaged**. Both numbers are recorded here rather than picking
the flattering one; a calibration pass must fix the scale before trusting either. This is the same
ambiguity 31-01 flagged and it is still unresolved.

## Next Phase Readiness

- **31-03 (delivery canary)** — unaffected. No file it owns was touched.
- **31-04 (D-11 opt-out)** — the `Legacy` arm is untouched and keeps today's behaviour; the idle
  timer is scoped to the `PipeOwning` arm only, so the opt-out flag disables the timeout for free by
  selecting `Legacy`.
- **31-05 (acceptance run)** — still the blocker, and this plan does not move it. The idle timeout
  has never fired against a real `claude` process. D-19 remains binding: a failing acceptance run
  means 999.64 is not closed whatever these 525 tests say.

## Self-Check: PASSED

All five claimed modified files exist on disk and are tracked. All three claimed commits exist in
`git log 51ad2c6..HEAD`: `3641cdb`, `149c11f`, `de7fae0`. Working tree clean with no untracked files
before each commit and after the last. No commit in this plan deleted a tracked file
(`git diff --diff-filter=D` empty for all three). No `--no-verify` was used; gitleaks ran on every
commit and reported no leaks.

---
*Phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl*
*Completed: 2026-08-03*

---
phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
verified: 2026-08-04T01:45:35Z
status: human_needed
score: 11/13 must-haves verified
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Confirm the fabricated-phase-97 contamination on feature/phase-31 (commits 0590537, 147c9bd, c2efa0d, aa317a5, 1302e93, 5caaa4c, 06832ab, 911bf50) has an operator disposition before this branch is merged to develop."
    expected: "An explicit operator decision — squash/rebase them out before merge, or accept them permanently into develop's history — recorded somewhere (this VERIFICATION.md, a commit, or STATE.md)."
    why_human: "31-ACCEPTANCE.md and 31-05-SUMMARY.md both flag this as needing 'an operator disposition before the branch merges to develop.' I re-confirmed independently that all eight commits are still present on feature/phase-31 as of this verification's HEAD (5647993) and no disposition commit exists. This is a genuine open decision point, not something I can resolve or default on."
  - test: "Confirm that a human operator, not an agent, actually adjudicated the D-19 checkpoint ('closes') referenced in 31-05-SUMMARY.md."
    expected: "The operator's own words confirming the phase closes, separate from the executor's self-report of that adjudication."
    why_human: "31-05-SUMMARY.md states 'The operator adjudicated closes on 2026-08-03' but this is the executor's own claim about a human decision. Per this project's standing rule, no agent message is the user's consent — only the permission system or the user's own words are. I cannot verify this claim from the codebase; it needs the operator's direct confirmation."
  - test: "Weigh the acceptance run's central limitation before trusting the delivery premise for anything beyond a single occurrence: the raw stream-json capture that would show the `background_tasks_changed` non-empty-then-drained sequence (the VOID guard) was deleted during cleanup and never committed. It survives only as transcription in 31-ACCEPTANCE.md."
    expected: "Treat 'the pipe-owning monitor delivered both completions on one real run' as established, and 'it does so reliably' as NOT established — the phase's own acceptance doc already says this, but it is worth re-stating here because it is the single most load-bearing claim in the phase and the primary artifact behind it no longer exists to be independently re-checked."
    why_human: "This is a judgment call about how much weight one un-reproducible, partially-corroborated live-run transcription should carry before the branch ships and 31-04's exit-code arbitration becomes load-bearing for every future Claude Code stage. I found real, independent corroboration (the CLI's background-task ids `a88358fe70a3a44e7` and `a572f3ed8b7f202e3` appear verbatim in the git merge-commit subjects, and the two-parent merge topology is real and re-derivable from git), which supports the transcription rather than contradicting it — but it does not substitute for the missing raw capture."
---

# Phase 31: Claude Adapter Launch Path — Pipe-Owning Monitor (999.64 arc close) Verification Report

**Phase Goal:** a DevFlow-driven phase containing a multi-plan wave completes that wave without
orphaning delegated work — the detached `sh` monitor is replaced by a pipe-owning monitor for the
Claude adapter, and the adapter emits `stream-json` always-on, making 30b's parser reachable in
production for the first time.

**Verified:** 2026-08-04T01:45Z
**Status:** human_needed
**Re-verification:** No — initial verification.

## What I checked and how

I did not trust SUMMARY.md or ACCEPTANCE.md claims at face value. For every load-bearing claim I
either read the actual source at HEAD (`5647993`), ran the real test suite myself, or re-derived
the git evidence independently rather than reading the transcription. Where I could not do either
(a self-report of "no git was run," a deleted raw capture, a claimed human adjudication), I say so
plainly below instead of counting it as verified.

**Ran myself, not trusted from a summary:**
- `cargo build --workspace --release` — exit 0.
- `cargo test --workspace` — exit 0 (captured directly, not through a pipe), **22 suites, 879
  passed, 0 failed**. (ACCEPTANCE.md's post-run number was 876 at commit `62e3a72`; three commits
  landed afterward and current HEAD reports 879 — consistent with a small number of tests added by
  those commits, not a discrepancy.)
- `bash scripts/check.sh all` — exit 0, `==> check.sh: all OK`.
- Two named tests with `-- --exact` plus a same-session negative control on a nonexistent test name
  (`this_test_does_not_exist_zzz` → `0 passed; 0 failed; 540 filtered out`, still reports `ok`) —
  confirming the repo's documented false-green trap and that the real test readings are genuine,
  not vacuous.
- `git show -s --format='%H %P'` on both merge commits (`1302e93`, `5caaa4c`) and their listed
  ancestors (`c2efa0d`, `0590537`, `aa317a5`, `147c9bd`) — independently reconstructing the exact
  topology ACCEPTANCE.md describes, rather than reading its prose.
- `git merge-base --is-ancestor 626131b... HEAD` and `git merge-base 1302e93 5caaa4c` — confirming
  the pre-run SHA is real and is the common ancestor of both merges.
- Direct `rg` reads of `monitor.rs`, `claude.rs`, `agent_result.rs`, `canary.rs`,
  `pipeline_launch.rs`, `commands.rs`, `main.rs`, `state.rs`, `config.rs` against the specific
  constraint and decision text, not against the plan's paraphrase of them.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | The detached `sh` monitor is replaced by a pipe-owning Rust monitor for the Claude adapter | ✓ VERIFIED | `monitor.rs:234-249` `MonitorLaunch::{PipeOwning,Legacy}`; `PipeOwning` arm spawns with `Stdio::piped()` (lines 637-638), `Legacy` keeps `Stdio::null()` byte-for-byte (lines 399-401, 478-480) |
| 2 | Adapter always emits `--input-format stream-json --output-format stream-json`, never selected by predicting which stages will background | ✓ VERIFIED | `claude.rs:56-59` unconditional in `exec_command`; rollout narrowness (`STREAM_JSON_STAGES = &[Stage::Code]`, `pipeline_launch.rs:446`) is an explicit sequencing choice (D-09/D-10), not a per-launch prediction — matches constraint 1's stated exception and is independently recorded in ROADMAP 999.73 |
| 3 | The prompt reaches the child on stdin, not argv | ✓ VERIFIED | `exec_command_carries_no_positional_prompt` test (source-level assertion), `monitor.rs:573-579` `user_turn_line` builds a JSON stdin turn via `serde_json` (not string interpolation, avoiding injection) |
| 4 | Close rule is an AND: marker inside a top-level `result` AND `background_tasks_changed` drained to `[]`/never-announced | ✓ VERIFIED | `monitor.rs:557-559` `should_close()` — `self.marker_seen && matches!(self.pending_background_tasks, None \| Some(0))`. Confirmed with a mutation-tested unit test (`close_rule_requires_both_marker_and_drained_background_tasks`, run directly by me: `1 passed; 0 failed; 539 filtered out`) |
| 5 | Idle timeout (not wall-clock) with a defensible floor is the sole silence guard; verdict written before termination; distinct first-class status; never auto-resumed | ✓ VERIFIED | `monitor.rs:51,97` `DEFAULT_IDLE_TIMEOUT_SECS`/`IDLE_TIMEOUT_FLOOR_SECS` both `120` in source (the D-02 amendment landed, not just documented); ordering (`agent_result.rs:1795` before `:1799`) and D-05/D-06/D-08 behavior covered by mutation-tested tests per 31-02-SUMMARY, present in the suite I ran |
| 6 | Constraint 9 residual: a stream-derived `Success` cannot outrank a contradicting non-zero exit code | ✓ VERIFIED | `agent_result.rs:2206-2246` `reconcile_stream_success_against_exit_code`, wired at `:2292`; ran `stream_success_cannot_stand_against_nonzero_exit_code` myself — `1 passed; 0 failed; 539 filtered out` |
| 7 | D-13 startup canary: a declared token is confirmed only inside a top-level `result`, refuses the run on absent/unverified | ✓ VERIFIED | `canary.rs` + `pipeline_launch.rs:111,333` `canary_gate`; also independently confirmed against the real CLI once, during the acceptance run (`claude_delivery_canary_confirmed`, CLI `2.1.220`, `.devflow/events.jsonl` — I read this event verbatim in the ACCEPTANCE.md transcription, which is the strongest evidence available since the underlying raw capture for it was not separately preserved either) |
| 8 | D-11 opt-out: explicit, off by default, loud on 3 channels, never auto-selected on parse failure | ✓ VERIFIED, with one pre-existing scoped exception | `commands.rs:122,151`, `main.rs:111,168-172`; `relaunch_checkpoint_session` hardcodes `MonitorLaunch::Legacy` unconditionally (pre-existing, unrelated to the parse-failure path, explicitly scoped out by the adversarial review's B2 finding and documented in 31-04-SUMMARY as a known un-migrated route) |
| 9 | 30b's stream parser is reachable and was genuinely exercised by a real production launch, including real backgrounding, real drain, and real notification delivery | ? UNCERTAIN | The launch path and argv are independently confirmed running against the real CLI (canary event, confirmed above). The specific evidence for the drain/background sequence — four `background_tasks_changed` events, non-empty then `[]` — exists **only as transcription** in 31-ACCEPTANCE.md; the raw `.devflow/phase-97-stdout` capture was deleted during cleanup and I could not re-read it. Partial independent corroboration: the CLI's own background-task ids (`a88358fe70a3a44e7`, `a572f3ed8b7f202e3`) appear verbatim inside the git merge-commit subjects I re-derived myself, which is real, non-transcribed evidence — but it corroborates the *branch* story, not the *stream* story directly |
| 10 | D-18: both plans in the acceptance wave produced a `SUMMARY.md` and both merged, through a topology inline work cannot produce | ✓ VERIFIED | Independently re-derived from git, not read from ACCEPTANCE.md's prose: `1302e93` parents `[626131b, c2efa0d]`, `5caaa4c` parents `[1302e93, aa317a5]`; `c2efa0d`→`0590537`→`626131b` and `aa317a5`→`147c9bd`→`626131b` are two chains that independently fork from the same pre-run commit — exactly the shape a single inline agent cannot produce |
| 11 | D-17: the orchestrator ran no `git` command of any kind while the executor held the working tree | ? UNCERTAIN | This is a negative claim about a completed, non-reproducible window (20:32:24Z–20:54:52Z). I have no way to check it after the fact beyond the `stop_until: "code"` state-file mechanism (which I confirmed exists and is real in source) and the executor's own transcript quotes, which I cannot independently audit |
| 12 | Full workspace test suite is green with no regressions | ✓ VERIFIED | Ran myself: `cargo test --workspace` exit 0, 22 suites, 879 passed, 0 failed; `scripts/check.sh all` exit 0 |
| 13 | No unresolved debt markers (`TBD`/`FIXME`/`XXX`) in files this phase touched | ✓ VERIFIED | `rg` swept all 16 files listed across the five plans' `key-files` sections — zero matches |

**Score:** 11/13 truths verified (2 uncertain — items 9 and 11 — routed to human verification, not
counted as failed; behavior-dependent state transitions in items 4–6 all had passing, mutation-tested
unit tests, so none of them needed to fall back to "present but unverified").

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/monitor.rs` | `MonitorLaunch`, `run_pipe_owning_monitor`, `CloseRule`, idle-timeout constants | ✓ VERIFIED | Exists, substantive, wired — confirmed by direct read and by the running test suite |
| `crates/devflow-core/src/agents/claude.rs` | stream-json argv, no positional prompt | ✓ VERIFIED | `exec_command` (lines 46-72) confirmed |
| `crates/devflow-core/src/agent_result.rs` | idle-timeout side channel, exit-code arbitration | ✓ VERIFIED | `reconcile_stream_success_against_exit_code` wired into `evaluate_agent_result`'s cascade |
| `crates/devflow-core/src/canary.rs` | declared-token delivery guard | ✓ VERIFIED | New file, `run_delivery_canary` etc., wired into `pipeline_launch.rs` |
| `crates/devflow-cli/src/main.rs` | `Monitor` subcommand, `--legacy-claude-launch` flag | ✓ VERIFIED | Both present |
| `crates/devflow-cli/src/pipeline_launch.rs` | `canary_gate`, launch-shape selection, opt-out wiring | ✓ VERIFIED | `canary_gate` called at the real launch site (line 111), not just defined |
| `OPERATIONS.md` | documents the new subcommand and env vars | ✓ VERIFIED | Confirmed via the doc-parity gate passing in the full suite run |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `ClaudeAgent::exec_command` | `MonitorLaunch::PipeOwning` | `pipeline_launch.rs` launch-shape resolution | WIRED | `resolve_launch_shape` selects the arm based on `claude_stream_launch_enabled`, which reads `STREAM_JSON_STAGES` and the opt-out flag |
| `monitor.rs` `CloseRule` | `agent_result.rs` `event_is_top_level_result_marker` | direct function call | WIRED | `CloseRule::observe` at line 539 calls it rather than re-implementing provenance trust |
| `canary.rs` `run_delivery_canary` | `pipeline_launch.rs` `canary_gate` | direct call at stage launch | WIRED | Confirmed at `pipeline_launch.rs:111`; a refused canary clears `monitor_pid` before returning, confirmed in source |
| `agent_result.rs` exit-code arbitration | `evaluate_agent_result` cascade | direct call | WIRED | Called at line 2292, after Layer 1 produces a `Success`, before it is returned |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| AND close rule fires only on both arms | `cargo test -p devflow-core --lib monitor::tests::close_rule_requires_both_marker_and_drained_background_tasks -- --exact` | `1 passed; 0 failed; 539 filtered out` | ✓ PASS |
| Negative control for the above | `cargo test -p devflow-core --lib monitor::tests::this_test_does_not_exist_zzz -- --exact` | `0 passed; 0 failed; 540 filtered out`, still `ok` | ✓ PASS (confirms the trap, not vacuous) |
| Stream success cannot outrank non-zero exit | `cargo test -p devflow-core --lib agent_result::tests::stream_success_cannot_stand_against_nonzero_exit_code -- --exact` | `1 passed; 0 failed; 539 filtered out` | ✓ PASS |
| Full workspace suite | `cargo test --workspace` | exit 0, 22/22 suites `ok`, 879 passed, 0 failed | ✓ PASS |
| Repo gate | `bash scripts/check.sh all` | exit 0 | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` convention exists in this repository and neither the PLAN files
nor SUMMARY files reference one. **Step 7c: SKIPPED (no probe scripts in this project; verification
performed via `cargo test` and direct git/source inspection instead).**

### Requirements Coverage

Not applicable. The ROADMAP entry states `Requirements: TBD — no REQ-IDs, consistent with this
project's convention for infrastructure phases; tracked by the constraint numbers above.` I tracked
against the six binding constraints and D-01..D-19 instead — see the Observable Truths table and
the constraint/decision cross-check below.

### Constraint and decision cross-check

| Item | Status | Note |
|------|--------|------|
| Constraint 1 (always-on switch, no behavior prediction) | ✓ satisfied | Truth 2 |
| Constraint 4 (AND close rule) | ✓ satisfied | Truth 4 |
| Constraint 5/8 (idle timeout, floor ≥30s) | ✓ satisfied | Truth 5; floor amended to 120s in source with recorded provenance (`31-IDLE-GAP-MEASUREMENTS.md`) |
| Constraint 7 (coalescing) | ✓ addressed | `coalesced_completions_do_not_undercount_children` test exists and is part of the passing suite; the acceptance run's own transcription notes it did NOT exercise true coalescing (two separate drains, not one coalesced pair) — a real, already-acknowledged gap in live evidence, not in the unit-level defense |
| Constraint 9 residual (exit-code arbitration) | ✓ satisfied | Truth 6 |
| D-14 (per-child tokens) | Deferred, recorded | CONTEXT.md records it as deferred on size. ROADMAP 999.73/DEN-94 lists it as "the adjacent deferral from the same planning session" rather than as its own dedicated backlog ticket — a minor accuracy note: 999.73/DEN-94 is primarily about widening `STREAM_JSON_STAGES`, not per-child tokens specifically. Not a phase gap; worth knowing if D-14 is ever searched for by ticket number |
| D-02 amendment (idle floor 30s→120s) | ✓ landed in source | `IDLE_TIMEOUT_FLOOR_SECS = 120`, `DEFAULT_IDLE_TIMEOUT_SECS = 120`, `CANARY_IDLE_SECS = 120` all confirmed by direct read |

### Anti-Patterns Found

None. Swept all 16 files this phase's five plans list under `key-files` for `TBD`/`FIXME`/`XXX`
(zero matches) and `TODO`/`HACK`/`PLACEHOLDER` (zero matches).

### Post-run repository health (re-verified independently, not trusted from ACCEPTANCE.md)

| Command | Exit | Result |
|---------|------|--------|
| `cargo build --workspace --release` | 0 | Builds clean |
| `cargo test --workspace` | 0 | 22 suites, 879 passed, 0 failed |
| `bash scripts/check.sh all` | 0 | `==> check.sh: all OK` |
| `git merge-base --is-ancestor 626131b HEAD` | 0 | Pre-run SHA is a real, reachable ancestor |
| `git status --porcelain` | — | clean |

### Human Verification Required

See frontmatter `human_verification` for the full items. Summarized:

1. **The fabricated phase-97 commits are still on `feature/phase-31`, unresolved.** All eight
   commits (`0590537` … `911bf50`) are present at current HEAD. Both `31-ACCEPTANCE.md` and
   `31-05-SUMMARY.md` explicitly say this "needs an operator disposition before the branch merges
   to `develop`." I found no disposition. This is an open action item, not something I am
   defaulting on.
2. **The claimed operator adjudication of D-19 ("closes") is the executor's self-report.** I cannot
   verify a human actually made that decision from the codebase alone.
3. **The central delivery-mechanism evidence (the `background_tasks_changed` drain sequence) exists
   only as a transcription**, because the raw capture that would let anyone re-verify it was deleted
   during the run's own cleanup. I found real, independent corroboration (background-task IDs
   embedded in git merge-commit subjects; the two-parent merge topology), which supports rather than
   contradicts the transcription — but it is not the same as being able to re-read the capture
   myself, and the phase's own documents already flag one passing run as "a weak reliability bound."

### Gaps Summary

No FAILED truths. Every constraint and decision I could check against source and a real test run
was actually implemented, not stubbed, and the mechanism-level claims (pipe ownership, the AND
close rule, the idle-timeout ordering, the exit-code arbitration, the canary) are all backed by
tests I ran myself and confirmed pass with genuine (non-vacuous) readings. The merge topology behind
D-18 — the exact inverse of the 999.64 failure this phase exists to close — is independently
re-derivable from git and I reproduced it myself rather than trusting the transcription.

What keeps this from a clean `passed` is not a missing or stubbed artifact — it is that the single
most important piece of live evidence (the raw stream capture proving the drain sequence actually
happened, as opposed to two plans running some other way) no longer exists to be checked, and one
real housekeeping loose end (the branch contamination) is still open and was explicitly flagged by
the phase's own authors as needing an operator's word before shipping. Both are legitimate items for
a human, not code defects.

---

*Verified: 2026-08-04T01:45:35Z*
*Verifier: Claude (gsd-verifier)*

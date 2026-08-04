---
phase: 30-keep-the-session-alive-past-turn-end
plan: 02
subsystem: experiment-harness
tags: ["experiment", "harness", "python", "claude-cli", "redaction", "reliability"]

# Dependency graph
requires:
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30a-evidence/run_experiment_v3.py — the experiment core (two-child prompt, held-open stdin) this harness reuses"
  - phase: 27-hermetic-git
    provides: "hermetic_command / REPO_LOCAL_GIT_VARS / ALSO_REDIRECTING_GIT_VARS — the env-scrub list parsed at runtime"
provides:
  - "30c-monitor-env-harness.py — spawn_monitor environment replica with a held-open stdin pipe"
  - "launch_in_monitor_env(workdir, prompt, argv, stage_dir, extra_scrub_names) -> LaunchedRun"
  - "run_single_trial / _run_replication_set — one trial, or N held-fixed trials"
  - "publish_jsonl / publish_text — validate -> structural redact -> secret-scan -> atomic replace"
  - "scan_for_secrets — pattern-name-only credential/PII scanner, reusable standalone"
  - "discover_agent_session_markers — CLAUDE*/ANTHROPIC*/AI_AGENT* enumeration"
  - "30c-VERDICT.md delivery: confirmed — the gate on Phase 31 being planned"
affects: ["30-04", "31-monitor-rewrite", "999.64"]

actuals:
  tokens: 19404
  tasks: 3
  commits: 7

tech-stack:
  added: []
  patterns:
    - "Stage raw subprocess output outside the committed tree; publish only through validate -> redact -> secret-scan -> atomic replace (an OS-level redirection cannot be intercepted by a redactor)"
    - "Secret scanners report matched PATTERN NAMES, never matched values"
    - "Prove a sanitiser with a paired control: scan the unsanitised staged input in the same breath, so a clean result is a real negative rather than a broken scanner"
    - "Replication sets use a FIXED observation window; a variable early-stop confounds trial comparison"
    - "Parse constants from live source at runtime rather than transcribing them; abort on a zero-yield parse"

key-files:
  created:
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-monitor-env-harness.py"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-VERDICT.md"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-VERDICT-scrubbed.md"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-VERDICT-reliability.md"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-evidence/"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-evidence-scrubbed/"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30c-evidence-reliability/trial-1..5/"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/deferred-items.md"
  modified:
    - ".gitignore"

key-decisions:
  - "Raw output stages in tempfile.mkdtemp outside .planning/; the committed write is a separate later step — the only ordering under which redaction can precede a committed write"
  - "Env-scrub list PARSED from crates/devflow-core/src/git.rs at runtime, aborting on unreadable file or zero-yield const (T-30-08)"
  - "Credential-named keys are redacted structurally so the scan's assignment rule cannot false-positive on them; numeric values are excluded so usage counters like cache_read_input_tokens do not block a clean publish"
  - "Scrubbed ANTHROPIC_API_KEY/ANTHROPIC_TOKEN in trials 2-7 on the apiKeySource:'none' reading — later shown to be over-scrubbing (see Findings F-2)"
  - "Fixed 75s observation window for the replication set rather than early-stop, so all five trials are comparable"
  - "Did NOT write the SUMMARY at the checkpoint: the plan requires the operator's Task 3 response verbatim, so writing it early would have fabricated an answer"

patterns-established:
  - "Cheap probe before an expensive measurement: a 20s PROBE-OK run validated the scrubbed environment before spending a 90s trial on it"
  - "Judge delivery per child (spawned task_id appears as a task_notification) plus the drain — never by counting notification-origin result events"

requirements-completed: ["30c", "constraint-6"]

coverage:
  - id: D1
    description: "Delivery of task-notification-origin results survives spawn_monitor's process environment (sh -c, git scrub, detached, no TTY, separated stderr)"
    requirement: "30c"
    verification:
      - kind: experiment
        ref: "30c-evidence/raw_output.jsonl — 3 result events, 2 notification-origin, drain at line 47"
        status: pass
    human_judgment: true
  - id: D2
    description: "The result is not an artifact of running inside an agent session"
    requirement: "30c"
    verification:
      - kind: experiment
        ref: "30c-evidence-scrubbed/ — 11 markers removed, claude_session_markers '(none)'; and 30c-evidence-operator/ — parent_process fish, markers genuinely absent"
        status: pass
    human_judgment: true
  - id: D3
    description: "Delivery is not a one-off: 8 trials across 3 environments, 0 refuted, 0 partial"
    requirement: "30c"
    verification:
      - kind: experiment
        ref: "30c-evidence-reliability/trial-1..5 (5/5) + 30c-evidence + 30c-evidence-scrubbed + 30c-evidence-operator"
        status: pass
    human_judgment: true
  - id: D4
    description: "No committed evidence file leaks home paths, usernames, session identifiers or credential-shaped tokens"
    requirement: "constraint-6"
    verification:
      - kind: scan
        ref: "scan_for_secrets over all published files -> 0 matches, with staged captures matching as a live control"
        status: pass
    human_judgment: false
  - id: D5
    description: "Verdict frontmatter counts match the raw capture"
    requirement: "30c"
    verification:
      - kind: script
        ref: "verify-verdict-frontmatter.py — ALL FIELDS MATCH, exit 0, on both 30c-VERDICT.md and 30c-VERDICT-scrubbed.md"
        status: pass
    human_judgment: false
  - id: D6
    description: "crates/ is byte-identical — monitor.rs and git.rs were read, never written"
    requirement: "30c"
    verification:
      - kind: manual
        ref: "git status --porcelain crates/ empty at every task boundary"
        status: pass
    human_judgment: false

duration: 41min
completed: 2026-08-02
status: complete
---

# Phase 30 Plan 02: 30c Production-Environment Experiment Summary

**`task-notification` delivery survives DevFlow's production launch environment — 8 trials, 3 environments, 0 refutations — so Phase 31 may be planned; but 8 successes bound reliability only above ~69%, so it must be planned with a retry/timeout path, not a happy path.**

## Operator sign-off (Task 3)

The operator's response to the blocking checkpoint, verbatim:

```
approved
```

## Performance

- **Duration:** ~41 min of execution (14:42–15:23 across 6 commits), spread over a longer window with two coordinator-directed extensions and the checkpoint open between them
- **Tasks:** 3 (2 auto + 1 blocking checkpoint)
- **Commits:** 7 (6 mine + the correction commit below)
- **Files:** 1 harness, 4 verdicts/logs authored, 24 evidence files published

## The verdict

`30c-VERDICT.md` frontmatter carries `delivery: confirmed`. Per the ROADMAP's locked decision that unblocks planning Phase 31.

| Trial | Environment | Children delivered | Drain | `result` events | Notification-origin | Verdict |
|-------|-------------|--------------------|-------|-----------------|---------------------|---------|
| 1 | agent session, markers inherited | 2 / 2 | yes | 3 | 2 | confirmed |
| 2 | markers scrubbed | 2 / 2 | yes | 2 | **1** | confirmed |
| rep 1–5 | markers scrubbed, git decoys planted | 2 / 2 each | yes each | 3 each | 2 each | confirmed ×5 |
| operator | **plain fish shell**, markers genuinely absent | 2 / 2 | yes | 3 | 2 | confirmed |

**8 / 8 across 3 environments.** Zero refuted, zero partial, zero unparseable lines.

The operator's eighth trial (`30c-evidence-operator/`, commit `e0a6078`) is the one no in-session trial could produce: `parent_process: fish`, `launched_from_agent_session: False`, `claude_session_markers: (none)` — absent by ancestry rather than by scrubbing. That closes the process-ancestry gap every earlier verdict listed under its own limits.

## Findings

### F-1 — Result-event count is NOT a count of delivered children (constraint 7)

**The most valuable thing this plan produced.**

Trial 2 delivered both children through **one** resumed turn: 2 `result` events, 1 notification-origin, both children delivered. The CLI coalesced two completions that landed 4.75s apart (t+30.62, t+35.37) into a single turn. Its own output says so unprompted: *"Both subagents completed and both notifications were delivered to this orchestrator turn."*

Coalescing occurred in **1 of 8 trials** — rare, not a fluke to discount. When it happens the signature is **indistinguishable from "one child lost"** unless the drain is checked separately.

A monitor that counts notification-origin `result` events to decide how many children returned would have undercounted trial 2. Delivered-children was 2 in all 8 trials; notification-origin results was 2 in seven and 1 in the eighth.

Bound into `ROADMAP.md` as **binding constraint 7** (line 2603), including why constraint 4's close rule is an `AND` rather than redundancy.

### F-2 — Trials 2–7 over-scrubbed, and my reasoning was over-read by one step

**Surfaced by the operator's plain-shell trial. Recorded as a correction, not restated.**

I scrubbed `ANTHROPIC_API_KEY` and `ANTHROPIC_TOKEN` along with the session markers, justified by `apiKeySource: 'none'` in both trial 1 and the 30a baseline, and concluded the CLI "was already ignoring those variables."

That inference does not hold. `apiKeySource: 'none'` establishes the CLI does not **authenticate** from the key. It does not establish the key is inert. The operator's mise config sets `ANTHROPIC_API_KEY` globally (`~/.config/mise/config.toml:13`), so **production carries it**, and their trial's stderr shows a warning none of my scrubbed trials produced:

```
⚠ claude.ai connectors are disabled because ANTHROPIC_API_KEY or another auth
source is set and takes precedence over your claude.ai login
```

So trials 2–7 ran with connectors **loaded** where production runs with them **disabled** — a real divergence from production, introduced by a scrub I described as making the environment closer to it.

**Delivery succeeded in both conditions** (6 scrubbed trials and 2 unscrubbed), so the verdict is unaffected. What was defective is the reasoning: a verified check extended one step past what it supports. `30c-VERDICT-scrubbed.md` now carries this correction inline rather than leaving the over-read conclusion standing.

The general lesson, which is the reusable part: *a check that proves X does not prove not-Y for any Y sharing the same variable.*

### F-3 — Idle-timeout floor for Phase 31 (constraint 8)

The longest quiet gap between events in a **healthy** run was 10.52–11.51s, in a tight band across all seven of my trials. **An idle timeout below ~12s would have killed every one of them.**

The drain-to-last-result lag varied 4.54s → 11.51s — a 2.5x spread, the least stable interval measured. That is exactly the window where a monitor would be tempted to conclude "the task set is empty, nothing more is coming." Concluding that at the drain would have **truncated the final orchestrator turn in all seven trials**. The drain is not a stop signal.

Bound into `ROADMAP.md` as **binding constraint 8** (line 2624).

### F-4 — The staged-then-publish ordering caught a real leak on every single run

Not theoretical. The secret scan matched `home_path`, `os_username` and `session_identifier` on the **staged** capture of every trial, and matched nothing on any published file. The redaction did real work 8 times, not just against a synthetic fixture.

The same scanner run over the three already-committed `30a-evidence` captures matches all three patterns — logged in `deferred-items.md` as an out-of-scope discovery with a proven cheap fix, deliberately not fixed here (scope fence) and deliberately not added to `WINDOWS.md` (an open entry blocks `/gsd-ship`, which is the operator's call).

## Statistical honesty — read this before planning Phase 31

**The gate proves the mechanism works. It does not establish that it is reliable.**

Eight consecutive successes give an exact one-sided 95% binomial bound of `0.05^(1/8) = 0.688`, so the true success rate is established only as **above roughly 69%**. Reaching a 95% floor at the same confidence needs ~59 consecutive successes.

Concretely: a mechanism that fails 1 run in 5 is entirely consistent with this evidence. **Phase 31 must be planned with a retry/timeout path, not a happy path.** Constraint 8's idle-timeout floor is one half of that; a retry or reconciliation path for a notification that never arrives is the other, and nothing here establishes it is unnecessary.

Further limits, all stated in the verdicts: both children are trivial sleeps (nothing about long-running agents or heavy output — the Phase 29 wave-2 shape in every respect but duration and payload); seven of eight trials ran back-to-back inside ~7 minutes and share that window's conditions; the 75s observation window is an assumption, though the latest result across all trials landed at 51.97s.

## Must-Haves Verified

| Truth | Status | Evidence |
|---|---|---|
| The production-environment question has a recorded binary answer on disk | met | `30c-VERDICT.md` `delivery: confirmed`, verifier exit 0 |
| Harness replicates five characteristics, deviates in exactly one, residue enumerated by name | met | Runtime smoke test: planted `GIT_DIR`/`GIT_WORK_TREE` absent in child, sid differs, `isatty` false ×3, stderr separate, 72 bytes reached child stdin |
| `monitor.rs` byte-identical to pre-phase state | met | `git status --porcelain crates/` empty at every task boundary |
| No raw unsanitised output ever written to a committed path | met | Staged in `tempfile.mkdtemp` outside repo; scan matched staged, clean on published, 8×|
| Archived evidence contains no home paths, usernames, session ids, output-file paths, tokens | met | `scan_for_secrets` 0 matches over all 24 published files |
| `confirmed` requires BOTH children delivered | met | Per-child `task_id` correlation in every verdict; trial 2 passes only via the drain arm |
| A refutation would be a first-class result | met (vacuously) | No trial refuted; `## Rejected options` correctly absent |

## Task Commits

1. **Task 1** — `fb408c4` (feat): the harness — five replicated characteristics, runtime-parsed scrub list, staged-then-publish pipeline
2. **Task 2** — `8e93c9c` (feat): trial 1 evidence + `30c-VERDICT.md`
3. **Out-of-scope log** — `8143617` (docs): `deferred-items.md`
4. **Extension A** — `89512ee` (feat): agent-marker scrub for the control arm
5. **Extension A** — `972a5a6` (feat): trial 2 evidence + `30c-VERDICT-scrubbed.md`
6. **Extension B** — `94f37b7` (feat): 5-trial replication set + `30c-VERDICT-reliability.md`
7. **Task 3 close** — this commit: operator sign-off, F-2 correction, SUMMARY

Operator-authored: `a148377`, `8fdc4cd` (constraints 7 and 8), `e0a6078` (verifier + plain-shell trial).

## Deviations from Plan

### 1. [Rule 3 — Blocking] `.gitignore` gains `__pycache__/`

Importing the harness writes bytecode into the phase directory, dirtying the tree on every run and on 30-04's reuse. Committed in `fb408c4`.

### 2. [Scope] Four artifacts beyond the plan's three `files_modified`

`30c-VERDICT-scrubbed.md`, `30c-VERDICT-reliability.md`, `30c-evidence-scrubbed/`, `30c-evidence-reliability/` — all from operator-approved coordinator extensions after the checkpoint opened, plus `deferred-items.md` per the executor's scope-boundary rule.

### 3. [Process] SUMMARY deliberately withheld at the checkpoint

The plan's `<output>` requires the operator's Task 3 response verbatim. Writing it at the checkpoint would have either fabricated an answer or marked an incomplete plan `status: complete`.

### 4. [Method] `select()` replaced by a polling tailer

Production's capture path is a file written by the `sh` redirection; `select()` on a regular file is always-ready and carries no information. The never-block guarantee is preserved with a bounded poll plus partial-line buffering.

## Issues Encountered

- **My first replication analysis reported a false 5/7.** Trials 1–2 predate the `signal_*_contents` run-log field, so my analyser read their children as `0/2`. Caught before reporting, corrected with a fallback to the recorded signal arrival time. Had I shipped it, the phase record would claim two failures that never happened.
- **Two wrong figures caught pre-commit in the reliability verdict:** a 394-line total (actual 280) and a rule-of-three attribution for the 55% bound (rule of three gives 60% for n=5; 55% is the exact binomial bound). Both corrected before the commit.
- **A concurrent session committed to `feature/phase-30` during execution** (`99372c8`, `36cd19a`), touching `ROADMAP.md` and briefly showing it as modified in my tree. Not my writes; content verified unchanged against `HEAD`.

## Next Phase Readiness

**Phase 31 is unblocked and may be planned**, per the ROADMAP's locked decision.

It inherits three things that change its design, not just its go/no-go:
- **Constraint 7** — never count result events as a child count (F-1).
- **Constraint 8** — idle timeout above ~12s; the drain is not a stop signal (F-3).
- **A ~69% reliability floor** — plan a retry/timeout path, not a happy path.

Plans 30-03 and 30-05 are unaffected: their conditional stop was contingent on a refutation that did not occur.

Reusable by 30-04 without reimplementation: `launch_in_monitor_env`, `run_single_trial`, `publish_jsonl` / `publish_text`, `scan_for_secrets`.

## Self-Check: PASSED

Verified on disk this session, not recalled:

- All 7 commits present in `git log`; the 6 attributed to me contain exactly the files listed.
- `verify-verdict-frontmatter.py` run on both committed verdicts: `ALL FIELDS MATCH`, `EXIT=0` both times.
- `scan_for_secrets` over all published evidence and all four verdicts: 0 matches, with staged captures still matching as a live control.
- Operator's `run.log` read directly: `parent_process: fish`, `launched_from_agent_session: False`, `claude_session_markers: (none)`; capture re-parsed: `spawned 2 delivered 2 drained True`, 3 results, 0 unparseable.
- ROADMAP constraints 7 and 8 located at lines 2603 and 2624.
- `~/.config/mise/config.toml:13` sets `ANTHROPIC_API_KEY` (name only read; value never printed).
- `0.05**(1/8) = 0.6877` and the 59-trial figure computed, not estimated.
- `git status --porcelain crates/` empty; `git diff HEAD` over `STATE.md` and `ROADMAP.md` empty.

---
*Phase: 30-keep-the-session-alive-past-turn-end*
*Completed: 2026-08-02*

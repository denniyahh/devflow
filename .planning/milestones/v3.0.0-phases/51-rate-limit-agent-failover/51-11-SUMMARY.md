---
phase: 51-rate-limit-agent-failover
plan: 11
subsystem: docs
tags: [operations, rate-limit, failover, doc_check, phase-gate]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "--fallback-agent (51-04), the devflow.toml fallback_agent key (51-14), the failover events and the relaunch-failure gate (51-01, 51-06), the idle-timeout cause and torn-line clause (51-03, 51-05), the WR-12 verdict (51-09)"
provides:
  - "Operator docs for the rate-limit fallback: start/resume rows, cron row, a Rate-limit failover section in OPERATIONS.md, README sentence, unattended-mode bullet, configuration-guide key"
  - "doc_check::operations_md_documents_the_fallback_agent_and_its_events pinning those fragments"
  - "Phase 51 gate run: check.sh all, supply-chain scan, semver-checks, container parity"
affects: [phase-51-verification, release-3.1.0]

actuals:
  tokens: 5070
  tasks: 2
  commits: 2
plan_head_before: f96adc120e128695103ca3ba8919f801b87a3810
plan_head_after: 1e610890c3ef0a3ca062905e34bfbe943eae3066

tech-stack:
  added: []
  patterns:
    - "Row-scoped doc pin: row_findings reads only the first line starting with a prefix, so a fragment moved to another row does not satisfy it"

key-files:
  created: []
  modified:
    - OPERATIONS.md
    - README.md
    - docs/guides/unattended-mode.md
    - docs/guides/configuration.md
    - crates/devflow-core/src/doc_check.rs

key-decisions:
  - "The failover description lives in a new OPERATIONS.md section, Rate-limit failover, placed before Notify hook. A paragraph inside the .devflow/ inventory table would have broken the table"
  - "WR-12 is closed (51-09), so the notify limit sentence now states the fixed behaviour: the gate reloads the state file when it opens and fires the edited command. The 999.154 reference is gone"
  - "The remedy after agent_handoff_relaunch_failed is 51-06's Operator remedy section, labelled in the doc as read from the code, not from a live run"
  - "Task 1's GREEN commit is docs(51-11), not feat(51-11): the change is documentation only"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "OPERATIONS.md documents --fallback-agent and the fallback_agent key on the start row, --fallback-agent on the resume row, the cron record's fallback condition, and the failover events and keys; README.md and the unattended guide name the flag; the configuration guide names the key"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::operations_md_documents_the_fallback_agent_and_its_events"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
    human_judgment: false
  - id: D2
    description: "The docs promise no more than the code does: best-effort launch validation, one hop at Code only, the cron arm on a refusal before the handoff, a Code gate on a relaunch that fails after it, and the key read by start only"
    requirement: SUPV-02
    verification: []
    human_judgment: true
    rationale: "The plan's prohibition is judgment-tier. The doc pin checks that fragments are present, not that the prose is accurate. Each claim was checked against the code by the executor (listed under Claims checked against the code), but no test asserts prose accuracy"
  - id: D3
    description: "Phase gate: scripts/check.sh all, the supply-chain scan, semver-checks and container parity all exit 0"
    verification:
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
      - kind: other
        ref: "cargo deny check bans licenses sources; cargo deny check -W vulnerability -W unmaintained -W unsound -W notice -W yanked advisories; cargo machete"
        status: pass
      - kind: other
        ref: "cargo semver-checks check-release -p devflow-core"
        status: pass
      - kind: other
        ref: "scripts/check-in-container.sh"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-11-08
status: complete
---

# Phase 51 Plan 11: Operator Docs for the Rate-Limit Fallback and the Phase Gate Summary

**The operator docs now say how to arm a Code-stage fallback by flag or by `devflow.toml`, what it does and does not do, which events it writes, and how to recover when its relaunch fails. A row-scoped `doc_check` test pins this. The phase gate is green on the host and in the pinned container. The scan found no advisories, and semver-checks reports no update required.**

## Performance

- **Duration:** about 25 min
- **Started:** 2026-11-08T00:02:03Z
- **Completed:** 2026-11-08T00:27:01Z
- **Tasks:** 2 of 2
- **Files modified:** 5

## Accomplishments

- **OPERATIONS.md**
  - The `devflow start` row adds `[--fallback-agent <agent>]` and states its limits: one hop, Code only, the same worktree and branch, saved in `state-NN.json`, refused when equal to `--agent`, refused when its binary is missing or its driver unhealthy, and only `claude` or `antigravity` in `--mode auto`. It says the launch check is best effort: a refusal at the handoff takes the cron arm, and a relaunch failure after the handoff stops at a Code gate. It also covers the `fallback_agent` key: the flag wins, a note names the key's source, and no environment variable selects a fallback.
  - The `devflow resume` row adds `[--agent <agent>] [--fallback-agent <agent>]`. `--agent` onto the saved, unused fallback is refused unless `--fallback-agent` names another agent. `--fallback-agent` sets the fallback or re-arms a spent one, and only on a run that is not live. `resume` does not re-read `devflow.toml`.
  - The `cron-instructions-NN.json` row says when the file is written and when it is not (R2-D1). It also notes that the cron arm writes no file at the infra ceiling (the run aborts) or when the retry time does not parse (the run stops at a gate).
  - A new **Rate-limit failover** section covers the two answers, the top-level-only rule for rate limits, and the idle-timeout cause and torn-line wording (D-15, D-16). It has a table of `agent_handoff`, `agent_handoff_refused` and `agent_handoff_relaunch_failed` with their keys. It says a never-silent Code gate (`gate_fired`) follows a failed relaunch, never cron, and why. It gives 51-06's remedy.
  - Notify hook: the 999.154 limit sentence is replaced with WR-12's fixed behaviour (D-17).
- **README.md:** the rate-limit sentence names both answers and links to the new section.
- **docs/guides/unattended-mode.md:** a third refused-launch bullet. A fallback must launch on the stream shape, so auto mode accepts only `claude` or `antigravity`. The guard is not widened.
- **docs/guides/configuration.md:** `fallback_agent = "antigravity"` is in the example. One paragraph says the key is read by `start` only, the flag overrides it, there is no env var, and a broken key or unparseable file still refuses `start` with the flag (51-14).
- **`doc_check::operations_md_documents_the_fallback_agent_and_its_events`**, with a `row_findings` helper that reads only the first row with each prefix. The fragment list holds neither `plan_incomplete_on_success` nor `capture_tail`. Checked with `rg` over `doc_check.rs` and `OPERATIONS.md`: no match. As a control, the same pattern matches 4 lines in the PLAN.

## Task Commits

1. **Task 1 RED:** `e3cccac` test(51-11): add failing doc pin for the fallback agent and its events
2. **Task 1 GREEN (tracer):** `1e61089` docs(51-11): document the rate-limit fallback agent and its events
3. **Task 2:** no commit. The gate needed no fix in this plan's files.

## TDD Gate Compliance

RED `e3cccac` → GREEN `1e61089`. The GREEN commit descends from the RED commit. No REFACTOR commit was needed. The GREEN commit is `docs(51-11)`, not `feat(51-11)`, because the change is documentation only. An executor-gate grep for `^feat\(51-11\)` therefore finds nothing. This is the same shape 51-09 recorded.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `doc_check::operations_md_documents_the_fallback_agent_and_its_events` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | The real-tree assertion at `doc_check.rs:881` listed all 13 findings: 4 row findings, 6 event or key fragments, and the 3 guide fragments | Intended. Every planted control passed first, because the panic is at the real-tree assertion and not at a control. No build or discovery fault |

The evidence came from a nextest JUnit report, written by a scratch `red` profile and passed through `gsd_run check tdd-red-evidence`. The record is `target/red-evidence/51-11-t1-docs.json` (gitignored). The RED run was on the unformatted test. `cargo fmt` then reflowed it before the commit, and that change is formatting only.

**Real-tree negative control (GREEN).** I removed `--fallback-agent` from only the `devflow resume` row of the real OPERATIONS.md. The flag was still in the start row and in the new section. The test failed (exit 100) with exactly one finding: `the row starting "| \`devflow resume" lacks "--fallback-agent"`. So the row check cannot be satisfied by the flag appearing elsewhere. The file was restored from a scratch copy and `cmp` confirmed it.

## Verification (each exit code captured from the command itself)

| Command | Exit | Result |
|---------|------|--------|
| Task 1 verify: the 5 chained `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::…)'` | 0 | 5/5 passed. Re-run as the tracer gate before Task 2: exit 0 |
| `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(/^doc_check::/)'` | 0 | 14/14 passed |
| `scripts/check.sh all` (fmt, clippy `-D warnings`, `cargo test --workspace --no-fail-fast`) | 0 | `devflow-core` lib 1140 passed and 2 ignored. `devflow` bin 683 passed and 4 ignored. No `test result: FAILED` line |
| `cargo deny --version` | 0 | cargo-deny 0.20.2 |
| `cargo machete --version` | 0 | 0.9.2 |
| `cargo deny check bans licenses sources` | 0 | `bans ok, licenses ok, sources ok`. One `warning[license-not-encountered]`, an unmatched license allowance in `deny.toml` and not a finding |
| `cargo deny check -W vulnerability -W unmaintained -W unsound -W notice -W yanked advisories` | 0 | **It printed no `warning[…]` advisory diagnostic.** The output is the single line `advisories ok`. The advisory DB directory was modified at 19:14:48 local, the time of the run |
| `cargo machete` | 0 | no unused dependencies |
| `scripts/check.sh deps` (not gating, the job CI's `Dependency checks` runs) | 0 | `deps OK`. CI's advisory job should therefore be green on this tree |
| `cargo semver-checks check-release -p devflow-core` | 0 | `229 checks: 229 pass, 31 skip`, `Summary no semver update required`. The baseline is the cached crates.io `devflow-core v3.0.0` (`v3.0.0 -> v3.0.0 (no change; assume patch)`) |
| `scripts/check-in-container.sh` (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`, cpus 0,1) | 0 | `check.sh: all OK`. lib 1140 passed and 2 ignored, bin 683 passed and 4 ignored |

**Advisory scan negative control.** A scratch crate pinned to `smallvec =1.6.0`, with this repository's `deny.toml` copied into it, went through the same `-W` command. It exited 0 and printed `warning[vulnerability]: Buffer overflow in SmallVec::insert_many`. The plain `cargo deny check advisories` exited 1 with `error[vulnerability]`. So the `-W` command does surface findings, and the repository's empty result is a real empty result. My first attempt passed `--config` after `check` and exited 2 with a usage error. I discarded it as the wrong failure and did not count it as a control. The scratch crate has been removed.

**Which Phase 51 tests are new.** There are 78 new test functions against `develop`'s merge base `3720b85`, and none were removed. I counted two ways. First, the difference in standalone `#[test]` lines across `crates/**/*.rs`: 2036 at the base and 2114 at HEAD. Second, the set difference of `#[test] … fn name` pairs: 78 new and 0 gone. The raw diff shows 103 added and 25 removed `#[test]` lines. That difference comes from moved tests and test text inside planted fixtures. Three integration suites are new or extended: `tests/failover_e2e.rs` (new), `tests/fallback_agent_config.rs` (new) and `tests/gate_notify_persist_e2e.rs` (extended).

**What these checks do not establish.**
- The doc pin checks that fragments are present in the right rows. It does not check that the prose is true. I checked prose accuracy by reading the code (below), and no test covers it.
- The remedy after `agent_handoff_relaunch_failed` comes from 51-06's reading of the code. No live run has exercised it, and the doc says so.
- The phase gate does not cover a real provider rate limit (D-13, optional evidence in `51-SUPV01-EVIDENCE.md`) or any behaviour off Linux.
- semver-checks compares against 3.0.0. It reports breaking changes only, so the new `pub` items, such as `config::fallback_agent`, do not register. The version has not been bumped to 3.1.0 in this plan.
- The suite passed once on the host and once in the container. That is not a flakiness measurement.

## Claims checked against the code

- One hop, at Code only, and only while unused and different from the agent: `failover_target` (`pipeline_launch.rs` L3064).
- The full preflight, binary and health checks run at the handoff, and a refusal emits `agent_handoff_refused` and then the cron arm: `hand_off_agent` and `fail_over_on_rate_limit` (L3141-3285).
- `agent_handoff` keys are `stage`, `from_agent`, `to_agent`, `reason` (`rate_limited` or `resume --agent`), `tree_dirty` and `commit_count`, and the last two are null when unmeasured (L3083-3084, L3163-3196).
- A failed relaunch emits `agent_handoff_relaunch_failed`, then `handle_infra_outcome`, which leads to a gate or to an abort at the ceiling (`pipeline_outcomes.rs` L44-77). The gate reason text is `the handoff to {to} could not relaunch the {stage} stage: …`.
- The cron arm writes no file at the infra ceiling (abort) or on an unparseable retry time (gate): `handle_rate_limited_outcome` (`pipeline_outcomes.rs` L89-125).
- `resume --agent` onto the unused fallback is refused unless `--fallback-agent` is given, and `--fallback-agent` goes through `validate_fallback_agent`: `refuse_fallback_for_resume` (L3659).
- Gate approve leads to `Advance`, and a reject without abort leads to `LoopBack`. Both relaunch the same stage: `handle_stage_failure` (`pipeline_outcomes.rs` ~L924-945).
- The idle-timeout clause text is `last result: terminal_reason …; origin …; errors [...]` and `torn final line, a quota denial cannot be ruled out`: `agent_result.rs` L2418-2432 and L2607.
- `fallback_agent` deserializes lowercase agent names (`AgentKind` uses `rename_all = "lowercase"`), and `DevflowConfig` has no `deny_unknown_fields`. So the example key does not break the other `devflow.toml` reader.

## Decisions Made

- The failover text is a new OPERATIONS.md section, not a paragraph in the inventory table. The start row links to it with `#rate-limit-failover`.
- The notify limit sentence is reworded to the fixed behaviour, not removed. An operator who edits the state file needs to know the edit is kept.
- The cron row also states the cron arm's own no-file cases (the ceiling and an unparseable retry time). These existed before this phase. Without them the row would read as "always written when no fallback applies".

## Deviations from Plan

None that change scope. Two notes:
- **Commit type:** Task 1's GREEN commit is `docs(51-11)`. See TDD Gate Compliance.
- **Task 2 made no commit.** Its `files` field names OPERATIONS.md for in-place fmt or doc fixes, and none was needed.

**Total deviations:** 0 auto-fixed. **Impact:** none.

## Issues Encountered

- My first run of the advisory negative control misused `cargo deny`'s `--config` flag and exited 2 with a usage error. I re-ran it with `deny.toml` in the crate root, and that run is the one recorded above.

## Known Stubs

None.

## Threat Flags

None. The change is docs plus one test, and it adds no new surface.

## User Setup Required

None.

## Next Phase Readiness

This is the last plan of Phase 51, and the phase is ready for verification. The version bump to 3.1.0 is not part of this plan.

## Self-Check: PASSED

- FOUND: OPERATIONS.md, README.md, docs/guides/unattended-mode.md, docs/guides/configuration.md, crates/devflow-core/src/doc_check.rs
- FOUND (ancestors of HEAD): `e3cccac`, `1e61089`
- Commits measured from the ledger: `git rev-list --count f96adc1..1e61089` = 2

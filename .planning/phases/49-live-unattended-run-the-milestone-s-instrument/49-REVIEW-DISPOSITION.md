---
phase: 49
review: 49-REVIEW.md
titles: json
findings:
  - id: WR-11
    severity: warning
    disposition: fixed
    title: "OPERATIONS.md says `devflow resume` changes a running phase's hook, but `resume` refuses every live run"
  - id: WR-12
    severity: warning
    disposition: deferred
    title: "The gate's entry save still reverts an on-disk edit, and the gate fires the copy loaded when `advance` started"
  - id: WR-13
    severity: warning
    disposition: deferred
    title: "Nothing fails if `resume` clears the saved command when the variable is unset, and that is the rate-limit cron path"
  - id: IN-10
    severity: info
    disposition: deferred
    title: "The WR-10 re-read silently discards an edit that left the state file unparseable"
  - id: IN-11
    severity: info
    disposition: fixed
    title: "The docs say a rate-limit resume ignores the shell environment, but that resume applies its scheduler's environment"
  - id: CR-03
    severity: critical
    disposition: fixed
    title: "A notify command set at `resume`/`advance` never reaches the detached chain, so unattended gates go silent for runs whose state holds no command"
  - id: WR-10
    severity: warning
    disposition: fixed
    title: "Editing `gate_notify_cmd` while a gate is parked is silently reverted when the gate is answered"
  - id: IN-09
    severity: info
    disposition: fixed
    title: "The new chain-environment e2e leaks the detached chain when it fails"
  - id: WR-07
    severity: warning
    disposition: fixed
    title: "The detached monitor chain freezes the notify command, so an in-flight run's hook cannot be disabled or rotated (regression for the chain)"
  - id: WR-08
    severity: warning
    disposition: fixed
    title: "Two of the four wiring points still fail no test when reverted, contrary to the e2e file's claim"
  - id: WR-09
    severity: warning
    disposition: fixed
    title: "`advance_gate_runs_nothing_without_a_persisted_command` cannot fail"
  - id: IN-06
    severity: info
    disposition: skipped
    title: "The `State::new` purity assertion only fails when the developer's shell exports the variable"
  - id: IN-07
    severity: info
    disposition: fixed
    title: "\"Configured trunk\" wording names a hard-coded constant"
  - id: IN-08
    severity: info
    disposition: fixed
    title: "Non-UTF-8 env values are now run (and persisted) lossily instead of treated as unset"
  - id: CR-01
    severity: critical
    disposition: deferred
    title: "Marker-reason substring heuristic auto-resumes ordinary failures as blocking-human checkpoints"
  - id: CR-02
    severity: critical
    disposition: fixed
    title: "Merged fix commits fail `cargo fmt --check` and block the push gate"
  - id: WR-01
    severity: warning
    disposition: fixed
    title: "Persisted notify command has inconsistent precedence and cannot be disabled for an in-flight run"
  - id: WR-02
    severity: warning
    disposition: fixed
    title: "Nothing fails if the notify-persistence wiring is reverted; the workflow Ship prompt change is untested"
  - id: WR-03
    severity: warning
    disposition: fixed
    title: "Ship prompt hard-codes `main` and a fixture-flavoured branch name for every project"
  - id: WR-04
    severity: warning
    disposition: deferred
    title: "Evidence sanitizer passes AWS keys, Slack webhooks, PEM private keys and JWTs"
  - id: WR-05
    severity: warning
    disposition: deferred
    title: "`launch-attempt.sh start` wedges the attempt and orphans the watcher if the run unit fails to launch"
  - id: WR-06
    severity: warning
    disposition: deferred
    title: "`surv-watch.py analyze` reports interleaving against dead writers' orphaned temp files"
  - id: IN-01
    severity: info
    disposition: deferred
    title: "Merged and stale doc comments around the checkpoint matcher"
  - id: IN-02
    severity: info
    disposition: fixed
    title: "`fire_gate_notify` has no production callers left"
  - id: IN-03
    severity: info
    disposition: deferred
    title: "A kill between `mv` and the ledger append wedges every later snapshot"
  - id: IN-04
    severity: info
    disposition: deferred
    title: "`prepare` pins `$repo/target/debug/devflow` regardless of the configured target dir"
  - id: IN-05
    severity: info
    disposition: deferred
    title: "`collect-attempt.sh` publishes `verdict.json` before the collection directory"
open: 0
total: 27
recorded: 2026-10-09T18:27:12.528Z
---

# Phase 49: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| WR-11 | warning | fixed | OPERATIONS.md: resume changes the hook only for a stopped run; live runs are changed by editing the state file |
| WR-12 | warning | deferred | backlog 999.154; fix rounds stopped after the third review per operator guidance; OPERATIONS.md states the limit |
| WR-13 | warning | deferred | backlog 999.154 (resume arm with the variable unset) |
| IN-10 | info | deferred | backlog 999.154 |
| IN-11 | info | fixed | OPERATIONS.md: a scheduled rate-limit resume is a devflow resume, so the scheduler variable applies |
| CR-03 | critical | fixed | b3f7fdf start/resume/advance apply one rule to the saved command (operator chose the state file as the single source, 2026-10-09); resume and advance e2e arms (not in the current review) |
| WR-10 | warning | fixed | b3f7fdf gate re-reads gate_notify_cmd from disk before its resolution save; in-process test, revert fails it (not in the current review) |
| IN-09 | info | fixed | b3f7fdf MarkedReaper kills marked processes on drop (not in the current review) |
| WR-07 | warning | fixed | 9e78c83 strips DEVFLOW_GATE_NOTIFY_CMD from both monitor spawns, the pipe-owning agent and the canary; e2e reads agent environ via /proc (not in the current review) |
| WR-08 | warning | fixed | 9e78c83 header names what is not covered (pipe __monitor re-exec and its agent spawn, staleness.rs call site); revert controls: legacy/canary/precedence caught (not in the current review) |
| WR-09 | warning | fixed | 9e78c83 replaced with advance_gate_prefers_a_set_variable_over_the_persisted_command (not in the current review) |
| IN-06 | info | skipped | assertion is correct but only discriminates when the developer exports the variable; left as documentation of the contract (not in the current review) |
| IN-07 | info | fixed | 9e78c83 wording: the trunk is config::MAIN, a constant (not in the current review) |
| IN-08 | info | fixed | 9e78c83 non-UTF-8 value counts as unset again, with a resolver test case (not in the current review) |
| CR-01 | critical | deferred | routed to inserted Phase 49.1 (Declared Checkpoint Detection), operator decision 2026-10-09 (not in the current review) |
| CR-02 | critical | fixed | 5574841 style(49): apply rustfmt — cargo fmt --check exit 0 (not in the current review) |
| WR-01 | warning | fixed | af38901 fix(gates): three-state env, start-time capture, one precedence; chain precedence corrected in 9e78c83 (WR-07); part (d), the state file holding a command DevFlow runs, remains by design and is recorded in 49-SECURITY.md (not in the current review) |
| WR-02 | warning | fixed | af38901 tests for resolver, monitor precedence, state round-trip, start/advance e2e; workflow Ship prompt test in c5d7d46 (not in the current review) |
| WR-03 | warning | fixed | c5d7d46 fix(prompt): name the configured trunk in the Ship PR instruction (not in the current review) |
| WR-04 | warning | deferred | backlog 999.152 (phase-49 tooling robustness); committed evidence re-scanned clean 2026-10-09 (not in the current review) |
| WR-05 | warning | deferred | backlog 999.152 (phase-49 tooling robustness); no effect on the three recorded attempts (not in the current review) |
| WR-06 | warning | deferred | backlog 999.152 (phase-49 tooling robustness); false-positive only, all attempts report interleaved_windows=0 (not in the current review) |
| IN-01 | info | deferred | Phase 49.1 replaces the checkpoint matcher these comments describe (not in the current review) |
| IN-02 | info | fixed | af38901 kept for API compatibility, documented as ignoring the persisted command (not in the current review) |
| IN-03 | info | deferred | backlog 999.152 (phase-49 tooling robustness) (not in the current review) |
| IN-04 | info | deferred | backlog 999.152 (phase-49 tooling robustness) (not in the current review) |
| IN-05 | info | deferred | backlog 999.152 (phase-49 tooling robustness); all attempts have collected/DONE (not in the current review) |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.

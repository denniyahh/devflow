---
phase: 49
review: 49-REVIEW.md
titles: json
findings:
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
total: 13
recorded: 2026-10-09T01:02:22.981Z
---

# Phase 49: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | deferred | routed to inserted Phase 49.1 (Declared Checkpoint Detection), operator decision 2026-10-09 |
| CR-02 | critical | fixed | 5574841 style(49): apply rustfmt — cargo fmt --check exit 0 |
| WR-01 | warning | fixed | af38901 fix(gates): three-state env, start-time capture, one precedence |
| WR-02 | warning | fixed | af38901 tests for resolver, monitor precedence, state round-trip, start/advance e2e; workflow Ship prompt test in c5d7d46 |
| WR-03 | warning | fixed | c5d7d46 fix(prompt): name the configured trunk in the Ship PR instruction |
| WR-04 | warning | deferred | backlog 999.152 (phase-49 tooling robustness); committed evidence re-scanned clean 2026-10-09 |
| WR-05 | warning | deferred | backlog 999.152 (phase-49 tooling robustness); no effect on the three recorded attempts |
| WR-06 | warning | deferred | backlog 999.152 (phase-49 tooling robustness); false-positive only, all attempts report interleaved_windows=0 |
| IN-01 | info | deferred | Phase 49.1 replaces the checkpoint matcher these comments describe |
| IN-02 | info | fixed | af38901 kept for API compatibility, documented as ignoring the persisted command |
| IN-03 | info | deferred | backlog 999.152 (phase-49 tooling robustness) |
| IN-04 | info | deferred | backlog 999.152 (phase-49 tooling robustness) |
| IN-05 | info | deferred | backlog 999.152 (phase-49 tooling robustness); all attempts have collected/DONE |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.

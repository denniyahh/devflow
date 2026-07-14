# Phase 13: MVP Core Loop

**Status:** Scoped | **Priority:** HIGH | **Target:** TBD

## Goal

Get the basic AI development loop — `devflow start` → Define→Plan→Code→
Validate→Ship — working end-to-end so the operator can start using DevFlow
on real projects again. Everything in this phase either unblocks the loop's
terminal stage, fixes a correctness hole in the loop's success signal, or
makes unattended daily use viable (never-silent failures, push notification
on gates).

Repurposed 2026-07-14 from the old "OSS Readiness + Hermes Plugin" phase
(content moved wholesale to Phase 15). MVP scope decisions, confirmed with
the operator:

- **Agents:** Claude Code + Codex. Hermes/Antigravity adapters deferred
  (Phase 15).
- **Gates:** answered via a pluggable notify hook (ntfy/desktop) —
  unattended runs, no terminal babysitting. Hermes plugin deferred
  (Phase 15).
- **Ship:** full loop including the automated Ship stage — the unclaimed
  `ship.rs` rewrite is claimed here.

---

## 13a — Ship Stage GSD-Native Rewrite

The loop's terminal stage. Previously flagged in `12-CONTEXT.md` as
**unclaimed by any phase** — `11-VALIDATION.md`'s largest coverage gap
(11h-1 through 11h-4). Claimed here.

- [ ] Rewrite `ship_phase()` against the v2 GSD-native flow: `/gsd-ship` +
      `/gsd-code-review` integration
- [ ] `ReviewFailed` / `AgentFailed` handling in the Ship stage
- [ ] Remove or replace the v1 `LastShip` bookkeeping in `ship.rs` —
      written for the deleted `devflow confirm` / `devflow rejectpr`
      commands
- [ ] Tests for the Ship-stage paths (build on the 12-09 advance()/finish
      test harness)
- [ ] Re-run the Full-Ship manual verification recorded as **BLOCKED** in
      `12-12-SUMMARY.md` — it must pass before this phase closes

## 13b — Completion Protocol Correctness *(moved from 14a/14b)*

The loop's success signal must mean "the work passed," not "the agent
exited."

- [ ] **Verdict vs. ran split (was 14a):** `advance()` /
      `handle_validate_outcome()` treats `DEVFLOW_RESULT: success` from
      Validate as passed → Ship. An agent that runs `/gsd-validate-phase`
      and *finds gaps* succeeded at its task while validation failed. Add
      a distinct verdict field (e.g. `"verdict": "pass|gaps"`) OR evaluate
      Validate from the validation report artifact rather than the agent's
      self-report.
- [ ] Commit-count fallback (Layer 2) scoped to Code-like stages only —
      Define and Validate legitimately produce zero commits.
- [ ] **Native envelope parsing (was 14b):** parse the Claude JSON
      envelope (`is_error`, `result`, `num_turns` — already requested via
      `--output-format json`, never parsed) and the Codex `--json` event
      stream as the authoritative per-adapter signals. `DEVFLOW_RESULT`
      marker stays as portable fallback.
- [ ] Tests: Validate-with-gaps does not advance to Ship; zero-commit
      Define/Validate runs don't trip Layer 2; envelope success/error per
      agent; fallback on absent/malformed envelope.

## 13c — Never-Silent Loop *(moved from 14d)*

Unattended runs must always surface a halt or a gate — silence is the
failure mode that makes dogfooding impossible.

- [ ] **WR-11 (Phase 11 code review):** `advance()`'s catch-all arm for
      non-Validate stage failures (`main.rs:360-374`) returns an error but
      fires no gate and sends no notification — state left dirty with
      `gate_pending: false`, so nothing ever surfaces the halt. Route
      these through the same gate + notify path.
- [ ] Pluggable notify hook fired on gate-write (arbitrary shell command;
      operator will wire it to ntfy/desktop notification).
- [ ] `GATE_TIMEOUT_SECS` (`main.rs:16`, hardcoded 7 days) becomes
      configurable.
- [ ] Tests: stage failure fires gate + hook; hook failure is fail-soft
      (never blocks the loop).

## 13d — Unattended-Safety Default *(moved from 14c)*

- [ ] Flip the default: `devflow start` uses a worktree unless an explicit
      opt-out flag is passed — the current default runs a
      `--dangerously-skip-permissions` agent directly in the primary
      checkout.
- [ ] Update CLI help text for the new default (full README rewrite stays
      in Phase 15); confirm worktree cleanup/`recover` paths handle the
      now-default case.

## 13e — MVP Acceptance: Dogfood Run

The phase's exit criterion is not "tasks complete" but "the loop ran."

- [ ] Full end-to-end run (Define→Plan→Code→Validate→Ship) on a real
      external project with the Claude adapter, gates answered via the
      notify hook
- [ ] Same loop exercised with the Codex adapter (at minimum through
      Code→Validate; confirm envelope parsing against the real
      `--json` stream)
- [ ] Any failure during dogfooding is in-scope for this phase — the loop
      working is the deliverable

---

## Explicitly Out of Scope (this phase)

- Hermes + Antigravity agent adapters, Hermes plugin — Phase 15
- README/ARCHITECTURE rewrite, `.devflow.yaml` decoy removal, IN-01
  rustdoc, `--help` snapshot CI test, dev container, CONTRIBUTING/CoC —
  Phase 15
- `devflow logs [--follow]`, `events.jsonl`, `devflow status` enrichment,
  `capture_agent_output()` sync-path decision — remain in Phase 14
- `devflow.toml` / configurable pipeline — still shelved per 2026-07-08
  decision
- crates.io publish — publish-prep done in Phase 12; actual publish
  belongs with Phase 15 OSS readiness

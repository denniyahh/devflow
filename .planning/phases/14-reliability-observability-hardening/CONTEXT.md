# Phase 14: Observability Hardening

**Status:** Scoped | **Priority:** MEDIUM | **Target:** TBD

## Goal

Give a running loop visibility instead of a black box between launch and
exit, and settle the remaining agent-execution architecture question
(`capture_agent_output()` sync path).

Source: external code review of the codebase (2026-07-08). All claims below
were independently verified against `main.rs`, `stage.rs`, `prompt.rs`,
`agents/claude.rs`, and `config.rs` before being scoped here.

**Rescoped 2026-07-14 (MVP restructure):** the reliability/correctness
items originally scoped here — verdict-vs-ran split (14a), native envelope
parsing (14b), worktree-by-default (14c), and from 14d the gate notify
hook, configurable `GATE_TIMEOUT_SECS`, and WR-11 silent-halt fix — moved
to **Phase 13 (MVP Core Loop)** because the core loop isn't usable
unattended without them. What remains here is pure observability plus one
deferred architecture decision.

`devflow.toml` / configurable pipeline (branch model, stage/agent command
templates) is explicitly **out of scope** for this phase — shelved for a
future phase pending a deliberate decision to reverse the Phase 11 "config
eliminated" call.

---

## 14a — Observability

- [ ] `devflow logs [--follow]` — tail the existing capture file
      (stdout/stderr) for the running/most-recent agent.
- [ ] Append-only `.devflow/events.jsonl` — one line per state
      transition, gate fire/response/ack, and hook run. Makes any future
      frontend (TUI, Hermes plugin, web) a reader instead of requiring a
      new integration. (The gate notify hook itself ships in Phase 13;
      events.jsonl should record its firings.)
- [ ] `devflow status` shows more than stage + PID where practical (last
      known action, elapsed time).

## 14b — capture_agent_output() Sync-Path Decision

Previously flagged in `12-CONTEXT.md` as **unclaimed by any phase**;
claimed here.

- [ ] Decide: should `sequentagent` keep a synchronous
      `capture_agent_output()` path (11i-5, still public/in use), or move
      behind monitor-owned execution like everything else?
- [ ] Implement the decision; remove the dead path if sequentagent moves
      to the monitor.

## Explicitly Out of Scope (this phase)

- Verdict-vs-ran split, native envelope parsing, worktree default, gate
  notify hook, configurable gate timeout, WR-11 — moved to Phase 13
  (MVP Core Loop), 2026-07-14.
- `devflow.toml` / configurable stage-agent pipeline, branch model,
  verify/lint command config — shelved for a future phase; requires a
  deliberate re-decision on the Phase 11 "config eliminated" call.
- Publishing to crates.io — publish-prep done in Phase 12; actual publish
  belongs with Phase 15 OSS readiness.
- ARCHITECTURE.md rewrite, `.devflow.yaml` decoy removal, `--help`
  snapshot CI test — routed to Phase 15 (OSS Readiness; same class of
  doc-accuracy work).

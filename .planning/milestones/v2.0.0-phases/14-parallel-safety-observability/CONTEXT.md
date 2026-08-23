# Phase 14: Parallel Safety + Observability

**Status:** Scoped | **Priority:** HIGH | **Target:** TBD

## Goal

Make concurrent phases safe by construction — fix the deferred CR-03 design
flaw (per-phase state files, phase-threaded monitor advance, coarse lock for
main-checkout mutations) — settle the remaining agent-execution architecture
question (`capture_agent_output()` sync path), and give a running loop
visibility instead of a black box between launch and exit.

Source: `13-DEFERRED-CR-03.md` (post-fix review, 2026-07-15) for the
parallel-safety work; external code review of the codebase (2026-07-08) for
the observability items.

**Rescoped 2026-07-16 (Phase 14 split):** all Hermes work (14c HermesAgent
adapter, 14d skill-file rewrite, 14e Hermes plugin) moved to **Phase 16
(Hermes Support)**. The 2026-07-14 move of Hermes into this phase was a
workload-balance call made *before* CR-03 was deferred here (2026-07-15),
which made this the heaviest phase instead of the slimmest. Hermes is
personal-infrastructure and should not gate parallel-safety correctness; its
gate watcher still consumes this phase's `events.jsonl` and Phase 13's notify
hook, so Phase 16 depends on this phase.

**Plan ordering (dependency-driven):** 14a (CR-03) → 14b → 14c. CR-03's
per-phase state files reshape what `status`/`recover`/`logs` enumerate, and
the `events.jsonl` schema must carry phase identity from day one — so
correctness lands first and observability is built once, on the final state
model, not rebuilt after it.

`devflow.toml` / configurable pipeline (branch model, stage/agent command
templates) is explicitly **out of scope** for this phase — shelved for a
future phase pending a deliberate decision to reverse the Phase 11 "config
eliminated" call.

---

## 14a — CR-03: Parallel Safety *(deferred from Phase 13)*

Fix shape and acceptance criteria in
`phases/13-mvp-core-loop/13-DEFERRED-CR-03.md` — read it first. Summary:
per-phase locks (CR-03 in `13-REVIEW.md`) promise independent sibling phases,
but the resources they mutate stayed project-global, so `devflow parallel` is
unsafe by construction.

- [ ] Per-phase state files: `state-{phase:02}.json` mirroring the lock
      naming; `workflow::load_state/save_state/clear_state` take the phase; a
      listing helper enumerates active phases for `status`/`recover`.
      One-shot migration read of legacy `state.json`.
- [ ] Thread the phase through the monitor: `devflow advance {root}
      --phase N` recorded at spawn time, so advance's identity never depends
      on a shared singleton (removes the pre-lock state read entirely).
- [ ] Short project-wide lock for main-checkout mutations only: wrap
      `finish_workflow`'s git-op section (and any other primary-checkout
      mutation) in a second, coarse lock held for seconds, not gate-days.
- [ ] Re-check `sequentagent` (takes no lock today) and
      `cron-instructions.json` (project-global single slot) against the
      per-phase model (overlaps 14b).
- [ ] Acceptance: interleaved-fake-agent integration test — two `parallel`
      phases each run start→advance→gate with no shared-file clobbering;
      concurrent `finish_workflow`s serialize on the coarse lock;
      `status`/`recover` enumerate all active phases.

## 14b — capture_agent_output() Sync-Path Decision

Previously flagged in `12-CONTEXT.md` as **unclaimed by any phase**;
claimed here. Decide alongside 14a's `sequentagent` re-check — same code
path, one design pass.

- [ ] Decide: should `sequentagent` keep a synchronous
      `capture_agent_output()` path (11i-5, still public/in use), or move
      behind monitor-owned execution like everything else?
- [ ] Implement the decision; remove the dead path if sequentagent moves
      to the monitor.

## 14c — Observability

- [ ] `devflow logs [--follow]` — tail the existing capture file
      (stdout/stderr) for the running/most-recent agent; phase-aware
      (defaults to the single active phase, `--phase N` to disambiguate).
- [ ] Append-only `.devflow/events.jsonl` — one line per state
      transition, gate fire/response/ack, and hook run, **each carrying the
      phase id**. Makes any future frontend (TUI, Hermes plugin, web) a
      reader instead of requiring a new integration. (The gate notify hook
      itself shipped in Phase 13; events.jsonl should record its firings.)
- [ ] `devflow status` shows more than stage + PID where practical (last
      known action, elapsed time), across all active phases per 14a's
      listing helper.

## Explicitly Out of Scope (this phase)

- HermesAgent adapter, Hermes skill-file rewrite, Hermes plugin — moved to
  Phase 16 (Hermes Support) on 2026-07-16.
- Antigravity agent adapter — stays in Phase 15 with the rest of the OSS
  work.
- Verdict-vs-ran split, native envelope parsing (Claude/Codex), worktree
  default, gate notify hook, configurable gate timeout, WR-11 — shipped in
  Phase 13 (MVP Core Loop).
- `devflow.toml` / configurable stage-agent pipeline, branch model,
  verify/lint command config — shelved for a future phase; requires a
  deliberate re-decision on the Phase 11 "config eliminated" call.
- Publishing to crates.io — publish-prep done in Phase 12; actual publish
  belongs with Phase 15 OSS readiness.
- ARCHITECTURE.md rewrite, `.devflow.yaml` decoy removal, `--help`
  snapshot CI test — routed to Phase 15 (OSS Readiness; same class of
  doc-accuracy work).

## Moved to Phase 16 (2026-07-16)

- `HermesAgent` adapter (was 14c)
- Hermes skill-file rewrite (was 14d)
- Hermes plugin session mode (was 14e)

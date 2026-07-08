# Phase 14: Reliability & Observability Hardening

**Status:** Scoped | **Priority:** HIGH | **Target:** TBD

## Goal

Close the gap between "the agent exited" and "the agent's work actually
passed," give a running loop visibility instead of a black box between
launch and exit, and remove defaults that make unattended runs riskier than
they need to be (silent multi-day gate timeout, no notification, full
permission bypass with worktree isolation opt-in).

Source: external code review of the codebase (2026-07-08). All claims below
were independently verified against `main.rs`, `stage.rs`, `prompt.rs`,
`agents/claude.rs`, and `config.rs` before being scoped here.

`devflow.toml` / configurable pipeline (branch model, stage/agent command
templates) is explicitly **out of scope** for this phase — shelved for a
future phase pending a deliberate decision to reverse the Phase 11 "config
eliminated" call.

---

## 14a — Verdict vs. Ran Split in Completion Protocol

- [ ] `advance()` / `handle_validate_outcome()` (`main.rs`) currently treats
      a `DEVFLOW_RESULT: success` marker from the Validate stage as
      `passed = true` → transitions to Ship. An agent that successfully
      *runs* `/gsd-validate-phase` and *finds gaps* has succeeded at its
      task while validation itself failed — this is not currently
      distinguishable.
- [ ] Add a distinct verdict field to the completion protocol (e.g.
      `"verdict": "pass|gaps"`), separate from `status`, OR evaluate
      Validate specifically from the validation report artifact rather
      than the agent's self-report.
- [ ] Commit-count fallback (Layer 2: "exit 0 + zero commits = no work
      done") currently applies uniformly across stages. Define and
      Validate legitimately produce zero commits in normal operation —
      confirm the fallback is scoped to Code-like stages only, or gated
      off for stages where zero commits is expected.
- [ ] Tests: Validate-with-gaps does not advance to Ship; Define/Validate
      zero-commit runs do not falsely trip the Layer-2 fallback.

## 14b — Native Structured Output as Primary Signal

- [ ] Claude is already invoked with `--output-format json`
      (`agents/claude.rs`) but the envelope (`is_error`, `result`,
      `num_turns`) isn't parsed — only the bespoke `DEVFLOW_RESULT` marker
      is read, and the skill docs already note agents forget to emit it.
- [ ] Parse each agent's native completion envelope as the authoritative
      per-adapter signal (Claude JSON envelope; Codex `--json` event
      stream if/when exercised), keeping `DEVFLOW_RESULT` as the portable
      fallback when a native envelope isn't available.
- [ ] Tests: native envelope parsing covers success/error cases per agent;
      fallback path still works when envelope is absent/malformed.

## 14c — Worktree Isolation Default

- [ ] `agents/claude.rs::exec_command` always appends
      `--dangerously-skip-permissions`. `--worktree` on `Start` is a
      separate opt-in bool (`main.rs`) — default `start` behavior runs an
      unattended, fully-permissioned agent directly in the primary
      checkout.
- [ ] Flip the default: `devflow start` uses a worktree unless an explicit
      opt-out flag is passed.
- [ ] Update README/CLI help text for the new default; confirm existing
      worktree cleanup/`recover` paths handle the now-default case without
      change.

## 14d — Observability

- [ ] `devflow logs [--follow]` — tail the existing capture file
      (stdout/stderr) for the running/most-recent agent.
- [ ] Append-only `.devflow/events.jsonl` — one line per state
      transition, gate fire/response/ack, and hook run. Makes any future
      frontend (TUI, Hermes plugin, web) a reader instead of requiring a
      new integration.
- [ ] Pluggable notify hook fired on gate-write (arbitrary shell command,
      e.g. wired to `ntfy`/Slack/desktop notification). `GATE_TIMEOUT_SECS`
      (`main.rs:16`, hardcoded to 7 days) becomes configurable — silent
      week-long blocking with no notification channel is the current
      default.
- [ ] `devflow status` shows more than stage + PID where practical (last
      known action, elapsed time).
- [ ] **(WR-11, Phase 11 code review)** `advance()`'s catch-all arm for
      non-Validate stage failures (`main.rs:360-374`, Define/Plan/Code/Ship)
      returns an error but fires no gate and sends no notification — state
      is left dirty with `gate_pending: false`, so nothing (not even the
      7-day gate timeout) will surface the halt. Same failure class as the
      rest of 14d: route these through the same notify hook so a stuck
      pipeline is never silent. Confirmed still present in current code
      (2026-07-08).

## Explicitly Out of Scope (this phase)

- `devflow.toml` / configurable stage-agent pipeline, branch model,
  verify/lint command config — shelved for a future phase; requires a
  deliberate re-decision on the Phase 11 "config eliminated" call.
- Publishing to crates.io — belongs in Phase 12 (Bootstrap + Housekeeping).
- ARCHITECTURE.md rewrite, `.devflow.yaml` decoy removal, `--help`
  snapshot CI test — routed to Phase 13 (already covers the README
  rewrite under 13b; same class of doc-accuracy work).

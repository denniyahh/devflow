---
phase: 13-mvp-core-loop
plan: "06"
subsystem: dogfood-verification
tags: [dogfood, e2e, claude, codex, gates, notify, worktree, ship]
requirements: [13a, 13b, 13c, 13d, 13e]
key-files:
  created:
    - .planning/phases/13-mvp-core-loop/13-06-SUMMARY.md
  modified:
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/lock.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-core/src/agents/codex.rs
    - crates/devflow-core/src/agent.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-cli/src/main.rs
metrics:
  tasks: 3
  duration: "2026-07-14 → 2026-07-15 (spanned a Claude session-limit reset)"
  dogfood_fixes_shipped: 6
---

# 13-06 — MVP Core-Loop Dogfood: Summary

Dogfood target: `denniyahh/devflow-dogfood` (private throwaway repo, real remote,
git-flow branches). Binary under test: `target/debug/devflow` built from this
phase's commits. Operator: Dennis; orchestration: Claude Code (operator-directed).

## Task 1 — Pre-flight: PASS

- `cargo test --workspace` green (operator-confirmed + independently run), clippy
  `-D warnings` clean, `cargo fmt --check` clean, `devflow doctor` exit 0
  (git 2.55.0, gh 2.96.0, claude 2.1.209, codex 0.144.4).
- Ship prompt sequencing verified in `prompt.rs`: `/gsd-code-review {N}` before
  `/gsd-ship {N}`, no-ship on Critical findings, `review:` reason contract.
- **Mandatory WR-11/13c round-trip (REAL, operator-observed):** fake failing
  `claude` on PATH → `devflow start --phase 1 --mode auto` on a scratch repo →
  Claude envelope `{"is_error":true,"num_turns":1}` parsed (13-03) → gate
  `01-define.json` written with `[never-silent] stage define failed: … (num_turns: 1)`
  (13-01) → `fire_gate_notify` ran the operator's
  `notify-send -u critical` command with `DEVFLOW_GATE_PHASE/STAGE/CONTEXT` and
  `DEVFLOW_NON_SILENT_GATE=1` → **desktop notification observed by operator**.
  Worktree `.worktrees/phase-01` created by default (13-04).
- Lesson recorded: default-urgency notifications expire in ~5s and were missed;
  `-u critical` (or an ntfy push) is the recommended notify command for
  unattended runs.

## Task 2 — Claude full loop + Full-Ship re-verification (12-12 unblock): PASS

- Full Define→Plan→Code→Validate→Ship on a real project with real `claude`.
- Define/Plan/Code/Validate all completed unattended. Live 13-05 evidence:
  Validate advanced only on `verdict: pass`; auto mode correctly did not gate
  at Validate.
- **ReviewFailed path exercised on a real bug:** Ship's `/gsd-code-review` found
  a genuine Critical (CR-01: `std::env::args()` panics on non-UTF-8 argv),
  refused to ship, reported `DEVFLOW_RESULT: failed, reason: "review: CR-01 …"`,
  `handle_ship_failure` looped back to Code without a gate (by design), Code
  fixed F-01/F-02/F-03, Validate re-passed, Ship re-ran clean.
- **Unforced Ship AgentFailed:** the operator's Claude session limit killed Ship
  mid-run (41 turns). Never-silent gate + notification fired with the exact
  reason ("You've hit your session limit · resets 11:30pm"). Retried after
  reset via gate response — this is stronger AgentFailed evidence than a staged
  failure.
- **Full-Ship verification (BLOCKED in 12-12): PASS** — `/gsd-ship` pushed and
  opened a real PR headless with no interactive stall (check 5b):
  https://github.com/denniyahh/devflow-dogfood/pull/1 (+1495/−20, merged by
  operator decision at the "Ship complete — approve merge?" gate).
  Note: DevFlow deliberately never merges — "approve merge?" records the
  human decision; the merge itself is done by the human/`gh`.

## Task 3 — Codex leg through Code→Validate + real `--json` parsing: PASS

- `devflow start --phase 2 --agent codex --mode supervise` (deviation: plan
  said `--mode auto`; supervise chosen to cap the run at the Validate gate —
  Task 3 needs only Code→Validate and Ship was already proven on the Claude
  leg).
- Deviation: Phase 2 planning artifacts (CONTEXT.md, 02-01-PLAN.md) were
  pre-baked by the operator; Define/Plan passed via the new idempotent prompt
  contract rather than live Codex discussions (see finding E1).
- Code stage: real Codex implemented `--loud`, committed atomically from
  inside `--sandbox workspace-write` (after fixes B4/B5 below):
  `af644c3 feat(02-01): add loud alias for shout`. GSD's verifier then
  demanded better test coverage twice (regression-coverage gap), closed via a
  pre-baked gap plan `02-02` →
  `26ebbdd test(02-02): prove loud alias through production parser`.
- Validate stage: ran `/gsd-validate-phase 2`, emitted
  `DEVFLOW_RESULT: {"status": "success", "verdict": "pass"}` — verdict split
  honored by a real Codex agent; supervise mode gated
  ("Validation passed — approve to ship?") instead of advancing. Codex leg
  ended intentionally at that gate (abort response; work preserved on
  `feature/phase-02`).

### Verbatim Codex `--json` capture (key lines)

```jsonl
{"type":"item.completed","item":{"id":"item_7","type":"agent_message","text":"DEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"Phase 2 already has CONTEXT.md and plans; the required update/view/skip decision needs user input, but interactive input is unavailable in this execution mode.\"}"}}
{"type":"turn.completed","usage":{"input_tokens":234467,"cached_input_tokens":204800,"output_tokens":2024,"reasoning_output_tokens":679}}
{"type":"item.completed","item":{"id":"item_29","type":"agent_message","text":"DEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"Phase 2 implementation and all tests passed, but required atomic commits were blocked because the linked worktree Git metadata is read-only (index.lock creation failed).\"}"}}
{"type":"turn.completed","usage":{"input_tokens":1688450,"cached_input_tokens":1595392,"output_tokens":6336,"reasoning_output_tokens":2388}}
DEVFLOW_RESULT: {"status": "success", "verdict": "pass"}   ← Validate, via agent_message
{"type":"turn.completed","usage":{"input_tokens":319217,"cached_input_tokens":275968,"output_tokens":4318,"reasoning_output_tokens":1199}}
```

**Parser delta found and reconciled (the empirically-safe practice from
12-12):** Codex delivers `DEVFLOW_RESULT` inside `agent_message` items — never
as a raw stdout line — and terminal turns are `turn.completed` even for
self-reported failures. Plan 03's parser missed both; fixed and regression-
tested (B2 below), then verified against the live stream.

## Failure classification (Cursor 13-06 requirement)

### DevFlow bugs — found live, fixed, tested, re-verified live

| # | Finding | Fix |
|---|---------|-----|
| B1 | `git tag` blocked on `$EDITOR` under global `tag.gpgsign=true` (VersionBump hook hung a finished run) | `09e2803` — `-c tag.gpgSign=false` scoped per invocation |
| B2 | Codex `agent_message` DEVFLOW_RESULT invisible to parser; a self-reported failure with exit 0 classified success | `27033bc` — marker extraction from agent_message events |
| B3 | Rate-limit heuristic scanned JSONL and false-matched doc text ("Rate limiting per key?"), stuffing a multi-KB line into the gate/notification; no reason cap | `27033bc` — skip JSON lines; 300-char reason cap |
| B4 | Codex sandbox blocked linked-worktree commits: worktree admin dir (`.git/worktrees/<n>`) read-only even with common `.git` granted | `6403c6a` — both writable roots (verified with `codex sandbox` probes) |
| B5 | Signed commits fail in-sandbox (no route to ssh/gpg agent) | `6403c6a` — `GIT_CONFIG_*` env scopes `commit/tag.gpgsign=false` to the agent process tree only |
| B6 | Stale `.devflow/lock` from a dead holder wedged every later `advance`, silently (also explains one "advance vanished" anomaly) | `f3951bf` — liveness-check + reclaim |

Hardening shipped alongside: `09f96ff` — idempotent Define/Plan prompts
(pre-existing artifact ⇒ no-op success) + `start --agent codex` pre-flight
error when the phase has no CONTEXT.md on develop.

### External-workflow issues (not DevFlow)

| # | Finding | Disposition |
|---|---------|-------------|
| E1 | GSD discuss-phase on Codex demands interactive input both fresh and over existing CONTEXT.md; `yolo: true` not honored for artifact decisions (`request_user_input is unavailable in Default mode`) | Upstream GSD Codex-port issue. DevFlow mitigations: idempotent prompts + fresh-codex pre-flight (`09f96ff`). Codex runs now require pre-existing context — documented prerequisite |
| E2 | Claude session limit killed Ship mid-run | Account quota, not code. Never-silent gate handled it correctly |
| E3 | GSD gap-closure protocol requires a gap plan before re-execution (Code retries alone spin) | GSD by design; worked immediately once a gap plan existed |

### Residual gaps / follow-ups

- One `advance` invocation (10:03) left no trace; strongest hypothesis is
  pre-fix lock contention, unprovable because advance's output died with its
  terminal → direct motivation for Phase 14 observability
  (`logs`/`events.jsonl`).
- Fresh headless Codex discussions remain unsupported until GSD's Codex port
  honors autonomous mode (file upstream).
- `devflow-dogfood` repo + PR #1 can be deleted at will; `v0.0.3` tag was never
  created (probe interrupted) — cosmetic only.

## Self-Check: PASSED

- All three checkpoint tasks verified by the operator or against live systems
- `cargo test --workspace` green after every fix (208 tests at close), clippy
  `-D warnings` clean, fmt clean
- All six fixes re-verified against the live dogfood run after landing

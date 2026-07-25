---
phase: 23
slug: end-to-end-dogfood
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-25
---

# Phase 23 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `23-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` harness — no separate framework or assertion crate |
| **Config file** | none — CI behavior driven by `.github/workflows/ci.yml` (three jobs: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`) |
| **Quick run command** | `cargo test -p devflow-core <filter>` (core) / `cargo test -p devflow <filter>` (CLI) |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~541 tests across 13 binaries; full workspace suite measured green at research time (2026-07-25) |

> **Package-name trap:** the CLI package is `devflow`, **not** `devflow-cli`.
> `cargo test --exact <name>` with a name that matches nothing still exits 0 —
> assert on the `N passed` count, never on the exit code alone.

---

## Sampling Rate

- **After every task commit:** Run the targeted module filter for the module just touched —
  `cargo test -p devflow-core -- <module>` or `cargo test -p devflow -- <module>`
- **After every plan wave:** Run `cargo test --workspace`, plus
  `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` (matches CI exactly)
- **Before `/gsd-verify-work`:** Full suite green **AND** the 23a scratch-repo probe artifact recorded (D-03)
  **AND** the D-02 self-hosted acceptance run completed
- **Max feedback latency:** targeted filter < 30s; full workspace suite is the wave-level gate

---

## Per-Task Verification Map

Task IDs are assigned by the planner; this table maps the phase's **units** to their
verification contract. `/gsd-validate-phase` fills the Task ID / Plan / Wave columns
once plans exist.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | 23a — probe records exact failure point in a scratch repo | — | N/A | manual/behavioral | N/A — the probe **is** the test; assert on `events.jsonl` + `.devflow/phase-N-*` captures per D-03 | N/A | ⬜ pending |
| TBD | TBD | TBD | 23b — GONE/STALE/ALIVE liveness with no PID; whole-tree teardown incl. severed-ppid orphans; takeover safety | T-23-01 | Socket mode `0600` — only the owning user may connect and issue `shutdown`/`ping` (R-L) | unit + integration | `cargo test -p devflow-core -- monitor::` | ❌ W0 — current tests exercise the shell-script monitor and must be **rewritten**, not extended | ⬜ pending |
| TBD | TBD | TBD | 23b — `supervisor` field round-trips through serde; absent field defaults correctly | — | N/A | unit | `cargo test -p devflow-core -- state::tests` | ❌ W0 — follow `monitor_pid_round_trips_through_serde` / `monitor_pid_absent_from_json_defaults_to_none` (`state.rs:312-345`) | ⬜ pending |
| TBD | TBD | TBD | 23b/D-10 — natural agent exit triggers `advance` with **no forked subprocess** | — | N/A | integration | `crates/devflow-core/tests/monitor_e2e.rs` (new/replaced cases) | ⚠️ Partial — file covers the OLD mechanism | ⬜ pending |
| TBD | TBD | TBD | 23b — a **STALE** socket renders as a distinct actionable state, never folded into `GONE`/`Unknown` | — | N/A | unit + integration | `cargo test -p devflow-core -- monitor::` / `cargo test -p devflow -- status doctor` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | 23c — `devflow stop` suppresses `advance` (R-M); idempotent on already-stopped/dead phase; preserves pre-existing `stop_reason` | T-23-02 | Anyone who can connect to the socket can stop the phase — mode `0600` is the control | unit + integration | `cargo test -p devflow -- stop` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | 23d — CLI no longer accepts `sequentagent`; help snapshot regenerated; zero dangling references | — | N/A | integration (regression guard) | `cargo test -p devflow -- help_snapshot` **and** `rg -c sequentagent crates/` returns 0 | ✅ existing guard; committed snapshot needs regeneration | ⬜ pending |
| TBD | TBD | TBD | `--yes-ship` — gate fires, auto-responds, records `responded_by`; **only** the primary Ship gate, not the finalization-retry gate | T-23-03 | D-06: auto-**answer**, never bypass — an approval must appear in `events.jsonl` and the gate ledger | unit + integration | `cargo test -p devflow -- pipeline_outcomes` | ⚠️ Partial — Ship-gate scaffolding exists near `pipeline_outcomes.rs:1514-1576` | ⬜ pending |
| TBD | TBD | TBD | `--yes-ship` D-05 — **not** settable from `devflow.toml` or env | T-23-04 | A standing unattended auto-merge must never become the silent default | unit | new test asserting no config/env path sets `yes_ship` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | 23b upgrade path — a phase whose `state.json` predates the `supervisor` field is not silently unreachable | — | N/A | unit + `doctor` finding | `cargo test -p devflow-core -- state::tests` / `cargo test -p devflow -- doctor` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Rewrite `crates/devflow-core/src/monitor.rs` `#[cfg(test)] mod tests` against the socket supervisor —
      the existing cases exercise the `sh -c` script and cannot be extended past D-08's big-bang replacement
- [ ] `crates/devflow-core/src/state.rs` — round-trip + absent-field-defaults tests for the new `supervisor` block,
      mirroring `monitor_pid_round_trips_through_serde` / `monitor_pid_absent_from_json_defaults_to_none` (`state.rs:312-345`)
- [ ] `crates/devflow-core/tests/monitor_e2e.rs` — new fake-agent cases for the rewritten mechanism
      (fast, deterministic; **not** a substitute for the real unattended run)
- [ ] New `devflow stop` test module (23c), including the R-M advance-suppression case
- [ ] New `--yes-ship` cases in `pipeline_outcomes` tests, including the negative case that the
      **finalization-retry** gate is *not* auto-approved
- [ ] New not-config-persistable test for `--yes-ship` (D-05)
- [ ] Regenerate the committed help snapshot (`devflow-help.txt`) after the `sequentagent` deletion

*Framework install: not required — `cargo test` is built in.*

*Any new test that shells out to git MUST use `devflow_core::test_support::{git_command, hermetic_command}`
behind the off-by-default `test-support` feature — never a bare `Command::new` (999.37).*

---

## Manual-Only Verifications

The phase's actual acceptance criterion is **behavioural** and lives entirely outside `cargo test`.
`monitor_e2e.rs` uses a fake `sh`/echo agent, not Claude — it proves the mechanism, never the criterion.

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| 23a probe — record exactly where `devflow start` dies | 23a / D-01, D-03 | Requires a real Claude Code invocation consuming real GSD slash commands over real wall-clock time | Run `devflow start --phase N` in a **scratch repo** (D-01, blast-radius isolation) with a **rebuilt ≥v1.8.1 binary**. Capture `events.jsonl` excerpts and the `.devflow/phase-N-*` captures. The artifact, not a verbal report, is the deliverable (D-03). |
| Final acceptance — one phase driven Define→completed Ship, unattended | Phase goal / D-02, D-07 | Self-hosted (this repo) so the self-dogfood staleness `Block` path (`staleness.rs:276-284`) is actually exercised — it is structurally **unreachable** in a scratch repo | Run **only after** the scratch probe has proven the supervisor. Drive a **low-stakes** phase and establish a **recovery point (tag or branch) before starting** — D-07 mitigations, operator-specified. The run performs a real merge to `develop`, a real version bump and a real changelog commit. |
| Stall-vs-pause observability under a live run | Phase goal (Finding 1) | The distinction only exists under a real multi-stage run with real inter-stage gaps | Poll `devflow status` / `.devflow/events.jsonl` **every 30–60s** — short enough to catch a stage transition, long enough not to interfere. |

### Evidence a run must capture to count as validated (D-03 — not optional)

- `events.jsonl` excerpts spanning **every** stage transition: `transition`, `stage_launched`,
  `gate_fired`, `gate_resolved`, `workflow_finished`
- `.devflow/phase-N-*` capture files (`stdout`, `stderr.log`, `exit`, `agent-pid`) for at least the
  stage where the run succeeds or first fails

A run that "seems to have worked" without this evidence trail **does not** satisfy the phase's
behavioural acceptance criterion.

### Precondition — rebuild before any probe

The installed `devflow` on PATH was measured at **v1.8.0** (built 2026-07-23) while the workspace
`Cargo.toml` reads **1.8.1**. 23a requires ≥v1.8.1. Rebuild and re-verify `devflow --version`
before either run; validating a dogfood fix against a stale binary is a known repeat failure here.

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s for targeted filters
- [ ] Behavioural acceptance evidence captured per D-03 (23a probe **and** D-02 acceptance run)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

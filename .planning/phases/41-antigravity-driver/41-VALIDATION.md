---
phase: 41
slug: antigravity-driver
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-19
revised: 2026-08-20 (adversarial review rework — verify commands corrected per review findings 5/9; see .planning/reviews/phase-41/SUMMARY.md)
---

# Phase 41 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Reworked 2026-08-20: every `<automated>` verify now matches real tests. Rules:
> (a) no trailing `::` — libtest does substring matching and `name::` never matches;
> (b) integration tests use `--test phase7_cli`, NOT `--bin devflow` (the bin target never
> reaches `crates/devflow-cli/tests/`); (c) assert a REAL `1 passed` with non-zero
> `filtered out` — 0 passed with exit 0 is a FAIL (CLAUDE.md heuristic defeated when the
> target is wrong).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo test (existing) |
| **Config file** | none — Wave 0 uses existing infrastructure |
| **Quick run command** | `cargo test -p devflow-core --lib antigravity -- --nocapture && cargo test -p devflow --test phase7_cli antigravity -- --nocapture` |
| **Full suite command** | `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` verify (see Per-Task Verification Map) and assert `1 passed` with non-zero `filtered out`
- **After every plan wave:** Run `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli`
- **Before `$gsd-verify-work`:** Full suite must be green (bin unit tests AND `--test phase7_cli` integration tests)
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 41-01-01 | 01 | 1 | ANTG-03 | T-41-01/02 | antigravity stream-json parser + evaluate_layer1 wiring (D-03) | unit | `cargo test -p devflow-core --lib antigravity_event -- --nocapture` | ✅ agent_result.rs | ⬜ pending |
| 41-01-02 | 01 | 1 | ANTG-02 | T-41-04 | agent-aware first-turn writer + stream-launch routing (PipeOwning) | unit | `cargo test -p devflow-core --lib user_turn_line -- --nocapture && cargo test -p devflow --bin devflow stream_launch -- --nocapture` | ✅ monitor.rs / pipeline_launch.rs | ⬜ pending |
| 41-01-03 | 01 | 1 | ANTG-02 | T-41-01 | AntigravityDriver (argv: no `-p`, no skip-permissions; parse_completion delegate) | unit | `cargo test -p devflow-core --lib antigravity_driver -- --nocapture` | ✅ antigravity.rs (new) | ⬜ pending |
| 41-01-04 | 01 | 1 | ANTG-01 | — | AgentKind variant + dispatch + conformance enrollment (4→5) | unit + integration | `cargo test -p devflow-core --lib agent_kind -- --nocapture && cargo test -p devflow-core --test agent_kind_antigravity -- --nocapture && cargo test -p devflow-core --lib conformance -- --nocapture` | ✅ state.rs / mod.rs / agent_kind_antigravity.rs (new) | ⬜ pending |
| 41-01-05 | 01 | 1 | ANTG-01 | T-41-05 | `devflow doctor` antigravity/`agy` entry (commands.rs) | unit | `cargo test -p devflow --bin devflow doctor -- --nocapture` | ✅ commands.rs | ⬜ pending |
| 41-01-06 | 01 | 1 | ANTG-03 | T-41-02/06 | marker-less gates at commit-gated Plan + happy path (antigravity-shaped events) + discrimination control | integration | `cargo test -p devflow --test phase7_cli antigravity -- --nocapture` | ✅ phase7_cli.rs | ⬜ pending |
| 41-02-01 | 02 | 2 | HYG-01 | T-41-HYG-01/04 | per-PID monitor reap guard + negative control (no ps-count) | integration | `cargo test -p devflow --test phase7_cli -- --nocapture` | ✅ phase7_cli.rs | ⬜ pending |
| 41-02-02 | 02 | 2 | HYG-02 | T-41-HYG-02/03 | check-in-container.sh worktree-aware mount (re-derived: NOT the 3 test files, NOT skip_if_root) | integration | `bash scripts/check-in-container.sh all` (from the worktree AND from the main checkout) | ✅ check-in-container.sh | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/agents/antigravity.rs` — AntigravityDriver impl (argv without `-p`) + unit tests
- [ ] `crates/devflow-core/src/agent_result.rs` — `is_antigravity_event_stream` + `parse_antigravity_event_result` + `evaluate_layer1` wiring (D-03 — the pre-review plan had NO implementing task)
- [ ] `crates/devflow-core/src/monitor.rs` — `user_turn_line_for` (`{"event":"user",...}` for Antigravity)
- [ ] `crates/devflow-cli/src/pipeline_launch.rs` — stream-launch predicate includes Antigravity → `PipeOwning`
- [ ] `crates/devflow-cli/src/commands.rs` — `devflow doctor` entry for `agy`
- [ ] `crates/devflow-core/tests/agent_kind_antigravity.rs` — FromStr/Display/driver_for tests
- [ ] `crates/devflow-cli/tests/phase7_cli.rs` — marker-less regression (gate at Plan, `wait_for_gate`) + happy path + discrimination control; NOT `#[ignore]`
- [ ] HYG-01 per-PID monitor reap guard in Phase-7 suite + negative control
- [ ] HYG-02 `check-in-container.sh` fixed for the worktree case, re-derived from container runs (main checkout = negative control)

*Wave 0 covers all MISSING references from the review: the parser, the transport, the doctor entry, and the corrected verify commands.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `agy` wrapper argv (no duplicate `--dangerously-skip-permissions`) | ANTG-02 | D-01 wrapper injects the flag; a duplicate is a silent-correctness risk | run `antigravity-cli --help` (NOT `agy -p --help` — `-p` is a string flag that consumes the next token as a prompt and invokes the model); confirm the driver build_command omits both `-p` and the flag |
| `devflow doctor` reports Antigravity installed | ANTG-01 | presence-only check (D-04) against live PATH | run `devflow doctor` with `agy` on PATH |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

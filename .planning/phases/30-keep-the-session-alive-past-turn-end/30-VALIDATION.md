---
phase: 30
slug: keep-the-session-alive-past-turn-end
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: true
created: 2026-08-02
updated: 2026-08-02
---

# Phase 30 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace; package `devflow` for the CLI, `devflow-core` for core — **never** `devflow-cli`, which is the directory name, not the package name) |
| **Config file** | Cargo.toml (workspace root) — no Wave 0 install needed. `tempfile = "3"` is already a dev-dependency of `devflow-core` (`crates/devflow-core/Cargo.toml:29`) |
| **Quick run command** | `cargo test -p devflow-core --lib agent_result::` |
| **Full suite command** | `scripts/check.sh all` (= `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`) — the repository's single definition of green, per that script's own header |
| **Measured baseline** | `89 passed; 0 failed; 366 filtered out` in `agent_result::`, **0.27s test time / 0.39s wall** (measured 2026-08-02, pre-Phase-30). This is the before-count plan 30-01 Task 2 asks the executor to compare against. |
| **Estimated runtime** | Quick command: < 1s after a warm compile. `scripts/check.sh all`: minutes (clippy `--all-targets` dominates). |

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core --lib agent_result::`
- **After every plan wave:** `scripts/check.sh all`
- **Before `/gsd-verify-work`:** full suite green, AND the 30c/30d evidence artifacts present on disk
- **Max feedback latency:** < 1 second for the scoped command

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Unit | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|------|------------|-----------------|-----------|-------------------|-------------|--------|
| 30-01-01 | 01 | 1 | 30b | T-30-01 / T-30-02 | Last-result selection reads only top-level JSONL objects; an agent-authored `origin`-shaped structure inside `result` text is inert | unit (end-to-end through `evaluate_layer1`) | `cargo test -p devflow-core --lib agent_result::tests::evaluate_layer1_parses_claude_stream_capture -- --exact` | ❌ W0 | ⬜ pending |
| 30-01-02 | 01 | 1 | 30b | T-30-02 / T-30-03 | The stream gate cannot consume a single-document envelope or a Codex stream | unit (isolation) | `cargo test -p devflow-core --lib agent_result::` then `scripts/check.sh all` | ❌ W0 | ⬜ pending |
| 30-02-01 | 02 | 1 | 30c | T-30-08 | Env-scrub list parsed from live `git.rs`; aborts rather than running an empty scrub | script parse + inspection | `python3 -c "import ast; ast.parse(open('.../30c-monitor-env-harness.py').read())"` | ❌ W0 | ⬜ pending |
| 30-02-02 | 02 | 1 | 30c | T-30-06 / T-30-09 | No home paths or usernames in committed evidence; verdict derived from raw JSONL, not console output | manual-only (experiment) — see below | `test -s .../30c-evidence/raw_output.jsonl && rg -q '^delivery: (confirmed\|refuted)$' .../30c-VERDICT.md` | ❌ W0 | ⬜ pending |
| 30-02-03 | 02 | 1 | 30c | T-30-09 | Operator independently recounts before the verdict gates Phase 31 | checkpoint:human-verify (blocking) | human | n/a | ⬜ pending |
| 30-03-01 | 03 | 2 | 30b | T-30-12 / T-30-13 / T-30-15 | Rate-limit outranks marker and envelope-failure; `rate_limit_info` read via direct `.get()` | unit | `cargo test -p devflow-core --lib agent_result::tests::claude_stream` | ❌ W0 | ⬜ pending |
| 30-03-02 | 03 | 2 | 30b | T-30-11 / T-30-14 | A `session_id` planted in agent-authored marker text is never returned; no `session_id` field added to `AgentResult` | unit (regression) | `cargo test -p devflow-core --lib agent_result::tests::claude_stream_session_id -- --exact` | ❌ W0 | ⬜ pending |
| 30-04-01 | 04 | 2 | 30d | T-30-17 / T-30-19 | Children reaped on every exit path; monotonic clock for intervals | script parse + inspection | `python3 -c "import ast; ast.parse(open('.../30d-exit-timing-harness.py').read())"` | ❌ W0 | ⬜ pending |
| 30-04-02 | 04 | 2 | 30d | T-30-16 / T-30-18 | Aggregates recomputable from archived per-trial files; no paths or usernames | manual-only (experiment) — see below | `rg -q '^mode_b_outcome: ' .../30d-MEASUREMENTS.md` | ❌ W0 | ⬜ pending |
| 30-05-01 | 05 | 3 | 30b | T-30-21 / T-30-22 / T-30-23 | Gate scan excludes `user`/`system` events and non-top-level events; no `json_scan` traversal | unit | `cargo test -p devflow-core --lib agent_result::tests::blocking_human` | ❌ W0 | ⬜ pending |
| 30-05-02 | 05 | 3 | 30b | T-30-21 / T-30-24 / T-30-25 | Prompt echo does not read as a live gate; a real declaration co-occurring with an echo still does | unit (regression cluster) | `cargo test -p devflow-core --lib agent_result::tests::blocking_human` then `scripts/check.sh all` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Sampling continuity:** no three consecutive tasks lack an automated verify. The two
experiment plans (30-02, 30-04) have script-parse automation on their build tasks and
artifact-existence automation on their run tasks; their *interpretive* content is
manual by nature and is listed under Manual-Only Verifications below.

*Note: `cargo test --exact` with a bare test name matches nothing and still exits 0 —
verification must assert on the reported `N passed` count, never on exit code alone.
This is a standing trap in this repository, reconfirmed in 30-RESEARCH.md Pitfall 4.*

---

## Wave 0 Requirements

- [x] **Fixture strategy decided at plan time — no Wave 0 task needed.** Fixtures are
      inline `concat!` string literals inside `agent_result.rs`'s existing `mod tests`,
      copied verbatim from `30a-evidence/raw_output_v3.jsonl` with a doc comment citing
      the source line numbers. **`include_str!` reaching into `.planning/` is forbidden:**
      `devflow-core` is published to crates.io, `cargo package` builds from a tarball
      containing only files under the crate root, and neither crate sets
      `package.include`/`exclude` — so a cross-boundary path would fail at the next
      real release and never reproduce locally (30-RESEARCH.md Pitfall 5).
- [x] Test module exists — `agent_result.rs`'s `mod tests` is extended, no new test file.
- [x] `tempfile` dev-dependency present for the `evaluate_layer1` file-level tests.

**Fixture honesty constraint (30-05):** no archived capture contains checkpoint gate
text, so 30-05's fixtures are *real event envelopes with synthetic text payloads* and
each test's doc comment must say so. They may not be labelled "real capture".

---

## Manual-Only Verifications

| Behavior | Unit | Why Manual | Test Instructions |
|----------|------|------------|-------------------|
| Does `task-notification` delivery survive DevFlow's real launch environment? | 30c | It is a live-CLI experiment against undocumented, unpinned upstream behavior. No mock validates the delivery premise — a mocked CLI would validate plumbing, which review constraint H4 explicitly rejects as a substitute. | Run `30c-monitor-env-harness.py`, then read `30c-evidence/raw_output.jsonl` directly and count `result` events carrying `origin.kind == "task-notification"`. Compare against the v3 interactive baseline (3 result events, the latter two task-notification-origin). Record in `30c-VERDICT.md`. |
| Operator sign-off on the verdict before it gates Phase 31 | 30c | A cancel-or-proceed decision on an M-sized phase. | Plan 30-02 Task 3, `checkpoint:human-verify`, `gate="blocking"`. The operator independently recounts from the raw JSONL before approving. |
| Exit latency distribution after stdin close | 30d | Wall-clock measurement of a live process; not expressible as a unit test. | Run `30d-exit-timing-harness.py` Mode A, ≥5 iterations; aggregates must recompute from the archived per-trial timings. |
| Close-with-pending-background-tasks behavior | 30d | Currently undefined upstream behavior with no expected outcome — the observation *is* the deliverable. | Run Mode B, ≥2 trials. Any of hang / clean exit / truncated result / lost child work is a valid finding. Disagreement across trials must be recorded as `nondeterministic`. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or a documented manual-only rationale
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (fixture strategy resolved at plan time)
- [x] No watch-mode flags
- [x] Feedback latency < 1s for the scoped command (measured 0.39s wall)
- [ ] `nyquist_compliant: true` — set by `/gsd-validate-phase` after execution, not here

**Approval:** pending — this contract is seeded by plan-phase and is validated after execution.

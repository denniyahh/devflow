---
phase: 30
slug: keep-the-session-alive-past-turn-end
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-02
updated: 2026-08-02
validated_at: "2026-08-02T23:50:00Z"
validated_by: "manual re-execution of every mapped command; no nyquist auditor spawned"
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
| 30-01-01 | 01 | 1 | 30b | T-30-01 / T-30-02 | Last-result selection reads only top-level JSONL objects; an agent-authored `origin`-shaped structure inside `result` text is inert | unit (end-to-end through `evaluate_layer1`) | `cargo test -p devflow-core --lib agent_result::tests::evaluate_layer1_parses_claude_stream_capture -- --exact` | ✅ | ✅ green |
| 30-01-02 | 01 | 1 | 30b | T-30-02 / T-30-03 | The stream gate cannot consume a single-document envelope or a Codex stream | unit (isolation) | `cargo test -p devflow-core --lib agent_result::` then `scripts/check.sh all` | ✅ | ✅ green |
| 30-02-01 | 02 | 1 | 30c | T-30-08 | Env-scrub list parsed from live `git.rs`; aborts rather than running an empty scrub | script parse + inspection | `python3 -c "import ast; ast.parse(open('.../30c-monitor-env-harness.py').read())"` | ✅ | ✅ green |
| 30-02-02 | 02 | 1 | 30c | T-30-06 / T-30-09 | No home paths or usernames in committed evidence; verdict derived from raw JSONL, not console output | manual-only (experiment) — see below | `test -s .../30c-evidence/raw_output.jsonl && rg -q '^delivery: (confirmed\|refuted)$' .../30c-VERDICT.md` | ✅ | ✅ green |
| 30-02-03 | 02 | 1 | 30c | T-30-09 | Operator independently recounts before the verdict gates Phase 31 | checkpoint:human-verify (blocking) | human | n/a | ✅ approved |
| 30-03-01 | 03 | 2 | 30b | T-30-12 / T-30-13 / T-30-15 | Rate-limit outranks marker and envelope-failure; `rate_limit_info` read via direct `.get()` | unit | `cargo test -p devflow-core --lib agent_result::tests::claude_stream` | ✅ | ✅ green |
| 30-03-02 | 03 | 2 | 30b | T-30-11 / T-30-14 | A `session_id` planted in agent-authored marker text is never returned; no `session_id` field added to `AgentResult` | unit (regression) | `cargo test -p devflow-core --lib agent_result::tests::claude_stream_session_id_` **(corrected at validation — see note A)** | ✅ | ✅ green |
| 30-04-01 | 04 | 2 | 30d | T-30-17 / T-30-19 | Children reaped on every exit path; monotonic clock for intervals | script parse + inspection | `python3 -c "import ast; ast.parse(open('.../30d-exit-timing-harness.py').read())"` | ✅ | ✅ green |
| 30-04-02 | 04 | 2 | 30d | T-30-16 / T-30-18 | Aggregates recomputable from archived per-trial files; no paths or usernames | manual-only (experiment) — see below | `rg -q '^mode_b_summary: ' .../30d-MEASUREMENTS.md` **(corrected at validation — see note B)** | ✅ | ✅ green |
| 30-05-01 | 05 | 3 | 30b | T-30-21 / T-30-22 / T-30-23 | Gate scan excludes `user`/`system` events and non-top-level events; no `json_scan` traversal | unit | `cargo test -p devflow-core --lib agent_result::tests::blocking_human` | ✅ | ✅ green |
| 30-05-02 | 05 | 3 | 30b | T-30-21 / T-30-24 / T-30-25 | Prompt echo does not read as a live gate; a real declaration co-occurring with an echo still does | unit (regression cluster) | `cargo test -p devflow-core --lib agent_result::tests::blocking_human` then `scripts/check.sh all` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Validation run — 2026-08-02, every command re-executed

Each row above was filled by **running its command and reading the reported count**, never
its exit code. Measured results:

| Task | Command result |
|---|---|
| 30-01-01 | `1 passed; 0 failed; 484 filtered out` |
| 30-01-02 | `agent_result::` 119 passed; `scripts/check.sh all` exit 0; container parity `485 passed`, 22/22 result lines `0 failed` |
| 30-02-01 | `ast.parse` exit 0 |
| 30-02-02 | evidence non-empty AND `delivery: confirmed` matched |
| 30-02-03 | operator-approved on 8 trials across 3 environments (recorded in `30-02-SUMMARY.md`) |
| 30-03-01 | `16 passed; 0 failed; 469 filtered out` |
| 30-03-02 | `4 passed; 0 failed; 481 filtered out` **after correction** |
| 30-04-01 | `ast.parse` exit 0 |
| 30-04-02 | `mode_b_summary: exits_cleanly` matched **after correction** |
| 30-05-01 | `16 passed; 0 failed; 469 filtered out` |
| 30-05-02 | as 30-05-01, plus `scripts/check.sh all` exit 0 |

**Note A — 30-03-02's seeded command matched ZERO tests and still exited 0.** The literal
`agent_result::tests::claude_stream_session_id -- --exact` reports
`0 passed; 0 failed; 485 filtered out`. This is the standing `--exact` trap named in this
very document, caught here by asserting on the count. Plan 30-03 had already found and
documented it (`30-03-SUMMARY.md`: *"matches ZERO tests (proven by running it), because a
test of that exact name would shadow the glob-imported function under test"*) but the map
row was never updated. Corrected to the `claude_stream_session_id_` prefix, which matches
the four real tests. **No coverage was missing — only the recorded command was wrong.**

**Note B — 30-04-02 checked a field that was deliberately renamed.** `mode_b_outcome` does
not exist in `30d-MEASUREMENTS.md`. Cross-AI review finding M2 required Mode B's result not
be collapsed into a single token, so 30-04 replaced it with eleven independent per-trial
fields plus a secondary `mode_b_summary`. The substance is **stronger** than the seeded
check demanded; the check itself was stale. Corrected to `mode_b_summary`.

Both notes are the same class of drift: a verification command written at plan time, made
obsolete by a deliberate change during execution, and never re-run until validation. Both
would have read as green to anyone checking exit codes.

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
- [x] `nyquist_compliant: true` — set 2026-08-02 after execution. All 11 mapped rows verified
      by re-running their commands and reading reported counts; two stale commands corrected
      (notes A and B). Sampling continuity holds: no three consecutive tasks lack an
      automated verify.

**Approval:** validated 2026-08-02.

### What this validation does NOT establish

Recorded because `nyquist_compliant: true` is easy to over-read as "the feature is proven":

- **Sampling adequacy, not production correctness.** Nyquist compliance says the phase was
  sampled densely enough during execution. It says nothing about whether the code works in
  production — and phase 30's stream parser is **unreachable in production** until Phase 31
  flips the launch path off `--output-format json`.
- **No fixture is a real capture.** No archived capture contains checkpoint gate text, and
  none contains a prompt echo at all. Every gate assertion uses a real event envelope with a
  synthetic payload. The prompt-echo false positive is closed as *reasoned*, not *witnessed*.
- **The suite did not catch what the code review did.** All rows were green *before* the
  cross-AI review found two High defects (`30-CODE-REVIEW.md`); one was a live fail-open,
  fixed in `06675da`. Green rows here did not, and do not, imply defect-free. Two confirmed
  defects remain open behind `evaluate_layer1` as ROADMAP constraint 9.
- **No nyquist auditor was spawned.** Validation was done by re-executing the mapped
  commands directly. This repo has previously had that agent write non-compiling tests on a
  mid-arc phase; running the commands is the cheaper and more honest check.

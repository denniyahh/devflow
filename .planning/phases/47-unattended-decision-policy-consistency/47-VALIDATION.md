---
phase: "47"
slug: "unattended-decision-policy-consistency"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-10"
---

# Phase 47 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
>
> **Not to be confused with `47-PHASE49-OBSERVATION.md`.** That file carries D-09/D-10's
> *behavioural* evidence standard for Phase 49's live run — what would count as the agent having
> followed or ignored the gate rule. This file is the Nyquist test-coverage map for executing
> Phase 47 itself. D-09 originally named `47-VALIDATION.md` for the memo; it was renamed so this
> path keeps its GSD-conventional meaning and `/gsd-validate-phase` finds a test map here.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | built-in `cargo test`, plus `insta` 1.48.0 — **not yet present; Wave 0 (47-01) installs it** |
| **Config file** | none for cargo; `scripts/check.sh` is the single definition of green (`scripts/check.sh:1-9`) |
| **Quick run command** | `cargo test -p devflow-core --lib prompt::` |
| **Full suite command** | `scripts/check.sh test` → `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast` after 47-01 |
| **Estimated runtime** | **not measured.** A cold build in the pinned container has run 2–9 min on this repo; the warm scoped `-p devflow-core --lib` run has not been timed for this phase. Measure at 47-01 rather than trusting a figure invented here. |

**`-p devflow --bin devflow`, never `-p devflow --lib`.** `devflow` is binary-only, so `--lib` exits
non-zero with `error: no library targets found` before running a single test. `-p devflow-core --lib`
*is* valid — that crate has a lib target.

---

## Sampling Rate

- **After every task commit:** the scoped `cargo test -p <crate>` for the crate that task touched.
- **After every plan wave:** `scripts/check.sh test`.
- **Before `/gsd-verify-work`:** full suite green.
- **Phase gate:** `scripts/check.sh all` green — also what `scripts/hooks/pre-push:217` runs.
- **Max feedback latency:** governed by the scoped run above; unmeasured (see Test Infrastructure).

---

## Per-Task Verification Map

Every task below carries a runnable `<automated>` block with a bound `<fails_when>` in its own plan;
the command column names the plan that owns the exact text rather than duplicating ~900-character
shell blocks here. All 14 are `bash -c '…'` (executor subagents run zsh, where `${PIPESTATUS[0]}`
expands to empty).

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 47-01-01 | 01 | 1 | DECN-02, DECN-03 | T-47-01 | Snapshot guard cannot be disabled by env | integration | `47-01-PLAN.md` Task 1 `<verify>` | ❌ W0 (installs insta) | ⬜ pending |
| 47-01-02 | 01 | 1 | DECN-02, DECN-03 | T-47-01 | Guard demonstrated failing-then-fixed (3-case control) | integration | `47-01-PLAN.md` Task 2 `<verify>` | ❌ W0 | ⬜ pending |
| 47-01-03 | 01 | 1 | DECN-02 | T-47-03 | New dev-dep leaves `cargo deny` / `machete` green on the REAL workspace | integration | `47-01-PLAN.md` Task 3 `<verify>` | ❌ W0 | ⬜ pending |
| 47-02-01 | 02 | 2 | DECN-02 | T-47-04 | Policy present on `FullExecute` across all six adapters; `GapsOnly`/`AuditFix` omit it | unit | `47-02-PLAN.md` Task 1 `<verify>` | ❌ W0 | ⬜ pending |
| 47-02-02 | 02 | 2 | DECN-02 | T-47-07 | Shared arm helper; `workflow_code_prompt` behaviour preserved | unit | `47-02-PLAN.md` Task 2 `<verify>` | ✅ `prompt.rs:454-474` | ⬜ pending |
| 47-02-03 | 02 | 2 | DECN-02 | T-47-05 | Omission control discriminates; `prompt.rs:793-822` unmodified | unit | `47-02-PLAN.md` Task 3 `<verify>` | ✅ `prompt.rs:793-822` | ⬜ pending |
| 47-03-01 | 03 | 3 | DECN-03 | T-47-06 | `resume_launch_shape` is production code, audit emission unmoved | unit | `47-03-PLAN.md` Task 1 `<verify>` | ✅ analog `pipeline_launch.rs:195` | ⬜ pending |
| 47-03-02 | 03 | 3 | DECN-03 | T-47-06 | Two-turn delivery pair asserts the rule in both turns | unit | `47-03-PLAN.md` Task 2 `<verify>` | ❌ W0 | ⬜ pending |
| 47-03-03 | 03 | 3 | DECN-03 | T-47-09 | One definition site; carve-out scoped to `blocking-human` only (D-02) | unit | `47-03-PLAN.md` Task 3 `<verify>` | ❌ W0 | ⬜ pending |
| 47-04-01 | 04 | 4 | DECN-02, DECN-03 | T-47-07 | Named baselines for the six policy-carrying prompts | unit | `47-04-PLAN.md` Task 1 `<verify>` | ❌ W0 | ⬜ pending |
| 47-04-02 | 04 | 4 | DECN-03 | T-47-07 | The two delivered turns snapshot distinctly | unit | `47-04-PLAN.md` Task 2 `<verify>` | ❌ W0 | ⬜ pending |
| 47-05-01 | 05 | 4 | DECN-03 | — | `unattended-mode.md`'s false "refusal is final" claim corrected | docs | `47-05-PLAN.md` Task 1 `<verify>` | ✅ `docs/guides/unattended-mode.md` | ⬜ pending |
| 47-05-02 | 05 | 4 | DECN-03 | — | Phase 49 observation memo + single-line ROADMAP pointer | docs | `47-05-PLAN.md` Task 2 `<verify>` | ❌ W0 | ⬜ pending |
| 47-05-03 | 05 | 4 | DECN-02, DECN-03 | — | ROADMAP/REQUIREMENTS name the real adapter split; 999.125/126 filed; declined follow-on absent | docs | `47-05-PLAN.md` Task 3 `<verify>` | ✅ `.planning/ROADMAP.md` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Sampling continuity:** no three consecutive tasks lack an automated verify — all 14 have one.

---

## Wave 0 Requirements

Wave 0 is plan **47-01** in its entirety, which is why it is its own plan at wave 1 rather than
folded into the fix work (operator decision, 2026-09-10). Baselines laid behind an unproven guard
would be worthless, so the guard must be demonstrably failing-then-fixed first.

- [ ] `insta` in `[workspace.dependencies]` and in both crate manifests under `[dev-dependencies]`
- [ ] `run_test` hardened to `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no …` (RESEARCH § A-0, § A-3)
- [ ] `*.snap.new` ignored in `.gitignore`, with `*.snap` explicitly kept tracked
- [ ] Initial `.snap` baselines created via `INSTA_UPDATE=always` and **read before committing**
- [ ] `resume_launch_shape` extracted (RESEARCH § B-0 step 1) — 47-03 Task 1

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Whether a model actually *follows* the carve-out when both rules are present | DECN-03 | Criterion 4 is explicitly **not closed by this phase** — it is a question about model instruction-priority, not answerable by reading or testing source | Phase 49's live `devflow start --mode auto` run, judged against the evidence standard in `47-PHASE49-OBSERVATION.md` (D-10). Claude adapter only — the resume route is gated on `AgentKind::Claude` (`pipeline_launch.rs:1569`), so the other three claude-style adapters cannot reach it (D-11) |
| That the `.snap` diff was READ before a baseline was blessed | DECN-02 | No machine can distinguish a read diff from an unread one; the automated gates can only prove a baseline changed, not that a human looked | 47-02 Task 2 requires the SUMMARY to carry the pre-re-bless failing output and an explicit statement that the diff was read (D-15) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies — 14/14
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [ ] Feedback latency measured — **outstanding**, deliberately not guessed (see Test Infrastructure)
- [ ] `nyquist_compliant: true` set in frontmatter — set by `/gsd-validate-phase`, not here

**Approval:** pending

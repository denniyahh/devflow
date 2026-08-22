---
phase: 34
slug: stream-json-coverage-and-the-validate-trust-boundary-999-73
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-05
---

# Phase 34 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase` from `34-RESEARCH.md` § "Validation Architecture".
> The Per-Task Verification Map is deliberately unfilled at plan time — it is populated once
> PLAN.md task IDs exist.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (standard Rust harness), workspace crates `devflow-core` and `devflow` |
| **Config file** | none dedicated — `scripts/check.sh` is the single definition of "green": `fmt`, `clippy --all-targets -- -D warnings`, `test` |
| **Quick run command** | `cargo test -p devflow-core --lib <module>::` or `cargo test -p devflow --lib <module>::` |
| **Full suite command** | `scripts/check.sh all` (host) / `scripts/check-in-container.sh all` (pinned CI image) |
| **Estimated runtime** | **unmeasured** — measure at Wave 0 and record here rather than assume |

**Package-name trap (CLAUDE.md, and it bites specifically here).** devflow-core's package is
`devflow-core`; **devflow-cli's package is `devflow`**, not `devflow-cli`
(`crates/devflow-cli/Cargo.toml:2`). `cargo test --exact <name>` **exits 0 when the name matches
nothing** — assert on a real `N passed` line with a non-zero `filtered out` count. Never trust the
exit code alone, and never trust a pipeline's exit code (it is the last command's).

---

## Sampling Rate

- **After every task commit:** targeted `cargo test -p <package> --lib <module>::` for the module touched
- **After every plan wave:** `scripts/check.sh all` (fmt + clippy + full suite)
- **Before `/gsd-verify-work`:** full suite green **and** the criteria 1/2 live-capture evidence
  committed as artifacts — `cargo test` does not reach those at all, so they need explicit manual
  sign-off in the phase's verification pass
- **Max feedback latency:** unmeasured — see Test Infrastructure

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| *(pending — filled once PLAN.md task IDs exist)* | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Requirement → verification, ahead of task IDs

| Req / Criterion | Behavior | Test Type | Command or artifact | Exists? |
|---|---|---|---|---|
| DOGFOOD-03 · c1 | Every widened stage backed by a real production capture; every un-widened stage carries a recorded reason | **live-run evidence** | manual capture run per RESEARCH § "Capture Acquisition Mechanics" | ❌ W0 (artifact, not a test) |
| DOGFOOD-03 · c2 | Capture answers what happens when the close rule does **not** fire, per stage | **live-run evidence** | same capture, inspected against `BackgroundTaskState` | ❌ W0 (artifact) |
| DOGFOOD-03 · c7 canary | `canary_gate_only_applies_to_the_stream_launch_path` rebuilt on a discriminator that survives full widening | unit | `cargo test -p devflow --lib pipeline_launch::tests::canary_gate_only_applies_to_the_stream_launch_path -- --exact` | ✅ exists, needs rebuild |
| DOGFOOD-03 · c7 retention | Retention eviction cannot silently drop an unread capture | unit | new test over the chosen mitigation (constant change or copy-at-landing) | ❌ W0 |
| DOGFOOD-04 · c3 | Exhaustive `(layer0, status, verdict)` match — 42 cells + named controls | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::` | ❌ W0 |
| DOGFOOD-04 · c4 | `reconcile_layer0_verdict` consults **Layer 1's** status before grafting its verdict | unit | extend `agent_result::tests::layer0_affirmative_success_consults_layer1_verdict_at_validate` with the `{"status":"failed","verdict":"pass"}` case | ✅ exists, extend |
| DOGFOOD-04 · c4 e2e | Pre-fix reaches Ship / post-fix gates, with both negative controls | integration (tempdir-backed, same crate) | new `devflow-core` test per RESEARCH § "The Smallest In-Repo Test Harness" | ❌ W0 |
| DOGFOOD-04 · c5 | `idle_timeout_result`'s `verdict: None` comment confirmed **live**, left unedited | diff review | assert the phase diff contains no edit to that comment | N/A |
| 999.76 · c6 | Layer 0 discovers from the **execution root**; worktree discovery distinguishable from main-checkout discovery | unit | companion fixture beside `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` (`agent_result.rs:5269`) | ❌ W0 |
| 999.76 · c6 site 2 | `phase_has_blocking_human_checkpoint` fixed in the same change | unit | new test in `devflow-core`'s `verify` module | ❌ W0 |

---

## Mandatory Negative Controls

This repo's stated discipline: **every measurement includes a case that must produce the opposite
result. If both cases agree, the measurement is broken — not the subject.** These are not optional
and each is individually named, because an unnamed control is one nobody notices went missing.

| # | Measurement | Required opposite-result case | Source |
|---|---|---|---|
| NC-1 | `(_, Success, Some(Pass)) → Passed` | positive control — must stay `Passed` after the rewrite | D-08 |
| NC-2 | `(true, Success, Some(Gaps)) → Ambiguous` | paired mirror `(false, Success, Some(Gaps)) → Failed` | D-08 |
| NC-3 | `(true, Success, None) → Ambiguous` | paired mirror `(false, Success, None) → Failed` | D-08 |
| NC-4 | Exhaustiveness is structural | deleting a status arm → **E0004**; adding an 8th `AgentStatus` variant → **E0004** | D-06 |
| NC-5 | Graft demonstration reaches Ship pre-fix | verdict **removed** → gates; verdict set to `gaps` → gates | D-15 |
| NC-6 | Graft demonstration reaches Ship pre-fix | **Layer 0 disabled** → `decide_action` intercepts | D-15 |
| NC-7 | Layer 0 discovers from execution root | `git ls-tree -r develop --name-only -- .planning/phases \| grep -c '/34-'` → 0 vs. same against `HEAD` → non-zero. **Use `-r`** — the non-recursive form returns 0 for every ref and proves nothing | 999.76 |
| NC-8 | Per-stage drain measurement | a stage showing `Pending(n>0)` **refutes** the vacuous-drain assumption for that stage rather than confirming it — the capture is framed as a refutation test | D-09 |

**Why NC-2/NC-3 pairing is load-bearing, not decorative.** `decided_by_layer` is
`#[serde(default)]` and its doc reserves `None` for fixtures, so the natural 21-cell sweep leaves
`external` false in every cell — **both `Ambiguous` arms go unexercised and a regression deleting
them both is green.** NC-1 does not catch it either: `(Success, Pass) → Passed` is
layer-independent. The `layer0` dimension is what makes the sweep 42 cells.

---

## What the evidence does NOT establish

Carried as a standing obligation on the phase summary, per D-10:

- **n=1 per stage** establishes that the shape occurred **once** — not that it is the stage's steady
  behaviour across prompts, phase shapes, or CLI versions. Phase 30 needed n=2–3 trials before its
  drain measurements meant anything (`30c` reliability trials, `30d` exit-timing).
- The launch **argv is stage-blind** (`ClaudeAgent::exec_command` ignores `_phase`, `_prompt`,
  `_extra_writable_roots`). A per-stage capture is evidence about **agent behaviour under that
  stage's prompt**, never about the transport.
- D-15's demonstration establishes the graft is reachable; it does **not** establish that a real
  agent emits a self-contradictory marker in practice — no parser cross-checks `status` against
  `verdict`.

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/agent_result.rs` — extend `layer0_affirmative_success_consults_layer1_verdict_at_validate` with the `{"status":"failed","verdict":"pass"}` case (D-15)
- [ ] `crates/devflow-core/src/agent_result.rs` — worktree-vs-main-checkout companion fixture for `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` (999.76)
- [ ] `crates/devflow-core/src/verify.rs` — test covering `phase_has_blocking_human_checkpoint` reading the execution root (999.76 second call site)
- [ ] `crates/devflow-cli/src/pipeline_outcomes.rs` — the 42-cell D-08 sweep plus NC-1/NC-2/NC-3
- [ ] `crates/devflow-cli/src/pipeline_launch.rs` — rebuild `canary_gate_only_applies_to_the_stream_launch_path` on the legacy-opt-out discriminator
- [ ] `.planning/phases/34-…/34-evidence/` — stub the Phase-30 evidence-directory layout (raw / scrubbed / operator split) **before** the live capture run, so the copy-at-landing step has a landing spot
- [ ] Measure and record the full-suite runtime in Test Infrastructure

*Framework install: none — `cargo test` is already configured in this workspace.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Per-stage stream-json capture exists and is real | DOGFOOD-03 c1 | Requires a live agent run through the pipe-owning monitor; no unit test can produce a real CLI stream | Run per RESEARCH § "Capture Acquisition Mechanics" — `--no-worktree` + rebuilt binary; copy the capture out of `.devflow/` (gitignored `*`) into the phase evidence directory |
| Drain behaviour per widened stage | DOGFOOD-03 c2 | Inspection of the capture's `background_tasks_changed` arity and drain timing relative to the marker | For each widened stage, name the observed `BackgroundTaskState` and what a recurrence costs on the next run |
| `idle_timeout_result` comment left unedited | DOGFOOD-04 c5 | A "no change was made" property is a diff review, not a test | Confirm the phase diff contains no edit to `agent_result.rs:1746-1750`'s comment |
| Un-widened stages carry a recorded reason | DOGFOOD-03 c1 | Prose in a doc comment; no assertion can judge whether a reason is genuine | Read the `STREAM_JSON_STAGES` doc comment; every stage absent from the list has a stated reason |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or a Wave 0 dependency
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all ❌ references above
- [ ] No watch-mode flags
- [ ] Every measurement in "Mandatory Negative Controls" has its opposite-result case present and passing
- [ ] Full-suite runtime measured and recorded (not assumed)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

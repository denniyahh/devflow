# Phase 35: Loop-Termination and Baseline Correctness - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-06
**Phase:** 35-loop-termination-and-baseline-correctness-999-77-999-78-999-79-999-84-999-86
**Areas offered:** Counter surgery (999.77 + 999.78), Staleness signal (999.79), Test harness depth (999.84), Signing probe surface (999.86)
**Areas discussed:** Signing probe surface (999.86), Test harness depth (999.84), plus one carve-out from Counter surgery (999.78 ceiling)

---

## Area selection

| Option | Description | Selected |
|--------|-------------|----------|
| Counter surgery (999.77 + 999.78) | Sibling vs replacing `phase_commit_count`; the never-reset total's ceiling and consequence | |
| Staleness signal (999.79) | Recorded plan-count comparison vs invalidating at `--force` | |
| Test harness depth (999.84) | Plain directory vs a real linked `git worktree` harness | ✓ |
| Signing probe surface (999.86) | Inline vs path key forms, agent requirement, fingerprint reporting, GPG branch | ✓ |

**Notes:** The two unselected areas were routed to Claude's Discretion, with resolutions and
reasoning written into CONTEXT.md rather than left open. One sub-decision was later carved back out
(see the 999.78 ceiling below), because its consequence — when unattended runs stop — was judged not
to be the orchestrator's to choose.

---

## Signing probe surface (999.86)

### Q1 — non-interactivity

Preceded by a live measurement showing a raw `ssh-keygen -Y sign` probe can block indefinitely on a
passphrase prompt (timed out at 6s against a 30s askpass), with `SSH_ASKPASS_REQUIRE=never` shown to
turn that into exit 255 in 0s while leaving the working signing path at exit 0.

| Option | Description | Selected |
|--------|-------------|----------|
| `SSH_ASKPASS_REQUIRE=never` + timeout | Both; the timeout covers what the env var does not (wedged agent, stalled PKCS11) | ✓ |
| `SSH_ASKPASS_REQUIRE=never` only | Simpler; leaves non-askpass blocking routes open | |
| Timeout only | Preserves the operator's real environment; pops a passphrase dialog on every check | |
| You decide | | |

**User's choice:** `SSH_ASKPASS_REQUIRE=never` + timeout → **D-01**

### Q2 — where an actionable `NotViable` reason comes from

| Option | Description | Selected |
|--------|-------------|----------|
| Probe decides; `ssh-add -l` retained for prose only | Keeps today's diagnostics; doubles spawn surface; predictor logic survives as prose | |
| Probe decides; fixed generic reason per failure class | Smallest surface, D-08-safe by construction, no second spawn; strictly less actionable | ✓ |
| Probe decides; classify `ssh-keygen`'s own stderr | Most truthful attribution; couples to OpenSSH message text across versions | |
| You decide | | |

**User's choice:** fixed generic reason per failure class → **D-02**
**Notes:** Three classes remain distinguishable from the probe alone — timed out, non-zero exit,
`ssh-keygen` absent. The two existing pre-probe early returns are unchanged. The loss of the
"agent reachable but this key not loaded" distinction was stated as an accepted cost, not glossed.

### Q3 — the inline `key::` form

Preceded by a measurement, with negative control, showing that materializing an inline blob to a
temp file **is** probe-able when the agent holds the key (exit 0) and fails when it does not (255).
So this option was rejected on surface cost, not feasibility.

| Option | Description | Selected |
|--------|-------------|----------|
| Materialize to a 0600 temp file and probe it | Full coverage, faithful; adds temp-file I/O and a cleanup path to a preflight check | |
| Inline blobs return `Unknown`, no probe | Fail-soft per 20d D-06; smallest surface; `key::` users get no verdict at all | ✓ |
| Probe the path form, keep the predictor for inline | No coverage lost; the defective predictor survives in a corner | |
| You decide | | |

**User's choice:** inline blobs return `Unknown`, unprobed → **D-03**

### Q4 — dead public API

| Option | Description | Selected |
|--------|-------------|----------|
| Remove `classify_ssh_add_status` + `SigningStatus`; treat it as the breaking change it is | Matches the delete-dead-code rule; a real public-API removal in a published crate | ✓ |
| Keep both, unused, with a doc comment | No semver event; dead public API still reads like the sanctioned way to judge viability | |
| Keep the enum, remove the fn | Splits the difference; a public enum no public function produces | |
| You decide | | |

**User's choice:** remove both → **D-04**

### Q5 — continue or move on

**User's choice:** Next area. What would otherwise have been asked here — the probe's `-n` namespace,
the timeout duration constant, the throwaway payload's location — was routed to the planner, with an
explicit instruction to *verify* the namespace against a real git-produced signature rather than
assume it. The GPG/openpgp branch stays untouched per the backlog's scope discipline.

---

## Test harness depth (999.84)

Preceded by a correction to the backlog entry's premise: real `git worktree add` fixtures already
exist in three places in the workspace (`staleness.rs`, `preflight.rs:1198`, `worktree.rs`), so the
"this would be the workspace's first" framing in the 999.84 and 999.76 entries is false at workspace
scope. Also established: `pipeline_launch.rs`'s test mod already drives a real `advance()` over a
real git repo, so the choice was fixture fidelity, not build-from-scratch.

### Q1 — fixture fidelity

| Option | Description | Selected |
|--------|-------------|----------|
| Plain dir + decoy PLAN in `project_root` | Same cost as the bare version; the revert then fails because the wrong root was read, not because the main checkout was empty | ✓ |
| Plain dir, `project_root` left empty | The backlog's proposal; discriminates partly by a condition production never satisfies | |
| Real linked `git worktree`, PLAN committed on `feature/phase-NN` | Fully faithful; disproportionate for an argument that resolves a path | |
| You decide | | |

**User's choice:** plain dir + decoy PLAN → **D-05**

### Q2 — keeping the negative control from decaying

| Option | Description | Selected |
|--------|-------------|----------|
| Mechanical opposite-result assertion in the same test | Re-runs on every `cargo test`; does not by itself prove `:1070` passes `execution_root` | ✓ |
| Prose record only (SUMMARY.md + doc comment) | Nothing re-runs it; a later refactor can silently stop it discriminating | |
| Committed evidence file under `35-evidence/` | Strongest provenance; heavier than a one-line revert warrants; still does not re-run | |
| You decide | | |

**User's choice:** mechanical opposite-result assertion, alongside the performed revert → **D-06**
**Notes:** Criterion 4's performed revert remains binding regardless. Three further points were
settled by fact rather than preference and not put to the operator: `advance()` is `pub(crate)` so
the test cannot live in `tests/`; the `init_repo` + scoped-thread + gate-polling harness already
exists; and `stub_agent_binary("claude")` with the `checkpoint_auto_decided` event as the observable
means nothing real is launched.

---

## Loop bound ceiling (999.78) — carve-out

Raised after the operator had declined the surrounding area, on the grounds that the ceiling's
consequence changes when unattended runs stop.

| Option | Description | Selected |
|--------|-------------|----------|
| Decide it — what does hitting the ceiling do? | | ✓ (via clarification) |
| It's yours — write it as discretion | | |
| Reopen the whole counter area | | |

**User's response:** *"What's the purpose of this counter again?"* — a clarification request, not a
selection. Answered in plain text: `consecutive_failures` is resettable by design (999.66), and the
reset is defeated by the ordinary behaviour of GSD commands committing `.planning/` artifacts every
cycle, so a run that makes no real progress never reaches the ceiling; the new never-reset total is
the backstop that trivial commits cannot defeat, and it is also the number the Supervise gate
message should be showing. The question was then re-put.

### Re-put — what happens at the ceiling

| Option | Description | Selected |
|--------|-------------|----------|
| Fire a human gate; run stays alive | Same shape as `MAX_CONSECUTIVE_FAILURES`; nothing destroyed | ✓ |
| Abort the phase outright | Strongest bound; destructive and irreversible relative to gating | |
| Gate in Supervise, abort in Auto | Mode-dependent; contradicts Auto's existing ceiling | |
| You decide after all | | |

**User's choice:** fire a human gate → **D-07**

---

## Wrap-up

**Question:** Anything still unclear before CONTEXT.md is written?
**User's choice:** "I'm ready for context."

---

## Claude's Discretion

Routed here by the operator's area selection, and written into CONTEXT.md as resolutions with
reasoning rather than left open:

- **999.77** — change `phase_commit_count`'s return type to `Option<u32>` rather than adding a
  sibling; the sibling reintroduces the two-implementations hazard the function's own doc comment
  says the extraction removed. Stacks a second public-API change on D-04.
- **999.77** — the two false doc comments are part of the deliverable; the two-cycle regression test
  is the only discriminating one; Gemini's AGREE on this logic does not clear it.
- **999.78** — the new counter's `State` shape, `#[serde(default)]`, untouched by `transition()`,
  ceiling constant (~10 suggested); the gate message leads with the cumulative total; IN-02's
  distinct `loop_back` reason string.
- **999.79** — a run-scoped content-fingerprint freshness signal, **departing from the backlog
  entry's plan-count proposal** and flagged as such. Enabled by the finding that `start()` calls
  `State::new` unconditionally. Both directions must be tested or Phase 33's `--gaps-only` path
  regresses silently.
- **999.86** — the `-n` namespace (to be *verified*, not assumed), the timeout constant, the payload
  location. The GPG branch stays untouched.
- **Plan decomposition and sequencing** across the five items.

## Deferred Ideas

- 999.76's open question — whether the 999.76-touched tests need a real linked `git worktree`
  harness. Stays open; its framing needs correcting first (such fixtures already exist).
- Richer `NotViable` diagnostics — the rejected "prose only" option is the recorded way back.
- Probing inline `key::` keys — deferred on surface cost, not feasibility; measured working.
- 999.85 — out of scope per REQUIREMENTS.md, and specifically not to be touched even though this
  phase edits the same file for 999.77.
- DEN-50 — `devflow release`'s real signing executor, unaffected and still separate.
- Any defect these fixes reveal — filed, not fixed in-phase (34/D-04).

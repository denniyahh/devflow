---
phase: 28
slug: close-the-checkpoint-answer-return-path
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-07-31
---

# Phase 28 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

**Verdict:** `SECURED` — 16/16 threats closed, `threats_open: 0` at the configured
`block_on: high` threshold.

**Register origin:** authored at plan time (all six PLAN.md files carry a parseable
`<threat_model>` block), so the audit verified mitigations rather than building a
retroactive STRIDE register.

**Audit depth:** ASVS L1 was configured, but L2/L3-depth tracing was applied to three
surfaces where a shallow grep would have been misleading: the static-vs-runtime gate
ordering, the `session_id` forgery guard, and the audit-trail emission point. The
auditor read each named regression test's *body* rather than confirming its existence,
and independently re-ran every cited test plus the full workspace suite.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| agent → `.planning/phases/**/*-PLAN.md` | Plan files are agent-writable during Code; the D-01 static scan reads them. Pre-existing boundary already documented and accepted by `verify.rs` — reused, not newly introduced. | Task gate declarations |
| agent → `.devflow/phase-NN-stdout` | Fully agent-controlled capture that the confirmation reader parses. Consulted only *after* the static declaration passes. | Checkpoint gate field |
| agent → `DEVFLOW_RESULT` marker | Agent-authored JSON deserialized into `AgentResult`. The reason `session_id` is deliberately kept *off* that struct. | Stage status |
| DevFlow → resumed Claude session | An instruction sent into an already-authenticated session with permissions bypassed, no human able to intervene. | Synthesized decision instruction |
| `devflow.toml` → `state.yes_ship` | A repository-resident, version-controllable file can now pre-authorize an unattended Ship (D-12, reversing Phase 23 D-05). | Ship pre-authorization |
| `.devflow/state-NN.json`, `.devflow/events.jsonl` → local filesystem | Where the session id and the auto-decide audit record come to rest. Both gitignored (`.gitignore:26`, `.gitignore:34`). | Session id, decision record |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-28-02 | Denial of Service | `ClaudeAgent::exec_resume_command` | **high** | mitigate | Re-passes both `--dangerously-skip-permissions` and `--output-format json` on the resume argv (`agents/claude.rs:64-77`); guarded by `resume_command_includes_permission_bypass` whose panic message warns future editors not to delete it as redundant | closed (PROVEN) |
| T-28-03 | Elevation of Privilege | Unconditional auto-decide (D-03) on an irreversible action | **high** | accept | Operator's twice-reaffirmed decision. Compensating controls verified in code: static gate bounds reachability; `MAX_CHECKPOINT_RESUMES=3` (`mode.rs:75,85`); audit event emitted before spawn (`pipeline_launch.rs:216-227`) | closed (controls PROVEN) |
| T-28-06 | Elevation of Privilege | `config::yes_ship` + `commands::start` combine | **high** | accept | Default `false`; OR-combine with never-silent notice (`commands.rs:133-142`); dry-run reports resolved state; D-13 untouched. CLI-boundary test `yes_ship_config.rs` (5/5, real process, real stdout) | closed (PROVEN) |
| T-28-08 | Denial of Service | `prompt::define_stage_prompt` | **high** | mitigate | Headless-hang branch deleted (`prompt.rs:181-194`); test `define_prompt_never_invokes_discuss_phase`. **Independently confirmed live** — see Live Verification below | closed (PROVEN + observed live) |
| T-28-11 | Elevation of Privilege | `pipeline_launch::resume` erasing an unfired `--until` cap | **high** | mitigate | Clear gated on `if state.stopped` (`pipeline_launch.rs:316-320`); test `resume_preserves_unfired_until_cap` asserts reloaded on-disk state | closed (PROVEN) |
| T-28-01 | Spoofing / EoP | `Action::GateReview` guard ordering | medium | mitigate + accept | Rust short-circuit `&&` at `pipeline_launch.rs:489-491` guarantees the agent-uncontrollable static PLAN.md scan runs strictly before the agent-controlled stdout confirmation | closed — ordering PROVEN; literal-correctness now PROVEN after `b22e6cf` (was DISPROVEN on first live run) |
| T-28-04 | Spoofing | `agent_result::claude_session_id` | medium | mitigate | Direct `value.get("session_id")` on the top-level envelope only, never nested traversal; `AgentResult` carries no `session_id` field; test `session_id_in_devflow_result_marker_is_not_returned` uses differing top-level/embedded ids | closed (PROVEN) |
| T-28-05 | Information Disclosure | Session id at rest | low | accept | `.gitignore:26`, `:34`; same local-filesystem boundary as existing captures of equal or greater capability | closed |
| T-28-07 | Tampering (self-inflicted) | Phase 28's own PLAN.md files | medium | mitigate | `rg -l 'gate="blocking[-]human"' .planning/phases/28-*/28-*-PLAN.md` → exit 1, no matches; fixtures build the literal via `const`/`format!` | closed (PROVEN) |
| T-28-09 | Tampering | `idempotent_stage_prompt` (Plan arm) | medium | mitigate | Plan-only, wording/artifact/command unchanged (`prompt.rs:148-169`); test `plan_prompt_is_idempotent` | closed (PROVEN) |
| T-28-10 | Repudiation | Define now performs no work | low | accept | D-14's intent; the Define prompt's own text states it in the capture, so the run record is not silent | closed |
| T-28-12 | Repudiation | Discarded cap left no record | low | accept | Discard removed rather than logged — the case can no longer occur | closed |
| T-28-13 | Elevation of Privilege | `devflow.toml` as shared-repo attack surface | medium | accept | Sits inside the same code-review boundary as the shipped source; never-silent notice surfaces it on first use. Strongest argument for revisiting D-12 if this ever ships to multi-operator repos | closed |
| T-28-14 | Elevation of Privilege | `run_gate_with_timeout` inheriting an authorization | medium | mitigate | `auto_response` is caller-supplied and doc-forbidden from deriving off `state` (`pipeline_gate.rs:281-297`); `yes_ship` appears only in `print_dry_run` and the guard test; `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` confirmed byte-for-byte untouched via `git show 57bcd2d` | closed (PROVEN) |
| T-28-15 | Denial of Service | Unbounded resume loop | medium | mitigate | `MAX_CHECKPOINT_RESUMES` ceiling (`pipeline_launch.rs:493`); test asserts **both** zero `checkpoint_auto_decided` events AND that the gate context names the exhaustion | closed (PROVEN) |
| T-28-16 | Repudiation | Unresolvable checkpoint reads as generic failure | medium | mitigate | `augment_unresolved_checkpoint_reason` (`pipeline_launch.rs:358-365`); tests assert the gate `context` field contains the naming text, not merely its presence | closed (PROVEN) |
| T-28-SC | Tampering | package-manager installs | n/a | n/a | No dependency changes — `git diff` on all three `Cargo.toml` files shows only pre-existing version metadata | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` count toward `threats_open`*

### D-05 invariant (constraint, not a threat row)

`crates/devflow-core/src/agents/mod.rs` last touched by commit `3225fd1` (Phase 17,
predating phase 28 entirely) — confirmed via `git log`. The `AgentAdapter` trait is
untouched, as D-05 requires.

---

## Live Verification (2026-07-31) — post-audit

A real headless `devflow start` run was executed against a synthetic project (phase 91)
whose plan carried a genuine human-blocking task, using a binary rebuilt from this
phase's merged code. This closed the A1 assumption the audit had to leave open. Two
findings, in opposite directions:

**1. T-28-08 confirmed live (upgrade).** The Define stage prompt DevFlow actually
generated contains *"you must NOT run an interactive discuss-phase or interview command,
and you must NOT ask for input — this run is headless"*. D-14's fix is observed working
in production, not merely unit-tested.

**2. T-28-01's literal-correctness DISPROVEN (defect found).** The checkpoint fired and
reached DevFlow's capture, but the observed rendering is:

```
**Gate:** `blocking-human`
```

— the value wrapped in **backticks**. `text_reports_human_gate` trims only `*` and space
before reading the value token, so `take_while(alnum || '-')` stops at the backtick and
yields an empty token. The reader returns `false` and the run fell through to the generic
gate. Verified by replaying the exact matcher algorithm against both strings: predicted
form → `true`, observed form → `false`.

**Status: FIXED in `b22e6cf`** — backtick added to the matcher's trim sets, three
regression tests built from the verbatim capture (confirmed RED first), and the full
path retested live: recognized → auto-decide → exactly one `checkpoint_auto_decided`
event with a real session id → resumed → resolved. Zero generic gate fires.

**Security assessment of that defect: it was a functional failure, not a security
regression.** The reader's documented safe-direction property held exactly as designed —
a false negative falls back to the never-silent generic gate. The live run's event log
shows `gate_fired` with `unexpected: true` and a human-review-needed context. Nothing was
silently authorized, no threat disposition changes, and `threats_open` stays 0. The
security controls worked; the feature did not.

The envelope also carried a top-level `session_id`, independently confirming D-04's
capture design.

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| R-28-01 | T-28-03 | Unconditional agent auto-decide of `blocking-human` checkpoints — a deliberate, twice-reaffirmed override of `checkpoints.md` rule 6. Adopted because no usable notification/response channel exists (D-08/D-09), so a "wait for a human" default would hang rather than degrade gracefully. Rated **costly**: undoing the policy is a code revert, but any autonomous decision executed while it was live is a real-world action that cannot be undone. | Operator (D-03, `28-CONTEXT.md`) | 2026-07-30 |
| R-28-02 | T-28-06, T-28-13 | Persisted `yes_ship` in `devflow.toml` — a deliberate reversal of Phase 23's D-05, which forbade exactly this so a standing unattended auto-merge could never become the silent default. Rated **costly** by Phase 23's own assessment: relaxing is easy, re-tightening after operators depend on it is not. Compensating control: standing is now the decision, but never silent. | Operator (D-12, `28-CONTEXT.md`) | 2026-07-30 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-31 | 16 (+1 n/a) | 16 | 0 | gsd-security-auditor (ASVS L1, L2/L3 depth on ordering / forgery / audit surfaces) |
| 2026-07-31 | — | — | — | Live `devflow start` run — closed A1; confirmed T-28-08 in production, disproved T-28-01's assumed literal (see Live Verification) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] D-05 invariant verified (`agents/mod.rs` untouched)
- [x] Self-referential hazard re-checked (no gate literal in phase 28's own plans)
- [x] **Follow-up CLOSED (functional, not security):** the confirmation reader now matches the real-world code-span rendering — fixed in `b22e6cf`, retested live end-to-end (recognized → auto-decide → one audit event → resumed → resolved). Security dispositions were unaffected throughout; `threats_open` stayed 0 because the safe-direction fallback held while the defect was live.

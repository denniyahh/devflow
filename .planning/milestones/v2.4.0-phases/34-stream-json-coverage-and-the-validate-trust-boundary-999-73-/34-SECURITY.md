---
phase: 34
slug: stream-json-coverage-and-the-validate-trust-boundary-999-73
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
security_block_on: high
created: 2026-08-06
---

# Phase 34 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

**Register origin:** authored at plan time (`register_authored_at_plan_time: true`). Five of six
plans carry a `<threat_model>` block; 34-06 does not, and its SUMMARY records why — every change in
that plan is inside `#[cfg(test)]` code or test fixtures.

**Audit depth:** ASVS L1 (grep depth), per `workflow.security_asvs_level`. Mitigations were checked
against source at HEAD rather than accepted from SUMMARY claims. See § Audit Limitations for what
that does and does not establish.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| agent process → DevFlow orchestrator | The agent writes `DEVFLOW_RESULT:` into its own stdout capture. Every field — `status`, `verdict`, `commits`, `summary` — is attacker-or-bug-controlled input from the orchestrator's point of view. | Self-reported outcome envelope |
| Layer 0 probe → Layer 1 self-report | Layer 0's probe outcome is independently derived (a shell command's exit status); Layer 1's verdict is self-reported. The graft crosses this boundary. | Verdict + status |
| agent-written envelope → Validate classification | `status` and `verdict` are deserialized independently with no cross-check; the classifier arbitrates them against provenance. | Classification inputs |
| classifier → pipeline state machine | `ValidateOutcome::Passed` is the only value reaching `transition(.., Stage::Ship)` without a gate in `Mode::Auto`. | Control-flow authority |
| PLAN.md declarations (agent-writable during Code) → external probe execution | `external_verify` commands are read from agent-writable files; execution additionally requires `DEVFLOW_TRUST_EXTERNAL_VERIFY` to hold the exact reviewed command array. | Shell command arrays |
| main checkout ↔ phase worktree | Two filesystem roots with divergent tracked content for a phase's duration. | `.planning/` tracked content |
| DevFlow orchestrator → local filesystem (`.devflow/`) | Agent stdout captures are written and pruned here; everything under it is gitignored by a literal `*`. | Raw agent transcripts |
| `.devflow/` capture → public git history | Copying a file out of `.devflow/` into `.planning/` makes it a permanent public artifact on push. | Home path, OS username, session id |
| compiled constant → runtime launch shape | `STREAM_JSON_STAGES` is baked into the binary; source and running binary can silently disagree. | Launch-path selection |
| operator shell `PATH` → which binary drives the run | More than one `devflow` can exist; `PATH` order decides, and a stale copy shadowing a fresh one produces a wrong result with no error. | Binary provenance |
| agent process → local filesystem (capture run) | The capture run spawns a real `claude` with `--dangerously-skip-permissions` into a scratch repository. | Filesystem write authority |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-34-01-01 | Spoofing | `reconcile_layer0_verdict` | high | mitigate | Graft gated on Layer 1's own status — `.filter(\|layer1\| layer1.status == AgentStatus::Success)` confirmed at `agent_result.rs:2203`. Pinned by `layer0_verdict_graft_declines_when_layer1_status_is_not_success`. | closed |
| T-34-01-02 | Tampering | breadth of the graft fix | medium | mitigate | NC-5 positive half `layer0_verdict_graft_still_transplants_a_passing_layer1_verdict` present, so the filter is not indiscriminate. | closed |
| T-34-01-03 | Repudiation | in-source record | low | mitigate | Two of the overstated notes corrected by 34-01 task 3. One residual instance disclosed — see F-34-02. | closed |
| T-34-01-04 | Denial of Service | `idle_timeout_result`'s guard | high | mitigate | Comment text confirmed unedited; `status: AgentStatus::IdleTimeout` / `verdict: None` intact at `agent_result.rs:1753,1758`. **But its stated rationale went stale in this same phase — see F-34-01.** | closed |
| T-34-01-SC | Tampering | package installs | low | accept | No `Cargo.toml` dependency added by the phase. | closed |
| T-34-02-01 | Denial of Service | `prune_history` / `DEFAULT_CAPTURE_RETENTION` | medium | mitigate | Raised to 12 with recorded arithmetic; retain and evict-one behaviour both pinned. | closed |
| T-34-02-02 | Repudiation | canary test premise | medium | mitigate | Discriminator moved onto the legacy opt-out; verified under a fully-widened constant. | closed |
| T-34-02-03 | Information Disclosure | evidence directory | high | mitigate | README names the three PII fields and the copy-out rule. Blocking control is 34-05's checkpoint; independently re-scanned clean — see Audit Trail. | closed |
| T-34-02-04 | Tampering | retention raise | low | accept | ~1 MB per phase locally; env and TOML overrides remain. | closed |
| T-34-02-SC | Tampering | package installs | low | accept | No dependency added. | closed |
| T-34-03-01 | Spoofing | `classify_validate_outcome` | high | mitigate | Status position enumerated, **0 wildcards**, all 7 `AgentStatus` variants named — read directly at `pipeline_outcomes.rs:228-269`. Rust exhaustiveness makes an 8th variant a compile error. | closed |
| T-34-03-02 | Denial of Service | the `(false, Success, Gaps\|None)` cells | high | mitigate | `pipeline_outcomes.rs:243` routes them to `Failed`, preserving the auto-loop rather than gating on cycle one. | closed |
| T-34-03-03 | Tampering | sweep fixture's `decided_by_layer` | high | mitigate | `classifier_fixture` sets the field explicitly in both directions; mutation control reintroduced the omission and required red. | closed |
| T-34-03-04 | Repudiation | rewritten doc comment | medium | mitigate | Corrected record present, naming criteria 3 and 4 as separate deliverables. | closed |
| T-34-03-05 | Elevation of Privilege | `RateLimited`'s explicit destination | low | accept | Cell unreachable today; delta from the superseded `_` arm is zero. Tension with `decide_action`'s `AutoResume` recorded inline at `pipeline_outcomes.rs:254-257`. | closed |
| T-34-03-SC | Tampering | package installs | low | accept | No dependency; NC-4 ran as a recorded compile experiment rather than adding `trybuild`. | closed |
| T-34-04-01 | Denial of Service | `evaluate_layer0`'s discovery | high | mitigate | `agent_result.rs:2057` confirmed reading `execution_root`; worktree and main-checkout fixtures both pass. | closed |
| T-34-04-02 | Elevation of Privilege | approval/discovery TOCTOU | high | mitigate | Approval comparison and exact-array parse confirmed untouched — fails closed on mismatch. | closed |
| T-34-04-03 | Spoofing | interaction with 34-01's graft | high | mitigate | `depends_on: ["34-01"]` edge plus a precondition assertion halting if the graft fix is absent at HEAD. | closed |
| T-34-04-04 | Denial of Service | `phase_has_blocking_human_checkpoint`'s root | medium | mitigate | Call site confirmed passing `execution_root` at `pipeline_launch.rs:1070`; both roots pinned with opposite-result assertions in `verify.rs`. **Mitigated by construction, not by demonstration** — no test drives the call site. Tracked as ROADMAP 999.84 / DEN-106. | closed |
| T-34-04-05 | Tampering | over-broad retargeting | medium | mitigate | `phase_commit_count` (`:1905`), `checkpoint_reported_in_capture` (`pipeline_launch.rs:1071`) and `evaluate_layer1` all confirmed still reading `project_root`. | closed |
| T-34-04-SC | Tampering | package installs | low | accept | No dependency added. | closed |
| T-34-05-01 | Information Disclosure | committed per-stage captures | high | mitigate | **Fired for real**; handled in 34-05 deviations 2 and 3. Independently re-scanned by the phase verifier with a working negative control — 0 matches. | closed |
| T-34-05-01a | Information Disclosure | `BINARY-PROMOTION.md` vs the PII scan | high | mitigate | Paths written placeholder-scrubbed from the outset; scrub scope extended to every file under `34-evidence/`; zero-match criterion kept with no exclusion. | closed |
| T-34-05-01b | Information Disclosure | stream-json `session_id` | medium | mitigate | Named explicitly in the scrub and re-read at the checkpoint; `<session-01>` placeholders confirmed in the committed captures. | closed |
| T-34-05-02 | Spoofing | evidence provenance | high | mitigate | Three binary gates (mtime+`Compiling`, `sha256sum` equality, behavioural proof); per-capture stream-path discriminating observation recorded in each `run.log`. | closed |
| T-34-05-03 | Tampering | this checkout's git state during the run | high | mitigate | Git-quiet window accepted at the first checkpoint and confirmed in the SUMMARY (CLAUDE.md's two 2026-08-02 failures). | closed |
| T-34-05-04 | Denial of Service | relocated canary refusal | medium | accept | Deliberate behaviour change; both alternatives rejected in D-15 and recorded in the constant's doc comment. | closed |
| T-34-05-05 | Denial of Service | widening on a non-draining capture | high | mitigate | Decision table requires a stated basis for calling a shape pathological; defaults to leaving a stage narrow when n=1 cannot supply it. | closed |
| T-34-05-06 | Repudiation | an un-widened stage's recorded reason | medium | mitigate | All five `Stage` variants named by name in `STREAM_JSON_STAGES`'s doc comment; blocking human read before commit. | closed |
| T-34-05-SC | Tampering | package installs | low | accept | No dependency; scratch repo scaffolded in-repo, `claude` CLI already present. | closed |
| F-34-01 | Repudiation | `idle_timeout_result` doc comment | low | accept | **New finding, this audit.** Below the `high` block threshold — non-blocking. Filed as 999.85 / DEN-107. See below. | closed — below high threshold (non-blocking) |
| F-34-02 | Repudiation | `agent_result.rs:6412-6417` test comment | low | accept | Residual overstated comment inside `stream_success_cannot_stand_against_nonzero_exit_code`, disclosed by 34-01's SUMMARY deviation 3 as out of scope. Filed with F-34-01 as 999.85 / DEN-107. | closed — below high threshold (non-blocking) |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## New Finding: F-34-01 — the guard survived, its stated reason did not

`idle_timeout_result`'s doc comment (`agent_result.rs:1746-1750`) was protected from edits by
criterion 5 and by T-34-01-04, and the phase honoured that prohibition exactly. The comment's
*conclusion* — `verdict` stays `None`, because inventing one would advance a run that never
reported — remains correct.

Its *stated mechanism* no longer holds, invalidated twice over by this phase's own fixes:

1. The comment says `classify_validate_outcome` "matches `Some(Verdict::Pass)` FIRST and would
   classify the stage as passed on the strength of that field alone, **whatever the status says**."
   After 34-03, `pipeline_outcomes.rs:233` reads
   `(_, AgentStatus::Success, Some(Verdict::Pass))` — the status position is no longer a wildcard,
   so a non-`Success` status cannot reach `Passed`.
2. The remaining route (a timeout's verdict grafted via `reconcile_layer0_verdict`, since
   `evaluate_layer1` returns the idle-timeout side channel as its **first statement**,
   `agent_result.rs:1795`) is now closed by 34-01's own `.filter(|layer1| layer1.status ==
   AgentStatus::Success)` at `:2203`. `idle_timeout_result` sets `status: AgentStatus::IdleTimeout`
   (`:1753`), so it is filtered.

**Why this is worth recording rather than ignoring.** The prohibition protected the comment's text
while the phase's other two fixes rotted its claim. A future reader who checks the stated mechanism
against the classifier will find it false, may conclude the guard is vestigial, and may "helpfully"
populate a verdict there — reintroducing a route this phase closed. The hazard is indirect and the
severity is low, but it is exactly the Repudiation class T-34-01-03 and T-34-03-04 were filed about.

**Not fixed here, deliberately.** Criterion 5 and T-34-01-04 forbid editing this comment, and that
prohibition was correct for the phase as scoped. The correction belongs to a follow-up that can
weigh it against the guard's history.

**Disposition:** accept for Phase 34, below the `high` block threshold. **Filed 2026-08-06 as
ROADMAP 999.85 / Linear DEN-107**, grouped with F-34-02 — same file, same root cause, same fix.
The follow-up rewrites both comments' rationale to cite the two structural defences that now carry
the invariant, keeping the `verdict: None` instruction intact and unweakened.

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| R-34-01 | T-34-02-04 | Retention raise costs ~1 MB per phase locally; env and TOML overrides remain. | plan 34-02 | 2026-08-06 |
| R-34-02 | T-34-03-05 | `RateLimited`'s classifier destination is unreachable today and the delta from the superseded `_` arm is zero; tension recorded inline. | plan 34-03 | 2026-08-06 |
| R-34-03 | T-34-05-04 | Canary refusal relocating Code→Define is a deliberate behaviour change; both alternatives rejected in D-15. | plan 34-05 | 2026-08-06 |
| R-34-04 | T-34-01-SC, T-34-02-SC, T-34-03-SC, T-34-04-SC, T-34-05-SC | The phase adds no `Cargo.toml` dependency, so no Package Legitimacy Gate run is required. | all plans | 2026-08-06 |
| R-34-05 | F-34-01 | Comment rationale stale but conclusion correct; the guard is now carried structurally in two places. Editing was prohibited by criterion 5. Low severity, below the `high` block threshold. Follow-up filed as 999.85 / DEN-107. | this audit | 2026-08-06 |
| R-34-06 | F-34-02 | Residual overstated comment inside a test, not production doc; disclosed as out of scope by 34-01's SUMMARY. Follow-up filed as 999.85 / DEN-107. | 34-01 | 2026-08-06 |

*Accepted risks do not resurface in future audit runs.*

---

## Audit Limitations

What this audit does **not** establish, stated so the sign-off is not read as more than it is:

- **Depth is ASVS L1 (grep).** Mitigations were confirmed to be *present in source at HEAD*.
  L2 boundary-placement and L3 end-to-end trace checks were not performed — the configured
  `security_asvs_level` is 1, and the workflow's short-circuit rule skips the auditor subagent when
  `threats_open: 0` with a register authored at plan time.
- **T-34-04-04 is mitigated by construction, not by demonstration.** The call site is correct by
  direct read, and no test would catch a regression in it — reverting the argument leaves the
  279-test binary suite green. Tracked as 999.84 / DEN-106.
- **`threats_open: 0` is scoped to the configured `high` threshold.** Two low-severity findings
  (F-34-01, F-34-02) are open in substance and accepted rather than fixed. A lower `block_on`
  would not have cleared this phase without addressing them.
- **One measurement in this audit was initially broken and corrected.** A first pass counted
  `_ =>` wildcards in `classify_validate_outcome` using an `awk` range that overran into the test
  module, returning 1 and appearing to contradict the phase verifier's 0. Reading
  `pipeline_outcomes.rs:228-269` directly resolved it: 0 wildcards in the status position, the
  wildcards being in the `layer0` and `verdict` positions where criterion 3 permits them.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-06 | 33 | 33 | 0 (at `high` threshold; 2 low-severity accepted) | Claude (orchestrator, ASVS L1) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed — at the configured `high` block threshold
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-06

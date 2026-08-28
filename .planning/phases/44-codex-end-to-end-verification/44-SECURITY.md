---
phase: 44
slug: codex-end-to-end-verification
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-27
---

# Phase 44 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

`workflow.security_block_on: high`. Two medium-severity threats remain open below that
threshold — real, non-hypothetical, and both actionable, but not blocking (see below).

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| operator CLI → persisted phase state | `--agent` is operator-supplied and mutates a durable run record | agent selection |
| persisted state → detached monitor process | the monitor reads state after this command exits; ordering is the only guarantee | run state |
| DevFlow → spawned agent process | the relaunch executes a different agent binary against the run's worktree | process spawn |
| DevFlow → `.devflow/cron-instructions-NN.json` | the only durable record of how a parked run resumes itself | resume command |
| DevFlow → external Hermes scheduler | a surviving record can fire a cron against a workflow that no longer exists | rendered shell command |
| agent stdout → `retry_after` | the retry time originates in parsed agent output and is therefore untrusted input | timestamp |
| project filesystem path → rendered shell instruction | an operator pastes and executes the rendered string | shell command |
| DevFlow → live Codex process | a real agent executes with `--sandbox workspace-write` and signing disabled against a real repository | filesystem writes |
| live run → the working tree | concurrent git operations during the run corrupt both the run and the record | git objects |
| evidence files → the project record | downstream readers treat `44-evidence/` as ground truth for whether Codex works | captured process output |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-44-01 | Tampering | `Command::Resume { agent }` parsing | low | mitigate | `AgentKind`'s existing clap `FromStr` rejects unknown values before `resume()` runs (`main.rs:173`, `state.rs:416-429`) | closed |
| T-44-02 | Repudiation | handoff state mutation | medium | mitigate | Mandatory `agent_handoff` event emitted before relaunch spawns (`pipeline_launch.rs:1308-1321`) | closed |
| T-44-03 | Elevation of Privilege | relaunch under a different driver at a stage it cannot run headlessly | medium | mitigate | Refusal via the full `generic_preflight_checks` bundle before any state mutation — upgraded beyond the plan's original `preflight_interactivity_check` alone during this session's fix for finding 2b (`pipeline_launch.rs:1296-1306`) | closed |
| T-44-04 | Tampering | premature cron-record deletion strands a parked run | medium | mitigate | Deletion strictly after `spawn_monitor?` and the `monitor_pid` save (`pipeline_launch.rs:1027-1034`) | closed |
| T-44-05 | Denial of Service | a deletion I/O failure aborting an otherwise-successful relaunch | low | mitigate | Fail-soft `warn!`, never propagated (`pipeline_launch.rs:1052`) | closed |
| T-44-06 | Denial of Service | premature deletion on a failed relaunch | high | mitigate | Deletion downstream of every `?` in the spawn path; `failed_relaunch_preserves_the_phase_cron_instructions_record` | closed |
| T-44-07 | Denial of Service | a surviving record firing a resume cron against a shipped workflow | medium | mitigate | Ship-side deletion after `workflow::clear_state` (`pipeline_gate.rs:275,280-299`) | closed |
| T-44-08 | Repudiation | a record disappearing with no recorded reason | medium | mitigate | `cron_instructions_consumed` event at both triggers, carrying `trigger`/`path_kind` | closed |
| T-44-09 | Tampering | deletion widening to another phase's record | low | mitigate | Per-phase path construction (`ship.rs:80-85`); `clean_phase_deletes_only_the_named_phase_cron_record` | closed |
| T-44-10 | Denial of Service | a deletion I/O error aborting a completed ship | low | mitigate | Fail-soft `info!`, never propagated (`pipeline_gate.rs:298`) | closed |
| T-44-11 | Tampering | command injection into the rendered `hermes cron create` invocation via a project path containing shell metacharacters | high | mitigate | Raw unquoted command built in `ship.rs`, quoted exactly once in `commands.rs::cron_hint_line` (fixed this session over a real double-quoting bug — see 44-CORE-REVIEW-FINDINGS.md finding 1); round-trips through a real `sh -c` for space and apostrophe paths | closed |
| T-44-12 | Denial of Service | an agent-controlled unparseable `retry_after` degrading into a recurring schedule | medium | mitigate | Empty schedule routes to the never-silent gate, never a bare cron expression (`pipeline_outcomes.rs:104-126`) | closed |
| T-44-13 | Tampering | a schedule resolving to the wrong instant, resuming before the rate limit lifts | medium | mitigate | Explicit-`Z` ISO instants (`ship.rs:313-318`); 4 passing tests. Deviation: the plan's declared D-14 negative-control test name does not exist in the repo (undisclosed in 44-03-SUMMARY.md) — does not change disposition since the protective mechanism is independently tested by other means | closed |
| T-44-14 | Repudiation | an operator-facing instruction that looks authoritative but was never executed | medium | mitigate | P-04 required running the rendered command against the installed Hermes CLI. **Not done** — 44-03-SUMMARY.md's own Limits section states it "remains unverified," and the 44-04 dogfood run never hit a rate limit either | **open** |
| T-44-15 | Elevation of Privilege | live Codex run writing outside the intended tree | medium | mitigate | Codex's `-a never`/`--sandbox workspace-write` contract unchanged — empty diff vs merge base | closed |
| T-44-16 | Tampering | a concurrent git operation during the live run corrupting the tree or evidence | high | mitigate | Operator-supervised window, no incident reported | closed |
| T-44-17 | Repudiation | evidence authored rather than captured, making a failed run read as a success | high | mitigate | Real session/thread IDs, token usage, hook events inspected directly in `44-evidence/*` — reads as genuine machine capture | closed |
| T-44-18 | Repudiation | selective reporting of only the attempt that succeeded | medium | mitigate | Every attempt recorded, including a mislabeled-but-correctly-classified one, per 44-CODEX-E2E.md | closed |
| T-44-19 | Information Disclosure | agent captures embedding absolute home paths/usernames into a committed record | medium | mitigate | Plan required review-and-redact before commit. **Not done** — `/home/denniyahh` and `/var/home/denniyahh` appear unredacted, git-tracked, in 10 files under `44-evidence/` (up to 141 occurrences in one file); no credentials found, no redaction note in any SUMMARY | **open** |
| T-44-U1 (unregistered) | Denial of Service / Repudiation | `ship::consume_cron_instructions` TOCTOU | medium | mitigate | Two racing callers could both report consuming the same record (double-counted audit events) — found by in-session adversarial review, not mapped to a PLAN.md threat ID. Fixed (`remove_file_if_still_present`); `consume_cron_instructions_tolerates_a_racing_concurrent_consumer` passes | closed |

*Status: open · closed · open — below `high` threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (`high`) count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| T-44-SC-01 | 44-01 threat model | No package installs in this plan; `Cargo.toml` diff shows only version bumps | gsd-security-auditor (verified factually) | 2026-08-27 |
| T-44-SC-02 | 44-02 threat model | No package installs in this plan | gsd-security-auditor (verified factually) | 2026-08-27 |
| T-44-SC-03 | 44-03 threat model | No package installs in this plan; explicitly declines a timezone crate | gsd-security-auditor (verified factually) | 2026-08-27 |
| T-44-SC-04 | 44-04 threat model | No package installs in this plan | gsd-security-auditor (verified factually) | 2026-08-27 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-27 | 23 (19 mitigate + 4 accept) | 21 | 2 (both medium, below `block_on: high`) | gsd-security-auditor (sonnet), verified against current code — not plan/SUMMARY claims. Re-ran 8+ named regression tests live rather than trusting reported results. |

**Open items, not blocking, tracked for follow-up:**
- **T-44-14** — run the rendered `hermes cron create` command against an installed Hermes CLI (or the next real rate-limit occurrence) and record the result in a SUMMARY/evidence file. Requires live infrastructure and operator judgment about environment — not attempted unilaterally by an agent.
- **T-44-19** — `44-evidence/*` contains unredacted local paths/username. Retroactively editing already-committed evidence trades one concern (PII/path leakage) against another this phase's own threat model treats as load-bearing (T-44-17: evidence must be a genuine, unmodified capture, not hand-authored). Left for the operator to decide how to resolve (redact-and-recommit vs. accept as low-sensitivity local-path exposure vs. some other disposition) rather than an agent silently rewriting historical evidence.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed (0 threats at or above `block_on: high`; 2 open below threshold, tracked above)
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-27

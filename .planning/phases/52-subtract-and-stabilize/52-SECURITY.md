---
phase: "52"
slug: "subtract-and-stabilize"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-11-11"
---

# Phase 52 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail. Built by `/gsd-secure-phase 52`
> (State B, from the 33 plans' `<threat_model>` blocks and the 7 SUMMARY `## Threat Flags` sections), run as
> `/gsd-execute-phase 52`'s verify:post hook. Verified at ASVS level 1 (block on high) by gsd-security-auditor:
> every mitigation and every named test was located in the tree; no test was re-run by the audit (the final tree's
> `scripts/check.sh all` exited 0 with 2058 passed). No dataflow trace (L2/L3) was done.

---

## Trust Boundaries

| Boundary | Description | First declared |
|----------|-------------|----------------|
| agent stream → DevFlow | `resetsAt` comes from the agent's output and is untrusted text | 52-01 |
| operator → CLI | the operator types verbs; a removed verb must fail loudly, not run something else | 52-02 |
| operator → gate answer | a gate answer names the stage it answers; a wrong stage would answer the wrong gate | 52-03 |
| test harness → verdict | e2e tests are the evidence the capture shape works; a test that silently switched shape would be a false green | 52-04 |
| DevFlow → agent process | the launch shape decides what environment and stdin the agent gets | 52-05 |
| release cut → crates.io | the preflight is the last check before an irreversible publish | 52-06 |
| DevFlow → git worktrees | `ensure_phase_worktree` creates the phase worktree from a base ref | 52-07 |
| DevFlow → git | finishing must leave the operator's branch and worktree untouched | 52-08 |
| DevFlow → notify command | an operator-supplied shell command receives the end-of-run context | 52-08 |
| agent output → DevFlow | the denial and its reset text come from the agent stream | 52-09 |
| `.devflow/` on disk → recover | recover deletes files under `.devflow/`; removing one arm must not widen another | 52-10 |
| DevFlow → operator | repair hints are instructions the operator follows | 52-11 |
| DevFlow → the operator's checkout | the removed hooks merged, bumped and deleted branches in the launch checkout | 52-12 |
| dependency graph → build | removing crates changes `Cargo.lock` | 52-13 |
| operator config → DevFlow | `devflow.toml` and env vars set run policy | 52-14 |
| `.devflow/` state on disk → DevFlow | a 3.x state or gate file naming `ship` no longer parses | 52-15 |
| state on disk → doctor/recover | these decide whether a run is live or never started, which decides what they clean | 52-16 |
| repository source → test verdict | the scan is the permanent guard that the removed surface stays removed | 52-18 |
| published crate → downstream users | the 4.0.0 break must be exactly the planned one | 52-18 |
| answer file on disk → waiting supervisor | any local process can write an answer file; the CLI is not the only writer | 52-19 |
| DevFlow → GSD subprocess | GSD's JSON decides whether a run may finish | 52-20 |
| `$HOME` → launcher path | the launcher is found under the operator's home | 52-20 |
| GSD reader → finish decision | the status decides whether a run may end as finished | 52-22 |
| DevFlow → operator stderr | diagnostics the operator relies on to notice a skipped or failed step | 52-23 |
| verification report → routing | which fix runs after Validate | 52-24 |
| `cleanup --force` → disk | deletes a worktree, a state file and possibly a branch | 52-25 |
| operator note → agent prompt | operator-authored text becomes part of an agent's instructions | 52-26 |
| agent commits → progress decision | whether the run keeps looping is decided from what the agent committed | 52-27 |
| agent's self-report → DevFlow | an agent can claim it re-verified when it did not (check 5) | 52-28 |
| GSD reader and git HEAD → finish decision | a fresh status and the commit decide whether a run may end as finished | 52-29 |
| gate file → answer routes | the gate file on disk decides which family an answer takes; its `context` can hold agent text | 52-29 |
| response file → supervisor | an answer can be written without `gate_respond` (by hand, by an older binary) | 52-29 |
| crates.io → build | a new dev-dependency enters `Cargo.lock` | 52-29 |
| agent failure text → gate file and notify command | untrusted agent output is stored and forwarded | 52-30 |
| supervisor timer → gate answer | an automatic writer answers a gate without a human | 52-31 |
| supervisor timer → stage launch | an unattended relaunch spends agent quota | 52-32 |
| gate file → print and way-on sentence | the gate file decides what the operator is told; its `context` can hold agent text | 52-33 |
| CLI → response file | `gate_respond` writes the answer the supervisor will act on | 52-33 |

---

## Threat Register

Threat IDs repeat across plans (each plan has its own `T-52-SC` supply-chain row), so every row is qualified by its plan.

| Plan | Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|------|-----------|----------|-----------|----------|-------------|------------|--------|
| 52-01 | T-52-01 | Denial of Service | `reset_time::parse_reset_time` | medium | mitigate | Checked arithmetic, overflow returns `None` (tested); a malformed offset returns `None` instead of a UTC reading (tested, review C14); a value is never rounded earlier, so a relaunch can never fire before the reset (tested); `None` leads to a plain gate (D-16) | closed |
| 52-01 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added; `Cargo.lock` unchanged | closed |
| 52-02 | T-52-02 | Repudiation | `main.rs` `enum Command` | low | mitigate | `removed_subcommands_are_unknown` asserts the clap rejection text for each removed verb, with `status` as the control | closed |
| 52-02 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added; `Cargo.lock` unchanged | closed |
| 52-03 | T-52-03 | Tampering | `resolve_gate_target` | low | mitigate | One stage selector (`--stage`); the gate_wedge suite answers through it and keeps its outcomes | closed |
| 52-03 | T-52-04 | Elevation of Privilege | doctor stray hint | low | mitigate | The hint makes the operator confirm the process identity with `ps -o args= -p <pid>` before `kill`, so a reused pid is not killed blindly | closed |
| 52-03 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-04 | T-52-05 | Tampering | e2e capture-shape coverage | medium | mitigate | The capture witness keeps its capture-only assertions, and a recorded negative control shows a stream-shape agent fails them | closed |
| 52-04 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-05 | T-52-06 | Information Disclosure | launch environment | medium | mitigate | No launch path is added; the existing tests that pin the notify variable's absence from every agent environment stay green under `scripts/check.sh all` | closed |
| 52-05 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-06 | T-52-07 | Tampering | `tools/release-preflight` | high | mitigate | Every ported check has a failing fixture among named tests (the check must bite), and `cut_release_runs_the_release_preflight_tool` pins the script to the tool | closed |
| 52-06 | T-52-08 | Tampering | `git::publish_order` | medium | mitigate | `publish_order_skips_an_unpublished_member` plus the two existing order tests keep core-before-cli and keep the tool out of the order | closed |
| 52-06 | T-52-SC | Tampering | cargo installs | high | mitigate | No external crate added; the only `Cargo.lock` change is the new workspace package | closed |
| 52-07 | T-52-09 | Tampering | `phase_worktree::ensure_phase_worktree` | low | mitigate | Moved verbatim with its fork-base test, which stays green | closed |
| 52-07 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-08 | T-52-10 | Tampering | `finish_run` | high | mitigate | `finish_run` makes no git call; the e2e asserts the worktree and branch exist and no `hook_run`/`workflow_shipped` event appears | closed |
| 52-08 | T-52-11 | Repudiation | end-of-run signal | medium | mitigate | One `workflow_finished` and one `notify_fired` per run end, each carrying the reason, asserted for both end sites | closed |
| 52-08 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-09 | T-52-12 | Denial of Service | `handle_rate_limited_outcome` | medium | mitigate | The run waits at a never-silent blocking gate (notify fires) instead of writing an unread file and dying; asserted by the rewritten fixture | closed |
| 52-09 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-10 | T-52-13 | Tampering | `recover.rs` sweep | medium | mitigate | Only the cron arm is removed; the remaining recover and sweep tests (per-artifact, per-phase scoping) stay green | closed |
| 52-10 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-11 | T-52-14 | Repudiation | repair hints | low | mitigate | Each hint is rewritten to an existing command and its test asserts the new text; the 52-18 source scan forbids the literal `devflow ship` | closed |
| 52-11 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-12 | T-52-15 | Tampering | after-ship hooks | high | mitigate | Deleting the hooks removes DevFlow's only writes to the launch checkout's branches; the 52-08 finish test asserts no `hook_run` event | closed |
| 52-12 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-13 | T-52-16 | Tampering | self-pin guard | medium | mitigate | `workspace_member_pins_match_the_workspace_version` is rewritten and run green before `version.rs` is deleted; the release tool (52-06) carries its own copy with a drift control | closed |
| 52-13 | T-52-SC | Tampering | cargo installs | high | mitigate | Packages are only removed; `git diff Cargo.lock` is checked to remove only `semver`/`git-conventional` (and their now-unused transitive deps) | closed |
| 52-14 | T-52-17 | Repudiation | leftover `yes_ship` key / env export | low | accept | No Ship exists for it to authorize, so ignoring it is safe; `a_leftover_yes_ship_key_is_ignored` pins that it does not fail the load; the CHANGELOG at release names the removal | closed |
| 52-14 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-15 | T-52-18 | Denial of Service | loading a 3.x state with stage `ship` | low | accept | No-migration rule (operator 2026-10-12, D-30): the load fails with the parse error naming the valid stages; close-out clears the operator's own `.devflow/` (VALIDATION manual-only row) | closed |
| 52-15 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-16 | T-52-19 | Tampering | never-launched predicate | medium | mitigate | A prepared Plan run must not read as never launched (control tests at each site), so recover/doctor never treat a live run as unstarted; the predicate reads only the run's own state, so an earlier run's events cannot make a fresh state read as launched (previous-run control test, review C5) | closed |
| 52-16 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-17 | T-52-20 | Denial of Service | loading a 3.x state with stage `define` | low | accept | No-migration rule (D-30); the parse error names the valid stages; close-out clears the operator's `.devflow/` | closed |
| 52-17 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-18 | T-52-21 | Tampering | `no_removed_surface_literal_remains` | medium | mitigate | Planted positives and decoys in the test, non-empty walker asserts, and a recorded plant in a real file that made it fail | closed |
| 52-18 | T-52-50 | Tampering | docs site (`docs/index.md`, `docs/architecture/*`, `docs/diagrams/*`) | low | mitigate | `docs_site_describes_only_the_kept_pipeline` fails on a Define or Ship stage word, a dead link or nav entry, or a deleted module in the map; planted positives and decoys prove each rule bites | closed |
| 52-18 | T-52-22 | Tampering | published `devflow-core` API | medium | mitigate | Default-run semver report matched line by line to planned removals; unplanned breaks are reported, not absorbed | closed |
| 52-18 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-19 | T-52-23 | Tampering | hand-written approve+abort answer | medium | mitigate | Refused at write in `Gates::answer`; read as `Abort` (never `Advance`) when found on disk; both tested | closed |
| 52-19 | T-52-24 | Spoofing | note text read as a command | medium | mitigate | `from_response` ignores the note; the "do not abort" e2e and the substring-rule control prove it | closed |
| 52-19 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-20 | T-52-25 | Spoofing | a planted `gsd_run` under `$HOME` | low | accept | `$HOME` is operator-controlled; `runtime_version` lets callers record the GSD version in an event | closed |
| 52-20 | T-52-26 | Denial of Service | a hung reader | medium | mitigate | Fixed timeout, kill and reap; `a_hung_reader_times_out` | closed |
| 52-20 | T-52-27 | Tampering | fail-open classification | high | mitigate | Closed status set; every failure class is a distinct `Err` with its own test; exit 0 is not success | closed |
| 52-20 | T-52-28 | Information Disclosure | GSD messages with absolute paths | low | mitigate | Event-safe `label()` has no path (`reader_failure_labels_carry_no_path`) | closed |
| 52-20 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added (`thiserror`, `serde_json` already in the tree) | closed |
| 52-21 | T-52-29 | Repudiation | suppressed provenance warning | low | mitigate | Suppression is limited to non-DevFlow projects; the self-dogfood block test is the control | closed |
| 52-21 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-22 | T-52-30 | Tampering | Validate verdict | high | mitigate | Finish requires both verdicts; every non-`passed` status and every reader failure routes away from finish (route-table tests, the never-lies e2e and its planted control) | closed |
| 52-22 | T-52-31 | Tampering | evidence root | medium | mitigate | `the_reader_reads_the_worktree_evidence_root` with the project-root control | closed |
| 52-22 | T-52-32 | Information Disclosure | events | low | mitigate | Events carry status or label only (`a_reader_failure_at_validate_fires_a_never_silent_gate` checks the payload) | closed |
| 52-22 | T-52-63 | Tampering | `State.validate_pass_head` | low | accept | The field lives in DevFlow's own state file under `.devflow/`, the same trust boundary as T-52-59 in 52-29; it is written only by `handle_validate_outcome` (acceptance criterion) and is one of two inputs to a finish, the other being a fresh GSD read (52-29) | closed |
| 52-22 | T-52-52 | Tampering | gate kind | medium | mitigate | The kind that later tells the status-less and rate-limit gates from the verification family (D-39) is a structured field set by the writer and required by the open path, not read from `context` (which can hold agent text); `each_validate_gate_records_its_kind` pins every Validate writer, with the same-stage same-status control, and `each_status_less_gate_records_its_kind` the other three | closed |
| 52-22 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-23 | T-52-33 | Repudiation | default `off` tracing filter | medium | mitigate | Site-by-site audit; class-(c) sites get a plain stderr line with a test; `RUST_LOG` still turns tracing on (control test) | closed |
| 52-23 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-24 | T-52-34 | Tampering | Validate routing | medium | mitigate | One routing input (GSD status); 999.192's case pinned by `a_verifier_written_report_from_code_routes_by_gsd_status` | closed |
| 52-24 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-25 | T-52-35 | Tampering | `cleanup --force` | medium | mitigate | Removes only the named phase's state file and worktree (tested); never deletes a branch a kept worktree uses (tested) | closed |
| 52-25 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-26 | T-52-36 | Tampering | note block | low | mitigate | Delimited block whose end marker cannot occur in the note; control characters other than newline and tab removed; passed by argv or stdin, never through a shell (tested) | closed |
| 52-26 | T-52-55 | Tampering | relaunch note in Plan, Code and Validate prompts | low | mitigate | Moved here from 52-31 with the carrier (same threat, same ID). Rendered only through the shared note-block renderer (control characters other than newline and tab removed, an end marker the note cannot contain); byte-identity and the no-note control are asserted for every stage in both styles (`a_relaunch_note_reaches_every_stage_prompt_in_both_styles`); cleared when the run leaves the relaunched stage, so a note never reaches a later stage's agent (`leaving_the_relaunched_stage_clears_the_relaunch_note`) | closed |
| 52-26 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-27 | T-52-37 | Denial of Service | gap loop | medium | mitigate | Planning-only rounds trip `MAX_CONSECUTIVE_FAILURES` after three, whether the Validate agent FAILs or PASSes over `gaps_found` (tested, U2); the per-phase ceiling of 10 stays as backstop | closed |
| 52-27 | T-52-38 | Tampering | finish over open gaps | high | mitigate | No gaps answer finishes the run (`approving_a_gaps_gate_runs_the_gap_loop` asserts no `workflow_finished`) | closed |
| 52-27 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-28 | T-52-39 | Repudiation | skipped re-verify | medium | mitigate | The next Validate's fresh read decides and a run never finishes over `stale` (control row of `a_stale_validate_runs_the_reverify_pass_then_validate`; 52-29's `can_finish`); each re-verify round is counted, so three unproductive rounds gate (`a_validate_that_takes_the_reverify_route_counts_one_failure`); a counted Plan-pass gate at the per-phase ceiling names it and resets on a continuing answer (`a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered`) | closed |
| 52-28 | T-52-40 | Tampering | stale gaps report | low | accept | D-45 rules 3 and 4: no digest check; a stale `gaps_found` runs the gap loop, whose execute-phase re-verifies (#6078); the gap rounds count toward the caps (D-12). Documented in OPERATIONS.md | closed |
| 52-28 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-29 | T-52-51 | Tampering | reject and approve routes | medium | mitigate | The routers read a gate file only through `answer_family` (structured `gate_kind`), never `context` or the recorded `gsd_status`; a verification approve routes on a fresh read; the decoy controls in both parity tests pin it (52-33's print tests pin the print side, T-52-65) | closed |
| 52-29 | T-52-53 | Elevation of Privilege | a finish without a fresh `passed` and a Validate pass on HEAD | high | mitigate | `finish_run` takes a `FinishPermit` only `can_finish` constructs (`can_finish_needs_passed_and_a_pass_on_the_current_head`, its dirty rows included, the `status.showUntrackedFiles=no` row among them); an uncommitted edit during a gate wait is no PASS on HEAD, and a Validate over a dirty tree records no PASS that a later clean-up could revive (D-46 and its amendment; `approving_a_clean_pass_over_an_uncommitted_change_runs_validate_again`, the proptest's tree dimension); `next_step` is the one table, pinned by the rule-6 proptest and its mutant control; `StatusLessStep` has no finish variant; a hand-written answer is screened before the claim (bypass rows) | closed |
| 52-29 | T-52-61 | Tampering | a report an agent edits to read `passed` | medium | accept | GSD's reader applies its own staleness rules (D-08); a finish also needs a Validate agent PASS recorded on the current HEAD, so a Code agent's edit alone cannot finish a run. Residual: a quick fix to a file the report does not cover can leave `passed` unchanged (#6096), accepted by D-45 rule 4 and documented in OPERATIONS.md | closed |
| 52-29 | T-52-62 | Denial of Service | approve cycling through Validate again or a reopened gate | low | accept | Each cycle ends at a supervise gate that waits for an operator answer (D-04), so nothing loops unattended; abort ends the run on every gate | closed |
| 52-29 | T-52-59 | Tampering | gate files under `.devflow/gates/` read for routing | low | accept | Assumption (pi S2): `.devflow/gates/` is outside a Code or Validate agent's intended write surface; not verified by a sandbox test. The screen still refuses what the routers refuse, however written, and a verification approve still needs `can_finish` to finish | closed |
| 52-29 | T-52-54 | Denial of Service | quick fix over a gaps report | low | accept | A rejected gaps gate runs `/gsd-quick`; GSD then reports `gaps_found` before staleness (#6078), so the next Validate runs the gap loop, whose execute-phase re-verifies (D-45 rule 4); each round is counted (D-12) and bounded by the caps | closed |
| 52-29 | T-52-SC | Tampering | cargo installs (`proptest`) | high | mitigate | Version 1.12.0 from RESEARCH's Package Legitimacy Audit addendum ([OK] verified: registry, repository, licence, not yanked; a scratch `cargo deny` run and its licence control), pinned in `devflow`'s `[dev-dependencies]` (orchestrator decision, 2026-11-10); `Cargo.lock` committed; `scripts/check.sh deps` (`cargo deny`: licenses, sources, advisories, bans; `cargo machete`) gates Task 3 | closed |
| 52-30 | T-52-42 | Denial of Service | stored gate context | low | mitigate | Bounded at 4096 characters (at most 16 KiB of UTF-8), sanitized as before (boundary tests) | closed |
| 52-30 | T-52-43 | Information Disclosure | notify hook payload | low | mitigate | Capped at 300 characters (`notify_context_is_capped_at_300_chars`), as before the change | closed |
| 52-30 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-31 | T-52-44 | Elevation of Privilege | timer writer | high | mitigate | Dedicated no-boolean writer that refuses any gate whose `gate_kind` is not `rate_limit` (tested), same first-writer-wins path as every answer | closed |
| 52-31 | T-52-45 | Tampering | double relaunch | high | mitigate | One claim per gate: the first-writer-wins answer (tested here: a human answer first makes the timer's write `AlreadyResponded`) and the single claim in `run_gate_with_timeout` (wired in 52-32); the durable used flag counts only the timer's own relaunch, so an operator's answer neither spends nor re-arms it (D-38) | closed |
| 52-31 | T-52-46 | Denial of Service | sweep reaping a waiting gate | medium | mitigate | Not-before time in the gate file; sweep skips until then (tested with an ordinary-gate control) | closed |
| 52-31 | T-52-67 | Repudiation | rate-limit relaunch over a recorded canary refusal | low | accept | Accepted assumption, no new code path and no test: a rate-limit relaunch never meets a recorded delivery-canary refusal for its agent, so it cannot end the supervisor without a loud stop (the 999.187 shape). Holds at a44b8d1 because `canary_gate` (`pipeline_launch.rs` ~L586-612) runs a canary only when none is recorded and records it once (~L608, the only production write besides the clear); a rate-limit denial comes from a session that launched: on the stream path `canary_gate` admitted it, so the run's recorded outcome for that agent is `Confirmed`, and the capture path records no canary; the record is cleared only by a committed handoff that changes the agent (~L3226), and a refused handoff returns before that commit (~L3195-3203); `legacy_claude_launch` only goes false to true (~L440). This plan's relaunch action and 52-32's timer add no canary write. Breaks if a change records a canary outcome mid-stage or during a gate wait, re-runs the canary for an agent that already has one recorded, lets `legacy_claude_launch` return to false, or changes `state.agent` outside the committed handoff; 52-33's verification states the same assumption for the stage-failure gate | closed |
| 52-31 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-32 | T-52-47 | Tampering | double launch (timer vs human) | high | mitigate | One claim per gate (D-38): first-writer-wins answer and the single claim in `run_gate_with_timeout`; the used flag is committed durably before the timer's answer and restored when an operator answered first; nothing re-publishes a timer answer after a crash (D-45 rule 5), so a crash can lose the automatic relaunch but never doubles it (crash row of `a_takeover_does_not_rearm_the_auto_relaunch`); `a_passed_reset_relaunches_once_and_a_second_denial_waits` counts Code launches two independent ways; the parity test's `RateLimit` row shows an approve launches once and the timer on the same gate then launches nothing | closed |
| 52-32 | T-52-48 | Denial of Service | relaunch loop | high | mitigate | At most one automatic relaunch per run (durable flag committed before the timer's answer), second denial after it waits; resume and takeover do not re-arm (tested). An operator's answer does not spend it (D-38), so a run can relaunch more than once in total, every launch beyond the one automatic relaunch caused by an operator's answer | closed |
| 52-32 | T-52-49 | Denial of Service | gate timeout ending the wait early, or the timer firing late | medium | mitigate | The gate's lifetime is the later of timeout and reset (`a_future_reset_outlives_the_gate_timeout`); the timer's wake is reset + margin whatever the lifetime (`a_reset_earlier_than_the_gate_timeout_relaunches_at_the_reset`, `the_timer_wakes_at_reset_plus_margin_whatever_the_gate_timeout`); at equal deadlines the wake wins (pinned by the equal-deadline row of `the_timer_wakes_at_reset_plus_margin_whatever_the_gate_timeout`; `a_reset_later_than_the_gate_timeout_relaunches_at_the_reset` pins the wiring, U8) | closed |
| 52-32 | T-52-64 | Denial of Service | automatic relaunch lost to a crash | low | accept | D-45 rule 5: at-most-once by design; the gate keeps naming the `devflow resume` command, and OPERATIONS.md says to run it | closed |
| 52-32 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |
| 52-33 | T-52-65 | Tampering | the route print and the way-on sentence | medium | mitigate | Both read a gate file only through `answer_family` and 52-29's routers, never `context` or the recorded `gsd_status`; the decoy rows of both print tests and the planted controls of `every_gate_names_only_the_routes_its_answers_run` pin it | closed |
| 52-33 | T-52-66 | Repudiation | a printed route that differs from the route run | medium | mitigate | One source: the print and the sentence come from the routers the supervisor dispatches through, and the CLI refuses through the supervisor's own screen function; `a_validate_gate_names_the_route_its_reject_prints_and_runs` asserts the printed route is the launched pass. Residual: a verification approve's print reads GSD's status at answer time and the supervisor reads it again at pickup, so a status change between the two can route differently from the print; the print says so | closed |
| 52-33 | T-52-SC | Tampering | cargo installs | high | mitigate | No package added | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

### Register text the code has since moved past (all still closed)

- **52-30 / T-52-43:** the row names `notify_context_is_capped_at_300_chars` and a 300-char cap. 52-33 raised
  `GATE_CONTEXT_DISPLAY_CHARS` to 450 (`crates/devflow-core/src/gates.rs:1834`); the test is now
  `gates::tests::notify_context_is_capped_at_the_display_bound`. The payload is still capped (52-30-SUMMARY).
- **52-30 / T-52-42:** the 4096-char bound applies to each reason, not the whole context (about 4.4K with the lead).
- **52-11 / T-52-14:** one named test, `stop_reaches_the_holder_past_a_claimed_ship_gate`, went with the Ship stage
  (65f4bf0); the ban itself is `doc_check`'s `devflow ship` token.
- **52-18 / T-52-22:** its semver evidence covers 52-01..52-17; 52-32's re-check maps every devflow-core break of the
  phase (15 kinds) to a plan (52-32-SUMMARY).
- **52-31 / T-52-67 (accept):** its `legacy_claude_launch` clause names a field 52-05 removed; the assumption (the
  only canary writes outside tests are in `canary_gate` and the clear in `hand_off_agent`) still holds.

### Unregistered threat flag (warning, non-blocking)

- 52-29's supervisor protocol reply `{"ok":false,"error":"already_finished"}`
  (`crates/devflow-core/src/supervisor/runtime.rs`, the operator's finish-wins decision) has no threat ID. 52-29's
  SUMMARY says an older `devflow stop` reads it as a refusal; not verified against an older binary. No migration is
  owed (D-28/D-30).

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-52-01 | 52-14/T-52-17 | no Ship stage left, so a leftover `yes_ship` key is ignored (`crates/devflow-core/src/config.rs`) | planner, in the plan's threat model | 2026-11-11 |
| AR-52-02 | 52-15/T-52-18 | no migration (D-30): a persisted `ship` stage fails to parse (`crates/devflow-core/src/stage.rs`) | planner, in the plan's threat model | 2026-11-11 |
| AR-52-03 | 52-17/T-52-20 | no migration (D-30): a persisted `define` stage fails to parse; leftover `NN-define` gate files are not cleaned up | planner, in the plan's threat model | 2026-11-11 |
| AR-52-04 | 52-20/T-52-25 | `$HOME` (where the GSD launcher is found) is under operator control; the GSD version is recorded | planner, in the plan's threat model | 2026-11-11 |
| AR-52-05 | 52-22/T-52-63 | `.devflow/` is the trust boundary and has one writer (`handle_validate_outcome`) | planner, in the plan's threat model | 2026-11-11 |
| AR-52-06 | 52-28/T-52-40 | D-45 rules 3/4: GSD's stale route limits (gsd-core#6078) are accepted upstream | planner, in the plan's threat model | 2026-11-11 |
| AR-52-07 | 52-29/T-52-61 | D-08 / D-45 rule 4: GSD re-verify limits (gsd-core#6096) are accepted upstream | planner, in the plan's threat model | 2026-11-11 |
| AR-52-08 | 52-29/T-52-62 | every supervise cycle ends at a gate (D-04) | planner, in the plan's threat model | 2026-11-11 |
| AR-52-09 | 52-29/T-52-59 | `.devflow/gates/` is assumed outside agent writes; no sandbox test exists (unverified by design) | planner, in the plan's threat model | 2026-11-11 |
| AR-52-10 | 52-29/T-52-54 | D-45 rule 4, bounded by the failure caps (D-12) | planner, in the plan's threat model | 2026-11-11 |
| AR-52-11 | 52-31/T-52-67 | a rate-limit relaunch never meets a delivery-canary refusal | planner, in the plan's threat model | 2026-11-11 |
| AR-52-12 | 52-32/T-52-64 | D-45 rule 5: at most one automatic relaunch by design; `devflow resume` is named for the rest | planner, in the plan's threat model | 2026-11-11 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-11 | 95 | 95 | 0 | gsd-security-auditor (ASVS L1), via /gsd-execute-phase 52 verify:post |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-11

## Security Audit 2026-11-11

| Metric | Count |
|---|---|
| Threats found | 95 |
| Closed | 95 |
| Open | 0 |

---
phase: "47"
slug: "unattended-decision-policy-consistency"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-09-12"
---

# Phase 47 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

Built by `/gsd-secure-phase 47` from the threat registers the seven plans wrote at plan time. The
audit ran at ASVS level 1 (grep depth) with blocking threshold `high`. No SUMMARY carried a Threat
Flags section.

**Threat IDs name their plan.** The gap-closure plans 47-06 and 47-07 numbered their registers
T-47-10 to T-47-19 again, reusing IDs that 47-03, 47-04 and 47-05 had already given to different
threats (filed upstream as open-gsd/gsd-core#4683). Every key below is `plan/ID`, which makes 30
distinct threats.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| developer/CI shell environment → `scripts/check.sh` (47-01) | Exported env vars cross into the build and can change what "green" means | environment variables (`INSTA_*`) |
| crates.io → workspace lockfile (47-01) | Three net-new dev-dependency crates enter the build graph | third-party source (`insta`, `similar`, `console`) |
| DevFlow → unattended agent, Code loop-back prompt (47-02) | The prompt is the only instruction channel; what it omits, the agent never learns | prompt text |
| DevFlow → resumed Claude session, turn 2 argv (47-03) | The resume instruction grants an unattended agent authority it does not otherwise have | prompt text carrying gate-resolution authority |
| operator gate approval → `launch_stage_inner` (47-03) | `GateAction::Advance` skips the just-adjudicated preflight check, which is how an unattended run reaches this path at all | operator approval |
| author intent → committed golden file (47-04) | A blessed baseline becomes the definition of correct; an unread one launders a wrong string into the contract | `.snap` baselines |
| operator reading `unattended-mode.md` → operating decisions (47-05) | A false statement in the guide is acted on as though it were a product guarantee | documentation |
| phase artifact → project backlog (47-05) | A defect recorded only inside a phase directory is invisible to every later phase | defect records |
| agent final message → `evaluate_layer1` (47-06) | Model-authored text decides a stage's status; everything above and below the result line is untrusted | untrusted model output |
| resumed agent's final message → `blocking_human_checkpoint_reported` (47-06) | Model-authored text on a failed resume decides whether DevFlow resumes the session again | untrusted model output |
| reviewed `.snap` baseline → CI drift guard (47-06) | A blessed baseline becomes the golden value every later wording change is judged against | `.snap` baselines |
| host global git config, injected git config and ssh-agent → scratch fixture repository (47-07) | Inherited hooks, signing settings and environment-injected config cross into a repository the test owns | git config; signing-key access |
| fixture isolation → guard under test (47-07) | Isolation must not change what the guard sees or how it is invoked | git environment |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| 47-01/T-47-01 | Tampering | `run_test` in `scripts/check.sh` | high | mitigate | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no` at `scripts/check.sh:64`; `check_script_pins_snapshots_against_env_override` green; drift check 2026-09-12 exited 101 hardened and 0 in the un-hardened control | closed |
| 47-01/T-47-02 | Repudiation | committed `.snap` baselines | high | mitigate | `.gitignore:81` ignores `*.snap.new` only; `gitignore_ignores_snapshot_scratch_but_not_the_baseline` green; 9 baselines tracked | closed |
| 47-01/T-47-03 | Tampering | supply chain — `insta`, `similar`, `console` | medium | mitigate | `47-RESEARCH.md` § Package Legitimacy Audit (`:557`); at `0afbff5`, `cargo deny check` reported advisories, bans, licenses and sources ok, and `cargo machete` found no unused dependency; both checks were shown to fail on a broken input (Remediation evidence below) | closed |
| 47-01/T-47-04 | Elevation of Privilege | `INSTA_UPDATE=always` in the baseline recipe | low | accept | Recipe documented as a one-shot in DEV-SETUP-CHECKLIST § 8; `check.sh` pins `INSTA_UPDATE=no`. See Accepted Risks Log | closed |
| 47-01/T-47-SC | Tampering | cargo installs | high | mitigate | Package Legitimacy Audit present with `[SLOP]` none and `[SUS]` none; its only `[ASSUMED]` tags are on age and download estimates, not on the load-bearing grounds (`47-RESEARCH.md:565-573`) | closed |
| 47-02/T-47-05 | Elevation of Privilege | `code_policy_applies_to_fix_arm` widening to `GapsOnly`/`AuditFix` | high | mitigate | `claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it` green; widening the helper turns it red (Remediation evidence below) | closed |
| 47-02/T-47-06 | Repudiation | a Validate loop-back decision made without the recorded-reasoning instruction | medium | mitigate | "record the reasoning and comparison" in `CODE_STAGE_POLICY` (`prompt.rs:117`); `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter` green | closed |
| 47-02/T-47-07 | Tampering | silent wording drift in the fix-arm prompt | medium | mitigate | `claude_style_full_execute_fix_prompt_snapshot` green under the pinned invocation | closed |
| 47-02/T-47-08 | Spoofing | an adapter switching render style without the policy following it | low | mitigate | `match kind` over every `AgentKind` variant, with no wildcard arm, in the presence test (`prompt.rs:990-996`) | closed |
| 47-03/T-47-09 | Elevation of Privilege | the carve-out widening beyond `blocking-human` to package verification | high | mitigate | `package_verification_prohibition_is_unconditional` green, and red under a widened sentence; the part not pinned mechanically was reviewed by the operator (`47-UAT.md` tests 14, 18) | closed |
| 47-03/T-47-10 | Repudiation | a self-resolved gate with no recorded reasoning | medium | mitigate | `GATE_RESOLUTION_RULE` defined once (`prompt.rs:88`); "MUST record your reasoning" (`prompt.rs:620`); `the_gate_rule_has_one_definition_site` green | closed |
| 47-03/T-47-11 | Tampering | moving the `checkpoint_auto_decided` emission after the spawn | high | mitigate | In `relaunch_checkpoint_session` the emission (`pipeline_launch.rs:1129`) precedes the `resume_launch_shape` call (`:1142`); the three `relaunch_checkpoint_session_*` tests green | closed |
| 47-03/T-47-12 | Spoofing | a test asserting a joined turn-1+turn-2 string | medium | mitigate | `the_gate_rule_holds_in_both_delivered_turns` captures each turn from its own constructor, with `assert_ne!` (`pipeline_launch.rs:4056`) and a two-way non-containment check (`:4061`); green | closed |
| 47-04/T-47-13 | Tampering | blessing a baseline without reading it | high | mitigate | Content assertions in `policy_carrying_full_execute_fix_prompt_snapshots` and the delivery test green. Reading is not machine-checkable; the operator confirmed the wording (`47-UAT.md` tests 14, 18) | closed |
| 47-04/T-47-14 | Spoofing | a joined turn-1+turn-2 snapshot | medium | mitigate | Two separate CLI baselines, `turn_one_code_prompt` and `turn_two_resume_prompt`; no joined file among the 9 tracked | closed |
| 47-04/T-47-15 | Repudiation | scope creep blessing a large unreviewed baseline | medium | mitigate | `policy_carrying_full_execute_fix_prompt_snapshots` green; the 9 tracked baselines are all in scope | closed |
| 47-05/T-47-16 | Repudiation | `unattended-mode.md` asserting a refusal cannot be overridden | high | mitigate | 47-05 Task 1 gate re-run green, with a broken-copy control that failed; operator confirmed the wording (`47-UAT.md` test 19) | closed |
| 47-05/T-47-17 | Elevation of Privilege | the preflight/resume predicate mismatch and the TOCTOU window | high | transfer | Transferred to the backlog as ROADMAP `999.125` and `999.126`, each with a Fix shape and an Acceptance (47-05 Task 3 gate green) | closed |
| 47-05/T-47-18 | Repudiation | DECN-03 read as fully closed when it closes for claude only | medium | mitigate | Claude-only limit cited in `47-PHASE49-OBSERVATION.md` and in the REQUIREMENTS DECN-03 entry (47-05 Task 2 and Task 3 gates green) | closed |
| 47-05/T-47-19 | Tampering | a whole-file write to `ROADMAP.md` | high | mitigate | 47-05's ROADMAP commits are scoped (`e3703ff` +1/−0, `b30adf2` +36/−4); `roadmap.analyze` exits 0 with a non-zero phase count | closed |
| 47-06/T-47-10 | Repudiation | decision record in `CODE_STAGE_POLICY` and `checkpoint_auto_decide_prompt` | high | mitigate | `COMPLETION_PROTOCOL` requires `DEVFLOW_RESULT` on the LAST line (`prompt.rs:46`); `decision_reasoning_above_the_result_line_parses_to_that_result` green | closed |
| 47-06/T-47-11 | Tampering | `parse_marker_lines` tail budget | medium | mitigate | "Output nothing after it." (`prompt.rs:55`); `TAIL_BUDGET_CHARS = 4000` (`agent_result.rs:2075`); the same test's trailing-text controls green | closed |
| 47-06/T-47-12 | Elevation of Privilege | D-02 gate-rule section and package-verification prohibition | high | mitigate | The only change to `prompt.rs` after 47-06 is a doc comment (`bd7bd7c`); `package_verification_prohibition_is_unconditional` green | closed |
| 47-06/T-47-13 | Tampering | nine re-blessed `.snap` baselines | medium | mitigate | The three snapshot tests green; wording confirmed by the operator (`47-UAT.md` tests 22, 23) | closed |
| 47-06/T-47-14 | Denial of Service | the detector reading a restated gate line in a failed resume's capture | medium | mitigate | `resume_prompt_does_not_read_as_a_blocking_human_checkpoint` green. The cited bounds are present: GateReview only from Failed/Unknown/IdleTimeout (`outcome_policy.rs:59-71`); Claude plus a declared blocking-human gate (`pipeline_launch.rs:1594-1595`); `MAX_CHECKPOINT_RESUMES = 3` (`mode.rs:106`); prior capture archived before rollover (`pipeline_launch.rs:1021`) | closed |
| 47-06/T-47-19 | Denial of Service | a resumed agent that copies the declaration line despite the instruction | low | accept | See Accepted Risks Log; Phase 49's live run observes it | closed |
| 47-07/T-47-15 | Denial of Service | `test-phase-worktree-guard.sh` bootstrap | medium | mitigate | Repository-local `core.hooksPath` and `commit.gpgsign false` (`scripts/test-phase-worktree-guard.sh:70-71`); harness green with 7a, 7b and 7c ok; deleting those two lines fails 7c | closed |
| 47-07/T-47-16 | Tampering | guard under test (`lint-phase-worktree.sh`) | medium | accept | Planned as mitigate. Superseded by review fixes `67cab06`/`c66cba2`, which null global git config before every guard call; the operator accepted the residual risk after measurement. See AR-47-03 and the detail below | closed |
| 47-07/T-47-17 | Information Disclosure | operator's SSH signing key | low | mitigate | The hostile config names a nonexistent key under `$TMP` (`:130`); the fixture pins `commit.gpgsign false` | closed |
| 47-07/T-47-18 | Tampering | environment-injected git config (`GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS`) | low | mitigate | Cleared by unsetting `git rev-parse --local-env-vars` (`:15-16`), which lists both on git 2.55. The harness passed 13/0 under an injected `GIT_CONFIG_COUNT` hook that refused a control commit outside it. The `GIT_CONFIG_PARAMETERS` form was not run | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

**47-07/T-47-16 detail.**

- **What 47-07 planned.** Fixture isolation through repository-local config plus clearing two inherited variables — "never an exported config override" — so the guard sees its normal environment.
- **What review changed.** Review finding WR-01 (`47-REVIEW.md`) found that a global `core.excludesFile` hiding `.planning/` aborted the harness, and that a leaked `GIT_OBJECT_DIRECTORY` escaped the fixture. `67cab06` closed that by exporting `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1`, and `c66cba2` moved the export below the harness's one query against the real checkout. Every guard call has since run without global or system config.
- **Measured 2026-09-13.** The real guard ran in scratch repositories under six global configs — nulled, the operator's own, `core.quotePath=false`, forced colour, `diff.relative=true`, and hostile hooks and signing — against five staged-path scenarios.
  - The operator's own config gave the nulled verdict in every scenario; it sets none of the keys that change `git diff --cached --name-only` output.
  - `core.quotePath=false` made the guard stricter.
  - `diff.relative=true` made it weaker, but only when the guard was started from a subdirectory. Git runs hooks from the repository root.
- **Accepted** by the operator on 2026-09-13 (AR-47-03). The subdirectory weakening has since been removed: `7400042` pins `--no-relative`.
- **Found while measuring, and fixed.** The guard did not refuse a staged `crates/` name that git quotes — any non-ASCII byte under the default config, or a quote, backslash or control character under any config — because a quoted line never matched `^crates/`. This is independent of the harness and of T-47-16. `7400042` reads names NUL-separated. Its harness case 8 failed against the old guard (`passed=16 failed=3`) and passes against the new one (`passed=19 failed=0`); the six-config matrix now refuses every source scenario.

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-47-01 | 47-01/T-47-04 | `INSTA_UPDATE=always` is a documented one-shot for creating baselines. `check.sh` pins `INSTA_UPDATE=no` and unsets `INSTA_FORCE_UPDATE`, so it cannot reach a build verdict | 47-01 plan threat register (plan-time disposition) | 2026-09-12 (recorded) |
| AR-47-02 | 47-06/T-47-19 | A prompt cannot enforce what a model writes. The robust fix, changing the detector, is out of scope. Worst case is up to three extra resumes before the ceiling routes to the review gate | 47-06 plan threat register; detector change out of scope per operator decision A1 | 2026-09-12 (recorded) |
| AR-47-03 | 47-07/T-47-16 | The harness exercises the guard under git's default config rather than the running machine's. Under the operator's config every measured verdict matched; the only weakening found needed `diff.relative=true` plus a manual run from a subdirectory, and `7400042` removed it | Operator, after the 2026-09-13 measurement | 2026-09-13 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-12 | 30 | 29 | 1 (medium, non-blocking) | `/gsd-secure-phase 47` orchestrator — ASVS 1 grep depth; security auditor not spawned (0 open at or above `high`) |
| 2026-09-13 | 30 | 30 | 0 | Remediation during `/gsd-verify-work 47` — T-47-16 accepted by the operator after measurement; evidence gaps from the first audit closed |

**Remediation evidence (2026-09-13).**

- **`cargo deny` control.** An empty license allowlist fails the licenses check (exit 4, `licenses FAILED`); the real `deny.toml` passes (exit 0). `cargo machete` flags an unused dependency in a throwaway crate.
- **Discrimination demos repeated at `dd873a6`.** Widening `code_policy_applies_to_fix_arm`, widening the package-verification sentence, and deleting the fixture's two isolation pins each turned their guarding test red; the restored tree is green and matches HEAD.
- **Full suite.** `scripts/check.sh all` passed at `dd873a6` in 147 s. That is one run, not a flake-rate measurement.

**What these audits do not establish.**

- **Depth.** Grep depth confirms each mitigation is present in the code or the planning record. The first pass ran at `d0b5699`, whose code is identical to `49bb324`; the 2026-09-13 pass re-checked only T-47-16, the guard finding and the evidence gaps above. Neither traces a mitigation end to end, as ASVS 2 or 3 would.
- **Controls.** Controls were run only where the Mitigation column or the remediation evidence names one.
- **Model behaviour.** Whether a live agent follows the prompts is Phase 49's to observe.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer) — 30/30
- [x] Accepted risks documented in Accepted Risks Log — 3
- [x] `threats_open: 0` confirmed — all 30 threats closed as of 2026-09-13
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-12; re-verified 2026-09-13 after remediation

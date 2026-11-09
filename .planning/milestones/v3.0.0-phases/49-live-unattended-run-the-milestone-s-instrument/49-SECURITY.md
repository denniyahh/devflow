---
phase: "49"
slug: "live-unattended-run-the-milestone-s-instrument"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-10-09"
---

# Phase 49 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| executor → planning docs (49-01) | The executor edits tracked planning text that later plans and the verifier read as the phase contract | see 49-01-PLAN threat model |
| operator argv → scaffold (49-02) | The destination path decides where a new repository and commits are written | see 49-02-PLAN threat model |
| scaffold → host git config (49-02) | Global signing and hooks would otherwise apply to the disposable repository | see 49-02-PLAN threat model |
| DevFlow gate process → hook (49-03) | Agent-influenced gate context reaches a shell script through the environment | see 49-03-PLAN threat model |
| local evidence → GitHub (49-03) | The collector queries the throwaway repository's PR | see 49-03-PLAN threat model |
| watcher → launch checkout (49-04) | An out-of-band process observes a directory DevFlow writes | see 49-04-PLAN threat model |
| local evidence → tracked repository (49-05) | Captures and transcripts cross into files that reach a pushed branch | see 49-05-PLAN threat model |
| scaffold → live run (49-06) | Payload content decides whether a correct DevFlow can reach Ship and whether an irreversible gate opens | see 49-06-PLAN threat model |
| negctl → systemd user manager (49-07) | A tool starts and stops a transient unit on the operator's session | see 49-07-PLAN threat model |
| negctl → DevFlow binary (49-07) | The pinned binary runs against a scratch repository and registers it in `~/.cache/devflow` | see 49-07-PLAN threat model |
| evidence directory → checker (49-08) | Files an agent or operator could have edited decide whether the phase's instrument passed | see 49-08-PLAN threat model |
| operator host → GitHub (49-09) | Read-only queries about the throwaway repository and the token | see 49-09-PLAN threat model |
| launcher → systemd user manager (49-09) | Two long-lived transient units run the agent and the watcher; one short `--wait --collect` unit runs the auth probe | see 49-09-PLAN threat model |
| launcher → nested agent (49-09) | The agent inherits the unit's environment: the user manager's environment plus the three `--setenv` values, never the calling shell's | see 49-09-PLAN threat model |
| launcher → Anthropic API (49-09) | The auth probe makes one bounded `claude -p` call per `start` or `probe-auth` | see 49-09-PLAN threat model |
| operator → GitHub (49-10) | Repository creation, pushes and deletion happen by hand from this document | see 49-10-PLAN threat model |
| operator → DevFlow gates (49-10) | Gate answers decide whether the run proceeds, loops or aborts | see 49-10-PLAN threat model |
| operator reply → GSD (49-11) | A typed reply resumes the plan | see 49-11-PLAN threat model |
| live run → GitHub (49-11) | The Ship agent pushes and opens a PR on a private throwaway repository | see 49-11-PLAN threat model |
| local evidence → tracked repository (49-12) | Captures cross into a branch that is pushed and merged into `workspace/denniyahh` | see 49-12-PLAN threat model |
| write-up → milestone decisions (49-12) | The verdict and the DECN-03/SURV-01 records are what later phases and the milestone audit rely on | see 49-12-PLAN threat model |

---

## Threat Register

Threat IDs are qualified by plan (`<plan>/<id>`) because each plan numbers its own register.

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| 49-01/T-49-01-01 | Tampering | ROADMAP.md criterion 2 | medium | mitigate | Exact before/after text in `<context>`; the gate asserts the new fragments and the absence of the old line inside the Phase 49 section only | closed |
| 49-01/T-49-01-02 | Tampering | ROADMAP.md whole-file rewrite | medium | mitigate | Scoped `Edit` only; the gate re-runs `roadmap.analyze` and counts five criteria | closed |
| 49-01/T-49-01-03 | Repudiation | Deleted payload plan | low | accept | Git history keeps the file (`72e3f29`, `8760677`); the gate proves it existed at `aaad4b9` before deletion | closed |
| 49-02/T-49-02-01 | Tampering | destination guard | high | mitigate | Refuse any `realpath -m` destination equal to or under the worktree root or the main checkout, and any existing path; selftest controls assert the refusal text and that nothing was created | closed |
| 49-02/T-49-02-02 | Tampering | git identity and signing | medium | mitigate | Repo-local `user.*`, `commit.gpgsign false`, `tag.gpgsign false` only; never `--global`, never a `GIT_*` export (T-23-02, 999.37) | closed |
| 49-02/T-49-02-03 | Denial of service | default mode drift | medium | mitigate | Independent tree-hash measurement in the plan gate pins the negative arm's fixture | closed |
| 49-03/T-49-03-01 | Elevation of privilege | snapshot-hook.sh context handling | high | mitigate | `DEVFLOW_GATE_CONTEXT` written with `printf '%s'` only; no `eval` (gate-checked); selftest control proves three substitution forms create no file | closed |
| 49-03/T-49-03-02 | Denial of service | hook blocking the gate process | medium | mitigate | Invoked as `timeout 60 …` (runbook); always exits 0; flock bounded at 30 s | closed |
| 49-03/T-49-03-03 | Tampering | evidence after capture | medium | mitigate | Per-snapshot and per-collection `MANIFEST.sha256`; the ledger records the manifest hash; the collector never overwrites a finished collection | closed |
| 49-03/T-49-03-04 | Spoofing | verdict | high | mitigate | `outcome` derives from DevFlow's `workflow_shipped` event; a stopped attempt needs a note and a shipped one refuses one | closed |
| 49-03/T-49-03-05 | Tampering | GitHub side effects | high | mitigate | Only a read-only `pr view`; the gate greps both tools for repository-creating, pushing or PR-mutating commands | closed |
| 49-04/T-49-04-01 | Tampering | watched `.devflow/` | medium | mitigate | Watcher only reads inotify events; selftest case asserts the watched tree is unchanged | closed |
| 49-04/T-49-04-02 | Repudiation | overstated SURV-01 result | medium | mitigate | Analysis text forbids verdict words (gate-checked); overflow and missing stop are reported as blind spots | closed |
| 49-04/T-49-04-03 | Denial of service | runaway watcher | low | accept | Runs in its own systemd user unit (49-09); stops on SIGTERM or a stop file | closed |
| 49-05/T-49-05-01 | Information disclosure | committed evidence | high | mitigate | Home spellings, escaped forms, emails and UUIDs redacted; source scanned first and any secret shape refuses the publish (tracked precedent 999.69) | closed |
| 49-05/T-49-05-02 | Information disclosure | refusal output | medium | mitigate | Messages carry file and pattern name only; the selftest asserts the planted value is absent from output | closed |
| 49-05/T-49-05-03 | Tampering | symlink escape | medium | mitigate | Symlinks refused during the walk; destination written via temp sibling and rename | closed |
| 49-05/T-49-05-04 | Denial of service | false credential refusal of a real capture | high | mitigate | Credential patterns run on JSON-decoded strings with a credential-shaped value (Task 2); Task 3 proves the publish on the host's real phase-43 captures that 30c's raw pattern refuses, with a planted-credential control on the same data | closed |
| 49-05/T-49-05-05 | Information disclosure | credentials the narrowed patterns do not match | medium | accept | A named value under 20 characters or without both a letter and a digit (a short password) is not matched; the evidence comes from a disposable project with no credentials of its own, the prefixed token shapes are unchanged, and the global gitleaks pre-commit hook scans the evidence commit as a second layer | closed |
| 49-06/T-49-06-01 | Denial of service | MAJOR-bump gate (cannot be auto-approved) | high | mitigate | `--check` refuses any breaking commit and any tag on `integration` before launch (D-21, R-8) | closed |
| 49-06/T-49-06-02 | Denial of service | `external_verify` at every stage | medium | mitigate | `--check` refuses any `external_verify` line (R-9) | closed |
| 49-06/T-49-06-03 | Tampering | dirty tracked config after the chain-flag round trip | medium | mitigate | Pretty-form check plus the REAL_SHAPE bridge and the Rust writer test | closed |
| 49-06/T-49-06-04 | Repudiation | a GSD probe that cannot fail | medium | mitigate | Each probe has a selftest control that flips its result | closed |
| 49-07/T-49-07-01 | Denial of service | stray transient units | medium | mitigate | EXIT trap stops then resets exactly the named unit; a selftest case asserts it on the gate-timeout path; Task 3's gate checks the unit is inactive | closed |
| 49-07/T-49-07-02 | Spoofing | base branch injected from the environment | medium | mitigate | The unit's `--setenv` set is only PATH, HOME and the hook command, and the command runs under `env -u DEVFLOW_BASE_BRANCH` (D-14), which removes that variable even if the user manager's inherited environment carried it (none did on 2026-09-29, measured by name); selftest-asserted | closed |
| 49-07/T-49-07-03 | Repudiation | exit-code-only refusal check | medium | mitigate | The checker asserts verbatim text and fields; controls mutate each (memory: control must fail for the right reason) | closed |
| 49-07/T-49-07-04 | Tampering | writes outside scratch | medium | mitigate | Scaffold guard refuses checkout destinations; Task 3 uses a `mktemp -d` home deleted by trap; no remote exists, nothing is pushed (gate-checked) | closed |
| 49-07/T-49-07-05 | Spoofing | the operator's signing key on a throwaway commit, or a headless signing prompt that hangs | medium | mitigate | The scaffold runs with `commit.gpgsign`/`tag.gpgsign` overridden for that command and negctl sets them false repo-locally before its own commit; selftest case with a planted failing signer asserts no signing call and no `gpgsig` header | closed |
| 49-08/T-49-08-01 | Spoofing | verdict.json | high | mitigate | The attempt gate re-derives `shipped` from the collected events and refuses a disagreeing verdict (selftest control) | closed |
| 49-08/T-49-08-02 | Tampering | collected evidence | high | mitigate | `sha256sum -c` over the collector's manifest before any judgement (selftest control) | closed |
| 49-08/T-49-08-03 | Repudiation | criterion read from agent documents | medium | mitigate | Criteria read DevFlow's events, snapshots and git only; `merge_result` is supporting only (R-2) | closed |
| 49-08/T-49-08-04 | Repudiation | a checker that cannot fail | medium | mitigate | Every positive check has a mutation control; the pair refuses arms that agree on the discriminating field | closed |
| 49-09/T-49-09-01 | Information disclosure | a public throwaway repository | high | mitigate | `start` refuses unless `gh repo view` says `PRIVATE` (selftest control) | closed |
| 49-09/T-49-09-02 | Tampering | run targeting this repository | high | mitigate | `start` refuses a `nameWithOwner` equal to this repository's origin (selftest control); the payload lives under `$P49` outside every checkout | closed |
| 49-09/T-49-09-03 | Spoofing | nested agent inheriting the Claude session environment | medium | mitigate | `systemd-run --user` passes only the three `--setenv` values from the caller, so no `CLAUDE*` variable of a Claude Code session reaches the agent, and Task 1's real gate asserts it (`session_names=0`); the environment it does get (the user manager's) is measured by name in `prelaunch/agent-env-names.txt`, not assumed clean; that record cannot show the other credential source, the `env` block of the Claude Code settings files (`~/.claude/settings.json` sets `ANTHROPIC_API_KEY` there, key name checked 2026-09-29), so a probe failure is diagnosed from both by name (`<context>`; 49-10 runbook step 4) | closed |
| 49-09/T-49-09-07 | Information disclosure | credential values in the environment record or on the terminal | high | mitigate | Names only, via `compgen -e` inside the unit; the probe's raw output and stderr stay in a scratch directory deleted on exit, and only derived fields are recorded; a selftest sentinel's name must appear and its value must appear nowhere under `$EV` or in the tool's output, and the real gate rejects any recorded line that is not a bare name | closed |
| 49-09/T-49-09-08 | Denial of service | an agent that cannot authenticate in the unit environment spends one of three attempts (D-11) | high | mitigate | `start` refuses with `auth probe failed` before any run unit starts; the real probe runs in Task 1's gate before any attempt, and a failure there stops for the operator's decision on which credential the agent uses | closed |
| 49-09/T-49-09-04 | Information disclosure | gh token in evidence | high | mitigate | Only the `Logged in to` and `Token scopes` lines are kept; the token is never read or copied | closed |
| 49-09/T-49-09-05 | Elevation of privilege | automated gate approval | high | mitigate | No gate verb and no `yes_ship` write in the tool (gate-checked); Ship pre-authorization comes only from the typed `--yes-ship` flag (D-23) | closed |
| 49-09/T-49-09-06 | Repudiation | unpinned or rebuilt binary mid-attempt | medium | mitigate | Pinned file named by its sha256, mode 0555, never overwritten; sha recorded in `attempt.json` and checked by `--pair` | closed |
| 49-10/T-49-10-01 | Tampering | deleting the wrong repository | high | mitigate | Deletion uses the `nameWithOwner` recorded in `$EV/prelaunch/repo.json`, only after verification and the evidence commit; the launcher already refused a repository matching this one | closed |
| 49-10/T-49-10-02 | Repudiation | evidence lost to cleanup | high | mitigate | Cleanup section ordered after 49-12's sanitized evidence commit; the step is labelled irreversible (D-24) | closed |
| 49-10/T-49-10-03 | Elevation of privilege | hook bypass during a fix | medium | mitigate | Runbook names the guard's escape hatch (`DEVFLOW_ALLOW_OFF_WORKTREE=1`) and contains no hook-bypass flag (gate-checked) | closed |
| 49-10/T-49-10-04 | Spoofing | a gate answered without reading it | medium | mitigate | `gate show` before every answer; unexpected gates recorded in `deviations.txt` (R-6) | closed |
| 49-10/T-49-10-05 | Information disclosure | runbook steering the operator to print credential values while diagnosing an auth-probe refusal | medium | mitigate | The runbook compares by variable name through `prelaunch/agent-env-names.txt` and by key name through the settings files' `env` blocks (`jq … keys[]`, `has("apiKeyHelper")`); both task gates reject a value-printing command in the runbook | closed |
| 49-10/T-49-10-06 | Tampering | a runbook flag checked against the wrong tool | low | mitigate | No line names two of the four tools (gate-checked in both tasks) and a line naming a tool carries no other program's flag (the plan's own content was transcribed and gate-run at planning time), so the flag cross-check attributes each flag to the one tool on its line | closed |
| 49-11/T-49-11-01 | Spoofing | reply text as evidence | high | mitigate | `<verify>` reads the evidence record through the attempt gate; a `done` over a stopped or unjudged attempt fails and GSD waits (R-1); `gate="blocking-human"` keeps auto-mode from answering the checkpoint at all | closed |
| 49-11/T-49-11-02 | Tampering | partial-plan state | high | mitigate | The task writes and commits nothing; `<verify>` asserts no commit for this plan exists before the SUMMARY | closed |
| 49-11/T-49-11-03 | Information disclosure | outward effects | medium | transfer | Repository creation, pushes and deletion are the operator's, from the runbook, on a private repository the launcher verified (D-24, R-10) | closed |
| 49-11/T-49-11-04 | Denial of service | endless retries | medium | mitigate | D-11 cap enforced by `prepare` and reported by the attempt gate as `cap_reached` | closed |
| 49-12/T-49-12-01 | Information disclosure | committed evidence | high | mitigate | Only the sanitizer's output is committed; transcripts, checkouts and the auth-probe variable-name record `prelaunch/agent-env-names.txt` (operator decision 2026-09-29, D-26) are excluded (gate-checked); the sanitizer scan runs over the evidence and the write-up | closed |
| 49-12/T-49-12-02 | Repudiation | overstated verdict | high | mitigate | Verdict line cross-checked against the raw attempt gate; committed copy re-checked in place; the DECN-03 outcome must equal the class Task 2's gate derives from the committed events and captures (jq presence and status checked, event count cross-counted on the raw lines), with quotes found verbatim in the resumed capture; SURV-01 fields compared with the committed analysis.txt, 49-04's closing sentence required, claim verbs only negated and the listed claim shapes absent | closed |
| 49-12/T-49-12-05 | Tampering | committed evidence on a re-execution | medium | mitigate | Task 1 never sanitizes into an existing directory; it replaces a differing `49-evidence/attempt-<N>/` only with `git rm -r` and `git clean` of exactly that directory (history keeps every committed copy; the clean removes only never-committed bytes that the raw evidence re-derives); Task 3 reuses its own earlier backlog entries and its gate fails a duplicated title | closed |
| 49-12/T-49-12-03 | Tampering | ROADMAP backlog edits | medium | mitigate | `add-backlog.md` procedure with `phase.next-decimal`; scoped Edit only; `roadmap.analyze` re-run | closed |
| 49-12/T-49-12-04 | Denial of service | oversized evidence | low | mitigate | 5 MiB per file, 20 MiB total, excerpts noted | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-49-01 | 49-01/T-49-01-03 | Git history keeps the file (`72e3f29`, `8760677`); the gate proves it existed at `aaad4b9` before deletion | plan-time acceptance in 49-01-PLAN | 2026-09-29 |
| AR-49-02 | 49-04/T-49-04-03 | Runs in its own systemd user unit (49-09); stops on SIGTERM or a stop file | plan-time acceptance in 49-04-PLAN | 2026-09-29 |
| AR-49-03 | 49-05/T-49-05-05 | A named value under 20 characters or without both a letter and a digit (a short password) is not matched; the evidence comes from a disposable project with no credentials of its own, the prefixed token shapes are unchanged, and the global gitleaks pre-commit hook scans the evidence commit as a second layer | plan-time acceptance in 49-05-PLAN | 2026-09-29 |

*Accepted risks do not resurface in future audit runs.*

---

## Residuals and Unregistered Flags (audit 2026-10-09)

Not blocking (`threats_open: 0`); recorded so they are not lost:

- **49-05/T-49-05-01 residual.** The plan says any secret shape refuses the publish; the sanitizer matches only `sk-`, `gh?_`, `Bearer` and named assignments (49-REVIEW WR-04, deferred to backlog 999.152). Committed evidence re-scanned clean for AWS, Slack, PEM and JWT shapes, with a planted control that is caught.
- **49-08/T-49-08-02 scope.** `--attempt-gate` re-verifies only `collected/MANIFEST.sha256`; `gates/*`, `prelaunch/*`, `start.stdout` and `negctl/*` are read but not re-hashed, although each gate manifest hash is in `ledger.jsonl`.
- **Unregistered: CR-01 checkpoint heuristic** (`agent_result.rs`) can auto-resume an ordinary failure as a blocking-human checkpoint. Deferred to Phase 49.1.
- **Unregistered: persisted gate notify command.** `.devflow/state-NN.json` now holds a shell string DevFlow runs via `sh -c` (49-REVIEW WR-01(d)); it may hold webhook tokens.
- **Unregistered: user-level `apiKeyHelper`** added to `~/.claude/settings.json` during 49-09 (operator-authorized); it applies to every Claude CLI session on the account.
- **Unregistered: external review export** of a bounded source archive during 49-10.
- No SUMMARY carries a `## Threat Flags` section.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-09 | 55 | 55 | 0 | gsd-security-auditor (ASVS L1, block_on high; all tool selftests re-run) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-09

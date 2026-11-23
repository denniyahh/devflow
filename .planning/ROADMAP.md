# DevFlow Roadmap

> Phase plan source of truth. Each phase drives a `devflow start` agent session.

## ✅ v3.0.0 milestone (CLOSED 2026-11-08 — Unattended Run Survivability)

**Declared 2026-09-03, closed 2026-11-08** (`override_closeout`: all seven verifications `passed`, five digests stale from
later edits; no milestone audit, operator decision). Phases 46-50.1 released as **3.0.0** on 2026-11-03/04; Phase 51 merged
to `develop` (PR #234) and ships inside 4.0.0 (3.1.0 skipped, operator 2026-11-08). Full phase details:
`.planning/milestones/v3.0.0-ROADMAP.md`; requirements: `.planning/milestones/v3.0.0-REQUIREMENTS.md`; phase
directories: `.planning/milestones/v3.0.0-phases/`.

<details>
<summary>✅ v3.0.0 Unattended Run Survivability (Phases 46-51) — CLOSED 2026-11-08</summary>

| Phase | Name | Requirements | Wave | Completed |
|---|---|---|---|---|
| 46 | CI Load Shape and Operator Input Validation | INFRA-01, VALID-01, VALID-02 | 1 | 2026-09-07 |
| 47 | Unattended Decision Policy Consistency | DECN-02, DECN-03 | 1 | 2026-09-13 |
| 48 | Survivable State Writes and Honest Gate Recovery | SURV-01, SURV-02, CHKPT-01, CHKPT-02, TEST-01 | 2 | 2026-09-23 |
| 49 | Live Unattended Run — The Milestone's Instrument | VERIFY-01 | 3 | 2026-10-09 |
| 50 | Addressable Monitor Liveness | SUPV-01 | 4 | 2026-10-21 |
| 50.1 | Split-Lock State-Handling Overhaul (INSERTED) | SUPV-03, SUPV-04, SUPV-05, SURV-03, SURV-04 | 4 (after 50) | 2026-11-02 |
| 51 | Rate-Limit Agent Failover | SUPV-02 | 5 | 2026-11-08 |

- [x] Phase 46 (9/9 plans) — completed 2026-09-07
- [x] Phase 47 (7/7 plans) — completed 2026-09-13
- [x] Phase 48 (20/20 plans) — completed 2026-09-23
- [x] Phase 49 (12/12 plans) — completed 2026-10-09
- [x] Phase 50 (29/29 plans) — completed 2026-10-21
- [x] Phase 50.1 (30/30 plans) — completed 2026-11-02
- [x] Phase 51 (13/13 plans; 51-08 dropped by operator ruling) — completed 2026-11-08

</details>

## 🚧 v4.0 milestone (ACTIVE — Session Continuity)

**Declared 2026-11-08.** A coding session that outlives any one agent. DevFlow becomes a detached runner for one agent
stage plus a decision channel through which an agent asks DevFlow a question instead of a human; GSD owns the workflow.
Define, Ship, the after-ship hooks, the cron-resume file and `devflow ship` are removed (cut list:
`.planning/user/V4-CUT-INVENTORY.md`); the supervise gate loop is fixed; an MCP `devflow_ask` tool with an "operator via
gate" answer path lands; a calibrated decision model then triages the questions; failover is generalized to any stage
with a rotation. Phase numbering continues at 52 (Phase 51 closed v3.0.0 and ships inside 4.0.0; 3.1.0 skipped).
**Major version** (4.0.0, operator 2026-11-08): the stage set and the CLI surface shrink, and `devflow-core` is a
published crate, so the removed `pub` items are a semver-major change.

**Sequencing.** The subtraction comes first and every later phase is smaller once it has landed: fewer stages to
prompt (ASK-03), no Ship gate to redefine `supervise` around (ASK-02, GATE-01), no cron arm beside the rate-limit gate
(FAIL-01). Phase 54 cannot start until Phase 53 has recorded real GSD discuss questions through the channel: the
Jev-vs-Laya bake-off (TRIAGE-01) scores those recordings, not a synthetic corpus. Phase 55 depends only on Phase 52
(the cron arm must be gone so an exhausted rotation ends at CUT-03's gate) and is sequenced last by the operator's
shape, not by a dependency on 53 or 54. Research was skipped (operator, 2026-11-08): the two unknowns, how each agent
consumes an MCP tool and Jev vs Laya, are spikes in the Phase 53 and Phase 54 discuss steps.

**Success is measured on a project that is not DevFlow:** a month of runs with no by-hand exception. Remote operator
replies (Telegram and similar) are a later milestone (REMOTE-01/02 under `REQUIREMENTS.md` § Future Requirements).

| Phase | Name | Requirements | Depends on | Status |
|---|---|---|---|---|
| 52 | Subtract and Stabilize | CUT-01, CUT-02, CUT-03, CUT-04, GATE-01, GATE-02, GATE-03, GATE-04, OPS-01, OPS-02, OPS-03, OPS-04 | — (Phase 51, v3.0.0, merged) | Executed — verification stale |
| 52.1 | Stabilize for Dogfooding (INSERTED) | backlog 999.221, 999.222, 999.223, 999.232, 999.224, 999.219, 999.229, 999.230, 999.231, 999.167 (no REQ IDs; plans bind to D-01..D-27) | 52 | Complete    |
| 52.2 | Foreground Runner (INSERTED) | none (carries 52.1's D-11, D-09.1, D-09.5, D-20, D-24, D-26, D-27, D-08, D-15) | 52.1 | Complete    |
| 53 | The Question Channel | ASK-01, ASK-02, ASK-03, ASK-04, ASK-05 | 52.2 (and through it 52.1 and 52 — the stage set must be final) | Complete    |
| 54 | Decision Triage | TRIAGE-01, TRIAGE-02, TRIAGE-03, TRIAGE-04 | 53 (its recorded questions are the bake-off corpus) | Not started |
| 55 | Failover Generalized | FAIL-01, FAIL-02, FAIL-03, FAIL-04, FAIL-05, FAIL-06 | 52 (independent of 53-54; sequenced last) | Not started |

### Phase 52: Subtract and Stabilize

**Goal**: A run is Plan → Code → Validate and ends there as finished — no Define, no Ship, no after-ship hooks, no
cron-resume file, no `devflow ship`, none of the confirmed cut rows — on a tree where `scripts/check.sh all` is green;
the supervise gate loop carries the operator's rejection note to the relaunched agent verbatim and can drive a
`gaps_found` verification to `passed` from inside the run; and `--version`, `cleanup`, `status`, `doctor` and the
command output tell the operator what the run did and why it stopped.
**Depends on**: Nothing in this milestone (first phase). Builds on Phase 51 (v3.0.0, merged to `develop` via PR #234).
**Sequencing note**: The subtraction lands first inside the phase, as its own plan wave, before any GATE or OPS fix:
every later fix is smaller on the cut tree (no Ship gate to redefine `supervise` around, no cron arm beside the
rate-limit gate, no Define prompt to carry a reject note). The Phase 52 discuss step confirms which of the inventory's
21 proposed `cut` rows join the 7 operator-decided ones (CUT-04). CUT-02 and GATE-02/GATE-03 are designed together
because both decide what happens at the Validate gate once it is the last gate of the run. Kept as one phase rather
than split: no requirement dependency forces a split, and the Validate-gate design is the seam the two halves share.
**Requirements**: CUT-01, CUT-02, CUT-03, CUT-04, GATE-01, GATE-02, GATE-03, GATE-04, OPS-01, OPS-02, OPS-03, OPS-04
**Success Criteria** (what must be TRUE):

  1. `devflow start --dry-run --phase N` lists Plan → Code → Validate and nothing else — no Define, no Ship, no
     after-ship hook — and a run that completes Validate records `workflow_finished` with `shipped: false` on
     `events.jsonl`, reports itself finished, and leaves the worktree and branch in place for the operator. The
     removed surface is gone, not hidden: `devflow ship`, `--yes-ship`, `DEVFLOW_YES_SHIP`, `parallel` and every
     other cut row confirmed in the discuss step are rejected by clap as unknown and absent from `--help`.
     **Control:** the same dry run on the installed 3.0.0 release binary lists Define and Ship (recorded once in the
     phase notes), and the e2e test asserts the exact stage list, so an empty dry run cannot pass. (CUT-01, CUT-02,
     CUT-04)
  2. `scripts/check.sh all` — fmt, clippy, the workspace tests and `doc_check` — passes on the cut tree, with the cut
     rows' tests, OPERATIONS.md rows, README/ARCHITECTURE/CONTRIBUTING mentions and `doc-check-allowlist.toml`
     entries removed rather than allowlisted around; a source-scan Rust test asserts that no literal of the removed
     surface (`cron-instructions`, `hermes_cron`, `yes_ship`, the four after-ship hook names, the removed `Stage`
     variants) remains under `crates/`, `docs/` or the top-level docs, comments stripped. `CHANGELOG.md` (release
     history, which records the removal), `.planning/` and the operator-owned `CLAUDE.md` are outside the scan. **Control:** the same test
     fails on a planted stray literal, and the plant is removed before the phase closes. (CUT-03, CUT-04; scan scope
     amended 2026-11-09, 52-CONTEXT D-40)
  3. A rate limit with no usable fallback waits at a gate whose context names the reset time and the exact
     `devflow resume --phase N ...` command to run after it. Once the reset time passes, the supervisor relaunches the
     stage by itself, at most once: after a supervisor crash past the timer's claim, the operator runs the gate's
     resume command (amended 2026-11-09, 52-CONTEXT D-45 rule 5). A denial with no reset time, or a second denial, leaves the gate open for the
     operator. No `cron-instructions-NN.json` exists under `.devflow/` afterwards. **Controls:** the Phase 51 fixture
     that asserted the cron file was written now asserts its absence and the gate's presence — one fixture, both
     directions. A denial whose reset time has passed is relaunched once, and a second denial in the same fixture is
     not relaunched again. (CUT-03; amended 2026-11-08, 52-CONTEXT D-15..D-17)
  4. `devflow gate reject <phase> --note "<text>"` relaunches the agent with `<text>` verbatim in its prompt — visible
     in the launch record or captured prompt, not inferred — and a note containing the word "abort" does **not**
     abort the run; only an explicit abort answer does. A Validate rejection relaunches only the fix work: GSD's gaps
     route for a `gaps_found` verdict, `/gsd-quick` with the note for a rejected clean pass (amended 2026-11-08,
     52-CONTEXT D-21); never the whole phase; a verification that reports `gaps_found` is driven to `passed` inside the same run
     (the relaunched agent is told to plan and close the gaps and re-verify), and the Validate gate never reads
     "passed" while the report says otherwise; a run finishes only when GSD reports `passed` and the Validate agent
     passed on the current commit (amended 2026-11-09, 52-CONTEXT D-45). `devflow gate show` prints a failure gate's full reason untruncated,
     and the banner and `gate list` cut keep the "way on" sentence first. **Controls:** the pre-fix prompt path, kept
     as a fixture, fails the verbatim assertion; a `gaps_found` report with the gate forced to "passed" fails the
     never-lies assertion; a note reading "do not abort, fix the test" leaves the run alive; a failure reason longer than 300 characters
     reaches `gate show` whole (the cap is applied at write time, 52-CONTEXT); three gap rounds with no progress force
     the gate (52-CONTEXT D-25). (GATE-01, GATE-02,
     GATE-03, GATE-04)
  5. `devflow --version` prints the version with the embedded build commit and dirty flag; `devflow cleanup` says why
     it keeps a worktree (the recorded stop reason, an `--until` halt, or a live run) and `cleanup --force` removes the
     run's state together with the worktree; `devflow status` never reports a run whose worktree is gone as active,
     shows a stopped run's stop reason and the fallback agent's state (armed, spent, none), and `devflow start` says
     when it replaces a stale state; command output carries no `tracing` lines unless `RUST_LOG` is set, `doctor`
     reports an unset `RUST_LOG` as information, and a project that is not DevFlow's own gets no build-provenance
     warning and one line, not two, for a missing remote. **Controls:** with `RUST_LOG=debug` the tracing lines do
     appear; on DevFlow's own checkout the provenance warning still fires; a run whose worktree is present still
     reports as active. (OPS-01, OPS-02, OPS-03, OPS-04)

**Plans**: 33/33 plans complete
- [x] 52-01-PLAN.md
- [x] 52-02-PLAN.md
- [x] 52-03-PLAN.md
- [x] 52-04-PLAN.md
- [x] 52-05-PLAN.md
- [x] 52-06-PLAN.md
- [x] 52-07-PLAN.md
- [x] 52-08-PLAN.md
- [x] 52-09-PLAN.md
- [x] 52-10-PLAN.md
- [x] 52-11-PLAN.md
- [x] 52-12-PLAN.md
- [x] 52-13-PLAN.md
- [x] 52-14-PLAN.md
- [x] 52-15-PLAN.md
- [x] 52-16-PLAN.md
- [x] 52-17-PLAN.md
- [x] 52-18-PLAN.md
- [x] 52-19-PLAN.md
- [x] 52-20-PLAN.md
- [x] 52-21-PLAN.md
- [x] 52-22-PLAN.md
- [x] 52-23-PLAN.md
- [x] 52-24-PLAN.md
- [x] 52-25-PLAN.md
- [x] 52-26-PLAN.md
- [x] 52-27-PLAN.md
- [x] 52-28-PLAN.md
- [x] 52-29-PLAN.md
- [x] 52-30-PLAN.md
- [x] 52-31-PLAN.md
- [x] 52-32-PLAN.md
- [x] 52-33-PLAN.md
**UI hint**: no

### Phase 52.1: Stabilize for Dogfooding (INSERTED)

**Goal:** Make DevFlow stable enough to resume dogfooding: a supervised run can drive a phase from Plan to a finished
Validate without hand-run stages or hand-edited state. Not dogfooded itself (operator, 2026-11-11).
**Candidate scope (agent recommendation, for discuss to settle; not an operator decision):** the test flakes 999.221,
999.222, 999.223 and 999.232; the `--force` redesign 999.224; the close-out script defects 999.230 and 999.231.
**Requirements**: backlog items 999.221, 999.222, 999.223, 999.232 (test flakes), 999.224 (`--force` redesign),
999.219, 999.229, 999.230, 999.231 and 999.167 (close-out scripts; 999.167 folds into D-12). This phase carries no REQ
IDs from `REQUIREMENTS.md` — its plans bind to its own decisions, D-01..D-27 in `52.1-CONTEXT.md`.
**Depends on:** Phase 52
**Closed at plan 07 (operator, 2026-11-15):** the foreground runner replaces the detached supervisor, so plans 08
to 15 moved to Phase 52.2 for replanning, and the goal's proof run moves with them. Delivered here: the test-flake
fixes, truthful launch lines, the close-out script fixes, the proof-run fixture, the push-time divergence check and
`discard`.
**Plans:** 7/7 plans complete

Plans:
**Wave 1**
- [x] 52.1-01-PLAN.md
- [x] 52.1-02-PLAN.md
- [x] 52.1-03-PLAN.md
- [x] 52.1-05-PLAN.md

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 52.1-04-PLAN.md
- [x] 52.1-06-PLAN.md

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 52.1-07-PLAN.md

### Phase 52.2: Foreground Runner (INSERTED)

**Goal:** Each stage runs in the foreground of the command that started it, and the detached supervisor and the
liveness machinery that exists only for it are removed; a long run is started as a user service
(`systemd-run --user`), which is what covers a hard kill of the runner (operator decisions, 2026-11-15). Then the
decisions carried from Phase 52.1 are built on the simpler runner, and the phase ends with 52.1's proof: a
supervised run drives a phase from Plan to a finished Validate without hand-run stages or hand-edited state.
**Requirements**: no REQ IDs. Carried from Phase 52.1 (its plans 09 to 15, kept for reference in
`carried-from-52.1/`, to be replanned, not executed as written): D-11, D-09.1, D-09.5, D-20, D-24, D-26, D-27,
D-08 and D-15 in `52.1-CONTEXT.md`. Plan 08 (D-21, D-25, D-09.3: an exit for an unprobeable or unanswering
supervisor) has no object once the supervisor is gone; 999.223, which plan 08 also carried, closes with the
supervisor's removal. Also owned here: 999.237, `discard` and `start` refusing over a live run. Evidence and spike results: `.planning/audits/2026-11-15-new-direction-brief.md`.
**Depends on:** Phase 52.1
**Plans:** 26/26 plans complete

Plans:
**Wave 1**
- [x] 52.2-01-PLAN.md
- [x] 52.2-20-PLAN.md

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 52.2-02-PLAN.md
- [x] 52.2-03-PLAN.md
- [x] 52.2-21-PLAN.md

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 52.2-04-PLAN.md
- [x] 52.2-05-PLAN.md

**Wave 4** *(blocked on Wave 3 completion)*
- [x] 52.2-06-PLAN.md

**Wave 5** *(blocked on Wave 4 completion)*
- [x] 52.2-07-PLAN.md
- [x] 52.2-08-PLAN.md
- [x] 52.2-22-PLAN.md

**Wave 6** *(blocked on Wave 5 completion)*
- [x] 52.2-09-PLAN.md

**Wave 7** *(blocked on Wave 6 completion)*
- [x] 52.2-10-PLAN.md

**Wave 8** *(blocked on Wave 7 completion)*
- [x] 52.2-11-PLAN.md

**Wave 9** *(blocked on Wave 8 completion)*
- [x] 52.2-26-PLAN.md

**Wave 10** *(blocked on Wave 9 completion)*
- [x] 52.2-12-PLAN.md

**Wave 11** *(blocked on Wave 10 completion)*
- [x] 52.2-23-PLAN.md

**Wave 12** *(blocked on Wave 11 completion)*
- [x] 52.2-25-PLAN.md

**Wave 13** *(blocked on Wave 12 completion)*
- [x] 52.2-24-PLAN.md

**Wave 14** *(blocked on Wave 13 completion)*
- [x] 52.2-13-PLAN.md

**Wave 15** *(blocked on Wave 14 completion)*
- [x] 52.2-14-PLAN.md

**Wave 16** *(blocked on Wave 15 completion)*
- [x] 52.2-15-PLAN.md

**Wave 17** *(blocked on Wave 16 completion)*
- [x] 52.2-16-PLAN.md

**Wave 18** *(blocked on Wave 17 completion)*
- [x] 52.2-17-PLAN.md

**Wave 19** *(blocked on Wave 18 completion)*
- [x] 52.2-18-PLAN.md

**Wave 20** *(blocked on Wave 19 completion)*
- [x] 52.2-19-PLAN.md

### Phase 53: The Question Channel

**Goal**: An agent running under DevFlow asks DevFlow instead of a human. DevFlow serves an MCP tool, `devflow_ask`,
over stdio; a call carries a question, typed options and context, blocks until an answer exists, and returns it to the
agent in the same session. The one policy this phase ships, "operator via gate", turns each question into a gate the
operator answers from the shell, with the notify hook firing as for any gate. Every stage prompt sends decision
checkpoints, `blocking-human` gates and GSD discuss questions through the tool, and every question and answer is on
the event log.
**Depends on**: Phase 52.2 (and through it Phases 52.1 and 52) — the stage set must be final before every stage prompt is rewritten (ASK-03), and the
checkpoint auto-decide relaunch that ASK-05 replaces is only worth replacing on the cut tree. Phase 52.2 must make DevFlow
stable enough to dogfood this phase (operator, 2026-11-11).
**Sequencing note**: The first unknown of the milestone lives here: how each supported agent consumes an MCP tool
(stdio server registration, tool discovery, blocking-call semantics). It is a spike in this phase's discuss step
(operator, 2026-11-08), not research done up front. Only the agents the rotation needs are proven live; the rest are
covered by the driver contract's conformance tests. The questions this phase records through ASK-04 are the corpus
Phase 54's bake-off scores, so a run on a project that is not DevFlow, through real GSD discuss questions, is part of
closing this phase.
**Requirements**: ASK-01, ASK-02, ASK-03, ASK-04, ASK-05
**Success Criteria** (what must be TRUE):

  1. An agent launched by DevFlow calls `devflow_ask` over stdio with a question, typed options and context; the call
     blocks until an answer exists and the answer comes back to the same agent session — proven live for claude and
     codex (53-PROOF-RUN.md), for pi by its registration spike against a stub MCP server (53-MCP-SPIKE.md, pi
     addendum), and by e2e tests that drive the real binary with stub claude, codex and pi agents; opencode, hermes
     and antigravity launches are refused by name until each has a registration (999.248), so `--mode auto`, which
     accepts only claude or antigravity as a fallback, has no usable non-claude fallback until then. **Control:** a
     call with a malformed payload (no options, or an option id the question did not offer) is refused with a typed
     error and the agent is not left blocked. (ASK-01)
  2. Under "operator via gate", an asked question appears in `devflow gate list` and `gate show` with its options;
     `devflow gate answer <phase> --gate-id <id> --option <id>` (or a free-text answer) resolves it; the notify hook
     fires and `notify_fired` is recorded exactly as for an approve/reject gate. **Control:** an answer addressed to a
     gate id other than the open question's leaves the question open — one answer decides only the gate it names (the
     v3.0.0 invariant, kept). (ASK-02)
  3. Every stage prompt (Plan, Code, Validate) tells the agent to call the tool for decision checkpoints,
     `blocking-human` gates and GSD discuss questions instead of self-resolving them positionally or stopping with a
     report, and a `blocking-human` checkpoint reached under a one-shot launch is answered through the channel with
     the stage continuing in the same session — no relaunch, no report-and-stop. **Controls:** a source-scan test
     asserts the positional self-resolution instruction is gone from every prompt template and trips on a planted
     copy; the Phase 49 checkpoint fixture that ended the stage now ends with the stage continued. (ASK-03, ASK-05;
     closes 999.57, 999.127)
  4. Every question and its answer are on `events.jsonl` with the answering policy or operator and the time, and
     `devflow history` shows them in order with the rest of the run. **Control:** a question still open has no answer
     record — the history view distinguishes "not yet answered" from an empty answer, and a run that asked nothing
     shows no question rows. (ASK-04)

**Plans**: 14/14 plans complete
**Wave 1**
- [x] 53-01-PLAN.md
- [x] 53-02-PLAN.md
- [x] 53-03-PLAN.md
- [x] 53-12-PLAN.md

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 53-04-PLAN.md
- [x] 53-05-PLAN.md
- [x] 53-06-PLAN.md

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 53-07-PLAN.md

**Wave 4** *(blocked on Wave 3 completion)*
- [x] 53-08-PLAN.md

**Wave 5** *(blocked on Wave 4 completion)*
- [x] 53-09-PLAN.md

**Wave 6** *(blocked on Wave 5 completion)*
- [x] 53-10-PLAN.md

**Wave 7** *(blocked on Wave 6 completion)*
- [x] 53-11-PLAN.md
- [x] 53-13-PLAN.md

**Wave 8** *(blocked on Wave 7 completion)*
- [x] 53-14-PLAN.md
**UI hint**: no

### Phase 53.1: Phase 53 remediation (INSERTED)

**Goal:** Fix the DevFlow defects found while dogfooding Phase 53 and in the PR #245 review, plus the backlog items earmarked for this phase (operator, 2026-11-19), so the next dogfooded phase runs without the workarounds Phase 53 needed. In scope: 999.250, 999.251, 999.252, 999.253, 999.254, 999.255, 999.257, 999.258 (from the Phase 53 run and review), and 999.240, 999.249, 999.219, 999.213, 999.206, 999.202, 999.201, 999.193, 999.189 (earmarked backlog). Some of the earmarked items may already be fixed by Phases 52–53; discuss checks each one against the current code and drops or narrows those, with the evidence.
**Requirements**: TBD (one per in-scope 999.x item, set in discuss)
**Depends on:** Phase 53
**Plans:** 8/8 plans complete

Plans:
**Wave 1**
- [x] 53.1-01-PLAN.md — stream `error_rate_limit` classified RateLimited (999.201); close the four fixed entries, 999.252 and 999.206 notes (wave 1)
- [x] 53.1-02-PLAN.md — rate-limit gate: reset as UTC time and distance, far reset leads with stop then `resume --agent` (999.250) (wave 1)
- [x] 53.1-03-PLAN.md — resume after `--until` starts the next stage (999.251); late cancel and pi comment (999.258) (wave 1)
- [x] 53.1-04-PLAN.md — cleanup of a stopped run stops contradicting itself (999.213); unparsable question gate id warns (999.258) (wave 1)
- [x] 53.1-05-PLAN.md — post-commit marker out of the tree (999.240/253), live-payload CONTEXT.md (999.255), atomic notify marker (999.257) (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 53.1-06-PLAN.md — stage-failure decisions note (999.249), capture-file suffix and gate show pin (999.206), post-run sign-off route (999.254) (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 53.1-07-PLAN.md — finish runs GSD's phase.complete and commit (999.259); OPERATIONS docs on how a run ends (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*
- [x] 53.1-08-PLAN.md — close the D-02 entries (999.206, 999.213, 999.251), file the resume residual, resume row, phase gate (wave 4)

### Phase 53.2: wsx/UDE first-use and dogfood remediation (INSERTED)

**Goal:** Fix what DevFlow's first use on other projects (wsx phase 7, universal-dev-env phase 3) and its own v4
dogfood runs exposed, and stabilize the flaky tests, so Phase 54 can run discuss on a project that is not DevFlow.
Scope (operator-approved 2026-11-22; GitHub issues on denniyahh/devflow):
- wsx/UDE first use: #246 (no route runs `/gsd-discuss-phase`; blocks Phase 54), #250 (Plan has no `--reviews`
  route), #248 (status compares against the env/config base, not the run's `base_branch`), #247 (question-gate
  banner offers approve/reject), #249 (status prints a raw Unix timestamp)
- Dogfood defects: #281 (resume after an `--until plan` halt via Plan→Validate starts Code), #324 (stop reason
  names the removed supervisor), #285 (`start` should warn when GSD's derived files are not ignored), #258
  (timed-out launch clears a run), #297 (`stop` vs concurrent `resume` reports "crashed"), #296 (fake service
  manager never reports `active`)
- Flaky tests: #265, #273, #276, #277, #279, #301, #308, #310, #178. #273 (999.257) was claimed fixed by 53.1-05:
  confirm first.
- Out: 999.237 (fixed) and 999.233 (obsolete), per `.planning/audits/2026-11-22-backlog-migration.md`.
Run conventions: discuss `--auto` in session, then `devflow start --phase 53.2 --agent claude --mode supervise
--until plan`, external review (Tier B), close-out with `scripts/cut-pr-branch.sh`.
**Requirements**: no REQ IDs; the issues above
**Depends on:** Phase 53.1
**Plans:** 9/17 plans executed

Plans:
**Wave 1**
- [x] 53.2-01-PLAN.md — #246 Plan prompt runs discuss first when the phase has no CONTEXT.md (wave 1)
- [x] 53.2-02-PLAN.md — #276 core stub writers through write_executable; #265 measured under load (wave 1)
- [x] 53.2-03-PLAN.md — #276 CLI src stubs and shared e2e fakes through write_executable (wave 1)
- [x] 53.2-04-PLAN.md — #276 remaining CLI integration-test stubs (wave 1)
- [x] 53.2-05-PLAN.md — #247 question-gate banner; #248 open branches against the run's base (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 53.2-06-PLAN.md — #296 fake systemctl reads active; held-lock readiness (wave 2)
- [x] 53.2-07-PLAN.md — #276 raw-writer guard scan; #277 bounded write-lock read (wave 2)
- [x] 53.2-08-PLAN.md — #301 one CI-aware poll budget; #279 wait for inactive (wave 2)
- [x] 53.2-09-PLAN.md — #249 ISO-8601 start time; #310 reproduce then bounded reads (wave 2)
- [ ] 53.2-10-PLAN.md — #250 StageIntent::PlanReviews and the planned semver break (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*
- [ ] 53.2-11-PLAN.md — #178 measure the leak; close #273 and #308 with evidence (wave 3)
- [ ] 53.2-12-PLAN.md — #285 start warns on unignored GSD files; #258 keep a progressed run (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*
- [ ] 53.2-13-PLAN.md — #297 stop holds the phase holder; #324 no supervisor in texts (wave 4)

**Wave 5** *(blocked on Wave 4 completion)*
- [ ] 53.2-14-PLAN.md — #281 (999.260) resume follows the until halt's target (wave 5)

**Wave 6** *(blocked on Wave 5 completion)*
- [ ] 53.2-15-PLAN.md — #250 start --reviews flag, refusal, dry run, resume (wave 6)

**Wave 7** *(blocked on Wave 6 completion)*
- [ ] 53.2-17-PLAN.md — #250 resume --reviews reruns a halted Plan as a reviews pass (wave 7)

**Wave 8** *(blocked on Wave 7 completion)*
- [ ] 53.2-16-PLAN.md — D-03 live proof of the discuss route, run by the orchestrating session (wave 8)

**Cross-cutting constraints:** <!-- gsd:generated -->
- D-17: each target test fails on the code before its GREEN commit with its assertion text, checked once by reverting the GREEN change

### Phase 54: Decision Triage

**Goal**: DevFlow answers the questions a calibrated model can answer and escalates the rest. A decision model (Jev or
Laya, chosen by a bake-off on the real GSD discuss questions Phase 53 recorded) scores each question's candidate
answers; above the configured threshold — or on a `route` of `auto` — DevFlow answers, otherwise the question goes to
the operator policy. Every decision, decided or escalated, is in a ledger the operator can list and override, and a
missing or failing model never auto-decides.
**Depends on**: Phase 53 — the channel is the only way a question reaches triage, and the bake-off corpus is the
questions ASK-04 recorded. No corpus, no bake-off: the phase does not start until Phase 53's live run has recorded real
discuss questions.
**Sequencing note**: The second unknown of the milestone, Jev vs Laya, is a spike in this phase's discuss step
(operator, 2026-11-08). The bake-off is scored on recordings, so the model choice is a measured result, not a
preference; its record (corpus size, agreement with the operator's answers, calibration) is a phase artifact. The
`typesafe-ai` skill is the reference for the System One primitives (`choice`, `score`, `noul`).
**Requirements**: TRIAGE-01, TRIAGE-02, TRIAGE-03, TRIAGE-04
**Success Criteria** (what must be TRUE):

  1. The bake-off record names the chosen model and the corpus it was scored on (Phase 53's recorded questions, with
     the operator's answers as the reference); in a run, a question whose top candidate scores above the configured
     threshold, or whose `route` is `auto`, is answered by DevFlow without a gate, and one below the threshold goes to
     the operator policy and appears as a gate. **Control:** the same question with the threshold set above any
     attainable probability always reaches the operator; with `route: auto` it never does. (TRIAGE-01)
  2. Every triaged decision is in the ledger with the question, the options, the chosen answer, the probability or
     route, and the policy that decided; the ledger lists decided and escalated entries separately. **Control:** an
     escalated question is listed as escalated with no chosen answer — absent, not an empty string — until the
     operator answers it. (TRIAGE-02, TRIAGE-03)
  3. The operator overrides one ledger decision from the shell, and on the agent's next launch the question is
     re-opened to it with the overridden answer; the ledger shows both the original decision and the override.
     **Control:** an override naming a decision that is not in the ledger is refused and changes nothing. (TRIAGE-03)
  4. With the model unavailable or failing (binary or endpoint missing, call erroring, response unparseable), every
     question goes to the operator policy, the run says so on the event log and in `status`, and nothing is
     auto-decided. **Control:** the same fixture with the model available decides the above-threshold questions, so
     the fallback is the model's absence, not triage switched off. (TRIAGE-04)

**Plans**: TBD
**UI hint**: no

### Phase 55: Failover Generalized

**Goal**: A quota denial at any stage — or rate-limit telemetry crossing a configured threshold before a denial —
hands the run to the next agent in a configured rotation, as many times as the rotation allows; the agent taking over
is told whose work it inherits and why; and the capture classifier never mistakes a torn line for a rate limit. Phase
51's single hop at Code (R-2) becomes the degenerate case of a rotation of length one.
**Depends on**: Phase 52 — the cron arm must be gone so an exhausted rotation ends at CUT-03's gate (reset time and
resume command), and Phase 51's failover (v3.0.0) is the base. Independent of Phases 53 and 54: nothing here needs
the question channel or the ledger. Sequenced last by the operator's shape.
**Sequencing note**: FAIL-04 and FAIL-05 are classifier and fixture work with no dependency on the rotation; they can
be the phase's first wave while the rotation and routing are designed. The live evidence to beat: one probe with a
synthetic denial (claude → codex, 2026-11-08) and no surviving raw capture of a real denial (999.212).
**Requirements**: FAIL-01, FAIL-02, FAIL-03, FAIL-04, FAIL-05, FAIL-06
**Success Criteria** (what must be TRUE):

  1. With a rotation configured as an ordered list (for example `claude, codex, antigravity`), a quota denial at
     Plan, Code or Validate hands the run to the next agent in the same worktree on the same branch, a second denial
     hands it again, and exhausting the rotation ends at CUT-03's gate, which names the reset time and the resume
     command and relaunches once after the reset (amended 2026-11-08, 52-CONTEXT D-15..D-17).
     **Control:** with no rotation configured the first denial ends at that gate; Phase 51's single-hop test still
     passes as a rotation of length one. (FAIL-01; supersedes Phase 51's R-2)
  2. When the current agent's rate-limit telemetry (`utilization`, `unifiedWindows`) crosses the configured threshold
     during a stage, the next stage launches on the next agent in the rotation before any denial, and the event log
     records the handoff with utilization, not denial, as its reason. **Control:** telemetry below the threshold keeps
     the next stage on the same agent. (FAIL-02)
  3. The relaunched agent's prompt names the agent it is taking over from, why (denial, utilization, canary refusal),
     and that the tree may hold that agent's uncommitted work which must not be discarded — verbatim in the captured
     prompt. **Control:** a first launch with no handoff carries none of that text. (FAIL-03; closes 999.215)
  4. Both torn-line orders classify as torn, never `RateLimited`: a partial line read only at EOF after the idle
     check, and a partial line that never reaches the capture because an escaped process holds stdout — each with its
     own named test. **Control:** each test has a paired case where the same line arrives complete and *does*
     classify as `RateLimited`. (FAIL-04; closes 999.207, 999.209)
  5. The first non-`allowed` `rate_limit_event` seen in a stream capture is archived under `.devflow/history/`, and
     the denial fixtures match the live event shape (`utilization`, `unifiedWindows`); a delivery canary refusal can
     be re-run for the same agent, and `devflow resume --agent <same>` says what it did. **Controls:** an `allowed`
     event is not archived; the fixtures are checked against the recorded live event's field set, so a fixture in the
     old shape fails; a second canary refusal for the same agent ends at a gate again, not in a loop. (FAIL-05,
     FAIL-06; closes 999.212, 999.194)

**Plans**: TBD
**UI hint**: no

## ✅ v2.8.0 milestone (CLOSED 2026-09-03 — Remaining Harness Support + Pi Dogfood)

**Declared 2026-08-18, closed 2026-09-03.** Onboarded the last coding harnesses onto the modular
`AgentDriver` contract and proved the pipeline against all six. **Antigravity CLI** and **Hermes**
got new `AgentKind` variants + drivers + conformance enrollment (Phases 41-42); the **OpenCode**
stub driver was completed (28 → 569 lines: real launch argv, `parse_opencode_event_result` against
three live captures, fail-closed health/capability probes — Phase 43); **Codex** was verified
end-to-end through a real `devflow resume --agent codex` dogfood with all surfaced gaps and 7
post-hoc review findings closed (Phase 44, shipped v2.11.0); **Pi** (shipped v2.7.0) was dogfooded
through a real supervised run with a live gate, and the two 999.85 stale comments rewritten
(Phase 40, MAINT-01). Phase 45 (added mid-milestone) hardened unattended `--mode auto`: configurable
base-branch resolution so worktrees fork from the `.planning/`-tracking branch (AUTO-01 / 999.110),
`affects_compiled_binary` scoped to Cargo workspace members (AUTO-02 / 999.109), and a shared
merit-based Code-stage decision policy (DECN-01 / 999.94), shipped v2.12.0.

Phases 40-44 released incrementally as **v2.9.0 … v2.11.0**; Phase 45 as **v2.12.0**.

**Closed as `override_closeout`.** All six phases (40-45) verified/passed; pre-close artifact audit
clear (9 items still suppressed from a prior close). **Known gap:** DECN-01 is partially delivered —
`CODE_STAGE_POLICY` reaches the first Code pass for both agent families but not the Claude/OpenCode
`fix_prompt` loop-back, and is contradicted by `checkpoint_auto_decide_prompt` for blocking-human
gates. Both holes deferred by operator decision to backlog **999.115** / **999.116**; AUTO-01's live
`--mode auto` end-to-end run deferred to **999.119**. Two audit integration warnings filed as
**999.120** / **999.121**. Milestone audit (`status: tech_debt`) and full phase detail archived to
`.planning/milestones/v2.8.0-MILESTONE-AUDIT.md`, `.planning/milestones/v2.8.0-ROADMAP.md`;
requirements to `.planning/milestones/v2.8.0-REQUIREMENTS.md`; phase dirs to
`.planning/milestones/v2.8.0-phases/`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 40 | Pi Dogfood (PIDG-01, MAINT-01) | Complete | v2.9.0 |
| 41 | Antigravity Driver (ANTG-01/02/03, HYG-01/02) | Complete | — |
| 42 | Hermes Driver (HRMS-01/02/03, ANTG-04) | Complete | — |
| 43 | OpenCode Driver Completion (OPCD-01/02/03) | Complete | — |
| 44 | Codex End-to-End Verification (CODE-01) | Complete | v2.11.0 |
| 45 | Unattended Auto-Mode Hardening (AUTO-01, AUTO-02, DECN-01◐) | Complete | v2.12.0 |

---

## ✅ v2.7.0 milestone (CLOSED 2026-08-18 — Pi End-to-End + Driver Contract Completion)

**Declared 2026-08-16, closed 2026-08-18.** Finished the `AgentDriver` migration and made **Pi**
run end-to-end. Phase 37.1 (research spike) returned a **VIABLE** verdict for `@bacnh85/pi-subagent`;
Phase 38 removed `AgentAdapter` and wired `InteractivityMode` plus the two 999.107 fixes; Phase 39
landed the Pi pipeline (provider-aware health, `Legacy` launch, subagent dispatch). Full phase detail
archived to `.planning/milestones/v2.7.0-ROADMAP.md`; phase directories to
`.planning/milestones/v2.7.0-phases/`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 37.1 | Pi Subagent-Extension Spike (research) | Complete | — |
| 38 | Driver Contract Completion (999.106 + 999.107) | Complete | — |
| 39 | Pi End-to-End | Complete    | — |

---

## ✅ v2.6.0 milestone (CLOSED 2026-08-16 — Multi-Agent Adapter Migration)

**Declared 2026-08-15, closed 2026-08-16.** Replaced the thin `AgentAdapter` trait with the modular
`AgentDriver` contract and onboarded **Pi** as the first newly-supported agent — Phase 36 (Pi
adapter + release hardening) and Phase 37 (`AgentDriver`/`StageIntent` migration: Codex fixed, Pi
as the second native driver, conformance suite). Full phase detail archived to
`.planning/milestones/v2.6.0-ROADMAP.md`; phase directories to `.planning/milestones/v2.6.0-phases/`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 36 | Pi Adapter Registration + Release Signing | Complete    | — |
| 37 | Modular Agent Driver Architecture + Pi Driver (999.31 + Pi) | Complete    | — |

---

## v2.5.0 milestone (CLOSED 2026-08-15 — Loop-Termination and Release Hardening)

**Declared 2026-08-06, closed 2026-08-15.** Closed five confirmed loop-termination / release
defects and one concurrency-blind-spot investigation (999.77, 999.78, 999.79, 999.83, 999.84,
999.86, 999.87). Full phase detail archived to `.planning/milestones/v2.5.0-ROADMAP.md`;
requirements to `.planning/milestones/v2.5.0-REQUIREMENTS.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 35 | Loop-Termination and Baseline Correctness | Complete (2026-08-07) | — |
| 35.1 | Unattended-Launch Prerequisites | Complete | — |
| 35.2 | Verification Provenance | Complete | — |
| 35.3 | Drain Gate Concurrency Measurement | Complete | — |

## Progress

**Scope note:** this table is global (spans every shipped milestone plus the active one), per
`deriveProgressFromRoadmap`'s own design (it reads the whole file, not a milestone-scoped
slice). Phases 26 (closed partial, never shipped) and 29 (aborted, never merged) are excluded —
neither belongs to any shipped milestone (see `PROJECT.md` § "Active" and
`.planning/superseded/26-release-cut-automation/`). `999.x` backlog entries are excluded per
`deriveProgressFromRoadmap`'s own `/^999(?:\.|$)/` filter. `Plans Complete` reads "—" where this
table's author did not have a directly-verified per-phase count on hand at authoring time
(2026-08-04); those cells don't affect the derived phase-completion counts this Progress table
exists to fix, only the (unused-by-HYGIENE-03) plans-total figure.

| Phase | Plans Complete | Status | Completed |
|---|---|---|---|
| 1 | 1/1 | Complete | — |
| 2 | 1/1 | Complete | — |
| 3 | 1/1 | Complete | — |
| 4 | 1/1 | Complete | — |
| 5 | 1/1 | Complete | — |
| 6 | 1/1 | Complete | — |
| 7 | 1/1 | Complete | — |
| 8 | 1/1 | Complete | — |
| 9 | 1/1 | Complete | — |
| 10 | 1/1 | Complete | — |
| 11 | 2/2 | Complete | — |
| 12 | — | Complete | — |
| 13 | — | Complete | — |
| 14 | — | Complete | — |
| 15 | — | Complete | — |
| 16 | — | Complete | — |
| 17 | — | Complete | — |
| 18 | 7/7 | Complete | — |
| 19 | 11/11 | Complete | — |
| 20 | 5/5 | Complete | — |
| 21 | 4/4 | Complete | — |
| 22 | 2/2 | Complete | — |
| 23 | — | Complete | — |
| 24 | — | Complete | — |
| 25 | 18/19 | Complete | — |
| 27 | 6/6 | Complete | — |
| 28 | — | Complete | — |
| 30 | 5/5 | Complete | — |
| 31 | 5/5 | Complete | — |
| 32 | 0/0 | Complete    | 2026-08-04 |
| 33 | 6/6 | Complete    | 2026-08-05 |
| 34 | 6/6 | Complete    | 2026-08-06 |
| 35 | 6/6 | Complete    | 2026-08-07 |
| 35.1 | 4/4 | Complete    | 2026-08-12 |
| 35.2 | 2/2 | Complete    | 2026-08-12 |
| 35.3 | 3/3 | Complete    | 2026-08-12 |
| 36 | 2/2 | Complete    | 2026-08-15 |
| 37 | 4/4 | Complete    | 2026-08-16 |
| 37.1 | — | Complete    | 2026-08-17 |
| 38 | — | Complete    | 2026-08-17 |
| 39 | 1/1 | Complete    | 2026-08-18 |
| 40 | 2/2 | Complete   | 2026-08-18 |
| 41 | — | Complete | 2026-08-20 |
| 42 | — | Complete | 2026-08-21 |
| 43 | 2/2 | Complete | 2026-08-24 |
| 44 | 5/5 | Complete | 2026-08-27 |
| 45 | 3/3 | Complete | 2026-09-02 |
| 46 | 9/9 | Complete   | 2026-09-07 |
| 47 | 7/7 | Complete   | 2026-09-13 |
| 48 | 20/20 | Complete | 2026-09-23 |
| 49 | 12/12 | Complete | 2026-10-09 |
| 50 | 29/29 | Complete | 2026-10-21 |
| 50.1 | 30/30 | Complete | 2026-11-02 |
| 51 | 13/13 | Complete | 2026-11-08 |
| 52 | 33/33 | Executed — verification stale | 2026-11-11 |
| 52.1 | 7/7 | Executed — closed at plan 07 (operator, 2026-11-15) | 2026-11-15 |
| 52.2 | 26/26 | In Progress | — |
| 53 | 14/14 | Complete | 2026-11-20 |
| 54 | 0/? | Not started | — |
| 55 | 0/? | Not started | — |
| 53.1. Phase 53 remediation | 8/8 | Complete | 2026-11-21 |
| 53.2. wsx/UDE first-use and dogfood remediation | 9/17 | In Progress | - |

## v2.4.0 milestone (CLOSED 2026-08-06 — Resume Unattended Dogfooding)

**Declared 2026-08-04, closed 2026-08-06.** Closed the structural defects blocking unattended,
multi-wave `devflow start` runs: the Code↔Validate loop no longer false-gates on healthy work,
Validate's reported outcome reflects derived status rather than the agent's self-report, and Layer 0
verification is no longer inert in worktree mode. All four items were pre-existing defects found
during the Phase 29 dogfood run and Phase 31 planning. Full detail archived to
`.planning/milestones/v2.4.0-ROADMAP.md`; requirements to
`.planning/milestones/v2.4.0-REQUIREMENTS.md`; phase directories to
`.planning/milestones/v2.4.0-phases/`.

**Closed as `override_closeout`, not `verified_closeout`.** Both phases verified and the pre-close
artifact audit was clear, but DOGFOOD-04's traceability row remains `Pending` — its core guarantee
is closed by Phase 34 criteria 3 and 4, while 999.76's second call site is correct by construction
with no regression guard (999.84 / DEN-106). No `/gsd-audit-milestone` was run. See
`.planning/MILESTONES.md` § Known Gaps.

**Not released.** This close is a planning-state operation. At close the workspace version was still
`2.3.0`, `CHANGELOG.md` had no 2.4.0 section, the work sat unmerged on `feature/phase-34`, and no
`v2.4.0` tag existed.

| Phase | Name | Status | Version |
|---|---|---|---|
| 33 | Loop-Back Correctness for Multi-Wave Validate→Code Cycles (999.65 + 999.66) | Complete | — |
| 34 | Stream-JSON Coverage, the Validate Trust Boundary, and Layer 0 in Worktree Mode (999.73 + 999.74 + 999.76) | Complete | — |

## gsd-hygiene milestone (CLOSED 2026-08-04 — GSD Workflow Hygiene)

**Declared 2026-08-04, closed 2026-08-04. Intentionally unversioned** — no `vX.Y.Z`, nothing
published to crates.io; pure `.planning/` documentation structure. Restructured this file so the
active milestone's own phase headings land inside its own heading-to-next-heading window,
closing backlog `999.72` (`roadmap.analyze` misreporting `phase_count: 0`; `milestone.complete
--dry-run` triggering its pass-all degrade) and `999.72a` (the missing `## Progress` table
above). Full detail archived to `.planning/milestones/gsd-hygiene-ROADMAP.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 32 | ROADMAP Layout Hygiene | Complete | — |

## v2.3.0 milestone (CLOSED 2026-08-04 — the unattended run)

**Declared 2026-08-02, closed 2026-08-04.** Deliberately **bounded**: this milestone closed when
999.64 was delivered — Phase 30 (the parser and the feasibility gate) plus Phase 31 (the launch
path itself). Full detail archived to `.planning/milestones/v2.3.0-ROADMAP.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 30 | Keep the Session Alive Past Turn End | Complete    | — |
| 31 | The Launch Path Itself | Complete    | 2.3.0 |

## v2.0.0 milestone (CLOSED 2026-08-02 — spanned releases 2.0.0, 2.1.0, 2.2.0)

**Closed 2026-08-02.** The label named an open-ended milestone rather than a bounded arc
(see "Milestone stays open" below), so it never had a scheduled endpoint and continued past
the 2.0.0 release through 2.1.0 and 2.2.0. Closed by operator decision once Phase 29 aborted
and Phase 30 was rescoped to the 999.64 arc, which is a distinct unit of work. Full detail
archived to `.planning/milestones/v2.0.0-ROADMAP.md`.

| Phase | Name | Status | Version |
|---|---|---|---|
| 12 | Bootstrap + Housekeeping | Complete | — |
| 13 | MVP Core Loop | Complete    | — |
| 14 | Parallel Safety + Observability | Complete | — |
| 15 | Dogfood Enablement + OSS Readiness | Complete | — |
| 16 | Pipeline Reliability Hardening | Complete    | — |
| 17 | Pipeline Dogfood Follow-Up | Complete    | — |
| 18 | Dogfood Reliability Hardening | Complete    | 1.5.0 |
| 19 | Release Integrity + `main.rs` Decomposition | Complete    | 1.6.0 |
| 20 | Release Correctness + Operator Control | Complete    | 1.7.0 |
| 21 | Operator Legibility & Observability | Complete    | 1.8.0 |
| 22 | Concurrency & Governance Correctness | Complete    | 1.8.1 |
| 23 | End-to-End Dogfood — One Phase, Define→Ship, Unattended, With Claude | Complete    | 2.0.0 |
| 24 | `release --check` Signing-Key Inline Classification | Complete    | 2.0.0 |
| 25 | End-to-End Dogfood Blockers — Start, Progress, Finish, Recover | Complete    | 2.1.0 |
| 27 | Scrub Redirecting Git Environment From Production Calls | Complete    | 2.2.0 |
| 28 | Close the Checkpoint Answer Return Path | Complete    | 2.2.0 |

**Not in this milestone** — Phase 26 (CLOSED PARTIAL, not shipped) moved to
`.planning/superseded/26-release-cut-automation/`; Phase 29 (ABORTED, never merged,
no directory) stays as prose-only history below.

## v1.0 milestone (CLOSED, retroactively labeled 2026-08-04 — never declared at the time)

**No milestone concept existed for this era.** Phases 1-11 shipped individually as versions
0.1.0 through ~1.2.0 before this project adopted "milestone" as a grouping concept at all
(v2.0.0 was the first, declared 2026-07-23). This label was applied retroactively by operator
decision on 2026-08-04, alongside the v2.0.0 archival, rather than leaving this era permanently
un-archived. Full detail archived to `.planning/milestones/v1.0-ROADMAP.md` (see that file for
why this differs from the pre-existing `v1.0-ASSESSMENT.md`, an unrelated older artifact).

| Phase | Name | Version |
|---|---|---|
| 1–5 | Core workflow, versioning, state machine | 0.1.0–0.6.0 |
| 6 | Agent Completion Protocol | — |
| 7 | Worktrees & PR Integration | — |
| 8 | Docs & Onboarding | 1.0.1 |
| 9 | Open-Source Polish | 1.2.0 |
| 10 | Logging + Planning Step | — |
| 11 | GSD-Native Architecture + Remediation | 1.2.0 |

## Reorganized (June 2026)

- **Conventional commits deprecated for versioning (June 2026), ban lifted 2026-07-27** — this bullet originally recorded a bare prohibition on deriving versions from commit messages, with no rationale, incident, or evidence attached anywhere in this file. The operator explicitly authorised deviating from that policy on 2026-07-27 to fully automate versioning (CONTEXT.md D-06); Phase 25 (25c) implements the superseding scheme — baseline from the highest reachable semver tag plus conventional-commit classification of the commits since that baseline (`crates/devflow-core/src/version.rs`)
- **Phase 10 shipped** — logging + Planning step (Planning known bug, addressed in Phase 11 refactor)
- **Phase 11 recast** — full architecture refactor to GSD-native execution engine
- **Phase 12** — Bootstrap (new-project, map-codebase) + versioning automation + publish `devflow` to crates.io (name confirmed available, 2026-07-08)
- **Phase 13** — OSS readiness (dev container, contributing, CI) + Hermes plugin + Hermes/Antigravity adapters
- **Phase 14** — reliability + observability hardening, scoped from external code review feedback (2026-07-08)

## Reorganized for MVP (2026-07-14)

- **Phase 13 repurposed as MVP Core Loop** — priority is getting Define→Plan→Code→Validate→Ship working end-to-end unattended (Claude + Codex, gates via notify hook) so DevFlow can be dogfooded on real projects again. Claims the previously unclaimed `ship.rs` GSD-native rewrite; absorbs the reliability items from old Phase 14 (verdict-vs-ran, native envelope parsing, WR-11, notify hook, gate timeout, worktree default).
- **Phase 14 rescoped to Observability + Hermes Support** — residual `devflow logs`/`events.jsonl`/`status` work plus the previously unclaimed `capture_agent_output()` sync-path decision (now claimed there). Hermes work (agent adapter, skill-file rewrite, plugin) moved in from Phase 15 (2026-07-14) — the plugin's gate watcher consumes this phase's `events.jsonl`, so they ship together.
- **Phase 15 (was 13)** — OSS readiness (docs, dev container, contributing, Antigravity adapter) plus the actual crates.io publish. Hermes items moved out to Phase 14.

## Phase 17 scoping (2026-07-18)

- **Phase 17 narrowed to four units** after source verification resolved the spike's decision gate: `Unknown` non-advance (17a), typed outcomes + retry policy (17b), preflight readiness gate (17c), build provenance (17d). Scoped as a focused repair phase rather than a Phase 16 remediation — only 17d traces to the proven Phase 16 defect.
- **Phase 18 gains 18d/18e** — `devflow doctor` state/event reconciliation and the WR-03 transient-capture test fix moved out of 17. 18d depends on 17b + 17d. See the 2026-07-18 decision entry in STATE.md.

## Phase 19 scoping (2026-07-21)

- **Milestone label corrected.** `v2.0.0` had been carried as the milestone name since Phase 11 while the project actually shipped 1.2.0 → 1.5.0. No v2.0.0 was ever released. The milestone now runs Phase 11–20 and genuinely closes at v2.0.0.
- **Phase 19 = four promoted backlog items**, in sequence: 999.10 (`.devflow/` artifact hygiene, Urgent/S), 999.11 (`commit_path` empty commits, High/S), 999.8 (split `main.rs`, High/L), 999.16 (AI change acceptance contract, High/M, parallel track). Promoted via `/gsd-review-backlog`; all four source claims re-verified present at HEAD during promotion.
- **Cuts v1.6.0, not v2.0.0.** Nothing in the phase is breaking and — apart from the PII fix — almost nothing is user-visible. Tagging a pure-move refactor as a major release would oversell the changeset and burn the 2.0 slot.
- **999.8 is near-alone by necessity.** It conflicts with every other high-priority candidate: 999.6 (`--until`), 999.7 (manual ship override) and 999.3 (`gate show`) all land in `main.rs`. Every phase run before the split makes the split harder and re-pays the serialization tax — Phase 18 burned 6 near-serial waves on 7 plans for exactly this reason, and the file has grown +35% (6,239 → 8,467 lines) since that was logged.
- **Phase 20 gets the deferred set** — 999.6, 999.7, 999.13, likely 999.3 — and is what the split makes plannable as one phase in ~3 waves rather than two phases at 6.

## Phase 20 scoping (2026-07-22)

- **Phase 20 = five promoted backlog items**, in sequence: 999.24 (`VersionBump` workspace self-pins, High/S), 999.23 (`phase7_cli.rs` git-fixture reliability, High/M), 999.6 (`--until` plan-only mode, High/M), 999.13 (release-cut preflight, High/L), 999.7 (manual ship override, High/L). Promoted via `/gsd-review-backlog`; all five source claims re-verified open at HEAD (`8ecbdf9`) during promotion.
- **999.23 re-sized S → M during promotion.** The ROADMAP entry described one flaky test (`reference_and_cleanup_worktree_cli_flow`, worktree removal race). DEN-48 had since been broadened — a second, unrelated flake in the same file (`start_worktree_mode_ignores_main_checkout_divergence`, git object-store corruption on run `29946629986`) reframes the item as a structural weakness in how `phase7_cli.rs`'s fixtures drive real `git` under CI concurrency. Two distinct root causes, and instance 1 likely has a product-side component.
- **999.3 deliberately left in backlog.** The Phase 19 note reserved it for Phase 20 "likely", but it is the only Low-priority item in that set and it bundles four distinct UX gaps (`gate show`, rate-limit reset surfacing, in-stage `status` progress, recovery-verb discoverability). Split it before promoting rather than carrying the largest lowest-value unit in a phase already holding two L-sized items.
- **Two release defects promoted ahead of the operator features.** 999.24 (S) has shipped broken two for two (v1.5.0 patched by `7ad260c`, v1.6.0 by PR #15) and is a *product* bug — any user with a published Cargo workspace hits it identically. 999.23 (M) sits in the release gate, and a coin-flip test trains the reader to re-run red CI instead of investigating it. Both make this phase's own release cut trustworthy, which is why they lead.
- **999.13 blocks on 999.24.** Its highest-value check is the workspace self-pin invariant; it must assert against 999.24's fix rather than encode today's manual patch as the expected state.
- **v2.0.0 is not yet earned.** The milestone reserves 2.0.0 for this phase, but nothing in the five units is inherently breaking, and Phase 19 already declined to burn the 2.0 slot on a non-breaking changeset. Decide at ship time: either the phase earns a breaking change or the milestone closes at 1.7.0 and the slot stays unspent.

## Milestone stays open (2026-07-23) — SUPERSEDED 2026-08-02

> **Superseded.** The open-ended framing recorded below governed v2.0.0 from 2026-07-23 until
> 2026-08-02, when the milestone was closed and the bounded v2.3.0 milestone was declared above.
> The lesson is recorded rather than the decision reversed silently: an open milestone with no
> closing condition drifted three releases and left `.planning/STATE.md` advertising a merged PR
> as open. v2.3.0 carries an explicit closing condition for that reason. The notes below remain
> accurate as history for v2.0.0.

- **Decided at Phase 20 ship time:** ships as **v1.7.0**, not v2.0.0 — nothing across the five units is breaking, consistent with Phase 19's earlier call not to spend the 2.0 slot on a non-breaking changeset.
- **The v2.0.0 milestone does NOT close at Phase 20 or at any other fixed phase.** Earlier notes above ("the milestone now runs Phase 11–20 and genuinely closes at v2.0.0," "the v2.0.0 milestone closes at Phase 20," "the milestone reserves 2.0.0 for this phase") described a *bounded* Phase 11–20 arc culminating in a 2.0.0 release. That framing is superseded: the milestone continues past Phase 20 with no predetermined phase count or closing version — 2.0.0 remains an eventual aspiration, not a scheduled endpoint. Future phases keep numbering forward (21, 22, …) under the same open milestone until a genuinely breaking change actually earns the 2.0 slot; `/gsd-complete-milestone` is not run at Phase 20.
- Table above renamed from "v2.0.0 (Phase 11–20)" to reflect this — the phase list is historical (what's shipped so far), not a closing boundary.

- **Phase 14 rescoped to Parallel Safety + Observability** — the 2026-07-14 move of Hermes into Phase 14 was a workload-balance call made before the CR-03 parallel-safety flaw was deferred there (2026-07-15), which made 14 the heaviest phase instead of the slimmest. Phase 14 now leads with CR-03 (per-phase state files, phase-threaded monitor advance, coarse lock for main-checkout mutations), keeps the `capture_agent_output()` sync-path decision, and builds observability (`logs`/`events.jsonl`/`status`) on the final per-phase state model — in that order, since the state-file shape dictates what `status`/`logs`/`events.jsonl` enumerate.
- **Phase 16 (new): Hermes Support** — HermesAgent adapter, skill-file rewrite, and Hermes plugin moved out of 14. Depends on Phase 14 (the plugin's gate watcher consumes `events.jsonl` and the Phase 13 notify hook); sits after Phase 15 so public-facing OSS readiness isn't gated on personal-infrastructure work.

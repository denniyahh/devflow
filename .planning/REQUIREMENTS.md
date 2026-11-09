# Requirements: DevFlow

**Defined:** 2026-11-08
**Core Value:** A coding session that outlives any one agent: the session is the repository, so when one agent runs out
of quota or budget another continues the same work, and the run asks a human only for the decisions nothing else can make.

## v4.0 Requirements

Milestone **v4.0 Session Continuity**. Each requirement maps to one roadmap phase (numbering continues at 52). Where a
requirement closes a backlog entry, the entry is named; `.planning/user/V4-CUT-INVENTORY.md` is the cut list.

### Subtraction (CUT)

- [ ] **CUT-01**: A run has no Define stage. `devflow start` begins at Plan (or at the stage the operator names with
      `--until`-style selection), and `--dry-run` shows no Define. Closes 999.183, 999.214.
- [ ] **CUT-02**: A run ends after Validate as *finished*: there is no Ship stage, no after-ship hooks (Merge, VersionBump,
      ChangelogAppend, BranchCleanup), no `devflow ship` and no `--yes-ship`; the event log records `workflow_finished`
      with `shipped: false` and the worktree and branch are left for the operator. Closes 999.180, 999.210; retires 999.111, 999.129, 999.132, 999.173, 999.178.
- [ ] **CUT-03**: A rate limit with no usable fallback waits at a gate whose context names the reset time and the exact
      resume command. Once the reset time passes, the supervisor relaunches the stage by itself, exactly once. With no
      reset time on the denial, or after a second denial, the gate stays open for the operator. No
      `cron-instructions-NN.json` is written and nothing in the tree or docs refers to it. Closes 999.185, 999.186;
      retires 999.137's R-2. (Amended 2026-11-08 by the operator in the Phase 52 discuss step: auto-relaunch at reset,
      52-CONTEXT D-15/D-16.)
- [ ] **CUT-04**: Every inventory row marked `cut` that the operator confirms in the Phase 52 discuss step is removed with
      its documentation, tests, allowlist entries and OPERATIONS.md rows, and `scripts/check.sh all` (including
      `doc_check`) passes on the result. `devflow parallel` is cut (operator 2026-11-08): closes 999.181, retires 999.26.

### Supervise gate loop (GATE)

- [ ] **GATE-01**: A gate rejection's note reaches the relaunched agent's prompt verbatim. A gate answer aborts the run
      only when it is an explicit abort answer, never because the word "abort" appears in a note. DevFlow's own
      infra-ceiling abort is out of this requirement's scope. Closes 999.189. (Amended 2026-11-08 by the operator after
      the Phase 52 context review, 52-CONTEXT D-20.)
- [ ] **GATE-02**: A Validate rejection relaunches only the fix work, not the whole phase. A `gaps_found` verdict runs
      GSD's gaps route (`plan-phase --gaps`, then `execute-phase --gaps-only`). A rejection of a clean pass runs
      `/gsd-quick` with the operator's note. Closes 999.192. (Amended 2026-11-08 by the operator after the Phase 52
      context review, 52-CONTEXT D-21.)
- [ ] **GATE-03**: A verification that reports `gaps_found` can be driven to `passed` from inside the run: the relaunched
      agent is told to plan and close the gaps and re-verify, and the Validate gate never says "passed" while the
      report says otherwise. Closes 999.193, 999.205, 999.208.
- [ ] **GATE-04**: `devflow gate show` prints a failure gate's full reason, and the banner and `gate list` cut keeps the
      "way on" sentence first. Closes 999.206.

### Operator surface (OPS)

- [ ] **OPS-01**: `devflow --version` prints the version with the embedded build commit and dirty flag. Closes 999.216.
- [ ] **OPS-02**: `devflow cleanup` says why it keeps a worktree (the recorded stop reason, an `--until` halt, or a live
      run), `cleanup --force` removes the run's state together with its worktree, `devflow status` never reports a run
      whose worktree is gone as active, and `devflow start` says when it replaces a stale state. Closes 999.213.
- [ ] **OPS-03**: Command output carries no `tracing` lines unless `RUST_LOG` is set; `devflow doctor` reports an unset
      `RUST_LOG` as information, not a failure; a project that is not DevFlow's own gets no build-provenance warning and
      one line, not two, for a missing remote. Closes 999.217.
- [ ] **OPS-04**: `devflow status` shows a stopped run's stop reason and the fallback agent's state (armed, spent, none).
      Closes 999.190, 999.202.

### Question channel (ASK)

- [ ] **ASK-01**: DevFlow serves an MCP tool (`devflow_ask`) over stdio that every supported agent can call with a
      question, typed options and context; the call blocks until an answer exists and returns it to the agent.
- [ ] **ASK-02**: The policy "operator via gate" turns each question into a gate the operator answers from the shell
      (`devflow gate answer <phase> --gate-id <id> --option <id>` or a free-text answer), with the notify hook firing
      as for any gate.
- [ ] **ASK-03**: Every stage prompt tells the agent to call the tool for decision checkpoints, `blocking-human` gates
      and GSD discuss questions instead of self-resolving them positionally or stopping with a report.
- [ ] **ASK-04**: Every question and its answer are recorded in the event log with the answering policy or operator
      and the time, and `devflow history` shows them.
- [ ] **ASK-05**: A `blocking-human` checkpoint reached under a one-shot launch is answered through the channel and the
      stage continues in the same session. Closes 999.57, 999.127.

### Decision triage (TRIAGE)

- [ ] **TRIAGE-01**: A calibrated decision model (Jev or Laya, chosen by a bake-off on real GSD discuss questions
      recorded through ASK-04) scores each question's candidate answers; above the configured threshold (or a `route`
      of `auto`) DevFlow answers, otherwise the question goes to the operator policy.
- [ ] **TRIAGE-02**: Every triaged decision is recorded in a ledger with the question, the options, the chosen answer,
      the probability or route, and the policy that decided.
- [ ] **TRIAGE-03**: The operator can list the ledger (decided vs escalated) and override one decision, which re-opens
      the question to the agent on its next launch.
- [ ] **TRIAGE-04**: An unavailable or failing model falls back to the operator policy and says so; it never
      auto-decides.

### Failover generalized (FAIL)

- [ ] **FAIL-01**: A quota denial at any stage hands the run to the next agent in a configured rotation (an ordered
      list), as many times as the rotation allows, not only at Code and not only once. Supersedes Phase 51's R-2.
- [ ] **FAIL-02**: Utilization-aware routing: when the current agent's rate-limit telemetry crosses a configured
      threshold, the next stage launches on the next agent in the rotation before a denial happens.
- [ ] **FAIL-03**: The relaunched agent's prompt says which agent it is taking over from and why, and that the tree may
      hold that agent's uncommitted work which must not be discarded. Closes 999.215.
- [ ] **FAIL-04**: Both torn-line orders — a partial line read only at EOF after the idle check, and a partial line that
      never reaches the capture because an escaped process holds stdout — classify as torn, never `RateLimited`, with
      a test for each order and its control. Closes 999.207, 999.209.
- [ ] **FAIL-05**: The first non-`allowed` `rate_limit_event` seen in a stream capture is archived under
      `.devflow/history/`, and the denial fixtures match the live event shape (`utilization`, `unifiedWindows`).
      Closes 999.212.
- [ ] **FAIL-06**: A delivery canary refusal can be re-run for the same agent, and `resume --agent <same>` says what it
      did. Closes 999.194.

## Future Requirements

Deferred to a later milestone. Tracked but not in the v4.0 roadmap.

### Remote operator (REMOTE)

- **REMOTE-01**: A question escalated to the operator can be answered from a messaging channel (Telegram, WhatsApp,
  email) and the reply becomes the gate answer.
- **REMOTE-02**: The notify hook can address a messaging channel directly, not only a shell command.

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Ship and release automation (merge, version bump, changelog, tag, publish) | Leaves DevFlow; `wsx` and `scripts/cut-release.sh` own it (operator, 2026-10-26 and 2026-11-08) |
| State-file migration between DevFlow versions | No users besides the operator (operator, 2026-10-12, Phase 50 D-20) |
| New agent drivers | Six drivers exist; the milestone changes what runs them, not how many |
| Live validation of the opencode, pi and hermes drivers | Deferred in the inventory; only the agents the rotation needs are proven live |
| Define-stage discuss integration beyond the ask tool | GSD's discuss runs through the channel like any other question; no separate Define feature |

## Traceability

Populated during roadmap creation (2026-11-08). Phase order and dependencies: ROADMAP.md § v4.0 milestone.

| Requirement | Phase | Status |
|-------------|-------|--------|
| CUT-01 | Phase 52 — Subtract and Stabilize | Pending |
| CUT-02 | Phase 52 — Subtract and Stabilize | Pending |
| CUT-03 | Phase 52 — Subtract and Stabilize | Pending |
| CUT-04 | Phase 52 — Subtract and Stabilize | Pending |
| GATE-01 | Phase 52 — Subtract and Stabilize | Pending |
| GATE-02 | Phase 52 — Subtract and Stabilize | Pending |
| GATE-03 | Phase 52 — Subtract and Stabilize | Pending |
| GATE-04 | Phase 52 — Subtract and Stabilize | Pending |
| OPS-01 | Phase 52 — Subtract and Stabilize | Pending |
| OPS-02 | Phase 52 — Subtract and Stabilize | Pending |
| OPS-03 | Phase 52 — Subtract and Stabilize | Pending |
| OPS-04 | Phase 52 — Subtract and Stabilize | Pending |
| ASK-01 | Phase 53 — The Question Channel | Pending |
| ASK-02 | Phase 53 — The Question Channel | Pending |
| ASK-03 | Phase 53 — The Question Channel | Pending |
| ASK-04 | Phase 53 — The Question Channel | Pending |
| ASK-05 | Phase 53 — The Question Channel | Pending |
| TRIAGE-01 | Phase 54 — Decision Triage | Pending |
| TRIAGE-02 | Phase 54 — Decision Triage | Pending |
| TRIAGE-03 | Phase 54 — Decision Triage | Pending |
| TRIAGE-04 | Phase 54 — Decision Triage | Pending |
| FAIL-01 | Phase 55 — Failover Generalized | Pending |
| FAIL-02 | Phase 55 — Failover Generalized | Pending |
| FAIL-03 | Phase 55 — Failover Generalized | Pending |
| FAIL-04 | Phase 55 — Failover Generalized | Pending |
| FAIL-05 | Phase 55 — Failover Generalized | Pending |
| FAIL-06 | Phase 55 — Failover Generalized | Pending |

**Coverage:**
- v4.0 requirements: 27 total
- Mapped to phases: 27
- Unmapped: 0 ✓
- Duplicates (a requirement owned by more than one phase): 0 ✓

---
*Requirements defined: 2026-11-08*
*Last updated: 2026-11-08 after roadmap creation (Phases 52-55)*

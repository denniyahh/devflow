# Requirements: DevFlow v3.0.0 — Unattended Run Survivability

**Defined:** 2026-09-03
**Core Value:** A long unattended run either makes real progress or stops somewhere an operator
can recover from — never silently does the wrong thing, and never wedges without a way out.

**Provenance.** Every requirement below traces to a defect verified against the v2.12.0 tree on
2026-09-03, not to a filing taken on trust. Where a claim was checked, the check is named. Where
something remains unverified, it is said so explicitly — those are the requirements most likely to
change shape once a live run exists.

## v3.0.0 Requirements

### Decision Policy (DECN)

The unattended merit-based decision policy DECN-01 shipped half-delivered in v2.8.0. These close it.

- [ ] **DECN-02**: The unattended decision policy reaches the Claude/OpenCode Code prompt on the
      Validate loop-back path, not only on the first Code pass.
      *Traces to 999.115.* Verified: `CODE_STAGE_POLICY` occurs at `prompt.rs:62/379/469` and in
      tests, never inside `fix_prompt` (`:567-580`); negative control — `COMPLETION_PROTOCOL` at
      `:578` proves the search range is non-empty.
      **Scope constraint:** the fix must be arm-scoped to `FullExecute`. A test at `prompt.rs:810`
      deliberately requires `GapsOnly`/`AuditFix` to omit the policy, and `workflow_code_prompt`
      honours that. The real defect is the two renderers disagreeing for
      `Code { fix: Some(FullExecute) }` alone.

- [ ] **DECN-03**: An agent resumed into a session carrying both `CODE_STAGE_POLICY` and
      `checkpoint_auto_decide_prompt` receives one consistent instruction about who may resolve a
      `blocking-human` gate.
      *Traces to 999.116.* Verified present and co-resident: policy at `prompt.rs:62`, resume
      prompt at `prompt.rs:537`, injected at `pipeline_launch.rs:1100`.
      **Not established:** which instruction an agent actually follows. That is a question about
      model instruction-priority and is not answerable by reading source — this requirement is
      deliberately scheduled to resolve against VERIFY-01's live run.

### Input Validation (VALID)

- [ ] **VALID-01**: A configured `base_branch` naming a computed revision rather than a branch is
      refused.
      *Traces to #204.* Reproduced with a negative control on 2026-09-03: `refs/heads/develop~1`,
      `develop@{0}` and `develop^{}` all pass `rev-parse --verify` (exit 0) while
      `refs/heads/nonexistent-xyz` correctly fails (exit 1). `develop^{}` was not in the original
      filing.
      **Implementation note:** extend the existing loop in
      `ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch`
      (`commands.rs:5459`) — it already carries its own negative control — rather than writing a
      new test.

- [ ] **VALID-02**: `devflow stop` accepts the project root the same way `start`, `resume` and
      `status` do.
      *Traces to #206.* Verified: `stop` takes `--root <ROOT>`; the others take a positional
      `[PROJECT]`. `devflow stop --phase 7 .` dies with a usage error naming no offending argument.
      Encountered live while reproducing #200, where it read as "stop did not work" rather than
      "stop rejected my input" — the wrong failure mode for a recovery verb.

### Run Survivability (SURV)

- [ ] **SURV-01**: Two processes writing state for the same phase cannot silently lose an update.
      *Traces to 999.118.* Verified unchanged: `workflow.rs:185-193` derives
      `path.with_extension("tmp")`, a pure function of the state path, so every writer for a phase
      writes the same temp file.
      **Not established:** no interleaving has been observed in the wild. Acceptance requires a
      test that fails on the current implementation, with a single-writer negative control that
      must still round-trip.

- [ ] **SURV-02**: A gate whose consumer is gone reports the recovery that exists instead of
      asserting a waiter that does not.
      *Traces to #200.* Reproduced live on v2.12.0, and the original filing corrected in two
      directions: the wedge requires the foreground `start` to be **interrupted** (left running, it
      self-resolves in ~40s), and recovery does **not** require `rm -f` — `devflow resume --phase N`
      clears it, which `devflow doctor` already prescribes via `check_gate_pending_without_gate`.
      What survives: `gate reject` and `stop` both print that a waiting process will pick up the
      response without ever checking whether one exists, and neither names the repair.

### Live Verification (VERIFY)

- [ ] **VERIFY-01**: A real `devflow start --mode auto` run is recorded end to end against a
      configured base — fork point, `unattended_config_condition = Holds`, unattended progression,
      and the Ship merge target — with a negative control that an unconfigured repo still forks
      from `develop`.
      *Traces to 999.119.* **The negative-control arm is already banked** from a live run on
      2026-09-03: an unconfigured scratch repo forked from `develop` (persisted
      `"base_branch": "develop"`) and correctly refused on the missing `.planning/config.json`.
      **The positive arm is wholly unverified** — nothing has run with `base_branch` set to a
      planning branch, and the Ship merge target has never been observed at all.
      **Carries a setup step:** this repository has no committed `devflow.toml`.

### Supervision (SUPV)

- [ ] **SUPV-01**: Monitor liveness is answerable from disk state alone without a PID, in three
      distinguishable states — GONE, STALE, ALIVE.
      *Traces to #185.* Design spike-proven (C1-C6); no socket exists in `crates/` today.
      **Scope correction:** `MonitorLaunch::PipeOwning` (Phase 31) is already a Rust supervisor, so
      this is "give an existing supervisor an address" for that variant and a genuine replacement
      only for `Legacy` — which still carries every non-Claude adapter and the checkpoint-resume
      relaunch. Both shapes must answer liveness identically or the result buys nothing for most
      agents.
      **Binding constraint:** `sun_path` is 108 bytes on Linux, 104 on macOS. The socket cannot
      live in the project tree; store its path in `state.json` and prefer `~/.cache/devflow/` over
      `$XDG_RUNTIME_DIR`, which systemd deletes at logout.

- [ ] **SUPV-02**: An agent that exhausts its token budget mid-stage continues under a configured
      fallback agent, in the same worktree on the same branch, with no manual conflict resolution.
      *Traces to #180.* **The issue's "What exists today" is stale** — the `n` verb, `git.rebase_in`
      and the rebase handoff were all removed (D-11, 23d); zero occurrences remain. Detection is
      intact (`detect_rate_limit`, `AgentStatus::RateLimited`, `RateLimited => Action::AutoResume`).
      **Shrunk by a v2.8.0 delivery:** `devflow resume --agent` already performs the driver
      substitution, so this reduces to firing it automatically on `RateLimited` rather than
      building the mechanism. Both exhaustion answers must coexist — fallback configured → switch
      agents; otherwise → `write_rate_limit_cron`.
      **Re-test the SUPV-01 dependency rather than assuming it.**

### Test Infrastructure (INFRA)

- [ ] **INFRA-01**: CI runs a job that reproduces the sequential `fmt → clippy → test` load shape
      on a 2-core runner.
      *Traces to #174.* Verified unchanged: `.github/workflows/ci.yml` runs three parallel jobs
      (`test:31`, `clippy:55`, `fmt:67`), each invoking one `scripts/check.sh` part; no `all` job
      exists. Every reproduction of the 999.47 `/proc` fork-inheritance race required the
      sequential ordering, so CI is structurally incapable of catching that class today — it
      rejected 0 of the pushes the local pre-push gate rejected 2 of 2.
      **Land first:** it is a workflow-file addition with no source change, and it should be
      running underneath the rest of this milestone rather than alongside it.

## Future Requirements

Acknowledged, not in this milestone.

| ID | Requirement | Why deferred |
|----|-------------|--------------|
| BASE-01 (#186) | Per-run `--base <branch>` flag on `devflow start` for stacking on an unmerged predecessor | Phase 45 delivered config/env base resolution; only the per-run flag remains, and it serves stacking rather than survivability. Demoted to P2 on 2026-09-03. |
| GATE-01 (#184) | `devflow diff --phase N` — see the change a gate is asking about | Real trust gap, but it improves a supervised workflow rather than an unattended one |
| GATE-02 (#170) | Checkpoint detection distinguishes a gate declaration from a gate mention | Reachable only once the launch path emits `stream-json`; bounded today by falling through to the generic human gate |
| SAFE-01 (#177) | `devflow cleanup` stops deleting `recovery/*` refs by ancestry | Small and real; not on the unattended path |
| SAFE-02 (#179) | Registry dedups `(root, phase, stage)` so the sweep preview matches the sweep | Over-reports a destructive preview; not on the unattended path |

## Out of Scope

| Feature | Reason |
|---------|--------|
| Domain/ecosystem research | Every item is a verified defect in this codebase with a known fix direction; the #185 socket design is already spike-proven. Operator decision, 2026-09-03. |
| 999.120 (ambient `git_flow_for_project` re-resolution) | Successor to the now-closed #163; narrower and needs its own framing against the resolve-once-then-persist model |
| 999.121 (OpenCode marker-less CLI regression test) | Driver-specific test coverage, unrelated to the unattended lifecycle |
| The P3 test-infra block (#189-#197) | #182 already proposes pruning it as speculative; re-scoping it is its own decision, not this milestone's |
| DevFlow HUD/TUI (#160), live telemetry (#159) | Presentation of run state, not survival of the run |
| Clean upstream PR extraction (#199) | Tooling around the repo, not the product's core loop |

## Traceability

Populated during roadmap creation (2026-09-03). Wave order is load-bearing — see ROADMAP.md
§ v3.0.0 milestone.

| Requirement | Phase | Wave | Status |
|-------------|-------|------|--------|
| INFRA-01 | Phase 46 — CI Load Shape and Operator Input Validation | 1 | Pending |
| VALID-01 | Phase 46 — CI Load Shape and Operator Input Validation | 1 | Pending |
| VALID-02 | Phase 46 — CI Load Shape and Operator Input Validation | 1 | Pending |
| DECN-02 | Phase 47 — Unattended Decision Policy Consistency | 1 | Pending |
| DECN-03 | Phase 47 — Unattended Decision Policy Consistency (behavioural arm resolves against Phase 49) | 1 | Pending |
| SURV-01 | Phase 48 — Survivable State Writes and Honest Gate Recovery (field arm observed, not settled, in Phase 49) | 2 | Pending |
| SURV-02 | Phase 48 — Survivable State Writes and Honest Gate Recovery | 2 | Pending |
| VERIFY-01 | Phase 49 — Live Unattended Run | 3 | Pending |
| SUPV-01 | Phase 50 — Addressable Monitor Liveness | 4 | Pending |
| SUPV-02 | Phase 51 — Rate-Limit Agent Failover | 5 | Pending |

**Coverage:**
- v3.0.0 requirements: 10 total
- Mapped to phases: 10
- Unmapped: 0 ✓
- Duplicates (a requirement owned by more than one phase): 0 ✓

**Cross-phase resolution notes.** Two requirements are *owned* by an earlier phase but carry an arm
that only a later phase can answer. The owning phase is still the single mapping — the later phase
records an observation, it does not take ownership:

- **DECN-03** — Phase 47 removes the prompt-level contradiction. Which instruction a model actually
  follows is a question about instruction-priority and is not answerable by reading source; Phase 49's
  live run answers it, or records explicitly that it never exercised that path.
- **SURV-01** — Phase 48's acceptance is a test that fails on the current implementation with a
  single-writer negative control. No interleaving has been observed in the wild; Phase 49 records
  whether the live run produced one. A run with no observed interleaving confirms nothing and must
  not be written up as if it did.

---
*Requirements defined: 2026-09-03*
*Last updated: 2026-09-03 after the v3.0.0 roadmap was created — 10/10 requirements mapped
across 6 phases (46-51), 5 waves.*

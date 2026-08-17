> invocation: antigravity -p "<prompt>" (config model temporarily 'Gemini 3.1 Pro', restored after)

### 1. Contradiction & False Deliverable: "End-to-End" Pi Support vs. Hardcoded Claude Stages
* **Categories:** (1) Contradiction between documents, (3) Decision failing contact with codebase.
* **Exact Citations:**
  * `36-SPEC.md` (lines 9–10):
    > `"DevFlow can drive **Pi** (the Pi coding-agent harness) end-to-end as a fourth first-class agent adapter"`
  * `36-CONTEXT.md` (§ Implementation Decisions, D-01):
    > `"- **D-01:** Code-stage vertical slice first. Prove the transport + completion parsing end-to-end on one stage before widening."`
  * `36-DISCUSSION-LOG.md` (§ Process Notes, lines 53–55):
    > `"devflow dogfood run (devflow start --phase 36 --mode supervise --until plan) was aborted: devflow's Define/Plan stages hardcode the Claude launch, and Phase 36 is precisely the phase that removes that limitation."`
* **Finding:** `36-SPEC.md` promises that DevFlow can drive Pi "end-to-end", and `36-DISCUSSION-LOG.md` asserts Phase 36 removes the limitation where Define/Plan hardcodes Claude. Both claims are false under `36-CONTEXT.md` D-01. Restricting Pi to a Code-stage-only vertical slice leaves Define, Plan, Discuss, and Verify hardcoded to Claude. A user cannot run `devflow start` end-to-end with Pi, and the exact dogfood run that aborted (`devflow start --until plan`) will still abort after Phase 36 lands.

---

### 2. Architectural Collision: `-p` Transport Discretion Breaks `monitor.rs` Drain Gate
* **Categories:** (1) Contradiction between documents, (3) Decision failing contact with tool/codebase.
* **Exact Citations:**
  * `36-CONTEXT.md` (§ Existing Code Insights, Integration Points):
    > `"- crates/devflow-core/src/monitor.rs — the pipe-owning monitor; its drain gate (CloseRule) reads agent-specific events, so Pi's completion detection (agent_end) needs a Pi-aware arm."`
  * `36-CONTEXT.md` (§ the agent's Discretion):
    > `"- Whether the -p or --mode json transport is used for the first cut, pending the plan-phase verification of exit codes and drain-gate mapping."`
  * `36-DISCUSSION-LOG.md` (§ Agent Discretion):
    > `"- -p vs --mode json transport for the first cut (pending plan-phase exit-code verification)."`
* **Finding:** The document delegates whether to use `-p` or `--mode json` to implementer discretion while simultaneously asserting that `monitor.rs` requires a `CloseRule` reading the `agent_end` JSON event. In `-p` (`--print`) mode, Pi outputs raw text and emits zero JSON events. If `-p` is chosen, the `monitor.rs` drain gate waiting on `agent_end` will hang or drop piped output. Leaving the transport mode open while specifying a JSON-event drain gate in `monitor.rs` is an architectural contradiction.

---

### 3. Scope Leak & Missing Spec Acceptance Criteria: Smuggled Preflight Fingerprint Check
* **Categories:** (4) Scope leak, (5) Acceptance criteria not falsifiable.
* **Exact Citations:**
  * `36-SPEC.md` (§ Scope D, Acceptance criteria 4):
    > `"D. 999.104 — release-signing key (default: one-line probe) ... repoint release --check's tag-signing probe at devflow.releaseSigningKey"`
    > `"4. 999.104: release --check's signing probe reports viable only for the maintainer key, with a negative control (the wrong key reported NOT viable)."`
  * `36-CONTEXT.md` (§ Implementation Decisions, D-02):
    > `"- **D-02:** One-line probe fix **plus** surfacing the fingerprint check at preflight (the decision-3 companion). The probe alone fixes *detection*; the preflight fingerprint check is what turns "silent until push" into "fails at preflight.""`
* **Finding:** D-02 sneaks a new preflight fingerprint validation gate into Phase 36 that does not exist in `36-SPEC.md`. The SPEC bounds 999.104 to repointing a single probe row inside `release --check` and provides an acceptance criterion only for `release --check`. The preflight fingerprint check has no defined CLI integration point, no error payload contract, and no falsifiable acceptance criteria or negative controls anywhere in the specification.

---

### 4. Unstated Assumption & Silent Failure Mode: Default Project Trust (`ask`) in Headless Invocations
* **Categories:** (2) Unstated assumption, (6) Missing failure mode.
* **Exact Citations:**
  * `36-CONTEXT.md` (§ Implementation Decisions, D-04):
    > `"- Project trust: non-interactive modes never prompt; they consult defaultProjectTrust (default ask → project resources ignored unless approved), overridable per-run with --approve / --no-approve."`
    > `"- Exact flag selection (json vs. print; approve vs. no-approve; no-session vs. session) and exit-code semantics are **plan-phase** decisions"`
* **Finding:** Non-interactive invocations of Pi default to `ask`, which causes Pi to silently ignore workspace resources, skills, and configuration without error or prompting. Deferring the flag selection to the planning phase treats `--approve` as an optional implementation detail. If DevFlow spawns `pi` without `--approve`, the run will exit with code 0 while executing in a crippled sandbox that dropped project context. The spec lacks any requirement or guard to ensure project trust is explicitly granted on headless spawns.

---

### 5. Unfalsifiable Acceptance Criterion: AC 1 Lacks Negative Controls and Execution Bounds
* **Categories:** (5) Acceptance criteria not falsifiable / no negative control.
* **Exact Citations:**
  * `36-SPEC.md` (§ Acceptance criteria):
    > `"1. A devflow run driven by Pi reaches a terminal completion, with its completion parsed and its

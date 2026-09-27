reviewer=agy model=gemini-3.1-pro-high effort=high review_root=/var/home/denniyahh/Github/devflow/.worktrees/phase-49 prompt_mode=compact_file_directed
### Adversarial Review of Phase 49 Plan Set

#### 1. Systemd Cleanup / Retry Safety: Orphaned Process Collision
* **Evidence:** `49-13-PLAN.md` explicitly instructs leaving leftover processes running: "Leave leftover test monitors running (999.141, RESEARCH Pitfall 8)." However, `49-08-PLAN.md` (`launch-attempt.sh` and `archive-attempt.sh`) only checks that the systemd unit `p49-a<N>-start` is inactive before proceeding.
* **Contradiction:** An operator running a retry (Attempt 2) will collide with `999.141` test monitors left running from Attempt 1. `launch-attempt.sh` does not sweep or terminate these orphaned processes, risking silent lock contention or resource exhaustion on the same worktree.
* **Severity:** HIGH
* **Required Property:** A retry wrapper (`launch-attempt.sh`) must positively sweep and terminate lingering agent/monitor processes holding phase locks from previous attempts.
* **Fix Hint:** Add a `pkill -f` or similar teardown step in `launch-attempt.sh` to kill lingering `devflow` monitors targeting the Phase 49.1 worktree before spawning Attempt N+1.

#### 2. Secret Sanitization: Fragile Static Whitelist
* **Evidence:** `49-15-PLAN.md` (Lines 94 and 149) implements redaction and gating using a hardcoded regex: `rg --count-matches -e "ghp_[A-Za-z0-9]|sk-ant-|-----BEGIN"`.
* **Contradiction:** This static regex fails to catch GitHub fine-grained PATs (`github_pat_`), OpenAI keys (`sk-proj-`), or generic environment variables (e.g., `ANTHROPIC_API_KEY=...` without the `sk-ant-` prefix) that might be dumped into `49-evidence/` from `ledger.jsonl` or `pr.json`.
* **Severity:** CRITICAL
* **Required Property:** Secret redaction must not rely on a whitelist of three narrow string prefixes to sanitize raw JSON state dumps committed to a public repo.
* **Fix Hint:** Instruct the agent to extract only specific, known-safe JSON keys using `jq` rather than dumping whole objects and attempting to redact them, or use a robust credential scanner in the gate.

#### 3. Hook/Container Behavior: Unattended Ship Bypass
* **Evidence:** `49-12-PLAN.md` runs `timeout 1800 bash ... scripts/check-in-container.sh all` manually as a preflight step, and then applies an ND-1 accommodation (`ship-accommodations.sh`) to strip this heavy check before the agent actually runs `devflow ship`.
* **Contradiction:** The milestone's core objective is to measure a "live unattended run" (Phase 49 Context). By manually front-loading the 30-minute container check and bypassing it during the agent's execution, the Ship stage is not running unattended under its actual production hook configuration.
* **Severity:** HIGH
* **Required Property:** The agent's `devflow ship` must execute the exact hooks that would fire in a production environment to accurately measure the pipeline's survivability and timeout behavior.
* **Fix Hint:** Configure the `devflow ship` hook timeout to explicitly accommodate the 30-minute test rather than stripping it, or explicitly declare this bypass as a limitation in the `49-LIVE-RUN.md` limits section.

#### 4. Evidence Integrity: Falsifiable Gates Lack Semantic Assertion
* **Evidence:** `49-15-PLAN.md` (Lines 149-151) automated gate only checks for the presence of markdown sections (e.g., `[ "$c" -eq 1 ]`) and a file count (`[ "$ef" -ge 5 ]`).
* **Contradiction:** The gate relies entirely on the LLM's prose generation to truthfully state `C1: MET`. It performs no mechanical verification that the excerpts copied into `49-evidence/` actually support the verdicts. A hallucinating agent could output `MET` while the actual logs show failures, and the gate would pass it.
* **Severity:** MEDIUM
* **Required Property:** The verification gate must mechanically assert that the raw captured evidence logically maps to the claimed `MET`/`NOT_MET` verdicts.
* **Fix Hint:** Add `jq` assertions in the gate script to verify that `ledger.jsonl` actually contains the specific `operator_response` or `transition` events required to support the written verdict.

#### 5. Backlog Collision: Ambiguous ROADMAP Appends
* **Evidence:** `49-15-PLAN.md` (Task 2, Line 177) instructs the agent to "add a `### Phase 999.<next>: <title> (BACKLOG)` entry".
* **Contradiction:** `ROADMAP.md` currently contains backlog entries up to `999.146` (and `STATE.md` tracks active items like `999.125`). The instruction does not provide a deterministic way to compute `<next>`, risking LLM hallucination, numbering collisions (e.g., overwriting `999.126`), or syntax breakage that would cause the `roadmap.analyze` gate to fail.
* **Severity:** MEDIUM
* **Required Property:** Backlog index assignment must be deterministic and mechanically protected against collisions.
* **Fix Hint:** Provide a shell one-liner in the plan to compute the highest existing `999.X` suffix, or have the agent invoke a `gsd-tools` command to file the backlog item safely.

#### What Static Review Cannot Establish
* **Model Instruction Priority:** Whether the agent will obey `CODE_STAGE_POLICY` or `checkpoint_auto_decide_prompt` when both are injected (DECN-03).
* **Systemd Unit Viability:** Whether `p49-a<N>-gatewatch` and `p49-a<N>-survwatch` will actually trigger on the correct inotify events or reliably survive session hang-ups in the operator's live environment.
* **Orphan Monitor Interference:** Whether the unkilled `999.141` test monitors will actually hold blocking filesystem locks that wedge a retry in practice.

review_exit=0

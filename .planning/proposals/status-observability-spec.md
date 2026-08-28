# Feature Specification: Granular Runtime Telemetry & Watch Mode for `devflow status`

**Status:** Proposed  
**Author:** Pair Programming Session  
**Target Component:** `crates/devflow-cli/src/commands.rs` (`status` command)  
**GitHub:** [#159](https://github.com/denniyahh/devflow/issues/159) (migrated from Linear DEN-116)

---

## 1. Executive Summary & Problem Statement

During real-world dogfood runs, `devflow status` proved too coarse-grained during active stage execution. Once a stage begins, the status display remains frozen on `last action: stage_launched (X minutes ago)` for the entire multi-minute turn.

### Observed Failure Modes & Blind Spots
1. **Opaque In-Flight Actions:** Operators cannot distinguish between a long-running test/compilation suite, a container pre-push check, parallel subagent review passes, or a deadlocked/hung process.
2. **Silent Watchdog Countdown:** No visibility into quiet-gap duration or configured watchdog floors (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`), leading to unexpected idle timeout kills.
3. **Blind Commit Velocity:** Branch divergence is displayed (e.g. `11 ahead`) but the latest committed messages/timestamps are invisible without manual git inspection.
4. **No Live Refresh:** Operators are forced to repeatedly invoke `devflow status` by hand every few seconds.

---

## 2. Target UX & Terminal Mockup

```text
project_root: /var/home/denniyahh/Github/devflow
active phases: 42

phase 42:
  stage: ship | mode: supervise | gate: none
  agent: Antigravity (PID: 359103 · Monitor PID: 359098)
  liveness: healthy (watchdog: 300s limit · silent for: 8s)
  in stage ship: 8m 12s | elapsed total: 4h 12m
  current action: ⚙️  run_command: git push origin feature/phase-42 (step 158)
    ↳ sub-task: pre-push container check (scripts/check-in-container.sh all)
  subagents: 5 completed (doc, security, build, claims, generalist)
  latest commit: b509e8f style: cargo fmt formatting adjustments (1m ago)
  telemetry: 187k tokens · 158 steps · 245 KB streamed

open branches:
  feature/phase-42 — 11 ahead
```

---

## 3. Functional Requirements

### FR-01: In-Flight Tool Action Parsing
- **Requirement:** `devflow status` shall inspect the trailing lines of `.devflow/phase-NN-stdout` to extract the most recent active step.
- **Payload:** Extract `tool_name` (e.g. `run_command`, `view_file`, `manage_task`) and primary argument summary (e.g. command string or file path).
- **Graceful Fallback:** If the agent is non-streaming or the stdout stream is empty/unparsable, fall back to the last known lifecycle event from `events.jsonl`.

### FR-02: Watchdog Silence & Health Telemetry
- **Requirement:** Measure the interval since the last byte was written to `.devflow/phase-NN-stdout` (comparing `SystemTime::now()` against the file's `mtime`).
- **Display:** Render `silent for: Xs / <limit>s` alongside the configured idle watchdog floor.

### FR-03: Latest Worktree Commit Context
- **Requirement:** If `state.worktree_path` exists, query `git log -1 --format="%h %s (%cr)"` in the worktree.
- **Display:** Render the latest commit short-hash, subject line, and relative timestamp.

### FR-04: Stream Telemetry & Token Metrics
- **Requirement:** Aggregate stream size (in KB/MB) and cumulative token usage (`input_tokens`, `output_tokens`) from the latest `usage` object in `.devflow/phase-NN-stdout`.

### FR-05: Live Watch Mode (`devflow status --watch`)
- **Requirement:** Support `--watch` / `-w [INTERVAL_SECS]` (default 2s) to continuously refresh the status display in-place using ANSI terminal cursor repositioning without screen flickering.

---

## 4. Technical Implementation Plan

1. **Telemetry Extractor Module (`crates/devflow-cli/src/telemetry.rs`)**:
   - `extract_stream_telemetry(project_root: &Path, phase: PhaseId) -> Option<StreamTelemetry>`
   - Reads the last 64 KB of `.devflow/phase-NN-stdout` using backward seeking.
   - Parses the latest JSONL `step_update` and `usage` entries.
2. **Status Renderer Update (`crates/devflow-cli/src/commands.rs`)**:
   - Incorporate `StreamTelemetry` into `render_phase_status`.
   - Add latest git commit inspection via `git log -1`.
3. **CLI Argument Update**:
   - Add `#[arg(short, long)] watch: Option<Option<u64>>` to `Commands::Status`.

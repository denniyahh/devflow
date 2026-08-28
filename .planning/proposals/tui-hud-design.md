# RFC: DevFlow TUI / HUD (Terminal User Interface)

**Status:** Proposed  
**Author:** Pair Programming Session  
**Target:** DevFlow CLI (`devflow hud` / `devflow tui`)  
**GitHub:** [#160](https://github.com/denniyahh/devflow/issues/160) (migrated from Linear DEN-115)

---

## 1. Executive Summary & Problem Statement

Currently, DevFlow operates as a detached supervisor daemon (`devflow __monitor`) managing agent subprocesses across isolated worktrees. While robust and resilient to terminal disconnection, this execution model introduces friction for the operator:

1. **Silent Gates:** When an agent halts at a human review gate (e.g. `Validate` supervision, `Ship` gate, or an escalated failure like an idle timeout or critical review blocker), the pipeline blocks silently unless the operator manually configured `DEVFLOW_GATE_NOTIFY_CMD` or repeatedly polls `devflow status`.
2. **Blind Polling:** Operators have no live, unified view of streaming output across multiple concurrent phases (e.g. during `devflow parallel`).
3. **Manual CLI Churn:** Resolving gates requires typing out commands by hand (e.g. `devflow gate approve 42 --stage ship` or `devflow gate reject 42 --stage ship --note "..."`).

This RFC proposes **DevFlow HUD** (`devflow hud` or `devflow tui`), a high-density, real-time Terminal User Interface built in Rust with `ratatui` and `crossterm`.

---

## 2. Architecture & Design Principles

### Decoupled Observer / Actuator Architecture
The TUI is strictly decoupled from the core workflow engine and pipeline execution:
- **No Direct IPC or Daemons:** The TUI does not manage process lifecycles directly; it observes DevFlow's existing on-disk state primitives:
  - `.devflow/state-*.json` — Phase status, current stage, mode, PID liveness.
  - `.devflow/events.jsonl` — Append-only chronological lifecycle event stream.
  - `.devflow/gates/NN-<stage>.json` — Active gate requests and metadata.
  - `.devflow/phase-NN-stdout` — Live agent telemetry streams.
- **Actuation via Standard File Protocols:**
  - Answering a gate writes `.devflow/gates/NN-<stage>.response.json` (identical to `devflow gate approve / reject`).
  - Stopping/resuming invokes the existing `devflow stop` and `devflow resume` routines.
- **Fail-Safe Operation:** Opening, closing, resizing, or crashing the TUI has **zero effect** on running agents or the supervisor monitors.

```
┌──────────────────────────────────────────────────────────┐
│                    DevFlow HUD (TUI)                     │
│  ┌───────────────────────┐   ┌────────────────────────┐  │
│  │ Active Phases Sidebar │   │ Live Telemetry Viewer  │  │
│  └───────────────────────┘   └────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │       Interactive Gate Action Bar & Modals         │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────┬───────────────────────────────▲───────────┘
               │ Writes Responses / Actions    │ Reads State & Streams
               ▼                               │ (via inotify / ticker)
┌──────────────────────────────────────────────────────────┐
│              `.devflow/` State & Gate Files              │
│  - state-*.json       - gates/*.json                     │
│  - events.jsonl       - phase-*-stdout                   │
└──────────────────────────────────────────────────────────┘
```

---

## 3. UI Layout & Visual Specifications

```
┌ DevFlow HUD ───────────────────────────────────────── Milestone: v2.8.0 [████████░░] 80% ─┐
│ Active Phases          │ Phase 42: Hermes Driver [Stage: Ship · Antigravity]             │
│                        │                                                                 │
│ ▶ 42: Hermes Driver 🟡 │ 20:45:09 [INFO] stage ship → launched Antigravity (pid: 286973) │
│   35.1: Keep Session ⚪│ 20:45:12 [AGENT] Starting multi-model code review...            │
│   37.1: OpenCode     ⚪│ 20:45:30 [AGENT] Checked 5 review dimensions.                   │
│   38: Codex Verif    ⚪│ 20:46:05 [WARN] Finding CR-01: Contradicted cadence claim.      │
│                        │ 20:46:12 [RESULT] DEVFLOW_RESULT: {"status": "failed"}          │
│                        │ 20:46:12 [GATE] Gate written: .devflow/gates/42-ship.json       │
├────────────────────────┴─────────────────────────────────────────────────────────────────┤
│ 🚨 PENDING GATE: Phase 42 (ship)                                                          │
│ Context: Review failed with CR-01 (Contradicted cadence claim & premature auto-mode)     │
│ [a] Approve   [r] Reject (Loop-to-Code)   [x] Abort   [n] Custom Note   [d] Doctor       │
└─ [Tab] Switch Phase  [Space] Stop  [Enter] Resume  [c] Clean Stale  [q] Quit ────────────┘
```

### Component Breakdown

1. **Header Bar:**
   - Current project path, active git branch, detected milestone target, and global phase completion progress bar.
2. **Left Sidebar (Active Phases):**
   - Sorted list of all active phase states.
   - Status indicators:
     - 🟢 `Running` — Agent is actively executing.
     - 🟡 `Gate Pending` — Awaiting human review/decision.
     - 🔴 `Failed / Error` — Unhandled failure or crashed monitor.
     - ⚪ `Paused / Stale` — Stopped or abandoned session.
   - Stage progress breadcrumbs (`D → P → C → V → S`).
3. **Main Viewport (Multi-Mode):**
   - **Mode 1 (Live Agent Stream):** Auto-scrolling tail of `.devflow/phase-NN-stdout` with ANSI color formatting.
   - **Mode 2 (Event Stream):** Structured timeline of lifecycle transitions from `.devflow/events.jsonl`.
   - **Mode 3 (Review & Diff Viewer):** Markdown viewer for `.planning/phases/NN-*/NN-REVIEW.md` and test artifacts.
4. **Bottom Action / Gate Bar:**
   - Appears or highlights in bright yellow/amber when an active gate requires operator intervention.
   - Supports single-keystroke resolution without switching windows.

---

## 4. Keyboard Shortcuts & Interaction Model

| Key | Context | Action |
|---|---|---|
| `Tab` / `Shift+Tab` / `↑`/`↓` | Global | Cycle through active phases |
| `1`, `2`, `3` | Main View | Switch between Agent Stream, Event Log, and Review Artifacts |
| `a` | Gate Pending | Approve the active gate (`devflow gate approve`) |
| `r` | Gate Pending | Reject and loop back to Code (`devflow gate reject`) |
| `x` | Gate Pending | Abort phase (`devflow gate reject --note "abort"`) |
| `n` | Gate Pending | Open input modal to write a custom rejection note |
| `Space` | Global | Stop / Pause selected phase (`devflow stop --phase N`) |
| `Enter` | Global | Resume selected phase (`devflow resume --phase N`) |
| `c` | Global | Sweep dead/stale phase states (`devflow recover --clean`) |
| `d` | Global | Run environment audit modal (`devflow doctor`) |
| `b` | Global | Toggle audio / terminal bell alerts (`\x07`) on gate arrival |
| `q` / `Esc` | Global | Exit TUI (keeps all background monitors running) |

---

## 5. Technical Implementation Plan

1. **Crate & Dependencies:**
   - Add `ratatui` (v0.29+) and `crossterm` (v0.28+) to `crates/devflow-cli/Cargo.toml`.
   - Use `notify` or a 150ms `tokio` ticker to poll `.devflow/` directory modifications.
2. **Subcommand Registration:**
   - Register `devflow hud` (and alias `devflow tui`) in `crates/devflow-cli/src/main.rs`.
3. **Module Architecture:**
   - `crates/devflow-cli/src/tui/app.rs`: State holder, active phase selection, event loop.
   - `crates/devflow-cli/src/tui/ui.rs`: Layout rendering, widgets, theme formatting.
   - `crates/devflow-cli/src/tui/events.rs`: Crossterm key event handler.
   - `crates/devflow-cli/src/tui/watcher.rs`: File system watcher for `.devflow/`.

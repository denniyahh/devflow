# DEVFLOW_RESULT — Agent Completion Protocol

> Design document for Phase 6 top-priority feature

## Problem

DevFlow currently has no way to know whether a coding agent succeeded or failed. The agent process runs, exits, and `devflow check` blindly advances the state machine. We have no signal from the agent.

## Solution: Structured Completion Signal

DevFlow's rich prompts already tell agents what to do. We extend them with one more instruction: **always output a machine-readable completion marker as your final message.**

### The Marker

```
DEVFLOW_RESULT: {"status": "success"}
```

or

```
DEVFLOW_RESULT: {"status": "failed", "reason": "clippy reported 3 errors that couldn't be fixed automatically"}
```

The marker:
- Is a single line starting with `DEVFLOW_RESULT:` (case-insensitive matching)
- Followed by a JSON object with at minimum a `status` field
- Must be the agent's very last output (or at minimum, in the final ~2000 chars)
- Is included in every agent prompt (Claude, Codex, OpenCode)

### JSON Schema

```json
{
  "status": "success" | "failed",
  "reason": "string (only when failed — what went wrong)",
  "commits": 3,           // optional: number of commits produced
  "summary": "string"     // optional: one-line summary of work done
}
```

### Prompt Injection

Each agent adapter injects this into its generated prompt. Example for Claude:

```
## Completion Protocol (REQUIRED)

After finishing all work, your FINAL message must be exactly:

DEVFLOW_RESULT: {"status": "success"}

If something prevents completion, your final message must be:

DEVFLOW_RESULT: {"status": "failed", "reason": "specific explanation"}

DevFlow reads this to determine whether the phase succeeded. Do NOT output anything after this line.
```

For Codex/OpenCode (simpler agents), the prompt is shorter and more directive:

```
## REQUIRED: Output one of these as your last message:

Success → DEVFLOW_RESULT: {"status": "success"}
Failure → DEVFLOW_RESULT: {"status": "failed", "reason": "why"}

Nothing after this line. DevFlow uses it to track completion.
```

---

## Three-Layer Decision Engine

`devflow check` uses a layered approach — each layer is tried, falling through if unavailable:

```
┌─────────────────────────────────────────────┐
│ LAYER 1: Explicit agent signal              │
│                                             │
│ Search stdout for DEVFLOW_RESULT marker      │
│   Found "success" → advance                 │
│   Found "failed"  → halt, show reason       │
│   Not found       → fall to Layer 2         │
└──────────────────┬──────────────────────────┘
                   │ (no marker found)
┌──────────────────▼──────────────────────────┐
│ LAYER 2: Exit code + commit gate            │
│                                             │
│ Read agent_exit_code from state.json        │
│ Count git log develop..feature/phase-NN     │
│   exit=0, commits>0 → advance (probable ok) │
│   exit=0, commits=0 → halt "no work done"   │
│   exit!=0           → halt "agent failed"   │
│   exit unknown      → fall to Layer 3       │
└──────────────────┬──────────────────────────┘
                   │ (monitor mode, exit lost)
┌──────────────────▼──────────────────────────┐
│ LAYER 3: Process existence + commit gate    │
│                                             │
│ Agent process is gone + commits exist       │
│   → advance with warning "unverified"       │
│ Agent process is gone + no commits          │
│   → halt "no work detected"                 │
└─────────────────────────────────────────────┘
```

Layer 1 is the goal — agents self-report. Layer 2 is the reliable fallback. Layer 3 is the last resort for when even exit codes are unavailable (monitor mode without exit code capture).

---

## Implementation Plan

### Step 1: State struct changes (`state.rs`)

Add to `State`:
```rust
pub agent_result: Option<AgentResult>,  // parsed from DEVFLOW_RESULT or exit code
pub agent_stdout_path: Option<PathBuf>,  // where agent stdout was saved
```

New struct in `state.rs` (or new `agent_result.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub status: AgentStatus,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub commits: Option<u32>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    Success,
    Failed,
    Unknown,  // no signal received, check exit code
}
```

### Step 2: Prompt enhancement (`agents/claude.rs`, `agents/codex.rs`, `agents/opencode.rs`)

Add `DEVFLOW_RESULT` instruction to each agent's prompt generation. The Claude prompt gets the full natural language version. Codex/OpenCode get the shorter version.

### Step 3: Stdout capture (`agent.rs` + `main.rs`)

**Blocking mode** (already captures stdout via `wait_for_agent`):
- After `child.wait()`, save stdout to `.devflow/phase-NN-stdout`
- Parse stdout for DEVFLOW_RESULT marker
- Store parsed result in state

**Monitor mode** (stdout currently lost):
- Before dropping Child, spawn a thin thread that:
  1. Reads child's stdout pipe
  2. `child.wait()` for exit code
  3. Writes stdout to `.devflow/phase-NN-stdout`
  4. Writes exit code to `.devflow/phase-NN-exit`
- This runs in parallel with the monitor daemon
- The daemon already detects process exit via kill -0

### Step 4: `check()` enhancement (`main.rs`)

New function `evaluate_agent_result()`:
```rust
fn evaluate_agent_result(project_root: &Path, state: &State) -> AgentResult {
    // Layer 1: parse DEVFLOW_RESULT from stdout file
    if let Some(result) = parse_devflow_result(project_root, state) {
        return result;
    }
    // Layer 2: exit code + commit gate
    if let Some(result) = evaluate_exit_code_and_commits(project_root, state) {
        return result;
    }
    // Layer 3: existence + commits
    evaluate_existence_and_commits(project_root, state)
}
```

In `check()`, after detecting agent exit:
```rust
let result = evaluate_agent_result(project_root, &state);
match result.status {
    AgentStatus::Success => {
        println!("agent reported success");
        // proceed to advance
    }
    AgentStatus::Failed => {
        return Err(CliError::Message(format!(
            "phase {} failed: {}",
            state.phase,
            result.reason.unwrap_or_default()
        )));
    }
    AgentStatus::Unknown => {
        // Layer 3 fallback — advance with warning
        println!("warning: could not verify agent completion status");
    }
}
```

### Step 5: Marker parsing (`agent_result.rs` — new module)

```rust
fn parse_devflow_result(stdout: &str) -> Option<AgentResult> {
    // Search last 4000 chars of stdout for DEVFLOW_RESULT: marker
    let tail = stdout.chars().rev().take(4000).collect::<String>().chars().rev().collect::<String>();
    for line in tail.lines().rev() {
        if let Some(json_str) = line.strip_prefix("DEVFLOW_RESULT: ")
            .or_else(|| line.strip_prefix("devflow_result: "))
        {
            if let Ok(mut result) = serde_json::from_str::<AgentResult>(json_str) {
                return Some(result);
            }
        }
    }
    None
}
```

### Step 6: Monitor stdout capture (`monitor.rs`)

Instead of the current shell script that only polls kill -0, extend it to also save the agent's exit code by reading `/proc/{pid}/stat` or using a waitpid wrapper:

```sh
# After agent exits (kill -0 fails):
# Read exit code from /proc
exit_code=$(awk '{print $3}' /proc/{agent_pid}/stat 2>/dev/null || echo "unknown")
echo "$exit_code" > .devflow/phase-{phase}-exit
# Now run check
devflow check {project_root}
```

Actually, this won't work reliably. `/proc/{pid}/stat` gives process state, not exit code. The exit code is only available via `waitpid()`. Better approach: the stdout-capture thread also `wait()`s on the child and writes the exit code.

Let me redesign Step 3 for monitor mode:

```rust
// In start(), for monitor mode:
let (mut child, pid) = agent::launch_agent(&state)?;

// Take stdout pipe before spawning monitor
let stdout = child.stdout.take();

// Spawn a thread to capture stdout + exit code
let phase_dir = project_root.join(".devflow");
std::thread::spawn(move || {
    // Read stdout
    let mut buf = String::new();
    if let Some(mut pipe) = stdout {
        let _ = pipe.read_to_string(&mut buf);
    }
    // Write stdout
    let _ = std::fs::write(phase_dir.join(format!("phase-{:02}-stdout", phase)), &buf);
    // Wait for exit
    match child.wait() {
        Ok(status) => {
            let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
            let _ = std::fs::write(phase_dir.join(format!("phase-{:02}-exit", phase)), &code);
        }
        Err(_) => {}
    }
});

// Spawn monitor (same as before)
monitor::spawn_monitor(&state)?;
```

This is cleaner — the stdout thread waits on the child (which gives us the exit code), writes both to files, and the monitor/check reads them later.

---

## Files Changed

| File | Change | Lines |
|---|---|---|
| `state.rs` | Add `AgentResult`, `AgentStatus`, new State fields | +40 |
| `agent_result.rs` | NEW — marker parsing, exit code evaluation | +80 |
| `agents/claude.rs` | Add DEVFLOW_RESULT to prompt | +15 |
| `agents/codex.rs` | Add DEVFLOW_RESULT to prompt | +8 |
| `agents/opencode.rs` | Add DEVFLOW_RESULT to prompt | +8 |
| `main.rs` start() | Stdout capture thread for monitor mode | +20 |
| `main.rs` check() | evaluate_agent_result() call + gating | +25 |
| `main.rs` wait_for_agent() | Save stdout to file + parse result | +15 |
| `monitor.rs` | No change needed (thread handles exit capture) | 0 |
| `lib.rs` | Add `pub mod agent_result` | +1 |
| **Total** | | **~212** |

## Edge Cases

| Scenario | Behavior |
|---|---|
| Agent outputs DEVFLOW_RESULT mid-work (then continues) | We scan LAST occurrence in stdout tail |
| DEVFLOW_RESULT appears in committed code | Marker is only in agent's stdout, not in repo files. Even if it appears, we search the last 4000 chars — code output is usually mid-stream |
| Agent crashes (no stdout) | Layer 1 not found → Layer 2: exit code non-zero → halt |
| Agent produces work but forgets marker | Layer 1 not found → Layer 2: exit=0, commits>0 → advance |
| Monitor mode, agent exits before stdout thread reads | `read_to_string` on pipe returns EOF (what was written so far) — partial output is better than nothing |
| JSON in reason field has special chars | Standard JSON escaping handles this |

## Verification

```bash
# Unit tests
cargo test agent_result          # marker parsing, edge cases

# Integration test: agent that outputs success
echo 'DEVFLOW_RESULT: {"status":"success"}' > .devflow/phase-01-stdout
cargo run -- check              # Should advance

# Integration test: agent that outputs failure
echo 'DEVFLOW_RESULT: {"status":"failed","reason":"test"}' > .devflow/phase-01-stdout
cargo run -- check              # Should halt with error

# Integration test: no marker, exit=0, commits exist
echo "" > .devflow/phase-01-stdout
echo "0" > .devflow/phase-01-exit
# (feature branch has commits)
cargo run -- check              # Should advance with warning

# Integration test: no marker, exit≠0
echo "1" > .devflow/phase-01-exit
cargo run -- check              # Should halt with failure
```

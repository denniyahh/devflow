# Phase 31: Claude Adapter Launch Path — Pipe-Owning Monitor (999.64 arc close) - Research

**Researched:** 2026-08-03
**Domain:** Rust child-process supervision (std-only) + Claude Code CLI `stream-json` wire protocol
**Confidence:** MEDIUM-HIGH — the wire protocol and current source are directly verified; the
monitor's *internal* architecture is genuinely novel to this codebase (no async runtime, no prior
pipe-owning precedent), so implementation-idiom claims are marked accordingly.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Idle-timeout policy**
- **D-01:** The idle timer resets on **every stream line** the monitor reads, not on milestone
  events only. 30d measured every-line gaps at 6.02–7.09s versus milestone gaps at 7.70–13.73s;
  the tighter signal carries more margin under a given timeout. It also rejects the
  "chatty but stuck" blind spot that a both-signals rule would have created.
- **D-02:** Idle timeout is **30s**, the measured constraint-8 floor. The ≥30s floor was derived
  from the *milestone* signal (pooled max 13.73s). Against the every-line signal chosen in D-01
  the observed max is 7.09s, so 30s is ~4.2x margin — comfortable, not marginal. Do not "correct"
  this to a larger value on the assumption that 30s is tight; it is tight only against a signal
  this phase is not using.
- **D-03:** **No outer wall-clock bound.** Idle-only. Constraint 5 rejected fixed wall-clock
  because no single value is safe for both hangs and legitimate ~47-minute stages; a healthy long
  stage keeps resetting the idle timer and is already protected.
- **D-04:** The timeout is **configurable but clamped** so it can never be set below the 30s
  floor, and the clamp **logs loudly** when it engages. Because the default is the floor, the
  value can only be raised. A silent clamp would be the exact failure class this project keeps
  paying for.

**What firing the timeout does**
- **D-05:** On firing, **write the authoritative result to disk first, then terminate the
  child.** The result must exist before anything can race it, so neither Layer 2 nor an exit-code
  path can overwrite the verdict. Reversibility: costly.
- **D-06:** An idle timeout records a **distinct first-class status** (e.g.
  `AgentStatus::IdleTimeout`), separate from both `Failed` and `ResourceKilled`. Reversibility:
  costly — touches every exhaustive match on `AgentStatus`.
- **D-07:** The recorded result **fails loudly and enumerates the commits** the agent made before
  going quiet. Do **not** roll the commits back.
- **D-08:** An idle timeout is **terminal, not retryable**. Stop at a never-silent gate and
  report.

**Rollout shape**
- **D-09:** **Sequence the rollout: one stage first, then widen.** An explicit sequencing choice,
  which constraint 1 permits (constraint 1 forbids predicting *behavior*, not sequencing).
- **D-10:** **Code is the first stage.** It is where 999.64 was observed and the stage that
  actually backgrounds.
- **D-11:** An **explicit opt-out flag, off by default**, can force the old single-document path.
  Its use must be logged loudly. **Automatic fallback on parse failure is rejected.**
- **D-12:** The shipped single-document path is protected by **30b's existing isolation tests** —
  treat those as the contract this phase must not break, rather than adding a full adapter-matrix
  test that would strain the M cap.

**CLI-behaviour guard (review M2)**
- **D-13:** The guard is a **startup canary with a declared token**, not a version-string check.
  One throwaway task at pipeline start declares the success token it will return; the
  orchestrator records the token up front and confirms it comes back. Two traps: (1) **Prompt
  echo** — the planted token *will* appear in the stream as an echo, so it must be matched
  **only inside a top-level `result` event**, reusing `is_top_level` / the stream-gate predicate
  rather than a new trust path. (2) **It proves delivery, not work** — the agent can see the
  token in its own prompt and could emit it without doing anything; summaries and merges remain
  the evidence of work (D-16).
- **D-14:** Scope of the token mechanism is the **startup canary only**, not every dispatched
  child. Per-child tokens are a deferred idea (would push past M).
- **D-15:** When the guard reports the behaviour absent or unverified, **refuse to run and report
  clearly**. Runs **once per run**, outcome recorded in the run's provenance.

**Acceptance run mechanics**
- **D-16:** The acceptance workload is a **minimal purpose-built two-plan wave** — cheapest
  workload that still crosses every seam under test.
- **D-17:** It runs on the **main checkout**, orchestrator touching no git at all while the
  executor holds the tree. This rule is in `CLAUDE.md` and is binding.
- **D-18:** **Pass = both plans produce a `SUMMARY.md` and merge.** Explicitly **not** "the stage
  reports Success" and **not** "both completions observed in the stream" (constraint 7 makes an
  observed count the very signal that can undercount).
- **D-19:** If the acceptance run fails, **the phase does not close** — diagnose and re-run.

### Claude's Discretion
- Where the constraint-9 exit-code guard physically lives in the layer cascade (monitor vs
  `evaluate_agent_result`) — implementation approach, deliberately not asked.
- The internal structure of the pipe-owning monitor and how it replaces `spawn_monitor`'s `sh`
  script.
- Near-simultaneous-completion test design (review M3) — resolved against constraint 7's
  coalescing evidence.

### Deferred Ideas (OUT OF SCOPE)
- **Per-child declared tokens** (from D-14) — extending the token mechanism from the startup
  canary to every dispatched child. Strong candidate for its own phase once 999.64 is closed and
  real captures exist.
- 999.65, 999.66, 999.46, 999.70, 999.71, anything release-related.
</user_constraints>

<phase_requirements>
## Phase Requirements

This project tracks infrastructure phases by ROADMAP constraint number, not REQ-ID (`.planning/REQUIREMENTS.md` does not exist). The binding constraints assigned to Phase 31:

| ID | Description | Research Support |
|----|-------------|------------------|
| Constraint 1 | Always-on `--input-format stream-json --output-format stream-json` adapter switch, never selected by predicting which stages background. Per-stage rollout permitted as sequencing (D-09/D-10). | See "The prompt is no longer positional argv" (Architecture Patterns, Pattern 1) — the argv AND the prompt-delivery mechanism both change together. Verified against `agents/claude.rs:15-31` and all three Phase 30 harnesses. |
| Constraint 4 | Close rule is an `AND`: marker in a top-level `result` event **AND** `background_tasks_changed` drained to `[]`. Drain arm is defensive, not load-bearing (30-04), but must be kept. | See "Close-rule and idle-timer design" (Pattern 2); 30d-MEASUREMENTS.md quantifies the drain-to-final-result lag (4.54–11.51s) that makes early-drain-as-stop-signal unsafe. |
| Constraint 5 / 8 | Idle timeout, not wall-clock, floor ≥30s (D-01–D-04); on firing, write authoritative first-class result before terminating. | See "Idle-timeout implementation" (Pattern 2) and Pitfall 3 ("an appended idle-timeout marker can be shadowed by a stale stream `result` event") — a verified, non-obvious landmine in the existing cascade. |
| Constraint 7 | Monitor must not count `result` events as the count of children that returned (coalescing). | Covered under "Don't Hand-Roll" / existing `background_tasks_changed` drain semantics; test-design guidance under Validation Architecture. |
| Constraint 9 (residual) | Stream-derived `Success` must not short-circuit a contradicting exit code (boundary-truncation residual). | Traced through `evaluate_agent_result_inner`'s `Layer0 → Layer1 → Layer2` `.or_else` cascade (`agent_result.rs:1855-1878`); Layer 1 always wins today. See Pitfall 4. |
| ROADMAP §999.67 | `parse_devflow_result` provenance-forging fold-in. | **Verified already closed in source** (commit `a557805`, tests passing live). See "999.67 is already fixed — do not re-plan it" below. |

</phase_requirements>

## Summary

This phase has one genuinely new piece of information beyond what CONTEXT.md and the ROADMAP
already settled: **the prompt can no longer travel as positional argv once `--input-format
stream-json` is set.** Every one of Phase 30's three experiment generations (30a's `run_experiment_v3.py`,
30c's `30c-monitor-env-harness.py`, 30d's harness) launches `claude -p --input-format stream-json
--output-format stream-json --verbose --dangerously-skip-permissions` with **no positional prompt
argument at all**, then writes `{"type":"user","message":{"role":"user","content":<prompt>}}\n` to
the child's stdin and flushes it. This is the actual, tested mechanism the arc's premise (delivery
confirmed 7/7) rests on — not an assumption, it is what `DEFAULT_CLI_ARGV` + the `proc.stdin.write`
call literally do in every archived harness. `agents/claude.rs`'s current `exec_command` embeds the
prompt as `args[1]`; that construction is incompatible with the tested wire shape and must change
alongside the flag switch, not as a separate follow-on.

The second load-bearing finding is architectural: because the prompt now arrives via a stdin
write and the idle timer resets on stdout reads, the monitor must perform both a write and a read
concurrently on two independent pipes to the same child — a classic pipe-deadlock shape if done
naively in one thread. The codebase has no async runtime (no `tokio` anywhere in the workspace)
and no existing precedent for concurrent-pipe process supervision (`sequentagent`, the prior
candidate, was removed in Phase 23). The standard std-only idiom — a dedicated writer thread for
stdin, a reader loop on stdout feeding an `mpsc::channel` that a timer loop drains with
`recv_timeout` — is documented below with the concrete APIs, none of which are novel to Rust but
all of which are novel to *this* codebase.

Third: reading the full `evaluate_layer1`/`or_else` cascade in source shows that **simply
appending an idle-timeout marker to the capture file is not sufficient** to make it win. If a real
`type:"result"` event already exists in the stream (extremely likely on any run long enough to
idle-time-out, since these are exactly the multi-turn task-notification streams), `parse_claude_event_result`
returns that event's marker regardless of what text follows it, and the `.or_else` cascade never
reaches `parse_devflow_result`'s raw-tail scan where the appended marker lives. D-05's "write the
result first" instruction is right about ordering in time; it needs an additional instruction
about *where* the write goes, because the obvious place (append to the stdout capture) can be
silently shadowed.

Fourth, an unrelated but material scope reduction: **ROADMAP §999.67 ("fold in, XS") is already
fixed in source.** The `a557805` root-cause refactor committed 2026-08-02 (before this phase's
ROADMAP entry was written 2026-08-03) already applies `normalise_stream_marker_provenance` on both
arms of `parse_devflow_result`, with regression tests `generic_marker_cannot_forge_layer0_provenance`
and `codex_marker_cannot_forge_layer0_provenance` both present and passing (`1 passed` each,
re-run live this session). The planner should not schedule this as new work — only verify test
coverage is adequate and drop the line item.

**Primary recommendation:** switch the Claude adapter's prompt-delivery mechanism from positional
argv to a stdin-written JSON `user` message (matching the exact shape Phase 30 tested), build the
pipe-owning monitor as a std-only two-thread (writer + reader) supervisor with an `mpsc`
idle-timer loop, and give the idle-timeout verdict its own side-channel (not an append to the
existing capture file) so it cannot be shadowed by a stale stream `result` event.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Agent process launch (argv, env) | Backend / CLI (`devflow-core::agents`) | — | `AgentAdapter::exec_command` already owns this; only the Claude arm's shape changes |
| Prompt delivery | Backend / CLI (new: monitor) | Backend / CLI (`agents::claude`) | Moves from argv-embedded (adapter's responsibility) to a stdin write the *monitor* performs after spawn — a new cross-cutting responsibility that doesn't fit the existing `exec_command` return shape |
| Stream parsing / verdict cascade | Backend / CLI (`devflow-core::agent_result`) | — | Already exists (Phase 30); Phase 31 makes it reachable, does not change its logic except the constraint-9 residual and (new) idle-timeout precedence |
| Idle-timer / close-rule supervision | Backend / CLI (new: monitor) | — | Entirely new; no prior tier owns "watch a live child's stdout for liveness" |
| Process termination (SIGTERM→SIGKILL, group) | Backend / CLI (`devflow-core::agent`, extended) | new: monitor | `terminate_and_verify` exists for single-pid termination; process-*group* termination for a directly-spawned (non-shell) child is new |
| Startup canary (D-13) | Backend / CLI (new: pipeline_launch or monitor) | — | Not a natural fit for the existing per-stage `AgentAdapter::preflight` hook (see Pitfall 5) — needs its own once-per-run gate |
| Exit-code vs stream-Success arbitration (constraint 9 residual) | Backend / CLI (`evaluate_agent_result_inner`) | monitor (writes the exit file) | Discretion item per CONTEXT.md; both plausible homes are in the same tier, so this is an intra-tier placement question, not a cross-tier one |

## Standard Stack

### Core
No new external dependency is required or recommended. Everything needed is already a workspace
dependency:

| Library | Version (Cargo.toml, verified) | Purpose | Why standard here |
|---------|---------|---------|--------------|
| `libc` | `0.2` (`devflow-core/Cargo.toml:19`) | `kill`, `sigaction`/`signal`, `setsid`/`setpgid` primitives for process-group control | Already the crate's sole process-signalling dependency (`agent.rs`'s `terminate`/`terminate_and_verify`); reusing it keeps the monitor's process-control code in the same idiom as everything else in this crate |
| `serde_json` | workspace (`Cargo.toml`) | Parsing the stream-json JSONL wire format | Already what `agent_result.rs`'s entire parser is built on |
| `std::process` | stdlib | `Command`, `Child`, `Stdio::piped()`, `ChildStdin`/`ChildStdout` | The only process-spawning primitive in the workspace — **no async runtime exists anywhere in this workspace** (`rg` for `tokio`/`async-` across every `Cargo.toml` returns nothing) `[VERIFIED: crates/devflow-core/Cargo.toml, crates/devflow-cli/Cargo.toml — read this session]` |
| `std::sync::mpsc` | stdlib | Channel from a stdout-reader thread to an idle-timer loop | Standard std-only idiom for "reset a timeout on activity from another thread" without pulling in an executor |

### Supporting
| Library | Version | Purpose | When to use |
|---------|---------|---------|-------------|
| none | — | — | A `signal-hook`-style crate is a plausible alternative for SIGTERM/SIGINT handling in the new monitor process, but this repo's existing pattern (`agent.rs`) hand-rolls signal delivery via raw `libc::kill`/`libc::signal` and introduces no signal-handling crate anywhere. Adding one here would be the only crate of its kind in the workspace for a problem the existing `libc` dependency already solves. **Not recommended** — flagged in Don't Hand-Roll below as the opposite case (do less, not more). |

### Alternatives Considered
| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| std threads + `mpsc` for idle-timer | `tokio` async runtime | Correctness-neutral, but introduces the workspace's first async runtime for a single call site — a much larger blast radius than the problem needs, and every other adapter/monitor path (Codex, OpenCode) would remain sync, creating a split execution model in one binary |
| Re-exec the `devflow` binary as a hidden monitor subcommand | Spawn a raw `claude` child directly from inside `devflow start`'s own process and detach via a double-fork/daemonize | The re-exec approach mirrors the current `sh` script's shape most closely (a distinct OS process that outlives `devflow start`'s own exit) and needs no new libc primitives beyond what a normal `Command::spawn()` + not-waiting already gives; a hand-rolled double-fork daemonize duplicates process-supervision logic this codebase does not currently have anywhere. See Open Questions — this is Claude's discretion per CONTEXT.md, not resolved here. |

**Installation:** none — no `cargo add` step for this phase; every crate needed is already a
dependency in `crates/devflow-core/Cargo.toml`.

**Version verification:** N/A, no new packages.

## Package Legitimacy Audit

**No external packages are introduced by this phase.** Every crate the implementation needs
(`libc 0.2`, `serde_json`, `tracing`, `thiserror`) is already a pinned workspace dependency,
verified by reading `crates/devflow-core/Cargo.toml` and `crates/devflow-cli/Cargo.toml` this
session (see Standard Stack above). No `npm view`/`pip index`/`cargo search` gate applies.

**Packages removed due to `[SLOP]` verdict:** none.
**Packages flagged as suspicious `[SUS]`:** none.

## Architecture Patterns

### System Architecture Diagram

```
devflow advance / devflow start (foreground CLI, exits quickly)
        │
        │  spawn (detached, not waited on)
        ▼
┌───────────────────────────────────────────────────────────────┐
│  Pipe-owning monitor process (new — replaces the `sh` script) │
│                                                                 │
│  1. Command::new("claude")                                    │
│       .args([-p, --input-format, stream-json,                 │
│              --output-format, stream-json, --verbose,         │
│              --dangerously-skip-permissions])                 │
│       .current_dir(workdir)                                   │
│       .stdin(Stdio::piped())   .stdout(Stdio::piped())         │
│       .stderr(Stdio::from(stderr_file))                       │
│       .process_group(0)  [own process group — see Pitfall 6]  │
│       .spawn()                                                │
│                                                                 │
│  2. writer thread: writes                                      │
│       {"type":"user","message":{"role":"user",                │
│         "content":"<stage prompt (+ D-13 canary token)>"}}     │
│       to child.stdin, flush, DO NOT close yet                 │
│                                                                 │
│  3. reader loop (main thread): BufReader::lines() on           │
│     child.stdout →                                             │
│       a) append line verbatim to phase-NN-stdout (tee,         │
│          preserves the existing capture-file contract)         │
│       b) send line over mpsc to the idle-timer loop            │
│       c) inspect for: background_tasks_changed→[] (drain),     │
│          top-level result event (marker / canary token)        │
│                                                                 │
│  4. idle-timer loop: recv_timeout(30s, configurable-but-        │
│     clamped). Timeout ⇒ write authoritative IdleTimeout        │
│     result to its OWN side-channel file (NOT appended to       │
│     phase-NN-stdout — see Pitfall 3), THEN terminate the        │
│     child (process-group SIGTERM → SIGKILL escalation,          │
│     mirroring agent::terminate_and_verify)                     │
│                                                                 │
│  5. close rule (constraint 4): close stdin only when            │
│     (marker seen in a top-level result event) AND               │
│     (background_tasks_changed drained to [])                   │
│                                                                 │
│  6. on exit: write $?/.code() to phase-NN-exit, then            │
│     continue the pipeline (re-exec `devflow advance` OR         │
│     call the advance logic in-process — Open Question)          │
└───────────────────────────────────────────────────────────────┘
        │
        ▼
existing cascade (unchanged logic, now reachable):
  evaluate_layer0 → evaluate_layer1 (parse_claude_event_result,
  now fed real production stream-json) → evaluate_layer2 (exit
  code + commit gate) → evaluate_layer3
```

### Recommended Project Structure

No new files are architecturally required — this is a modification of three existing files plus
new test modules within them:

```
crates/devflow-core/src/
├── monitor.rs           # spawn_monitor rewritten: pipe-owning, not sh-script-owning
├── agents/claude.rs      # exec_command: drops the prompt from argv; new argv shape
├── agent_result.rs        # AgentStatus::IdleTimeout variant + its wire-format/match-arm sites;
│                           # idle-timeout side-channel reader; constraint-9 residual wiring
└── (new, in monitor.rs or a sibling module) the writer-thread / idle-timer-loop supervisor code
```

### Pattern 1: The prompt moves from argv to a stdin-written JSON message

**What:** Under `--input-format stream-json`, the CLI's positional `prompt` argument is dropped
entirely; the initial user turn is instead the first JSONL line written to stdin, in the shape
`{"type":"user","message":{"role":"user","content":"<prompt text>"},"parent_tool_use_id":null}`
(the trailing `parent_tool_use_id` key is optional — every archived trial omits it and
`is_top_level` treats absence as top-level).

**When to use:** Every Claude adapter launch, once the always-on switch (constraint 1) lands.

**Verified evidence (not a docs claim alone):**
```python
# Source: .planning/phases/30-keep-the-session-alive-past-turn-end/30c-monitor-env-harness.py:255-264,341,355-356
# [VERIFIED: 30c-monitor-env-harness.py:255-264,341,355-356 — read this session]
DEFAULT_CLI_ARGV = [
    "claude", "-p",
    "--input-format", "stream-json",
    "--output-format", "stream-json",
    "--verbose",
    "--dangerously-skip-permissions",
]
# ... argv = list(argv or DEFAULT_CLI_ARGV)   # no positional prompt anywhere in argv
proc.stdin.write(json.dumps({"type": "user", "message": {"role": "user", "content": prompt}}) + "\n")
proc.stdin.flush()
```

The same shape appears in 30a's `run_experiment_v3.py:58-59,67-73` (`[VERIFIED: 30a-evidence/run_experiment_v3.py:58-59,67-73]`)
and is corroborated by the official docs' `SDKUserMessage` type (`type: "user"`, `message:
MessageParam`, `parent_tool_use_id: string | null`) `[CITED: code.claude.com/docs/en/agent-sdk/typescript]`.

**This is not optional cosmetic wiring.** The current `ClaudeAgent::exec_command`
(`agents/claude.rs:15-31`) constructs:
```rust
// [VERIFIED: crates/devflow-core/src/agents/claude.rs:15-31 — read this session]
(
    "claude",
    vec![
        "-p".into(), prompt.to_string(),
        "--output-format".into(), "json".into(),
        "--dangerously-skip-permissions".into(),
    ],
)
```
Adding `--input-format stream-json` to this argv while *keeping* `prompt.to_string()` positional
is an **untested configuration** — every trial this arc's confirmed-delivery verdict rests on
launched with no positional prompt and delivered the turn via stdin instead. The `AgentAdapter`
trait's `exec_command(&self, phase, prompt, extra_writable_roots) -> (&'static str, Vec<String>)`
signature has no way to express "and also write this to stdin after spawn" — that responsibility
does not belong in the adapter trait as it exists today; it has to become new logic in the
monitor (see Architectural Responsibility Map). This is the single most consequential correction
this research makes to the phase's premise as stated in CONTEXT.md/ROADMAP, which describe the
change purely as "the argv D-09/D-10 flip" without mentioning the prompt-delivery-channel change
it forces.

**Codex/OpenCode are unaffected** — their adapters keep prompt-as-argv; only the Claude arm's
`exec_command` (or the monitor's Claude-specific branch) needs the new shape.

### Pattern 2: Idle-timer + close-rule supervision (std-only, no async runtime)

**What:** A dedicated stdout-reader thread parses lines and forwards them (or just a "line seen"
signal) over an `mpsc::channel` to a supervisor loop that calls `recv_timeout(idle_duration)` in a
loop — every successful `recv` resets the timeout window (D-01: reset on every line, not just
milestone events), and a `RecvTimeoutError::Timeout` is the idle-timeout firing.

**When to use:** This is the core of the pipe-owning monitor rewrite (constraint 5/8, D-01–D-05).

**Concrete idiom** (`[ASSUMED]` — standard Rust systems-programming pattern, not verified against
an existing implementation in this codebase since none exists; the crate/API names themselves are
stdlib and their behavior is documented Rust, not adapter-specific):
```rust
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::time::Duration;

// child spawned with .stdin(Stdio::piped()).stdout(Stdio::piped())
let mut stdin = child.stdin.take().expect("piped stdin");
let stdout = child.stdout.take().expect("piped stdout");

// Writer thread: sends the initial user-message JSON, does NOT close stdin
// (closing is a separate, later decision gated on the close-rule, constraint 4).
std::thread::spawn(move || {
    let msg = serde_json::json!({"type": "user", "message": {"role": "user", "content": prompt}});
    let _ = writeln!(stdin, "{msg}");
    let _ = stdin.flush();
    // `stdin` is moved into this thread and dropped here only once told to close —
    // in practice this thread needs to itself receive the close signal (e.g. via
    // a second channel or an Arc<Mutex<Option<ChildStdin>>>) rather than closing
    // immediately after the write, or constraint 4's AND condition can never be
    // honoured. Left as an explicit structural note for the planner, not resolved.
});

// Reader thread: tees each line to the capture file and forwards a liveness signal.
let (tx, rx) = mpsc::channel::<String>();
std::thread::spawn(move || {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        match line {
            Ok(l) => { /* append `l` to phase-NN-stdout here */ let _ = tx.send(l); }
            Err(_) => break, // pipe closed / read error — treat as EOF
        }
    }
    // tx dropped here -> rx.recv_timeout will eventually see Disconnected
});

// Idle-timer loop (main thread or a third thread):
let idle_timeout = Duration::from_secs(30); // clamped-configurable, D-04
loop {
    match rx.recv_timeout(idle_timeout) {
        Ok(line) => { /* inspect `line` for close-rule / canary conditions */ }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // D-05: write authoritative result FIRST, then terminate.
            // See Pitfall 3 for WHERE to write it.
            break;
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            // Child's stdout closed (process exited or pipe broke) — not an idle
            // timeout; fall through to the ordinary exit-code path.
            break;
        }
    }
}
```

**Process-group termination**, mirroring the already-proven Rust pattern in this crate
(`[VERIFIED: crates/devflow-core/src/agent.rs:75-159]` — `terminate`/`terminate_and_verify`
already implement SIGTERM-then-SIGKILL-escalation with a verified liveness re-check via
`agent_running`/`/proc/<pid>/status`), extended to a *process group* rather than a lone pid:
```rust
// [ASSUMED] — extends the verified single-pid pattern above to group scope.
use std::os::unix::process::CommandExt;

let child = std::process::Command::new("claude")
    // ... args, stdio ...
    .process_group(0) // makes the child its own process-group leader (its pid == its pgid)
    .spawn()?;
let pgid = child.id() as libc::pid_t;
unsafe { libc::kill(-pgid, libc::SIGTERM) }; // negative pid = whole group
// then poll agent_running-style, escalate to `libc::kill(-pgid, libc::SIGKILL)` on timeout,
// exactly as agent::terminate_and_verify already does for a single pid.
```
`std::os::unix::process::CommandExt::process_group` has been stable since Rust 1.64; this
workspace's `rustc 1.97.1` / edition 2024 `[VERIFIED: Cargo.toml:10, rustc --version — checked
this session]` supports it with no new dependency.

### Anti-Patterns to Avoid
- **Writing the prompt to stdin and reading stdout in the same thread, in that order.** If the
  prompt is large (DevFlow's stage prompts embed substantial context) and the child hasn't started
  draining stdin fast enough, or the child starts emitting `stdout` before the parent has finished
  its `stdin` write, this is the textbook two-pipe deadlock: the child blocks writing to a full
  stdout pipe (nobody reading it yet) while the parent blocks writing to stdin (child not draining
  it). Always give the write and the read independent execution contexts (thread or non-blocking
  I/O) before either can block. Pipe buffer size on Linux is commonly 64KiB — a DevFlow stage
  prompt can plausibly exceed that in a single write.
- **Reusing `sh -c` for the new monitor.** The current `sh` script's only reason to exist is that
  it can hold `stdin`/`stdout` open across the CLI process's own exit while running as a detached
  background job. A native Rust monitor process gets this for free via `Command::spawn()` (not
  `.wait()`-ed) plus piped stdio — and removing the shell layer also removes the literal-argv
  injection-safety trick (`spawn_monitor_treats_agent_args_as_literal_argv`,
  `[VERIFIED: crates/devflow-core/src/monitor.rs:606-638]`) as a *category*, since there is no
  shell left to inject into. This is a net simplification, not a discretionary style choice.
- **Treating `background_tasks_changed` draining to `[]` as a stop signal on its own.** 30d
  measured the drain-to-final-`result` lag at 4.54–11.51s across 14 trials
  `[VERIFIED: 30c-VERDICT-reliability.md — "Drain → last result" row, read this session]`; closing
  at the drain would have truncated the final orchestrator turn in every one of the seven 30d
  trials. The `AND` in constraint 4 is load-bearing precisely here.

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| SIGTERM→SIGKILL escalation with a verified liveness re-check | A new kill-and-hope routine in the monitor | `crate::agent::terminate_and_verify` (extended to accept a negative/group pid, or a thin group-aware sibling) | Already exists, already has a documented 999.44-driven rationale for why the SIGKILL escalation is not optional, already tested |
| Detecting whether a pid is still alive | Re-parsing `/proc/<pid>/status` ad hoc | `crate::agent::agent_running` | Already handles the `pid<=0`/wraparound hazards this crate has been bitten by before (`kill(0,...)`, `kill(-1,...)`) |
| A clamped, loudly-logging env-var config reader (D-04) | Anything modeled on `DEVFLOW_GATE_TIMEOUT_SECS`'s *shape* | A new function, written fresh | **Correction to CONTEXT.md's citation:** `config_parse.rs`'s existing timeout readers
(`gate_timeout_secs`, `foreground_gate_timeout_secs`, `checkout_lock_timeout`) all follow a
"parse-or-default" pattern (`[VERIFIED: crates/devflow-cli/src/config_parse.rs:16-71 — read this
session]`) — **none of them clamp against a floor, and none of them log when a fallback engages.**
The one function that *does* have fail-safe-on-a-floor semantics, `parse_gate_max_unattended_age`
(`config_parse.rs:80-86`), silently substitutes the default on `Some(0)` with **no log line**
either. There is no existing "clamp below floor + log loudly" precedent anywhere in this
workspace to imitate — the planner should write this fresh, keeping only the *pure-function,
env-access-free, unit-testable* shape all four existing readers share (`[VERIFIED:
crates/devflow-cli/src/config_parse.rs:100-168 — every test in this file calls the pure `parse_*`
function directly, never mutates process env]`), which is worth reusing for its own sake |
| An agent-provenance-forging fix for `parse_devflow_result` (999.67) | Anything — this is done | N/A | See "999.67 is already fixed" below |
| Signal handling (SIGTERM/SIGINT) for the new monitor process | A `signal-hook`-style crate | Raw `libc::signal`/`sigaction`, matching `agent.rs`'s existing style | This workspace has exactly one process-signalling dependency (`libc`) and zero signal-handling crates; introducing one for this phase alone would be the first of its kind here for a problem the existing dependency already solves |

**Key insight:** almost everything this phase needs at the *signal/process* layer already exists
in `agent.rs` and only needs to be reached from a new call site (the monitor) or generalized from
single-pid to group-pid. The genuinely new work is entirely at the *pipe I/O and stream-parsing
integration* layer, where this codebase has zero precedent — treat that half of the design with
more scrutiny than the process-control half.

## Common Pitfalls

### Pitfall 1: Assuming the argv flip is the whole change
**What goes wrong:** Implementing only "`--output-format json` → `--input-format stream-json
--output-format stream-json`" while leaving the prompt as `args[1]` (positional).
**Why it happens:** CONTEXT.md and the ROADMAP both describe this as "the argv D-09/D-10 flip,"
which reads as argv-only.
**How to avoid:** Move the prompt out of argv entirely for the Claude adapter; deliver it as the
first stdin JSON line, exactly matching the shape in Pattern 1. Any acceptance-run smoke test
should assert the CLI actually receives and acts on the prompt (not just that it starts).
**Warning signs:** A live run producing an `init` event but the agent immediately asking "what
would you like me to do?" — a sign the CLI started headless with no first turn, because the
positional prompt was ignored under `--input-format stream-json` (unconfirmed by docs whether it
errors, is ignored, or is treated as a second message — this exact behavior was not directly
tested in Phase 30 and is flagged as an Open Question below).

### Pitfall 2: Two-pipe write/read deadlock
**What goes wrong:** Writing the initial prompt to `child.stdin` synchronously before starting to
read `child.stdout`, in a design where both operations happen in a single thread.
**Why it happens:** It looks sequential and correct — "send the prompt, then start listening" —
and works fine in quick manual tests with short prompts.
**How to avoid:** Give the stdin write and the stdout read independent threads (Pattern 2) from
the moment the child is spawned.
**Warning signs:** A monitor that hangs specifically on longer/more context-heavy stage prompts
but works on trivial smoke-test prompts.

### Pitfall 3: An appended idle-timeout marker can be silently shadowed by a stale stream `result`
**What goes wrong:** If the monitor's response to an idle timeout is to append a
`DEVFLOW_RESULT: {"status":"idle_timeout",...}` line to the existing `phase-NN-stdout` capture
file (the natural-looking place, since that's what `parse_devflow_result`'s raw-tail scan reads),
it will frequently **not be seen**. `evaluate_layer1`'s cascade
(`[VERIFIED: crates/devflow-core/src/agent_result.rs:1519-1528]`) is:
```rust
detect_claude_rate_limit(&stdout).map(rate_limited_result)
    .or_else(|| detect_claude_envelope_failure(&stdout))
    .or_else(|| parse_claude_event_result(&stdout))   // <-- runs BEFORE parse_devflow_result
    .or_else(|| parse_devflow_result(&stdout))         // <-- the appended marker lives here
    .or_else(|| parse_codex_event_result(&stdout))
    .or_else(|| detect_codex_rate_limit(&stdout).map(rate_limited_result))
```
`parse_claude_event_result` selects `last_top_level_result` — the last `type:"result"` **JSON
event** in the stream — independent of what plain-text lines follow it
(`[VERIFIED: crates/devflow-core/src/agent_result.rs:1359-1406]`, specifically
`last_top_level_result(&capture.events)?` at line 1386). If a real, earlier `result` event already
carries a success marker (routine on any run long enough to idle-time-out — these are exactly the
multi-turn task-notification streams where an earlier turn plausibly already succeeded on
*something*), `parse_claude_event_result` returns `Some(stale_success)` and the `.or_else` chain
**never reaches** `parse_devflow_result` at all — the appended idle-timeout line is invisible.
**Why it happens:** the cascade is `.or_else()`-short-circuiting by construction (this is exactly
constraint 9's residual, one layer up — Layer 1 internally has the same "earlier authoritative
thing shadows a later one" shape that constraint 9 names between Layer 1 and Layer 2).
**How to avoid:** give the idle-timeout verdict its own side-channel that a cascade step checks
*before* `parse_claude_event_result` gets a chance to return a stale success — e.g. a dedicated
file (`phase-NN-idle-timeout` or similar) that `evaluate_agent_result_inner` (or a new
Layer-0.5-style check) reads before calling `evaluate_layer1`, mirroring how Layer 0's external
post-condition already runs before Layer 1 today (`agent_result.rs:1862-1869`). This is squarely
"where the idle-timeout write physically lives," left to Claude's discretion per CONTEXT.md — but
the discretion is bounded by this finding: appending to the existing stdout capture is a real
correctness bug waiting to happen, not a style choice.
**Warning signs:** an idle-timeout test that only exercises a *marker-less* stream (no prior
`result` event) will pass while the exact same mechanism silently fails on a stream that already
had one successful task-notification turn before the hang. Any idle-timeout regression test MUST
include a prior real `result` event in its fixture, or it is testing the easy case only.

### Pitfall 4: Constraint 9's residual — same shape, one layer up
**What goes wrong:** Layer 1 (stream parser) reports `Success` from a torn/boundary-truncated
capture whose real tail (where a non-zero exit would show up) never arrived; because
`evaluate_agent_result_inner`'s cascade (`agent_result.rs:1855-1878`) only calls `evaluate_layer2`
when Layer 1 returns `None`, a Layer-1 `Success` always wins over a contradicting exit code.
**Why it happens:** this is the existing, by-design `.or_else` shape — Layer 1 is "authoritative"
precisely so it doesn't need Layer 2's slower git-log-based fallback in the ordinary case.
**How to avoid:** per CONTEXT.md this is explicitly Claude's discretion (monitor vs
`evaluate_agent_result`), but whichever site is chosen must read `phase-NN-exit` (verified path:
`[VERIFIED: crates/devflow-core/src/agent_result.rs:1897-1898]`,
`devflow_dir(project_root).join(format!("phase-{:02}-exit", phase))`) and refuse to let a
stream-derived `Success` stand if that file records a non-zero code.
**Warning signs:** a truncation-boundary test that only checks the parser's own return value
(`None`/`Failed`) rather than the end-to-end `evaluate_agent_result` result against a non-zero
exit file is not exercising this residual at all — it is exercising constraint 9 items 1/2, which
are already closed.

### Pitfall 5: The D-13 canary doesn't fit the existing `preflight` hook
**What goes wrong:** Wiring the startup canary into `AgentAdapter::preflight`
(`[VERIFIED: crates/devflow-core/src/agents/mod.rs:46-58]`) because it looks like the obvious
existing extension point.
**Why it happens:** `preflight` is the only "check something before launching" hook the trait
already has.
**How to avoid:** `run_preflight` (`[VERIFIED: crates/devflow-cli/src/preflight.rs:920-927]`) is
invoked from `launch_stage` on **every stage launch**, not once per run — confirmed by its own
retry-ceiling/gate logic (`preflight.rs:895-919`), which exists precisely because it can run
repeatedly across a multi-stage pipeline. D-15 requires the canary to run **once per run**, with
its outcome recorded in the run's provenance (`events::emit`,
`[VERIFIED: crates/devflow-core/src/events.rs:35-50]`, which writes to `.devflow/events.jsonl`).
The canary needs its own gate — e.g. a check keyed on whether this run has already recorded a
`claude_delivery_canary_confirmed` event — not a reuse of the per-stage `preflight` hook.
**Warning signs:** a canary that silently re-runs (and re-spends a real throwaway agent
invocation) at every stage transition within one run is a sign it landed inside the wrong hook.

### Pitfall 6: The current `spawn_monitor` may not actually be "detached" the way 30c's harness assumed
**What goes wrong:** Assuming the current shipped monitor's process-group/session shape already
matches what 30c/30d's harnesses measured, and therefore that the new pipe-owning monitor merely
needs to preserve today's behavior.
**Why it happens:** `30c-VERDICT.md`'s own comparison table describes production as "detached, own
session leader (verified: harness sid ≠ child sid)... start_new_session equivalent of a detached
monitor" `[CITED: 30c-VERDICT.md — "Side-by-side against the 30a interactive baseline" table]`.
**What's actually verified:** reading `monitor.rs` end to end this session
(`[VERIFIED: crates/devflow-core/src/monitor.rs:1-179]`) shows **no** call to `setsid`,
`process_group`, or a `pre_exec` closure anywhere — `spawn_monitor_inner` builds a plain
`hermetic_command("sh", workdir_path)...spawn()` with no session/group configuration at all. The
python harnesses' `start_new_session=True` is the *harness's own choice* to model an assumed
detachment property; it is not a citation of anything present in `spawn_monitor`'s actual source.
Today's "detachment" comes entirely from the CLI process not `.wait()`-ing on the spawned child,
not from any session/group API call.
**How to avoid:** don't assume this gap is harmless. If session-level detachment (surviving a
`SIGHUP` from the controlling terminal) matters for reliability, the pipe-owning rewrite should
add it explicitly (`.process_group(0)` at minimum for group isolation, or a `pre_exec` calling
`libc::setsid()` for full session detachment) rather than assuming it is inherited from the
current implementation — because verified source shows it currently is not.
**Warning signs:** a reliability report that cites 30c/30d's "detached, own session" framing as
evidence about *production's current* behavior, when it is evidence about the *harness's*
behavior only.

## Code Examples

### The verified `background_tasks_changed` / `task_notification` / `result` event shapes

Read directly from the real archived capture this session (paths/session-ids below are
paraphrased/generalized rather than reproduced verbatim, since the committed evidence files are a
known, tracked leak — ROADMAP §999.69 — and this document should not add another copy of the same
un-redacted values):

```jsonc
// system/init — gates is_claude_event_stream (agent_result.rs:823-828)
{"type":"system","subtype":"init","cwd":"...","session_id":"<uuid>",
 "claude_code_version":"2.1.220", ...}

// background_tasks_changed — the drain signal (constraint 4's second arm)
{"type":"system","subtype":"background_tasks_changed",
 "tasks":[{"task_id":"<id>","task_type":"local_agent","description":"..."}], ...}
// drains to: {"type":"system","subtype":"background_tasks_changed","tasks":[], ...}

// task_notification — informational per-child completion signal
{"type":"system","subtype":"task_notification","task_id":"<id>",
 "tool_use_id":"<toolu_id>","status":"completed","output_file":"...",
 "summary":"...", "usage":{...}, ...}

// top-level result, first turn (no origin key — orchestrator's own turn)
{"is_error":false, "num_turns":3, "stop_reason":"end_turn",
 "session_id":"<uuid>", "type":"result", "result":"...", ...}

// top-level result, task-notification-driven turn (constraint 7's coalescing signature)
{"is_error":false, ..., "origin":{"kind":"task-notification"}, "subtype":"success",
 "type":"result", "result":"Both subagents completed...", ...}
```
`[VERIFIED: .planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl
lines 5,8,19,26,29,37,54 — read this session]`

### Reusable termination primitive

```rust
// Source: crates/devflow-core/src/agent.rs:118-159 (verified this session, quoted for reference)
pub fn terminate_and_verify(
    pid: u32,
    wait: std::time::Duration,
    poll: std::time::Duration,
) -> bool {
    // sends SIGTERM, polls agent_running(), escalates to SIGKILL on expiry,
    // polls again, returns a VERIFIED liveness fact — never an assumption.
}
```
The pipe-owning monitor's D-05 "terminate the child" step should extend this pattern to
process-group scope (negative pid to `libc::kill`) rather than reimplementing escalation logic.

## State of the Art

| Old approach | Current/new approach | When changed | Impact |
|--------------|------------------------|---------------|--------|
| `spawn_monitor`: detached `sh` script, `.stdin(Stdio::null())`, prompt in argv, `--output-format json` | Pipe-owning Rust monitor, prompt via stdin JSON, `--input-format/--output-format stream-json` always-on | This phase (31) | Makes 30b's stream parser reachable in production for the first time; closes 999.64 |
| `claude -p "<prompt>" --output-format json` | `claude -p --input-format stream-json --output-format stream-json --verbose --dangerously-skip-permissions` + stdin-written first turn | Phase 30's experiments (2026-08-01/02), wired into production by Phase 31 | Prompt delivery channel changes, not just the output shape |
| Verdict cascade fed only single-document envelopes in production | Same cascade, now fed real multi-turn JSONL captures | Phase 31 | `parse_claude_event_result`'s "last-result-wins, multiple results per process is the new normal" behavior (already coded, per Phase 30) becomes load-bearing rather than dormant |
| ROADMAP-cited "0.38s" post-close exit latency | ~0.24s median (5-trial archived distribution, 169.5–279.7ms) | Corrected by 30-04 (30d-MEASUREMENTS.md), 2026-08-02 | Informs how conservative any post-close wait needs to be — the CLI exits fast on a drained close |
| ROADMAP-cited "~12s" idle-timeout floor | ≥30s | Corrected by 30-04, same day, from a second independent 7-trial set | This is D-02's locked number; do not re-derive a smaller one from the older 30c-only data |

**Deprecated/outdated:**
- `claude_stream_gate_shape` as a function name: 31-CONTEXT.md and 30-VERIFICATION.md's W-02 cite
  it, but source shows the gate predicate for checkpoint scanning is now `classify(&capture) ==
  CaptureKind::ClaudeStream` (`[VERIFIED: crates/devflow-core/src/agent_result.rs:626-630,
  959-1046]`) — `claude_stream_gate_shape` appears only in comments/doc-history now, not as a live
  function. This is a *further* staleness beyond what 30-VERIFICATION.md's W-02 already flagged
  (W-02 corrected an even older name, `is_claude_event_stream`, to `claude_stream_gate_shape`; the
  `a557805` refactor superseded both with `classify()`/`CaptureKind`). Cite `classify()` /
  `CaptureKind::ClaudeStream`, not `claude_stream_gate_shape`, in the plan.

## 999.67 is already fixed — do not re-plan it

ROADMAP.md's Phase 31 entry (constraint 6, "Fold in 999.67 (XS)") and 31-CONTEXT.md both describe
this as outstanding work: "a one-line overwrite plus a mirror test." Reading `parse_devflow_result`
this session (`[VERIFIED: crates/devflow-core/src/agent_result.rs:147-162]`) shows the fix is
**already applied**:
```rust
pub fn parse_devflow_result(stdout: &str) -> Option<AgentResult> {
    if let Some(inner) = extract_json_result_text(stdout)
        && let Some(result) = parse_marker_lines(&inner)
    {
        return Some(normalise_stream_marker_provenance(result));  // <-- applied
    }
    parse_marker_lines(stdout).map(normalise_stream_marker_provenance)  // <-- applied
}
```
`git log` traces this to commit `a557805` ("refactor(30): make lossiness and capture kind
first-class — closes 3 root causes", 2026-08-02 22:04, **before** the Phase 31 ROADMAP entry was
authored 2026-08-03), whose commit message explicitly states: *"pass-4 M1 + a 5th instance found
by reading: decided_by_layer is now overwritten on the generic AND codex marker paths (999.67's
class)."* Regression tests `generic_marker_cannot_forge_layer0_provenance` and
`codex_marker_cannot_forge_layer0_provenance` both exist and both pass:
```
$ cargo test -p devflow-core --lib agent_result::tests::generic_marker_cannot_forge_layer0_provenance -- --exact
test agent_result::tests::generic_marker_cannot_forge_layer0_provenance ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 501 filtered out
$ cargo test -p devflow-core --lib agent_result::tests::codex_marker_cannot_forge_layer0_provenance -- --exact
test agent_result::tests::codex_marker_cannot_forge_layer0_provenance ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 501 filtered out
```
`[VERIFIED: live `cargo test` run this session, real `1 passed` counts confirmed per this
project's own "assert on a real `1 passed`" rule]`.

**Recommendation:** the planner should drop constraint 6 as a work item and instead spend the XS
budget it freed on either (a) a brief note verifying test coverage is adequate, or (b) folding the
saved capacity into the genuinely new work above (the idle-timeout side-channel design, Pitfall
3).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The std-only writer-thread / reader-thread / `mpsc` idle-timer idiom (Pattern 2) is the right shape for this monitor. | Architecture Patterns, Pattern 2 | If wrong, the planner picks a different (but still std-only, per the no-new-runtime constraint) concurrency shape — low risk, since alternatives (e.g. non-blocking reads via `set_nonblocking` + a poll loop) are also std-only and address the same deadlock hazard |
| A2 | `--verbose` is required (or at least was present in every tested trial) for `--output-format stream-json` to behave as measured; its strict necessity is not confirmed by official docs alone. | Standard Stack / Pattern 1 | Low — every archived trial included it; dropping it is untested territory the planner should not do without re-verifying |
| A3 | Whether a positional prompt argument alongside `--input-format stream-json` errors, is silently ignored, or is treated as an additional message is unconfirmed — no Phase 30 trial tested this combination (all trials omitted the positional prompt entirely). | Pitfall 1, Open Questions | If the planner's plan accidentally keeps the positional prompt "for safety," the actual runtime behavior is unknown and should be smoke-tested before relying on it |
| A4 | Re-exec-self-as-hidden-monitor-subcommand vs an in-process detach is presented as the two leading options for the monitor's outer process shape, but no third option was exhaustively ruled out. | Alternatives Considered, Open Questions | Low — this is explicitly Claude's discretion per CONTEXT.md; either leading option is workable |
| A5 | Session-level detachment (`setsid`) is *not* currently present in `spawn_monitor`, based on a full read of `monitor.rs`; whether this omission has ever caused a real reliability problem is not established either way. | Pitfall 6 | If the omission has been silently fine for years, adding `setsid` is a no-op improvement; if it has been silently causing occasional SIGHUP losses, this research surfaces a latent bug Phase 31 should consider fixing as a side effect, not the reverse |

**If this table is empty:** N/A — populated above.

## Open Questions

1. **Does a positional prompt argument alongside `--input-format stream-json` do anything
   observable?**
   - What we know: no archived Phase 30 trial tested this combination; all omitted the positional
     prompt entirely and used stdin exclusively.
   - What's unclear: whether keeping it (out of caution, or by oversight) is inert, an error, or
     silently creates two turns.
   - Recommendation: don't keep it. Match the tested shape exactly (no positional prompt argument
     for the Claude adapter once stream-json is always-on); if the planner wants a belt-and-braces
     smoke test, assert the argv contains no prompt text.

2. **Where does the D-05 idle-timeout write physically live, given Pitfall 3?**
   - What we know: appending to the existing stdout capture risks being shadowed by an earlier
     genuine `result` event.
   - What's unclear: whether the cleanest fix is a new side-channel file consulted before Layer 1,
     a change inside `parse_claude_event_result` itself, or something else — explicitly left to
     Claude's discretion by CONTEXT.md.
   - Recommendation: a dedicated file read before `evaluate_layer1` (mirroring Layer 0's existing
     "authoritative external check runs first" shape) is the option with the smallest blast radius
     on the existing, well-tested cascade.

3. **What is the monitor's own outer-process shape — re-exec `devflow` as a hidden subcommand, or
   something else?**
   - What we know: the CLI invocation (`devflow start`/`advance`) must return quickly while the
     monitor outlives it; `sequentagent`, a prior candidate for "a devflow-driven background
     supervisor," was removed in Phase 23 (23d) and offers no working precedent to inspect.
   - What's unclear: whether re-exec-with-a-hidden-argument is preferred over some other detach
     shape, and whether the final `devflow advance --phase N` step should be a fresh child process
     (as today) or an in-process call once the monitor is already inside the `devflow` binary.
   - Recommendation: re-exec is the lowest-risk option since it needs no new daemonization
     primitives beyond what `Command::spawn()`-without-`.wait()` already gives, and it preserves
     the existing "advance runs as a distinct step" boundary this codebase already reasons about
     (lock acquisition, `events::emit`, etc. all currently assume `advance` is invoked as its own
     process). Left to Claude's discretion per CONTEXT.md.

4. **Does `spawn_monitor`'s current lack of `setsid`/`process_group` matter in practice?**
   - What we know: verified absent from source; 30c's harness modeled a detached-session shape
     that production does not actually have today.
   - What's unclear: whether this has ever caused a real incident (no forensics record cites a
     SIGHUP-related monitor loss).
   - Recommendation: add `.process_group(0)` (minimum, cheap, closes the group-signal ambiguity)
     to the new monitor regardless; treat full `setsid()` session detachment as a nice-to-have the
     planner can decide is in-scope or out-of-scope for the M cap.

## Environment Availability

| Dependency | Required by | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `claude` CLI | The entire phase — the adapter this phase rewires | ✓ | `2.1.220` (`[VERIFIED: claude --version, run this session]`) — **exactly matches** the version every Phase 30 experiment and this arc's premise was witnessed on | none needed; if a future CLI update changes this, D-13's canary is the designed detection mechanism |
| `rustc`/`cargo` | Build | ✓ | `1.97.1`, edition 2024 (`[VERIFIED: rustc --version, Cargo.toml:10]`) | none needed — `process_group` (stable since 1.64) and every other API used here is long-stable |
| `libc` crate | Process-group signalling | ✓ (already a pinned dependency) | `0.2` | none needed |
| async runtime (`tokio` or similar) | N/A — explicitly not used | n/a (absent, by design) | — | std-only concurrency (Pattern 2) is the fallback-as-primary-plan here, not a degraded fallback |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none — everything needed is already present.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (no separate test framework/config file in this workspace) |
| Config file | none — `[Cargo.toml]` workspace convention, `[VERIFIED: Cargo.toml, scripts/check.sh]` |
| Quick run command | `cargo test -p devflow-core --lib agent_result::` (parser/cascade changes), `cargo test -p devflow-core --lib monitor::` (monitor changes), `cargo test -p devflow-core --lib agents::claude::` (adapter argv changes), `cargo test -p devflow-core --lib outcome_policy::` (new `AgentStatus` variant's action mapping) |
| Full suite command | `cargo test --workspace` (unit + integration), or the project's own gate `scripts/check.sh all` / `scripts/check-in-container.sh all` (fmt + clippy `-D warnings` + test) `[VERIFIED: scripts/check.sh:1-40]` |

**Reminder from `CLAUDE.md`:** `cargo test --exact <name>` exits 0 when the name matches
nothing — assert on a real `N passed` count, not merely exit code 0 (demonstrated live in this
session's 999.67 re-verification above). The CLI package name is `devflow`, not `devflow-cli`
(`Cargo.toml` `[package] name = "devflow"` under the `devflow-cli` directory).

### Phase Requirements → Test Map
| Constraint | Behavior | Test type | Automated command | File exists? |
|--------|----------|-----------|-------------------|-------------|
| Constraint 1 | Claude adapter always emits `--input-format stream-json --output-format stream-json`, prompt delivered via stdin not argv | unit | `cargo test -p devflow-core --lib agents::claude::tests -- --exact` (new tests replacing `claude_wraps_prompt_in_noninteractive_flags`'s `--output-format json` assertion) | ❌ Wave 0 — needs new/updated tests in `agents/claude.rs` and `agents/mod.rs` |
| Constraint 4 | Close rule fires only on marker-in-top-level-result AND drained `background_tasks_changed` | unit (fixture-driven, reusing 30a/30c archived captures) | `cargo test -p devflow-core --lib monitor::tests -- --exact` (new) | ❌ Wave 0 — no existing monitor test exercises the close rule; `monitor.rs`'s current tests only cover the `sh`-script shape |
| Constraint 5/8 | Idle timer resets on every line; fires at ≥30s (clamped-configurable); writes authoritative result before terminating; the Pitfall-3 shadowing case is covered | unit + a torn/coalesced-stream regression fixture | `cargo test -p devflow-core --lib monitor::tests -- --exact`, `cargo test -p devflow-core --lib agent_result::tests -- --exact` | ❌ Wave 0 — new |
| Constraint 7 | Monitor does not count `result` events as children-returned; near-simultaneous/coalesced completions handled | unit, using the real coalesced-shape fixture already archived (30c trial 2, 1 result event for 2 children) | `cargo test -p devflow-core --lib monitor::tests -- --exact` | ❌ Wave 0 — but the **fixture data already exists** (`30c-evidence/`, `30c-evidence-reliability/`) and should be reused, not re-synthesized |
| Constraint 9 (residual) | A stream-derived `Success` must not override a contradicting non-zero exit code | unit (fixture: torn/truncated capture whose exit file is non-zero) | `cargo test -p devflow-core --lib agent_result::tests -- --exact` | ❌ Wave 0 — this is new coverage beyond the already-closed constraint 9 items 1/2 |
| ROADMAP §999.67 | Agent cannot forge `decided_by_layer` | unit | `cargo test -p devflow-core --lib agent_result::tests::generic_marker_cannot_forge_layer0_provenance agent_result::tests::codex_marker_cannot_forge_layer0_provenance` | ✅ **already exists and passes** — no Wave 0 gap |
| Phase acceptance criterion | The live Phase 29 wave-2 re-run (D-16/D-17/D-18) | **manual-only, by design** | N/A — per review constraint H4, "not substitutable by integration tests"; this is `devflow start` driving a real two-plan wave on the main checkout | N/A |

### Sampling Rate
- **Per task commit:** the relevant narrow `cargo test -p devflow-core --lib <module>::` command above.
- **Per wave merge:** `cargo test --workspace`.
- **Phase gate:** `scripts/check.sh all` (or `scripts/check-in-container.sh all` for CI parity)
  green, **then** the live D-16 acceptance run — the acceptance run is not replaced by any
  automated command.

### Wave 0 Gaps
- [ ] `crates/devflow-core/src/monitor.rs` — no test module currently exercises pipe-ownership,
  idle-timeout, or the close-rule `AND`; every existing test targets the `sh`-script shape and
  will need rewriting alongside the implementation, not merely extending.
- [ ] `crates/devflow-core/src/agents/claude.rs` — `claude_wraps_prompt_in_noninteractive_flags`
  (in `agents/mod.rs`) currently asserts `--output-format json` and a positional prompt in argv;
  this assertion must be replaced, not merely extended, since the new contract removes the
  positional prompt.
- [ ] `crates/devflow-core/src/agent_result.rs` — `AgentStatus::IdleTimeout` has no representation
  anywhere yet; every exhaustive match (`as_wire_str`, `outcome_policy::decide_action`) needs a new
  arm, and each needs its own test.
- [ ] A fixture reusing the real coalesced-completion capture (`30c-evidence-reliability/`) for the
  monitor's close-rule/count-does-not-equal-children test — the data exists, the test harness
  around it in `monitor.rs` does not.
- Framework install: none — `cargo test` is already fully configured.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard control |
|---------------|---------|-----------------|
| V2 Authentication | no | No auth boundary changes; Claude's own `session_id` concept is orthogonal to DevFlow's auth model |
| V3 Session Management | no (in the ASVS web-session sense) | The Claude CLI's `session_id`/`--resume` mechanics are already handled per D-04/T-28-04's top-level-only-read discipline, unchanged by this phase |
| V4 Access Control | no | No new privilege boundary |
| V5 Input Validation | **yes** | Parsing untrusted, adversarial-capable JSONL from an external subprocess's stdout — already handled by `ParsedCapture`'s torn/noise-line classification and `is_top_level`'s provenance check; the monitor's real-time line inspection (for the close rule and D-13's canary match) must reuse these same predicates rather than introduce a second, divergent notion of "trustworthy line" |
| V6 Cryptography | no | No crypto introduced; D-13's canary token needs only to be unguessable enough that an agent cannot produce it by chance inside its own generated text — a random UUID/nonce, not a secret in the cryptographic sense |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard mitigation |
|---------|--------|---------------------|
| Agent echoes the D-13 canary token in a non-authoritative context (prompt echo, subagent narration) and DevFlow mistakes it for confirmed delivery | Spoofing | Match the token **only** inside a top-level `result` event, reusing `is_top_level` (`[VERIFIED: agent_result.rs:1063-1068]`) — already the locked design per D-13 |
| Shell-injection via agent argv (present today, mitigated by the literal-argv trick) | Tampering | **Removed by construction**, not merely mitigated, once the monitor spawns `claude` directly via `std::process::Command` with no intervening `sh -c` — see Anti-Patterns |
| Killing the wrong process due to pid reuse when a stray-reaper acts on a stale on-disk pid | Tampering / Elevation of Privilege (acting on an unrelated process) | `agent::process_start_time`/`is_same_process` already exist for this; the monitor's own in-memory `Child` handle is not exposed to this hazard while it holds the handle, only the on-disk `phase-NN-agent-pid` file consumed by *other* tools is |
| Agent forges `decided_by_layer` inside its own self-reported marker JSON to claim Layer-0 (external-verification) provenance it did not earn | Spoofing / Elevation of Privilege | `normalise_stream_marker_provenance` — **already applied on all three marker-parsing paths** (stream, generic, Codex) per the 999.67 finding above |
| A torn/boundary-truncated capture's real (failing) tail never arrives, and an earlier turn's success is mistaken for the final verdict | Repudiation (the real outcome is unrecorded) | Constraint 9 items 1/2 closed (`ParsedCapture`, `is_top_level`); the residual (stream-Success vs. exit-code) is this phase's remaining obligation |

## Sources

### Primary (HIGH confidence)
- `crates/devflow-core/src/monitor.rs`, `crates/devflow-core/src/agents/claude.rs`,
  `crates/devflow-core/src/agents/mod.rs`, `crates/devflow-core/src/agent_result.rs`,
  `crates/devflow-core/src/agent.rs`, `crates/devflow-core/src/outcome_policy.rs`,
  `crates/devflow-cli/src/pipeline_launch.rs`, `crates/devflow-cli/src/preflight.rs`,
  `crates/devflow-cli/src/config_parse.rs`, `crates/devflow-core/src/events.rs`,
  `crates/devflow-core/src/git.rs`, `Cargo.toml`, `crates/devflow-core/Cargo.toml`,
  `crates/devflow-cli/Cargo.toml` — all read directly this session.
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30c-monitor-env-harness.py`,
  `30a-evidence/run_experiment_v3.py`, `30a-evidence/raw_output_v3.jsonl`, `30c-VERDICT.md`,
  `30c-VERDICT-reliability.md`, `30d-MEASUREMENTS.md`, `30-H1-CONTEXT-FOR-31.md`,
  `30-VERIFICATION.md` — all read directly this session.
- `claude --version` (live check, this session: `2.1.220`) and `claude --help` (live check, this
  session — `--input-format`, `--output-format`, `--replay-user-messages` flag text).
- Live `cargo test` runs this session (999.67 regression tests, `agent_result::` module 136
  passed).

### Secondary (MEDIUM confidence)
- Context7 `/websites/code_claude` — `SDKUserMessage` type shape, `--input-format`/`--output-format`
  CLI reference entries, `--replay-user-messages` requiring both stream-json flags. Official docs
  site, but the SDK-level `SDKUserMessage` type is a TypeScript/Python SDK abstraction layered over
  the raw CLI wire format — the raw-CLI shape is corroborated directly by the archived harness
  code (Primary), which is the stronger source for this specific question.

### Tertiary (LOW confidence)
- None retained — every claim in this document either traces to a file read this session or a
  live command run this session.

## Metadata

**Confidence breakdown:**
- Standard stack / no-new-dependency claim: HIGH — verified by reading both crates' `Cargo.toml`
  files directly.
- Wire protocol (prompt-via-stdin): HIGH — corroborated by three independent archived harnesses
  plus official SDK-type docs.
- Monitor internal architecture (threads/mpsc): MEDIUM — the APIs are stable stdlib and the
  deadlock hazard is well-established systems knowledge, but no implementation of this shape
  exists anywhere in this codebase to verify against directly.
- Idle-timeout shadowing pitfall (Pitfall 3): HIGH — traced directly through the live cascade
  source, not inferred.
- 999.67 already-fixed finding: HIGH — verified via `git log`, source read, and live passing tests.

**Research date:** 2026-08-03
**Valid until:** ~7 days for the CLI-behavior-dependent claims (undocumented behavior on
`claude_code_version 2.1.220`, per D-13's own premise that a CLI update can invalidate it
silently); ~30 days for the source-code-structural claims (cascade ordering, existing helper
functions), which only change if this codebase's own files change.

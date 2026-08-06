# Phase 34: Stream-JSON Coverage, the Validate Trust Boundary, and Layer 0 in Worktree Mode - Research

**Researched:** 2026-08-05
**Domain:** DevFlow's own pipeline internals (Rust) — stage-launch transport selection, the
Validate outcome classifier, and the Layer 0 external-verification cascade.
**Confidence:** HIGH on every source-code claim below (all carry a `file:line` read this session,
several with `cargo test` runs reproduced independently). MEDIUM/LOW only on things nobody can
verify without a live agent run (see Assumptions Log).

This phase has already been researched to death by two adversarial review passes (`34-REVIEW.md`).
This document does not re-litigate D-01…D-15 — it answers the five things CONTEXT.md explicitly
left open for planning, with every claim checked against HEAD `973a115` on `feature/phase-34` (the
main checkout, currently checked out directly on this branch — not a linked worktree; `git
worktree list` shows one entry).

## Summary

The phase has two genuinely open engineering questions and three where the research is really just
"confirm the exact call-site mechanics so the plan can cite line numbers instead of paraphrasing."

**999.73 (capture acquisition, the biggest open item):** getting a real per-stage capture requires
running `devflow` **on the main checkout with `--no-worktree`**, using a `devflow` binary that was
rebuilt after locally widening `STREAM_JSON_STAGES` — because `STREAM_JSON_STAGES` is a Rust
constant baked into the binary at compile time, not something read from the worktree's source tree
at runtime. Widening it in a worktree's checked-out files does nothing to the orchestrator process
already running; only rebuilding-and-reinstalling the binary changes which code path
`claude_stream_launch_enabled` takes. `devflow start`'s default `--worktree` path forks a **fresh**
worktree from `develop`'s tip via `worktree::add(project_root, &wt, &branch, DEVELOP, true)`
(`crates/devflow-cli/src/parallel.rs:30`) — confirming CONTEXT.md's practical conclusion even though
the constant itself is not "read from" anywhere; and this fork happens at
`crates/devflow-cli/src/commands.rs:238-244`, roughly 60 lines **before**
`enforce_build_staleness` at `commands.rs:305` — so the ordering claim in D-02 is also verified.
Phase 30's `NNx-evidence/` layout (four real directories inspected this session, not the two named
in CONTEXT.md) is the concrete template; `.devflow/.gitignore` is literally `*`
(`/var/home/denniyahh/Github/devflow/.devflow/.gitignore`, read this session), so a capture must be
**copied out of `.devflow/`** into the phase's own evidence directory to be committed at all — that
is what Phase 30 did, not `git add -f`.

**999.74/D-15 (the graft fix):** `reconcile_layer0_verdict`
(`crates/devflow-core/src/agent_result.rs:2143-2156`) already has, in scope at its call site, the
exact value the fix needs: `evaluate_layer1(project_root, state.phase)` returns `Option<AgentResult>`
whose `.status` field is discarded today (`.and_then(|layer1| layer1.verdict)` — verdict taken,
status dropped). The fix is to gate the graft on `layer1.status == AgentStatus::Success` before
transplanting `layer1.verdict`. An existing test,
`layer0_affirmative_success_consults_layer1_verdict_at_validate`
(`agent_result.rs:5488`, read and re-run this session — `1 passed`), already exercises this exact
scenario for THREE verdict states with a **passing** Layer-1 status; extending it with a fourth case
where Layer 1's status is `Failed` is the smallest in-repo harness for D-15's demonstration — no
out-of-repo temp project required, because `evaluate_agent_result_inner` and `reconcile_layer0_verdict`
both live in `devflow-core` and are reachable from `#[cfg(test)]` in the same file.

**999.76 (Layer 0 worktree discovery):** verified live and current — right now, on this exact
checkout, `git ls-tree -r develop --name-only -- .planning/phases | grep -c '/34-'` returns **0**
while the same command against `HEAD` returns **3**. This is a fresher, more relevant reproduction
of the negative control than CONTEXT.md's Phase-33 example (`git ls-tree -r develop ... '/33-'`),
which now reads **21 on both refs** because Phase 33 shipped and merged to `develop` on 2026-08-05
(`STATE.md`: "Phase 33 shipped — PR #90") — that example is now stale and the plan should not reuse
it verbatim.

None of the five open questions surfaced a reason to distrust D-01…D-15 or the review's findings.
Every mechanism CONTEXT.md described was reproducible at HEAD.

**Primary recommendation:** sequence 999.74 (cheap, self-contained, one crate) and 999.76 (cheap,
one root-cause, two call sites) first — both are pure-function/no-agent-run changes, verifiable
entirely by `cargo test`. Do 999.73's capture campaign last, since it is the only part of the phase
that needs a real `--no-worktree` agent run and therefore the only part that trips the CLAUDE.md
git-ops-during-executor constraint.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Stage transport selection (`STREAM_JSON_STAGES`) | Backend / Orchestrator (devflow-cli) | — | Compile-time constant consulted once per launch inside the CLI process; no client or storage tier involved. |
| Validate outcome classification | Backend / Orchestrator (devflow-cli) | — | `classify_validate_outcome` is a pure function inside the CLI crate, called from `advance()`'s dispatch. |
| Layer 0 external verification | Backend / Orchestrator (devflow-core) | Filesystem (`.planning/`, worktree) | Reads PLAN declarations from the main checkout, executes probes against the worktree — a cross-root read that is exactly what 999.76 fixes. |
| Capture storage / retention | Filesystem (`.devflow/`) | — | `.devflow/history/phase-{NN}/` is DevFlow's own local, gitignored storage; not a database, not a service. |
| Evidence commit (per-stage captures) | Git / VCS | Filesystem | Captures must be relocated out of the gitignored `.devflow/` tree into `.planning/phases/{N}/` before `git add` can see them at all. |

This phase touches no browser, no CDN, no external service — it is entirely inside DevFlow's own
Rust orchestrator process and its local `.devflow`/`.planning` filesystem state. There is no
tier-misassignment risk of the kind this map usually screens for; it is included per protocol.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DOGFOOD-03 | Operator can trust that every stage DevFlow launches through the stream-json path was put there on real per-stage behavioural evidence; any un-evidenced stage is visibly and deliberately narrow; the phase moves the rollout forward or explicitly escalates why it could not (999.73) | § "Capture Acquisition Mechanics" below gives the exact command sequence, evidence-directory layout, and retention-eviction fix the delivery-floor language requires. |
| DOGFOOD-04 | Operator can trust that a Validate stage's reported outcome reflects its actually-derived status, not just the agent's self-reported verdict field (999.74) | § "The Exhaustive-Match Rewrite" and § "The `reconcile_layer0_verdict` Fix" below give the verified match shape and the graft fix's exact mechanics, including the in-repo test harness. |
| (999.76, no v1 ID — folded in on scope freed by criteria 1 and 4) | Layer 0 external verification must discover its declaration from the execution root so it actually runs in worktree mode | § "999.76 — Layer 0 Discovery from the Execution Root" below. |
</phase_requirements>

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

D-01…D-15 are locked and are NOT re-opened by this research. Full text lives in
`34-CONTEXT.md` §"Implementation Decisions"; the load-bearing summary, for the planner's
convenience:

- **D-01:** `STREAM_JSON_STAGES` stays an explicit named const, widened to name all five stages
  (not inverted to a deny-list, not deleted).
- **D-02 [AMENDED, three times]:** the evidence gate is COMMIT-TIME, not build-time. A stage whose
  real capture cannot be obtained stays off the list with a recorded reason. `devflow __monitor` is
  explicitly NOT an equivalent evidence route (it skips `resolve_launch_shape`). The working-tree
  route needs `--no-worktree` plus a rebuild, stated explicitly in the plan.
- **D-03:** no new runtime per-stage dial — `legacy_opt_out` (`DEVFLOW_CLAUDE_LEGACY_LAUNCH`) stays
  the single predicate governing launch shape, the canary gate, and the loud notice together.
- **D-04:** a parser/monitor defect a capture reveals is filed as a numbered `999.x` entry, not
  fixed in-phase; the stage stays narrow.
- **D-05 [AMENDED — cost was false]:** status/verdict disagreement routes to `Ambiguous`, retained as
  defence-in-depth even though its accepted-cost pairs are unreachable in production (R-01).
- **D-06 [AMENDED — tuple was wrong]:** the fix is an exhaustive match, no wildcard reaching `Passed`
  OR `Failed`, on the tuple `(layer0, status, verdict)` where `layer0` is normalised
  `decided_by_layer == Some(0)` — NOT the composite `external` predicate. 42 cells
  (2 × 7 × 3), not 21 or 10-arms-with-a-narrower-encoding — see § "The Exhaustive-Match Rewrite"
  below for the verified shape.
- **D-07 [AMENDED twice]:** the originally-specified pre-fix pin test is unwritable (would bypass
  `advance()`). The amended deliverable is (a) the corrected written finding and (b) an executable
  demonstration of the graft, pre-fix Ship / post-fix gate, with negative controls.
- **D-08 [AMENDED — under-dimensioned]:** criterion 3 is a full 42-cell sweep (not 21), with named
  controls: the positive control `(_, Success, Some(Pass)) → Passed`, and the two `Ambiguous` cells
  `(true, Success, Some(Gaps))` / `(true, Success, None)` each paired with its `layer0 = false`
  mirror asserting `Failed`.
- **D-09:** criterion 2 is satisfied empirically per stage — does `background_tasks_changed` appear,
  what arity, did it drain — functioning as a negative control against the vacuous-drain assumption.
- **D-10:** n=1 production capture per stage. The summary must state what n=1 does NOT establish.
- **D-11 [AMENDED — cost was understated]:** a capture showing a non-draining list still widens the
  stage, narrowed to require naming *why* the shape was pathological rather than routine — because a
  non-draining list forces the idle timeout, terminates the child, and gates every subsequent run
  where the shape recurs. `Unreadable` is explicitly excluded (see D-14).
- **D-12:** `31/D-14` (per-child declared tokens) stays deferred, re-filed as its own `999.x` entry.
- **D-13:** 999.76 is folded in; it does NOT need to land before D-06's match rewrite (that
  dependency runs the other way — see D-15).
- **D-14:** `BackgroundTaskState::Unreadable` is governed by D-04 (file it, stage stays narrow), not
  D-11 — the governing reason is the parser gap, not the timeout. `Unreadable` does NOT block
  `should_close()` permanently; it clears to `Pending(n)` on a later readable announcement.
- **D-15:** `reconcile_layer0_verdict` is in scope and is 999.74's real defect — see § "The
  `reconcile_layer0_verdict` Fix" below. `idle_timeout_result`'s `verdict: None` comment documents a
  live guard and must NOT be "corrected."

### Claude's Discretion

The operator explicitly declined to discuss these — resolved by this research, not re-opened as
questions:

- **Capture acquisition** — where the four real per-stage captures come from, and what the per-stage
  pass bar is. Resolved below in § "Capture Acquisition Mechanics."
- **Plan sequencing within the phase** — 999.73 and 999.74/999.76 have no structural dependency on
  each other. This research recommends 999.74+999.76 first (cheap, no-agent-run, all `cargo test`),
  999.73 last (the only part needing a live `--no-worktree` run).
- **Where the exhaustive-match rewrite physically lands**, and how the D-07 demonstration is
  scaffolded. Resolved below.

### Deferred Ideas (OUT OF SCOPE)

- `31/D-14` per-child declared tokens (per D-12) — re-file as its own `999.x` entry + Linear issue.
- Any parser/monitor defect a per-stage capture reveals (per D-04) — file as a numbered `999.x`
  entry with the capture as evidence, not fixed here.
- The PARTIAL-close rule for an unobtainable stage was RETIRED 2026-08-05 (D-02 Amendment 2) — do
  not resurrect it. An un-widened stage with a recorded reason satisfies DOGFOOD-03 as reworded.
</user_constraints>

## Standard Stack

Not applicable in the conventional sense — this phase adds no new dependency. It is entirely
internal Rust changes to `crates/devflow-core` and `crates/devflow-cli`, both already in the
workspace. No new crate, no new external service, no new CLI tool.

**Toolchain already in place, verified this session:**

| Tool | Verified version | Purpose |
|------|------------------|---------|
| `cargo` | 1.97.1 (`cargo --version`, run this session) | build/test |
| `rustc` | 1.97.1 | compiler |
| `git` | 2.55.0 | worktree / branch operations |
| `claude` CLI | 2.1.222 (Claude Code) | the agent this phase's captures are taken from |

**Installation:** none required.

## Package Legitimacy Audit

**Not applicable — this phase installs no external packages.** All work is internal Rust source
changes to existing workspace crates (`devflow-core`, `devflow-cli`). No `Cargo.toml` dependency
additions are implied by any of the five open questions researched below. If a planner introduces
one anyway (e.g., a JSON-diff helper for capture comparison), the Package Legitimacy Gate protocol
must be re-run at that time — it was not run here because there is nothing to check.

## Capture Acquisition Mechanics (999.73's open question)

### Why widening the constant alone does nothing without a rebuild

`STREAM_JSON_STAGES` is a `const` (`crates/devflow-cli/src/pipeline_launch.rs:446`), consulted by
`claude_stream_launch_enabled` (`pipeline_launch.rs:478-480`, verified verbatim):

```rust
fn claude_stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
    !legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)
}
```

This runs **inside the `devflow` process** — it is not a config file read, not a per-project value,
not something sourced from the phase's own worktree. It is baked into whichever `devflow` binary
happens to be on `PATH` (or invoked directly) when `devflow start`/`devflow advance` runs. Editing
the constant's source and leaving it uncommitted, or committing it to a feature branch the
orchestrating binary was not built from, changes nothing about the running orchestrator's behaviour.
**The only way to make a real launch take the stream-json path for a newly-widened stage is to
rebuild the `devflow` binary from source that contains the widened constant, and then use that
rebuilt binary to drive the launch.**

### Why `devflow start`'s default `--worktree` mode defeats this

`commands::start` (`crates/devflow-cli/src/commands.rs:112`) does, **in this exact order**:

1. `if worktree { let wt = ensure_phase_worktree(project_root, phase, force)?; ... state.worktree_path = Some(wt); }` — `commands.rs:238-244`.
2. `enforce_build_staleness(project_root, &state, env!("DEVFLOW_BUILD_COMMIT"), ...)?` — `commands.rs:305`.

`ensure_phase_worktree` (`crates/devflow-cli/src/parallel.rs:15-40`) calls
`worktree::add(project_root, &wt, &branch, DEVELOP, true)` (`parallel.rs:30`) — a **fresh** git
worktree checked out from `develop`'s current tip, independent of anything uncommitted in the main
checkout. `[VERIFIED: crates/devflow-cli/src/parallel.rs:30]`.

The practical consequence CONTEXT.md draws — "a real capture needs `--no-worktree`" — holds, but the
underlying mechanism is subtly different from CONTEXT.md's framing ("an uncommitted widened constant
is absent from the worktree and the run captures the legacy path"). `STREAM_JSON_STAGES` is not
"absent from the worktree" in any sense that matters — the worktree's file contents are irrelevant
to which code path the already-running orchestrator binary takes. What actually matters is simpler
and stronger: **`--worktree` mode does not rebuild or reinstall `devflow`'s own binary at any point**
— the operator's already-running (or freshly re-invoked) `devflow` binary is whatever was last built,
and nothing in the `devflow start --worktree` code path changes that. `--no-worktree` does not fix
this by itself either — what fixes it is a manual rebuild step (`cargo build` from the tree
containing the widened constant, and reinstalling/re-pathing that binary) **before** invoking
`devflow start` or `devflow advance` for the capture run. `--no-worktree` matters for a different,
also-real reason: it keeps the phase driving directly in the main checkout, so there is exactly one
source tree in play (no worktree-vs-main-checkout confusion about which one the rebuilt binary
should track), and it is also the scenario CLAUDE.md's "never run git operations while an executor
holds the working tree" rule directly names.

`enforce_build_staleness` (`crates/devflow-core/src/staleness.rs:324`, called from `commands.rs:305`)
is the self-dogfood staleness gate, but it does **not** detect the "binary rebuilt from a locally
edited but not-yet-committed constant" scenario at all: `combined_staleness`
(`staleness.rs:209-223`) checks the EXECUTION root's (worktree's, or main checkout's) OWN git
dirtiness and ancestry against the binary's embedded build commit — not whether the running binary's
compiled-in constants match anything. A fresh worktree checked out cleanly from `develop` is never
"stale" by this check even when the running binary is years out of date, because staleness here means
"source changed since build," not "binary reflects the newest source." `[VERIFIED:
crates/devflow-core/src/staleness.rs:209-223, 324-331]`. This confirms the constant's presence is a
pure binary-build question, orthogonal to the staleness gate.

### The concrete command sequence for a real per-stage capture

Recommended minimal sequence, honoring the "cheapest workload that still crosses the seams under
test" preference and CLAUDE.md's git-ops prohibition:

1. **Edit** `crates/devflow-cli/src/pipeline_launch.rs:446` to add the target stage to
   `STREAM_JSON_STAGES` (this edit is the phase's own deliverable — it should be a normal, committed
   change on `feature/phase-34`, made through the ordinary Code stage of THIS phase, not a throwaway).
2. **Rebuild:** `cargo build --release -p devflow` (package name is `devflow`, verified —
   `crates/devflow-cli/Cargo.toml:2`) — or `cargo build -p devflow` for a debug binary; either way,
   this step must run **after** step 1's edit lands in the tree the build reads from.
3. **Reinstall/re-path** so subsequent `devflow` invocations use the freshly built binary (however
   this repo normally promotes a locally built binary — check for an install script or symlink
   convention already in use; not investigated further here, out of this research's scope).
4. **Run a minimal scratch phase** with `devflow start --phase <N> --no-worktree --agent claude
   --mode auto` targeting only the stage(s) under test. Given DOGFOOD-03's per-stage evidence need
   and D-10's n=1, the cheapest workload is a phase whose Define/Plan/Code/Validate/Ship stages each
   do close to nothing (e.g., a documentation-only or single-file change) — enough to reach and exit
   each targeted stage's real `claude -p --input-format stream-json ...` launch and produce a
   `background_tasks_changed`-bearing or vacuously-draining capture, without exercising anything else
   this phase does not need.
5. **CLAUDE.md governs step 4 absolutely:** while this `--no-worktree` run's executor holds the main
   checkout, the orchestrator (this research/planning session, or any other agent) must not run ANY
   git operation — no `add`, `commit`, `push`, branch, or tag — until the executor exits. This is not
   a suggestion; CLAUDE.md records two real failures from violating it on 2026-08-02.
6. **After the run exits**, copy the resulting capture(s) out of `.devflow/` (see next section for
   exact paths) into the phase's own evidence directory, and only then perform any git operation.

`devflow __monitor` (`crates/devflow-cli/src/main.rs:133`, hidden subcommand) is confirmed NOT
usable for this — `run_monitor` (`pipeline_launch.rs:493`) calls
`monitor::run_pipe_owning_monitor` directly (`pipeline_launch.rs:513`) and never consults
`claude_stream_launch_enabled` or `resolve_launch_shape` at all. `[VERIFIED:
crates/devflow-cli/src/pipeline_launch.rs:493,513]`. Usable only for smoke-testing the monitor
mechanism in isolation, never as criterion-1 evidence — matches D-02's correction exactly.

### Where captures physically land, and what committing one requires

The LIVE (in-progress) capture for a phase's current stage lives at:

```
{project_root}/.devflow/phase-{NN}-stdout       (stdout_path, agent_result.rs:2338-2340)
{project_root}/.devflow/phase-{NN}-exit         (exit_code_path, agent_result.rs:2348-2351)
{project_root}/.devflow/phase-{NN}-stderr.log   (stderr_path, agent_result.rs:2342-2346)
```

On every stage transition, `archive_phase_files` (`agent_result.rs:2435-2547`, called from
`pipeline_launch.rs:571-580` before every launch) renames the PREVIOUS stage's stdout/exit/REVIEW
files into:

```
{project_root}/.devflow/history/phase-{NN}/{stamp}-stdout   (history_dir, agent_result.rs:2402-2406)
{project_root}/.devflow/history/phase-{NN}/{stamp}-exit
{project_root}/.devflow/history/phase-{NN}/{stamp}-REVIEW.md
```

`{stamp}` is `{nanos}-{seq}` (`archive_stamp`, `agent_result.rs:2417-2424`).

`.devflow/.gitignore` is literally `*` — read directly this session:
`/var/home/denniyahh/Github/devflow/.devflow/.gitignore` contains exactly one line, `*`. **Both the
live and the archived capture paths are inside `.devflow/`, so both are gitignored.** Committing a
capture requires either `git add -f` on the exact path, or — the pattern Phase 30 actually used and
the one this research recommends — **copying** the file to a path outside `.devflow/`, e.g. into
`.planning/phases/34-.../34x-evidence/raw_output.jsonl`, which is never gitignored and needs no
force-add.

### `DEFAULT_CAPTURE_RETENTION = 5` — the eviction mechanism, verified concretely

`pub const DEFAULT_CAPTURE_RETENTION: usize = 5;` — `[VERIFIED:
crates/devflow-core/src/config.rs:12]`, quoted verbatim. `archive_phase_files_with_stamp`
(`agent_result.rs:2444-2547`) calls `prune_history(&history_dir, retain)` at its tail
(`agent_result.rs:2545`), which prunes the history directory down to the newest `retain` stamps —
**per phase**, not globally. Every stage transition within a single phase run counts as one archive
event. A phase that runs Define→Plan→Code→Validate→[loop back]→Code→Validate→Ship produces **six**
archive events (one per transition into a new stage), exceeding `retain=5` and evicting the OLDEST
archived stamp — which, for a phase with any Validate→Code loop-back, is Define's capture. This
confirms D-02/criterion-7's concern exactly: **do not assume all captured stages survive to the end
of a multi-wave run.**

Two concrete mitigations, matching criterion 7's "changing the constant or copying captures at
landing" language exactly:

- **Raise the constant** — change `DEFAULT_CAPTURE_RETENTION` (or set
  `DEVFLOW_CAPTURE_RETENTION` — confirmed env override exists, `config.rs:136-149`, `## Sources`)
  to a value exceeding the phase's expected transition count for the capture-taking run specifically.
- **Copy at landing** — after each targeted stage's capture is produced (before the NEXT stage's
  launch archives it), copy `.devflow/phase-{NN}-stdout` straight to the evidence directory,
  independent of `DEFAULT_CAPTURE_RETENTION`'s later eviction.

The second is cheaper for a scratch, single-purpose capture run (no config drift risk); the first is
appropriate if the capture-taking run is expected to genuinely need more than 5 stage transitions
(e.g., deliberately forcing a loop-back to observe Validate's second-pass drain behaviour).

### Phase 30's evidence-directory layout — concrete, all four directories inspected this session

`.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/` contains, at HEAD:

| Directory | Contents | Role |
|-----------|----------|------|
| `30a-evidence/` | `README.md`, `raw_output.jsonl`, `raw_output_v2.jsonl`, `raw_output_v3.jsonl`, `run_experiment*.py` | Early exploratory harness runs — script + output side by side. |
| `30c-evidence/` | `raw_output.jsonl` (55 lines), `run.log`, `stderr.log` | Trial 1: the agent-session-marker-laden baseline. |
| `30c-evidence-scrubbed/` | `raw_output.jsonl` (56 lines), `run.log`, `stderr.log` | Trial 2: a SEPARATE run with `CLAUDE_*`/`AI_AGENT*`/`ANTHROPIC*` env markers programmatically removed before launch (`30c-monitor-env-harness.py --scrub-agent-markers`) — NOT a redacted copy of trial 1's file (line counts and every UUID differ; confirmed by `diff` this session). Its own text describes a separate redaction pipeline for committed artifacts: "validate → structural redact → secret-scan → atomic replace" (`30c-VERDICT-scrubbed.md:216`), which is why its `cwd` fields read as the literal placeholder string `<cwd>/devflow` rather than a real path. |
| `30c-evidence-operator/` | `raw_output.jsonl` (53 lines), `run.log`, `stderr.log` | A THIRD variant: the operator's own genuinely plain-shell trial, run outside any agent session at all — distinct from the scrubbed-but-agent-launched trials 2–7 (`30c-VERDICT-scrubbed.md:160-176`). |
| `30d-evidence/` | Seven numbered subdirectories, each with `raw_output.jsonl`, `run.log`, `stderr.log`, `timings.json` | The exit-timing measurement trials, one directory per trial. |

**What "scrubbed" removes, concretely:** paths and identifiers that would leak the operator's
filesystem layout or account — `home_path`, `os_username`, `session_identifier` are the three fields
named in the redaction table (`30c-VERDICT-scrubbed.md:220`), replaced with placeholders like
`<cwd>` and `<session-01>`. It is NOT a de-agentified re-run in the sense of removing the agent's own
output — every committed evidence file already carries these placeholders in its `cwd` field,
confirmed by inspection this session.

**What this phase's capture directories should reproduce:** `raw_output.jsonl` (the capture itself,
copied from `.devflow/`), `run.log` (a short human-readable summary: command invoked, stage, agent
version, outcome), and PII-scrubbed paths before commit — following the same three-field redaction
list. A `README.md` per Phase 30a's example is optional but cheap and useful given four separate
stages will each need their own short provenance note (which command, which build, which git commit
of `STREAM_JSON_STAGES`).

## The Exhaustive-Match Rewrite (999.74 criterion 3)

### The verified discrete values the match must enumerate

`AgentStatus` has exactly **seven** variants, read verbatim this session
(`crates/devflow-core/src/agent_result.rs:47-81`):

```rust
pub enum AgentStatus {
    Success,
    Failed,
    RateLimited,
    Unknown,
    ResourceKilled,
    AgentUnavailable,
    IdleTimeout,
}
```

`Verdict` has exactly **two** variants (`agent_result.rs:107-113`), and the field it lives in is
`Option<Verdict>` — three observable states (`Some(Pass)`, `Some(Gaps)`, `None`):

```rust
pub enum Verdict {
    Pass,
    Gaps,
}
```

`decided_by_layer: Option<u8>` (`agent_result.rs:38-42`) is `#[serde(default)]`, and its own doc
comment says, quoted verbatim: *"`None` is reserved for test-only fixture literals that don't route
through the real cascade."* `[VERIFIED: crates/devflow-core/src/agent_result.rs:38-42]`. This
confirms D-08's warning: a hand-written test fixture that omits `decided_by_layer` silently produces
`None`, which normalises to `layer0 = false` — exercising only HALF of the 42-cell matrix unless the
sweep fixture sets it explicitly for every cell meant to represent Layer 0.

### The current implementation, verified verbatim

`classify_validate_outcome` (`crates/devflow-cli/src/pipeline_outcomes.rs:203-215`):

```rust
pub(crate) fn classify_validate_outcome(result: &agent_result::AgentResult) -> ValidateOutcome {
    let external = result.decided_by_layer == Some(0) && result.status == AgentStatus::Success;
    match (external, result.verdict) {
        (_, Some(Verdict::Pass)) => ValidateOutcome::Passed,
        (true, Some(Verdict::Gaps)) => ValidateOutcome::Ambiguous(
            "external verification passed but the agent reported gaps".to_string(),
        ),
        (true, None) => ValidateOutcome::Ambiguous(
            "external verification passed but no agent verdict arrived".to_string(),
        ),
        _ => ValidateOutcome::Failed,
    }
}
```

This is the exact match D-06's rewrite replaces. `ValidateOutcome` has three variants:
`Passed`, `Failed`, `Ambiguous(String)` (`pipeline_outcomes.rs:160-171`).

### The verified rewrite shape

D-06's amendment (`34-CONTEXT.md`) specifies: normalise `decided_by_layer` to a LAYER-ONLY boolean
FIRST (`let layer0 = result.decided_by_layer == Some(0);`), never reusing the composite `external`
predicate (which folds a `status == Success` equality test back in — exactly the hand-audited
construct D-06 exists to eliminate). Then `match (layer0, status, verdict)` — a wildcard is
permitted in the `layer0` or `verdict` position but **forbidden in the `status` position**, both
toward `Passed` and toward `Failed` (the earlier draft only banned the `Passed` direction, which
still let `_ => Failed` compile untouched against a new `AgentStatus` variant).

`34-REVIEW.md`'s second pass records this was compiled and verified writable independently by two
lanes: a wildcard-free match over `(Option<u8>, AgentStatus, Option<Verdict>)` compiles in 10 arms
(not 42 written arms — several `AgentStatus` variants share an identical destination and can be
combined with `|` since only the `status` **position** must be enumerated, not each combination
written out separately), and two negative controls hold: deleting a status arm and adding an 8th
`AgentStatus` variant both produce `E0004` (non-exhaustive match). This research did not
independently re-derive the compile because doing so would require editing source (forbidden by the
verification discipline for a research-only session) — it is reported here as `[CITED:
34-REVIEW.md S-03]`, MEDIUM confidence, since the review's own claim is a compiler-checked fact
reported by two independent lanes rather than reasoned prose.

The two `external`-gated `Ambiguous` arms must survive as `(true, Success, Some(Gaps))` and
`(true, Success, None)`; `(false, Success, Some(Gaps) | None)` must stay `Failed` — this is what
"preserving the ordinary auto-loop" (D-05's "what this does NOT cover" paragraph) requires, and
matches criterion 4/5's note that "criterion 3's fix does not close" the graft defect: these arms
gate on the DERIVED `status`/`verdict`, and the graft's bug is upstream of them — it makes
`result.status` and `result.verdict` genuinely (but wrongly) both read as an affirmative pair before
the classifier ever runs.

**Where it physically lands (Claude's Discretion):** `classify_validate_outcome` itself is 13 lines
today and already a single pure function with no other callers of its internal match shape — the
research recommends keeping the match INLINE rather than extracting a helper. A helper only earns
its keep if the 42-cell sweep test needs to construct the match's input tuple independently of a
full `AgentResult` — which it does not, since `AgentResult` already exists as the natural fixture
type and the existing test module already builds full `AgentResult` literals for this file's other
tests (see `pipeline_outcomes.rs`'s test module, not read in full this session but referenced by
the D-08 sweep requirement).

### `RateLimited` and `AgentUnavailable` — the two variants criterion 3 explicitly adds

Both are unreachable at the classifier today (per R-01/R-08: `decide_action` routes `RateLimited` to
`AutoResume` and both `ResourceKilled`/`AgentUnavailable` to `GateInfra`, never to
`Action::Advance`, so `classify_validate_outcome` is never called with either status). The exhaustive
match still needs a decided destination for them (compile-time exhaustiveness demands it even for
unreachable inputs), and `decide_action`'s routing of `RateLimited` to `AutoResume`
(`crates/devflow-core/src/outcome_policy.rs:47`, quoted: `AgentStatus::RateLimited =>
Action::AutoResume`) is a LIVE, DEFENDED choice — sending that cell to an immediate gate inside
`classify_validate_outcome` would not contradict production behaviour (the cell is unreachable there
either way) but WOULD contradict the intent recorded at the routing layer if a future change ever
made it reachable. The safe, consistent choice for both is `Failed` (the same destination the
current `_` arm already gives them), not a new distinct behaviour.

## The `reconcile_layer0_verdict` Fix (999.74 criteria 4/5, the graft)

### The exact defect, re-verified verbatim this session

`crates/devflow-core/src/agent_result.rs:2143-2156`:

```rust
fn reconcile_layer0_verdict(
    project_root: &Path,
    state: &State,
    result: AgentResult,
) -> AgentResult {
    if state.stage != Stage::Validate
        || result.status != AgentStatus::Success
        || result.decided_by_layer != Some(0)
    {
        return result;
    }
    let verdict = evaluate_layer1(project_root, state.phase).and_then(|layer1| layer1.verdict);
    AgentResult { verdict, ..result }
}
```

Called from `evaluate_agent_result_inner` (`agent_result.rs:2297-2330`) on the Layer 0 arm:

```rust
if let Some(result) = evaluate_layer0(project_root, state, approved_commands) {
    return Ok(reconcile_layer0_verdict(project_root, state, result));
}
```

— `[VERIFIED: crates/devflow-core/src/agent_result.rs:2304-2305]`.

### What value, from which function, is available at the call site

`evaluate_layer1` (`agent_result.rs:1789-1806`, `pub fn`) returns `Option<AgentResult>`. Its
FIRST statement, before any parser runs, is the idle-timeout side channel:

```rust
if let Some(timed_out) = parse_idle_timeout_side_channel(project_root, phase) {
    return Some(timed_out);
}
```

The returned `AgentResult` — call it `layer1` — carries BOTH `.status` and `.verdict`. Today's code
takes only `.verdict` (`.and_then(|layer1| layer1.verdict)`) and discards `.status` entirely. **The
fix is exactly one field away from what's already computed:** change the graft's condition to also
require `layer1.status == AgentStatus::Success` (equivalently, bind `layer1` once and check both
fields) before transplanting `layer1.verdict`; when Layer 1's own status is not `Success` (e.g. a
self-reported `{"status":"failed","verdict":"pass"}` marker, which parses to `layer1.status ==
Failed`), the graft must not run and `result.verdict` stays whatever Layer 0 already produced
(`None`, since `evaluate_layer0`'s affirmative-success constructor sets `verdict: None` —
`agent_result.rs:2093-2101`, the `None =>` arm).

Concretely, with that fix, a marker `{"status":"failed","verdict":"pass"}` at Validate with Layer 0
affirmative success produces `result.status == Success` (Layer 0's own, unaffected — the fix touches
only `.verdict`), `result.verdict == None` (graft did not fire), `result.decided_by_layer ==
Some(0)`. Fed into the (rewritten, D-06) classifier: `layer0=true, status=Success, verdict=None` →
the `(true, Success, None)` arm → `Ambiguous` → immediate gate, not `Passed`/Ship. This is exactly
criterion 4's required post-fix behaviour, traced through both fixes together.

### The smallest in-repo test harness — no out-of-repo temp project needed

D-15's demonstration was originally run "against a HEAD-built `advance` binary in out-of-repo temp
projects" (CONTEXT.md). That was necessary for a CROSS-CUTTING end-to-end demonstration touching
`advance()`'s dispatch (in `devflow-cli`) plus the graft (in `devflow-core`), but a smaller
same-crate harness already exists and needs only a fourth case added.

`layer0_affirmative_success_consults_layer1_verdict_at_validate`
(`crates/devflow-core/src/agent_result.rs:5488-5540`, run this session — `1 passed; 0 failed`)
already builds exactly the required fixture: a tempdir with a `.planning/phases/16-reliability/16-01-PLAN.md`
declaring `external_verify: "test -f shipped"`, the file `shipped` present so the probe passes
(Layer 0 → `Success`, `decided_by_layer: Some(0)`), `state.stage = Stage::Validate`, and an
`approval` vector matching the declared command — then calls
`evaluate_agent_result_inner(dir.path(), &state, &GitFlowConfig::default(), Some(&approval))`
directly (the real cascade, not a mock), writing three different `DEVFLOW_RESULT` markers to
`stdout_path` in turn and asserting `result.verdict` for each: `{"status":"success","verdict":"pass"}` →
`Some(Pass)`, `{"status":"success","verdict":"gaps"}` → `Some(Gaps)`, no marker at all → `None`.

**The smallest addition for D-15's exploit case:** a fourth marker,
`{"status":"failed","verdict":"pass"}`, asserting PRE-FIX `result.verdict == Some(Pass)` (the
exploit) and — once the fix lands — `result.verdict == None` (the graft correctly declines because
`layer1.status != Success`). This is entirely within `devflow-core`'s own test module; no
`devflow-cli` crate, no `classify_validate_outcome`, no spawned `advance` binary, and no out-of-repo
directory required. `state.worktree_path` defaults unset in this fixture (`state_in`,
`agent_result.rs:2662-2666`, sets only `stage`), so the harness also does NOT depend on 999.76's
worktree-discovery fix — it exercises the main-checkout code path, which is where D-15's
`external_verify_enabled` default (`true`, `config.rs:81`) already applies.

**For the end-to-end Ship/gate demonstration criterion 4 also asks for**, a second, `devflow-cli`
crate-level test is needed, because `classify_validate_outcome` is `pub(crate)` to `devflow-cli`
(cannot be called from `devflow-core`'s test module). That test can construct an `AgentResult`
literal directly (status/verdict/decided_by_layer set by hand) rather than re-running the full
cascade — it only needs to prove the CLASSIFIER's cell mapping, which the 42-cell D-08 sweep already
requires writing. The two tests together (one in each crate) fully cover the graft
(`devflow-core`, real cascade) and its downstream routing consequence (`devflow-cli`, direct
construction) without needing a spawned binary or a temp project outside the repo.

### Negative controls, verified mechanically

- **Verdict removed or set to `gaps`, status still `failed` (same graft precondition otherwise):**
  Layer 1 reports `verdict: None` or `Some(Gaps)`; even PRE-FIX, the graft transplants that same
  value onto `result.verdict` — `None`/`Some(Gaps)` either way route to `Ambiguous`, not `Passed`.
  This proves the specific exploit needs BOTH `status: failed` AND `verdict: pass` together — a
  weaker marker does not reproduce it, pre-fix or post-fix.
- **Layer 0 disabled (`external_verify_enabled(project_root) == false`, or no PLAN declaration):**
  `evaluate_layer0` (`agent_result.rs:2032-2038`) returns `None` immediately, the cascade falls
  through to Layer 1 directly (`agent_result.rs:2315`), which reports `status: Failed` verbatim
  (no Layer 0 to launder it) — `decide_action(stage, AgentStatus::Failed)` routes to `GateReview`
  (`outcome_policy.rs:56`, quoted: `AgentStatus::Failed => Action::GateReview`), never reaching
  `Action::Advance` or `classify_validate_outcome` at all. This is the R-01 mechanism, independently
  re-verified this session by running `cargo test -p devflow-core --lib outcome_policy::`: **9
  passed; 0 failed; 538 filtered out** (non-zero filtered count confirms the selector matched real
  tests, per CLAUDE.md's `--exact` false-green caution — this run used a module-path filter, not
  `--exact`, and the result includes a per-non-`Success`-variant named test).

### The corollary CONTEXT.md flags — verified, not touched

`idle_timeout_result`'s doc comment (`agent_result.rs:1742-1749`, read verbatim): *"`verdict` stays
`None` deliberately: at `Stage::Validate`, `classify_validate_outcome` matches
`Some(Verdict::Pass)` FIRST and would classify the stage as passed on the strength of that field
alone, whatever the status says. A timeout has no verdict to offer, and inventing one here would
advance a run that never reported."* This documents a mechanism the graft fix does not remove
(D-06's rewrite still checks `verdict` structurally, and `reconcile_layer0_verdict`'s own guard —
`state.stage != Stage::Validate || result.status != AgentStatus::Success || result.decided_by_layer
!= Some(0)` — already excludes an idle timeout twice over: its `status` is `IdleTimeout` not
`Success`, and its `decided_by_layer` is `Some(1)` not `Some(0)`, per the function's own doc
comment at `agent_result.rs:2138-2142`). **Do not edit this comment** — confirmed live, per D-15.

## 999.76 — Layer 0 Discovery from the Execution Root (criterion 6)

### The defect, re-verified verbatim and re-run live this session

`evaluate_layer0` (`crates/devflow-core/src/agent_result.rs:2032-2042`):

```rust
fn evaluate_layer0(
    project_root: &Path,
    state: &State,
    approved_commands: Option<&[String]>,
) -> Option<AgentResult> {
    if !crate::config::external_verify_enabled(project_root) {
        return None;
    }

    let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
    let commands = crate::verify::external_verify_commands(project_root, state.phase);
    ...
```

`execution_root` is computed and then used only later (to RUN probes); the DISCOVERY call on the
very next line reads `project_root` unconditionally. `[VERIFIED:
crates/devflow-core/src/agent_result.rs:2036-2042]`.

The prior-decision comment CONTEXT.md flags is real and reads, verbatim, at `agent_result.rs:2025-2031`:

> "Two roots are intentionally kept distinct (review Plan 03 MEDIUM, OpenCode): `project_root` is
> used to DISCOVER the PLAN's declared commands (`.planning/phases/` lives there, not in a worktree
> checkout), while `execution_root` — the worktree, when one is set — is where probes actually RUN.
> Conflating the two previously meant a worktree-based phase could not find its own declaration and
> silently mis-hit the 'PLAN removed' veto below."

This is exactly what 999.76's fix must knowingly overturn — it is not an oversight to quietly patch,
it is a recorded prior decision (from Plan 03's own review) whose premise the criterion asserts is
false: `.planning/` is TRACKED content, so an in-flight phase's `{N}-PLAN.md` lives on
`feature/phase-{N}` inside the worktree and IS absent from the main checkout for the phase's whole
duration — the comment's premise has the direction backwards.

### The second call site, same root cause

`crates/devflow-cli/src/pipeline_launch.rs:957`:

```rust
&& verify::phase_has_blocking_human_checkpoint(project_root, phase)
```

`phase_has_blocking_human_checkpoint` (`crates/devflow-core/src/verify.rs:121-127`, `pub fn`) calls
`phase_plan_files(project_root, phase)` (`verify.rs:36`), which reads
`project_root.join(".planning/phases")` unconditionally — never `state.worktree_path`.
`[VERIFIED: crates/devflow-core/src/verify.rs:36-44, 121-127]`. This silently kills the plan-28-03
checkpoint auto-decide path in worktree mode, exactly as CONTEXT.md describes — the correct fix
threads the same `execution_root`-style value through this call site too.

### The negative control, re-run live this session — fresher than CONTEXT.md's example

CONTEXT.md's illustrative negative control (`git ls-tree -r develop --name-only -- .planning/phases
| grep -c '/33-'` = 0 vs. the same against `HEAD` = 17) is **now stale**: Phase 33 shipped and merged
to `develop` on 2026-08-05 (`STATE.md`: `status: "Phase 33 shipped — PR #90"`), so both refs now
return **21**. Re-run live against THIS phase's own docs instead:

```
$ git ls-tree -r develop --name-only -- .planning/phases | grep -c '/34-'
0
$ git ls-tree -r HEAD --name-only -- .planning/phases | grep -c '/34-'
3
```

`develop` is at `91a1b58` (merge of PR #91, docs/phase-33-closeout-housekeeping); `HEAD` (this main
checkout, on `feature/phase-34`) is at `973a115`. This is the correct live reproduction for a test
written during Phase 34 itself: `.planning/phases/34-.../34-CONTEXT.md`,
`34-DISCUSSION-LOG.md`, `34-REVIEW.md` exist on `HEAD` and not on `develop`. The plan's own test
should either reproduce this pattern against Phase 34's own docs, or (more robustly, so it does not
go stale again once Phase 34 ships) construct its own tempdir fixture with a synthetic worktree and
main-checkout divergence, following the pattern `layer0_affirmative_success_consults_layer1_verdict_at_validate`
already uses for Layer 0 tests. Note the caution already recorded in CONTEXT.md: the
NON-recursive `git ls-tree` form (no `-r`) returns 0 for every ref regardless of content, and proves
nothing — always use `-r`.

### The existing green test that structurally cannot catch this

`external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree`
(`crates/devflow-core/src/agent_result.rs:5269`, doc comment `5259-5266`) writes the PLAN under the
tempdir standing in for `project_root` while pointing `state.worktree_path` at an EMPTY sibling
directory — manufacturing exactly the layout the defect assumes is universal, so it is structurally
incapable of exercising the bug. `[VERIFIED: crates/devflow-core/src/agent_result.rs:5259-5280]`.
Fixing the root read alone will not break this test (it still passes if discovery correctly reads
`project_root` and that IS where the PLAN lives in this fixture) — but the fix's own test coverage
must ALSO add a companion fixture where the PLAN lives in the WORKTREE-standing-in directory and
`project_root` does not have it, asserting discovery now succeeds there. `phase_commit_count`
(`agent_result.rs:1832` area) must NOT be retargeted — its own doc comment states, verbatim:
*"Must be called with the main `project_root`, never a worktree path — git worktrees share refs and
the object database, so a commit made inside a linked worktree is immediately visible to a count run
from the main checkout, which is the property every caller already relies on."* This is a DIFFERENT,
correct asymmetry (discovery on the worktree, commit-counting on the main checkout) that the fix must
preserve, not collapse.

## Criterion 7 Collateral — the Canary Test and the Legacy-Flag Test

### `canary_gate_only_applies_to_the_stream_launch_path` — becomes unconstructible, confirmed

`crates/devflow-cli/src/pipeline_launch.rs:1754-1780` (exact lines re-read this session):

```rust
fn canary_gate_only_applies_to_the_stream_launch_path() {
    ...
    state.stage = Stage::Plan;
    let stream_launch =
        claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
    assert!(
        !stream_launch,
        "Stage::Plan must still resolve to the legacy path for this test to mean anything"
    );
    // Negative control: the same predicate DOES fire for the widened stage,
    // so the reading above is a real discrimination and not a constant.
    assert!(
        claude_stream_launch_enabled(AgentKind::Claude, Stage::Code, false),
        "the predicate must still say yes somewhere, or the check above is vacuous"
    );
```

Once all five stages are in `STREAM_JSON_STAGES`, `state.stage = Stage::Plan` no longer resolves to
`false` — the FIRST assertion fails, and the test is unconstructible AS WRITTEN. Two candidate
discriminators still exist and were checked this session:

1. **The legacy opt-out (`legacy_claude_launch` / `DEVFLOW_CLAUDE_LEGACY_LAUNCH`)** — passing
   `legacy_opt_out = true` still makes `claude_stream_launch_enabled` return `false` regardless of
   which stages are widened, per the predicate's own logic (`!legacy_opt_out && ...`). This
   discriminator survives widening unconditionally — it is a boolean the predicate always respects,
   independent of `STREAM_JSON_STAGES`'s contents.
2. **A non-Claude agent** — `agent == AgentKind::Claude` is a separate `&&` term; any other
   `AgentKind` (Codex, etc.) also always returns `false` regardless of stage or the opt-out.

Both still yield `false` after widening. The opt-out (option 1) is the smaller, more targeted rebuild
— it keeps the test's ORIGINAL intent (a single-stage, real `Stage` value proving the discrimination
is stage-driven) closer to what the test's own doc comment claims to prove, whereas switching to a
non-Claude agent would change what property the test demonstrates (agent-kind gating, not stage
gating) — recommend rebuilding on the legacy opt-out, keeping the non-Claude-agent path available as
a secondary/companion assertion if the plan wants both discriminators covered.

### `legacy_launch_flag_forces_the_single_document_path` — confirmed does NOT need updating

`crates/devflow-cli/src/pipeline_launch.rs:2320-2334` (re-read this session): `legacy_state`
(`:2309-2314`) hardcodes `state.stage = Stage::Code` and `state.legacy_claude_launch = opt_out`. The
test's precondition assertion (`claude_stream_launch_enabled(state.agent, state.stage, false)` must
be `true`) and its main assertion (with `legacy_claude_launch = true`, must be `false`) both remain
valid regardless of how many OTHER stages join `STREAM_JSON_STAGES` — `Stage::Code` is already in
the list today and stays in it; the test never depends on any OTHER stage's membership. Confirmed:
this test requires no change.

## Runtime State Inventory

Not applicable — this is not a rename/refactor/migration phase. No string is being renamed across
stored data, live service config, OS-registered state, secrets, or build artifacts. Skipped per
protocol.

## Common Pitfalls

### Pitfall 1: Assuming `STREAM_JSON_STAGES` widening is evidenced by transport verification

**What goes wrong:** treating "I launched the widened stage and it used `--input-format
stream-json`" as sufficient evidence for criterion 1.
**Why it happens:** `ClaudeAgent::exec_command` (`crates/devflow-core/src/agents/claude.rs:46-61`,
re-verified verbatim this session) ignores `_phase`, `_prompt`, and `_extra_writable_roots` and
returns a byte-identical fixed argv for every stage — the transport itself carries zero per-stage
information.
**How to avoid:** the capture must be evaluated for what the AGENT does under that stage's specific
prompt (does it background work, does `background_tasks_changed` appear, does it drain) — never for
what flags the launch used.
**Warning signs:** a plan task whose acceptance criterion only checks the argv or the presence of
`--input-format stream-json` in the capture, without inspecting `background_tasks_changed` events.

### Pitfall 2: Rebuilding the binary without the widened constant already committed to the branch the rebuild reads from

**What goes wrong:** editing `STREAM_JSON_STAGES` in an editor, forgetting to save, or editing a
file in a stale worktree, then rebuilding and getting the OLD behaviour with no error.
**Why it happens:** the constant is compiled in; there is no runtime error for "the binary doesn't
have your edit" — it silently uses whatever WAS compiled.
**How to avoid:** verify the rebuilt binary's behaviour directly before trusting a capture run —
e.g., a one-line smoke check that `claude_stream_launch_enabled` returns `true` for the target stage
(a `#[test]` run against the just-built binary, or a debug print during the scratch run).
**Warning signs:** a "capture" that shows the legacy single-document path (no `--input-format
stream-json` in the launched argv) for a stage that was supposedly widened.

### Pitfall 3: Forgetting captures are gitignored

**What goes wrong:** running the capture, then `git add`-ing the evidence directory and finding
nothing staged, or discovering later that the capture was never committed.
**Why it happens:** `.devflow/.gitignore` is `*` — anything left inside `.devflow/` is invisible to
ordinary `git add`.
**How to avoid:** copy the capture file OUT of `.devflow/` into the phase's own
`.planning/phases/34-.../{unit}-evidence/` directory before any git operation, following Phase 30's
precedent exactly.
**Warning signs:** `git status` shows nothing new after a capture run that should have produced a
committable artifact.

### Pitfall 4: Running a capture in `--worktree` mode and expecting it to reflect an uncommitted constant edit

**What goes wrong:** `devflow start --phase N` (default `--worktree`) forks a clean worktree from
`develop`; if the operator's mental model is "the worktree gets my local edit," the capture silently
reproduces legacy behaviour with no error, and the operator may not notice until reading the raw
capture and finding no `stream-json` events.
**Why it happens:** `worktree::add(project_root, &wt, &branch, DEVELOP, true)`
(`crates/devflow-cli/src/parallel.rs:30`) forks strictly from `develop`'s current tip, independent of
any uncommitted state anywhere else.
**How to avoid:** use `--no-worktree` for the capture-taking run specifically, and verify (per
Pitfall 2) that the binary driving the run has the widened constant before trusting the capture.
**Warning signs:** a capture from a `--worktree` run of a stage that should show
`--input-format stream-json` in argv, but doesn't.

### Pitfall 5: Believing `enforce_build_staleness` will catch a stale-binary capture attempt

**What goes wrong:** assuming the self-dogfood staleness gate will refuse to run if the binary
doesn't match the widened source, and skipping the manual verification in Pitfall 2.
**Why it happens:** the gate's name and its hard-block behaviour ("self-dogfood stale build
blocked") sound like exactly this protection.
**How to avoid:** understand what it actually checks — `combined_staleness`
(`crates/devflow-core/src/staleness.rs:209-223`) compares the EXECUTION root's own git ancestry and
working-tree dirtiness against the binary's embedded build commit; it says nothing about whether the
binary's COMPILED-IN CONSTANTS match anything, and a cleanly-checked-out fresh worktree (the default
`--worktree` case) is never flagged Stale by this check regardless of how old the running binary is.
**Warning signs:** `devflow start` proceeding without any staleness warning, taken as confirmation
the binary is current — it is not that kind of check.

### Pitfall 6: Reusing 31-ACCEPTANCE.md's pass bar verbatim for a non-backgrounding stage

**What goes wrong:** applying the VOID rule — *"VOID unless the capture shows a
`background_tasks_changed` event with a NON-EMPTY `tasks` array followed by a drain to `[]`"*
(`31-ACCEPTANCE.md:25-26`, quoted verbatim, re-verified this session) — to a stage (e.g. Define) that
never backgrounds anything at all.
**Why it happens:** it is the only precedented per-stage pass bar in the repo, and it is tempting to
copy it directly.
**How to avoid:** a stage whose capture shows `BackgroundTaskState::NeverAnnounced` throughout is
**vacuously drained by design** (`crates/devflow-core/src/monitor.rs:533-547`, the enum's own doc
comment) — this is a VALID pass for that stage, not a void run. Apply the VOID rule only to a stage
that DOES announce background tasks.
**Warning signs:** a plan task that fails a Define-stage capture for lacking a
`background_tasks_changed` event, when Define legitimately never backgrounds anything.

## Code Examples

### The verified pre-fix vs. post-fix graft behaviour, side by side

```rust
// PRE-FIX (current, agent_result.rs:2143-2156):
fn reconcile_layer0_verdict(project_root: &Path, state: &State, result: AgentResult) -> AgentResult {
    if state.stage != Stage::Validate
        || result.status != AgentStatus::Success
        || result.decided_by_layer != Some(0)
    { return result; }
    let verdict = evaluate_layer1(project_root, state.phase).and_then(|layer1| layer1.verdict);
    AgentResult { verdict, ..result }
}

// POST-FIX (shape only — the exact binding style is Claude's Discretion):
fn reconcile_layer0_verdict(project_root: &Path, state: &State, result: AgentResult) -> AgentResult {
    if state.stage != Stage::Validate
        || result.status != AgentStatus::Success
        || result.decided_by_layer != Some(0)
    { return result; }
    let layer1 = evaluate_layer1(project_root, state.phase);
    let verdict = layer1
        .filter(|l1| l1.status == AgentStatus::Success)
        .and_then(|l1| l1.verdict);
    AgentResult { verdict, ..result }
}
```

The `.filter(...)` is one illustrative shape, not a locked implementation choice — the load-bearing
requirement (D-15) is "consult Layer 1's status before transplanting Layer 1's verdict," which this
satisfies; other equivalent shapes (an explicit `if let`/`match`) are equally valid.

## State of the Art

Not applicable in the usual sense (no external library/framework version drift to track). The only
"state of the art" axis here is DevFlow's own accumulated understanding of its own defect, which
`34-REVIEW.md`'s two-pass structure already documents in full — R-01's conclusion (unreachable) was
itself superseded by S-01 (reachable via the graft) within the same day. This research found no
further reversal; every claim in the second pass held under re-verification this session.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `[ASSUMED]` The recommended step-3 "reinstall/re-path" mechanism for a locally rebuilt `devflow` binary was not investigated (no install script or symlink convention was located in this research session). | Capture Acquisition Mechanics, step 3 | The planner may need a short investigation task to confirm how this repo normally promotes a local build (`cargo install --path`, a symlink, a `PATH` convention) before the capture-run plan can specify an exact command. |
| A2 | `[ASSUMED]` D-06's rewrite compiling in "10 arms" is reported per `34-REVIEW.md` S-03's own claim, not independently recompiled in this research session (editing source was out of scope for a research-only pass). | The Exhaustive-Match Rewrite | Low — the review's claim is a compiler-checked fact reported by two independent lanes (`hermes` and an internal agent), not reasoned prose; if wrong, `cargo check` will say so immediately once the plan implements it, at negligible cost. |
| A3 | `[ASSUMED]` The minimal scratch-phase shape for capture acquisition (a documentation-only or single-file change reaching all five stages) was not test-run this session — it is a recommendation based on the operator's stated "cheapest workload" preference, not a verified minimal reproduction. | Capture Acquisition Mechanics, step 4 | If the chosen scratch content is TOO minimal, a stage might complete so fast it produces no `background_tasks_changed` event at all even where the real workload would — the plan should sanity-check that the scratch phase still exercises genuine agent tool-use per stage, not a no-op. |

**All other claims in this research are `[VERIFIED: file:line]`** — read directly this session, with
verbatim quotes beside every discrete value cited (enum variants, constants, doc-comment text) — or
`[CITED: 34-REVIEW.md ...]` for the one compiler-checked claim (A2) this session did not re-derive
by editing source.

## Open Questions (both RESOLVED by the Phase 34 plan set, 2026-08-05)

> Annotated after planning, per the plan-checker's warning 1. Neither question is a live ambiguity —
> each is discharged by a named task or a recorded convention. The original text is kept intact
> below so the reasoning that produced the question stays legible.

1. **Exact local-binary promotion mechanism for a rebuilt `devflow`.** — **RESOLVED as an explicit
   investigation task, not as an assumption.** `34-05-PLAN.md` Task 1 is exactly the "short
   investigation task" this question recommends, with concrete acceptance criteria and a
   `BINARY-PROMOTION.md` output. Planning also surfaced a **shadowing hazard the task must confront
   rather than inherit**: `/home/linuxbrew/.linuxbrew/bin/devflow` is a symlink into `target/release/`
   (tracks every release build) while `~/.local/bin/devflow` is a stale static copy at **v1.8.0** —
   so which one the shell resolves determines whether a capture run uses the rebuilt binary at all.
   That is a finding, not an answer; the task still has to establish the convention.
   - What we know: `cargo build -p devflow` produces a binary at `target/debug/devflow` (or
     `target/release/devflow`); the package name is `devflow` (`crates/devflow-cli/Cargo.toml:2`,
     verified).
   - What's unclear: whether this repo has an existing convention (a `cargo install --path
     crates/devflow-cli` step, a `PATH` entry, a Justfile/Makefile target) for promoting a locally
     built binary to be "the" `devflow` the operator's shell resolves.
   - Recommendation: a short investigation task at the start of Plan work for 999.73 (grep for
     `cargo install`, check `~/.cargo/bin/devflow`'s provenance, or check any `justfile`/`Makefile`
     in the repo root) rather than guessing.

2. **Whether the 42-cell D-08 sweep and the 6-case D-15 demonstration should live in one test
   function or several.** — **RESOLVED: many small, descriptively-named `#[test]` functions**, this
   question's own recommendation. `34-PATTERNS.md` independently confirmed the convention by reading
   the actual test bodies in both files rather than restating this claim, and `34-01-PLAN.md` /
   `34-03-PLAN.md` follow it. No table-driven macro is introduced.
   - What we know: both are pure-function tests over `AgentResult`/`ValidateOutcome` values, no I/O
     needed for the classifier sweep (constructed literals), and the graft demonstration needs the
     real cascade (`evaluate_agent_result_inner`, filesystem-backed tempdir fixture).
   - What's unclear: test-organization preference — one giant table-driven test vs. many small named
     tests (the existing pattern in both files favors many small named tests with descriptive names).
   - Recommendation: follow the existing convention (many small, descriptively-named `#[test]`
     functions) rather than introducing a table-driven macro pattern this codebase does not
     currently use elsewhere in these two files.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `cargo` | Rebuilding `devflow` with the widened constant; all `cargo test` verification | Yes | 1.97.1 | — |
| `rustc` | Compilation | Yes | 1.97.1 | — |
| `git` | Worktree creation, `ls-tree` negative controls, evidence commits | Yes | 2.55.0 | — |
| `claude` CLI | The real per-stage captures (criterion 1's evidence) | Yes | 2.1.222 (Claude Code) | — |

**Missing dependencies with no fallback:** none — everything the phase needs is present on this
machine, verified this session.

**Missing dependencies with fallback:** none applicable.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (standard Rust test harness), workspace crates `devflow-core` and `devflow` (devflow-cli's package name) |
| Config file | none dedicated — `scripts/check.sh` (read this session) is the single definition of "green": `fmt`, `clippy --all-targets -- -D warnings`, `test` |
| Quick run command | `cargo test -p devflow-core --lib <module_path>::` for a targeted module, or `cargo test -p devflow --lib <module_path>::` for devflow-cli |
| Full suite command | `scripts/check.sh all` (host) or `scripts/check-in-container.sh all` (pinned CI image) |

**Package-name caution (CLAUDE.md, re-stated because it is easy to get wrong here specifically):**
the devflow-core crate's package is `devflow-core`; the devflow-cli crate's package is **`devflow`**,
not `devflow-cli` — verified this session (`crates/devflow-cli/Cargo.toml:2`: `name = "devflow"`).
`cargo test --exact <name>` exits 0 when the name matches nothing — always assert on a real `N
passed` line with a non-zero `filtered out` count, never trust exit code alone.

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DOGFOOD-03 (criterion 1) | Every widened stage carries a real production capture; un-widened stages record a reason | live-run evidence, not unit-testable | manual capture run per § "Capture Acquisition Mechanics" | ❌ Wave 0 — no test file, this is an evidence artifact, not a pass/fail unit test |
| DOGFOOD-03 (criterion 2) | Capture answers what happens when the close rule does NOT fire, per stage | live-run evidence + inspection | same capture run, manually inspected against `BackgroundTaskState` | ❌ Wave 0 — evidence artifact |
| DOGFOOD-03 (criterion 7, canary test) | `canary_gate_only_applies_to_the_stream_launch_path` rebuilt on a surviving discriminator | unit | `cargo test -p devflow --lib pipeline_launch::tests::canary_gate_only_applies_to_the_stream_launch_path -- --exact` (assert `1 passed`) | ✅ existing test, needs rebuild, not a new file |
| DOGFOOD-03 (criterion 7, retention) | Retention eviction does not silently drop an unread capture | unit | new test in `devflow-core`'s `agent_result` test module asserting `prune_history`/`archive_phase_files` behaviour under the chosen mitigation | ❌ Wave 0 — new test needed |
| DOGFOOD-04 (criterion 3) | Exhaustive `(layer0, status, verdict)` match, 42 cells + named controls | unit | `cargo test -p devflow --lib pipeline_outcomes::tests::` (new tests added to this module) | ❌ Wave 0 — new tests needed, existing module confirmed present |
| DOGFOOD-04 (criterion 4) | `reconcile_layer0_verdict` consults Layer 1's status before grafting | unit | `cargo test -p devflow-core --lib agent_result::tests::layer0_affirmative_success_consults_layer1_verdict_at_validate -- --exact` (extend with 4th marker case) | ✅ existing test to extend |
| DOGFOOD-04 (criterion 4, end-to-end) | Pre-fix Ship / post-fix gate, with negative controls | integration (same-crate, tempdir-backed, no spawned binary) | new `devflow-core` test per § "The Smallest In-Repo Test Harness" | ❌ Wave 0 — new test needed |
| DOGFOOD-04 (criterion 5) | `idle_timeout_result`'s comment is confirmed NOT stale, left unedited | documentation review, not a test | N/A — verified this session, re-verify no edit was made in the diff | N/A |
| 999.76 (criterion 6) | Layer 0 discovery uses execution root; worktree discovery distinguishable from main-checkout discovery | unit | new companion fixture alongside `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` (`agent_result.rs:5269`) with the PLAN in the worktree-standing-in dir | ❌ Wave 0 — new fixture needed |
| 999.76 (criterion 6, second call site) | `phase_has_blocking_human_checkpoint` fixed together | unit | new test in `devflow-core`'s `verify` test module | ❌ Wave 0 — new test needed |

### Sampling Rate

- **Per task commit:** targeted `cargo test -p <package> --lib <module>::` for the module touched.
- **Per wave merge:** `scripts/check.sh all` (fmt + clippy + full test suite).
- **Phase gate:** full suite green before `/gsd-verify-work`; additionally, the live capture-run
  evidence (criteria 1/2) must exist as committed artifacts before the phase can close — these are
  not caught by `cargo test` at all and need explicit manual sign-off in the phase's own
  verification pass.

### Wave 0 Gaps

- [ ] `crates/devflow-core/src/agent_result.rs` — extend `layer0_affirmative_success_consults_layer1_verdict_at_validate` with the `{"status":"failed","verdict":"pass"}` case (D-15 demonstration, in-repo harness).
- [ ] `crates/devflow-core/src/agent_result.rs` — new worktree-vs-main-checkout companion fixture for `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` (999.76 criterion 6).
- [ ] `crates/devflow-core/src/verify.rs` — new test covering `phase_has_blocking_human_checkpoint` reading the execution root (999.76's second call site).
- [ ] `crates/devflow-cli/src/pipeline_outcomes.rs` — the 42-cell D-08 sweep plus its three named controls (criterion 3).
- [ ] `crates/devflow-cli/src/pipeline_launch.rs` — rebuild `canary_gate_only_applies_to_the_stream_launch_path` on the legacy-opt-out discriminator (criterion 7).
- [ ] Evidence directories under `.planning/phases/34-.../` — no test file, but Wave 0 should stub the directory layout (per Phase 30's precedent) before the live capture run, so the capture-copy step has a landing spot.
- Framework install: none — `cargo test` is already fully configured in this workspace.

## Security Domain

`security_enforcement` is absent from `.planning/config.json` — treated as enabled per protocol.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | No | This phase touches no auth surface — DevFlow's own orchestration process, no user-facing login. |
| V3 Session Management | No | Not applicable — `session_id` handling (Claude's own resume mechanism) is unmodified by this phase. |
| V4 Access Control | No | No access-control surface changes. |
| V5 Input Validation | Yes, narrowly | The graft fix and the exhaustive match are themselves a class of input-validation hardening — treating an agent's self-reported `verdict`/`status` fields as UNTRUSTED input that must be cross-checked against independently-derived signals (Layer 0's own probe result, `decided_by_layer` provenance) before being trusted for a Ship transition. This is the whole point of D-06/D-15; no new library needed, the existing `deserialize_verdict_lenient` (`agent_result.rs`, referenced in the `Verdict`-field doc comment) already handles malformed input by falling back to `None` rather than failing the parse. |
| V6 Cryptography | No | No cryptographic operations in this phase. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Self-reported agent status/verdict trusted over independently-derived signal (the exact 999.74 defect) | Tampering / Spoofing (an agent process "lying" about its own outcome, whether adversarially or through a benign self-contradiction) | Cross-check the self-reported field against an independently-derived one before trusting it for a state transition — exactly D-06's exhaustive match and D-15's status-gated graft. This is a general pattern worth naming for future similar fixes: any field an AGENT PROCESS writes into its own result envelope is, by construction, not more trustworthy than the process's own exit code / probe outcome, and a design that lets the self-reported field OVERRIDE the independently-derived one (as the current wildcard match and the current graft both do) is the general shape of this vulnerability class. |
| Layer 0's silent-skip failure mode (999.76) | Denial of Service (of the verification guarantee itself — a declared probe set silently never runs) | Layer 0 already fails LOUD when explicitly enabled and mis-configured (`"external verification approval mismatch"`); the 999.76 defect is the opposite failure mode — a correctly-declared probe set silently never executing because discovery reads the wrong root. The fix (read the execution root for discovery, matching where probes actually run) closes this without introducing a new loud-failure path that could itself become a new DoS vector (e.g., false-positive "PLAN removed" vetoes in worktree mode, which the SAME defect currently also causes on the OTHER branch of `evaluate_layer0`'s logic). |

## Sources

### Primary (HIGH confidence) — all read this session, all with exact `file:line`

- `crates/devflow-cli/src/pipeline_launch.rs` — `STREAM_JSON_STAGES` (446), `claude_stream_launch_enabled` (478-480), `run_monitor`/`run_pipe_owning_monitor` call (493, 513), `canary_gate` (333-354), `canary_gate_only_applies_to_the_stream_launch_path` (1754-1780), `legacy_launch_flag_forces_the_single_document_path` (2309-2334), `advance()`/`decide_action` dispatch (836-937), `archive_phase_files` call site (571-580).
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `ValidateOutcome` (160-171), `classify_validate_outcome` (203-215), `ValidateResult` (226-229), `handle_validate_outcome` (298-395+).
- `crates/devflow-cli/src/commands.rs` — `start()` (112-345), worktree-then-staleness ordering (238-244, 305).
- `crates/devflow-cli/src/parallel.rs` — `ensure_phase_worktree` (15-40), `worktree::add(..., DEVELOP, ...)` (30).
- `crates/devflow-cli/src/staleness.rs` — `combined_staleness` (209-223), `is_self_dogfood_workspace` (235-274), `staleness_outcome` (282-296), `enforce_build_staleness` (324-370+).
- `crates/devflow-cli/src/main.rs` — `__monitor` subcommand definition (125-145+).
- `crates/devflow-core/src/agent_result.rs` — `AgentResult` struct incl. `decided_by_layer` doc (20-42), `AgentStatus` (47-81), `Verdict` (107-113), `idle_timeout_result` (1742-1755+), `evaluate_layer1` (1789-1806), `evaluate_layer2` (~1900-1955), `evaluate_layer3` (~1963-2015), `evaluate_layer0` (2025-2101, incl. the "intentionally kept distinct" comment at 2025-2031), `reconcile_layer0_verdict` (2113-2156), `evaluate_agent_result`/`evaluate_agent_result_inner` (2288-2330), `history_dir`/`archive_stamp`/`archive_phase_files`/`archive_phase_files_with_stamp`/`prune_history` call (2402-2547), `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` (5255-5280), `layer0_affirmative_success_consults_layer1_verdict_at_validate` (5483-5540), `MARKER_SUCCESS_CLAIMING_PASS` (5863-5867), `state_in` (2662-2666).
- `crates/devflow-core/src/outcome_policy.rs` — `Action` (16-27), `decide_action` (38-63, full body).
- `crates/devflow-core/src/monitor.rs` — `BackgroundTaskState` (533-547), `CloseRule` (553-593), supervise loop incl. `RecvTimeoutError::Timeout`/`fire_idle_timeout` (760-810), `close_rule_is_vacuously_drained_when_no_background_tasks_event_appears` (1303).
- `crates/devflow-core/src/stage.rs` — `Stage` enum, five variants (12-26).
- `crates/devflow-core/src/config.rs` — `DEFAULT_CAPTURE_RETENTION` (12), `Default for DevflowConfig` incl. `external_verify_enabled: true` (77-85), `capture_retention()`/`external_verify_enabled()` accessors, `capture_retention()` free fn w/ env override (136-149), `external_verify_enabled()` free fn (172-181).
- `crates/devflow-core/src/verify.rs` — `TRUST_EXTERNAL_VERIFY_ENV` (10), `external_verification_approval` (17-20), `phase_plan_files` (36-52), `phase_has_blocking_human_checkpoint` (109-127).
- `crates/devflow-core/src/agents/claude.rs` — `exec_command` (46-61, ignored `_phase`/`_prompt`/`_extra_writable_roots`).
- `/var/home/denniyahh/Github/devflow/.devflow/.gitignore` — literal `*`, read directly.
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/{30a,30c,30c-scrubbed,30c-operator,30d}-evidence/` — all four/five evidence-directory variants listed and inspected (file listings + line-count diffs).
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30c-VERDICT-scrubbed.md` — redaction-field table (216-223), operator-trial correction (160-176).
- `.planning/milestones/v2.3.0-phases/31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl/31-ACCEPTANCE.md` — VOID pass-bar quote (10-29).
- `.planning/milestones/v2.3.0-phases/31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl/31-VERIFICATION.md` — raw capture deletion note (line 15, and constraint-9 item 9 at line 114).
- `cargo test -p devflow-core --lib outcome_policy::` — run live this session, `9 passed; 0 failed; 538 filtered out`.
- `cargo test -p devflow-core --lib layer0_affirmative_success_consults_layer1_verdict_at_validate` — run live this session, `1 passed; 0 failed; 546 filtered out`.
- `git ls-tree -r develop/HEAD --name-only -- .planning/phases | grep -c '/34-'` — run live this session, `0` / `3`.
- `cargo --version`, `rustc --version`, `git --version`, `claude --version` — run live this session.
- `crates/devflow-core/Cargo.toml:2`, `crates/devflow-cli/Cargo.toml:2` — package names `devflow-core` and `devflow`.

### Secondary (MEDIUM confidence)

- `34-CONTEXT.md`, `34-REVIEW.md` — both read in full; treated as the phase's own prior research
  rather than independently re-verified where they cite `cargo test` results this session also
  reproduced (outcome_policy::, 9 passed / 538 filtered — matches exactly).
- `34-REVIEW.md` S-03's "10-arm, two negative controls (E0004)" compile claim — reported, not
  independently recompiled (would require editing source, out of scope for a research-only pass).

### Tertiary (LOW confidence)

- None used unmarked as such — every claim in this document is either `[VERIFIED: file:line]`,
  `[CITED: 34-REVIEW.md ...]`, or listed explicitly in the Assumptions Log as `[ASSUMED]`.

## Metadata

**Confidence breakdown:**
- Capture acquisition mechanics: HIGH on the mechanism (worktree-fork-from-develop,
  gitignore-everything, retention-eviction, all read and in two cases re-run live this session);
  MEDIUM on the exact local-binary-promotion step (A1, not investigated) and the minimal
  scratch-phase shape (A3, not test-run).
- Exhaustive-match rewrite: HIGH on the enumerated values (AgentStatus/Verdict read verbatim);
  MEDIUM on the exact arm count (A2, review-reported compile result, not independently recompiled).
- `reconcile_layer0_verdict` fix: HIGH — the defect, the call-site value already available, the
  existing test to extend, and all three negative controls were read/re-run/derived directly this
  session.
- 999.76: HIGH — the defect, the second call site, the stale-example correction, and a fresh live
  negative control were all verified this session.
- Criterion 7 collateral: HIGH — both tests read verbatim, both discriminators checked against the
  predicate's actual logic.

**Research date:** 2026-08-05
**Valid until:** this is internal, fast-moving, in-flight source under active development on the
same branch this research was conducted against (`feature/phase-34`, HEAD `973a115`) — treat this
research as valid only until the next commit touches any of the cited files. Re-verify line numbers
before the plan is executed if any wave lands source changes to `pipeline_launch.rs`,
`pipeline_outcomes.rs`, `agent_result.rs`, `verify.rs`, `monitor.rs`, `config.rs`, or `staleness.rs`
ahead of a later wave that also cites them.

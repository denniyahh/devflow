# Phase 47: Unattended Decision Policy Consistency — Pattern Map

**Mapped:** 2026-09-10
**Files analyzed:** 15 (2 Rust source, 2 new snapshot dirs, 6 build/tooling, 5 docs/planning)
**Analogs found:** 13 / 15 exact-or-role match · 1 partial · **1 with NO analog (`insta` — say so, do not invent one)**

> Every analog path below was checked with `git ls-files` and is **tracked source** in this
> worktree. No gitignored mirror paths appear here.

---

## File Classification

| New/Modified File | New/Mod | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|---|
| `crates/devflow-core/src/prompt.rs` | MOD | pure-function prompt builder + unit tests | transform (data → string) | itself: `workflow_code_prompt` `prompt.rs:454-474` | **exact** (self-analog: the correct arm already exists) |
| `crates/devflow-cli/src/pipeline_launch.rs` | MOD | launch-shape resolver + unit tests | request-response (build argv, no spawn) | `resolve_launch_shape` `pipeline_launch.rs:186-217` | **exact** (explicit stated precedent, 31-04) |
| `crates/devflow-core/src/snapshots/*.snap` | NEW | golden artifact | file-I/O (generated, reviewed, committed) | `crates/devflow-cli/tests/snapshots/devflow-help.txt` | **partial** — different mechanism, same review contract |
| `crates/devflow-cli/src/snapshots/*.snap` | NEW | golden artifact | file-I/O | same as above | **partial** |
| *(insta snapshot test bodies)* | NEW | test | transform | **NONE — zero `insta` usage in this workspace** | **no analog** |
| `Cargo.toml` (`[workspace.dependencies]`) | MOD | config | build-time | `Cargo.toml:19-32` (`serde_json` entry, `:24-28`) | **exact** |
| `crates/devflow-core/Cargo.toml` | MOD | config | build-time | `crates/devflow-core/Cargo.toml:32-33` (`tempfile`) | **exact** |
| `crates/devflow-cli/Cargo.toml` | MOD | config | build-time | `crates/devflow-cli/Cargo.toml:24-27` | **exact** |
| `Cargo.lock` | MOD (generated) | lockfile | build-time | n/a — never hand-edited | n/a |
| `scripts/check.sh` (`run_test`) | MOD | shared build script | batch (sequential fail-fast) | `run_clippy` `scripts/check.sh:34-38` | **exact** (sibling function, same file) |
| `.gitignore` (`*.snap.new`) | MOD | config | n/a | `.gitignore:63-72` (commented block idiom) | **exact** |
| `docs/guides/unattended-mode.md` | MOD | docs | n/a | itself (`:29-31`, `:85-88`) | **exact** |
| `.planning/ROADMAP.md` | MOD | planning record | n/a | prior phase-goal amendments in the same file | **role-match** |
| `.planning/REQUIREMENTS.md` | MOD | planning record | n/a | DECN-02/DECN-03 entries `:18-34`, `:185-187` | **role-match** |
| `.planning/user/DEV-SETUP-CHECKLIST.md` | MOD | docs/checklist | n/a | `DEV-SETUP-CHECKLIST.md:398-407` (§8 Rust tooling) | **exact** |
| `.planning/phases/47-.../47-VALIDATION.md` | NEW | planning record | n/a | prior phases' `NN-VALIDATION.md` | **role-match** |

**Optional-but-recommended (planner decision, D-16 hardening):** a new
`crates/devflow-cli/tests/*.rs` guard asserting `run_test` carries `env -u INSTA_FORCE_UPDATE`.
Its analog is exact — see § "Shared Patterns → Asserting on a shell script's source".

---

## Pattern Assignments

### `crates/devflow-core/src/prompt.rs` (prompt builder + tests, transform)

**Analog for the shared-constant hoist (D-04): `CODE_STAGE_POLICY` itself, `prompt.rs:57-89`.**

The file's own convention for a policy constant is a `const &str` with backslash-continuation and a
doc comment that names its counterpart. `GATE_RESOLUTION_RULE` should be the same shape, sited
immediately above `CODE_STAGE_POLICY`:

```rust
// prompt.rs:57-62
/// Shared policy for full-execute Code prompts. This advises the agent during
/// one-shot Code execution, while [`checkpoint_auto_decide_prompt`] is injected
/// into a resumed session after DevFlow's own human-blocking gate finds no
/// operator. They are complementary, not duplicates; neither should replace or
/// be deleted as the other.
const CODE_STAGE_POLICY: &str = "\
```

The text being made conditional, verbatim (`prompt.rs:86-89`):

```rust
This authority does not extend to a `blocking-human` gate or a \
package-verification checkpoint. Those remain human-only: do not self-resolve \
or approve them; report them instead. This policy must not pause execution or \
request human input.";
```

> ⚠️ **D-02 narrowing must preserve the package-verification half.** That sentence names *two*
> classes in one breath. The carve-out attaches only to `` `blocking-human` ``. Splitting the
> sentence is the mechanical move; deleting it is not.
> ⚠️ **The doc comment at `:57-61` mentions `checkpoint_auto_decide_prompt`.** Per CLAUDE.md
> ("a grep over source counts comment prose"), every verify-block count over this file must strip
> comments first — `grep -v "^[[:space:]]*//"`, as 46-04/46-06/46-07 do.

**Analog for the shared arm helper (D-05): `workflow_code_prompt`, `prompt.rs:454-474`.**
This is the already-correct implementation the helper must be extracted *from* without changing
its behaviour:

```rust
// prompt.rs:454-474
fn workflow_code_prompt(phase: PhaseId, fix: Option<FixType>, workflow_root: &str) -> String {
    match fix {
        Some(FixType::AuditFix) => format!(/* … no CODE_STAGE_POLICY … */),
        Some(FixType::GapsOnly) => format!(/* … no CODE_STAGE_POLICY … */),
        Some(FixType::FullExecute) | None => format!(
            "Read and follow the GSD workflow file at {workflow_root}/execute-phase.md for \
            phase {phase} --auto. The `--auto` flag is part of the workflow invocation and \
            must be preserved verbatim.\n\n\
            {CODE_STAGE_POLICY}\n\
            \n\
            {COMPLETION_PROTOCOL}"
        ),
    }
}
```

**The gap being closed — `fix_prompt`, `prompt.rs:567-581`.** Note it takes `FixType` (not
`Option<FixType>`), so the shared helper's signature must bridge the two:

```rust
// prompt.rs:567-581
pub fn fix_prompt(fix_type: FixType, phase: PhaseId) -> String {
    let command = match fix_type {
        FixType::AuditFix   => format!("/gsd-audit-fix {phase}"),
        FixType::GapsOnly   => format!("/gsd-execute-phase {phase} --gaps-only {AUTO_CHAIN_PRESERVING_FLAG}"),
        FixType::FullExecute=> format!("/gsd-execute-phase {phase} {AUTO_CHAIN_PRESERVING_FLAG}"),
    };
    format!(
        "Validation reported issues. Run the fix command for this loop:\n\n    {command}\n\n{COMPLETION_PROTOCOL}"
    )
}
```

Single production call site (`prompt.rs:399`), inside `render_claude_style`:

```rust
// prompt.rs:391-399
pub fn render_claude_style(intent: &StageIntent) -> String {
    match intent {
        StageIntent::Code { phase, fix: None } => code_stage_prompt(*phase),
        StageIntent::Code { phase, fix: Some(fix) } => fix_prompt(*fix, *phase),
        …
```

**Formatting pattern to copy from `code_stage_prompt` (`prompt.rs:372-383`)** — the policy is
interpolated with a bare blank line between it and the protocol, and `COMPLETION_PROTOCOL` stays
last (pinned by `both_code_prompts_still_end_with_the_completion_protocol`, `prompt.rs:825`):

```rust
format!(
    "Run the GSD workflow command for this stage:\n\n    {command}\n\n\
    {CODE_STAGE_POLICY}\n\
    \n\
    {COMPLETION_PROTOCOL}"
)
```

**Analog for the D-08 omission negative control: `prompt.rs:794-822`.** This is the existing
control the new one mirrors — note it drives **`render_workflow_style` only**, which is exactly
why a claude-style twin is mandatory:

```rust
// prompt.rs:794-822
#[test]
fn code_policy_is_absent_from_prompts_that_must_not_carry_it() {
    let phase = PhaseId::new(45);
    let gaps_only = render_workflow_style(
        &StageIntent::Code { phase, fix: Some(FixType::GapsOnly) }, "/workflows",
    );
    let audit_fix = render_workflow_style(
        &StageIntent::Code { phase, fix: Some(FixType::AuditFix) }, "/workflows",
    );

    for prompt in [stage_prompt(Stage::Validate, phase), stage_prompt(Stage::Ship, phase), gaps_only, audit_fix] {
        assert!(
            !prompt.contains(CODE_STAGE_POLICY),
            "only full-execute Code prompts may carry the shared policy"
        );
    }
}
```

> **Criterion 1 requires this test pass UNMODIFIED.** The claude-style twin is a *new* `#[test]`
> beside it, built by swapping `render_workflow_style(…, "/workflows")` for
> `render_claude_style(&StageIntent::Code { phase, fix: Some(FixType::GapsOnly) })`.

**Analog for the D-07 presence assertion: `code_policy_is_identical_across_both_renderers`,
`prompt.rs:777-792`** — the existing two-renderer presence test, which the six-adapter version
generalizes:

```rust
// prompt.rs:777-792
#[test]
fn code_policy_is_identical_across_both_renderers() {
    let phase = PhaseId::new(45);
    let claude_prompt = stage_prompt(Stage::Code, phase);
    let workflow_prompt = render_workflow_style(&StageIntent::Code { phase, fix: None }, "/workflows");

    assert!(claude_prompt.contains(CODE_STAGE_POLICY), "the Claude/OpenCode Code prompt must deliver the shared policy");
    assert!(workflow_prompt.contains(CODE_STAGE_POLICY), "the Codex/Pi Code prompt must deliver the shared policy");
}
```

**Other prompt.rs tests that will need re-reading before edit** (they assert on the exact text this
phase changes and are the likeliest incidental breakage):
`code_policy_excludes_blocking_human_and_package_checkpoints` `:755-765` (asserts the lowercased
prompt contains both `blocking-human` **and** `package-verification` — D-02's split must keep both
substrings present), `code_stage_prompt_is_deterministic` `:768-775`,
`fix_prompts_select_the_right_command` `:913`,
`fix_prompts_carry_the_chain_flag_token_only_where_it_reaches_execute_phase` `:936`,
`checkpoint_auto_decide_prompt_is_deterministic` `:989`,
`checkpoint_auto_decide_prompt_terminates_with_completion_protocol` `:997`,
`checkpoint_auto_decide_prompt_states_no_operator_judgment_and_record_reasoning` `:1008`.

**The `checkpoint_auto_decide_prompt` text D-02 tightens (`prompt.rs:537-549`)** — the generic
prose is one phrase, and its doc comment makes byte-determinism a stated contract because the
audit event quotes it:

```rust
// prompt.rs:537-549
pub fn checkpoint_auto_decide_prompt(phase: PhaseId) -> String {
    format!(
        "This is phase {phase} of a headless DevFlow run. You previously \
        stopped at a human-blocking checkpoint, but no human operator is \
        available to answer it — this run is unattended, and none is \
        coming. DevFlow's policy is for you to resolve the checkpoint \
        yourself, using your own best judgment, and continue the work. You \
        MUST record your reasoning for the decision you made in your final \
        message, so the decision is auditable after the fact.\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}
```

---

### `crates/devflow-cli/src/pipeline_launch.rs` — `resume_launch_shape` (extraction, request-response)

**Analog: `resolve_launch_shape`, `pipeline_launch.rs:186-217`.** RESEARCH § B-0 names this and it
holds up — its doc comment states the extraction rationale the new function reuses verbatim:

```rust
// pipeline_launch.rs:186-217
/// Resolve a stage launch into `(program, argv, monitor arm)`.
///
/// Extracted from [`launch_stage_inner`] unchanged (31-04) so the shape a
/// launch resolves to is assertable without spawning a process. The body is the
/// pre-extraction `if/else if/else` verbatim; `stream_launch` is the caller's
/// already-computed [`stream_launch_enabled`] reading, threaded in
/// rather than recomputed so one predicate still governs the launch shape, the
/// canary gate, and the D-11 notice.
fn resolve_launch_shape(
    agent: AgentKind,
    driver: &dyn agents::AgentDriver,
    phase: PhaseId,
    prompt: String,
    roots: &[std::path::PathBuf],
    stream_launch: bool,
) -> (&'static str, Vec<String>, monitor::MonitorLaunch) {
    if stream_launch {
        let (program, args) = driver.build_command(phase, &prompt, roots);
        (program, args, monitor::MonitorLaunch::PipeOwning { prompt })
    } else if agent == AgentKind::Claude { … }
}
```

**Copy these four properties:** private `fn` in the same module (not `pub(crate)`); returns the
built shape rather than spawning; doc comment naming the plan that extracted it and *why it is
assertable without a process*; caller destructures at the call site (`:155-162`).

**The code being lifted — `relaunch_checkpoint_session`, `pipeline_launch.rs:1095-1133`:**

```rust
// pipeline_launch.rs:1095-1119
pub(crate) fn relaunch_checkpoint_session(state: &mut State, session_id: &str) -> Result<(), CliError> {
    state.checkpoint_resumes = state.checkpoint_resumes.saturating_add(1);
    let instruction = prompt::checkpoint_auto_decide_prompt(state.phase);   // :1100

    // D-07: recorded BEFORE the relaunch spawns, so a spawn failure still
    // leaves the decision on record …
    events::emit(&state.project_root, state.phase, "checkpoint_auto_decided",
        serde_json::json!({ …, "instruction": truncate_reason(&instruction), … }));  // :1102-1117

    let (program, args) = agents::ClaudeDriver::exec_resume_command(session_id, &instruction);  // :1119
    …
}
```

> ⚠️ **`instruction` is consumed twice** — by `events::emit` at `:1113` and by
> `exec_resume_command` at `:1119`. D-10 forbids moving the emission. Either return `instruction`
> as a third tuple element from `resume_launch_shape`, or keep `let instruction` in the caller and
> pass it in. Both are acceptable; the emission stays at `:1102-1117`.

**Existing tests over `relaunch_checkpoint_session` that must survive unmodified** (RESEARCH § B-4
flagged this as unresolved assumption **A5, Medium risk** — this is the enumeration it wanted):

| Test | Line | What it pins |
|---|---|---|
| `relaunch_checkpoint_session_emits_exactly_one_audit_event` | `:2754-2795` | exactly one `checkpoint_auto_decided`, with `session_id`/`stage`/`attempt` |
| `relaunch_checkpoint_session_increments_and_persists_counter` | `:2803` | saturating increment persisted to disk |
| `relaunch_checkpoint_session_does_not_change_stage` | `:2849` | stage unchanged across relaunch |

All three call `relaunch_checkpoint_session(&mut state, "sess-…")` through its public signature and
assert on emitted events / persisted state — **none reads the argv**. A pure extraction that leaves
the signature and the emission alone should not touch them. **Verify, do not assume; if one has to
change, say so out loud (D-05's rule).**

**Analog for the D-03 two-turn delivery test: `legacy_launch_flag_forces_the_single_document_path`,
`pipeline_launch.rs:3935-3972`.** House style to copy — the "Precondition … for this test to mean
anything" assertion, and comparing against the production constructor byte-for-byte rather than a
re-typed literal:

```rust
// pipeline_launch.rs:3935-3972
/// D-11: one flag forces the pre-31 shape back on, even for a stage the
/// rollout HAS reached — otherwise the escape hatch is unreachable exactly
/// where it is needed.
#[test]
fn legacy_launch_flag_forces_the_single_document_path() {
    let dir = tempfile::tempdir().unwrap();
    let state = legacy_state(dir.path(), PhaseId::new(130), true);

    // Precondition: without the opt-out this stage IS in the rollout, so
    // the assertion below is a real discrimination and not a stage that
    // was going to be legacy anyway.
    assert!(
        stream_launch_enabled(state.agent, state.stage, false),
        "Stage::Code must be in STREAM_JSON_STAGES for this test to mean anything"
    );

    let driver = agents::driver_for(state.agent);
    let (program, args, launch) = resolve_launch_shape(
        state.agent, driver.as_ref(), state.phase,
        "the stage prompt".to_string(), &[], false,
    );

    assert!(matches!(launch, monitor::MonitorLaunch::Legacy));
    assert_eq!(program, "claude");
    assert_eq!(
        (program, args),
        agents::ClaudeDriver::exec_command_single_document("the stage prompt"),
        "the forced path must be exec_command_single_document byte-for-byte, \
         not an approximation of it"
    );
}
```

Two sibling tests use the same shape: `pi_resolves_to_legacy_launch` `:3978-4009` and
`legacy_launch_is_off_by_default` `:4014-4043`.

**Turn-2 source of truth — `ClaudeDriver::exec_resume_command`, `agents/claude.rs:122-135`.**
The instruction is `argv[1]`, positionally:

```rust
// crates/devflow-core/src/agents/claude.rs:122-135
pub fn exec_resume_command(session_id: &str, instruction: &str) -> (&'static str, Vec<String>) {
    ("claude", vec![
        "-p".into(), instruction.to_string(),
        "--resume".into(), session_id.to_string(),
        "--output-format".into(), "json".into(),
        "--dangerously-skip-permissions".into(),
    ])
}
```

**Turn-1 source of truth — `agents/claude.rs:21-23` + `build_command`'s doc comment `:24-40`,**
which states plainly that the prompt is *deliberately absent from argv* and arrives on stdin:

```rust
// crates/devflow-core/src/agents/claude.rs:21-23
fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
    crate::prompt::render_claude_style(intent)
}
```
> "**The prompt is deliberately absent from the returned argv.** Under `--input-format stream-json`
> the CLI takes its initial user turn from stdin as a JSON document …" (`claude.rs:25-32`)

That is why the test must capture turn 1 from `MonitorLaunch::PipeOwning { prompt }`, not from argv.

---

### D-07's six-adapter presence test (test, transform)

**Analog A — the explicit-array loop idiom, `pipeline_launch.rs:4288-4302`:**

```rust
// pipeline_launch.rs:4296-4302
// The other adapters never stream.
for agent in [AgentKind::Codex, AgentKind::OpenCode, AgentKind::Pi] {
    assert!(
        !stream_launch_enabled(agent, Stage::Code, false),
        "{agent:?} must stay off the stream path"
    );
}
```

**Analog B — per-driver `render_prompt` assertions, `agents/mod.rs:218-242`:**

```rust
// crates/devflow-core/src/agents/mod.rs:218-242
#[test]
fn drivers_reproduce_legacy_adapter_behavior() {
    let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));
    assert_eq!(ClaudeDriver.render_prompt(&intent), crate::prompt::render_claude_style(&intent));
    assert_eq!(OpenCodeDriver.render_prompt(&intent), crate::prompt::render_claude_style(&intent));
}
```

Driver construction for a kind-driven loop (`agents/mod.rs:173-182`):

```rust
pub fn driver_for(kind: AgentKind) -> Box<dyn AgentDriver> {
    match kind {
        AgentKind::Claude => Box::new(ClaudeDriver),
        AgentKind::Codex => Box::new(CodexDriver),
        AgentKind::OpenCode => Box::new(OpenCodeDriver),
        AgentKind::Pi => Box::new(PiDriver),
        AgentKind::Antigravity => Box::new(AntigravityDriver),
        AgentKind::Hermes => Box::new(HermesDriver),
    }
}
```

> ⚠️ **There is no `AgentKind::ALL` / iterator in this workspace** (searched
> `crates/devflow-core/src`). Every existing multi-agent test hardcodes an array literal. So D-07's
> "all six" loop **will not fail when a seventh variant is added** — the array is a manual list.
> If the planner wants that property, the mechanism is an exhaustive `match` over `AgentKind`
> inside the test (compile error on a new variant), mirroring `driver_for`'s own shape. Worth
> stating explicitly in the plan rather than leaving a reader to assume the loop is exhaustive.

---

### insta snapshot tests (D-14 / D-15) — **NO ANALOG**

**There is zero `insta` usage anywhere in this workspace.** Verified: no match in `Cargo.toml`,
`crates/*/Cargo.toml`, or `Cargo.lock`. There is no `.snap` file and no `src/snapshots/` directory
in either crate. Nothing to copy. The planner must treat this as greenfield and follow
`47-RESEARCH.md § A-0/A-6` (which is experimentally grounded), not a repo precedent.

**Closest thing in-repo, and it is a *different* mechanism — `crates/devflow-cli/tests/help_snapshot.rs`:**

```rust
// crates/devflow-cli/tests/help_snapshot.rs:1-7, 26-43
//! `--help` snapshot guard (15a): the committed snapshot is the contract
//! between the CLI surface and the docs (OPERATIONS.md, README). If this
//! test fails, the CLI changed — update the docs, then regenerate:
//!
//! ```bash
//! cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt
//! ```
#[test]
fn help_output_matches_committed_snapshot() {
    let actual = normalized(&String::from_utf8_lossy(&output.stdout));
    let expected = normalized(include_str!("snapshots/devflow-help.txt"));
    assert_eq!(actual, expected, "\n`devflow --help` drifted from …\n\
        If the CLI change is intentional: update OPERATIONS.md …, then regenerate the snapshot:\n\
        cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt\n");
}
```

**Two things the planner should take from it, and one it should not:**

- **Take:** the golden file is *committed and reviewed* (`crates/devflow-cli/tests/snapshots/devflow-help.txt`),
  and the failure message carries the exact regeneration command. `insta` baselines should be
  documented the same way — RESEARCH § A-7 says `INSTA_UPDATE=always cargo test -p <crate> <testname>`,
  and § A-7's closing paragraph says to put that recipe in `DEV-SETUP-CHECKLIST.md`.
- **Take:** it normalizes trailing whitespace before comparing, to avoid cosmetic false diffs.
- **Do NOT take** the `assert_eq!(actual, include_str!(…))` mechanism itself for prompt text.
  RESEARCH § A-7 rejected it specifically: on a multi-hundred-byte prompt it produces an unreadable
  failure and tempts pasting actual→expected, which is the exact D-15 hazard. `help_snapshot.rs`
  gets away with it because `--help` is short and structured.
- **Note the location difference:** `help_snapshot.rs` is an *integration* test, so its golden lives
  at `tests/snapshots/`. D-15's suite is *unit* tests inside `prompt.rs` / `pipeline_launch.rs`, so
  insta writes to `crates/<crate>/src/snapshots/` (RESEARCH § A-6). Different directory; both must
  be tracked.

---

### `scripts/check.sh` — `run_test` hardening (config/script, batch)

**Analog: `run_clippy`, the sibling function in the same file (`scripts/check.sh:34-38`)** — the
established shape is a `# comment explaining WHY this flag is load-bearing`, then an
`echo "==> <the literal command>"`, then the command with no pipeline:

```bash
# scripts/check.sh:34-38
run_clippy() {
    # --all-targets so test and bench code is linted too; a lint that only
    # covers src/ misses the majority of this repo's unsafe blocks.
    echo "==> cargo clippy --workspace --all-targets -- -D warnings"
    cargo clippy --workspace --all-targets -- -D warnings
}
```

Current `run_test` (`scripts/check.sh:41-49`) to be replaced:

```bash
run_test() {
    # --no-fail-fast deliberately: without it cargo stops at the first failing
    # test BINARY, so a failure in devflow-core hides every failure in
    # devflow-cli. …
    echo "==> cargo test --workspace --no-fail-fast"
    cargo test --workspace --no-fail-fast
}
```

> ⚠️ **The `echo "==> …"` banner is part of the pattern and it is asserted on.** Keep the echo
> consistent with the command actually run; `ci_parity_guards.rs` parses this file's non-comment
> lines (see below). `set -euo pipefail` is at `:9` and must stay before every cargo invocation.
> The CI job that runs this is `.github/workflows/ci.yml:41-63`, ending `- run: scripts/check.sh test`
> at `:63`.

---

### `.gitignore` — adding `*.snap.new`

**Analog: `.gitignore:63-72`,** the graphify block — this file's convention is a commented block
explaining *what is deliberately not tracked and why*, not a bare pattern:

```
# Graphify knowledge graph — the portable payload (graph.json, manifest.json,
# .graphify_analysis.json) IS tracked on the personal workspace branch so the
# graph travels between machines. Everything below is deliberately not:
graphify-out/cache/
```

Write the snapshot entry the same way, and say explicitly that `*.snap` stays **tracked** while
`*.snap.new` does not.

**Test analog if the planner wants the ignore asserted: `crates/devflow-cli/tests/gitignore_coverage.rs`.**
It shells out to `git check-ignore` **one path per invocation**, with the reason recorded inline:

```rust
// crates/devflow-cli/tests/gitignore_coverage.rs:44-50
#[test]
fn gitignore_covers_devflow_runtime_state_paths() {
    // Checked one path per invocation rather than as a single argv:
    // `git check-ignore` exits 0 when ANY argument matches, so a batched
    // call would stay green while individual paths silently lost coverage —
    // precisely the regression this guard exists to catch.
```

That per-path discipline is the negative-control habit; a `*.snap.new` guard should also assert the
**opposite direction** (that a `.snap` path is *not* ignored), or it does not discriminate.

---

### `Cargo.toml` / crate manifests — the `insta` dev-dependency

**Analog for the workspace entry: `Cargo.toml:19-32`,** including the precedent that a
load-bearing feature choice gets a comment explaining what breaks without it:

```toml
# Cargo.toml:19-32
[workspace.dependencies]
devflow-core = { path = "crates/devflow-core", version = "2.12.0" }
serde = { version = "1", features = ["derive"] }
# `preserve_order` is load-bearing, not a preference: … without this feature that
# round trip re-sorts every top-level key alphabetically — turning a tracked,
# committed file into a full-file diff on every single stage launch (35.1-01).
serde_json = { version = "1", features = ["preserve_order"] }
```

**Analog for the per-crate dev-dependency:**

```toml
# crates/devflow-core/Cargo.toml:32-33
[dev-dependencies]
tempfile = "3"
```
```toml
# crates/devflow-cli/Cargo.toml:24-27
[dev-dependencies]
tempfile = "3"
# `test_support::git_command` for this crate's test targets (999.37).
devflow-core = { workspace = true, features = ["test-support"] }
```

Note the in-file idiom: a one-line comment naming *which* code needs the dev-dep and the plan/issue
that introduced it. `insta = { workspace = true }` should carry a comment naming D-14/D-15 and the
`env -u INSTA_FORCE_UPDATE` obligation in `check.sh`.

> ⚠️ **`crates/devflow-core/Cargo.toml` already has a `[features]` block (`:16-20`).** Do not put
> `insta` behind a feature; it is a plain dev-dependency (RESEARCH § A-5 confirms `cargo machete`
> is satisfied by a fully-qualified `insta::assert_snapshot!` path with no `use`).

---

### `docs/guides/unattended-mode.md` (docs)

Two edit sites, verbatim as they stand today:

```markdown
<!-- docs/guides/unattended-mode.md:29-31 — QUALIFY, do not invert -->
Checkpoints marked `blocking-human`, and `checkpoint:human-action` tasks, are
never auto-approved by any mode. A phase whose plans declare one is refused
before it starts — see the next section.
```

```markdown
<!-- docs/guides/unattended-mode.md:85-88 — FALSE, rewrite -->
### A refusal is final

There is no `--force-unattended` flag and no environment variable that turns a
refusal into a warning. Your options are to fix the condition or to run in
supervise mode.
```

**Pattern to copy from the surrounding prose (`:71-84`):** each condition is stated, then the
*recovery* is named on its own line ("The recovery for either is the same: …", "A refusal here
means replanning the phase without the human-only marker, or running in supervise mode …").
Keep that shape when describing the `GateAction::Advance` override — state the mechanism, then what
it costs the operator.

> ⚠️ The paragraph immediately after `:88` reads "**Both external review lanes objected to that, on
> record, and the decision stands.**" That sentence is anchored to the "refusal is final" claim
> D-13 is rewriting. Read `:88-110` before editing or the rewrite will orphan it.

---

### Planning-record edits (`ROADMAP.md`, `REQUIREMENTS.md`, `DEV-SETUP-CHECKLIST.md`, `47-VALIDATION.md`)

**`DEV-SETUP-CHECKLIST.md` — analog is §8, `:398-407`.** The convention is a `- [ ]` bullet with a
`**[GLOBAL]** / **[PROJECT]**` scope tag where applicable:

```markdown
## 8. Language/build tooling — [PROJECT], Rust-specific

- [ ] Cargo workspace: `devflow-core` (lib) + `devflow-cli` (bin), zero network-dependency
  policy (serde, clap, thiserror, tracing only).
- [ ] **[GLOBAL / PROJECT]** Rust AI harness tooling: `cargo-deny` (`deny.toml`), `cargo-machete`, …
```

> ⚠️ **The "zero network-dependency policy (serde, clap, thiserror, tracing only)" line is a claim
> this phase falsifies for dev-dependencies.** Adding `insta` means §8's own text needs updating,
> not just an appended bullet. RESEARCH § A-7 also wants the baseline-creation recipe
> (`INSTA_UPDATE=always cargo test -p <crate> <testname>`) recorded here — a future contributor is
> otherwise stuck, because `cargo-insta` is not installed and `check.sh` pins `INSTA_UPDATE=no`.
> Repo rule: **same commit as the dependency and CI change**, enforced advisorily by
> `scripts/hooks/post-commit`.

**`ROADMAP.md` / `REQUIREMENTS.md`:** no code analog. Use `/gsd:phase` per CLAUDE.md's "Prefer GSD
commands" rule rather than hand-editing, and note the known defect that `phase.add` files new
entries at the document's last `---`. Targets: ROADMAP § `### Phase 47` goal (D-06: name hermes and
antigravity), ROADMAP § `### Phase 49` Success Criteria (D-09 pointer only — **no other Phase 49
amendment; D-12 is rescinded**), REQUIREMENTS DECN-02 `:18-26` / DECN-03 `:28-34` and the DECN-03
note `:185-187`, plus the two 999.x backlog entries from D-13.

---

## Shared Patterns

### Negative controls are structural, not optional
**Sources:** `prompt.rs:794-822` · `pipeline_launch.rs:3944-3948` (precondition assert) ·
`gitignore_coverage.rs:44-49` · `plan_bashism_scanner.rs:24-26`
**Apply to:** every test this phase adds.

The house form is an inline comment naming what the test would fail to discriminate without the
control:

```rust
// pipeline_launch.rs:3944-3948
// Precondition: without the opt-out this stage IS in the rollout, so
// the assertion below is a real discrimination and not a stage that
// was going to be legacy anyway.
assert!(
    stream_launch_enabled(state.agent, state.stage, false),
    "Stage::Code must be in STREAM_JSON_STAGES for this test to mean anything"
);
```

```rust
// plan_bashism_scanner.rs:24-26
//! `legit-PLAN.md` and `no-bashism-PLAN.md` are the load-bearing negative controls.
//! Without them a scanner that rejected every input would satisfy every other test
//! in this file.
```

Concretely for this phase: D-07's `codex`/`pi` passing controls, D-08's claude-style omission twin,
D-03's `assert_ne!(turn1, turn2)`, and D-16's three-case drift/force-update/clean run.

### Asserting on a shell script's source
**Source:** `crates/devflow-cli/tests/ci_parity_guards.rs:94-135`
**Apply to:** any Rust guard the planner adds over `run_test`'s `env -u INSTA_FORCE_UPDATE` wiring.

```rust
// crates/devflow-cli/tests/ci_parity_guards.rs:97-127
#[test]
fn check_script_fails_fast_before_any_cargo_invocation() {
    let path = repo_root().join("scripts/check.sh");
    let script = read(&path);

    let lines: Vec<&str> = script.lines().map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))   // strips comment prose
        .collect();

    let set_idx = lines.iter().position(|l| l.starts_with("set -") && l.contains('e'))
        .expect("expected a `set -e`-family line in scripts/check.sh");
    assert!(lines[set_idx].contains("-euo pipefail"), "…, not a weaker form — …");

    if let Some(first_cargo_idx) = lines.iter().position(|l| l.contains("cargo ")) {
        assert!(set_idx < first_cargo_idx, "fail-fast (line {set_idx}) must precede …");
    }
}
```

Note the `.filter(|l| !l.starts_with('#'))` — the file-scoped version of CLAUDE.md's "a grep over
source counts comment prose". `repo_root()` is the shared helper idiom
(`PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()`), identical in
`ci_parity_guards.rs:37-41`, `gitignore_coverage.rs:16-21` and `plan_bashism_scanner.rs:32-36`.

### Standalone script + Rust suite = the real enforcement
**Sources:** `scripts/lint-plan-bashisms.sh` ← `scripts/hooks/pre-commit:107-125` ←
`crates/devflow-cli/tests/plan_bashism_scanner.rs`

The three-part shape: a standalone executable script holds the logic; a hook delegates to it and
**refuses rather than degrading to a pass** when the script is missing; a Rust suite *executes* the
script against fixtures and is the actual enforcement.

```bash
# scripts/hooks/pre-commit:107-125
bashism_scanner="scripts/lint-plan-bashisms.sh"
if [ ! -x "$bashism_scanner" ]; then
    bashism_scanner="$(dirname "$0")/../lint-plan-bashisms.sh"
fi
if [ ! -x "$bashism_scanner" ]; then
    echo "pre-commit: missing or non-executable '$bashism_scanner'." >&2
    echo "  … Refusing rather than degrading to a silent pass — a guard that skips is the failure class it exists to prevent." >&2
    exit 1
fi
"$bashism_scanner" --staged
```

```rust
// crates/devflow-cli/tests/plan_bashism_scanner.rs:9-16
//! These tests **execute the scanner**. That is a deliberate departure from
//! `pre_commit_branch_guard.rs`, which asserts on the hook's *source* … That idiom is
//! correct for statically-checkable properties and wrong here: review finding C-04
//! is entirely about behaviour that source inspection missed for the life of the hook.
```

> **Bearing on phase 47, stated plainly:** this phase does **not** add a standalone scanner script.
> `scripts/lint-phase-worktree.sh` (named in the orchestrator's guidance) **does not exist in this
> tree** — `scripts/` contains `assert-cpu-pin.sh`, `assert-image-parity.sh`,
> `check-in-container.sh`, `check.sh`, `cut-pr-branch.sh`, `cut-release.sh`, `deploy-docs.sh`,
> `drain-drill.sh`, `install.sh`, `lint-plan-bashisms.sh`, `scratch-dogfood-repo.sh`,
> `sync-main-to-develop.sh`, `sync-workspace.sh`, `unattended-drill.sh` and nothing else.
> So the applicable half of this pattern is the **source-asserting** half
> (`ci_parity_guards.rs`, above), not the script-executing half — and the scanner suite's own
> header explains exactly when each is correct. If the planner adds a `run_test` guard, cite
> `ci_parity_guards.rs` as the model and say why the behaviour-executing idiom was not used
> (executing `check.sh test` inside a test would recurse the whole suite).

### One definition site for a rule stated in two places
**Sources:** `prompt.rs:57-61` (doc comment binding `CODE_STAGE_POLICY` to
`checkpoint_auto_decide_prompt`) · Phase 46 D-03 precedent
**Apply to:** D-04's `GATE_RESOLUTION_RULE`.

The file already documents the coupling in prose; D-04 converts prose into an interpolated constant.
Keep the existing doc comment and extend it to name the new constant, so a future reader finds the
binding from either end.

### Doc-comment-as-rationale
**Source:** everywhere in this codebase — `prompt.rs:551-566` (`fix_prompt`'s 15-line rationale),
`pipeline_launch.rs:186-194`, `claude.rs:24-40`, `check.sh:41-46`, `.gitignore:63-69`.
**Apply to:** every symbol and every config line this phase adds.

The consistent shape is: what it does, *which plan/phase/review finding produced it*, and what
breaks if someone "cleans it up". `claude.rs:24-32` is the extreme case — "it is unused here on
purpose, not by oversight". New code should cite `47-CONTEXT.md` decision ids (D-04, D-05, D-16)
the way existing code cites `31-04`, `D-11`, `CR-01`, `WR-08`.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `insta` snapshot test bodies + `crates/*/src/snapshots/*.snap` | test + golden artifact | transform / file-I/O | **Zero `insta` usage in this workspace.** Not in `Cargo.toml`, either crate manifest, or `Cargo.lock`; no `.snap` file and no `src/snapshots/` dir exists. Use `47-RESEARCH.md § A-0`, `§ A-6`, `§ B-3` — which are experimentally verified — rather than a repo pattern. `help_snapshot.rs` is a *different* mechanism and RESEARCH § A-7 explicitly rejects it for this use. |
| `.planning/ROADMAP.md` / `REQUIREMENTS.md` amendments | planning record | n/a | No code analog; governed by CLAUDE.md's "Prefer GSD commands" rule, not by a file pattern. |

---

## Traps Carried Forward Into Plan Authoring

These are not patterns to copy; they are patterns to **avoid**, each already verified in
`47-RESEARCH.md` § C and each directly reachable by this phase's own text.

| Trap | Where verified | Applies because |
|---|---|---|
| A backtick inside `bash -c '…'` becomes command substitution → silent `0` count | RESEARCH § C-5 | The text under change is `` `blocking-human` `` (`prompt.rs:86-89`). Use `.` in place of each backtick in every grep. |
| `rg -c PAT \| rg '^0$'` is a constant-fail, not a zero-check | CLAUDE.md; RESEARCH § C-3 | Use `n=$(grep -c PAT \|\| true); echo "n=$n"; test "$n" -eq 0; echo "gate_exit=$?"`. |
| `${PIPESTATUS[0]}` under zsh expands to empty | CLAUDE.md; RESEARCH § C-0 | Wrap every `<automated>` block in `bash -c '…'`; **use double quotes inside** (a `'"'"'` escape makes the scanner fail closed — RESEARCH § C-4). |
| The pre-commit hook does **not** enforce "must use `bash -c`" | RESEARCH § C-2 | It flags three bashism families only; a dead gate passes clean. Author against § C-0, treat the hook as a backstop. |
| A grep over `prompt.rs` counts its doc comments | CLAUDE.md; `prompt.rs:57-61` | Strip comments before counting. |
| A test naming `GATE_RESOLUTION_RULE` before it exists is a compile error, not a red test | RESEARCH § B-2, Pitfall 4 | Stage the red run on string literals, strengthen to the constant after the fix. |
| `cargo test -p devflow --lib` verifies nothing | CLAUDE.md | `-p devflow --bin devflow` for `pipeline_launch.rs`; `-p devflow-core --lib` for `prompt.rs`. |

---

## Metadata

**Analog search scope:** `crates/devflow-core/src/{prompt.rs,agents/}`, `crates/devflow-cli/src/{pipeline_launch.rs,preflight.rs}`,
`crates/devflow-cli/tests/`, `scripts/`, `.github/workflows/`, `Cargo.toml` + both crate manifests,
`.gitignore`, `docs/guides/`, `.planning/user/`
**Files scanned:** 18 read (targeted ranges), plus directory listings of `scripts/` and `crates/devflow-cli/tests/`
**Tracked-source verification:** `git ls-files` returned all 18 cited analog paths; zero mirror paths
**Pattern extraction date:** 2026-09-10

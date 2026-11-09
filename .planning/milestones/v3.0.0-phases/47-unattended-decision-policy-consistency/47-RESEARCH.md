# Phase 47: Unattended Decision Policy Consistency - Research

**Researched:** 2026-09-10
**Domain:** Rust snapshot testing (`insta`), production-faithful prompt-delivery tests, plan verify-command shell safety
**Confidence:** HIGH (all three areas settled by experiment in this session, each with a negative control)

> **Scope.** This is a *scoped* research run, chosen by the operator over a full pass. Areas A, B
> and C below are the only ones investigated. Everything the revised `47-CONTEXT.md` marks as
> settled (adapter split 4/2, `fix_prompt` omission, the `AgentKind::Claude` resume gate,
> `GateAction::Advance` skipping preflight, the pre-spawn `checkpoint_auto_decided` emission, the
> two non-equivalent checkpoint predicates, `cargo test --check` being invalid) was **not**
> re-derived. Where I happened to read one of those files for another reason and the claim was
> confirmed in passing, I say so — but I did not go looking.

---

## Summary

All three open areas resolved, and **two of them changed the answer the plan should encode.**

**Area A (`insta`) — adopt it; the cost D-14 priced in is almost entirely absent.** `cargo deny`
does not traverse dev-dependencies at all in this repo's configuration (proven with a negative
control), `cargo machete` does not flag a macro-only dev-dependency, and only **three** crates are
net-new to the lockfile. But D-16's wiring **as written in CONTEXT.md is defeatable**:
`INSTA_FORCE_UPDATE=1` in the environment overrides `INSTA_UPDATE=no`, silently rewrites the
baseline, and exits 0. That is precisely this repo's dead-gate class, caught before it shipped. The
wiring needs `env -u INSTA_FORCE_UPDATE` as well.

**Area B (two-turn delivery test) — buildable, and there is a direct in-repo precedent for exactly
this shape.** `resolve_launch_shape` (`pipeline_launch.rs:187-217`) was extracted in 31-04 with the
stated purpose "so the shape a launch resolves to is assertable **without spawning a process**", and
returns `MonitorLaunch::PipeOwning { prompt }` carrying the byte-exact stdin turn. The resume side
needs one matching extraction to avoid being a proxy. Both artifacts are then real production
constructors, and both assertions go red on the pre-fix tree while still compiling.

**Area C (verify commands) — CLAUDE.md overstates what the hook enforces, and this phase will walk
into a trap neither the hook nor CLAUDE.md covers.** The hook does *not* refuse a `*PLAN.md` that
omits `bash -c`; it refuses one that uses a *listed bashism* outside a `bash -c '…'` region. The
`rg -c` dead gate still passes it cleanly. And backticks inside a `bash -c '…'` block become command
substitution — which matters here because this phase's plans will grep for prompt text containing
`` `blocking-human` ``.

**Primary recommendation:** Adopt `insta` at workspace level with the hardened `run_test` wiring
from A-1; build the D-03 test on `resolve_launch_shape` plus a new symmetric `resume_launch_shape`
extraction; author every `<automated>` block from the C-1 template.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Prompt text (policy, carve-out, resume instruction) | `devflow-core` / `prompt.rs` | — | `CODE_STAGE_POLICY` appears only in `prompt.rs` [VERIFIED: crates/devflow-core/src/prompt.rs:62-89] |
| Turn-1 delivery shape (stdin vs argv) | `devflow-cli` / `pipeline_launch.rs` | `devflow-core` / `monitor.rs` | `resolve_launch_shape` decides the arm [VERIFIED: pipeline_launch.rs:195-217]; `MonitorLaunch` defines it [VERIFIED: monitor.rs:267-282] |
| Turn-2 resume argv | `devflow-core` / `agents/claude.rs` | `devflow-cli` / `pipeline_launch.rs` | `exec_resume_command` is a pure builder [VERIFIED: claude.rs:122-135]; called at [VERIFIED: pipeline_launch.rs:1119] |
| Snapshot enforcement | `scripts/check.sh` `run_test` | `.github/workflows/ci.yml` | CI's required Test job runs `scripts/check.sh test` [VERIFIED: .github/workflows/ci.yml:63] |
| Plan verify-command safety | `scripts/lint-plan-bashisms.sh` | `scripts/hooks/pre-commit` | Hook delegates; scanner decides [VERIFIED: scripts/hooks/pre-commit:107-125] |

---

# AREA A — `insta` integration (D-14, D-15, D-16)

## A-0. Recommendation, first

**Adopt `insta`. Do not hand-roll golden strings.** D-14 wrote "or equivalent"; the equivalent is
worse here, for a reason specific to this phase (see A-7).

Concrete form:

```toml
# Cargo.toml  [workspace.dependencies]
insta = "1"
```
```toml
# crates/devflow-core/Cargo.toml  AND  crates/devflow-cli/Cargo.toml, [dev-dependencies]
insta = { workspace = true }
```

```bash
# scripts/check.sh — replace the body of run_test (currently :44-49)
run_test() {
    # INSTA_UPDATE=no keeps a snapshot mismatch failing even when a developer
    # (or a stale shell) exports INSTA_UPDATE=always, which SILENTLY rewrites
    # the baseline and exits 0. `env -u INSTA_FORCE_UPDATE` is equally
    # load-bearing: INSTA_FORCE_UPDATE=1 OVERRIDES INSTA_UPDATE=no and
    # re-blesses the baseline green. Both were verified by experiment; see
    # 47-RESEARCH.md § A-3. Dropping either reopens a green-over-unread guard.
    echo "==> cargo test --workspace --no-fail-fast (snapshots pinned)"
    env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast
}
```

Add `*.snap.new` to `.gitignore`. Do **not** ignore `*.snap` — those are the committed baseline.

## A-1. `INSTA_UPDATE=no` semantics — the question that mattered most

The scope brief flagged the **missing-baseline** case as the one that decides whether the guard is
dead on arrival. **It is not dead.** Experiment, scratch crate, `insta 1.48.0`, non-CI shell
(`env -u CI`), exit codes captured with `bash -c '…; echo "cargo_exit=$?"'` rather than through a
pipeline:

| Case | `INSTA_UPDATE` | cargo exit | `.snap` written? | Verdict |
|------|----------------|-----------|------------------|---------|
| Missing baseline | *(unset — default)* | **101** | no `.snap`; writes `.snap.new` | fails |
| Missing baseline | `no` | **101** | nothing written | fails |
| Mismatch | *(unset)* | **101** | writes `.snap.new` | fails |
| Mismatch | `no` | **101** | nothing written | fails |
| Match | `no` | **0** | — | passes (negative control: the guard is not a constant-fail) |

[VERIFIED: experiment, `/tmp/…/scratchpad/instalab`, insta 1.48.0, this session]

**The finding the planner must carry:** `INSTA_UPDATE=no` does **not** change pass/fail on a clean
machine. Its only delta there is suppressing `.snap.new` artifacts. If D-16's mandatory red run is
performed on a clean shell, it will go red *whether or not the env var is present* — so that red run
**does not establish that the env var is doing anything**. It is a proxy for the wiring.

## A-2. What the env var is actually for

```
INSTA_UPDATE=always exported, no override   → cargo exit 0, baseline SILENTLY REWRITTEN
INSTA_UPDATE=always exported, `INSTA_UPDATE=no` prefix → cargo exit 101, baseline intact
```
[VERIFIED: experiment, this session]

So the guard defends against a *polluted environment*, not against the default one. That is a real
threat (a developer who once ran `INSTA_UPDATE=always` in an exported shell, or a CI image that
inherits it), and it justifies D-16 — but the justification is different from the one CONTEXT.md
gives, and the negative control has to match it.

## A-3. ⚠️ D-16 as written is defeatable — `INSTA_FORCE_UPDATE`

| Env | cargo exit | baseline after | |
|-----|-----------|----------------|---|
| `INSTA_FORCE_UPDATE=1 INSTA_UPDATE=no` | **0** | **rewritten to the drifted value** | guard defeated |
| `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no` | **101** | unchanged | guard holds |
| `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`, snapshot matches | **0** | — | negative control: does not over-block |

[VERIFIED: experiment, this session — all three run back-to-back from a reset baseline, with the
baseline value printed before and after each run]

`INSTA_FORCE_UPDATE=1` takes precedence over `INSTA_UPDATE=no`, rewrites the `.snap`, and exits 0.
A run so configured is green over a check nobody read — the exact class as the `rg -c` dead gate and
the `${PIPESTATUS[0]}` vacuous assertion already in CLAUDE.md. **The plan must use the `env -u`
form, and the mandatory D-16 red run must include this case**, not just a plain drifted snapshot.

## A-4. `cargo deny` — the obligation D-14 priced in is not incurred

```
cargo deny --config <devflow deny.toml> list      # insta as a DEV-dependency
  MIT (1): instalab@0.1.0
```
Only the root crate. cargo-deny 0.20.2 with this repo's `deny.toml` **does not traverse
dev-dependencies**.

**Negative control** (without which the above is worthless — a broken invocation looks identical):
moving `insta` to `[dependencies]` and re-running the same command lists all 17 crates
(`Apache-2.0 (16)`, `MIT (16)`, …). The measurement discriminates.
[VERIFIED: experiment, this session]

And in the worst case — if a future `deny.toml` did include dev-deps — the tree still passes:
`cargo deny check licenses` with `insta` as a *regular* dependency returned **exit 0, `licenses ok`,
0 errors**. Every licence in the subtree is satisfied by the existing allow list
[VERIFIED: deny.toml:89-94 — `allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception",
"Unicode-3.0"]`]:

| Crate | Licence |
|-------|---------|
| `insta` 1.48.0 | `Apache-2.0` |
| `similar` 2.7.0 | `Apache-2.0` |
| `console` 0.16.6 | `MIT` |
| `once_cell`, `tempfile`, `libc`, `cfg-if`, `bitflags`, `fastrand`, `getrandom`, `errno`, `encode_unicode`, `windows-*` | `MIT OR Apache-2.0` |
| `rustix`, `linux-raw-sys` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` |
| `r-efi` 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` — the `OR` resolves to MIT; deny accepted it |

**No `deny.toml` entry is needed.**

## A-5. `cargo machete` — no suppression needed

`cargo machete` on the scratch crate with `insta` as a dev-dependency referenced **only** through
`insta::assert_snapshot!` (fully qualified, no `use`): *"didn't find any unused dependencies. Good
job!"*, exit 0. [VERIFIED: experiment, this session]

machete matches on the crate name appearing in source, and a fully-qualified macro path is such an
appearance. No `[package.metadata.cargo-machete] ignored = [...]` entry is required.

## A-6. Features, footprint, snapshot locations

`insta`'s default feature set is `default = [colors]` → `colors = [console]` → `console` + `libc`
[VERIFIED: `cargo info insta`, this session]. Everything else (`json`, `yaml`, `redactions`,
`filters`, `glob`, `serde`, `csv`, `toml`, `ron`, `pest`) is opt-in and **not needed** for
`assert_snapshot!` on plain strings.

Net-new crates in this workspace's lockfile:

| With defaults (recommended) | With `default-features = false` |
|---|---|
| `insta`, `console`, `similar` — **3 net-new** | `insta`, `similar` — **2 net-new** |

Every other transitive (`once_cell`, `tempfile`, `fastrand`, `getrandom`, `rustix`, `bitflags`,
`linux-raw-sys`, `libc`, `cfg-if`) is **already in `Cargo.lock`**
[VERIFIED: grep over `/var/home/denniyahh/Github/devflow/Cargo.lock`, this session].

**Recommendation: keep default features.** The delta is one crate (`console`), and it buys coloured
diff output — which is the whole point of D-14 ("surfaces as a review diff").
`default-features = false` was verified to still run `assert_snapshot!` green if the repo prefers it.

**Snapshot file locations** [VERIFIED: experiment, this session]:

| Test lives in | `.snap` lands at |
|---|---|
| `tests/snap.rs` (integration) | `tests/snapshots/snap__<test>.snap` |
| `src/lib.rs` `mod tests` (unit) | `src/snapshots/<crate>__tests__<test>.snap` |

D-15's suite is unit tests inside `prompt.rs` and `pipeline_launch.rs`, so expect:
- `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__<name>.snap`
- `crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__<name>.snap`

`.gitignore`: add `*.snap.new` (transient, written by the local default mode). There is currently
**no** `snap` entry in `.gitignore` [VERIFIED: grep, this session]. `*.snap` must stay tracked.

## A-7. Should it be `insta` at all? — recommendation with reasoning

**Yes, `insta`, not a hand-rolled golden string.** The reasoning is specific to this phase rather
than generic:

1. **The cost D-14 accepted is mostly not real.** No `deny.toml` entry, no machete suppression,
   3 net-new crates. The only genuine cost is the CI wiring and the review habit.
2. **A hand-rolled golden test fails at the thing D-14 wants.** `assert_eq!(rendered, EXPECTED)` on
   a multi-hundred-byte prompt produces an unreadable failure and, worse, tempts the fixer to paste
   the actual output back in — which is *exactly* "a wrong string becomes the golden value", the
   failure D-15 is written to avoid. `insta` prints a line diff and, under `INSTA_UPDATE=no`,
   refuses to write anything.
3. **The baseline-review property D-15 asks for is native.** A `.snap` file is a reviewable artifact
   in the PR diff. A `const EXPECTED: &str` embedded in a test file is a wall of `\` continuations
   in this codebase's style (see `CODE_STAGE_POLICY` at `prompt.rs:62-89`) and reviews badly.

**One caveat the planner must plan around.** With `INSTA_UPDATE=no` wired into `check.sh` and
`cargo-insta` not installed, there is **no ergonomic way to create the initial baseline**. The
working method verified this session is `INSTA_UPDATE=always cargo test -p <crate> <testname>`
(exit 0, writes the `.snap`), run once, deliberately, followed by reading the generated file before
committing it. Document that in `DEV-SETUP-CHECKLIST.md` alongside the dependency — it is the step
a future contributor will otherwise be stuck on.

## A-8. What I could not establish (Area A)

- **Whether adding `insta` changes anything in the *real* workspace's `cargo deny`/`machete` run.**
  I was constrained not to modify this repository, so every A-result comes from a scratch crate at
  `/tmp/…/instalab` with this repo's `deny.toml` copied in. The dev-dependency-exclusion finding is
  a property of cargo-deny + this config and should transfer, but the confirming run on the real
  workspace has **not** been performed. The plan should make that run an explicit task, not an
  assumption.
- **Whether the CI container exports `INSTA_UPDATE` or `INSTA_FORCE_UPDATE`.** Not checked. The
  `env -u` form in A-0 makes it not matter, which is why I recommend it rather than investigating.
- **`INSTA_UNREFERENCED=reject`** (catches `.snap` files orphaned by a deleted test) exists in
  insta's env surface but I did **not** test it. Flagging as a candidate, not a recommendation.

---

# AREA B — The two-turn delivery test (D-03)

## B-0. Recommendation, first

Build the test on the **existing extraction precedent**, and add one symmetric extraction so the
resume side is production code rather than a re-typed pair of lines.

**Step 1 — add `resume_launch_shape`, mirroring `resolve_launch_shape`.** In `pipeline_launch.rs`,
lift lines 1100 and 1119 out of `relaunch_checkpoint_session`:

```rust
/// Resolve a checkpoint resume into `(program, argv)`.
///
/// Extracted from [`relaunch_checkpoint_session`] unchanged, for the same
/// reason 31-04 extracted [`resolve_launch_shape`]: the instruction a resume
/// actually DELIVERS is then assertable without spawning a process.
fn resume_launch_shape(phase: PhaseId, session_id: &str) -> (&'static str, Vec<String>) {
    let instruction = prompt::checkpoint_auto_decide_prompt(phase);
    agents::ClaudeDriver::exec_resume_command(session_id, &instruction)
}
```
`relaunch_checkpoint_session` then calls it. The `checkpoint_auto_decided` emission at `:1106-1117`
stays where it is — it needs `instruction` too, so either return it as a third element or keep the
`let instruction` in the caller and pass it in. **Either is fine; do not move the emission**, D-10
depends on it staying before the spawn.

**Step 2 — the test.** Same file, same `mod tests`, mirroring `legacy_launch_flag_forces_the_single_document_path`
(`pipeline_launch.rs:3935-3972`) including its explicit precondition comment style:

```rust
/// D-03: the shared gate rule must hold in BOTH artifacts a resumed Claude
/// session receives — the stdin user turn of the Code loop-back (turn 1) and
/// the positional resume instruction (turn 2). Deliberately NOT a
/// concatenation: no production code ever joins these, they co-reside across
/// two conversation turns. Each is captured from the production constructor
/// that actually delivers it.
#[test]
fn the_gate_rule_holds_in_both_delivered_turns() {
    let phase = PhaseId::new(47);
    let agent = AgentKind::Claude;
    let driver = agents::driver_for(agent);

    // Precondition: Code must be in the stream-json rollout, or turn 1 would
    // resolve to Legacy and there would be no stdin turn to capture at all —
    // the assertion below would then be vacuous rather than false.
    assert!(
        stream_launch_enabled(agent, Stage::Code, false),
        "Stage::Code must be stream-launch-enabled for this test to mean anything"
    );

    // --- TURN 1: exactly pipeline_gate.rs:198-203, then the launch shape.
    let loop_back_prompt = driver.render_prompt(&prompt::StageIntent::Code {
        phase,
        fix: Some(prompt::FixType::FullExecute),
    });
    let (_p1, _a1, launch) =
        resolve_launch_shape(agent, driver.as_ref(), phase, loop_back_prompt, &[], true);
    let turn1 = match launch {
        monitor::MonitorLaunch::PipeOwning { prompt } => prompt,
        monitor::MonitorLaunch::Legacy => panic!("Code loop-back must deliver a stdin turn"),
    };

    // --- TURN 2: the real resume builder.
    let (_p2, argv) = resume_launch_shape(phase, "session-under-test");
    assert_eq!(argv[0], "-p", "resume argv shape changed; turn 2 is not argv[1]");
    let turn2 = argv[1].clone();   // index, do not .get() — an absent element must panic

    // --- Negative control: two DISTINCT artifacts, not one string twice and
    // not one containing the other. Without this the pair assertion below
    // would still pass if both captures collapsed to the same source.
    assert_ne!(turn1, turn2);
    assert!(!turn1.contains(&turn2) && !turn2.contains(&turn1));

    // --- The shared rule, asserted in each delivered artifact independently.
    assert!(turn1.contains(GATE_RESOLUTION_RULE), "turn 1 lost the gate rule");
    assert!(turn2.contains("blocking-human"), "turn 2 does not name the gate class it governs");
}
```

## B-1. Why this is not a proxy

D-03 rejects "a test that manually joins two strings". This test never joins anything. Each side is
the output of the constructor production uses:

| Turn | Production path | Test path |
|---|---|---|
| 1 | `loop_back_to_code` returns `driver.render_prompt(&StageIntent::Code { phase, fix: Some(fix) })` [VERIFIED: crates/devflow-cli/src/pipeline_gate.rs:198-203] → `launch_stage(state, Some(prompt), …)` → `resolve_launch_shape(…, stream_launch)` [VERIFIED: pipeline_launch.rs:155-162] → `MonitorLaunch::PipeOwning { prompt }` | identical, minus the `State`/spawn |
| 2 | `relaunch_checkpoint_session`: `prompt::checkpoint_auto_decide_prompt(state.phase)` [VERIFIED: pipeline_launch.rs:1100] then `ClaudeDriver::exec_resume_command(session_id, &instruction)` [VERIFIED: pipeline_launch.rs:1119] | the same two calls, moved into `resume_launch_shape` so the test drives production code |

`MonitorLaunch::PipeOwning { prompt }` is documented as the string "delivered … as a JSON user turn
on the child's stdin" [VERIFIED: crates/devflow-core/src/monitor.rs:268-275], and
`exec_resume_command`'s argv carries the instruction **positionally** [VERIFIED:
crates/devflow-core/src/agents/claude.rs:122-135]. So `turn1` and `turn2` are the two delivered
artifacts, captured separately, asserted separately.

**The precedent is explicit and in-repo.** `resolve_launch_shape`'s own doc comment says it was
"Extracted from `launch_stage_inner` unchanged (31-04) **so the shape a launch resolves to is
assertable without spawning a process**" [VERIFIED: crates/devflow-cli/src/pipeline_launch.rs:187-194].
Three existing tests already drive it this way [VERIFIED: pipeline_launch.rs:3935-3972, 3978-4009,
4014-4043], two of which open with a "Precondition: … for this test to mean anything" assertion —
the house style the snippet above copies.

## B-2. Going red on the pre-fix tree — the sequencing that matters

D-03 requires both tests red pre-fix. There is a trap: **a test referencing `GATE_RESOLUTION_RULE`
does not compile before that constant exists, and a compile error is not a `test result: FAILED`.**
CLAUDE.md's own entry — *"a revert that hangs is not a revert that fails … require the failing
direction to print a failure"* — applies directly.

Pre-fix facts that make a *runnable* red possible:

- `fix_prompt` interpolates only `{command}` and `{COMPLETION_PROTOCOL}` — no `CODE_STAGE_POLICY` on
  any of the three arms [VERIFIED: crates/devflow-core/src/prompt.rs:567-580].
- `checkpoint_auto_decide_prompt` says *"You previously stopped at a **human-blocking checkpoint**"*
  — the literal `blocking-human` does not appear [VERIFIED: crates/devflow-core/src/prompt.rs:537-549].

So sequence the plan as:

1. **Red run.** Land the test with *string-literal* assertions:
   `turn1.contains("## Unattended decision checkpoints")` and `turn2.contains("blocking-human")`.
   Both compile against the pre-fix tree and both **fail**, printing `test result: FAILED`.
   Capture that output — it is the D-03 evidence.
2. Land the fix: hoist `GATE_RESOLUTION_RULE` (D-04), interpolate it into `CODE_STAGE_POLICY`, route
   `fix_prompt`'s `FullExecute` arm through the D-05 shared helper, tighten
   `checkpoint_auto_decide_prompt`.
3. Strengthen the turn-1 assertion to `contains(GATE_RESOLUTION_RULE)`. Green.
4. **Second negative control.** Revert *only* the `fix_prompt` edit, re-run, confirm a real
   `test result: FAILED`, restore. This proves the strengthened assertion still discriminates —
   step 1's red was against a different assertion text.

## B-3. Interaction with D-15's snapshot of the pair

D-15 wants the two-turn pair in the snapshot suite. Snapshot the **two artifacts separately**, not a
joined string — joining them in a snapshot would reintroduce exactly the composition D-03 rejects:

```rust
insta::assert_snapshot!("resume_pair_turn1_stdin", turn1);
insta::assert_snapshot!("resume_pair_turn2_argv",  turn2);
```

Named snapshots keep the two `.snap` files independently reviewable, which is the D-15 property.

## B-4. What I could not establish (Area B)

- **Whether `relaunch_checkpoint_session` can be refactored without touching its existing tests.**
  D-05 imposes a behaviour-preserving constraint on `workflow_code_prompt`; the same discipline
  should apply to `resume_launch_shape`, but I did not enumerate the existing tests over
  `relaunch_checkpoint_session` to confirm none would need editing. **The plan must verify this, and
  say so out loud if a test has to change** — same rule D-05 sets for the other extraction.
- **Whether `StageIntent::Code`/`FixType` are re-exported where the test needs them.** I read the
  definitions in `prompt.rs` but did not confirm the exact import path available inside
  `pipeline_launch.rs`'s `mod tests`. Trivial to resolve at authoring time; flagged so the planner
  does not treat the snippet's paths as verified.
- **The `--gaps-only` arm's delivery.** D-08 requires asserting `GapsOnly`/`AuditFix` still omit the
  policy. I confirmed the *source* omits it [VERIFIED: prompt.rs:567-580] but did not design that
  control's delivery-level shape; a constant-level assertion on `fix_prompt` directly is almost
  certainly sufficient and cheaper, matching the existing workflow-style control at `prompt.rs:793-822`.

---

# AREA C — Verify-command form for `<automated>` blocks

## C-0. Recommendation, first — the template

```
<automated>bash -c 'cd "$(git rev-parse --show-toplevel)"; cargo test -p devflow-core --lib prompt:: 2>&1 | tee /tmp/47-01-t1.log; echo "cargo_exit=${PIPESTATUS[0]}"; grep "test result:" /tmp/47-01-t1.log; f=$(grep -c "test result: FAILED" /tmp/47-01-t1.log || true); echo "failed_binaries=$f"; test "$f" -eq 0; echo "suite_green_exit=$?"'</automated>
```

Five rules, each earned by a verified failure mode:

1. **Wrap in `bash -c '…'`.** The executor's Bash tool runs zsh; `${PIPESTATUS[0]}` expands to empty
   there. Enforced by the scanner (C-2).
2. **Never `rg -c <pat> | rg '^0$'`.** Constant-fail. Use `n=$(grep -c … || true); echo "n=$n"; test "$n" -eq 0; echo "gate_exit=$?"` — print the count *and* the gate's own exit on separate lines (C-3).
3. **Only double quotes inside the `bash -c '…'` region.** A `'"'"'` escape makes the scanner fail
   closed and refuse the plan (C-4).
4. **Never put a backtick inside the block.** Substitute `.` in the regex (C-5). **This phase is
   unusually exposed to this** — its assertions target `` `blocking-human` `` and
   `` `decision` ``.
5. **Assert `1 passed` with a non-zero `filtered out`**, not a bare exit code, when using `--exact`
   (existing CLAUDE.md rule; 46-04's `grep -E "test result: ok\. 1 passed"` + `grep -oE "[0-9]+ filtered out"` is the shipped form).

## C-1. Phase-47-specific templates

**Red run for the fix_prompt gap (goes red pre-fix, prints a real failure):**
```
<automated>bash -c 'cd "$(git rev-parse --show-toplevel)"; cargo test -p devflow-core --lib the_gate_rule_holds 2>&1 | tee /tmp/47-red.log; echo "cargo_exit=${PIPESTATUS[0]}"; grep "test result:" /tmp/47-red.log; n=$(grep -c "test result: FAILED" /tmp/47-red.log || true); echo "failed_binaries=$n"; test "$n" -ge 1; echo "expected_red_exit=$?"'</automated>
```

**Grepping the policy text (backtick-safe — note the `.` where a backtick would be):**
```
<automated>bash -c 'cd "$(git rev-parse --show-toplevel)"; f=crates/devflow-core/src/prompt.rs; n=$(grep -c "does not extend to a .blocking-human. gate" "$f" || true); echo "unconditional_prohibition_sites=$n"; test "$n" -eq 0; echo "prohibition_removed_exit=$?"'</automated>
```

**The D-16 snapshot negative control (must cover the force-update case, per A-3):**
```
<automated>bash -c 'cd "$(git rev-parse --show-toplevel)"; s=crates/devflow-core/src/snapshots; cp "$s"/*prompt*policy*.snap /tmp/47-snap.bak; printf "DRIFT\n" >> "$s"/$(ls "$s" | head -1); scripts/check.sh test > /tmp/47-drift.log 2>&1; echo "drifted_check_exit=$?"; cp /tmp/47-snap.bak "$s"/$(ls "$s" | head -1); INSTA_FORCE_UPDATE=1 scripts/check.sh test > /tmp/47-force.log 2>&1; echo "force_update_exit=$?"; scripts/check.sh test > /tmp/47-clean.log 2>&1; echo "clean_check_exit=$?"'</automated>
```
Expected: `drifted_check_exit` non-zero, `force_update_exit` **non-zero** (this is the A-3 hole — it
will be **0** if `env -u INSTA_FORCE_UPDATE` was omitted from `run_test`), `clean_check_exit=0`.

## C-2. What `scripts/hooks/pre-commit` actually enforces — CLAUDE.md is imprecise

CLAUDE.md states: *"`scripts/hooks/pre-commit` now refuses a staged `*PLAN.md` that does not [use
`bash -c`]."* **That is not what it does.**

The hook delegates to `scripts/lint-plan-bashisms.sh --staged` and refuses only on its exit
[VERIFIED: scripts/hooks/pre-commit:107-125]. The scanner flags a **fixed list of bash-only
constructs** and only when they are **not contained** in a single-quoted region opened by `bash -c`.
The matcher [VERIFIED: scripts/lint-plan-bashisms.sh:226]:

```awk
/\$\{[A-Za-z_][A-Za-z0-9_]*\[[^]]*\]\}|\$\{[A-Za-z_][A-Za-z0-9_]*(,,|\^\^)\}|declare -A/
```

i.e. **any `${NAME[…]}` index**, **`${v,,}` / `${v^^}`**, and **`declare -A`**. Nothing else.

Verified by running the scanner on four constructed plans [VERIFIED: experiment, this session]:

| Block | scanner exit |
|---|---|
| `bash -c 'cargo test \| tail -3; echo "x=${PIPESTATUS[0]}"'` | **0** (accepted — the legitimate form) |
| `cargo test \| tail -3; echo "x=${PIPESTATUS[0]}"` | **1** (refused) |
| `bash -c "echo ${PIPESTATUS[0]}"` (double quotes) | **1** (refused) |
| `cargo test --workspace 2>&1 \| rg -c 'FAILED' \| rg '^0$'` | **0 — ACCEPTED** |

**The last row is the finding.** A plan with **no** `bash -c` at all and a **dead gate** passes the
hook clean. The hook is not a general "must use bash -c" rule, and it does not cover the `rg -c`
class at all. Author against C-0's rules, not against the hook.

## C-3. The `rg -c` dead gate — re-verified, both directions

```
green.log ("test result: ok")     → rg -c 'FAILED' | rg '^0$'  exits 1
red.log   ("test result: FAILED") → rg -c 'FAILED' | rg '^0$'  exits 1
```
[VERIFIED: experiment, this session]

Root cause confirmed directly: `rg -c` prints **nothing** and exits **1** on zero matches (`green rg
-c exit=1`, no output), so the downstream `rg '^0$'` never receives a line. CLAUDE.md's claim is
exactly right. The form is a constant-fail that never discriminated.

CLAUDE.md notes it survives in `44-01`..`44-04` and `43-02`. I did **not** re-scan for surviving
instances — out of scope, and CLAUDE.md already records it.

## C-4. The quoting case that fails closed

A `'"'"'` embedded-quote escape inside an `<automated>` block makes the scanner **refuse the plan**
with `[quoting cannot be decided - restructure the command]` [VERIFIED: experiment, this session;
mechanism at scripts/lint-plan-bashisms.sh:182-185]. This is deliberate — a conservative false
positive costs one edit, a false negative is the whole finding. **Use double quotes inside the
`bash -c '…'` region.** Every 46-04-onward block in this repo does.

## C-5. ⚠️ Backticks inside `bash -c '…'` — a silent zero, and this phase is exposed

```
bash -c 'n=$(grep -cF "This authority does not extend to a `blocking-human` gate" …); echo "…=$n"'
  → bash: line 1: blocking-human: command not found
  → backtick_inside_singlequote_count=0
```
vs. the safe form:
```
bash -c 'n=$(grep -c "does not extend to a .blocking-human. gate" …); echo "…=$n"'
  → safe_regex_dot_count=1
```
[VERIFIED: experiment against `crates/devflow-core/src/prompt.rs`, this session]

The outer single quotes protect the backticks from the *executor's* shell, but `bash -c` then
re-parses the string and the backticks sit inside the inner **double** quotes, where they are
command substitution. The result is a count of **0** that looks like a clean pass.

This is not hypothetical for phase 47: the text under change is
``This authority does not extend to a `blocking-human` gate or a package-verification checkpoint``
[VERIFIED: crates/devflow-core/src/prompt.rs:86-89, quoted verbatim], and D-02's whole point is
tightening prose around `` `blocking-human` ``. **Every grep in this phase's plans will hit this
unless the `.` substitution is used.** Neither CLAUDE.md nor the scanner covers it.

## C-6. Prior-phase state, for the planner's context

Phase 46's plans are **split**: `46-01`, `46-02`, `46-03` carry bare `${PIPESTATUS[0]}` outside any
`bash -c` (they predate the scanner, which 46-05 introduced); `46-04` through `46-09` use the
`bash -c 'cd "$(git rev-parse --show-toplevel)"; …'` form throughout
[VERIFIED: grep over the phase-46 plan files, this session].

**If `gsd-tools` harvested prior verify commands and offered `46-01`-style ones, they are the stale
half.** Harvest from `46-04`+ only. `46-07:266` is the closest existing model for a full-suite gate.

## C-7. What I could not establish (Area C)

- **Whether the executor's Bash tool is zsh in *this* project's current harness.** I did not
  re-verify it; the claim comes from CLAUDE.md and the scanner's own header. My own Bash tool in
  this session behaves as documented for `${PIPESTATUS[0]}` (I hit it twice — see the two corrected
  measurements in A-1), which is consistent but is not the same shell as an executor subagent's.
- **Whether `crates/devflow-cli/tests/plan_bashism_scanner.rs` covers the backtick case.** I read
  the scanner and the hook but not that suite. Given the scanner's regex (C-2) contains no backtick
  clause, it almost certainly does not — but I did not confirm, so this is **unverified**.

---

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `insta` 1.48.0 | crates.io | est. 8 yrs (1.x since 2020) | very high | github.com/mitsuhiko/insta | OK | Approved |
| `similar` 2.7.0 | crates.io | est. 5 yrs | high | github.com/mitsuhiko/similar | OK | Approved (transitive) |
| `console` 0.16.6 | crates.io | est. 8 yrs | very high | github.com/console-rs/console | OK | Approved (transitive) |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

**Provenance note.** `insta` was named by the operator in D-14, not discovered by me via search.
It was resolved on the **correct ecosystem registry** (`cargo search insta` → `insta = "1.48.0"`;
`cargo info insta` → `version: 1.48.0`) and **built, fetched and executed** in a scratch crate this
session — registry existence is not the only evidence here, the code ran. The `package-legitimacy`
seam was not invoked (it covers npm/pypi/crates verdict shapes; the crates lane was satisfied
directly). Age and download figures above are `[ASSUMED]` — I did not query crates.io metadata for
them, and they are not load-bearing for any decision in this document.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Detecting prompt-text drift | `assert_eq!(rendered, CONST_EXPECTED)` | `insta::assert_snapshot!` | Unreadable failures; invites pasting actual→expected, which is how a wrong string becomes golden (the exact D-15 hazard) |
| Capturing "what was delivered" | A test that concatenates turn 1 + turn 2 | `resolve_launch_shape` + a new `resume_launch_shape` | No production code concatenates them (D-03); a join is a proxy |
| A zero-match gate in a verify block | `rg -c PAT \| rg '^0$'` | `n=$(grep -c PAT \|\| true); echo "n=$n"; test "$n" -eq 0; echo "gate_exit=$?"` | The `rg` form is a constant-fail — verified exit 1 in both directions |
| Pinning snapshots in CI | A bespoke "check snapshots" script | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no` in `run_test` | Already inside the required CI job (`ci.yml:63`) and the pre-push gate (`pre-push:217`) |

**Key insight:** every custom variant above fails *green*. That is the specific reason this repo
keeps paying for them.

---

## Common Pitfalls

### Pitfall 1: `INSTA_FORCE_UPDATE` silently re-blesses the baseline
**What goes wrong:** D-16's wiring exits 0 and rewrites the `.snap`.
**Why:** `INSTA_FORCE_UPDATE=1` takes precedence over `INSTA_UPDATE=no`.
**Avoid:** `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`.
**Warning sign:** a `check.sh test` run that passes while `git status` shows a modified `.snap`.

### Pitfall 2: The D-16 red run proves the wrong thing
**What goes wrong:** a drifted-snapshot red run goes red with or without the env var, so it
certifies insta, not the wiring.
**Avoid:** the three-case control in C-1 (drift / force-update / clean).

### Pitfall 3: A backtick in a verify block returns a silent 0
**What goes wrong:** `grep -c "… \`blocking-human\` …"` inside `bash -c '…'` runs command
substitution and counts 0.
**Avoid:** `.` in place of each backtick.

### Pitfall 4: A test referencing the not-yet-existing shared constant produces a compile error, not a red test
**Avoid:** stage the assertions (B-2) — literals for the red run, constant afterwards.

### Pitfall 5: Assuming the pre-commit hook will catch a bad verify command
**What goes wrong:** it catches three bashism families and nothing else; a dead gate passes clean.
**Avoid:** author against C-0, treat the hook as a backstop.

---

## Project Constraints (from CLAUDE.md)

| Directive | Bearing on this phase |
|---|---|
| Never run git ops while an executor holds the working tree | `use_worktrees: false` [VERIFIED: .planning/config.json], so dispatch is sequential — every executor prompt must carry the absolute worktree root and a branch assertion |
| `cargo test -p devflow --lib` is invalid | `devflow` is binary-only → `-p devflow --bin devflow`. `-p devflow-core --lib` **is** valid, and is what the `prompt.rs` snapshot tests need |
| `cargo test --exact <name>` exits 0 on no match | Assert `1 passed` + non-zero `filtered out` |
| A grep over source counts comment prose | `CODE_STAGE_POLICY`'s doc comment at `prompt.rs:57-61` mentions `checkpoint_auto_decide_prompt` — strip comments (`grep -v "^[[:space:]]*//"`) before counting, as 46-04/46-06/46-07 do |
| `${PIPESTATUS[0]}` under zsh; `bash -c` required | See Area C |
| Keep `DEV-SETUP-CHECKLIST.md` in sync in the same commit | D-16 already carries this; A-7's baseline-creation recipe belongs there too |
| Prefer GSD commands over hand-editing `.planning/` | Applies to the D-13 backlog filings |

---

## Validation Architecture

`workflow.nyquist_validation: true` [VERIFIED: .planning/config.json].

### Test Framework
| Property | Value |
|----------|-------|
| Framework | built-in `cargo test` + `insta 1.48.0` (to be added) |
| Config file | none for cargo; `scripts/check.sh` is the single definition of green [VERIFIED: scripts/check.sh:1-9] |
| Quick run command | `cargo test -p devflow-core --lib prompt::` |
| Full suite command | `scripts/check.sh test` → `cargo test --workspace --no-fail-fast` [VERIFIED: scripts/check.sh:44-49] |

### Phase Requirements → Test Map
| Req | Behavior | Type | Command | Exists? |
|-----|----------|------|---------|---------|
| DECN-02 | policy present on `FullExecute` fix arm, all 6 adapters (D-07) | unit | `cargo test -p devflow-core --lib fix_prompt` | ❌ Wave 0 |
| DECN-02 | `GapsOnly`/`AuditFix` still omit, claude-style (D-08) | unit | same | ❌ Wave 0 |
| DECN-02 | `workflow_code_prompt` tests pass **unmodified** (D-05) | unit | `cargo test -p devflow-core --lib` | ✅ `prompt.rs:793-822` |
| DECN-03 | unconditional prohibition gone / conditional present | unit | `cargo test -p devflow-core --lib` | ❌ Wave 0 |
| DECN-03 | two-turn delivery pair (D-03) | unit | `cargo test -p devflow --bin devflow the_gate_rule_holds` | ❌ Wave 0 |
| D-16 | snapshot mismatch fails the build (3-case control) | integration | C-1 block | ❌ Wave 0 |

### Sampling Rate
- Per task commit: the scoped `cargo test -p <crate>` for the touched crate
- Per wave merge: `scripts/check.sh test`
- Phase gate: `scripts/check.sh all` green (also what pre-push runs [VERIFIED: scripts/hooks/pre-push:217])

### Wave 0 Gaps
- [ ] `insta` dev-dependency in both crates + `[workspace.dependencies]` entry
- [ ] `run_test` hardening (A-0)
- [ ] `*.snap.new` in `.gitignore`
- [ ] Initial `.snap` baselines, created via `INSTA_UPDATE=always` and **read before committing**
- [ ] `resume_launch_shape` extraction (B-0 step 1)

---

## Security Domain

`security_enforcement` absent from `.planning/config.json` → treated as enabled.

| ASVS Category | Applies | Control |
|---|---|---|
| V2 Authentication | no | — |
| V3 Session Management | no | Claude session ids are opaque and already handled |
| V4 Access Control | **yes, narrowly** | This phase *widens* an agent's authority to self-resolve a `blocking-human` gate. D-02 keeps it two-way reversible and scoped to one gate class; package-verification keeps the unconditional prohibition |
| V5 Input Validation | no | No new external input |
| V6 Cryptography | no | — |

| Pattern | STRIDE | Mitigation |
|---|---|---|
| Authority carve-out over-widens to package verification | Elevation of Privilege | D-02's narrow scope; D-08's negative control |
| Snapshot guard silently disabled by env | Tampering / Repudiation | A-3's `env -u INSTA_FORCE_UPDATE` |
| Supply chain via new dev-dependency | Tampering | 3 net-new crates, all MIT/Apache-2.0, all from established maintainers; A-4 |

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Adding `insta` to the **real** workspace leaves `cargo deny`/`machete` green | A-4, A-5, A-8 | Low — proven in a scratch crate with this repo's `deny.toml`; a confirming run is a plan task |
| A2 | `insta`/`similar`/`console` age and download figures | Package audit | None — not load-bearing |
| A3 | The executor subagent's Bash is zsh **today** | C-7 | Low — CLAUDE.md and the scanner both assert it; `bash -c` is correct either way |
| A4 | `plan_bashism_scanner.rs` does not cover the backtick case | C-7 | Low — worst case the plan adds a redundant rule |
| A5 | `relaunch_checkpoint_session`'s existing tests survive the B-0 extraction | B-4 | **Medium** — if false, D-05's "say so explicitly rather than quietly editing a test" rule applies |
| A6 | `INSTA_UNREFERENCED=reject` behaves as documented | A-8 | None — not recommended, only flagged |

---

## Open Questions

1. **Does `run_test` hardening belong to this phase or to the fix plan?**
   - Known: it is a 3-line edit inside the existing required CI job; D-14/D-16 already own it.
   - Unclear: CONTEXT.md leaves plan-splitting to Claude's discretion.
   - Recommendation: **its own small plan**, sequenced first. The A-3 control must be demonstrable
     before any snapshot baseline is blessed, or the baselines land behind an unproven guard.

2. **Should A-3's finding be filed to the 999.x backlog?**
   - It is a defect in a guard this phase is *about to write*, not in shipped code — so fixing it
     inline is correct and no entry is needed.
   - But CONTEXT.md's Claude's-Discretion item ("one backlog entry or two" for D-13's latent
     defects) is adjacent. **Not a decision I should make** — flagging for the planner.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| `cargo` / `rustc` | everything | ✓ | 1.97.1 / 1.97.1 | — |
| `cargo-deny` | A-4 | ✓ | 0.20.2 | — |
| `cargo-machete` | A-5 | ✓ | 0.9.2 | — |
| `cargo-insta` CLI | D-16 alternative form | ✗ | — | `INSTA_UPDATE` env form (recommended anyway; needs no binary) |
| crates.io network | fetching `insta` | ✓ | — | — |
| `rg` | verify blocks | ✓ | — | `grep -c … \|\| true` (preferred — see C-0 rule 2) |

**Missing with no fallback:** none.
**Missing with fallback:** `cargo-insta` — D-16 already prefers the env form for this reason
[VERIFIED: 47-CONTEXT.md D-16 "Provisioning"], and A-1/A-3 confirm the env form is sufficient.

---

## Sources

### Primary (HIGH confidence)
- Direct experiment in `/tmp/…/scratchpad/instalab` (insta 1.48.0) — every A-section result, each with a negative control
- Direct experiment in `/tmp/…/scratchpad/bashism` — scanner behaviour across 5 constructed plans
- Direct reads of this repository (all `file:line` citations above)

### Secondary (MEDIUM confidence)
- `cargo info insta` / `cargo search insta` — version and feature list
- `47-CONTEXT.md` (revised) — settled findings not re-derived, per scope

### Tertiary (LOW confidence)
- Package age/download estimates in the legitimacy audit — training knowledge, tagged `[ASSUMED]`

---

## Metadata

**Confidence breakdown:**
- Area A: **HIGH** — every claim from a run command with a negative control; the one gap (real-workspace confirmation) is named
- Area B: **HIGH** on the mechanism (precedent read in source, both delivery paths traced), **MEDIUM** on the snippet compiling as written (import paths unverified — A5/B-4)
- Area C: **HIGH** — scanner behaviour, dead gate, and backtick trap all reproduced both directions

**Research date:** 2026-09-10
**Valid until:** 2026-10-10 (stable — `insta` 1.x, and the repo-internal findings are structural)

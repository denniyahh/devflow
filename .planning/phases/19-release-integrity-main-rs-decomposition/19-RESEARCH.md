# Phase 19: Release Integrity + `main.rs` Decomposition - Research

**Researched:** 2026-07-21
**Domain:** Rust module decomposition (binary crate, zero-behavior pure move) + two small git/PII fixes + a GSD-process contract (no source)
**Confidence:** HIGH (all claims in this document are grounded in live source read at HEAD, 2026-07-21, on `develop`, unless explicitly tagged otherwise)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01: A single shared `ENV_MUTEX` must survive the split.** Hoist it into a
  shared test-support module that every split module imports.
- **D-02: Rejected — per-module mutexes.** `PATH` is mutated 36 times across 12
  separate lock regions spanning at least three target clusters.
- **D-03: Rejected — eliminating env mutation via dependency injection.**
  Behavioral change to 106 tests; deferred idea.
- **D-04: The invariant to document is not "one mutex."** It is *"every env var
  is guarded by exactly one mutex, and no var is touched under two."*
- **D-05: Flat sibling modules, all staying in `devflow-cli`.** `preflight.rs`,
  `staleness.rs`, `commands.rs`, `parallel.rs`, `config_parse.rs`, plus a thin
  `main.rs` retaining only `main`, `run`, and arg routing.
- **D-06: Split the pipeline state machine further.** ~1,040 lines, absorbed 3
  of Phase 18's 7 plans. Natural seams: `launch_stage`/`advance`/`transition`,
  the `handle_*_outcome` family, and `run_gate`/`finish_workflow`.
- **D-07: Rejected — `commands/` per-subcommand subdirectory.** Buys zero wave
  reduction against Phase 18's real workload.
- **D-08: Do NOT move `staleness`/`preflight` into `devflow-core` this phase.**
  Cheap follow-up once the split is proven.
- **D-09: Pure move, zero behavioral change.** No logic edits bundled in.
- **D-10: Split the test module in the same operation.** Tests move with their
  cluster's code; tests spanning clusters need explicit handling.
- **D-11: Verify on a branch with CI. Local-green is explicitly insufficient.**
- **D-12: A partially-completed split with the `ENV_MUTEX` question surfaced is
  an acceptable outcome.**
- **D-13: Apply both 19a fixes; either alone leaves half the problem open.**
- **D-14: WR-01 — `lock::ensure_devflow_dir` writes a `.devflow/.gitignore`
  containing `*` on creation.** (See Pitfall 3 below — this function does not
  exist at HEAD; treat the name as a design target, not a location.)
- **D-15: WR-02 — emit only `current_exe().file_name()`, not the full path.**
- **D-16: Drop `--allow-empty` from `commit_path`** and let the existing
  `nothing to commit` arm become the genuine no-op.
- **D-17: `commit_all` at `git.rs:312` is OUT of scope — but check and record
  why.**
- **D-18: Wire the 19g contract into `/gsd-code-review`'s actual criteria,
  plus prose in `CONTRIBUTING.md`.** Reject a net-new mechanical lint pass.
- **D-19: The five 19g requirements** — regression test that fails before the
  change; assertion at a public/stable boundary; evidence the test fails for
  the intended reason; full affected-package tests + clippy `-D warnings` +
  fmt; independent review of both implementation and test signal.
- **D-20: 19a and 19b land BEFORE the split.**
- **D-21: 19g has zero source overlap and may run in any wave.**

### Claude's Discretion

- Exact file names and the precise cut points for the pipeline sub-split.
- Where the shared test-support module lives and what it is called.
- Whether `config_parse` is large enough to warrant its own file or folds into
  the thin `main.rs`.

### Deferred Ideas (OUT OF SCOPE)

- Eliminate env mutation in tests via dependency injection (D-03).
- Move `staleness`/`preflight` into `devflow-core` (D-08).
- Enforce the one-var-one-mutex invariant mechanically (D-04 follow-up).
- Regenerate `.planning/codebase/STRUCTURE.md` and `TESTING.md` (mechanical
  follow-up owned by this phase per the dossier, but not a locked Task here —
  see Open Questions).
- Phase 20 (v2.0.0): `--until`, manual ship override, release-cut automation,
  likely `gate show`.

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| 19a | `.devflow/` artifact hygiene (WR-01 `.gitignore` + WR-02 `exe_path` redaction) | Pitfall 3 (no existing chokepoint — `ensure_devflow_dir` must be newly designed); confirmed `start()` call order proves `workflow::save_state` is the earliest universal `.devflow/` creator; confirmed `gitignore_coverage.rs`/`doc_check.rs:283` cover only this repo, not target repos (no interaction risk) |
| 19b | `commit_path` empty commits (drop `--allow-empty`) | Confirmed zero existing tests pin `commit_path`'s empty-commit behavior; confirmed both `hooks.rs` call sites (`changelog_append` AND `version_bump`) are affected, not just `version_bump` |
| 19c–19f | Split `main.rs` (pure move) | Cluster boundaries re-measured at HEAD (function-level, not stale line estimates); `ENV_MUTEX`/shared-test-support mechanics confirmed sound via Rust's compilation model; preflight↔pipeline circular call dependency found and characterized; 4 explicit cross-cluster tests identified by name; equivalence-proof procedure specified |
| 19g | AI change acceptance contract | Located the actual wiring point: DevFlow has **no** `.claude/skills/` or `.agents/skills/` directory today — the contract must be added as a **new project skill** (which `gsd-code-reviewer`'s own `<project_context>` protocol already auto-discovers), not an edit to the global agent definition |

</phase_requirements>

## Summary

This phase has three genuinely independent workstreams sharing one file. 19a and
19b are small, well-scoped git/core fixes with straightforward code paths —
but 19a's premise (a single existing `.devflow/` constructor to patch) does
not hold at HEAD: `lock::ensure_devflow_dir` does not exist, and at least
seven independent modules create `.devflow/` subpaths directly. 19b is safe:
no existing test pins `commit_path`'s empty-commit behavior, and the fix's
blast radius includes *both* `commit_path` call sites (`version_bump` and
`changelog_append`), not just the one CONTEXT.md names.

19c–19f (the split) is the real engineering content of this phase. Live-source
verification confirms the cluster boundaries CONTEXT.md flagged as stale, and
turns up two mechanically important findings the dossier didn't have: (1) the
production code has a genuine **bidirectional call dependency between the
preflight cluster and the pipeline cluster** (`run_preflight` calls
`launch_stage_inner` directly; `launch_stage` calls `run_preflight` directly) —
this is not a compile hazard (Rust's module graph, unlike its crate graph, is
allowed to be cyclic) but it is a real coupling fact the plan must account for;
and (2) the pipeline cluster's own D-06 sub-seams (`launch_stage`/`advance`,
`handle_*_outcome`, `transition`/`run_gate`/`finish_workflow`) also call each
other in a cycle (`transition()` calls `launch_stage()`), so a 3-way pipeline
sub-split produces three mutually-dependent files, not three independent
ones — still fine to compile and still fine for *this* pure-move phase, but
the wave-parallelism payoff for *future* pipeline work depends on which
functions a plan touches, not simply on file count. The `ENV_MUTEX` hoist
(D-01) is confirmed mechanically sound: because all D-05 target modules stay
inside the single `devflow` binary crate, `cargo test -p devflow` compiles
them into exactly one test binary regardless of module count, so one
`pub(crate) static ENV_MUTEX: Mutex<()>` in a shared module preserves the
exact one-instance-per-process guarantee that exists today by accident.

19g's wiring point is not where CONTEXT.md's framing implies. `/gsd-code-review`
is a *global* agent definition (`~/.claude/agents/gsd-code-reviewer.md`,
mirrored per-tool), not a DevFlow-repo file — editing it would change review
behavior for every GSD project on the machine, not just DevFlow. The actual
project-scoped extension point the reviewer agent already reads is
`.claude/skills/` (or `.agents/skills/`) — DevFlow has neither today. The
plan needs a Task to create this directory with a `SKILL.md` + `rules/*.md`
encoding the five D-19 requirements.

**Primary recommendation:** Sequence exactly as CONTEXT.md locks (19a, 19b
before the split; 19g anywhere), but scope 19a as "design and land a new
shared `.devflow/` constructor" rather than "patch an existing one," and scope
the pipeline sub-split with an explicit acknowledgment that
`preflight.rs` ↔ `pipeline*.rs` will be a genuinely coupled pair, not two
independent files.

## Architectural Responsibility Map

DevFlow is a two-crate Rust workspace (binary `devflow-cli` over library
`devflow-core`), not a web-tiered app — the table below maps each capability
to *crate* and, within `devflow-cli`, to the *target module cluster* this
phase creates.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| CLI arg parsing / dispatch | `devflow-cli` (`main.rs`, thin) | — | `Cli`/`Command`/`GateCmd` (clap derives) + `main`/`run` stay put per D-05 |
| Agent preflight (binary/gh-auth/interactivity checks) | `devflow-cli` (`preflight.rs`, new) | `devflow-cli` (`pipeline*.rs`, new) | `run_preflight` calls `launch_stage_inner` directly (main.rs:861) — a real cross-module dependency, not just a test one |
| Build staleness / provenance | `devflow-cli` (`staleness.rs`, new) | — | Self-contained: `embedded_commit_is_stale` → `enforce_build_staleness` call chain does not reach outside this cluster |
| Pipeline state machine (launch/advance/transition/gate) | `devflow-cli` (`pipeline*.rs`, new) | `devflow-cli` (`preflight.rs`, new) | Bidirectional with preflight (see above); internally cyclic across its own D-06 sub-seams (see Pitfall 1) |
| Parallel/sequentagent orchestration | `devflow-cli` (`parallel.rs`, new) | `devflow-cli` (`pipeline*.rs`, new) | Spawns/monitors phases that each run the pipeline state machine |
| Commands/display (`status`, `doctor`, `logs`, `gate`, `list`, `recover`) | `devflow-cli` (`commands.rs`, new) | — | Largest cluster (~1,283 lines); already has an internal `mod doctor_reconciliation` test sub-namespace, unaffected by the split |
| `.devflow/` directory + file lifecycle | `devflow-core` (7 independent modules — see Pitfall 3) | `devflow-cli` (call sites) | No single existing chokepoint; 19a must create one |
| Git commit semantics (`commit_all`/`commit_path`) | `devflow-core::git` | `devflow-core::hooks` | Two call sites in `hooks.rs` (`docs_update`→`commit_all`; `changelog_append`+`version_bump`→`commit_path`) |
| AI change acceptance contract enforcement | GSD agent-definition tier (`~/.claude/agents/gsd-code-reviewer.md`, global) | **DevFlow project-skill tier** (`.claude/skills/`, does not exist yet) | The global tier cannot be safely edited for one project; the project-skill tier is the correct, already-supported extension point |

## Standard Stack

No new external dependencies are introduced by any unit in this phase.

- **19a/19b:** Use only `std::fs`/`std::env` and the existing `devflow-core::git`/`workflow` APIs.
- **19c–19f:** Pure Rust module-system refactor (`mod`, `pub(crate)`). No new crates.
- **19g:** Markdown prose (`SKILL.md`, `rules/*.md`, `CONTRIBUTING.md`). No source, no crates.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `mod x;` + `x.rs` sibling files (D-05/D-07 decision) | `mod x;` + `x/mod.rs` subdirectories | Rejected by D-07 on measured wave-count grounds, not correctness — both compile identically |
| Hand-written pub(crate) audit | `cargo-expand` or a custom "diff public surface" script | `cargo-expand` is not installed/used anywhere in this workspace at HEAD (`rg cargo-expand` finds nothing in Cargo.lock or CI) — do not introduce a new dev-dependency for this; the git-diff-per-function procedure below (Code Examples) needs no new tooling and is arguably more precise for a *private*-item move, since `cargo-expand`'s value is macro expansion, not visibility auditing `[ASSUMED — reasoning about cargo-expand's applicability, not separately web-verified]` |

**Installation:** none required.

## Package Legitimacy Audit

Not applicable — this phase introduces zero new external packages in any unit.

## Architecture Patterns

### System Architecture Diagram — current vs. target

```
CURRENT (main.rs, 8,467 lines, single translation unit):

  devflow start/parallel/sequentagent/advance/status/...
        │
        ▼
  ┌─────────────────────────── main.rs (crate root) ───────────────────────────┐
  │  Cli/Command/GateCmd (arg types)                                           │
  │  main() / run() ──► dispatch to command fns, ALL private to this one file  │
  │                                                                             │
  │  preflight fns ──────► launch_stage_inner()  (production call, main.rs:861)│
  │       ▲                        │                                          │
  │       └── launch_stage() ──────┘  (production call, main.rs:1389)         │
  │                                                                             │
  │  staleness fns (called from start()/launch_stage path)                     │
  │  pipeline: launch_stage/advance/transition/handle_*_outcome/run_gate ──┐   │
  │       advance() ──► handle_*_outcome() ──► transition()/run_gate() ────┘   │
  │       transition() ──► launch_stage()  (cycle closes here, main.rs:2083)   │
  │  parallel/sequentagent fns                                                 │
  │  commands/display fns (status, doctor, logs, gate, list, recover)          │
  │  ──────────────────────────────────────────────────────────────           │
  │  #[cfg(test)] mod tests { static ENV_MUTEX; init_repo(); 106 #[test] fns } │
  └─────────────────────────────────────────────────────────────────────────┘

TARGET (D-05/D-06, pure move — same call graph, new file boundaries):

  main.rs (thin: Cli/Command/GateCmd, main(), run(), routing)
        │  uses pub(crate) items from:
        ▼
  ┌───────────────┐   pub(crate) fn run_preflight()   ┌──────────────────────┐
  │ preflight.rs  │ ─────────────────────────────────► │ pipeline*.rs         │
  │               │ ◄───────────────────────────────── │ (launch/advance,     │
  └───────────────┘   pub(crate) fn launch_stage_*()   │  handle_*_outcome,   │
                                                         │  transition/gate)   │
  ┌───────────────┐                                     │  — internally       │
  │ staleness.rs  │ ──(called from launch/start path)──►│  cyclic, see        │
  └───────────────┘                                     │  Pitfall 1          │
                                                         └──────────────────────┘
  ┌───────────────┐   spawns/polls phases running       ┌──────────────────────┐
  │ parallel.rs   │ ─────────────────────────────────►  │ pipeline*.rs (same)  │
  └───────────────┘                                     └──────────────────────┘
  ┌───────────────┐
  │ commands.rs   │  (status/doctor/logs/gate/list/recover — self-contained)
  └───────────────┘
  ┌───────────────────────────────────────────────────────────────────────┐
  │ test_support.rs (#[cfg(test)], NEW):                                   │
  │   pub(crate) static ENV_MUTEX; pub(crate) fn init_repo() (+ variants); │
  │   AlwaysFailAdapter/FailOnceAdapter; stub_agent_binary(); prepend_path │
  │ — imported by every sibling module's own #[cfg(test)] mod tests block │
  └───────────────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure

```
crates/devflow-cli/src/
├── main.rs          # Cli/Command/GateCmd, main(), run(), arg routing only
├── preflight.rs      # worktree_writable_roots..run_preflight (main.rs:648-893 today)
├── staleness.rs       # embedded_commit_is_stale..enforce_build_staleness (main.rs:936-1253)
├── pipeline.rs OR pipeline_{launch,outcomes,gate}.rs   # main.rs:1254-2290 (see D-06 discretion)
├── parallel.rs        # main.rs:2291-2742
├── commands.rs         # main.rs:2743-4025 (status/doctor/logs/gate/list/recover)
├── config_parse.rs    # OPTIONAL — main.rs:30-53, four small env-parsing fns (Claude's discretion: may fold into main.rs instead)
└── test_support.rs    # #[cfg(test)] shared fixtures — NEW, not in D-05's named list; content identified below
```

### Pattern 1: The pipeline cluster is internally cyclic — expect mutual `pub(crate)` imports, not a clean DAG

**What:** D-06 names three seams inside the ~1,037-line pipeline cluster
(`launch_stage`/`advance`/`transition` at main.rs:1254-1573,2059-2091;
`handle_*_outcome` family at main.rs:1573-1953; `run_gate`/`finish_workflow`
at main.rs:2130-2290). Verified call graph at HEAD:

- `advance()` (main.rs:1446) calls `handle_infra_outcome`, `handle_rate_limited_outcome`,
  `handle_validate_outcome`, `handle_ship_outcome`, `handle_stage_failure` — **A → B**.
- `handle_validate_outcome`/`handle_ship_outcome`/`handle_stage_failure` call
  `run_gate` (main.rs:1762,1793,1870) and `finish_workflow` (main.rs:1814) — **B → C**.
- `transition()` (main.rs:2059-2083) — squarely in "group C" per D-06's own
  naming — calls `launch_stage()` at its last line (main.rs:2083). `loop_back_to_code()`
  (main.rs:2088-2092, also group C) calls `launch_stage()` too. **C → A, closing the cycle.**

**When to use this finding:** When deciding whether to split pipeline into 1
file or 3. Rust has no restriction against this (module-level cycles compile
fine; only the *crate* dependency graph must be acyclic — confirmed via the
Rust community's own explanation of the module system:
`[CITED: users.rust-lang.org/t/how-to-organize-crate-with-circular-dependencies/19111]`).
So a 3-way split is legal and will compile. But each of the three files will
need `pub(crate)` on the functions the others call, and — architecturally —
a future plan that changes pipeline *logic* (not this phase, which is pure
move) is likely to touch two or three of these files together regardless of
how they're split, since the call graph loops through all three. Recommend
the plan state this explicitly rather than promise "N independent files" as
a hard wave-parallelism guarantee for pipeline-internal work; the file split
still helps because *other* clusters (preflight, staleness, parallel,
commands) genuinely gain independence from pipeline and each other.

### Pattern 2: `preflight.rs` and `pipeline*.rs` will be a genuinely coupled pair at the production-code level

**What:** `run_preflight` (main.rs:813-893, preflight cluster) calls
`launch_stage_inner(state, None, None)` directly at **main.rs:861** — this is
the 18-07 fix's "Advance arm skips the just-adjudicated check" behavior
(STATE.md 2026-07-21 decision log). In the other direction, `launch_stage`
(main.rs:1363-1402, pipeline cluster) calls `run_preflight` directly at
**main.rs:1389**. This is a real, bidirectional, non-test production
dependency between the two clusters D-05 places in separate files.

**When to use this finding:** Confirm the plan does not assume `preflight.rs`
can be reviewed, tested, or reasoned about in isolation from
`pipeline*.rs` — it will need `use crate::pipeline::launch_stage_inner;` (or
whatever the pipeline module's literal name ends up being), and vice versa.
This is not a blocker for the *pure move* (D-09) — it only affects future
wave-planning expectations.

### Pattern 3: The shared test-support module needs more than `ENV_MUTEX`

**What:** D-01 names `ENV_MUTEX` explicitly, but scanning every helper
function/struct defined directly inside `mod tests` (i.e., not inside a
`#[test]` fn) shows several more items used across what will become
different files after the split:

| Item | Defined at | Used by (non-exhaustive) |
|------|-----------|---------------------------|
| `static ENV_MUTEX: Mutex<()>` | main.rs:4034 | 18 `.lock()` sites across preflight, pipeline, staleness clusters' tests |
| `fn init_repo(root: &Path)` | main.rs:4708 | Used pervasively — virtually every integration-style unit test across every cluster |
| `fn init_repo_no_version_file(root: &Path)` | main.rs:4302 | hooks/checkout-hook tests (pipeline cluster) |
| `fn init_repo_with_diverged_commit(root: &Path)` | main.rs:7179 | staleness cluster only |
| `struct AlwaysFailAdapter;` / `impl AgentAdapter` | main.rs:6348 | preflight tests AND cross-cluster preflight+pipeline tests |
| `struct FailOnceAdapter` / `impl AgentAdapter` | main.rs:6382 | same — cross-cluster preflight+pipeline tests |
| `fn stub_agent_binary(name: &str)` | main.rs:6483 | preflight + pipeline tests (fake `claude`/`codex` binaries) |
| `fn agent_free_git_only_path_dir()` / `agent_free_dir_with_agent_stub()` | main.rs:6437,6463 | preflight tests |
| `fn prepend_path(...)` | main.rs:6497 | preflight + pipeline tests (PATH manipulation, under ENV_MUTEX) |
| `fn stage_launched_count(root, phase)` | main.rs:6511 | preflight + pipeline cross tests |
| `fn worktree_staleness_fixture()` | main.rs:6989 | staleness cluster only — can move with staleness.rs instead of the shared module |
| `fn drive_validate_advance_and_read_gate_context(...)` | main.rs:5052 | pipeline cluster only — can move with pipeline |

**When to use this finding:** Scope the "shared test-support module" Task to
include `ENV_MUTEX`, `init_repo` (+ its two variants used outside staleness),
`AlwaysFailAdapter`/`FailOnceAdapter`, `stub_agent_binary`,
`agent_free_git_only_path_dir`/`agent_free_dir_with_agent_stub`,
`prepend_path`, and `stage_launched_count`. Items used by only one cluster
(`worktree_staleness_fixture`, `drive_validate_advance_and_read_gate_context`)
should move with that cluster's own test module instead of bloating the
shared one.

### Pattern 4: Cross-cluster tests exist and are identifiable by name

**What:** Four tests call BOTH `run_preflight()` (preflight) AND
`launch_stage()`/`launch_stage_inner()` (pipeline) directly in the test body,
not just through internal production coupling:

- `run_preflight_advance_gate_launches_agent_exactly_once` (main.rs:6533)
- `run_preflight_loopback_gate_launches_agent_exactly_once` (main.rs:6594)
- `run_preflight_advance_skips_recheck_on_idempotently_failing_check` (main.rs:6683)
- `run_preflight_loopback_bounds_recursion` (main.rs:6756)

A fifth, `preflight_retries_reset_on_pass` (main.rs:6826), calls only
`run_preflight()` but exercises the internal `run_preflight → launch_stage_inner`
production coupling transitively — it can live in `preflight.rs` alone.

**When to use this finding:** D-10 says "tests spanning clusters need
explicit handling" — this is the concrete list. Recommend placing these four
tests in `preflight.rs` (they are fundamentally preflight-behavior tests that
happen to assert on launch side-effects) and importing
`pipeline::launch_stage`/`launch_stage_inner` + `test_support` items into
`preflight.rs`'s own `#[cfg(test)] mod tests`. Do not split a single test
function's body across files.

### Anti-Patterns to Avoid

- **Treating the split as N independent files:** as shown above, at least
  two cluster pairs (preflight↔pipeline, and pipeline's own three internal
  seams) are genuinely coupled. Plan waves accordingly — D-12 already
  authorizes surfacing coupling findings rather than forcing artificial
  independence.
- **Converting `workflow::devflow_dir()` into a side-effecting function:** it
  is a pure path-computation helper called from ~10+ sites including tests
  that assert on the *path*, not on I/O. Adding `create_dir_all` + gitignore
  writing to it would silently add filesystem I/O to a function whose
  contract today is "no I/O." Use a new, separately-named function instead
  (see Pitfall 3).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Proving a pure move made no content changes | A custom AST-diff tool | Per-function `git diff --no-index` on extracted byte ranges (old vs. new location) — see Code Examples | No new tooling/dependency; works on *private* items where `cargo public-api` (crate-API-surface only) would not help `[ASSUMED — cargo public-api's scope, not verified against this workspace since it targets pub APIs and everything here is pub(crate)]` |
| Detecting whether the whole test suite still passes with identical coverage | A custom test-inventory script | `cargo test --workspace -- --list` captured before and after, diffed on the trailing function name (module path prefix will legitimately change) | Built into `cargo test`, already used implicitly by this project's CI |
| PII exposure fix (WR-02) | A custom path-redaction library | `std::path::Path::file_name()` (already used pattern in this codebase for similar redactions — see `staleness.rs`'s `execution_root` naming precedent in 18-06's SUMMARY) | One stdlib call; D-15 explicitly wants only the binary name preserved |

**Key insight:** Nothing in this phase needs a new crate. The temptation is to
reach for `cargo-expand`, an AST tool, or a custom lint for the equivalence
proof — the git-diff-per-function-range procedure below is simpler, needs no
new dev-dependency, and is directly auditable by a human reviewer.

## Runtime State Inventory

**Not triggered in the strict sense this template targets** (renaming
identifiers/strings that appear in persisted data, external service configs,
or OS-registered state) — 19c–19f is a structural Rust module split with
**zero string/schema changes** by design (D-09). Explicitly verified:

- **Stored data:** No `.devflow/*.json` schema, field name, or event name
  changes as part of the split. `events.jsonl` event names (`workflow_started`,
  `transition`, `loop_back`, `workflow_finished`, etc.) are untouched — they
  are `serde_json::json!` literals inside function bodies that move verbatim.
- **Live service config:** None — this phase touches no n8n/Datadog/Tailscale-
  style external config.
- **OS-registered state:** None — no Task Scheduler/pm2/systemd surface.
- **Secrets/env vars:** None renamed. `DEVFLOW_GATE_TIMEOUT_SECS`,
  `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, `PATH`, `DEVFLOW_GATE_NOTIFY_CMD` keep
  their exact names; only which *file* reads them changes.
- **Build artifacts:** `Cargo.toml` package/binary name (`devflow`) is
  unchanged; no new `[lib]` target is introduced (confirmed:
  `crates/devflow-cli/Cargo.toml` has no `[lib]` section — the split stays
  entirely inside the existing `[[bin]]`-implicit `main.rs` module tree).

19a and 19b DO touch persisted output shape narrowly and deliberately (in
scope, not incidental): 19a adds a new file (`.devflow/.gitignore`, content
`*`) that did not exist before; 19b changes `events.jsonl`'s downstream git
behavior (no more empty commits) but no event schema changes. Both are the
locked, intended behavior changes for those units, not runtime-state drift.

## Common Pitfalls

### Pitfall 1: The pipeline cluster's own three D-06 seams call each other in a cycle

**What goes wrong:** A plan that sub-splits pipeline.rs into
`pipeline_launch.rs` / `pipeline_outcomes.rs` / `pipeline_gate.rs` (or
similar) and assumes each file is independently reviewable/testable will be
wrong — see Architecture Pattern 1 above for the exact call chain
(`advance → handle_*_outcome → run_gate/finish_workflow/transition →
launch_stage`, closing back on itself).
**Why it happens:** The pipeline state machine's actual control flow *is* a
loop by design (Code → Validate → Ship, with loop-backs) — the code
structure mirrors that.
**How to avoid:** Plan the pipeline sub-split (if attempted) as producing
`pub(crate)` boundaries for auditability and wave-scoping of *unrelated*
future work, not as a guarantee that any single pipeline sub-file can be
edited without touching its siblings.
**Warning signs:** A plan task that claims "editing `pipeline_gate.rs` alone
closes this pipeline bug" — verify the bug's root function against the call
graph above before accepting that scope.

### Pitfall 2: `run_preflight` ↔ `launch_stage`/`launch_stage_inner` is a real production dependency, not a test artifact

**What goes wrong:** Assuming `preflight.rs` can be split and reviewed fully
independently of the pipeline module.
**Why it happens:** The 18-07 fix (STATE.md 2026-07-21) deliberately made
`run_preflight`'s `Advance` arm call `launch_stage_inner` directly, skipping
the just-adjudicated check — this is intentional production behavior, not
test scaffolding.
**How to avoid:** Design `preflight.rs`'s public surface (`pub(crate) fn
run_preflight`) knowing it will both call into and be called from whatever
module hosts `launch_stage`/`launch_stage_inner`. Import explicitly; do not
try to route this call through an event/callback indirection as part of this
phase — D-09 forbids logic changes.
**Warning signs:** A plan task that lists `preflight.rs` and `pipeline*.rs`
in different waves under the "zero file overlap" rule without noting they
share `pub(crate)` surface — the file-overlap rule is about literal file
paths, so this is fine mechanically, but the *reviewer* should expect to see
both files' diffs together for the split's own PR.

### Pitfall 3: `lock::ensure_devflow_dir` does not exist — 19a needs a new chokepoint, not a patch to an existing one

**What goes wrong:** Scoping 19a as "add gitignore-write logic to
`lock::ensure_devflow_dir`" and discovering mid-task that no such function
exists.
**Why it happens:** `rg -n "ensure_devflow_dir" crates/` returns **zero
matches** at HEAD. `crates/devflow-core/src/lock.rs` has no function by that
name — its only directory-creation call is `fs::create_dir_all(parent)?` at
`lock.rs:82`, inside the *lock-file* path constructor (`acquire_path`),
which is specific to lock files and only runs when `devflow advance` (not
`devflow start`) is invoked. At least **seven** independent, unrelated
functions across seven files perform their own `create_dir_all` for a
`.devflow/` subpath in production code:
  - `crates/devflow-core/src/workflow.rs:95` (`write_state_atomic`, used by `save_state`)
  - `crates/devflow-core/src/gates.rs:325` (`write_atomic`, gate files)
  - `crates/devflow-core/src/monitor.rs:98` (stdout/stderr/exit/pid capture files)
  - `crates/devflow-core/src/agent_result.rs:964` (`history_dir`, archive-on-relaunch only)
  - `crates/devflow-core/src/events.rs:58` (`events.jsonl`, fail-soft on error)
  - `crates/devflow-core/src/ship.rs:85` (`write_cron_instructions`)
  - `crates/devflow-core/src/lock.rs:82` (lock files — `advance()` only, not `start()`)
**How to avoid:** Design a genuinely new `pub fn ensure_devflow_dir(project_root:
&Path) -> io::Result<PathBuf>` (co-located with `workflow::devflow_dir` in
`workflow.rs` is the natural home — it already owns the pure path-computation
version) that creates the directory AND writes `.devflow/.gitignore`
idempotently, then call it from the **one call path that is verified to run
earliest for every phase**: `start()`'s `workflow::save_state(&state)?;` at
**main.rs:622**, which runs before `events::emit` (main.rs:623) and before
`launch_stage` (main.rs:629) in every `start()` invocation (verified by
reading `start()` in full, main.rs:521-641). The `.gitignore` only needs to
exist before `docs_update`'s `git add .` — which fires much later, at Ship —
so it does not need to be written by literally the *first* directory
creator; it needs to be written by *some* code that unconditionally runs
before Ship, and `save_state` inside `start()` is that path for every normal
run. Note `devflow parallel`/`sequentagent` also call `save_state` per-phase
early (worth a quick confirming grep at plan time, not done exhaustively
here) — if any command reaches `git add .` without ever calling
`save_state` or the new function first, that command needs its own call
site too.
**Warning signs:** A plan task titled "patch `lock::ensure_devflow_dir`" —
the function must first be created, and its natural home is `workflow.rs`,
not `lock.rs`, based on where the earliest universal `.devflow/` creation
call already lives.

### Pitfall 4: `agent_result.rs` has its own private, duplicate `devflow_dir()` function

**What goes wrong:** Assuming `workflow::devflow_dir()` is the only path
computation for `.devflow/`.
**Why it happens:** `crates/devflow-core/src/agent_result.rs:872` defines a
second, private `fn devflow_dir(project_root: &Path) -> PathBuf { ... }` —
same name, same likely body (`project_root.join(".devflow")`), independent
of `workflow::devflow_dir` (main.rs:3198 and `workflow.rs:33` both use the
`pub` one; `agent_result.rs`'s internal stdout/stderr/exit/pid path helpers
use its own private one).
**How to avoid:** Not in scope to fix (pre-existing duplication, not part of
any locked decision) — but be aware when auditing ".devflow/ constructors"
that this second definition exists and do not assume a single source of
truth for the path itself, only for the *gitignore write*, which should be
one new function regardless of how many places compute the path.
**Warning signs:** A search for "the" `devflow_dir` function that stops at
the first (`pub`) hit and misses the second.

### Pitfall 5: `cargo clippy --workspace --all-targets` on a binary-only crate without `[lib]` can produce transient dead-code false positives during a staged move

**What goes wrong:** If any Task splits a cluster's *production* code from
its *test* code into separate commits (rather than moving both atomically),
an item used only by not-yet-moved test code can trip `-D warnings` dead-code
lints on the plain `bin` target build, which — per the 18-01 precedent
(STATE.md 2026-07-20 decision entry) — compiles `crates/devflow-cli` without
`#[cfg(test)]` because it has no `[lib]` target.
**Why it happens:** `devflow` (the `devflow-cli` package) is binary-only; this
already bit Phase 18 for an unrelated pure-core/wiring split.
**How to avoid:** Move a cluster's production code and its test module in the
**same commit** (D-10 already mandates this for a different reason — this is
a second, independent reason to keep them atomic). If a Task must stage the
move, apply the same `#[allow(dead_code)]`-with-a-removal-commit-reference
pattern 18-01 used, verified clean independently after each commit.
**Warning signs:** `cargo clippy --workspace --all-targets -- -D warnings`
failing mid-split with `dead_code` on an item that is used exclusively by
`#[cfg(test)]` code not yet moved into the same file.

### Pitfall 6: Every top-level type/fn in `main.rs` is currently unqualified-private — the visibility pass is large but mechanical

**What goes wrong:** Underestimating the size of the "add `pub(crate)`"
pass. Verified: `rg "^(pub )?(enum|struct|fn) " crates/devflow-cli/src/main.rs`
against the production half (lines 1-4025) shows **zero** existing `pub` or
`pub(crate)` items — everything (`CliError`, `Staleness`, `StalenessOutcome`,
`ValidateOutcome`, `ValidateResult`, `Liveness`, `Check`, `Severity`,
`PhaseFacts`, `PhaseFinding`, and every helper `fn`) is bare, private-to-the-
current-module-which-today-is-the-whole-file.
**Why it happens:** The file has never needed intra-crate visibility control
because it has always been one module.
**How to avoid:** `CliError` alone is returned by nearly every function in
the file — it becomes `pub(crate)` and every sibling module needs `use
crate::CliError;`. Treat "make `CliError` pub(crate) and re-export/import it
everywhere" as its own explicit, early sub-step (it blocks compiling any
other moved cluster), not an incidental detail of moving a first cluster.
**Warning signs:** A plan that lists visibility changes as a one-line
afterthought inside a larger move task rather than a named, orderable
sub-step.

## Code Examples

### The shared test-support module skeleton (Pattern 3)

```rust
// Source: derived from this codebase's own existing pattern (main.rs:4026-4034,
// the current ENV_MUTEX definition) — not an external reference.
// crates/devflow-cli/src/test_support.rs (NEW — name is Claude's discretion)
#![cfg(test)]

use std::sync::Mutex;

/// Serializes tests that mutate process-global env vars (`set_var`/
/// `remove_var` are process-wide and `cargo test` runs in parallel by
/// default) so they don't race each other. Shared across every split
/// module's own `#[cfg(test)] mod tests` block — see D-01/D-04. Because all
/// split modules stay inside the single `devflow` binary crate (D-05/D-08),
/// `cargo test -p devflow` compiles them into exactly one test binary, so
/// this remains a true one-instance-per-process mutex regardless of module
/// count `[CITED: doc.rust-lang.org Rust Book, Test Organization — unit
/// tests share the crate's single compilation]`.
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());

pub(crate) fn init_repo(root: &std::path::Path) { /* moved verbatim from main.rs:4708 */ }
// ... AlwaysFailAdapter, FailOnceAdapter, stub_agent_binary, prepend_path,
// stage_launched_count, agent_free_git_only_path_dir, agent_free_dir_with_agent_stub
```

Each sibling module's own test block then does:

```rust
#[cfg(test)]
mod tests {
    use super::*;                    // this file's own pub(crate) items
    use crate::test_support::*;      // ENV_MUTEX, init_repo, fake adapters, etc.

    #[test]
    fn some_moved_test() {
        let _guard = ENV_MUTEX.lock().unwrap();
        // ...
    }
}
```

### Equivalence-proof procedure for the pure move (per-function, not per-file)

```bash
# Source: procedure derived for this phase, not an external reference —
# no cargo-expand / AST-diff dependency required.
# 1. Before the move, snapshot the exact byte range of a function from the
#    pre-split commit:
git show <pre-split-sha>:crates/devflow-cli/src/main.rs | sed -n '813,893p' > /tmp/before.rs

# 2. After the move, extract the same function from its new home, stripping
#    only the leading indentation change and the new `pub(crate)` keyword
#    (both expected, both auditable by eye in a small diff):
sed -n '<new-start>,<new-end>p' crates/devflow-cli/src/preflight.rs > /tmp/after.rs

# 3. Diff — the ONLY hunks allowed are: added `pub(crate)`, changed `use`
#    paths for now-external items (e.g. `crate::CliError` instead of bare
#    `CliError`), and de-indentation from being inside `mod tests { ... }`
#    if applicable. Any other hunk is a behavioral change and fails D-09.
diff -u /tmp/before.rs /tmp/after.rs
```

```bash
# Whole-suite equivalence check: same test SET before/after, not just same
# count. Module-path prefixes legitimately change (e.g. `tests::foo` becomes
# `preflight::tests::foo`), so compare trailing function names, not full paths.
cargo test -p devflow -- --list | grep '^tests::\|::tests::' | sed 's/.*::tests:://' | sort > /tmp/before_names.txt
# ... after the split ...
cargo test -p devflow -- --list | grep '::tests::' | sed 's/.*::tests:://' | sort > /tmp/after_names.txt
diff /tmp/before_names.txt /tmp/after_names.txt   # must be empty
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `commit_path`'s doc comment claims "Ok(()) whether or not the path had changes" | `--allow-empty` actually makes it always commit, contradicting its own doc comment | Introduced with `commit_path` itself (17-12, per `hooks.rs` comments) | 19b restores the doc comment's truth and revives the `nothing to commit` arm as reachable dead code |
| Single flat `main.rs` for all CLI logic | Six-plus module split (this phase) | 2026-07-21 (this phase) | Enables true wave parallelism for Phase 20's `main.rs`-heavy backlog items |

**Deprecated/outdated:** None — this phase does not remove any public
capability; it relocates private code and closes two release-integrity gaps.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `cargo-expand` is not installed/used in this workspace and is not the right tool for a `pub(crate)`-visibility pure-move audit (vs. crate-API-surface tools like `cargo public-api`) | Standard Stack, Don't Hand-Roll | Low — the recommended git-diff-per-function procedure is tool-independent and works regardless; if the planner prefers `cargo-expand` it can be added as a genuine dev-dependency instead, no correctness risk either way |
| A2 | `devflow parallel`/`sequentagent` also call `workflow::save_state` early enough to be covered by the same `ensure_devflow_dir` chokepoint recommended for `start()` | Pitfall 3 | Medium — if some command path reaches a `.devflow/` write (e.g., an events emit) before ever calling `save_state`, that path would create `.devflow/` without the `.gitignore`, reopening a narrower version of WR-01. Verify at plan time with a targeted grep of each command's call order before finalizing the single-insertion-point design. |
| A3 | Recommended file names (`preflight.rs`, `staleness.rs`, `pipeline.rs`/`pipeline_*.rs`, `parallel.rs`, `commands.rs`, `config_parse.rs`, `test_support.rs`) are reasonable defaults | Recommended Project Structure | None — explicitly Claude's Discretion per CONTEXT.md; the plan can rename freely |

## Open Questions

1. **Does `workflow_started_payload` (main.rs:894-935, containing the WR-02
   `exe_path` emission) belong in `staleness.rs` or travel with `start()`
   in the thin `main.rs`/a future `commands.rs`?**
   - What we know: It sits contiguously before the staleness cluster
     (embedded_commit_is_stale onward, main.rs:936) in the current file, and
     is only ever called from `start()` (main.rs:627).
   - What's unclear: D-05's named cluster list doesn't call it out
     separately, and it has no functional relationship to build staleness —
     it just happens to be adjacent in the file today.
   - Recommendation: Since 19a lands *before* the split (D-20), fix WR-02 in
     place first; by the time the split happens the function's final home is
     a low-stakes naming choice — leave it as Claude's Discretion at plan
     time, noting it is NOT part of the staleness cluster's actual logic.

2. **Should the mechanical `TESTING.md`/`STRUCTURE.md` regeneration (flagged
   as "owned by this phase" in the backlog dossier's "Mechanical follow-up"
   note) be a locked Task in the plan, or left implicit?**
   - What we know: Both docs are confirmed stale today — `TESTING.md` cites
     a deleted test; `STRUCTURE.md` still describes `main.rs` as
     "single 1000+ line file" (now further wrong post-split either way).
   - What's unclear: CONTEXT.md's `## Deferred Ideas` section lists this as
     deferred, but the dossier's own "Mechanical follow-up owned by this
     phase" section says "not worth its own backlog number, but do not let
     it slide past this phase's completion" — these two documents disagree
     on whether it's in-scope.
   - Recommendation: Treat it as in-scope for this phase's completion (the
     dossier's framing is more specific and post-dates the general deferred
     list), scoped as a small final Task after the split lands — trivial
     effort now that the actual line counts/cluster names are known from
     this research.

3. **19a's two fixes (WR-01 `.gitignore`, WR-02 `exe_path`) — same Task or
   separate Tasks?**
   - What we know: D-13 requires both; they are independent code paths
     (`workflow.rs`/new function vs. `main.rs`'s `workflow_started_payload`)
     with no shared files.
   - What's unclear: Nothing blocking — flagging only because they could be
     split into two waves within 19a's own scope for review clarity, given
     WR-01 is now confirmed larger in scope (new function + call-site
     wiring) than WR-02 (one-line `.file_name()` change + one test update at
     main.rs:6879).
   - Recommendation: Two Tasks, potentially two Plans, is reasonable given
     the size asymmetry now measured.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo`/`rustc` (stable) | All units | ✓ | pinned via `rust-toolchain.toml` (`stable` + clippy/rustfmt) | — |
| `git` CLI | 19b, `gitignore_coverage.rs`, equivalence-proof procedure | ✓ | system git | — |
| `rg` (ripgrep) | Research/plan-time auditing (not a build dependency) | ✓ | — | `grep -r` |
| CI (GitHub Actions, push-triggered) | D-11 | ✓ already configured — `.github/workflows/ci.yml` triggers on `on: push` with no branch filter, so pushing the split branch runs `cargo test`/`clippy --workspace --all-targets -D warnings`/`fmt --check` automatically | — | — |
| `cargo-expand` | Not required (see Standard Stack) | not checked/not installed | — | n/a — not recommended for this phase |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none — everything required is already present.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (no external harness) |
| Config file | none — implicit via `Cargo.toml`/`rust-toolchain.toml` |
| Quick run command | `cargo test -p devflow <test_name>` (NOTE: `--lib` does not work on this binary-only crate — confirmed dead end per STATE.md's 2026-07-20 18-01 decision entry; use the bare form) |
| Full suite command | `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 19a-WR01 | New `.devflow/` dirs are self-ignoring | integration | new test in `crates/devflow-core/src/workflow.rs` (or wherever `ensure_devflow_dir` lands) asserting `.devflow/.gitignore` content `*` after the function runs, PLUS a live-repo reproduction of the exact scratch-repo scenario from `17-REVIEW.md` (`git add . && git commit` no longer sweeps `.devflow/*`) | ❌ Wave 0 — needs writing alongside the fix |
| 19a-WR02 | `events.jsonl`'s `exe_path` field carries only a filename, no path separators | unit | existing `workflow_started_payload_carries_build_provenance` (main.rs:6867) needs updating, not a new file — assert `!payload["exe_path"].as_str().unwrap_or("").contains('/')` | ✅ update existing test |
| 19b | `commit_path` on a byte-identical re-run is a genuine no-op (no commit created) | unit | new test in `crates/devflow-core/src/git.rs` `mod tests` — RED-then-GREEN: call `commit_path` twice with identical content, assert `git rev-list --count HEAD` unchanged after the second call | ❌ Wave 0 — no existing test covers this exact case (confirmed via `[VERIFIED: local grep]` search for "nothing to commit"/"allow-empty"/"empty commit" across `git.rs`/`hooks.rs`/`main.rs`/`tests/*.rs`) |
| 19c–19f | Zero behavioral change (pure move) | equivalence proof, not a new test | Per-function `diff` procedure (Code Examples) PLUS `cargo test --workspace -- --list` name-set diff PLUS full `cargo test --workspace` pass-count identity (baseline: 296 devflow-core lib + 2 `monitor_e2e` + 106 devflow-cli unit + ~20 devflow-cli integration tests, `[VERIFIED: cargo test -p devflow-core -- --list` and `cargo test -p devflow -- --list`, run live this session]) | n/a — this is a structural proof, not a new test file |
| 19g | Reviewer applies the five D-19 requirements | manual/process | New `.claude/skills/<name>/SKILL.md` + `rules/*.md`; verify by re-running `/gsd-code-review` against a deliberately non-compliant test-only diff and confirming it is flagged (dogfood the contract on itself) | ❌ Wave 0 — skill directory does not exist |

### Sampling Rate

- **Per task commit:** `cargo test -p devflow <affected_test_or_module>` (fast, targeted)
- **Per wave merge:** `cargo test --workspace` (baseline: currently 296+2+106+~20 ≈ 424 tests workspace-wide before this phase's own new tests, `[VERIFIED: live cargo test --list this session, 2026-07-21]`)
- **Phase gate:** Full suite green **on a CI run against the branch**, not just local-green — D-11 is explicit and CI is already configured to trigger automatically on push (see Environment Availability)

### Wave 0 Gaps

- [ ] New test in `crates/devflow-core/src/workflow.rs` (or new function's home) for the `.gitignore` write — covers 19a-WR01
- [ ] Update `workflow_started_payload_carries_build_provenance` (main.rs:6867) — covers 19a-WR02
- [ ] New test in `crates/devflow-core/src/git.rs` for `commit_path`'s no-op-on-identical-content behavior — covers 19b
- [ ] New `.claude/skills/<name>/SKILL.md` + `rules/*.md` — covers 19g; no existing project skill directory to extend

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | This phase touches no auth surface |
| V3 Session Management | no | n/a |
| V4 Access Control | no | n/a |
| V5 Input Validation | marginal | `.gitignore` content is a fixed literal (`*`), not user input; no new parsing introduced |
| V6 Cryptography | no | n/a |
| **V8 Data Protection** | **yes — this IS the phase's core security content for 19a** | Redact PII/path data before persisting to any log/event file; never persist absolute filesystem paths containing usernames to a file documented as safe to share (`OPERATIONS.md:105`'s "tail it from any tool") |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Absolute path (containing OS username/home dir) written to a log file advertised as shareable | Information Disclosure | 19a-WR02: emit only `current_exe().file_name()` (D-15) |
| Sensitive/unredacted runtime artifact swept into a `git commit` by a broad `git add .` | Information Disclosure | 19a-WR01: self-ignoring `.devflow/.gitignore` written on directory creation (D-14), independent of the target repo's own root `.gitignore` |
| Release tag placed on a commit with no actual content change (integrity of the release artifact, not confidentiality) | Tampering (of the release record, not malicious) | 19b: drop `--allow-empty` so `commit_path` genuinely no-ops instead of fabricating a commit |

## Sources

### Primary (HIGH confidence — live source read this session, 2026-07-21, HEAD of `develop`)

- `crates/devflow-cli/src/main.rs` (full function inventory, line 1-8467, all cluster boundaries, all 106 test names, all 18 `ENV_MUTEX.lock()` sites mapped to enclosing test fn)
- `crates/devflow-core/src/lock.rs`, `workflow.rs`, `gates.rs`, `monitor.rs`, `agent_result.rs`, `events.rs`, `ship.rs`, `git.rs`, `hooks.rs` (all `.devflow/` construction sites; both `--allow-empty` sites; `commit_path`/`commit_all` call sites)
- `crates/devflow-cli/tests/gitignore_coverage.rs`, `crates/devflow-core/src/doc_check.rs` (existing gitignore-coverage guards, confirmed self-repo-scoped only)
- `.github/workflows/ci.yml` (confirmed `on: push` with no branch filter — D-11's CI-on-branch requirement needs no new CI config)
- `Cargo.toml`, `crates/devflow-cli/Cargo.toml`, `rust-toolchain.toml` (no `[lib]` target confirmed; edition 2024; no new-dependency need)
- `CONTRIBUTING.md`, `OPERATIONS.md` (confirmed at repo root, not `docs/` — CONTEXT.md's canonical_refs paths for these two files are slightly stale)
- `~/.claude/agents/gsd-code-reviewer.md` and `$HOME/.claude/gsd-core/references/project-skills-discovery.md` (confirmed the project-skill discovery mechanism the reviewer already implements; confirmed no `.claude/skills/` or `.agents/skills/` directory exists in this repo today)
- Live `cargo test -p devflow-core -- --list` and `cargo test -p devflow -- --list` runs this session (test counts: 296 devflow-core lib, 2 `monitor_e2e`, 106 devflow-cli unit, ~20 devflow-cli integration)

### Secondary (MEDIUM confidence — web-verified against official/community sources)

- [Rust Book — Test Organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html) — unit tests inside `#[cfg(test)] mod tests { use super::*; }`, and `pub(crate)` as the standard visibility for cross-module-but-not-external items
- [users.rust-lang.org — How to organize crate with circular dependencies](https://users.rust-lang.org/t/how-to-organize-crate-with-circular-dependencies/19111) — confirms module-level (not crate-level) cyclic references compile fine in Rust; only the crate dependency graph must be acyclic

### Tertiary (LOW confidence)

- None used for factual claims in this document; the `cargo-expand` non-recommendation (Assumption A1) is reasoning from tool purpose, not a verified negative claim.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies exist to get wrong; verified directly against `Cargo.toml`
- Architecture (cluster boundaries, call graph, coupling findings): HIGH — every claim traced to a specific `main.rs`/`crates/devflow-core/src/*.rs` line number, verified live this session
- 19a implementation surface: HIGH — `ensure_devflow_dir`'s non-existence and the 7 alternative constructors are directly grepped facts, not inference
- 19b blast radius: HIGH — absence of any pinning test is a direct, exhaustive grep result (`nothing to commit`/`allow-empty`/`empty commit` across the relevant files)
- 19g wiring point: HIGH for "no `.claude/skills/` exists today" (directly verified `ls`); MEDIUM for "this is definitely the right mechanism" — the reviewer agent's own instructions say it applies project-skill rules, but this project has never exercised that path before, so it is unverified *by DevFlow specifically*, only by the agent definition's stated contract
- Pitfalls: HIGH — each pitfall cites the exact line(s) that produce it

**Research date:** 2026-07-21
**Valid until:** Effectively permanent for the historical claims (line numbers/call graph as of the cited commit); re-verify cluster line numbers if any other phase lands commits to `main.rs` before 19c–19f executes, since this file has grown +35% since the last stale snapshot and could drift again

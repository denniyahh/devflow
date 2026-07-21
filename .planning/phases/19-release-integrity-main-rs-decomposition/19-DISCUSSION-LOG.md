# Phase 19: Release Integrity + `main.rs` Decomposition - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-21
**Phase:** 19-release-integrity-main-rs-decomposition
**Areas discussed:** Env serialization (`ENV_MUTEX`), Module layout, 19a fix shape, 19b scope, 19g placement

---

## Env serialization across the split (`ENV_MUTEX`)

| Option | Description | Selected |
|--------|-------------|----------|
| One shared mutex in a test-support module | Hoist `ENV_MUTEX` into a shared module all split modules import. Preserves today's guarantee with no per-module reasoning. | ✓ |
| Per-module mutexes + prove disjointness | Each module keeps its own mutex; add a lint/test asserting no var is touched under two. Matches devflow-core's existing pattern. | |
| Eliminate env mutation in tests | Replace `set_var` with dependency injection. Removes the failure class permanently. | |

**User's choice:** One shared mutex in a test-support module.

**Notes:** Selected against evidence gathered during scouting, not on preference.
Scouting found there is no single `ENV_MUTEX` today — three independent
`static Mutex<()>` definitions exist (`main.rs:4034`, `gates.rs:348`,
`config.rs:174`), sound only because each guards a disjoint variable set, which
is documented nowhere. Decisive datum: `PATH` is mutated **36 times across 12
lock regions spanning lines 4522–6795**, mapping onto at least three target
clusters, so per-module mutexes would silently break exactly the serialization
that 19i raced on. Option 3 was rejected as a behavioral change to 106 tests
that would destroy the refactor's equivalence proof — preserved as a deferred
idea and arguably the real fix for the class behind 19i / GAP-2 / DEN-29.

---

## Module layout

**First pass** — options presented: flat siblings in `devflow-cli`; flat
siblings + move `staleness`/`preflight` to `devflow-core`; `commands/`
subdirectory per subcommand.

**User's initial response:** leaned toward the `commands/` subdirectory, and
challenged the framing — *"i don't care about changing the public API since
nobody's using devflow at this point other than me. my biggest concern is
maximizing development productivity, and this option sounds like it would
achieve exactly that. am i correct?"*

**Finding: no.** Phase 18's seven plans were mapped onto the clusters they
actually touched and each cluster sized at HEAD:

| Cluster | ~Lines | Phase 18 plans |
|---|---|---|
| commands/display | ~1,280 | 2 (18-01 doctor, 18-03 status+liveness) |
| **pipeline state machine** | ~1,040 | **3** (18-04, 18-05, 18-07) ← bottleneck |
| parallel/sequentagent | ~410 | 0 |
| staleness/provenance | ~320 | 1 (18-06) |
| preflight | ~200 | 1 (18-07) |

A `commands/` split buys **zero** wave reduction: `pipeline.rs` would still hold
3 serial plans, and 18-01/18-03 would still collide on `doctor.rs` since 18-03
touched both `status` and `doctor`. Two corrections were issued against
Claude's own earlier framing: (a) the "inconsistent with the codebase"
objection was **withdrawn** — `agents/mod.rs` + `claude.rs`/`codex.rs` is
exactly that pattern; (b) the genuine risk is that shared display helpers
collect into a `commands/common.rs` that re-centralizes contention.

**Second pass** — revised options presented:

| Option | Description | Selected |
|--------|-------------|----------|
| Flat siblings + split `pipeline.rs` further | Targets the measured bottleneck at its natural seams. Takes Phase 18's shape from 3 waves to 2. | ✓ |
| Flat siblings, pipeline stays whole | Conservative pure move; leaves 3-plan pipeline contention in place. | |
| Flat siblings + split pipeline + move to core | As recommended plus the crate-boundary move. | |
| `commands/` subdirectory anyway | Available for readability rather than parallelism. | |

**User's choice:** Flat siblings + split `pipeline.rs` further.

**Notes:** On the crate boundary — the operator confirmed the public-API change
is not itself a cost (no external consumers). It was still deferred, on the
different ground that touching two crates' module trees inside a pure-move
refactor adds surface for the equivalence proof to go wrong on. Recorded as a
cheap follow-up once the split is proven.

---

## 19a — `.devflow/` artifact hygiene

**User's initial response:** *"i really need help to understand what this fix is
about. explain the options to me like i'm 5."* A plain-language explanation was
given before re-asking: DevFlow writes scratch files containing raw agent output
and the operator's home path into `.devflow/`; a `git add .` in the docs-update
hook then commits that folder into a *downstream user's* repo if their
`.gitignore` lacks the entry — publishing a stranger's username and home
directory to their public GitHub without their knowledge.

| Option | Description | Selected |
|--------|-------------|----------|
| Both A and B: `.devflow/.gitignore` + record binary name only | A stops the folder being committable anywhere; B stops the data being written. | ✓ |
| Both, but drop `exe_path` entirely | Strictly less to leak; changes the `events.jsonl` schema. | |
| Narrow: scope the `git add` + binary name only | Fixes one call site; leaves `.devflow/` committable. | |

**User's choice:** Both A and B.

**Notes:** The two fixes are independent and address different halves — B stops
the sensitive data being written, A stops the file being published. Either alone
leaves the other open. The narrow variant was rejected because it fixes only the
current call site, so the next broad `git add` reopens the hole; the
`ensure_devflow_dir` approach closes it for every constructor at once.

---

## 19b — scope of the `--allow-empty` fix

| Option | Description | Selected |
|--------|-------------|----------|
| Fix `commit_path` only, note `commit_all` | Stay inside the finding as written; check and record whether `commit_all`'s behavior is load-bearing. | ✓ |
| Fix both sites | Treat `--allow-empty` as wrong in both places. | |

**User's choice:** Fix `commit_path` only, note `commit_all`.

**Notes:** `commit_all` at `git.rs:312` is also the call site 19a modifies, so
changing it here would collide the two units.

---

## 19g — AI change acceptance contract placement

| Option | Description | Selected |
|--------|-------------|----------|
| `gsd-code-review` criteria + `CONTRIBUTING.md` prose | Enforce where a blocking review already runs before Ship. | ✓ |
| Both: review criteria + mechanical lint pass | Adds automated detection of cheap-to-spot anti-patterns. | |
| Review criteria only, defer the prose | Tightest scope. | |

**User's choice:** `gsd-code-review` criteria + `CONTRIBUTING.md` prose.

**Notes:** The lint pass was rejected as net-new tooling in a phase already
carrying an L-sized refactor.

---

## Claude's Discretion

- Exact file names and precise cut points for the `pipeline.rs` sub-split — the
  seams are named in D-06, but final boundaries should follow what the code
  shows at plan time.
- Where the shared test-support module lives and what it is called.
- Whether `config_parse` warrants its own file or folds into the thin `main.rs`.

## Deferred Ideas

- Eliminate env mutation in tests via dependency injection — the permanent fix
  for the `ENV_MUTEX` failure class; rejected here only because it breaks the
  equivalence proof.
- Move `staleness`/`preflight` into `devflow-core` — cheap follow-up once the
  split is proven.
- Enforce the one-var-one-mutex invariant mechanically (lint or test).
- Regenerate `.planning/codebase/STRUCTURE.md` (found stale during this
  discussion — describes `main.rs` as "1000+ lines") alongside the already-owned
  `TESTING.md` refresh.
- Phase 20 (v2.0.0): DEN-31 `--until`, DEN-32 manual ship override, DEN-38
  release-cut automation, likely DEN-28 `gate show`.

# Scope Creep & Complexity Review

Date: 2026-07-24
Requested by: operator, mid-session, prompted by concern that functionality
has grown beyond what one person can reasonably manage with limited resources.

## Method

This is a first-pass review grounded in direct evidence gathered over the
course of this session (CLI command surface, `monitor.rs`/`hooks.rs`/
`recover.rs` source, ROADMAP.md's full backlog, module line counts) — not an
exhaustive line-by-line audit. Findings are ranked by confidence/actionability.
Where a claim is inferred rather than directly confirmed, it's marked as a
question to verify, not a recommendation to act on blind.

## Findings

### 1. `devflow list` is redundant with `devflow status` (CONFIRMED)

Both call the identical `git.list_feature_branches()`:
`crates/devflow-cli/src/commands.rs:1161` (`list`, a formatted table) and
`:1181` (`print_open_branches`, called automatically inside every `status`
run, an abbreviated inline rendering of the *same data*). There is no
scenario where `list` shows information `status` doesn't already surface.

**Recommendation:** fold the table format into `status` (or a `--verbose`
flag) and drop `list` as a top-level command. Zero information lost, less
surface to remember.

### 2. Five overlapping "what's happening" commands (HIGH CONFIDENCE)

`status`, `doctor`, `recover` (no `--clean`), `history`, `list` each show a
different slice of workflow state, for a tool operated by exactly one
person. `recover`'s inspect mode (agent liveness, staleness, lock holder —
`crates/devflow-core/src/recover.rs`) covers largely the same ground
`doctor`'s reconciliation (18a/21b) and `status`'s liveness section already
do.

**Question to resolve, not yet a recommendation:** does `recover`'s
read-only inspect mode surface anything `doctor`/`status` don't? If not,
fold `recover --clean` into `cleanup`/`doctor --repair` and drop the
standalone inspect verb.

### 3. The backlog itself is a complexity signal (HIGH CONFIDENCE)

27 open backlog items as of this session (7 High / 15 Medium / 5 Low
priority — see `.planning/ROADMAP.md` "## Backlog"). A meaningful cluster is
enterprise-grade test-infrastructure investment for a tool with exactly one
user and no external contributors depending on it: mutation testing
(999.17), property/fuzz testing for parsers (999.18), differential coverage
enforcement (999.20), fast/slow CI lanes (999.19), hermetic shell-entrypoint
tests (999.15).

**Recommendation:** explicitly close or indefinitely deprioritize this
cluster. It's real planned work aimed at guarantees a solo tool mostly
doesn't need, and pruning it costs nothing (a backlog edit, not a code
change) while directly reducing the "things we're on the hook for" list.

### 4. Two parallel multi-agent orchestration mechanisms (QUESTION)

`parallel` (N phases concurrently, isolated worktrees) and `sequentagent`
(two agents sequentially on one phase, rebase handoff —
`crates/devflow-cli/src/parallel.rs`, 622 lines) each carry independent
worktree-creation and monitor-spawning machinery; `sequentagent` additionally
owns rebase-conflict handoff logic.

**Question to resolve:** is `sequentagent` exercised regularly in real use,
or built once and now mostly maintenance weight? If the latter, strong cut
candidate. Not yet verified against real usage history (events.jsonl/git
log) — flagged as a next step, not asserted.

### 5. Shipping/release logic is scattered across four places (HIGH CONFIDENCE)

Ship's hooks (Merge/VersionBump/ChangelogAppend/BranchCleanup, in-CLI);
`release --check` (separate read-only preflight); a full release-cut
*executor* that's backlogged but unbuilt (999.25/DEN-50); and
`scripts/sync-main-to-develop.sh`, which runs entirely outside the CLI as a
manual step on every real release per STATE.md's own release narratives.

**Recommendation:** don't build 999.25 as net-new scope. Instead fold
`sync-main-to-develop.sh` into what `release --check`/`ship` already own, so
release becomes one coherent, testable path instead of three partial
mechanisms plus a hand-run script.

### 6. Config-knob sprawl cuts against the "opinionated" positioning (MEDIUM CONFIDENCE)

`devflow.toml` (`capture_retention`, `review_angles`,
`external_verify_enabled`) × an env var override for each
(`DEVFLOW_CAPTURE_RETENTION`, `DEVFLOW_REVIEW_ANGLES`,
`DEVFLOW_EXTERNAL_VERIFY_ENABLED`) × `DEVFLOW_GATE_TIMEOUT_SECS` ×
`DEVFLOW_GATE_NOTIFY_CMD` × `DEVFLOW_LOG_FORMAT` × `RUST_LOG`. Sits in
tension with the 2026-07-24 reframe of DevFlow as one opinionated system
rather than a configurable platform (see git commit `22344f0`).

**Question to resolve:** which of these knobs have actually been changed
from their defaults in real use? Knobs never touched are candidates to
become fixed constants — cutting parsing, precedence-resolution, and
per-knob test code, not just a documentation claim.

### 7. Systemic pattern: self-dogfooding as a complexity generator (NAMED, NOT SIZED)

A large share of this project's own phase history (18 "Dogfood Reliability
Hardening", 21 "Operator Legibility", the 999.29 staleness saga, this
session's 999.33/34/35/36) is the tool fixing bugs that exist *only because*
it recursively orchestrates its own development. Every self-hosting edge
case (stale-binary detection, dogfood-workspace detection, worktree-aware
staleness) becomes permanent product surface, not a one-off fix.

**Recommendation:** treat this as a standing question, not a line item —
whether the intensity of self-dogfooding is worth its complexity tax, or
whether a lighter-touch practice (use it manually sometimes, without
demanding it survive every self-hosting edge case) would free up a
meaningful share of the backlog above.

## Sizing Context

`crates/devflow-cli/src/commands.rs` is still 3,766 lines even after Phase
19 dedicated an entire phase to splitting the original 8,467-line
`main.rs` into 9 modules — the growth pattern that forced that split hasn't
stopped, just moved. Full current module sizes (production + inline tests):

| Largest modules | Lines |
|---|---|
| `crates/devflow-cli/src/commands.rs` | 3,766 |
| `crates/devflow-core/src/agent_result.rs` | 2,671 |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | 1,719 |
| `crates/devflow-core/src/git.rs` | 1,706 |
| `crates/devflow-cli/src/staleness.rs` | 1,525 |
| `crates/devflow-cli/src/pipeline_gate.rs` | 1,311 |
| `crates/devflow-core/src/version.rs` | 1,173 |

## Status

Not yet acted on. Findings 1 and 3 are ready to execute (low risk,
high confidence). Findings 2, 4, and 6 need a follow-up verification pass
before a decision. Operator to pick which threads to pursue.

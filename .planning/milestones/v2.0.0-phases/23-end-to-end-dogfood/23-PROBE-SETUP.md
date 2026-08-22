# 23-PROBE-SETUP: Rebuilt Binary + Scratch Probe Target

Produced by `23-01-PLAN.md`. Records the two preconditions closed before
23a's probe (`23-02-PLAN.md`) can be trusted: a stale binary, and the
absence of a valid probe target.

## Task 1 — Rebuilt binary, proven not stale

**Finding at plan time (RESEARCH.md § Common Pitfalls, Pitfall 6):** the
`devflow` binary resolved on `PATH` reported `1.8.0`, built 2026-07-23,
while the workspace's own `Cargo.toml` `[workspace.package].version` had
already moved to `1.8.1`. Reproduced live at execution time — `devflow
--version` printed `devflow 1.8.0` before this task ran.

**Fix applied:** `cargo build --release --workspace`, run against the
checkout at commit `2228222ad774c9f44fd6917a98728e01f075b1d1` (this
worktree's base commit; the sibling main checkout was confirmed on the
same commit via its `feature/phase-23` ref before the build, so the
rebuild reflects byte-identical source to this plan's own commit).
`/home/linuxbrew/.linuxbrew/bin/devflow` is a symlink into
`/var/home/denniyahh/Github/devflow/target/release/devflow` (the checkout
directory's release output), so the build directly refreshed the binary
already resolved on `PATH` — no separate reinstall/repoint step was
needed.

**Verification (re-run immediately before this record was written):**

```
$ devflow --version
devflow 1.8.1
$ grep -m1 '^version' Cargo.toml
version = "1.8.1"
```

Versions match. `stat -c %Y` on the resolved binary
(`/var/home/denniyahh/Github/devflow/target/release/devflow`) is newer
than the `mtime` of `crates/devflow-cli/src/main.rs`, confirming the
binary postdates the source it was built from.

## Task 2 — Scratch probe target

### Scaffolding decision

Per RESEARCH.md Open Question 3 and the "Recorded planning choices"
section of `23-01-PLAN.md`: the probe target is a **synthetic
single-task phase in a fresh repo**, not an imported real backlog item.
The probe is about the supervisor/pipeline mechanism, not about the
content of the work — importing real work into a throwaway repo adds risk
for no signal.

Minimum scaffold (implemented in `scripts/scratch-dogfood-repo.sh`):

- a git repo with `main` + `develop` and one initial commit
- `.planning/{PROJECT.md,ROADMAP.md,STATE.md}`
- one phase directory (`01-add-probe-marker`) carrying a pre-written,
  trivial single-file-change plan (`01-01-PLAN.md`), so Define/Plan have
  a real artifact to work against instead of inventing scope during a
  probe

**`.claude/` scaffolding — deliberately omitted, with evidence.**
`crates/devflow-cli/src/preflight.rs`'s `run_preflight` was read in full
before writing the script (per the plan's `<read_first>` instruction).
The only check relevant to `--agent claude` is
`ensure_agent_binary(agent_program(agent))`, which resolves
`agent_program(AgentKind::Claude)` to the literal program name `claude`
and checks it is an executable file reachable via `PATH`
(`agent_binary_available`, `preflight.rs:61-75`). No check in
`preflight.rs`, `generic_preflight_checks`, or `run_preflight` inspects a
project-local `.claude/` directory, GSD command files, or any other
per-repo agent-runtime configuration. `rg -n '\.claude' crates/` (outside
test modules) returns zero non-test matches. There is therefore nothing
for the script to copy or symlink to satisfy `devflow`'s own preflight —
the script scaffolds no `.claude/` directory, and this is a source-backed
finding, not an assumption.

Branch names (`main` / `develop`) are hardcoded in the script as literal
strings, verified once against
`crates/devflow-core/src/config.rs:15,17` (`pub const MAIN: &str =
"main";`, `pub const DEVELOP: &str = "develop";`) rather than re-read
live by the script — see the script's own header comment.

### Resolved destination path

`/tmp/devflow-probe-check` (created fresh for this record via `rm -rf`
then `scripts/scratch-dogfood-repo.sh /tmp/devflow-probe-check`).

### Refusal-path check (T-23-01)

Running the script with a destination inside this checkout
(`./scratch-inside-check`, resolving to
`/var/home/denniyahh/Github/devflow/.claude/worktrees/agent-afb306e0bd1f7fa47/scratch-inside-check`)
exits non-zero with a refusal message:

```
ERROR: refusing to scaffold inside this checkout (/var/home/denniyahh/Github/devflow/.claude/worktrees/agent-afb306e0bd1f7fa47).
       Requested destination: /var/home/denniyahh/Github/devflow/.claude/worktrees/agent-afb306e0bd1f7fa47/scratch-inside-check
       Pick a destination outside the checkout (e.g. under $TMPDIR or ~).
```

### Git identity isolation check (T-23-02)

`git config --global --get user.email` before running the script:
`d10475u5@outlook.com`. After running the script (twice, across two
separate scaffold runs during this task): unchanged, still
`d10475u5@outlook.com`. `git -C /tmp/devflow-probe-check config --get
user.email` inside the scratch repo returns `devflow-probe@localhost` —
a repo-local value set via `git -C <dest> config` (never `--global`), and
no `GIT_*` environment variable was exported at any point.

### `devflow doctor` — exact output

```
  git                  git version 2.55.0   ✓
  sh (POSIX shell)     built-in             ✓
  cargo/rust           cargo 1.97.1 (c980f4866 2026-06-30) ✓
  gh CLI               gh version 2.96.0 (2026-07-02) ✓
  claude               2.1.220 (Claude Code) ✓
  codex                codex-cli 0.145.0    ✓
  opencode             1.18.5               ✓
  devflow v1.8.1       1.8.1                ✓
  RUST_LOG             not set — defaulting to info ✗ — Set RUST_LOG=info for better diagnostics

reconciliation:
  no active phases — nothing to reconcile

planning docs: consistent with git tags
```

Exit code: `0`. The `RUST_LOG` line is a `warn`/`missing`-status
environment-tool check (`cmd_check`/manual match in `commands.rs`'s
`doctor` checks list), not a `Severity::Problem` reconciliation finding —
`reconcile_phase` never runs against this scratch repo at all, because it
has no `.devflow/` state (`"no active phases — nothing to reconcile"`).
No line here is a blocking finding.

### `devflow start --phase 1 --agent claude --mode auto --dry-run` — exact output

```
dry run — phase 1 | agent claude | mode auto

stage pipeline:
  define /gsd-discuss-phase 1
  plan /gsd-plan-phase 1
  code /gsd-execute-phase 1
  validate /gsd-validate-phase 1 [GATE after 3 failures]
            ↳ hooks: [DocsUpdate]
  ship /gsd-ship 1 [GATE]

after ship: [Merge, VersionBump, ChangelogAppend, BranchCleanup]
```

Exit code: `0`. All five stages (define, plan, code, validate, ship) are
named.

### `claude --version` — exact output

```
2.1.220 (Claude Code)
```

Exit code: `0`, no auth prompt.

### What this does and does not prove (review finding — Codex, MEDIUM)

`crates/devflow-cli/src/commands.rs:119-122` — `start`'s `dry_run` branch
calls `print_dry_run(&state)` and returns **immediately**, before
`ensure_agent_binary(agent_program(agent))` at line 126 and before any
worktree/branch/launch work. Confirmed by direct read of `start`'s body
(see the block quoted above at those exact line numbers) — read this
file yourself if the claim above is doubted, do not take it on faith.

That bounds the claim two ways:

- **Structural (proven above):** the dry run resolves and names all five
  pipeline stages against this scratch repo, and `devflow doctor` finds
  no blocking condition. This proves stage resolution and pipeline
  shape — nothing more.
- **Runtime, partial (proven above):** `claude --version` exits `0` with
  no auth prompt. This proves the `claude` binary is present and
  authenticated. It does **not** prove that a GSD slash command resolves
  inside a real Claude Code invocation, or that Claude Code can actually
  drive this scratch repo through a stage. That is a behavioral claim,
  and only a real (non-dry-run) launch can produce it — that proof
  belongs to `23-02-PLAN.md`'s probe run, the only place it can come
  from.

This record does not claim, anywhere above, that the dry run walked the
full pipeline start to finish — only that it named the five stages
structurally.

# Replicating DevFlow's Dev Setup & Workflow — Checklist

Verified live against this repo and this machine on 2026-08-04 (not reconstructed from memory).
Each item is tagged:

- **[GLOBAL]** — lives on this machine outside any repo; reusable as-is on any project, but
  currently **not version-controlled anywhere** (see the gap at the end).
- **[PROJECT]** — specific to DevFlow (Rust/crates.io); needs adapting per language/stack.
- **[TRACKED]** — already captured as a file *in this repo*, pulled in with one setup command.
  These are the parts of "replicate the setup" that are already solved — copy the mechanism.
- **[PATTERN]** — the *mechanism* is generic and worth keeping; the *content* is project-specific.

**Note on scope:** this document overlaps with `CONTRIBUTING.md`, which is the authoritative,
contributor-facing source for everything project-specific below. This document exists for a
different purpose — extracting the *pattern* for reuse on other projects — not to duplicate it.
Where the two diverge, `CONTRIBUTING.md` wins; update it first, then this file.

---

## 1. Git branch model & signing — mostly already [TRACKED]

- [ ] **[TRACKED]** A tracked [`.gitconfig`](../.gitconfig) at repo root, pulled in with one
  command:
  ```bash
  git config --local include.path ../.gitconfig
  ```
  This is the actual answer to "how do I version-control git settings" from the earlier
  conversation — it's already solved *for git config specifically*, just not yet generalized.
  It sets:
  - `gpg.format = ssh`, `commit.gpgsign = true`, `tag.gpgsign = true` — SSH-based signing, not
    GPG, for every contributor.
  - `[gitflow "branch"]` (`master = main`, `develop = develop`) and `[gitflow "prefix"]`
    (`feature/`, `release/`, `hotfix/`, `support/`, `v`) — the full git-flow model, so
    `git flow feature start <name>` works correctly without ever running `git flow init -d`
    (whose defaults assume a `master` branch this repo doesn't have).
  - Deliberately **excludes** anything naming a specific person, key file, or home directory —
    `.gitconfig`'s own header comment states this explicitly, since the repo is public.
- [ ] **[PROJECT, local-only by design]** `user.signingkey` — per-contributor, set locally, never
  tracked. On this machine: the agent's own key (`~/.ssh/github_ed25519.pub`), so agent commits
  are attributable to the agent rather than impersonating the maintainer.
- [ ] **[PATTERN, local-only by design]** `devflow.releaseSigningKey` — a **second signing key,
  for release tags and `main` only**, set once locally (`git config --local
  devflow.releaseSigningKey ~/.ssh/<key>.pub`), enforced by the pre-push hook (§3) by comparing
  key *fingerprints* (not the signer string — both keys share `user.email`, so the string alone
  can't distinguish them). Unset = no enforcement, so non-release contributors need nothing.
- [ ] **[GLOBAL]** `gh auth status` — GitHub CLI authenticated via keyring, scopes include
  `repo`, `workflow`, `admin:ssh_signing_key`.

## 2. GitHub repository settings (not files — verify via `gh api`, not by reading the repo)

- [ ] **[PROJECT]** Default branch: `main`.
- [ ] **[PROJECT]** Merge button config: squash ✓, merge-commit ✓, rebase ✗.
  `squash_merge_commit_title: PR_TITLE`. `delete_branch_on_merge: true`.
- [ ] **[PROJECT]** Branch protection on `main`: required status checks `Test`, `Clippy`,
  `Format` (app-scoped, `strict: false`), `required_approving_review_count: 0`,
  `enforce_admins: true`, force-push and deletion both disabled.
- [ ] **[PROJECT]** Branch protection on `develop`: force-push/deletion disabled,
  `enforce_admins: true`, **no required status checks or review count set** (lighter than `main`).
- [ ] Dump/apply commands, for portability:
  ```bash
  gh api repos/OWNER/REPO/branches/main/protection
  gh api repos/OWNER/REPO --jq '{default_branch,allow_squash_merge,allow_merge_commit,allow_rebase_merge,delete_branch_on_merge,squash_merge_commit_title}'
  ```
  **No official `gh` subcommand applies these from a file** — this is the concrete gap from the
  "portable setup" conversation. A small script wrapping `gh api -X PUT .../protection` with a
  JSON payload is the realistic fix if you want this replicated automatically, not just documented.

## 3. Local git hooks — `core.hooksPath`, two layers

- [ ] **[PROJECT]** Repo-local: `git config core.hooksPath scripts/hooks` (tracked in-repo, so
  every clone gets it after one `git config` line — see CONTRIBUTING.md for the exact install
  step in this repo).
- [ ] **[PATTERN]** `scripts/hooks/pre-commit` and `post-commit` **chain to whatever hook the
  global `core.hooksPath` already pointed at**, rather than silently replacing it — critical
  because setting a repo-local `core.hooksPath` otherwise disables any global hook wholesale
  (secret scanners, etc.). `pre-commit` additionally **refuses to commit directly on
  `develop`/`main`** (both PR-protected, so the commit would be rejected at push time), with no
  override — the fix is `git switch -c <branch>` first.
- [ ] **[PROJECT]** `scripts/hooks/pre-push`:
  - Scrubs `GIT_DIR`/`GIT_WORK_TREE`/etc. before doing anything (prevents a worktree push from
    retargeting the wrong repo — a real incident here, `999.37`).
  - Enforces the release-signing-key policy from §1 for tags and refuses direct pushes to `main`.
  - Runs the **full CI-equivalent check inside the pinned container** (§5), not host tools —
    `scripts/check-in-container.sh all`, with `DEVFLOW_SKIP_CONTAINER_CHECK=1` as an explicit,
    loudly-non-equivalent escape hatch.
- [ ] **[PROJECT]** `scripts/hooks/post-commit`: warns (never auto-edits) when a `*-SUMMARY.md`
  lands but `STATE.md`'s authored prose wasn't updated to match; also self-heals the
  `UPSTREAM-GSD-ISSUES.md` symlink (§8) if `git clean -fdx` deleted it.
- [ ] **[PROJECT]** `scripts/hooks/post-commit` also warns when a dev-setup file changes without
  this checklist moving too. Its path lives in one variable, `checklist_path`, at the top of that
  block — **if this file ever moves again, change that variable**. From the move into
  `.planning/user/` until 2026-09-02 the hook still matched the old `.planning/` path, so the
  check could never pass: it warned on every setup commit even when the checklist *was* updated,
  and no input could silence it. A guard now fails loudly when `checklist_path` names a file that
  does not exist, so the next move surfaces immediately instead of degrading to noise.
- [ ] **[GLOBAL]** Machine-wide `core.hooksPath` at `~/.config/git/hooks/pre-commit` runs
  **gitleaks** (`gitleaks protect --staged --verbose`, brew-installed, v8.30.1) on every commit
  across every repo, then chains to the repo-local hook if one exists. This is what the
  repo-local pre-commit hook (above) is careful not to shadow.

## 4. GitHub Actions CI

- [ ] **[PROJECT]** `.github/workflows/ci.yml` — three required jobs (`Test`, `Clippy`, `Format`),
  each running **inside the exact same pinned container image** the devcontainer and the
  pre-push hook use (§5) — the whole point being zero host/CI drift.
- [ ] **[PATTERN]** A dedicated `scripts/assert-image-parity.sh` step fails the build if the
  image tag in CI drifts from `.devcontainer/devcontainer.json` — because GitHub Actions can't
  interpolate `env` into `jobs.*.container.image`, so the tag is duplicated by hand and needs an
  explicit guard against silent rot.
- [ ] **[PROJECT]** `.github/workflows/devcontainer.yml` (separate workflow — check its trigger
  and purpose if replicating).
- [ ] **[PROJECT]** `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/*.md`.
- [ ] No CODEOWNERS file in this repo (confirmed absent) — not part of the current setup.

## 5. Devcontainer / toolchain pinning

- [ ] **[PROJECT]** `.devcontainer/devcontainer.json` — pins `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`
  explicitly (never a floating tag), persists cargo registry + `target/` as named Docker volumes.
- [ ] **[PROJECT]** `rust-toolchain.toml` — pins an **exact** rustc version (`1.97.1`), not a
  floating `channel = "stable"`, for the same drift-prevention reason as the image pin.
- [ ] **[PATTERN]** `scripts/check.sh` (runs fmt/clippy/tests) and
  `scripts/check-in-container.sh` (the same, but launched inside the pinned image) — the single
  source of truth CI, the pre-push hook, and a developer's manual check all call into.
- [ ] **[PATTERN]** `check-in-container.sh` is WORKTREE-AWARE (phase 41, HYG-02): when the
  invocation runs from a git worktree, its `.git` is a FILE pointing at
  `<main>/.git/worktrees/<N>` and the COMMON gitdir (`<main>/.git`) is a second path outside
  the mount — both are bind-mounted at their absolute host paths (detected via
  `git rev-parse --absolute-git-dir` + `--git-common-dir`). A fresh replication that runs
  phase work from `.worktrees/`-style worktrees must keep this or git dies with
  `fatal: not a git repository` inside the container.
- [ ] **[PATTERN]** `check-in-container.sh` derives the `devflow-ci-target` / `devflow-ci-registry`
  volume names PER CHECKOUT (a hash of `REPO_ROOT` suffix). A single shared volume mounted at
  the same `/workspace` path aliases cargo's path-keyed fingerprints across the main checkout
  and worktrees — the worktree build reuses a stale `devflow-core` rmeta and dies with
  `E0599: no variant named Antigravity` (false RED), and the inverse yields a false GREEN.

## 6. Claude Code / agent tooling — mostly [GLOBAL], currently unversioned

- [ ] **[GLOBAL]** `~/.claude/CLAUDE.md` is a **symlink** to `~/.config/agents/AGENTS.md` — your
  actual global instructions file (RULE ZERO, reporting style, coding methodology, etc.).
- [ ] **[GLOBAL]** `~/.claude/rules/*.md` — `git-workflow.md`, `code-style.md`, `context7.md`,
  `effort-routing.md`. Loaded into every session automatically.
- [ ] **[PROJECT]** `CLAUDE.md` at repo root — repo-specific constraints only (this repo's own
  says explicitly: "Global rules live in `~/.config/agents/AGENTS.md`; this file holds only
  constraints specific to how *this* repository is worked on").
- [ ] **[GLOBAL]** GSD-core installed as a global npm package (`@opengsd/gsd-pi`, currently
  `1.12.0`), not vendored per-repo — one install serves every project.
- [ ] **[PROJECT]** `.planning/config.json` — per-project GSD config (branching strategy, phase
  branch template, model overrides, feature toggles). This is the part that's genuinely
  project-specific even though the GSD engine itself is global.
- [ ] **[GLOBAL]** MCP servers registered via `claude mcp list` (Linear, Google Drive/Gmail/
  Calendar, GitHub, plus two currently broken — `gsd-workflow`, `gsd-browser` — worth fixing or
  dropping if replicating cleanly rather than copying the breakage).
- [ ] **[GLOBAL, currently empty]** Auto-memory directory convention:
  `~/.claude/projects/<project-path>/memory/` — per-project persistent memory, keyed by encoded
  filesystem path, so it naturally travels with wherever the project lives on disk.
- [ ] **[PROJECT, enabled 2026-08-04, curation widened 2026-08-07]** MemPalace —
  `.planning/config.json`'s `mempalace` block: `enabled: true`, `capture_artifacts: true`,
  `wing: "devflow"`, `memory_mode: "augment"`, `recall_on_plan: true`, plus as of 2026-08-07
  `mirror_kg: true`, `diary_journal: true`, `cross_project_tunnels: true` — the three `ship:post`
  curation tasks the `gsd-mempalace-curator` agent independently gates on (diary entry per phase,
  extract-learnings → temporal-KG mirror, cross-project tunnel proposals; a fourth,
  wing-scoped-prune, is unconditional). Requires the `mempalace` CLI on `PATH` (this machine:
  v3.6.0, installed via `~/.local/bin/mempalace`) — no MCP server registered yet, so transport
  falls back to CLI-only, which the GSD skills support as a first-class path, not a degraded one.
  **The palace itself starts empty** (`mempalace status` reported no palace at
  `~/.mempalace/palace` as of enabling) — capture accrues from new phases going forward; nothing
  was backfilled from this repo's existing `.planning/` history.
- [ ] **[PROJECT, enabled 2026-08-07]** Broken-windows ship gate — `.planning/config.json`'s
  `workflow.windows_enforce: true`. Blocks `/gsd-ship` while `.planning/WINDOWS.md`'s
  `open_count > 0`, and fails closed (blocks) if the ledger's frontmatter counts don't match its
  own entries rather than trusting an unreadable/inconsistent count — caught and fixed once
  already (frontmatter claimed 1 open when the entries showed 0, stale since a Phase 28 fix landed
  without a recount). Resolve items with `gsd-tools windows fixed <id>` or
  `gsd-tools windows waive <id> "<reason>"` (reason required); no automatic recompute verb exists,
  so a frontmatter/entries mismatch needs a manual counter fix.
- [ ] **[PROJECT, enabled 2026-08-05]** TDD capability — `.planning/config.json`'s
  `workflow.tdd_mode: true`. The `tdd` capability ships installed/enabled/active by default, but
  **both** its hooks gate on this key, so it is inert until the key is set: a `plan:pre`
  *contribution* (injects test-first guidance into plan authoring) and an `execute:post` *gate*
  (the end-of-phase TDD review). Enabled from Phase 34 onward; Phases ≤33 predate it and carry no
  `gate_status:` commit trailers, which is why `/gsd-ship`'s TDD Audit section self-suppressed on
  PR #90. **Advisory, not blocking, on its own** — the `execute:post` gate is declared
  `blocking: false`, so a violation prints a table and execution continues. It escalates to
  genuinely blocking only under **MVP+TDD** (`workflow.mvp_mode: true` as well), which additionally
  arms a per-task RED-commit gate: every behavior-adding task must be preceded by a
  `test(NN-PP):` commit touching a test path, or execution halts. `mvp_mode` is **not** set on
  this project, so today's posture is guidance + advisory review.

## 7. GSD planning structure (`.planning/`)

- [ ] **[PROJECT]** `PROJECT.md`, `ROADMAP.md`, `STATE.md`, `REQUIREMENTS.md` (recreated fresh
  per milestone), `phases/`, `milestones/` (archives), `debug/` (+ `debug/resolved/`),
  `superseded/`, `codebase/` — the standard GSD document set; content is project-specific but the
  *shape* is entirely reusable via `/gsd-new-project`.
- [ ] **[PATTERN]** `.planning/UPSTREAM-GSD-ISSUES.md` as a **symlink** into a sibling
  `gsd-core-personal-workspace` checkout (`../../gsd-core-personal-workspace/scratch/UPSTREAM-GSD-ISSUES.md`)
  — gitignored, self-healed by the post-commit hook (§3). Only makes sense if you keep a local
  `gsd-core-personal-workspace` source checkout alongside your projects; otherwise this is
  DevFlow-specific plumbing to skip.
- [ ] **[PROJECT]** Linear integration: team `Denniskim`, project `DevFlow` — ROADMAP.md backlog
  items are meant to stay in sync with Linear issues (per your own stated convention).

## 8. Language/build tooling — [PROJECT], Rust-specific

- [ ] Cargo workspace: `devflow-core` (lib) + `devflow-cli` (bin), zero network-dependency
  policy (serde, clap, thiserror, tracing only).
- [ ] `~/.cargo/credentials.toml` exists (crates.io publish auth) — contents not inspected here;
  needed to reproduce the publish step, not the dev-loop itself.
- [ ] Publish order matters and is enforced by convention, not tooling: `devflow-core` before
  `devflow`.
- [ ] **[GLOBAL / PROJECT]** Rust AI harness tooling: `cargo-deny` (`deny.toml`), `cargo-machete`,
  `cargo-semver-checks`, `cargo-mutants`, `cargo-llvm-cov`, `cargo-nextest`, and `miri` (via nightly).


## 9. Token/usage tracking (from earlier conversation, for completeness)

- [ ] **[GLOBAL]** `/usage` slash command — current-session token/cost, live in Claude Code.
- [ ] **[GLOBAL]** `npx ccusage@latest claude session` — historical per-session token/cost
  across all projects, reading Claude Code's own JSONL transcripts
  (`~/.claude/projects/<project>/*.jsonl`). Verified working on this machine.

---

## The actual gap, restated plainly

§1's `.gitconfig` + `include.path` is the template: a small tracked file, pulled in with one
setup command, deliberately scrubbed of anything person- or machine-specific. That pattern isn't
generalized yet.

Everything tagged **[GLOBAL]** above already applies to every project on this machine — but
**none of it is version-controlled**. `~/.claude/`, `~/.config/agents/`, and
`~/.config/git/hooks/` are not git repos. If this machine were lost or a new one set up, none of
this would come back automatically. That's the concrete first task for the "capture and
replicate" mini-project discussed earlier: a dotfiles-style repo covering the GLOBAL-tagged items
above, plus a small `gh api`-based script for the GitHub branch-protection settings in §2, which
have no file representation to version-control in the first place — `.gitconfig` can't help there
since GitHub's branch protection lives in the platform's settings, not in anything git tracks.

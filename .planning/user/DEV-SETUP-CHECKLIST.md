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
- [ ] **[PROJECT]** `scripts/hooks/pre-commit` also **refuses a staged `*PLAN.md` whose
  `<automated>` verify command uses bash-only syntax** (`${PIPESTATUS[...]}`, any `${name[N]}`
  index, `${BASH_REMATCH[N]}`, `${v,,}`/`${v^^}`, `declare -A`) without wrapping it in `bash -c`.
  GSD executor subagents run their Bash tool under **zsh**, where `PIPESTATUS` does not exist —
  `${PIPESTATUS[0]}` expands to the empty string, so `echo "exit=${PIPESTATUS[0]}"` prints `exit=`
  and every assertion about that exit code passes without ever reading it. That is a green check
  over an unread result, the same class as the `rg -c` dead gate in CLAUDE.md. All three phase-46
  plans shipped with it and each executor rediscovered it independently on the clock.
- [ ] **[PATTERN]** **The scan is a standalone, tested script the hook DELEGATES to** —
  `scripts/lint-plan-bashisms.sh`, exercised by `crates/devflow-cli/tests/plan_bashism_scanner.rs`
  (11 cases, run by `cargo test` and therefore by every CI job through `scripts/check.sh`). A hook
  is early feedback; the test suite is the enforcement. The first version of this guard was inline
  in the hook and **untested**, and an external review (46-REVIEWS.md C-04) bypassed it twice by
  *actually committing through it*.
- [ ] **[PATTERN]** **The allow rule is CONTAINMENT, not ordering, and that distinction is the
  whole guard.** A bashism is permitted only when it lies inside the single-quoted region that a
  `bash -c '` opened; single quotes do not nest in POSIX shell, so the region boundary is
  unambiguous. Four cases, all now regression fixtures — measured against the old hook, which
  **committed all four**:
  - `bash -c 'echo "${PIPESTATUS[0]}"'` — accepted; genuinely runs under bash.
  - `echo "${PIPESTATUS[0]}" # bash -c` — refused; a trailing comment opens no region. The old
    hook's `grep -v 'bash -c'` dropped any line *containing* the substring anywhere.
  - `bash -c ':'; echo "${PIPESTATUS[0]}"` — refused; the region closed before the bashism, which
    runs in the outer shell. An ordering rule ("`bash -c` appears first") accepts this.
  - `bash -c "echo ${PIPESTATUS[0]}"` — refused; double quotes open no single-quoted region and
    the outer shell expands before bash starts. Ordering accepts this too.
- [ ] **[PATTERN]** **Parse BLOCKS, not lines, and treat an unterminated opening tag as an error.**
  A line-based scan cannot see inside a multi-line `<automated>` block, which was the first proven
  bypass; and a parser that matches only *complete* blocks is bypassed by construction — open a
  tag, never close it, and there is nothing to check. Both are fixtures.
- [ ] **[PATTERN]** **List staged files with `--cached --name-only -z --diff-filter=ACM` and
  consume with `read -r -d ''`.** Without `-z`, git QUOTES a path containing non-ASCII or a tab and
  the subsequent `[ -f "$path" ]` fails, skipping the file *silently*. Keep `--diff-filter=ACM` in
  the same breath: a deleted plan has no content to scan and reintroduces the silent skip from the
  other end. A named path that cannot be read is an **error**, never a skip — a check reporting
  success having inspected nothing is the failure class the guard exists to prevent.
- [ ] **[PATTERN]** **When probing a hook end-to-end in a scratch repo, attribute the refusal.**
  `git commit` failing proves only that *some* hook objected. The first run of this repo's own
  probe read `legit=WRONGLY_REFUSED` and the cause was the **commit-msg** Conventional Commit
  validator rejecting the message `t3` — nothing to do with the guard under test. Use a conforming
  commit message, choose a branch and paths that trip neither the protected-branch nor the
  personal-artifact arm, and grep the captured output for the specific guard's own text.
- [ ] **[PROJECT]** `scripts/hooks/commit-msg`: Conventional Commit validator — enforces
  `<type>(<scope>)!: <description>` against the allowed type list, refuses a subject ending in a
  period, warns (does not refuse) past 72 chars, and exempts merge/fixup/squash subjects.
  `DEVFLOW_ALLOW_ANY_COMMIT=1` is the escape hatch for automated tooling. Chains to the global
  hook, then to a displaced repo-local one, the same way `pre-commit`/`post-commit` do.
- [ ] **[PATTERN]** **Every hook file must be committed mode `100755`, and that is worth checking
  rather than assuming.** A hook git tracks as `100644` is silently skipped — git prints only a
  one-line `hint:` about the file not being executable, which is easy to miss in a wall of commit
  output, and *nothing else* indicates the gate is not running. This repo shipped `commit-msg` as
  `100644` from its introduction until 2026-09-03, so Conventional Commit validation was inert
  that whole time. Verify with `git ls-files -s scripts/hooks/` — the mode in the index is what
  propagates to other clones, so `chmod +x` alone is not enough; the mode change has to be
  committed. This is the same class as a gate that runs but cannot fail (see the `rg -c` dead-gate
  note in CLAUDE.md): documenting a check is not the same as it executing.

  **Root cause on this machine: `core.fileMode` is `false` in this repo, so `chmod +x` is invisible
  to git and stages nothing.** That is why the hook was committed non-executable in the first
  place, and it will silently do the same to the next hook added. The fix for an individual file is
  `git update-index --chmod=+x <path>`, which writes the mode into the index directly. Do not
  "fix" this by flipping `core.fileMode` to `true` on a whim — it is commonly set false to suppress
  spurious mode churn on filesystems that do not preserve the bit, and turning it on can produce a
  large unrelated diff. Prefer `update-index` per hook, and verify with `git ls-files -s`.
- [ ] **[PROJECT]** `scripts/hooks/pre-push`:
  - Scrubs `GIT_DIR`/`GIT_WORK_TREE`/etc. before doing anything (prevents a worktree push from
    retargeting the wrong repo — a real incident here, `999.37`).
  - Enforces the release-signing-key policy from §1 for tags and refuses direct pushes to `main`.
  - Runs the **full CI-equivalent check inside the pinned container** (§5), not host tools —
    `scripts/check-in-container.sh all`, with `DEVFLOW_SKIP_CONTAINER_CHECK=1` as an explicit,
    loudly-non-equivalent escape hatch.
- [ ] **[PROJECT]** `scripts/hooks/post-commit`: warns (never auto-edits) when a `*-SUMMARY.md`
  lands but `STATE.md`'s authored prose wasn't updated to match.
- [ ] **[PROJECT]** `scripts/hooks/post-commit` also warns when a dev-setup file changes without
  this checklist moving too. Its path lives in one variable, `checklist_path`, at the top of that
  block — **if this file ever moves again, change that variable**. From the move into
  `.planning/user/` until 2026-09-02 the hook still matched the old `.planning/` path, so the
  check could never pass: it warned on every setup commit even when the checklist *was* updated,
  and no input could silence it. A guard now fails loudly when `checklist_path` names a file that
  does not exist, so the next move surfaces immediately instead of degrading to noise.
  Note the block resolves its own `checklist_root` — the script's `root` variable is assigned
  only inside the SUMMARY.md branch and is unbound elsewhere under `set -u`.
- [ ] **[GLOBAL]** Machine-wide `core.hooksPath` at `~/.config/git/hooks/pre-commit` runs
  **gitleaks** (`gitleaks protect --staged --verbose`, brew-installed, v8.30.1) on every commit
  across every repo, then chains to the repo-local hook if one exists. This is what the
  repo-local pre-commit hook (above) is careful not to shadow.

## 4. GitHub Actions CI

- [ ] **[PROJECT]** `.github/workflows/ci.yml` — **four** jobs, each running **inside the exact
  same pinned container image** the devcontainer and the pre-push hook use (§5) — the whole point
  being zero host/CI drift. Three (`Test`, `Clippy`, `Format`) are **required** status contexts;
  the fourth (`Sequential 2-CPU check`) is deliberately **advisory**. The **fourth required
  context is `Build + test in devcontainer`, which lives in `devcontainer.yml`, not here** — so
  the required set spans two workflow files, and reading only this one under-counts it.
- [ ] **[PATTERN]** **Any container job whose `run:` steps use bash syntax must declare
  `shell: bash`.** GitHub defaults a `run:` step to `bash -e {0}` only on a *bare runner*; inside
  a `container:` job it is `sh -e {0}`, and these Debian images ship dash as `/bin/sh`. So
  `set -euo pipefail` is a hard error there (`set: Illegal option -o pipefail`, exit 2) and other
  bashisms (`[[`, `${v,,}`, arrays) fail *quietly*. `Sequential 2-CPU check` shipped without it
  and died on line 1 of its first two live runs on PR #208 — 42s to red, having executed none of
  the suite, while presenting as an ordinary failing check rather than a misconfiguration.
  Enforced job-scoped (not file-scoped) by `container_jobs_using_bash_syntax_declare_a_bash_shell`
  in `crates/devflow-cli/tests/ci_parity_guards.rs`; a file-scoped check would pass with the
  declaration sitting on the wrong job. Same false-green family as the zsh `${PIPESTATUS[0]}`
  hazard in CLAUDE.md.
- [ ] **[PATTERN]** **A workflow guard must be a FUNCTION over text, driven by fixtures — not a
  `contains` search over the live file.** A guard that can only ever run against the real workflow
  cannot be made to fail on demand, so it is only ever observed passing, which is not evidence.
  Measured here: with the pinned suite step commented out, the whole-file version still reported
  `1 passed`; a decoy job supplying the tokens passed it too. The replacement
  (`split_jobs` + `recognise_pinned_sequential_job`, same file) scopes to the target job's own body
  and is exercised by five fixtures in `crates/devflow-cli/tests/fixtures/ci-parity/` — one
  positive control plus four negative. Two details that are load-bearing rather than stylistic: the
  splitter anchors to the top-level `jobs:` key (otherwise `push:`/`pull_request:` under `on:` parse
  as jobs — six headers for four real jobs, which made an `>= 4` anti-vacuity assert satisfiable by
  two non-jobs), and it reads RAW lines because indentation is the only thing separating a top-level
  key from a nested one, so a trim-everything helper destroys the signal before use.
- [ ] **[PATTERN]** The CI CPU pin has exactly **one definition site**, `scripts/lib/ci-cpus.sh`.
  Both consumers source it — `scripts/check-in-container.sh` (the pre-push gate) and the
  `Sequential 2-CPU check` job — rather than re-typing the value, because a second copy lets the
  local gate and CI measure different load shapes while both report green. Same class as the
  image-tag duplication above, and enforced the same way: a test
  (`cpu_pin_has_exactly_one_definition_site`) rather than a "keep in sync" comment.
- [ ] **[PATTERN]** The `DEVFLOW_CI_CPUS=all` escape hatch is decided in **one function**,
  `cpu_pin_prefix()` in that same fragment, which sets an argv-prefix array `CPU_PIN` — empty for
  `all`, `(taskset -c "$CPUS")` otherwise. All three consumers call it: the pre-push gate and
  **both** pin sites in the `Sequential 2-CPU check` job. It builds an array rather than running
  the command because the gate needs the value as a prefix inside `docker run`, where a host-shell
  function does not exist. A shell function cannot cross a GitHub Actions step boundary, so each CI
  step sources the fragment again — that is the one definition site being read twice, not a second
  copy. The value of doing it this way is measured, not stylistic: while the conditional existed
  only in the local gate, `DEVFLOW_CI_CPUS=all` passed locally and killed CI with
  `taskset: failed to parse CPU list: all` (exit 1) — a parity break inside the pair of files whose
  only purpose is parity.
- [ ] **[PATTERN]** **A negative control must ASSERT, and the decider must not be the measurer.**
  The `Sequential 2-CPU check` job compares `nproc` unpinned against `nproc` under the pin, and
  the comparison lives in `scripts/assert-cpu-pin.sh <cpu_list> <unpinned> <pinned>` — a script
  that takes all three values as ARGUMENTS and measures nothing. That split is what makes the
  failing direction runnable: a decider inlined in the workflow can only ever be observed on a
  runner that happens to disagree with itself, so its interesting branch never executes. It
  shipped originally as three bare `echo`s under a comment calling one count the negative control
  for the other, which is the shape to watch for — documenting a gate is not implementing one.
  The assertion is conditional (skipped when the list is `all`, and on a <= 2-CPU host where a
  2-CPU pin cannot narrow anything) and **both skips print their reason**, because a silent skip
  and a pass look identical in a job log. Four decision cases are unit-tested in
  `crates/devflow-cli/tests/ci_parity_guards.rs`. What a pass does NOT establish: that rustc or
  the harness threads were confined, or that the runner had 2 cores — only that the pin narrowed
  the count visible to one child process. Affinity inheritance across fork/exec is the mechanism
  relied on, and `nproc` does not measure it.
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
- [ ] **[PATTERN]** **`~/.gsd/defaults.json` is NOT a fallback layer under a project.** Verified
  2026-09-03: a repo containing `.planning/` but no `config.json` inherits *nothing* from the
  global file — every key returns not-found, `commit_docs` included, even though the global sets
  it. The global applies only in the loader's Branch D, which requires **no `.planning/` directory
  at all** (so: `new-project` bootstrap, and GSD commands run outside a project). The lone
  exception is the `effort` block, which the install-time effort sync does merge globally — which
  is why `effort` is excluded from the shadowed-key warning. Consequences when replicating:
  - Never "align" a project with the global by **deleting** the project config. You inherit
    nothing and lose the project-only blocks (`git.*`, `mempalace`, `model_overrides`,
    `review.default_reviewers`, `intel`) that have no global counterpart.
  - Never align by **copying** the global verbatim either. The two files use different shapes:
    the global puts `granularity`, `parallelization`, `runtime`, `resolve_model_ids` and `effort`
    at the top level. Copy blindly and you import values at paths that are never read, believing
    you changed something.
  - Validate values against `gsd-core/references/planning-config.md` before porting. Three invalid
    values were found here in one pass: `granularity: "medium"` (valid: `coarse`/`standard`/`fine`
    — it had been silently falling back to the standard band), `discuss_mode: "exploratory"`
    (valid: `discuss`/`assumptions`), and `subagent_timeout: 900` against a **milliseconds**
    schema whose default is `300000`.
  - When porting `effort`, carry `routing_tier_defaults` across with it. Creating an `effort` block
    without them silently kills the tier defaults.
  - `05e264d` (2026-09-01) hit this exact trap the other direction: it copied `skip_discuss`,
    `research_before_questions` and `use_worktrees` **out of** `~/.gsd/defaults.json` into this
    project's `.planning/config.json`, believing they were "already in effect as defaults" — but
    the global file does not layer here, so the effective values had been GSD's built-ins
    (`skip_discuss: false`, `research_before_questions: false`). `use_worktrees` was reverted to
    `false` in `dc66867`; `skip_discuss` was reset to `false` (discuss runs) on 2026-09-07, and
    the global default corrected to match. This project deliberately **runs** the discuss phase.
- [ ] **[GLOBAL]** MCP servers registered via `claude mcp list` (Google Drive/Gmail/Calendar,
  GitHub, plus two currently broken — `gsd-workflow`, `gsd-browser` — worth fixing or dropping if
  replicating cleanly rather than copying the breakage). A Linear server may still be registered;
  it is **not** part of this project's workflow any more (see §7) and does not need replicating.
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
- [ ] **[PROJECT]** **Upstream GSD-core defects are filed as GitHub issues against
  `@opengsd/gsd-core`**, in that project's own tracker — not held in a local document. This is
  already the rule CLAUDE.md states for bypassing a known-broken GSD command: the bypass is
  legitimate only when the defect is filed upstream. File it there, then reference the upstream
  issue number from the ROADMAP entry or DevFlow issue that hit it.

  **Retired 2026-09-03:** `.planning/UPSTREAM-GSD-ISSUES.md` — a gitignored symlink into a sibling
  `gsd-core-personal-workspace/scratch/` checkout. Nothing replicating this setup should recreate
  it. Two notes for anyone reading older commits that reference it: the symlink was already absent
  from `.planning/` when this entry was retired, and the claim elsewhere in this checklist that
  `scripts/hooks/post-commit` self-healed it was **never true of the shipped hook** — verified
  2026-09-03, zero occurrences of `upstream` or `symlink` in that file, against a control showing
  the behaviours the hook *does* have (`checklist_path`, `SUMMARY`) present 6 times each. So the
  document had no automated upkeep and drifted unnoticed; that is a large part of why it is being
  replaced by a tracker that other people can see.

  Numbering tells the two trackers apart at a glance: DevFlow issues are low (≤ #206 as of
  2026-09-03); gsd-core issue numbers are in the thousands and turn up inside warning strings
  emitted by the installed package (e.g. the `#3532` shadowed-global-defaults diagnostic). A
  four-digit number in GSD output is never one of yours.
- [ ] **[PROJECT]** `.planning/config.json` sets **`workflow.use_worktrees: false`**, deliberately
  (2026-09-05). This is not a preference — on Claude Code, GSD executor worktrees are forked from
  `origin/HEAD` and the harness ignores the project's `worktree.baseRef` (#48, upstream
  claude-code#44965). A phase branch is by definition ahead of `origin/HEAD` for its whole life,
  so `worktree.base-check` auto-degrades to sequential on **every** phase — parallel executor
  worktrees are structurally unavailable here, not merely unavailable today.

  Declaring the degrade in config rather than discovering it per-dispatch is what makes it
  survivable. The isolation sentinel that records a shell-computed degrade carries a **10-minute
  TTL** (`hooks/lib/isolation-sentinel.js`), and `execute-phase.md` writes it once at `initialize`
  and never refreshes it — so any executor running longer than 10 minutes stales it and the *next*
  plan's dispatch is denied (filed upstream as gsd-core#4317). `workflow.use_worktrees: false` is
  the one degrade the guard re-derives from config on a stale or absent sentinel, so it is immune
  to both that TTL and the re-query clobber of gsd-core#4222. Verified: with the sentinel deleted,
  `gsd-tools query dispatch-isolation --raw` returned `harness-worktree` before the change and
  `none` after.

  Consequence to know about: sequential dispatch has no hard-pin to the orchestrator's worktree
  root (gsd-core#4254), so an executor subagent can spawn with cwd on the *primary checkout* and
  commit to the wrong branch silently. Until that lands upstream, every executor dispatch must
  carry the orchestrator's absolute worktree path and a branch assertion in its prompt — see
  CLAUDE.md.
- [ ] **[PROJECT]** Issue tracking is **GitHub Issues only** (`denniyahh/devflow`). ROADMAP.md
  `999.x` backlog entries are the primary record; mirror one to a GitHub issue when it needs to be
  visible outside the repo. **Linear is no longer used** (retired 2026-09-02). Historical
  `**Linear:** [DEN-nnn]` links in ROADMAP.md and STATE.md are left in place deliberately — they
  record where an item was tracked at the time, and rewriting them would falsify the history.
  Migrated items already read `**GitHub:** [#nnn] (migrated from Linear DEN-nnn)`; follow that
  form if you migrate more.

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

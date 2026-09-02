# Phase 45: Unattended Auto-Mode Hardening (999.110 + 999.109 + 999.94) - Pattern Map

**Mapped:** 2026-09-01
**Files analyzed:** 7 to be modified (+2 doc files conditionally)
**Analogs found:** 7 / 7 (all in-repo; this is a modify-existing phase, no new files)
**Upstream inputs:** `45-CONTEXT.md`, `45-REVIEWS.md` (no RESEARCH.md — `--reviews` replan)

> Every path named below was verified present with `git ls-files`. No gitignored
> mirror paths appear in this document.

---

## Headline Finding (answers REVIEWS finding 1 directly)

**The GitFlow lifecycle already reads a config value — but that config is unreachable.**

`GitFlow` holds a private `config: GitFlowConfig` field and **every** develop-consuming
method reads `self.config.develop`, not a literal `"develop"`:

| Method | File:line | Reads |
|--------|-----------|-------|
| `feature_start` | `crates/devflow-core/src/git.rs:133` | `self.config.develop` |
| `feature_start_force` | `crates/devflow-core/src/git.rs:142` | `self.config.develop` |
| `merge_feature_into_develop` | `crates/devflow-core/src/git.rs:161` | `self.config.develop` |
| `is_merged_into_develop` | `crates/devflow-core/src/git.rs:177` | `self.config.develop` |
| `release_finish` | `crates/devflow-core/src/git.rs:200`, `:209` | `self.config.main`, `self.config.develop` |
| `delete_branch` (protection) | `crates/devflow-core/src/git.rs:234` | `self.config.main`, `self.config.develop` |
| `cleanup_merged` | `crates/devflow-core/src/git.rs:317-318` | `self.config.develop` (both the `--merged` arg and the protected list) |
| `divergence_from_develop` | `crates/devflow-core/src/git.rs:397` | `self.config.develop` |

**But `GitFlow::new` is the only constructor and hardcodes the default:**

```rust
// crates/devflow-core/src/git.rs:116-124
impl GitFlow {
    /// Create a git-flow helper for a project root, using the hardcoded
    /// git-flow constants (`main`, `develop`, `feature/`).
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            config: GitFlowConfig::default(),
        }
    }
```

**Explicitly ABSENT** (searched, does not exist under any name): there is no
`GitFlow::with_config`, `GitFlow::with_git_flow`, or any other constructor taking a
`GitFlowConfig`. `grep`ping the workspace for `GitFlowConfig` returns only
`::default()` constructions plus by-reference parameters into `agent_result`.

So the planner's threading job for D-01 is **one new constructor plus call-site
audit**, not a rewrite of the lifecycle methods. That is materially cheaper than
REVIEWS finding 1 assumed.

### Every production site that must be re-pointed if `develop` becomes configurable

`GitFlowConfig::default()` — production (test-only sites excluded):

| Site | What it feeds |
|------|---------------|
| `crates/devflow-core/src/git.rs:122` | the `GitFlow` struct itself |
| `crates/devflow-core/src/monitor.rs:1249-1251` | builds `format!("{}..{branch}", git_flow.develop)` for idle-timeout commit enumeration |
| `crates/devflow-cli/src/pipeline_launch.rs:1471` | `agent_result::evaluate_agent_result(..., &git_flow)` |
| `crates/devflow-cli/src/pipeline_outcomes.rs:1056` | `HookContext.git_flow` (see below) and `evaluate_agent_result` |
| `crates/devflow-core/src/ship_evidence.rs:161` | `GitFlowConfig::default().feature_prefix` |

`GitFlow::new(...)` — production:
`ship_evidence.rs:158`; `commands.rs:321`, `:343`, `:672`, `:2052`, `:2072`;
`hooks.rs:125`, `:132`, `:181`, `:239`, `:278`, `:289`; `gsd_config.rs:320`;
`parallel.rs:28`.

**Latent inconsistency worth flagging in the plan:** `HookContext` carries a
`pub git_flow: GitFlowConfig` field (`crates/devflow-core/src/hooks.rs:44`), but it is
read for `feature_prefix` **only** (`hooks.rs:135`, `hooks.rs:184`). Every git
operation inside a hook re-constructs `GitFlow::new(&ctx.project_root)` and therefore
re-defaults `develop`, discarding whatever `ctx.git_flow` says. `merge_feature`
(`hooks.rs:180-206`) is the one that calls `is_merged_into_develop` /
`merge_feature_into_develop`. Threading config into `HookContext` alone would be a
silent no-op.

**Correction to REVIEWS risk 1.2 (local-only branch has no `origin/` ref):** this is
already handled and does **not** fail. `base_ref_currency` returns
`BaseRefCurrency::Undeterminable` when `origin/{base}` does not resolve
(`preflight.rs:383-390`), and `ensure_base_ref_current` treats `Undeterminable` as
`Ok(())` with a warning (`preflight.rs:540-547`), per the module's documented
fail-open-where-blind contract. The plan should *assert* this behaviour with a test
rather than build new handling for it.

---

## File Classification

| File to modify | Role | Data flow | Closest analog | Match quality |
|---|---|---|---|---|
| `crates/devflow-core/src/config.rs` | config | request-response (resolver) | `yes_ship` field + resolver, same file `:97`, `:217-229` | exact |
| `crates/devflow-core/src/git.rs` | service (git wrapper) | request-response | `GitFlow::new` itself, `git.rs:119-124` | exact (extend, not copy) |
| `crates/devflow-cli/src/parallel.rs` | utility | request-response | own body, `parallel.rs:16-41` | exact |
| `crates/devflow-cli/src/commands.rs` | controller (CLI entry) | request-response | `yes_ship` combine idiom, `commands.rs:215-232` | exact |
| `crates/devflow-cli/src/preflight.rs` | middleware (guard) | request-response | `phase_reachability_on_base` / `base_ref_currency`, already parameterised on `base` | exact |
| `crates/devflow-cli/src/staleness.rs` | utility (pure predicate) | transform | `is_self_dogfood_workspace` + its two tests, `staleness.rs:235-274`, `:470-533` | exact |
| `crates/devflow-core/src/prompt.rs` | template/provider | transform | `## Advisory incremental self-review` block, `prompt.rs:344-353` | exact |

---

## Pattern Assignments

### D-01a — `crates/devflow-core/src/config.rs` (config, resolver)

**Analog:** `yes_ship`, in the same file. It is the newest field and shows the complete
three-part shape: struct field with a decision comment, an env-over-file-over-default
resolver, and a documented interaction with a CLI flag.

**Field pattern** (`config.rs:74-98`, abridged):
```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct DevflowConfig {
    /// Number of capture generations to retain per pipeline stage.
    pub capture_retention: usize,
    ...
    pub yes_ship: bool,
}

impl Default for DevflowConfig {
    fn default() -> Self {
        Self {
            capture_retention: DEFAULT_CAPTURE_RETENTION,
            review_angles: None,
            external_verify_enabled: true,
            yes_ship: false,
        }
    }
}
```

**Resolver pattern** (`config.rs:210-229`) — copy this verbatim in shape:
```rust
pub fn yes_ship(project_root: &Path) -> bool {
    if let Some(value) = env_value("DEVFLOW_YES_SHIP") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(
                value,
                %error,
                "invalid DEVFLOW_YES_SHIP; using devflow.toml or default"
            ),
        }
    }
    load_config(project_root).yes_ship
}
```
Note `env_value` (`config.rs:265-267`) filters empty strings, and `load_config`
(`config.rs:138-158`) is **fail-soft** — a parse failure warns and returns defaults.
A `base_branch` resolver returns `String`, so the `parse()`/`warn` arm collapses to a
direct `return value`; keep the `env_value` call with the **literal** variable name (see
the doc-check constraint below).

**Constants pattern** (`config.rs:38-43`):
```rust
pub const MAIN: &str = "main";
pub const DEVELOP: &str = "develop";
pub const FEATURE_PREFIX: &str = "feature/";
```
`DEVELOP` is the fallback default and is imported directly by `parallel.rs:4` and
`commands.rs:27`.

**Existing test analog** (`config.rs:294-315`):
```rust
#[test]
fn default_uses_hardcoded_constants() {
    let config = GitFlowConfig::default();
    assert_eq!(config.main, "main");
    assert_eq!(config.develop, "develop");
    assert_eq!(config.feature_prefix, "feature/");
}

#[test]
fn file_overrides_capture_retention_default() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("devflow.toml"), "capture_retention = 9\n").unwrap();
    assert_eq!(load_config(dir.path()).capture_retention(), 9);
}
```
Env-mutating tests in this module serialize on `ENV_MUTEX` and use the `EnvOverride`
RAII guard (`config.rs:274-292`) — required, since `std::env::set_var` is `unsafe` under
Rust 2024.

**BLOCKING constraint the planner must budget for:** `doc_check` scans Rust source for
`env_value("DEVFLOW_*")` (`crates/devflow-core/src/doc_check.rs:217-228`) and the test
`source_devflow_env_vars_and_subcommands_are_documented` (`doc_check.rs:407-428`)
asserts each such token appears in `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`,
`OPERATIONS.md`, or `docs/guides/*.md` — or is listed in
`doc-check-allowlist.toml` with a non-empty `reason`. **Adding `DEVFLOW_BASE_BRANCH`
without a docs edit in the same plan turns the workspace test suite red.**

---

### D-01b — `crates/devflow-core/src/git.rs` (service, request-response)

**Change shape:** add a config-taking constructor beside `new`; do **not** touch the
eight `self.config.develop` read sites — they already do the right thing.

The `new` doc comment (`git.rs:117-118`) currently asserts "using the hardcoded git-flow
constants" and must be updated if `new` keeps defaulting.

**Existing behavioural test analog** (`git.rs:1119-1249`) — four `cleanup_merged` tests,
including `cleanup_merged_is_relative_to_develop_not_current_head` (`git.rs:1146`) and
`cleanup_merged_deletes_when_head_is_not_on_develop` (`git.rs:1234`). These construct a
real temp repo and assert branch survival/deletion; a "honours a configured base branch"
test should copy this fixture shape rather than invent one.

---

### D-01c — `crates/devflow-cli/src/parallel.rs` (utility, request-response)

**Current code — the exact line to change** (`parallel.rs:16-41`):
```rust
use devflow_core::config::{DEVELOP, FEATURE_PREFIX};

pub(crate) fn ensure_phase_worktree(
    project_root: &Path,
    phase: PhaseId,
    force: bool,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{FEATURE_PREFIX}phase-{padded}", padded = phase.padded());

    if force {
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        let _ = GitFlow::new(project_root).delete_branch(&branch, true);
    }

    match worktree::add(project_root, &wt, &branch, DEVELOP, true) {
        Ok(()) => Ok(wt),
        Err(devflow_core::worktree::WorktreeError::Exists(path)) => {
            Err(CliError::Message(format!(
                "worktree already exists at {} — use --force to recreate it",
                path.display()
            )))
        }
        Err(err) => Err(err.into()),
    }
}
```

`worktree::add` is already fully parameterised — `start_point: &str`
(`crates/devflow-core/src/worktree.rs:64-83`) — so **no change is needed in
`worktree.rs`**. The whole D-01 worktree half is: `DEVELOP` → a resolved value, threaded
into `ensure_phase_worktree`.

**Worktree test analog** (`worktree.rs:209-305`): `init` helper creating `main` +
`develop` with one commit, then `add(root, &wt, "feature/phase-07", "develop", true)`.
A "honours configured base" test creates a third branch and passes it as `start_point`.

---

### D-01d — `crates/devflow-cli/src/commands.rs` (controller, request-response)

**The two call sites** (`commands.rs:270`, `:280`):
```rust
ensure_base_ref_current(project_root, DEVELOP)?;
...
ensure_phase_reachable_on_base(project_root, phase, DEVELOP)?;
```
Both already take `base: &str`. Threading is a substitution at the call site only.
`commands.rs:334-341` then calls `ensure_phase_worktree`.

**Config-combining idiom to copy** (`commands.rs:215-232`) — how a config value meets a
CLI flag, including the operator-visible note:
```rust
let config_yes_ship = config::yes_ship(project_root);
...
if config_yes_ship && !yes_ship {
    println!(
        "note: Ship gate pre-authorized by devflow.toml (yes_ship = true) — see D-12, 28-CONTEXT.md"
    );
}
state.yes_ship = yes_ship || config_yes_ship;
```

**Also at `commands.rs:321-332`:** `GitFlow::new(project_root).divergence_from_develop()`
in the `--no-worktree` arm, with two hardcoded operator strings naming "develop"
(`"develop is {behind} commits ahead"`). If `develop` becomes configurable these
messages go stale — cheap to fix, easy to miss.

---

### D-01e — `crates/devflow-cli/src/preflight.rs` (middleware/guard, request-response)

**Nothing here is hardcoded.** `phase_reachability_on_base` (`:146`),
`unreachable_message` (`:225`), `base_ref_currency` (`:368`), `stale_base_message`
(`:430`), `base_is_checked_out_anywhere` (`:460`), `fast_forward_base_ref` (`:485`) and
`ensure_base_ref_current` (`:537`) all take `base: &str`. The only literal is the
remote name:
```rust
// preflight.rs:322-325
/// Remote name this probe fetches and compares against — hardcoded, matching
/// `devflow_core::git::origin_main_ancestor_status`'s existing convention
/// (this project has no remote-name configuration knob).
const ORIGIN: &str = "origin";
```

**The file already anticipates this phase** (`preflight.rs:142-145`):
```rust
/// NOTE: `base` is always `devflow_core::config::DEVELOP` at the one call
/// site today. If the base branch ever becomes configurable (999.30 WR-02
/// already flagged the sibling hardcoded `"main"`), this function's callers
/// must be re-pointed at that configuration alongside it.
```
The plan should delete or rewrite this note as part of the change — a stale "always
DEVELOP" claim is exactly the doc drift this repo's post-commit hook exists to catch.

**Fail-open pattern to preserve verbatim** (`preflight.rs:538-547`) — the answer to
REVIEWS risk 1.2:
```rust
match base_ref_currency(project_root, base) {
    BaseRefCurrency::Current | BaseRefCurrency::Ahead => Ok(()),
    BaseRefCurrency::Undeterminable => {
        println!(
            "warning: could not determine whether `{base}` is current with `{ORIGIN}/{base}` \
             — proceeding without a currency check (fail-open, per this module's \
             fail-open-where-blind contract)"
        );
        Ok(())
    }
```

**Message-hygiene constraint (WR-02 / 999.10), enforced by convention not by test**
(`preflight.rs:218-224`, `:424-429`): no operator-facing preflight string may embed an
absolute filesystem path or a username. `unattended_config_condition`
(`preflight.rs:944-960`) shows the same rule applied to error rendering —
`GsdConfigError::Missing` carries a `PathBuf` and its `Display` is deliberately unused;
the `Io` arm reports `err.kind()` rather than the message. Any new base-branch refusal
string inherits this rule.

---

### D-02 — `crates/devflow-cli/src/staleness.rs` (utility, transform)

**Function to change, verbatim as it stands** (`staleness.rs:183-194`):
```rust
fn affects_compiled_binary(rel_path: &str) -> bool {
    const BUILD_AFFECTING_FILES: [&str; 4] = [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "rust-toolchain.toml",
    ];
    rel_path.ends_with(".rs")
        || BUILD_AFFECTING_FILES
            .iter()
            .any(|name| rel_path == *name || rel_path.ends_with(&format!("/{name}")))
}
```
Both failure modes D-02 targets are visible here: any `.rs` anywhere matches, and the
`ends_with(&format!("/{name}"))` arm matches `.planning/spikes/foo/Cargo.toml`.

**Its two callers, and the different input shapes they feed it:**
```rust
// staleness.rs:110-117 — raw `git diff --name-only` lines, NO status prefix
fn ancestry_range_affects_build(execution_root: &Path, embedded_commit: &str) -> bool {
    run_git_stdout(execution_root, &["diff", "--name-only", embedded_commit, "HEAD"])
        .map(|out| out.lines().any(affects_compiled_binary))
        .unwrap_or(true)   // fail toward Stale
}

// staleness.rs:154-158 — porcelain lines, normalized first
    Some(
        status
            .lines()
            .any(|line| porcelain_tracked_path(line).is_some_and(affects_compiled_binary)),
    )
```

**The normalizer** (`staleness.rs:166-173`):
```rust
fn porcelain_tracked_path(line: &str) -> Option<&str> {
    if line.len() < 4 || line.starts_with("??") {
        return None;
    }
    let path = &line[3..];
    let path = path.rsplit(" -> ").next().unwrap_or(path);
    Some(path.trim_matches('"'))
}
```
It handles the status bytes, renames and quoting REVIEWS finding 2 lists. It does
**not** strip a leading `./` — `git status --porcelain` does not emit one, so this is
absent rather than broken; note it, do not "fix" it speculatively.

**Test analog — this is the one to copy** (`staleness.rs:470-533`). Note the negative
controls are the point of both tests, and each carries a comment saying what going wrong
would cost:
```rust
/// D-17: matches only when BOTH exact member paths appear inside the
/// `members = [...]` array — never a package `name` match.
#[test]
fn is_self_dogfood_workspace_matches_both_member_paths_only() { ... }

/// WR-02: member paths that merely *contain* the real member names must
/// not classify an unrelated workspace as self-dogfood — that combination
/// hard-blocks the project's entire pipeline when its build reads Stale.
#[test]
fn is_self_dogfood_workspace_requires_exact_member_paths_not_substrings() {
    let lookalike = tempfile::tempdir().unwrap();
    ...
    assert!(
        !is_self_dogfood_workspace(lookalike.path()),
        "`devflow-core-extras`/`devflow-cli-plugin` are not the real members — \
         a substring match here would hard-block an unrelated project"
    );
    let prefixed = tempfile::tempdir().unwrap();  // "vendor/crates/devflow-core"
    assert!(!is_self_dogfood_workspace(prefixed.path()), "...");
}
```
`affects_compiled_binary` is a pure `&str -> bool`, so its tests need **no** tempdir at
all — a straight table of `(input, expected)` with the `.planning/spikes/*` and
`vendor/crates/*` negative controls.

**Explicitly ABSENT:** there are currently **no** direct unit tests for
`affects_compiled_binary` or for `porcelain_tracked_path`. Both are exercised only
indirectly through git-fixture tests: `docs_only_range_is_fresh` (`staleness.rs:1486`),
`mixed_range_docs_and_source_is_stale` (`staleness.rs:1537-1591`),
`divergent_lineage_docs_only_range_is_fresh` (`:1891`). The plan is adding the first
direct tests, not extending an existing block.

**Regression guard the plan must not break:** `mixed_range_docs_and_source_is_stale`
(`staleness.rs:1537`) asserts a range touching `.planning/x.md` **and**
`crates/devflow-cli/src/main.rs` still reads `Stale`. A `crates/`-prefix rule keeps this
green; a rule that also required a `src/` segment or a workspace-member lookup might not.

**Git fixture boilerplate to copy** for any new fixture test (`staleness.rs:1541-1556`):
```rust
let git = |args: &[&str]| {
    assert!(
        devflow_core::test_support::git_command(root).args(args).output().unwrap().status.success(),
        "git {args:?} failed"
    );
};
git(&["init", "-q"]);
git(&["config", "user.email", "t@e.st"]);
git(&["config", "user.name", "t"]);
git(&["config", "commit.gpgsign", "false"]);
git(&["config", "core.hooksPath", "/dev/null"]);
```

---

### D-03 — `crates/devflow-core/src/prompt.rs` (template/provider, transform)

**Analog: the existing policy block in the very function being changed.** The "Advisory
incremental self-review" section is precisely the composition shape D-03 needs — a `##`
heading, one paragraph, a closing sentence that forbids pausing, a blank line, then
`{COMPLETION_PROTOCOL}` last.

```rust
// crates/devflow-core/src/prompt.rs:338-355
fn code_stage_prompt(phase: PhaseId) -> String {
    let command = format!(
        "{} {AUTO_CHAIN_PRESERVING_FLAG}",
        gsd_command_for(Stage::Code, phase)
    );
    format!(
        "Run the GSD workflow command for this stage:\n\n    {command}\n\n\
        ## Advisory incremental self-review\n\
        \n\
        After each plan or wave lands, perform a quick, shallow self-check \
        for doc accuracy, leaked data, CI/build correctness, and \
        external-state claims. Record any drift in the working output and \
        continue execution; the authoritative review happens during Ship. \
        This check must not pause execution or request human input.\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}
```

**HIGH-VALUE PLANNER NOTE — the block is duplicated, and both copies must move
together.** The identical paragraph appears verbatim in the workflow-style renderer used
by Codex and Pi:
```rust
// crates/devflow-core/src/prompt.rs:437-450  (workflow_code_prompt, FullExecute|None arm)
        Some(FixType::FullExecute) | None => format!(
            "Read and follow the GSD workflow file at {workflow_root}/execute-phase.md for \
            phase {phase} --auto. The `--auto` flag is part of the workflow invocation and \
            must be preserved verbatim.\n\n\
            ## Advisory incremental self-review\n\
            \n\
            After each plan or wave lands, perform a quick, shallow self-check \
            ... \
            This check must not pause execution or request human input.\n\
            \n\
            {COMPLETION_PROTOCOL}"
        ),
```
`render_claude_style` (`:363-378`) routes Claude/OpenCode to `code_stage_prompt`;
`render_workflow_style` (`:390-403`) routes Codex/Pi to `workflow_code_prompt`. Adding
the decision policy to only one leaves the two agent families with different unattended
semantics. There is no shared constant for these blocks today — extracting one is a
reasonable, in-scope option for the planner to consider.

**Boundary clarification for the plan (REVIEWS finding 3):** `checkpoint_auto_decide_prompt`
(`prompt.rs:515-527`) is a *different* code path — it is the instruction injected into a
**resumed** session after DevFlow's own human-blocking gate found nobody to answer. It
is not reached during a normal one-shot Code stage. Its text is the closest existing
prose model for D-03's wording:
```rust
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

**Test analog A — the pinning test that will need updating** (`prompt.rs:660-691`).
Note the not-contains assertions; a new policy block must not reintroduce
`AskUserQuestion`/`request_user_input`:
```rust
#[test]
fn code_stage_prompt_is_unchanged_single_command_template() {
    let prompt = stage_prompt(Stage::Code, PhaseId::new(9));
    assert!(prompt.contains("/gsd-execute-phase 9"));
    assert!(prompt.contains("DEVFLOW_RESULT"));
    assert!(!prompt.contains("/gsd-code-review"), "...");
    assert!(!prompt.contains("already exists"), "...");
    assert!(prompt.contains("Advisory incremental self-review"));
    for angle in ["doc accuracy", "leaked data", "CI/build correctness", "external-state claims"] {
        assert!(prompt.contains(angle), "Code prompt missing angle: {angle}");
    }
    assert!(!prompt.contains("AskUserQuestion"));
    assert!(!prompt.contains("request_user_input"));
}
```

**Test analog B — the prompt-policy test trio to copy for the new block**
(`prompt.rs:840-880`): determinism, terminator, and lowercased keyword assertions with
a stated reason per assert:
```rust
#[test]
fn checkpoint_auto_decide_prompt_is_deterministic() {
    assert_eq!(
        checkpoint_auto_decide_prompt(PhaseId::new(28)),
        checkpoint_auto_decide_prompt(PhaseId::new(28))
    );
}

#[test]
fn checkpoint_auto_decide_prompt_terminates_with_completion_protocol() {
    let prompt = checkpoint_auto_decide_prompt(PhaseId::new(28));
    assert!(prompt.ends_with(COMPLETION_PROTOCOL), "...");
    assert!(prompt.contains("DEVFLOW_RESULT"));
}

#[test]
fn checkpoint_auto_decide_prompt_states_no_operator_judgment_and_record_reasoning() {
    let prompt = checkpoint_auto_decide_prompt(PhaseId::new(28)).to_lowercase();
    assert!(prompt.contains("no human operator") || prompt.contains("nobody"), "...");
    assert!(prompt.contains("judgment") || prompt.contains("judgement"), "...");
    assert!(
        prompt.contains("record") && prompt.contains("reasoning"),
        "must require recording the reasoning in the final message, since \
         this is the ONLY record of what was decided (D-07)"
    );
}
```

---

## Shared Patterns

### Fail-direction is always explicit and commented
**Sources:** `staleness.rs:110-117` (`unwrap_or(true)` — fail toward Stale);
`preflight.rs:456-472` (`base_is_checked_out_anywhere` returns `true` on error,
"the OPPOSITE polarity to this module's fail-open-where-blind contract, because the
consequence of a wrong answer here is a destructive ref write");
`preflight.rs:540-547` (`Undeterminable` fails open with a printed warning).
**Apply to:** every new predicate or guard in D-01 and D-02. State the direction and
the reason in the doc comment, not just in the code.

### Operator-facing strings carry no absolute path and no username
**Source:** `preflight.rs:218-224` and `:424-429` (WR-02 / 999.10); enforced by
convention and by review, plus `unattended_config_condition`'s deliberate refusal to use
`GsdConfigError::Missing`'s `Display` (`preflight.rs:939-958`).
**Apply to:** any new base-branch refusal or note string in `commands.rs`/`preflight.rs`.

### New `DEVFLOW_*` env var ⇒ same-plan docs edit
**Source:** `doc_check.rs:217-228` + `doc_check.rs:407-428`; scoped docs list at
`doc_check.rs:8-13`; escape hatch is `doc-check-allowlist.toml` (requires a non-empty
`reason`, validated at `doc_check.rs:28-38`).
**Apply to:** D-01 if it introduces `DEVFLOW_BASE_BRANCH`. Read via the literal
`env_value("DEVFLOW_...")` form — `config.rs:246-250` documents that a const-mediated
read is invisible to this scanner and would pass green while the variable stayed
undocumented.

### Config resolution order: env → `devflow.toml` → built-in default, fail-soft
**Source:** `config.rs:138-158` (`load_config` warns and returns defaults on read/parse
failure), `config.rs:162-229` (four resolvers, identical shape).
**Apply to:** the D-01 `base_branch` resolver.

### Env-mutating tests use the `ENV_MUTEX` + `EnvOverride` guard
**Source:** `config.rs:272-292`. Required under Rust 2024 (`set_var` is `unsafe`).
**Apply to:** any D-01 test asserting env precedence.

---

## No Analog Found

None. Every file in scope is an existing file with an in-repo precedent for the kind of
change required.

---

## Deltas Between REVIEWS Assumptions and the Code as It Stands

| REVIEWS claim | What the source actually shows |
|---|---|
| "GitFlow lifecycle remains hardcoded to `develop`" | Methods read `self.config.develop`; the hardcoding is in `GitFlow::new` (`git.rs:122`) only. One constructor + a call-site audit, not a lifecycle rewrite. |
| "`base_ref_currency` ... causing `ensure_base_ref_current` failures" for local-only branches | Already fails **open** with a warning via `BaseRefCurrency::Undeterminable` (`preflight.rs:383-390`, `:540-547`). Needs a test, not new handling. |
| "Must handle git porcelain ... `./` prefixes" | `porcelain_tracked_path` (`staleness.rs:166-173`) handles status bytes, renames and quotes. `./` is not emitted by `git status --porcelain`; no such handling exists and none appears to be needed. |
| "`code_stage_prompt` policy advises the agent during one-shot execution" | True, but the same block must also be added to `workflow_code_prompt` (`prompt.rs:437-450`) or Codex/Pi diverge from Claude/OpenCode. |

---

## Metadata

**Analog search scope:** `crates/devflow-core/src/`, `crates/devflow-cli/src/`
**Files read (targeted ranges):** `git.rs`, `config.rs`, `worktree.rs`, `parallel.rs`,
`commands.rs`, `preflight.rs`, `staleness.rs`, `prompt.rs`, `hooks.rs`, `doc_check.rs`,
`monitor.rs`, `pipeline_launch.rs`, `pipeline_outcomes.rs`
**Tracked-source gate:** all 15 named paths confirmed via `git ls-files`
**Pattern extraction date:** 2026-09-01

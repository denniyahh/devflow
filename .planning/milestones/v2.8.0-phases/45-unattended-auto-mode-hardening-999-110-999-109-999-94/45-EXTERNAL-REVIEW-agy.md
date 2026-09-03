## CONFIRMED

### BLOCKER: Config fail-open — TOML syntax errors silently redirect the trunk to `develop`
- **File:** `crates/devflow-core/src/config.rs:195`
- **Defect:** `load_config` catches `toml::from_str` syntax errors, logs a warning, and returns `DevflowConfig::default()`. The `base_branch` resolver treats the resulting `None` as an absent configuration and falls back to `DEVELOP`. This violates the explicitly documented FAIL-HARD requirement for `base_branch`.
- **Trigger:** An operator configures `base_branch` in `devflow.toml` but makes a syntax error (e.g., `base_branch = "main`).
- **Consequence:** The resolver silently falls back to `develop`, completely bypassing the refusal of production branches and forking/merging the phase on the wrong trunk without failing or halting.
- **Evidence:** 
  ```rust
      match toml::from_str(&contents) {
          Ok(config) => config,
          Err(error) => {
              tracing::warn!(path = %path.display(), %error, "failed to parse devflow config; using defaults");
              DevflowConfig::default()
          }
      }
  ```

### BLOCKER: Build staleness ignores `.cargo/config.toml` and non-`crates/` workspace members
- **File:** `crates/devflow-cli/src/staleness.rs:271`
- **Defect:** `affects_compiled_binary` strictly filters build-affecting files using `starts_with("crates/")` and a hardcoded list of exactly four root files (`Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`). It completely ignores `.cargo/config.toml` (which configures rustflags, targets, etc.) and any Cargo workspace members explicitly declared in `Cargo.toml` that reside outside the `crates/` directory (e.g., `libs/`).
- **Trigger:** The operator modifies `.cargo/config.toml` or modifies source code in a valid Cargo workspace member outside of `crates/`.
- **Consequence:** The dogfood pipeline evaluates the build as "Fresh" and skips compilation, allowing execution to proceed with a stale binary despite its compiler inputs having been modified.
- **Evidence:**
  ```rust
      const BUILD_AFFECTING_FILES: [&str; 4] = [
          "Cargo.toml",
          "Cargo.lock",
          "build.rs",
          "rust-toolchain.toml",
      ];
      // ...
      rel_path.starts_with(WORKSPACE_MEMBER_PREFIX)
  ```

### BLOCKER: TOCTOU in `write_state_atomic` guarantees data loss on concurrent state saves
- **File:** `crates/devflow-core/src/workflow.rs:189`
- **Defect:** `write_state_atomic` uses a fixed `path.with_extension("tmp")` for the temporary file during atomic saves. It does not mix in a PID, thread ID, or random nonce.
- **Trigger:** Two processes (e.g., a background monitor and the active agent, or two agents in parallel) attempt to save the state for the same phase simultaneously.
- **Consequence:** Both processes write to the exact same `.tmp` file. Process A renames the file to `state.json`. Process B then attempts to rename the now-missing `.tmp` file and crashes with `ENOENT`, or Process B overwrites Process A's data before the rename, resulting in corrupted or lost state updates.
- **Evidence:**
  ```rust
      let tmp = path.with_extension("tmp");
      std::fs::write(&tmp, contents)?;
      std::fs::rename(&tmp, path)?;
  ```

### BLOCKER: Shell script unquoted expansion of `$FORBIDDEN_UNMERGED` breaks cleanup
- **File:** `scripts/cut-pr-branch.sh:84`
- **Defect:** The `for f in $FORBIDDEN_UNMERGED` loop iterates over an unquoted variable. If a forbidden file path contains spaces, the shell word-splits the path into fragments.
- **Trigger:** A PR branch is being cut and there is an unmerged conflict in a forbidden planning artifact whose name contains a space (e.g., `.planning/my plan.md`).
- **Consequence:** `git rm -f` fails to find the fragmented paths. The file remains unmerged in the index, triggering the subsequent `REMAINING_UNMERGED` check which crashes the script and aborts the PR branch extraction.
- **Evidence:**
  ```bash
          FORBIDDEN_UNMERGED=$(echo "$UNMERGED" | grep -E "$FORBIDDEN_REGEX" || true)
          if [ -n "$FORBIDDEN_UNMERGED" ]; then
              for f in $FORBIDDEN_UNMERGED; do
                  git rm -f "$f" >/dev/null 2>&1 || true
              done
          fi
  ```

### WARNING: CR-04 deferral creates a direct prompt contradiction for headless checkpoints
- **File:** `crates/devflow-core/src/prompt.rs:537` and `crates/devflow-core/src/prompt.rs:86`
- **Defect:** `CODE_STAGE_POLICY` strictly commands the agent: "This authority does not extend to a `blocking-human` gate... do not self-resolve or approve them". But `checkpoint_auto_decide_prompt` (injected on resume) commands the exact opposite: "You previously stopped at a human-blocking checkpoint... DevFlow's policy is for you to resolve the checkpoint yourself".
- **Trigger:** An unattended phase pauses at a human-blocking gate, times out, and is resumed via `devflow resume` without an operator present.
- **Consequence:** The LLM receives directly contradictory instructions in the same prompt context. It may refuse to resolve the checkpoint, halting the unattended run again and completely breaking the feature.

## SUSPECTED

### WARNING: Porcelain parser extracts the wrong path if a new filename contains ` -> `
- **File:** `crates/devflow-cli/src/staleness.rs:204`
- **Defect:** `porcelain_tracked_path` splits all paths using `rsplit(" -> ")` before stripping quotes. If a file is modified (e.g., ` M "crates/devflow-core/src/a -> b.rs"`), the parser splits on the ` -> ` sequence located inside the quoted filename itself, rather than preserving the file.
- **Trigger:** A workspace member file legitimately containing ` -> ` in its name is modified.
- **Consequence:** The parser incorrectly extracts only the trailing fragment (`b.rs`). Because this fragment loses the `crates/` prefix, `affects_compiled_binary` evaluates to `false` and silently drops the file from the staleness evaluation.
- **Evidence:** 
  ```rust
      let path = &line[3..];
      let path = path.rsplit(" -> ").next().unwrap_or(path);
      Some(path.trim_matches('"'))
  ```

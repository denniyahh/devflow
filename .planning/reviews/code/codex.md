### Needs my awareness

1. **HIGH — Pi preflight always invokes an invalid command.**  
   [pi.rs:57](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-core/src/agents/pi.rs:57) runs `pi auth check --json`, but Pi 0.84.1 requires `--provider` or `--model`; reproduced exit code 2. Consequently, every Pi launch fails preflight regardless of credentials. The remediation at [pi.rs:73](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-core/src/agents/pi.rs:73) also recommends `pi auth login`, which is absent from that version’s documented auth commands. Pass the same explicit provider/model used for execution and integration-test the spawned command, not only the classifier.

2. **HIGH — The changelog check false-greens the exact forgotten-bump case it claims to prevent.**  
   [commands.rs:2614](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:2614) returns success whenever two mutable files agree, without comparing against the latest released tag. The active milestone is v2.6.0, but Cargo.toml and CHANGELOG.md both remain at 2.5.0; `devflow release --check .` nevertheless exited 0. The agreement fixture at [commands.rs:6666](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:6666) enshrines this false-green. Compare against the latest release tag or another immutable baseline.

3. **HIGH — Deleting the signing probe leaves the release integration suite broken.**  
   The feature branch fails 3/10 tests at [release_check.rs:403](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/tests/release_check.rs:403), [release_check.rs:499](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/tests/release_check.rs:499), and [release_check.rs:548](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/tests/release_check.rs:548), all still expecting deleted signing output. Negative control: the same 10-test suite passes on `develop`, proving this is branch-induced rather than a broken harness.

4. **MEDIUM — `release --check` still promises signing viability after silently dropping it.**  
   The CLI advertises tag-signing viability at [main.rs:289](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/main.rs:289), while [commands.rs:2404](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:2404) now performs only the changelog check. The replacement script checks only presence/readability at [cut-release.sh:123](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/scripts/cut-release.sh:123); actual signing is deferred until [cut-release.sh:137](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/scripts/cut-release.sh:137). Thus an unusable key fails later, after release PR work. Either restore early viability checking or update help, docs, snapshots, and tests to state the reduced contract.

5. **MEDIUM — Doctor reports unusable Pi installations as healthy.**  
   [commands.rs:2316](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:2316) checks only `pi --version`; it never tests provider readiness. A credentialless Pi therefore receives a green doctor result even though start cannot use it. Reuse a corrected auth-readiness check and distinguish “installed” from “headless-capable.”

### Handled
### Needs my awareness

1. **HIGH — Pi preflight always invokes an invalid command.**  
   [pi.rs:57](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-core/src/agents/pi.rs:57) runs `pi auth check --json`, but Pi 0.84.1 requires `--provider` or `--model`; reproduced exit code 2. Consequently, every Pi launch fails preflight regardless of credentials. The remediation at [pi.rs:73](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-core/src/agents/pi.rs:73) also recommends `pi auth login`, which is absent from that version’s documented auth commands. Pass the same explicit provider/model used for execution and integration-test the spawned command, not only the classifier.

2. **HIGH — The changelog check false-greens the exact forgotten-bump case it claims to prevent.**  
   [commands.rs:2614](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:2614) returns success whenever two mutable files agree, without comparing against the latest released tag. The active milestone is v2.6.0, but Cargo.toml and CHANGELOG.md both remain at 2.5.0; `devflow release --check .` nevertheless exited 0. The agreement fixture at [commands.rs:6666](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:6666) enshrines this false-green. Compare against the latest release tag or another immutable baseline.

3. **HIGH — Deleting the signing probe leaves the release integration suite broken.**  
   The feature branch fails 3/10 tests at [release_check.rs:403](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/tests/release_check.rs:403), [release_check.rs:499](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/tests/release_check.rs:499), and [release_check.rs:548](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/tests/release_check.rs:548), all still expecting deleted signing output. Negative control: the same 10-test suite passes on `develop`, proving this is branch-induced rather than a broken harness.

4. **MEDIUM — `release --check` still promises signing viability after silently dropping it.**  
   The CLI advertises tag-signing viability at [main.rs:289](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/main.rs:289), while [commands.rs:2404](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:2404) now performs only the changelog check. The replacement script checks only presence/readability at [cut-release.sh:123](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/scripts/cut-release.sh:123); actual signing is deferred until [cut-release.sh:137](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/scripts/cut-release.sh:137). Thus an unusable key fails later, after release PR work. Either restore early viability checking or update help, docs, snapshots, and tests to state the reduced contract.

5. **MEDIUM — Doctor reports unusable Pi installations as healthy.**  
   [commands.rs:2316](/var/home/denniyahh/Github/devflow/.worktrees/phase-36/crates/devflow-cli/src/commands.rs:2316) checks only `pi --version`; it never tests provider readiness. A credentialless Pi therefore receives a green doctor result even though start cannot use it. Reuse a corrected auth-readiness check and distinguish “installed” from “headless-capable.”

### Handled

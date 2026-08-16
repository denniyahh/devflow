> antigravity -p (Gemini 3.1 Pro, 15m timeout)

### Finding 1: Direct Contradiction on `tag()` Mutation and Signing Contract
* **Category:** Contradiction between plan requirements
* **Exact References:**
  * [36-02-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-02-PLAN.md) (`must_haves.truths`):
    > `"The separate tag() (VersionBump's lightweight auto-bump tag) is deliberately left unsigned — it is not a release tag."`
  * [36-02-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-02-PLAN.md) (`P-01`):
    > `"The release tag becomes signed; the VersionBump tag does not... tag() is left untouched."`
  * [36-02-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-02-PLAN.md) (`P-03`):
    > `"`release_finish` and `tag` read `devflow.releaseSigningKey` (git config); if unset or the file is unreadable, return an error naming the config key and the fix — never fall back to the agent's `user.signingkey`, never sign unsigned silently."`
  * [36-02-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-02-PLAN.md) (`Task 1 <verify>`):
    > `"`release_finish`/`tag` fail with a clear error when `devflow.releaseSigningKey` is unset"`
* **Flaw:**
  The plan cannot decide whether `tag()` is modified or untouched. `must_haves` and `P-01` explicitly state `tag()` is deliberately unsigned and untouched. `P-03` and `Task 1 <verify>` demand that `tag()` read `devflow.releaseSigningKey` and error out if unset. Implementing `P-01` will fail the acceptance test in `Task 1`, while implementing `P-03` violates `must_haves` and breaks auto-bumping in environments lacking release signing keys.

---

### Finding 2: Preflight Logic Contains a False-Positive Credential Check
* **Category:** Failure mode / Logic flaw
* **Exact References:**
  * [36-01-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-01-PLAN.md) (`must_haves.truths`):
    > `"...when the binary is present but no provider credential resolves (no DEVFLOW_PI_PROVIDER, no stored pi auth)."`
  * [36-01-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-01-PLAN.md) (`Task 2 <behavior>`):
    > `"...check a provider credential resolves — DEVFLOW_PI_PROVIDER set, or GEMINI_API_KEY set, or pi auth already stored."`
* **Flaw:**
  Treating `DEVFLOW_PI_PROVIDER` as satisfying "a provider credential resolves" is fundamentally broken. `DEVFLOW_PI_PROVIDER` specifies an engine name (e.g., `anthropic`, `openai`), **not** an authentication secret. If an operator sets `DEVFLOW_PI_PROVIDER=anthropic` without exporting `ANTHROPIC_API_KEY`, `PiAgent::preflight` will return `Ok(())` ("headless capable"), and `pi` will immediately crash on execution.

---

### Finding 3: Missing Argument Delimiter (`--`) Breaks Positional Prompt Transport
* **Category:** Decision that does not survive contact with CLI tools
* **Exact References:**
  * [36-01-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-01-PLAN.md) (`must_haves.truths`):
    > `"`PiAgent::exec_command` returns `(\"pi\", vec![\"-p\", \"--approve\", \"--no-session\", <model/provider flags>, <prompt>])` — the prompt is POSITIONAL"`
  * [36-01-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-01-PLAN.md) (`Task 1 <behavior>`):
    > `"`exec_command()` → `(\"pi\", vec![\"-p\", \"--approve\", \"--no-session\", …, prompt.to_string()])`"`
* **Flaw:**
  DevFlow phase prompts commonly begin with markdown lists (e.g., `- [ ] Task 1...`) or CLI options (`--flag`). Because `pi` uses positional prompt transport instead of stdin, standard CLI argument parsers will treat any leading hyphen in `<prompt>` as an unknown option flag (e.g., `error: unknown option '- '`). Without a bare `--` argument separator preceding `<prompt>`, positional execution will fail on markdown-formatted prompts.

---

### Finding 4: Deterministic Signing Fails for SSH Keys by Omitting `gpg.format=ssh`
* **Category:** Unstated assumption / Contact with tool
* **Exact References:**
  * [36-RESEARCH.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-RESEARCH.md) (`Release path`):
    > `"`check_ssh_signing_viability` (`git.rs:1112-1150`) + `check_signing_viability` — the capability probe to remove."`
  * [36-02-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-02-PLAN.md) (`P-01`):
    > `"git -c user.signingkey=<devflow.releaseSigningKey> tag -s -m \"v<version>\" v<version>"`
* **Flaw:**
  The research establishes that SSH signing keys were previously checked via `check_ssh_signing_viability`. In Git, `git tag -s` invokes GPG format (`openpgp`) by default. If `devflow.releaseSigningKey` is configured with an SSH public key or key path (e.g., `~/.ssh/id_ed25519.pub`), executing `git -c user.signingkey=<key> tag -s` without `-c gpg.format=ssh` will cause `gpg` to fail to parse the key. Deleting the SSH capability probe while hardcoding `git tag -s` breaks SSH-based release signing unless the operator's ambient global Git config already happens to set `gpg.format = ssh`.
  *(Note: Labelled as speculation if the project strictly mandates GPG keys, but the deleted codebase specifically accommodated SSH signing).*

---

### Finding 5: "Stored Pi Auth" Check Lacks Mechanism Specification and Leaks Ambient State
* **Category:** Acceptance criteria not falsifiable / Missing negative control
* **Exact References:**
  * [36-01-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-01-PLAN.md) (`Task 2 <behavior>`):
    > `"...or pi auth already stored. Distinct Err reasons for \"binary absent\" vs \"binary present but no credential\""`
  * [36-01-PLAN.md](file:///home/denniyahh/.gemini/antigravity-cli/scratch/36-01-PLAN.md) (`Task 2 <action>`):
    > `"use a test-only env-guard so the check reads a controllable source rather than the operator's live environment"`
* **Flaw:**
  1. The plan specifies no mechanism (file path, CLI invocation, or format) for how `devflow-core` determines whether "pi auth is stored".
  2. The proposed negative control isolates only environment variables ("test-only env-guard"). If the implementation checks the filesystem (e.g., `~/.pi/agent/auth.json`), any developer or CI runner with existing local credentials will pass the check regardless of the env-guard. The negative control does not isolate on-disk credentials, creating an unfalsifiable proxy test that will pass on credentialed machines and fail on clean ones.

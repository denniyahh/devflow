#!/usr/bin/env bash
#
# Scaffold a throwaway git repository that is a legal `devflow start`
# target — for probing the Define->Ship pipeline (phase 23a) without
# touching this checkout's own working tree or git state (D-01
# blast-radius isolation, T-23-01/T-23-02).
#
# Scaffolding decision (Claude's Discretion item #2, recorded in
# 23-CONTEXT.md / 23-01-PLAN.md "Recorded planning choices"): the probe
# target is a SYNTHETIC single-task phase, not an imported real backlog
# item — the probe is about the supervisor/pipeline mechanism, not about
# the content of the work.
#
# Minimum scaffold, per `crates/devflow-cli/src/preflight.rs`'s
# `run_preflight` (read in full before writing this script — the only
# thing it checks for `--agent claude` is that the `claude` binary
# resolves on PATH; there is no project-local `.claude/` requirement, so
# none is scaffolded here):
#   - a git repo with `main` + `develop` and one initial commit
#   - `.planning/{PROJECT.md,ROADMAP.md,STATE.md}` (ROADMAP's phase heading must
#     be `### Phase N:` — three hashes — to satisfy the reachability guard in
#     `preflight.rs`, and the current preflight also requires a minimal
#     `.planning/config.json` so the 35.1 unattended-launch chain-flag
#     prerequisite has somewhere to hold `workflow._auto_chain_active`)
#   - one phase directory carrying a trivial, already-written plan, so
#     Define/Plan have a real target instead of inventing scope
#
# Branch names below are hardcoded to "main"/"develop" — verified against
# `crates/devflow-core/src/config.rs:15,17`
# (`pub const MAIN: &str = "main";` / `pub const DEVELOP: &str = "develop";`)
# at plan time, not re-read live by this script.
#
# The default mode's output is also the Phase 49 negative arm's fixture
# (D-14/D-25), so it must stay byte-identical: `--selftest` pins its tree hash.
#
# `--live-payload` (Phase 49, D-20): the disposable project for the
# milestone's live unattended run — again a SYNTHETIC single-task phase, for
# the same reason as above. Same destination guard and repo-local identity,
# plus repo-local commit/tag signing off. Topology (D-06, R-5): `main` carries
# the payload; `integration` forks from it, adds `devflow.toml`
# (`base_branch = "integration"`, and `capture_retention = 64` so the
# auto-decide resume's captures survive, R-7) and is left checked out.
# `.planning/config.json` is written in serde-pretty form with the chain flag
# present and false. Every commit is a non-breaking `chore:` and nothing is
# tagged, so the payload cannot classify as a MAJOR bump (D-21, R-8). DevFlow
# runs no tests at Validate; it drives /gsd-validate-phase over the payload's
# own test (R-9), which plan 49-06 adds with the phase directory. Nothing is
# launched and nothing is pushed.
#
# WHAT `--check` DOES NOT ESTABLISH: it proves the layout and config values
# above, read-only. It does NOT prove that DevFlow's preflight or GSD's
# config resolution will accept the run; only a real launch exercises those.
#
# Usage:
#   scripts/scratch-dogfood-repo.sh [destination]
#   scripts/scratch-dogfood-repo.sh --live-payload [--gsd-tools <path>] [destination]
#   scripts/scratch-dogfood-repo.sh --check <destination>
#   scripts/scratch-dogfood-repo.sh --selftest
#   destination defaults to $TMPDIR (or /tmp)/devflow-dogfood-scratch-$$
#   if omitted. Never inside this checkout or a sibling `.worktrees/` entry.
#   --gsd-tools defaults to $HOME/.claude/gsd-core/bin/gsd-tools.cjs.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
SELF="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"

die() {
    echo "ERROR: $*" >&2
    exit 1
}

MODE=default
DEST=""
HAVE_DEST=false
CHECK_DEST=""
# shellcheck disable=SC2034  # consumed by the payload's GSD-resolution probes (49-06)
GSD_TOOLS="$HOME/.claude/gsd-core/bin/gsd-tools.cjs"

set_mode() {
    [ "$MODE" = default ] || [ "$MODE" = "$1" ] || die "conflicting modes: --$MODE and --$1"
    MODE="$1"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --live-payload) set_mode live-payload ;;
        --check)
            set_mode check
            shift
            [ $# -gt 0 ] || die "--check needs a path"
            CHECK_DEST="$1"
            ;;
        --selftest) set_mode selftest ;;
        --gsd-tools)
            shift
            [ $# -gt 0 ] || die "--gsd-tools needs a path"
            # shellcheck disable=SC2034  # consumed by the payload's GSD-resolution probes (49-06)
            GSD_TOOLS="$1"
            ;;
        -h | --help)
            sed -n '/^# Usage:/,/^#$/p' "$SELF"
            exit 0
            ;;
        -*) die "unknown option: $1" ;;
        *)
            [ "$HAVE_DEST" = false ] || die "unexpected second destination: $1"
            DEST="$1"
            HAVE_DEST=true
            ;;
    esac
    shift
done

case "$MODE" in
    check | selftest)
        [ "$HAVE_DEST" = false ] || die "--$MODE takes no positional destination (got: $DEST)"
        ;;
esac

# ---------------------------------------------------------------------------
# --selftest: runs this script as a child against scratch destinations and
# asserts both directions. Case line `  ok   <name>` or `  FAIL <name> — <why>`,
# last line `SELFTEST passed=<P> failed=<F>`, exit 0 only if F=0 and P>0.
# Every control first proves its own precondition, and refusals are asserted
# on their text, never only on the exit code.
# ---------------------------------------------------------------------------
ST_TMP=""
N_PASS=0
N_FAIL=0

case_ok() {
    printf '  ok   %s\n' "$1"
    N_PASS=$((N_PASS + 1))
}

case_fail() {
    printf '  FAIL %s — %s\n' "$1" "$2"
    N_FAIL=$((N_FAIL + 1))
}

# st_count <fixed-string> <file>: occurrences, 0 when none (rg -c prints nothing).
st_count() {
    local count
    count="$(rg --count-matches -F -e "$1" -- "$2" || true)"
    printf '%s\n' "${count:-0}"
}

st_live_topology() {
    local name="live payload: integration checked out with devflow.toml"
    local lp="$ST_TMP/lp" rc=0 head
    bash "$SELF" --live-payload "$lp" >"$ST_TMP/lp.log" 2>&1 || rc=$?
    if [ "$rc" -ne 0 ]; then
        case_fail "$name" "--live-payload exited $rc: $(tail -n 1 "$ST_TMP/lp.log")"
        return
    fi
    head="$(git -C "$lp" symbolic-ref -q HEAD 2>/dev/null || true)"
    if [ "$head" != refs/heads/integration ]; then
        case_fail "$name" "HEAD is '${head:-unreadable}', want refs/heads/integration"
        return
    fi
    if ! git -C "$lp" show-ref --verify -q refs/heads/main; then
        case_fail "$name" "refs/heads/main is missing"
        return
    fi
    # Spelled independently of the scaffold's heredoc, so a typo there fails here.
    printf 'base_branch = "integration"\ncapture_retention = 64\n' >"$ST_TMP/want.toml"
    if ! git -C "$lp" show integration:devflow.toml >"$ST_TMP/got.toml" 2>/dev/null ||
        ! cmp -s "$ST_TMP/want.toml" "$ST_TMP/got.toml"; then
        case_fail "$name" "integration:devflow.toml is absent or differs from the two expected lines"
        return
    fi
    if git -C "$lp" cat-file -e main:devflow.toml 2>/dev/null; then
        case_fail "$name" "devflow.toml is committed on main; it belongs on integration only"
        return
    fi
    case_ok "$name"
}

st_live_check_passes() {
    local name="live payload: check passes"
    local rc=0 last
    bash "$SELF" --check "$ST_TMP/lp" >"$ST_TMP/check.out" 2>&1 || rc=$?
    last="$(tail -n 1 "$ST_TMP/check.out")"
    if [ "$rc" -ne 0 ]; then
        case_fail "$name" "--check exited $rc, last line '${last:-<empty>}'"
        return
    fi
    if ! printf '%s\n' "$last" | rg -q '^CHECK passed=[1-9][0-9]* failed=0$'; then
        case_fail "$name" "last line is '${last:-<empty>}', want CHECK passed=<P≥1> failed=0"
        return
    fi
    case_ok "$name"
}

st_control_compact_config() {
    local name="control: check refuses a compact config.json"
    local copy="$ST_TMP/compact" cfg rc=0
    if [ ! -d "$ST_TMP/lp/.git" ]; then
        case_fail "$name" "precondition: no live payload at $ST_TMP/lp to copy"
        return
    fi
    cp -a "$ST_TMP/lp" "$copy"
    cfg="$copy/.planning/config.json"
    if ! { jq -c . "$cfg" >"$ST_TMP/compact.json" && mv "$ST_TMP/compact.json" "$cfg" &&
        git -C "$copy" commit -q -am "chore: compact config for the control"; }; then
        case_fail "$name" "precondition: could not commit a compact config.json in the copy"
        return
    fi
    if cmp -s <(jq --indent 2 . "$cfg") "$cfg"; then
        case_fail "$name" "precondition: the copy's config.json still equals jq --indent 2 of itself"
        return
    fi
    bash "$SELF" --check "$copy" >"$ST_TMP/compact.out" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
        case_fail "$name" "--check accepted a compact config.json (exit 0)"
        return
    fi
    if [ "$(st_count '  FAIL config.json is in pretty form' "$ST_TMP/compact.out")" -ne 1 ]; then
        case_fail "$name" "exit $rc but no FAIL line naming 'config.json is in pretty form'"
        return
    fi
    # Only the form differs, so only the form check may fail.
    if [ "$(st_count '  FAIL ' "$ST_TMP/compact.out")" -ne 1 ]; then
        case_fail "$name" "more than one check failed; the control no longer isolates the form"
        return
    fi
    case_ok "$name"
}

st_control_head_on_main() {
    local name="control: check refuses HEAD on main"
    local copy="$ST_TMP/onmain" head rc=0
    if [ ! -d "$ST_TMP/lp/.git" ]; then
        case_fail "$name" "precondition: no live payload at $ST_TMP/lp to copy"
        return
    fi
    cp -a "$ST_TMP/lp" "$copy"
    git -C "$copy" checkout -q main
    head="$(git -C "$copy" symbolic-ref -q HEAD 2>/dev/null || true)"
    if [ "$head" != refs/heads/main ]; then
        case_fail "$name" "precondition: HEAD is '${head:-unreadable}', want refs/heads/main"
        return
    fi
    bash "$SELF" --check "$copy" >"$ST_TMP/onmain.out" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
        case_fail "$name" "--check accepted HEAD on main (exit 0)"
        return
    fi
    if [ "$(st_count '  FAIL head is integration' "$ST_TMP/onmain.out")" -ne 1 ]; then
        case_fail "$name" "exit $rc but no FAIL line naming 'head is integration'"
        return
    fi
    case_ok "$name"
}

# The negative arm's fixture (D-14/D-25) is pinned by content: tree hashes
# carry no timestamps, so they are deterministic where commit SHAs are not.
# Measured at bc73b1b and re-measured at aaad4b9; one appended byte to
# config.json gives 8ffe954e… instead.
DEFAULT_TREE=f6120b337cb9b37d658c6111b7ac14694338dc56

st_default_tree() {
    local name="default mode: tree hash unchanged"
    local dflt="$ST_TMP/dflt" rc=0 dev main_tree
    bash "$SELF" "$dflt" >"$ST_TMP/dflt.log" 2>&1 || rc=$?
    if [ "$rc" -ne 0 ]; then
        case_fail "$name" "default scaffold exited $rc: $(tail -n 1 "$ST_TMP/dflt.log")"
        return
    fi
    dev="$(git -C "$dflt" rev-parse -q --verify 'develop^{tree}' 2>/dev/null || echo ABSENT)"
    main_tree="$(git -C "$dflt" rev-parse -q --verify 'main^{tree}' 2>/dev/null || echo ABSENT)"
    if [ "$dev" != "$DEFAULT_TREE" ] || [ "$main_tree" != "$DEFAULT_TREE" ]; then
        case_fail "$name" "develop^{tree}=$dev main^{tree}=$main_tree, want $DEFAULT_TREE"
        return
    fi
    case_ok "$name"
}

st_default_branches() {
    local name="default mode: develop checked out with only main and develop"
    local dflt="$ST_TMP/dflt" head refs
    if [ ! -d "$dflt/.git" ]; then
        case_fail "$name" "precondition: no default scaffold at $dflt"
        return
    fi
    head="$(git -C "$dflt" symbolic-ref -q HEAD 2>/dev/null || true)"
    refs="$(git -C "$dflt" for-each-ref --format='%(refname)' refs/heads | tr '\n' ' ')"
    if [ "$head" != refs/heads/develop ]; then
        case_fail "$name" "HEAD is '${head:-unreadable}', want refs/heads/develop"
        return
    fi
    if [ "$refs" != "refs/heads/develop refs/heads/main " ]; then
        case_fail "$name" "local branches are '$refs', want exactly develop and main"
        return
    fi
    case_ok "$name"
}

# st_guard_probe <name> <root>: scaffolding <root>/.p49-guard-probe-<pid> must
# be refused for the right reason and leave nothing behind. If the guard fails
# to refuse, remove exactly the probe path it created, and only after proving
# it is that path: directly under <root>, carrying the probe name, and absent
# before this case ran.
st_guard_probe() {
    local name="$1" root="$2"
    local probe_name=".p49-guard-probe-$$"
    local probe="$root/$probe_name" rc=0
    if [ -z "$root" ] || [ ! -d "$root" ]; then
        case_fail "$name" "precondition: root '$root' is not a directory"
        return
    fi
    if [ -e "$probe" ]; then
        case_fail "$name" "precondition: probe path already exists: $probe"
        return
    fi
    bash "$SELF" "$probe" >"$ST_TMP/probe.out" 2>"$ST_TMP/probe.err" || rc=$?
    if [ -e "$probe" ]; then
        if [ "$(dirname -- "$probe")" = "$root" ] && [ "$(basename -- "$probe")" = "$probe_name" ]; then
            rm -rf -- "$probe"
        fi
        case_fail "$name" "the scaffold was not refused (exit $rc) and created $probe; removed it"
        return
    fi
    if [ "$rc" -eq 0 ]; then
        case_fail "$name" "exit 0 without creating the probe; the refusal did not happen"
        return
    fi
    if [ "$(st_count 'refusing to scaffold inside this checkout' "$ST_TMP/probe.err")" -lt 1 ]; then
        case_fail "$name" "exit $rc but stderr lacks 'refusing to scaffold inside this checkout'"
        return
    fi
    case_ok "$name"
}

st_control_existing_dest() {
    local name="control: refuses an existing destination"
    local existing="$ST_TMP/existing" rc=0
    mkdir "$existing"
    if [ -n "$(find "$existing" -mindepth 1 -print -quit)" ]; then
        case_fail "$name" "precondition: $existing is not empty"
        return
    fi
    bash "$SELF" "$existing" >"$ST_TMP/existing.out" 2>"$ST_TMP/existing.err" || rc=$?
    if [ "$rc" -eq 0 ]; then
        case_fail "$name" "an existing destination was accepted (exit 0)"
        return
    fi
    if [ "$(st_count 'destination already exists' "$ST_TMP/existing.err")" -lt 1 ]; then
        case_fail "$name" "exit $rc but stderr lacks 'destination already exists'"
        return
    fi
    if [ -n "$(find "$existing" -mindepth 1 -print -quit)" ]; then
        case_fail "$name" "refused, but $existing is no longer empty"
        return
    fi
    case_ok "$name"
}

st_control_unknown_option() {
    local name="control: unknown option refused"
    local rc=0
    bash "$SELF" --no-such-flag >"$ST_TMP/unknown.out" 2>"$ST_TMP/unknown.err" || rc=$?
    if [ "$rc" -eq 0 ]; then
        case_fail "$name" "--no-such-flag was accepted (exit 0)"
        return
    fi
    if [ "$(st_count 'unknown option: --no-such-flag' "$ST_TMP/unknown.err")" -ne 1 ]; then
        case_fail "$name" "exit $rc but stderr lacks 'unknown option: --no-such-flag'"
        return
    fi
    case_ok "$name"
}

run_selftest() {
    # Hermetic: the host checkout's git environment and global/system config
    # (signing, hooks, excludes) may not leak into the scratch repositories.
    local -a git_local_env_vars
    mapfile -t git_local_env_vars < <(git rev-parse --local-env-vars)
    unset "${git_local_env_vars[@]}"
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    ST_TMP="$(mktemp -d)"
    trap 'rm -rf "$ST_TMP"' EXIT

    st_live_topology
    st_live_check_passes
    st_control_compact_config
    st_control_head_on_main
    st_default_tree
    st_default_branches
    st_guard_probe "control: refuses a destination inside this checkout" "$REPO_ROOT"
    # The main checkout, computed here independently of the guard's own
    # variables: the parent of the absolute common git dir. From a linked
    # worktree it differs from $REPO_ROOT, which is the gap this control covers.
    st_guard_probe "control: refuses a destination inside the main checkout" \
        "$(dirname -- "$(git -C "$SCRIPT_DIR" rev-parse --path-format=absolute --git-common-dir)")"
    st_control_existing_dest
    st_control_unknown_option

    echo "SELFTEST passed=$N_PASS failed=$N_FAIL"
    [ "$N_FAIL" -eq 0 ] && [ "$N_PASS" -gt 0 ]
}

# ---------------------------------------------------------------------------
# --check <dest>: read-only verification of a --live-payload scaffold. One
# named line per check, `CHECK passed=<P> failed=<F>` last, exit 0 only if
# F=0 and P>0. `git --no-optional-locks` keeps `status` from rewriting the
# index, so the destination is not modified.
# ---------------------------------------------------------------------------
check_ref_heads() {
    local dest="$1" missing="" branch
    for branch in main integration; do
        git -C "$dest" show-ref --verify -q "refs/heads/$branch" 2>/dev/null || missing="$missing $branch"
    done
    if [ -z "$missing" ]; then
        case_ok "main and integration exist"
    else
        case_fail "main and integration exist" "missing local branch(es):$missing"
    fi
}

# check_exact_line <name> <file> <line>: the file holds a line exactly <line>.
check_exact_line() {
    if rg -qxF -e "$3" -- "$2" 2>/dev/null; then
        case_ok "$1"
    else
        case_fail "$1" "no line exactly '$3' in $2"
    fi
}

check_config() {
    local cfg="$1" keys
    # Order: workflow._auto_chain_active, workflow.auto_mode,
    # workflow.security_enforcement, workflow.nyquist_validation,
    # workflow.verifier, workflow.use_worktrees, git.base_branch,
    # git.branching_strategy. `getpath` yields null for an absent key, so
    # absent never reads as false.
    local want='[false,false,false,true,true,false,"main","none"]'
    if [ ! -f "$cfg" ]; then
        case_fail "config.json is in pretty form" "$cfg is missing"
        case_fail "config.json keys resolve as required" "$cfg is missing"
        return
    fi
    if cmp -s <(jq --indent 2 . "$cfg" 2>/dev/null) "$cfg"; then
        case_ok "config.json is in pretty form"
    else
        case_fail "config.json is in pretty form" "bytes differ from jq --indent 2 of the file (or it is not JSON)"
    fi
    keys="$(jq -c '[getpath(["workflow","_auto_chain_active"]), getpath(["workflow","auto_mode"]),
        getpath(["workflow","security_enforcement"]), getpath(["workflow","nyquist_validation"]),
        getpath(["workflow","verifier"]), getpath(["workflow","use_worktrees"]),
        getpath(["git","base_branch"]), getpath(["git","branching_strategy"])]' "$cfg" 2>/dev/null ||
        echo UNREADABLE)"
    if [ "$keys" = "$want" ]; then
        case_ok "config.json keys resolve as required"
    else
        case_fail "config.json keys resolve as required" "got $keys, want $want"
    fi
}

run_check() {
    local dest="$1" head status
    head="$(git -C "$dest" symbolic-ref -q HEAD 2>/dev/null || true)"
    if [ "$head" = refs/heads/integration ]; then
        case_ok "head is integration"
    else
        case_fail "head is integration" "HEAD is '${head:-detached or unreadable}'"
    fi
    if ! status="$(git --no-optional-locks -C "$dest" status --porcelain 2>&1)"; then
        case_fail "working tree clean" "git status failed: ${status%%$'\n'*}"
    elif [ -n "$status" ]; then
        case_fail "working tree clean" "dirty, first entry: ${status%%$'\n'*}"
    else
        case_ok "working tree clean"
    fi
    check_ref_heads "$dest"
    check_exact_line "devflow.toml sets base_branch integration" "$dest/devflow.toml" 'base_branch = "integration"'
    check_exact_line "devflow.toml sets capture_retention 64" "$dest/devflow.toml" 'capture_retention = 64'
    check_config "$dest/.planning/config.json"
    check_exact_line "roadmap has the phase 1 heading" "$dest/.planning/ROADMAP.md" '### Phase 1: Retry budget constant'
    echo "CHECK passed=$N_PASS failed=$N_FAIL"
    [ "$N_FAIL" -eq 0 ] && [ "$N_PASS" -gt 0 ]
}

case "$MODE" in
    selftest)
        run_selftest
        exit 0
        ;;
    check)
        run_check "$CHECK_DEST"
        exit 0
        ;;
esac

DEST="${DEST:-${TMPDIR:-/tmp}/devflow-dogfood-scratch-$$}"

# T-23-01 (Tampering, high, mitigate): refuse any destination inside this
# checkout — resolve both to absolute paths and compare prefixes.
DEST_ABS="$(realpath -m -- "$DEST")"
if [ "$DEST_ABS" = "$REPO_ROOT" ] || case "$DEST_ABS" in "$REPO_ROOT"/*) true ;; *) false ;; esac; then
    echo "ERROR: refusing to scaffold inside this checkout ($REPO_ROOT)." >&2
    echo "       Requested destination: $DEST_ABS" >&2
    echo "       Pick a destination outside the checkout (e.g. under \$TMPDIR or ~)." >&2
    exit 1
fi

if [ -e "$DEST_ABS" ]; then
    echo "ERROR: destination already exists: $DEST_ABS" >&2
    echo "       Remove it first, or pick a different destination." >&2
    exit 1
fi

mkdir -p "$DEST_ABS"

# Force the initial branch to "main" regardless of the operator's
# init.defaultBranch config, without requiring a git version new enough
# for `git init -b`.
git init -q "$DEST_ABS"
git -C "$DEST_ABS" symbolic-ref HEAD refs/heads/main

# T-23-02 (Tampering, high, mitigate): repo-local identity only — never
# --global, and no GIT_* env export (999.37 class).
git -C "$DEST_ABS" config user.name "DevFlow Probe"
git -C "$DEST_ABS" config user.email "devflow-probe@localhost"

# ---------------------------------------------------------------------------
# --live-payload (Phase 49). Diverges from the default mode here, after the
# shared init/identity block; the default mode below is untouched.
# ---------------------------------------------------------------------------
scaffold_live_payload() {
    # RESEARCH Pitfall 7: the operator's global config signs commits and tags,
    # and the headless monitor's merge and CHANGELOG commits cannot answer a
    # signing prompt. Repo-local and this mode only (T-23-02).
    git -C "$DEST_ABS" config commit.gpgsign false
    git -C "$DEST_ABS" config tag.gpgsign false

    mkdir -p "$DEST_ABS/.planning"

    cat > "$DEST_ABS/.gitignore" <<'EOF'
.devflow/
.worktrees/
__pycache__/
*.pyc
EOF

    # Written in serde_json::to_string_pretty's exact rendering with a trailing
    # newline, and with `_auto_chain_active` present and false, so DevFlow's
    # Code-stage chain-flag set/clear round trip leaves no tracked diff for
    # /gsd-ship to trip on (RESEARCH Pitfall 3, gsd_config.rs:133-178).
    # Non-default keys, one reason each:
    # `auto_mode` false: the ephemeral chain flag stays the only auto-approval path.
    # `security_enforcement` false: removes GSD's blocking `ship:pre` gate.
    # `verifier` true: keeps VERIFICATION.md, which /gsd-ship requires.
    # `nyquist_validation` true: keeps the Validate stage doing real work.
    # `ui_phase`/`ui_safety_gate`/`ui_review`/`schema_drift_gate` false: remove default-active wave gates.
    # `use_worktrees` false: no nested executor worktrees inside DevFlow's phase worktree.
    # `git.branching_strategy` none: GSD stays on the branch DevFlow created.
    # `git.base_branch` main: fixes GSD's PR base instead of reading origin/HEAD (R-5).
    cat > "$DEST_ABS/.planning/config.json" <<'EOF'
{
  "commit_docs": true,
  "parallelization": false,
  "workflow": {
    "auto_mode": false,
    "_auto_chain_active": false,
    "verifier": true,
    "nyquist_validation": true,
    "security_enforcement": false,
    "ui_phase": false,
    "ui_safety_gate": false,
    "ui_review": false,
    "schema_drift_gate": false,
    "code_review": true,
    "tdd_mode": false,
    "use_worktrees": false,
    "research": false,
    "plan_check": false
  },
  "git": {
    "branching_strategy": "none",
    "base_branch": "main"
  }
}
EOF

    cat > "$DEST_ABS/.planning/ROADMAP.md" <<'EOF'
# Roadmap

### Phase 1: Retry budget constant

**Goal**: The probe module defines a default retry budget chosen from three offered values, and a helper that
computes a retry deadline from a start time.
**Requirements**: BUDGET-01
**Success Criteria** (what must be TRUE):

  1. `python3 -m unittest discover -s tests -v` passes.
  2. `probe.budget.DEFAULT_RETRY_BUDGET_SECS` is one of 30, 120 or 300.

**Plans**: 1 plan

- [ ] 01-01-PLAN.md — choose the retry budget, then implement the module the existing test exercises
EOF

    # R-8: non-breaking `chore:` messages and no tags, so the MAJOR-bump
    # classifier has nothing to fire on (D-21).
    git -C "$DEST_ABS" add -A
    git -C "$DEST_ABS" commit -q -m "chore: scaffold devflow live-run payload"
    git -C "$DEST_ABS" checkout -q -b integration

    cat > "$DEST_ABS/devflow.toml" <<'EOF'
base_branch = "integration"
capture_retention = 64
EOF

    git -C "$DEST_ABS" add devflow.toml
    git -C "$DEST_ABS" commit -q -m "chore: configure devflow base branch"

    # `integration` stays checked out: DevFlow reads devflow.toml from the
    # launch checkout's working tree (RESEARCH Pitfall 5).
    echo "Scaffolded devflow live-run payload: $DEST_ABS"
    echo "next: create the private repository and push main then integration (runbook)"
}

if [ "$MODE" = live-payload ]; then
    scaffold_live_payload
    exit 0
fi

mkdir -p "$DEST_ABS/.planning/phases/01-add-probe-marker"

cat > "$DEST_ABS/.planning/PROJECT.md" <<'EOF'
# Devflow Probe Target

## What This Is

A throwaway, single-purpose scratch repository scaffolded by
`scripts/scratch-dogfood-repo.sh` (DevFlow phase 23a) solely to give
`devflow start --phase 1` a legal, isolated target for probing the
Define->Ship pipeline mechanism. Nothing in this repo is meant to be kept.

## Core Value

Prove the pipeline mechanism carries a trivial change end-to-end — not to
build anything real.

## Requirements

### Active

- Phase 1: add a one-line marker file (`PROBE.md`) — deliberately trivial
  so Define/Plan exercise the pipeline, not the work.
EOF

cat > "$DEST_ABS/.planning/ROADMAP.md" <<'EOF'
# Roadmap

### Phase 1: Add PROBE marker file

Goal: create `PROBE.md` at the repo root containing exactly one line,
proving the pipeline can carry a trivial change from Define through Ship.

Status: not started
EOF

cat > "$DEST_ABS/.planning/STATE.md" <<'EOF'
---
gsd_state_version: 1.0
current_phase: 1
current_phase_name: Add PROBE marker file
status: planned
---

# Devflow Probe Target — Project State

## Active Phase

Phase 1 — Add PROBE marker file. Plan 01-01 is pre-written (see
`.planning/phases/01-add-probe-marker/01-01-PLAN.md`) so Define/Plan have
a real artifact to work against rather than inventing scope during a
probe.

## Current Position

Phase: 1
Plan: 01-01
Status: Planned — ready for `devflow start`
EOF

cat > "$DEST_ABS/.planning/phases/01-add-probe-marker/01-01-PLAN.md" <<'EOF'
---
phase: 01-add-probe-marker
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - PROBE.md
autonomous: true
---

<objective>
Create `PROBE.md` containing exactly one line, proving DevFlow's
Define->Ship pipeline can carry a trivial change end-to-end. This is a
throwaway scratch repo (scripts/scratch-dogfood-repo.sh) — the change
itself has no value beyond exercising the pipeline mechanism.
</objective>

<tasks>

<task type="auto">
  <name>Task 1: Create PROBE.md</name>
  <files>PROBE.md</files>
  <action>
Create a file named `PROBE.md` at the repository root containing exactly
one line: `probe ok`.
  </action>
  <acceptance_criteria>
    - `PROBE.md` exists at the repository root
    - Its content is exactly `probe ok`
  </acceptance_criteria>
  <verify>
    <automated>test "$(cat PROBE.md)" = "probe ok" && echo PROBE_OK</automated>
  </verify>
  <done>`PROBE.md` exists with the single line `probe ok`.</done>
</task>

</tasks>

<verification>
1. `PROBE.md` exists and contains exactly `probe ok`.
</verification>

<success_criteria>
- The pipeline produced a single committed file with the exact expected
  content.
</success_criteria>

<output>
Create `.planning/phases/01-add-probe-marker/01-01-SUMMARY.md` when done.
</output>
EOF

cat > "$DEST_ABS/.planning/config.json" <<'EOF'
{
  "workflow": {}
}
EOF

git -C "$DEST_ABS" add -A
git -C "$DEST_ABS" commit -q -m "chore: scaffold devflow probe target"
git -C "$DEST_ABS" branch develop
git -C "$DEST_ABS" checkout -q develop

echo "Scaffolded devflow probe target: $DEST_ABS"
echo "Next: devflow start --phase 1 --agent claude --mode auto $DEST_ABS"

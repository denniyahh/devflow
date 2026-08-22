#!/usr/bin/env bash
#
# scripts/cut-release.sh — the step-gated release cut.
#
# `devflow release --check` (pre-cut preflight) and `--verify` (post-cut
# verification) are read-only; this script is the glue that runs the release
# steps IN ORDER and refuses to proceed out of order. The two merge steps
# pause for a human (develop/main are PR-protected and merge on GitHub); the
# tag, sync, and publish steps run here, each gated on the prior step's
# invariant.
#
# Usage (one step at a time, strictly in order):
#   scripts/cut-release.sh check          # devflow release --check (preflight)
#   scripts/cut-release.sh branch         # create release/vX.Y.Z off develop
#   scripts/cut-release.sh pr-develop     # open the bump+changelog PR -> develop
#   scripts/cut-release.sh pr-main        # open develop -> main (squash) PR
#   scripts/cut-release.sh tag            # sign vX.Y.Z on origin/main (maintainer key)
#   scripts/cut-release.sh sync           # sync main back into develop (PR, merge-commit)
#   scripts/cut-release.sh publish        # cargo publish devflow-core then devflow
#   scripts/cut-release.sh github-release # create GitHub Release with CHANGELOG notes
#   scripts/cut-release.sh docs           # deploy documentation wiki to GitHub Pages
#   scripts/cut-release.sh verify         # devflow release --verify (post-cut)
#
# Every step re-verifies its precondition and refuses with a fix hint rather
# than silently proceeding out of order — the failure mode that made the
# v2.5.0 cut need three corrective passes (commit on develop, tag on develop,
# skipped sync).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
cd "$REPO_ROOT"

die() { echo "ERROR: $*" >&2; exit 1; }
note() { echo "[release] $*"; }

run_devflow() {
    if command -v devflow >/dev/null 2>&1; then
        devflow "$@"
    else
        cargo run -q -- "$@"
    fi
}

STEP="${1:-}"
shift || true

# --- helpers ---------------------------------------------------------------

require_clean() {
    [ -z "$(git status --porcelain)" ] || die "working tree is not clean"
}

require_develop() {
    [ "$(git rev-parse --abbrev-ref HEAD)" = "develop" ] || \
        die "must be on develop (currently on '$(git rev-parse --abbrev-ref HEAD)')"
}

workspace_version() {
    # Reads [workspace.package] version without a TOML parser: the key is the
    # first bare `version = "…"` under [workspace.package].
    sed -n '/^\[workspace.package\]/,/^\[/p' Cargo.toml | sed -n 's/^version = "\([^"]*\)".*/\1/p' | head -1
}

require_merged_to_develop() {
    # A step that opens a PR depends on the previous one having merged; check
    # that the named string is reachable on origin/develop.
    #
    # Uses `git log --grep` + command substitution rather than piping into
    # `grep -q`: under `set -o pipefail`, `grep -q` exits at the first match
    # and SIGPIPEs the still-streaming `git log`, turning a successful match
    # into a non-zero pipeline exit (and a spurious "merge the previous PR
    # first" refusal).
    local needle="$1"
    git fetch origin develop main --quiet
    local found
    found="$(git log --oneline origin/develop --grep="$needle" -1 2>/dev/null || true)"
    [ -n "$found" ] || \
        die "origin/develop does not yet contain '$needle' — merge the previous PR first"
}

# --- steps -----------------------------------------------------------------

step_check() {
    require_clean
    git fetch origin develop main --quiet
    run_devflow release --check
}

step_branch() {
    require_clean
    require_develop
    local v; v="$(workspace_version)"
    [ -n "$v" ] || die "could not read workspace version from Cargo.toml"

    # Derive baseline semver tag and bump from Conventional Commits
    local target_info
    target_info="$(python3 -c "
import subprocess, re, sys

try:
    tags = subprocess.check_output(['git', 'tag', '--merged', 'HEAD'], stderr=subprocess.DEVNULL).decode().splitlines()
    semver_tags = []
    for t in tags:
        m = re.match(r'^v?(\d+)\.(\d+)\.(\d+)$', t.strip())
        if m:
            semver_tags.append((int(m.group(1)), int(m.group(2)), int(m.group(3)), t.strip()))
    if not semver_tags:
        print('0.1.0\npatch\nNo tags found')
        sys.exit(0)
    semver_tags.sort()
    major, minor, patch, baseline_tag = semver_tags[-1]

    # Resolve anchor
    ancestry = subprocess.check_output(['git', 'rev-list', '--ancestry-path', '--reverse', f'{baseline_tag}..HEAD'], stderr=subprocess.DEVNULL).decode().splitlines()
    anchor = baseline_tag
    for cand in [c.strip() for c in ancestry if c.strip()]:
        try:
            fp = subprocess.check_output(['git', 'rev-parse', f'{cand}^1'], stderr=subprocess.DEVNULL).decode().strip()
            if subprocess.run(['git', 'merge-base', '--is-ancestor', baseline_tag, fp], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode != 0:
                anchor = cand
                break
        except Exception:
            anchor = cand
            break

    commits = subprocess.check_output(['git', 'log', '--no-merges', f'{anchor}..HEAD', '--format=%s'], stderr=subprocess.DEVNULL).decode().splitlines()
    has_breaking = False
    has_feat = False
    for c in commits:
        if '!' in c.split(':', 1)[0] or 'BREAKING CHANGE' in c:
            has_breaking = True
        elif c.startswith('feat'):
            has_feat = True

    if has_breaking:
        bump = 'major'
        next_v = f'{major + 1}.0.0'
    elif has_feat:
        bump = 'minor'
        next_v = f'{major}.{minor + 1}.0'
    else:
        bump = 'patch'
        next_v = f'{major}.{minor}.{patch + 1}'

    print(f'{next_v}\n{bump}\n{baseline_tag}\n{anchor}')
except Exception as e:
    # Fallback to minor bump
    v_clean = '$v'.lstrip('v')
    parts = [int(p) for p in v_clean.split('.')]
    print(f'{parts[0]}.{parts[1] + 1}.0\nminor\nunknown\nunknown')
")"

    local target; target="$(echo "$target_info" | sed -n '1p')"
    local bump_type; bump_type="$(echo "$target_info" | sed -n '2p')"
    local base_tag; base_tag="$(echo "$target_info" | sed -n '3p')"
    local anchor; anchor="$(echo "$target_info" | sed -n '4p')"

    note "detected $bump_type bump (target: v$target, baseline: $base_tag)"
    local branch="release/v$target"
    if git show-ref --verify --quiet "refs/heads/$branch"; then
        note "branch $branch already exists — reusing"
        git checkout -q "$branch"
    else
        git checkout -q -b "$branch"
        note "created $branch off develop"
    fi

    echo ""
    echo "────────────────────────────────────────────────────────────────────────"
    echo "Suggested CHANGELOG.md entry for ## $target — $(date +%Y-%m-%d):"
    echo "────────────────────────────────────────────────────────────────────────"
    python3 -c "
import subprocess

anchor = '$anchor'
try:
    commits = subprocess.check_output(['git', 'log', '--no-merges', f'{anchor}..HEAD', '--format=%s'], stderr=subprocess.DEVNULL).decode().splitlines()
    added, fixed, changed = [], [], []
    for c in commits:
        c = c.strip()
        if not c: continue
        if c.startswith('feat'):
            added.append(c.split(':', 1)[-1].strip())
        elif c.startswith('fix') or c.startswith('perf'):
            fixed.append(c.split(':', 1)[-1].strip())
        elif not c.startswith('release:') and not c.startswith('Merge'):
            changed.append(c.split(':', 1)[-1].strip())

    print(f'## $target — $(date +%Y-%m-%d)\n')
    if added:
        print('### Added\n')
        for item in added: print(f'- {item}')
        print('')
    if fixed:
        print('### Fixed\n')
        for item in fixed: print(f'- {item}')
        print('')
    if changed:
        print('### Changed\n')
        for item in changed: print(f'- {item}')
        print('')
except Exception:
    pass
"
    echo "────────────────────────────────────────────────────────────────────────"
    echo ""
    note "now bump Cargo.toml (two places) to $target and append the CHANGELOG section, then commit"
    note "commit message convention: release: v$target — <description>"
}

step_pr_develop() {
    require_clean
    local v; v="$(workspace_version)"
    local branch="release/v${v#v}"
    [ "$(git rev-parse --abbrev-ref HEAD)" = "$branch" ] || \
        die "must be on '$branch' (run: git checkout $branch)"
    git log --oneline develop..HEAD | grep -q . || die "no commits on $branch yet — bump + changelog first"
    gh pr create --base develop --head "$branch" \
        --title "release: v$v" --body "Cut v$v (version bump + changelog)."
    note "merge the PR into develop, then run: scripts/cut-release.sh pr-main"
}

step_pr_main() {
    require_clean
    require_develop
    require_merged_to_develop "release: v"
    git fetch origin develop main --quiet
    gh pr create --base main --head develop \
        --title "release: v$(workspace_version)" \
        --body "Squash-merge develop into main for v$(workspace_version)."
    note "squash-merge the PR into main, then run: scripts/cut-release.sh tag"
}

step_tag() {
    require_clean
    local v; v="$(workspace_version)"
    git fetch origin main --quiet

    # The release tag MUST be signed with the maintainer's key. If
    # devflow.releaseSigningKey is unset, `git config --get` returns empty and
    # the tag silently signs with whatever `user.signingkey` defaults to — the
    # wrong-identity trap 999.104 catalogues. Fail loudly instead.
    local release_key; release_key="$(git config --get devflow.releaseSigningKey || true)"
    if [ -z "$release_key" ]; then
        echo "cut-release: devflow.releaseSigningKey is not set." >&2
        echo "  Set it: git config --local devflow.releaseSigningKey <path-to-maintainer-key>" >&2
        exit 1
    fi
    local release_key_expanded="${release_key/#\~/$HOME}"
    if [ ! -r "$release_key_expanded" ]; then
        echo "cut-release: devflow.releaseSigningKey points at an unreadable file: $release_key" >&2
        exit 1
    fi

    # The tag MUST land on main's squash commit, not the develop release
    # commit — enforced here, and re-checked by `release --verify`. Pass the
    # tilde-expanded path so git does not rely on its own `~` handling.
    git -c user.signingkey="$release_key_expanded" \
        tag -s "v$v" origin/main -m "release: v$v"
    git verify-tag "v$v"
    git push origin "v$v"
    note "tag v$v signed on origin/main and pushed"
}

step_sync() {
    require_clean
    require_develop
    git fetch origin main develop --quiet
    scripts/sync-main-to-develop.sh
    note "sync pull request created targeting develop (must use 'Create a merge commit')."
    note "after merging on GitHub, run: scripts/sync-main-to-develop.sh --finish"
}

step_publish() {
    require_clean
    # crates.io order: devflow-core before devflow (devflow's path-dependency
    # verifies against the published devflow-core).
    cargo publish -p devflow-core
    cargo publish -p devflow
    note "published devflow-core and devflow (run: scripts/cut-release.sh github-release next)"
}

step_github_release() {
    local v; v="$(workspace_version)"
    local tag="v$v"

    # Verify the tag exists on remote
    git fetch origin --tags --quiet
    git rev-parse --verify --quiet "$tag" >/dev/null || \
        die "tag '$tag' does not exist locally or on origin (run tag step first)"

    # Extract CHANGELOG notes for this version
    local notes
    notes="$(python3 -c "
import sys
with open('CHANGELOG.md', 'r', encoding='utf-8') as f:
    lines = f.readlines()
collecting = False
buf = []
target_header = '## $v'
for line in lines:
    if line.startswith('## '):
        if collecting:
            break
        if line.startswith(target_header):
            collecting = True
            continue
    elif collecting:
        buf.append(line)
print(''.join(buf).strip())
")"

    if [ -z "$notes" ]; then
        die "no changelog entry found in CHANGELOG.md for version $v"
    fi

    if gh release view "$tag" >/dev/null 2>&1; then
        note "GitHub release $tag already exists — skipping creation"
    else
        gh release create "$tag" --title "$tag" --notes "$notes"
        note "created GitHub release $tag (run: scripts/cut-release.sh docs next)"
    fi
}

step_docs() {
    require_clean
    scripts/deploy-docs.sh
    note "documentation deployed to GitHub Pages (run: scripts/cut-release.sh verify next)"
}

step_verify() {
    git fetch origin develop main --quiet
    run_devflow release --verify
}

# --- dispatch --------------------------------------------------------------

case "$STEP" in
    check)          step_check ;;
    branch)         step_branch ;;
    pr-develop)     step_pr_develop ;;
    pr-main)        step_pr_main ;;
    tag)            step_tag ;;
    sync)           step_sync ;;
    publish)        step_publish ;;
    github-release) step_github_release ;;
    docs)           step_docs ;;
    verify)         step_verify ;;
    "" | -h | --help)
        sed -n '12,23p' "${BASH_SOURCE[0]}"
        exit 0 ;;
    *) die "unknown step '$STEP' (see --help)" ;;
esac

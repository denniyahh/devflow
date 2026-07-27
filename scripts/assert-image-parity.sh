#!/usr/bin/env bash
# Fail if the image CI runs in differs from the one .devcontainer/devcontainer.json
# declares. GitHub Actions cannot read a job's container image from a file, so
# the tag is necessarily duplicated in .github/workflows/ci.yml; this turns
# that duplication from a silent rot risk into a hard failure.
#
# Without this, bumping only one side reintroduces exactly the local-vs-CI
# divergence the container parity work was done to eliminate — and it would
# look green until something environment-sensitive broke.
set -euo pipefail

CI_IMAGE="${1:?usage: assert-image-parity.sh <image-used-by-ci>}"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
DEVCONTAINER="$REPO_ROOT/.devcontainer/devcontainer.json"

if [ ! -f "$DEVCONTAINER" ]; then
    echo "error: $DEVCONTAINER not found" >&2
    exit 1
fi

# Strip // comments (devcontainer.json permits them) before matching.
DC_IMAGE="$(sed 's|//.*||' "$DEVCONTAINER" \
    | grep -o '"image"[[:space:]]*:[[:space:]]*"[^"]*"' \
    | head -1 | sed 's|.*"image"[[:space:]]*:[[:space:]]*"||; s|"$||')"

if [ -z "$DC_IMAGE" ]; then
    echo "error: could not read \"image\" from $DEVCONTAINER" >&2
    exit 1
fi

if [ "$CI_IMAGE" != "$DC_IMAGE" ]; then
    cat >&2 <<EOF
error: CI image does not match the devcontainer definition.

  .github/workflows/ci.yml : $CI_IMAGE
  .devcontainer/...json    : $DC_IMAGE

These must be identical or local checks stop predicting CI. Update both.
EOF
    exit 1
fi

echo "image parity OK: $CI_IMAGE"

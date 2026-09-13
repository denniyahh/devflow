#!/usr/bin/env bash
# Install the dependency-hygiene tools that `scripts/check.sh deps` runs.
#
# This is the single place their versions are pinned. CI's advisory
# `Dependency checks` job runs it before `scripts/check.sh deps`, and a
# developer can run it too. Neither tool ships in the pinned devcontainer image,
# which is why `deps` is not part of `scripts/check.sh all`.
set -euo pipefail

CARGO_DENY_VERSION=0.20.2
CARGO_MACHETE_VERSION=0.9.2

cargo install --locked "cargo-deny@${CARGO_DENY_VERSION}" "cargo-machete@${CARGO_MACHETE_VERSION}"

#!/usr/bin/env bash
# Deploy the wiki to GitHub Pages (gh-pages branch).
# Run from the project root.
set -euo pipefail

DOCS_DIR="${1:-docs}"
MKDOCS_CONFIG="${DOCS_DIR}/mkdocs.yml"

if [ ! -f "$MKDOCS_CONFIG" ]; then
    echo "❌ Not found: $MKDOCS_CONFIG"
    echo "   Run scaffold.sh first."
    exit 1
fi

REPO_URL=$(git remote get-url origin 2>/dev/null || true)
if [ -z "$REPO_URL" ]; then
    echo "❌ No git remote 'origin' found."
    exit 1
fi

echo "🚀 Deploying wiki to GitHub Pages..."
echo "   Repo: $REPO_URL"
echo ""

# Enable GitHub Pages in repo settings (source: gh-pages branch)
if command -v gh &>/dev/null; then
    REPO=$(echo "$REPO_URL" | sed 's|.*github.com[:/]||; s|\.git$||')
    gh api "repos/${REPO}/pages" -X PUT -F "source[branch]=gh-pages" -F "source[path]=/" 2>/dev/null && \
        echo "   ✓ GitHub Pages configured (source: gh-pages)" || \
        echo "   ⚠️  Could not configure Pages via API (may need manual setup)"
fi

# Build and push to gh-pages
mkdocs gh-deploy --config-file "$MKDOCS_CONFIG" --force --message "Deploy wiki [skip ci]"

echo ""
echo "✅ Deployed!"
REPO_PATH=$(echo "$REPO_URL" | sed 's|.*github.com[:/]||; s|\.git$||')
echo "   https://${REPO_PATH%/*}.github.io/${REPO_PATH#*/}"

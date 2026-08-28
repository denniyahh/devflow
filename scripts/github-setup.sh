#!/usr/bin/env bash
# DevFlow GitHub Setup Script
# Automates Tier 1 & 2 GitHub repository configuration
# Requirements: gh CLI (https://github.com/cli/cli)
#
# Usage: scripts/github-setup.sh [--dry-run]

set -euo pipefail

REPO="${DEVFLOW_REPO:-denniyahh/devflow}"
DRY_RUN=0

for arg in "$@"; do
    case "$arg" in
        --dry-run|-n)
            DRY_RUN=1
            ;;
        --help|-h)
            echo "Usage: $0 [--dry-run]"
            echo ""
            echo "Options:"
            echo "  --dry-run, -n    Preview changes without applying them"
            echo "  --help, -h       Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--dry-run]" >&2
            exit 2
            ;;
    esac
done

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

run_gh() {
    local description="$1"
    shift
    local args=("$@")

    log_info "$description"
    if [[ $DRY_RUN -eq 1 ]]; then
        echo "  [DRY RUN] gh ${args[*]}"
        return 0
    fi

    if gh "${args[@]}"; then
        log_success "Done: $description"
    else
        log_error "Failed: $description"
        return 1
    fi
    echo ""
}

# Check prerequisites
check_gh_cli() {
    if ! command -v gh &> /dev/null; then
        log_error "gh CLI not found. Install from: https://github.com/cli/cli#installation"
        exit 1
    fi
    local gh_version
    gh_version="$(gh --version | head -n 1)"
    log_success "gh CLI found: $gh_version"
    echo ""
}

# Verify authentication
check_auth() {
    log_info "Verifying GitHub authentication..."
    if ! gh auth status &> /dev/null; then
        log_error "Not authenticated with GitHub. Run: gh auth login"
        exit 1
    fi
    log_success "Authenticated with GitHub"
    echo ""
}

# Tier 1: Quick Wins
tier1_topics() {
    log_info "=== TIER 1: Quick Wins ==="
    echo ""

    local topics="rust,cli,automation,ai,workflow,coding-agents,devops"
    run_gh "Adding repository topics: $topics" repo edit "$REPO" --add-topic "$topics"
}

tier1_homepage() {
    run_gh "Setting homepage to crates.io" repo edit "$REPO" --homepage "https://crates.io/crates/devflow"
}

tier1_description() {
    local desc="An opinionated AI development workflow automation CLI — automates branching, monitoring, verifying, documenting, and shipping for AI coding agents"
    run_gh "Updating repository description" repo edit "$REPO" --description "$desc"
}

# Tier 2: Medium Impact
tier2_features() {
    log_info "=== TIER 2: Medium Impact ==="
    echo ""

    run_gh "Enabling GitHub Discussions and Projects" repo edit "$REPO" --enable-discussions --enable-projects
}

tier2_projects_guide() {
    log_info "GitHub Projects Roadmap Setup (Manual Guide)"
    echo ""

    cat <<'EOF'
To configure the DevFlow Roadmap project via GitHub UI:

1. Go to: https://github.com/denniyahh/devflow/projects
2. Click "New project"
3. Name: "DevFlow Roadmap"
4. Set visibility: Public
5. Choose template: Table or Board
6. Add recommended views / columns:
   - 📋 Planned (upcoming phases)
   - 🚀 In Progress (current phases)
   - ✅ Completed (recent releases)
   - ⏸️ Deferred (known gaps)

EOF
}

# Verify branch-specific files exist (actual check, not hardcoded assumption)
verify_files() {
    log_info "=== Verifying Repository Files ==="
    echo ""

    local files_to_check=("INSTALL_VERIFY.md" ".github/FUNDING.yml")
    local repo_root
    repo_root="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

    for file in "${files_to_check[@]}"; do
        if [[ -f "$repo_root/$file" ]]; then
            log_success "Found locally: $file"
        elif git ls-tree -r origin/develop --name-only 2>/dev/null | grep -q "^$file\$"; then
            log_success "Found on origin/develop: $file"
        elif git ls-tree -r develop --name-only 2>/dev/null | grep -q "^$file\$"; then
            log_success "Found on develop branch: $file"
        else
            log_warn "Missing: $file (not found in working tree or develop branch)"
        fi
    done
    echo ""
}

# Main execution
main() {
    if [[ $DRY_RUN -eq 1 ]]; then
        log_warn "Running in DRY RUN mode. No changes will be made."
        echo ""
    fi

    echo -e "${BLUE}════════════════════════════════════════${NC}"
    echo -e "${BLUE}  DevFlow GitHub Setup Script${NC}"
    echo -e "${BLUE}════════════════════════════════════════${NC}"
    echo ""

    check_gh_cli
    check_auth

    # Tier 1
    tier1_topics
    tier1_homepage
    tier1_description

    # Tier 2
    tier2_features
    tier2_projects_guide

    # Verify
    verify_files

    # Summary
    echo -e "${BLUE}════════════════════════════════════════${NC}"
    if [[ $DRY_RUN -eq 1 ]]; then
        log_warn "DRY RUN complete: Review the planned actions above"
    else
        log_success "GitHub setup execution complete!"
    fi
    echo -e "${BLUE}════════════════════════════════════════${NC}"

    cat <<'EOF'

Next steps:
1. Visit: https://github.com/denniyahh/devflow
2. Verify topics, homepage, discussions, and description
3. Create GitHub Projects board for roadmap (if needed)

EOF
}

main "$@"

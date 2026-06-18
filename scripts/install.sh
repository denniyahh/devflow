#!/usr/bin/env bash
#
# DevFlow Installer
# One-command bootstrap for any POSIX system.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash
#
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log()  { echo -e "${GREEN}[✓]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }
err()  { echo -e "${RED}[✗]${NC} $1"; exit 1; }

echo "DevFlow Installer"
echo "================="
echo ""

# ---- Detect OS ----
OS="$(uname -s)"
case "$OS" in
    Linux)  OS=linux ;;
    Darwin) OS=macos ;;
    *)      err "Unsupported OS: $OS" ;;
esac
log "Detected OS: $OS"

# ---- Check/install git ----
if command -v git &>/dev/null; then
    log "git: $(git --version | cut -d' ' -f3)"
else
    warn "git not found"
    case "$OS" in
        linux)
            if command -v apt-get &>/dev/null; then
                sudo apt-get update -qq && sudo apt-get install -y -qq git
            elif command -v dnf &>/dev/null; then
                sudo dnf install -y git
            else
                err "Install git manually: https://git-scm.com/downloads"
            fi
            ;;
        macos)
            if command -v brew &>/dev/null; then
                brew install git
            else
                err "Install Homebrew first: https://brew.sh"
            fi
            ;;
    esac
    log "git: installed"
fi

# ---- Check/install Rust ----
if command -v cargo &>/dev/null; then
    RUST_VERSION="$(cargo --version | cut -d' ' -f2)"
    log "cargo: $RUST_VERSION"
else
    warn "Rust not found — installing via rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    log "Rust: installed"
fi

# ---- Install devflow ----
echo ""
echo "Installing DevFlow..."
cargo install devflow 2>/dev/null || {
    warn "cargo install failed — building from source"
    TEMPDIR="$(mktemp -d)"
    git clone https://github.com/denniyahh/devflow.git "$TEMPDIR"
    cd "$TEMPDIR"
    cargo build --release
    mkdir -p "$HOME/.local/bin"
    cp target/release/devflow "$HOME/.local/bin/devflow"
    cd - > /dev/null
    rm -rf "$TEMPDIR"
}
log "DevFlow: $(devflow --version 2>/dev/null || echo 'installed')"

# ---- Check optional deps ----
echo ""
echo "Optional dependencies:"

check_opt() {
    local name="$1" cmd="$2" install_hint="$3"
    if command -v "$cmd" &>/dev/null; then
        log "$name: found"
    else
        warn "$name: not found — install: $install_hint"
    fi
}

check_opt "gh CLI"    gh      "brew install gh / apt install gh"
check_opt "Claude"    claude  "npm i -g @anthropic-ai/claude-code"
check_opt "Codex"     codex   "npm i -g @openai/codex"
check_opt "OpenCode"  opencode "cargo install opencode"

# ---- Verify ----
echo ""
echo "Running devflow doctor..."
devflow doctor 2>/dev/null || warn "devflow doctor not available yet — run manually after setup"

echo ""
log "Installation complete. Run 'devflow doctor' to verify."

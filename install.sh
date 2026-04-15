#!/bin/bash
# Installer for ultranano (un)

set -euo pipefail

REPO="stackwill/ultranano"
INSTALL_DIR="$HOME/.local/bin"
BIN_NAME="un"

# Colors
RESET='\033[0m'
BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[38;5;114m'
CYAN='\033[38;5;117m'
YELLOW='\033[38;5;221m'
RED='\033[38;5;203m'
GRAY='\033[38;5;245m'

step() { printf "\n ${CYAN}${BOLD}→${RESET} ${BOLD}%s${RESET}\n" "$1"; }
ok()   { printf "   ${GREEN}✓${RESET} ${DIM}%s${RESET}\n" "$1"; }
warn() { printf "   ${YELLOW}!${RESET} ${DIM}%s${RESET}\n" "$1"; }
die()  { printf "\n   ${RED}✗${RESET} ${BOLD}%s${RESET}\n\n" "$1" >&2; exit 1; }

# Header
printf "\n${BOLD}  ultranano${RESET}${GRAY} installer${RESET}\n"
printf "  ${GRAY}────────────────────${RESET}\n"

# Detect OS
step "Detecting platform"
OS="$(uname -s)"
ARCH="$(uname -m)"

[[ "$OS" == "Linux" ]] || die "Only Linux is supported (detected: $OS)"

case "$ARCH" in
    x86_64)        ARCH_NAME="x86_64" ;;
    arm64|aarch64) ARCH_NAME="aarch64" ;;
    *)             die "Unsupported architecture: $ARCH" ;;
esac

ok "Linux / $ARCH_NAME"

# Download
BINARY_NAME="${BIN_NAME}-linux-${ARCH_NAME}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}"

step "Downloading latest release"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

if command -v curl &> /dev/null; then
    curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP" 2>&1 | sed 's/^/   /'
elif command -v wget &> /dev/null; then
    wget -qO "$TMP" "$DOWNLOAD_URL"
else
    die "curl or wget is required"
fi

ok "Downloaded $BINARY_NAME"

# Install
step "Installing"
mkdir -p "$INSTALL_DIR"
cp "$TMP" "${INSTALL_DIR}/${BIN_NAME}"
chmod +x "${INSTALL_DIR}/${BIN_NAME}"
ok "Installed to ${INSTALL_DIR}/${BIN_NAME}"

# PATH
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*)
        ok "Already in PATH"
        ;;
    *)
        SHELL_CONFIG=""
        if [[ "$SHELL" == *"bash"* ]]; then
            SHELL_CONFIG="$HOME/.bashrc"
        elif [[ "$SHELL" == *"zsh"* ]]; then
            SHELL_CONFIG="$HOME/.zshrc"
        elif [[ "$SHELL" == *"fish"* ]]; then
            SHELL_CONFIG="$HOME/.config/fish/config.fish"
        fi

        if [[ -n "$SHELL_CONFIG" ]]; then
            printf '\n# Added by ultranano installer\n' >> "$SHELL_CONFIG"
            printf 'export PATH="$PATH:%s"\n' "$INSTALL_DIR" >> "$SHELL_CONFIG"
            ok "Added to PATH via $SHELL_CONFIG"
            warn "Run 'source $SHELL_CONFIG' or restart your terminal"
        fi
        ;;
esac

# Done
printf "\n  ${GRAY}────────────────────${RESET}\n"
printf "  ${GREEN}${BOLD}Ready!${RESET}  run ${BOLD}un <filename>${RESET} to start editing\n\n"

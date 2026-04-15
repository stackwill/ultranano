#!/bin/bash
# Uninstaller for ultranano (un)

set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
BIN_NAME="un"

echo "Uninstalling ultranano (un)..."

if [[ -f "${INSTALL_DIR}/${BIN_NAME}" ]]; then
    rm -- "${INSTALL_DIR}/${BIN_NAME}"
    echo "Removed ${INSTALL_DIR}/${BIN_NAME}"
else
    echo "Binary not found at ${INSTALL_DIR}/${BIN_NAME}"
fi

# Offer to remove PATH entry
SHELL_CONFIG=""
if [[ "$SHELL" == *"bash"* ]]; then
    SHELL_CONFIG="$HOME/.bashrc"
elif [[ "$SHELL" == *"zsh"* ]]; then
    SHELL_CONFIG="$HOME/.zshrc"
elif [[ "$SHELL" == *"fish"* ]]; then
    SHELL_CONFIG="$HOME/.config/fish/config.fish"
fi

if [[ -n "$SHELL_CONFIG" ]] && [[ -f "$SHELL_CONFIG" ]]; then
    if grep -q "# Added by ultranano installer" "$SHELL_CONFIG" 2>/dev/null; then
        echo "Remove PATH entry from ${SHELL_CONFIG}? [y/N]"
        read -r response
        if [[ "$response" =~ ^[Yy]$ ]]; then
            if [[ "$(uname)" == "Darwin" ]]; then
                sed -i '' '/# Added by ultranano installer/d' "$SHELL_CONFIG"
                sed -i '' "s|export PATH="\$PATH:${INSTALL_DIR}"||g" "$SHELL_CONFIG"
            else
                sed -i '/# Added by ultranano installer/d' "$SHELL_CONFIG"
                sed -i "s|export PATH="\$PATH:${INSTALL_DIR}"||g" "$SHELL_CONFIG"
            fi
            echo "Removed PATH entry. Restart your terminal."
        fi
    fi
fi

echo "Uninstallation complete."
#!/bin/sh
set -e

# boxr installer & environment setup script
# Usage: curl -fsSL https://raw.githubusercontent.com/kchaitanya863/kc-docker/main/install.sh | sh
#    or: ./install.sh

echo "=========================================="
echo "          Installing boxr 📦             "
echo "  Fast OCI Container Engine & Runtime     "
echo "=========================================="

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${ARCH}" in
    x86_64|amd64)
        BOXR_ARCH="x86_64"
        ;;
    arm64|aarch64)
        BOXR_ARCH="aarch64"
        ;;
    *)
        echo "Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

echo "Detected Platform: ${OS} (${BOXR_ARCH})"

BOXR_HOME="${HOME}/.boxr"
BIN_DIR="${BOXR_HOME}/bin"
COMPLETIONS_DIR="${BOXR_HOME}/completions"

mkdir -p "${BIN_DIR}" "${COMPLETIONS_DIR}"

# Check if building from local repository source or downloading
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd 2>/dev/null || echo ".")"

if [ -f "${SCRIPT_DIR}/Cargo.toml" ]; then
    echo "Building boxr release binary from local source..."
    cargo build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml"
    cp "${SCRIPT_DIR}/target/release/boxr" "${BIN_DIR}/boxr"
elif command -v cargo >/dev/null 2>&1; then
    echo "Cargo detected. Building boxr..."
    cargo install --git https://github.com/kchaitanya863/kc-docker.git --bin boxr --root "${BOXR_HOME}"
else
    echo "Error: cargo (Rust toolchain) is required to build boxr."
    echo "Install Rust via https://rustup.rs or 'brew install rust', then re-run this script."
    exit 1
fi

chmod +x "${BIN_DIR}/boxr"
echo "✓ Installed boxr binary to ${BIN_DIR}/boxr"

# Create Docker drop-in wrapper
cat << 'EOF' > "${BIN_DIR}/docker"
#!/bin/sh
exec "$HOME/.boxr/bin/boxr" "$@"
EOF

chmod +x "${BIN_DIR}/docker"
echo "✓ Installed Docker drop-in wrapper to ${BIN_DIR}/docker"

# Generate shell completions
"${BIN_DIR}/boxr" completion bash > "${COMPLETIONS_DIR}/boxr.bash" 2>/dev/null || true
"${BIN_DIR}/boxr" completion zsh > "${COMPLETIONS_DIR}/_boxr" 2>/dev/null || true

if [ -d "${HOME}/.config/fish" ]; then
    mkdir -p "${HOME}/.config/fish/completions"
    "${BIN_DIR}/boxr" completion fish > "${HOME}/.config/fish/completions/boxr.fish" 2>/dev/null || true
fi
echo "✓ Generated shell completions in ${COMPLETIONS_DIR}"

# Update Shell Profiles
SHELL_NAME="$(basename "${SHELL:-bash}")"
PATH_EXPORT="export PATH=\"\$HOME/.boxr/bin:\$PATH\""

case "${SHELL_NAME}" in
    zsh)
        RC_FILE="${HOME}/.zshrc"
        ;;
    bash)
        RC_FILE="${HOME}/.bashrc"
        ;;
    *)
        RC_FILE="${HOME}/.profile"
        ;;
esac

if [ -f "${RC_FILE}" ]; then
    if ! grep -q ".boxr/bin" "${RC_FILE}"; then
        echo "" >> "${RC_FILE}"
        echo "# boxr container engine" >> "${RC_FILE}"
        echo "${PATH_EXPORT}" >> "${RC_FILE}"
        echo "✓ Added ~/.boxr/bin to ${RC_FILE}"
    fi
fi

echo ""
echo "=========================================="
echo "      boxr installation complete! 🎉      "
echo "=========================================="
echo ""
echo "To get started right away in this shell:"
echo "  export PATH=\"\$HOME/.boxr/bin:\$PATH\""
echo ""
echo "Verify installation:"
echo "  boxr --version"
echo "  docker --version"
echo ""
echo "Run your first container:"
echo "  boxr run hello-world"
echo ""

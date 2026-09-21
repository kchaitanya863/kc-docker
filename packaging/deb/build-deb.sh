#!/usr/bin/env bash
set -euo pipefail

# Build .deb package from compiled boxr binary
# Usage: ./packaging/deb/build-deb.sh <binary_path> <version> <arch: amd64|arm64> <output_dir>

BINARY_PATH="${1:-target/release/boxr}"
VERSION="${2:-0.1.0}"
ARCH="${3:-amd64}"
OUTPUT_DIR="${4:-.}"

BUILD_ROOT=$(mktemp -d)
trap 'rm -rf "$BUILD_ROOT"' EXIT

PACKAGE_DIR="$BUILD_ROOT/boxr_${VERSION}_${ARCH}"
mkdir -p "$PACKAGE_DIR/DEBIAN"
mkdir -p "$PACKAGE_DIR/usr/bin"
mkdir -p "$PACKAGE_DIR/usr/share/bash-completion/completions"
mkdir -p "$PACKAGE_DIR/usr/share/zsh/vendor-completions"
mkdir -p "$PACKAGE_DIR/usr/share/fish/vendor_completions.d"

# Copy binary
cp "$BINARY_PATH" "$PACKAGE_DIR/usr/bin/boxr"
chmod 755 "$PACKAGE_DIR/usr/bin/boxr"

# Generate completions
GEN_CMD="$BINARY_PATH"
if ! "$GEN_CMD" --version >/dev/null 2>&1; then
    if [ -f "target/release/boxr" ] && target/release/boxr --version >/dev/null 2>&1; then
        GEN_CMD="target/release/boxr"
    elif [ -f "target/debug/boxr" ] && target/debug/boxr --version >/dev/null 2>&1; then
        GEN_CMD="target/debug/boxr"
    else
        GEN_CMD="cargo run --quiet --"
    fi
fi

$GEN_CMD completion bash > "$PACKAGE_DIR/usr/share/bash-completion/completions/boxr" 2>/dev/null || true
$GEN_CMD completion zsh > "$PACKAGE_DIR/usr/share/zsh/vendor-completions/_boxr" 2>/dev/null || true
$GEN_CMD completion fish > "$PACKAGE_DIR/usr/share/fish/vendor_completions.d/boxr.fish" 2>/dev/null || true

# Control file
cat << EOF > "$PACKAGE_DIR/DEBIAN/control"
Package: boxr
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: Boxr Contributors <https://github.com/kchaitanya863/boxr>
Description: Fast, lightweight OCI container engine and runtime in Rust
 Boxr is a zero-dependency, rootless-by-default container engine, image
 builder, compose orchestrator, and runtime written in pure Rust.
EOF

dpkg-deb --root-owner-group --build "$PACKAGE_DIR" "$OUTPUT_DIR/boxr_${VERSION}_${ARCH}.deb"
echo "✓ Built $OUTPUT_DIR/boxr_${VERSION}_${ARCH}.deb"

#!/usr/bin/env bash
set -e

BINARY="neon_descent"
DIST="dist"

echo "Building $BINARY..."

# Ensure targets are installed
rustup target add aarch64-apple-darwin 2>/dev/null
rustup target add x86_64-apple-darwin 2>/dev/null

# Build both architectures
echo "  → Apple Silicon (aarch64)"
cargo build --release --target aarch64-apple-darwin

echo "  → Intel (x86_64)"
cargo build --release --target x86_64-apple-darwin

# Stitch into a universal binary
mkdir -p "$DIST"
lipo -create \
  "target/aarch64-apple-darwin/release/$BINARY" \
  "target/x86_64-apple-darwin/release/$BINARY" \
  -output "$DIST/$BINARY"

echo ""
echo "Done. Universal binary: $DIST/$BINARY"
file "$DIST/$BINARY"

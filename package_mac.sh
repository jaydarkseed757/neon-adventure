#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Neon Descent"
BUNDLE_ID="com.jcollins.neondescent"
BINARY_NAME="neon_descent"
VERSION="0.1.0"
APP_DIR="${APP_NAME}.app"

echo "Building release binary..."
cargo build --release

echo "Creating .app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"

echo "Writing Info.plist..."
cat > "$APP_DIR/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.games</string>
</dict>
</plist>
PLIST

echo "Copying binary..."
cp "target/release/${BINARY_NAME}" "$APP_DIR/Contents/MacOS/${BINARY_NAME}"

echo "Stripping quarantine flag..."
xattr -cr "$APP_DIR"

echo ""
echo "Done: ${APP_DIR}"
echo "Save file will appear in the user's home directory (~/${BINARY_NAME}.sav) when launched from Finder."

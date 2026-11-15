#!/usr/bin/env bash
# Builds Carbon.app into dist/. The page is built first because the app carries it inside.
set -euo pipefail
cd "$(dirname "$0")/.."
(cd ui && bun install --silent && bun run build)
cargo build --release -p carbon-desktop
APP=dist/Carbon.app
VERSION=$(grep -m1 '^version' desktop/Cargo.toml | cut -d'"' -f2)
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/Carbon "$APP/Contents/MacOS/Carbon"
cp brand/Carbon.icns "$APP/Contents/Resources/Carbon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Carbon</string>
  <key>CFBundleDisplayName</key><string>Carbon</string>
  <key>CFBundleIdentifier</key><string>sh.carbonsoft.carbon</string>
  <key>CFBundleExecutable</key><string>Carbon</string>
  <key>CFBundleIconFile</key><string>Carbon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict></plist>
PLIST
# Signed for this machine only, which is enough to run it here
codesign --force --deep --sign - "$APP" >/dev/null 2>&1
echo "$APP"

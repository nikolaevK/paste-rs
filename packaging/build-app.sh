#!/bin/zsh
# Builds Paste.app (release, Apple Silicon) into ./dist and ad-hoc signs it.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
APP=dist/Paste.app
BIN=target/release/paste

echo "▸ cargo build --release"
cargo build --release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/paste"
sed "s/__VERSION__/$VERSION/g" packaging/Info.plist > "$APP/Contents/Info.plist"
echo -n "APPL????" > "$APP/Contents/PkgInfo"

echo "▸ rendering app icon"
ICONSET=$(mktemp -d)/AppIcon.iconset
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  "$BIN" --render-icon "$ICONSET/icon_${size}x${size}.png" $size
  "$BIN" --render-icon "$ICONSET/icon_${size}x${size}@2x.png" $((size * 2))
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

echo "▸ codesign (ad-hoc)"
codesign --force --deep --sign - --identifier io.paste.rs "$APP"

echo "✓ built $APP ($VERSION)"
echo "  Install:  cp -R $APP /Applications/"
echo "  Run:      open $APP"

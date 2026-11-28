#!/usr/bin/env bash
# Builds BetterOdoo.app around a compiled binary and wraps it in a drag-to-Applications .dmg.
# Usage: build-dmg.sh <binary> <version> <out.dmg>
set -euo pipefail

bin=$1 version=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
stage=$(mktemp -d)
app="$stage/BetterOdoo.app"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/better-odoo"
cp "$root/assets/logo/icon.icns" "$app/Contents/Resources/icon.icns"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"

# Ad-hoc signature: required for the arm64 binary to run inside a bundle.
codesign --force --deep --sign - "$app"

ln -s /Applications "$stage/Applications"
rm -f "$out"
hdiutil create -volname BetterOdoo -srcfolder "$stage" -ov -format UDZO "$out"
rm -rf "$stage"

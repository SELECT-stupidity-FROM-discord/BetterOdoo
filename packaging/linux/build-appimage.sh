#!/usr/bin/env bash
# Wraps a compiled binary in an AppImage (x86_64). Needs network to fetch appimagetool.
# Usage: build-appimage.sh <binary> <out.AppImage>
set -euo pipefail

bin=$1 out=$2
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work=$(mktemp -d)
appdir="$work/BetterOdoo.AppDir"

mkdir -p "$appdir/usr/bin"
cp "$bin" "$appdir/usr/bin/better-odoo"
cp "$here/better-odoo.desktop" "$appdir/"
cp "$root/assets/logo/icon-512.png" "$appdir/better-odoo.png"
ln -s usr/bin/better-odoo "$appdir/AppRun"

curl -fsSL -o "$work/appimagetool" \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$work/appimagetool"

# Extract-and-run avoids needing FUSE on CI runners.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$work/appimagetool" "$appdir" "$out"
rm -rf "$work"

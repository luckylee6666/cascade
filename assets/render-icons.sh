#!/usr/bin/env bash
# Regenerate the app icon set from assets/logo.png (macOS only).
# Toolchain: sips/iconutil (macOS) + PIL (ico).
# To redesign: replace assets/logo.png (1024x1024 RGBA) then run this script.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$ROOT/crates/cc-desktop/icons"
SRC="$ROOT/assets/logo.png"

[ -f "$SRC" ] || { echo "missing $SRC"; exit 1; }

echo "▶ PNG sizes"
cp "$SRC" "$ICONS/icon.png"
sips -z 128 128 "$SRC" --out "$ICONS/128x128.png" >/dev/null
sips -z 32 32 "$SRC" --out "$ICONS/32x32.png" >/dev/null
sips -z 256 256 "$SRC" --out "$ICONS/128x128@2x.png" >/dev/null

echo "▶ icon.icns"
WORK="$(mktemp -d)"
ISET="$WORK/icon.iconset"
mkdir -p "$ISET"
for s in 16 32 64 128 256 512; do
  sips -z "$s" "$s" "$SRC" --out "$ISET/icon_${s}x${s}.png" >/dev/null
done
cp "$ISET/icon_32x32.png" "$ISET/icon_16x16@2x.png"
cp "$ISET/icon_64x64.png" "$ISET/icon_32x32@2x.png"
cp "$ISET/icon_256x256.png" "$ISET/icon_128x128@2x.png"
cp "$ISET/icon_512x512.png" "$ISET/icon_256x256@2x.png"
cp "$SRC" "$ISET/icon_512x512@2x.png"
iconutil -c icns "$ISET" -o "$ICONS/icon.icns"
rm -rf "$WORK"

echo "▶ icon.ico"
python3 - "$SRC" "$ICONS" <<'PY'
import sys
from PIL import Image
src, icons = sys.argv[1], sys.argv[2]
Image.open(src).save(
    f"{icons}/icon.ico",
    sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
PY

echo "▶ UI brand mark (128px)"
sips -z 128 128 "$SRC" --out "$ROOT/crates/cc-desktop/ui/logo.png" >/dev/null

echo "✅ done: $ICONS"

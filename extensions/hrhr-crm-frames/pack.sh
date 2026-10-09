#!/usr/bin/env bash
# 配布用 zip を作る。dist/hrhr-crm-frames-<version>.zip (コミットしない)。
# test/ package.json node_modules icons/make_icons.py は含めない。
set -euo pipefail
EXT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$EXT/../.." && pwd)"
VERSION="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['version'])" "$EXT/manifest.json")"
OUT_DIR="$REPO/dist"
OUT="$OUT_DIR/hrhr-crm-frames-$VERSION.zip"
mkdir -p "$OUT_DIR"
rm -f "$OUT"
cd "$EXT"
zip -r -X "$OUT" manifest.json managed_schema.json background.js content.js \
  popup.html popup.js options.html options.js lib \
  icons/icon16.png icons/icon32.png icons/icon48.png icons/icon128.png >/dev/null
echo "$OUT"
unzip -l "$OUT"

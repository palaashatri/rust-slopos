#!/usr/bin/env bash
# Package the five first-party SLOPOS-I apps as .app bundles.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT="$ROOT/packaging/apps/build-app-bundle.sh"
OUTDIR="${OUTDIR:-/tmp/Applications}"
VERSION="0.1.0"

mkdir -p "$OUTDIR"

build() {
  local crate="$1" name="$2" id="$3" icon="$4"
  if [ -n "$icon" ] && [ -f "$ROOT/$icon" ]; then
    bash "$SCRIPT" "$crate" "$name" "$id" "$VERSION" "$OUTDIR" "$ROOT/$icon"
  else
    bash "$SCRIPT" "$crate" "$name" "$id" "$VERSION" "$OUTDIR"
  fi
}

build finder "Finder" com.slopos.finder "themes/platinum/icons/finder.png"
build settings "Settings" com.slopos.settings "themes/platinum/icons/settings.png"
build textedit "TextEdit" com.slopos.textedit "themes/platinum/icons/textedit.png"
build terminal "Terminal" com.slopos.terminal "themes/platinum/icons/terminal.png"
if [ -f "$ROOT/themes/platinum/icons/appstore.png" ]; then
  build appstore "App Store" com.slopos.appstore "themes/platinum/icons/appstore.png"
else
  build appstore "App Store" com.slopos.appstore ""
fi

echo "Bundles in $OUTDIR:"
ls -d "$OUTDIR"/*.app
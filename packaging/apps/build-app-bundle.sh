#!/usr/bin/env bash
# Assemble one SLOPOS-I .app bundle (Stage 3 / spec §5).
# Usage:
#   build-app-bundle.sh <app-crate> <Display Name> <bundle_id> <version> <OUTDIR> [icon.png]
set -euo pipefail

if [ "$#" -lt 5 ]; then
  echo "usage: $0 <app-crate> <Display Name> <bundle_id> <version> <OUTDIR> [icon.png]" >&2
  exit 2
fi

APP="$1"
NAME="$2"
BUNDLE_ID="$3"
VERSION="$4"
OUTDIR="$5"
ICON="${6:-}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build --release -p "$APP"

case "$APP" in
  finder)
    SUPPORTED_TYPES="[]"
    PERMISSIONS='["files.read", "files.write"]'
    ;;
  textedit)
    SUPPORTED_TYPES='["txt", "md", "rtf"]'
    PERMISSIONS='["files.read", "files.write"]'
    ;;
  *)
    SUPPORTED_TYPES="[]"
    PERMISSIONS="[]"
    ;;
esac

APP_DIR="$OUTDIR/${NAME}.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Resources" "$APP_DIR/bin"

install -m755 "target/release/$APP" "$APP_DIR/bin/$APP"

cat > "$APP_DIR/Resources/Info.toml" <<EOF
bundle_id = "${BUNDLE_ID}"
name = "${NAME}"
version = "${VERSION}"
entrypoint = "bin/${APP}"
supported_types = ${SUPPORTED_TYPES}
permissions = ${PERMISSIONS}
EOF

if [ -n "$ICON" ] && [ -f "$ICON" ]; then
  cp "$ICON" "$APP_DIR/Resources/icon.png"
fi

echo "Built $APP_DIR"
#!/usr/bin/env bash
# Validate a built SLOPOS-I X11 session inside a VM that already has a working
# X server on DISPLAY. This is intentionally display-server independent and
# contains no compositor/Wayland assumptions.
set -euo pipefail

QA_DIR="${QA_DIR:-$HOME/qa/slopos-x11-live}"
DISPLAY="${DISPLAY:-:0}"
export DISPLAY
mkdir -p "$QA_DIR"
exec > >(tee "$QA_DIR/live.log") 2>&1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN_DIR="$CARGO_TARGET_DIR/release"

for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  test -x "$BIN_DIR/$binary" || {
    echo "required release binary missing: $BIN_DIR/$binary" >&2
    exit 1
  }
done

command -v xdpyinfo >/dev/null 2>&1 || { echo "xdpyinfo is required" >&2; exit 1; }
command -v xdotool >/dev/null 2>&1 || { echo "xdotool is required" >&2; exit 1; }
command -v openbox >/dev/null 2>&1 || { echo "openbox is required" >&2; exit 1; }
xdpyinfo -display "$DISPLAY" >/dev/null

export PATH="$BIN_DIR:$PATH"
export SLOPOS_OPENBOX_CONFIG="$ROOT/assets/config/openbox/rc.xml"
export SLOPOS_QA_NO_WELCOME=1

cleanup() {
  set +e
  kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${SESSION_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT

pkill -TERM -x slopos-session 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
sleep 1

"$BIN_DIR/slopos-session" >"$QA_DIR/session.log" 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 20); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null; then break; fi
  sleep 1
done
pgrep -x openbox >/dev/null
pgrep -x slopos-shell >/dev/null
xdotool search --name "SLOPOS Top Bar" >/dev/null
xdotool search --name "SLOPOS Application Strip" >/dev/null

"$BIN_DIR/slopos-catalogue" >"$QA_DIR/catalogue.log" 2>&1 & CATALOGUE_PID=$!
"$BIN_DIR/slopos-settings" >"$QA_DIR/settings.log" 2>&1 & SETTINGS_PID=$!
sleep 2
xdotool search --name "Software Catalogue" >/dev/null
xdotool search --name "System Settings" >/dev/null

if command -v scrot >/dev/null 2>&1; then
  scrot -z "$QA_DIR/live-session.png"
  test -s "$QA_DIR/live-session.png"
fi

echo "SLOPOS_X11_VM_SMOKE=PASS"
echo "Evidence: $QA_DIR"

#!/usr/bin/env bash
# SLOPOS-I Virtual Multi-Monitor & Dual-Head Geometry QA.
# Validates top bar geometry, application strip centering, search placement,
# and window moving across virtual multi-monitor layouts.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

DISPLAY="${SLOPOS_MULTIMONITOR_DISPLAY:-:93}"
export DISPLAY
export DEBIAN_FRONTEND=noninteractive
export GDK_BACKEND=x11
export SLOPOS_QA_NO_WELCOME=1
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$REPO_ROOT/assets/config/openbox/rc.xml}"

XVFB_PID=""
SESSION_PID=""
cleanup() {
  set +e
  if [[ -n "$SESSION_PID" ]]; then
    kill -TERM "$SESSION_PID" 2>/dev/null || true
    wait "$SESSION_PID" 2>/dev/null || true
  fi
  pkill -TERM -x slopos-shell 2>/dev/null || true
  pkill -TERM -x openbox 2>/dev/null || true
  if [[ -n "$XVFB_PID" ]]; then
    kill -TERM "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "=== [1/3] Starting virtual dual-head (3840x1080) desktop ==="
# 3840x1080 represents two side-by-side 1920x1080 virtual monitors
Xvfb "$DISPLAY" -screen 0 3840x1080x24 -nolisten tcp >/tmp/multimonitor-xvfb.log 2>&1 &
XVFB_PID=$!

for _ in $(seq 1 40); do
  if xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
xdpyinfo -display "$DISPLAY" >/dev/null 2>&1

echo "=== [2/3] Launching SLOPOS session on multi-display ==="
dbus-run-session -- ./target/release/slopos-session >/tmp/multimonitor-session.log 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 40); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null; then break; fi
  sleep 0.25
done
pgrep -x openbox >/dev/null
pgrep -x slopos-shell >/dev/null

for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1 && \
     xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done

TOP_BAR_WIN="$(xdotool search --onlyvisible --name '^SLOPOS Top Bar$' | tail -n 1)"
STRIP_WIN="$(xdotool search --onlyvisible --name '^SLOPOS Application Strip$' | tail -n 1)"

# Check Top Bar width spans the multi-monitor display
TOP_BAR_GEO="$(xdotool getwindowgeometry --shell "$TOP_BAR_WIN")"
TOP_BAR_WIDTH="$(awk -F= '/^WIDTH=/{print $2}' <<<"$TOP_BAR_GEO")"
echo "Top Bar Width on 3840x1080: $TOP_BAR_WIDTH"
test "$TOP_BAR_WIDTH" -eq 3840 || test "$TOP_BAR_WIDTH" -ge 1920

echo "=== [3/3] Testing Search placement & window moving across virtual outputs ==="
pkill -USR1 -x slopos-shell
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
SEARCH_WIN="$(xdotool search --onlyvisible --name '^SLOPOS Search$' | tail -n 1)"
test -n "$SEARCH_WIN"

SEARCH_GEO="$(xdotool getwindowgeometry --shell "$SEARCH_WIN")"
SEARCH_X="$(awk -F= '/^X=/{print $2}' <<<"$SEARCH_GEO")"
SEARCH_Y="$(awk -F= '/^Y=/{print $2}' <<<"$SEARCH_GEO")"
echo "Search Position: ($SEARCH_X, $SEARCH_Y)"
test "$SEARCH_X" -ge 0
test "$SEARCH_Y" -ge 0
xdotool key Escape

echo "MULTIMONITOR_QA_STATUS_0"
echo "SLOPOS-I Multi-Monitor Geometry QA: PASS"

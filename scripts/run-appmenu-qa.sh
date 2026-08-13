#!/usr/bin/env bash
# SLOPOS-I X11 AppMenu capability/fallback smoke.
#
# This deliberately assumes a pre-provisioned image and an existing release
# build. It never installs packages or downloads a toolchain. The test checks
# that an ordinary Mousepad window keeps its upstream local menu, that a
# synthetic X11 AppMenu advertisement is detected without importing or
# fabricating commands, and that the advertisement can be removed again.
set -euo pipefail

export DISPLAY="${DISPLAY:-:99}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/slopos-appmenu-qa-runtime}"
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-/workspace/assets/config/openbox/rc.xml}"
export SLOPOS_QA_NO_WELCOME=1
export GDK_BACKEND=x11

required=(Xvfb dbus-run-session xdotool xprop mousepad)
for program in "${required[@]}"; do
  command -v "$program" >/dev/null 2>&1 || {
    echo "ERROR: pre-provisioned AppMenu QA image lacks $program" >&2
    exit 2
  }
done
test -x target/release/slopos-session

mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

cleanup() {
  set +e
  xprop -id "${APP_WINDOW:-0}" -remove _GTK_UNIQUE_BUS_NAME 2>/dev/null || true
  xprop -id "${APP_WINDOW:-0}" -remove _GTK_APP_MENU_OBJECT_PATH 2>/dev/null || true
  kill "${APP_PID:-}" "${SESSION_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
  pkill -TERM -x mousepad 2>/dev/null || true
  pkill -TERM -x slopos-shell 2>/dev/null || true
  pkill -TERM -x slopos-session 2>/dev/null || true
  pkill -TERM -x openbox 2>/dev/null || true
  pkill -TERM -x Xvfb 2>/dev/null || true
}
trap cleanup EXIT

window_for_pid() {
  local pid="$1" window window_pid
  for window in $(xdotool search --onlyvisible --name '.*' 2>/dev/null || true); do
    window_pid="$(xdotool getwindowpid "$window" 2>/dev/null || true)"
    if [[ "$window_pid" == "$pid" ]]; then
      printf '%s\n' "$window"
      return 0
    fi
  done
  return 1
}

wait_window_for_pid() {
  local pid="$1"
  for _ in $(seq 1 40); do
    if window_for_pid "$pid" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "ERROR: visible window not found for pid $pid" >&2
  return 1
}

wait_visible() {
  local pattern="$1"
  for _ in $(seq 1 40); do
    if xdotool search --onlyvisible --name "$pattern" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "ERROR: visible window not found: $pattern" >&2
  return 1
}

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/slopos-appmenu-xvfb.log 2>&1 &
XVFB_PID=$!
sleep 1
xsetroot -solid "#758090"

dbus-run-session -- ./target/release/slopos-session >/tmp/slopos-appmenu-session.log 2>&1 &
SESSION_PID=$!
wait_visible '^SLOPOS Top Bar$'

mousepad /workspace/README.md >/tmp/slopos-appmenu-mousepad.log 2>&1 &
APP_PID=$!
wait_window_for_pid "$APP_PID"
APP_WINDOW="$(window_for_pid "$APP_PID")"
test -n "$APP_WINDOW"
xdotool windowactivate --sync "$APP_WINDOW"
sleep 1

# Mousepad has no AppMenu properties in this clean launch. Its own native
# menu remains visible and is the only application menu the user should use.
xdotool search --onlyvisible --name '^/workspace/README.md - Mousepad$' >/dev/null
if xprop -id "$APP_WINDOW" | grep -qE '_GTK_(UNIQUE_BUS_NAME|APP_MENU_OBJECT_PATH)'; then
  echo "ERROR: clean Mousepad unexpectedly advertises an AppMenu" >&2
  exit 1
fi
echo "Mousepad local menu remains upstream-owned"
echo "NON_EXPORTER_STATUS_0"

# Advertise the standard X11 properties on the real Mousepad window. This is
# a capability fixture only: no DBus object is fabricated and no menu command
# is imported. SLOPOS must keep the local menu and report the limitation.
xprop -id "$APP_WINDOW" -f _GTK_UNIQUE_BUS_NAME 8s -set _GTK_UNIQUE_BUS_NAME ':1.77'
xprop -id "$APP_WINDOW" -f _GTK_APP_MENU_OBJECT_PATH 8s -set _GTK_APP_MENU_OBJECT_PATH '/com/canonical/dbusmenu'
xdotool windowactivate --sync "$APP_WINDOW"
sleep 1
grep -Fq 'advertises AppMenu' /tmp/slopos-appmenu-session.log
grep -Fq 'keeping its local menu' /tmp/slopos-appmenu-session.log
echo "EXPORTER_FIXTURE_STATUS_0"

# Remove the fixture and ensure capability disappears after focus polling.
xprop -id "$APP_WINDOW" -remove _GTK_UNIQUE_BUS_NAME
xprop -id "$APP_WINDOW" -remove _GTK_APP_MENU_OBJECT_PATH
sleep 1
grep -Fq 'exports no AppMenu; using its local menu' /tmp/slopos-appmenu-session.log
echo "NON_EXPORTER_RETURN_STATUS_0"

echo "SLOPOS AppMenu capability evidence PASS"

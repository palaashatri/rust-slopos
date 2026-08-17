#!/usr/bin/env bash
# Dedicated test script for SLOPOS-I GUI session management controls
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Building release binaries ==="
cargo build --release --workspace --locked

DISPLAY=:95
export DISPLAY
Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/session-gui-xvfb.log 2>&1 &
XVFB_PID=$!
sleep 1
xsetroot -solid "#758090"

dbus-run-session -- ./target/release/slopos-session >/tmp/session-gui.log 2>&1 &
SESSION_PID=$!

cleanup() {
  kill -TERM "$SESSION_PID" "$XVFB_PID" 2>/dev/null || true
  pkill -TERM -x slopos-shell 2>/dev/null || true
  pkill -TERM -x openbox 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 40); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null; then break; fi
  sleep 0.25
done

for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1; then break; fi
  sleep 0.25
done

echo "=== Testing System Menu Opening via hotkey (SIGUSR2) ==="
pkill -USR2 -x slopos-shell
sleep 0.5

echo "=== Testing About Dialog ==="
xdotool key Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^About SLOPOS-I$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
test -n "$(xdotool search --onlyvisible --name '^About SLOPOS-I$')"
xdotool key Return
sleep 0.3

echo "=== Testing Shut Down Modal Dialog (Up 1) ==="
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Shut Down$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
test -n "$(xdotool search --onlyvisible --name '^Shut Down$')"
xdotool key Escape
sleep 0.3

echo "=== Testing Restart Modal Dialog (Up 2) ==="
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Restart$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
test -n "$(xdotool search --onlyvisible --name '^Restart$')"
xdotool key Escape
sleep 0.3

echo "=== Testing Log Out Modal Dialog (Up 3) ==="
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Up Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Log Out$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
test -n "$(xdotool search --onlyvisible --name '^Log Out$')"
xdotool key Escape
sleep 0.3

echo "=== Testing Sleep Modal Dialog (Up 4) ==="
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Up Up Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Sleep$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
test -n "$(xdotool search --onlyvisible --name '^Sleep$')"
xdotool key Escape
sleep 0.3

echo "=== Testing Switch User Modal Dialog (Up 5) ==="
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Up Up Up Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Switch User$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
test -n "$(xdotool search --onlyvisible --name '^Switch User$')"
xdotool key Escape
sleep 0.3

echo "SESSION_GUI_QA_STATUS_0"
echo "SLOPOS-I GUI Session Controls QA: PASS"

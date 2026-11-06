#!/bin/bash
# dock-autohide.sh — runtime check of the Dock's auto-hide under Xvfb + Openbox.
# Starts the Dock with auto-hide on, moves the pointer onto and off its edge, and
# checks the window's height and reserved strut at each step against the expected
# values. The last steps turn auto-hide off from the Dock's right-click menu (Down,
# Return) and check that the choice is saved. Exits nonzero on any mismatch.
# Needs Xvfb, openbox, xdotool and ImageMagick's import. Build the workspace first.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.work/qa-runtime/dock-autohide"
BIN="$ROOT/target/debug"
DISPLAY_NUMBER=:61
rm -rf "$OUT" && mkdir -p "$OUT/home/.config"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_DATA_HOME="$OUT/home/.local/share"
export DISPLAY="$DISPLAY_NUMBER"
FAILED=0
Xvfb "$DISPLAY_NUMBER" -screen 0 1280x800x24 -noreset >"$OUT/xvfb.log" 2>&1 &
XVFB=$!
sleep 1
openbox >"$OUT/openbox.log" 2>&1 &
OPENBOX=$!
sleep 1
"$BIN/slopos-dock" --autohide on || { kill $XVFB $OPENBOX; exit 1; }
echo "config: $(tr '\n' ' ' < "$XDG_CONFIG_HOME/slopos-i/dock.conf")"
RUST_LOG=debug "$BIN/slopos-dock" >"$OUT/dock.log" 2>&1 &
DOCK=$!
sleep 2
WID=$(timeout 5 xdotool search --class '^slopos-dock$' | head -1)
if [ -z "$WID" ]; then
  echo "FAIL: the Dock window was not found"
  kill $DOCK $OPENBOX $XVFB 2>/dev/null
  exit 1
fi
# check LABEL HEIGHT STRUT_BOTTOM: the window's height and the bottom of its strut.
check() {
  local height bottom
  height=$(xwininfo -id "$WID" | awk '/Height:/{print $2}')
  bottom=$(xprop -id "$WID" _NET_WM_STRUT_PARTIAL | sed 's/.*= //' | awk -F', ' '{print $4}')
  if [ "$height" = "$2" ] && [ "$bottom" = "$3" ]; then
    echo "PASS $1: height $height, strut bottom $bottom"
  else
    echo "FAIL $1: height $height (expected $2), strut bottom $bottom (expected $3)"
    FAILED=1
  fi
}
check "1 start, hidden" 4 0
xdotool mousemove 640 799; sleep 0.6
check "2 pointer on the edge" 66 66
xdotool mousemove 640 400; sleep 1.2
check "3 pointer away, hidden again" 4 0
xdotool mousemove 640 799; sleep 0.3; xdotool mousemove 640 760; sleep 0.3
check "4 pointer inside the revealed Dock" 66 66
xdotool mousemove 640 400; sleep 1.2
check "5 pointer away again" 4 0
xdotool mousemove 640 799; sleep 0.4; xdotool click 3; sleep 0.6
xdotool key Down; sleep 0.2; xdotool key Return; sleep 1.2
check "6 menu chose auto-hide off" 66 66
if grep -q 'autohide = false' "$XDG_CONFIG_HOME/slopos-i/dock.conf"; then
  echo "PASS saved preference: autohide = false"
else
  echo "FAIL saved preference: $(tr '\n' ' ' < "$XDG_CONFIG_HOME/slopos-i/dock.conf")"
  FAILED=1
fi
xdotool mousemove 640 400; sleep 1.5
check "7 auto-hide off stays shown" 66 66
import -window root "$OUT/dock-autohide-off.png"
kill $DOCK $OPENBOX $XVFB 2>/dev/null
sleep 1
echo "dock log (auto-hide decisions):"
grep "auto-hide" "$OUT/dock.log" | sed 's/^\[[^]]*\] //'
exit $FAILED

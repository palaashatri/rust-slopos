#!/bin/bash
# dock-autohide.sh — runtime check of the Dock's auto-hide under Xvfb + Openbox.
# Starts the Dock with auto-hide on, moves the pointer onto and off its edge, and
# checks the window's height and reserved strut at each step against the expected
# values. The last steps turn auto-hide off from the Dock's right-click menu (Down,
# Return) and check that the choice is saved. Exits nonzero on any mismatch.
# Needs Xvfb, openbox, xdotool, xwininfo, xprop, wmctrl and ImageMagick's import,
# convert and display. Build the workspace first.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.work/qa-runtime/dock-autohide"
BIN="$ROOT/target/debug"
DISPLAY_NUMBER=:61
DOCK= OPENBOX= XVFB= COVER=
# However the script ends, including a signal, reap what it started so the
# display number is free for the next run.
cleanup() { kill $DOCK $OPENBOX $XVFB $COVER 2>/dev/null; wait $DOCK $OPENBOX $XVFB $COVER 2>/dev/null; }
trap cleanup EXIT
trap 'exit 130' INT TERM
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
# check LABEL HEIGHT STRUT_BOTTOM: the window's height, the bottom of its strut, and
# the strut's horizontal span, which must be the window's root-relative span.
check() {
  local height bottom ax width span
  height=$(xwininfo -id "$WID" | awk '/Height:/{print $2}')
  bottom=$(xprop -id "$WID" _NET_WM_STRUT_PARTIAL | sed 's/.*= //' | awk -F', ' '{print $4}')
  ax=$(xwininfo -id "$WID" | awk '/Absolute upper-left X/{print $4}')
  width=$(xwininfo -id "$WID" | awk '/Width:/{print $2}')
  span=$(xprop -id "$WID" _NET_WM_STRUT_PARTIAL | sed 's/.*= //' | awk -F', ' '{print $11"-"$12}')
  if [ "$height" = "$2" ] && [ "$bottom" = "$3" ] && [ "$span" = "$ax-$((ax + width - 1))" ]; then
    echo "PASS $1: height $height, strut bottom $bottom, span $span"
  else
    echo "FAIL $1: height $height (expected $2), strut bottom $bottom (expected $3), span $span (window $ax + $width)"
    FAILED=1
  fi
}
# span_of: the strut's horizontal span, which must follow the Dock wherever it is placed.
span_of() { xprop -id "$WID" _NET_WM_STRUT_PARTIAL | sed 's/.*= //' | awk -F', ' '{print $11"-"$12}'; }
check "1 start, hidden" 4 0
xdotool mousemove 640 799; sleep 0.6
check "2 pointer on the edge" 66 66
xdotool mousemove 640 400; sleep 1.2
check "3 pointer away, hidden again" 4 0
xdotool mousemove 640 799; sleep 0.3; xdotool mousemove 640 760; sleep 0.3
check "4 pointer inside the revealed Dock" 66 66
xdotool mousemove 640 400; sleep 1.2
check "5 pointer away again" 4 0
# Occlusion: the pointer rests on the revealed Dock, then a fullscreen window covers
# the screen under it. The window takes the pointer, so the Dock must count it as a
# leave (its own crossing filter cannot see the pointer over the Dock) and hide.
convert -size 1280x800 xc:'#400020' "$OUT/cover.png"
xdotool mousemove 640 799; sleep 0.6
check "5a revealed under the pointer" 66 66
span_before=$(span_of)
display -title SLOPOS-QA-COVER "$OUT/cover.png" >/dev/null 2>&1 &
COVER=$!
sleep 1.5
wmctrl -r SLOPOS-QA-COVER -b add,fullscreen; sleep 0.6
xdotool mousemove 640 700; sleep 1.2
check "5b a fullscreen window covers the revealed Dock" 4 0
# The cover's window adds a tile, so the Dock moves; the strut must have moved with it.
if [ "$(span_of)" != "$span_before" ]; then echo "PASS 5b the strut moved with the Dock: $span_before -> $(span_of)"; else echo "FAIL 5b the Dock did not move under the cover (span $span_before)"; FAILED=1; fi
kill $COVER 2>/dev/null; wait $COVER 2>/dev/null; sleep 0.8
xdotool mousemove 640 400; sleep 1.2
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
# Shown, so the strut is live: a window that adds a tile makes the Dock widen and move,
# and the strut must follow the new root-relative span, then return when it closes.
span_shown=$(span_of)
convert -size 300x200 xc:'#303030' "$OUT/tile.png"
display -title SLOPOS-QA-TILE "$OUT/tile.png" >/dev/null 2>&1 &
TILE=$!
sleep 1.5
check "8 a window adds a tile while the Dock is shown" 66 66
if [ "$(span_of)" != "$span_shown" ]; then echo "PASS 8 the strut moved with the Dock: $span_shown -> $(span_of)"; else echo "FAIL 8 the Dock did not move (span $span_shown)"; FAILED=1; fi
kill $TILE 2>/dev/null; wait $TILE 2>/dev/null; sleep 1.5
check "9 the tile closed; the Dock is back where it was" 66 66
if [ "$(span_of)" = "$span_shown" ]; then echo "PASS 9 the span returned to $span_shown"; else echo "FAIL 9 span $(span_of), expected $span_shown"; FAILED=1; fi
import -window root "$OUT/dock-autohide-off.png"
kill $DOCK $OPENBOX $XVFB 2>/dev/null
sleep 1
echo "dock log (auto-hide decisions):"
grep "auto-hide" "$OUT/dock.log" | sed 's/^\[[^]]*\] //'
exit $FAILED

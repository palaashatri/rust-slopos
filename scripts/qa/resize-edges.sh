#!/bin/bash
# resize-edges.sh — runtime check that Openbox lets the user resize a SLOPOS window.
# Starts Xvfb and Openbox with the repo's rc.xml, opens the Calculator, and drags its
# frame edges, bottom-right corner and Alt+right-drag with xdotool. After each drag the
# frame's size is read back and must have grown. It also checks that the client's
# WM_NORMAL_HINTS do not pin minimum and maximum to the same size. Exits nonzero on any
# mismatch. Needs Xvfb, openbox, xdotool, xwininfo and xprop (ImageMagick's import is
# optional, for a screenshot). Build the workspace first.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.work/qa-runtime/resize-edges"
BIN="$ROOT/target/debug"
RC="$ROOT/assets/config/openbox/rc.xml"
DISPLAY_NUMBER=:62
OPENBOX= XVFB= CALC=
# However the script ends, including a signal, reap what it started so the
# display number is free for the next run.
cleanup() { kill $CALC $OPENBOX $XVFB 2>/dev/null; wait $CALC $OPENBOX $XVFB 2>/dev/null; }
trap cleanup EXIT
trap 'exit 130' INT TERM
rm -rf "$OUT" && mkdir -p "$OUT/home/.config" "$OUT/home/.local/share/themes"
# Openbox looks up the rc.xml theme under XDG_DATA_HOME/themes, so link the Platinum theme there.
ln -s "$ROOT/themes/slopos-openbox" "$OUT/home/.local/share/themes/slopos-openbox"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_DATA_HOME="$OUT/home/.local/share"
export DISPLAY="$DISPLAY_NUMBER"
FAILED=0
# wait_until SECONDS COMMAND...: runs COMMAND every 0.2 s until it succeeds, and fails once
# SECONDS have passed, so a display, window manager or app that never appears cannot hang
# the check.
wait_until() {
  local end=$((SECONDS + $1)); shift
  until "$@" 2>/dev/null; do
    [ "$SECONDS" -ge "$end" ] && return 1
    sleep 0.2
  done
}
x_ready() { xdotool getdisplaygeometry >/dev/null 2>&1; }
# Openbox publishes its check window on the root once it has started.
wm_ready() { xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q 'window id'; }
calc_window() { WID=$(timeout 5 xdotool search --class '^slopos-calculator$' | head -1); [ -n "$WID" ]; }

# frame: prints "X Y W H" of the frame Openbox put around the client, from the client's
# absolute geometry and the _NET_FRAME_EXTENTS the window manager sets (left, right, top, bottom).
frame() {
  local x y w h left right top bottom
  x=$(xwininfo -id "$WID" | awk '/Absolute upper-left X/{print $4}')
  y=$(xwininfo -id "$WID" | awk '/Absolute upper-left Y/{print $4}')
  w=$(xwininfo -id "$WID" | awk '/Width:/{print $2}')
  h=$(xwininfo -id "$WID" | awk '/Height:/{print $2}')
  read -r left right top bottom < <(xprop -id "$WID" _NET_FRAME_EXTENTS 2>/dev/null \
    | sed 's/.*= //' | tr -d ',')
  echo "$((x - ${left:-0})) $((y - ${top:-0})) $((w + ${left:-0} + ${right:-0})) $((h + ${top:-0} + ${bottom:-0}))"
}

# drag NAME X Y DX DY [MODIFIER]: presses button 1 (button 3 with MODIFIER=alt) at the
# point (X, Y), drags by (DX, DY) and releases. Openbox resizes from the edge or corner it
# finds under the press.
drag() {
  local name=$1 x=$2 y=$3 dx=$4 dy=$5 mod=${6:-}
  if [ "$mod" = alt ]; then
    xdotool keydown Alt_L mousemove --sync "$x" "$y" mousedown 3 \
      mousemove --sync "$((x + dx / 2))" "$((y + dy / 2))" mousemove --sync "$((x + dx))" "$((y + dy))" \
      mouseup 3 keyup Alt_L
  else
    xdotool mousemove --sync "$x" "$y" mousedown 1 \
      mousemove --sync "$((x + dx / 2))" "$((y + dy / 2))" mousemove --sync "$((x + dx))" "$((y + dy))" \
      mouseup 1
  fi
  sleep 0.5
}

# expect NAME BEFORE AFTER AXIS: the frame's width (AXIS=w) or height (AXIS=h) must have
# grown by at least 40 device pixels from BEFORE to AFTER.
expect() {
  local name=$1 before=$2 after=$3 axis=$4 grew
  if [ "$axis" = w ]; then grew=$(( $(echo "$after" | awk '{print $3}') - $(echo "$before" | awk '{print $3}') ))
  else grew=$(( $(echo "$after" | awk '{print $4}') - $(echo "$before" | awk '{print $4}') )); fi
  if [ "$grew" -ge 40 ]; then
    echo "PASS $name: frame ${axis} grew by $grew ($before -> $after)"
  else
    echo "FAIL $name: frame ${axis} grew by $grew, expected at least 40 ($before -> $after)"
    FAILED=1
  fi
}

Xvfb "$DISPLAY_NUMBER" -screen 0 1280x800x24 -noreset >"$OUT/xvfb.log" 2>&1 &
XVFB=$!
wait_until 10 x_ready || { echo "FAIL: the X display did not start"; exit 1; }
openbox --config-file "$RC" >"$OUT/openbox.log" 2>&1 &
OPENBOX=$!
wait_until 10 wm_ready || { echo "FAIL: Openbox did not start"; exit 1; }
"$BIN/slopos-calculator" >"$OUT/calculator.log" 2>&1 &
CALC=$!
wait_until 10 calc_window || { echo "FAIL: the Calculator window was not found"; exit 1; }
# Let Openbox finish reparenting and placing the window before the first measurement.
sleep 1

# WM_NORMAL_HINTS must not pin the window to one size, or no drag can change it.
min=$(xprop -id "$WID" WM_NORMAL_HINTS 2>/dev/null | awk -F': ' '/minimum size/{print $2}')
max=$(xprop -id "$WID" WM_NORMAL_HINTS 2>/dev/null | awk -F': ' '/maximum size/{print $2}')
if [ -n "$min" ] && [ "$min" = "$max" ]; then
  echo "FAIL 0 the Calculator pins its size: minimum and maximum are both $min"; FAILED=1
else
  echo "PASS 0 the Calculator's size hints leave a range (minimum: ${min:-none}, maximum: ${max:-none})"
fi

before=$(frame)
echo "start frame: $before"

# 1. The right edge: grab the frame's last pixel column at mid height.
read -r fx fy fw fh <<<"$before"
drag "right edge" "$((fx + fw - 1))" "$((fy + fh / 2))" 120 0
after=$(frame); expect "1 right edge" "$before" "$after" w
before=$after

# 2. The bottom edge: grab the frame's last pixel row at mid width.
read -r fx fy fw fh <<<"$before"
drag "bottom edge" "$((fx + fw / 2))" "$((fy + fh - 1))" 0 80
after=$(frame); expect "2 bottom edge" "$before" "$after" h
before=$after

# 3. The bottom-right corner: grows both width and height.
read -r fx fy fw fh <<<"$before"
drag "bottom-right corner" "$((fx + fw - 1))" "$((fy + fh - 1))" 60 40
after=$(frame); expect "3 bottom-right corner (width)" "$before" "$after" w
expect "3 bottom-right corner (height)" "$before" "$after" h
before=$after

# 4. Alt+right-drag anywhere in the window (the Frame context binding in rc.xml).
read -r fx fy fw fh <<<"$before"
drag "Alt+right-drag" "$((fx + fw / 2))" "$((fy + fh / 2))" 100 0 alt
after=$(frame); expect "4 Alt+right-drag" "$before" "$after" w

if command -v import >/dev/null 2>&1; then
  import -window root "$OUT/resize-edges.png"
fi
echo "frame after all drags: $(frame)"
exit $FAILED

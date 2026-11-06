#!/bin/bash
# dock-autohide.sh — runtime check of the Dock's auto-hide under Xvfb + Openbox.
# Starts the Dock with auto-hide on, moves the pointer onto and off its edge, and
# checks the window's height and reserved strut at each step. The last step turns
# auto-hide off from the Dock's right-click menu (Down, Return) and checks that
# the choice is saved. Needs Xvfb, openbox, xdotool and ImageMagick's import.
# Binaries come from target/debug (cargo build --workspace first).
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.work/qa-runtime/dock-autohide"
BIN="$ROOT/target/debug"
DISPLAY_NUMBER=:61
rm -rf "$OUT" && mkdir -p "$OUT/home/.config"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_DATA_HOME="$OUT/home/.local/share"
export DISPLAY="$DISPLAY_NUMBER"
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
geom() { xwininfo -id "$WID" | awk '/Absolute upper-left Y/{y=$4} /Height:/{h=$2} /Width:/{w=$2} END{print "y=" y " w=" w " h=" h}'; }
strut() { xprop -id "$WID" _NET_WM_STRUT_PARTIAL | sed 's/.*= //'; }
report() { echo "$1: $(geom)  strut=[$(strut)]"; }
report "1 start (hidden, expect h=4)"
xdotool mousemove 640 799; sleep 0.6
report "2 pointer on the edge (expect h=66)"
xdotool mousemove 640 400; sleep 1.2
report "3 pointer away (expect h=4 after 600 ms)"
xdotool mousemove 640 799; sleep 0.3; xdotool mousemove 640 760; sleep 0.3
report "4 pointer inside the revealed Dock (expect still h=66)"
xdotool mousemove 640 400; sleep 1.2
report "5 pointer away again (expect h=4)"
xdotool mousemove 640 799; sleep 0.4; xdotool click 3; sleep 0.6
xdotool key Down; sleep 0.2; xdotool key Return; sleep 1.2
report "6 menu chose off (expect h=66)"
echo "config after the menu: $(tr '\n' ' ' < "$XDG_CONFIG_HOME/slopos-i/dock.conf")"
xdotool mousemove 640 400; sleep 1.5
report "7 auto-hide off stays shown (expect h=66)"
import -window root "$OUT/dock-autohide-off.png"
kill $DOCK $OPENBOX $XVFB 2>/dev/null
sleep 1
echo "dock log (auto-hide decisions):"
grep "auto-hide" "$OUT/dock.log" | sed 's/^\[[^]]*\] //'

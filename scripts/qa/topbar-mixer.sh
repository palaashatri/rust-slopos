#!/bin/bash
# topbar-mixer.sh — runtime check of the top bar's Volume Mixer popover.
# Runs the top bar on Xvfb + Openbox inside a session bus, with the pactl fixture
# on the path. Pass the x position of the sound item (read it from the first
# screenshot, topbar-0-bar.png): a click there must open the popover, and a press
# outside it must close it. Needs dbus-run-session, xdotool and ImageMagick.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.work/qa-runtime/topbar-mixer"
BIN="$ROOT/target/debug"
SOUND_X="${1:-}"
DISPLAY_NUMBER=:64
if [ -z "${INSIDE_BUS:-}" ]; then
  mkdir -p "$OUT"
  exec env INSIDE_BUS=1 dbus-run-session -- bash "$0" "$SOUND_X"
fi
BAR= OPENBOX= XVFB=
# However the script ends, including a signal, reap what it started so the
# display number is free for the next run.
cleanup() { kill $BAR $OPENBOX $XVFB 2>/dev/null; wait $BAR $OPENBOX $XVFB 2>/dev/null; }
trap cleanup EXIT
trap 'exit 130' INT TERM
rm -rf "$OUT/home" && mkdir -p "$OUT/home/.config" "$OUT/fake-pactl/state"
rm -f "$OUT/fake-pactl/state/calls"
cp "$ROOT/scripts/qa/fake-pactl/pactl" "$OUT/fake-pactl/pactl"
printf 85 > "$OUT/fake-pactl/state/output_volume"; printf 60 > "$OUT/fake-pactl/state/input_volume"
printf 0 > "$OUT/fake-pactl/state/output_mute"; printf 0 > "$OUT/fake-pactl/state/input_mute"
printf sink.one > "$OUT/fake-pactl/state/default_output"; printf source.one > "$OUT/fake-pactl/state/default_input"
printf 100 > "$OUT/fake-pactl/state/stream_volume"; printf no > "$OUT/fake-pactl/state/stream_mute"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_DATA_HOME="$OUT/home/.local/share"
rm -rf "$OUT/late-bin" && mkdir -p "$OUT/late-bin"
export DISPLAY="$DISPLAY_NUMBER" PATH="$OUT/late-bin:$PATH" FAKE_PACTL_STATE="$OUT/fake-pactl/state"
Xvfb "$DISPLAY_NUMBER" -screen 0 1280x800x24 -noreset >"$OUT/xvfb.log" 2>&1 &
XVFB=$!
sleep 1
openbox >"$OUT/openbox.log" 2>&1 &
OPENBOX=$!
sleep 1
RUST_LOG=info "$BIN/slopos-topbar" >"$OUT/topbar.log" 2>&1 &
BAR=$!
sleep 3
import -window root "$OUT/topbar-0-bar.png"
FAILED=0
# The bar started before the sound server existed, so nothing has reached it. The
# server is installed now; the first opening of the mixer asks for a new connection.
if [ -e "$OUT/fake-pactl/state/calls" ]; then echo "FAIL the sound server was reached before the mixer opened"; FAILED=1; fi
cp "$OUT/fake-pactl/pactl" "$OUT/late-bin/pactl"
if [ -z "$SOUND_X" ]; then
  echo "usage: topbar-mixer.sh SOUND_X   (the sound item's x, from topbar-0-bar.png)" >&2
  kill $BAR $OPENBOX $XVFB 2>/dev/null
  exit 2
fi
xdotool mousemove "$SOUND_X" 12; sleep 0.2; xdotool click 1; sleep 1.2
import -window root "$OUT/topbar-1-popover.png"
xdotool mousemove 600 500; sleep 0.2; xdotool click 1; sleep 0.8
import -window root "$OUT/topbar-2-closed.png"
kill $BAR $OPENBOX $XVFB 2>/dev/null
sleep 1
# Pixel checks: inside the popover's body (#DDDDDD) when open, black once closed.
pixel() { convert "$1" -crop 1x1+900+150 txt:- | tail -1 | grep -o '#[0-9A-F]*' | head -1; }
opened=$(pixel "$OUT/topbar-1-popover.png")
closed=$(pixel "$OUT/topbar-2-closed.png")
if [ "$opened" = "#DDDDDD" ]; then echo "PASS popover open: pixel $opened"; else echo "FAIL popover open: pixel $opened (expected #DDDDDD)"; FAILED=1; fi
if [ "$closed" = "#000000" ]; then echo "PASS popover closed by a press outside: pixel $closed"; else echo "FAIL popover closed: pixel $closed (expected #000000)"; FAILED=1; fi
if [ -s "$OUT/fake-pactl/state/calls" ]; then echo "PASS the mixer's opening reached the sound server (reconnect)"; else echo "FAIL the mixer's opening did not reconnect"; FAILED=1; fi
exit $FAILED

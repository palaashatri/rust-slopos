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
rm -rf "$OUT/home" && mkdir -p "$OUT/home/.config" "$OUT/fake-pactl/state"
cp "$ROOT/scripts/qa/fake-pactl/pactl" "$OUT/fake-pactl/pactl"
printf 85 > "$OUT/fake-pactl/state/output_volume"; printf 60 > "$OUT/fake-pactl/state/input_volume"
printf 0 > "$OUT/fake-pactl/state/output_mute"; printf 0 > "$OUT/fake-pactl/state/input_mute"
printf sink.one > "$OUT/fake-pactl/state/default_output"; printf source.one > "$OUT/fake-pactl/state/default_input"
printf 100 > "$OUT/fake-pactl/state/stream_volume"; printf no > "$OUT/fake-pactl/state/stream_mute"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_DATA_HOME="$OUT/home/.local/share"
export DISPLAY="$DISPLAY_NUMBER" PATH="$OUT/fake-pactl:$PATH"
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
if [ -n "$SOUND_X" ]; then
  xdotool mousemove "$SOUND_X" 12; sleep 0.2; xdotool click 1; sleep 1.2
  import -window root "$OUT/topbar-1-popover.png"
  echo "popover: screenshot topbar-1-popover.png (a left click on the sound item)"
  xdotool mousemove 600 500; sleep 0.2; xdotool click 1; sleep 0.8
  import -window root "$OUT/topbar-2-closed.png"
  echo "outside press: screenshot topbar-2-closed.png"
fi
kill $BAR $OPENBOX $XVFB 2>/dev/null
sleep 1

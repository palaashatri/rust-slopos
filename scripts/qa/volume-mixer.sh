#!/bin/bash
# volume-mixer.sh — runtime check of the Volume Mixer app under Xvfb + Openbox.
# 1. Without a sound server (no pactl on the path): the window must say so.
# 2. With scripts/qa/fake-pactl (a PulseAudio-compatible fixture over state files):
#    a drag on the application's fader and a click on the speakers' Mute box must
#    reach the fixture. Screenshots are written next to the logs.
# SCREEN_GEOMETRY (default 1280x800x24) sets the display, e.g. 800x600x24 for a narrow one.
# Needs Xvfb, openbox, xdotool and ImageMagick's import. Build the workspace first.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/.work/qa-runtime/volume-mixer"
BIN="$ROOT/target/debug"
DISPLAY_NUMBER=:63
SCREEN="${SCREEN_GEOMETRY:-1280x800x24}"
rm -rf "$OUT" && mkdir -p "$OUT/home/.config" "$OUT/fake-pactl/state"
cp "$ROOT/scripts/qa/fake-pactl/pactl" "$OUT/fake-pactl/pactl"
STATE="$OUT/fake-pactl/state"
printf 85 > "$STATE/output_volume"; printf 60 > "$STATE/input_volume"
printf 0 > "$STATE/output_mute"; printf 0 > "$STATE/input_mute"
printf sink.one > "$STATE/default_output"; printf source.one > "$STATE/default_input"
printf 100 > "$STATE/stream_volume"; printf no > "$STATE/stream_mute"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_DATA_HOME="$OUT/home/.local/share"
export DISPLAY="$DISPLAY_NUMBER"
Xvfb "$DISPLAY_NUMBER" -screen 0 "$SCREEN" -noreset >"$OUT/xvfb.log" 2>&1 &
XVFB=$!
sleep 1
openbox >"$OUT/openbox.log" 2>&1 &
OPENBOX=$!
sleep 1
wid_of() { timeout 5 xdotool search --name '^Volume Mixer$' | head -1; }
absolute() { xwininfo -id "$1" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} END{print x, y}'; }

if command -v pactl >/dev/null 2>&1 && [ "$(command -v pactl)" = "$OUT/fake-pactl/pactl" ]; then :; fi
env PATH=/usr/bin:/bin RUST_LOG=info "$BIN/slopos-volume-mixer" >"$OUT/mixer-unavailable.log" 2>&1 &
MIX=$!
sleep 2.5
WID=$(wid_of)
echo "1 without a sound server: window=${WID:-none}"
[ -n "$WID" ] && import -window "$WID" "$OUT/mixer-1-unavailable.png"
kill $MIX 2>/dev/null; wait $MIX 2>/dev/null; sleep 1

export PATH="$OUT/fake-pactl:$PATH"
RUST_LOG=info "$BIN/slopos-volume-mixer" >"$OUT/mixer.log" 2>&1 &
MIX=$!
sleep 2.5
WID=$(wid_of)
read AX AY < <(absolute "$WID")
echo "2 with the fixture: window=$WID at $AX,$AY size=$(xwininfo -id "$WID" | awk '/Width:/{w=$2} /Height:/{h=$2} END{print w"x"h}')"
import -window "$WID" "$OUT/mixer-2-initial.png"
# The application's fader (third channel): 0..150 from y 44 to y 194; y 119 is about 75.
xdotool mousemove $((AX + 220)) $((AY + 119)); sleep 0.2
xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 1.5
echo "3 application volume reached the server: $(cat "$STATE/stream_volume")% (expect about 74)"
import -window "$WID" "$OUT/mixer-3-dragged.png"
# The speakers' Mute box (first channel).
xdotool mousemove $((AX + 30)) $((AY + 209)); sleep 0.2
xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 1.5
echo "4 output mute reached the server: $(cat "$STATE/output_mute") (expect 1)"
import -window "$WID" "$OUT/mixer-4-muted.png"
kill $MIX 2>/dev/null; sleep 1
kill $OPENBOX $XVFB 2>/dev/null

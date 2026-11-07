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
XVFB= OPENBOX= MIX=
# However the script ends, including a signal, reap what it started so the
# display number is free for the next run.
cleanup() { kill $MIX $OPENBOX $XVFB 2>/dev/null; wait $MIX $OPENBOX $XVFB 2>/dev/null; }
trap cleanup EXIT
trap 'exit 130' INT TERM
SCREEN="${SCREEN_GEOMETRY:-1280x800x24}"
rm -rf "$OUT" && mkdir -p "$OUT/home/.config" "$OUT/fake-pactl/state"
cp "$ROOT/scripts/qa/fake-pactl/pactl" "$OUT/fake-pactl/pactl"
STATE="$OUT/fake-pactl/state"
# The fixture finds its state through this, wherever a copy of it is on the path.
export FAKE_PACTL_STATE="$STATE"
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
# Sets WID to the mixer window once it appears. If none appears within 10 s, the check stops
# here, so no later step runs with an empty window ID.
mixer_window() {
  local end=$((SECONDS + 10))
  while :; do
    WID=$(wid_of)
    [ -n "$WID" ] && return 0
    if [ "$SECONDS" -ge "$end" ]; then echo "FAIL the Volume Mixer window did not appear"; exit 1; fi
    sleep 0.2
  done
}

if command -v pactl >/dev/null 2>&1 && [ "$(command -v pactl)" = "$OUT/fake-pactl/pactl" ]; then :; fi
FAILED=0
# late-bin is the only PATH entry, and it holds no pactl yet, so a host pactl cannot
# be found and the server really is absent.
rm -rf "$OUT/late-bin" && mkdir -p "$OUT/late-bin"
env PATH="$OUT/late-bin" RUST_LOG=info "$BIN/slopos-volume-mixer" >"$OUT/mixer-unavailable.log" 2>&1 &
MIX=$!
mixer_window
echo "1 without a sound server: window=${WID:-none}"
[ -n "$WID" ] && import -window "$WID" "$OUT/mixer-1-unavailable.png"
# The server appears after the window opened. R asks for a new connection, which finds it.
if [ -e "$STATE/calls" ]; then echo "FAIL 1b the sound server was reached before the retry"; FAILED=1; fi
cp "$OUT/fake-pactl/pactl" "$OUT/late-bin/pactl"
[ -n "$WID" ] && xdotool windowfocus --sync "$WID" 2>/dev/null
xdotool key r; sleep 1.5
if [ -s "$STATE/calls" ]; then echo "PASS 1b the retry reached the sound server"; else echo "FAIL 1b the retry did not reach the sound server"; FAILED=1; fi
[ -n "$WID" ] && import -window "$WID" "$OUT/mixer-1b-retried.png"
kill $MIX 2>/dev/null; wait $MIX 2>/dev/null; sleep 1

# 1c: the Try again button does what R does. A fresh window starts with no server; the
# fixture is installed, and a click on the button (bottom-left, 8 px in) reaches it.
rm -f "$OUT/late-bin/pactl" "$STATE/calls"
# The fixture needs sleep and cat, so the PATH holds links to just those, and no pactl.
rm -rf "$OUT/tools" && mkdir -p "$OUT/tools"
ln -s "$(command -v sleep)" "$OUT/tools/sleep"
ln -s "$(command -v cat)" "$OUT/tools/cat"
env PATH="$OUT/late-bin:$OUT/tools" RUST_LOG=info "$BIN/slopos-volume-mixer" >"$OUT/mixer-click.log" 2>&1 &
MIX=$!
mixer_window
read CX CY < <(absolute "$WID")
H=$(xwininfo -id "$WID" | awk '/Height:/{print $2}')
if [ -e "$STATE/calls" ]; then echo "FAIL 1c the sound server was reached before the click"; FAILED=1; fi
cp "$OUT/fake-pactl/pactl" "$OUT/late-bin/pactl"
xdotool mousemove $((CX + 8 + 60)) $((CY + H - 8 - 13)); sleep 0.2
xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 1.5
if [ -s "$STATE/calls" ]; then echo "PASS 1c the Try again button reached the sound server"; else echo "FAIL 1c the Try again button did not reach the sound server"; FAILED=1; fi
[ -n "$WID" ] && import -window "$WID" "$OUT/mixer-1c-clicked.png"
# The recovered controls must work too: drag the application's fader (as step 3 does)
# and check that the server received the level.
xdotool mousemove $((CX + 220)) $((CY + 119)); sleep 0.2
xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 1.5
recovered=$(cat "$STATE/stream_volume")
if [ "$recovered" -ge 70 ] && [ "$recovered" -le 80 ]; then echo "PASS 1c the recovered controls reached the server: $recovered%"; else echo "FAIL 1c the recovered controls: $recovered% (expected 70-80)"; FAILED=1; fi
kill $MIX 2>/dev/null; wait $MIX 2>/dev/null; sleep 1

export PATH="$OUT/fake-pactl:$PATH"
RUST_LOG=info "$BIN/slopos-volume-mixer" >"$OUT/mixer.log" 2>&1 &
MIX=$!
mixer_window
read AX AY < <(absolute "$WID")
echo "2 with the fixture: window=$WID at $AX,$AY size=$(xwininfo -id "$WID" | awk '/Width:/{w=$2} /Height:/{h=$2} END{print w"x"h}')"
import -window "$WID" "$OUT/mixer-2-initial.png"
# The application's fader (third channel): 0..150 from y 44 to y 194; y 119 is about 75.
xdotool mousemove $((AX + 220)) $((AY + 119)); sleep 0.2
xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 1.5
volume=$(cat "$STATE/stream_volume")
if [ "$volume" -ge 70 ] && [ "$volume" -le 80 ]; then
  echo "PASS 3 application volume reached the server: $volume% (expected 70-80)"
else
  echo "FAIL 3 application volume: $volume% (expected 70-80)"; FAILED=1
fi
# The speakers' Mute box (first channel).
xdotool mousemove $((AX + 30)) $((AY + 209)); sleep 0.2
xdotool mousedown 1; sleep 0.2; xdotool mouseup 1; sleep 1.5
import -window "$WID" "$OUT/mixer-4-muted.png"
if [ "$(cat "$STATE/output_mute")" = 1 ]; then
  echo "PASS 4 output mute reached the server"
else
  echo "FAIL 4 output mute: $(cat "$STATE/output_mute") (expected 1)"; FAILED=1
fi
kill $MIX 2>/dev/null; sleep 1
kill $OPENBOX $XVFB 2>/dev/null
exit $FAILED

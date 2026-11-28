#!/usr/bin/env bash
# smoke-session.sh — start a real SLOPOS-I session on a private Xvfb display,
# check that the shell comes up, capture screenshots, drive the native
# Calculator and Text Editor (open a fixture, type, save, quit), log out, and
# check that every session process exits. Output goes to the folder given as
# $1 (CI uses $RUNNER_TEMP/smoke, uploaded as the session-smoke artifact).
#
# Usage: scripts/smoke-session.sh [OUTPUT_DIR]
#   SLOPOS_BIN_DIR   directory with the built binaries (default: target/debug)
#   SMOKE_DISPLAY    X display to use (default: :97)
#
# Needs: Xvfb, openbox, dbus-run-session, xdotool, and ImageMagick's `import`
# for screenshots (optional).
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$(mkdir -p "${1:-$ROOT/artifacts/smoke}" && cd "${1:-$ROOT/artifacts/smoke}" && pwd)"
BIN_DIR="$(cd "${SLOPOS_BIN_DIR:-$ROOT/target/debug}" && pwd)"
export DISPLAY="${SMOKE_DISPLAY:-:97}"

for tool in Xvfb openbox dbus-run-session xdotool; do
  command -v "$tool" >/dev/null || { echo "smoke: $tool is required" >&2; exit 2; }
done
for binary in slopos-session slopos-topbar; do
  test -x "$BIN_DIR/$binary" || { echo "smoke: build $BIN_DIR/$binary first" >&2; exit 2; }
done

WORK="$(mktemp -d)"
export HOME="$WORK/home" XDG_RUNTIME_DIR="$WORK/run" XDG_CONFIG_HOME="$WORK/home/.config"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export SLOPOS_SESSION_BIN="$BIN_DIR/slopos-session" SLOPOS_QA_NO_WELCOME=1 SLOPOS_NO_COMPOSITOR=1

XVFB_PID=""
SESSION_PID=""
cleanup() {
  set +e
  [ -n "$SESSION_PID" ] && kill "$SESSION_PID" 2>/dev/null
  # Only reap processes from the session this run started; before it starts,
  # $DISPLAY may belong to another X server.
  if [ -n "$SESSION_PID" ]; then
    for pid in $(session_pids); do kill "$pid" 2>/dev/null; done
  fi
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

fail() {
  echo "SMOKE FAIL: $*" >&2
  echo "----- session log -----" >&2
  tail -n 60 "$OUT/session.log" >&2 || true
  exit 1
}

# Session processes that belong to our display.
session_pids() {
  local name pid
  for name in slopos-session slopos-topbar slopos-desktop slopos-dock openbox; do
    for pid in $(pgrep -x "$name" 2>/dev/null || true); do
      if { tr '\0' '\n' < "/proc/$pid/environ"; } 2>/dev/null | grep -qx "DISPLAY=$DISPLAY"; then
        echo "$pid"
      fi
    done
  done
}

shoot() {
  if command -v import >/dev/null; then
    import -window root "$OUT/$1.png" 2>/dev/null || true
  fi
}

if xdotool getdisplaygeometry >/dev/null 2>&1; then
  fail "display $DISPLAY is already in use; set SMOKE_DISPLAY to a free display"
fi
Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
xdotool getdisplaygeometry >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY"

(cd "$ROOT" && exec dbus-run-session -- bash scripts/start-slopos-i) >"$OUT/session.log" 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 150); do
  xdotool search --name '^SLOPOS Top Bar$' >/dev/null 2>&1 && break
  kill -0 "$SESSION_PID" 2>/dev/null || fail "the session exited during startup"
  sleep 0.2
done
xdotool search --name '^SLOPOS Top Bar$' >/dev/null 2>&1 || fail "the menu bar never appeared"
pgrep -x openbox >/dev/null || fail "Openbox is not running"
for _ in $(seq 1 50); do
  xdotool search --name '^SLOPOS Dock$' >/dev/null 2>&1 && break
  sleep 0.2
done
xdotool search --name '^SLOPOS Dock$' >/dev/null 2>&1 || fail "the Dock never appeared"
if [ -x "$BIN_DIR/slopos-desktop" ]; then
  for _ in $(seq 1 50); do
    xdotool search --class '^slopos-desktop$' >/dev/null 2>&1 && break
    sleep 0.2
  done
  xdotool search --class '^slopos-desktop$' >/dev/null 2>&1 || fail "the desktop never appeared"
fi
sleep 1
shoot desktop
echo "smoke: menu bar and Dock are up"

# Open the File menu on the desktop menu bar and make sure the shell survives.
xdotool mousemove 72 12 click 1
sleep 0.7
shoot file-menu
xdotool key Escape
sleep 0.3
[ -n "$(session_pids)" ] || fail "session processes disappeared after opening a menu"

# Native toolkit app under the real window manager: type a sum, then close it.
DOCK_WIDTH_BEFORE="$(xdotool getwindowgeometry "$(xdotool search --name '^SLOPOS Dock$' | head -1)" | sed -n 's/.*Geometry: \([0-9]*\)x.*/\1/p')"
"$BIN_DIR/slopos-calculator" >"$OUT/calculator.log" 2>&1 &
CALC_PID=$!
for _ in $(seq 1 50); do
  CALC_WINDOW="$(xdotool search --name '^Calculator$' 2>/dev/null | head -1 || true)"
  [ -n "$CALC_WINDOW" ] && break
  kill -0 "$CALC_PID" 2>/dev/null || fail "the calculator exited during startup"
  sleep 0.2
done
[ -n "${CALC_WINDOW:-}" ] || fail "the calculator window never appeared"
timeout 10 xdotool windowactivate --sync "$CALC_WINDOW" 2>/dev/null || xdotool windowfocus "$CALC_WINDOW"
xdotool type --window "$CALC_WINDOW" '7*6='
sleep 0.5
shoot calculator
# The Dock follows the window list, so it now has a tile for Calculator.
DOCK_WINDOW="$(xdotool search --name '^SLOPOS Dock$' | head -1)"
DOCK_WIDTH="$(xdotool getwindowgeometry "$DOCK_WINDOW" | sed -n 's/.*Geometry: \([0-9]*\)x.*/\1/p')"
[ "${DOCK_WIDTH:-0}" -gt "${DOCK_WIDTH_BEFORE:-0}" ] || fail "the Dock did not add a tile for the calculator (${DOCK_WIDTH_BEFORE:-?} -> ${DOCK_WIDTH:-?})"
xdotool windowclose "$CALC_WINDOW"
for _ in $(seq 1 25); do kill -0 "$CALC_PID" 2>/dev/null || break; sleep 0.2; done
if kill -0 "$CALC_PID" 2>/dev/null; then
  kill "$CALC_PID"
  fail "the calculator kept running after its window was destroyed"
fi
echo "smoke: calculator opened and closed (typed input is not asserted)"

# Native Text Editor (EDITOR-01). Open a fixture, check that its text is painted
# in a 1x screenshot, type a character, save, check the file on disk, and quit
# with Ctrl+W. Every step fails the smoke; nothing here is best-effort.
EDITOR_FILE="$WORK/editor-fixture.txt"
printf 'hello editor\n' >"$EDITOR_FILE"
"$BIN_DIR/slopos-text-editor" "$EDITOR_FILE" >"$OUT/text-editor.log" 2>&1 &
EDITOR_PID=$!
editor_fail() {
  { echo "----- text editor log -----"; tail -n 40 "$OUT/text-editor.log"; } >&2 || true
  fail "text editor: $*"
}
title_is() { [ "$(xdotool getwindowname "$EDITOR_WINDOW" 2>/dev/null)" = "$1" ]; }
# Run a check every 0.2 s, at most $1 times; succeed as soon as it passes.
wait_for() {
  local tries="$1" i
  shift
  for i in $(seq 1 "$tries"); do
    "$@" && return 0
    sleep 0.2
  done
  return 1
}
capture_editor() {
  command -v import >/dev/null && command -v convert >/dev/null ||
    fail "text editor: ImageMagick import and convert are required for the screenshots"
  import -window "$EDITOR_WINDOW" "$OUT/$1.png" || editor_fail "could not capture the window as $1.png"
  [ -s "$OUT/$1.png" ] || editor_fail "$1.png is empty"
}
# Fraction of dark pixels in the crop w x h at x,y of a PNG; 0 means nothing is painted there.
ink_fraction() {
  convert "$1" -crop "${4}x${5}+${2}+${3}" +repage -colorspace Gray -threshold 50% -format '%[fx:1-mean]' info:
}

EDITOR_WINDOW=""
for _ in $(seq 1 50); do
  EDITOR_WINDOW="$(xdotool search --name 'Text Editor$' 2>/dev/null | head -1 || true)"
  [ -n "$EDITOR_WINDOW" ] && break
  kill -0 "$EDITOR_PID" 2>/dev/null || editor_fail "the editor exited during startup"
  sleep 0.2
done
[ -n "$EDITOR_WINDOW" ] || editor_fail "the editor window never appeared within 10 s"
timeout 10 xdotool windowactivate --sync "$EDITOR_WINDOW" 2>/dev/null ||
  xdotool windowfocus "$EDITOR_WINDOW" 2>/dev/null || editor_fail "could not focus the editor window"
sleep 0.5

# The menu bar is 20 px tall and the first line sits just below it. The crop
# starts right of the caret column (x 4), so only glyphs can make ink there.
capture_editor text-editor-1x
INK="$(ink_fraction "$OUT/text-editor-1x.png" 8 26 192 22)" || editor_fail "could not measure the screenshot"
awk -v v="$INK" 'BEGIN { exit !(v > 0.001) }' ||
  editor_fail "the fixture text is not painted in the text area (ink fraction $INK)"
echo "smoke: text editor painted the fixture text (ink fraction $INK)"

xdotool type --delay 50 'Z' || editor_fail "xdotool could not type into the editor"
wait_for 25 title_is '*editor-fixture.txt - Text Editor' ||
  editor_fail "typing did not mark the document as changed (title: $(xdotool getwindowname "$EDITOR_WINDOW" 2>/dev/null))"
capture_editor text-editor-typed

xdotool key ctrl+s || editor_fail "xdotool could not send Ctrl+S"
wait_for 25 grep -qx 'Zhello editor' "$EDITOR_FILE" ||
  editor_fail "Ctrl+S did not write the typed text to the fixture file"
wait_for 25 title_is 'editor-fixture.txt - Text Editor' ||
  editor_fail "saving did not clear the modified marker"

xdotool key ctrl+w || editor_fail "xdotool could not send Ctrl+W"
for _ in $(seq 1 25); do kill -0 "$EDITOR_PID" 2>/dev/null || break; sleep 0.2; done
if kill -0 "$EDITOR_PID" 2>/dev/null; then
  kill "$EDITOR_PID" 2>/dev/null || true
  editor_fail "Ctrl+W did not quit the editor within 5 s"
fi
EDITOR_STATUS=0
wait "$EDITOR_PID" || EDITOR_STATUS=$?
[ "$EDITOR_STATUS" -eq 0 ] || editor_fail "the editor exited with status $EDITOR_STATUS"
if grep -Eq "panicked at|RUST_BACKTRACE" "$OUT/text-editor.log"; then
  editor_fail "the editor panicked"
fi
echo "smoke: text editor showed its file, typed, saved and quit on Ctrl+W"

"$BIN_DIR/slopos-session" --logout || fail "slopos-session --logout failed"
for _ in $(seq 1 50); do
  [ -z "$(session_pids)" ] && break
  sleep 0.2
done
[ -z "$(session_pids)" ] || fail "processes still running after logout: $(session_pids | tr '\n' ' ')"
if grep -E "panicked at|RUST_BACKTRACE" "$OUT/session.log" "$OUT/calculator.log" >/dev/null; then
  fail "a SLOPOS process panicked"
fi
echo "smoke: logout ended the session cleanly"

# slopos-appearance must keep a legacy ~/.gtkrc-2.0 when it regenerates the
# file outside a managed session, then include the saved copy rather than the
# generated file itself.
fake_bin="$WORK/appearance-bin"
mkdir -p "$fake_bin"
for tool in openbox pkill slopos-session; do
  printf '#!/bin/sh\nexit 0\n' > "$fake_bin/$tool"
  chmod +x "$fake_bin/$tool"
done
printf 'gtk-font-name = "Legacy Sans 11"\n' > "$HOME/.gtkrc-2.0"
run_appearance() {
  (cd "$ROOT" && env -u SLOPOS_SESSION_MANAGED -u XDG_CURRENT_DESKTOP \
    PATH="$fake_bin:$PATH" scripts/slopos-appearance platinum) ||
    fail "slopos-appearance failed outside a managed session"
}
run_appearance
run_appearance
legacy_copy="$XDG_CONFIG_HOME/slopos-i/gtkrc-2.0.legacy"
[ -f "$legacy_copy" ] || fail "slopos-appearance did not keep the legacy GTK 2 file"
grep -Fq 'Legacy Sans 11' "$legacy_copy" || fail "the legacy GTK 2 copy lost its contents"
grep -Fq "include \"$legacy_copy\"" "$HOME/.gtkrc-2.0" ||
  fail "the replaced GTK 2 file does not include the saved legacy settings"
if grep -Fq "include \"$HOME/.gtkrc-2.0\"" "$HOME/.gtkrc-2.0"; then
  fail "the replaced GTK 2 file includes itself"
fi
echo "smoke: slopos-appearance kept the legacy GTK 2 settings"

# Display-only checks of the Dock, the Volume Mixer and the top bar's popover (AGENTS.md
# rule 6). Each starts its own display and exits nonzero on a mismatch. They run the
# binaries in target/debug; 1044 is the sound item's x on the 1280-wide screen.
qa_log() { echo "$OUT/qa-$1.log"; }
bash "$ROOT/scripts/qa/dock-autohide.sh" >"$(qa_log dock-autohide)" 2>&1 \
  || { tail -n 40 "$(qa_log dock-autohide)" >&2; fail "the Dock auto-hide check failed"; }
echo "smoke: the Dock follows auto-hide, the strut and a window's tile"
bash "$ROOT/scripts/qa/volume-mixer.sh" >"$(qa_log volume-mixer)" 2>&1 \
  || { tail -n 40 "$(qa_log volume-mixer)" >&2; fail "the Volume Mixer app check failed"; }
echo "smoke: the Volume Mixer app reaches the sound server and retries"
bash "$ROOT/scripts/qa/topbar-mixer.sh" 1044 >"$(qa_log topbar-mixer)" 2>&1 \
  || { tail -n 40 "$(qa_log topbar-mixer)" >&2; fail "the top bar's mixer popover check failed"; }
echo "smoke: the top bar's mixer popover opens, closes and reconnects"

echo "SMOKE PASS (screenshots in $OUT)"

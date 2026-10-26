#!/usr/bin/env bash
# smoke-session.sh — start a real SLOPOS-I session on a private Xvfb display,
# check that the shell comes up, capture screenshots, log out, and check that
# every session process exits.
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
for binary in slopos-session slopos-shell; do
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
  for name in slopos-session slopos-shell slopos-dock openbox; do
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
for tool in openbox pkill slopos-wallpaper xsetroot; do
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

echo "SMOKE PASS (screenshots in $OUT)"

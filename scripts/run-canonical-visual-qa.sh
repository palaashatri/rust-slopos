#!/usr/bin/env bash
# Capture reproducible SLOPOS-I desktop scenes for human/vision review.
# This script deliberately does not assign a visual score to its own output.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="${SLOPOS_VISUAL_QA_OUT:-$REPO_ROOT/artifacts/qa/canonical-visual}"
DISPLAY_ID="${SLOPOS_VISUAL_QA_DISPLAY:-:90}"
SOURCE_COMMIT="${SOURCE_SHA:-$(git rev-parse --verify HEAD 2>/dev/null || printf '%s' unknown)}"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.png "$OUT_DIR"/manifest.txt

for command in Xvfb dbus-run-session xdotool scrot openbox; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "visual QA requires '$command'" >&2
    exit 2
  }
done

cargo build --release --workspace --locked

QA_HOME="$(mktemp -d)"
cleanup() {
  set +e
  [[ -n "${SESSION_PID:-}" ]] && kill "$SESSION_PID" >/dev/null 2>&1
  [[ -n "${XVFB_PID:-}" ]] && kill "$XVFB_PID" >/dev/null 2>&1
  rm -rf "$QA_HOME"
}
trap cleanup EXIT INT TERM

export HOME="$QA_HOME"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_CACHE_HOME="$HOME/.cache"
export XDG_CURRENT_DESKTOP=SLOPOS
export XDG_SESSION_DESKTOP=slopos
export SLOPOS_SESSION_MANAGED=1
export SLOPOS_QA_NO_WELCOME=1
export GDK_BACKEND=x11
export DISPLAY="$DISPLAY_ID"

mkdir -p \
  "$HOME/.config/gtk-3.0" \
  "$HOME/.config/openbox" \
  "$HOME/.themes/slopos-openbox/openbox-3" \
  "$HOME/.themes/slopos-openbox-classic/openbox-3" \
  "$HOME/.themes/slopos-openbox-graphite/openbox-3" \
  "$HOME/.themes/slopos-openbox-oled/openbox-3" \
  "$HOME/.themes/slopos-gtk/gtk-3.0" \
  "$HOME/.themes/slopos-gtk-classic/gtk-3.0" \
  "$HOME/.themes/slopos-gtk-graphite/gtk-3.0" \
  "$HOME/.themes/slopos-gtk-oled/gtk-3.0" \
  "$HOME/.icons/SLOPOS-Platinum" \
  "$HOME/.local/share/icons/SLOPOS-Platinum"

cp assets/config/gtk-3.0/gtk.css "$HOME/.config/gtk-3.0/gtk.css"
cp assets/config/gtk-3.0/settings.ini "$HOME/.config/gtk-3.0/settings.ini"
cp assets/config/openbox/rc.xml "$HOME/.config/openbox/rc.xml"
cp themes/slopos-openbox/openbox-3/themerc "$HOME/.themes/slopos-openbox/openbox-3/themerc"
cp themes/slopos-openbox-classic/openbox-3/themerc "$HOME/.themes/slopos-openbox-classic/openbox-3/themerc"
cp themes/slopos-openbox-graphite/openbox-3/themerc "$HOME/.themes/slopos-openbox-graphite/openbox-3/themerc"
cp themes/slopos-openbox-oled/openbox-3/themerc "$HOME/.themes/slopos-openbox-oled/openbox-3/themerc"
cp assets/config/gtk-3.0/gtk.css "$HOME/.themes/slopos-gtk/gtk-3.0/gtk.css"
cp assets/config/gtk-3.0/gtk-classic.css "$HOME/.themes/slopos-gtk-classic/gtk-3.0/gtk.css"
cp assets/config/gtk-3.0/gtk-graphite.css "$HOME/.themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
cp assets/config/gtk-3.0/gtk-oled.css "$HOME/.themes/slopos-gtk-oled/gtk-3.0/gtk.css"
cp -a themes/platinum/icon-theme/. "$HOME/.icons/SLOPOS-Platinum/"
cp -a themes/platinum/icon-theme/. "$HOME/.local/share/icons/SLOPOS-Platinum/"

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >"$OUT_DIR/xvfb.log" 2>&1 &
XVFB_PID=$!
sleep 1
xsetroot -solid "#758090"

DBUS_ENV="$OUT_DIR/dbus.env"
rm -f "$DBUS_ENV"
dbus-run-session -- bash -c '
  printf "export DBUS_SESSION_BUS_ADDRESS=%q\n" "$DBUS_SESSION_BUS_ADDRESS" > "$1"
  exec "$2"
' bash "$DBUS_ENV" ./target/release/slopos-session >"$OUT_DIR/session.log" 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 60); do
  if [[ -s "$DBUS_ENV" ]] && \
     xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1 && \
     xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

test -s "$DBUS_ENV"
# shellcheck disable=SC1090
source "$DBUS_ENV"
xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null
xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null

capture() {
  local name="$1"
  xdotool mousemove 1260 780 >/dev/null 2>&1 || true
  sleep 0.25
  scrot -zo "$OUT_DIR/$name"
  test -s "$OUT_DIR/$name"
  printf '%s\n' "$name" >> "$OUT_DIR/manifest.txt"
}

close_named() {
  local pattern="$1"
  while read -r window; do
    [[ -n "$window" ]] && xdotool windowclose "$window" >/dev/null 2>&1 || true
  done < <(xdotool search --onlyvisible --name "$pattern" 2>/dev/null || true)
}

capture "01_platinum_desktop_1280x800.png"

pkill -USR2 -x slopos-shell
sleep 0.35
capture "02_system_menu_1280x800.png"
xdotool key Escape

pkill -USR1 -x slopos-shell
sleep 0.35
xdotool type --delay 35 "Terminal"
capture "03_application_search_1280x800.png"
xdotool key Escape

notify-send -t 5000 -a "SLOPOS QA" "Visual QA" "Notification surface and typography check" || true
sleep 0.35
capture "04_notification_1280x800.png"
close_named '^SLOPOS Notification'

./target/release/slopos-settings >/dev/null 2>&1 &
sleep 0.6
capture "05_system_settings_1280x800.png"
close_named '^System Settings$'

./target/release/slopos-settings --appearance >/dev/null 2>&1 &
sleep 0.6
capture "06_appearance_settings_1280x800.png"
close_named '^Appearance$'

./target/release/slopos-settings --wallpaper >/dev/null 2>&1 &
sleep 0.6
capture "07_wallpaper_settings_1280x800.png"
close_named '^Desktop & Wallpaper$'

if [[ -x ./target/release/slopos-catalogue ]]; then
  ./target/release/slopos-catalogue >/dev/null 2>&1 &
  sleep 0.7
  capture "08_software_catalogue_1280x800.png"
  close_named 'Software Catalogue'
fi

if command -v mousepad >/dev/null 2>&1; then
  mousepad README.md >/dev/null 2>&1 &
  sleep 0.7
  capture "09_text_editor_integration_1280x800.png"
  pkill -TERM -x mousepad >/dev/null 2>&1 || true
fi

if command -v pcmanfm >/dev/null 2>&1; then
  pcmanfm "$REPO_ROOT" >/dev/null 2>&1 &
  sleep 0.8
  capture "10_file_manager_integration_1280x800.png"
  pkill -TERM -x pcmanfm >/dev/null 2>&1 || true
fi

if command -v xfce4-terminal >/dev/null 2>&1; then
  xfce4-terminal >/dev/null 2>&1 &
  sleep 0.7
  capture "11_terminal_integration_1280x800.png"
  pkill -TERM -x xfce4-terminal >/dev/null 2>&1 || true
fi

if [[ -x scripts/slopos-wallpaper ]]; then
  scripts/slopos-wallpaper set 03_slate_blue.png --mode fill >/dev/null 2>&1 || true
  sleep 0.4
  capture "12_slate_blue_wallpaper_1280x800.png"
fi

if [[ -x scripts/slopos-appearance ]]; then
  scripts/slopos-appearance graphite >/dev/null 2>&1 || true
  sleep 1
  capture "13_graphite_desktop_1280x800.png"
fi

printf 'source_commit=%s\n' "$SOURCE_COMMIT" >> "$OUT_DIR/manifest.txt"
printf 'captured_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$OUT_DIR/manifest.txt"
printf 'CANONICAL_VISUAL_CAPTURE_OK\n'

#!/usr/bin/env bash
# SLOPOS-I X11 Docker/Xvfb development QA.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-qa-runtime
export SLOPOS_OPENBOX_CONFIG=/workspace/assets/config/openbox/rc.xml
export SLOPOS_QA_NO_WELCOME=1
DBUS_ENV_FILE="$XDG_RUNTIME_DIR/dbus-env.sh"
mkdir -p "$XDG_RUNTIME_DIR" artifacts/qa/screenshots
chmod 700 "$XDG_RUNTIME_DIR"

# The container is disposable, but the mounted workspace is not. Remove
# stale processes and every generated capture so a failed run cannot satisfy
# a later run with an old window or a scrot-generated `_000` image.
for process in slopos-session slopos-shell slopos-settings slopos-catalogue openbox; do
  pkill -TERM -x "$process" 2>/dev/null || true
done
rm -f artifacts/qa/screenshots/*.png

cleanup() {
  set +e
  kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${TERM_PID:-}" "${PCMAN_PID:-}" \
       "${SESSION_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT

wait_visible_window() {
  local pattern="$1"
  for _ in $(seq 1 40); do
    if xdotool search --onlyvisible --name "$pattern" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "ERROR: visible window not found: $pattern" >&2
  return 1
}

close_visible_windows_by_class() {
  local class="$1"
  for _ in $(seq 1 20); do
    local windows
    windows="$(xdotool search --onlyvisible --class "$class" 2>/dev/null || true)"
    if [[ -z "$windows" ]]; then
      return 0
    fi
    while read -r window; do
      [[ -n "$window" ]] && xdotool windowclose "$window"
    done <<<"$windows"
    sleep 0.25
  done
  echo "ERROR: visible windows remain for class: $class" >&2
  return 1
}

echo "[1/8] Installing X11/GTK QA dependencies"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  xvfb openbox pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator \
  libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev pkg-config \
  python3 scrot imagemagick x11-utils x11-xserver-utils xdotool wmctrl dbus-x11 librsvg2-common curl git build-essential \
  ca-certificates adwaita-icon-theme fonts-liberation fonts-dejavu-core libnotify-bin

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
fi

mkdir -p "$HOME/.themes/slopos-openbox/openbox-3" /usr/share/themes/slopos-openbox/openbox-3
cp themes/slopos-openbox/openbox-3/themerc "$HOME/.themes/slopos-openbox/openbox-3/themerc"
cp themes/slopos-openbox/openbox-3/themerc /usr/share/themes/slopos-openbox/openbox-3/themerc

mkdir -p /usr/share/themes/slopos-gtk/gtk-3.0 "$HOME/.config/gtk-3.0"
cp assets/config/gtk-3.0/gtk.css /usr/share/themes/slopos-gtk/gtk-3.0/gtk.css
cp assets/config/gtk-3.0/gtk.css "$HOME/.config/gtk-3.0/gtk.css"
if [[ -f assets/config/gtk-3.0/settings.ini ]]; then
  cp assets/config/gtk-3.0/settings.ini "$HOME/.config/gtk-3.0/settings.ini"
fi

echo "[2/8] Build + test"
cargo build --workspace --release --locked
cargo test --workspace --locked

echo "[3/8] Start Xvfb and SLOPOS session"
Xvfb :99 -screen 0 1280x800x24 >artifacts/qa/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 2
xsetroot -solid "#758090"
rm -f "$DBUS_ENV_FILE"
dbus-run-session -- bash -c '
  printf "export DBUS_SESSION_BUS_ADDRESS=%q\\n" "$DBUS_SESSION_BUS_ADDRESS" > "$1"
  exec "$2"
' bash "$DBUS_ENV_FILE" ./target/release/slopos-session >artifacts/qa/session.log 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 20); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null && [[ -s "$DBUS_ENV_FILE" ]]; then break; fi
  sleep 1
done
pgrep -x openbox >/dev/null
pgrep -x slopos-shell >/dev/null
test -s "$DBUS_ENV_FILE"
# shellcheck source=/dev/null
source "$DBUS_ENV_FILE"
wait_visible_window '^SLOPOS Top Bar$'
wait_visible_window '^SLOPOS Application Strip$'
TOPBAR_WINDOW="$(xdotool search --onlyvisible --name '^SLOPOS Top Bar$' | tail -n 1)"
test -n "$TOPBAR_WINDOW"

echo "[4/8] Verify launcher hotkey toggles existing shell"
SHELL_COUNT_BEFORE="$(pgrep -xc slopos-shell)"
pkill -USR1 -x slopos-shell
sleep 2
SHELL_COUNT_AFTER="$(pgrep -xc slopos-shell)"
test "$SHELL_COUNT_BEFORE" = "$SHELL_COUNT_AFTER"
wait_visible_window '^SLOPOS Search$'
scrot -zo artifacts/qa/screenshots/search_open_1280x800.png
xdotool key Escape || true

echo "[5/8] Verify session recovery after child failure"
shell_before="$(pgrep -xo slopos-shell)"
kill "$shell_before"
shell_after=""
for _ in $(seq 1 40); do
  shell_after="$(pgrep -xo slopos-shell 2>/dev/null || true)"
  if [[ -n "$shell_after" && "$shell_after" != "$shell_before" ]]; then break; fi
  sleep 0.25
done
test -n "$shell_after"
test "$shell_after" != "$shell_before"
wait_visible_window '^SLOPOS Top Bar$'

wm_before="$(pgrep -xo openbox)"
kill "$wm_before"
wm_after=""
for _ in $(seq 1 40); do
  wm_after="$(pgrep -xo openbox 2>/dev/null || true)"
  if [[ -n "$wm_after" && "$wm_after" != "$wm_before" ]]; then break; fi
  sleep 0.25
done
test -n "$wm_after"
test "$wm_after" != "$wm_before"
wait_visible_window '^SLOPOS Application Strip$'

echo "[6/8] Capture canonical scenes"
scrot -zo artifacts/qa/screenshots/clean_desktop_1280x800.png

# Exercise a real top-bar menu and its About dialog. The dialog assertion also
# proves that the menu click/keyboard path reached a functional item.
xdotool windowactivate --sync "$TOPBAR_WINDOW"
xdotool mousemove --window "$TOPBAR_WINDOW" --sync 14 13
xdotool click 1
sleep 1
scrot -zo artifacts/qa/screenshots/menu_open_1280x800.png
xdotool key Down Return
wait_visible_window '^About SLOPOS-I$'
scrot -zo artifacts/qa/screenshots/modal_about_1280x800.png
xdotool key Return
sleep 1

# Exercise the actual freedesktop notification path on the session bus.
notify-send -a "SLOPOS QA" -i dialog-information "SLOPOS QA Notification" \
  "A real D-Bus notification rendered by the SLOPOS presenter."
wait_visible_window '^SLOPOS Notification [0-9]+$'
scrot -zo artifacts/qa/screenshots/notification_1280x800.png
sleep 7

pcmanfm /workspace >artifacts/qa/pcmanfm.log 2>&1 & PCMAN_PID=$!
sleep 2
xdotool search --onlyvisible --class pcmanfm >/dev/null
scrot -zo artifacts/qa/screenshots/active_app_1280x800.png

xfce4-terminal >artifacts/qa/terminal.log 2>&1 & TERM_PID=$!
sleep 2
xdotool search --onlyvisible --class xfce4-terminal >/dev/null
scrot -zo artifacts/qa/screenshots/multi_window_1280x800.png
close_visible_windows_by_class xfce4-terminal
kill "$TERM_PID" 2>/dev/null || true
unset TERM_PID
scrot -zo artifacts/qa/screenshots/file_manager_1280x800.png
kill "$PCMAN_PID" 2>/dev/null || true
unset PCMAN_PID

xfce4-terminal >artifacts/qa/terminal.log 2>&1 & TERM_PID=$!
sleep 2
xdotool search --onlyvisible --class xfce4-terminal >/dev/null
scrot -zo artifacts/qa/screenshots/terminal_1280x800.png
kill "$TERM_PID" 2>/dev/null || true
unset TERM_PID

./target/release/slopos-catalogue >artifacts/qa/catalogue.log 2>&1 & CATALOGUE_PID=$!
for _ in $(seq 1 20); do
  CATALOGUE_WINDOW="$(xdotool search --onlyvisible --name '^Software Catalogue$' 2>/dev/null | tail -n 1 || true)"
  if [[ -n "$CATALOGUE_WINDOW" ]]; then break; fi
  sleep 1
done
test -n "${CATALOGUE_WINDOW:-}"
test "$(xdotool getwindowpid "$CATALOGUE_WINDOW")" = "$CATALOGUE_PID"
scrot -zo artifacts/qa/screenshots/catalogue_store_1280x800.png
kill "$CATALOGUE_PID" 2>/dev/null || true
unset CATALOGUE_PID

./target/release/slopos-settings >artifacts/qa/settings.log 2>&1 & SETTINGS_PID=$!
for _ in $(seq 1 20); do
  SETTINGS_WINDOW="$(xdotool search --onlyvisible --name '^System Settings$' 2>/dev/null | tail -n 1 || true)"
  if [[ -n "$SETTINGS_WINDOW" ]]; then break; fi
  sleep 1
done
test -n "${SETTINGS_WINDOW:-}"
test "$(xdotool getwindowpid "$SETTINGS_WINDOW")" = "$SETTINGS_PID"
scrot -zo artifacts/qa/screenshots/system_settings_1280x800.png
kill "$SETTINGS_PID" 2>/dev/null || true
unset SETTINGS_PID

echo "[7/8] Validate screenshot evidence"
for image in \
  artifacts/qa/screenshots/clean_desktop_1280x800.png \
  artifacts/qa/screenshots/menu_open_1280x800.png \
  artifacts/qa/screenshots/search_open_1280x800.png \
  artifacts/qa/screenshots/notification_1280x800.png \
  artifacts/qa/screenshots/modal_about_1280x800.png \
  artifacts/qa/screenshots/active_app_1280x800.png \
  artifacts/qa/screenshots/multi_window_1280x800.png \
  artifacts/qa/screenshots/file_manager_1280x800.png \
  artifacts/qa/screenshots/terminal_1280x800.png \
  artifacts/qa/screenshots/catalogue_store_1280x800.png \
  artifacts/qa/screenshots/system_settings_1280x800.png; do
  test -s "$image"
  test "$(identify -format '%wx%h' "$image")" = "1280x800"
done

echo "[8/8] Product-contract sanity checks"
! grep -Eq 'slopos-compositor|share/wayland-sessions' install.sh
! grep -Eq 'smithay|wayland-client|wayland-server' Cargo.toml
! grep -Fq 'create_stub_appimage' crates/slopos-catalogue/src/installer.rs
grep -Fq 'eq_ignore_ascii_case(EMPTY_FILE_SHA256)' crates/slopos-catalogue/src/model.rs

echo "SLOPOS-I Docker/Xvfb functional evidence PASS"
echo "Canonical screenshots captured under artifacts/qa/screenshots/."
echo "Visual acceptance remains a separate human/vision review gate."

#!/usr/bin/env bash
# SLOPOS-I X11 Docker/Xvfb development QA.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-qa-runtime
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$REPO_ROOT/assets/config/openbox/rc.xml}"
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
  kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${TERM_PID:-}" "${PCMAN_PID:-}" "${TEXT_PID:-}" \
       "${APPMENU_FIXTURE_PID:-}" "${SESSION_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
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

window_for_pid() {
  local pid="$1" window window_pid
  for window in $(xdotool search --onlyvisible --name '.*' 2>/dev/null || true); do
    window_pid="$(xdotool getwindowpid "$window" 2>/dev/null || true)"
    if [[ "$window_pid" == "$pid" ]]; then
      printf '%s\n' "$window"
      return 0
    fi
  done
  return 1
}

wait_window_for_pid() {
  local pid="$1"
  for _ in $(seq 1 40); do
    if window_for_pid "$pid" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "ERROR: visible window not found for pid: $pid" >&2
  return 1
}

capture_screenshot() {
  local output="$1"
  local width height
  read -r width height < <(xdotool getdisplaygeometry)
  # Keep pointer-driven tooltips out of canonical evidence. This is capture
  # hygiene only; it does not alter application input or focus.
  xdotool mousemove "$((width - 24))" "$((height - 24))"
  sleep 0.35
  scrot -zo "$output"
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

if [[ "${SLOPOS_QA_SKIP_DEPS:-0}" == "1" ]]; then
  echo "[1/8] Using pre-provisioned X11/GTK QA dependencies"
else
  echo "[1/8] Installing X11/GTK QA dependencies"
  apt-get update -qq
  apt-get install -y -qq --no-install-recommends \
    xvfb openbox pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator \
    libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev pkg-config \
    python3 scrot imagemagick x11-utils x11-xserver-utils xdotool wmctrl dbus-x11 librsvg2-common curl git build-essential \
    ca-certificates adwaita-icon-theme fonts-liberation fonts-dejavu-core libnotify-bin
fi

# Build a disposable, standard DBusMenu exporter for the end-to-end AppMenu
# check.  The fixture is QA-only: it is compiled into /tmp, never installed,
# and owns no application UI.  If the cached image does not carry the DBus
# development headers, the real-exporter leg remains an explicit skip rather
# than silently turning a property fixture into evidence.
APPMENU_FIXTURE_BIN=/tmp/slopos-qa-dbusmenu-exporter
APPMENU_FIXTURE_AVAILABLE=0
APPMENU_MOUSEPAD_SCREENSHOT="appmenu_fallback_mousepad_1280x800.png"
if command -v gcc >/dev/null 2>&1 && command -v pkg-config >/dev/null 2>&1 && \
   pkg-config --exists dbus-1; then
  gcc -std=c11 -Wall -Wextra -Werror scripts/qa-dbusmenu-exporter.c \
    $(pkg-config --cflags --libs dbus-1) -o "$APPMENU_FIXTURE_BIN"
  APPMENU_FIXTURE_AVAILABLE=1
  echo "APPMENU_REAL_FIXTURE_COMPILE_STATUS_0"
else
  echo "APPMENU_REAL_FIXTURE_STATUS_SKIPPED_NO_DBUS_DEV"
  if [[ "${SLOPOS_QA_REQUIRE_REAL_APPMENU:-0}" == "1" ]]; then
    echo "ERROR: real AppMenu QA requested but gcc/pkg-config/libdbus-1 are unavailable" >&2
    exit 2
  fi
fi

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" != "1" ]] && ! command -v cargo >/dev/null 2>&1; then
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

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" == "1" ]]; then
  echo "[2/8] Using prebuilt release binaries"
  for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
    test -x "target/release/$binary"
  done
else
  echo "[2/8] Build + test"
  cargo build --workspace --release --locked
  cargo test --workspace --locked
fi

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
capture_screenshot artifacts/qa/screenshots/search_open_1280x800.png
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
wait_visible_window '^SLOPOS Top Bar$'
sleep 1
# The supervisor recreates shell windows after the deliberate recovery test;
# refresh the ID before later pointer-driven AppMenu checks instead of
# reusing the destroyed pre-recovery top-bar window.
TOPBAR_WINDOW="$(xdotool search --onlyvisible --name '^SLOPOS Top Bar$' | tail -n 1)"
test -n "$TOPBAR_WINDOW"

echo "[6/8] Capture canonical scenes"
capture_screenshot artifacts/qa/screenshots/clean_desktop_1280x800.png

# Exercise a real top-bar menu and its About dialog. The dialog assertion also
# proves that the menu click/keyboard path reached a functional item.
xdotool windowactivate --sync "$TOPBAR_WINDOW"
xdotool mousemove --window "$TOPBAR_WINDOW" --sync 14 13
xdotool click 1
sleep 1
capture_screenshot artifacts/qa/screenshots/menu_open_1280x800.png
xdotool key Down Return
wait_visible_window '^About SLOPOS-I$'
capture_screenshot artifacts/qa/screenshots/modal_about_1280x800.png
xdotool key Return
sleep 1

# Exercise the actual freedesktop notification path on the session bus.
# An empty icon asks the SLOPOS presenter to use its packaged mark, making
# the canonical notification scene exercise the product identity rather than
# a generic desktop icon.
if command -v notify-send >/dev/null 2>&1; then
  notify-send -t 60000 -a "SLOPOS QA" -i "" "SLOPOS QA Notification" \
    "A real D-Bus notification rendered by the SLOPOS presenter."
else
  dbus-send --session --dest=org.freedesktop.Notifications --type=method_call \
    /org/freedesktop/Notifications org.freedesktop.Notifications.Notify \
    string:"SLOPOS QA" uint32:0 string:"" string:"SLOPOS QA Notification" \
    string:"A real D-Bus notification rendered by the SLOPOS presenter." \
    array:string: dict:string:variant: int32:60000
fi
wait_visible_window '^SLOPOS Notification [0-9]+$'
capture_screenshot artifacts/qa/screenshots/notification_1280x800.png
# The long timeout keeps the notification visible long enough to capture, but
# it must not contaminate later canonical scenes. Close only the fresh visible
# SLOPOS notification window and assert that it is gone before continuing.
for notification_window in $(xdotool search --onlyvisible --name '^SLOPOS Notification [0-9]+$' 2>/dev/null || true); do
  xdotool windowclose "$notification_window"
done
for _ in $(seq 1 20); do
  if ! xdotool search --onlyvisible --name '^SLOPOS Notification [0-9]+$' >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
! xdotool search --onlyvisible --name '^SLOPOS Notification [0-9]+$' >/dev/null 2>&1

# The active-application scene must be distinct from the file-manager scene;
# Mousepad gives the visual gate a real text-editor surface to inspect.
mousepad "$REPO_ROOT/README.md" >artifacts/qa/mousepad.log 2>&1 & TEXT_PID=$!
wait_window_for_pid "$TEXT_PID"
TEXT_WINDOW="$(window_for_pid "$TEXT_PID")"
test -n "$TEXT_WINDOW"
xdotool windowactivate --sync "$TEXT_WINDOW"
capture_screenshot artifacts/qa/screenshots/active_app_1280x800.png

# Mousepad in the provisioned image exports the X11 AppMenu properties. Exercise
# the real top-bar App button when that capability is present. A genuine
# DBusMenu exporter must produce an additional visible popup; a property-only
# or malformed exporter must produce an explicit fail-closed fallback instead
# of a fabricated menu. A non-exporting build records an honest skip.
if [[ "$APPMENU_FIXTURE_AVAILABLE" == 1 ]] || \
   xprop -id "$TEXT_WINDOW" | grep -qE '_GTK_(UNIQUE_BUS_NAME|APP_MENU_OBJECT_PATH|MENUBAR_OBJECT_PATH)'; then
  APPMENU_MOUSEPAD_CAPTURED=1
  before_appmenu_windows="$(xdotool search --onlyvisible --name '.*' | wc -l)"
  # Keep Mousepad focused while clicking the top-bar button: activating the
  # shell window first would correctly clear the focused exporter before the
  # callback can consume the cached capability. The active-title label is
  # capped at 28 characters, placing App at this stable 1280px coordinate.
  xdotool windowfocus --sync "$TEXT_WINDOW"
  if [[ "$APPMENU_FIXTURE_AVAILABLE" == 1 ]]; then
    APPMENU_BUS_FILE=/tmp/slopos-qa-dbusmenu.bus
    APPMENU_EVENT_FILE=/tmp/slopos-qa-dbusmenu.events
    rm -f "$APPMENU_BUS_FILE" "$APPMENU_EVENT_FILE"
    "$APPMENU_FIXTURE_BIN" "$APPMENU_BUS_FILE" "$APPMENU_EVENT_FILE" \
      >/tmp/slopos-qa-dbusmenu.log 2>&1 & APPMENU_FIXTURE_PID=$!
    for _ in $(seq 1 40); do
      [[ -s "$APPMENU_BUS_FILE" ]] && break
      sleep 0.1
    done
    test -s "$APPMENU_BUS_FILE"
    APPMENU_BUS_NAME="$(cat "$APPMENU_BUS_FILE")"
    xprop -id "$TEXT_WINDOW" -f _GTK_UNIQUE_BUS_NAME 8s \
      -set _GTK_UNIQUE_BUS_NAME "$APPMENU_BUS_NAME"
    xprop -id "$TEXT_WINDOW" -f _GTK_APP_MENU_OBJECT_PATH 8s \
      -set _GTK_APP_MENU_OBJECT_PATH '/org/slopos/qa/dbusmenu'
    xdotool windowfocus --sync "$TEXT_WINDOW"
    sleep 1
  fi
  grep -Fq 'exports AppMenu bus=' artifacts/qa/session.log
  APPMENU_FOCUS_BEFORE="$(xdotool getactivewindow)"
  test "$APPMENU_FOCUS_BEFORE" = "$TEXT_WINDOW"
  xdotool mousemove --window "$TOPBAR_WINDOW" --sync "${SLOPOS_QA_APP_MENU_X:-270}" 13
  xdotool click 1
  APPMENU_FOCUS_AFTER="$(xdotool getactivewindow)"
  # Desktop chrome must not steal X11 focus from the exporter window.  This
  # keeps the imported menu tied to the app the user was actually using.
  test "$APPMENU_FOCUS_AFTER" = "$APPMENU_FOCUS_BEFORE"
  sleep 1
  if grep -Fq "Focused application's AppMenu was not imported" artifacts/qa/session.log; then
    if [[ "$APPMENU_FIXTURE_AVAILABLE" == 1 ]]; then
      echo "ERROR: real DBusMenu fixture did not import" >&2
      exit 1
    fi
    echo "APPMENU_MOUSEPAD_FALLBACK_STATUS_0"
  else
    appmenu_popup_windows=""
    for _ in $(seq 1 20); do
      appmenu_popup_windows="$(xdotool search --onlyvisible --name '.*' | wc -l)"
      if [[ "$appmenu_popup_windows" -gt "$before_appmenu_windows" ]]; then
        break
      fi
      sleep 0.25
    done
    test "$appmenu_popup_windows" -gt "$before_appmenu_windows"
    if [[ "$APPMENU_FIXTURE_AVAILABLE" == 1 ]]; then
      # The imported menu contains one real item.  Activating it must travel
      # back through the protocol's Event call; no shell-side command is
      # fabricated for the item.
      xdotool key Down Return
      for _ in $(seq 1 20); do
        grep -Fq 'clicked id=1 event=clicked' "$APPMENU_EVENT_FILE" && break
        sleep 0.1
      done
      grep -Fq 'clicked id=1 event=clicked' "$APPMENU_EVENT_FILE"
      echo "APPMENU_REAL_IMPORT_STATUS_0"
    else
      echo "APPMENU_MOUSEPAD_IMPORT_STATUS_0"
    fi
    # Keep the retained filename truthful: only a successful layout/event
    # path may be called imported. Property-only or UnknownMethod fallback
    # evidence remains explicitly named as fallback below.
    APPMENU_MOUSEPAD_SCREENSHOT="appmenu_imported_mousepad_1280x800.png"
  fi
  capture_screenshot "artifacts/qa/screenshots/$APPMENU_MOUSEPAD_SCREENSHOT"
  xdotool key Escape
  echo "APPMENU_MOUSEPAD_STATUS_0"
else
  APPMENU_MOUSEPAD_CAPTURED=0
  echo "APPMENU_MOUSEPAD_STATUS_SKIPPED_NO_EXPORTER"
fi
close_visible_windows_by_class mousepad
kill "$TEXT_PID" 2>/dev/null || true
unset TEXT_PID

pcmanfm "$REPO_ROOT" >artifacts/qa/pcmanfm.log 2>&1 & PCMAN_PID=$!
wait_window_for_pid "$PCMAN_PID"
PCMAN_WINDOW="$(window_for_pid "$PCMAN_PID")"
test -n "$PCMAN_WINDOW"

xfce4-terminal >artifacts/qa/terminal.log 2>&1 & TERM_PID=$!
sleep 2
TERM_WINDOW="$(xdotool search --onlyvisible --class xfce4-terminal | tail -n 1)"
test -n "$TERM_WINDOW"
# Arrange the overlap scene deliberately so both upstream windows remain fully
# visible above the Application Strip instead of relying on WM placement luck.
xdotool windowmove --sync "$TERM_WINDOW" 520 300
xdotool windowsize "$TERM_WINDOW" 610 360
sleep 1
ACTIVE_BEFORE="$(xdotool getactivewindow)"
xdotool key --clearmodifiers alt+Tab
sleep 0.5
ACTIVE_AFTER="$(xdotool getactivewindow)"
test "$ACTIVE_BEFORE" != "$ACTIVE_AFTER"
xdotool windowactivate --sync "$TERM_WINDOW"
xdotool windowfocus --sync "$TERM_WINDOW"
sleep 1
capture_screenshot artifacts/qa/screenshots/multi_window_1280x800.png
close_visible_windows_by_class xfce4-terminal
kill "$TERM_PID" 2>/dev/null || true
unset TERM_PID
capture_screenshot artifacts/qa/screenshots/file_manager_1280x800.png
kill "$PCMAN_PID" 2>/dev/null || true
unset PCMAN_PID

xfce4-terminal >artifacts/qa/terminal.log 2>&1 & TERM_PID=$!
sleep 2
xdotool search --onlyvisible --class xfce4-terminal >/dev/null
capture_screenshot artifacts/qa/screenshots/terminal_1280x800.png
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
capture_screenshot artifacts/qa/screenshots/catalogue_store_1280x800.png
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
capture_screenshot artifacts/qa/screenshots/system_settings_1280x800.png
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
if [[ "$APPMENU_MOUSEPAD_CAPTURED" == 1 ]]; then
  test -s "artifacts/qa/screenshots/$APPMENU_MOUSEPAD_SCREENSHOT"
  test "$(identify -format '%wx%h' "artifacts/qa/screenshots/$APPMENU_MOUSEPAD_SCREENSHOT")" = "1280x800"
fi

echo "[8/8] Product-contract sanity checks"
! grep -Eq 'slopos-compositor|share/wayland-sessions' install.sh
! grep -Eq 'smithay|wayland-client|wayland-server' Cargo.toml
! grep -Fq 'create_stub_appimage' crates/slopos-catalogue/src/installer.rs
grep -Fq 'eq_ignore_ascii_case(EMPTY_FILE_SHA256)' crates/slopos-catalogue/src/model.rs
grep -Fq 'non_empty_metadata(&self.description)' crates/slopos-catalogue/src/model.rs
grep -Fq 'valid_icon_name(&self.icon_name)' crates/slopos-catalogue/src/model.rs
grep -Fq 'if !valid_id(&app.id)' crates/slopos-catalogue/src/installer.rs

echo "SLOPOS-I Docker/Xvfb functional evidence PASS"
echo "Canonical screenshots captured under artifacts/qa/screenshots/."
echo "Visual acceptance remains a separate human/vision review gate."

#!/usr/bin/env bash
# End-to-end visual/integration acceptance for the SLOPOS-owned desktop UX.
# This intentionally runs under one D-Bus session so real GtkApplication
# GMenu exports can be consumed by the SLOPOS top bar.
set -euo pipefail

if [[ "${SLOPOS_UI_QA_INNER:-0}" != "1" ]]; then
  exec dbus-run-session -- env SLOPOS_UI_QA_INNER=1 bash "$0" "$@"
fi

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/artifacts/qa/ui-ux}"
TMP="$(mktemp -d)"
HOME_DIR="$TMP/home"
mkdir -p "$OUT" "$HOME_DIR"

export HOME="$HOME_DIR"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_CACHE_HOME="$HOME/.cache"
export XDG_CURRENT_DESKTOP=SLOPOS
export XDG_SESSION_DESKTOP=slopos
export XDG_SESSION_TYPE=x11
export SLOPOS_SESSION_MANAGED=1
export SLOPOS_QA_NO_WELCOME=1
export SLOPOS_SHARE_DIR="$ROOT"
export SLOPOS_DESKTOP_PROFILE=slopos
export SLOPOS_SESSION_BIN="$ROOT/target/release/slopos-session"
export PATH="$ROOT/scripts:$ROOT/target/release:$PATH"
export DISPLAY="${DISPLAY:-:94}"

XVFB_PID=""
SESSION_PID=""
APP_PIDS=()
cleanup() {
  set +e
  for pid in "${APP_PIDS[@]:-}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  if command -v pcmanfm >/dev/null 2>&1; then
    pcmanfm --profile="$SLOPOS_DESKTOP_PROFILE" --desktop-off >/dev/null 2>&1 || true
  fi
  [[ -n "$SESSION_PID" ]] && kill "$SESSION_PID" >/dev/null 2>&1 || true
  [[ -n "$XVFB_PID" ]] && kill "$XVFB_PID" >/dev/null 2>&1 || true
  pkill -TERM -x pcmanfm >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT

wait_for() {
  local description="$1"
  shift
  for _ in $(seq 1 100); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  echo "Timed out waiting for $description" >&2
  return 1
}

window_for_class() {
  xdotool search --onlyvisible --class "$1" 2>/dev/null | tail -n 1
}

window_for_name() {
  xdotool search --onlyvisible --name "$1" 2>/dev/null | tail -n 1
}

capture() {
  local name="$1"
  local width height
  read -r width height < <(xdotool getdisplaygeometry)
  xdotool mousemove "$((width - 20))" "$((height - 20))" >/dev/null 2>&1 || true
  sleep 0.25
  scrot "$OUT/$name.png"
  test -s "$OUT/$name.png"
}

mkdir -p "$XDG_CONFIG_HOME/gtk-3.0" "$XDG_DATA_HOME/icons" "$XDG_DATA_HOME/file-manager/actions" "$XDG_DATA_HOME/applications" "$HOME/.themes"
cp "$ROOT/assets/config/gtk-3.0/settings.ini" "$XDG_CONFIG_HOME/gtk-3.0/settings.ini"
cp -a "$ROOT/themes/platinum/icon-theme" "$XDG_DATA_HOME/icons/SLOPOS-Platinum"
cp -a "$ROOT/assets/file-manager/actions/"* "$XDG_DATA_HOME/file-manager/actions/" 2>/dev/null || true
cp -a "$ROOT/assets/applications/"* "$XDG_DATA_HOME/applications/" 2>/dev/null || true
mkdir -p "$HOME/.themes/slopos-gtk/gtk-3.0" "$HOME/.themes/slopos-gtk-classic/gtk-3.0" "$HOME/.themes/slopos-gtk-graphite/gtk-3.0" "$HOME/.themes/slopos-gtk-oled/gtk-3.0"
cp "$ROOT/assets/config/gtk-3.0/gtk.css" "$HOME/.themes/slopos-gtk/gtk-3.0/gtk.css"
cp "$ROOT/assets/config/gtk-3.0/gtk-classic.css" "$HOME/.themes/slopos-gtk-classic/gtk-3.0/gtk.css"
cp "$ROOT/assets/config/gtk-3.0/gtk-graphite.css" "$HOME/.themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
cp "$ROOT/assets/config/gtk-3.0/gtk-oled.css" "$HOME/.themes/slopos-gtk-oled/gtk-3.0/gtk.css"
mkdir -p "$HOME/.themes/slopos-openbox/openbox-3" "$HOME/.themes/slopos-openbox-classic/openbox-3" "$HOME/.themes/slopos-openbox-graphite/openbox-3" "$HOME/.themes/slopos-openbox-oled/openbox-3"
cp -a "$ROOT/themes/slopos-openbox/openbox-3/." "$HOME/.themes/slopos-openbox/openbox-3/"
cp -a "$ROOT/themes/slopos-openbox-classic/openbox-3/." "$HOME/.themes/slopos-openbox-classic/openbox-3/"
cp -a "$ROOT/themes/slopos-openbox-graphite/openbox-3/." "$HOME/.themes/slopos-openbox-graphite/openbox-3/"
cp -a "$ROOT/themes/slopos-openbox-oled/openbox-3/." "$HOME/.themes/slopos-openbox-oled/openbox-3/"

required_icons=(
  folder user-home user-desktop text-x-generic drive-harddisk user-trash
  go-previous go-next go-up go-home view-refresh edit-find
)
for icon in "${required_icons[@]}"; do
  find "$XDG_DATA_HOME/icons/SLOPOS-Platinum" -type f -name "$icon.svg" -print -quit | grep -q . || {
    echo "Missing required SLOPOS icon: $icon" >&2
    exit 1
  }
done

for command in Xvfb openbox pcmanfm xdotool scrot wmctrl; do
  command -v "$command" >/dev/null || {
    echo "Required UI/UX QA command is missing: $command" >&2
    exit 1
  }
done

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
XVFB_PID=$!
wait_for Xvfb xdpyinfo -display "$DISPLAY"
xsetroot -solid '#2B7798'

# Prove the release UI does not depend on generic Adwaita for its core file
# manager vocabulary, and prove GTK is told the shell owns GtkApplication
# menubars. This must execute after Xvfb exists so GtkSettings has a screen.
python3 - <<'PY'
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
settings = Gtk.Settings.get_default()
assert settings is not None
assert settings.get_property("gtk-icon-theme-name") == "SLOPOS-Platinum"
assert bool(settings.get_property("gtk-shell-shows-menubar")) is True
assert bool(settings.get_property("gtk-shell-shows-app-menu")) is False
PY

cd "$ROOT"
"$ROOT/scripts/start-slopos-i" >"$OUT/session.log" 2>&1 &
SESSION_PID=$!
wait_for "SLOPOS top bar" bash -c 'wmctrl -l | grep -q "SLOPOS Top Bar"'
wait_for "classic desktop manager" pgrep -x pcmanfm
if wmctrl -l | grep -q 'SLOPOS Application Strip'; then
  echo "Retired Application Strip returned in dockless parity QA" >&2
  exit 1
fi
for desktop_object in \
  "$HOME/Desktop/slopos-home.desktop" \
  "$HOME/Desktop/slopos-network.desktop" \
  "$HOME/Desktop/slopos-documents.desktop" \
  "$HOME/Desktop/slopos-trash.desktop"; do
  test -f "$desktop_object"
done
capture 01-platinum-desktop

# Real upstream file manager, but under the SLOPOS profile and icon vocabulary.
# This is the closest current stand-in for the classic Finder/System Folder
# surface; the evidence makes regressions in this layer visible while we keep
# tightening it rather than pretending it is already a custom application.
pcmanfm --profile="$SLOPOS_DESKTOP_PROFILE" "$ROOT" >"$OUT/pcmanfm.log" 2>&1 &
APP_PIDS+=("$!")
wait_for PCManFM bash -c 'xdotool search --onlyvisible --class pcmanfm >/dev/null 2>&1'
PCMANFM_WIN="$(window_for_class pcmanfm)"
xdotool windowactivate --sync "$PCMANFM_WIN"
sleep 0.8
capture 02-pcmanfm-slopos-icons

# Real upstream GtkApplication global menu. Mousepad and the shell deliberately
# share this D-Bus session; the shell must discover the X11 exporter and import
# its GMenu model instead of showing the old local-menu placeholder.
printf 'SLOPOS global menu QA\n' >"$TMP/global-menu.txt"
mousepad "$TMP/global-menu.txt" >"$OUT/mousepad.log" 2>&1 &
MOUSEPAD_PID=$!
APP_PIDS+=("$MOUSEPAD_PID")
wait_for Mousepad bash -c 'xdotool search --onlyvisible --class mousepad >/dev/null 2>&1'
MOUSEPAD_WIN="$(window_for_class mousepad)"
xdotool windowactivate --sync "$MOUSEPAD_WIN"
sleep 1
xprop -id "$MOUSEPAD_WIN" \
  _GTK_UNIQUE_BUS_NAME \
  _GTK_MENUBAR_OBJECT_PATH \
  _GTK_APPLICATION_OBJECT_PATH \
  _GTK_WINDOW_OBJECT_PATH >"$OUT/gmenu-xprop.txt"
grep -Eq '^_GTK_UNIQUE_BUS_NAME.*= ":' "$OUT/gmenu-xprop.txt"
grep -Eq '^_GTK_MENUBAR_OBJECT_PATH.*= "/' "$OUT/gmenu-xprop.txt"
grep -Eq '^_GTK_APPLICATION_OBJECT_PATH.*= "/' "$OUT/gmenu-xprop.txt"
wait_for "SLOPOS GTK GMenu import" grep -q 'Imported GTK global menubar' "$OUT/session.log"
if grep -q 'App (local)' "$OUT/session.log"; then
  echo "Legacy App (local) placeholder leaked into global-menu QA" >&2
  exit 1
fi
capture 03-real-gtk-global-menu

# Shipping Settings delegates: these must exist in the release environment;
# the all-disabled screenshot is retained only by separate missing-tool tests.
for command in arandr pavucontrol nm-connection-editor blueman-manager xfce4-power-manager-settings xfce4-display-settings xfce4-mouse-settings; do
  command -v "$command" >/dev/null || {
    echo "Required Settings delegate is missing in QA image: $command" >&2
    exit 1
  }
done
./target/release/slopos-settings >"$OUT/settings.log" 2>&1 &
SETTINGS_PID=$!
APP_PIDS+=("$SETTINGS_PID")
wait_for Settings bash -c 'xdotool search --onlyvisible --name "System Settings" >/dev/null 2>&1'
SETTINGS_WIN="$(window_for_name 'System Settings')"
xdotool windowactivate --sync "$SETTINGS_WIN"
sleep 0.5
capture 04-settings-available
kill "$SETTINGS_PID" >/dev/null 2>&1 || true

# First-class dark mode: persistence, shell/Openbox reload and a real SLOPOS
# control panel must render in Graphite during the same session.
OLD_SHELL_PID="$(pgrep -n -x slopos-shell || true)"
slopos-appearance graphite >"$OUT/appearance.log" 2>&1
test "$(slopos-appearance status)" = graphite
if [[ -n "$OLD_SHELL_PID" ]]; then
  wait_for "Graphite shell restart" bash -c "test \"\$(pgrep -n -x slopos-shell || true)\" != '$OLD_SHELL_PID'"
else
  wait_for "Graphite shell" pgrep -x slopos-shell
fi
wait_for "Graphite top bar" bash -c 'wmctrl -l | grep -q "SLOPOS Top Bar"'
sleep 0.8
capture 05-graphite-desktop

./target/release/slopos-settings >>"$OUT/settings.log" 2>&1 &
GRAPHITE_SETTINGS_PID=$!
APP_PIDS+=("$GRAPHITE_SETTINGS_PID")
wait_for "Graphite Settings" bash -c 'xdotool search --onlyvisible --name "System Settings" >/dev/null 2>&1'
GRAPHITE_SETTINGS_WIN="$(window_for_name 'System Settings')"
xdotool windowactivate --sync "$GRAPHITE_SETTINGS_WIN"
sleep 0.5
capture 06-graphite-settings

# Leave the isolated profile in the default release appearance and prove the
# switch is reversible/persistent.
kill "$GRAPHITE_SETTINGS_PID" >/dev/null 2>&1 || true
slopos-appearance platinum >>"$OUT/appearance.log" 2>&1
test "$(slopos-appearance status)" = platinum

printf '%s\n' "${GITHUB_SHA:-$(git rev-parse HEAD)}" >"$OUT/source-sha.txt"
printf 'desktop_manager=pcmanfm\n' >"$OUT/composition.txt"
printf 'dock=absent\n' >>"$OUT/composition.txt"
printf 'managed_desktop_objects=4\n' >>"$OUT/composition.txt"
printf 'UI/UX QA PASS\n' >"$OUT/status.txt"

echo "SLOPOS UI/UX QA passed; evidence: $OUT"
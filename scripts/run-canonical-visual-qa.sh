#!/usr/bin/env bash
# SLOPOS-I Canonical Visual QA Capture Script.
# Generates all 16 canonical scenes required by AGENTS.md Section 13 for independent vision review:
#  1. Clean Desktop (Platinum) [1280x800]
#  2. System Menu Open [1280x800]
#  3. Search Palette Open [1280x800]
#  4. Notification [1280x800]
#  5. Modal Dialog (About SLOPOS-I) [1280x800]
#  6. Active Application Window (Mousepad) [1280x800]
#  7. Multi-window / Overlapping Focus [1280x800]
#  8. File Manager (PCManFM + SLOPOS-Platinum Icons) [1280x800]
#  9. Terminal (Xfce4 Terminal) [1280x800]
# 10. Software Catalogue [1280x800]
# 11. System Settings Control Panels [1280x800]
# 12. Graphite Dark Desktop [1280x800]
# 13. Graphite Settings Presentation [1280x800]
# 14. Ultrawide Layout [3440x1440]
# 15. HiDPI [2560x1600 Scale=2]
# 16. Multi-window Workspace State [1920x1080]
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="$REPO_ROOT/artifacts/qa/canonical-visual"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.png "$OUT_DIR"/manifest.json "$OUT_DIR"/evidence-manifest.txt

export DEBIAN_FRONTEND=noninteractive
export GDK_BACKEND=x11
export SLOPOS_QA_NO_WELCOME=1
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$REPO_ROOT/assets/config/openbox/rc.xml}"

SOURCE_COMMIT="${SOURCE_SHA:-$(git -C "$REPO_ROOT" rev-parse --verify HEAD 2>/dev/null || printf '%s' unknown)}"
STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Set up user theme and icon assets
mkdir -p "$HOME/.themes/slopos-openbox/openbox-3" "$HOME/.themes/slopos-openbox-graphite/openbox-3"
cp "$REPO_ROOT/themes/slopos-openbox/openbox-3/themerc" "$HOME/.themes/slopos-openbox/openbox-3/themerc"
cp "$REPO_ROOT/themes/slopos-openbox-graphite/openbox-3/themerc" "$HOME/.themes/slopos-openbox-graphite/openbox-3/themerc"

mkdir -p "$HOME/.themes/slopos-gtk/gtk-3.0" "$HOME/.themes/slopos-gtk-graphite/gtk-3.0" "$HOME/.config/gtk-3.0"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk.css" "$HOME/.themes/slopos-gtk/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk-graphite.css" "$HOME/.themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk.css" "$HOME/.config/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/settings.ini" "$HOME/.config/gtk-3.0/settings.ini"

mkdir -p "$HOME/.local/share/icons"
cp -a "$REPO_ROOT/themes/platinum/icon-theme" "$HOME/.local/share/icons/SLOPOS-Platinum"

capture_screen() {
  local filename="$1"
  local width height
  read -r width height < <(xdotool getdisplaygeometry)
  xdotool mousemove "$((width - 10))" "$((height - 10))"
  sleep 0.3
  scrot -zo "$OUT_DIR/$filename"
  test -s "$OUT_DIR/$filename"
  echo "Captured: $filename ($(identify -format '%wx%h' "$OUT_DIR/$filename"))"
}

# --- SECTION 1: Standard 1280x800 Platinum & Graphite Scenes ---
DISPLAY=:90
export DISPLAY
DBUS_ENV_90="/tmp/vis-dbus-90.env"
rm -f "$DBUS_ENV_90"

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/vis-xvfb-90.log 2>&1 &
XVFB_90_PID=$!
sleep 1
xsetroot -solid "#758090"

dbus-run-session -- bash -c '
  printf "export DBUS_SESSION_BUS_ADDRESS=%q\n" "$DBUS_SESSION_BUS_ADDRESS" > "$1"
  exec "$2"
' bash "$DBUS_ENV_90" ./target/release/slopos-session >/tmp/vis-session-90.log 2>&1 &
SESSION_90_PID=$!

for _ in $(seq 1 40); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null && [[ -s "$DBUS_ENV_90" ]]; then break; fi
  sleep 0.25
done
source "$DBUS_ENV_90"

for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1 && \
     xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done

TOPBAR_WIN="$(xdotool search --onlyvisible --name '^SLOPOS Top Bar$' | tail -n 1)"

# 1. Clean Desktop (Platinum)
capture_screen "01_clean_desktop_platinum_1280x800.png"

# 2. System Menu Open
xdotool windowactivate --sync "$TOPBAR_WIN"
xdotool mousemove --window "$TOPBAR_WIN" --sync 14 13
xdotool click 1
sleep 0.5
scrot -zo "$OUT_DIR/02_system_menu_open_1280x800.png"
echo "Captured: 02_system_menu_open_1280x800.png"

# 5. Modal Dialog (About SLOPOS-I)
xdotool key Down Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^About SLOPOS-I$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
capture_screen "05_modal_about_dialog_1280x800.png"
xdotool key Return
sleep 0.3

# 17. Modal Dialog (Shut Down)
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Shut Down$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
capture_screen "17_modal_shutdown_dialog_1280x800.png"
xdotool key Escape
sleep 0.3

# 18. Modal Dialog (Switch User)
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Down Down Down Down Down Down Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Switch User$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
capture_screen "18_modal_switch_user_dialog_1280x800.png"
xdotool key Escape
sleep 0.3

# 3. Search Palette Open
pkill -USR1 -x slopos-shell
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
xdotool type "Terminal"
sleep 0.3
capture_screen "03_search_palette_open_1280x800.png"
xdotool key Escape
sleep 0.3

# 4. Notification
notify-send -t 60000 -a "SLOPOS QA" "SLOPOS Platinum Environment" "System ready. Welcome to SLOPOS Platinum classic desktop." || true
sleep 0.5
capture_screen "04_notification_1280x800.png"
for notif in $(xdotool search --onlyvisible --name '^SLOPOS Notification' 2>/dev/null || true); do
  xdotool windowclose "$notif"
done
sleep 0.3

# 6. Active App (Mousepad)
mousepad "$REPO_ROOT/README.md" >/dev/null 2>&1 &
MOUSEPAD_PID=$!
sleep 1
MOUSEPAD_WIN="$(xdotool search --onlyvisible --class mousepad | tail -n 1)"
xdotool windowactivate --sync "$MOUSEPAD_WIN"
capture_screen "06_active_app_mousepad_1280x800.png"

# 7. Multi-window / Overlapping Focus
xfce4-terminal >/dev/null 2>&1 &
TERM_PID=$!
sleep 1
TERM_WIN="$(xdotool search --onlyvisible --class xfce4-terminal | tail -n 1)"
xdotool windowmove --sync "$TERM_WIN" 450 220
xdotool windowsize --sync "$TERM_WIN" 620 380
xdotool windowactivate --sync "$TERM_WIN"
sleep 0.5
capture_screen "07_multi_window_focus_1280x800.png"
kill "$TERM_PID" "$MOUSEPAD_PID" 2>/dev/null || true
sleep 0.5

# 8. File Manager (PCManFM)
pcmanfm "$REPO_ROOT" >/dev/null 2>&1 &
PCMAN_PID=$!
sleep 1
PCMAN_WIN="$(xdotool search --onlyvisible --class pcmanfm | tail -n 1)"
xdotool windowactivate --sync "$PCMAN_WIN"
capture_screen "08_file_manager_pcmanfm_1280x800.png"
kill "$PCMAN_PID" 2>/dev/null || true
sleep 0.5

# 9. Terminal (Xfce4 Terminal)
xfce4-terminal >/dev/null 2>&1 &
TERM_PID=$!
sleep 1
TERM_WIN="$(xdotool search --onlyvisible --class xfce4-terminal | tail -n 1)"
xdotool windowactivate --sync "$TERM_WIN"
capture_screen "09_terminal_xfce4_1280x800.png"
kill "$TERM_PID" 2>/dev/null || true
sleep 0.5

# 10. Software Catalogue
./target/release/slopos-catalogue >/dev/null 2>&1 &
CAT_PID=$!
sleep 1
CAT_WIN="$(xdotool search --onlyvisible --name '^Software Catalogue$' | tail -n 1)"
xdotool windowactivate --sync "$CAT_WIN"
capture_screen "10_software_catalogue_1280x800.png"
kill "$CAT_PID" 2>/dev/null || true
sleep 0.5

# 11. System Settings Control Panels
./target/release/slopos-settings >/dev/null 2>&1 &
SET_PID=$!
sleep 1
SET_WIN="$(xdotool search --onlyvisible --name '^System Settings$' | tail -n 1)"
xdotool windowactivate --sync "$SET_WIN"
capture_screen "11_system_settings_control_panels_1280x800.png"
kill "$SET_PID" 2>/dev/null || true
sleep 0.5

# 12. Graphite Dark Desktop & 13. Graphite Settings Presentation
bash scripts/slopos-appearance graphite >/dev/null 2>&1 || true
sleep 1
capture_screen "12_graphite_dark_desktop_1280x800.png"

./target/release/slopos-settings >/dev/null 2>&1 &
SET_PID=$!
sleep 1
SET_WIN="$(xdotool search --onlyvisible --name '^System Settings$' | tail -n 1)"
xdotool windowactivate --sync "$SET_WIN"
capture_screen "13_graphite_settings_1280x800.png"
kill "$SET_PID" 2>/dev/null || true

# Reset appearance
bash scripts/slopos-appearance platinum >/dev/null 2>&1 || true

kill -TERM "$SESSION_90_PID" "$XVFB_90_PID" 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
pkill -TERM -x openbox 2>/dev/null || true
sleep 1

# --- SECTION 2: Ultrawide Layout (3440x1440) ---
DISPLAY=:96
export DISPLAY
Xvfb "$DISPLAY" -screen 0 3440x1440x24 -nolisten tcp >/tmp/vis-xvfb-96.log 2>&1 &
XVFB_96_PID=$!
sleep 1
xsetroot -solid "#758090"
dbus-run-session -- ./target/release/slopos-session >/tmp/vis-session-96.log 2>&1 &
SESSION_96_PID=$!
sleep 2
capture_screen "14_ultrawide_desktop_3440x1440.png"
kill -TERM "$SESSION_96_PID" "$XVFB_96_PID" 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
pkill -TERM -x openbox 2>/dev/null || true
sleep 1

# --- SECTION 3: HiDPI (2560x1600 Scale=2) ---
DISPLAY=:97
export DISPLAY
export GDK_SCALE=2
Xvfb "$DISPLAY" -screen 0 2560x1600x24 -nolisten tcp >/tmp/vis-xvfb-97.log 2>&1 &
XVFB_97_PID=$!
sleep 1
xsetroot -solid "#758090"
dbus-run-session -- ./target/release/slopos-session >/tmp/vis-session-97.log 2>&1 &
SESSION_97_PID=$!
sleep 2
capture_screen "15_hidpi_scale2_2560x1600.png"
kill -TERM "$SESSION_97_PID" "$XVFB_97_PID" 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
pkill -TERM -x openbox 2>/dev/null || true
unset GDK_SCALE
sleep 1

# --- SECTION 4: Multi-window Workspace State (1920x1080) ---
DISPLAY=:98
export DISPLAY
Xvfb "$DISPLAY" -screen 0 1920x1080x24 -nolisten tcp >/tmp/vis-xvfb-98.log 2>&1 &
XVFB_98_PID=$!
sleep 1
xsetroot -solid "#758090"
dbus-run-session -- ./target/release/slopos-session >/tmp/vis-session-98.log 2>&1 &
SESSION_98_PID=$!
sleep 2

pcmanfm "$REPO_ROOT" >/dev/null 2>&1 &
PCMAN_PID=$!
xfce4-terminal >/dev/null 2>&1 &
TERM_PID=$!
./target/release/slopos-catalogue >/dev/null 2>&1 &
CAT_PID=$!
sleep 1.5

TERM_WIN="$(xdotool search --onlyvisible --class xfce4-terminal | tail -n 1)"
xdotool windowactivate --sync "$TERM_WIN"
sleep 0.5
capture_screen "16_workspace_multi_window_1920x1080.png"

kill "$PCMAN_PID" "$TERM_PID" "$CAT_PID" 2>/dev/null || true
kill -TERM "$SESSION_98_PID" "$XVFB_98_PID" 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
pkill -TERM -x openbox 2>/dev/null || true

# --- SECTION 5: Verify Manifest & Output ---
echo "=== Manifest generation ==="
{
  printf 'source_commit=%s\n' "$SOURCE_COMMIT"
  printf 'started_utc=%s\n' "$STARTED_UTC"
  printf 'completed_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  for img in "$OUT_DIR"/*.png; do
    name="${img##*/}"
    sha="$(sha256sum "$img" | awk '{print $1}')"
    dim="$(identify -format '%wx%h' "$img")"
    printf 'screenshot=%s sha256=%s dimensions=%s\n' "$name" "$sha" "$dim"
  done
} > "$OUT_DIR/evidence-manifest.txt"

count="$(find "$OUT_DIR" -name "*.png" | wc -l)"
echo "Total canonical scenes captured: $count"
test "$count" -ge 16 || { echo "Expected at least 16 canonical scenes, got $count" >&2; exit 1; }

echo "CANONICAL_VISUAL_QA_STATUS_0"
echo "SLOPOS-I Canonical Visual Scenes Capture: PASS"

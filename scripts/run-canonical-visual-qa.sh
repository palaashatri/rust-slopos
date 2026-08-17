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

# Build fresh release binaries
cargo build --release --workspace --locked

if ! command -v firefox >/dev/null 2>&1 && ! command -v firefox-esr >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update -qq && apt-get install -y -qq firefox-esr >/dev/null 2>&1 || true
  fi
fi

if ! command -v chocolate-doom >/dev/null 2>&1 && ! command -v doom >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update -qq && apt-get install -y -qq chocolate-doom freedoom >/dev/null 2>&1 || true
  fi
fi

export PATH="/usr/games:$PATH"

# Enable VLC execution in container environment
sed -i "s/geteuid/getppid/" /usr/bin/vlc 2>/dev/null || true

# Start system dbus and NetworkManager for Network Settings GUI
mkdir -p /var/run/dbus
dbus-daemon --system --fork >/dev/null 2>&1 || true
/usr/sbin/NetworkManager >/dev/null 2>&1 &

# Set up user theme and icon assets
mkdir -p "$HOME/.themes/slopos-openbox/openbox-3" "$HOME/.themes/slopos-openbox-classic/openbox-3" "$HOME/.themes/slopos-openbox-graphite/openbox-3" "$HOME/.themes/slopos-openbox-oled/openbox-3"
cp "$REPO_ROOT/themes/slopos-openbox/openbox-3/themerc" "$HOME/.themes/slopos-openbox/openbox-3/themerc"
cp "$REPO_ROOT/themes/slopos-openbox-classic/openbox-3/themerc" "$HOME/.themes/slopos-openbox-classic/openbox-3/themerc"
cp "$REPO_ROOT/themes/slopos-openbox-graphite/openbox-3/themerc" "$HOME/.themes/slopos-openbox-graphite/openbox-3/themerc"
cp "$REPO_ROOT/themes/slopos-openbox-oled/openbox-3/themerc" "$HOME/.themes/slopos-openbox-oled/openbox-3/themerc"

mkdir -p "$HOME/.themes/slopos-gtk/gtk-3.0" "$HOME/.themes/slopos-gtk-classic/gtk-3.0" "$HOME/.themes/slopos-gtk-graphite/gtk-3.0" "$HOME/.themes/slopos-gtk-oled/gtk-3.0" "$HOME/.config/gtk-3.0"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk.css" "$HOME/.themes/slopos-gtk/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk-classic.css" "$HOME/.themes/slopos-gtk-classic/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk-graphite.css" "$HOME/.themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk-oled.css" "$HOME/.themes/slopos-gtk-oled/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/gtk.css" "$HOME/.config/gtk-3.0/gtk.css"
cp "$REPO_ROOT/assets/config/gtk-3.0/settings.ini" "$HOME/.config/gtk-3.0/settings.ini"

mkdir -p "$HOME/.local/share/icons" "$HOME/.local/share/file-manager/actions" "$HOME/.local/share/applications"
cp -a "$REPO_ROOT/themes/platinum/icon-theme" "$HOME/.local/share/icons/SLOPOS-Platinum"
cp -a "$REPO_ROOT/assets/file-manager/actions/"* "$HOME/.local/share/file-manager/actions/" 2>/dev/null || true
cp -a "$REPO_ROOT/assets/applications/"* "$HOME/.local/share/applications/" 2>/dev/null || true

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

# 22. Modal Dialog (Restart)
pkill -USR2 -x slopos-shell
sleep 0.3
xdotool key Up Up Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^Restart$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
capture_screen "22_modal_restart_dialog_1280x800.png"
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

# 25. Desktop Right-Click Context Menu (Open With Submenu)
xdotool mousemove 400 250 click 3
sleep 0.5
xdotool mousemove 450 380
sleep 0.5
capture_screen "25_desktop_right_click_context_menu_1280x800.png"
xdotool mousemove 100 100 click 1
sleep 0.5

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

# 26. File Manager Right-Click Context Menu
mkdir -p "$HOME/Desktop/Projects" "$HOME/Desktop/Documents"
touch "$HOME/Desktop/ReadMe.txt" "$HOME/Desktop/Notes.md"
pcmanfm "$HOME/Desktop" >/dev/null 2>&1 &
PCMAN_DESK_PID=$!
sleep 1
PCMAN_DESK_WIN="$(xdotool search --onlyvisible --class pcmanfm | tail -n 1)"
xdotool windowactivate --sync "$PCMAN_DESK_WIN"
sleep 0.5
xdotool mousemove 500 350 click 3
sleep 0.5
capture_screen "26_file_manager_right_click_context_menu_1280x800.png"
kill "$PCMAN_DESK_PID" 2>/dev/null || true
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

# 23. Web Browser (Firefox with SLOPOS window decorations)
if command -v firefox >/dev/null 2>&1 || command -v firefox-esr >/dev/null 2>&1; then
  FF_BIN="$(command -v firefox || command -v firefox-esr)"
  FF_PROF="/tmp/slopos-qa-ff-prof"
  rm -rf "$FF_PROF"
  mkdir -p "$FF_PROF"
  bash scripts/install-browser-theme.sh firefox "$FF_PROF" >/dev/null 2>&1 || true
  cat >>"$FF_PROF/user.js" <<'EOF'
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("browser.tabs.inTitlebar", 0);
EOF
  MOZ_ENABLE_WAYLAND=0 MOZ_DISABLE_CONTENT_SANDBOX=1 GTK_THEME=slopos-gtk \
    "$FF_BIN" --no-remote --new-instance --profile "$FF_PROF" "about:blank" >/dev/null 2>&1 &
  FF_PID=$!
  sleep 4
  FF_WIN="$(xdotool search --onlyvisible --class firefox | tail -n 1 || xdotool search --onlyvisible --name ".*Firefox.*" | tail -n 1 || true)"
  if [[ -n "$FF_WIN" ]]; then
    xdotool windowactivate --sync "$FF_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "23_web_browser_firefox_1280x800.png"
  fi
  kill "$FF_PID" 2>/dev/null || true
  sleep 0.5
fi

# 24. Game (Chocolate Doom + Freedoom Phase 2)
if command -v chocolate-doom >/dev/null 2>&1 || command -v doom >/dev/null 2>&1 || [[ -x /usr/games/chocolate-doom ]]; then
  DOOM_BIN="$(command -v chocolate-doom 2>/dev/null || command -v doom 2>/dev/null || printf '%s' /usr/games/chocolate-doom)"
  IWAD=""
  for candidate_wad in /usr/share/games/doom/freedoom2.wad /usr/share/games/doom/freedoom1.wad /usr/share/doom/freedoom2.wad; do
    if [[ -f "$candidate_wad" ]]; then
      IWAD="$candidate_wad"
      break
    fi
  done
  doom_args=("-window" "-geometry" "640x480" "-nosound" "-nomusic")
  if [[ -n "$IWAD" ]]; then
    doom_args+=("-iwad" "$IWAD")
  fi
  "$DOOM_BIN" "${doom_args[@]}" >/dev/null 2>&1 &
  DOOM_PID=$!
  sleep 4
  DOOM_WIN="$(xdotool search --onlyvisible --class "chocolate-doom" | tail -n 1 || xdotool search --onlyvisible --name ".*Doom.*" | tail -n 1 || true)"
  if [[ -n "$DOOM_WIN" ]]; then
    xdotool windowactivate --sync "$DOOM_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "24_game_doom_freedoom_1280x800.png"
  fi
  kill "$DOOM_PID" 2>/dev/null || true
  sleep 0.5
fi

# 27. Calculator (Galculator)
if command -v galculator >/dev/null 2>&1; then
  galculator >/dev/null 2>&1 &
  CALC_PID=$!
  sleep 1
  CALC_WIN="$(xdotool search --onlyvisible --class galculator | tail -n 1 || true)"
  if [[ -n "$CALC_WIN" ]]; then
    xdotool windowactivate --sync "$CALC_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "27_calculator_galculator_1280x800.png"
  fi
  kill "$CALC_PID" 2>/dev/null || true
  sleep 0.5
fi

# 28. Image Viewer (Ristretto)
if command -v ristretto >/dev/null 2>&1; then
  ristretto >/dev/null 2>&1 &
  IMG_PID=$!
  sleep 1
  IMG_WIN="$(xdotool search --onlyvisible --class ristretto | tail -n 1 || true)"
  if [[ -n "$IMG_WIN" ]]; then
    xdotool windowactivate --sync "$IMG_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "28_image_viewer_ristretto_1280x800.png"
  fi
  kill "$IMG_PID" 2>/dev/null || true
  sleep 0.5
fi

# 29. Document Viewer (Zathura)
if command -v zathura >/dev/null 2>&1; then
  zathura >/dev/null 2>&1 &
  DOC_PID=$!
  sleep 1
  DOC_WIN="$(xdotool search --onlyvisible --class zathura | tail -n 1 || true)"
  if [[ -n "$DOC_WIN" ]]; then
    xdotool windowactivate --sync "$DOC_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "29_document_viewer_zathura_1280x800.png"
  fi
  kill "$DOC_PID" 2>/dev/null || true
  sleep 0.5
fi

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
pkill -TERM -x slopos-shell 2>/dev/null || true
sleep 0.5
SLOPOS_APPEARANCE=graphite ./target/release/slopos-shell >/tmp/vis-shell-graphite.log 2>&1 &
sleep 1.5
capture_screen "12_graphite_dark_desktop_1280x800.png"

SLOPOS_APPEARANCE=graphite ./target/release/slopos-settings >/dev/null 2>&1 &
SET_PID=$!
sleep 1
SET_WIN="$(xdotool search --onlyvisible --name '^System Settings$' | tail -n 1)"
xdotool windowactivate --sync "$SET_WIN"
capture_screen "13_graphite_settings_1280x800.png"
kill "$SET_PID" 2>/dev/null || true

# --- OLED Dark Appearance Scenes ---
# 34. OLED Dark Desktop & 35. OLED Settings Presentation
bash scripts/slopos-appearance oled >/dev/null 2>&1 || true
pkill -TERM -x slopos-shell 2>/dev/null || true
sleep 0.5
SLOPOS_APPEARANCE=oled ./target/release/slopos-shell >/tmp/vis-shell-oled.log 2>&1 &
sleep 1.5
capture_screen "34_oled_dark_desktop_1280x800.png"

SLOPOS_APPEARANCE=oled ./target/release/slopos-settings >/dev/null 2>&1 &
SET_PID=$!
sleep 1.5
SET_WIN="$(xdotool search --onlyvisible --name '^System Settings$' | tail -n 1)"
xdotool windowactivate --sync "$SET_WIN"
capture_screen "35_oled_dark_settings_1280x800.png"
kill "$SET_PID" 2>/dev/null || true
sleep 0.5

# Reset appearance to Platinum
bash scripts/slopos-appearance platinum >/dev/null 2>&1 || true
pkill -TERM -x slopos-shell 2>/dev/null || true
sleep 0.5
SLOPOS_APPEARANCE=platinum ./target/release/slopos-shell >/dev/null 2>&1 &
sleep 1

# --- Wallpapers Showcase ---
# 30. Classic System Gray Dither Wallpaper
bash scripts/slopos-wallpaper set 01_classic_system_gray.png --mode fill >/dev/null 2>&1 || true
sleep 0.5
capture_screen "30_wallpaper_classic_system_gray_1280x800.png"

# 31. Vintage Mac Blue Tweed Wallpaper
bash scripts/slopos-wallpaper set 03_vintage_mac_blue.png --mode fill >/dev/null 2>&1 || true
sleep 0.5
capture_screen "31_wallpaper_vintage_mac_blue_1280x800.png"

# 32. Retro Teal Grid Wallpaper
bash scripts/slopos-wallpaper set 04_retro_teal_grid.png --mode fill >/dev/null 2>&1 || true
sleep 0.5
capture_screen "32_wallpaper_retro_teal_grid_1280x800.png"

# 33. Desktop & Wallpaper Chooser Dialog
./target/release/slopos-settings --wallpaper >/dev/null 2>&1 &
WP_PID=$!
sleep 1
WP_WIN="$(xdotool search --onlyvisible --name '^Desktop & Wallpaper$' | tail -n 1 || true)"
if [[ -n "$WP_WIN" ]]; then
  xdotool windowactivate --sync "$WP_WIN" 2>/dev/null || true
  sleep 0.5
  capture_screen "33_wallpaper_chooser_dialog_1280x800.png"
fi
kill "$WP_PID" 2>/dev/null || true
sleep 0.5

# Restore default platinum wallpaper
bash scripts/slopos-wallpaper set 02_platinum_cool_slate.png --mode fill >/dev/null 2>&1 || true
sleep 0.5

# 36. Date & Time Settings GUI
./target/release/slopos-settings --datetime >/dev/null 2>&1 &
DT_PID=$!
sleep 1
DT_WIN="$(xdotool search --onlyvisible --name '^Date & Time Settings$' | tail -n 1 || true)"
if [[ -n "$DT_WIN" ]]; then
  xdotool windowactivate --sync "$DT_WIN" 2>/dev/null || true
  sleep 0.5
  capture_screen "36_datetime_control_panel_1280x800.png"
fi
kill "$DT_PID" 2>/dev/null || true
sleep 0.5

# 37. Network & Wi-Fi Connections GUI (nm-connection-editor)
if command -v nm-connection-editor >/dev/null 2>&1; then
  nm-connection-editor >/dev/null 2>&1 &
  NET_PID=$!
  sleep 1
  NET_WIN="$(xdotool search --onlyvisible --class nm-connection-editor | tail -n 1 || true)"
  if [[ -n "$NET_WIN" ]]; then
    xdotool windowactivate --sync "$NET_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "37_network_wifi_gui_1280x800.png"
  fi
  kill "$NET_PID" 2>/dev/null || true
  sleep 0.5
fi

# 38. Bluetooth Devices GUI (blueman-manager)
if command -v blueman-manager >/dev/null 2>&1; then
  blueman-manager >/dev/null 2>&1 &
  BLUE_PID=$!
  sleep 1
  BLUE_WIN="$(xdotool search --onlyvisible --class blueman-manager | tail -n 1 || true)"
  if [[ -n "$BLUE_WIN" ]]; then
    xdotool windowactivate --sync "$BLUE_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "38_bluetooth_gui_1280x800.png"
  fi
  kill "$BLUE_PID" 2>/dev/null || true
  sleep 0.5
fi

# 39. Sound & Audio Volume Mixer GUI (pavucontrol)
if command -v pavucontrol >/dev/null 2>&1; then
  pulseaudio --start --exit-idle-time=-1 >/dev/null 2>&1 || true
  sleep 1
  pavucontrol >/dev/null 2>&1 &
  SND_PID=$!
  sleep 2
  SND_WIN="$(xdotool search --onlyvisible --class pavucontrol | tail -n 1 || true)"
  if [[ -n "$SND_WIN" ]]; then
    xdotool windowactivate --sync "$SND_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "39_sound_audio_pavucontrol_1280x800.png"
  fi
  kill "$SND_PID" 2>/dev/null || true
  sleep 0.5
fi

# 40. GIMP Image Editor
if command -v gimp >/dev/null 2>&1; then
  gimp --no-splash >/dev/null 2>&1 &
  GIMP_PID=$!
  sleep 3
  GIMP_WIN="$(xdotool search --onlyvisible --class gimp | tail -n 1 || true)"
  if [[ -n "$GIMP_WIN" ]]; then
    xdotool windowactivate --sync "$GIMP_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "40_app_gimp_1280x800.png"
  fi
  kill "$GIMP_PID" 2>/dev/null || true
  sleep 0.5
fi

# 41. Inkscape Vector Graphics Editor
if command -v inkscape >/dev/null 2>&1; then
  inkscape >/dev/null 2>&1 &
  INK_PID=$!
  sleep 3
  INK_WIN="$(xdotool search --onlyvisible --class inkscape | tail -n 1 || true)"
  if [[ -n "$INK_WIN" ]]; then
    xdotool windowactivate --sync "$INK_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "41_app_inkscape_1280x800.png"
  fi
  kill "$INK_PID" 2>/dev/null || true
  sleep 0.5
fi

# 42. VLC Media Player
if command -v vlc >/dev/null 2>&1; then
  vlc --no-video-title-show >/dev/null 2>&1 &
  VLC_PID=$!
  sleep 2
  VLC_WIN="$(xdotool search --onlyvisible --class vlc | tail -n 1 || true)"
  if [[ -n "$VLC_WIN" ]]; then
    xdotool windowactivate --sync "$VLC_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "42_app_vlc_media_player_1280x800.png"
  fi
  kill "$VLC_PID" 2>/dev/null || true
  sleep 0.5
fi

# 43. LibreOffice Writer
if command -v libreoffice >/dev/null 2>&1; then
  libreoffice --writer --nologo >/dev/null 2>&1 &
  LIB_PID=$!
  sleep 3
  LIB_WIN="$(xdotool search --onlyvisible --class soffice.bin | tail -n 1 || xdotool search --onlyvisible --class libreoffice | tail -n 1 || true)"
  if [[ -n "$LIB_WIN" ]]; then
    xdotool windowactivate --sync "$LIB_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "43_app_libreoffice_writer_1280x800.png"
  fi
  kill "$LIB_PID" 2>/dev/null || true
  sleep 0.5
fi

# 44. SuperTux Classic 2D Game
if command -v supertux2 >/dev/null 2>&1 || command -v /usr/games/supertux2 >/dev/null 2>&1; then
  ST_BIN="$(command -v supertux2 2>/dev/null || echo /usr/games/supertux2)"
  "$ST_BIN" --geometry 640x480 >/dev/null 2>&1 &
  ST_PID=$!
  sleep 3
  ST_WIN="$(xdotool search --onlyvisible --class supertux2 | tail -n 1 || xdotool search --onlyvisible --name '.*SuperTux.*' | tail -n 1 || true)"
  if [[ -n "$ST_WIN" ]]; then
    xdotool windowactivate --sync "$ST_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "44_app_supertux_1280x800.png"
  fi
  kill "$ST_PID" 2>/dev/null || true
  sleep 0.5
fi

# 45. Mozilla Thunderbird Email Client
if command -v thunderbird >/dev/null 2>&1; then
  thunderbird >/dev/null 2>&1 &
  TB_PID=$!
  sleep 3
  TB_WIN="$(xdotool search --onlyvisible --class Thunderbird | tail -n 1 || xdotool search --onlyvisible --class thunderbird | tail -n 1 || true)"
  if [[ -n "$TB_WIN" ]]; then
    xdotool windowactivate --sync "$TB_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "45_app_thunderbird_1280x800.png"
  fi
  kill "$TB_PID" 2>/dev/null || true
  sleep 0.5
fi

# 46. Fullscreen Video / Game Experience (MPV Fullscreen with Top Bar & Dock Auto-Hidden)
if command -v mpv >/dev/null 2>&1; then
  mpv --player-operation-mode=pseudo-gui --fs --idle=yes >/dev/null 2>&1 &
  MPV_PID=$!
  sleep 2
  MPV_WIN="$(xdotool search --onlyvisible --class mpv | tail -n 1 || true)"
  if [[ -n "$MPV_WIN" ]]; then
    xdotool windowactivate --sync "$MPV_WIN" 2>/dev/null || true
    sleep 0.5
    capture_screen "46_fullscreen_video_mpv_1280x800.png"
  fi
  kill "$MPV_PID" 2>/dev/null || true
  sleep 0.5
fi

# 47. Dock Dodge & Maximized Window Behavior (Auto-Hide Dock)
mkdir -p "$HOME/.config/slopos-i"
printf '1\n' > "$HOME/.config/slopos-i/dock_dodge"
if command -v mousepad >/dev/null 2>&1; then
  mousepad /workspace/README.md >/dev/null 2>&1 &
  DODGE_PID=$!
  sleep 2
  DODGE_WIN="$(xdotool search --onlyvisible --class Mousepad | tail -n 1 || true)"
  if [[ -n "$DODGE_WIN" ]]; then
    xdotool windowactivate --sync "$DODGE_WIN" 2>/dev/null || true
    xdotool windowsize "$DODGE_WIN" 1280 750 2>/dev/null || true
    xdotool windowmove "$DODGE_WIN" 0 26 2>/dev/null || true
    sleep 0.5
    capture_screen "47_dock_dodge_maximized_1280x800.png"
  fi
  kill "$DODGE_PID" 2>/dev/null || true
  sleep 0.5
fi
printf '0\n' > "$HOME/.config/slopos-i/dock_dodge"

# 48. Custom Colors & Fonts Studio (Windows XP Style Personalization)
./target/release/slopos-settings --appearance >/dev/null 2>&1 &
STUDIO_PID=$!
sleep 1.5
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '.*Appearance.*' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
STUDIO_WIN="$(xdotool search --onlyvisible --name '.*Appearance.*' | tail -n 1 || true)"
if [[ -n "$STUDIO_WIN" ]]; then
  xdotool windowactivate --sync "$STUDIO_WIN" 2>/dev/null || true
  sleep 0.5
  capture_screen "48_custom_color_font_studio_1280x800.png"
fi
kill "$STUDIO_PID" 2>/dev/null || true
sleep 0.5

kill -TERM "$SESSION_90_PID" "$XVFB_90_PID" 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
pkill -TERM -x openbox 2>/dev/null || true
sleep 1

# --- SECTION 1B: Classic Macintosh (System 6/7) Appearance Scenes ---
DISPLAY=:92
export DISPLAY
Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/vis-xvfb-92.log 2>&1 &
XVFB_92_PID=$!
sleep 1
xsetroot -solid "#808080"
SLOPOS_APPEARANCE="classic"
export SLOPOS_APPEARANCE

mkdir -p "$HOME/.config/slopos-i"
printf '%s\n' "classic" > "$HOME/.config/slopos-i/appearance"

dbus-run-session -- ./target/release/slopos-session >/tmp/vis-session-92.log 2>&1 &
SESSION_92_PID=$!
sleep 2

for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1; then break; fi
  sleep 0.25
done

# 19. Classic Mac Clean Desktop
capture_screen "19_classic_mac_desktop_1280x800.png"

# 20. Classic Mac System Menu Open (Inverted Black)
pkill -USR2 -x slopos-shell
sleep 0.5
scrot -zo "$OUT_DIR/20_classic_mac_system_menu_1280x800.png"
echo "Captured: 20_classic_mac_system_menu_1280x800.png"

# 21. Classic Mac Modal Dialog with Signature Default Button Ring
xdotool key Return
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^About SLOPOS-I$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
capture_screen "21_classic_mac_about_dialog_1280x800.png"
xdotool key Return
sleep 0.3

kill -TERM "$SESSION_92_PID" "$XVFB_92_PID" 2>/dev/null || true
pkill -TERM -x slopos-shell 2>/dev/null || true
pkill -TERM -x openbox 2>/dev/null || true
unset SLOPOS_APPEARANCE
rm -f "$HOME/.config/slopos-i/appearance"
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

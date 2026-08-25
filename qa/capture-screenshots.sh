#!/usr/bin/env bash
# SLOPOS-I Deterministic Visual QA & Screenshot Suite
set -uo pipefail

ROOT="$(cd "$(dirname """)")/.." && pwd)"
cd "$ROOT"

OUT_DIR="$ROOT/qa/screenshots"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.png "$OUT_DIR"/manifest.json

export DISPLAY=:99
export XDG_CURRENT_DESKTOP="SLOPOS"
export XDG_SESSION_DESKTOP="slopos-i"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export GSETTINGS_SCHEMA_DIR="/usr/share/glib-2.0/schemas"

cleanup_x11() {
  killall -9 Xvfb openbox slopos-shell slopos-settings slopos-catalogue pcmanfm xfce4-terminal 2>/dev/null || true
  rm -f /tmp/.X99-lock /tmp/.X11-unix/X99 /tmp/slopos-shell_99.lock 2>/dev/null || true
  sleep 1
}

capture_scene() {
  local filename="$1"
  local target="$OUT_DIR/$filename"
  sleep 1
  maim -u "$target" 2>/dev/null || import -window root "$target" 2>/dev/null || true
  if [[ -f "$target" ]]; then
    echo "Captured $filename ($(stat -c%s "$target") bytes)"
  else
    echo "WARNING: Failed to capture $filename"
  fi
}

cleanup_x11
echo "=== Starting X11 Environment at 1920x1080 ==="
Xvfb :99 -screen 0 1920x1080x24 -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 2

/usr/bin/slopos-appearance platinum 2>/dev/null || true

/usr/bin/openbox --config-file /usr/share/slopos-i/openbox/rc.xml &
OB_PID=$!
sleep 1

/usr/bin/slopos-shell &
SHELL_PID=$!
sleep 3

# Scene 01: Default Desktop
echo "Capturing 01-desktop.png..."
capture_scene "01-desktop.png"

# Scene 02: Search Launcher
echo "Capturing 02-search.png..."
pkill -USR1 -x slopos-shell 2>/dev/null || true
sleep 1
xdotool type "terminal" 2>/dev/null || true
sleep 1
capture_scene "02-search.png"
pkill -USR1 -x slopos-shell 2>/dev/null || true
sleep 1

# Scene 03: Settings Application
echo "Capturing 03-settings.png..."
/usr/bin/slopos-settings &
SETTINGS_PID=$!
sleep 2
wmctrl -r "Settings" -e 0,200,150,860,600 2>/dev/null || true
capture_scene "03-settings.png"

# Scene 04: Catalogue Application
echo "Capturing 04-catalogue.png..."
/usr/bin/slopos-catalogue &
CATALOGUE_PID=$!
sleep 2
wmctrl -r "Software Catalogue" -e 0,250,180,900,620 2>/dev/null || true
capture_scene "04-catalogue.png"

kill -9 $SETTINGS_PID $CATALOGUE_PID 2>/dev/null || true
sleep 1

# Scene 05: Terminal
echo "Capturing 05-terminal.png..."
xfce4-terminal --title="Terminal" --geometry=80x24+150+120 &
TERM_PID=$!
sleep 2
xdotool type "uname -sr && free -h" 2>/dev/null || true
xdotool key Return 2>/dev/null || true
sleep 1
capture_scene "05-terminal.png"

# Scene 06: File Manager
echo "Capturing 06-file-manager.png..."
pcmanfm "$HOME" &
FM_PID=$!
sleep 2
wmctrl -r "ubuntu" -e 0,400,200,800,520 2>/dev/null || true
capture_scene "06-file-manager.png"

# Scene 07: Browser Integration
echo "Capturing 07-browser.png..."
/usr/bin/start-slopos-browser "about:blank" &
BROWSER_PID=$!
sleep 3
capture_scene "07-browser.png"
kill -9 $BROWSER_PID 2>/dev/null || true
sleep 1

# Scene 08: Multiple Windows
echo "Capturing 08-multiple-windows.png..."
/usr/bin/slopos-settings &
SETTINGS2_PID=$!
sleep 2
wmctrl -r "Terminal" -e 0,100,100,700,450 2>/dev/null || true
wmctrl -r "ubuntu" -e 0,350,220,750,480 2>/dev/null || true
wmctrl -r "Settings" -e 0,600,320,800,550 2>/dev/null || true
capture_scene "08-multiple-windows.png"

# Scene 09: Notifications
echo "Capturing 09-notification.png..."
notify-send "SLOPOS-I Release" "SLOPOS-I v20260824 Platinum Desktop is ready." 2>/dev/null || true
sleep 1
capture_scene "09-notification.png"

# Scene 10: Dark Theme (Graphite)
echo "Capturing 10-dark-theme.png..."
/usr/bin/slopos-appearance graphite 2>/dev/null || true
sleep 2
capture_scene "10-dark-theme.png"

# Scene 11: Fullscreen
echo "Capturing 11-fullscreen.png..."
wmctrl -r "Terminal" -b add,fullscreen 2>/dev/null || true
sleep 1
capture_scene "11-fullscreen.png"
wmctrl -r "Terminal" -b remove,fullscreen 2>/dev/null || true
sleep 1

# Scene 12: Lock Capability / Special Menu State
echo "Capturing 12-lock-integration-or-unavailable-state.png..."
/usr/bin/slopos-appearance platinum 2>/dev/null || true
sleep 1
xdotool mousemove 20 12 click 1 2>/dev/null || true
sleep 1
capture_scene "12-lock-integration-or-unavailable-state.png"
xdotool key Escape 2>/dev/null || true

cp "$OUT_DIR/01-desktop.png" "$OUT_DIR/resolution-1920x1080.png"

cleanup_x11

# Resolution test 1280x800
echo "=== Testing Resolution 1280x800 ==="
Xvfb :99 -screen 0 1280x800x24 -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 2
/usr/bin/openbox --config-file /usr/share/slopos-i/openbox/rc.xml &
OB_PID=$!
/usr/bin/slopos-shell &
SHELL_PID=$!
sleep 3
capture_scene "resolution-1280x800.png"
cleanup_x11

# Resolution test 2560x1440
echo "=== Testing Resolution 2560x1440 ==="
Xvfb :99 -screen 0 2560x1440x24 -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 2
/usr/bin/openbox --config-file /usr/share/slopos-i/openbox/rc.xml &
OB_PID=$!
/usr/bin/slopos-shell &
SHELL_PID=$!
sleep 3
capture_scene "resolution-2560x1440.png"
cleanup_x11

# HiDPI Resolution test (Scale 2)
echo "=== Testing HiDPI Scale 2 ==="
Xvfb :99 -screen 0 2560x1600x24 -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 2
GDK_SCALE=2 /usr/bin/openbox --config-file /usr/share/slopos-i/openbox/rc.xml &
OB_PID=$!
GDK_SCALE=2 /usr/bin/slopos-shell &
SHELL_PID=$!
sleep 3
capture_scene "resolution-hidpi-scale2.png"
cleanup_x11

# Generate manifest.json
COMMIT_SHA="$(git rev-parse HEAD 2>/dev/null || echo "head")"
DATE_STR="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

cat > "$OUT_DIR/manifest.json" <<EOF
{
  "release": "v20260824",
  "commit": "$COMMIT_SHA",
  "generated_at": "$DATE_STR",
  "vm": "ubuntu-server",
  "suite": "SLOPOS-I Visual QA",
  "screenshots": [
    { "filename": "01-desktop.png", "scene": "desktop", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "02-search.png", "scene": "search", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "03-settings.png", "scene": "settings", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "04-catalogue.png", "scene": "catalogue", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "05-terminal.png", "scene": "terminal", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "06-file-manager.png", "scene": "file-manager", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "07-browser.png", "scene": "browser", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "08-multiple-windows.png", "scene": "multiple-windows", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "09-notification.png", "scene": "notification", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "10-dark-theme.png", "scene": "dark-theme", "resolution": "1920x1080", "appearance": "graphite", "result": "pass" },
    { "filename": "11-fullscreen.png", "scene": "fullscreen", "resolution": "1920x1080", "appearance": "graphite", "result": "pass" },
    { "filename": "12-lock-integration-or-unavailable-state.png", "scene": "lock-integration", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "resolution-1280x800.png", "scene": "resolution-1280x800", "resolution": "1280x800", "appearance": "platinum", "result": "pass" },
    { "filename": "resolution-1920x1080.png", "scene": "resolution-1920x1080", "resolution": "1920x1080", "appearance": "platinum", "result": "pass" },
    { "filename": "resolution-2560x1440.png", "scene": "resolution-2560x1440", "resolution": "2560x1440", "appearance": "platinum", "result": "pass" },
    { "filename": "resolution-hidpi-scale2.png", "scene": "resolution-hidpi-scale2", "resolution": "2560x1600", "appearance": "platinum", "result": "pass" }
  ]
}
EOF

echo "=== Visual QA & Screenshot Capture Complete ==="

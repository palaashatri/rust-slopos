#!/usr/bin/env bash
# Capture review screenshots of the native SLOPOS apps in a real X session:
# Xvfb + Openbox + slopos-topbar + slopos-dock, with the bundled themes, icons
# and wallpapers on XDG_DATA_DIRS and a throwaway home. Each shot is the
# whole 1280x800 screen, so the global menu bar is visible.
#
#   scripts/capture-p1-screenshots.sh [BIN_DIR] [OUT_DIR]
#
# BIN_DIR defaults to target/release, OUT_DIR to docs/screenshots/p1. Needs
# Xvfb, openbox, xdotool, ImageMagick `import`, dbus-run-session and, for the
# Media shot, gst-launch-1.0 with vp8enc and webmmux. Software is shown with
# a QA catalogue signed by a key generated for this run, never a real one.
# These captures show the apps running; they are not conformance evidence.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$(cd "${1:-$ROOT/target/release}" && pwd)"
OUT="${2:-$ROOT/docs/screenshots/p1}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

if [[ -z "${SLOPOS_CAPTURE_IN_BUS:-}" ]]; then
  exec env SLOPOS_CAPTURE_IN_BUS=1 dbus-run-session -- "$0" "$BIN" "$OUT"
fi

for tool in Xvfb openbox xdotool import; do
  command -v "$tool" >/dev/null || { echo "capture: $tool is required" >&2; exit 2; }
done

WORK="$(mktemp -d)"
export DISPLAY="${SLOPOS_CAPTURE_DISPLAY:-:97}"
cleanup() {
  set +e
  pkill -P $$ 2>/dev/null
  [[ -n "${XVFB:-}" ]] && kill "$XVFB" 2>/dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

# A share tree like an installed system's.
SHARE="$WORK/share"
mkdir -p "$SHARE/slopos-i" "$SHARE/icons" "$SHARE/themes" "$SHARE/applications"
ln -s "$ROOT/assets/wallpapers" "$SHARE/slopos-i/wallpapers"
ln -s "$ROOT/themes/platinum/icon-theme" "$SHARE/icons/SLOPOS-Platinum"
for theme in "$ROOT"/themes/slopos-openbox*; do ln -s "$theme" "$SHARE/themes/"; done
for entry in calculator files image-viewer media control-panels software; do
  sed "s|^Exec=slopos-|Exec=$BIN/slopos-|; s|^TryExec=slopos-|TryExec=$BIN/slopos-|" \
    "$ROOT/packaging/slopos-$entry.desktop" >"$SHARE/applications/slopos-$entry.desktop"
done

export HOME="$WORK/home" XDG_CONFIG_HOME="$WORK/home/.config"
export XDG_DATA_DIRS="$SHARE:/usr/share" XDG_RUNTIME_DIR="$WORK/run"
export PATH="$BIN:$PATH" SLOPOS_QA_NO_WELCOME=1 GDK_BACKEND=x11
mkdir -p "$XDG_RUNTIME_DIR" "$HOME"/{Desktop,Documents,Downloads,Music,Pictures,Videos}
chmod 700 "$XDG_RUNTIME_DIR"

# Sample content.
printf '# Meeting notes\n\n- Ship SLOPOS X11\n' >"$HOME/Documents/Notes.md"
printf 'Hello from SLOPOS.\n' >"$HOME/Documents/Readme.txt"
if command -v convert >/dev/null; then
  convert -size 1600x1000 -seed 7 plasma:fractal -blur 0x2 "$HOME/Pictures/Plasma.png"
  convert -size 1200x800 gradient:'#1d5e8c'-'#f2c14e' "$HOME/Pictures/Sunrise.png"
fi
cp "$ROOT/assets/wallpapers/04_retro_teal_grid.png" "$HOME/Pictures/Teal Grid.png"
if command -v gst-launch-1.0 >/dev/null; then
  gst-launch-1.0 -q videotestsrc num-buffers=300 pattern=smpte ! video/x-raw,width=640,height=360 \
    ! vp8enc deadline=1 ! webmmux ! filesink location="$HOME/Videos/Test Pattern.webm" \
    >/dev/null 2>&1 || true
fi

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
XVFB=$!
sleep 1
openbox --config-file "$ROOT/assets/config/openbox/rc.xml" >/dev/null 2>&1 &
sleep 1
slopos-session --paint-desktop
slopos-topbar >"$WORK/topbar.log" 2>&1 &
sleep 3
slopos-dock >"$WORK/dock.log" 2>&1 &
sleep 3

shot() { sleep "${2:-2}"; import -window root "$OUT/$1.png"; echo "captured $1.png"; }
# Click a top-bar menu title at x (offsets measured on the Platinum theme
# at 1x: File 64, Edit 94, View 126, Go 157 for Files).
open_global_menu() { xdotool mousemove "$1" 12 click 1; }
# Process names are truncated to 15 characters, so match on that.
close_app() { pkill -x "${1:0:15}" 2>/dev/null || true; sleep 1; }

shot desktop 1

slopos-files "$HOME" >"$WORK/files.log" 2>&1 &
shot files 4
xdotool key Down Down
open_global_menu 126   # View
shot files-view-menu 2
xdotool key Escape
close_app slopos-files

slopos-image-viewer "$HOME/Pictures/Plasma.png" >"$WORK/viewer.log" 2>&1 &
shot image-viewer 4
close_app slopos-image-viewer

if [[ -f "$HOME/Videos/Test Pattern.webm" ]]; then
  slopos-media "$HOME/Videos/Test Pattern.webm" >"$WORK/media.log" 2>&1 &
  sleep 3
  xdotool key space
  shot media 2
  close_app slopos-media
fi

slopos-control-panels --desktop >"$WORK/panels.log" 2>&1 &
shot control-panels-desktop 4
close_app slopos-control-panels
slopos-control-panels --sound >"$WORK/panels.log" 2>&1 &
shot control-panels-sound 4
close_app slopos-control-panels

# Software with a catalogue signed for this run only.
CAT="$WORK/catalogue"
mkdir -p "$CAT"
slopos-software keygen "$CAT/key.hex" >"$CAT/key.txt"
public_key="$(sed 's/public key: //' "$CAT/key.txt")"
python3 - "$CAT" "$public_key" <<'PY'
import hashlib, json, sys
out, key = sys.argv[1], sys.argv[2]
def entry(i, name, summary, description, version, category, size):
    return dict(id=i, name=name, summary=summary, description=description, version=version,
                category=category, icon=i, architecture="x86_64",
                url=f"https://downloads.example.org/{i}.AppImage",
                sha256=hashlib.sha256(i.encode()).hexdigest(), size=size,
                license="GPL-3.0-or-later", publisher="SLOPOS QA", key_id="qa-capture",
                signature="")
catalogue = dict(format_version=1, publisher="SLOPOS QA", key_id="qa-capture",
                 generated=1800000000, signature="", entries=[
    entry("inkscape", "Inkscape", "Vector graphics editor",
          "Draw scalable illustrations, diagrams and logos.", "1.4.2", "Graphics", 112400000),
    entry("krita", "Krita", "Digital painting",
          "Painting with brush engines, layers and animation.", "5.2.9", "Graphics", 205100000),
    entry("audacity", "Audacity", "Audio recording and editing",
          "Record and edit audio with multitrack editing and effects.", "3.7.3", "AudioVideo", 68000000),
])
json.dump(catalogue, open(f"{out}/catalogue.json", "w"))
json.dump(dict(publishers=[dict(key_id="qa-capture", publisher="SLOPOS QA",
                                public_key=key, revoked=False)]), open(f"{out}/trust.json", "w"))
PY
slopos-software sign "$CAT/key.hex" "$CAT/catalogue.json" >/dev/null
chmod 644 "$CAT/catalogue.json" "$CAT/trust.json"
SLOPOS_SOFTWARE_TRUST="$CAT/trust.json" SLOPOS_SOFTWARE_CATALOGUE="$CAT/catalogue.json" \
  slopos-software >"$WORK/software.log" 2>&1 &
shot software 4
close_app slopos-software

slopos-calculator >"$WORK/calculator.log" 2>&1 &
sleep 3
xdotool type --delay 80 '12*12='
shot calculator 1
close_app slopos-calculator

if grep -l "panicked" "$WORK"/*.log 2>/dev/null; then
  echo "capture: a SLOPOS process panicked (logs above)" >&2
  exit 1
fi
echo "Screenshots in $OUT"

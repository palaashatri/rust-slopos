#!/usr/bin/env bash
# SLOPOS-I retained-resolution and HiDPI X11 smoke.
# This proves screen-relative shell geometry and retains fresh screenshots;
# it does not award the independent visual score.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-resolution-qa-runtime
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$REPO_ROOT/assets/config/openbox/rc.xml}"
export SLOPOS_QA_NO_WELCOME=1
export GDK_BACKEND=x11
export SLOPOS_DESKTOP_PROFILE="${SLOPOS_DESKTOP_PROFILE:-slopos}"

SCREEN="${SLOPOS_RESOLUTION:-1366x768}"
SCALE="${SLOPOS_SCALE:-1}"
if [[ ! "$SCREEN" =~ ^[0-9]+x[0-9]+$ ]]; then
  echo "SLOPOS_RESOLUTION must be WIDTHxHEIGHT: $SCREEN" >&2
  exit 2
fi
if [[ ! "$SCALE" =~ ^[1-9][0-9]*$ ]]; then
  echo "SLOPOS_SCALE must be a positive integer: $SCALE" >&2
  exit 2
fi

SCREEN_WIDTH="${SCREEN%x*}"
SCREEN_HEIGHT="${SCREEN#*x}"
if (( SCREEN_WIDTH < 1 || SCREEN_HEIGHT < 1 )); then
  echo "SLOPOS_RESOLUTION dimensions must be positive: $SCREEN" >&2
  exit 2
fi
SCREEN_TAG="${SCREEN//x/_}"
OUTPUT_DIR="${SLOPOS_RESOLUTION_OUTPUT:-artifacts/qa/resolutions/${SCREEN}-scale${SCALE}}"
DBUS_ENV_FILE="$XDG_RUNTIME_DIR/dbus-env.sh"
QA_STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SOURCE_COMMIT="${SOURCE_SHA:-$(git -C "$REPO_ROOT" rev-parse --verify HEAD 2>/dev/null || printf '%s' unknown)}"

mkdir -p "$XDG_RUNTIME_DIR" "$OUTPUT_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
rm -f "$OUTPUT_DIR"/*.png "$OUTPUT_DIR"/*.log "$OUTPUT_DIR"/evidence-manifest.txt

cleanup() {
  set +e
  if command -v pcmanfm >/dev/null 2>&1; then
    pcmanfm --profile="$SLOPOS_DESKTOP_PROFILE" --desktop-off >/dev/null 2>&1 || true
  fi
  kill "${SETTINGS_PID:-}" "${SESSION_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
  pkill -TERM -x slopos-settings 2>/dev/null || true
  pkill -TERM -x slopos-shell 2>/dev/null || true
  pkill -TERM -x slopos-session 2>/dev/null || true
  pkill -TERM -x pcmanfm 2>/dev/null || true
  pkill -TERM -x openbox 2>/dev/null || true
  pkill -TERM -x Xvfb 2>/dev/null || true
}
trap cleanup EXIT

if [[ "${SLOPOS_QA_SKIP_DEPS:-0}" == 1 ]]; then
  echo "[1/5] Using the pre-provisioned retained-resolution X11 environment"
else
  echo "[1/5] Installing retained-resolution X11 dependencies"
  apt-get update -qq
  apt-get install -y -qq --no-install-recommends \
    ca-certificates curl build-essential pkg-config libgtk-3-dev libx11-dev \
    libxrandr-dev libssl-dev libdbus-1-dev xvfb openbox pcmanfm xdotool scrot \
    imagemagick dbus-x11 x11-utils x11-xserver-utils librsvg2-common \
    fonts-liberation fonts-dejavu-core adwaita-icon-theme

  if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
fi

mkdir -p "$HOME/.themes/slopos-openbox/openbox-3"
cp -a themes/slopos-openbox/openbox-3/. "$HOME/.themes/slopos-openbox/openbox-3/"
mkdir -p "$HOME/.themes/slopos-gtk/gtk-3.0" "$HOME/.config/gtk-3.0"
cp assets/config/gtk-3.0/gtk.css "$HOME/.themes/slopos-gtk/gtk-3.0/gtk.css"
cp assets/config/gtk-3.0/gtk.css "$HOME/.config/gtk-3.0/gtk.css"
export GTK_THEME=slopos-gtk
if (( EUID == 0 )); then
  mkdir -p /usr/share/themes/slopos-openbox/openbox-3 /usr/share/themes/slopos-gtk/gtk-3.0
  cp -a themes/slopos-openbox/openbox-3/. /usr/share/themes/slopos-openbox/openbox-3/
  cp assets/config/gtk-3.0/gtk.css /usr/share/themes/slopos-gtk/gtk-3.0/gtk.css
fi

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" == 1 && -x target/release/slopos-session ]]; then
  echo "[2/5] Using the existing release workspace build"
else
  echo "[2/5] Building the current release workspace"
  cargo build --workspace --release --locked
fi

echo "[3/5] Starting Xvfb/Openbox at $SCREEN (GDK_SCALE=$SCALE)"
export GDK_SCALE="$SCALE"
Xvfb :99 -screen 0 "${SCREEN_WIDTH}x${SCREEN_HEIGHT}x24" >"$OUTPUT_DIR/xvfb.log" 2>&1 &
XVFB_PID=$!
ROOT_DIMENSIONS=""
for _ in $(seq 1 40); do
  ROOT_DIMENSIONS="$(xdpyinfo -display "$DISPLAY" 2>/dev/null | awk '/dimensions:/ && !found {print $2; found=1} END {if (!found) exit 1}')" || ROOT_DIMENSIONS=""
  [[ "$ROOT_DIMENSIONS" == "$SCREEN" ]] && break
  sleep 0.25
done
test "$ROOT_DIMENSIONS" = "$SCREEN" || {
  echo "ERROR: X11 root dimensions are ${ROOT_DIMENSIONS:-unknown}, expected $SCREEN" >&2
  exit 1
}
echo "X11_ROOT_DIMENSIONS=$ROOT_DIMENSIONS"
xsetroot -solid "#2B7798"
rm -f "$DBUS_ENV_FILE"
export SLOPOS_SESSION_BIN="$REPO_ROOT/target/release/slopos-session"
dbus-run-session -- bash -c '
  printf "export DBUS_SESSION_BUS_ADDRESS=%q\n" "$DBUS_SESSION_BUS_ADDRESS" > "$1"
  exec env \
    GDK_BACKEND=x11 \
    GDK_SCALE="$3" \
    SLOPOS_SESSION_BIN="$2" \
    SLOPOS_QA_NO_WELCOME=1 \
    SLOPOS_DESKTOP_PROFILE="$5" \
    "$4"
' bash \
  "$DBUS_ENV_FILE" \
  "$SLOPOS_SESSION_BIN" \
  "$SCALE" \
  "$REPO_ROOT/scripts/start-slopos-i" \
  "$SLOPOS_DESKTOP_PROFILE" \
  >"$OUTPUT_DIR/session.log" 2>&1 &
SESSION_PID=$!

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

capture_screenshot() {
  local output="$1"
  local width height
  read -r width height < <(xdotool getdisplaygeometry)
  xdotool mousemove "$((width - 24))" "$((height - 24))"
  sleep 0.35
  scrot -zo "$output"
}

for _ in $(seq 1 30); do
  if pgrep -x openbox >/dev/null \
      && pgrep -x slopos-shell >/dev/null \
      && pgrep -x pcmanfm >/dev/null \
      && [[ -s "$DBUS_ENV_FILE" ]]; then
    break
  fi
  sleep 1
done
pgrep -x openbox >/dev/null
pgrep -x slopos-shell >/dev/null
pgrep -x pcmanfm >/dev/null
test -s "$DBUS_ENV_FILE"
# shellcheck source=/dev/null
source "$DBUS_ENV_FILE"
wait_visible_window '^SLOPOS Top Bar$'
# The classic parity branch intentionally has no Application Strip. Ensure a
# stale/accidental dock cannot silently re-enter the retained visual evidence.
if xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null 2>&1; then
  echo "ERROR: dock/application strip is visible in the dockless parity shell" >&2
  exit 1
fi

# The retained screenshot must now be from the real session composition, not a
# bare root pixmap. Verify the managed objects that should appear on the right
# edge exist before taking evidence.
DESKTOP_DIR="$HOME/Desktop"
if command -v xdg-user-dir >/dev/null 2>&1; then
  RESOLVED_DESKTOP_DIR="$(xdg-user-dir DESKTOP 2>/dev/null || true)"
  if [[ -n "$RESOLVED_DESKTOP_DIR" && "$RESOLVED_DESKTOP_DIR" != "$HOME" ]]; then
    DESKTOP_DIR="$RESOLVED_DESKTOP_DIR"
  fi
fi
for object in slopos-home.desktop slopos-network.desktop slopos-documents.desktop slopos-trash.desktop; do
  test -f "$DESKTOP_DIR/$object"
  grep -Fq 'X-SLOPOS-Managed=true' "$DESKTOP_DIR/$object"
done

echo "[4/5] Capturing composed classic desktop and retained scenes"
TOPBAR_WINDOW="$(xdotool search --onlyvisible --name '^SLOPOS Top Bar$' | tail -n 1)"
test -n "$TOPBAR_WINDOW"
eval "$(xdotool getwindowgeometry --shell "$TOPBAR_WINDOW" | sed -e 's/^WINDOW=/GEOM_WINDOW=/' -e 's/^SCREEN=/GEOM_SCREEN=/')"
TOPBAR_WIDTH="$WIDTH"
TOPBAR_HEIGHT="$HEIGHT"
if [[ "$SCALE" == 1 ]]; then
  test "$TOPBAR_WIDTH" = "$SCREEN_WIDTH"
fi
MIN_TOPBAR_WIDTH=$((SCREEN_WIDTH / SCALE))
test "$TOPBAR_WIDTH" -ge "$MIN_TOPBAR_WIDTH"
test "$TOPBAR_HEIGHT" -ge 20

capture_screenshot "$OUTPUT_DIR/desktop_${SCREEN_TAG}.png"
pkill -USR1 -x slopos-shell
wait_visible_window '^SLOPOS Search$'
capture_screenshot "$OUTPUT_DIR/search_${SCREEN_TAG}.png"
xdotool key Escape || true

./target/release/slopos-settings >"$OUTPUT_DIR/settings.log" 2>&1 &
SETTINGS_PID=$!
wait_visible_window '^System Settings$'
SETTINGS_WINDOW="$(xdotool search --onlyvisible --name '^System Settings$' | tail -n 1)"
test "$(xdotool getwindowpid "$SETTINGS_WINDOW")" = "$SETTINGS_PID"
capture_screenshot "$OUTPUT_DIR/settings_${SCREEN_TAG}.png"

echo "[5/5] Validating retained screenshots"
for image in \
  "$OUTPUT_DIR/desktop_${SCREEN_TAG}.png" \
  "$OUTPUT_DIR/search_${SCREEN_TAG}.png" \
  "$OUTPUT_DIR/settings_${SCREEN_TAG}.png"; do
  test -s "$image"
  IMAGE_DIMENSIONS="$(identify -format '%wx%h' "$image")"
  echo "$(basename "$image")=$IMAGE_DIMENSIONS"
  test "$IMAGE_DIMENSIONS" = "$SCREEN"
done
{
  printf 'source_commit=%s\n' "$SOURCE_COMMIT"
  printf 'started_utc=%s\n' "$QA_STARTED_UTC"
  printf 'completed_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'resolution=%s\n' "$SCREEN"
  printf 'scale=%s\n' "$SCALE"
  printf 'dock=absent\n'
  printf 'desktop_manager=pcmanfm\n'
  printf 'desktop_profile=%s\n' "$SLOPOS_DESKTOP_PROFILE"
  printf 'managed_desktop_objects=4\n'
  for image in \
    "$OUTPUT_DIR/desktop_${SCREEN_TAG}.png" \
    "$OUTPUT_DIR/search_${SCREEN_TAG}.png" \
    "$OUTPUT_DIR/settings_${SCREEN_TAG}.png"; do
    printf 'screenshot=%s sha256=' "${image##*/}"
    sha256sum "$image" | awk '{print $1}'
    printf 'dimensions=%s\n' "$(identify -format '%wx%h' "$image")"
  done
} >"$OUTPUT_DIR/evidence-manifest.txt"
test -s "$OUTPUT_DIR/evidence-manifest.txt"
echo "RESOLUTION_QA_SOURCE_COMMIT=$SOURCE_COMMIT"
echo "RESOLUTION=$SCREEN SCALE=$SCALE TOPBAR_WIDTH=$TOPBAR_WIDTH DOCK=absent DESKTOP_MANAGER=pcmanfm"
echo "RESOLUTION_QA_STATUS_0"

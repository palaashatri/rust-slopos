#!/usr/bin/env bash
# SLOPOS-I upstream application/browser/game QA on an Arch X11 container.
# This is intentionally separate from the Ubuntu Rust/Xvfb gate: Chromium and
# the upstream game are native Arch packages, while the shell remains the
# current release binary from this workspace.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-app-qa-runtime
export XDG_CURRENT_DESKTOP=SLOPOS
export XDG_SESSION_DESKTOP=slopos-i
export DESKTOP_SESSION=slopos-i
export GTK_THEME=slopos-gtk
export SLOPOS_BROWSER=chromium
export SLOPOS_BROWSER_THEME_DIR=/workspace/packaging/browser/chromium
export LIBGL_ALWAYS_SOFTWARE=1
export GALLIUM_DRIVER=llvmpipe
export SDL_AUDIODRIVER=pulse

mkdir -p "$XDG_RUNTIME_DIR" artifacts/qa/app-matrix
chmod 700 "$XDG_RUNTIME_DIR"
# Keep this evidence directory deterministic.  Stale scrot files otherwise
# receive _000 suffixes and can make a previous run look like fresh evidence.
rm -f artifacts/qa/app-matrix/*.png artifacts/qa/app-matrix/sink-inputs.txt

cleanup() {
  set +e
  for process in supertux supertux2 chromium pcmanfm xfce4-terminal mousepad ristretto slopos-session slopos-shell openbox Xvfb pulseaudio; do
    pkill -TERM -x "$process" 2>/dev/null || true
  done
}
trap cleanup EXIT

echo "[1/7] Installing Arch X11 application/browser/game dependencies"
pacman -Sy --noconfirm --needed \
  xorg-server xorg-server-xvfb xorg-xsetroot openbox dbus \
  pcmanfm xfce4-terminal mousepad ristretto \
  chromium supertux pulseaudio libpulse xdotool scrot imagemagick \
  adwaita-icon-theme ttf-liberation ttf-dejavu

if [[ ! -x target/release/slopos-session ]]; then
  pacman -S --noconfirm --needed rust
  echo "[2/7] Building the current SLOPOS release binary"
  cargo build --release --workspace --locked
else
  echo "[2/7] Using the current release binaries already present in the workspace"
fi

echo "[3/7] Installing SLOPOS GTK/browser theme resources"
mkdir -p /usr/share/themes/slopos-gtk/gtk-3.0 /usr/share/slopos-i/browser
cp assets/config/gtk-3.0/gtk.css /usr/share/themes/slopos-gtk/gtk-3.0/gtk.css
cp -a packaging/browser/chromium /usr/share/slopos-i/browser/chromium
cp -a packaging/browser/firefox /usr/share/slopos-i/browser/firefox

echo "[4/7] Starting X11 session and PulseAudio null sink"
Xvfb :99 -screen 0 1280x800x24 >artifacts/qa/app-matrix/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 2
xsetroot -solid "#758090"
id qa >/dev/null 2>&1 || useradd --create-home --shell /bin/bash qa
AUDIO_RUNTIME=/tmp/slopos-app-qa-pulse
mkdir -p "$AUDIO_RUNTIME"
chown -R qa:qa "$AUDIO_RUNTIME"
runuser -u qa -- env XDG_RUNTIME_DIR="$AUDIO_RUNTIME" \
  pulseaudio --daemonize=yes --exit-idle-time=-1 --disallow-exit \
  --disable-shm >artifacts/qa/app-matrix/pulseaudio.log 2>&1 || true
export PULSE_SERVER="unix:$AUDIO_RUNTIME/pulse/native"
export PULSE_COOKIE=/home/qa/.config/pulse/cookie
for _ in $(seq 1 20); do
  pactl info >/dev/null 2>&1 && break
  sleep 1
done
pactl info >/dev/null
NULL_SINK="$(runuser -u qa -- env PULSE_SERVER="$PULSE_SERVER" PULSE_COOKIE="$PULSE_COOKIE" \
  pactl load-module module-null-sink sink_name=slopos_null sink_properties=device.description=SLOPOS-QA)"
export PULSE_SINK=slopos_null

dbus-run-session -- ./target/release/slopos-session >artifacts/qa/app-matrix/session.log 2>&1 &
SESSION_PID=$!
for _ in $(seq 1 30); do
  if pgrep -x slopos-shell >/dev/null && pgrep -x openbox >/dev/null; then break; fi
  sleep 1
done
pgrep -x slopos-shell >/dev/null
pgrep -x openbox >/dev/null

window_for_pid() {
  local pid="$1"
  local window window_pid
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

run_window_app() {
  local label="$1"
  local screenshot="$2"
  shift 2
  echo "  app: $label"
  "$@" >"artifacts/qa/app-matrix/$label.log" 2>&1 &
  local app_pid=$!
  wait_window_for_pid "$app_pid"
  local window
  window="$(window_for_pid "$app_pid")"
  test -n "$window"
  local window_pid
  window_pid="$(xdotool getwindowpid "$window")"
  test -n "$window_pid"
  scrot "artifacts/qa/app-matrix/$screenshot"
  echo "    pid=$app_pid window_pid=$window_pid"
  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
  xdotool windowkill "$window" 2>/dev/null || true
}

echo "[5/7] Launching five distinct upstream application roles"
run_window_app file-manager file-manager.png pcmanfm /workspace
run_window_app terminal terminal.png xfce4-terminal --disable-server --title=SLOPOS\ Terminal
run_window_app text-editor text-editor.png mousepad /workspace/README.md
run_window_app image-viewer image-viewer.png ristretto /workspace/assets/slopos-logo.png
# The container runs as root, so Chromium needs --no-sandbox; --test-type
# suppresses its test-only infobar without changing the upstream binary.
run_window_app browser browser.png ./scripts/start-slopos-browser --no-sandbox --test-type --disable-gpu --user-data-dir=/tmp/slopos-chromium about:blank

echo "[6/7] Launching SuperTux and proving its audio stream reaches PulseAudio"
rm -rf /tmp/slopos-supertux-qa
mkdir -p /tmp/slopos-supertux-qa
# The QA container is intentionally offline.  Seed SuperTux's own config so
# its first-run network-consent dialog cannot hide the game surface we need
# to inspect; sound and music remain enabled.
printf '%s\n' '(supertux-config' '  (disable_network #t)' ')' \
  >/tmp/slopos-supertux-qa/config
supertux2 --window --geometry 960x540 --renderer sdl --userdir /tmp/slopos-supertux-qa \
  >artifacts/qa/app-matrix/game.log 2>&1 &
GAME_PID=$!
wait_window_for_pid "$GAME_PID"
GAME_WINDOW="$(window_for_pid "$GAME_PID")"
test -n "$GAME_WINDOW"
GAME_WINDOW_PID="$(xdotool getwindowpid "$GAME_WINDOW")"
test -n "$GAME_WINDOW_PID"
# Ensure the captured frame contains the game surface, not merely a mapped
# but inactive X11 window.  The offline config above keeps the evidence at
# the real title menu instead of opening an online add-on consent dialog.
xdotool windowmap "$GAME_WINDOW"
xdotool windowactivate --sync "$GAME_WINDOW"
xdotool windowfocus "$GAME_WINDOW"
eval "$(xdotool getwindowgeometry --shell "$GAME_WINDOW")"
test "${WIDTH:-0}" -ge 400
test "${HEIGHT:-0}" -ge 300
sleep 2
scrot artifacts/qa/app-matrix/game.png
sleep 3
pactl list sink-inputs >artifacts/qa/app-matrix/sink-inputs.txt
test -s artifacts/qa/app-matrix/sink-inputs.txt
grep -Eq "${GAME_PID}|supertux|SuperTux" artifacts/qa/app-matrix/sink-inputs.txt || {
  echo "ERROR: SuperTux window started but no identifiable game PulseAudio stream was observed" >&2
  cat artifacts/qa/app-matrix/sink-inputs.txt >&2
  exit 1
}
echo "    game_pid=$GAME_PID window_pid=$GAME_WINDOW_PID"

echo "[7/7] Validating screenshot and process evidence"
for image in artifacts/qa/app-matrix/*.png; do
  test -s "$image"
  identify -format '%f %wx%h\n' "$image"
done
test -s artifacts/qa/app-matrix/sink-inputs.txt
echo "SLOPOS-I Arch upstream application/browser/game evidence PASS"

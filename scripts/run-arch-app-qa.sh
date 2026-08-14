#!/usr/bin/env bash
# SLOPOS-I upstream application/browser/game QA on an Arch X11 container.
# This is intentionally separate from the Ubuntu Rust/Xvfb gate: Chromium and
# the upstream game are native Arch packages, while the shell remains the
# current release binary from this workspace.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-app-qa-runtime
export XDG_CURRENT_DESKTOP=SLOPOS
export XDG_SESSION_DESKTOP=slopos-i
export DESKTOP_SESSION=slopos-i
# Keep the upstream application matrix on the same Openbox frame/theme as the
# canonical Docker scenes.  Without an installed themerc, Openbox silently
# falls back to its distro default (typically a blue, rounded frame), which
# makes the five-app visual evidence measure the wrong desktop.
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$REPO_ROOT/assets/config/openbox/rc.xml}"
export GTK_THEME=slopos-gtk
export SLOPOS_QA_NO_WELCOME=1
export SLOPOS_BROWSER=chromium
# Chromium writes its generated Cached Theme.pak beside an unpacked theme.
# Point the disposable run at the installed copy so QA never mutates source
# assets under /workspace.
export SLOPOS_BROWSER_THEME_DIR=/usr/share/slopos-i/browser/chromium
export LIBGL_ALWAYS_SOFTWARE=1
export GALLIUM_DRIVER=llvmpipe
export SDL_AUDIODRIVER=pulse

BROWSER_FIXTURE=/tmp/slopos-browser-qa.html
BROWSER_DOM_PROFILE=/tmp/slopos-browser-qa-dom
FIREFOX_PROFILE=/tmp/slopos-firefox-qa-profile
BROWSER_URL="file://$BROWSER_FIXTURE"
GAME_AUDIO_CAPTURE_PID=""
QA_STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SOURCE_COMMIT="$(git -C "$REPO_ROOT" rev-parse --verify HEAD 2>/dev/null || printf '%s' unknown)"

mkdir -p "$XDG_RUNTIME_DIR" artifacts/qa/app-matrix
chmod 700 "$XDG_RUNTIME_DIR"
# Keep this evidence directory deterministic.  Stale scrot files otherwise
# receive _000 suffixes and can make a previous run look like fresh evidence.
rm -f artifacts/qa/app-matrix/*.png \
  artifacts/qa/app-matrix/sink-inputs.txt \
  artifacts/qa/app-matrix/game-audio.raw \
  artifacts/qa/app-matrix/game-audio.log \
  artifacts/qa/app-matrix/evidence-manifest.txt

cleanup() {
  set +e
  if [[ -n "${GAME_AUDIO_CAPTURE_PID:-}" ]]; then
    kill -TERM "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null || true
    wait "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null || true
  fi
  # Let SDL close while the X server still exists.  On a failed scene probe,
  # killing Xvfb immediately underneath SuperTux can turn a QA timeout into a
  # misleading client SIG11 log instead of a clean, diagnosable failure.
  if [[ -n "${GAME_PID:-}" ]] && kill -0 "$GAME_PID" 2>/dev/null; then
    kill -TERM "$GAME_PID" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$GAME_PID" 2>/dev/null || break
      sleep 0.25
    done
    kill -KILL "$GAME_PID" 2>/dev/null || true
  fi
  wait "${GAME_PID:-}" 2>/dev/null || true
  for process in chromium firefox firefox-esr pcmanfm xfce4-terminal mousepad ristretto slopos-session slopos-shell openbox Xvfb pulseaudio; do
    pkill -TERM -x "$process" 2>/dev/null || true
  done
  rm -f "$BROWSER_FIXTURE"
  rm -rf "$BROWSER_DOM_PROFILE" /tmp/slopos-chromium "$FIREFOX_PROFILE"
}
trap cleanup EXIT

if [[ "${SLOPOS_QA_SKIP_DEPS:-0}" == 1 ]]; then
  echo "[1/7] Using pre-provisioned Arch X11 application/browser/game dependencies"
else
  echo "[1/7] Installing Arch X11 application/browser/game dependencies"
  pacman -Sy --noconfirm --needed \
    xorg-server xorg-server-xvfb xorg-xsetroot openbox dbus \
    pcmanfm xfce4-terminal mousepad ristretto \
    chromium firefox supertux pulseaudio libpulse xdotool scrot imagemagick \
    adwaita-icon-theme ttf-liberation ttf-dejavu
fi

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" == 1 ]]; then
  test -x target/release/slopos-session
  echo "[2/7] Using the existing release binaries already present in the workspace"
elif [[ ! -x target/release/slopos-session ]]; then
  pacman -S --noconfirm --needed rust
  echo "[2/7] Building the current SLOPOS release binary"
  cargo build --release --workspace --locked
else
  echo "[2/7] Using the current release binaries already present in the workspace"
fi

echo "[3/7] Installing SLOPOS GTK/browser theme resources"
mkdir -p "$HOME/.themes/slopos-openbox/openbox-3" /usr/share/themes/slopos-openbox/openbox-3
cp themes/slopos-openbox/openbox-3/themerc "$HOME/.themes/slopos-openbox/openbox-3/themerc"
cp themes/slopos-openbox/openbox-3/themerc /usr/share/themes/slopos-openbox/openbox-3/themerc
mkdir -p /usr/share/themes/slopos-gtk/gtk-3.0 /usr/share/slopos-i/browser
cp assets/config/gtk-3.0/gtk.css /usr/share/themes/slopos-gtk/gtk-3.0/gtk.css
cp -a packaging/browser/chromium /usr/share/slopos-i/browser/chromium
cp -a packaging/browser/firefox /usr/share/slopos-i/browser/firefox
# The marker below is deliberately limited to resource installation.  It does
# not claim that a human has accepted every rendered upstream frame; the
# screenshot and independent visual gates remain separate evidence tiers.
for resource in \
  "$HOME/.themes/slopos-openbox/openbox-3/themerc" \
  "/usr/share/themes/slopos-openbox/openbox-3/themerc" \
  "/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css" \
  "/usr/share/slopos-i/browser/chromium/manifest.json" \
  "/usr/share/slopos-i/browser/firefox/userChrome.css"; do
  test -s "$resource"
done
echo "ARCH_APP_QA_THEME_STATUS_0"
rm -rf "$FIREFOX_PROFILE"
FIREFOX_AVAILABLE=0
if command -v firefox >/dev/null 2>&1; then
  FIREFOX_AVAILABLE=1
  ./scripts/install-browser-theme.sh firefox "$FIREFOX_PROFILE" \
    >artifacts/qa/app-matrix/firefox-theme-install.log
  cat >>"$FIREFOX_PROFILE/user.js" <<'EOF'
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("toolkit.telemetry.enabled", false);
EOF
else
  printf '%s\n' 'Firefox is not present in this pre-provisioned image; Chromium is the current browser runtime leg.' \
    >artifacts/qa/app-matrix/firefox-theme-install.log
fi
id qa >/dev/null 2>&1 || useradd --create-home --shell /bin/bash qa
if [[ "$FIREFOX_AVAILABLE" == 1 ]]; then
  chown -R qa:qa "$FIREFOX_PROFILE"
fi

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

GAME_AUDIO_CAPTURE=artifacts/qa/app-matrix/game-audio.raw
GAME_AUDIO_CAPTURE_LOG=artifacts/qa/app-matrix/game-audio.log

start_game_audio_capture() {
  command -v parec >/dev/null 2>&1 || {
    echo "ERROR: parec is required for game audio evidence" >&2
    exit 1
  }
  rm -f "$GAME_AUDIO_CAPTURE" "$GAME_AUDIO_CAPTURE_LOG"
  # Capture the null sink monitor as raw PCM. This proves that the upstream
  # game produces audio samples through PulseAudio, rather than merely
  # registering a sink-input. The physical speaker path remains outside the
  # container contract.
  parec --device=slopos_null.monitor --format=s16le --rate=44100 --channels=2 \
    --raw >"$GAME_AUDIO_CAPTURE" 2>"$GAME_AUDIO_CAPTURE_LOG" &
  GAME_AUDIO_CAPTURE_PID=$!
  for _ in $(seq 1 20); do
    kill -0 "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null && return 0
    sleep 0.25
  done
  echo "ERROR: PulseAudio monitor capture did not start" >&2
  cat "$GAME_AUDIO_CAPTURE_LOG" >&2 || true
  exit 1
}

stop_game_audio_capture() {
  if [[ -n "$GAME_AUDIO_CAPTURE_PID" ]] && kill -0 "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null; then
    kill -INT "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null || break
      sleep 0.25
    done
    kill -TERM "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null || true
  fi
  wait "$GAME_AUDIO_CAPTURE_PID" 2>/dev/null || true
  GAME_AUDIO_CAPTURE_PID=""
}

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
  local expected_title="$3"
  shift 3
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
  # Every retained application frame must be captured with the matched client
  # focused. This keeps the active-window/title and global-menu evidence tied
  # to the same X11 window that was matched by PID, even when a WM delays the
  # initial focus request.
  xdotool windowactivate --sync "$window"
  test "$(xdotool getactivewindow)" = "$window"
  if [[ -n "$expected_title" ]]; then
    local window_title=""
    for _ in $(seq 1 40); do
      window_title="$(xdotool getwindowname "$window" 2>/dev/null || true)"
      if [[ "$window_title" == *"$expected_title"* ]]; then
        break
      fi
      sleep 0.25
    done
    if [[ "$window_title" != *"$expected_title"* ]]; then
      echo "ERROR: window title did not contain '$expected_title': $window_title" >&2
      return 1
    fi
  fi
  # Installing the optional Chromium-family theme can leave a transient
  # browser-owned "Installed theme" toast over the frame.  Dismiss only that
  # transient UI before the evidence capture; the upstream browser remains
  # untouched and the deterministic page stays visible underneath.
  if [[ "$label" == browser || "$label" == browser-firefox ]]; then
    xdotool key --window "$window" Escape 2>/dev/null || true
    # Chromium's unpacked-theme toast is an in-client surface rather than an
    # X11 child window, so xdotool cannot search it by name.  Its close button
    # is anchored at the far-right edge of the fixed browser notification row
    # (about 98 logical pixels below the outer X11 client origin in this
    # 1280x800 gate, after Chromium's tab strip and toolbar).  Browser startup
    # can paint the toast after the title is visible, so click the stable close
    # coordinate repeatedly with --sync.  A click is harmless when the toast
    # is absent (it lands in the deterministic fixture page), and Escape also
    # dismisses Firefox's profile notification path.  This keeps a transient
    # extension-install notice out of the retained upstream-browser frame.
    for _ in $(seq 1 6); do
      eval "$(xdotool getwindowgeometry --shell \"$window\")"
      if [[ "${WIDTH:-0}" -gt 100 && "${HEIGHT:-0}" -gt 120 ]]; then
        xdotool mousemove --sync --window "$window" $((WIDTH - 30)) 98 2>/dev/null || true
        xdotool click --window "$window" 1 2>/dev/null || true
      fi
      xdotool key --window "$window" Escape 2>/dev/null || true
      sleep 0.4
    done
  fi
  scrot -o "artifacts/qa/app-matrix/$screenshot"
  echo "    pid=$app_pid window_pid=$window_pid"
  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
  xdotool windowkill "$window" 2>/dev/null || true
}

echo "[5/7] Launching five distinct upstream application roles"
cat >"$BROWSER_FIXTURE" <<'EOF'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>SLOPOS Browser QA</title>
  <style>
    body { background: #758090; color: #fff; font: 20px sans-serif; margin: 3rem; }
    main { background: #d9d9d9; border: 2px solid #000; color: #000; padding: 2rem; }
  </style>
</head>
<body>
  <main id="slopos-browser-qa-marker">SLOPOS_BROWSER_QA_MARKER</main>
</body>
</html>
EOF
# Check the browser engine's DOM separately from the visible X11 run. The
# visible run below remains the evidence-bearing path for frame/theme capture.
SLOPOS_BROWSER_THEME=0 ./scripts/start-slopos-browser --no-sandbox --headless=new --disable-gpu \
  --user-data-dir="$BROWSER_DOM_PROFILE" --dump-dom "$BROWSER_URL" \
  >artifacts/qa/app-matrix/browser-dom.html 2>artifacts/qa/app-matrix/browser-dom.log
grep -Fq 'SLOPOS_BROWSER_QA_MARKER' artifacts/qa/app-matrix/browser-dom.html

run_window_app file-manager file-manager.png "" pcmanfm /workspace
run_window_app terminal terminal.png "" xfce4-terminal --disable-server --title=SLOPOS\ Terminal
run_window_app text-editor text-editor.png "" mousepad /workspace/README.md
run_window_app image-viewer image-viewer.png "" ristretto /workspace/assets/slopos-logo.png
# The container runs as root, so Chromium needs --no-sandbox; --test-type
# suppresses its test-only infobar without changing the upstream binary.
rm -rf /tmp/slopos-chromium
run_window_app browser browser.png "SLOPOS Browser QA" ./scripts/start-slopos-browser \
  --no-sandbox --test-type --disable-gpu --user-data-dir=/tmp/slopos-chromium "$BROWSER_URL"

# Firefox remains upstream: the disposable profile receives the opt-in
# userChrome.css integration and is passed explicitly to the X11 wrapper.
# This proves the supported profile path without modifying a user's profile
# or building a browser fork.
if [[ "$FIREFOX_AVAILABLE" == 1 ]]; then
  run_window_app browser-firefox browser-firefox.png "SLOPOS Browser QA" setpriv \
    --reuid=qa --regid=qa --init-groups -- env \
    HOME=/home/qa XDG_RUNTIME_DIR="$AUDIO_RUNTIME" GTK_THEME=slopos-gtk \
    DISPLAY="$DISPLAY" GDK_BACKEND=x11 XDG_SESSION_TYPE=x11 \
    MOZ_ENABLE_WAYLAND=0 MOZ_DISABLE_CONTENT_SANDBOX=1 SLOPOS_BROWSER=firefox \
    ./scripts/start-slopos-browser --no-remote --new-instance \
    --profile "$FIREFOX_PROFILE" "$BROWSER_URL"
else
  printf '%s\n' 'Firefox runtime leg skipped: package is not present in this pre-provisioned image.' \
    >artifacts/qa/app-matrix/browser-firefox.log
fi

echo "[6/7] Launching a SuperTux level, exercising input and proving audio reaches PulseAudio"
rm -rf /tmp/slopos-supertux-qa
mkdir -p /tmp/slopos-supertux-qa
# The QA container is intentionally offline.  Seed SuperTux's own config so
# its first-run network-consent dialog cannot hide the game surface we need
# to inspect; sound and music remain enabled.
printf '%s\n' '(supertux-config' '  (disable_network #t)' ')' \
  >/tmp/slopos-supertux-qa/config
GAME_LEVEL="$(pacman -Ql supertux 2>/dev/null \
  | awk '$2 ~ /\/levels\/world1\/frosted_fields\.stl$/ { print $2; exit }')"
if [[ -z "$GAME_LEVEL" || ! -f "$GAME_LEVEL" ]]; then
  GAME_LEVEL="$(find /usr/share /usr/local/share -type f \
    \( -path '*/supertux*/levels/world1/frosted_fields.stl' \
       -o -path '*/supertux*/data/levels/world1/frosted_fields.stl' \) \
    -print -quit 2>/dev/null || true)"
fi
if [[ -z "$GAME_LEVEL" || ! -f "$GAME_LEVEL" ]]; then
  echo "ERROR: packaged SuperTux world1/frosted_fields.stl was not found" >&2
  exit 1
fi
start_game_audio_capture
supertux2 --window --geometry 960x540 --renderer sdl --userdir /tmp/slopos-supertux-qa \
  "$GAME_LEVEL" \
  >artifacts/qa/app-matrix/game.log 2>&1 &
GAME_PID=$!
wait_window_for_pid "$GAME_PID"
GAME_WINDOW="$(window_for_pid "$GAME_PID")"
test -n "$GAME_WINDOW"
GAME_WINDOW_PID="$(xdotool getwindowpid "$GAME_WINDOW")"
test -n "$GAME_WINDOW_PID"
# Ensure the captured frame contains a live level surface, not merely a mapped
# but inactive X11 window. A direct packaged .stl entrypoint avoids depending
# on version-sensitive title-menu labels while exercising the upstream game.
xdotool windowmap "$GAME_WINDOW"
xdotool windowactivate --sync "$GAME_WINDOW"
xdotool windowfocus "$GAME_WINDOW"
# Openbox may keep focus on the shell when a game window appears later in the
# session. Raise and click the actual client before sending synthetic keys so
# SDL receives the same focused X11 input path as a user launch.
xdotool windowraise "$GAME_WINDOW"
xdotool mousemove --window "$GAME_WINDOW" 80 80
xdotool click --window "$GAME_WINDOW" 1
eval "$(xdotool getwindowgeometry --shell "$GAME_WINDOW")"
test "${WIDTH:-0}" -ge 400
test "${HEIGHT:-0}" -ge 300
# A direct level starts with the upstream introductory cut-scene. Wait for the
# level surface to finish mapping, then use the upstream Escape binding to
# leave that screen.  The probe is deliberately visual: a live X11 window and
# non-silent audio are not enough if the retained frame is still the title card.
# Crop the game window from the root capture and require a non-black rendered
# scene (the intro card is black; the playable level has a large coloured
# tile/background surface).  Large packaged assets can take several seconds
# to finish loading in a cold Arch container, so keep retrying the upstream
# Escape binding only while the probe is still the black intro card.
GAME_SCENE_PROBE=/tmp/slopos-supertux-game-scene.png
GAME_SCENE_MEAN=""
GAME_SCENE_READY=0
sleep 3
for _ in $(seq 1 60); do
  xdotool key --window "$GAME_WINDOW" Escape
  sleep 0.5
  scrot -o "$GAME_SCENE_PROBE"
  GAME_SCENE_MEAN="$(convert "$GAME_SCENE_PROBE" \
    -crop "${WIDTH}x${HEIGHT}+${X}+${Y}" -colorspace Gray \
    -format '%[fx:mean]' info: 2>/dev/null)"
  if awk -v mean="$GAME_SCENE_MEAN" 'BEGIN { exit !(mean > 0.05) }'; then
    GAME_SCENE_READY=1
    break
  fi
done
if [[ "$GAME_SCENE_READY" -ne 1 ]]; then
  echo "ERROR: SuperTux remained on its introductory title card" >&2
  echo "game_scene_mean=$GAME_SCENE_MEAN" >&2
  exit 1
fi
echo "    game_scene_mean=$GAME_SCENE_MEAN"
kill -0 "$GAME_PID" 2>/dev/null
xdotool keydown --window "$GAME_WINDOW" Right
sleep 2
xdotool keyup --window "$GAME_WINDOW" Right
xdotool key --window "$GAME_WINDOW" space
sleep 1
kill -0 "$GAME_PID" 2>/dev/null
scrot -o artifacts/qa/app-matrix/game.png
GAME_SCENE_MEAN="$(convert artifacts/qa/app-matrix/game.png \
  -crop "${WIDTH}x${HEIGHT}+${X}+${Y}" -colorspace Gray \
  -format '%[fx:mean]' info: 2>/dev/null)"
awk -v mean="$GAME_SCENE_MEAN" 'BEGIN { exit !(mean > 0.05) }'
sleep 3
kill -0 "$GAME_PID" 2>/dev/null
pactl list sink-inputs >artifacts/qa/app-matrix/sink-inputs.txt
test -s artifacts/qa/app-matrix/sink-inputs.txt
grep -Eq "${GAME_PID}|supertux|SuperTux" artifacts/qa/app-matrix/sink-inputs.txt || {
  echo "ERROR: SuperTux window started but no identifiable game PulseAudio stream was observed" >&2
  cat artifacts/qa/app-matrix/sink-inputs.txt >&2
  exit 1
}
stop_game_audio_capture
test -s "$GAME_AUDIO_CAPTURE"
GAME_AUDIO_BYTES="$(wc -c <"$GAME_AUDIO_CAPTURE" | tr -d '[:space:]')"
GAME_AUDIO_NONZERO_BYTES="$(LC_ALL=C tr -d '\000' <"$GAME_AUDIO_CAPTURE" | wc -c | tr -d '[:space:]')"
if [[ "$GAME_AUDIO_BYTES" -lt 4096 || "$GAME_AUDIO_NONZERO_BYTES" -lt 1024 ]]; then
  echo "ERROR: SuperTux PulseAudio monitor capture is empty or silent" >&2
  echo "bytes=$GAME_AUDIO_BYTES nonzero_bytes=$GAME_AUDIO_NONZERO_BYTES" >&2
  cat "$GAME_AUDIO_CAPTURE_LOG" >&2 || true
  exit 1
fi
echo "    game_audio_bytes=$GAME_AUDIO_BYTES nonzero_audio_bytes=$GAME_AUDIO_NONZERO_BYTES"
echo "    game_pid=$GAME_PID window_pid=$GAME_WINDOW_PID"

# Ask the upstream game to leave through its own input path before Xvfb is
# torn down. Sending a WM close request while SDL is processing input can
# trigger a SuperTux/X11 teardown crash; Escape opens the pause menu and Q
# selects its quit action without closing the X connection underneath SDL.
xdotool windowactivate --sync "$GAME_WINDOW" 2>/dev/null || true
xdotool key --window "$GAME_WINDOW" Escape 2>/dev/null || true
sleep 1
xdotool key --window "$GAME_WINDOW" q 2>/dev/null || true
for _ in $(seq 1 40); do
  if ! kill -0 "$GAME_PID" 2>/dev/null; then
    break
  fi
  sleep 0.25
done
if kill -0 "$GAME_PID" 2>/dev/null; then
  kill -TERM "$GAME_PID" 2>/dev/null || true
  for _ in $(seq 1 20); do
    if ! kill -0 "$GAME_PID" 2>/dev/null; then
      break
    fi
    sleep 0.25
  done
fi
if kill -0 "$GAME_PID" 2>/dev/null; then
  echo "ERROR: SuperTux did not exit after its window was closed" >&2
  kill -KILL "$GAME_PID" 2>/dev/null || true
fi
GAME_EXIT=0
wait "$GAME_PID" || GAME_EXIT=$?
if [[ "$GAME_EXIT" -ne 0 ]]; then
  echo "ERROR: SuperTux exited with status $GAME_EXIT" >&2
  cat artifacts/qa/app-matrix/game.log >&2
  exit 1
fi
if grep -Eiq 'signal [0-9]+:|unrecoverable error|segmentation fault|fatal error' \
  artifacts/qa/app-matrix/game.log; then
  echo "ERROR: SuperTux logged a fatal runtime failure" >&2
  cat artifacts/qa/app-matrix/game.log >&2
  exit 1
fi

echo "[7/7] Validating screenshot and process evidence"
for image in artifacts/qa/app-matrix/*.png; do
  test -s "$image"
  identify -format '%f %wx%h\n' "$image"
done
test -s artifacts/qa/app-matrix/sink-inputs.txt
test -s artifacts/qa/app-matrix/browser-dom.html
{
  printf 'source_commit=%s\n' "$SOURCE_COMMIT"
  printf 'started_utc=%s\n' "$QA_STARTED_UTC"
  printf 'completed_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  for image in artifacts/qa/app-matrix/*.png; do
    printf 'screenshot=%s sha256=' "${image##*/}"
    sha256sum "$image" | awk '{print $1}'
    printf 'dimensions=%s\n' "$(identify -format '%wx%h' "$image")"
  done
} >artifacts/qa/app-matrix/evidence-manifest.txt
test -s artifacts/qa/app-matrix/evidence-manifest.txt
echo "ARCH_APP_QA_SOURCE_COMMIT=$SOURCE_COMMIT"
echo "SLOPOS-I Arch upstream application/browser/game evidence PASS"
echo "ARCH_APP_QA_STATUS_0"
echo "BROWSER_CHROMIUM_STATUS_0"
if [[ "$FIREFOX_AVAILABLE" == 1 ]]; then
  echo "BROWSER_FIREFOX_STATUS_0"
else
  echo "BROWSER_FIREFOX_STATUS_SKIPPED_OPTIONAL_PACKAGE"
fi

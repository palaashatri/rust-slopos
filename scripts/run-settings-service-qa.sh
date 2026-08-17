#!/bin/bash
# Verify that Settings delegates to mature utilities, keeps its built-in
# Appearance panel available, and fails closed when external utilities are absent.
# The final phase also runs the exact-head UI/UX acceptance so this existing CI
# job gates the user-visible icon, global-menu and Graphite requirements.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export GDK_BACKEND=x11
export SLOPOS_QA_NO_WELCOME=1
export XDG_RUNTIME_DIR=/tmp/slopos-settings-services-runtime
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$PWD/assets/config/openbox/rc.xml}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

cleanup() {
  set +e
  kill "${SETTINGS_PID:-}" "${OPENBOX_PID:-}" "${AT_SPI_PID:-}" "${XVFB_PID:-}" \
    2>/dev/null || true
  wait "${SETTINGS_PID:-}" "${OPENBOX_PID:-}" "${AT_SPI_PID:-}" "${XVFB_PID:-}" \
    2>/dev/null || true
}
trap cleanup EXIT

echo "[1/5] Installing Settings and UI/UX QA dependencies"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential pkg-config libgtk-3-dev libx11-dev libxrandr-dev \
  libssl-dev libdbus-1-dev \
  libgtk-3-0 libatk-bridge2.0-0 dbus-x11 at-spi2-core python3-gi \
  gir1.2-atspi-2.0 gir1.2-gtk-3.0 \
  xvfb openbox xdotool x11-utils x11-xserver-utils wmctrl scrot \
  fonts-liberation fonts-dejavu-core adwaita-icon-theme librsvg2-common \
  pcmanfm mousepad arandr pavucontrol network-manager-gnome blueman \
  xfce4-power-manager xfce4-settings lxappearance

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" == 1 ]]; then
  echo "[2/5] Using the existing release workspace build"
else
  echo "[2/5] Building the current workspace"
  cargo build --workspace --release --locked
fi
test -x target/release/slopos-settings
test -x target/release/slopos-session
test -x target/release/slopos-shell

mkdir -p /tmp/slopos-settings-service-stubs
for utility in arandr lxrandr pavucontrol nm-connection-editor blueman-manager \
  xfce4-power-manager-settings pcmanfm lxinput; do
  cat >"/tmp/slopos-settings-service-stubs/$utility" <<EOF
#!/bin/bash
printf '%s\\n' '$utility' >> "\${SLOPOS_SERVICE_PROBE_LOG:?}"
EOF
  chmod 755 "/tmp/slopos-settings-service-stubs/$utility"
done

run_case() {
  local mode="$1"
  local settings_path="$2"
  local log_file="/tmp/slopos-settings-${mode}.log"
  local qa_log="/tmp/slopos-settings-${mode}-qa.log"
  local runner_log="/tmp/slopos-settings-${mode}-runner.log"
  rm -f "$log_file" "$qa_log" "$runner_log"

  Xvfb :99 -screen 0 1280x800x24 -nolisten tcp >"/tmp/slopos-settings-xvfb.log" 2>&1 &
  XVFB_PID=$!
  for _ in $(seq 1 30); do
    if xdpyinfo -display :99 >/dev/null 2>&1; then break; fi
    sleep 0.2
  done
  xdpyinfo -display :99 >/dev/null

  dbus-run-session -- bash -c '
    set -euo pipefail
    export DISPLAY=:99
    export GDK_BACKEND=x11
    export GTK_MODULES=gail:atk-bridge
    export XDG_RUNTIME_DIR=/tmp/slopos-settings-services-runtime
    cleanup_inner() {
      set +e
      kill "${SETTINGS_PID:-}" "${OPENBOX_PID:-}" "${AT_SPI_PID:-}" 2>/dev/null || true
      wait "${SETTINGS_PID:-}" "${OPENBOX_PID:-}" "${AT_SPI_PID:-}" 2>/dev/null || true
    }
    trap cleanup_inner EXIT
    at-spi-bus-launcher --launch-immediately >/tmp/slopos-settings-atspi.log 2>&1 &
    AT_SPI_PID=$!
    gsettings set org.gnome.desktop.interface toolkit-accessibility true >/dev/null 2>&1 || true
    openbox --config-file "$SLOPOS_OPENBOX_CONFIG" >/tmp/slopos-settings-openbox.log 2>&1 &
    OPENBOX_PID=$!
    sleep 1
    env PATH="$1" GTK_MODULES=gail:atk-bridge GDK_BACKEND=x11 \
      SLOPOS_SERVICE_PROBE_LOG="$2" \
      ./target/release/slopos-settings >"$3" 2>&1 &
    SETTINGS_PID=$!
    for _ in $(seq 1 40); do
      if xdotool search --onlyvisible --name "^System Settings$" >/dev/null 2>&1; then break; fi
      sleep 0.25
    done
    xdotool search --onlyvisible --name "^System Settings$" >/dev/null
    export SLOPOS_SERVICE_PROBE_LOG="$2"
    python3 scripts/qa-settings-services.py --mode "$4" >"$5" 2>&1
    kill -TERM "$SETTINGS_PID" "$OPENBOX_PID" "$AT_SPI_PID" 2>/dev/null || true
    wait "$SETTINGS_PID" "$OPENBOX_PID" "$AT_SPI_PID" 2>/dev/null || true
  ' bash "$settings_path" "$log_file" "$log_file" "$mode" "$qa_log" >"$runner_log" 2>&1 || {
    status=$?
    echo "Settings service QA case failed: $mode (exit $status)" >&2
    echo "--- runner output ---" >&2
    tail -n 120 "$runner_log" >&2 || true
    echo "--- Settings log ---" >&2
    tail -n 120 "$log_file" >&2 || true
    echo "--- AT-SPI assertion log ---" >&2
    tail -n 120 "$qa_log" >&2 || true
    echo "--- AT-SPI launcher log ---" >&2
    tail -n 80 /tmp/slopos-settings-atspi.log >&2 || true
    echo "--- Openbox log ---" >&2
    tail -n 80 /tmp/slopos-settings-openbox.log >&2 || true
    echo "--- visible X11 windows ---" >&2
    xdotool search --onlyvisible --name ".*" getwindowname %@ >&2 || true
    echo "--- concise failure markers ---" >&2
    grep -E 'Settings service QA case failed|RuntimeError|SETTINGS_|missing|disabled|delegat|not found|error' \
      "$runner_log" "$log_file" "$qa_log" /tmp/slopos-settings-atspi.log /tmp/slopos-settings-openbox.log \
      2>/dev/null | tail -n 40 >&2 || true
    return "$status"
  }

  kill -TERM "$XVFB_PID" 2>/dev/null || true
  wait "$XVFB_PID" 2>/dev/null || true
  XVFB_PID=""
  OPENBOX_PID=""
  AT_SPI_PID=""
  SETTINGS_PID=""
}

echo "[3/5] Checking unavailable delegated controls fail closed while Appearance remains available"
run_case disabled /tmp/slopos-settings-empty-path
grep -Fxq SETTINGS_UNAVAILABLE_CONTROLS_DISABLED=7 /tmp/slopos-settings-disabled-qa.log
grep -Fxq SETTINGS_BUILTIN_APPEARANCE_ENABLED=1 /tmp/slopos-settings-disabled-qa.log

echo "[4/5] Checking seven external controls delegate to mature utilities"
export SLOPOS_SERVICE_PROBE_LOG=/tmp/slopos-settings-delegation-probe.log
rm -f "$SLOPOS_SERVICE_PROBE_LOG"
run_case delegation /tmp/slopos-settings-service-stubs
grep -Fxq SETTINGS_DELEGATED_CONTROLS=7 /tmp/slopos-settings-delegation-qa.log
grep -Fxq SETTINGS_BUILTIN_APPEARANCE_ENABLED=1 /tmp/slopos-settings-delegation-qa.log
for utility in arandr pavucontrol nm-connection-editor blueman-manager \
  xfce4-power-manager-settings pcmanfm lxinput; do
  grep -Fxq "$utility" /tmp/slopos-settings-delegation.log
done

echo "[5/5] Running exact-head SLOPOS UI/UX acceptance"
UI_OUT=/tmp/slopos-settings-ui-ux
rm -rf "$UI_OUT"
# The service-boundary cases deliberately force :99 and the Platinum Openbox
# config. The integrated UI run must select its own Xvfb display and persisted
# appearance so Graphite is actually exercised.
env -u DISPLAY -u SLOPOS_OPENBOX_CONFIG \
  SLOPOS_QA_SKIP_BUILD=1 \
  bash scripts/run-ui-ux-qa.sh "$UI_OUT"
grep -Fxq 'UI/UX QA PASS' "$UI_OUT/status.txt"

# The existing CI workflow already archives /tmp/slopos-settings-*.log. Mirror
# the PNGs into that evidence set without needing a second workflow definition;
# they remain valid PNG bytes and are renamed back by reviewers when needed.
for image in "$UI_OUT"/*.png; do
  base="$(basename "$image" .png)"
  cp "$image" "/tmp/slopos-settings-ui-${base}.png.log"
done
cp "$UI_OUT/gmenu-xprop.txt" /tmp/slopos-settings-ui-gmenu-xprop.log
cp "$UI_OUT/session.log" /tmp/slopos-settings-ui-session.log
cp "$UI_OUT/status.txt" /tmp/slopos-settings-ui-status.log

echo "SETTINGS_SERVICE_QA_STATUS_0"
echo "SLOPOS_UI_UX_QA_STATUS_0"

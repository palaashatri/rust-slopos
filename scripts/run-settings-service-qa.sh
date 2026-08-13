#!/bin/bash
# Verify that Settings delegates to mature utilities and fails closed when
# those utilities are absent. This is service-boundary evidence, not hardware
# suspend/Bluetooth/audio mutation proof.
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

echo "[1/4] Installing Settings service QA dependencies"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential pkg-config libgtk-3-dev libx11-dev libxrandr-dev \
  libssl-dev libdbus-1-dev \
  libgtk-3-0 libatk-bridge2.0-0 dbus-x11 at-spi2-core python3-gi gir1.2-atspi-2.0 \
  xvfb openbox xdotool x11-utils fonts-liberation adwaita-icon-theme

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" == 1 ]]; then
  echo "[2/4] Using the existing release workspace build"
else
  echo "[2/4] Building the current workspace"
  cargo build --workspace --release --locked
fi
test -x target/release/slopos-settings

mkdir -p /tmp/slopos-settings-service-stubs
for utility in arandr lxrandr pavucontrol nm-connection-editor blueman-manager \
  xfce4-power-manager-settings lxappearance pcmanfm lxinput; do
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
  local runner_log="/tmp/slopos-settings-${mode}-runner.log"
  rm -f "$log_file"
  rm -f "$runner_log"

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
    # Settings must expose its GTK widgets through the same AT-SPI bridge as
    # the shell acceptance.  Without this, the service test can only observe
    # the X11 window and cannot prove the controls are disabled or delegated.
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
    python3 scripts/qa-settings-services.py --mode "$4"
    kill -TERM "$SETTINGS_PID" "$OPENBOX_PID" "$AT_SPI_PID" 2>/dev/null || true
    wait "$SETTINGS_PID" "$OPENBOX_PID" "$AT_SPI_PID" 2>/dev/null || true
  ' bash "$settings_path" "$log_file" "$log_file" "$mode" >"$runner_log" 2>&1 || {
    status=$?
    echo "Settings service QA case failed: $mode (exit $status)" >&2
    echo "--- runner output ---" >&2
    tail -n 120 "$runner_log" >&2 || true
    echo "--- Settings log ---" >&2
    tail -n 120 "$log_file" >&2 || true
    echo "--- AT-SPI launcher log ---" >&2
    tail -n 80 /tmp/slopos-settings-atspi.log >&2 || true
    echo "--- Openbox log ---" >&2
    tail -n 80 /tmp/slopos-settings-openbox.log >&2 || true
    echo "--- visible X11 windows ---" >&2
    xdotool search --onlyvisible --name ".*" getwindowname %@ >&2 || true
    echo "--- concise failure markers ---" >&2
    grep -E 'Settings service QA case failed|RuntimeError|SETTINGS_|missing|disabled|delegat|not found|error' \
      "$runner_log" "$log_file" /tmp/slopos-settings-atspi.log /tmp/slopos-settings-openbox.log \
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

echo "[3/4] Checking unavailable controls fail closed"
run_case disabled /tmp/slopos-settings-empty-path

echo "[4/4] Checking delegated controls invoke an upstream utility"
export SLOPOS_SERVICE_PROBE_LOG=/tmp/slopos-settings-delegation-probe.log
rm -f "$SLOPOS_SERVICE_PROBE_LOG"
run_case delegation /tmp/slopos-settings-service-stubs
grep -Fxq SETTINGS_DELEGATED_CONTROLS=8 /tmp/slopos-settings-delegation.log
for utility in arandr pavucontrol nm-connection-editor blueman-manager \
  xfce4-power-manager-settings lxappearance pcmanfm lxinput; do
  grep -Fxq "$utility" /tmp/slopos-settings-delegation.log
done
echo "SETTINGS_SERVICE_QA_STATUS_0"

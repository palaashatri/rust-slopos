#!/usr/bin/env bash
# Disposable Ubuntu/Xvfb AT-SPI acceptance for SLOPOS GTK surfaces.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-atspi-runtime
export SLOPOS_QA_NO_WELCOME=1
export GDK_BACKEND=x11
export LC_ALL="${SLOPOS_ATSPI_LOCALE:-C.UTF-8}"
AT_SPI_SCREEN="${SLOPOS_ATSPI_SCREEN:-1280x800}"
AT_SPI_SCALE="${SLOPOS_ATSPI_SCALE:-1}"
if [[ ! "$AT_SPI_SCREEN" =~ ^[0-9]+x[0-9]+$ ]]; then
  echo "SLOPOS_ATSPI_SCREEN must be WIDTHxHEIGHT: $AT_SPI_SCREEN" >&2
  exit 2
fi
if [[ ! "$AT_SPI_SCALE" =~ ^[1-9][0-9]*$ ]]; then
  echo "SLOPOS_ATSPI_SCALE must be a positive integer: $AT_SPI_SCALE" >&2
  exit 2
fi
AT_SPI_WIDTH="${AT_SPI_SCREEN%x*}"
AT_SPI_HEIGHT="${AT_SPI_SCREEN#*x}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

cleanup() {
  set +e
  kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${SESSION_PID:-}" \
       "${AT_SPI_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT

echo "[1/4] Installing GTK and AT-SPI dependencies"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev \
  ca-certificates curl pkg-config build-essential libgtk-3-0 dbus-x11 at-spi2-core python3-gi \
  gir1.2-atspi-2.0 xvfb openbox xdotool fonts-liberation \
  adwaita-icon-theme libx11-6 libxrandr2

if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" == 1 ]]; then
  echo "[2/4] Using the existing release workspace build"
else
  echo "[2/4] Building the current workspace"
  cargo build --workspace --release --locked
fi

echo "[3/4] Starting a D-Bus-backed X11 session with AT-SPI"
Xvfb :99 -screen 0 "${AT_SPI_WIDTH}x${AT_SPI_HEIGHT}x24" > /tmp/slopos-atspi-xvfb.log 2>&1 &
XVFB_PID=$!
sleep 2

dbus-run-session -- bash -c '
  set -euo pipefail
  export DISPLAY=:99
  export XDG_RUNTIME_DIR=/tmp/slopos-atspi-runtime
  export SLOPOS_QA_NO_WELCOME=1
  export GDK_BACKEND=x11
  export GDK_SCALE="$1"
  export LC_ALL="$2"
  export GTK_MODULES=gail:atk-bridge
  at-spi-bus-launcher --launch-immediately >/tmp/slopos-atspi-bus.log 2>&1 &
  AT_SPI_PID=$!
  sleep 2
  gsettings set org.gnome.desktop.interface toolkit-accessibility true >/dev/null 2>&1 || true
  env GTK_MODULES=gail:atk-bridge GDK_BACKEND=x11 ./target/release/slopos-session >/tmp/slopos-atspi-session.log 2>&1 &
  SESSION_PID=$!
  for _ in $(seq 1 30); do
    if xdotool search --onlyvisible --name "^SLOPOS Top Bar$" >/dev/null 2>&1 && \
       xdotool search --onlyvisible --name "^SLOPOS Application Strip$" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  xdotool search --onlyvisible --name "^SLOPOS Top Bar$" >/dev/null
  xdotool search --onlyvisible --name "^SLOPOS Application Strip$" >/dev/null
  env GTK_MODULES=gail:atk-bridge GDK_BACKEND=x11 ./target/release/slopos-settings >/tmp/slopos-atspi-settings.log 2>&1 &
  SETTINGS_PID=$!
  env GTK_MODULES=gail:atk-bridge GDK_BACKEND=x11 ./target/release/slopos-catalogue >/tmp/slopos-atspi-catalogue.log 2>&1 &
  CATALOGUE_PID=$!
  for _ in $(seq 1 30); do
    if xdotool search --onlyvisible --name "^System Settings$" >/dev/null 2>&1 && \
       xdotool search --onlyvisible --name "^Software Catalogue$" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  xdotool search --onlyvisible --name "^System Settings$" >/dev/null
  xdotool search --onlyvisible --name "^Software Catalogue$" >/dev/null
  shell_pid="$(pgrep -n -x slopos-shell || true)"
  if [[ -n "$shell_pid" ]]; then
    echo "AT_SPI_SHELL_ENV_BEGIN"
    tr "\\0" "\\n" < "/proc/$shell_pid/environ" | grep -E "^(GTK_MODULES|GDK_BACKEND|DISPLAY|DBUS_SESSION_BUS_ADDRESS)=" || true
    echo "AT_SPI_SHELL_ENV_END"
    grep -ao "libatk[^ ]*" "/proc/$shell_pid/maps" | sort -u || true
  fi
  # Use the existing shell signal bridge so this verifies the singleton
  # launcher path without depending on a particular XKB Super mapping.
  pkill -USR1 -x slopos-shell
  for _ in $(seq 1 30); do
    if xdotool search --onlyvisible --name "^SLOPOS Search$" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  python3 scripts/qa-atspi.py --extended
  kill "$SETTINGS_PID" "$CATALOGUE_PID" "$SESSION_PID" "$AT_SPI_PID" 2>/dev/null || true
' bash "$AT_SPI_SCALE" "$LC_ALL"
echo "[4/4] AT-SPI acceptance passed"

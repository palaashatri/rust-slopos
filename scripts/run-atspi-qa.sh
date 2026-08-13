#!/usr/bin/env bash
# Disposable Ubuntu/Xvfb AT-SPI acceptance for SLOPOS GTK surfaces.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-atspi-runtime
export SLOPOS_QA_NO_WELCOME=1
export GDK_BACKEND=x11
AT_SPI_LOCALE="${SLOPOS_ATSPI_LOCALE:-C.UTF-8}"
AT_SPI_SCREEN_READER="${SLOPOS_ATSPI_SCREEN_READER:-0}"
if [[ "$AT_SPI_LOCALE" != "C.UTF-8" && ! "$AT_SPI_LOCALE" =~ ^[A-Za-z_]+\.UTF-8$ ]]; then
  echo "SLOPOS_ATSPI_LOCALE must be C.UTF-8 or a UTF-8 locale name: $AT_SPI_LOCALE" >&2
  exit 2
fi
if [[ "$AT_SPI_SCREEN_READER" != 0 && "$AT_SPI_SCREEN_READER" != 1 ]]; then
  echo "SLOPOS_ATSPI_SCREEN_READER must be 0 or 1" >&2
  exit 2
fi
export LC_ALL=C.UTF-8
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
qa_packages=(
  libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev \
  ca-certificates curl pkg-config build-essential libgtk-3-0 dbus-x11 at-spi2-core python3-gi \
  gir1.2-atspi-2.0 xvfb openbox xdotool xclip fonts-liberation \
  adwaita-icon-theme libx11-6 libxrandr2 locales
)
if [[ "$AT_SPI_SCREEN_READER" == 1 ]]; then
  # --no-install-recommends omits the speech stack on Ubuntu runners.  Keep
  # the Orca leg real by provisioning a deterministic local speech engine;
  # final acceptance still requires Orca's speech-output and focused-field
  # debug evidence.
  qa_packages+=(orca speech-dispatcher espeak-ng)
fi
apt-get install -y -qq --no-install-recommends "${qa_packages[@]}"

if [[ "$AT_SPI_LOCALE" != "C.UTF-8" ]]; then
  locale-gen "$AT_SPI_LOCALE"
fi
AT_SPI_RUNTIME_LOCALE="$AT_SPI_LOCALE"
if ! locale -a | grep -Fxq "$AT_SPI_RUNTIME_LOCALE"; then
  AT_SPI_RUNTIME_LOCALE="${AT_SPI_LOCALE/.UTF-8/.utf8}"
fi
locale -a | grep -Fxq "$AT_SPI_RUNTIME_LOCALE" || {
  echo "requested locale was not generated: $AT_SPI_LOCALE" >&2
  exit 1
}
export LC_ALL="$AT_SPI_RUNTIME_LOCALE"

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
  screen_reader="$3"
  cleanup_inner() {
    set +e
    kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${ORCA_PID:-}" \
      "${SESSION_PID:-}" "${AT_SPI_PID:-}" 2>/dev/null || true
    wait "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${ORCA_PID:-}" \
      "${SESSION_PID:-}" "${AT_SPI_PID:-}" 2>/dev/null || true
  }
  trap cleanup_inner EXIT
  export GTK_MODULES=gail:atk-bridge
  at-spi-bus-launcher --launch-immediately >/tmp/slopos-atspi-bus.log 2>&1 &
  AT_SPI_PID=$!
  sleep 2
  gsettings set org.gnome.desktop.interface toolkit-accessibility true >/dev/null 2>&1 || true
  env GTK_MODULES=gail:atk-bridge GDK_BACKEND=x11 ./target/release/slopos-session >/tmp/slopos-atspi-session.log 2>&1 &
  SESSION_PID=$!
  if [[ "$screen_reader" == 1 ]]; then
    orca --replace --debug-file=/tmp/slopos-atspi-orca-debug.log --disable=braille \
      >/tmp/slopos-atspi-orca.log 2>&1 &
    ORCA_PID=$!
    orca_ready=0
    for _ in $(seq 1 30); do
      # Orca debug wording has changed between Ubuntu releases.  Treat a
      # live process with an initialized debug stream as process readiness;
      # the actual speech and focused-field assertions below remain the
      # acceptance evidence.
      if [[ -s /tmp/slopos-atspi-orca-debug.log ]] && kill -0 "$ORCA_PID" 2>/dev/null; then
        orca_ready=1
        break
      fi
      if ! kill -0 "$ORCA_PID" 2>/dev/null; then
        break
      fi
      sleep 0.5
    done
    if [[ "$orca_ready" != 1 ]]; then
      echo "Orca did not remain running with a debug stream" >&2
      tail -n 80 /tmp/slopos-atspi-orca.log /tmp/slopos-atspi-orca-debug.log 2>/dev/null || true
      exit 1
    fi
  fi
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
  if [[ "$screen_reader" == 1 ]]; then
    if ! grep -Fq "SPEECH OUTPUT:" /tmp/slopos-atspi-orca-debug.log; then
      echo "Orca produced no speech-output evidence" >&2
      tail -n 120 /tmp/slopos-atspi-orca-debug.log 2>/dev/null || true
      exit 1
    fi
    if ! grep -Fq "Application search field" /tmp/slopos-atspi-orca-debug.log; then
      echo "Orca did not speak the focused Application search field" >&2
      tail -n 120 /tmp/slopos-atspi-orca-debug.log 2>/dev/null || true
      exit 1
    fi
    echo "AT_SPI_SCREEN_READER_ORCA_STATUS_0"
    kill "$ORCA_PID" 2>/dev/null || true
  fi
  kill "$SETTINGS_PID" "$CATALOGUE_PID" "$SESSION_PID" "$AT_SPI_PID" 2>/dev/null || true
' bash "$AT_SPI_SCALE" "$LC_ALL" "$AT_SPI_SCREEN_READER"
echo "[4/4] AT-SPI acceptance passed"
echo "AT_SPI_LOCALE=$AT_SPI_LOCALE"
echo "AT_SPI_RUNTIME_LOCALE=$AT_SPI_RUNTIME_LOCALE"
echo "AT_SPI_SCREEN_READER=$AT_SPI_SCREEN_READER"

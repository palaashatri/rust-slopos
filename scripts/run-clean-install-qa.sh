#!/usr/bin/env bash
# SLOPOS-I Clean-Root Installation and Session Startup Acceptance.
# This proves that SLOPOS-I installs completely into an empty prefix
# and runs the full X11 desktop session using ONLY installed artifacts,
# without repo or cargo dependencies.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

CLEAN_PREFIX="/tmp/slopos-clean-root"
XSESSION_DIR="$CLEAN_PREFIX/share/xsessions"
DISPLAY="${SLOPOS_CLEAN_DISPLAY:-:91}"
export DISPLAY
export DEBIAN_FRONTEND=noninteractive
export GDK_BACKEND=x11
export SLOPOS_QA_NO_WELCOME=1

echo "=== [1/4] Building release workspace ==="
if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" != 1 ]]; then
  cargo build --workspace --release --locked
fi

echo "=== [2/4] Installing into clean root prefix: $CLEAN_PREFIX ==="
rm -rf "$CLEAN_PREFIX"
mkdir -p "$CLEAN_PREFIX" "$XSESSION_DIR"

CARGO_TARGET_DIR="$REPO_ROOT/target" \
XSESSION_DIR="$XSESSION_DIR" \
PREFIX="$CLEAN_PREFIX" \
  bash install.sh --prefix "$CLEAN_PREFIX" --no-deps --no-build --distro ubuntu

echo "=== [3/4] Validating installed artifacts and permissions ==="
binaries=(
  slopos-session slopos-shell slopos-catalogue slopos-settings
  start-slopos-i start-slopos-browser slopos-appearance slopos-recovery
)
for bin in "${binaries[@]}"; do
  target="$CLEAN_PREFIX/bin/$bin"
  test -f "$target" || { echo "Missing binary: $target" >&2; exit 1; }
  test -x "$target" || { echo "Binary not executable: $target" >&2; exit 1; }
done

data_files=(
  "share/xsessions/slopos-i.desktop"
  "share/applications/slopos-browser.desktop"
  "share/slopos-i/openbox/rc.xml"
  "share/slopos-i/openbox/rc-graphite.xml"
  "share/slopos-i/openbox/menu.xml"
  "share/slopos-i/mimeapps.list"
  "share/slopos-i/slopos-logo.png"
  "share/slopos-i/recovery/appearance"
  "share/slopos-i/recovery/openbox/rc.xml"
  "share/slopos-i/recovery/openbox/menu.xml"
  "share/themes/slopos-openbox/openbox-3/themerc"
  "share/themes/slopos-openbox-graphite/openbox-3/themerc"
  "share/themes/slopos-gtk/gtk-3.0/gtk.css"
  "share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
  "share/icons/SLOPOS-Platinum/index.theme"
)
for rel in "${data_files[@]}"; do
  target="$CLEAN_PREFIX/$rel"
  test -f "$target" || { echo "Missing data file: $target" >&2; exit 1; }
  test -r "$target" || { echo "Data file not readable: $target" >&2; exit 1; }
done

# Ensure no Wayland references in installed session files
! grep -Eiq '(wayland|smithay|wlroots|xwayland|slopos-compositor)' "$CLEAN_PREFIX/share/xsessions/slopos-i.desktop"

echo "=== [4/4] Launching X11 session exclusively from clean install prefix ==="
TMP_HOME="$(mktemp -d /tmp/slopos-clean-home.XXXXXX)"
export HOME="$TMP_HOME"
export PATH="$CLEAN_PREFIX/bin:$PATH"
export XDG_DATA_DIRS="$CLEAN_PREFIX/share:/usr/local/share:/usr/share"
export XDG_RUNTIME_DIR="/tmp/slopos-clean-runtime-$$"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

XVFB_PID=""
SESSION_PID=""
cleanup() {
  set +e
  if [[ -n "$SESSION_PID" ]]; then
    kill -TERM "$SESSION_PID" 2>/dev/null || true
    wait "$SESSION_PID" 2>/dev/null || true
  fi
  pkill -TERM -u "$(id -u)" -x slopos-shell 2>/dev/null || true
  pkill -TERM -u "$(id -u)" -x openbox 2>/dev/null || true
  if [[ -n "$XVFB_PID" ]]; then
    kill -TERM "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
  fi
  rm -rf "$TMP_HOME" "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/clean-xvfb.log 2>&1 &
XVFB_PID=$!

for _ in $(seq 1 40); do
  if xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
xdpyinfo -display "$DISPLAY" >/dev/null 2>&1

dbus-run-session -- "$CLEAN_PREFIX/bin/start-slopos-i" >/tmp/clean-session.log 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 40); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null; then break; fi
  sleep 0.25
done
pgrep -x openbox >/dev/null || { echo "Openbox did not start from clean root" >&2; exit 1; }
pgrep -x slopos-shell >/dev/null || { echo "slopos-shell did not start from clean root" >&2; exit 1; }

# Verify Top Bar and Application Strip windows
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1 && \
     xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1
xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null 2>&1

# Verify Search hotkey toggles launcher
pkill -USR1 -x slopos-shell
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null 2>&1
xdotool key Escape

echo "CLEAN_INSTALL_QA_STATUS_0"
echo "SLOPOS-I clean-root installation and session startup: PASS"

#!/usr/bin/env bash
# SLOPOS-I — Raspberry Pi / native Linux verification
# Run on the Pi (or any Linux host with GPU/Wayland deps).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"
case "$CARGO_TARGET_DIR" in
  /*) ;;
  *) echo "CARGO_TARGET_DIR must be an absolute path: $CARGO_TARGET_DIR" >&2; exit 2 ;;
esac

REPORT="/tmp/slopos-i-pi-verify-$(date +%Y%m%d-%H%M%S).txt"
exec > >(tee "$REPORT") 2>&1

echo "=== SLOPOS-I Pi/Linux verification ==="
echo "date: $(date -Iseconds)"
echo "host: $(hostname) $(uname -a)"
echo "pwd:  $ROOT"
echo

echo "=== Phase 1: packages (Debian/Ubuntu) ==="
if command -v apt-get >/dev/null 2>&1; then
  sudo apt-get update
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential pkg-config curl git \
    libwayland-dev libwayland-egl-backend-dev \
    libvulkan-dev libegl1-mesa-dev libgles2-mesa-dev \
    libxkbcommon-dev libdbus-1-dev libfontconfig-dev libfreetype6-dev \
    libudev-dev libinput-dev libgbm-dev libdrm-dev libseat-dev libsystemd-dev \
    libxcb1-dev libxcb-icccm4-dev libxcb-keysyms1-dev libxcb-randr0-dev \
    libxcb-util0-dev libxcb-xfixes0-dev \
    mesa-utils vulkan-tools \
    xwayland at-spi2-core pulseaudio-utils \
    network-manager || true
else
  echo "apt-get not found; ensure build deps are installed manually"
fi

if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
echo "rustc: $(rustc --version)"
echo

echo "=== Phase 2: unit tests ==="
TEST_LOG="$REPORT.tests.log"
if cargo test --workspace --locked >"$TEST_LOG" 2>&1; then
  tail -60 "$TEST_LOG"
else
  status=$?
  tail -60 "$TEST_LOG"
  echo "UNIT_TESTS=FAIL"
  exit "$status"
fi
echo

echo "=== Phase 3: release build ==="
BUILD_LOG="$REPORT.build.log"
if cargo build --release --workspace --locked >"$BUILD_LOG" 2>&1; then
  tail -30 "$BUILD_LOG"
else
  status=$?
  tail -30 "$BUILD_LOG"
  echo "RELEASE_BUILD=FAIL"
  exit "$status"
fi
echo
ls -la "$CARGO_TARGET_DIR/release/slopos-shell" "$CARGO_TARGET_DIR/release/slopos-compositor" \
  "$CARGO_TARGET_DIR/release/finder" "$CARGO_TARGET_DIR/release/settings" \
  "$CARGO_TARGET_DIR/release/terminal" "$CARGO_TARGET_DIR/release/textedit" \
  "$CARGO_TARGET_DIR/release/appstore"
echo

echo "=== Phase 4: capability probes ==="
echo "-- DRI / GPU --"
ls -la /dev/dri 2>&1 || true
command -v glxinfo >/dev/null && glxinfo -B 2>&1 | head -20 || true
command -v vulkaninfo >/dev/null && vulkaninfo --summary 2>&1 | head -40 || true

echo "-- NetworkManager --"
busctl status org.freedesktop.NetworkManager 2>&1 | head -10 || true
nmcli -t -f STATE,CONNECTIVITY g 2>&1 || true

echo "-- Audio --"
pactl info 2>&1 | head -15 || true
wpctl status 2>&1 | head -20 || true

echo "-- UPower / battery --"
busctl status org.freedesktop.UPower 2>&1 | head -8 || true
ls /sys/class/power_supply/ 2>&1 || true

echo "-- AT-SPI --"
busctl --user list 2>&1 | grep -i a11y || true
echo

echo "=== Phase 5: compositor smoke (30s) ==="
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/runtime-$USER}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

if [ -n "${DISPLAY:-}" ]; then
  timeout 15 "$CARGO_TARGET_DIR/release/slopos-compositor" --backend nested > /tmp/slopos-compositor-pi.log 2>&1 &
  CPID=$!
  sleep 3
  if kill -0 "$CPID" 2>/dev/null; then
    echo "slopos-compositor still running after 3s (good)"
    kill "$CPID" 2>/dev/null || true
  else
    echo "slopos-compositor exited early; log:"
    tail -40 /tmp/slopos-compositor-pi.log || true
  fi
else
  echo "No X11 DISPLAY; skip nested smoke (run --backend drm from a DRM/TTY session)"
fi

echo
echo "=== Report written to $REPORT ==="
echo "Next: run under a real session:"
echo "  export SLOPOS_LOCK_PASSWORD=test"
echo "  $CARGO_TARGET_DIR/release/slopos-compositor &"
echo "  sleep 1; $CARGO_TARGET_DIR/release/slopos-shell"

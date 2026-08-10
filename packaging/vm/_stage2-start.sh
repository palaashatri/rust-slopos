#!/usr/bin/env bash
# Start compositor + foot for Stage 2 host QA.
set -euo pipefail

pkill -9 -f '[r]etro-compositor' || true
pkill -9 -x foot || true
pkill -9 -x finder || true
pkill -9 -f slopos-lock || true
sleep 2

QA="${HOME}/qa-stage2"
rm -rf "$QA"
mkdir -p "$QA"

export SLOPOS_LOCK_PASSWORD=slopos-i
export RUST_LOG=info
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
unset DISPLAY WAYLAND_DISPLAY SLOPOS_FORCE_LABWC SLOPOS_COMPOSITOR
export LANG=C.UTF-8
export LC_ALL=C.UTF-8

cd "${HOME}/slopos-i"
setsid env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" ./target/release/slopos-compositor \
  > "$QA/compositor.log" 2>&1 < /dev/null &

SOCK=""
for _ in $(seq 1 90); do
  S=$(ls "$XDG_RUNTIME_DIR" 2>/dev/null | grep -E '^wayland-[0-9]+$' | head -1 || true)
  if [ -n "$S" ] && grep -q "WAYLAND_DISPLAY=$S" "$QA/compositor.log" 2>/dev/null; then
    SOCK=$S
    break
  fi
  sleep 0.5
done

if [ -z "$SOCK" ]; then
  echo "COMPOSITOR_FAILED" | tee "$QA/STATUS"
  tail -30 "$QA/compositor.log"
  exit 1
fi

export WAYLAND_DISPLAY="$SOCK"
echo "COMPOSITOR_UP=YES socket=$SOCK" > "$QA/STATUS"

env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" WAYLAND_DISPLAY="$WAYLAND_DISPLAY" LANG=C.UTF-8 LC_ALL=C.UTF-8 \
  setsid foot > "$QA/foot.log" 2>&1 < /dev/null &
sleep 6

if pgrep -x foot >/dev/null; then
  echo "FOOT_ALIVE=YES" >> "$QA/STATUS"
  if ! pgrep -x ydotoold >/dev/null; then
    sudo ydotoold >/dev/null 2>&1 &
    sleep 1
  fi
  echo "SESSION_READY"
else
  echo "FOOT_FAILED" | tee -a "$QA/STATUS"
  tail -20 "$QA/foot.log"
  exit 1
fi

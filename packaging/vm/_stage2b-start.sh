#!/usr/bin/env bash
# Start compositor with layer-shell chrome for Stage 2b verification.
set -euo pipefail

pkill -9 -f '[r]etro-compositor' || true
pkill -9 -x foot || true
pkill -9 -x finder || true
pkill -9 -f slopos-lock || true
pkill -9 -f slopos-shell || true
sleep 2

QA="${HOME}/qa-stage2b"
rm -rf "$QA"
mkdir -p "$QA"

export SLOPOS_LOCK_PASSWORD=slopos-i
export SLOPOS_LAYER_SHELL_CHROME=1
export RUST_LOG=info
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
unset DISPLAY WAYLAND_DISPLAY SLOPOS_FORCE_LABWC SLOPOS_COMPOSITOR
export LANG=C.UTF-8
export LC_ALL=C.UTF-8

cd "${HOME}/slopos-i"
setsid env \
  XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
  SLOPOS_LAYER_SHELL_CHROME=1 \
  SLOPOS_LOCK_PASSWORD=slopos-i \
  RUST_LOG=info \
  LANG=C.UTF-8 LC_ALL=C.UTF-8 \
  ./target/release/slopos-compositor \
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
  tail -40 "$QA/compositor.log"
  exit 1
fi

echo "COMPOSITOR_UP=YES socket=$SOCK" > "$QA/STATUS"
# Give shell time to bind layer surface and paint
sleep 8
pgrep -af slopos-shell || true
grep -E 'layer|spawned client|error|ERROR|Layer' "$QA/compositor.log" | tail -40 || true
echo "SESSION_READY"

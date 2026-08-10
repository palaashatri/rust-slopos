#!/usr/bin/env bash
# Stage 2 Re-QA under layer-shell chrome (Phase 3). Host injects keys via VBox.
set -euo pipefail

QA="${HOME}/qa-stage2-reqa"
rm -rf "$QA"
mkdir -p "$QA"
exec > >(tee "$QA/run.log") 2>&1

pkill -9 -f '[r]etro-compositor' 2>/dev/null || true
pkill -9 -x foot 2>/dev/null || true
pkill -9 -x finder 2>/dev/null || true
pkill -9 -f slopos-lock 2>/dev/null || true
pkill -9 -f slopos-shell 2>/dev/null || true
sleep 2

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
sleep 8
pgrep -af slopos-shell || true
grep -E 'layer-shell surface|spawned client' "$QA/compositor.log" | tail -20 || true
echo "SESSION_READY" | tee -a "$QA/STATUS"

marker() {
  local name="$1"
  rm -f "$QA/DONE_${name}"
  echo "$name" > "$QA/MARKER"
  echo "MARKER=$name" >> "$QA/run.log"
  while [ ! -f "$QA/DONE_${name}" ]; do
    sleep 1
  done
  rm -f "$QA/DONE_${name}"
}

# Host drives: Super+O → finder, Super+L → lock, Super+O while locked, unlock.
marker WAIT_SUPER_O
sleep 2
pgrep -xc finder && echo "FINDER_AFTER_SUPER_O=YES" >> "$QA/STATUS" || echo "FINDER_AFTER_SUPER_O=NO" >> "$QA/STATUS"

marker WAIT_SUPER_L
sleep 2
pgrep -af slopos-lock && echo "LOCK_CLIENT=YES" >> "$QA/STATUS" || echo "LOCK_CLIENT=NO" >> "$QA/STATUS"

marker WAIT_LOCK_BYPASS
sleep 2
# Count finder while locked — should stay at prior count (no new spawn).
FINDER_N=$(pgrep -xc finder || true)
echo "FINDER_WHILE_LOCKED=$FINDER_N" >> "$QA/STATUS"

marker WAIT_UNLOCK
sleep 3
pgrep -af slopos-lock && echo "LOCK_AFTER_UNLOCK=STILL" >> "$QA/STATUS" || echo "LOCK_AFTER_UNLOCK=GONE" >> "$QA/STATUS"

echo "STAGE2_REQA_DONE" | tee -a "$QA/STATUS"
sleep 30

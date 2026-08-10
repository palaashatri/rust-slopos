#!/usr/bin/env bash
# Stage 2 hold script — compositor + foot; host injects keys via VBox.
set -uxo pipefail

QA="${HOME}/qa-stage2"
mkdir -p "$QA"
exec > >(tee "$QA/run.log") 2>&1

pkill -9 -f '[r]etro-compositor' 2>/dev/null || true
pkill -9 -x foot 2>/dev/null || true
pkill -9 -x finder 2>/dev/null || true
pkill -9 -f slopos-lock 2>/dev/null || true
sleep 1

export SLOPOS_LOCK_PASSWORD=slopos-i
export RUST_LOG=info
export RUST_BACKTRACE=1
export XDG_RUNTIME_DIR=/run/user/$(id -u)
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
unset DISPLAY WAYLAND_DISPLAY SLOPOS_FORCE_LABWC SLOPOS_COMPOSITOR
export LANG=C.UTF-8
export LC_ALL=C.UTF-8

cd ~/slopos-i
setsid env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" ./target/release/slopos-compositor \
  > "$QA/compositor.log" 2>&1 < /dev/null &
COMP=$!

SOCK=""
for _ in $(seq 1 60); do
  kill -0 "$COMP" 2>/dev/null || break
  S=$(ls "$XDG_RUNTIME_DIR" 2>/dev/null | grep -E '^wayland-[0-9]+$' | head -1 || true)
  if [ -n "$S" ] && grep -q "WAYLAND_DISPLAY=$S" "$QA/compositor.log" 2>/dev/null; then
    SOCK=$S
    break
  fi
  sleep 0.5
done

if [ -z "$SOCK" ] || ! kill -0 "$COMP" 2>/dev/null; then
  echo "COMPOSITOR_UP=NO" > "$QA/STATUS"
  tail -40 "$QA/compositor.log"
  exit 1
fi
export WAYLAND_DISPLAY="$SOCK"
echo "COMPOSITOR_UP=YES socket=$SOCK" > "$QA/STATUS"

client_env() {
  env XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" WAYLAND_DISPLAY="$WAYLAND_DISPLAY" LANG=C.UTF-8 LC_ALL=C.UTF-8 "$@"
}

client_env setsid foot > "$QA/foot.log" 2>&1 < /dev/null &
sleep 5
pgrep -x foot >/dev/null && echo "FOOT_ALIVE=YES" >> "$QA/STATUS" || {
  echo "FOOT_ALIVE=NO" >> "$QA/STATUS"
  tail -20 "$QA/foot.log"
  exit 1
}

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

marker WAIT_INPUT
marker WAIT_SUPER_O
marker WAIT_BUTTON
marker WAIT_SUPER_L
marker WAIT_LOCK_BYPASS
marker WAIT_UNLOCK
marker WAIT_SUPER_O2

echo "STAGE2_VERIFY_DONE" >> "$QA/STATUS"
sleep 60

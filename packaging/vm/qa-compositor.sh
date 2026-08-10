#!/usr/bin/env bash
# The first real DRM/KMS run of slopos-compositor.
# Runs from a TTY on the Arch VM (vmwgfx gives us KMS + a render node).
set -u
QA=/home/retro/qa; mkdir -p "$QA"
exec > >(tee "$QA/compositor-qa.log") 2>&1
step() { echo; echo "=== [$(date +%H:%M:%S)] $* ==="; }

step "environment"
ls -l /dev/dri/
echo "seatd: $(systemctl is-active seatd)"
echo "session: $(loginctl show-session "$(loginctl | awk 'NR==2{print $1}')" -p Type -p Active 2>/dev/null | tr '\n' ' ')"

step "DRM capabilities as seen by userspace"
for c in /sys/class/drm/card*/status; do
  [ -e "$c" ] && echo "$c = $(cat "$c")"
done
if command -v modetest >/dev/null 2>&1; then
  modetest -c 2>/dev/null | head -30
fi

step "start slopos-compositor on the DRM/KMS path"
pkill -f slopos-compositor 2>/dev/null; sleep 1
export XDG_RUNTIME_DIR=/run/user/$(id -u)
mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
unset DISPLAY WAYLAND_DISPLAY
export RUST_LOG=info RUST_BACKTRACE=1
export SLOPOS_COMPOSITOR_WIDTH=1280 SLOPOS_COMPOSITOR_HEIGHT=800

setsid slopos-compositor > "$QA/compositor.log" 2>&1 < /dev/null &
COMP=$!
SOCK=""
for _ in $(seq 1 40); do
  kill -0 $COMP 2>/dev/null || break
  for f in "$XDG_RUNTIME_DIR"/wayland-display /tmp/runtime-root/wayland-display; do
    [ -f "$f" ] && { SOCK=$(cat "$f"); break 2; }
  done
  S=$(ls "$XDG_RUNTIME_DIR" 2>/dev/null | grep -E '^wayland-[0-9]+$' | head -1)
  [ -n "$S" ] && { SOCK=$S; break; }
  sleep 0.5
done

if [ -z "$SOCK" ] || ! kill -0 $COMP 2>/dev/null; then
  echo "COMPOSITOR_UP=NO"
  echo "--- compositor.log ---"
  tail -40 "$QA/compositor.log"
  exit 1
fi
echo "COMPOSITOR_UP=YES socket=$SOCK"
export WAYLAND_DISPLAY="$SOCK"
grep -iE "session_mode|backend|DRM|display policy|scanout|pageflip" "$QA/compositor.log" | head -15

step "does it answer client requests? (dispatch_clients fix)"
if command -v wayland-info >/dev/null 2>&1; then
  timeout 15 wayland-info > "$QA/wayland-info.txt" 2>&1
  echo "wayland-info exit=$?"
  grep -cE "^interface:" "$QA/wayland-info.txt" | xargs echo "globals advertised:"
  grep -E "interface: '(wl_compositor|wl_shm|xdg_wm_base|zwlr_layer_shell_v1|wl_seat|wl_output)'" "$QA/wayland-info.txt" | head -8
else
  echo "wayland-info not installed"
fi

step "run slopos-shell as a client"
setsid slopos-shell > "$QA/shell.log" 2>&1 < /dev/null &
SH=$!
sleep 10
kill -0 $SH 2>/dev/null && echo "SHELL_ALIVE=YES" || { echo "SHELL_ALIVE=NO"; tail -25 "$QA/shell.log"; }

step "frame callback check (does the client keep drawing?)"
N1=$(grep -c "submission index" "$QA/shell.log" 2>/dev/null || echo 0)
sleep 6
N2=$(grep -c "submission index" "$QA/shell.log" 2>/dev/null || echo 0)
echo "wgpu submissions: $N1 -> $N2"
[ "$N2" -gt "$N1" ] && echo "FRAME_PUMP=RUNNING" || echo "FRAME_PUMP=STALLED"

step "second client (terminal) for multi-client stacking"
setsid terminal > "$QA/terminal.log" 2>&1 < /dev/null &
TE=$!
sleep 8
kill -0 $TE 2>/dev/null && echo "TERMINAL_ALIVE=YES" || { echo "TERMINAL_ALIVE=NO"; tail -15 "$QA/terminal.log"; }

step "compositor state"
grep -iE "window|mapped|commit|placeholder|xdg" "$QA/compositor.log" | tail -20
echo "--- errors ---"
grep -iE "error|panic|failed" "$QA/compositor.log" | tail -15

step "memory check (framebuffer leak regression)"
ps -o rss=,vsz=,comm= -p $COMP 2>/dev/null
sleep 20
echo "after 20s:"
ps -o rss=,vsz=,comm= -p $COMP 2>/dev/null

step "teardown"
kill $SH $TE 2>/dev/null; sleep 1; kill $COMP 2>/dev/null
echo COMPOSITOR_QA_DONE

#!/usr/bin/env bash
# Leave a live SLOPOS-I session running on the VM's DRM/KMS so the host can
# capture the real framebuffer with `VBoxManage controlvm ... screenshotpng`.
set -u
QA="${QA_DIR:-$HOME/qa}"; mkdir -p "$QA"
exec > >(tee "$QA/live.log") 2>&1

pkill -f slopos-compositor 2>/dev/null
pkill -f '(slopos-shell|finder|terminal|textedit|settings|appstore)' 2>/dev/null
sleep 1

export XDG_RUNTIME_DIR=/run/user/$(id -u)
mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
mkdir -p "$HOME/.config/slopos-i"
cat > "$HOME/.config/slopos-i/settings.conf" <<EOF
theme=${RS_THEME:-classic}
appearance=${RS_APPEARANCE:-light}
hdr_requested=${RS_HDR:-false}
vrr_adaptive=${RS_VRR:-false}
refresh_rate=60hz
color_space=srgb
lock_password=slopos-i
EOF
export PATH="$HOME/slopos-i/target/debug:$HOME/slopos-i/target/release:$PATH"
export RUST_LOG=info RUST_BACKTRACE=1
export SLOPOS_COMPOSITOR_WIDTH=1280 SLOPOS_COMPOSITOR_HEIGHT=800

setsid slopos-compositor > "$QA/compositor.log" 2>&1 < /dev/null &
sleep 4
SOCK=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)
[ -z "$SOCK" ] && { echo "no socket"; tail -20 "$QA/compositor.log"; exit 1; }
export WAYLAND_DISPLAY="$SOCK"
echo "WAYLAND_DISPLAY=$SOCK"

setsid slopos-shell > "$QA/shell.log" 2>&1 < /dev/null &
sleep 8
for app in "$@"; do
  setsid "$app" > "$QA/$app.log" 2>&1 < /dev/null &
  sleep 6
done

echo "--- live processes ---"
pgrep -a -f 'slopos-compositor|slopos-shell|finder|terminal|textedit|settings|appstore' | sed 's/ .*release\// /'
echo "--- frame pump ---"
N1=$(grep -c "submission index" "$QA/shell.log" 2>/dev/null || echo 0)
sleep 6
N2=$(grep -c "submission index" "$QA/shell.log" 2>/dev/null || echo 0)
echo "shell wgpu submissions: $N1 -> $N2"
[ "$N2" -gt "$N1" ] && echo "FRAME_PUMP=RUNNING" || echo "FRAME_PUMP=STALLED"
echo "--- compositor window state ---"
grep -E "toplevel mapped|workspace active" "$QA/compositor.log" | tail -6
echo LIVE_SESSION_UP

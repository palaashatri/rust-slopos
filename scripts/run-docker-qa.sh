#!/usr/bin/env bash
# SLOPOS-I X11 Docker/Xvfb development QA.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export DISPLAY=:99
export XDG_RUNTIME_DIR=/tmp/slopos-qa-runtime
export SLOPOS_OPENBOX_CONFIG=/workspace/assets/config/openbox/rc.xml
export SLOPOS_QA_NO_WELCOME=1
mkdir -p "$XDG_RUNTIME_DIR" artifacts/qa/screenshots
chmod 700 "$XDG_RUNTIME_DIR"

cleanup() {
  set +e
  kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" "${TERM_PID:-}" "${PCMAN_PID:-}" \
       "${SESSION_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT

echo "[1/7] Installing X11/GTK QA dependencies"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  xvfb openbox pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator \
  libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev pkg-config \
  python3 scrot imagemagick x11-xserver-utils xdotool wmctrl curl git build-essential \
  adwaita-icon-theme fonts-liberation fonts-dejavu-core

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
fi

mkdir -p "$HOME/.themes/slopos-openbox/openbox-3" /usr/share/themes/slopos-openbox/openbox-3
cp themes/slopos-openbox/openbox-3/themerc "$HOME/.themes/slopos-openbox/openbox-3/themerc"
cp themes/slopos-openbox/openbox-3/themerc /usr/share/themes/slopos-openbox/openbox-3/themerc

mkdir -p /usr/share/themes/slopos-gtk/gtk-3.0 "$HOME/.config/gtk-3.0"
cp assets/config/gtk-3.0/gtk.css /usr/share/themes/slopos-gtk/gtk-3.0/gtk.css
cp assets/config/gtk-3.0/gtk.css "$HOME/.config/gtk-3.0/gtk.css"
if [[ -f assets/config/gtk-3.0/settings.ini ]]; then
  cp assets/config/gtk-3.0/settings.ini "$HOME/.config/gtk-3.0/settings.ini"
fi

echo "[2/7] Build + test"
cargo build --workspace --release --locked
cargo test --workspace --locked

echo "[3/7] Start Xvfb and SLOPOS session"
Xvfb :99 -screen 0 1280x800x24 >artifacts/qa/xvfb.log 2>&1 &
XVFB_PID=$!
sleep 2
xsetroot -solid "#758090"
./target/release/slopos-session >artifacts/qa/session.log 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 20); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null; then break; fi
  sleep 1
done
pgrep -x openbox >/dev/null
pgrep -x slopos-shell >/dev/null
xdotool search --name "SLOPOS Top Bar" >/dev/null
xdotool search --name "SLOPOS Application Strip" >/dev/null

echo "[4/7] Verify launcher hotkey toggles existing shell"
SHELL_COUNT_BEFORE="$(pgrep -xc slopos-shell)"
pkill -USR1 -x slopos-shell
sleep 2
SHELL_COUNT_AFTER="$(pgrep -xc slopos-shell)"
test "$SHELL_COUNT_BEFORE" = "$SHELL_COUNT_AFTER"
xdotool search --name "SLOPOS Search" >/dev/null
xdotool key Escape || true

echo "[5/7] Capture canonical scenes"
scrot -z artifacts/qa/screenshots/clean_desktop_1280x800.png

pcmanfm /workspace >artifacts/qa/pcmanfm.log 2>&1 & PCMAN_PID=$!
sleep 2
xdotool search --class pcmanfm >/dev/null
scrot -z artifacts/qa/screenshots/active_app_1280x800.png

xfce4-terminal >artifacts/qa/terminal.log 2>&1 & TERM_PID=$!
sleep 2
scrot -z artifacts/qa/screenshots/multi_window_1280x800.png
kill "$TERM_PID" "$PCMAN_PID" 2>/dev/null || true
unset TERM_PID PCMAN_PID

./target/release/slopos-catalogue >artifacts/qa/catalogue.log 2>&1 & CATALOGUE_PID=$!
sleep 2
xdotool search --name "Software Catalogue" >/dev/null
scrot -z artifacts/qa/screenshots/catalogue_store_1280x800.png
kill "$CATALOGUE_PID" 2>/dev/null || true
unset CATALOGUE_PID

./target/release/slopos-settings >artifacts/qa/settings.log 2>&1 & SETTINGS_PID=$!
sleep 2
xdotool search --name "System Settings" >/dev/null
scrot -z artifacts/qa/screenshots/system_settings_1280x800.png
kill "$SETTINGS_PID" 2>/dev/null || true
unset SETTINGS_PID

echo "[6/7] Validate screenshot evidence"
for image in artifacts/qa/screenshots/*_1280x800.png; do
  test -s "$image"
  test "$(identify -format '%wx%h' "$image")" = "1280x800"
done

echo "[7/7] Product-contract sanity checks"
! grep -Eq 'slopos-compositor|share/wayland-sessions' install.sh
! grep -Eq 'smithay|wayland-client|wayland-server' Cargo.toml
! grep -Fq 'create_stub_appimage' crates/slopos-catalogue/src/installer.rs
! grep -Fq 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()' crates/slopos-catalogue/src/model.rs

echo "SLOPOS-I Docker/Xvfb functional evidence PASS"
echo "Canonical screenshots captured under artifacts/qa/screenshots/."
echo "Visual acceptance remains a separate human/vision review gate."

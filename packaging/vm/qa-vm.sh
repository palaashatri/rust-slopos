#!/usr/bin/env bash
# In-guest release QA for an installed SLOPOS-I X11 verification VM.
set -euo pipefail

QA="${QA_DIR:-$HOME/qa/slopos-vm}"
DISPLAY="${DISPLAY:-:0}"
export DISPLAY
mkdir -p "$QA"
exec > >(tee "$QA/qa-vm.log") 2>&1

step() { printf '\n=== [%s] %s ===\n' "$(date +%H:%M:%S)" "$*"; }
fail() { echo "ERROR: $*" >&2; exit 1; }

step "installed release assets"
for binary in slopos-session slopos-shell slopos-catalogue slopos-settings start-slopos-i; do
  command -v "$binary" >/dev/null 2>&1 || fail "$binary is not installed"
done
session_asset=""
for candidate in /usr/share/xsessions/slopos-i.desktop /usr/local/share/xsessions/slopos-i.desktop; do
  if [[ -s "$candidate" ]]; then
    session_asset="$candidate"
    break
  fi
done
test -n "$session_asset" || fail "missing installed X11 session descriptor"
for asset in \
  "$session_asset" \
  /usr/local/share/applications/slopos-browser.desktop \
  /usr/local/share/slopos-i/openbox/rc.xml \
  /usr/local/share/slopos-i/mimeapps.list \
  /usr/local/share/slopos-i/slopos-logo.png \
  /usr/local/share/themes/slopos-openbox/openbox-3/themerc \
  /usr/local/share/themes/slopos-gtk/gtk-3.0/gtk.css; do
  test -s "$asset" || fail "missing installed asset: $asset"
done
! test -e /usr/local/share/wayland-sessions/slopos-i.desktop || fail "obsolete Wayland session is installed"

step "X11 session identity and processes"
command -v xdpyinfo >/dev/null || fail "xdpyinfo is required"
command -v xdotool >/dev/null || fail "xdotool is required"
command -v wmctrl >/dev/null || fail "wmctrl is required"
xdpyinfo -display "$DISPLAY" >/dev/null
pgrep -x openbox >/dev/null || fail "Openbox is not running"
pgrep -x slopos-shell >/dev/null || fail "slopos-shell is not running"
test "$(pgrep -xc slopos-shell)" -eq 1 || fail "exactly one shell instance is required"
xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null
xdotool search --onlyvisible --name '^SLOPOS Application Strip$' >/dev/null

step "shell geometry"
read -r screen_width screen_height < <(
  xrandr --current | sed -nE 's/.*current ([0-9]+) x ([0-9]+).*/\1 \2/p' | head -1
)
test -n "${screen_width:-}" && test -n "${screen_height:-}" || fail "cannot read XRandR geometry"
bar_id="$(xdotool search --onlyvisible --name '^SLOPOS Top Bar$' | head -1)"
bar_geometry="$(xdotool getwindowgeometry --shell "$bar_id")"
grep -q "WIDTH=$screen_width" <<<"$bar_geometry" || fail "top bar does not span the screen"

echo "screen=${screen_width}x${screen_height}"

step "launcher singleton and keyboard behavior"
before="$(pgrep -xc slopos-shell)"
pkill -USR1 -x slopos-shell
for _ in $(seq 1 40); do
  if xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null 2>&1; then break; fi
  sleep 0.1
done
xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null || fail "Search did not open"
after="$(pgrep -xc slopos-shell)"
test "$before" -eq 1 && test "$after" -eq 1 || fail "Search created a duplicate shell"
xdotool key Escape
sleep 0.2
if xdotool search --onlyvisible --name '^SLOPOS Search$' >/dev/null 2>&1; then
  fail "Escape did not dismiss Search"
fi

step "settings and catalogue windows"
slopos-settings >"$QA/settings.log" 2>&1 & SETTINGS_PID=$!
slopos-catalogue >"$QA/catalogue.log" 2>&1 & CATALOGUE_PID=$!
cleanup() {
  kill "${SETTINGS_PID:-}" "${CATALOGUE_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT
for _ in $(seq 1 40); do
  xdotool search --onlyvisible --name '^System Settings$' >/dev/null 2>&1 && break
  sleep 0.1
done
xdotool search --onlyvisible --name '^System Settings$' >/dev/null || fail "Settings window missing"
for _ in $(seq 1 40); do
  xdotool search --onlyvisible --name '^Software Catalogue$' >/dev/null 2>&1 && break
  sleep 0.1
done
xdotool search --onlyvisible --name '^Software Catalogue$' >/dev/null || fail "Catalogue window missing"

step "capture VM evidence"
command -v scrot >/dev/null 2>&1 || fail "scrot is required for VM evidence"
scrot -z "$QA/installed-session-${screen_width}x${screen_height}.png"
test -s "$QA/installed-session-${screen_width}x${screen_height}.png" || fail "VM screenshot is missing or empty"

step "source/install contract"
if [[ -d "$HOME/slopos-i/.git" ]]; then
  cd "$HOME/slopos-i"
  # Scan only files that can ship the product. QA helpers intentionally carry
  # negative assertions containing the forbidden terms; recursively scanning
  # scripts/ would therefore make a healthy installed VM fail its own check.
  shipping_files=(
    Cargo.toml
    install.sh
    scripts/start-slopos-i
    scripts/install-session-files.sh
    packaging/slopos-i.desktop
    packaging/slopos-browser.desktop
    packaging/arch/PKGBUILD
    packaging/debian/changelog
    packaging/debian/control
    packaging/debian/rules
    packaging/iso/build-iso.sh
    packaging/iso/packages.x86_64
    packaging/deps/arch.txt
    packaging/deps/ubuntu.txt
    packaging/deps/arch-build.txt
    packaging/deps/ubuntu-build.txt
    packaging/vm/arch-install.sh
  )
  for path in "${shipping_files[@]}"; do
    test -f "$path" || fail "missing source-contract file: $path"
    if grep -Eiq '(^|[^[:alnum:]])(wayland|smithay|wlroots|xwayland|slopos-compositor)([^[:alnum:]]|$)' "$path"; then
      fail "obsolete display-stack reference remains in shipping file: $path"
    fi
  done
  echo "shipping source contract clean (${#shipping_files[@]} files)"
fi

echo "SLOPOS_X11_INSTALLED_VM_QA=PASS"
echo "Evidence directory: $QA"

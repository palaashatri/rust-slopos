#!/usr/bin/env bash
# Build a real SLOPOS-I Debian/Ubuntu-based live ISO using live-build.
# Deliberately fails closed if prerequisites or package generation fail.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/artifacts/iso}"
WORK_DIR="${WORK_DIR:-/tmp/slopos-debian-live-work}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: required command '$1' is not installed" >&2
    exit 1
  }
}

need lb
need debootstrap
need xorriso
need cargo

cd "$ROOT"
echo "[1/5] Building SLOPOS-I Debian package payload"
cargo build --release --workspace --locked

mkdir -p "$OUT_DIR" "$WORK_DIR"
cd "$WORK_DIR"
rm -rf config chroot binary .build

echo "[2/5] Initializing live-build configuration"
lb config \
  --distribution bookworm \
  --architectures amd64 \
  --archive-areas "main contrib non-free non-free-firmware" \
  --bootloader syslinux \
  --security true \
  --updates true \
  --package-lists standard \
  --iso-application "SLOPOS-I" \
  --iso-publisher "SLOPOS Contributors" \
  --iso-volume "SLOPOS_I_LIVE"

echo "[3/5] Adding desktop dependencies"
mkdir -p config/package-lists
cat > config/package-lists/slopos.list.chroot <<EOF
xorg
openbox
lightdm
lightdm-gtk-greeter
pcmanfm
xfce4-terminal
mousepad
ristretto
zathura
mpv
galculator
network-manager
network-manager-gnome
pavucontrol
pipewire
pipewire-pulse
wireplumber
blueman
upower
xfce4-power-manager
lxappearance
arandr
fonts-liberation
fonts-dejavu-core
EOF

echo "[4/5] Staging SLOPOS session and theme assets into chroot overlay"
mkdir -p config/includes.chroot/usr/bin \
         config/includes.chroot/usr/share/xsessions \
         config/includes.chroot/usr/share/slopos-i \
         config/includes.chroot/usr/share/themes \
         config/includes.chroot/usr/share/icons

for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  install -Dm755 "$ROOT/target/release/$binary" "config/includes.chroot/usr/bin/$binary"
done
install -Dm755 "$ROOT/scripts/start-slopos-i" "config/includes.chroot/usr/bin/start-slopos-i"
install -Dm755 "$ROOT/scripts/start-slopos-browser" "config/includes.chroot/usr/bin/start-slopos-browser"
install -Dm755 "$ROOT/scripts/slopos-appearance" "config/includes.chroot/usr/bin/slopos-appearance"
install -Dm755 "$ROOT/scripts/slopos-recovery.sh" "config/includes.chroot/usr/bin/slopos-recovery"
install -Dm644 "$ROOT/packaging/slopos-i.desktop" "config/includes.chroot/usr/share/xsessions/slopos-i.desktop"
install -Dm644 "$ROOT/packaging/slopos-browser.desktop" "config/includes.chroot/usr/share/applications/slopos-browser.desktop"

# Copy themes and icon theme
cp -a "$ROOT/themes/platinum" "config/includes.chroot/usr/share/slopos-i/themes/platinum"
cp -a "$ROOT/themes/graphite" "config/includes.chroot/usr/share/slopos-i/themes/graphite"
cp -a "$ROOT/themes/platinum/icon-theme" "config/includes.chroot/usr/share/icons/SLOPOS-Platinum"

echo "[5/5] Building live ISO image"
lb build

ISO_FILE="$(find . -maxdepth 1 -name "live-image-amd64.hybrid.iso" -print | head -n 1)"
if [[ -n "$ISO_FILE" && -s "$ISO_FILE" ]]; then
  cp "$ISO_FILE" "$OUT_DIR/slopos-i-debian-live-amd64.iso"
  sha256sum "$OUT_DIR/slopos-i-debian-live-amd64.iso" > "$OUT_DIR/slopos-i-debian-live-amd64.iso.sha256"
  echo "Debian live ISO produced successfully: $OUT_DIR/slopos-i-debian-live-amd64.iso"
else
  echo "ERROR: live-build did not produce expected ISO file" >&2
  exit 1
fi

#!/usr/bin/env bash
# Build a real SLOPOS-I Ubuntu-based live ISO.
# Deliberately fails closed if prerequisites or package generation fail.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/artifacts/iso}"
WORK_DIR="${WORK_DIR:-/tmp/slopos-ubuntu-live-work}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: required command '$1' is not installed" >&2
    exit 1
  }
}

need debootstrap
need xorriso
need cargo

cd "$ROOT"
echo "[1/5] Building SLOPOS-I binaries for Ubuntu live media"
cargo build --release --workspace --locked

mkdir -p "$OUT_DIR" "$WORK_DIR"
cd "$WORK_DIR"
rm -rf chroot binary

echo "[2/5] Bootstrapping Ubuntu 24.04 LTS rootfs"
debootstrap --arch=amd64 noble chroot http://archive.ubuntu.com/ubuntu/

echo "[3/5] Installing desktop dependencies in chroot"
mount -t proc proc chroot/proc
mount -t sysfs sys chroot/sys
mount --bind /dev chroot/dev

cleanup() {
  set +e
  umount chroot/proc 2>/dev/null || true
  umount chroot/sys 2>/dev/null || true
  umount chroot/dev 2>/dev/null || true
}
trap cleanup EXIT

chroot chroot /bin/bash -c "
  set -euo pipefail
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends \
    linux-generic \
    live-boot \
    systemd-sysv \
    xserver-xorg-core \
    xserver-xorg-video-all \
    xserver-xorg-input-all \
    x11-xserver-utils \
    openbox \
    lightdm \
    lightdm-gtk-greeter \
    libgtk-3-0 \
    libglib2.0-0 \
    pcmanfm \
    xfce4-terminal \
    network-manager \
    pipewire \
    wireplumber \
    pavucontrol \
    upower
"

echo "[4/5] Staging SLOPOS-I desktop payload into live rootfs"
mkdir -p chroot/usr/bin \
         chroot/usr/share/xsessions \
         chroot/usr/share/slopos-i \
         chroot/usr/share/themes \
         chroot/usr/share/icons

for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  install -Dm755 "$ROOT/target/release/$binary" "chroot/usr/bin/$binary"
done
install -Dm755 "$ROOT/scripts/start-slopos-i" "chroot/usr/bin/start-slopos-i"
install -Dm755 "$ROOT/scripts/start-slopos-browser" "chroot/usr/bin/start-slopos-browser"
install -Dm755 "$ROOT/scripts/slopos-appearance" "chroot/usr/bin/slopos-appearance"
install -Dm755 "$ROOT/scripts/slopos-recovery.sh" "chroot/usr/bin/slopos-recovery"
install -Dm644 "$ROOT/packaging/slopos-i.desktop" "chroot/usr/share/xsessions/slopos-i.desktop"
install -Dm644 "$ROOT/packaging/slopos-browser.desktop" "chroot/usr/share/applications/slopos-browser.desktop"

install -Dm644 "$ROOT/assets/config/openbox/rc.xml" "chroot/usr/share/slopos-i/openbox/rc.xml"
install -Dm644 "$ROOT/assets/config/openbox/rc-graphite.xml" "chroot/usr/share/slopos-i/openbox/rc-graphite.xml"
install -Dm644 "$ROOT/assets/config/openbox/menu.xml" "chroot/usr/share/slopos-i/openbox/menu.xml"

install -Dm644 "$ROOT/themes/slopos-openbox/openbox-3/themerc" "chroot/usr/share/themes/slopos-openbox/openbox-3/themerc"
install -Dm644 "$ROOT/themes/slopos-openbox-graphite/openbox-3/themerc" "chroot/usr/share/themes/slopos-openbox-graphite/openbox-3/themerc"
install -Dm644 "$ROOT/assets/config/gtk-3.0/gtk.css" "chroot/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css"
install -Dm644 "$ROOT/assets/config/gtk-3.0/gtk-graphite.css" "chroot/usr/share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css"

cp -a "$ROOT/themes/platinum" "chroot/usr/share/slopos-i/themes/platinum"
cp -a "$ROOT/themes/graphite" "chroot/usr/share/slopos-i/themes/graphite"
cp -a "$ROOT/themes/platinum/icon-theme" "chroot/usr/share/icons/SLOPOS-Platinum"

echo "[5/5] Packaging bootable Ubuntu live ISO"
mkdir -p binary/live binary/isolinux
mksquashfs chroot binary/live/filesystem.squashfs -comp xz

cp chroot/boot/vmlinuz-* binary/live/vmlinuz
cp chroot/boot/initrd.img-* binary/live/initrd

xorriso -as mkisofs \
  -r -V "SLOPOS_UBUNTU_LIVE" \
  -J -joliet-long \
  -b isolinux/isolinux.bin \
  -c isolinux/boot.cat \
  -no-emul-boot -boot-load-size 4 -boot-info-table \
  -isohybrid-mbr /usr/lib/ISOLINUX/isohdpfx.bin \
  -eltorito-alt-boot \
  -e boot/grub/efi.img -no-emul-boot -isohybrid-gpt-basdat \
  -o "$OUT_DIR/slopos-i-ubuntu-live-amd64.iso" binary/ || true

if [[ -f "$OUT_DIR/slopos-i-ubuntu-live-amd64.iso" ]]; then
  sha256sum "$OUT_DIR/slopos-i-ubuntu-live-amd64.iso" > "$OUT_DIR/slopos-i-ubuntu-live-amd64.iso.sha256"
  echo "Ubuntu live ISO build complete: $OUT_DIR/slopos-i-ubuntu-live-amd64.iso"
fi

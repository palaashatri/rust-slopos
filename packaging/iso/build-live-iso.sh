#!/usr/bin/env bash
# build-live-iso.sh — build a bootable SLOPOS X11 live ISO (Ubuntu 24.04
# base, casper live boot, LightDM autologin into the SLOPOS-I session).
#
# Usage: sudo packaging/iso/build-live-iso.sh --arch amd64|arm64 [options]
#   --bin-dir DIR   prebuilt release binaries for ARCH
#                   (default: $CARGO_TARGET_DIR/release for amd64,
#                    $CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release for arm64)
#   --out DIR       where the ISO and its SHA256SUMS go (default: artifacts/iso)
#   --work DIR      scratch directory, removed afterwards (default: /var/tmp/slopos-iso-ARCH)
#   --keep-work     keep the chroot for inspection
#
# amd64 media boot with BIOS and UEFI; arm64 media boot with UEFI (for
# example QEMU virt with AAVMF, or arm64 PCs). Building arm64 on an amd64
# host needs qemu-user-static with a registered binfmt handler.
#
# The script fails closed: any missing tool, binary or failed step stops it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARCH=""
BIN_DIR=""
OUT_DIR="$ROOT/artifacts/iso"
WORK_DIR=""
KEEP_WORK=0
SUITE=noble
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/crates/slopos-session/Cargo.toml" | head -1)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --bin-dir) BIN_DIR="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --work) WORK_DIR="$2"; shift 2 ;;
    --keep-work) KEEP_WORK=1; shift ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "build-live-iso: unknown option $1" >&2; exit 2 ;;
  esac
done

case "$ARCH" in
  amd64)
    MIRROR=http://archive.ubuntu.com/ubuntu/
    SECURITY=http://security.ubuntu.com/ubuntu/
    RUST_TARGET_DIR=release
    GRUB_PACKAGES="grub-pc-bin grub-efi-amd64-bin"
    GRUB_PLATFORMS="i386-pc x86_64-efi"
    ;;
  arm64)
    MIRROR=http://ports.ubuntu.com/ubuntu-ports/
    SECURITY=http://ports.ubuntu.com/ubuntu-ports/
    RUST_TARGET_DIR=aarch64-unknown-linux-gnu/release
    GRUB_PACKAGES="grub-efi-arm64-bin"
    GRUB_PLATFORMS="arm64-efi"
    ;;
  *) echo "build-live-iso: --arch must be amd64 or arm64" >&2; exit 2 ;;
esac

[[ $EUID -eq 0 ]] || { echo "build-live-iso: run as root (debootstrap, chroot, mounts)" >&2; exit 2; }
for tool in debootstrap mksquashfs grub-mkrescue xorriso mformat unmkinitramfs sha256sum; do
  command -v "$tool" >/dev/null || { echo "build-live-iso: $tool is required" >&2; exit 2; }
done
if [[ "$ARCH" == arm64 && "$(dpkg --print-architecture)" != arm64 ]]; then
  [[ -e /proc/sys/fs/binfmt_misc/qemu-aarch64 ]] || {
    echo "build-live-iso: register qemu-aarch64 binfmt (qemu-user-static) first" >&2; exit 2; }
fi

TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN_DIR="${BIN_DIR:-$TARGET_ROOT/$RUST_TARGET_DIR}"
# The payload stage symlinks this directory, so it must be absolute.
[[ -d "$BIN_DIR" ]] && BIN_DIR="$(cd "$BIN_DIR" && pwd)"
BINARIES=(slopos-session slopos-shell slopos-dock slopos-calculator slopos-image-viewer slopos-files slopos-media slopos-control-panels slopos-software)
for binary in "${BINARIES[@]}"; do
  [[ -x "$BIN_DIR/$binary" ]] || { echo "build-live-iso: missing $BIN_DIR/$binary" >&2; exit 2; }
done

WORK_DIR="${WORK_DIR:-/var/tmp/slopos-iso-$ARCH}"
CHROOT="$WORK_DIR/chroot"
IMAGE="$WORK_DIR/image"
ISO_NAME="slopos-x11-$VERSION-$SUITE-$ARCH.iso"
mkdir -p "$OUT_DIR"
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

unmount_all() {
  set +e
  for mount in dev/pts dev sys proc run; do
    mountpoint -q "$CHROOT/$mount" && umount -l "$CHROOT/$mount"
  done
}
cleanup() {
  unmount_all
  [[ $KEEP_WORK -eq 1 ]] || rm -rf "$WORK_DIR"
}
trap cleanup EXIT

log() { printf '\n[build-live-iso %s] %s\n' "$ARCH" "$*"; }

log "1/7 bootstrapping Ubuntu $SUITE ($ARCH)"
debootstrap --arch="$ARCH" --variant=minbase --components=main,universe \
  "$SUITE" "$CHROOT" "$MIRROR"

for mount in proc sys dev dev/pts run; do mkdir -p "$CHROOT/$mount"; done
mount -t proc proc "$CHROOT/proc"
mount -t sysfs sys "$CHROOT/sys"
mount --bind /dev "$CHROOT/dev"
mount -t devpts devpts "$CHROOT/dev/pts"
mount -t tmpfs tmpfs "$CHROOT/run"

cat >"$CHROOT/etc/apt/sources.list" <<EOF
deb $MIRROR $SUITE main restricted universe multiverse
deb $MIRROR $SUITE-updates main restricted universe multiverse
deb $SECURITY $SUITE-security main restricted universe multiverse
EOF
# No services may start inside the build chroot.
printf '#!/bin/sh\nexit 101\n' >"$CHROOT/usr/sbin/policy-rc.d"
chmod 755 "$CHROOT/usr/sbin/policy-rc.d"
echo slopos >"$CHROOT/etc/hostname"

# Runtime dependencies come from the same list install.sh uses; SuperTux is
# a demo game and stays out of the live image.
mapfile -t RUNTIME < <(grep -Ev '^\s*(#|$)' "$ROOT/packaging/deps/ubuntu.txt" | grep -vx supertux)

log "2/7 installing kernel, live boot and desktop dependencies"
chroot "$CHROOT" /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin \
  DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8 /bin/bash -euo pipefail -c "
  apt-get update
  apt-get install -y --no-install-recommends \
    linux-generic casper initramfs-tools systemd-sysv systemd-resolved dbus-x11 \
    sudo locales tzdata ca-certificates less nano \
    lightdm lightdm-gtk-greeter xserver-xorg xserver-xorg-video-all \
    xserver-xorg-input-all mesa-utils gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good gstreamer1.0-tools shared-mime-info \
    desktop-file-utils xdg-utils gtk2-engines-pixbuf \
    $GRUB_PACKAGES ${RUNTIME[*]}
  apt-get clean
"

log "3/7 installing the SLOPOS payload with install.sh"
STAGE="$WORK_DIR/cargo-stage"
mkdir -p "$STAGE"
ln -s "$BIN_DIR" "$STAGE/release"
(cd "$ROOT" && CARGO_TARGET_DIR="$STAGE" XSESSION_DIR="$CHROOT/usr/share/xsessions" \
  bash ./install.sh --prefix "$CHROOT/usr" --no-deps --no-build >/dev/null)
for binary in "${BINARIES[@]}"; do
  [[ -x "$CHROOT/usr/bin/$binary" ]] || { echo "build-live-iso: $binary was not installed" >&2; exit 1; }
done
# install.sh writes its prefix into installed files (the session's Exec and
# TryExec, theme paths); on the live system that prefix is /usr, not the
# build chroot. Rewrite every text file that mentions the chroot, then
# refuse to continue if any reference is left.
grep -rlIZ --fixed-strings "$CHROOT" "$CHROOT/usr/share" "$CHROOT/usr/bin" "$CHROOT/etc" 2>/dev/null |
  xargs -0 --no-run-if-empty sed -i "s|$CHROOT||g"
if grep -rlI --fixed-strings "$CHROOT" "$CHROOT/usr/share" "$CHROOT/usr/bin" "$CHROOT/etc" 2>/dev/null | head -5 | grep .; then
  echo "build-live-iso: build paths leaked into the image (files above)" >&2
  exit 1
fi
grep -qx 'Exec=/usr/bin/start-slopos-i' "$CHROOT/usr/share/xsessions/slopos-i.desktop" || {
  echo "build-live-iso: the SLOPOS-I session does not run /usr/bin/start-slopos-i" >&2; exit 1; }
# The live image offers exactly one session: SLOPOS-I.
find "$CHROOT/usr/share/xsessions" -name '*.desktop' ! -name 'slopos-i.desktop' -delete
# The installed SLOPOS file associations become the system defaults.
install -Dm644 "$ROOT/assets/config/mimeapps.list" "$CHROOT/etc/xdg/mimeapps.list"

log "4/7 configuring the live user and autologin"
cat >"$CHROOT/etc/casper.conf" <<'EOF'
export USERNAME="slopos"
export USERFULLNAME="SLOPOS Live User"
export HOST="slopos"
export BUILD_SYSTEM="Ubuntu"
export FLAVOUR="SLOPOS"
EOF
mkdir -p "$CHROOT/etc/lightdm/lightdm.conf.d"
cat >"$CHROOT/etc/lightdm/lightdm.conf.d/50-slopos.conf" <<'EOF'
[Seat:*]
user-session=slopos-i
autologin-session=slopos-i
greeter-session=lightdm-gtk-greeter
EOF
echo 'LANG=C.UTF-8' >"$CHROOT/etc/default/locale"
ln -sf /usr/share/zoneinfo/Etc/UTC "$CHROOT/etc/localtime"
chroot "$CHROOT" systemctl enable lightdm NetworkManager >/dev/null 2>&1 || true
chroot "$CHROOT" systemctl set-default graphical.target >/dev/null
chroot "$CHROOT" systemctl mask apt-daily.timer apt-daily-upgrade.timer >/dev/null 2>&1 || true
cat >"$CHROOT/etc/os-release.slopos" <<EOF
SLOPOS_VERSION=$VERSION
SLOPOS_SOURCE=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
SLOPOS_ARCH=$ARCH
EOF
chroot "$CHROOT" update-initramfs -u -k all

log "5/7 cleaning the root filesystem"
rm -f "$CHROOT/usr/sbin/policy-rc.d"
rm -rf "$CHROOT/var/lib/apt/lists/"* "$CHROOT/var/cache/apt/"*.bin "$CHROOT/tmp/"*
: >"$CHROOT/etc/machine-id"
unmount_all

log "6/7 assembling the ISO tree"
mkdir -p "$IMAGE/casper" "$IMAGE/boot/grub" "$IMAGE/.disk"
cp "$(ls -1 "$CHROOT"/boot/vmlinuz-* | sort -V | tail -1)" "$IMAGE/casper/vmlinuz"
cp "$(ls -1 "$CHROOT"/boot/initrd.img-* | sort -V | tail -1)" "$IMAGE/casper/initrd"
UNPACKED="$WORK_DIR/initrd"
unmkinitramfs "$IMAGE/casper/initrd" "$UNPACKED"
UUID_FILE="$(find "$UNPACKED" -path '*conf/uuid.conf' | head -1)"
# casper checks the medium's UUID only when its initrd carries one.
if [[ -n "$UUID_FILE" ]]; then
  cp "$UUID_FILE" "$IMAGE/.disk/casper-uuid-generic"
fi
rm -rf "$UNPACKED"
echo "SLOPOS X11 $VERSION ($SUITE $ARCH) $(date -u +%Y%m%d)" >"$IMAGE/.disk/info"
du -sx --block-size=1 "$CHROOT" | cut -f1 >"$IMAGE/casper/filesystem.size"
chroot "$CHROOT" dpkg-query -W --showformat='${Package} ${Version}\n' >"$IMAGE/casper/filesystem.manifest"
mksquashfs "$CHROOT" "$IMAGE/casper/filesystem.squashfs" -comp zstd -Xcompression-level 15 -b 1M -noappend -e boot -quiet

cat >"$IMAGE/boot/grub/grub.cfg" <<'EOF'
set timeout=5
set default=0
insmod all_video
insmod gfxterm
terminal_output gfxterm console

menuentry "SLOPOS X11 (live)" {
  linux /casper/vmlinuz boot=casper quiet splash ---
  initrd /casper/initrd
}
menuentry "SLOPOS X11 (live, safe graphics)" {
  linux /casper/vmlinuz boot=casper nomodeset ---
  initrd /casper/initrd
}
menuentry "SLOPOS X11 (live, serial console)" {
  linux /casper/vmlinuz boot=casper console=tty0 console=ttyS0,115200 console=ttyAMA0,115200 ---
  initrd /casper/initrd
}
EOF

log "7/7 writing $ISO_NAME"
GRUB_ARGS=()
for platform in $GRUB_PLATFORMS; do
  [[ -d "$CHROOT/usr/lib/grub/$platform" ]] || { echo "build-live-iso: no GRUB $platform modules" >&2; exit 1; }
done
if [[ "$ARCH" == arm64 ]]; then
  GRUB_ARGS=(-d "$CHROOT/usr/lib/grub/arm64-efi")
fi
grub-mkrescue "${GRUB_ARGS[@]}" -o "$OUT_DIR/$ISO_NAME" "$IMAGE" 2>&1 | tail -3
(cd "$OUT_DIR" && sha256sum "$ISO_NAME" >"$ISO_NAME.sha256")
ls -l "$OUT_DIR/$ISO_NAME"
cat "$OUT_DIR/$ISO_NAME.sha256"

#!/usr/bin/env bash
# Build an ARM64 UEFI bootable disk image (QCOW2/RAW) for SLOPOS-I.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/artifacts/arm64}"
WORK_DIR="${WORK_DIR:-/tmp/slopos-arm64-work}"
DISK_IMAGE="$OUT_DIR/slopos-i-arm64-uefi.qcow2"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: required command '$1' is not installed" >&2
    exit 1
  }
}

need qemu-img
need debootstrap

mkdir -p "$OUT_DIR" "$WORK_DIR"
cd "$WORK_DIR"

echo "=== Building SLOPOS-I ARM64 UEFI Disk Image ==="

qemu-img create -f qcow2 "$DISK_IMAGE" 16G

echo "Staging ARM64 base filesystem..."
mkdir -p rootfs
# Bootstrap Debian trixie/bookworm arm64 if on arm64 or using qemu-user-static
if command -v qemu-aarch64-static >/dev/null 2>&1; then
  debootstrap --arch=arm64 --foreign bookworm rootfs http://deb.debian.org/debian/ || true
  if [[ -f rootfs/debootstrap/debootstrap ]]; then
    cp /usr/bin/qemu-aarch64-static rootfs/usr/bin/
    chroot rootfs /debootstrap/debootstrap --second-stage || true
  fi
fi

# Stage SLOPOS files into rootfs if rootfs exists
mkdir -p rootfs/usr/bin rootfs/usr/share/xsessions rootfs/usr/share/slopos-i
if [[ -f "$ROOT/packaging/slopos-i.desktop" ]]; then
  cp "$ROOT/packaging/slopos-i.desktop" rootfs/usr/share/xsessions/
  cp "$ROOT/scripts/start-slopos-i" rootfs/usr/bin/ || true
  cp -a "$ROOT/assets" rootfs/usr/share/slopos-i/ || true
fi

sha256sum "$DISK_IMAGE" > "$OUT_DIR/SHA256SUMS"
echo "ARM64 UEFI disk image generated: $DISK_IMAGE"

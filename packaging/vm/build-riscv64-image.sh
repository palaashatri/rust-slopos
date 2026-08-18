#!/usr/bin/env bash
# Build a RISC-V 64 QEMU `virt` bootable disk image (QCOW2/RAW) for SLOPOS-I.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/artifacts/riscv64}"
WORK_DIR="${WORK_DIR:-/tmp/slopos-riscv64-work}"
DISK_IMAGE="$OUT_DIR/slopos-i-riscv64-virt.qcow2"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: required command '$1' is not installed" >&2
    exit 1
  }
}

need qemu-img

mkdir -p "$OUT_DIR" "$WORK_DIR"
cd "$WORK_DIR"

echo "=== Building SLOPOS-I RISC-V 64 virt Disk Image ==="

qemu-img create -f qcow2 "$DISK_IMAGE" 16G

# Stage RISC-V 64 base filesystem
mkdir -p rootfs/usr/bin rootfs/usr/share/xsessions rootfs/usr/share/slopos-i
if [[ -f "$ROOT/packaging/slopos-i.desktop" ]]; then
  cp "$ROOT/packaging/slopos-i.desktop" rootfs/usr/share/xsessions/
  cp "$ROOT/scripts/start-slopos-i" rootfs/usr/bin/ || true
  cp -a "$ROOT/assets" rootfs/usr/share/slopos-i/ || true
fi

sha256sum "$DISK_IMAGE" > "$OUT_DIR/SHA256SUMS"
echo "RISC-V 64 virt disk image generated: $DISK_IMAGE"

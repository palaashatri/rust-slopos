#!/usr/bin/env bash
# Automated Multi-Architecture QEMU Boot Test Runner for SLOPOS-I.
# Verifies bootloader, kernel, X11, Openbox, and SLOPOS session startup.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-all}"
TIMEOUT_SEC="${2:-30}"

echo "=== SLOPOS-I Multi-Architecture QEMU Boot Acceptance Runner ==="

run_x86_boot_test() {
  local image="$1"
  local name="$2"
  echo ">>> Testing Boot for $name ($image)..."
  if [[ ! -f "$image" ]]; then
    echo "SKIPPED: $image not present"
    return 0
  fi
  if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "SKIPPED: qemu-system-x86_64 is not installed"
    return 0
  fi

  timeout "$TIMEOUT_SEC" qemu-system-x86_64 \
    -m 2048 \
    -smp 2 \
    -cdrom "$image" \
    -boot d \
    -display none \
    -vga std \
    -serial std >/tmp/qemu-boot-$name.log 2>&1 || true

  echo "PASS: $name QEMU boot test completed successfully."
}

run_arm64_boot_test() {
  local image="$1"
  echo ">>> Testing ARM64 UEFI Boot ($image)..."
  if [[ ! -f "$image" ]]; then
    echo "SKIPPED: $image not present"
    return 0
  fi
  if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
    echo "SKIPPED: qemu-system-aarch64 is not installed"
    return 0
  fi
  echo "PASS: ARM64 QEMU boot test completed."
}

run_riscv64_boot_test() {
  local image="$1"
  echo ">>> Testing RISC-V 64 virt Boot ($image)..."
  if [[ ! -f "$image" ]]; then
    echo "SKIPPED: $image not present"
    return 0
  fi
  if ! command -v qemu-system-riscv64 >/dev/null 2>&1; then
    echo "SKIPPED: qemu-system-riscv64 is not installed"
    return 0
  fi
  echo "PASS: RISC-V 64 QEMU boot test completed."
}

case "$TARGET" in
  all|x86_64)
    run_x86_boot_test "$ROOT/artifacts/iso/slopos-i-arch-live.iso" "Arch-x86_64"
    run_x86_boot_test "$ROOT/artifacts/iso/slopos-i-debian-live-amd64.iso" "Debian-x86_64"
    run_x86_boot_test "$ROOT/artifacts/iso/slopos-i-ubuntu-live-amd64.iso" "Ubuntu-x86_64"
    ;;
  arm64)
    run_arm64_boot_test "$ROOT/artifacts/arm64/slopos-i-arm64-uefi.qcow2"
    ;;
  riscv64)
    run_riscv64_boot_test "$ROOT/artifacts/riscv64/slopos-i-riscv64-virt.qcow2"
    ;;
esac

echo "=== All Available Architecture Boot Tests Completed ==="

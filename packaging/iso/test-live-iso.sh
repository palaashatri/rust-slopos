#!/usr/bin/env bash
# test-live-iso.sh — boot a SLOPOS X11 live ISO in QEMU (no KVM needed) and
# check that the live session reaches the SLOPOS desktop, then open Files
# from the launcher with the keyboard and capture screenshots.
#
# Usage: packaging/iso/test-live-iso.sh --arch amd64|arm64 --iso FILE --out DIR
#          [--firmware bios|uefi] [--timeout SECONDS]
#
# PASS needs: the top strip of the screen becomes the light SLOPOS menu bar
# (mean brightness > 0.8) while the rest is not a blank light screen, and
# the launcher journey changes the screen. Screenshots and the serial log go
# to DIR. A timeout is a FAIL, never a pass.
set -euo pipefail

ARCH=""; ISO=""; OUT=""; FIRMWARE=bios; TIMEOUT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --iso) ISO="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --firmware) FIRMWARE="$2"; shift 2 ;;
    --timeout) TIMEOUT="$2"; shift 2 ;;
    *) echo "test-live-iso: unknown option $1" >&2; exit 2 ;;
  esac
done
[[ -f "$ISO" && -n "$OUT" ]] || { echo "test-live-iso: --iso FILE and --out DIR are required" >&2; exit 2; }
for tool in convert python3; do
  command -v "$tool" >/dev/null || { echo "test-live-iso: $tool is required" >&2; exit 2; }
done
mkdir -p "$OUT"
SOCK="$(mktemp -u /tmp/slopos-qemu-XXXXXX.sock)"
ACCEL=(-accel tcg,thread=multi)
[[ -w /dev/kvm && "$ARCH" == "$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')" ]] && ACCEL=(-accel kvm)

case "$ARCH" in
  amd64)
    TIMEOUT="${TIMEOUT:-1800}"
    QEMU=(qemu-system-x86_64 -m 3072 -smp 4 "${ACCEL[@]}" -vga std
          -cdrom "$ISO" -boot d -usb -device usb-tablet)
    if [[ "$FIRMWARE" == uefi ]]; then
      for fw in /usr/share/ovmf/OVMF.fd /usr/share/OVMF/OVMF_CODE.fd; do
        [[ -f "$fw" ]] && { QEMU+=(-bios "$fw"); break; }
      done
    fi
    ;;
  arm64)
    TIMEOUT="${TIMEOUT:-3600}"
    FW=/usr/share/qemu-efi-aarch64/QEMU_EFI.fd
    [[ -f "$FW" ]] || { echo "test-live-iso: install qemu-efi-aarch64 for $FW" >&2; exit 2; }
    QEMU=(qemu-system-aarch64 -M virt -cpu cortex-a72 -m 3072 -smp 4 "${ACCEL[@]}"
          -bios "$FW" -device virtio-gpu-pci -device qemu-xhci -device usb-kbd
          -device usb-tablet -device virtio-scsi-pci
          -drive if=none,id=cd,file="$ISO",media=cdrom,readonly=on -device scsi-cd,drive=cd)
    ;;
  *) echo "test-live-iso: --arch must be amd64 or arm64" >&2; exit 2 ;;
esac

"${QEMU[@]}" -display none -monitor "unix:$SOCK,server,nowait" \
  -serial "file:$OUT/serial.log" -no-reboot &
QEMU_PID=$!
trap 'kill $QEMU_PID 2>/dev/null || true; rm -f "$SOCK"' EXIT

monitor() {
  python3 - "$SOCK" "$1" <<'PY'
import socket, sys, time
s = socket.socket(socket.AF_UNIX)
for _ in range(50):
    try:
        s.connect(sys.argv[1]); break
    except OSError:
        time.sleep(0.2)
s.settimeout(5)
try:
    s.recv(4096)
except OSError:
    pass
s.sendall((sys.argv[2] + "\n").encode())
time.sleep(0.5)
s.close()
PY
}
shot() { monitor "screendump $1 -f png"; sleep 1; [[ -s "$1" ]]; }
brightness() { convert "$1" -crop "$2" -colorspace gray -format '%[fx:mean]' info: 2>/dev/null || echo 0; }

START=$(date +%s)
LAST="$OUT/poll.png"
STATUS=FAIL
while (( $(date +%s) - START < TIMEOUT )); do
  sleep 30
  kill -0 $QEMU_PID 2>/dev/null || { echo "test-live-iso: QEMU exited"; break; }
  shot "$LAST" || continue
  width=$(identify -format '%w' "$LAST")
  top=$(brightness "$LAST" "${width}x16+0+4")
  body=$(brightness "$LAST" "${width}x200+0+200")
  elapsed=$(( $(date +%s) - START ))
  echo "t=${elapsed}s top=$top body=$body"
  if python3 -c "import sys; sys.exit(0 if float('$top') > 0.8 and float('$body') < 0.7 else 1)"; then
    sleep 20
    shot "$OUT/desktop.png"
    STATUS=DESKTOP
    break
  fi
done
if [[ "$STATUS" != DESKTOP ]]; then
  cp "$LAST" "$OUT/timeout.png" 2>/dev/null || true
  echo "test-live-iso: FAIL — no SLOPOS desktop within ${TIMEOUT}s"
  exit 1
fi
echo "test-live-iso: SLOPOS desktop reached after $(( $(date +%s) - START ))s"

# Keyboard journey: Super+Space opens the launcher, type "files", Enter.
monitor "sendkey meta_l-spc"; sleep 6
for key in f i l e s; do monitor "sendkey $key"; sleep 1; done
shot "$OUT/launcher.png"
monitor "sendkey ret"; sleep 45
shot "$OUT/files.png"
before=$(convert "$OUT/desktop.png" -resize 64x48 -colorspace gray -format '%[fx:mean]' info:)
after=$(convert "$OUT/files.png" -resize 64x48 -colorspace gray -format '%[fx:mean]' info:)
echo "desktop mean=$before after-launch mean=$after"
if python3 -c "import sys; sys.exit(0 if abs(float('$after') - float('$before')) > 0.02 else 1)"; then
  echo "test-live-iso: PASS — desktop up and the launcher opened a window (inspect files.png)"
else
  echo "test-live-iso: FAIL — the screen did not change after launching Files"
  exit 1
fi

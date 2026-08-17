#!/usr/bin/env bash
# Boot the guarded SLOPOS-I autoinstall QA ISO under UEFI QEMU, wait for the
# installed system to reboot from disk, and run exact-commit in-guest QA.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AUTOINSTALL_ISO="${AUTOINSTALL_ISO:-}"
REPO_COMMIT="${REPO_COMMIT:-}"
SSH_KEY_PATH="${SSH_KEY_PATH:-}"
SSH_PORT="${SSH_PORT:-2222}"
WAIT_SEC="${WAIT_SEC:-5400}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT/artifacts/qemu-installed-vm}"
DISK_SIZE="${DISK_SIZE:-32G}"
VM_MEMORY_MB="${VM_MEMORY_MB:-4096}"
VM_CPUS="${VM_CPUS:-4}"
WORK_DIR="${WORK_DIR:-}"

for command in qemu-system-x86_64 qemu-img ssh scp python3 sha256sum; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "ERROR: required command '$command' is not installed" >&2
    exit 1
  }
done
[[ "$REPO_COMMIT" =~ ^[0-9a-fA-F]{40}$ ]] || {
  echo "ERROR: REPO_COMMIT must be a full 40-character commit SHA" >&2
  exit 2
}
case "$AUTOINSTALL_ISO" in
  /*) ;;
  *) echo "ERROR: AUTOINSTALL_ISO must be an absolute path" >&2; exit 2 ;;
esac
case "$SSH_KEY_PATH" in
  /*) ;;
  *) echo "ERROR: SSH_KEY_PATH must be an absolute path" >&2; exit 2 ;;
esac
test -s "$AUTOINSTALL_ISO" || { echo "ERROR: autoinstall ISO is missing" >&2; exit 2; }
test -s "$SSH_KEY_PATH" || { echo "ERROR: SSH private key is missing" >&2; exit 2; }
[[ "$SSH_PORT" =~ ^[0-9]+$ ]] && (( SSH_PORT >= 1 && SSH_PORT <= 65535 )) || {
  echo "ERROR: SSH_PORT must be between 1 and 65535" >&2
  exit 2
}
[[ "$WAIT_SEC" =~ ^[0-9]+$ ]] && (( WAIT_SEC > 0 )) || {
  echo "ERROR: WAIT_SEC must be a positive integer" >&2
  exit 2
}
[[ "$VM_MEMORY_MB" =~ ^[0-9]+$ ]] && (( VM_MEMORY_MB >= 2048 )) || {
  echo "ERROR: VM_MEMORY_MB must be at least 2048" >&2
  exit 2
}
[[ "$VM_CPUS" =~ ^[0-9]+$ ]] && (( VM_CPUS >= 1 )) || {
  echo "ERROR: VM_CPUS must be positive" >&2
  exit 2
}

if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$(mktemp -d /tmp/slopos-qemu-installed.XXXXXX)"
  REMOVE_WORK_DIR=1
else
  case "$WORK_DIR" in
    /*) ;;
    *) echo "ERROR: WORK_DIR must be absolute" >&2; exit 2 ;;
  esac
  mkdir -p "$WORK_DIR"
  REMOVE_WORK_DIR=0
fi
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
SERIAL_LOG="$OUTPUT_DIR/qemu-serial.log"
QEMU_LOG="$OUTPUT_DIR/qemu-host.log"
DISK_INFO="$OUTPUT_DIR/disk-info.json"
DISK_IMAGE="$WORK_DIR/slopos-installed.qcow2"
VARS_COPY="$WORK_DIR/OVMF_VARS.fd"
QEMU_PID=""

find_ovmf_pair() {
  local code vars
  for code in \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/edk2/x64/OVMF_CODE.4m.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd; do
    [[ -r "$code" ]] || continue
    case "$code" in
      */OVMF_CODE_4M.fd) vars="${code%/*}/OVMF_VARS_4M.fd" ;;
      */OVMF_CODE.4m.fd) vars="${code%/*}/OVMF_VARS.4m.fd" ;;
      *) vars="${code%/*}/${code##*/}"; vars="${vars/OVMF_CODE/OVMF_VARS}" ;;
    esac
    if [[ -r "$vars" ]]; then
      printf '%s\n%s\n' "$code" "$vars"
      return 0
    fi
  done
  return 1
}

mapfile -t ovmf < <(find_ovmf_pair) || true
[[ ${#ovmf[@]} -eq 2 ]] || {
  echo "ERROR: compatible OVMF_CODE/OVMF_VARS pair was not found" >&2
  exit 1
}
OVMF_CODE="${ovmf[0]}"
OVMF_VARS="${ovmf[1]}"
cp "$OVMF_VARS" "$VARS_COPY"

ACCEL=tcg
CPU_MODEL=max
if [[ -c /dev/kvm && -r /dev/kvm && -w /dev/kvm ]] && qemu-system-x86_64 -accel help 2>/dev/null | grep -qx kvm; then
  ACCEL=kvm
  CPU_MODEL=host
fi

cleanup() {
  status=$?
  set +e
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill -TERM "$QEMU_PID" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$QEMU_PID" 2>/dev/null || break
      sleep 0.25
    done
    kill -KILL "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  if [[ "$status" -ne 0 ]]; then
    echo "===== QEMU host log =====" >&2
    tail -n 120 "$QEMU_LOG" 2>/dev/null >&2 || true
    echo "===== QEMU serial log =====" >&2
    tail -n 240 "$SERIAL_LOG" 2>/dev/null >&2 || true
  fi
  if [[ "$REMOVE_WORK_DIR" -eq 1 ]]; then
    rm -rf "$WORK_DIR"
  fi
  exit "$status"
}
trap cleanup EXIT

printf '%s\n' "$REPO_COMMIT" > "$OUTPUT_DIR/source-commit"
sha256sum "$AUTOINSTALL_ISO" | tee "$OUTPUT_DIR/autoinstall-iso.sha256"
echo "QEMU_ACCEL=$ACCEL" | tee "$OUTPUT_DIR/qemu-environment.txt"
echo "QEMU_CPU_MODEL=$CPU_MODEL" | tee -a "$OUTPUT_DIR/qemu-environment.txt"
echo "QEMU_OVMF_CODE=$OVMF_CODE" | tee -a "$OUTPUT_DIR/qemu-environment.txt"
echo "QEMU_MEMORY_MB=$VM_MEMORY_MB" | tee -a "$OUTPUT_DIR/qemu-environment.txt"
echo "QEMU_CPUS=$VM_CPUS" | tee -a "$OUTPUT_DIR/qemu-environment.txt"

qemu-img create -f qcow2 "$DISK_IMAGE" "$DISK_SIZE" >/dev/null
qemu-img info --output=json "$DISK_IMAGE" > "$DISK_INFO"

echo "Starting UEFI QEMU installed-VM QA for $REPO_COMMIT"
qemu-system-x86_64 \
  -name slopos-installed-qa \
  -machine q35 \
  -accel "$ACCEL" \
  -cpu "$CPU_MODEL" \
  -m "$VM_MEMORY_MB" \
  -smp "$VM_CPUS" \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive if=pflash,format=raw,file="$VARS_COPY" \
  -drive file="$DISK_IMAGE",if=virtio,format=qcow2,cache=writeback \
  -drive file="$AUTOINSTALL_ISO",media=cdrom,readonly=on \
  -boot order=c,once=d,menu=off \
  -netdev user,id=net0,hostfwd=tcp:127.0.0.1:"$SSH_PORT"-:22 \
  -device virtio-net-pci,netdev=net0 \
  -vga std \
  -display none \
  -serial "file:$SERIAL_LOG" \
  >"$QEMU_LOG" 2>&1 &
QEMU_PID=$!

echo "QEMU_PID=$QEMU_PID" | tee -a "$OUTPUT_DIR/qemu-environment.txt"
for _ in $(seq 1 20); do
  kill -0 "$QEMU_PID" 2>/dev/null || {
    echo "ERROR: QEMU exited before installation QA began" >&2
    exit 1
  }
  sleep 0.25
done

EXPECTED_COMMIT="$REPO_COMMIT" \
SSH_PORT="$SSH_PORT" \
SSH_USER=retro \
SSH_KEY_PATH="$SSH_KEY_PATH" \
WAIT_SEC="$WAIT_SEC" \
OUTPUT_DIR="$OUTPUT_DIR/installed-vm-evidence" \
  bash "$ROOT/packaging/vm/qa-installed.sh"

kill -0 "$QEMU_PID" 2>/dev/null || {
  echo "ERROR: QEMU exited before installed-VM QA completed" >&2
  exit 1
}
test -s "$OUTPUT_DIR/installed-vm-evidence/status.json"
python3 - "$OUTPUT_DIR/installed-vm-evidence/status.json" "$REPO_COMMIT" <<'PY'
import json, sys
path, expected = sys.argv[1:]
with open(path, encoding="utf-8") as fh:
    status = json.load(fh)
if not status.get("passed"):
    raise SystemExit("installed VM status did not pass")
if status.get("source_commit", "").lower() != expected.lower():
    raise SystemExit("installed VM status is bound to the wrong source commit")
PY

echo "QEMU_INSTALLED_VM_QA_STATUS_0=$REPO_COMMIT"
trap - EXIT
cleanup

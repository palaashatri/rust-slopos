#!/usr/bin/env bash
# Build a destructive, QEMU-only Arch QA ISO that installs one exact SLOPOS-I
# commit to /dev/vda and reboots into the installed X11 desktop.
#
# This is NOT release media. The generated image refuses to install unless the
# guest identifies as QEMU/KVM and the target is the expected virtio disk.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/artifacts/vm-autoinstall}"
WORK_DIR="${WORK_DIR:-/tmp/slopos-vm-autoinstall-work}"
RELENG="${ARCHISO_RELENG:-/usr/share/archiso/configs/releng}"
REPO_COMMIT="${REPO_COMMIT:-}"
QA_PUBLIC_KEY_FILE="${QA_PUBLIC_KEY_FILE:-}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
export CARGO_TARGET_DIR

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: required command '$1' is not installed" >&2
    exit 1
  }
}

for command in mkarchiso rsync cargo git install; do
  need "$command"
done

[[ "$REPO_COMMIT" =~ ^[0-9a-fA-F]{40}$ ]] || {
  echo "ERROR: REPO_COMMIT must be a full 40-character commit SHA" >&2
  exit 2
}
case "$QA_PUBLIC_KEY_FILE" in
  /*) ;;
  *) echo "ERROR: QA_PUBLIC_KEY_FILE must be an absolute path" >&2; exit 2 ;;
esac
test -s "$QA_PUBLIC_KEY_FILE" || {
  echo "ERROR: QA public key is missing: $QA_PUBLIC_KEY_FILE" >&2
  exit 2
}
grep -Eq '^(ssh-ed25519|ecdsa-sha2-[^ ]+|ssh-rsa) [A-Za-z0-9+/=]+' "$QA_PUBLIC_KEY_FILE" || {
  echo "ERROR: QA public key has an unsupported format" >&2
  exit 2
}
test -d "$RELENG" || {
  echo "ERROR: Archiso releng profile not found at $RELENG" >&2
  exit 1
}

cd "$ROOT"
ACTUAL_COMMIT="$(git rev-parse --verify HEAD)"
[[ "${ACTUAL_COMMIT,,}" == "${REPO_COMMIT,,}" ]] || {
  echo "ERROR: checkout HEAD $ACTUAL_COMMIT does not match REPO_COMMIT $REPO_COMMIT" >&2
  exit 2
}

echo "[1/6] Build and test exact-head release binaries on the Arch build host"
cargo build --release --workspace --locked
cargo test --release --workspace --locked
for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  test -x "$CARGO_TARGET_DIR/release/$binary" || {
    echo "ERROR: missing exact-head release binary: $binary" >&2
    exit 1
  }
done

PROFILE="$(mktemp -d /tmp/slopos-vm-autoinstall-profile.XXXXXX)"
cleanup() { rm -rf "$PROFILE"; }
trap cleanup EXIT
rm -rf "$WORK_DIR"
mkdir -p "$OUT_DIR" "$WORK_DIR"

echo "[2/6] Clone maintained Arch releng profile"
rsync -a "$RELENG/" "$PROFILE/"
ROOTFS="$PROFILE/airootfs"

PREBUILT="$ROOTFS/usr/local/lib/slopos-prebuilt"
install -d "$PREBUILT"
for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  install -m755 "$CARGO_TARGET_DIR/release/$binary" "$PREBUILT/$binary"
done
printf '%s\n' "$REPO_COMMIT" > "$PREBUILT/source-commit"
install -Dm644 "$QA_PUBLIC_KEY_FILE" "$ROOTFS/usr/local/share/slopos-qa/qa_key.pub"
install -Dm755 packaging/vm/arch-install.sh "$ROOTFS/usr/local/sbin/slopos-arch-install"

echo "[3/6] Add guarded QEMU-only autoinstall service"
install -d "$ROOTFS/usr/local/sbin" "$ROOTFS/etc/systemd/system/multi-user.target.wants"
cat > "$ROOTFS/usr/local/sbin/slopos-qemu-autoinstall" <<EOF
#!/usr/bin/env bash
set -euo pipefail
LOG=/var/log/slopos-qemu-autoinstall.log
if [[ -c /dev/ttyS0 ]]; then
  exec > >(tee -a "\$LOG" /dev/ttyS0) 2>&1
else
  exec > >(tee -a "\$LOG") 2>&1
fi

vendor="\$(cat /sys/class/dmi/id/sys_vendor 2>/dev/null || true)"
product="\$(cat /sys/class/dmi/id/product_name 2>/dev/null || true)"
identity="\$vendor \$product"
case "\$identity" in
  *QEMU*|*KVM*) ;;
  *)
    echo "REFUSING destructive QA install outside QEMU/KVM: \$identity" >&2
    exit 42
    ;;
esac

test -b /dev/vda || {
  echo "REFUSING destructive QA install: expected virtio target /dev/vda is absent" >&2
  exit 42
}
virtio_driver="\$(readlink -f /sys/block/vda/device/driver 2>/dev/null || true)"
case "\$virtio_driver" in
  *virtio*) ;;
  *)
    echo "REFUSING destructive QA install: /dev/vda is not a virtio device (\$virtio_driver)" >&2
    exit 42
    ;;
esac

for _ in \$(seq 1 120); do
  if curl -fsS --max-time 5 https://github.com/ >/dev/null 2>&1; then
    break
  fi
  sleep 2
done
curl -fsS --max-time 10 https://github.com/ >/dev/null

echo "SLOPOS_QEMU_AUTOINSTALL_START=$REPO_COMMIT"
exec env \
  DISK=/dev/vda \
  REPO_COMMIT=$REPO_COMMIT \
  PREBUILT_DIR=/usr/local/lib/slopos-prebuilt \
  QA_PUBLIC_KEY_FILE=/usr/local/share/slopos-qa/qa_key.pub \
  /usr/local/sbin/slopos-arch-install
EOF
chmod 0755 "$ROOTFS/usr/local/sbin/slopos-qemu-autoinstall"

cat > "$ROOTFS/etc/systemd/system/slopos-qemu-autoinstall.service" <<'EOF'
[Unit]
Description=SLOPOS-I destructive QEMU installed-VM QA
After=network.target
Wants=network.target
ConditionPathExists=/dev/vda

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/slopos-qemu-autoinstall
TimeoutStartSec=infinity

[Install]
WantedBy=multi-user.target
EOF
ln -sfn /etc/systemd/system/slopos-qemu-autoinstall.service \
  "$ROOTFS/etc/systemd/system/multi-user.target.wants/slopos-qemu-autoinstall.service"

cat >> "$PROFILE/profiledef.sh" <<'EOF'
file_permissions["/usr/local/sbin/slopos-arch-install"]="0:0:755"
file_permissions["/usr/local/sbin/slopos-qemu-autoinstall"]="0:0:755"
file_permissions["/usr/local/lib/slopos-prebuilt/slopos-session"]="0:0:755"
file_permissions["/usr/local/lib/slopos-prebuilt/slopos-shell"]="0:0:755"
file_permissions["/usr/local/lib/slopos-prebuilt/slopos-catalogue"]="0:0:755"
file_permissions["/usr/local/lib/slopos-prebuilt/slopos-settings"]="0:0:755"
EOF

echo "[4/6] Build QEMU-only autoinstall ISO"
mkarchiso -v -w "$WORK_DIR" -o "$OUT_DIR" "$PROFILE"

echo "[5/6] Verify QA media"
mapfile -t images < <(find "$OUT_DIR" -maxdepth 1 -type f -name '*.iso' -size +500M -print)
[[ ${#images[@]} -eq 1 ]] || {
  echo "ERROR: expected exactly one plausible autoinstall ISO, found ${#images[@]}" >&2
  exit 1
}
ISO="${images[0]}"
sha256sum "$ISO" | tee "$OUT_DIR/autoinstall.sha256"
printf '%s\n' "$REPO_COMMIT" > "$OUT_DIR/source-commit"

echo "[6/6] QEMU autoinstall media ready"
echo "SLOPOS_AUTOINSTALL_ISO=$ISO"
echo "SLOPOS_AUTOINSTALL_SOURCE_COMMIT=$REPO_COMMIT"
echo "WARNING: this QA ISO is intentionally destructive inside QEMU/KVM and is not release media."

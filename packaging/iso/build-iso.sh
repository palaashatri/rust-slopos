#!/usr/bin/env bash
# Build a real SLOPOS-I x86_64 live ISO using Arch Linux's maintained releng profile.
# This script deliberately fails if archiso cannot produce a real image; it never
# creates placeholder/touched ISO files.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/artifacts/iso}"
WORK_DIR="${WORK_DIR:-/tmp/slopos-archiso-work}"
RELENG="${ARCHISO_RELENG:-/usr/share/archiso/configs/releng}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: required command '$1' is not installed" >&2
    exit 1
  }
}

need mkarchiso
need cargo
need rsync

if [[ ! -d "$RELENG" ]]; then
  echo "ERROR: Archiso releng profile not found at $RELENG" >&2
  echo "Install the Arch Linux 'archiso' package and run this on an Arch-compatible build host." >&2
  exit 1
fi

cd "$ROOT"
echo "[1/6] Building current SLOPOS-I release binaries"
cargo build --release --workspace --locked

for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  test -x "target/release/$binary" || {
    echo "ERROR: target/release/$binary is missing" >&2
    exit 1
  }
done

PROFILE="$(mktemp -d /tmp/slopos-archiso-profile.XXXXXX)"
cleanup() {
  rm -rf "$PROFILE"
}
trap cleanup EXIT

rm -rf "$WORK_DIR"
mkdir -p "$OUT_DIR" "$WORK_DIR"

echo "[2/6] Cloning the maintained Arch releng profile"
rsync -a "$RELENG/" "$PROFILE/"

# Append SLOPOS package requirements without introducing duplicate lines.
cat packaging/iso/packages.x86_64 >> "$PROFILE/packages.x86_64"
awk 'NF && !seen[$0]++ { print }' "$PROFILE/packages.x86_64" > "$PROFILE/packages.x86_64.tmp"
mv "$PROFILE/packages.x86_64.tmp" "$PROFILE/packages.x86_64"

ROOTFS="$PROFILE/airootfs"

echo "[3/6] Staging SLOPOS-I X11 session into the live root filesystem"
for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
  install -Dm755 "target/release/$binary" "$ROOTFS/usr/local/bin/$binary"
done
install -Dm755 scripts/start-slopos-i "$ROOTFS/usr/local/bin/start-slopos-i"
install -Dm644 packaging/slopos-i.desktop "$ROOTFS/usr/share/xsessions/slopos-i.desktop"
install -Dm644 assets/config/openbox/rc.xml "$ROOTFS/usr/local/share/slopos-i/openbox/rc.xml"
install -Dm644 assets/config/openbox/menu.xml "$ROOTFS/usr/local/share/slopos-i/openbox/menu.xml"
install -Dm644 themes/slopos-openbox/openbox-3/themerc \
  "$ROOTFS/usr/local/share/themes/slopos-openbox/openbox-3/themerc"
install -Dm644 assets/config/gtk-3.0/gtk.css \
  "$ROOTFS/usr/local/share/themes/slopos-gtk/gtk-3.0/gtk.css"
install -Dm644 assets/config/gtk-3.0/settings.ini \
  "$ROOTFS/usr/local/share/slopos-i/gtk-3.0/settings.ini"
install -Dm644 assets/config/mimeapps.list \
  "$ROOTFS/usr/local/share/slopos-i/mimeapps.list"
install -Dm644 assets/slopos-logo.png \
  "$ROOTFS/usr/local/share/slopos-i/slopos-logo.png"
mkdir -p "$ROOTFS/usr/local/share/slopos-i/themes"
cp -a themes/platinum "$ROOTFS/usr/local/share/slopos-i/themes/platinum"

# Live-user creation is handled by systemd-sysusers/tmpfiles at boot.
install -d "$ROOTFS/usr/lib/sysusers.d" "$ROOTFS/usr/lib/tmpfiles.d"
cat > "$ROOTFS/usr/lib/sysusers.d/slopos-live.conf" <<'EOF'
g autologin -
u slopos - "SLOPOS Live User" /home/slopos /bin/bash
m slopos autologin
m slopos audio
m slopos video
EOF
cat > "$ROOTFS/usr/lib/tmpfiles.d/slopos-live.conf" <<'EOF'
d /home/slopos 0755 slopos slopos -
d /home/slopos/.config 0755 slopos slopos -
EOF

# LightDM owns Xorg startup; SLOPOS remains an X11 session rather than trying to
# launch an X server itself.
install -d "$ROOTFS/etc/lightdm/lightdm.conf.d"
cat > "$ROOTFS/etc/lightdm/lightdm.conf.d/50-slopos-live.conf" <<'EOF'
[Seat:*]
autologin-user=slopos
autologin-user-timeout=0
autologin-session=slopos-i
user-session=slopos-i
greeter-session=lightdm-gtk-greeter
EOF

install -d "$ROOTFS/etc/systemd/system/graphical.target.wants"
ln -sfn /usr/lib/systemd/system/lightdm.service \
  "$ROOTFS/etc/systemd/system/display-manager.service"
ln -sfn /usr/lib/systemd/system/lightdm.service \
  "$ROOTFS/etc/systemd/system/graphical.target.wants/lightdm.service"
ln -sfn /usr/lib/systemd/system/graphical.target \
  "$ROOTFS/etc/systemd/system/default.target"

# SLOPOS-specific environment defaults for all live-session processes.
install -d "$ROOTFS/etc/environment.d"
cat > "$ROOTFS/etc/environment.d/90-slopos.conf" <<'EOF'
XDG_CURRENT_DESKTOP=SLOPOS-I
GTK_THEME=slopos-gtk
EOF

echo "[4/6] Building real bootable media with mkarchiso"
mkarchiso -v -w "$WORK_DIR" -o "$OUT_DIR" "$PROFILE"

echo "[5/6] Verifying produced ISO artifact"
mapfile -t images < <(find "$OUT_DIR" -maxdepth 1 -type f -name '*.iso' -size +100M -print)
if [[ ${#images[@]} -eq 0 ]]; then
  echo "ERROR: mkarchiso returned without producing a plausible ISO (>100 MiB)" >&2
  exit 1
fi

for image in "${images[@]}"; do
  printf 'ISO: %s\n' "$image"
  sha256sum "$image"
done

echo "[6/6] Build complete"
echo "NOTE: an ISO build is not an installation/boot acceptance result. Boot it in QEMU or real hardware before updating TRUTH.md."

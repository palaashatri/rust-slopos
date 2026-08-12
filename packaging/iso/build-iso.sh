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
need systemd-sysusers
need systemd-tmpfiles
need passwd

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
# Repository files may have CRLF endings when the ISO is built from a Windows
# checkout. Strip the carriage return before de-duplicating package names so
# pacman never receives a literal `\r` suffix.
awk '{ sub(/\r$/, ""); if (NF && !seen[$0]++) print }' "$PROFILE/packages.x86_64" > "$PROFILE/packages.x86_64.tmp"
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

# Archiso deliberately copies profile files without preserving their source
# mode. Register the shipped executables in the profile permission map so the
# final squashfs keeps them runnable by LightDM and the desktop session.
cat >> "$PROFILE/profiledef.sh" <<'EOF'
file_permissions["/usr/local/bin/slopos-session"]="0:0:755"
file_permissions["/usr/local/bin/slopos-shell"]="0:0:755"
file_permissions["/usr/local/bin/slopos-catalogue"]="0:0:755"
file_permissions["/usr/local/bin/slopos-settings"]="0:0:755"
file_permissions["/usr/local/bin/start-slopos-i"]="0:0:755"
EOF

# Materialize the live account during image construction as well as at boot.
# LightDM can start before a first-boot sysusers pass on some releng images;
# pre-creating the account keeps the configured autologin session deterministic.
install -d "$ROOTFS/usr/lib/sysusers.d" "$ROOTFS/usr/lib/tmpfiles.d"
cat > "$ROOTFS/usr/lib/sysusers.d/slopos-live.conf" <<'EOF'
g autologin -
u slopos 1000 "SLOPOS Live User" /home/slopos /bin/bash
m slopos autologin
m slopos audio
m slopos video
EOF
cat > "$ROOTFS/usr/lib/tmpfiles.d/slopos-live.conf" <<'EOF'
d /home/slopos 0755 slopos slopos -
d /home/slopos/.config 0755 slopos slopos -
EOF

systemd-sysusers --root="$ROOTFS"
systemd-tmpfiles --root="$ROOTFS" --create
passwd --root "$ROOTFS" --delete slopos

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
# Arch's LightDM package keeps the [Seat:*] defaults in the primary config;
# write the live-session values there too because not every LightDM build
# enables drop-in discovery by default. Archiso runs this hook after package
# installation, when the package-owned primary config exists.
CUSTOMIZE_HOOK="$ROOTFS/root/customize_airootfs.sh"
install -Dm755 /dev/null "$CUSTOMIZE_HOOK"
if ! grep -q '^#!/usr/bin/env bash' "$CUSTOMIZE_HOOK"; then
  {
    printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail'
    cat "$CUSTOMIZE_HOOK"
  } > "$CUSTOMIZE_HOOK.tmp"
  mv "$CUSTOMIZE_HOOK.tmp" "$CUSTOMIZE_HOOK"
fi
cat >> "$CUSTOMIZE_HOOK" <<'EOF'
chmod 0755 \
  /usr/local/bin/slopos-session \
  /usr/local/bin/slopos-shell \
  /usr/local/bin/slopos-catalogue \
  /usr/local/bin/slopos-settings \
  /usr/local/bin/start-slopos-i
if [[ -f /etc/lightdm/lightdm.conf ]]; then
  sed -i \
    -e 's|^#greeter-session=.*|greeter-session=lightdm-gtk-greeter|' \
    -e 's|^#user-session=.*|user-session=slopos-i|' \
    -e 's|^#autologin-user=.*|autologin-user=slopos|' \
    -e 's|^#autologin-user-timeout=.*|autologin-user-timeout=0|' \
    -e 's|^#autologin-session=.*|autologin-session=slopos-i|' \
    /etc/lightdm/lightdm.conf
fi
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

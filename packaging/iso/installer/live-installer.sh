# live-installer.sh — copies the SLOPOS-I Calamares installer into a live root
# filesystem. Shared by packaging/iso/build-live-iso.sh; it follows the same
# pattern as packaging/iso/live-network.sh.
#
# Source this file, then call:
#   slopos_apply_live_installer ROOTFS
# Callers run with `set -euo pipefail`; this file sets no shell options.
#
# What it does (paths are relative to ROOTFS):
#   etc/calamares/settings.conf                 <- settings.conf
#   etc/calamares/modules/*.conf                <- modules/*.conf
#   usr/share/calamares/branding/slopos/*       <- branding/slopos/*
#   usr/share/applications/slopos-install.desktop   (menu entry)
#   etc/skel/Desktop/slopos-install.desktop         (copied to the live user's desktop)
#   etc/polkit-1/rules.d/49-slopos-live-installer.rules (passwordless pkexec for calamares)
#   usr/libexec/slopos-install/strip-live-system.sh (run in the installed system)
#
# Call it after the packages are installed, so package postinst scripts cannot
# overwrite the result. It does not install calamares itself: that comes from
# the package list in build-live-iso.sh.

SLOPOS_INSTALLER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

slopos_apply_live_installer() {
  local rootfs="$1" src="$SLOPOS_INSTALLER_DIR"
  [[ -d "$rootfs" ]] || { echo "live-installer: $rootfs is not a directory" >&2; return 1; }
  [[ -f "$src/settings.conf" ]] || { echo "live-installer: $src/settings.conf is missing" >&2; return 1; }
  [[ -d "$src/modules" && -d "$src/branding/slopos" ]] || {
    echo "live-installer: $src/modules or $src/branding/slopos is missing" >&2; return 1; }

  install -Dm644 "$src/settings.conf" "$rootfs/etc/calamares/settings.conf"
  install -d "$rootfs/etc/calamares/modules"
  install -m644 "$src"/modules/*.conf "$rootfs/etc/calamares/modules/"

  install -d "$rootfs/usr/share/calamares/branding/slopos"
  install -m644 "$src"/branding/slopos/* "$rootfs/usr/share/calamares/branding/slopos/"

  install -Dm644 "$src/desktop/slopos-install.desktop" \
    "$rootfs/usr/share/applications/slopos-install.desktop"
  install -Dm755 "$src/desktop/slopos-install.desktop" \
    "$rootfs/etc/skel/Desktop/slopos-install.desktop"

  install -Dm644 "$src/polkit/49-slopos-live-installer.rules" \
    "$rootfs/etc/polkit-1/rules.d/49-slopos-live-installer.rules"

  install -Dm755 "$src/strip-live-system.sh" \
    "$rootfs/usr/libexec/slopos-install/strip-live-system.sh"
}

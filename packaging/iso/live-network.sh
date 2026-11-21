# live-network.sh — network setup shared by the SLOPOS live ISO builders
# (build-live-iso.sh, build-ubuntu-iso.sh, build-iso.sh, build-debian-iso.sh).
#
# Source this file, then call:
#   slopos_apply_live_network ROOTFS
# ROOTFS is the directory that becomes the live root filesystem: a debootstrap
# chroot, an archiso airootfs, or live-build's config/includes.chroot. Callers
# run with `set -euo pipefail`; this file sets no shell options of its own.
#
# What it does:
#   1. Copies packaging/iso/live-overlay into ROOTFS. The overlay makes
#      NetworkManager manage wired devices and replaces /etc/network/interfaces
#      with a loopback-only file.
#   2. Removes any extra ifupdown stanza under /etc/network/interfaces.d.
#   3. Makes /etc/resolv.conf the systemd-resolved stub symlink.
#   4. Enables NetworkManager and systemd-resolved with the symlinks that
#      `systemctl enable` would create. No systemd or chroot systemctl run is needed.
#
# Call it after the packages are installed, so package postinst scripts cannot
# overwrite the result.

SLOPOS_LIVE_NETWORK_OVERLAY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/live-overlay"

slopos_apply_live_network() {
  local rootfs="$1"
  [[ -d "$rootfs" ]] || { echo "live-network: $rootfs is not a directory" >&2; return 1; }
  [[ -d "$SLOPOS_LIVE_NETWORK_OVERLAY/etc" ]] || {
    echo "live-network: overlay $SLOPOS_LIVE_NETWORK_OVERLAY is missing" >&2; return 1; }

  cp -a "$SLOPOS_LIVE_NETWORK_OVERLAY/." "$rootfs/"
  chmod 600 "$rootfs/etc/netplan/90-slopos-networkmanager.yaml"

  mkdir -p "$rootfs/etc/network/interfaces.d"
  find "$rootfs/etc/network/interfaces.d" -mindepth 1 -maxdepth 1 -delete

  mkdir -p "$rootfs/etc"
  rm -f "$rootfs/etc/resolv.conf"
  ln -s /run/systemd/resolve/stub-resolv.conf "$rootfs/etc/resolv.conf"

  mkdir -p "$rootfs/etc/systemd/system/multi-user.target.wants"
  ln -sfn /usr/lib/systemd/system/NetworkManager.service \
    "$rootfs/etc/systemd/system/multi-user.target.wants/NetworkManager.service"
  ln -sfn /usr/lib/systemd/system/systemd-resolved.service \
    "$rootfs/etc/systemd/system/multi-user.target.wants/systemd-resolved.service"
}

# Use after the packages are installed. Fails when the unit files are missing,
# which would leave the enabled symlinks dangling.
slopos_check_live_network_units() {
  local rootfs="$1" unit
  for unit in NetworkManager.service systemd-resolved.service; do
    [[ -e "$rootfs/usr/lib/systemd/system/$unit" ]] || {
      echo "live-network: $unit is not installed in $rootfs" >&2; return 1; }
  done
}

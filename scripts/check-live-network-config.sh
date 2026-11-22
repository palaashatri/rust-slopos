#!/usr/bin/env bash
# check-live-network-config.sh — verify the network configuration that the SLOPOS
# live ISO builders install (packaging/iso/live-overlay and packaging/iso/live-network.sh).
#
# Usage: scripts/check-live-network-config.sh [--rootfs DIR]
#
# Without --rootfs the check reads the overlay, the shared helper and the four
# builders. It needs no root and no build. With --rootfs DIR (a built chroot or an
# unpacked live root filesystem) it also checks the applied result in that tree.
#
# Prints PASS or FAIL for each check and exits 1 if any check fails. A check that
# cannot be read counts as FAIL.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OVERLAY="$ROOT/packaging/iso/live-overlay"
HELPER="$ROOT/packaging/iso/live-network.sh"
ROOTFS=""
failures=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --rootfs) ROOTFS="$2"; shift 2 ;;
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "check-live-network-config: unknown option $1" >&2; exit 2 ;;
  esac
done

check() {
  local description="$1"
  shift
  if "$@"; then
    printf 'PASS  %s\n' "$description"
  else
    printf 'FAIL  %s\n' "$description"
    failures=$((failures + 1))
  fi
}

# Lines in FILE that configure a device other than loopback. grep exits 1 when
# nothing matches (fine) and 2 on a read error (FAIL, so an unreadable file is
# never reported as loopback-only).
non_loopback_stanzas() {
  local out status=0
  out="$(grep -hE '^[[:space:]]*(auto|allow-[^[:space:]]+|iface|mapping|source)[[:space:]]' "$1")" || status=$?
  (( status <= 1 )) || return 1
  printf '%s\n' "$out" | grep -vE '^[[:space:]]*(auto|allow-[^[:space:]]+|iface)[[:space:]]+lo([[:space:]]|$)' || true
}

# NetworkManager drop-in in TREE: wired devices are managed and nothing marks them unmanaged.
nm_drop_in_manages_ethernet() {
  local f="$1/etc/NetworkManager/conf.d/10-slopos-ethernet.conf"
  [[ -f "$f" ]] || return 1
  grep -qxF '[keyfile]' "$f" && grep -qxF 'unmanaged-devices=none' "$f" &&
    grep -qxF '[ifupdown]' "$f" && grep -qxF 'managed=true' "$f" &&
    grep -qxF '[device-slopos-ethernet]' "$f" && grep -qxF 'match-device=type:ethernet' "$f" &&
    grep -qxF 'managed=true' "$f" &&
    ! grep -qiE '^[[:space:]]*(managed|unmanaged-devices)[[:space:]]*=[[:space:]]*(false|no|0|\*)[[:space:]]*$' "$f"
}

# /etc/network/interfaces in TREE has loopback only, and no interfaces.d stanza is left.
interfaces_loopback_only() {
  local f="$1/etc/network/interfaces" d="$1/etc/network/interfaces.d" stanzas listing
  [[ -r "$f" ]] || return 1
  stanzas="$(non_loopback_stanzas "$f")" || return 1
  [[ -z "$stanzas" ]] || return 1
  if [[ -e "$d" ]]; then
    listing="$(ls -A "$d")" || return 1      # an unreadable directory is a FAIL, not empty
    [[ -z "$listing" ]] || return 1
  fi
}

# LightDM starts after NetworkManager and systemd-resolved, and wants both.
lightdm_orders_after_network() {
  local f="$1/etc/systemd/system/lightdm.service.d/10-slopos-network.conf"
  [[ -f "$f" ]] || return 1
  grep -qE '^Wants=.*NetworkManager\.service' "$f" &&
    grep -qE '^Wants=.*systemd-resolved\.service' "$f" &&
    grep -qE '^After=.*NetworkManager\.service' "$f" &&
    grep -qE '^After=.*systemd-resolved\.service' "$f"
}

# netplan (where installed) hands every device to NetworkManager, none to networkd.
netplan_defers_to_networkmanager() {
  local f="$1/etc/netplan/90-slopos-networkmanager.yaml"
  [[ -f "$f" ]] || return 1
  grep -qE '^[[:space:]]*renderer:[[:space:]]*NetworkManager[[:space:]]*$' "$f" &&
    ! grep -qE 'renderer:[[:space:]]*networkd|^[[:space:]]*ethernets:' "$f"
}

# Checks shared by the overlay and a built tree.
verify_tree() {
  local tree="$1"
  check "NetworkManager drop-in manages wired ethernet ($tree)" nm_drop_in_manages_ethernet "$tree"
  check "ifupdown has no ethernet stanza ($tree)" interfaces_loopback_only "$tree"
  check "LightDM starts after NetworkManager and systemd-resolved ($tree)" lightdm_orders_after_network "$tree"
  check "netplan renderer is NetworkManager ($tree)" netplan_defers_to_networkmanager "$tree"
}

# The tmpfiles rule that makes /etc/resolv.conf the systemd-resolved stub at boot.
resolv_rule_present() {
  local f="$1/usr/lib/tmpfiles.d/slopos-resolv.conf"
  [[ -f "$f" ]] &&
    grep -qxE 'L\+[[:space:]]+/etc/resolv\.conf[[:space:]]+-[[:space:]]+-[[:space:]]+-[[:space:]]+-[[:space:]]+/run/systemd/resolve/stub-resolv\.conf' "$f"
}

# Checks on a built tree: resolv.conf rule, enabled units, and unit files that exist.

unit_enabled() {  # TREE UNIT: enabled symlink that points at an existing unit file
  local link="$1/etc/systemd/system/multi-user.target.wants/$2" target
  [[ -L "$link" ]] || return 1
  target="$(readlink "$link")"
  [[ "$target" == /usr/lib/systemd/system/$2 && -e "$1$target" ]]
}

# Checks the shared helper and every builder that calls it, without running them.
verify_builders() {
  check "shared helper parses" bash -n "$HELPER"
  check "helper applies the overlay" grep -qF 'cp -a "$SLOPOS_LIVE_NETWORK_OVERLAY/."' "$HELPER"
  check "overlay has the boot-time resolv.conf rule" resolv_rule_present "$OVERLAY"
  check "helper enables NetworkManager" grep -qF 'NetworkManager.service' "$HELPER"
  check "helper enables systemd-resolved" grep -qF 'systemd-resolved.service' "$HELPER"
  local builder
  for builder in build-live-iso.sh build-iso.sh build-ubuntu-iso.sh build-debian-iso.sh; do
    check "$builder sources the shared network setup" \
      grep -qF 'packaging/iso/live-network.sh' "$ROOT/packaging/iso/$builder"
    check "$builder applies the live network config" \
      grep -qE '^slopos_apply_live_network ' "$ROOT/packaging/iso/$builder"
  done
}

echo "== static: overlay and builders =="
check "overlay directory exists" test -d "$OVERLAY"
verify_tree "$OVERLAY"
verify_builders

if [[ -n "$ROOTFS" ]]; then
  [[ -d "$ROOTFS" ]] || { echo "check-live-network-config: $ROOTFS is not a directory" >&2; exit 2; }
  ROOTFS="$(cd "$ROOTFS" && pwd)"
  echo "== built tree: $ROOTFS =="
  verify_tree "$ROOTFS"
  check "resolv.conf rule points at the systemd-resolved stub" resolv_rule_present "$ROOTFS"
  check "NetworkManager is enabled" unit_enabled "$ROOTFS" NetworkManager.service
  check "systemd-resolved is enabled" unit_enabled "$ROOTFS" systemd-resolved.service
  check "NetworkManager unit file is installed" test -e "$ROOTFS/usr/lib/systemd/system/NetworkManager.service"
  check "systemd-resolved unit file is installed" test -e "$ROOTFS/usr/lib/systemd/system/systemd-resolved.service"
fi

if [[ $failures -gt 0 ]]; then
  echo "live network config: $failures check(s) FAILED"
  exit 1
fi
echo "live network config: all checks passed"

#!/usr/bin/env bash
# install-nvidia-driver.sh — install the proprietary NVIDIA driver, and only
# when an NVIDIA display controller is present.
#
# Runs in two places:
#   - the installed system, from Calamares shellprocess (inside the target);
#   - the live session, once at boot (slopos-nvidia-driver.service).
#
# The package comes from driver.conf, next to this installer's files. Nothing
# is installed on a machine without an NVIDIA GPU, or where /proc already shows
# the proprietary module, or where no line matches the distribution.
#
# Usage: install-nvidia-driver.sh [--check]
#   --check  report what would happen and change nothing.
#
# The environment variables let the static checks point the helper at fixtures.
set -euo pipefail

CONFIG="${SLOPOS_NVIDIA_CONFIG:-/usr/share/slopos/nvidia/driver.conf}"
PCI_ROOT="${SLOPOS_PCI_DEVICES:-/sys/bus/pci/devices}"
OS_RELEASE="${SLOPOS_OS_RELEASE:-/etc/os-release}"
PROPRIETARY_MARKER="${SLOPOS_NVIDIA_PROC:-/proc/driver/nvidia/version}"

check_only=0
case "${1:-}" in
  "") ;;
  --check) check_only=1 ;;
  *) echo "install-nvidia-driver: unknown option $1" >&2; exit 2 ;;
esac

log() { printf 'slopos-nvidia: %s\n' "$*"; }

# True when a PCI display controller from NVIDIA is present. Same rule as the
# Rust detection in crates/slopos-services/src/gpu.rs: vendor 0x10de, class 0x03.
nvidia_gpu_present() {
  local dir vendor class
  [[ -d "$PCI_ROOT" ]] || return 2
  for dir in "$PCI_ROOT"/*/; do
    [[ -r "$dir/vendor" && -r "$dir/class" ]] || continue
    vendor="$(<"$dir/vendor")"
    class="$(<"$dir/class")"
    if [[ "$vendor" == 0x10de && "$class" == 0x03* ]]; then
      return 0
    fi
  done
  return 1
}

# The package named for this distribution in CONFIG, or nothing.
configured_package() {
  local id="" codename=""
  id="$(sed -n 's/^ID=//p' "$OS_RELEASE" | tr -d '"' | head -1)"
  codename="$(sed -n 's/^VERSION_CODENAME=//p' "$OS_RELEASE" | tr -d '"' | head -1)"
  [[ -n "$id" && -n "$codename" && -r "$CONFIG" ]] || return 0
  awk -v id="$id" -v codename="$codename" '
    /^[[:space:]]*(#|$)/ { next }
    $1 == id && $2 == codename { print $3; exit }
  ' "$CONFIG"
}

package_installed() {
  [[ "$(dpkg-query -W -f='${Status}' "$1" 2>/dev/null || true)" == "install ok installed" ]]
}

nvidia_gpu_present && present=0 || present=$?
case "$present" in
  0) ;;
  1) log "no NVIDIA GPU detected; the proprietary driver is not installed"; exit 0 ;;
  *) log "PCI devices are unavailable; the proprietary driver is not installed"; exit 0 ;;
esac

if [[ -e "$PROPRIETARY_MARKER" ]]; then
  log "the proprietary NVIDIA module is already loaded; nothing to do"
  exit 0
fi

package="$(configured_package)"
if [[ -z "$package" ]]; then
  log "no proprietary driver package is configured for this release; nothing installed"
  exit 0
fi

if [[ $check_only -eq 1 ]]; then
  log "NVIDIA GPU detected; would install $package"
  exit 0
fi

if package_installed "$package"; then
  log "$package is already installed; reboot to load the driver"
  exit 0
fi

log "NVIDIA GPU detected; installing $package"
export DEBIAN_FRONTEND=noninteractive
if ! apt-get update; then
  log "apt-get update failed; $package was not installed"
  exit 1
fi
if ! apt-get install -y "$package"; then
  log "apt-get install $package failed; the system keeps the open driver"
  exit 1
fi
log "installed $package; reboot to load the proprietary driver"
exit 0

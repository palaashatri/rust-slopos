#!/usr/bin/env bash
# check-live-installer-config.sh — verify the Calamares installer that the SLOPOS
# live ISO builder adds: the packages in packaging/iso/build-live-iso.sh, the
# configuration in packaging/iso/installer/, the polkit rule, the desktop entry,
# the branding and the ledger row.
#
# Usage: scripts/check-live-installer-config.sh
#
# Needs no root and no build. It stages the installer files into a temporary
# directory with the same helper the builder calls, then checks that tree. It
# never runs Calamares or installs to a disk: that install is NOT_RUN here.
#
# Prints PASS, FAIL or NOT_RUN for each check and exits 1 if any check fails.
# A check that cannot be read counts as FAIL.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALLER="$ROOT/packaging/iso/installer"
BUILDER="$ROOT/packaging/iso/build-live-iso.sh"
WORKFLOW="$ROOT/.github/workflows/live-iso.yml"
SETTINGS="$INSTALLER/settings.conf"
RULE="$INSTALLER/polkit/49-slopos-live-installer.rules"
STRIP="$INSTALLER/strip-live-system.sh"
LEDGER="$ROOT/docs/contract/ledger.md"
failures=0

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

not_run() {
  printf 'NOT_RUN  %s\n' "$1"
}

# Stock Calamares 3.3 module names. Anything else in the sequence is a typo or
# a module this image does not ship.
STOCK_MODULES=(welcome locale keyboard partition users summary mount unpackfs
  machineid fstab localecfg networkcfg hwclock shellprocess initramfs bootloader
  umount displaymanager finished)

# Modules the installer cannot run without.
REQUIRED_SEQUENCE=(welcome locale keyboard partition users displaymanager
  networkcfg bootloader unpackfs machineid fstab shellprocess mount umount
  summary finished)

# Modules that need a file in modules/ for their options.
CONFIGURED_MODULES=(welcome partition users displaymanager bootloader unpackfs
  machineid fstab finished shellprocess)

# The package block of the live image (the apt-get install in step 2).
apt_block() {
  sed -n '/installing kernel, live boot and desktop dependencies/,/apt-get clean/p' "$BUILDER"
}

# Packages the installer needs, each as its own word in that block.
package_listed() {
  local block tokens
  block="$(apt_block)"
  tokens="$(printf '%s' "$block" | tr ' \t\\' '\n\n\n')"
  grep -qxF -- "$1" <<<"$tokens"
}

# Module names in the sequence: bare "- name" lines, not "- show:" or "- exec:".
sequence_modules() {
  local seq
  seq="$(sed -n '/^sequence:/,/^[a-z]/p' "$SETTINGS")"
  grep -E '^[[:space:]]+-[[:space:]]+[a-z0-9-]+[[:space:]]*$' <<<"$seq" |
    sed -E 's/^[[:space:]]+-[[:space:]]+//; s/[[:space:]]*$//' || true
}

# Module names in the exec block only, in order (from "- exec:" to the last "- show:").
exec_modules() {
  local block
  block="$(sed -n '/^  - exec:/,/^  - show:/p' "$SETTINGS")"
  grep -E '^[[:space:]]+-[[:space:]]+[a-z0-9-]+[[:space:]]*$' <<<"$block" |
    sed -E 's/^[[:space:]]+-[[:space:]]+//; s/[[:space:]]*$//' || true
}

exec_position() {  # NAME: 1-based position in the exec block
  local modules
  modules="$(exec_modules)"
  awk -v n="$1" '$0 == n { print NR; found = 1; exit } END { exit !found }' <<<"$modules"
}

exec_runs_after() {  # A B: A is run after B in the exec block
  local a b
  a="$(exec_position "$1")" && b="$(exec_position "$2")" || return 1
  [[ -n "$a" && -n "$b" && "$a" -gt "$b" ]]
}

exec_runs_before() {  # A B: A is run before B in the exec block
  local a b
  a="$(exec_position "$1")" && b="$(exec_position "$2")" || return 1
  [[ -n "$a" && -n "$b" && "$a" -lt "$b" ]]
}

in_list() {  # NAME LIST...: exact match
  local needle="$1" item
  shift
  for item in "$@"; do [[ "$item" == "$needle" ]] && return 0; done
  return 1
}

sequence_has() {  # MODULE: listed in the sequence
  local seq
  seq="$(sequence_modules)"
  grep -qxF -- "$1" <<<"$seq"
}

sequence_all_stock() {
  local m seq
  seq="$(sequence_modules)"
  [[ -n "$seq" ]] || return 1
  while IFS= read -r m; do
    in_list "$m" "${STOCK_MODULES[@]}" || { echo "  unknown module: $m" >&2; return 1; }
  done <<<"$seq"
}

module_file_referenced() {  # FILE: modules/NAME.conf whose NAME is in the sequence
  local name
  name="$(basename "$1" .conf)"
  sequence_has "$name"
}

branding_value() {  # KEY: value of a "key: value" line in branding.desc, quotes removed
  sed -nE "s/^[[:space:]]*$1:[[:space:]]*\"?([^\"]*)\"?[[:space:]]*$/\1/p" \
    "$INSTALLER/branding/slopos/branding.desc" | head -1
}

branding_image_exists() {  # KEY: the file named by an images/slideshow key exists
  local value
  value="$(branding_value "$1")"
  [[ -n "$value" && -f "$INSTALLER/branding/slopos/$value" ]]
}

desktop_entry_ok() {
  local f="$INSTALLER/desktop/slopos-install.desktop"
  [[ -f "$f" ]] &&
    grep -qxF 'Type=Application' "$f" &&
    grep -qxF 'Name=Install SLOPOS-I' "$f" &&
    grep -qxF 'Exec=pkexec /usr/bin/calamares' "$f" &&
    grep -qxF 'Categories=System;' "$f"
}

# Finding 1: the squashfs keeps /boot, so the installed system gets a kernel.
squashfs_keeps_boot() {
  ! grep -qE '^mksquashfs.*-e boot' "$BUILDER" &&
    grep -qE '^mksquashfs ' "$BUILDER"
}

# Finding 2: the polkit rule lets only the live user run calamares, and only in
# the active local session, and its user is the casper live user.
polkit_rule_scoped() {
  [[ -f "$RULE" ]] &&
    grep -qF 'action.lookup("program") == "/usr/bin/calamares"' "$RULE" &&
    grep -qF 'subject.user == "slopos"' "$RULE" &&
    grep -qF 'subject.local && subject.active' "$RULE" &&
    grep -qF 'polkit.Result.YES' "$RULE" &&
    ! grep -qE 'polkit[.]Result[.](AUTH_SELF|AUTH_ADMIN)' "$RULE"
}

polkit_user_matches_live_user() {  # the rule's subject.user is the casper live user
  grep -qF 'export USERNAME="slopos"' "$BUILDER" &&
    grep -qF 'subject.user == "slopos"' "$RULE"
}

# Finding 3: casper is removed from the installed system before the initramfs job.
strip_removes_casper() {
  grep -qF 'purge casper' "$STRIP" &&
    grep -qF 'default-boot-to-casper.conf' "$STRIP" &&
    grep -qF '/etc/casper.conf' "$STRIP"
}

strip_removes_live_only_files() {
  grep -qF '49-slopos-live-installer.rules' "$STRIP" &&
    grep -qF 'slopos-install.desktop' "$STRIP" &&
    grep -qF '/etc/skel/Desktop/slopos-install.desktop' "$STRIP"
}

# Finding 4: displaymanager runs after users, and it does not write autologin itself.
displaymanager_config_ok() {
  local f="$INSTALLER/modules/displaymanager.conf"
  [[ -f "$f" ]] &&
    grep -qxF '  - lightdm' "$f" &&
    ! grep -qE '^[^#]*autologin' "$f"
}

users_autologin_off_by_default() {
  grep -qE '^doAutologin:[[:space:]]+false[[:space:]]*$' "$INSTALLER/modules/users.conf" &&
    grep -qE '^autologinGroup:' "$INSTALLER/modules/users.conf"
}

# Finding 5: the ledger has a dated row for this installer, and it says the install is NOT_RUN.
ledger_row_ok() {
  grep -qE '^\| [0-9]{4}-[0-9]{2}-[0-9]{2} \| INSTALLER-CALAMARES \|.*NOT_RUN' "$LEDGER"
}

# Runs the real helper against a temporary root and checks the result.
stage_tree() {
  local tmp rc=0
  tmp="$(mktemp -d)"
  (
    # shellcheck source=packaging/iso/installer/live-installer.sh
    . "$INSTALLER/live-installer.sh"
    slopos_apply_live_installer "$tmp"
  ) || rc=$?
  if [[ $rc -eq 0 ]]; then
    local f
    for f in etc/calamares/settings.conf \
             usr/share/calamares/branding/slopos/branding.desc \
             usr/share/calamares/branding/slopos/show.qml \
             usr/share/calamares/branding/slopos/logo.svg \
             usr/share/calamares/branding/slopos/welcome.svg \
             usr/share/applications/slopos-install.desktop \
             etc/skel/Desktop/slopos-install.desktop \
             etc/polkit-1/rules.d/49-slopos-live-installer.rules \
             usr/libexec/slopos-install/strip-live-system.sh \
             usr/libexec/slopos-nvidia/install-nvidia-driver.sh \
             usr/share/slopos/nvidia/driver.conf \
             usr/lib/systemd/system/slopos-nvidia-driver.service; do
      [[ -e "$tmp/$f" ]] || { echo "  not staged: $f" >&2; rc=1; }
    done
    local m
    for m in "${CONFIGURED_MODULES[@]}"; do
      [[ -f "$tmp/etc/calamares/modules/$m.conf" ]] || { echo "  not staged: $m.conf" >&2; rc=1; }
    done
    [[ -x "$tmp/usr/libexec/slopos-install/strip-live-system.sh" ]] || rc=1
  fi
  rm -rf "$tmp"
  return "$rc"
}

echo "== static: package list (packaging/iso/build-live-iso.sh) =="
check "build-live-iso.sh has the live package block" test -n "$(apt_block)"
for pkg in calamares pkexec squashfs-tools rsync gdisk dosfstools parted efibootmgr grub2-common; do
  check "live image installs $pkg" package_listed "$pkg"
done

echo "== static: installer configuration (packaging/iso/installer) =="
check "settings.conf exists" test -f "$SETTINGS"
check "settings.conf selects the slopos branding" grep -qxF 'branding: slopos' "$SETTINGS"
check "settings.conf uses the local module search path" grep -qxF 'modules-search: [ local ]' "$SETTINGS"
check "settings.conf has a show, exec and show sequence" \
  bash -c "grep -qxF '  - show:' '$SETTINGS' && grep -qxF '  - exec:' '$SETTINGS'"
check "every sequence module is a stock Calamares module" sequence_all_stock
for m in "${REQUIRED_SEQUENCE[@]}"; do
  check "sequence runs $m" sequence_has "$m"
done
for m in "${CONFIGURED_MODULES[@]}"; do
  check "modules/$m.conf exists" test -f "$INSTALLER/modules/$m.conf"
  check "sequence references $m (for modules/$m.conf)" sequence_has "$m"
done
for f in "$INSTALLER"/modules/*.conf; do
  check "modules/$(basename "$f") names a module in the sequence" module_file_referenced "$f"
  check "modules/$(basename "$f") names a stock module" \
    in_list "$(basename "$f" .conf)" "${STOCK_MODULES[@]}"
done
check "partition defaults to erase" grep -qxF 'initialPartitioningChoice: erase' "$INSTALLER/modules/partition.conf"
check "partition hides manual partitioning" grep -qxF 'allowManualPartitioning: false' "$INSTALLER/modules/partition.conf"
check "bootloader installs GRUB for UEFI and BIOS" \
  bash -c "grep -qxF 'efiBootLoader: \"grub\"' '$INSTALLER/modules/bootloader.conf' && grep -qxF 'grubInstall: \"grub-install\"' '$INSTALLER/modules/bootloader.conf'"
check "unpackfs reads squashfs from the path build-live-iso.sh writes" \
  bash -c "grep -qF 'sourcefs: squashfs' '$INSTALLER/modules/unpackfs.conf' && grep -qF '/cdrom/casper/filesystem.squashfs' '$INSTALLER/modules/unpackfs.conf' && grep -qF 'casper/filesystem.squashfs' '$BUILDER'"
check "finding 1: the squashfs keeps /boot, so the installed system gets a kernel" squashfs_keeps_boot
check "finding 2: pkexec for calamares is allowed by a scoped polkit rule" polkit_rule_scoped
check "finding 2: the polkit rule's user is the casper live user" polkit_user_matches_live_user
if command -v node >/dev/null 2>&1; then
  # node --check refuses the .rules extension, so compile the file with vm.Script.
  check "finding 2: the polkit rule parses as JavaScript" \
    node -e "new (require('vm').Script)(require('fs').readFileSync(process.argv[1], 'utf8'))" "$RULE"
else
  not_run "finding 2: the polkit rule parses as JavaScript (node is not installed)"
fi
check "finding 3: the cleanup script purges casper and its boot-to-casper config" strip_removes_casper
check "the cleanup script removes the launchers and the polkit rule from the installed system" strip_removes_live_only_files
check "shellprocess runs the cleanup script" \
  grep -qF '/usr/libexec/slopos-install/strip-live-system.sh' "$INSTALLER/modules/shellprocess.conf"
check "finding 3: the cleanup runs before the initramfs job" exec_runs_before shellprocess initramfs
check "finding 4: displaymanager runs after users" exec_runs_after displaymanager users
check "finding 4: displaymanager.conf sets up lightdm and leaves autologin to the users page" displaymanager_config_ok
check "finding 4: automatic login is offered on the users page and is off by default" users_autologin_off_by_default

echo "== static: branding, desktop entry and helper =="
check "branding/slopos/branding.desc exists" test -f "$INSTALLER/branding/slopos/branding.desc"
check "branding names the product SLOPOS-I" grep -qxF '    productName:         "SLOPOS-I"' "$INSTALLER/branding/slopos/branding.desc"
check "branding product icon exists" branding_image_exists productIcon
check "branding product logo exists" branding_image_exists productLogo
check "branding welcome image exists" branding_image_exists productWelcome
check "branding slideshow exists" branding_image_exists slideshow
check "desktop entry 'Install SLOPOS-I' runs calamares through pkexec" desktop_entry_ok
check "helper installs the desktop entry in the menu" \
  grep -qF 'usr/share/applications/slopos-install.desktop' "$INSTALLER/live-installer.sh"
check "helper installs the desktop entry on the live desktop" \
  grep -qF 'etc/skel/Desktop/slopos-install.desktop' "$INSTALLER/live-installer.sh"
check "helper installs the polkit rule in the live root" \
  grep -qF 'etc/polkit-1/rules.d/49-slopos-live-installer.rules' "$INSTALLER/live-installer.sh"
check "live-installer.sh parses" bash -n "$INSTALLER/live-installer.sh"
check "strip-live-system.sh parses as sh" sh -n "$STRIP"
check "build-live-iso.sh sources the installer helper" \
  grep -qF 'packaging/iso/installer/live-installer.sh' "$BUILDER"
check "build-live-iso.sh applies the installer to the live root" \
  grep -qE '^slopos_apply_live_installer ' "$BUILDER"
check "staging the installer into a temporary root succeeds" stage_tree

echo "== static: CI wiring (.github/workflows/live-iso.yml) =="
check "live-iso.yml runs the installer check" grep -qF 'scripts/check-live-installer-config.sh' "$WORKFLOW"
check "installer check runs before the ISO build" bash -c "
  check_line=\$(grep -nF 'scripts/check-live-installer-config.sh' '$WORKFLOW' | head -1 | cut -d: -f1)
  build_line=\$(grep -nE '^[[:space:]]+sudo .*packaging/iso/build-live-iso[.]sh' '$WORKFLOW' | head -1 | cut -d: -f1)
  [[ -n \"\$check_line\" && -n \"\$build_line\" && \$check_line -lt \$build_line ]]"

echo "== static: evidence =="
check "finding 5: ledger has a dated INSTALLER-CALAMARES row that says the install is NOT_RUN" ledger_row_ok

NVIDIA_DIR="$INSTALLER/nvidia"
NVIDIA_CONF="$NVIDIA_DIR/driver.conf"
NVIDIA_HELPER="$NVIDIA_DIR/install-nvidia-driver.sh"
NVIDIA_UNIT="$NVIDIA_DIR/slopos-nvidia-driver.service"
NVIDIA_LEDGER_ID="NVIDIA-DRIVER-SUPPORT"

# Data lines of driver.conf: no comments, no blank lines.
nvidia_conf_data() { grep -Ev '^[[:space:]]*(#|$)' "$NVIDIA_CONF"; }

nvidia_conf_format_ok() {  # every data line is "<ID> <VERSION_CODENAME> <package>"
  local lines
  lines="$(nvidia_conf_data)" || return 1
  [[ -n "$lines" ]] || return 1
  awk 'NF != 3 { exit 1 }' <<<"$lines"
}

nvidia_conf_has() {  # ID CODENAME PACKAGE_REGEX: a line names a matching package
  awk -v id="$1" -v codename="$2" -v re="$3" \
    '$1 == id && $2 == codename && $3 ~ re { found = 1 } END { exit !found }' \
    <<<"$(nvidia_conf_data)"
}

# Runs the helper in --check mode against fixtures: a PCI tree with one device
# (VENDOR, CLASS), an os-release (ID, CODENAME), and optionally the proprietary
# marker. Passes when it exits 0 and prints EXPECT.
helper_check_case() {  # VENDOR CLASS ID CODENAME MARKER(yes|no) EXPECT
  local tmp rc=0 out
  tmp="$(mktemp -d)"
  mkdir -p "$tmp/pci/0000:01:00.0"
  printf '%s\n' "$1" >"$tmp/pci/0000:01:00.0/vendor"
  printf '%s\n' "$2" >"$tmp/pci/0000:01:00.0/class"
  printf 'ID=%s\nVERSION_CODENAME=%s\n' "$3" "$4" >"$tmp/os-release"
  if [[ "$5" == yes ]]; then : >"$tmp/proprietary-version"; fi
  out="$(SLOPOS_PCI_DEVICES="$tmp/pci" SLOPOS_OS_RELEASE="$tmp/os-release" \
    SLOPOS_NVIDIA_CONFIG="$NVIDIA_CONF" SLOPOS_NVIDIA_PROC="$tmp/proprietary-version" \
    bash "$NVIDIA_HELPER" --check 2>&1)" || rc=$?
  rm -rf "$tmp"
  [[ $rc -eq 0 ]] && grep -qF -- "$6" <<<"$out"
}

helper_check_missing_pci_tree() {  # the PCI tree cannot be read: nothing installed, exit 0
  local tmp rc=0 out
  tmp="$(mktemp -d)"
  printf 'ID=ubuntu\nVERSION_CODENAME=noble\n' >"$tmp/os-release"
  out="$(SLOPOS_PCI_DEVICES="$tmp/absent" SLOPOS_OS_RELEASE="$tmp/os-release" \
    SLOPOS_NVIDIA_CONFIG="$NVIDIA_CONF" SLOPOS_NVIDIA_PROC="$tmp/absent-version" \
    bash "$NVIDIA_HELPER" --check 2>&1)" || rc=$?
  rm -rf "$tmp"
  [[ $rc -eq 0 ]] && grep -qF 'PCI devices are unavailable' <<<"$out"
}

helper_rejects_unknown_option() {
  local rc=0
  bash "$NVIDIA_HELPER" --bogus >/dev/null 2>&1 || rc=$?
  [[ $rc -eq 2 ]]
}

ledger_nvidia_row_ok() {
  grep -qE "^\| [0-9]{4}-[0-9]{2}-[0-9]{2} \| $NVIDIA_LEDGER_ID \|.*NOT_RUN" "$LEDGER"
}

echo "== static: NVIDIA proprietary driver (packaging/iso/installer/nvidia) =="
check "driver.conf exists" test -f "$NVIDIA_CONF"
check "driver.conf has one '<ID> <VERSION_CODENAME> <package>' line per release" nvidia_conf_format_ok
check "driver.conf names an nvidia-driver-<branch> package for ubuntu noble" \
  nvidia_conf_has ubuntu noble '^nvidia-driver-[0-9]+$'
check "the live ISO suite (build-live-iso.sh) is the noble release driver.conf covers" \
  grep -qxF 'SUITE=noble' "$BUILDER"
check "driver.conf names nvidia-driver (non-free) for debian trixie" \
  nvidia_conf_has debian trixie '^nvidia-driver$'
check "driver.conf names nvidia-driver (non-free) for debian bookworm" \
  nvidia_conf_has debian bookworm '^nvidia-driver$'
check "install-nvidia-driver.sh parses" bash -n "$NVIDIA_HELPER"
check "the helper names no driver package (packages are in driver.conf)" \
  bash -c "! grep -qE 'nvidia-driver-[0-9]|nvidia-driver([[:space:]]|\$)' '$NVIDIA_HELPER'"
check "the helper detects NVIDIA by PCI vendor 0x10de and display class 0x03" \
  bash -c "grep -qF '0x10de' '$NVIDIA_HELPER' && grep -qF '0x03' '$NVIDIA_HELPER'"
check "the helper reads driver.conf from the installed path" \
  grep -qF '/usr/share/slopos/nvidia/driver.conf' "$NVIDIA_HELPER"
check "the helper skips when the proprietary module is loaded (/proc/driver/nvidia/version)" \
  grep -qF '/proc/driver/nvidia/version' "$NVIDIA_HELPER"
check "helper: NVIDIA GPU on ubuntu noble would install the configured package" \
  helper_check_case 0x10de 0x030000 ubuntu noble no "would install nvidia-driver-570"
check "helper: an NVIDIA 3D controller (class 0x0302) counts as a GPU" \
  helper_check_case 0x10de 0x030200 ubuntu noble no "would install nvidia-driver-570"
check "helper: debian trixie would install nvidia-driver" \
  helper_check_case 0x10de 0x030000 debian trixie no "would install nvidia-driver"
check "helper: no NVIDIA GPU installs nothing" \
  helper_check_case 0x8086 0x030000 ubuntu noble no "no NVIDIA GPU detected"
check "helper: an NVIDIA network card is not a GPU" \
  helper_check_case 0x10de 0x020000 ubuntu noble no "no NVIDIA GPU detected"
check "helper: an already loaded proprietary module is left alone" \
  helper_check_case 0x10de 0x030000 ubuntu noble yes "already loaded"
check "helper: a release without a line installs nothing" \
  helper_check_case 0x10de 0x030000 fedora "" no "no proprietary driver package is configured"
check "helper: an unreadable PCI tree installs nothing and says so" helper_check_missing_pci_tree
check "helper: an unknown option is refused" helper_rejects_unknown_option
check "slopos-nvidia-driver.service runs the installed helper" \
  grep -qxF 'ExecStart=/usr/libexec/slopos-nvidia/install-nvidia-driver.sh' "$NVIDIA_UNIT"
check "slopos-nvidia-driver.service waits for the network" \
  grep -qxF 'Wants=network-online.target' "$NVIDIA_UNIT"
check "shellprocess runs the NVIDIA helper" \
  grep -qF '/usr/libexec/slopos-nvidia/install-nvidia-driver.sh' "$INSTALLER/modules/shellprocess.conf"
check "the NVIDIA helper runs after the live cleanup in shellprocess" \
  bash -c "
  strip_line=\$(grep -nF 'strip-live-system.sh' '$INSTALLER/modules/shellprocess.conf' | head -1 | cut -d: -f1)
  nvidia_line=\$(grep -nF 'install-nvidia-driver.sh' '$INSTALLER/modules/shellprocess.conf' | head -1 | cut -d: -f1)
  [[ -n \"\$strip_line\" && -n \"\$nvidia_line\" && \$strip_line -lt \$nvidia_line ]]"
check "the cleanup script removes the live-only NVIDIA boot unit and its enable link" \
  bash -c "grep -qF 'rm -f /usr/lib/systemd/system/slopos-nvidia-driver.service' '$STRIP' && grep -qF 'rm -f /etc/systemd/system/multi-user.target.wants/slopos-nvidia-driver.service' '$STRIP'"
check "live-installer.sh stages the NVIDIA helper, driver.conf and boot unit" \
  bash -c "grep -qF 'nvidia/install-nvidia-driver.sh' '$INSTALLER/live-installer.sh' && grep -qF 'nvidia/driver.conf' '$INSTALLER/live-installer.sh' && grep -qF 'nvidia/slopos-nvidia-driver.service' '$INSTALLER/live-installer.sh'"
check "evidence: ledger has a dated NVIDIA-DRIVER-SUPPORT row that says the RTX 3080 run is NOT_RUN" ledger_nvidia_row_ok
not_run "the proprietary driver installs and loads on an RTX 3080 (live ISO and installed system)"

echo "== YAML syntax =="
if python3 -c 'import yaml' 2>/dev/null; then
  check "settings.conf and modules/*.conf parse as YAML" \
    python3 -c 'import sys, yaml
for path in sys.argv[1:]:
    with open(path) as handle:
        yaml.safe_load(handle)' "$SETTINGS" "$INSTALLER"/modules/*.conf
  check "branding.desc parses as YAML" python3 -c 'import sys, yaml
with open(sys.argv[1]) as handle:
    yaml.safe_load(handle)' "$INSTALLER/branding/slopos/branding.desc"
else
  not_run "YAML parse (PyYAML is not installed on this host)"
fi

if [[ $failures -gt 0 ]]; then
  echo "live installer config: $failures check(s) FAILED"
  exit 1
fi
echo "live installer config: all checks passed (the disk install itself is NOT_RUN)"

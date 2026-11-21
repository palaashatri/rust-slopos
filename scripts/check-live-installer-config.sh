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
             usr/libexec/slopos-install/strip-live-system.sh; do
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

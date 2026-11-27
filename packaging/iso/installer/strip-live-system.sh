#!/bin/sh
# Runs in the installed system: Calamares shellprocess, after users and before
# the initramfs job, so the initrd that job builds is clean. It removes what
# only the live session needs:
#   - the casper package and its configuration. casper's initramfs default
#     (BOOT=casper) would make the installed disk boot through the live path;
#   - the "Install SLOPOS-I" launchers (menu, /etc/skel and every home);
#   - the passwordless polkit rule for the installer;
#   - the live session's NVIDIA boot unit (the installed system runs the
#     driver helper from shellprocess instead).
# Missing files are not an error.
set -eu

if dpkg -s casper >/dev/null 2>&1; then
    DEBIAN_FRONTEND=noninteractive apt-get -y purge casper
fi
rm -f /etc/casper.conf
rm -f /etc/polkit-1/rules.d/49-slopos-live-installer.rules
rm -f /usr/lib/systemd/system/slopos-nvidia-driver.service
rm -f /etc/systemd/system/multi-user.target.wants/slopos-nvidia-driver.service
find /etc /usr/share /usr/lib -name default-boot-to-casper.conf -exec rm -f {} + || true

rm -f /usr/share/applications/slopos-install.desktop
rm -f /etc/skel/Desktop/slopos-install.desktop
for home in /home/*; do
    [ -d "$home" ] || continue
    rm -f "$home/Desktop/slopos-install.desktop"
done

#!/bin/sh
# Runs in the installed system (Calamares shellprocess, after users). Removes
# the live-only "Install SLOPOS-I" launchers from the menu, /etc/skel and every
# home directory created during install. Missing files are not an error.
set -eu
rm -f /usr/share/applications/slopos-install.desktop
rm -f /etc/skel/Desktop/slopos-install.desktop
for home in /home/*; do
    [ -d "$home" ] || continue
    rm -f "$home/Desktop/slopos-install.desktop"
done

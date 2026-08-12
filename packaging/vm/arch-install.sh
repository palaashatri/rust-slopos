#!/usr/bin/env bash
# Unattended Arch Linux installer for the SLOPOS-I X11 verification VM.
#
# This is intentionally a QA image builder, not an end-user OS installer. It
# creates a small UEFI VM that auto-logs in on tty1 and starts Xorg + SLOPOS so
# boot/session evidence can be collected reproducibly.
set -euo pipefail

DISK="${DISK:-/dev/sda}"
HOSTNAME="${HOSTNAME:-slopos-i-vm}"
USERNAME="${USERNAME:-retro}"
PASSWORD="${PASSWORD:-retro}"
REPO_URL="${REPO_URL:-https://github.com/palaashatri/rust-slopos.git}"
REPO_BRANCH="${REPO_BRANCH:-pivot}"
HOST_HTTP="${HOST_HTTP:-http://10.0.2.2:8000}"
GUEST_TARGET_DIR="${CARGO_TARGET_DIR:-/home/$USERNAME/.cache/slopos-i/cargo-target}"

case "$GUEST_TARGET_DIR" in
  /*) ;;
  *) echo "CARGO_TARGET_DIR must be absolute: $GUEST_TARGET_DIR" >&2; exit 2 ;;
esac

if [[ $EUID -ne 0 ]]; then
  echo "Run this script as root from the Arch ISO." >&2
  exit 1
fi
if [[ ! -b "$DISK" ]]; then
  echo "Target disk does not exist: $DISK" >&2
  exit 1
fi

echo "=== enable clock and current keyring ==="
timedatectl set-ntp true || true
pacman -Sy --noconfirm archlinux-keyring

echo "=== partition $DISK: 512 MiB ESP + root ==="
sgdisk --zap-all "$DISK"
sgdisk -n 1:0:+512M -t 1:ef00 -c 1:EFI "$DISK"
sgdisk -n 2:0:0 -t 2:8300 -c 2:ROOT "$DISK"
partprobe "$DISK"
sleep 2
mkfs.fat -F32 "${DISK}1"
mkfs.ext4 -F "${DISK}2"
mount "${DISK}2" /mnt
mkdir -p /mnt/boot
mount "${DISK}1" /mnt/boot

echo "=== install base system, X11 and representative desktop applications ==="
pacstrap -K /mnt \
  base linux linux-firmware networkmanager sudo git curl base-devel rust pkgconf \
  xorg-server xorg-xinit xorg-xsetroot xorg-xdpyinfo openbox \
  gtk3 libx11 libxrandr openssl dbus librsvg \
  ttf-dejavu ttf-liberation \
  pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator \
  pavucontrol nm-connection-editor blueman xfce4-power-manager lxappearance \
  xdotool wmctrl scrot imagemagick \
  pipewire pipewire-pulse wireplumber upower bluez \
  openssh grub efibootmgr

genfstab -U /mnt >>/mnt/etc/fstab

echo "=== configure installed system ==="
arch-chroot /mnt /bin/bash -euo pipefail <<CHROOT
ln -sf /usr/share/zoneinfo/UTC /etc/localtime
hwclock --systohc
echo 'en_US.UTF-8 UTF-8' >/etc/locale.gen
locale-gen
echo 'LANG=en_US.UTF-8' >/etc/locale.conf
echo '$HOSTNAME' >/etc/hostname
cat >/etc/hosts <<EOF
127.0.0.1 localhost
::1 localhost
127.0.1.1 $HOSTNAME.localdomain $HOSTNAME
EOF

echo 'root:$PASSWORD' | chpasswd
useradd -m -G wheel,video,input -s /bin/bash '$USERNAME'
echo '$USERNAME:$PASSWORD' | chpasswd
install -Dm440 /dev/stdin /etc/sudoers.d/10-wheel <<'EOF'
%wheel ALL=(ALL:ALL) NOPASSWD: ALL
EOF

systemctl enable NetworkManager
systemctl enable sshd
systemctl enable bluetooth || true

grub-install --target=x86_64-efi --efi-directory=/boot --bootloader-id=SLOPOS-QA --removable
sed -i 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=1/' /etc/default/grub
grub-mkconfig -o /boot/grub/grub.cfg

mkdir -p /etc/systemd/system/getty@tty1.service.d
cat >/etc/systemd/system/getty@tty1.service.d/autologin.conf <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty --noclear --autologin $USERNAME %I \$TERM
EOF
CHROOT

# Optional host-served SSH key. Password access remains available for this QA VM.
if curl -fsS "$HOST_HTTP/qa_key.pub" -o /tmp/slopos-qa-key 2>/dev/null; then
  install -d -m700 "/mnt/home/$USERNAME/.ssh"
  install -m600 /tmp/slopos-qa-key "/mnt/home/$USERNAME/.ssh/authorized_keys"
  chown -R 1000:1000 "/mnt/home/$USERNAME/.ssh"
fi

echo "=== clone, build and install current X11 product ==="
arch-chroot /mnt /bin/bash -euo pipefail <<CHROOT
runuser -u '$USERNAME' -- bash -lc '
  set -euo pipefail
  git clone --depth 1 --branch "$REPO_BRANCH" "$REPO_URL" ~/slopos-i
  cd ~/slopos-i
  export CARGO_TARGET_DIR="$GUEST_TARGET_DIR"
  mkdir -p "\$CARGO_TARGET_DIR"
  cargo build --release --workspace --locked
  cargo test --release --workspace --locked
'
cd "/home/$USERNAME/slopos-i"
CARGO_TARGET_DIR="$GUEST_TARGET_DIR" ./install.sh --no-deps --no-build --distro arch

cat >"/home/$USERNAME/.xinitrc" <<'EOF'
#!/bin/sh
exec /usr/local/bin/start-slopos-i
EOF
chmod +x "/home/$USERNAME/.xinitrc"
cat >"/home/$USERNAME/.bash_profile" <<'EOF'
if [ -z "${DISPLAY:-}" ] && [ "$(tty 2>/dev/null)" = /dev/tty1 ]; then
  exec startx -- :0 vt1 -nolisten tcp
fi
EOF
chown '$USERNAME:$USERNAME' "/home/$USERNAME/.xinitrc" "/home/$USERNAME/.bash_profile"
CHROOT

echo "=== installation complete ==="
echo "The VM will boot to tty1, start Xorg, Openbox and the SLOPOS shell automatically."
echo "QA login: $USERNAME / $PASSWORD"
umount -R /mnt
systemctl reboot

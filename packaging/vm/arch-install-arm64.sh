#!/usr/bin/env bash
# Unattended Arch (aarch64) install for the SLOPOS-I UTM verification VM.
#
# Run from the aarch64 Arch/archboot live environment:
#   curl -sL http://10.0.2.2:8000/arch-install-arm64.sh | bash
#
# Produces a machine that boots to an autologin TTY with sshd + the host's key,
# on real virtio-gpu DRM/KMS — the environment SLOPOS-I has never been run on.
set -euxo pipefail

DISK=/dev/vda                       # virtio-blk (NOT /dev/sda)
HOSTNAME=slopos-i-vm
USERNAME=retro
PASSWORD=retro
REPO_URL="${REPO_URL:-https://github.com/palaashatri/slopos-i.git}"
REPO_BRANCH="${REPO_BRANCH:-main}"
HOST_HTTP="${HOST_HTTP:-http://10.0.2.2:8000}"   # host file server (Task 0.2)

# CONFIRM AT RUNTIME: which aarch64 kernel package this live env provides.
#   pacman -Ss '^linux$' ; pacman -Ss '^linux-aarch64$'
# archboot-based aarch64 Arch typically uses `linux`; Arch Linux ARM uses
# `linux-aarch64`. Override with KERNEL_PKG=... if the default is not found.
KERNEL_PKG="${KERNEL_PKG:-linux}"

echo "=== clock + keyring ==="
timedatectl set-ntp true || true
pacman -Sy --noconfirm archlinux-keyring || true

echo "=== partition $DISK (GPT: 512M ESP + rest root) ==="
sgdisk --zap-all "$DISK"
sgdisk -n 1:0:+512M -t 1:ef00 -c 1:EFI "$DISK"
sgdisk -n 2:0:0     -t 2:8300 -c 2:ROOT "$DISK"
partprobe "$DISK"
sleep 2
mkfs.fat -F32 "${DISK}1"
mkfs.ext4 -F "${DISK}2"
mount "${DISK}2" /mnt
mkdir -p /mnt/boot
mount "${DISK}1" /mnt/boot

echo "=== pacstrap base + SLOPOS-I build/runtime deps (aarch64) ==="
pacstrap -K /mnt \
  base "$KERNEL_PKG" linux-firmware \
  networkmanager sudo vim nano git curl wget \
  base-devel pkgconf \
  rust \
  wayland wayland-protocols libxkbcommon libinput seatd libdrm mesa \
  vulkan-icd-loader vulkan-swrast vulkan-tools \
  libdisplay-info pixman \
  dbus at-spi2-core \
  fontconfig freetype2 ttf-dejavu ttf-liberation \
  pipewire pipewire-pulse wireplumber libpipewire \
  polkit \
  xorg-xwayland \
  labwc foot \
  imagemagick grim wl-clipboard \
  networkmanager nm-connection-editor \
  upower \
  openssh htop \
  qemu-guest-agent \
  grub efibootmgr

genfstab -U /mnt >> /mnt/etc/fstab

echo "=== configure the installed system in chroot ==="
arch-chroot /mnt /bin/bash -euxo pipefail <<CHROOT
ln -sf /usr/share/zoneinfo/UTC /etc/localtime
hwclock --systohc || true
echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen
locale-gen
echo "LANG=en_US.UTF-8" > /etc/locale.conf
echo "$HOSTNAME" > /etc/hostname

# Load virtio_gpu early so KMS is up at boot (virtio-gpu provides /dev/dri).
sed -i 's/^MODULES=.*/MODULES=(virtio_gpu)/' /etc/mkinitcpio.conf
mkinitcpio -P

# Users
echo "root:$PASSWORD" | chpasswd
useradd -m -G wheel,video,input,seat -s /bin/bash $USERNAME
echo "$USERNAME:$PASSWORD" | chpasswd
echo "%wheel ALL=(ALL:ALL) NOPASSWD: ALL" > /etc/sudoers.d/wheel

# Services
systemctl enable NetworkManager
systemctl enable seatd
systemctl enable sshd
systemctl enable qemu-guest-agent || true

# Bootloader: aarch64 UEFI, removable so UTM's edk2 finds BOOTAA64.EFI.
grub-install --target=arm64-efi --efi-directory=/boot --bootloader-id=GRUB --removable
sed -i 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=1/' /etc/default/grub
grub-mkconfig -o /boot/grub/grub.cfg

# Autologin retro on tty1 so the VM lands in a shell.
mkdir -p /etc/systemd/system/getty@tty1.service.d
cat > /etc/systemd/system/getty@tty1.service.d/autologin.conf <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty -o '-p -f -- \\\\u' --noclear --autologin $USERNAME %I \$TERM
EOF

# Install the host's SSH public key for the retro user.
install -d -m 700 -o $USERNAME -g $USERNAME /home/$USERNAME/.ssh
curl -sL $HOST_HTTP/qa_key.pub -o /home/$USERNAME/.ssh/authorized_keys
chown $USERNAME:$USERNAME /home/$USERNAME/.ssh/authorized_keys
chmod 600 /home/$USERNAME/.ssh/authorized_keys
CHROOT

echo "=== clone + build SLOPOS-I as $USERNAME ==="
arch-chroot /mnt /bin/bash -euxo pipefail <<CHROOT
su - $USERNAME -c '
  set -euxo pipefail
  git clone --branch "$REPO_BRANCH" "$REPO_URL" ~/slopos-i || git clone "$REPO_URL" ~/slopos-i
  mkdir -p ~/.config/slopos-i
  cat > ~/.config/slopos-i/settings.conf <<EOF
theme=classic
appearance=light
lock_password=slopos-i
EOF
'
CHROOT

echo "=== done; rebooting into the installed system ==="
umount -R /mnt
systemctl reboot

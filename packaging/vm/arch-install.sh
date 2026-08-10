#!/usr/bin/env bash
# Unattended Arch Linux install for the SLOPOS-I verification VM.
#
# Fetched and run from the archiso live environment:
#   curl -sL http://10.0.2.2:8000/arch-install.sh | bash
#
# Produces a machine that boots straight into slopos-compositor on real
# DRM/KMS (VirtualBox VMSVGA -> vmwgfx), which is the environment the
# project has never actually been tested on.
set -euxo pipefail

DISK=/dev/sda
HOSTNAME=slopos-i-vm
USERNAME=retro
PASSWORD=retro
REPO_URL="${REPO_URL:-https://github.com/palaashatri/slopos-i.git}"
REPO_BRANCH="${REPO_BRANCH:-main}"
HOST_HTTP="${HOST_HTTP:-http://10.0.2.2:8000}"   # host file server (qa_key.pub)

echo "=== clock + mirrors ==="
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

echo "=== pacstrap base system + SLOPOS-I build/runtime deps ==="
pacstrap -K /mnt \
  base linux linux-firmware \
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
  imagemagick xdotool wl-clipboard \
  networkmanager nm-connection-editor \
  upower \
  openssh htop \
  grub efibootmgr

genfstab -U /mnt >> /mnt/etc/fstab

echo "=== configure system inside chroot ==="
arch-chroot /mnt /bin/bash -euxo pipefail <<CHROOT
ln -sf /usr/share/zoneinfo/UTC /etc/localtime
hwclock --systohc
echo "en_US.UTF-8 UTF-8" > /etc/locale.gen
locale-gen
echo "LANG=en_US.UTF-8" > /etc/locale.conf
echo "$HOSTNAME" > /etc/hostname
cat > /etc/hosts <<EOF
127.0.0.1   localhost
::1         localhost
127.0.1.1   $HOSTNAME.localdomain $HOSTNAME
EOF

echo "root:$PASSWORD" | chpasswd
useradd -m -G wheel,video,input,seat -s /bin/bash $USERNAME
echo "$USERNAME:$PASSWORD" | chpasswd
echo "%wheel ALL=(ALL:ALL) NOPASSWD: ALL" > /etc/sudoers.d/wheel
chmod 440 /etc/sudoers.d/wheel

systemctl enable NetworkManager
systemctl enable seatd
systemctl enable sshd

# Bootloader (UEFI)
grub-install --target=x86_64-efi --efi-directory=/boot --bootloader-id=GRUB --removable
sed -i 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=1/' /etc/default/grub
grub-mkconfig -o /boot/grub/grub.cfg

# Autologin on tty1 so the VM lands in a shell we can drive
mkdir -p /etc/systemd/system/getty@tty1.service.d
cat > /etc/systemd/system/getty@tty1.service.d/autologin.conf <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty -o '-p -f -- \\\\u' --noclear --autologin $USERNAME %I \$TERM
EOF

# Install the host's SSH public key for the retro user (served by Task 0.2).
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
  cd ~/slopos-i
  cargo build --release --workspace 2>&1 | tail -40
  mkdir -p ~/.config/slopos-i
  cat > ~/.config/slopos-i/settings.conf <<EOF
theme=classic
appearance=light
hdr_requested=false
vrr_adaptive=false
refresh_rate=60hz
color_space=srgb
lock_password=slopos-i
EOF
'
CHROOT

echo "=== install session files ==="
arch-chroot /mnt /bin/bash -euxo pipefail <<CHROOT
install -Dm755 /home/$USERNAME/slopos-i/target/release/slopos-compositor /usr/local/bin/slopos-compositor
install -Dm755 /home/$USERNAME/slopos-i/target/release/slopos-shell      /usr/local/bin/slopos-shell
install -Dm755 /home/$USERNAME/slopos-i/target/release/slopos-lock       /usr/local/bin/slopos-lock || true
for a in finder settings textedit terminal appstore; do
  install -Dm755 /home/$USERNAME/slopos-i/target/release/\$a /usr/local/bin/\$a || true
done
install -Dm755 /home/$USERNAME/slopos-i/scripts/start-slopos-i /usr/local/bin/start-slopos-i || true
install -Dm644 /home/$USERNAME/slopos-i/packaging/slopos-i.desktop /usr/share/wayland-sessions/slopos-i.desktop || true
CHROOT

echo "=== done; rebooting into the installed system ==="
umount -R /mnt
systemctl reboot

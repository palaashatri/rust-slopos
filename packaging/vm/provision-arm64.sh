#!/usr/bin/env bash
# Provision an ALREADY-INSTALLED Arch Linux ARM VM (e.g. the UTM gallery image)
# with SLOPOS-I's build + runtime deps, the host SSH key, tty1 autologin, and a
# built workspace. Run INSIDE the VM as a sudo-capable user:
#   curl -sL http://10.0.2.2:8000/provision-arm64.sh | bash
#
# This does NOT partition or format any disk. It is safe to run on a live system,
# and idempotent (safe to re-run). It is the Path-A (prebuilt image) counterpart
# to arch-install-arm64.sh, and matches Stage 4's layer-onto-existing model.
set -euxo pipefail

USERNAME="${SUDO_USER:-$(whoami)}"
REPO_URL="${REPO_URL:-https://github.com/palaashatri/slopos-i.git}"
REPO_BRANCH="${REPO_BRANCH:-main}"
HOST_HTTP="${HOST_HTTP:-http://10.0.2.2:8000}"

echo "=== refresh keyring + sync ==="
sudo pacman -Sy --noconfirm archlinux-keyring || true

echo "=== install build + runtime deps (no base/kernel; system already installed) ==="
sudo pacman -S --needed --noconfirm \
  base-devel pkgconf git curl wget rust \
  wayland wayland-protocols libxkbcommon libinput seatd libdrm mesa \
  vulkan-icd-loader vulkan-swrast vulkan-tools \
  libdisplay-info pixman \
  dbus at-spi2-core \
  fontconfig freetype2 ttf-dejavu ttf-liberation \
  pipewire pipewire-pulse wireplumber libpipewire \
  polkit xorg-xwayland labwc foot \
  imagemagick grim wl-clipboard \
  networkmanager nm-connection-editor upower \
  openssh htop qemu-guest-agent

echo "=== ensure virtio_gpu in initramfs (KMS at boot) ==="
if ! grep -q 'virtio_gpu' /etc/mkinitcpio.conf; then
  sudo sed -i 's/^MODULES=(\(.*\))/MODULES=(\1 virtio_gpu)/' /etc/mkinitcpio.conf
  sudo mkinitcpio -P
fi

echo "=== groups for seat/DRM/input ==="
sudo usermod -aG video,input "$USERNAME"
sudo usermod -aG seat "$USERNAME" || true   # 'seat' group may not exist until seatd

echo "=== services ==="
sudo systemctl enable --now seatd || true
sudo systemctl enable --now sshd
sudo systemctl enable --now qemu-guest-agent || true

echo "=== install host SSH public key ==="
install -d -m 700 "$HOME/.ssh"
curl -sL "$HOST_HTTP/qa_key.pub" -o "$HOME/.ssh/authorized_keys"
chmod 600 "$HOME/.ssh/authorized_keys"

echo "=== autologin on tty1 ==="
sudo mkdir -p /etc/systemd/system/getty@tty1.service.d
sudo tee /etc/systemd/system/getty@tty1.service.d/autologin.conf >/dev/null <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin $USERNAME --noclear %I \$TERM
EOF

echo "=== clone + build SLOPOS-I ==="
if [ ! -d "$HOME/slopos-i" ]; then
  git clone --branch "$REPO_BRANCH" "$REPO_URL" "$HOME/slopos-i" \
    || git clone "$REPO_URL" "$HOME/slopos-i"
fi
mkdir -p "$HOME/.config/slopos-i"
cat > "$HOME/.config/slopos-i/settings.conf" <<EOF
theme=classic
appearance=light
lock_password=slopos-i
EOF
cd "$HOME/slopos-i"
cargo build --release --workspace

echo "=== done; reboot so virtio_gpu + group membership take effect ==="

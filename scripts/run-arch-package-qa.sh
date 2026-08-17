#!/usr/bin/env bash
# SLOPOS-I Arch Linux Package QA (PKGBUILD build, payload contract, extraction)
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

SOURCE_SHA="$(git rev-parse HEAD)"
mkdir -p artifacts/arch-package

echo "=== [1/3] Building Arch package in archlinux container ==="
docker run --rm \
  --volume "$REPO_ROOT:/workspace" \
  --workdir /workspace \
  --env SOURCE_SHA="$SOURCE_SHA" \
  archlinux:base-devel \
  bash -lc '
    set -euo pipefail
    echo "DisableSandbox" >> /etc/pacman.conf
    pacman -Syu --noconfirm --needed \
      git rust pkgconf gtk3 gdk-pixbuf2 libx11 libxrandr openssl dbus libarchive
    git config --global --add safe.directory /workspace
    test "$(git -C /workspace rev-parse HEAD)" = "$SOURCE_SHA"

    rm -rf /tmp/slopos-package-src.git /tmp/slopos-pkg
    git clone --bare /workspace /tmp/slopos-package-src.git
    git config --global protocol.file.allow always

    mkdir -p /tmp/slopos-pkg
    cp /workspace/packaging/arch/PKGBUILD /tmp/slopos-pkg/PKGBUILD
    sed -i \
      "s#rust-slopos::git+https://github.com/palaashatri/rust-slopos.git#rust-slopos::git+file:///tmp/slopos-package-src.git#" \
      /tmp/slopos-pkg/PKGBUILD

    useradd -m builder
    chown -R builder:builder /tmp/slopos-pkg /tmp/slopos-package-src.git
    runuser -u builder -- bash -lc "
      set -euo pipefail
      git config --global protocol.file.allow always
      cd /tmp/slopos-pkg
      makepkg --nodeps --cleanbuild --noconfirm
    "

    mapfile -t packages < <(find /tmp/slopos-pkg -maxdepth 1 -type f -name "slopos-i-git-*.pkg.tar.zst" ! -name "*-debug-*.pkg.tar.zst" -print)
    test "${#packages[@]}" -eq 1
    cp "${packages[0]}" /workspace/artifacts/arch-package/slopos-i.pkg.tar.zst
    bsdtar -tf "${packages[0]}" > /workspace/artifacts/arch-package/payload-paths.txt
    sha256sum "${packages[0]}" > /workspace/artifacts/arch-package/SHA256SUMS
    printf "%s\n" "$SOURCE_SHA" > /workspace/artifacts/arch-package/source-commit
  '

echo "=== [2/3] Requiring canonical X11 product payload ==="
paths="artifacts/arch-package/payload-paths.txt"
required=(
  'usr/bin/slopos-session'
  'usr/bin/slopos-shell'
  'usr/bin/slopos-catalogue'
  'usr/bin/slopos-settings'
  'usr/bin/start-slopos-i'
  'usr/bin/start-slopos-browser'
  'usr/bin/slopos-appearance'
  'usr/bin/slopos-recovery'
  'usr/share/xsessions/slopos-i.desktop'
  'usr/share/applications/slopos-browser.desktop'
  'usr/share/slopos-i/openbox/rc.xml'
  'usr/share/slopos-i/openbox/rc-graphite.xml'
  'usr/share/slopos-i/mimeapps.list'
  'usr/share/slopos-i/recovery/appearance'
  'usr/share/slopos-i/recovery/openbox/rc.xml'
  'usr/share/slopos-i/recovery/openbox/menu.xml'
  'usr/share/themes/slopos-openbox/openbox-3/themerc'
  'usr/share/themes/slopos-openbox-graphite/openbox-3/themerc'
  'usr/share/themes/slopos-gtk/gtk-3.0/gtk.css'
  'usr/share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css'
  'usr/share/icons/SLOPOS-Platinum/index.theme'
)

for path in "${required[@]}"; do
  grep -Fqx "$path" "$paths" || {
    echo "Missing Arch package payload: $path" >&2
    exit 1
  }
done

if grep -Eiq '(^|/)(wayland|smithay|wlroots|xwayland|slopos-compositor)(/|$)' "$paths"; then
  echo "Forbidden non-X11 components found in package payload!" >&2
  exit 1
fi

echo "=== [3/3] Extracting and smoke-checking packaged executable modes ==="
root="$(mktemp -d /tmp/slopos-arch-root.XXXXXX)"
cleanup() { rm -rf "$root"; }
trap cleanup EXIT

bsdtar -xf artifacts/arch-package/slopos-i.pkg.tar.zst -C "$root"
for binary in slopos-session slopos-shell slopos-catalogue slopos-settings start-slopos-i start-slopos-browser slopos-appearance slopos-recovery; do
  test -x "$root/usr/bin/$binary"
done
grep -Fqx platinum "$root/usr/share/slopos-i/recovery/appearance"
grep -Fq 'Exec=' "$root/usr/share/xsessions/slopos-i.desktop"

echo "ARCH_PACKAGE_QA_STATUS_0"
echo "SLOPOS-I Arch Linux Package QA: PASS"

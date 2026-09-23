#!/usr/bin/env bash
# SLOPOS-I Debian Package QA (.deb package build, payload contract, clean extraction)
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

SOURCE_SHA="$(git rev-parse HEAD)"
echo "=== [1/4] Staging canonical Debian metadata ==="
rm -rf debian /tmp/slopos-debian-build /tmp/slopos-deb-root
mkdir -p /tmp/slopos-debian-build artifacts/debian-package
cp -a packaging/debian debian
test -s debian/changelog
test -s debian/control
test -x debian/rules
test ! -e debian/compat
grep -Fq 'debhelper-compat (= 13)' debian/control

echo "=== [2/4] Building Debian binary package ==="
export CARGO_TARGET_DIR="/tmp/slopos-debian-build/target"
dpkg-buildpackage --build=binary --no-sign -d

mapfile -t packages < <(find .. -maxdepth 1 -type f -name 'slopos-i_*.deb' -print)
if [[ "${#packages[@]}" -ne 1 ]]; then
  echo "Expected exactly 1 debian package, found ${#packages[@]}" >&2
  exit 1
fi

cp "${packages[0]}" artifacts/debian-package/slopos-i.deb
dpkg-deb --info artifacts/debian-package/slopos-i.deb > artifacts/debian-package/control-info.txt
dpkg-deb --contents artifacts/debian-package/slopos-i.deb > artifacts/debian-package/payload.txt
dpkg-deb --fsys-tarfile artifacts/debian-package/slopos-i.deb | tar -tf - > artifacts/debian-package/payload-paths.txt
sha256sum artifacts/debian-package/slopos-i.deb > artifacts/debian-package/SHA256SUMS
printf '%s\n' "$SOURCE_SHA" > artifacts/debian-package/source-commit

echo "=== [3/4] Requiring canonical X11 product payload ==="
paths="artifacts/debian-package/payload-paths.txt"
required=(
  './usr/bin/slopos-session'
  './usr/bin/slopos-shell'
  './usr/bin/slopos-catalogue'
  './usr/bin/slopos-settings'
  './usr/bin/start-slopos-i'
  './usr/bin/start-slopos-browser'
  './usr/bin/slopos-appearance'
  './usr/bin/slopos-recovery'
  './usr/share/xsessions/slopos-i.desktop'
  './usr/share/applications/slopos-browser.desktop'
  './usr/share/slopos-i/openbox/rc.xml'
  './usr/share/slopos-i/openbox/rc-graphite.xml'
  './usr/share/slopos-i/mimeapps.list'
  './usr/share/slopos-i/recovery/appearance'
  './usr/share/slopos-i/recovery/openbox/rc.xml'
  './usr/share/slopos-i/recovery/openbox/menu.xml'
  './usr/share/themes/slopos-openbox/openbox-3/themerc'
  './usr/share/themes/slopos-openbox-graphite/openbox-3/themerc'
  './usr/share/themes/slopos-gtk/gtk-3.0/gtk.css'
  './usr/share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css'
  './usr/share/icons/SLOPOS-Platinum/index.theme'
)

for path in "${required[@]}"; do
  grep -Fqx "$path" "$paths" || {
    echo "Missing Debian package payload: $path" >&2
    exit 1
  }
done

if grep -Eiq '(^|/)(wayland|smithay|wlroots|xwayland|slopos-compositor)(/|$)' "$paths"; then
  echo "Forbidden non-X11 components found in package payload!" >&2
  exit 1
fi

echo "=== [4/4] Extracting and smoke-checking packaged executable modes ==="
root="/tmp/slopos-deb-root"
mkdir -p "$root"
dpkg-deb --extract artifacts/debian-package/slopos-i.deb "$root"

for binary in slopos-session slopos-shell slopos-catalogue slopos-settings start-slopos-i start-slopos-browser slopos-appearance slopos-recovery; do
  test -x "$root/usr/bin/$binary"
done

grep -Fqx platinum "$root/usr/share/slopos-i/recovery/appearance"
grep -Fq 'Exec=' "$root/usr/share/xsessions/slopos-i.desktop"
grep -Fq 'Type=Application' "$root/usr/share/xsessions/slopos-i.desktop"

rm -rf debian

echo "DEBIAN_PACKAGE_QA_STATUS_0"
echo "SLOPOS-I Debian Package QA: PASS"

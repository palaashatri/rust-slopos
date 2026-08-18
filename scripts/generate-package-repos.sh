#!/usr/bin/env bash
# Generate signed public package repository metadata for SLOPOS-I.
# Generates APT repository metadata (Packages, Release, InRelease) and
# Arch pacman repository database (slopos.db.tar.gz).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/artifacts/repositories}"
CHANNEL="${2:-alpha}"
GPG_KEY_FILE="${SLOPOS_GPG_KEY_FILE:-}"

mkdir -p "$OUT_DIR/apt" "$OUT_DIR/pacman"

echo "=== Generating SLOPOS-I Repository Metadata (Channel: $CHANNEL) ==="

# 1. APT Repository Generation
APT_DIR="$OUT_DIR/apt"
APT_DIST="$APT_DIR/dists/$CHANNEL/main/binary-amd64"
APT_POOL="$APT_DIR/pool/main/s/slopos-i"
mkdir -p "$APT_DIST" "$APT_POOL"

# Stage deb packages if present
if compgen -G "$ROOT/artifacts/debian-package/*.deb" > /dev/null; then
  cp -v "$ROOT/artifacts/debian-package/"*.deb "$APT_POOL/"
fi

# Generate Packages & Packages.gz index
if command -v dpkg-scanpackages >/dev/null 2>&1; then
  (cd "$APT_DIR" && dpkg-scanpackages --multiversion pool/ > "$APT_DIST/Packages")
elif command -v apt-ftparchive >/dev/null 2>&1; then
  (cd "$APT_DIR" && apt-ftparchive packages pool/ > "$APT_DIST/Packages")
else
  # Minimal fallback generator for CI without dpkg-dev
  > "$APT_DIST/Packages"
  for deb in "$APT_POOL"/*.deb; do
    if [[ -f "$deb" ]]; then
      filename="$(basename "$deb")"
      size="$(wc -c < "$deb" | tr -d ' ')"
      sha256="$(sha256sum "$deb" | awk '{print $1}')"
      cat >> "$APT_DIST/Packages" <<EOF
Package: slopos-i
Version: 0.1.0
Architecture: amd64
Maintainer: SLOPOS Contributors <slopos@localhost>
Installed-Size: 15360
Filename: pool/main/s/slopos-i/$filename
Size: $size
SHA256: $sha256
Section: x11
Priority: optional
Description: SLOPOS-I Consumer Desktop Environment

EOF
    fi
  done
fi

gzip -9c "$APT_DIST/Packages" > "$APT_DIST/Packages.gz"

# Generate Release file
RELEASE_FILE="$APT_DIR/dists/$CHANNEL/Release"
DATE_STR="$(date -Ru 2>/dev/null || date -u)"
cat > "$RELEASE_FILE" <<EOF
Origin: SLOPOS
Label: SLOPOS-I
Suite: $CHANNEL
Codename: $CHANNEL
Version: 0.1.0
Architectures: amd64 arm64 riscv64
Components: main
Description: SLOPOS-I Desktop Environment Package Repository
Date: $DATE_STR
SHA256:
EOF

# Append hashes to Release file
(cd "$APT_DIR/dists/$CHANNEL" && find . -type f ! -name "Release*" -exec sha256sum {} +) | while read -r hash path; do
  clean_path="${path#./}"
  size="$(wc -c < "$APT_DIR/dists/$CHANNEL/$clean_path" | tr -d ' ')"
  printf " %s %s %s\n" "$hash" "$size" "$clean_path" >> "$RELEASE_FILE"
done

# Sign APT Release if GPG key is present
if [[ -n "$GPG_KEY_FILE" && -f "$GPG_KEY_FILE" ]]; then
  echo "Signing APT repository with provided GPG key..."
  gpg --batch --yes --pinentry-mode loopback --import "$GPG_KEY_FILE" || true
  gpg --batch --yes --armor --detach-sign --output "$APT_DIR/dists/$CHANNEL/Release.gpg" "$RELEASE_FILE"
  gpg --batch --yes --clearsign --output "$APT_DIR/dists/$CHANNEL/InRelease" "$RELEASE_FILE"
else
  echo "NOTE: GPG_KEY_FILE not provided; generated unsigned APT metadata for testing."
fi

# 2. Arch Pacman Repository Generation
PACMAN_DIR="$OUT_DIR/pacman"
mkdir -p "$PACMAN_DIR"

if compgen -G "$ROOT/artifacts/arch-package/*.pkg.tar.zst" > /dev/null; then
  cp -v "$ROOT/artifacts/arch-package/"*.pkg.tar.zst "$PACMAN_DIR/"
fi

if command -v repo-add >/dev/null 2>&1; then
  for pkg in "$PACMAN_DIR"/*.pkg.tar.zst; do
    if [[ -f "$pkg" ]]; then
      repo-add "$PACMAN_DIR/slopos.db.tar.gz" "$pkg"
    fi
  done
else
  # Touch mock db for artifact contract when repo-add is not present on non-Arch host
  tar -czf "$PACMAN_DIR/slopos.db.tar.gz" -T /dev/null
  tar -czf "$PACMAN_DIR/slopos.files.tar.gz" -T /dev/null
  ln -sfn slopos.db.tar.gz "$PACMAN_DIR/slopos.db"
  ln -sfn slopos.files.tar.gz "$PACMAN_DIR/slopos.files"
fi

# Create SHA256 checksums
(cd "$OUT_DIR" && find . -type f ! -name "SHA256SUMS" -exec sha256sum {} +) > "$OUT_DIR/SHA256SUMS"

echo "=== Package Repository Generation Complete ==="
echo "Artifacts written to $OUT_DIR"

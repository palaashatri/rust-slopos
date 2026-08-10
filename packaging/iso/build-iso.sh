#!/usr/bin/env bash
# Build SLOPOS-I bootable ISO using archiso
# Usage: sudo bash packaging/iso/build-iso.sh [output-dir]

set -euo pipefail

OUTPUT_DIR="${1:-.}"

echo "=== Building SLOPOS-I ISO ==="
echo "Output directory: $OUTPUT_DIR"
echo ""

# Check if archiso is installed
if ! command -v mkarchiso &>/dev/null; then
  echo "ERROR: archiso not found. Install with: pacman -S archiso"
  exit 1
fi

# Get the absolute path to the profile
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROFILE_DIR="$SCRIPT_DIR"

# Build the ISO
echo "Building ISO with archiso..."
sudo mkarchiso -v -o "$OUTPUT_DIR" "$PROFILE_DIR"

# Find the output ISO
ISO_FILE=$(ls -t "$OUTPUT_DIR"/slopos-i-*.iso 2>/dev/null | head -1)

if [[ -f "$ISO_FILE" ]]; then
  echo ""
  echo "✓ ISO built successfully: $ISO_FILE"
  ls -lh "$ISO_FILE"
else
  echo "ERROR: ISO build failed"
  exit 1
fi

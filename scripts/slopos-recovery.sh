#!/usr/bin/env bash
# SLOPOS-I Emergency Desktop Config Recovery Script
set -euo pipefail

echo "=========================================================="
echo " SLOPOS-I Session Recovery & Configuration Reset"
echo "=========================================================="

HOME_DIR="${HOME:-/root}"
CONFIG_DIR="$HOME_DIR/.config/slopos-i"
OPENBOX_DIR="$HOME_DIR/.config/openbox"

echo "[1/3] Backing up existing configuration to $HOME_DIR/slopos-config-backup-$(date +%s)..."
if [ -d "$CONFIG_DIR" ]; then
  cp -rf "$CONFIG_DIR" "$HOME_DIR/slopos-config-backup-$(date +%s)"
fi

echo "[2/3] Resetting default Openbox & SLOPOS desktop configurations..."
mkdir -p "$CONFIG_DIR" "$OPENBOX_DIR"
if [ -d "/etc/slopos-i" ]; then
  cp -rf /etc/slopos-i/* "$CONFIG_DIR/"
fi

echo "[3/3] Restarting Openbox and SLOPOS Desktop Shell..."
pkill -x openbox || true
pkill -x slopos-shell || true
pkill -x slopos-session || true

echo "=========================================================="
echo " ✅ Recovery complete. You can now launch: slopos-session"
echo "=========================================================="

#!/usr/bin/env bash
# Automatic script run at first boot on the live ISO

set -euo pipefail

echo "SLOPOS-I Live ISO: Initializing..."

# Configure greetd
cat > /etc/greetd/config.toml <<'EOF'
[general]
session_wrapper = "bash"
sessions_dir = "/usr/share/wayland-sessions"

[default_session]
command = "tuigreet --time --cmd start-slopos-i"
EOF

# Enable and start greetd
systemctl enable greetd

echo "SLOPOS-I Live ISO: Setup complete. Press Ctrl+D to exit or type 'exit' for the greeter."

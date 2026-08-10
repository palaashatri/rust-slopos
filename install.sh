#!/usr/bin/env bash
# SLOPOS-I layered installer — primary distribution path
# Installs SLOPOS-I (DE + apps) onto a running Arch or Ubuntu system
# Usage: sudo ./install.sh [--prefix /usr/local] [--no-deps] [--no-build] [--with-greeter] [--distro auto|arch|ubuntu]

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

PREFIX="${PREFIX:-/usr/local}"
NO_DEPS=0
NO_BUILD=0
WITH_GREETER=0
DISTRO="auto"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      PREFIX="$2"
      shift 2
      ;;
    --no-deps)
      NO_DEPS=1
      shift
      ;;
    --no-build)
      NO_BUILD=1
      shift
      ;;
    --with-greeter)
      WITH_GREETER=1
      shift
      ;;
    --distro)
      DISTRO="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

# ── Distro detection ──────────────────────────────────────────────────────────

if [[ "$DISTRO" == "auto" ]]; then
  if [[ -f /etc/os-release ]]; then
    # shellcheck source=/dev/null
    source /etc/os-release
    if [[ "${ID:-}" == "arch" ]]; then
      DISTRO="arch"
    elif [[ "${ID:-}" == "ubuntu" || "${ID_LIKE:-}" =~ debian ]]; then
      DISTRO="ubuntu"
    else
      echo "ERROR: Unsupported distribution. Use --distro arch|ubuntu to override." >&2
      exit 1
    fi
  else
    echo "ERROR: Could not detect distribution. Use --distro arch|ubuntu." >&2
    exit 1
  fi
fi

echo "=== SLOPOS-I Installer ==="
echo "Distro: $DISTRO"
echo "Prefix: $PREFIX"
echo "Install deps: $([[ $NO_DEPS -eq 0 ]] && echo yes || echo no)"
echo "Build from source: $([[ $NO_BUILD -eq 0 ]] && echo yes || echo no)"
echo "With greeter: $([[ $WITH_GREETER -eq 0 ]] && echo no || echo yes)"
echo ""

# ── Install dependencies ──────────────────────────────────────────────────────

if [[ $NO_DEPS -eq 0 ]]; then
  echo "Installing dependencies..."

  if [[ "$DISTRO" == "arch" ]]; then
    # Install runtime deps
    RUNTIME_DEPS=$(grep -v '^#' packaging/deps/arch.txt | tr '\n' ' ')
    # shellcheck disable=SC2086
    sudo pacman -S --needed --noconfirm $RUNTIME_DEPS

    # Install build deps if building
    if [[ $NO_BUILD -eq 0 ]]; then
      BUILD_DEPS=$(grep -v '^#' packaging/deps/arch-build.txt | tr '\n' ' ')
      # shellcheck disable=SC2086
      sudo pacman -S --needed --noconfirm $BUILD_DEPS
    fi

  elif [[ "$DISTRO" == "ubuntu" ]]; then
    # Install runtime deps
    sudo apt-get update
    RUNTIME_DEPS=$(grep -v '^#' packaging/deps/ubuntu.txt | tr '\n' ' ')
    # shellcheck disable=SC2086
    sudo apt-get install -y $RUNTIME_DEPS

    # Install build deps if building
    if [[ $NO_BUILD -eq 0 ]]; then
      BUILD_DEPS=$(grep -v '^#' packaging/deps/ubuntu-build.txt | tr '\n' ' ')
      # shellcheck disable=SC2086
      sudo apt-get install -y $BUILD_DEPS
    fi
  fi

  echo "✓ Dependencies installed"
else
  echo "Skipping dependency installation"
fi

# ── Build from source ─────────────────────────────────────────────────────────

if [[ $NO_BUILD -eq 0 ]]; then
  echo ""
  echo "Building SLOPOS-I from source..."

  # Ensure Rust is available
  if ! command -v cargo &>/dev/null; then
    if [[ "$DISTRO" == "ubuntu" ]]; then
      echo "  Installing Rust via rustup..."
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
      # shellcheck source=/dev/null
      source "$HOME/.cargo/env"
    else
      echo "ERROR: cargo not found. Install Rust first." >&2
      exit 1
    fi
  fi

  cargo build --release --workspace
  echo "✓ Build complete"
else
  echo "Skipping build (assuming binaries already exist)"
fi

# ── Install binaries ──────────────────────────────────────────────────────────

echo ""
echo "Installing binaries to $PREFIX/bin..."

BINARIES=(
  "target/release/slopos-session"
  "target/release/slopos-compositor"
  "target/release/slopos-shell"
  "target/release/finder"
  "target/release/settings"
  "target/release/textedit"
  "target/release/terminal"
  "target/release/appstore"
)

for bin in "${BINARIES[@]}"; do
  if [[ ! -f "$bin" ]]; then
    echo "ERROR: $bin not found. Build may have failed." >&2
    exit 1
  fi
  BINNAME=$(basename "$bin")
  sudo install -Dm755 "$bin" "$PREFIX/bin/$BINNAME"
done

# Install start-slopos-i script
sudo install -Dm755 scripts/start-slopos-i "$PREFIX/bin/start-slopos-i"

echo "✓ Binaries installed"

# ── Install session files ─────────────────────────────────────────────────────

echo ""
echo "Installing session files..."
bash scripts/install-session-files.sh --prefix "$PREFIX"
echo "✓ Session files installed"

# ── Configure greeter (optional) ──────────────────────────────────────────────

if [[ $WITH_GREETER -eq 1 ]]; then
  echo ""
  echo "Configuring greetd + tuigreet..."

  # Install greeter packages
  if [[ "$DISTRO" == "arch" ]]; then
    sudo pacman -S --needed --noconfirm greetd tuigreet
  elif [[ "$DISTRO" == "ubuntu" ]]; then
    sudo apt-get install -y greetd tuigreet
  fi

  # Write greetd config
  sudo tee /etc/greetd/config.toml > /dev/null <<EOF
# greetd configuration
[general]
session_wrapper = "bash"
sessions_dir = "/usr/share/wayland-sessions"

[default_session]
command = "tuigreet --time --cmd start-slopos-i"
EOF

  # Enable greetd
  sudo systemctl enable greetd

  echo ""
  echo "✓ Greeter configured (greetd + tuigreet)"
  echo "  NOTE: Reboot or restart your display manager to see the SLOPOS-I session."
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "=== Installation Complete ==="
echo ""
echo "SLOPOS-I has been installed to: $PREFIX"
echo ""
echo "Binaries:"
echo "  $PREFIX/bin/slopos-session"
echo "  $PREFIX/bin/slopos-compositor"
echo "  $PREFIX/bin/slopos-shell"
echo "  $PREFIX/bin/finder, settings, textedit, terminal, appstore"
echo "  $PREFIX/bin/start-slopos-i"
echo ""
echo "Session files:"
echo "  ~/.config/wayland-sessions/slopos-i-wayland.desktop"
echo "  ~/.local/share/systemd/user/slopos-i.service"
echo ""

if [[ $WITH_GREETER -eq 0 ]]; then
  echo "To select SLOPOS-I:"
  echo "  1. Log out and log back in"
  echo "  2. At the login screen, select SLOPOS-I from the session menu"
  echo "  3. Or run: start-slopos-i"
else
  echo "Greeter is configured. Reboot or restart your DM to see the session selection."
fi

echo ""
echo "For a graphical session, ensure a display server is running (Wayland or X11)."

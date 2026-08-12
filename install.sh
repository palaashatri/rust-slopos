#!/usr/bin/env bash
# SLOPOS-I X11 layered installer
# Installs the SLOPOS X11 desktop onto a supported Arch/Ubuntu-family system.
set -euo pipefail

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"

PREFIX="${PREFIX:-/usr/local}"
XSESSION_DIR="${XSESSION_DIR:-/usr/share/xsessions}"
NO_DEPS=0
NO_BUILD=0
WITH_GREETER=0
DISTRO="auto"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --no-deps) NO_DEPS=1; shift ;;
    --no-build) NO_BUILD=1; shift ;;
    --with-greeter) WITH_GREETER=1; shift ;;
    --distro) DISTRO="$2"; shift 2 ;;
    -h|--help)
      cat <<EOF
Usage: sudo ./install.sh [--prefix /usr/local] [--no-deps] [--no-build]
                         [--with-greeter] [--distro auto|arch|ubuntu]

Installs the X11-only SLOPOS-I desktop.
The XSESSION_DIR environment variable overrides the display-manager session
directory (default: /usr/share/xsessions).
EOF
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

if [[ "$DISTRO" == "auto" ]]; then
  if [[ ! -f /etc/os-release ]]; then
    echo "ERROR: /etc/os-release is unavailable; pass --distro arch|ubuntu" >&2
    exit 1
  fi
  # shellcheck source=/dev/null
  source /etc/os-release
  if [[ "${ID:-}" == "arch" || "${ID_LIKE:-}" =~ arch ]]; then
    DISTRO="arch"
  elif [[ "${ID:-}" == "ubuntu" || "${ID:-}" == "debian" || "${ID_LIKE:-}" =~ debian ]]; then
    DISTRO="ubuntu"
  else
    echo "ERROR: Unsupported distribution '${ID:-unknown}'. Pass --distro arch|ubuntu only when compatible." >&2
    exit 1
  fi
fi

if [[ "$DISTRO" != "arch" && "$DISTRO" != "ubuntu" ]]; then
  echo "ERROR: --distro must be arch or ubuntu" >&2
  exit 2
fi

echo "=== SLOPOS-I X11 Installer ==="
echo "Distribution family: $DISTRO"
echo "Prefix: $PREFIX"

if [[ $NO_DEPS -eq 0 ]]; then
  echo "Installing X11 runtime/build dependencies..."
  if [[ "$DISTRO" == "arch" ]]; then
    mapfile -t runtime_deps < <(grep -Ev '^\s*(#|$)' packaging/deps/arch.txt)
    pacman -S --needed --noconfirm "${runtime_deps[@]}"
    if [[ $NO_BUILD -eq 0 ]]; then
      mapfile -t build_deps < <(grep -Ev '^\s*(#|$)' packaging/deps/arch-build.txt)
      pacman -S --needed --noconfirm "${build_deps[@]}"
    fi
  else
    apt-get update
    mapfile -t runtime_deps < <(grep -Ev '^\s*(#|$)' packaging/deps/ubuntu.txt)
    apt-get install -y "${runtime_deps[@]}"
    if [[ $NO_BUILD -eq 0 ]]; then
      mapfile -t build_deps < <(grep -Ev '^\s*(#|$)' packaging/deps/ubuntu-build.txt)
      apt-get install -y "${build_deps[@]}"
    fi
  fi
fi

if [[ $NO_BUILD -eq 0 ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: cargo is required. Install Rust before running this installer." >&2
    exit 1
  fi
  cargo build --release --workspace --locked
fi

BINARIES=(slopos-session slopos-shell slopos-catalogue slopos-settings)
for name in "${BINARIES[@]}"; do
  src="$CARGO_TARGET_DIR/release/$name"
  if [[ ! -x "$src" ]]; then
    echo "ERROR: missing built binary: $src" >&2
    exit 1
  fi
  install -Dm755 "$src" "$PREFIX/bin/$name"
done
install -Dm755 scripts/start-slopos-i "$PREFIX/bin/start-slopos-i"

# X11 session descriptor. Display managers conventionally scan /usr/share;
# override XSESSION_DIR for a deliberately self-contained custom prefix.
bash scripts/install-session-files.sh --prefix "$PREFIX" --session-dir "$XSESSION_DIR"

# SLOPOS-specific Openbox configuration and theme. The session supervisor points
# Openbox at this config rather than overwriting a user's normal Openbox profile.
install -Dm644 assets/config/openbox/rc.xml "$PREFIX/share/slopos-i/openbox/rc.xml"
install -Dm644 assets/config/openbox/menu.xml "$PREFIX/share/slopos-i/openbox/menu.xml"
install -Dm644 themes/slopos-openbox/openbox-3/themerc \
  "$PREFIX/share/themes/slopos-openbox/openbox-3/themerc"

# GTK theme and desktop defaults.
install -Dm644 assets/config/gtk-3.0/gtk.css \
  "$PREFIX/share/themes/slopos-gtk/gtk-3.0/gtk.css"
if [[ -f assets/config/gtk-3.0/settings.ini ]]; then
  install -Dm644 assets/config/gtk-3.0/settings.ini \
    "$PREFIX/share/slopos-i/gtk-3.0/settings.ini"
fi
install -Dm644 assets/config/mimeapps.list "$PREFIX/share/slopos-i/mimeapps.list"

# Original SLOPOS theme resources.
rm -rf "$PREFIX/share/slopos-i/themes/platinum"
mkdir -p "$PREFIX/share/slopos-i/themes"
cp -a themes/platinum "$PREFIX/share/slopos-i/themes/platinum"
install -Dm644 assets/slopos-logo.png "$PREFIX/share/slopos-i/slopos-logo.png"

if [[ $WITH_GREETER -eq 1 ]]; then
  if [[ "$DISTRO" == "arch" ]]; then
    pacman -S --needed --noconfirm greetd tuigreet
  else
    apt-get install -y greetd
  fi
  mkdir -p /etc/greetd
  cat >/etc/greetd/config.toml <<'EOF'
[default_session]
command = "tuigreet --time --cmd start-slopos-i"
EOF
  systemctl enable greetd
fi

cat <<EOF

=== SLOPOS-I installation complete ===
Binaries:
  $PREFIX/bin/slopos-session
  $PREFIX/bin/slopos-shell
  $PREFIX/bin/slopos-catalogue
  $PREFIX/bin/slopos-settings
  $PREFIX/bin/start-slopos-i

X11 session:
  $XSESSION_DIR/slopos-i.desktop

This release is X11-only. Select “SLOPOS-I” from your display manager's X11 session list,
or start it from an existing X server with: start-slopos-i
EOF

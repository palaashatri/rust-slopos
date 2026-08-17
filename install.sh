#!/usr/bin/env bash
# SLOPOS-I X11 layered installer
# Installs the SLOPOS X11 desktop onto a supported Arch/Ubuntu-family system.
set -euo pipefail

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}"
export CARGO_TARGET_DIR

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

for install_path_name in PREFIX XSESSION_DIR; do
  install_path_value="${!install_path_name}"
  case "$install_path_value" in
    /*) ;;
    *)
      echo "ERROR: $install_path_name must be an absolute path: $install_path_value" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$CARGO_TARGET_DIR"

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
install -Dm755 scripts/start-slopos-browser "$PREFIX/bin/start-slopos-browser"
install -Dm755 scripts/install-browser-theme.sh "$PREFIX/bin/install-browser-theme.sh"
install -Dm755 scripts/slopos-appearance "$PREFIX/bin/slopos-appearance"
install -Dm755 scripts/slopos-wallpaper "$PREFIX/bin/slopos-wallpaper"
install -Dm755 scripts/slopos-recovery.sh "$PREFIX/bin/slopos-recovery"
install -Dm644 packaging/slopos-browser.desktop "$PREFIX/share/applications/slopos-browser.desktop"

bash scripts/install-session-files.sh --prefix "$PREFIX" --session-dir "$XSESSION_DIR"

install -Dm644 assets/config/openbox/rc.xml "$PREFIX/share/slopos-i/openbox/rc.xml"
install -Dm644 assets/config/openbox/rc-classic.xml "$PREFIX/share/slopos-i/openbox/rc-classic.xml"
install -Dm644 assets/config/openbox/rc-graphite.xml "$PREFIX/share/slopos-i/openbox/rc-graphite.xml"
install -Dm644 assets/config/openbox/rc-oled.xml "$PREFIX/share/slopos-i/openbox/rc-oled.xml"
install -Dm644 assets/config/openbox/menu.xml "$PREFIX/share/slopos-i/openbox/menu.xml"
install -Dm644 themes/slopos-openbox/openbox-3/themerc \
  "$PREFIX/share/themes/slopos-openbox/openbox-3/themerc"
install -Dm644 themes/slopos-openbox-classic/openbox-3/themerc \
  "$PREFIX/share/themes/slopos-openbox-classic/openbox-3/themerc"
install -Dm644 themes/slopos-openbox-graphite/openbox-3/themerc \
  "$PREFIX/share/themes/slopos-openbox-graphite/openbox-3/themerc"
install -Dm644 themes/slopos-openbox-oled/openbox-3/themerc \
  "$PREFIX/share/themes/slopos-openbox-oled/openbox-3/themerc"

install -Dm644 assets/config/gtk-3.0/gtk.css \
  "$PREFIX/share/themes/slopos-gtk/gtk-3.0/gtk.css"
install -Dm644 assets/config/gtk-3.0/gtk-classic.css \
  "$PREFIX/share/themes/slopos-gtk-classic/gtk-3.0/gtk.css"
install -Dm644 assets/config/gtk-3.0/gtk-graphite.css \
  "$PREFIX/share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
install -Dm644 assets/config/gtk-3.0/gtk-oled.css \
  "$PREFIX/share/themes/slopos-gtk-oled/gtk-3.0/gtk.css"
if [[ -f assets/config/gtk-3.0/settings.ini ]]; then
  install -Dm644 assets/config/gtk-3.0/settings.ini \
    "$PREFIX/share/slopos-i/gtk-3.0/settings.ini"
fi
install -Dm644 assets/config/mimeapps.list "$PREFIX/share/slopos-i/mimeapps.list"
install -Dm644 assets/file-manager/actions/set-wallpaper.desktop \
  "$PREFIX/share/file-manager/actions/set-wallpaper.desktop"
install -Dm644 assets/applications/slopos-set-wallpaper.desktop \
  "$PREFIX/share/applications/slopos-set-wallpaper.desktop"

# Install retro wallpapers
mkdir -p "$PREFIX/share/slopos-i/wallpapers"
if [[ -d assets/wallpapers ]]; then
  cp -a assets/wallpapers/* "$PREFIX/share/slopos-i/wallpapers/"
fi

# Install theme preview thumbnails
mkdir -p "$PREFIX/share/slopos-i/themes"
if [[ -d assets/themes ]]; then
  cp -a assets/themes/* "$PREFIX/share/slopos-i/themes/"
fi

# Recovery defaults are intentionally a tiny user-config reset payload rather
# than a copy of the whole system share tree. Reset always returns to Platinum.
mkdir -p "$PREFIX/share/slopos-i/recovery"
printf '%s\n' platinum >"$PREFIX/share/slopos-i/recovery/appearance"
install -Dm644 assets/config/openbox/rc.xml \
  "$PREFIX/share/slopos-i/recovery/openbox/rc.xml"
install -Dm644 assets/config/openbox/menu.xml \
  "$PREFIX/share/slopos-i/recovery/openbox/menu.xml"

rm -rf "$PREFIX/share/slopos-i/themes/platinum" "$PREFIX/share/slopos-i/themes/graphite"
mkdir -p "$PREFIX/share/slopos-i/themes"
cp -a themes/platinum "$PREFIX/share/slopos-i/themes/platinum"
cp -a themes/graphite "$PREFIX/share/slopos-i/themes/graphite"

# Install the original SLOPOS freedesktop icon theme at the standard location
# so upstream GTK applications such as PCManFM resolve SLOPOS folders, files,
# devices and actions instead of falling straight through to Adwaita.
rm -rf "$PREFIX/share/icons/SLOPOS-Platinum"
mkdir -p "$PREFIX/share/icons"
cp -a themes/platinum/icon-theme "$PREFIX/share/icons/SLOPOS-Platinum"
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$PREFIX/share/icons/SLOPOS-Platinum" >/dev/null 2>&1 || true
fi

mkdir -p "$PREFIX/share/slopos-i/browser"
cp -a packaging/browser/chromium "$PREFIX/share/slopos-i/browser/chromium"
cp -a packaging/browser/firefox "$PREFIX/share/slopos-i/browser/firefox"
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
  $PREFIX/bin/slopos-appearance
  $PREFIX/bin/slopos-recovery

X11 session:
  $XSESSION_DIR/slopos-i.desktop

Appearance:
  slopos-appearance platinum
  slopos-appearance graphite

Recovery:
  slopos-recovery

This release is X11-only. Select “SLOPOS-I” from your display manager's X11 session list,
or start it from an existing X server with: start-slopos-i
EOF

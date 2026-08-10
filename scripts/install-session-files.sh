#!/usr/bin/env bash
# install-session-files.sh — install greeter/session packaging under PREFIX.
#
# Copies packaging desktops + start-slopos-i (and optional systemd user unit)
# into an FHS layout matching SessionPackagingLayout::under_prefix:
#
#   $PREFIX/share/wayland-sessions/slopos-i.desktop
#   $PREFIX/share/xsessions/slopos-i.desktop
#   $PREFIX/bin/start-slopos-i
#   $PREFIX/lib/systemd/user/slopos-i.service
#
# Usage:
#   ./scripts/install-session-files.sh [--dry-run] [--prefix PREFIX]
#
# Defaults:
#   PREFIX=/usr/local
#
# Does NOT claim a live display manager was configured or tested. After install,
# pick SLOPOS-I on the greeter on a real seat to prove §12 criterion 1.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="/usr/local"
DRY_RUN=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [--dry-run] [--prefix PREFIX] [-h|--help]

Install SLOPOS-I session files for display-manager greeters.

  --dry-run       Print actions without writing files
  --prefix PATH   Install prefix (default: /usr/local)
  -h, --help      Show this help

Artifacts (from repo):
  packaging/slopos-i-wayland.desktop → \$PREFIX/share/wayland-sessions/slopos-i.desktop
  packaging/slopos-i.desktop         → \$PREFIX/share/xsessions/slopos-i.desktop
  scripts/start-slopos-i             → \$PREFIX/bin/start-slopos-i
  packaging/slopos-i.service         → \$PREFIX/lib/systemd/user/slopos-i.service

Note: Exec=start-slopos-i requires \$PREFIX/bin on PATH for the greeter user.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --prefix)
      PREFIX="${2:?--prefix requires a path}"
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#--prefix=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "install-session-files: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

WAYLAND_SRC="$ROOT/packaging/slopos-i-wayland.desktop"
XSESSION_SRC="$ROOT/packaging/slopos-i.desktop"
START_SRC="$ROOT/scripts/start-slopos-i"
SERVICE_SRC="$ROOT/packaging/slopos-i.service"

WAYLAND_DST="$PREFIX/share/wayland-sessions/slopos-i.desktop"
XSESSION_DST="$PREFIX/share/xsessions/slopos-i.desktop"
START_DST="$PREFIX/bin/start-slopos-i"
SERVICE_DST="$PREFIX/lib/systemd/user/slopos-i.service"

for src in "$WAYLAND_SRC" "$XSESSION_SRC" "$START_SRC" "$SERVICE_SRC"; do
  if [[ ! -f "$src" ]]; then
    echo "install-session-files: missing source: $src" >&2
    exit 1
  fi
done

run_install() {
  local mode="$1" src="$2" dst="$3"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "DRY-RUN install -Dm${mode} $src $dst"
  else
    install -Dm"${mode}" "$src" "$dst"
    echo "installed $dst"
  fi
}

echo "install-session-files: PREFIX=$PREFIX dry_run=$DRY_RUN"
echo "install-session-files: source tree=$ROOT"
echo "install-session-files: note — packaging only; live greeter login not verified by this script"

run_install 644 "$WAYLAND_SRC" "$WAYLAND_DST"
run_install 644 "$XSESSION_SRC" "$XSESSION_DST"
run_install 755 "$START_SRC" "$START_DST"
run_install 644 "$SERVICE_SRC" "$SERVICE_DST"

echo
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "install-session-files: dry-run complete (no files written)"
else
  echo "install-session-files: install complete under $PREFIX"
  echo "install-session-files: ensure $PREFIX/bin is on PATH for greeter sessions"
  echo "install-session-files: log out and select SLOPOS-I on a real DM to prove session start"
fi
exit 0

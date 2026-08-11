#!/usr/bin/env bash
# install-session-files.sh — install greeter/session packaging under PREFIX.
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="/usr/local"
DRY_RUN=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [--dry-run] [--prefix PREFIX] [-h|--help]

Install SLOPOS-I X11 session files for display-manager greeters.

  --dry-run       Print actions without writing files
  --prefix PATH   Install prefix (default: /usr/local)
  -h, --help      Show this help

Artifacts:
  packaging/slopos-i.desktop → \$PREFIX/share/xsessions/slopos-i.desktop
  scripts/start-slopos-i     → \$PREFIX/bin/start-slopos-i
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

XSESSION_SRC="$ROOT/packaging/slopos-i.desktop"
START_SRC="$ROOT/scripts/start-slopos-i"

XSESSION_DST="$PREFIX/share/xsessions/slopos-i.desktop"
START_DST="$PREFIX/bin/start-slopos-i"

run_install() {
  local mode="$1" src="$2" dst="$3"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "DRY-RUN install -Dm${mode} $src $dst"
  else
    mkdir -p "$(dirname "$dst")"
    install -Dm"${mode}" "$src" "$dst"
    echo "installed $dst"
  fi
}

echo "install-session-files: PREFIX=$PREFIX dry_run=$DRY_RUN"
run_install 644 "$XSESSION_SRC" "$XSESSION_DST"
run_install 755 "$START_SRC" "$START_DST"

echo "install-session-files: install complete under $PREFIX"

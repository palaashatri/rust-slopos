#!/usr/bin/env bash
# install-session-files.sh — install greeter/session packaging under PREFIX.
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="/usr/local"
SESSION_DIR=""
DRY_RUN=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [--dry-run] [--prefix PREFIX] [--session-dir DIR] [-h|--help]

Install SLOPOS-I X11 session files for display-manager greeters.

  --dry-run       Print actions without writing files
  --prefix PATH   Install prefix (default: /usr/local)
  --session-dir DIR
                  X11 session descriptor directory (default: PREFIX/share/xsessions)
  -h, --help      Show this help

Artifacts:
  packaging/slopos-i.desktop → \$SESSION_DIR/slopos-i.desktop
  scripts/start-slopos-i     → \$PREFIX/bin/start-slopos-i

The installed session descriptor uses \$PREFIX/bin/slopos-session for both
Exec and TryExec so custom-prefix installs remain discoverable by display
managers whose environment does not include that prefix.
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
    --session-dir)
      SESSION_DIR="${2:?--session-dir requires a path}"
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

if [[ -z "$SESSION_DIR" ]]; then
  SESSION_DIR="$PREFIX/share/xsessions"
fi
XSESSION_DST="$SESSION_DIR/slopos-i.desktop"
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

install_session_descriptor() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "DRY-RUN install -Dm644 $XSESSION_SRC $XSESSION_DST (Exec=$PREFIX/bin/slopos-session)"
    return
  fi

  mkdir -p "$(dirname "$XSESSION_DST")"
  local temporary
  local saw_exec=0
  local saw_tryexec=0
  temporary="$(mktemp "${XSESSION_DST}.XXXXXX")"
  while IFS= read -r line || [[ -n "$line" ]]; do
    case "$line" in
      Exec=*)
        saw_exec=1
        printf 'Exec=%s\n' "$PREFIX/bin/slopos-session"
        ;;
      TryExec=*)
        saw_tryexec=1
        printf 'TryExec=%s\n' "$PREFIX/bin/slopos-session"
        ;;
      *) printf '%s\n' "$line" ;;
    esac
  done < "$XSESSION_SRC" > "$temporary"
  if [[ "$saw_exec" -ne 1 || "$saw_tryexec" -ne 1 ]]; then
    rm -f -- "$temporary"
    echo "install-session-files: source descriptor must contain Exec= and TryExec=" >&2
    return 1
  fi
  chmod 644 "$temporary"
  mv -f "$temporary" "$XSESSION_DST"
  echo "installed $XSESSION_DST"
}

echo "install-session-files: PREFIX=$PREFIX dry_run=$DRY_RUN"
install_session_descriptor
run_install 755 "$START_SRC" "$START_DST"

echo "install-session-files: install complete under $PREFIX"

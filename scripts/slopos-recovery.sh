#!/usr/bin/env bash
# SLOPOS-I emergency desktop configuration recovery.
#
# Recovery is deliberately bounded: preserve the user's configuration, stage
# vendor defaults when they are installed, and restart only the session
# children that the existing supervisor owns. Killing the supervisor itself
# would leave the desktop stopped while this script claimed recovery had
# completed.
set -euo pipefail

echo "=========================================================="
echo " SLOPOS-I Session Recovery & Configuration Reset"
echo "=========================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_PREFIX="$(cd "$SCRIPT_DIR/.." && pwd)"
DEFAULT_VENDOR_DIR="$INSTALL_PREFIX/share/slopos-i/recovery"
# Retain the old /etc location only as a compatibility fallback for early
# development installs. Current packages place bounded recovery defaults under
# their own prefix so custom-prefix installs never write unrelated /etc state.
if [[ ! -d "$DEFAULT_VENDOR_DIR" && -d /etc/slopos-i ]]; then
  DEFAULT_VENDOR_DIR=/etc/slopos-i
fi

HOME_DIR="${HOME:-/root}"
CONFIG_DIR="$HOME_DIR/.config/slopos-i"
OPENBOX_DIR="$HOME_DIR/.config/openbox"
CONFIG_PARENT="$HOME_DIR/.config"
VENDOR_DIR="${SLOPOS_VENDOR_CONFIG_DIR:-$DEFAULT_VENDOR_DIR}"
BACKUP_DIR="${SLOPOS_RECOVERY_BACKUP_DIR:-$HOME_DIR/slopos-config-backup-$(date +%Y%m%d-%H%M%S)-$$}"
RECOVERY_LOG="${SLOPOS_RECOVERY_LOG:-${TMPDIR:-/tmp}/slopos-recovery-session-$(id -u).log}"

if [[ -z "$HOME_DIR" || "$HOME_DIR" == "/" ]]; then
  echo "slopos-recovery: refusing an unsafe HOME directory: '$HOME_DIR'" >&2
  exit 2
fi
case "$HOME_DIR" in
  /*) ;;
  *)
    echo "slopos-recovery: HOME must be an absolute path: '$HOME_DIR'" >&2
    exit 2
    ;;
esac
case "$BACKUP_DIR" in
  /*) ;;
  *)
    echo "slopos-recovery: backup destination must be an absolute path: '$BACKUP_DIR'" >&2
    exit 2
    ;;
esac
case "$VENDOR_DIR" in
  /*) ;;
  *)
    echo "slopos-recovery: vendor defaults must use an absolute path: '$VENDOR_DIR'" >&2
    exit 2
    ;;
esac
if [[ -L "$CONFIG_PARENT" ]]; then
  echo "slopos-recovery: refusing a symlinked config parent: $CONFIG_PARENT" >&2
  exit 2
fi

pid_for() {
  local name="$1"
  pgrep -u "$(id -u)" -x "$name" | head -n 1 || true
}

session_pid="$(pid_for slopos-session)"
start_session=""
if [[ -z "$session_pid" ]]; then
  start_session="$(command -v start-slopos-i || true)"
  if [[ -z "$start_session" ]]; then
    for candidate in "$INSTALL_PREFIX/bin/start-slopos-i" /usr/local/bin/start-slopos-i /usr/bin/start-slopos-i; do
      if [[ -x "$candidate" ]]; then
        start_session="$candidate"
        break
      fi
    done
  fi
  if [[ -z "$start_session" ]]; then
    echo "slopos-recovery: slopos-session is not running and start-slopos-i is unavailable" >&2
    exit 1
  fi
fi

if [[ -e "$BACKUP_DIR" ]]; then
  echo "slopos-recovery: backup destination already exists: $BACKUP_DIR" >&2
  exit 2
fi
mkdir -p "$CONFIG_PARENT"
mkdir "$BACKUP_DIR"

echo "[1/3] Preserving existing configuration in $BACKUP_DIR"
if [[ -d "$CONFIG_DIR" || -L "$CONFIG_DIR" || -f "$CONFIG_DIR" ]]; then
  mv -- "$CONFIG_DIR" "$BACKUP_DIR/slopos-i"
fi
if [[ -d "$OPENBOX_DIR" || -L "$OPENBOX_DIR" || -f "$OPENBOX_DIR" ]]; then
  mv -- "$OPENBOX_DIR" "$BACKUP_DIR/openbox"
fi

echo "[2/3] Staging installed vendor defaults"
mkdir -p "$CONFIG_DIR" "$OPENBOX_DIR"
if [[ -d "$VENDOR_DIR" ]]; then
  # Recovery defaults are deliberately bounded user configuration, not a copy
  # of the full system share tree.
  if [[ -d "$VENDOR_DIR/openbox" ]]; then
    cp -a "$VENDOR_DIR/openbox/." "$OPENBOX_DIR/"
  fi
  find "$VENDOR_DIR" -mindepth 1 -maxdepth 1 ! -name openbox -exec cp -a {} "$CONFIG_DIR/" \;
else
  echo "No vendor defaults found at $VENDOR_DIR; using empty SLOPOS config."
fi

wait_for_child_restart() {
  local name="$1" old_pid="$2" new_pid
  for _ in $(seq 1 120); do
    new_pid="$(pid_for "$name")"
    if [[ -n "$new_pid" && ( -z "$old_pid" || "$new_pid" != "$old_pid" ) ]]; then
      printf '%s=%s\n' "$name" "$new_pid"
      return 0
    fi
    sleep 0.25
  done
  echo "slopos-recovery: $name did not recover" >&2
  return 1
}

echo "[3/3] Restarting managed X11 children"
old_wm_pid="$(pid_for openbox)"
old_shell_pid="$(pid_for slopos-shell)"

if [[ -n "$session_pid" ]]; then
  # slopos-session owns and respawns these children. Keep it alive so its
  # backoff/health policy remains in force and no duplicate supervisor starts.
  [[ -z "$old_wm_pid" ]] || kill -TERM "$old_wm_pid"
  [[ -z "$old_shell_pid" ]] || kill -TERM "$old_shell_pid"
else
  # A manually launched child may survive after a supervisor crash. Stop it
  # before starting the replacement so recovery cannot create duplicate shell
  # or window-manager instances.
  [[ -z "$old_wm_pid" ]] || kill -TERM "$old_wm_pid"
  [[ -z "$old_shell_pid" ]] || kill -TERM "$old_shell_pid"
  : >"$RECOVERY_LOG"
  nohup "$start_session" >"$RECOVERY_LOG" 2>&1 &
  session_pid=$!
  echo "Started a new SLOPOS session supervisor (pid $session_pid); log: $RECOVERY_LOG"
fi

wait_for_child_restart openbox "$old_wm_pid"
wait_for_child_restart slopos-shell "$old_shell_pid"

echo "SLOPOS_RECOVERY_STATUS_0"
echo "Recovery complete; previous configuration is preserved at $BACKUP_DIR"

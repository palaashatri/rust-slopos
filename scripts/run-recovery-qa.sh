#!/usr/bin/env bash
# Exercise SLOPOS configuration recovery against a real Xvfb/Openbox session.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DISPLAY="${SLOPOS_RECOVERY_QA_DISPLAY:-:97}"
export DISPLAY
export GDK_BACKEND=x11
export SLOPOS_QA_NO_WELCOME=1
export SLOPOS_OPENBOX_CONFIG="$ROOT/assets/config/openbox/rc.xml"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

fail() { echo "RECOVERY_QA_ERROR: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "missing command: $1"; }
for command in Xvfb xdpyinfo pgrep dbus-run-session cargo openbox; do need "$command"; done

# Keep every desktop process on one private session bus. The recovery command
# may have to start a replacement supervisor, and that replacement must inherit
# the same bus as the shell it is validating.
if [[ "${SLOPOS_RECOVERY_QA_INNER:-0}" != 1 ]]; then
  exec dbus-run-session -- env \
    SLOPOS_RECOVERY_QA_INNER=1 \
    SLOPOS_RECOVERY_QA_DISPLAY="$DISPLAY" \
    CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
    bash "$0"
fi

cd "$ROOT"
if [[ "${SLOPOS_QA_SKIP_BUILD:-0}" != 1 ]]; then
  cargo build --release --workspace --locked
fi
for binary in slopos-session slopos-shell; do
  test -x "$CARGO_TARGET_DIR/release/$binary" || fail "missing release binary: $binary"
done

tmp="$(mktemp -d /tmp/slopos-recovery-qa.XXXXXX)"
XVFB_PID=""
SESSION_PID=""
cleanup() {
  status=$?
  set +e
  if [[ -n "$SESSION_PID" ]] && kill -0 "$SESSION_PID" 2>/dev/null; then
    kill -TERM "$SESSION_PID" 2>/dev/null || true
    wait "$SESSION_PID" 2>/dev/null || true
  fi
  pkill -TERM -u "$(id -u)" -x slopos-session 2>/dev/null || true
  pkill -TERM -u "$(id -u)" -x slopos-shell 2>/dev/null || true
  pkill -TERM -u "$(id -u)" -x openbox 2>/dev/null || true
  if [[ -n "$XVFB_PID" ]]; then
    kill -TERM "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
  fi
  if [[ "$status" -ne 0 ]]; then
    echo "===== recovery session log =====" >&2
    cat "$tmp/session.log" 2>/dev/null >&2 || true
    echo "===== recovery command logs =====" >&2
    find "$tmp" -maxdepth 1 -name 'recovery*.log' -print -exec cat {} \; 2>/dev/null >&2 || true
    echo "===== process table =====" >&2
    ps -ef | grep -E 'Xvfb|openbox|slopos' >&2 || true
  fi
  rm -rf "$tmp"
  exit "$status"
}
trap cleanup EXIT

wait_for() {
  local description="$1"; shift
  for _ in $(seq 1 120); do
    if "$@" >/dev/null 2>&1; then
      echo "ready: $description"
      return 0
    fi
    sleep 0.1
  done
  fail "timed out waiting for $description"
}

wait_for_pid_change() {
  local name="$1" old_pid="$2" current
  for _ in $(seq 1 120); do
    current="$(pgrep -u "$(id -u)" -xo "$name" 2>/dev/null || true)"
    if [[ -n "$current" && "$current" != "$old_pid" ]]; then
      printf '%s\n' "$current"
      return 0
    fi
    sleep 0.1
  done
  return 1
}

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >"$tmp/xvfb.log" 2>&1 &
XVFB_PID=$!
wait_for "X server" xdpyinfo -display "$DISPLAY"

"$CARGO_TARGET_DIR/release/slopos-session" >"$tmp/session.log" 2>&1 &
SESSION_PID=$!
wait_for "session supervisor" pgrep -u "$(id -u)" -x slopos-session
wait_for "Openbox" pgrep -u "$(id -u)" -x openbox
wait_for "SLOPOS shell" pgrep -u "$(id -u)" -x slopos-shell

test "$(pgrep -u "$(id -u)" -xc slopos-session)" -eq 1 || fail "expected one session supervisor"
test "$(pgrep -u "$(id -u)" -xc openbox)" -eq 1 || fail "expected one Openbox child"
test "$(pgrep -u "$(id -u)" -xc slopos-shell)" -eq 1 || fail "expected one shell child"

home1="$tmp/home-existing"
vendor1="$tmp/vendor-existing"
backup1="$tmp/backup-existing"
mkdir -p "$home1/.config/slopos-i" "$home1/.config/openbox" "$vendor1/openbox"
printf '%s\n' 'graphite' >"$home1/.config/slopos-i/appearance"
printf '%s\n' 'preserve-me' >"$home1/.config/slopos-i/user-marker"
printf '%s\n' '<broken-openbox/>' >"$home1/.config/openbox/rc.xml"
printf '%s\n' 'platinum' >"$vendor1/appearance"
printf '%s\n' '<vendor-openbox/>' >"$vendor1/openbox/rc.xml"

supervisor_before="$(pgrep -u "$(id -u)" -xo slopos-session)"
wm_before="$(pgrep -u "$(id -u)" -xo openbox)"
shell_before="$(pgrep -u "$(id -u)" -xo slopos-shell)"
HOME="$home1" \
SLOPOS_VENDOR_CONFIG_DIR="$vendor1" \
SLOPOS_RECOVERY_BACKUP_DIR="$backup1" \
SLOPOS_RECOVERY_LOG="$tmp/recovery-existing-session.log" \
  bash scripts/slopos-recovery.sh | tee "$tmp/recovery-existing.log"
grep -Fqx 'SLOPOS_RECOVERY_STATUS_0' "$tmp/recovery-existing.log" || fail "existing-session recovery marker missing"

test "$(pgrep -u "$(id -u)" -xo slopos-session)" = "$supervisor_before" || fail "recovery replaced the healthy supervisor"
wm_after="$(wait_for_pid_change openbox "$wm_before")" || fail "Openbox did not restart"
shell_after="$(wait_for_pid_change slopos-shell "$shell_before")" || fail "shell did not restart"
test "$wm_after" != "$wm_before" || fail "Openbox PID did not change"
test "$shell_after" != "$shell_before" || fail "shell PID did not change"
test "$(pgrep -u "$(id -u)" -xc slopos-session)" -eq 1 || fail "duplicate supervisor after recovery"
test "$(pgrep -u "$(id -u)" -xc openbox)" -eq 1 || fail "duplicate Openbox after recovery"
test "$(pgrep -u "$(id -u)" -xc slopos-shell)" -eq 1 || fail "duplicate shell after recovery"
grep -Fqx 'graphite' "$backup1/slopos-i/appearance" || fail "appearance was not preserved in backup"
grep -Fqx 'preserve-me' "$backup1/slopos-i/user-marker" || fail "SLOPOS config was not preserved in backup"
grep -Fqx '<broken-openbox/>' "$backup1/openbox/rc.xml" || fail "Openbox config was not preserved in backup"
grep -Fqx 'platinum' "$home1/.config/slopos-i/appearance" || fail "vendor appearance default was not staged"
grep -Fqx '<vendor-openbox/>' "$home1/.config/openbox/rc.xml" || fail "vendor Openbox default was not staged"

echo 'RECOVERY_EXISTING_SUPERVISOR_STATUS_0'

# Fail closed before touching state when paths are unsafe.
if HOME=/ SLOPOS_RECOVERY_BACKUP_DIR="$tmp/unsafe-root-backup" bash scripts/slopos-recovery.sh >"$tmp/recovery-unsafe-root.log" 2>&1; then
  fail "unsafe HOME=/ was accepted"
fi
home_symlink="$tmp/home-symlink"
mkdir -p "$home_symlink" "$tmp/config-target"
ln -s "$tmp/config-target" "$home_symlink/.config"
if HOME="$home_symlink" SLOPOS_RECOVERY_BACKUP_DIR="$tmp/unsafe-symlink-backup" bash scripts/slopos-recovery.sh >"$tmp/recovery-unsafe-symlink.log" 2>&1; then
  fail "symlinked .config parent was accepted"
fi
home_existing_backup="$tmp/home-existing-backup"
mkdir -p "$home_existing_backup/.config/slopos-i" "$tmp/preexisting-backup"
printf '%s\n' untouched >"$home_existing_backup/.config/slopos-i/marker"
if HOME="$home_existing_backup" SLOPOS_RECOVERY_BACKUP_DIR="$tmp/preexisting-backup" bash scripts/slopos-recovery.sh >"$tmp/recovery-existing-backup.log" 2>&1; then
  fail "pre-existing backup destination was accepted"
fi
grep -Fqx untouched "$home_existing_backup/.config/slopos-i/marker" || fail "failed recovery mutated config before rejecting backup path"
echo 'RECOVERY_PATH_SAFETY_STATUS_0'

# Prove the fallback path after the supervisor itself is absent. The recovery
# command must not leave orphaned children or create duplicate supervisors.
kill -TERM "$supervisor_before"
wait "$supervisor_before" 2>/dev/null || true
SESSION_PID=""
for _ in $(seq 1 120); do
  if ! pgrep -u "$(id -u)" -x slopos-session >/dev/null 2>&1 \
     && ! pgrep -u "$(id -u)" -x slopos-shell >/dev/null 2>&1 \
     && ! pgrep -u "$(id -u)" -x openbox >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
! pgrep -u "$(id -u)" -x slopos-session >/dev/null 2>&1 || fail "old supervisor did not stop"
! pgrep -u "$(id -u)" -x slopos-shell >/dev/null 2>&1 || fail "old shell did not stop"
! pgrep -u "$(id -u)" -x openbox >/dev/null 2>&1 || fail "old Openbox did not stop"

home2="$tmp/home-missing-supervisor"
vendor2="$tmp/vendor-missing-supervisor"
backup2="$tmp/backup-missing-supervisor"
mkdir -p "$home2/.config/slopos-i" "$home2/.config/openbox" "$vendor2/openbox"
printf '%s\n' 'graphite' >"$home2/.config/slopos-i/appearance"
printf '%s\n' 'platinum' >"$vendor2/appearance"
printf '%s\n' '<vendor-openbox-2/>' >"$vendor2/openbox/rc.xml"
PATH="$ROOT/scripts:$PATH" \
HOME="$home2" \
SLOPOS_VENDOR_CONFIG_DIR="$vendor2" \
SLOPOS_RECOVERY_BACKUP_DIR="$backup2" \
SLOPOS_RECOVERY_LOG="$tmp/recovery-new-session.log" \
  bash scripts/slopos-recovery.sh | tee "$tmp/recovery-new.log"
grep -Fqx 'SLOPOS_RECOVERY_STATUS_0' "$tmp/recovery-new.log" || fail "new-session recovery marker missing"
wait_for "replacement supervisor" pgrep -u "$(id -u)" -x slopos-session
wait_for "replacement Openbox" pgrep -u "$(id -u)" -x openbox
wait_for "replacement shell" pgrep -u "$(id -u)" -x slopos-shell
SESSION_PID="$(pgrep -u "$(id -u)" -xo slopos-session)"
test "$(pgrep -u "$(id -u)" -xc slopos-session)" -eq 1 || fail "fallback created duplicate supervisors"
test "$(pgrep -u "$(id -u)" -xc openbox)" -eq 1 || fail "fallback created duplicate Openbox processes"
test "$(pgrep -u "$(id -u)" -xc slopos-shell)" -eq 1 || fail "fallback created duplicate shells"
grep -Fqx 'graphite' "$backup2/slopos-i/appearance" || fail "fallback recovery did not preserve config"
grep -Fqx 'platinum' "$home2/.config/slopos-i/appearance" || fail "fallback recovery did not stage defaults"

echo 'RECOVERY_NEW_SUPERVISOR_STATUS_0'
echo 'SLOPOS_RECOVERY_QA_STATUS_0'
trap - EXIT
cleanup

#!/usr/bin/env bash
set -Eeuo pipefail
umask 077

# Linux UTM launcher for one exact-head DRM session.  This script deliberately
# does not build, discover, or terminate any process by name; it only starts
# the recorded release session in the foreground after all ownership gates.

readonly EXPECTED_USER="ubuntu"
readonly EXPECTED_UID="1000"
readonly CHECKOUT="/home/ubuntu/rust-slopos-qa-d42d09e"
readonly EXPECTED_HEAD="4816b4bf15ee2973f1c832f18808b1ea51cd5459"
readonly SOURCE_REL="crates/slopos-compositor/src/screenshot.rs"
readonly EXPECTED_SOURCE_SHA256="b7d39231b7dffec11e92e614c4b4f61cb970ba74ea52a21a3d3b5944ee0e8f19"
readonly TASK_ROOT="/tmp/slopos-utm-ui-baseline-4816b4b-live"
readonly XDG_RUNTIME_DIR_EXPECTED="/run/user/1000"
readonly DBUS_SOCKET="/run/user/1000/bus"
readonly CARGO_TARGET_DIR_EXPECTED="/home/ubuntu/.cache/slopos-i/cargo-target"
readonly BIN_DIR="/home/ubuntu/.cache/slopos-i/cargo-target/release"

fail() {
    printf 'run-live-session: %s\n' "$*" >&2
    exit 1
}

require_dir_700_uid() {
    local path="$1"
    [[ -d "$path" ]] || fail "required directory is missing: $path"
    [[ "$(stat -c '%u %a' "$path")" == "$EXPECTED_UID 700" ]] || \
        fail "unsafe directory owner/mode for $path: $(stat -c '%u %a' "$path")"
}

require_new_output() {
    local path="$1"
    [[ ! -e "$path" ]] || fail "retained output already exists (refusing overwrite): $path"
}

[[ "$(id -un)" == "$EXPECTED_USER" ]] || fail "must run as $EXPECTED_USER"
[[ "$(id -u)" == "$EXPECTED_UID" ]] || fail "must run with UID $EXPECTED_UID"
[[ -t 0 ]] || fail "stdin is not a console TTY"
TTY_PATH="$(tty 2>/dev/null)" || fail "unable to identify console TTY"
[[ "$TTY_PATH" == "/dev/tty1" ]] || fail "required console is /dev/tty1 (got $TTY_PATH)"

command -v loginctl >/dev/null 2>&1 || fail "loginctl is required"
export XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR_EXPECTED"
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
require_dir_700_uid "$XDG_RUNTIME_DIR"
[[ -S "$DBUS_SOCKET" ]] || fail "session D-Bus socket is missing: $DBUS_SOCKET"
[[ "$(stat -c '%u' "$DBUS_SOCKET")" == "$EXPECTED_UID" ]] || \
    fail "session D-Bus socket is not owned by UID $EXPECTED_UID"

ACTIVE_SESSION=""
for session_id in $(loginctl list-sessions --no-legend | awk -v uid="$EXPECTED_UID" '$2 == uid {print $1}'); do
    session_user="$(loginctl show-session "$session_id" -p User --value 2>/dev/null || true)"
    session_active="$(loginctl show-session "$session_id" -p Active --value 2>/dev/null || true)"
    session_remote="$(loginctl show-session "$session_id" -p Remote --value 2>/dev/null || true)"
    session_type="$(loginctl show-session "$session_id" -p Type --value 2>/dev/null || true)"
    session_tty="$(loginctl show-session "$session_id" -p TTY --value 2>/dev/null || true)"
    if [[ "$session_user" == "$EXPECTED_UID" && "$session_active" == "yes" && \
        "$session_remote" == "no" && "$session_type" == "tty" && "$session_tty" == "tty1" ]]; then
        ACTIVE_SESSION="$session_id"
        break
    fi
done
[[ -n "$ACTIVE_SESSION" ]] || fail "no active local ubuntu tty1 logind session"
loginctl show-session "$ACTIVE_SESSION" -p Name -p User -p Type -p Class -p Active -p Remote -p TTY

require_dir_700_uid "$TASK_ROOT"
for owned_dir in evidence config data cache; do
    require_dir_700_uid "$TASK_ROOT/$owned_dir"
done
export XDG_CONFIG_HOME="$TASK_ROOT/config"
export XDG_DATA_HOME="$TASK_ROOT/data"
export XDG_CACHE_HOME="$TASK_ROOT/cache"

cd "$CHECKOUT" || fail "checkout is not accessible: $CHECKOUT"
[[ "$(git rev-parse --show-toplevel)" == "$CHECKOUT" ]] || fail "unexpected checkout root"
[[ "$(git rev-parse HEAD)" == "$EXPECTED_HEAD" ]] || fail "checkout HEAD is not $EXPECTED_HEAD"
if ! git diff --quiet; then
    fail "tracked worktree changes are present"
fi
if ! git diff --cached --quiet; then
    fail "staged/index changes are present"
fi

STATUS_FILE="$TASK_ROOT/git-status.porcelain"
require_new_output "$STATUS_FILE"
git status --porcelain=v1 -z --untracked-files=all >"$STATUS_FILE"
while IFS= read -r -d '' status_entry; do
    status_code="${status_entry:0:2}"
    status_path="${status_entry:3}"
    [[ "$status_code" == "??" ]] || fail "unexpected tracked/index status: $status_entry"
    case "$status_path" in
        artifacts/qa/*) ;;
        *) fail "untracked path is outside artifacts/qa: $status_path" ;;
    esac
done <"$STATUS_FILE"

SOURCE_PATH="$CHECKOUT/$SOURCE_REL"
[[ -f "$SOURCE_PATH" ]] || fail "source file is missing: $SOURCE_PATH"
SOURCE_SHA256="$(sha256sum "$SOURCE_PATH" | awk '{print $1}')"
[[ "$SOURCE_SHA256" == "$EXPECTED_SOURCE_SHA256" ]] || fail "source SHA-256 mismatch"

readonly BINARIES=(slopos-session slopos-compositor slopos-shell finder settings textedit)
for binary in "${BINARIES[@]}"; do
    binary_path="$BIN_DIR/$binary"
    [[ -f "$binary_path" && -x "$binary_path" ]] || fail "release binary is missing or not executable: $binary_path"
done

PID_FILE="$TASK_ROOT/session.pid"
HEAD_FILE="$TASK_ROOT/head.txt"
SOURCE_FILE="$TASK_ROOT/source.sha256"
BINARY_FILE="$TASK_ROOT/binaries.sha256"
PROVENANCE_FILE="$TASK_ROOT/provenance.env"
SESSION_LOG="$TASK_ROOT/session.log"
for output_file in "$PID_FILE" "$HEAD_FILE" "$SOURCE_FILE" "$BINARY_FILE" "$PROVENANCE_FILE" "$SESSION_LOG"; do
    require_new_output "$output_file"
done

printf '%s\n' "$$" >"$PID_FILE"
printf '%s\n' "$EXPECTED_HEAD" >"$HEAD_FILE"
printf '%s  %s\n' "$SOURCE_SHA256" "$SOURCE_REL" >"$SOURCE_FILE"
{
    for binary in "${BINARIES[@]}"; do
        sha256sum "$BIN_DIR/$binary"
    done
} >"$BINARY_FILE"

unset DISPLAY WAYLAND_DISPLAY SWAYSOCK SLOPOS_SESSION_RUNTIME_DIR \
    SLOPOS_CLIENT_WAYLAND_DISPLAY SLOPOS_HOST_WAYLAND_DISPLAY \
    SLOPOS_START_APP SLOPOS_START_SPOTLIGHT SLOPOS_VISION_MODELS_DIR SLOPOS_VISION_SOCKET
export CARGO_TARGET_DIR="$CARGO_TARGET_DIR_EXPECTED"
export PATH="$BIN_DIR:${PATH:-/usr/bin:/bin}"
export RUST_LOG="info"
export RUST_BACKTRACE="1"
export SLOPOS_COMPOSITOR_WIDTH="1280"
export SLOPOS_COMPOSITOR_HEIGHT="800"

{
    printf 'launcher_pid=%s\n' "$$"
    printf 'session_pid=%s\n' "$$"
    printf 'user=%s\n' "$EXPECTED_USER"
    printf 'uid=%s\n' "$EXPECTED_UID"
    printf 'tty=%s\n' "$TTY_PATH"
    printf 'active_session=%s\n' "$ACTIVE_SESSION"
    printf 'checkout=%s\n' "$CHECKOUT"
    printf 'head=%s\n' "$EXPECTED_HEAD"
    printf 'source=%s\n' "$SOURCE_REL"
    printf 'source_sha256=%s\n' "$SOURCE_SHA256"
    printf 'cargo_target_dir=%s\n' "$CARGO_TARGET_DIR"
    printf 'bin_dir=%s\n' "$BIN_DIR"
    printf 'xdg_runtime_dir=%s\n' "$XDG_RUNTIME_DIR"
    printf 'xdg_config_home=%s\n' "$XDG_CONFIG_HOME"
    printf 'xdg_data_home=%s\n' "$XDG_DATA_HOME"
    printf 'xdg_cache_home=%s\n' "$XDG_CACHE_HOME"
    printf 'dbus_session_bus_address=%s\n' "$DBUS_SESSION_BUS_ADDRESS"
    printf 'command=%s --backend drm\n' "$BIN_DIR/slopos-session"
} >"$PROVENANCE_FILE"

{
    printf 'launcher=run-live-session.sh\n'
    printf 'checkout=%s\n' "$CHECKOUT"
    printf 'head=%s\n' "$EXPECTED_HEAD"
    printf 'source_sha256=%s\n' "$SOURCE_SHA256"
    printf 'session_pid=%s\n' "$$"
    printf 'command=%s --backend drm\n' "$BIN_DIR/slopos-session"
} >"$SESSION_LOG"

exec "$BIN_DIR/slopos-session" --backend drm >>"$SESSION_LOG" 2>&1

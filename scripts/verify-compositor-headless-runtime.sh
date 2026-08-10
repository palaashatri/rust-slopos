#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Runtime protocol smoke test for the SLOPOS-I compositor's own headless backend.
# It proves that the exact build owns a private socket, publishes authenticated
# readiness, serves registry clients, survives abrupt role disconnects, applies
# live xdg-toplevel presentation transitions, completes xdg-popup configure and
# reposition lifecycles, rejects client DnD without an implicit-grab serial,
# accepts a healthy client after stress, exercises text-input-v3/input-method-v2
# with two native clients, and terminates. The positive DnD check uses an
# explicit, env-gated synthetic input control path and is protocol evidence
# only; it does not claim physical input, GTK/Qt/Electron or hardware.
# It does not claim DRM/KMS, rendering, popup grabs, HDR, VRR or XWayland.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  printf 'verify-compositor-headless-runtime: Linux is required\n' >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in cargo git sed grep stat timeout wayland-info python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'verify-compositor-headless-runtime: missing required tool: %s\n' "$tool" >&2
    exit 2
  fi
done

commit_sha="$(git rev-parse HEAD)"
branch="$(git branch --show-current || true)"
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
artifact_dir="${SLOPOS_QA_ARTIFACT_DIR:-artifacts/qa/compositor-headless-runtime}"
mkdir -p "$artifact_dir"
artifact="$artifact_dir/${commit_sha}.json"
compositor_log="$artifact_dir/${commit_sha}-compositor.log"
globals_log="$artifact_dir/${commit_sha}-wayland-info.log"
stress_log="$artifact_dir/${commit_sha}-disconnect-stress.log"
protocol_log="$artifact_dir/${commit_sha}-xdg-protocol.log"
pointer_constraints_log="$artifact_dir/${commit_sha}-pointer-constraints.log"
clipboard_log="$artifact_dir/${commit_sha}-clipboard.log"
clipboard_source_log="$artifact_dir/${commit_sha}-clipboard-source.log"
clipboard_replacement_source_log="$artifact_dir/${commit_sha}-clipboard-replacement-source.log"
clipboard_sink_log="$artifact_dir/${commit_sha}-clipboard-sink.log"
clipboard_sink_abort_log="$artifact_dir/${commit_sha}-clipboard-sink-abort.log"
primary_selection_log="$artifact_dir/${commit_sha}-primary-selection.log"
primary_selection_source_log="$artifact_dir/${commit_sha}-primary-selection-source.log"
primary_selection_sink_log="$artifact_dir/${commit_sha}-primary-selection-sink.log"
dnd_log="$artifact_dir/${commit_sha}-dnd.log"
dnd_source_log="$artifact_dir/${commit_sha}-dnd-source.log"
dnd_target_log="$artifact_dir/${commit_sha}-dnd-target.log"
dnd_abort_source_log="$artifact_dir/${commit_sha}-dnd-target-disconnect-source.log"
dnd_abort_target_log="$artifact_dir/${commit_sha}-dnd-target-disconnect-target.log"
text_input_log="$artifact_dir/${commit_sha}-text-input.log"
text_input_app_log="$artifact_dir/${commit_sha}-text-input-app.log"
text_input_ime_log="$artifact_dir/${commit_sha}-text-input-ime.log"
runtime_dir="$(mktemp -d "${TMPDIR:-/tmp}/slopos-headless-runtime.XXXXXX")"
chmod 700 "$runtime_dir"

compositor_pid=""
clipboard_source_pid=""
primary_selection_source_pid=""
dnd_source_pid=""
dnd_target_pid=""
dnd_abort_source_pid=""
dnd_abort_target_pid=""
text_input_ime_pid=""
socket_name=""
shutdown_status="not_started"
socket_cleanup="not_observed"

has_protocol_marker() {
  local marker="$1"
  [[ -s "$protocol_log" ]] && grep -q "^${marker} " "$protocol_log"
}

stress_passed() {
  [[ -s "$stress_log" ]] && grep -q '^SLOPOS_ABRUPT_DISCONNECT_STRESS cycles=' "$stress_log"
}

has_clipboard_marker() {
  local marker="$1"
  for log in "$clipboard_sink_log" "$clipboard_sink_abort_log"; do
    if [[ -s "$log" ]] && grep -q "^${marker} " "$log"; then
      return 0
    fi
  done
  return 1
}

has_compositor_marker() {
  local marker="$1"
  [[ -s "$compositor_log" ]] && grep -q "$marker" "$compositor_log"
}

has_selection_target_disconnected_marker() {
  for log in "$compositor_log" "$clipboard_source_log" "$clipboard_replacement_source_log"; do
    if [[ -s "$log" ]] && grep -q "$1" "$log"; then
      return 0
    fi
  done
  return 1
}

clipboard_source_cancelled_exactly_once() {
  [[ -s "$clipboard_source_log" ]] &&
    [[ "$(grep -c '^SLOPOS_CLIPBOARD_SOURCE_CANCELLED$' "$clipboard_source_log" || true)" == 1 ]]
}

has_primary_selection_marker() {
  local marker="$1"
  [[ -s "$primary_selection_sink_log" ]] && grep -q "^${marker} " "$primary_selection_sink_log"
}

has_dnd_marker() {
  [[ -s "$dnd_log" ]] && grep -Eq "^$1([[:space:]]|$)" "$dnd_log"
}

has_dnd_abort_marker() {
  local marker="$1"
  for log in "$dnd_abort_source_log" "$dnd_abort_target_log"; do
    if [[ -s "$log" ]] && grep -Eq "^${marker}([[:space:]]|$)" "$log"; then
      return 0
    fi
  done
  return 1
}

has_text_input_marker() {
  local marker="$1"
  for log in "$text_input_app_log" "$text_input_ime_log"; do
    if [[ -s "$log" ]] && grep -q "^${marker} " "$log"; then
      return 0
    fi
  done
  return 1
}

text_input_marker_exactly_once() {
  local marker="$1"
  local count=0
  for log in "$text_input_app_log" "$text_input_ime_log"; do
    if [[ -s "$log" ]]; then
      count=$((count + $(grep -c "^${marker} " "$log" || true)))
    fi
  done
  [[ "$count" == 1 ]]
}

send_headless_input() {
  local event_json="$1"
  python3 - "$runtime_dir/control.sock" "$event_json" <<'PY'
import json
import socket
import sys

socket_path, event_json = sys.argv[1:]
payload = {
    "HeadlessTestInput": {
        "event": json.loads(event_json),
    }
}
sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sock.sendto(json.dumps(payload).encode("utf-8"), socket_path)
sock.close()
PY
}

last_window_origins() {
  sed -n "s/.*surface mapped at (\\([0-9][0-9]*\\),\\([0-9][0-9]*\\)).*/\\1 \\2/p" "$compositor_log" | tail -n 2
}

combine_clipboard_logs() {
  : >"$clipboard_log"
  [[ -f "$clipboard_source_log" ]] && cat "$clipboard_source_log" >>"$clipboard_log"
  [[ -f "$clipboard_replacement_source_log" ]] && cat "$clipboard_replacement_source_log" >>"$clipboard_log"
  [[ -f "$clipboard_sink_log" ]] && cat "$clipboard_sink_log" >>"$clipboard_log"
  [[ -f "$clipboard_sink_abort_log" ]] && cat "$clipboard_sink_abort_log" >>"$clipboard_log"
}

combine_dnd_logs() {
  # The invalid-serial probe and the successful source/target logs are already
  # appended to dnd_log in sequence. Preserve those markers and append only the
  # target-disconnect failure-path logs here.
  [[ -f "$dnd_abort_source_log" ]] && cat "$dnd_abort_source_log" >>"$dnd_log"
  [[ -f "$dnd_abort_target_log" ]] && cat "$dnd_abort_target_log" >>"$dnd_log"
}

combine_primary_selection_logs() {
  : >"$primary_selection_log"
  [[ -f "$primary_selection_source_log" ]] && cat "$primary_selection_source_log" >>"$primary_selection_log"
  [[ -f "$primary_selection_sink_log" ]] && cat "$primary_selection_sink_log" >>"$primary_selection_log"
}

combine_text_input_logs() {
  : >"$text_input_log"
  [[ -f "$text_input_app_log" ]] && cat "$text_input_app_log" >>"$text_input_log"
  [[ -f "$text_input_ime_log" ]] && cat "$text_input_ime_log" >>"$text_input_log"
}

write_artifact() {
  local status="$1"
  local failure="${2:-}"
  cat >"$artifact.tmp" <<JSON
{
  "schema": 12,
  "component": "slopos-compositor",
  "commit": "$commit_sha",
  "branch": "$branch",
  "started_at_utc": "$timestamp",
  "status": "$status",
  "failure": "$failure",
  "evidence_level": "headless_runtime_protocol_smoke",
  "backend": "headless",
  "runtime_verified": $([[ "$status" == "passed" ]] && printf true || printf false),
  "registry_client_verified": $([[ -s "$globals_log" ]] && printf true || printf false),
  "abrupt_disconnect_recovery_verified": $(stress_passed && printf true || printf false),
  "xdg_toplevel_configure_verified": $(has_protocol_marker SLOPOS_XDG_TOPLEVEL_CONFIGURED && printf true || printf false),
  "xdg_toplevel_maximize_verified": $(has_protocol_marker SLOPOS_XDG_TOPLEVEL_MAXIMIZED && printf true || printf false),
  "xdg_toplevel_fullscreen_verified": $(has_protocol_marker SLOPOS_XDG_TOPLEVEL_FULLSCREEN && printf true || printf false),
  "xdg_toplevel_restore_verified": $(has_protocol_marker SLOPOS_XDG_TOPLEVEL_RESTORED && printf true || printf false),
  "xdg_popup_configure_verified": $(has_protocol_marker SLOPOS_XDG_POPUP_CONFIGURED && printf true || printf false),
  "xdg_popup_reposition_verified": $(has_protocol_marker SLOPOS_XDG_POPUP_REPOSITIONED && printf true || printf false),
  "pointer_constraints_registry_verified": $([[ -s "$globals_log" ]] && grep -q "interface: 'zwp_pointer_constraints_v1'" "$globals_log" && printf true || printf false),
  "pointer_lock_request_verified": $([[ -s "$pointer_constraints_log" ]] && grep -q "^SLOPOS_POINTER_LOCK_REQUEST_ACCEPTED " "$pointer_constraints_log" && printf true || printf false),
  "pointer_confine_request_verified": $([[ -s "$pointer_constraints_log" ]] && grep -q "^SLOPOS_POINTER_CONFINE_REQUEST_ACCEPTED " "$pointer_constraints_log" && printf true || printf false),
  "clipboard_offer_verified": $(has_clipboard_marker SLOPOS_CLIPBOARD_OFFER_VERIFIED && printf true || printf false),
  "clipboard_transfer_verified": $(has_clipboard_marker SLOPOS_CLIPBOARD_TRANSFER_VERIFIED && printf true || printf false),
  "clipboard_large_transfer_verified": $(has_clipboard_marker SLOPOS_CLIPBOARD_LARGE_TRANSFER_VERIFIED && printf true || printf false),
  "clipboard_missing_mime_eof_verified": $(has_clipboard_marker SLOPOS_CLIPBOARD_MISSING_MIME_EOF_VERIFIED && printf true || printf false),
  "clipboard_source_death_cleared": $(has_clipboard_marker SLOPOS_CLIPBOARD_SOURCE_DEATH_CLEARED && printf true || printf false),
  "clipboard_source_cancelled_verified": $(clipboard_source_cancelled_exactly_once && printf true || printf false),
  "clipboard_target_death_recovered_verified": $(has_clipboard_marker SLOPOS_CLIPBOARD_TARGET_DEATH_RECOVERED && printf true || printf false),
  "selection_target_disconnected_verified": $(has_selection_target_disconnected_marker SLOPOS_SELECTION_TARGET_DISCONNECTED && printf true || printf false),
  "primary_selection_offer_verified": $(has_primary_selection_marker SLOPOS_PRIMARY_SELECTION_OFFER_VERIFIED && printf true || printf false),
  "primary_selection_transfer_verified": $(has_primary_selection_marker SLOPOS_PRIMARY_SELECTION_TRANSFER_VERIFIED && printf true || printf false),
  "primary_selection_missing_mime_eof_verified": $(has_primary_selection_marker SLOPOS_PRIMARY_SELECTION_MISSING_MIME_EOF_VERIFIED && printf true || printf false),
  "dnd_invalid_serial_rejected": $(has_dnd_marker SLOPOS_DND_INVALID_SERIAL_REJECTED && printf true || printf false),
  "dnd_cross_client_offer_verified": $(has_dnd_marker SLOPOS_DND_OFFER_VERIFIED && printf true || printf false),
  "dnd_cross_client_text_transfer_verified": $(has_dnd_marker SLOPOS_DND_TEXT_TRANSFER_VERIFIED && printf true || printf false),
  "dnd_cross_client_uri_transfer_verified": $(has_dnd_marker SLOPOS_DND_URI_TRANSFER_VERIFIED && printf true || printf false),
  "dnd_cross_client_client_started": $(has_compositor_marker SLOPOS_DND_CLIENT_STARTED && printf true || printf false),
  "dnd_cross_client_drag_icon_verified": $(has_compositor_marker SLOPOS_DND_ICON_ATTACHED && printf true || printf false),
  "dnd_cross_client_client_dropped": $(has_compositor_marker SLOPOS_DND_DROPPED && printf true || printf false),
  "dnd_cross_client_drop_verified": $(has_dnd_marker SLOPOS_DND_SOURCE_DROP_PERFORMED && printf true || printf false),
  "dnd_target_disconnect_cancelled_verified": $(has_dnd_abort_marker SLOPOS_DND_SOURCE_CANCELLED && printf true || printf false),
  "dnd_target_disconnect_target_exit_verified": $(has_dnd_abort_marker SLOPOS_DND_TARGET_DISCONNECTED && printf true || printf false),
  "text_input_registry_verified": $([[ -s "$globals_log" ]] && grep -q "interface: 'zwp_text_input_manager_v3'" "$globals_log" && grep -q "interface: 'zwp_input_method_manager_v2'" "$globals_log" && printf true || printf false),
  "text_input_app_enter_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_APP_ENTER && printf true || printf false),
  "text_input_app_done_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_APP_DONE && printf true || printf false),
  "text_input_preedit_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_PREEDIT_VERIFIED && printf true || printf false),
  "text_input_commit_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_COMMIT_VERIFIED && printf true || printf false),
  "text_input_delete_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_DELETE_VERIFIED && printf true || printf false),
  "input_method_activate_verified": $(text_input_marker_exactly_once SLOPOS_IME_ACTIVATE && printf true || printf false),
  "input_method_surrounding_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_SURROUNDING_VERIFIED && printf true || printf false),
  "input_method_content_type_verified": $(has_text_input_marker SLOPOS_TEXT_INPUT_CONTENT_TYPE_VERIFIED && printf true || printf false),
  "input_method_commit_sent_verified": $(has_text_input_marker SLOPOS_IME_COMMIT_SENT && printf true || printf false),
  "input_method_deactivate_verified": $(text_input_marker_exactly_once SLOPOS_IME_DEACTIVATE && printf true || printf false),
  "text_input_log": "$(basename "$text_input_log")",
  "text_input_app_log": "$(basename "$text_input_app_log")",
  "text_input_ime_log": "$(basename "$text_input_ime_log")",
  "hardware_verified": false,
  "drm_verified": false,
  "rendering_verified": false,
  "input_verified": false,
  "popup_grab_verified": false,
  "socket": "$socket_name",
  "compositor_pid": "${compositor_pid:-}",
  "shutdown_status": "$shutdown_status",
  "socket_cleanup": "$socket_cleanup",
  "runtime_directory_owner": "slopos-session",
  "compositor_log": "$(basename "$compositor_log")",
  "wayland_info_log": "$(basename "$globals_log")",
  "disconnect_stress_log": "$(basename "$stress_log")",
  "xdg_protocol_log": "$(basename "$protocol_log")",
  "pointer_constraints_log": "$(basename "$pointer_constraints_log")",
  "clipboard_log": "$(basename "$clipboard_log")",
  "clipboard_source_log": "$(basename "$clipboard_source_log")",
  "clipboard_replacement_source_log": "$(basename "$clipboard_replacement_source_log")",
  "clipboard_sink_log": "$(basename "$clipboard_sink_log")",
  "clipboard_sink_abort_log": "$(basename "$clipboard_sink_abort_log")",
  "primary_selection_log": "$(basename "$primary_selection_log")",
  "primary_selection_source_log": "$(basename "$primary_selection_source_log")",
  "primary_selection_sink_log": "$(basename "$primary_selection_sink_log")",
  "dnd_log": "$(basename "$dnd_log")",
  "dnd_source_log": "$(basename "$dnd_source_log")",
  "dnd_target_log": "$(basename "$dnd_target_log")",
  "dnd_abort_source_log": "$(basename "$dnd_abort_source_log")",
  "dnd_abort_target_log": "$(basename "$dnd_abort_target_log")"
}
JSON
  mv "$artifact.tmp" "$artifact"
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  if [[ -n "$clipboard_source_pid" ]] && kill -0 "$clipboard_source_pid" 2>/dev/null; then
    kill -TERM "$clipboard_source_pid" 2>/dev/null || true
    wait "$clipboard_source_pid" 2>/dev/null || true
  fi
  if [[ -n "$primary_selection_source_pid" ]] && kill -0 "$primary_selection_source_pid" 2>/dev/null; then
    kill -TERM "$primary_selection_source_pid" 2>/dev/null || true
    wait "$primary_selection_source_pid" 2>/dev/null || true
  fi
  if [[ -n "$dnd_source_pid" ]] && kill -0 "$dnd_source_pid" 2>/dev/null; then
    kill -TERM "$dnd_source_pid" 2>/dev/null || true
    wait "$dnd_source_pid" 2>/dev/null || true
  fi
  if [[ -n "$dnd_target_pid" ]] && kill -0 "$dnd_target_pid" 2>/dev/null; then
    kill -TERM "$dnd_target_pid" 2>/dev/null || true
    wait "$dnd_target_pid" 2>/dev/null || true
  fi
  if [[ -n "$dnd_abort_source_pid" ]] && kill -0 "$dnd_abort_source_pid" 2>/dev/null; then
    kill -TERM "$dnd_abort_source_pid" 2>/dev/null || true
    wait "$dnd_abort_source_pid" 2>/dev/null || true
  fi
  if [[ -n "$dnd_abort_target_pid" ]] && kill -0 "$dnd_abort_target_pid" 2>/dev/null; then
    kill -TERM "$dnd_abort_target_pid" 2>/dev/null || true
    wait "$dnd_abort_target_pid" 2>/dev/null || true
  fi
  if [[ -n "$text_input_ime_pid" ]] && kill -0 "$text_input_ime_pid" 2>/dev/null; then
    kill -TERM "$text_input_ime_pid" 2>/dev/null || true
    wait "$text_input_ime_pid" 2>/dev/null || true
  fi
  combine_clipboard_logs
  combine_primary_selection_logs
  combine_text_input_logs
  if [[ -n "$compositor_pid" ]] && kill -0 "$compositor_pid" 2>/dev/null; then
    kill -TERM "$compositor_pid" 2>/dev/null || true
    for _ in $(seq 1 50); do
      if ! kill -0 "$compositor_pid" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$compositor_pid" 2>/dev/null; then
      kill -KILL "$compositor_pid" 2>/dev/null || true
    fi
    wait "$compositor_pid" 2>/dev/null || true
  fi
  rm -rf "$runtime_dir"
  if [[ $exit_code -ne 0 && ! -f "$artifact" ]]; then
    write_artifact failed "unexpected_exit_$exit_code"
  fi
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  write_artifact failed "tracked_worktree_dirty"
  printf 'verify-compositor-headless-runtime: tracked working tree is dirty\n' >&2
  exit 2
fi

printf 'Building exact-commit compositor %s\n' "$commit_sha"
cargo build -p slopos-compositor --bin slopos-compositor --examples --locked

export XDG_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_TOKEN="headless-smoke-${commit_sha}-$$"
export SLOPOS_TEST_INPUT=1
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY

printf 'Starting SLOPOS-owned headless compositor\n'
target/debug/slopos-compositor --backend headless >"$compositor_log" 2>&1 &
compositor_pid=$!

readiness="$runtime_dir/readiness"
for _ in $(seq 1 100); do
  if [[ -s "$readiness" ]]; then
    break
  fi
  if ! kill -0 "$compositor_pid" 2>/dev/null; then
    wait "$compositor_pid" || true
    write_artifact failed "compositor_exited_before_readiness"
    cat "$compositor_log" >&2
    exit 1
  fi
  sleep 0.1
done

if [[ ! -s "$readiness" ]]; then
  write_artifact failed "readiness_timeout"
  cat "$compositor_log" >&2
  exit 1
fi

socket_name="$(sed -n '1p' "$readiness")"
ready_pid="$(sed -n 's/^pid=//p' "$readiness")"
ready_token="$(sed -n 's/^token=//p' "$readiness")"
ready_width="$(sed -n 's/^width=//p' "$readiness")"
ready_height="$(sed -n 's/^height=//p' "$readiness")"

if [[ ! "$socket_name" =~ ^wayland-[0-9]+$ ]]; then
  write_artifact failed "invalid_socket_name"
  exit 1
fi
if [[ "$ready_pid" != "$compositor_pid" ]]; then
  write_artifact failed "readiness_pid_mismatch"
  exit 1
fi
if [[ "$ready_token" != "$SLOPOS_SESSION_TOKEN" ]]; then
  write_artifact failed "readiness_token_mismatch"
  exit 1
fi
if [[ ! "$ready_width" =~ ^[1-9][0-9]*$ || ! "$ready_height" =~ ^[1-9][0-9]*$ ]]; then
  write_artifact failed "invalid_output_dimensions"
  exit 1
fi
if [[ ! -S "$runtime_dir/$socket_name" ]]; then
  write_artifact failed "wayland_socket_missing"
  exit 1
fi

runtime_mode="$(stat -c '%a' "$runtime_dir")"
if [[ "$runtime_mode" != "700" ]]; then
  write_artifact failed "runtime_directory_not_private"
  exit 1
fi

printf 'Connecting registry client to %s\n' "$socket_name"
WAYLAND_DISPLAY="$socket_name" timeout 10s wayland-info >"$globals_log" 2>&1
for required_global in wl_compositor wl_shm wl_seat xdg_wm_base zwp_relative_pointer_manager_v1 zwp_pointer_constraints_v1; do
  if ! grep -q "interface: '${required_global}'" "$globals_log"; then
    write_artifact failed "missing_global_${required_global}"
    cat "$globals_log" >&2
    exit 1
  fi
done

printf 'Exercising pointer-constraint request/destroy lifecycle\n'
WAYLAND_DISPLAY="$socket_name" timeout 20s \
  target/debug/examples/headless_pointer_constraints_client >"$pointer_constraints_log" 2>&1
for marker in SLOPOS_POINTER_LOCK_REQUEST_ACCEPTED SLOPOS_POINTER_CONFINE_REQUEST_ACCEPTED SLOPOS_POINTER_CONSTRAINTS_OK; do
  if ! grep -q "^${marker}" "$pointer_constraints_log"; then
    write_artifact failed "missing_${marker}"
    cat "$pointer_constraints_log" >&2
    exit 1
  fi
done
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_pointer_constraints"
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising client DnD invalid-serial rejection (no input grab available headlessly)\n'
WAYLAND_DISPLAY="$socket_name" timeout 20s \
  target/debug/examples/headless_clipboard_client dnd-invalid-serial >"$dnd_log" 2>&1
if ! has_dnd_marker SLOPOS_DND_INVALID_SERIAL_REJECTED; then
  write_artifact failed "missing_SLOPOS_DND_INVALID_SERIAL_REJECTED"
  cat "$dnd_log" >&2
  exit 1
fi
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_dnd_invalid_serial"
  cat "$dnd_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising native cross-client text/URI DnD with synthetic headless input\n'
WAYLAND_DISPLAY="$socket_name" timeout 45s \
  target/debug/examples/headless_dnd_client source >"$dnd_source_log" 2>&1 &
dnd_source_pid=$!
source_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_DND_SOURCE_READY$' "$dnd_source_log" 2>/dev/null; then
    source_ready=true
    break
  fi
  if ! kill -0 "$dnd_source_pid" 2>/dev/null; then
    wait "$dnd_source_pid" || true
    write_artifact failed "dnd_source_exited_before_ready"
    cat "$dnd_source_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$source_ready" != true ]]; then
  write_artifact failed "dnd_source_readiness_timeout"
  cat "$dnd_source_log" >&2
  exit 1
fi

WAYLAND_DISPLAY="$socket_name" timeout 45s \
  target/debug/examples/headless_dnd_client target >"$dnd_target_log" 2>&1 &
dnd_target_pid=$!
target_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_DND_TARGET_READY$' "$dnd_target_log" 2>/dev/null; then
    target_ready=true
    break
  fi
  if ! kill -0 "$dnd_target_pid" 2>/dev/null; then
    wait "$dnd_target_pid" || true
    write_artifact failed "dnd_target_exited_before_ready"
    cat "$dnd_target_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$target_ready" != true ]]; then
  write_artifact failed "dnd_target_readiness_timeout"
  cat "$dnd_target_log" >&2
  exit 1
fi

# The source is first in the compositor cascade at (64,64); the target is the
# later window at (96,96). The clients commit 320x240 buffers, so x=400..410
# is inside the target buffer but outside the raised source buffer (which ends
# at x=384 after the press focuses it). Move to the source, press to create a
# real implicit-grab serial, move over the target twice so the DnD grab emits
# both enter and motion, and release. These coordinates are protocol-test
# input, not a claim about physical pointer delivery.
send_headless_input '{"Motion":{"x":70,"y":70,"time_msec":100}}'
send_headless_input '{"Button":{"button":272,"pressed":true,"time_msec":110}}'
sleep 0.2
send_headless_input '{"Motion":{"x":400,"y":110,"time_msec":120}}'
sleep 0.2
send_headless_input '{"Motion":{"x":410,"y":120,"time_msec":125}}'
sleep 0.2
send_headless_input '{"Button":{"button":272,"pressed":false,"time_msec":130}}'

if ! wait "$dnd_target_pid"; then
  write_artifact failed "dnd_target_runtime_failed"
  cat "$dnd_source_log" >&2
  cat "$dnd_target_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi
dnd_target_pid=""
if ! wait "$dnd_source_pid"; then
  write_artifact failed "dnd_source_runtime_failed"
  cat "$dnd_source_log" >&2
  cat "$dnd_target_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi
dnd_source_pid=""
cat "$dnd_source_log" "$dnd_target_log" >>"$dnd_log"
for marker in \
  SLOPOS_DND_SOURCE_STARTED \
  SLOPOS_DND_ENTER_VERIFIED \
  SLOPOS_DND_OFFER_VERIFIED \
  SLOPOS_DND_TEXT_TRANSFER_VERIFIED \
  SLOPOS_DND_URI_TRANSFER_VERIFIED \
  SLOPOS_DND_SOURCE_DROP_PERFORMED; do
  if ! has_dnd_marker "$marker"; then
    write_artifact failed "missing_${marker}"
    cat "$dnd_source_log" >&2
    cat "$dnd_target_log" >&2
    cat "$compositor_log" >&2
    exit 1
  fi
done
if ! has_compositor_marker SLOPOS_DND_CLIENT_STARTED; then
  write_artifact failed "missing_SLOPOS_DND_CLIENT_STARTED"
  cat "$compositor_log" >&2
  exit 1
fi
if ! has_compositor_marker SLOPOS_DND_ICON_ATTACHED; then
  write_artifact failed "missing_SLOPOS_DND_ICON_ATTACHED"
  cat "$compositor_log" >&2
  exit 1
fi
if ! has_compositor_marker SLOPOS_DND_DROPPED; then
  write_artifact failed "missing_SLOPOS_DND_DROPPED"
  cat "$compositor_log" >&2
  exit 1
fi
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_cross_client_dnd"
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising DnD target disconnect cancellation and compositor survival\n'
WAYLAND_DISPLAY="$socket_name" timeout 45s \
  target/debug/examples/headless_dnd_client source >"$dnd_abort_source_log" 2>&1 &
dnd_abort_source_pid=$!
source_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_DND_SOURCE_READY$' "$dnd_abort_source_log" 2>/dev/null; then
    source_ready=true
    break
  fi
  if ! kill -0 "$dnd_abort_source_pid" 2>/dev/null; then
    wait "$dnd_abort_source_pid" || true
    write_artifact failed "dnd_abort_source_exited_before_ready"
    cat "$dnd_abort_source_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$source_ready" != true ]]; then
  write_artifact failed "dnd_abort_source_readiness_timeout"
  cat "$dnd_abort_source_log" >&2
  exit 1
fi

WAYLAND_DISPLAY="$socket_name" timeout 45s \
  target/debug/examples/headless_dnd_client target-abort >"$dnd_abort_target_log" 2>&1 &
dnd_abort_target_pid=$!
target_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_DND_TARGET_READY$' "$dnd_abort_target_log" 2>/dev/null; then
    target_ready=true
    break
  fi
  if ! kill -0 "$dnd_abort_target_pid" 2>/dev/null; then
    wait "$dnd_abort_target_pid" || true
    write_artifact failed "dnd_abort_target_exited_before_ready"
    cat "$dnd_abort_target_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$target_ready" != true ]]; then
  write_artifact failed "dnd_abort_target_readiness_timeout"
  cat "$dnd_abort_target_log" >&2
  exit 1
fi

mapfile -t abort_origins < <(last_window_origins)
if [[ "${#abort_origins[@]}" -ne 2 ]]; then
  write_artifact failed "dnd_abort_window_geometry_missing"
  cat "$compositor_log" >&2
  exit 1
fi
read -r abort_source_x abort_source_y <<<"${abort_origins[0]}"
read -r abort_target_x abort_target_y <<<"${abort_origins[1]}"
# The origins come from the compositor's authoritative mapped-window log. The
# offsets stay within each 320x240 test buffer while remaining independent of
# the cascade position used by earlier protocol clients.
send_headless_input "{\"Motion\":{\"x\":$((abort_source_x + 8)),\"y\":$((abort_source_y + 8)),\"time_msec\":200}}"
send_headless_input '{"Button":{"button":272,"pressed":true,"time_msec":210}}'
sleep 0.2
# The source is raised for the implicit drag grab. Its right edge is 320px
# after the source origin; the target begins 32px later, so +300 in the target
# buffer is target-only while remaining inside the 320px target width.
send_headless_input "{\"Motion\":{\"x\":$((abort_target_x + 300)),\"y\":$((abort_target_y + 100)),\"time_msec\":220}}"

if ! wait "$dnd_abort_target_pid"; then
  write_artifact failed "dnd_abort_target_runtime_failed"
  cat "$dnd_abort_source_log" >&2
  cat "$dnd_abort_target_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi
dnd_abort_target_pid=""
send_headless_input '{"Button":{"button":272,"pressed":false,"time_msec":230}}' || true
if ! wait "$dnd_abort_source_pid"; then
  write_artifact failed "dnd_abort_source_runtime_failed"
  cat "$dnd_abort_source_log" >&2
  cat "$dnd_abort_target_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi
dnd_abort_source_pid=""
combine_dnd_logs
for marker in SLOPOS_DND_TARGET_ABORTING SLOPOS_DND_TARGET_DISCONNECTED SLOPOS_DND_SOURCE_CANCELLED; do
  if ! has_dnd_abort_marker "$marker"; then
    write_artifact failed "missing_${marker}"
    cat "$dnd_abort_source_log" >&2
    cat "$dnd_abort_target_log" >&2
    cat "$compositor_log" >&2
    exit 1
  fi
done
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_dnd_target_disconnect"
  cat "$dnd_abort_source_log" >&2
  cat "$dnd_abort_target_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising native text-input-v3/input-method-v2 lifecycle\n'
WAYLAND_DISPLAY="$socket_name" timeout 30s \
  target/debug/examples/headless_text_input_client ime >"$text_input_ime_log" 2>&1 &
text_input_ime_pid=$!
ime_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_IME_READY observed=true$' "$text_input_ime_log" 2>/dev/null; then
    ime_ready=true
    break
  fi
  if ! kill -0 "$text_input_ime_pid" 2>/dev/null; then
    wait "$text_input_ime_pid" || true
    write_artifact failed "text_input_ime_exited_before_ready"
    cat "$text_input_ime_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$ime_ready" != true ]]; then
  write_artifact failed "text_input_ime_readiness_timeout"
  cat "$text_input_ime_log" >&2
  exit 1
fi

if ! WAYLAND_DISPLAY="$socket_name" timeout 30s \
  target/debug/examples/headless_text_input_client app >"$text_input_app_log" 2>&1; then
  write_artifact failed "text_input_app_runtime_failed"
  cat "$text_input_app_log" >&2
  cat "$text_input_ime_log" >&2
  exit 1
fi
if ! wait "$text_input_ime_pid"; then
  write_artifact failed "text_input_ime_runtime_failed"
  cat "$text_input_app_log" >&2
  cat "$text_input_ime_log" >&2
  exit 1
fi
text_input_ime_pid=""
combine_text_input_logs
for marker in \
  SLOPOS_TEXT_INPUT_APP_ENTER \
  SLOPOS_TEXT_INPUT_APP_DONE \
  SLOPOS_TEXT_INPUT_PREEDIT_VERIFIED \
  SLOPOS_TEXT_INPUT_COMMIT_VERIFIED \
  SLOPOS_TEXT_INPUT_DELETE_VERIFIED \
  SLOPOS_IME_ACTIVATE \
  SLOPOS_IME_COMMIT_SENT \
  SLOPOS_TEXT_INPUT_SURROUNDING_VERIFIED \
  SLOPOS_TEXT_INPUT_CONTENT_TYPE_VERIFIED \
  SLOPOS_IME_DEACTIVATE; do
  if ! has_text_input_marker "$marker"; then
    write_artifact failed "missing_${marker}"
    cat "$text_input_app_log" >&2
    cat "$text_input_ime_log" >&2
    exit 1
  fi
done
if ! text_input_marker_exactly_once SLOPOS_IME_ACTIVATE \
  || ! text_input_marker_exactly_once SLOPOS_IME_DEACTIVATE; then
  write_artifact failed "duplicate_text_input_activation_marker"
  cat "$text_input_log" >&2
  exit 1
fi
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_text_input"
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising native cross-client clipboard offer, transfer and missing-MIME EOF\n'
WAYLAND_DISPLAY="$socket_name" timeout 120s \
  target/debug/examples/headless_clipboard_client source >"$clipboard_source_log" 2>&1 &
clipboard_source_pid=$!
source_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_CLIPBOARD_SOURCE_READY ' "$clipboard_source_log" 2>/dev/null; then
    source_ready=true
    break
  fi
  if ! kill -0 "$clipboard_source_pid" 2>/dev/null; then
    wait "$clipboard_source_pid" || true
    write_artifact failed "clipboard_source_exited_before_ready"
    cat "$clipboard_source_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$source_ready" != true ]]; then
  write_artifact failed "clipboard_source_readiness_timeout"
  cat "$clipboard_source_log" >&2
  exit 1
fi

WAYLAND_DISPLAY="$socket_name" timeout 30s \
  target/debug/examples/headless_clipboard_client sink >"$clipboard_sink_log" 2>&1
for marker in \
  SLOPOS_CLIPBOARD_OFFER_VERIFIED \
  SLOPOS_CLIPBOARD_TRANSFER_VERIFIED \
  SLOPOS_CLIPBOARD_LARGE_TRANSFER_VERIFIED \
  SLOPOS_CLIPBOARD_MISSING_MIME_EOF_VERIFIED; do
  if ! has_clipboard_marker "$marker"; then
    write_artifact failed "missing_${marker}"
    cat "$clipboard_source_log" >&2
    cat "$clipboard_sink_log" >&2
    exit 1
  fi
done

printf 'Exercising clipboard target death during a large transfer\n'
if ! WAYLAND_DISPLAY="$socket_name" timeout 30s \
  target/debug/examples/headless_clipboard_client sink-abort >"$clipboard_sink_abort_log" 2>&1; then
  write_artifact failed "clipboard_target_death_sink_failed"
  cat "$clipboard_source_log" >&2
  cat "$clipboard_sink_abort_log" >&2
  exit 1
fi
if ! has_clipboard_marker SLOPOS_CLIPBOARD_TARGET_DEATH_RECOVERED; then
  write_artifact failed "missing_SLOPOS_CLIPBOARD_TARGET_DEATH_RECOVERED"
  cat "$clipboard_sink_abort_log" >&2
  exit 1
fi
selection_target_disconnected=false
for _ in $(seq 1 100); do
  if has_selection_target_disconnected_marker SLOPOS_SELECTION_TARGET_DISCONNECTED; then
    selection_target_disconnected=true
    break
  fi
  if ! kill -0 "$compositor_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if [[ "$selection_target_disconnected" != true ]]; then
  write_artifact failed "missing_SLOPOS_SELECTION_TARGET_DISCONNECTED"
  cat "$clipboard_sink_abort_log" >&2
  cat "$clipboard_source_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_clipboard_target_death"
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising clipboard source cancellation on selection replacement\n'
if ! WAYLAND_DISPLAY="$socket_name" timeout 10s \
  target/debug/examples/headless_clipboard_client source-once >"$clipboard_replacement_source_log" 2>&1; then
  write_artifact failed "clipboard_source_replacement_failed"
  cat "$clipboard_source_log" >&2
  cat "$clipboard_replacement_source_log" >&2
  exit 1
fi
if ! grep -q '^SLOPOS_CLIPBOARD_SOURCE_READY ' "$clipboard_replacement_source_log"; then
  write_artifact failed "clipboard_source_replacement_not_ready"
  cat "$clipboard_replacement_source_log" >&2
  exit 1
fi
source_cancelled=false
for _ in $(seq 1 100); do
  if clipboard_source_cancelled_exactly_once; then
    source_cancelled=true
    break
  fi
  if ! kill -0 "$clipboard_source_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if [[ "$source_cancelled" != true ]]; then
  write_artifact failed "missing_or_duplicate_SLOPOS_CLIPBOARD_SOURCE_CANCELLED"
  cat "$clipboard_source_log" >&2
  cat "$clipboard_replacement_source_log" >&2
  exit 1
fi
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_clipboard_source_cancellation"
  cat "$compositor_log" >&2
  exit 1
fi

kill -TERM "$clipboard_source_pid" 2>/dev/null || true
wait "$clipboard_source_pid" 2>/dev/null || true
clipboard_source_pid=""

printf 'Exercising clipboard source-death clearing\n'
if ! WAYLAND_DISPLAY="$socket_name" timeout 10s \
  target/debug/examples/headless_clipboard_client source-once >>"$clipboard_source_log" 2>&1; then
  write_artifact failed "clipboard_source_death_source_failed"
  cat "$clipboard_source_log" >&2
  exit 1
fi
sleep 1
if ! WAYLAND_DISPLAY="$socket_name" timeout 10s \
  target/debug/examples/headless_clipboard_client sink-after-source-death >>"$clipboard_sink_log" 2>&1; then
  write_artifact failed "clipboard_source_death_sink_failed"
  cat "$clipboard_source_log" >&2
  cat "$clipboard_sink_log" >&2
  exit 1
fi
if ! has_clipboard_marker SLOPOS_CLIPBOARD_SOURCE_DEATH_CLEARED; then
  write_artifact failed "missing_SLOPOS_CLIPBOARD_SOURCE_DEATH_CLEARED"
  cat "$clipboard_source_log" >&2
  cat "$clipboard_sink_log" >&2
  exit 1
fi

combine_clipboard_logs
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_clipboard_transfer"
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Exercising native cross-client primary-selection offer, transfer and missing-MIME EOF\n'
WAYLAND_DISPLAY="$socket_name" timeout 120s \
  target/debug/examples/headless_clipboard_client primary-source >"$primary_selection_source_log" 2>&1 &
primary_selection_source_pid=$!
primary_source_ready=false
for _ in $(seq 1 100); do
  if grep -q '^SLOPOS_PRIMARY_SELECTION_SOURCE_READY ' "$primary_selection_source_log" 2>/dev/null; then
    primary_source_ready=true
    break
  fi
  if ! kill -0 "$primary_selection_source_pid" 2>/dev/null; then
    wait "$primary_selection_source_pid" || true
    write_artifact failed "primary_selection_source_exited_before_ready"
    cat "$primary_selection_source_log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$primary_source_ready" != true ]]; then
  write_artifact failed "primary_selection_source_readiness_timeout"
  cat "$primary_selection_source_log" >&2
  exit 1
fi

WAYLAND_DISPLAY="$socket_name" timeout 30s \
  target/debug/examples/headless_clipboard_client primary-sink >"$primary_selection_sink_log" 2>&1
for marker in \
  SLOPOS_PRIMARY_SELECTION_OFFER_VERIFIED \
  SLOPOS_PRIMARY_SELECTION_TRANSFER_VERIFIED \
  SLOPOS_PRIMARY_SELECTION_MISSING_MIME_EOF_VERIFIED; do
  if ! has_primary_selection_marker "$marker"; then
    write_artifact failed "missing_${marker}"
    cat "$primary_selection_source_log" >&2
    cat "$primary_selection_sink_log" >&2
    exit 1
  fi
done
kill -TERM "$primary_selection_source_pid" 2>/dev/null || true
wait "$primary_selection_source_pid" 2>/dev/null || true
primary_selection_source_pid=""
combine_primary_selection_logs

printf 'Stressing abrupt toplevel and popup disconnect cleanup\n'
WAYLAND_DISPLAY="$socket_name" SLOPOS_DISCONNECT_STRESS_CYCLES=64 timeout 45s \
  target/debug/examples/headless_disconnect_stress >"$stress_log" 2>&1
if ! stress_passed; then
  write_artifact failed "abrupt_disconnect_stress_failed"
  cat "$stress_log" >&2
  cat "$compositor_log" >&2
  exit 1
fi
if ! kill -0 "$compositor_pid" 2>/dev/null; then
  write_artifact failed "compositor_died_after_disconnect_stress"
  cat "$compositor_log" >&2
  exit 1
fi

printf 'Completing healthy presentation and popup lifecycles after stress\n'
WAYLAND_DISPLAY="$socket_name" timeout 30s \
  target/debug/examples/headless_toplevel_client >"$protocol_log" 2>&1
for marker in \
  SLOPOS_XDG_TOPLEVEL_CONFIGURED \
  SLOPOS_XDG_TOPLEVEL_MAXIMIZED \
  SLOPOS_XDG_TOPLEVEL_FULLSCREEN \
  SLOPOS_XDG_TOPLEVEL_RESTORED \
  SLOPOS_XDG_POPUP_CONFIGURED \
  SLOPOS_XDG_POPUP_REPOSITIONED; do
  if ! has_protocol_marker "$marker"; then
    write_artifact failed "missing_${marker}"
    cat "$protocol_log" >&2
    exit 1
  fi
done

kill -TERM "$compositor_pid"
for _ in $(seq 1 50); do
  if ! kill -0 "$compositor_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$compositor_pid" 2>/dev/null; then
  shutdown_status="timeout"
  write_artifact failed "shutdown_timeout"
  exit 1
fi
wait "$compositor_pid" 2>/dev/null || true
shutdown_status="terminated"

if [[ -e "$runtime_dir/$socket_name" ]]; then
  socket_cleanup="supervisor_required"
else
  socket_cleanup="removed_by_compositor"
fi

write_artifact passed
printf 'Headless runtime protocol smoke passed for %s\n' "$commit_sha"
printf 'Evidence: %s\n' "$artifact"
printf 'This does not prove DRM/KMS, rendering, input, popup grabs, XWayland, HDR, VRR, or hardware compatibility.\n'

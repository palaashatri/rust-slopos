#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Headless runtime proof for the compositor-owned synthetic gesture path.
# This exercises the same Space reducer/transition used by DRM libinput, but
# intentionally does not claim physical touchpad or hardware evidence.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    printf '%s\n' 'verify-compositor-gesture-runtime: Linux is required' >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"
for tool in git python3 mktemp grep; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'verify-compositor-gesture-runtime: missing required tool: %s\n' "$tool" >&2
        exit 2
    }
done

commit_sha="$(git rev-parse HEAD)"
branch="$(git branch --show-current || true)"
artifact_dir="${SLOPOS_QA_ARTIFACT_DIR:-artifacts/qa/compositor-gesture-runtime}"
mkdir -p "$artifact_dir"
runtime_dir="$(mktemp -d /tmp/slopos-gesture-runtime.XXXXXX)"
data_dir="$(mktemp -d /tmp/slopos-gesture-data.XXXXXX)"
chmod 700 "$runtime_dir"
chmod 700 "$data_dir"
compositor_pid=""
status=failed
failure=not_started

send_event() {
    local event_json="$1"
    python3 - "$runtime_dir/control.sock" "$event_json" <<'PY'
import json
import socket
import sys

socket_path, event_json = sys.argv[1:]
payload = {"HeadlessTestInput": {"event": json.loads(event_json)}}
sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sock.sendto(json.dumps(payload).encode("utf-8"), socket_path)
sock.close()
PY
}

active_space() {
    python3 - "$runtime_dir/spaces-state.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["active_space"])
PY
}

wait_for_active() {
    local expected="$1"
    for _ in $(seq 1 160); do
        if [[ -s "$runtime_dir/spaces-state.json" ]] && [[ "$(active_space)" == "$expected" ]]; then
            return 0
        fi
        sleep 0.05
    done
    return 1
}

cleanup() {
    local code=$?
    set +e
    if [[ -n "$compositor_pid" ]] && kill -0 "$compositor_pid" 2>/dev/null; then
        kill -TERM "$compositor_pid" 2>/dev/null
        wait "$compositor_pid" 2>/dev/null
    fi
    if [[ -f "$runtime_dir/compositor.log" ]]; then
        cp "$runtime_dir/compositor.log" "$artifact_dir/${commit_sha}-compositor.log"
    fi
    cat >"$artifact_dir/${commit_sha}.json" <<JSON
{
  "schema": 1,
  "component": "slopos-compositor-synthetic-gesture-runtime",
  "commit": "$commit_sha",
  "branch": "$branch",
  "status": "$status",
  "failure": "$failure",
  "evidence_level": "headless_synthetic_input_runtime",
  "synthetic_next_verified": ${synthetic_next_verified:-false},
  "synthetic_previous_verified": ${synthetic_previous_verified:-false},
  "short_gesture_rejected": ${short_gesture_rejected:-false},
  "cancelled_gesture_rejected": ${cancelled_gesture_rejected:-false},
  "physical_input_verified": false,
  "hardware_verified": false
}
JSON
    printf 'qa_exit=%s\n' "$code" >"$artifact_dir/status.txt"
    rm -rf "$runtime_dir" "$data_dir"
    exit "$code"
}
trap cleanup EXIT INT TERM

synthetic_next_verified=false
synthetic_previous_verified=false
short_gesture_rejected=false
cancelled_gesture_rejected=false

export XDG_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_TOKEN="gesture-runtime-$$"
export XDG_DATA_HOME="$data_dir"
export SLOPOS_TEST_INPUT=1
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY

"$CARGO_TARGET_DIR/release/slopos-compositor" --backend headless >"$runtime_dir/compositor.log" 2>&1 &
compositor_pid=$!
for _ in $(seq 1 200); do
    if [[ -s "$runtime_dir/readiness" ]]; then
        break
    fi
    kill -0 "$compositor_pid" 2>/dev/null || { failure=compositor_exit; exit 1; }
    sleep 0.05
done
[[ -s "$runtime_dir/readiness" ]] || { failure=readiness_timeout; exit 1; }
wait_for_active 1 || { failure=initial_space_timeout; exit 1; }

send_event '{"GestureSwipeBegin":{"fingers":3,"time_msec":100}}'
send_event '{"GestureSwipeUpdate":{"delta_x":-50,"delta_y":0,"time_msec":110}}'
send_event '{"GestureSwipeUpdate":{"delta_x":-50,"delta_y":1,"time_msec":120}}'
send_event '{"GestureSwipeEnd":{"cancelled":false,"time_msec":130}}'
wait_for_active 2 || { failure=next_space_timeout; exit 1; }
grep -q 'headless three-finger swipe committed: next Space' "$runtime_dir/compositor.log"
synthetic_next_verified=true

send_event '{"GestureSwipeBegin":{"fingers":3,"time_msec":200}}'
send_event '{"GestureSwipeUpdate":{"delta_x":50,"delta_y":0,"time_msec":210}}'
send_event '{"GestureSwipeUpdate":{"delta_x":50,"delta_y":-1,"time_msec":220}}'
send_event '{"GestureSwipeEnd":{"cancelled":false,"time_msec":230}}'
wait_for_active 1 || { failure=previous_space_timeout; exit 1; }
grep -q 'headless three-finger swipe committed: previous Space' "$runtime_dir/compositor.log"
synthetic_previous_verified=true

send_event '{"GestureSwipeBegin":{"fingers":3,"time_msec":300}}'
send_event '{"GestureSwipeUpdate":{"delta_x":-40,"delta_y":0,"time_msec":310}}'
send_event '{"GestureSwipeEnd":{"cancelled":false,"time_msec":320}}'
wait_for_active 1 || { failure=short_gesture_changed_space; exit 1; }
short_gesture_rejected=true

send_event '{"GestureSwipeBegin":{"fingers":3,"time_msec":400}}'
send_event '{"GestureSwipeUpdate":{"delta_x":-100,"delta_y":0,"time_msec":410}}'
send_event '{"GestureSwipeEnd":{"cancelled":true,"time_msec":420}}'
wait_for_active 1 || { failure=cancelled_gesture_changed_space; exit 1; }
cancelled_gesture_rejected=true

status=passed
failure=""
printf 'synthetic gesture Space transitions verified at %s\n' "$commit_sha"

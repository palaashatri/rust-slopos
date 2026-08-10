#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

set -euo pipefail

# Real headless recovery gate for the compositor-owned Spaces persistence file.
# The fixture is intentionally malformed; the compositor must preserve it in a
# unique quarantine file and continue with a valid default model.

if (( $# != 2 )); then
    printf 'usage: %s REPO_ROOT LOG_DIR\n' "$0" >&2
    exit 2
fi

repo_root="$1"
log_dir="$2"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}"
export CARGO_TARGET_DIR
mkdir -p "$log_dir"

runtime_dir="$(mktemp -d /tmp/slopos-spaces-recovery-runtime.XXXXXX)"
data_dir="$(mktemp -d /tmp/slopos-spaces-recovery-data.XXXXXX)"
chmod 700 "$runtime_dir" "$data_dir"
mkdir -p "$data_dir/slopos-i"

persisted="$data_dir/slopos-i/spaces.json"
invalid_payload='{"spaces":[]}'
printf '%s\n' "$invalid_payload" >"$persisted"

compositor_pid=""
status=0

cleanup() {
    status=$?
    set +e
    if [[ -n "$compositor_pid" ]] && kill -0 "$compositor_pid" 2>/dev/null; then
        kill -TERM "$compositor_pid" 2>/dev/null
        wait "$compositor_pid" 2>/dev/null
    fi
    printf 'qa_exit=%s\n' "$status" >"$log_dir/status.txt"
    rm -rf "$runtime_dir" "$data_dir"
    exit "$status"
}
trap cleanup EXIT INT TERM

export XDG_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_TOKEN="spaces-recovery-$$"
export XDG_DATA_HOME="$data_dir"
export SLOPOS_TEST_INPUT=1
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY

compositor_log="$log_dir/compositor.log"
compositor_bin="${SLOPOS_COMPOSITOR_BIN:-$CARGO_TARGET_DIR/release/slopos-compositor}"
"$compositor_bin" --backend headless >"$compositor_log" 2>&1 &
compositor_pid=$!

readiness="$runtime_dir/readiness"
for _ in $(seq 1 200); do
    [[ -s "$readiness" ]] && break
    kill -0 "$compositor_pid" 2>/dev/null || exit 1
    sleep 0.05
done
[[ -s "$readiness" ]]

if [[ -e "$persisted" ]]; then
    printf 'invalid_persisted_path_still_present\n' >&2
    exit 1
fi

shopt -s nullglob
quarantined=("$data_dir"/slopos-i/.spaces.json.invalid-*)
if (( ${#quarantined[@]} != 1 )); then
    printf 'expected_one_quarantine_file count=%s\n' "${#quarantined[@]}" >&2
    exit 1
fi
cmp -s <(printf '%s\n' "$invalid_payload") "${quarantined[0]}"
grep -q 'quarantined invalid persisted Spaces model' "$compositor_log"

state_file="$runtime_dir/spaces-state.json"
for _ in $(seq 1 200); do
    [[ -s "$state_file" ]] && break
    sleep 0.05
done
python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
spaces = state["spaces"]
if len(spaces) != 8:
    raise SystemExit(f"default_spaces_not_restored: {len(spaces)}")
if state["active_space"] != 1:
    raise SystemExit(f"unexpected_active_space: {state['active_space']}")
PY

cp "${quarantined[0]}" "$log_dir/quarantined-spaces.json"
cp "$state_file" "$log_dir/recovered-state.json"
printf 'quarantine_path=%s\n' "${quarantined[0]}" >"$log_dir/result.txt"
printf 'invalid_state_quarantined=true\ndefault_spaces_restored=true\n' >>"$log_dir/result.txt"
printf 'qa_complete=true\n' >>"$log_dir/result.txt"

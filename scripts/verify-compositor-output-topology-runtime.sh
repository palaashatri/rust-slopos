#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Headless runtime proof for atomic logical-output add/reorder/remove.
# This does not claim DRM/KMS connector hotplug or physical multi-monitor proof.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  printf 'verify-compositor-output-topology-runtime: Linux is required\n' >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
for tool in cargo git grep sed stat timeout wayland-info python3; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'missing required tool: %s\n' "$tool" >&2
    exit 2
  }
done

commit_sha="$(git rev-parse HEAD)"
branch="$(git branch --show-current || true)"
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
artifact_dir="${SLOPOS_QA_ARTIFACT_DIR:-artifacts/qa/compositor-output-topology-runtime}"
mkdir -p "$artifact_dir"
artifact="$artifact_dir/${commit_sha}.json"
compositor_log="$artifact_dir/${commit_sha}-compositor.log"
added_log="$artifact_dir/${commit_sha}-two-outputs.log"
removed_log="$artifact_dir/${commit_sha}-one-output.log"
runtime_dir="$(mktemp -d "${TMPDIR:-/tmp}/slopos-topology-runtime.XXXXXX")"
chmod 700 "$runtime_dir"
compositor_pid=""
socket_name=""

write_artifact() {
  local status="$1"
  local failure="${2:-}"
  cat >"$artifact.tmp" <<JSON
{
  "schema": 1,
  "component": "slopos-compositor-output-topology",
  "commit": "$commit_sha",
  "branch": "$branch",
  "started_at_utc": "$timestamp",
  "status": "$status",
  "failure": "$failure",
  "evidence_level": "headless_runtime_topology",
  "logical_output_add_verified": $([[ "$status" == passed ]] && printf true || printf false),
  "logical_output_reorder_verified": $([[ "$status" == passed ]] && printf true || printf false),
  "logical_output_remove_verified": $([[ "$status" == passed ]] && printf true || printf false),
  "output_snapshot_verified": $([[ "$status" == passed ]] && printf true || printf false),
  "rejected_layout_preserves_snapshot_verified": $([[ "$status" == passed ]] && printf true || printf false),
  "surface_migration_source_contract_verified": $([[ "$status" == passed ]] && printf true || printf false),
  "hardware_verified": false,
  "drm_hotplug_verified": false,
  "physical_multi_monitor_verified": false,
  "compositor_log": "$(basename "$compositor_log")",
  "two_output_log": "$(basename "$added_log")",
  "one_output_log": "$(basename "$removed_log")"
}
JSON
  mv "$artifact.tmp" "$artifact"
}

cleanup() {
  local code=$?
  trap - EXIT INT TERM
  if [[ -n "$compositor_pid" ]] && kill -0 "$compositor_pid" 2>/dev/null; then
    kill -TERM "$compositor_pid" 2>/dev/null || true
    wait "$compositor_pid" 2>/dev/null || true
  fi
  rm -rf "$runtime_dir"
  if [[ $code -ne 0 && ! -f "$artifact" ]]; then
    write_artifact failed "unexpected_exit_$code"
  fi
  exit "$code"
}
trap cleanup EXIT INT TERM

if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  write_artifact failed tracked_worktree_dirty
  exit 2
fi

cargo build -p slopos-compositor --locked
export XDG_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_TOKEN="topology-${commit_sha}-$$"
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY

target/debug/slopos-compositor --backend headless >"$compositor_log" 2>&1 &
compositor_pid=$!
readiness="$runtime_dir/readiness"
for _ in $(seq 1 100); do
  [[ -s "$readiness" ]] && break
  kill -0 "$compositor_pid" 2>/dev/null || {
    write_artifact failed compositor_exited_before_readiness
    cat "$compositor_log" >&2
    exit 1
  }
  sleep 0.1
done
[[ -s "$readiness" ]] || {
  write_artifact failed readiness_timeout
  exit 1
}
socket_name="$(sed -n '1p' "$readiness")"
control="$runtime_dir/control.sock"
[[ -S "$control" ]] || {
  write_artifact failed control_socket_missing
  exit 1
}
outputs_state="$runtime_dir/outputs-state.json"
for _ in $(seq 1 100); do
  [[ -s "$outputs_state" ]] && break
  sleep 0.1
done
[[ -s "$outputs_state" ]] || {
  write_artifact failed output_snapshot_missing
  exit 1
}

assert_snapshot() {
  local expected_backend="$1"
  local expected_names="$2"
  local expected_revision="$3"
  python3 - "$outputs_state" "$expected_backend" "$expected_names" "$expected_revision" <<'PY'
import json
import sys

path, backend, names, revision = sys.argv[1:]
with open(path, encoding="utf-8") as stream:
    data = json.load(stream)
assert data["backend"] == backend, data
assert data["revision"] == int(revision), data
actual = [item["name"] for item in data["outputs"]]
assert actual == names.split(","), (actual, names)
assert all(item["scale_percent"] == 100 for item in data["outputs"]), data
PY
}

snapshot_revision() {
  python3 - "$outputs_state" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    print(json.load(stream)["revision"])
PY
}

send_layout() {
  local layout="$1"
  python3 - "$control" "$layout" <<'PY'
import json
import socket
import sys
path, layout = sys.argv[1], sys.argv[2]
payload = json.dumps({"ReconfigureOutputs": {"layout": layout}}).encode("utf-8")
sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sock.sendto(payload, path)
sock.close()
PY
}

wait_for_apply_count() {
  local expected="$1"
  for _ in $(seq 1 100); do
    [[ "$(grep -c 'runtime output topology applied' "$compositor_log" || true)" -ge "$expected" ]] && return 0
    sleep 0.1
  done
  return 1
}

assert_snapshot headless X11-1 1

send_layout 'LEFT:800x600@0,0:s100;RIGHT:1024x768@800,0:s100'
wait_for_apply_count 1 || {
  write_artifact failed two_output_apply_timeout
  cat "$compositor_log" >&2
  exit 1
}
assert_snapshot headless LEFT,RIGHT 2
WAYLAND_DISPLAY="$socket_name" timeout 10s wayland-info >"$added_log" 2>&1
[[ "$(grep -c "interface: 'wl_output'" "$added_log")" -eq 2 ]] || {
  write_artifact failed two_output_registry_count
  cat "$added_log" >&2
  exit 1
}
grep -q "name: 'LEFT'" "$added_log" || grep -q 'LEFT' "$added_log" || {
  write_artifact failed left_output_name_missing
  exit 1
}
grep -q "name: 'RIGHT'" "$added_log" || grep -q 'RIGHT' "$added_log" || {
  write_artifact failed right_output_name_missing
  exit 1
}
[[ "$(sed -n 's/^width=//p' "$readiness")" == 1824 ]] || {
  write_artifact failed two_output_readiness_width
  cat "$readiness" >&2
  exit 1
}

# Malformed and scale-mismatched requests must be rejected transactionally;
# neither may advance the compositor's authoritative output projection.
before_rejection_revision="$(snapshot_revision)"
send_layout 'LEFT:800x600@0,0:s100;broken-token'
send_layout 'LEFT:800x600@0,0:s125;RIGHT:1024x768@800,0:s125'
sleep 0.3
[[ "$(snapshot_revision)" == "$before_rejection_revision" ]] || {
  write_artifact failed rejected_layout_changed_snapshot
  cat "$compositor_log" >&2
  exit 1
}
assert_snapshot headless LEFT,RIGHT "$before_rejection_revision"

# Reorder and resize while preserving one total headless canvas transaction.
send_layout 'RIGHT:1024x768@0,0:s100;LEFT:800x600@1024,0:s100'
wait_for_apply_count 2 || {
  write_artifact failed reorder_apply_timeout
  exit 1
}
assert_snapshot headless RIGHT,LEFT 3

send_layout 'RIGHT:1024x768@0,0:s100'
wait_for_apply_count 3 || {
  write_artifact failed one_output_apply_timeout
  exit 1
}
assert_snapshot headless RIGHT 4
WAYLAND_DISPLAY="$socket_name" timeout 10s wayland-info >"$removed_log" 2>&1
[[ "$(grep -c "interface: 'wl_output'" "$removed_log")" -eq 1 ]] || {
  write_artifact failed one_output_registry_count
  cat "$removed_log" >&2
  exit 1
}
grep -q "name: 'RIGHT'" "$removed_log" || grep -q 'RIGHT' "$removed_log" || {
  write_artifact failed surviving_output_name_missing
  exit 1
}
[[ "$(sed -n 's/^width=//p' "$readiness")" == 1024 ]] || {
  write_artifact failed one_output_readiness_width
  cat "$readiness" >&2
  exit 1
}

kill -TERM "$compositor_pid"
wait "$compositor_pid" 2>/dev/null || true
compositor_pid=""
write_artifact passed
printf 'Headless runtime output topology gate passed for %s\n' "$commit_sha"
printf 'Evidence: %s\n' "$artifact"
printf 'This does not prove DRM/KMS connector hotplug or physical multi-monitor compatibility.\n'

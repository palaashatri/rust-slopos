#!/usr/bin/env bash
set -euo pipefail

# Evidence gate for compositor-owned application-ID Spaces policies. It drives
# the real session socket and a real Preview Wayland client.

if (( $# != 2 )); then
    printf 'usage: %s REPO_ROOT LOG_DIR\n' "$0" >&2
    exit 2
fi
repo_root="$1"
log_dir="$2"
mkdir -p "$log_dir"

runtime_dir="$(mktemp -d /tmp/slopos-spaces-app-policy-runtime.XXXXXX)"
data_dir="$(mktemp -d /tmp/slopos-spaces-app-policy-data.XXXXXX)"
chmod 700 "$runtime_dir" "$data_dir"

compositor_pid=""
preview_pid=""
readiness=""
state_file=""
control=""
compositor_log=""

cleanup() {
    status=$?
    set +e
    if [[ -n "$preview_pid" ]] && kill -0 "$preview_pid" 2>/dev/null; then
        kill -TERM "$preview_pid" 2>/dev/null
        wait "$preview_pid" 2>/dev/null
    fi
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
export SLOPOS_SESSION_TOKEN="spaces-app-policy-$$"
export XDG_DATA_HOME="$data_dir"
export SLOPOS_TEST_INPUT=1
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY

snapshot() {
    label="$1"
    cp "$state_file" "$log_dir/state-$label.json"
    python3 - "$state_file" "$label" <<'PY' | tee -a "$log_dir/steps.log"
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
print("state=" + sys.argv[2] + " " + json.dumps(state, sort_keys=True, separators=(",", ":")))
PY
}

send_request() {
    python3 - "$control" "$1" <<'PY'
import json
import socket
import sys

path, encoded = sys.argv[1:]
sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sock.sendto(json.dumps(json.loads(encoded), separators=(",", ":")).encode(), path)
sock.close()
PY
}

current_revision() {
    python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["revision"])
PY
}

wait_revision() {
    expected="$1"
    revision=0
    for _ in $(seq 1 200); do
        revision="$(current_revision 2>/dev/null || printf '0')"
        if (( revision >= expected )); then
            return 0
        fi
        sleep 0.05
    done
    printf 'revision_timeout expected=%s actual=%s\n' "$expected" "$revision" >&2
    return 1
}

wait_for_window() {
    window_id=""
    for _ in $(seq 1 200); do
        window_id="$(sed -n 's/.*assign window_id=\([^ ]*\).*/\1/p' "$compositor_log" | tail -1)"
        if [[ -n "$window_id" ]]; then
            printf '%s\n' "$window_id"
            return 0
        fi
        sleep 0.05
    done
    return 1
}

wait_counts() {
    expected="$1"
    for _ in $(seq 1 200); do
        if python3 - "$state_file" "$expected" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
counts = {space["id"]: space["window_count"] for space in state["spaces"]}
expected = {int(key): value for key, value in json.loads(sys.argv[2]).items()}
raise SystemExit(0 if counts == expected else 1)
PY
        then
            return 0
        fi
        sleep 0.05
    done
    return 1
}

assert_counts() {
    label="$1"
    expected="$2"
    python3 - "$state_file" "$label" "$expected" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
counts = {space["id"]: space["window_count"] for space in state["spaces"]}
expected = {int(key): value for key, value in json.loads(sys.argv[3]).items()}
if counts != expected:
    raise SystemExit(f"{sys.argv[2]}: unexpected window counts {counts}")
PY
    printf 'counts=%s %s\n' "$label" "$expected" | tee -a "$log_dir/steps.log"
}

start_compositor() {
    label="$1"
    compositor_log="$log_dir/compositor-$label.log"
    "$repo_root/target/release/slopos-compositor" --backend headless >"$compositor_log" 2>&1 &
    compositor_pid=$!
    readiness="$runtime_dir/readiness"
    for _ in $(seq 1 200); do
        [[ -s "$readiness" ]] && break
        kill -0 "$compositor_pid" 2>/dev/null || return 1
        sleep 0.05
    done
    [[ -s "$readiness" ]]
    socket_name="$(sed -n '1p' "$readiness")"
    export WAYLAND_DISPLAY="$socket_name"
    control="$runtime_dir/control.sock"
    state_file="$runtime_dir/spaces-state.json"
    [[ -S "$control" && -f "$state_file" ]]
}

start_compositor first
printf 'runtime_dir=%s\ndata_dir=%s\nwayland_display=%s\ncompositor_pid=%s\n' \
    "$runtime_dir" "$data_dir" "$WAYLAND_DISPLAY" "$compositor_pid" >"$log_dir/provenance.txt"
snapshot initial

before="$(current_revision)"
send_request '{"Spaces":{"command":{"command":"set_application_policy","app_id":"com.slopos.preview","target":{"id":{"id":2}}}}}'
wait_revision "$((before + 1))"
snapshot policy-id
python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
policies = state["application_policies"]
if policies != [{"app_id": "com.slopos.preview", "target": {"id": {"id": 2}}}]:
    raise SystemExit(f"policy_id_readback_failed: {policies}")
PY
printf 'policy_id_readback=2\n' | tee -a "$log_dir/steps.log"

WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_RUNTIME_DIR="$runtime_dir" \
    SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir" \
    "$repo_root/target/release/preview" "$repo_root/assets/slopos-logo.png" \
    >"$log_dir/preview.log" 2>&1 &
preview_pid=$!
window_id="$(wait_for_window)"
printf 'preview_window_id=%s\n' "$window_id" | tee -a "$log_dir/steps.log"
wait_counts '{"1":0,"2":1,"3":0,"4":0,"5":0,"6":0,"7":0,"8":0}'
snapshot mapped-by-id
assert_counts mapped-by-id '{"1":0,"2":1,"3":0,"4":0,"5":0,"6":0,"7":0,"8":0}'

before="$(current_revision)"
send_request '{"Spaces":{"command":{"command":"set_application_policy","app_id":"com.slopos.preview","target":"all"}}}'
wait_revision "$((before + 1))"
snapshot policy-all
python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
policies = state["application_policies"]
if policies != [{"app_id": "com.slopos.preview", "target": "all"}]:
    raise SystemExit(f"policy_all_readback_failed: {policies}")
PY
printf 'policy_all_readback=all\n' | tee -a "$log_dir/steps.log"
wait_counts '{"1":1,"2":1,"3":1,"4":1,"5":1,"6":1,"7":1,"8":1}'
snapshot existing-window-reassigned-all
assert_counts existing-window-reassigned-all '{"1":1,"2":1,"3":1,"4":1,"5":1,"6":1,"7":1,"8":1}'

before="$(current_revision)"
send_request '{"Spaces":{"command":{"command":"set_application_policy","app_id":"com.slopos.preview","target":"current"}}}'
wait_revision "$((before + 1))"
snapshot policy-current-cleared
python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
if state["application_policies"]:
    raise SystemExit(f"policy_current_did_not_clear: {state['application_policies']}")
PY
printf 'policy_current_readback=cleared\n' | tee -a "$log_dir/steps.log"
wait_counts '{"1":1,"2":0,"3":0,"4":0,"5":0,"6":0,"7":0,"8":0}'
snapshot existing-window-reassigned-current
assert_counts existing-window-reassigned-current '{"1":1,"2":0,"3":0,"4":0,"5":0,"6":0,"7":0,"8":0}'

before="$(current_revision)"
send_request '{"Spaces":{"command":{"command":"set_application_policy","app_id":"com.slopos.preview","target":{"id":{"id":999}}}}}'
sleep 0.2
after="$(current_revision)"
[[ "$before" == "$after" ]]
snapshot invalid-space-rejected
printf 'invalid_space_target_rejected revision=%s\n' "$after" | tee -a "$log_dir/steps.log"

before="$(current_revision)"
send_request '{"Spaces":{"command":{"command":"set_application_policy","app_id":"com.slopos.preview\nbad","target":"all"}}}'
sleep 0.2
after="$(current_revision)"
[[ "$before" == "$after" ]]
snapshot invalid-app-id-rejected
printf 'invalid_app_id_rejected revision=%s\n' "$after" | tee -a "$log_dir/steps.log"

before="$(current_revision)"
send_request '{"Spaces":{"command":{"command":"set_application_policy","app_id":"com.slopos.preview","target":{"id":{"id":3}}}}}'
wait_revision "$((before + 1))"
snapshot persisted-policy
python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
if state["application_policies"] != [{"app_id": "com.slopos.preview", "target": {"id": {"id": 3}}}]:
    raise SystemExit(f"policy_persist_setup_failed: {state['application_policies']}")
PY
persisted="$data_dir/slopos-i/spaces.json"
[[ -s "$persisted" ]]
cp "$persisted" "$log_dir/persisted-spaces.json"
printf 'persisted_policy_file=%s\n' "$persisted" | tee -a "$log_dir/steps.log"

kill -TERM "$preview_pid" 2>/dev/null
wait "$preview_pid" 2>/dev/null || true
preview_pid=""
kill -TERM "$compositor_pid" 2>/dev/null
wait "$compositor_pid" 2>/dev/null || true
compositor_pid=""
rm -f "$readiness" "$state_file"

start_compositor restart
for _ in $(seq 1 200); do
    if python3 - "$state_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    state = json.load(handle)
raise SystemExit(0 if state["application_policies"] == [{"app_id": "com.slopos.preview", "target": {"id": {"id": 3}}}] else 1)
PY
    then
        break
    fi
    sleep 0.05
done
snapshot restart-policy-restored
printf 'restart_policy_restored=3\n' | tee -a "$log_dir/steps.log"

WAYLAND_DISPLAY="$WAYLAND_DISPLAY" XDG_RUNTIME_DIR="$runtime_dir" \
    SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir" \
    "$repo_root/target/release/preview" "$repo_root/assets/slopos-logo.png" \
    >"$log_dir/preview-restart.log" 2>&1 &
preview_pid=$!
restart_window_id="$(wait_for_window)"
printf 'restart_preview_window_id=%s\n' "$restart_window_id" | tee -a "$log_dir/steps.log"
wait_counts '{"1":0,"2":0,"3":1,"4":0,"5":0,"6":0,"7":0,"8":0}'
snapshot restart-mapped-by-policy
assert_counts restart-mapped-by-policy '{"1":0,"2":0,"3":1,"4":0,"5":0,"6":0,"7":0,"8":0}'

if ! grep -q 'rejecting Spaces command' "$log_dir/compositor-first.log"; then
    printf 'missing_invalid_policy_rejection_log\n' >&2
    exit 1
fi

printf 'qa_complete=true\n' >"$log_dir/result.txt"

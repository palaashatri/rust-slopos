#!/usr/bin/env bash
# Deterministic Linux-host post-reboot QA for an installed SLOPOS-I VM.
set -euo pipefail

SSH_PORT="${SSH_PORT:-2222}"
SSH_USER="${SSH_USER:-retro}"
SSH_KEY_PATH="${SSH_KEY_PATH:-}"
OUTPUT_DIR="${OUTPUT_DIR:-$(pwd)/artifacts/installed-vm-evidence}"
EXPECTED_COMMIT="${EXPECTED_COMMIT:-}"
WAIT_SEC="${WAIT_SEC:-5400}"

[[ "$SSH_PORT" =~ ^[0-9]+$ ]] && (( SSH_PORT >= 1 && SSH_PORT <= 65535 )) || {
  echo "ERROR: SSH_PORT must be between 1 and 65535" >&2
  exit 2
}
[[ "$SSH_USER" =~ ^[a-z_][a-z0-9_-]*$ ]] || {
  echo "ERROR: SSH_USER must be a simple Linux account name" >&2
  exit 2
}
[[ "$EXPECTED_COMMIT" =~ ^[0-9a-fA-F]{40}$ ]] || {
  echo "ERROR: EXPECTED_COMMIT must be a full 40-character commit SHA" >&2
  exit 2
}
[[ "$WAIT_SEC" =~ ^[0-9]+$ ]] && (( WAIT_SEC > 0 )) || {
  echo "ERROR: WAIT_SEC must be a positive integer" >&2
  exit 2
}
case "$SSH_KEY_PATH" in
  /*) ;;
  *) echo "ERROR: SSH_KEY_PATH must be an absolute path" >&2; exit 2 ;;
esac
test -s "$SSH_KEY_PATH" || {
  echo "ERROR: SSH private key is missing: $SSH_KEY_PATH" >&2
  exit 2
}
command -v ssh >/dev/null 2>&1 || { echo "ERROR: ssh is required" >&2; exit 1; }
command -v scp >/dev/null 2>&1 || { echo "ERROR: scp is required" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "ERROR: python3 is required" >&2; exit 1; }

mkdir -p "$OUTPUT_DIR"
chmod 600 "$SSH_KEY_PATH"
QA_LOG="$OUTPUT_DIR/qa-vm.log"
STATUS_JSON="$OUTPUT_DIR/status.json"
SOURCE_COMMIT=""
QA_EXIT=""
QA_MARKER=0
EVIDENCE_COPIED=0
STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

ssh_opts=(
  -i "$SSH_KEY_PATH"
  -p "$SSH_PORT"
  -o BatchMode=yes
  -o IdentitiesOnly=yes
  -o ConnectTimeout=5
  -o ConnectionAttempts=1
  -o LogLevel=ERROR
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile=/dev/null
)
scp_opts=(
  -i "$SSH_KEY_PATH"
  -P "$SSH_PORT"
  -o BatchMode=yes
  -o IdentitiesOnly=yes
  -o ConnectTimeout=5
  -o LogLevel=ERROR
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile=/dev/null
)
remote="$SSH_USER@127.0.0.1"

finalize() {
  status=$?
  passed=false
  if [[ "$status" -eq 0 && "$QA_EXIT" == "0" && "$QA_MARKER" -eq 1 && "$EVIDENCE_COPIED" -eq 1 ]]; then
    passed=true
  fi
  COMPLETED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  STATUS="$status" PASSED="$passed" SOURCE_COMMIT="$SOURCE_COMMIT" QA_EXIT="${QA_EXIT:-}" \
  QA_MARKER="$QA_MARKER" EVIDENCE_COPIED="$EVIDENCE_COPIED" STARTED_UTC="$STARTED_UTC" \
  EXPECTED_COMMIT="$EXPECTED_COMMIT" SSH_PORT="$SSH_PORT" SSH_USER="$SSH_USER" STATUS_JSON="$STATUS_JSON" \
  python3 - <<'PY' || true
import json, os
payload = {
    "ssh_user": os.environ["SSH_USER"],
    "ssh_port": int(os.environ["SSH_PORT"]),
    "expected_commit": os.environ["EXPECTED_COMMIT"],
    "source_commit": os.environ.get("SOURCE_COMMIT", ""),
    "qa_exit": int(os.environ["QA_EXIT"]) if os.environ.get("QA_EXIT", "").isdigit() else None,
    "qa_marker": os.environ.get("QA_MARKER") == "1",
    "evidence_copied": os.environ.get("EVIDENCE_COPIED") == "1",
    "passed": os.environ.get("PASSED") == "true",
    "host_exit": int(os.environ["STATUS"]),
    "started_utc": os.environ["STARTED_UTC"],
    "completed_utc": os.environ["COMPLETED_UTC"],
}
with open(os.environ["STATUS_JSON"], "w", encoding="utf-8") as fh:
    json.dump(payload, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY
}
trap finalize EXIT

echo "Waiting for installed VM SSH on 127.0.0.1:$SSH_PORT"
deadline=$((SECONDS + WAIT_SEC))
ssh_ready=0
while (( SECONDS < deadline )); do
  if ssh "${ssh_opts[@]}" "$remote" true >/dev/null 2>&1; then
    ssh_ready=1
    break
  fi
  sleep 5
done
[[ "$ssh_ready" -eq 1 ]] || {
  echo "ERROR: SSH did not become ready within ${WAIT_SEC}s" >&2
  exit 1
}

echo "Installed VM SSH is ready"
SOURCE_COMMIT="$(ssh "${ssh_opts[@]}" "$remote" "git -C /home/$SSH_USER/slopos-i rev-parse --verify HEAD" | tr -d '\r\n')"
[[ "$SOURCE_COMMIT" =~ ^[0-9a-fA-F]{40}$ ]] || {
  echo "ERROR: installed source checkout did not report a full commit SHA" >&2
  exit 1
}
[[ "${SOURCE_COMMIT,,}" == "${EXPECTED_COMMIT,,}" ]] || {
  echo "ERROR: installed source commit $SOURCE_COMMIT does not match expected $EXPECTED_COMMIT" >&2
  exit 1
}
echo "Installed source commit verified: $SOURCE_COMMIT"

echo "Waiting for installed X11 session, Openbox and SLOPOS shell"
ssh "${ssh_opts[@]}" "$remote" "
  set -e
  for _ in \$(seq 1 180); do
    if DISPLAY=:0 XAUTHORITY=/home/$SSH_USER/.Xauthority xdpyinfo >/dev/null 2>&1 \
       && pgrep -x openbox >/dev/null 2>&1 \
       && pgrep -x slopos-shell >/dev/null 2>&1; then
      exit 0
    fi
    sleep 2
  done
  exit 1
"

remote_qa="DISPLAY=:0 XAUTHORITY=/home/$SSH_USER/.Xauthority SLOPOS_SOURCE_ROOT=/home/$SSH_USER/slopos-i SLOPOS_EXPECTED_COMMIT=$EXPECTED_COMMIT bash /home/$SSH_USER/slopos-i/packaging/vm/qa-vm.sh"
set +e
set -o pipefail
ssh "${ssh_opts[@]}" "$remote" "$remote_qa" 2>&1 | tee "$QA_LOG"
QA_EXIT="${PIPESTATUS[0]}"
set -e
if [[ "$QA_EXIT" -ne 0 ]]; then
  echo "ERROR: in-guest qa-vm.sh failed with exit code $QA_EXIT" >&2
  exit "$QA_EXIT"
fi
grep -Fqx 'SLOPOS_X11_INSTALLED_VM_QA=PASS' "$QA_LOG" || {
  echo "ERROR: in-guest QA exited successfully without its PASS marker" >&2
  exit 1
}
grep -Fq "SLOPOS_SOURCE_COMMIT=$EXPECTED_COMMIT" "$QA_LOG" || {
  echo "ERROR: in-guest QA did not bind its evidence to the expected commit" >&2
  exit 1
}
QA_MARKER=1

mkdir -p "$OUTPUT_DIR/guest"
scp "${scp_opts[@]}" -r "$remote:/home/$SSH_USER/qa/slopos-vm/." "$OUTPUT_DIR/guest/"
find "$OUTPUT_DIR/guest" -maxdepth 1 -type f -name 'installed-session-*.png' -size +0c | grep -q . || {
  echo "ERROR: installed VM screenshot evidence was not copied" >&2
  exit 1
}
test -s "$OUTPUT_DIR/guest/qa-vm.log" || {
  echo "ERROR: installed VM guest QA log was not copied" >&2
  exit 1
}
EVIDENCE_COPIED=1

echo "INSTALLED_VM_QA_STATUS_0"
echo "Evidence: $OUTPUT_DIR"
